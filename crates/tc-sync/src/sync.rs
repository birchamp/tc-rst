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
        },
    };
    let (mut state, mut actions) = next(&SyncState::Idle, &SyncEvent::Start);
    while let Some(action) = actions.pop() {
        if let SyncAction::Report(ended) = action {
            run.report.ended = ended;
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
    packs: Vec<ReceivedPack>,
    /// The merge's answer: the take-in and the commit come back together.
    merged: Option<Result<CheckpointReport, String>>,
    report: SyncReport,
}

impl Run<'_> {
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
        let Some(git_dir) = facts.git_dir else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        let Some((url, branch)) = origin(&git_dir) else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        let Some(repo) = repo_of(self.client, &url) else {
            return SyncEvent::GateRefused(GateRefusal::Block(SyncBlock::NotShared));
        };
        if !self.client.internet().is_on() {
            return refuse(SyncRefusal::Offline);
        }
        let Some(session) = self.session.clone() else {
            return refuse(SyncRefusal::NotSignedIn);
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
            // missing one is a first push only for an empty repository this project never
            // fetched from; else its owner deleted it (gap-tc4-straggler-sync-safety, 3b).
            let advertised = tc_door43::git::ls_remote(&http, &repo, &target.remote)
                .map_err(|e| refusal_of(&e))?;
            let name = format!("refs/heads/{}", target.branch);
            if !advertised.iter().any(|(n, _)| *n == name) {
                let fetched_before = repo.refname_to_id(&tracking).is_ok();
                return Ok(if target.empty && !fetched_before {
                    None
                } else {
                    Some(None)
                });
            }
            let refspec = format!("+{name}:{tracking}");
            tc_door43::git::fetch(&http, &repo, &target.remote, &[refspec.as_str()])
                .map_err(|e| refusal_of(&e))?;
            let Ok(remote) = repo.refname_to_id(&tracking) else {
                return Ok(Some(None));
            };
            let new = match repo.head().ok().and_then(|h| h.target()) {
                Some(head) => {
                    head != remote && !repo.graph_descendant_of(head, remote).unwrap_or(false)
                }
                None => true,
            };
            Ok(Some(Some((remote, new))))
        })
        .await;
        match fetched {
            Ok(Ok(Some(Some((remote, new))))) => {
                self.fetched = Some(remote);
                SyncEvent::Fetched(if new { Remote::New } else { Remote::NothingNew })
            }
            Ok(Ok(Some(None))) => SyncEvent::Fetched(Remote::BranchMissing),
            Ok(Ok(None)) => {
                self.fetched = None;
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
            Ok(Ok(Intake::Packs(packs))) => {
                self.packs = packs;
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
            Err(e) => SyncEvent::TakeInFailed {
                reason: e.to_string(),
            },
        }
    }

    /// S8: HEAD onto Door43's branch, a fast-forward or nothing.
    async fn push(&mut self) -> SyncEvent {
        let Some(target) = self.target.clone() else {
            return SyncEvent::PushFailed(SyncRefusal::NotSignedIn);
        };
        let http = self.client.http().clone();
        let pushed = tokio::task::spawn_blocking(move || {
            let repo = open(&target.git_dir).map_err(Pushed::Failed)?;
            let head = repo
                .head()
                .ok()
                .and_then(|h| h.target())
                .ok_or(Pushed::Nothing)?;
            let local = repo
                .head()
                .ok()
                .and_then(|h| h.shorthand().ok().map(str::to_owned))
                .unwrap_or_else(|| target.branch.clone());
            let refspec = FastForwardRefspec::onto(&local, &target.branch)
                .map_err(|e| Pushed::Failed(refusal_of(&e)))?;
            match tc_door43::git::push(&http, &repo, &target.remote, &refspec) {
                Ok(()) => Ok(head),
                Err(Door43Error::RefMoved { .. }) => Err(Pushed::Raced),
                Err(e) => Err(Pushed::Failed(refusal_of(&e))),
            }
        })
        .await;
        match pushed {
            Ok(Ok(_)) | Ok(Err(Pushed::Nothing)) => SyncEvent::Pushed,
            Ok(Err(Pushed::Raced)) => SyncEvent::PushRaced,
            Ok(Err(Pushed::Failed(refusal))) => SyncEvent::PushFailed(refusal),
            Err(e) => SyncEvent::PushFailed(SyncRefusal::Io {
                reason: e.to_string(),
            }),
        }
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
                    self.report.confirmed = Some(head);
                }
                SyncEvent::Confirmed { equal }
            }
            _ => SyncEvent::ConfirmFailed,
        }
    }
}

enum Pushed {
    /// No commit to push yet.
    Nothing,
    Raced,
    Failed(SyncRefusal),
}

enum Intake {
    Packs(Vec<ReceivedPack>),
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
    match blob_at(&theirs, FORMAT_JSON) {
        Some(bytes) => {
            if let Ok(doc) = tc_journal::format::FormatDoc::parse(&bytes)
                && let tc_journal::format::FormatGate::Refused { major } = doc.gate()
            {
                let number = |n: &tc_journal::format::FormatNumber| {
                    n.to_string().parse::<u16>().unwrap_or(u16::MAX)
                };
                return Ok(Intake::Blocked(SyncBlock::NeedsNewerApp {
                    major: number(&major),
                    min_reader_minor: number(&doc.min_reader_minor),
                }));
            }
        }
        None if theirs
            .get_path(std::path::Path::new(TC4_JOURNAL.trim_end_matches('/')))
            .is_ok() =>
        {
            return Ok(Intake::Blocked(SyncBlock::Tc4TeamRepository));
        }
        None => {}
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
    Ok(Intake::Packs(packs))
}

/// A Door43 error as the refusal the person reads (§9.5).
fn refusal_of(error: &Door43Error) -> SyncRefusal {
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
