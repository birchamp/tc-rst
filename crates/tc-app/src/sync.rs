//! Sync with Door43 from a Home card (unit B10; tC4 #678): the `door43/sync` job and what its
//! toast shows. The sync's own steps are unit X's: until X lands, the outcome a test scripted
//! answers for it (AD6, `test/script`), and a sync with none answers `Unavailable(Sync)`.
//!
//! A scripted sync that receives teammates' changes writes them into the project as X's intake
//! will, then reads the project again, so the toast, the card's conflict chip and the tiles'
//! conflict counts all come from the fold's own conflicts (X12).

use std::collections::BTreeMap;

use tc_api::ApiError;
use tc_api::door43::SyncProject;
use tc_api::error::Feature;
use tc_api::ids::{JobId, ProjectRef};
use tc_api::jobs::{JobKind, Phase, SyncPhase, SyncResultView};
use tc_api::shown::PersonLabel;
use tc_journal::fold::{ConflictCounts, ConflictKind};
use tc_journal::{ChangeId, DeviceId};
use tc_resources::{ProjectPins, ResourcePin};

use crate::app::App;
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::flows::FlowRequest;
use crate::jobs::{JobEnd, JobSpec};

/// A project's open conflicts as the toast counts them: verse conflicts (the kinds
/// `ConflictKind::is_verse` names, as Home's chip counts them), verses with two alignments
/// (#791), and every other kind, which shows its newest version until it is reviewed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConflictTally {
    pub verse: u32,
    pub alignments: u32,
    pub others: u32,
}

impl ConflictTally {
    /// From the fold's counts by book and kind, so the toast walks no conflict list.
    pub fn of(counts: &ConflictCounts) -> Self {
        let verse = [
            ConflictKind::VerseText,
            ConflictKind::VerseStructure,
            ConflictKind::Passage,
            ConflictKind::StructureIssue,
        ]
        .map(|kind| kind.order());
        let alignment = ConflictKind::Alignment.order();
        let mut tally = Self::default();
        for (&(_, kind), &n) in counts {
            let slot = if verse.contains(&kind) {
                &mut tally.verse
            } else if kind == alignment {
                &mut tally.alignments
            } else {
                &mut tally.others
            };
            *slot = slot.saturating_add(n);
        }
        tally
    }
}

/// What a completed sync's toast shows, read from the project after the sync: `received` are
/// the teammates' changes it took in, each sender named by the fold's label for its device
/// (the one label the review gives, #680), and `installed` answers whether this computer holds
/// a pinned version.
pub fn synced_view(
    snapshot: &CountingSnapshot,
    me: DeviceId,
    received: &[ChangeId],
    restored: u32,
    installed: &dyn Fn(&ResourcePin) -> bool,
) -> SyncResultView {
    let tally = ConflictTally::of(snapshot.conflict_counts());
    let labels = snapshot.person_labels(me);
    let senders: BTreeMap<DeviceId, PersonLabel> = received
        .iter()
        .filter_map(|id| snapshot.body(id))
        .filter_map(|body| {
            let label = labels.get(&body.device)?;
            Some((body.device, crate::wire::scope::person_label(label)))
        })
        .collect();
    let mut from: Vec<PersonLabel> = Vec::new();
    for label in senders.into_values() {
        if !from.contains(&label) {
            from.push(label);
        }
    }
    SyncResultView {
        received: u32::try_from(received.len()).unwrap_or(u32::MAX),
        from,
        kept_newest: tally.others,
        verse_conflicts: tally.verse,
        alignments: tally.alignments,
        restored,
        missing_pins: missing_pins(snapshot, installed),
    }
}

/// tC4's `missingPinsAfterSync`: 0 when the pins cannot be read.
fn missing_pins(snapshot: &CountingSnapshot, installed: &dyn Fn(&ResourcePin) -> bool) -> u32 {
    let pins = snapshot
        .render(&tc_journal::fold::DerivedFile::Resources)
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .and_then(|json| ProjectPins::from_resources_json(&json).ok());
    pins.map_or(0, |pins| {
        let n = tc_resources::resolve::missing_pin_count(&pins, installed);
        u32::try_from(n).unwrap_or(u32::MAX)
    })
}

/// The sync before unit X: how the next sync of each project ends, as a test set it. A test
/// sets it from what Door43 holds, so each setting replaces the one before, as a change on
/// Door43 changes what the next sync finds.
#[cfg(feature = "test-hooks")]
#[derive(Default)]
pub(crate) struct ScriptedSync {
    next: std::sync::Mutex<std::collections::HashMap<ProjectRef, tc_api::test::ScriptedSync>>,
}

#[cfg(feature = "test-hooks")]
impl ScriptedSync {
    fn take(&self, project: &ProjectRef) -> Option<tc_api::test::ScriptedSync> {
        crate::app::lock(&self.next).remove(project)
    }

    pub(crate) fn clear(&self) {
        crate::app::lock(&self.next).clear();
    }
}

impl App {
    /// The next `door43/sync` of `project` ends with `outcome`, whatever was set before.
    #[cfg(feature = "test-hooks")]
    pub fn script_sync(&self, project: ProjectRef, outcome: tc_api::test::ScriptedSync) {
        crate::app::lock(&self.inner.sync.next).insert(project, outcome);
    }

    /// One sync of `folder`, after the checkpoint of the project just left (tC4 `syncRun`).
    async fn sync_job(&self, folder: ProjectRef) -> Result<SyncResultView, JobEnd> {
        self.inner.sessions.settled(&folder).await;
        #[cfg(feature = "test-hooks")]
        if let Some(outcome) = self.inner.sync.take(&folder) {
            return self.scripted_sync(folder, outcome).await;
        }
        Err(JobEnd::Failed(ApiError::Unavailable(Feature::Sync)))
    }

    #[cfg(feature = "test-hooks")]
    async fn scripted_sync(
        &self,
        folder: ProjectRef,
        outcome: tc_api::test::ScriptedSync,
    ) -> Result<SyncResultView, JobEnd> {
        use serde_json::json;
        use tc_api::test::ScriptedSync as Scripted;
        let (teammates, joined, restored) = match outcome {
            Scripted::Failed(failure) => {
                return Err(JobEnd::Failed(ApiError::Refused(tc_api::Refusal::Sync(
                    failure,
                ))));
            }
            Scripted::Synced {
                teammates,
                joined,
                restored,
            } => (teammates, joined, restored),
        };
        let failed = |what: &str, reason: String| {
            JobEnd::Failed(self.internal(what, crate::reports::facts(json!({ "reason": reason }))))
        };
        let mut received = match teammates {
            None => Vec::new(),
            Some(edit) => {
                let (Some(chapter), Some(verse)) = (
                    tc_journal::ChapterNo::new(edit.chapter),
                    tc_journal::VerseNo::new(edit.verse),
                ) else {
                    return Err(JobEnd::Failed(ApiError::BadRequest {
                        field: "teammates".to_owned(),
                        reason: "no such verse".to_owned(),
                    }));
                };
                let key = tc_journal::RecordKey::Verse(edit.book, chapter, verse);
                let at = folder.clone();
                let written = self
                    .with_store(move |store| {
                        let texts: Vec<&str> = edit.texts.iter().map(String::as_str).collect();
                        store.teammates_edit_verse(&at, &key, &texts)
                    })
                    .await
                    .map_err(JobEnd::Failed)?;
                written
                    .map_err(|e| failed("the teammates' changes were not written", e.to_string()))?
            }
        };
        if let Some(join) = joined {
            let (Some(chapter), Some(first), Some(last)) = (
                tc_journal::ChapterNo::new(join.chapter),
                tc_journal::VerseNo::new(join.first),
                tc_journal::VerseNo::new(join.last),
            ) else {
                return Err(JobEnd::Failed(ApiError::BadRequest {
                    field: "joined".to_owned(),
                    reason: "no such verse".to_owned(),
                }));
            };
            let at = folder.clone();
            let written = self
                .with_store(move |store| {
                    store.teammates_join_verses(&at, join.book, chapter, first, last)
                })
                .await
                .map_err(JobEnd::Failed)?;
            received.extend(
                written
                    .map_err(|e| failed("the teammates' join was not written", e.to_string()))?,
            );
        }
        // Read the project again as a sync's intake does: the fold takes the received changes
        // in and its files are written, so Home counts the conflicts they make.
        let progress =
            crate::jobs::ProgressSink::new(Phase::Open(tc_api::jobs::OpenPhase::FinishingWork));
        let lease = self
            .inner
            .sessions
            .open_side(&folder, progress)
            .await
            .map_err(|e| failed("the synced project did not open", format!("{e:?}")))?;
        let view = {
            let session = lease.session();
            self.inner
                .sessions
                .store()
                .synced(&folder, session.handle().snapshot().fold().clone());
            let me = session.handle().facts().device;
            let resources = &self.inner.resources;
            synced_view(&session.read(), me, &received, restored, &|pin| {
                resources.has_dcs(pin)
            })
        };
        drop(lease);
        self.inner.sessions.settled(&folder).await;
        self.inner
            .bus
            .publish(tc_api::event::Event::HomeCard { project: folder });
        Ok(view)
    }
}

impl Handle<SyncProject> for App {
    /// tC4's `syncProject` after its question: a Door43 sign-in first (the sign-in step comes
    /// before the sync, and the sync runs after it), one sync at a time (AD31), then the job.
    async fn handle(&self, req: SyncProject) -> Result<JobId, ApiError> {
        self.door43_session().await?;
        let pass = self
            .inner
            .flows
            .enter(FlowRequest::Sync(req.project.clone()))?;
        let spec = JobSpec::new(JobKind::Sync, Phase::Sync(SyncPhase::Checkpoint))
            .project(req.project.clone())
            .pass(pass);
        let app = self.clone();
        Ok(self
            .inner
            .jobs
            .start_typed::<SyncProject, _, _>(spec, move |_ctx| async move {
                app.sync_job(req.project).await
            }))
    }
}

#[cfg(feature = "test-hooks")]
impl Handle<tc_api::test::Script> for App {
    async fn handle(&self, req: tc_api::test::Script) -> Result<(), ApiError> {
        use tc_api::test::Scripted;
        self.hooks()?;
        match req.next {
            Scripted::OpenDoor43Fails(failure) => {
                self.script_open_door43(Err(ApiError::Refused(tc_api::Refusal::Sync(failure))))
            }
            Scripted::Sync { project, outcome } => self.script_sync(project, outcome),
            next @ (Scripted::Updates(_) | Scripted::Plan(_)) => {
                self.inner.scripted_resources.script(next);
            }
        }
        Ok(())
    }
}
