//! Sync with Door43 from a Home card (unit B10; tC4 #678): the `door43/sync` job and what its
//! toast shows. The sync's own steps are unit X's: until X lands, the outcome a test scripted
//! answers for it (AD6, `test/script`), and a sync with none answers `Unavailable(Sync)`.
//!
//! A scripted sync that receives teammates' changes writes them into the project as X's intake
//! will, then reads the project again, so the toast, the card's conflict chip and the tiles'
//! conflict counts all come from the fold's own conflicts (X12).

use std::collections::BTreeMap;

use tc_api::ApiError;
use tc_api::door43::{LastSyncView, SyncProject};
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
    let (replaced, records) = replaced(snapshot, received, &labels);
    SyncResultView {
        received: u32::try_from(received.len()).unwrap_or(u32::MAX),
        from,
        // tC4's "{n} conflicts kept the newest version", one per record that held two
        // versions (tC4 sync.mjs): under H5, each such record a newer version settled.
        kept_newest: records,
        verse_conflicts: tally.verse,
        alignments: tally.alignments,
        restored,
        missing_pins: missing_pins(snapshot, installed),
        replaced,
        hand_edited: Vec::new(),
    }
}

/// Owner ruling H5: each received change that won as the newest over another version, by who
/// replaced whose and of what kind ("yours" when the other version is this person's). Costs
/// the changes received.
fn replaced(
    snapshot: &CountingSnapshot,
    received: &[ChangeId],
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
) -> (Vec<tc_api::jobs::ReplacedView>, u32) {
    use tc_api::jobs::ReplacedKind;
    use tc_journal::RecordKey;
    use tc_journal::fold::{AutoRule, Resolved};
    let mut rows: Vec<((DeviceId, DeviceId), tc_api::jobs::ReplacedView)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut records = 0u32;
    for id in received {
        let Some(body) = snapshot.body(id) else {
            continue;
        };
        let key = body.record.key();
        let kind = match &key {
            RecordKey::Alignment(..) => ReplacedKind::Alignment,
            RecordKey::Check(_) => ReplacedKind::Check,
            RecordKey::Comment(_) => ReplacedKind::Comment,
            RecordKey::Setting(_)
            | RecordKey::ProjectMeta(_)
            | RecordKey::Versification
            | RecordKey::BookLayout(_) => ReplacedKind::Setting,
            RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => ReplacedKind::Resources,
            RecordKey::BookPresence(_) => ReplacedKind::Book,
            RecordKey::CarriedFile(_) => ReplacedKind::File,
            _ => continue,
        };
        if !seen.insert(key.clone()) {
            continue;
        }
        // A teammate's removal that removed a book: its verses and the rest stay in History, and
        // the sheet says so even though no second version of the book's presence lost.
        if let RecordKey::BookPresence(book) = &key
            && matches!(
                body.record,
                tc_journal::Record::BookPresence {
                    value: tc_journal::record::BookPresence::Removed { .. },
                    ..
                }
            )
            && snapshot.book(*book) == tc_journal::fold::BookView::Removed
            && let Some(by) = labels.get(&body.device)
            && labels.get(&body.device) != Some(&tc_journal::fold::PersonLabel::Mine)
        {
            rows.push((
                (body.device, body.device),
                tc_api::jobs::ReplacedView {
                    by: crate::wire::scope::person_label(by),
                    whose: crate::wire::scope::person_label(&tc_journal::fold::PersonLabel::Mine),
                    kind,
                    place: place_of(&body.record, &key),
                    count: 1,
                    was: None,
                },
            ));
            continue;
        }
        // Every record a received change touched that settled on its newest head: the received
        // change won, or lost (a teammate's decision at a check that moved), never silently.
        let Some(state) = snapshot.key_state(&key) else {
            continue;
        };
        let Resolved::SettledByRule {
            head: winner,
            rule: AutoRule::Newest,
        } = state.state
        else {
            continue;
        };
        let Some(winner_device) = snapshot.body(&winner).map(|b| b.device) else {
            continue;
        };
        let Some(by) = labels.get(&winner_device) else {
            continue;
        };
        if state.heads.len() > 1 {
            records += 1;
        }
        let won = snapshot.body(&winner).map(|b| b.record);
        for h in state.heads.iter().filter(|h| **h != winner) {
            let Some(lost) = snapshot.body(h) else {
                continue;
            };
            let loser = lost.device;
            let was = won.as_ref().and_then(|won| was_of(&lost.record, won));
            let Some(whose) = labels.get(&loser) else {
                continue;
            };
            // One row per record, as tC4 lists each replaced alignment (#791), so the person can
            // find each one; a direction's two records say it once.
            match rows.iter_mut().find(|(device, row)| {
                *device == (winner_device, loser)
                    && row.kind == kind
                    && row.was.is_some()
                    && row.was == was
            }) {
                Some(_) => {}
                None => rows.push((
                    (winner_device, loser),
                    tc_api::jobs::ReplacedView {
                        by: crate::wire::scope::person_label(by),
                        whose: crate::wire::scope::person_label(whose),
                        kind,
                        place: place_of(&body.record, &key),
                        count: 1,
                        was,
                    },
                )),
            }
        }
    }
    (rows.into_iter().map(|(_, row)| row).collect(), records)
}

/// Where a received change's row points: a comment at the verse, check or frame it is on.
fn place_of(
    record: &tc_journal::Record,
    key: &tc_journal::RecordKey,
) -> tc_api::conflicts::ReviewPlace {
    use tc_journal::RecordKey;
    use tc_journal::record::CommentTarget;
    match record {
        tc_journal::Record::Comment { value, .. } => {
            crate::conflicts::place_of(&match &value.target {
                CommentTarget::Verse {
                    book,
                    chapter,
                    verse,
                } => RecordKey::Verse(*book, *chapter, *verse),
                CommentTarget::Check(check) => RecordKey::Check(check.clone()),
                CommentTarget::Frame { story, frame } => RecordKey::Frame(*story, *frame),
            })
        }
        _ => crate::conflicts::place_of(key),
    }
}

/// The older value of a setting or of the pins that `won` replaced, as the Received sheet
/// names it: the font, the direction, or the pinned resources that differ.
fn was_of(
    lost: &tc_journal::Record,
    won: &tc_journal::Record,
) -> Option<tc_api::jobs::ReplacedValue> {
    use tc_api::jobs::ReplacedValue;
    use tc_journal::{Json, Record, keys::MetaField};
    let direction = |text: &str| match text {
        "rtl" => Some(ReplacedValue::Direction(tc_api::shown::Direction::Rtl)),
        "ltr" => Some(ReplacedValue::Direction(tc_api::shown::Direction::Ltr)),
        _ => None,
    };
    match lost {
        Record::Setting {
            key,
            value: Some(Json::Str(text)),
        } => match key.as_str() {
            "textFont" => Some(ReplacedValue::Font(text.clone())),
            "textDirection" => direction(text),
            _ => None,
        },
        Record::ProjectMeta {
            field: MetaField::Languages,
            value: Some(Json::Arr(languages)),
        } => match languages.first() {
            Some(Json::Obj(first)) => match first.get("scriptDirection") {
                Some(Json::Str(text)) => direction(text),
                _ => None,
            },
            _ => None,
        },
        Record::Pins { value: lost, .. } => {
            let Record::Pins { value: won, .. } = won else {
                return None;
            };
            let named: Vec<String> = (lost.0.iter())
                .filter(|(slot, value)| won.0.get(*slot) != Some(*value))
                .filter_map(|(_, value)| pin_name(&value.0))
                .collect();
            (!named.is_empty()).then_some(ReplacedValue::Resources(named))
        }
        _ => None,
    }
}

/// A pin as a person names it: a gateway language by its code, a resource by its repository's
/// name and version (`en_tn v90`).
fn pin_name(pin: &tc_journal::Json) -> Option<String> {
    use tc_journal::Json;
    let Json::Obj(pin) = pin else {
        return None;
    };
    let text = |key: &str| match pin.get(key) {
        Some(Json::Str(text)) => Some(text.as_str()),
        _ => None,
    };
    if let Some(language) = text("languageId") {
        return Some(language.to_owned());
    }
    let repo = text("repoPath")?.rsplit('/').next()?;
    Some(match text("version") {
        Some(version) => format!("{repo} {version}"),
        None => repo.to_owned(),
    })
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

impl App {
    /// The next `door43/sync` of `project` ends with `outcome`, whatever was set before.
    #[cfg(feature = "test-hooks")]
    pub fn script_sync(&self, project: ProjectRef, outcome: tc_api::test::ScriptedSync) {
        if let Some(scripted) = self.inner.sync.scripted() {
            scripted.script_sync(project, outcome);
        }
    }

    /// The scripted sync port, when the app runs on one (tests and journeys).
    #[cfg(feature = "test-hooks")]
    pub fn scripted_sync_port(&self) -> Option<&crate::ports::scripted::ScriptedSyncPort> {
        self.inner.sync.scripted()
    }

    /// One sync of `folder`, after the checkpoint of the project just left (tC4 `syncRun`).
    async fn sync_job(&self, folder: ProjectRef) -> Result<SyncResultView, JobEnd> {
        self.inner.sessions.settled(&folder).await;
        let view = self.inner.sync.sync(self.clone(), folder.clone()).await?;
        crate::app::lock(&self.inner.last_sync).insert(folder.clone(), view.clone());
        self.inner
            .bus
            .publish(tc_api::event::Event::HomeCard { project: folder });
        Ok(view)
    }

    /// What the scripted sync port does for a sync a test scripted: the teammates' changes
    /// written through the store, then the project read again as a sync's intake does.
    #[cfg(feature = "test-hooks")]
    pub(crate) async fn scripted_sync(
        &self,
        folder: ProjectRef,
        outcome: tc_api::test::ScriptedSync,
    ) -> Result<SyncResultView, JobEnd> {
        use serde_json::json;
        use tc_api::test::ScriptedSync as Scripted;
        let (teammates, joined, kinds, restored) = match outcome {
            Scripted::Failed(failure) => {
                return Err(JobEnd::Failed(ApiError::Refused(tc_api::Refusal::Sync(
                    failure,
                ))));
            }
            Scripted::Synced {
                teammates,
                joined,
                kinds,
                restored,
            } => (teammates, joined, kinds, restored),
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
        for kind in kinds {
            let at = folder.clone();
            let written = self
                .with_store(move |store| store.teammates_change_kind(&at, kind))
                .await
                .map_err(JobEnd::Failed)?;
            received.extend(
                written.map_err(|e| {
                    failed("the teammates' changes were not written", e.to_string())
                })?,
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

impl Handle<tc_api::door43::LastSync> for App {
    async fn handle(&self, req: tc_api::door43::LastSync) -> Result<LastSyncView, ApiError> {
        let view = crate::app::lock(&self.inner.last_sync)
            .get(&req.project)
            .cloned();
        Ok(LastSyncView { view })
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
        let scripts_door43 = matches!(
            req.next,
            Scripted::OpenDoor43Fails(_) | Scripted::Sync { .. }
        );
        if scripts_door43 && self.scripted_sync_port().is_none() {
            return Err(ApiError::BadRequest {
                field: "next".to_owned(),
                reason: "this app's sync port is not the scripted one".to_owned(),
            });
        }
        match req.next {
            Scripted::OpenDoor43Fails(failure) => {
                self.script_open_door43(Err(ApiError::Refused(tc_api::Refusal::Sync(failure))))
            }
            Scripted::Sync { project, outcome } => self.script_sync(project, outcome),
            Scripted::CheckpointFails { project } => {
                self.inner.sessions.script_checkpoint_failure(project);
            }
            next @ (Scripted::Updates(_) | Scripted::Plan(_)) => {
                self.inner.scripted_resources.script(next);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use tc_api::jobs::ReplacedValue;
    use tc_api::shown::Direction;
    use tc_journal::keys::{PinGroup, SettingKey};
    use tc_journal::record::PinGroupValue;
    use tc_journal::{Json, Record};

    use super::was_of;

    fn setting(key: &str, text: &str) -> Record {
        Record::Setting {
            key: SettingKey::parse(key).unwrap(),
            value: Some(Json::Str(text.to_owned())),
        }
    }

    fn pins(tn: &str) -> Record {
        let value = serde_json::json!({
            "gatewayLanguage": { "languageId": "en", "owner": "unfoldingWord" },
            "translationNotes": { "repoPath": "git.door43.org/unfoldingWord/en_tn", "version": tn },
        });
        Record::Pins {
            group: PinGroup::Primary,
            value: PinGroupValue::from_json(&value).unwrap(),
        }
    }

    /// Owner ruling H5 for settings and pins: the Received row names the older value, so the
    /// person can set it again where it is set.
    #[test]
    fn a_settings_row_names_the_older_value() {
        assert_eq!(
            was_of(
                &setting("textFont", "Noto Sans"),
                &setting("textFont", "Gentium")
            ),
            Some(ReplacedValue::Font("Noto Sans".into()))
        );
        assert_eq!(
            was_of(
                &setting("textDirection", "rtl"),
                &setting("textDirection", "ltr")
            ),
            Some(ReplacedValue::Direction(Direction::Rtl))
        );
        assert_eq!(
            was_of(&pins("v90"), &pins("v91")),
            Some(ReplacedValue::Resources(vec!["en_tn v90".into()])),
            "only the pins that differ"
        );
        assert_eq!(
            was_of(&setting("teammateNote", "a"), &setting("teammateNote", "b")),
            None
        );
    }
}
