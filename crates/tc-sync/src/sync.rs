//! A whole sync with Door43 (`docs/STORAGE.md` §9.1): the executor of [`crate::machine`].
//!
//! Each action the machine asks for runs against the project's store, Door43's REST API or
//! the project's git repository (network calls through `tc_door43::git`, on a blocking
//! thread), and its answer is the machine's next event. The machine alone decides what comes
//! next, so the side of intake a failure is reported on cannot be wrong here.

use crate::machine::{
    Ended, GateRefusal, MAX_ATTEMPTS, Remote, SyncAction, SyncEvent, SyncState, next,
};
use std::path::PathBuf;
use std::time::Duration;
use tc_api::refusal::{SyncBlock, SyncRefusal};
use tc_door43::{Door43Client, Door43Error, FastForwardRefspec, GitRemote, Session};
use tc_store::{
    CheckpointReason, CheckpointReport, CheckpointSkip, IntakeReport, Mode, ProjectHandle,
    ReceivedPack,
};

/// The folder Door43's teammates' packs are in, in every tree.
const PACKS: &str = "ingredients/tc/changes/";
const FORMAT_JSON: &str = "ingredients/tc/format.json";
const PROJECT_JSON: &str = "ingredients/tc/project.json";
const TC4_JOURNAL: &str = "ingredients/checking/journal/";

/// What a sync did (STORAGE §9.7).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub ended: Ended,
    /// What the merge took in, summed over the rounds.
    pub received: IntakeReport,
    /// The commit Door43's branch was read back at, when it was.
    pub confirmed: Option<String>,
    /// Rounds that lost a race to a teammate's push.
    pub raced: u8,
    /// New changes received, by the person who made them (§9.7 `received`; devices grouped by
    /// Door43 username).
    pub received_by: std::collections::BTreeMap<tc_journal::fold::PersonLabel, u32>,
    /// Conflicts the sync brought that were not listed before it (§9.7 `new_conflicts`).
    pub new_conflicts: Vec<tc_journal::fold::ConflictId>,
    /// Commits on Door43 made by another tool that changed the project's files (§9.2 N4,
    /// `rulings/2026-10-22-H18.md` B): the project kept its own text, and each is said once.
    pub other_tools: Vec<OtherToolCommit>,
    /// Door43's branch was gone (its owner deleted it) and this sync pushed the project's copy
    /// as the branch again (owner ruling, `rulings/2026-10-24-H5.md`): "The project was missing
    /// from Door43, so tC4 put your copy back. Ask your teammates to sync."
    pub recreated: bool,
    /// A large push failed for want of a connection three times in a row (§9.5): the report
    /// offers "Save an exchange file instead" (§10).
    pub offer_exchange: bool,
    /// Changes of teammates this sync pushed again because Door43's branch had lost them
    /// (moved back behind a teammate's push; §9.7 `restored`): "Door43 did not have 1 change
    /// from a teammate. tC4 sent it again."
    pub restored: u32,
}

/// One commit made on Door43 by another tool (§9.2 N4): "Titus was changed on Door43 by jdoe
/// (commit 3fa2c1d) with another tool. translationCore kept its own text. That version is in
/// Door43's history."
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherToolCommit {
    /// The commit's first seven hex digits.
    pub commit: String,
    pub author: String,
    /// The books whose files it changed.
    pub books: Vec<tc_journal::BookId>,
    /// The project's files it changed, by path.
    pub paths: Vec<String>,
}

/// The pauses between rounds. Tests count them instead of waiting.
pub trait Pause: Send + Sync {
    fn pause(&self, attempt: u8) -> impl std::future::Future<Output = ()> + Send;
}

/// `min(2^attempt, 16) s × U(0.7, 1.3)`, cut so that the pauses of one sync add up to at most
/// 30 s (Q11). One per sync.
#[derive(Default)]
pub struct Backoff {
    waited_ms: std::sync::atomic::AtomicU64,
}

/// The longest one sync waits between rounds, in all.
pub const MAX_WAIT: Duration = Duration::from_secs(30);

impl Pause for Backoff {
    async fn pause(&self, attempt: u8) {
        use std::sync::atomic::Ordering;
        let base = 2f64.powi(i32::from(attempt.min(4)));
        let [a, b] = tc_store::random::bytes::<2>();
        let unit = f64::from(u16::from_le_bytes([a, b])) / f64::from(u16::MAX);
        let planned = (base * (0.7 + 0.6 * unit) * 1000.0) as u64;
        let left =
            (MAX_WAIT.as_millis() as u64).saturating_sub(self.waited_ms.load(Ordering::Acquire));
        let ms = planned.min(left);
        self.waited_ms.fetch_add(ms, Ordering::AcqRel);
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

/// Syncs `project` with its Door43 repository: `origin` and `tc.branch` of its git
/// repository, set when it was shared, joined or connected (§9.6).
pub async fn sync(
    project: &ProjectHandle,
    client: &Door43Client,
    session: Option<&Session>,
    pause: &impl Pause,
) -> SyncReport {
    let mut run = Run {
        project,
        client,
        session: session.cloned(),
        target: None,
        fetched: None,
        branch_gone: false,
        packs: Vec::new(),
        merged: None,
        report: SyncReport {
            ended: Ended::Synced {
                received: crate::machine::Received::Nothing,
                confirmed: false,
            },
            received: IntakeReport::default(),
            confirmed: None,
            raced: 0,
            other_tools: Vec::new(),
            received_by: Default::default(),
            new_conflicts: Vec::new(),
            recreated: false,
            offer_exchange: false,
            restored: 0,
        },
        conflicts_before: project
            .snapshot()
            .fold()
            .conflicts()
            .map(|c| c.id)
            .collect(),
        received_from: Default::default(),
        received_batches: Default::default(),
        received_ids: Vec::new(),
    };
    let (mut state, mut actions) = next(&SyncState::Idle, &SyncEvent::Start);
    while let Some(action) = actions.pop() {
        if let SyncAction::Report(ended) = action {
            run.report.ended = ended;
            run.tally();
            break;
        }
        let event = run.act(&action, pause).await;
        let (after, more) = next(&state, &event);
        state = after;
        actions = more;
    }
    run.report
}

/// Where the project syncs to.
#[derive(Debug, Clone)]
struct Target {
    git_dir: PathBuf,
    remote: GitRemote,
    branch: String,
    /// Door43 said the repository has no commits: a missing branch is a first push, not a
    /// deleted one.
    empty: bool,
}

struct Run<'a> {
    project: &'a ProjectHandle,
    client: &'a Door43Client,
    session: Option<Session>,
    target: Option<Target>,
    /// The commit the last fetch found on Door43.
    fetched: Option<git2::Oid>,
    /// The last fetch found the branch deleted on Door43: the push makes it again.
    branch_gone: bool,
    packs: Vec<ReceivedPack>,
    /// The merge's answer: the take-in and the commit come back together.
    merged: Option<Result<CheckpointReport, String>>,
    report: SyncReport,
    conflicts_before: std::collections::BTreeSet<tc_journal::fold::ConflictId>,
    /// A teammate's saves in the received packs, by the device that made them: one per batch
    /// of new changes that is work (a verse and the alignment its edit reflowed are one save).
    received_from: std::collections::BTreeMap<tc_journal::DeviceId, u32>,
    /// The batches counted in `received_from`.
    received_batches: std::collections::BTreeSet<(tc_journal::DeviceId, tc_journal::BatchId)>,
    /// The ids of every new change, bookkeeping included (the Received sheet's rows).
    received_ids: Vec<tc_journal::ChangeId>,
}

impl Run<'_> {
    /// §9.7: who sent what, and which conflicts are new, read from the fold after the sync.
    /// It costs the received devices and the listed conflicts, not the project.
    fn tally(&mut self) {
        let snapshot = self.project.snapshot();
        let fold = snapshot.fold();
        let me = self.project.facts().device;
        let labels = fold.person_labels(me);
        for (device, count) in &self.received_from {
            let label = labels
                .get(device)
                .cloned()
                .unwrap_or(tc_journal::fold::PersonLabel::Teammate(0));
            *self.report.received_by.entry(label).or_insert(0) += count;
        }
        self.report.new_conflicts = fold
            .conflicts()
            .map(|c| c.id)
            .filter(|id| !self.conflicts_before.contains(id))
            .collect();
    }

    /// The new changes of the received packs, by device: those the fold does not hold yet.
    fn count_received(&mut self, packs: &[ReceivedPack]) {
        let snapshot = self.project.snapshot();
        let project = self.project.facts().id;
        for pack in packs {
            for change in tc_store::read_received_pack(&pack.path, &pack.bytes, project).changes {
                if snapshot.fold().change(&change.id()).is_none() {
                    // Bookkeeping is never counted, and work is counted once per batch: one
                    // save (`crate::is_bookkeeping`).
                    let body = change.body();
                    if !crate::is_bookkeeping(&body.record)
                        && self.received_batches.insert((body.device, body.batch.id))
                    {
                        *self.received_from.entry(body.device).or_insert(0) += 1;
                    }
                    self.received_ids.push(change.id());
                }
            }
        }
    }

    async fn act(&mut self, action: &SyncAction, pause: &impl Pause) -> SyncEvent {
        match action {
            SyncAction::Gate => self.gate().await,
            SyncAction::Checkpoint => {
                match self.project.checkpoint(CheckpointReason::BeforeSync).await {
                    Ok(report) => match report.skipped {
                        Some(CheckpointSkip::ReadOnly) => SyncEvent::CheckpointFailed {
                            reason: "the project is read-only".into(),
                        },
                        Some(CheckpointSkip::SecondWriter) => SyncEvent::CheckpointFailed {
                            reason: "another installation is saving into this folder".into(),
                        },
                        _ => SyncEvent::CheckpointMade,
                    },
                    Err(e) => SyncEvent::CheckpointFailed {
                        reason: e.to_string(),
                    },
                }
            }
            SyncAction::Fetch => self.fetch().await,
            SyncAction::ReadRemote => self.read_remote().await,
            SyncAction::TakeIn => self.take_in().await,
            SyncAction::MergeCommit => match self.merged.take() {
                Some(Ok(report)) => match report.skipped {
                    None | Some(CheckpointSkip::NoChanges) => SyncEvent::Merged,
                    Some(why) => SyncEvent::MergeFailed {
                        reason: format!("{why:?}"),
                    },
                },
                Some(Err(reason)) => SyncEvent::MergeFailed { reason },
                None => SyncEvent::MergeFailed {
                    reason: "no merge was made".into(),
                },
            },
            SyncAction::Push => self.push().await,
            SyncAction::Wait { attempt } => {
                self.report.raced += 1;
                pause.pause(*attempt).await;
                SyncEvent::WaitOver
            }
            SyncAction::Confirm => self.confirm().await,
            SyncAction::Repack => SyncEvent::Repacked,
            SyncAction::Report(_) => SyncEvent::Repacked,
        }
    }

    /// S0 and Q8: the project can sync, the internet is on, a session is there, and the
    /// account may push to the repository, all before any git request.
    async fn gate(&mut self) -> SyncEvent {
        let refuse = |r| SyncEvent::GateRefused(GateRefusal::Refusal(r));
        let facts = self.project.facts();
        if let Mode::ReadOnly(_) = facts.mode {
            return refuse(SyncRefusal::ReadOnly);
        }
        // tC4's order: signed in, online, then the repository (syncOperation.ts:580-584).
        let Some(session) = self.session.clone() else {
            return refuse(SyncRefusal::NotSignedIn);
        };
        if !self.client.internet().is_on() {
            return refuse(SyncRefusal::Offline);
        }
        let Some(git_dir) = facts.git_dir else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        let Some((url, branch)) = origin(&git_dir) else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        let Some(repo) = repo_of(self.client, &url) else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        let empty = match self.client.repo(&repo, Some(&session)).await {
            Ok(info) if !info.can_push => {
                return refuse(SyncRefusal::NoWriteAccess {
                    server_message: String::new(),
                });
            }
            Ok(info) => info.empty,
            Err(e) => return refuse(refusal_of(&e)),
        };
        self.target = Some(Target {
            git_dir,
            remote: GitRemote {
                server: self.client.server().clone(),
                repo,
                session: Some(session),
            },
            branch,
            empty,
        });
        SyncEvent::GatePassed
    }

    /// S3, S4: Door43's branch into `refs/remotes/origin/{branch}`, compared with HEAD.
    async fn fetch(&mut self) -> SyncEvent {
        let Some(target) = self.target.clone() else {
            return SyncEvent::FetchFailed(SyncRefusal::NotSignedIn);
        };
        let http = self.client.http().clone();
        let fetched = tokio::task::spawn_blocking(move || {
            let repo = open(&target.git_dir)?;
            let tracking = format!("refs/remotes/origin/{}", target.branch);
            // A fetch does not prune: Door43's list says whether the branch is there. A
            // missing one is a first push for an empty repository this project never fetched
            // from; else its owner deleted it, and the push recreates it from this copy, as
            // tC4 does (gap-tc4-straggler-sync-safety 3b; owner ruling 2026-10-24).
            let advertised = tc_door43::git::ls_remote(&http, &repo, &target.remote)
                .map_err(|e| refusal_of(&e))?;
            let name = format!("refs/heads/{}", target.branch);
            if !advertised.iter().any(|(n, _)| *n == name) {
                let fetched_before = repo.refname_to_id(&tracking).is_ok();
                return Ok(Err(!target.empty || fetched_before));
            }
            let refspec = format!("+{name}:{tracking}");
            tc_door43::git::fetch(&http, &repo, &target.remote, &[refspec.as_str()])
                .map_err(|e| refusal_of(&e))?;
            let remote = repo.refname_to_id(&tracking).map_err(|e| SyncRefusal::Io {
                reason: e.to_string(),
            })?;
            let new = match repo.head().ok().and_then(|h| h.target()) {
                Some(head) => {
                    head != remote && !repo.graph_descendant_of(head, remote).unwrap_or(false)
                }
                None => true,
            };
            Ok(Ok((remote, new)))
        })
        .await;
        match fetched {
            Ok(Ok(Ok((remote, new)))) => {
                self.fetched = Some(remote);
                SyncEvent::Fetched(if new { Remote::New } else { Remote::NothingNew })
            }
            Ok(Ok(Err(deleted))) => {
                self.fetched = None;
                // Each round reads Door43 afresh: a teammate who made the branch again
                // while this round raced is not reported as this copy put back.
                self.branch_gone = deleted;
                SyncEvent::Fetched(Remote::NothingNew)
            }
            Ok(Err(refusal)) => SyncEvent::FetchFailed(refusal),
            Err(e) => SyncEvent::FetchFailed(SyncRefusal::Io {
                reason: e.to_string(),
            }),
        }
    }

    /// §9.2: the format and the project of the fetched tree, and the packs this project's
    /// HEAD does not hold. It reads the object database only.
    async fn read_remote(&mut self) -> SyncEvent {
        let (Some(target), Some(remote)) = (self.target.clone(), self.fetched) else {
            return SyncEvent::RemoteUnreadable {
                reason: "nothing was fetched".into(),
            };
        };
        let read = tokio::task::spawn_blocking(move || read_tree(&target.git_dir, remote)).await;
        match read {
            Ok(Ok(Intake::Packs(packs, other_tools))) => {
                self.count_received(&packs);
                self.packs = packs;
                for commit in other_tools {
                    if !self.report.other_tools.contains(&commit) {
                        self.report.other_tools.push(commit);
                    }
                }
                SyncEvent::RemoteRead
            }
            Ok(Ok(Intake::Blocked(block))) => SyncEvent::RemoteBlocked(block),
            Ok(Err(reason)) => SyncEvent::RemoteUnreadable { reason },
            Err(e) => SyncEvent::RemoteUnreadable {
                reason: e.to_string(),
            },
        }
    }

    /// §9.3 and S7: the store takes the packs in and makes the merge commit in one job, so
    /// the received work and the saves made since S1 are in it.
    async fn take_in(&mut self) -> SyncEvent {
        let Some(remote) = self.fetched else {
            return SyncEvent::TakeInFailed {
                reason: "nothing was fetched".into(),
            };
        };
        let Ok(remote) = remote
            .as_bytes()
            .try_into()
            .map(tc_journal::GitOid::from_bytes)
        else {
            return SyncEvent::TakeInFailed {
                reason: "a commit id that is not 20 bytes".into(),
            };
        };
        let packs = std::mem::take(&mut self.packs);
        match self.project.sync_merge(remote, packs).await {
            Ok(report) => match report.skipped {
                // Nothing was taken in: the store refused before its intake.
                Some(CheckpointSkip::ReadOnly | CheckpointSkip::SecondWriter) => {
                    SyncEvent::TakeInFailed {
                        reason: format!("{:?}", report.skipped),
                    }
                }
                _ => {
                    add(&mut self.report.received, &report.intake);
                    self.merged = Some(Ok(report));
                    SyncEvent::TakenIn
                }
            },
            // The store's job takes the packs in pack by pack before it seals and commits:
            // a failure after some were folded leaves received work in the project, so the
            // sync must not say nothing changed (PLAN Q8).
            Err(e) if self.some_received_is_held() => {
                self.merged = Some(Err(e.to_string()));
                SyncEvent::TakenIn
            }
            Err(e) => SyncEvent::TakeInFailed {
                reason: e.to_string(),
            },
        }
    }

    /// Whether the project holds any change of the received packs now.
    fn some_received_is_held(&self) -> bool {
        let snapshot = self.project.snapshot();
        self.received_ids
            .iter()
            .any(|id| snapshot.fold().change(id).is_some())
    }

    /// S8: HEAD onto Door43's branch, a fast-forward or nothing. A large push goes as a chain
    /// (§9.5): each link is pushed and read back before the next, so a dropped connection
    /// loses only the link in flight and the next sync goes on from the last one confirmed.
    async fn push(&mut self) -> SyncEvent {
        let Some(target) = self.target.clone() else {
            return SyncEvent::PushFailed(SyncRefusal::NotSignedIn);
        };
        let http = self.client.http().clone();
        let remote = self.fetched;
        let bound = self.project.chain_bytes();
        let facts = self.project.facts();
        let (device, project) = (facts.device, facts.id);
        let pushed = tokio::task::spawn_blocking(move || {
            let mut chain = Chain::default();
            let result = push_chain(&http, &target, remote, bound, (device, project), &mut chain);
            (result, chain)
        })
        .await;
        let (result, chain) = match pushed {
            Ok(done) => done,
            Err(e) => {
                return SyncEvent::PushFailed(SyncRefusal::Io {
                    reason: e.to_string(),
                });
            }
        };
        if let Some(link) = chain.confirmed {
            self.report.confirmed = Some(link.to_string());
        }
        match result {
            Ok(restored) => {
                // Said only once the push made the branch again (tC4 reports it after the push).
                self.report.recreated = self.branch_gone || self.put_back(None).await;
                self.report.restored += restored;
                SyncEvent::Pushed
            }
            Err(Pushed::Nothing) => {
                self.report.recreated = self.branch_gone;
                SyncEvent::Pushed
            }
            Err(Pushed::Raced) => SyncEvent::PushRaced,
            Err(Pushed::Failed(refusal)) => {
                if self.branch_gone && chain.confirmed.is_some() {
                    // A link made the branch again; this failed sync cannot say so.
                    self.put_back(Some(true)).await;
                }
                if refusal == SyncRefusal::Unreachable
                    && let Some(tip) = chain.tip
                {
                    self.report.offer_exchange =
                        self.chain_failed(tip.to_string(), chain.links > 0).await;
                }
                SyncEvent::PushFailed(refusal)
            }
        }
    }

    /// The mark of a branch put back that no report has said yet: `Some(true)` sets it, `None`
    /// reads and clears it. A file that cannot be read or written holds no mark.
    async fn put_back(&self, set: Option<bool>) -> bool {
        let Ok(Some(mut file)) = self.project.sync_file().await else {
            return false;
        };
        let was = file.put_back_unsaid;
        let now = set.unwrap_or(false);
        if was != now {
            file.put_back_unsaid = now;
            let _ = self.project.set_sync_file(file).await;
        }
        was
    }

    /// Counts a large push's failure for want of a connection in `.tc/sync.json`; `true` from
    /// the third in a row. Saves made between the tries give the chain a new tip but leave it
    /// the same push, so only a confirmed push starts the count again. A push with no links
    /// counts only as the rest of a chain that failed before: its last commit, after every link
    /// went through. A file that cannot be written counts nothing.
    async fn chain_failed(&self, tip: String, chained: bool) -> bool {
        let Ok(Some(mut file)) = self.project.sync_file().await else {
            return false;
        };
        if !chained && file.chain_failures.is_none() {
            return false;
        }
        let count = file
            .chain_failures
            .as_ref()
            .map_or(1, |f| f.count.saturating_add(1));
        file.chain_failures = Some(tc_store::ChainFailures { tip, count });
        let _ = self.project.set_sync_file(file).await;
        count >= 3
    }

    /// S9: Door43's branch read back; at HEAD, it is confirmed.
    async fn confirm(&mut self) -> SyncEvent {
        let Some(target) = self.target.clone() else {
            return SyncEvent::ConfirmFailed;
        };
        let http = self.client.http().clone();
        let read = tokio::task::spawn_blocking(move || {
            let repo = open(&target.git_dir).ok()?;
            let head = repo.head().ok()?.target()?;
            let advertised = tc_door43::git::ls_remote(&http, &repo, &target.remote).ok()?;
            let name = format!("refs/heads/{}", target.branch);
            let at = advertised
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, oid)| oid.to_string());
            let equal = at.as_deref() == Some(head.to_string().as_str());
            if equal {
                let _ = repo.reference(
                    &format!("refs/remotes/origin/{}", target.branch),
                    head,
                    true,
                    "confirmed by tC",
                );
            }
            Some((equal, head.to_string()))
        })
        .await;
        match read {
            Ok(Some((equal, head))) => {
                if equal {
                    // "Your work up to 14:32 is on Door43" reads this; a file that cannot be
                    // written leaves the sync confirmed, and the next one writes it.
                    let _ = self.project.confirm_sync(head.clone()).await;
                    self.report.confirmed = Some(head);
                }
                SyncEvent::Confirmed { equal }
            }
            _ => SyncEvent::ConfirmFailed,
        }
    }
}

/// What a push did of its chain (§9.5).
#[derive(Default)]
struct Chain {
    /// The commit the push ends at.
    tip: Option<git2::Oid>,
    /// Links before it.
    links: usize,
    /// The last link Door43's branch was read back at.
    confirmed: Option<git2::Oid>,
}

/// S8 on the blocking thread: the links of the chain, each pushed and read back, then HEAD.
fn push_chain(
    http: &tc_door43::Http,
    target: &Target,
    remote: Option<git2::Oid>,
    bound: u64,
    (device, project): (tc_journal::DeviceId, tc_journal::ProjectId),
    chain: &mut Chain,
) -> Result<u32, Pushed> {
    let repo = open(&target.git_dir).map_err(Pushed::Failed)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .ok_or(Pushed::Nothing)?;
    chain.tip = Some(head);
    // Door43's branch behind ours holds fewer packs than HEAD: the teammates' changes in the
    // packs it lacks are sent again (§9.7 `restored`); our own are new work.
    let restored = match remote {
        Some(remote) if remote != head => restored_changes(&repo, head, remote, device, project),
        _ => 0,
    };
    let local = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(str::to_owned))
        .unwrap_or_else(|| target.branch.clone());
    let links = links_of(&repo, head, remote, bound).unwrap_or_default();
    chain.links = links.len();
    let sent = (|| {
        for link in links {
            repo.reference(
                tc_door43::git::LINK_REF,
                link,
                true,
                "a link of a large push",
            )
            .map_err(|e| {
                Pushed::Failed(SyncRefusal::Io {
                    reason: e.message().to_owned(),
                })
            })?;
            let refspec = FastForwardRefspec::link_onto(&target.branch)
                .map_err(|e| Pushed::Failed(refusal_of(&e)))?;
            send(http, &repo, &target.remote, &refspec)?;
            if read_back(http, &repo, target)? != Some(link) {
                // Door43's branch is elsewhere: someone pushed after this link.
                return Err(Pushed::Raced);
            }
            chain.confirmed = Some(link);
        }
        Ok(())
    })();
    if let Ok(mut made) = repo.find_reference(tc_door43::git::LINK_REF) {
        let _ = made.delete();
    }
    sent?;
    let refspec = FastForwardRefspec::onto(&local, &target.branch)
        .map_err(|e| Pushed::Failed(refusal_of(&e)))?;
    send(http, &repo, &target.remote, &refspec)?;
    Ok(restored)
}

fn send(
    http: &tc_door43::Http,
    repo: &git2::Repository,
    remote: &GitRemote,
    refspec: &FastForwardRefspec,
) -> Result<(), Pushed> {
    match tc_door43::git::push(http, repo, remote, refspec) {
        Ok(()) => Ok(()),
        Err(Door43Error::RefMoved { .. }) => Err(Pushed::Raced),
        Err(e) => Err(Pushed::Failed(refusal_of(&e))),
    }
}

/// Door43's branch as it advertises it now; at a commit, `refs/remotes/origin/{b}` is set
/// to it, so the next sync's fetch starts from there.
fn read_back(
    http: &tc_door43::Http,
    repo: &git2::Repository,
    target: &Target,
) -> Result<Option<git2::Oid>, Pushed> {
    let advertised = tc_door43::git::ls_remote(http, repo, &target.remote)
        .map_err(|e| Pushed::Failed(refusal_of(&e)))?;
    let name = format!("refs/heads/{}", target.branch);
    let at = advertised
        .iter()
        .find(|(n, _)| *n == name)
        .and_then(|(_, oid)| git2::Oid::from_str(&oid.to_string()).ok());
    if let Some(at) = at {
        let _ = repo.reference(
            &format!("refs/remotes/origin/{}", target.branch),
            at,
            true,
            "confirmed by tC",
        );
    }
    Ok(at)
}

/// §9.5: the commits a push sends before `head`, oldest first. On the way from Door43's
/// commit `remote` (or the first commit) to `head`, each commit whose new blobs bring the
/// run since the last link to `bound` bytes is a link. Every one is a descendant of `remote`,
/// so each push is a fast-forward of the one before. None when `head` is not reached from
/// `remote` this way.
fn links_of(
    repo: &git2::Repository,
    head: git2::Oid,
    remote: Option<git2::Oid>,
    bound: u64,
) -> Result<Vec<git2::Oid>, git2::Error> {
    let mut way: Vec<(git2::Oid, Option<git2::Oid>)> = Vec::new();
    let mut at = head;
    while Some(at) != remote {
        let commit = repo.find_commit(at)?;
        let mut next = None;
        for parent in commit.parent_ids() {
            let on_the_way = match remote {
                None => true,
                Some(r) => parent == r || repo.graph_descendant_of(parent, r).unwrap_or(false),
            };
            if on_the_way {
                next = Some(parent);
                break;
            }
        }
        way.push((at, next));
        match next {
            Some(parent) => at = parent,
            None if remote.is_none() => break,
            None => return Ok(Vec::new()),
        }
    }
    let odb = repo.odb()?;
    let mut links = Vec::new();
    let mut run = 0u64;
    for (commit, parent) in way.into_iter().rev() {
        let old = parent
            .map(|p| repo.find_commit(p).and_then(|c| c.tree()))
            .transpose()?;
        let new = repo.find_commit(commit)?.tree()?;
        let diff = repo.diff_tree_to_tree(old.as_ref(), Some(&new), None)?;
        for delta in diff.deltas() {
            let blob = delta.new_file().id();
            if !blob.is_zero() {
                run += odb
                    .read_header(blob)
                    .map(|(size, _)| size as u64)
                    .unwrap_or(0);
            }
        }
        if run >= bound && commit != head {
            links.push(commit);
            run = 0;
        }
    }
    Ok(links)
}

enum Pushed {
    /// No commit to push yet.
    Nothing,
    Raced,
    Failed(SyncRefusal),
}

enum Intake {
    Packs(Vec<ReceivedPack>, Vec<OtherToolCommit>),
    Blocked(SyncBlock),
}

fn add(sum: &mut IntakeReport, more: &IntakeReport) {
    sum.packs += more.packs;
    sum.salvaged += more.salvaged;
    sum.held += more.held;
    sum.left.extend(more.left.iter().cloned());
}

fn open(git_dir: &std::path::Path) -> Result<git2::Repository, SyncRefusal> {
    git2::Repository::open(git_dir).map_err(|e| SyncRefusal::Io {
        reason: e.message().to_owned(),
    })
}

/// `remote.origin.url` and `tc.branch` of the project's repository.
fn origin(git_dir: &std::path::Path) -> Option<(String, String)> {
    let repo = git2::Repository::open(git_dir).ok()?;
    let config = repo.config().ok()?;
    let url = config.get_string("remote.origin.url").ok()?;
    let branch = config.get_string("tc.branch").ok()?;
    (!url.is_empty() && !branch.is_empty()).then_some((url, branch))
}

/// The repository an `origin` address names, when it is on the client's server and carries
/// no credentials.
fn repo_of(client: &Door43Client, url: &str) -> Option<tc_door43::RepoRef> {
    let url = url::Url::parse(url).ok()?;
    if !client.server().is_same_origin(&url)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let path = url.path().trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    path.parse().ok()
}

/// §9.2 N1 to N3 over the fetched commit's tree.
/// The changes of other devices in the packs HEAD holds and `remote` does not. Zero when the
/// trees cannot be read: a count is never a reason to stop a push.
fn restored_changes(
    repo: &git2::Repository,
    head: git2::Oid,
    remote: git2::Oid,
    device: tc_journal::DeviceId,
    project: tc_journal::ProjectId,
) -> u32 {
    let packs_of = |commit: git2::Oid| -> Option<std::collections::BTreeMap<String, git2::Oid>> {
        let tree = repo.find_commit(commit).ok()?.tree().ok()?;
        let mut packs = std::collections::BTreeMap::new();
        let _ = tree.walk(git2::TreeWalkMode::PreOrder, |root, entry| {
            let path = format!("{root}{}", entry.name().unwrap_or_default());
            if entry.kind() == Some(git2::ObjectType::Blob) && path.starts_with(PACKS) {
                packs.insert(path, entry.id());
            }
            git2::TreeWalkResult::Ok
        });
        Some(packs)
    };
    let (Some(ours), Some(theirs)) = (packs_of(head), packs_of(remote)) else {
        return 0;
    };
    ours.iter()
        .filter(|(path, _)| !theirs.contains_key(*path))
        .filter(|(path, _)| tc_store::pack_sealer(path).is_some_and(|sealer| sealer != device))
        .filter_map(|(path, oid)| {
            let blob = repo.find_blob(*oid).ok()?;
            let read = tc_store::read_received_pack(path, blob.content(), project);
            // Bookkeeping is not work sent again, as in `count_received`.
            let work = read
                .changes
                .iter()
                .filter(|c| !crate::is_bookkeeping(&c.body().record))
                .count();
            Some(work as u32)
        })
        .sum()
}

fn read_tree(git_dir: &std::path::Path, remote: git2::Oid) -> Result<Intake, String> {
    let repo = git2::Repository::open(git_dir).map_err(|e| e.message().to_owned())?;
    let theirs = repo
        .find_commit(remote)
        .and_then(|c| c.tree())
        .map_err(|e| e.message().to_owned())?;
    let ours = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let blob_at = |tree: &git2::Tree<'_>, path: &str| -> Option<Vec<u8>> {
        let entry = tree.get_path(std::path::Path::new(path)).ok()?;
        let blob = repo.find_blob(entry.id()).ok()?;
        Some(blob.content().to_vec())
    };
    // N1: a newer format, or tC4's team format.
    if let Some(block) = gate_of(&repo, &theirs) {
        return Ok(Intake::Blocked(block));
    }
    // N2: two projects are never merged.
    let id = |bytes: Option<Vec<u8>>| -> Option<String> {
        let json: serde_json::Value = serde_json::from_slice(&bytes?).ok()?;
        json.get("projectId")?.as_str().map(str::to_owned)
    };
    let theirs_id = id(blob_at(&theirs, PROJECT_JSON));
    let ours_id = ours.as_ref().and_then(|t| id(blob_at(t, PROJECT_JSON)));
    if let (Some(a), Some(b)) = (&theirs_id, &ours_id)
        && a != b
    {
        return Ok(Intake::Blocked(SyncBlock::ProjectMismatch));
    }
    // N3: every pack of theirs that our HEAD does not hold.
    let mut packs = Vec::new();
    theirs
        .walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            let path = format!("{dir}{}", entry.name().unwrap_or_default());
            if !path.starts_with(PACKS) || entry.kind() != Some(git2::ObjectType::Blob) {
                return git2::TreeWalkResult::Ok;
            }
            let held = ours
                .as_ref()
                .and_then(|t| t.get_path(std::path::Path::new(&path)).ok())
                .is_some_and(|e| e.id() == entry.id());
            if !held && let Ok(blob) = repo.find_blob(entry.id()) {
                packs.push(ReceivedPack {
                    path,
                    bytes: blob.content().to_vec(),
                    blob: tc_journal::GitOid::from_bytes(
                        entry.id().as_bytes().try_into().unwrap_or([0; 20]),
                    ),
                });
            }
            git2::TreeWalkResult::Ok
        })
        .map_err(|e| e.message().to_owned())?;
    let head = repo.head().ok().and_then(|h| h.target());
    Ok(Intake::Packs(packs, other_tools(&repo, remote, head)?))
}

/// §9.2 N4: the commits of Door43's branch this history lacks that no translationCore made
/// (no `Tc-Frontier` trailer) and that changed a file of the project, oldest first. Nothing
/// is merged from them: the merge tree takes the fold's projection. Once merged, they are in
/// this history, so the next sync does not walk them again.
fn other_tools(
    repo: &git2::Repository,
    remote: git2::Oid,
    head: Option<git2::Oid>,
) -> Result<Vec<OtherToolCommit>, String> {
    let git = |e: git2::Error| e.message().to_owned();
    let mut walk = repo.revwalk().map_err(git)?;
    walk.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::REVERSE)
        .map_err(git)?;
    walk.push(remote).map_err(git)?;
    if let Some(head) = head {
        walk.hide(head).map_err(git)?;
    }
    let mut out = Vec::new();
    for id in walk {
        let commit = repo.find_commit(id.map_err(git)?).map_err(git)?;
        let message = commit.message().unwrap_or_default();
        if message.lines().any(|line| line.starts_with("Tc-Frontier:")) {
            continue;
        }
        let tree = commit.tree().map_err(git)?;
        let parent = commit.parent(0).ok().and_then(|p| p.tree().ok());
        let diff = repo
            .diff_tree_to_tree(parent.as_ref(), Some(&tree), None)
            .map_err(git)?;
        let mut paths = Vec::new();
        let mut books = Vec::new();
        for delta in diff.deltas() {
            let Some(path) = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str())
            else {
                continue;
            };
            let project_file = path.starts_with("ingredients/")
                && !path.starts_with(PACKS)
                && !path.starts_with("ingredients/tc/")
                && !path.split('/').any(|part| part.starts_with('.'));
            if !project_file {
                continue;
            }
            if let Some(book) =
                tc_journal::fold::DerivedFile::from_path(path).and_then(|f| f.book())
                && !books.contains(&book)
            {
                books.push(book);
            }
            paths.push(path.to_owned());
        }
        if !paths.is_empty() {
            let hex = commit.id().to_string();
            out.push(OtherToolCommit {
                commit: hex[..7].to_owned(),
                author: commit.author().name().unwrap_or_default().to_owned(),
                books,
                paths,
            });
        }
    }
    Ok(out)
}

/// §9.2 N1 for a tree: a newer format than this app reads, or tC4's team format.
pub(crate) fn gate_of(repo: &git2::Repository, tree: &git2::Tree<'_>) -> Option<SyncBlock> {
    let blob_at = |path: &str| -> Option<Vec<u8>> {
        let entry = tree.get_path(std::path::Path::new(path)).ok()?;
        Some(repo.find_blob(entry.id()).ok()?.content().to_vec())
    };
    match blob_at(FORMAT_JSON) {
        Some(bytes) => match tc_journal::format::FormatDoc::parse(&bytes).map(|d| (d.gate(), d)) {
            Ok((tc_journal::format::FormatGate::Refused { major }, doc)) => {
                let number = |n: &tc_journal::format::FormatNumber| {
                    n.to_string().parse::<u16>().unwrap_or(u16::MAX)
                };
                Some(SyncBlock::NeedsNewerApp {
                    major: number(&major),
                    min_reader_minor: number(&doc.min_reader_minor),
                })
            }
            _ => None,
        },
        None if tree
            .get_path(std::path::Path::new(TC4_JOURNAL.trim_end_matches('/')))
            .is_ok() =>
        {
            Some(SyncBlock::Tc4TeamRepository)
        }
        None => None,
    }
}

/// A Door43 error as the refusal the person reads (§9.5).
pub(crate) fn refusal_of(error: &Door43Error) -> SyncRefusal {
    match error {
        Door43Error::Offline { .. } => SyncRefusal::Offline,
        Door43Error::Unreachable { .. } => SyncRefusal::Unreachable,
        Door43Error::AuthFailed { .. } => SyncRefusal::AuthFailed,
        Door43Error::NoWriteAccess { server_message, .. } => SyncRefusal::NoWriteAccess {
            server_message: server_message.clone(),
        },
        Door43Error::RefRejected { server_message, .. } => SyncRefusal::RefRejected {
            server_message: server_message.clone(),
        },
        Door43Error::RefMoved { .. } => SyncRefusal::Busy {
            attempts: MAX_ATTEMPTS,
        },
        Door43Error::NotFound { .. } => SyncRefusal::RemoteMissing,
        other => SyncRefusal::Io {
            reason: other.to_string(),
        },
    }
}
