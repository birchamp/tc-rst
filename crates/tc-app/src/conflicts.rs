//! The conflict review (units B11 and C7a; STORAGE §8.5; tC4 `ConflictReview.jsx`, #679): a
//! tab's entries with every tab's count, one record's History (#78), and the choices: one on
//! a conflict ([`choose`], the only way a choice becomes a store command), *Restore this
//! version* from History, and a bulk keep of one person's versions.
//!
//! A tab costs the conflict list it reviews, never the project: the counts come from the
//! fold's counts by kind, the entries from one counted walk of the conflict list that decodes
//! nothing beyond the tab's own conflicts, whose candidates the fold holds decoded.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::json;
use tc_api::conflicts::{
    BulkKept, ConflictEntry, ConflictKind, DetachedEntry, DetachedKind, History, HistoryVersion,
    HistoryView, KeepChoice, List, MadeBy, NoticeEntry, NoticeView, Resolve, ResolveBulk, Restore,
    ReviewEntries, ReviewPlace, ReviewTab, ReviewView, TabCount, Version, VersionCause,
    VersionValue, Versions,
};
use tc_api::ids::{ChangeRef, ConflictRef, WallMs};
use tc_api::shown::{Computer, PersonLabel, Who};
use tc_api::write::{Ack, SaveRefusal};
use tc_api::{ApiError, NotSaved};
use tc_journal::command::Choice;
use tc_journal::fold::{Anomaly, Conflict, ConflictId, ConflictKind as Kind, Notice};
use tc_journal::record::{BookPresence, ChapterFront, CheckValue, VerseValue};
use tc_journal::{Cause, ChangeId, DeviceId, DeviceKind, Record, RecordKey, WallTime};

use crate::app::App;
use crate::backend::{CommandError, ReviewToken, StoreCommand, StoreNotice, StoreReply};
use crate::counters::CountingSnapshot;
use crate::dispatch::Handle;
use crate::wire;

/// A version dated more than this after this computer's clock was made on a computer whose
/// clock may be wrong (STORAGE §6.3 `ClockAhead`).
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// The tab a conflict kind is reviewed in (STORAGE §8.5).
fn tab_of(kind: &Kind) -> ReviewTab {
    match kind {
        Kind::VerseText
        | Kind::VerseStructure
        | Kind::Passage
        | Kind::StructureIssue
        | Kind::BookHeader
        | Kind::ChapterFront
        | Kind::StoryBase
        | Kind::Frame
        | Kind::StoryRef
        // H5: the one book question left, *Restore the text* against *Start empty*, decides
        // verse text.
        | Kind::BookPresence => ReviewTab::Text,
        Kind::Alignment => ReviewTab::Alignments,
        Kind::Check | Kind::CheckMoved | Kind::Comment => ReviewTab::Checks,
        Kind::BookRemovedWhileEdited { .. }
        | Kind::LastBookRemoved
        | Kind::BookLayout
        | Kind::Pins
        | Kind::Setting
        | Kind::ProjectMeta
        | Kind::Versification
        | Kind::CarriedFile
        | Kind::PathCase
        | Kind::Unknown
        | Kind::DeviceProfile => ReviewTab::Settings,
    }
}

/// Every kind, so the tab counts read the fold's counts by kind and walk nothing.
const KINDS: [Kind; 25] = [
    Kind::VerseText,
    Kind::VerseStructure,
    Kind::Passage,
    Kind::StructureIssue,
    Kind::BookHeader,
    Kind::ChapterFront,
    Kind::BookPresence,
    Kind::BookRemovedWhileEdited { unseen: 0 },
    Kind::LastBookRemoved,
    Kind::Alignment,
    Kind::Check,
    Kind::CheckMoved,
    Kind::Comment,
    Kind::Pins,
    Kind::Setting,
    Kind::ProjectMeta,
    Kind::Versification,
    Kind::StoryBase,
    Kind::Frame,
    Kind::StoryRef,
    Kind::CarriedFile,
    Kind::PathCase,
    Kind::Unknown,
    Kind::BookLayout,
    Kind::DeviceProfile,
];

fn kind_view(kind: &Kind) -> ConflictKind {
    match kind {
        Kind::VerseText => ConflictKind::VerseText,
        Kind::VerseStructure => ConflictKind::VerseStructure,
        Kind::Passage => ConflictKind::Passage,
        Kind::StructureIssue => ConflictKind::StructureIssue,
        Kind::BookHeader => ConflictKind::BookHeader,
        Kind::ChapterFront => ConflictKind::ChapterFront,
        Kind::BookPresence => ConflictKind::BookPresence,
        Kind::BookRemovedWhileEdited { .. } => ConflictKind::BookRemovedWhileEdited,
        Kind::LastBookRemoved => ConflictKind::LastBookRemoved,
        Kind::BookLayout => ConflictKind::BookLayout,
        Kind::Alignment => ConflictKind::Alignment,
        Kind::Check => ConflictKind::Check,
        Kind::CheckMoved => ConflictKind::CheckMoved,
        Kind::Comment => ConflictKind::Comment,
        Kind::Pins => ConflictKind::Pins,
        Kind::Setting => ConflictKind::Setting,
        Kind::ProjectMeta => ConflictKind::ProjectMeta,
        Kind::Versification => ConflictKind::Versification,
        Kind::StoryBase => ConflictKind::StoryBase,
        Kind::Frame => ConflictKind::Frame,
        Kind::StoryRef => ConflictKind::StoryRef,
        Kind::CarriedFile => ConflictKind::CarriedFile,
        Kind::PathCase => ConflictKind::PathCase,
        Kind::DeviceProfile => ConflictKind::DeviceProfile,
        Kind::Unknown => ConflictKind::Unknown,
    }
}

/// Where a record is, as the review names it.
pub(crate) fn place_of(key: &RecordKey) -> ReviewPlace {
    match key {
        RecordKey::Verse(book, chapter, verse) | RecordKey::Alignment(book, chapter, verse) => {
            ReviewPlace::Verse {
                book: *book,
                chapter: chapter.get(),
                verse: verse.get(),
            }
        }
        RecordKey::ChapterFront(book, chapter) => ReviewPlace::ChapterFront {
            book: *book,
            chapter: chapter.get(),
        },
        RecordKey::BookPresence(book)
        | RecordKey::BookHeader(book)
        | RecordKey::BookLayout(book) => ReviewPlace::Book { book: *book },
        RecordKey::Check(check) => match crate::check::place(&check.target) {
            Ok(place) => ReviewPlace::Check {
                tool: crate::plans::api_tool(check.tool),
                place,
            },
            Err(_) => ReviewPlace::Project,
        },
        RecordKey::Frame(story, frame) => ReviewPlace::Story {
            story: *story,
            frame: Some(frame.get()),
        },
        RecordKey::StoryBase(story) | RecordKey::StoryRef(story) => ReviewPlace::Story {
            story: *story,
            frame: None,
        },
        _ => ReviewPlace::Project,
    }
}

/// What the review shows of a value: text where the screens show text. A verse shows all
/// its content, as tC4's review shows `version.text.trim()`: versions that differ only in a
/// footnote, a heading or a poetry line must look different before one replaces the other.
fn value_of(record: &Record) -> VersionValue {
    match record {
        Record::Verse { value, .. } => match value {
            VerseValue::Text { words, layout, .. } => {
                VersionValue::Text(format!("{words}{layout}").trim().to_owned())
            }
            VerseValue::Covered { by } => VersionValue::Joined { by: by.get() },
            VerseValue::Absent { .. } => VersionValue::Absent,
        },
        Record::ChapterFront { value, .. } => match value {
            ChapterFront::Present(usfm) => VersionValue::Text(tc_usfm::display_text(&usfm.0)),
            ChapterFront::Absent => VersionValue::Absent,
        },
        Record::BookHeader { value, .. } => VersionValue::Text(tc_usfm::display_text(&value.0)),
        Record::Frame { value, .. } | Record::StoryBase { value, .. } => {
            VersionValue::Text(value.trim().to_owned())
        }
        Record::StoryRef { value, .. } => match value {
            Some(text) => VersionValue::Text(text.trim().to_owned()),
            None => VersionValue::Absent,
        },
        Record::BookPresence {
            value: BookPresence::Removed { .. },
            ..
        } => VersionValue::Absent,
        Record::Check {
            value: CheckValue::Decision(d),
            ..
        } => VersionValue::Check {
            selected: (d.judgement.selections.iter().flatten())
                .map(|s| s.text.clone())
                .collect(),
            nothing_to_select: d.judgement.nothing_to_select,
            status: d.judgement.status.map(|s| match s {
                tc_journal::record::UserStatus::Valid => tc_api::check::DecisionStatus::Valid,
                tc_journal::record::UserStatus::Invalid => tc_api::check::DecisionStatus::Invalid,
                tc_journal::record::UserStatus::Todo => tc_api::check::DecisionStatus::Todo,
            }),
            bookmarked: d.bookmarked,
            comment: d.comment.clone(),
        },
        Record::Comment { value, .. } => VersionValue::Text(value.text.trim().to_owned()),
        Record::Alignment { value, .. } => VersionValue::Alignment {
            pairs: (value.alignments.iter())
                .filter(|g| !g.bottom_words.is_empty())
                .map(|g| {
                    let words = |w: Vec<&str>| w.join(" ");
                    format!(
                        "{} = {}",
                        words(g.top_words.iter().map(|w| w.word.as_str()).collect()),
                        words(g.bottom_words.iter().map(|w| w.word.as_str()).collect())
                    )
                })
                .collect(),
            done: value.done,
            marked_invalid: value.marked_invalid,
        },
        _ => VersionValue::NotShown,
    }
}

/// Who made a change: a person by the one label (#680), else the source a change no person
/// typed here came from (STORAGE §8.5).
fn made_by(
    device: &DeviceId,
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
) -> MadeBy {
    match device.kind() {
        DeviceKind::Door43Edit => MadeBy::Door43,
        DeviceKind::ExchangeEdit => MadeBy::ExchangeFile,
        DeviceKind::Importer => MadeBy::Import,
        DeviceKind::Local | DeviceKind::Tc4Actor | DeviceKind::Tc3User => {
            match labels.get(device) {
                Some(label) => MadeBy::Person(wire::scope::person_label(label)),
                None => MadeBy::Import,
            }
        }
    }
}

fn clock_ahead(at: WallTime, now: WallMs) -> bool {
    at.0.saturating_sub(now.0) > DAY_MS
}

fn cause_of(cause: &Cause) -> VersionCause {
    use tc_journal::change::ExternalSource;
    match cause {
        Cause::Edit => VersionCause::Edited,
        Cause::Resolve => VersionCause::Kept,
        Cause::Restore { .. } => VersionCause::Restored,
        Cause::CarryOver { .. } => VersionCause::CarriedOver,
        Cause::External { source, .. } => match source {
            ExternalSource::Door43 { .. } => VersionCause::EditedOnDoor43,
            ExternalSource::LocalFile { .. } => VersionCause::EditedInAnotherProgram,
            ExternalSource::ExchangeFile { .. } => VersionCause::FromAFile,
        },
        Cause::Import(_) => VersionCause::Imported,
        Cause::Seed => VersionCause::Created,
        Cause::StartOver => VersionCause::StartedOver,
        Cause::System(_) => VersionCause::ByTheApp,
        Cause::Newer { .. } => VersionCause::Newer,
    }
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// A notice in the review's words. A whitespace-only outside edit was not recorded and
/// changes nothing a person reads, so it has none.
fn notice_view(
    notice: &Notice,
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
) -> Option<NoticeView> {
    let file = |path: &dyn std::fmt::Display| path.to_string();
    Some(match notice {
        Notice::QuarantinedPack { salvaged, .. } | Notice::QuarantinedPending { salvaged, .. } => {
            NoticeView::DamagedChanges {
                recovered: *salvaged,
            }
        }
        Notice::NewerPack { .. } => NoticeView::NewerApp,
        Notice::ClockAhead { device, by_ms } => NoticeView::ClockAhead {
            by: labels.get(device).map(wire::scope::person_label),
            days: count(usize::try_from(by_ms.div_euclid(DAY_MS).max(1)).unwrap_or(usize::MAX)),
        },
        Notice::CarriedBytesMissing { path, .. } | Notice::PackMissing { path } => {
            NoticeView::FileMissing { file: file(path) }
        }
        Notice::DeviceRotated { .. } | Notice::RestoredBackup { .. } => NoticeView::FolderCopied,
        Notice::OutsideEditRecorded { path, changes, .. } => NoticeView::OutsideEditRecorded {
            file: file(path),
            changes: *changes,
        },
        Notice::OutsideEditUnreadable { path, .. } => {
            NoticeView::OutsideEditUnreadable { file: file(path) }
        }
        Notice::FileMoved { from, to } => NoticeView::FileMoved {
            file: from.to_string(),
            to: to.to_string(),
        },
        Notice::MissingFileRebuilt { path } | Notice::PackRestored { path, .. } => {
            NoticeView::FileRepaired { file: file(path) }
        }
        Notice::FileUnreadable { path, .. } => NoticeView::FileUnreadable {
            file: path.to_string(),
        },
        Notice::StorageDamage { path, .. } => NoticeView::StorageDamage { file: file(path) },
        Notice::OutsideEditHeld { path, .. } => NoticeView::OutsideEditHeld { file: file(path) },
        Notice::MetadataEditedOutside { .. } => NoticeView::DetailsEditedOutside,
        Notice::MaterializeBlocked { paths, .. } => NoticeView::WriteBlocked {
            file: paths.first().map(|p| file(p)).unwrap_or_default(),
        },
        Notice::OutsideRemovalKept { path, .. } | Notice::FixedFileRestored { path, .. } => {
            NoticeView::FileRepaired { file: file(path) }
        }
        Notice::DerivedFieldsEdited { .. } => NoticeView::DetailsEditedOutside,
        // D1 gives these their own words; until then the review lists nothing for them.
        Notice::WhitespaceOnlyEdit { .. }
        | Notice::CarriedTooLarge { .. }
        | Notice::HistoryRebuilt { .. }
        | Notice::HistoryUnavailable { .. }
        | Notice::PendingWrittenAgain { .. }
        | Notice::UnsavedAttemptKept { .. } => return None,
        Notice::SecondWriter { .. } => NoticeView::SecondWriter,
        Notice::SavedWithDiskError { .. } => NoticeView::SavedWithDiskError,
        Notice::ImportUnfinished { .. } => NoticeView::ImportUnfinished,
    })
}

/// The journal's anomalies in the review's words. A missing parent is said on the conflict it
/// touches (`incomplete_history`), and while a teammate's earlier changes have not arrived it
/// is no fault, so it has no notice of its own.
fn anomaly_view(anomaly: &Anomaly) -> Option<NoticeView> {
    Some(match anomaly {
        Anomaly::QuarantinedChange { .. }
        | Anomaly::ParentOnOtherRecord { .. }
        | Anomaly::InvalidForBurrito { .. } => NoticeView::KeptApart { changes: 1 },
        Anomaly::RecordsWithoutBook { records, .. } => NoticeView::KeptApart { changes: *records },
        Anomaly::IncompleteBatch { have, of, .. } => NoticeView::IncompleteChanges {
            have: *have,
            of: *of,
        },
        Anomaly::DuplicateDevice { .. } => NoticeView::FolderCopied,
        Anomaly::UnknownKinds { .. } => NoticeView::NewerApp,
        Anomaly::MissingParent { .. } => return None,
    })
}

fn notices(
    read: &CountingSnapshot,
    store: &[StoreNotice],
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
) -> Vec<NoticeEntry> {
    store
        .iter()
        .filter_map(|n| notice_view(&n.notice, labels))
        .chain(read.anomalies().iter().filter_map(anomaly_view))
        .map(|notice| NoticeEntry { notice })
        .collect()
}

fn detached_kind(key: &RecordKey) -> DetachedKind {
    match key {
        RecordKey::Check(_) => DetachedKind::Decision,
        RecordKey::Alignment(..) => DetachedKind::Alignment,
        RecordKey::Comment(_) => DetachedKind::Comment,
        _ => DetachedKind::Other,
    }
}

/// Tells apart the computers of versions that carry one person's label (STORAGE §8.5): one
/// person's two computers, or a team's computers signed in to one Door43 account. This
/// computer's versions say so, and another computer is numbered among the person's other
/// computers. Another computer signed in as this computer's person is named by its username,
/// as tC4 names it, so the versions of two computers are never both "Mine".
fn by_computer(
    versions: &mut [(DeviceId, Version)],
    me: DeviceId,
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
    username: impl Fn(DeviceId) -> Option<String>,
) {
    let mut computers: HashMap<&PersonLabel, BTreeSet<DeviceId>> = HashMap::new();
    for (device, version) in versions.iter() {
        if let MadeBy::Person(label) = &version.by {
            computers.entry(label).or_default().insert(*device);
        }
    }
    let mut told: BTreeMap<DeviceId, PersonLabel> = BTreeMap::new();
    for (label, devices) in computers.into_iter().filter(|(_, d)| d.len() > 1) {
        for device in devices {
            let told_apart = if device == me {
                PersonLabel {
                    who: label.who.clone(),
                    computer: Some(Computer::This),
                }
            } else {
                let person = labels.get(&device);
                let n = labels
                    .iter()
                    .filter(|(d, l)| **d != me && Some(*l) == person)
                    .position(|(d, _)| *d == device)
                    .map_or(u16::MAX, |i| u16::try_from(i + 1).unwrap_or(u16::MAX));
                let who = match &label.who {
                    Who::Mine => username(device).map_or(Who::Mine, Who::Username),
                    who => who.clone(),
                };
                PersonLabel {
                    who,
                    computer: Some(Computer::Other(n)),
                }
            };
            told.insert(device, told_apart);
        }
    }
    for (device, version) in versions.iter_mut() {
        if let Some(label) = told.get(device) {
            version.by = MadeBy::Person(label.clone());
        }
    }
}

/// The versions of a conflict, newest first: each candidate by who made it, the version
/// the screens show marked.
fn versions(
    conflict: &Conflict,
    labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
    me: DeviceId,
    now: WallMs,
) -> Vec<Version> {
    let mut candidates: Vec<_> = conflict.candidates.iter().collect();
    candidates.sort_by_key(|c| std::cmp::Reverse((c.at, c.head)));
    let mut versions: Vec<(DeviceId, Version)> = candidates
        .iter()
        .filter_map(|c| {
            Some((
                c.device,
                Version {
                    change: ChangeRef::parse(&c.head.to_hex()).ok()?,
                    place: place_of(&c.value.key()),
                    by: made_by(&c.device, labels),
                    at: WallMs(c.at.0),
                    clock_ahead: clock_ahead(c.at, now),
                    shown: c.head == conflict.provisional || c.also.contains(&conflict.provisional),
                    value: value_of(&c.value),
                },
            ))
        })
        .collect();
    // A candidate's author is its profile's username, else its display name; a computer
    // labelled "Mine" that is not this one shares this computer's username, so it has one.
    by_computer(&mut versions, me, labels, |device| {
        candidates
            .iter()
            .find(|c| c.device == device)
            .and_then(|c| c.author.as_deref().map(str::to_owned))
    });
    versions.into_iter().map(|(_, v)| v).collect()
}

impl App {
    fn conflict_entries(
        &self,
        read: &CountingSnapshot,
        tab: ReviewTab,
        labels: &BTreeMap<DeviceId, tc_journal::fold::PersonLabel>,
        me: DeviceId,
    ) -> Result<Vec<ConflictEntry>, ApiError> {
        let now = self.inner.clock.now();
        let mine: Vec<&Conflict> = read
            .conflicts()
            .filter(|c| tab_of(&c.kind) == tab)
            .collect();
        // One upgrade against another is one group: the conflicts of one set of batches.
        let mut groups: BTreeMap<Vec<tc_journal::BatchId>, u32> = BTreeMap::new();
        let mut entries = Vec::with_capacity(mine.len());
        for conflict in mine {
            let mut batches = conflict.batches.clone();
            batches.sort();
            let next = count(groups.len());
            let group = *groups.entry(batches).or_insert(next);
            let keys = std::iter::once(&conflict.key).chain(conflict.also_keys.iter());
            let review = keys
                .map(|key| {
                    read.mint_review(key)
                        .map_err(|e| e.to_string())
                        .and_then(|t| wire::token::review(&t).map_err(|e| e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()
                .map_err(|reason| {
                    self.internal(
                        "a conflict's review token could not be made",
                        crate::reports::facts(json!({"reason": reason})),
                    )
                })?;
            let (Ok(id), Some(record)) = (
                ConflictRef::parse(&conflict.id.to_hex()),
                wire::scope::record_ref(&conflict.key),
            ) else {
                continue;
            };
            entries.push(ConflictEntry {
                conflict: id,
                kind: kind_view(&conflict.kind),
                record,
                place: place_of(&conflict.key),
                group,
                versions: versions(conflict, labels, me, now),
                review,
                incomplete_history: conflict.incomplete_history,
                unreadable: count(conflict.unreadable.len()),
            });
        }
        Ok(entries)
    }
}

impl Handle<List> for App {
    async fn handle(&self, req: List) -> Result<ReviewView, ApiError> {
        let session = self.read(&req.at)?;
        let read = session.read();
        let handle = session.handle();
        let me = handle.facts().device;
        let labels = read.person_labels(me);
        let mut counts: BTreeMap<ReviewTab, u32> = BTreeMap::new();
        for kind in &KINDS {
            let n: u32 = read
                .conflict_counts()
                .iter()
                .filter(|((_, order), _)| *order == kind.order())
                .map(|(_, n)| *n)
                .sum();
            *counts.entry(tab_of(kind)).or_default() += n;
        }
        let detached = read.detached();
        let store_notices = handle.notices();
        let notices = notices(&read, &store_notices, &labels);
        counts.insert(ReviewTab::Detached, count(detached.len()));
        counts.insert(ReviewTab::Notices, count(notices.len()));
        let entries = match req.tab {
            ReviewTab::Detached => ReviewEntries::Detached(
                detached
                    .iter()
                    .filter_map(|key| {
                        Some(DetachedEntry {
                            record: wire::scope::record_ref(key)?,
                            what: detached_kind(key),
                            place: place_of(key),
                        })
                    })
                    .collect(),
            ),
            ReviewTab::Notices => ReviewEntries::Notices(notices),
            tab => ReviewEntries::Conflicts(self.conflict_entries(&read, tab, &labels, me)?),
        };
        Ok(ReviewView {
            view: wire::scope::view(read.seq()),
            tabs: ReviewTab::ALL
                .into_iter()
                .map(|tab| TabCount {
                    tab,
                    count: counts.get(&tab).copied().unwrap_or(0),
                })
                .collect(),
            entries,
        })
    }
}

impl Handle<History> for App {
    async fn handle(&self, req: History) -> Result<HistoryView, ApiError> {
        let key = wire::scope::record_key(&req.record)?;
        self.history(&req.at, &key)
    }
}

impl Handle<Versions> for App {
    async fn handle(&self, req: Versions) -> Result<HistoryView, ApiError> {
        let session = self.read(&req.at)?;
        let shown = wire::token::edit_token(session.handle().as_ref(), &req.shown)?;
        drop(session);
        self.history(&req.at, shown.key())
    }
}

impl App {
    fn history(&self, at: &tc_api::ids::Scope, key: &RecordKey) -> Result<HistoryView, ApiError> {
        let session = self.read(at)?;
        let read = session.read();
        let key = key.clone();
        let me = session.handle().facts().device;
        let labels = read.person_labels(me);
        let now = self.inner.clock.now();
        let shown = read.projected_change(&key);
        let (mut versions, causes): (Vec<(DeviceId, Version)>, Vec<VersionCause>) = read
            .history(&key)
            .into_iter()
            .filter_map(|id| {
                let body = read.body(&id)?;
                let version = Version {
                    change: ChangeRef::parse(&id.to_hex()).ok()?,
                    place: place_of(&key),
                    by: made_by(&body.device, &labels),
                    at: WallMs(body.at.0),
                    clock_ahead: clock_ahead(body.at, now),
                    shown: shown == Some(id),
                    value: value_of(&body.record),
                };
                Some(((body.device, version), cause_of(&body.cause)))
            })
            .unzip();
        by_computer(&mut versions, me, &labels, |device| {
            match read.projected(&RecordKey::DeviceProfile(device)) {
                Some(Record::DeviceProfile { value, .. }) => {
                    value.door43_username.map(|name| name.as_str().to_owned())
                }
                _ => None,
            }
        });
        let mut versions: Vec<HistoryVersion> = versions
            .into_iter()
            .zip(causes)
            .map(|((_, version), cause)| HistoryVersion { version, cause })
            .collect();
        versions.reverse();
        Ok(HistoryView { versions })
    }
}

fn bad_token() -> ApiError {
    ApiError::NotSaved(NotSaved::BadToken)
}

fn change_id(change: &ChangeRef) -> Result<ChangeId, ApiError> {
    ChangeId::parse(change.as_str()).ok_or_else(bad_token)
}

/// The one way a choice of the review becomes a store command: every [`KeepChoice`] to its
/// journal [`Choice`], over the review's tokens, one per record of the conflict. A choice
/// with no token writes nothing, and the store refuses one that lacks a record's token
/// (`check_review_tokens`), so no record is written over a version the screen did not show.
/// A Restore is the old version's value kept over every head of its record (`Values`): a
/// restore over the shown head only would let the fold settle the conflict on the other side.
fn choose(
    read: &CountingSnapshot,
    conflict: &ConflictRef,
    review: Vec<ReviewToken>,
    choice: &KeepChoice,
) -> Result<StoreCommand, ApiError> {
    let conflict = ConflictId::parse(conflict.as_str()).ok_or_else(bad_token)?;
    if review.is_empty() {
        return Err(bad_token());
    }
    let heads = |picks: &[ChangeRef]| picks.iter().map(change_id).collect::<Result<Vec<_>, _>>();
    let choice = match choice {
        KeepChoice::Candidate { change } => Choice::Candidate {
            head: change_id(change)?,
        },
        KeepChoice::Passage { picks } => Choice::KeepPassage {
            heads: heads(picks)?,
        },
        KeepChoice::Separate { picks } => Choice::KeepSeparate {
            heads: heads(picks)?,
        },
        KeepChoice::Joined { picks } => Choice::KeepJoined {
            heads: heads(picks)?,
        },
        KeepChoice::Restore { from } => {
            let body = read.body(&change_id(from)?).ok_or_else(|| {
                ApiError::NotSaved(NotSaved::Refused(SaveRefusal::Choice {
                    reason: "this version cannot be read on this computer yet".to_owned(),
                }))
            })?;
            Choice::Values(vec![body.record])
        }
    };
    Ok(StoreCommand::resolve(conflict, review, choice))
}

impl App {
    /// A resolution's or a restore's reply as its `Ack`.
    fn ack(&self, reply: &StoreReply) -> Result<Ack, ApiError> {
        let (stored, disk_warning) = wire::error::stored(&reply.stored);
        let tokens = reply
            .tokens
            .iter()
            .map(|(key, token)| wire::token::token_update(key, token))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| {
                self.internal(
                    "a resolution's reply could not be written",
                    crate::reports::facts(json!({"reason": e.to_string()})),
                )
            })?;
        Ok(Ack {
            view: wire::scope::view(reply.seq),
            stored,
            tokens,
            new_conflicts: Vec::new(),
            saved: Vec::new(),
            rows: Vec::new(),
            disk_warning,
        })
    }

    /// Why the store wrote nothing, in the API's words; a refusal no screen has words for is
    /// reported.
    fn not_written(&self, error: CommandError) -> ApiError {
        match wire::error::command_error(error) {
            Ok(api) => api,
            Err(unworded) => self.internal(
                "the store refused a resolution no screen has words for yet",
                crate::reports::facts(json!({"reason": unworded.0})),
            ),
        }
    }

    /// The handle of the open project a write names, or `StaleScope`.
    fn writer(
        &self,
        at: &tc_api::ids::Scope,
    ) -> Result<std::sync::Arc<crate::session::ProjectSession>, ApiError> {
        self.inner
            .sessions
            .reader(at)
            .map_err(|_| ApiError::NotSaved(NotSaved::StaleScope))
    }
}

impl Handle<Resolve> for App {
    /// One choice (STORAGE §5.9): one `Resolve` batch whose changes name every version the
    /// review showed. Versions that changed since answer `LookAgain`, and the reply cache
    /// gives a second send of one request its first reply (A-27).
    async fn handle(&self, req: Resolve) -> Result<Ack, ApiError> {
        let session = self.writer(&req.at)?;
        let handle = session.handle().clone();
        let review = req
            .review
            .iter()
            .map(|token| wire::token::review_token(handle.as_ref(), token))
            .collect::<Result<Vec<_>, _>>()?;
        let command =
            choose(&session.read(), &req.conflict, review, &req.choice)?.requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => self.ack(&reply),
            Err(error) => Err(self.not_written(error)),
        }
    }
}

impl Handle<Restore> for App {
    /// *Restore this version* from a record's History, outside any conflict (owner ruling H5):
    /// the old version's value over every head the screen's token shows, so a version the
    /// screen did not show is never written over (it answers `LookAgain`).
    async fn handle(&self, req: Restore) -> Result<Ack, ApiError> {
        let session = self.writer(&req.at)?;
        let handle = session.handle().clone();
        let shown = wire::token::edit_token(handle.as_ref(), &req.shown)?;
        let command = StoreCommand::restore(shown, change_id(&req.from)?).requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => self.ack(&reply),
            Err(error) => Err(self.not_written(error)),
        }
    }
}

impl Handle<ResolveBulk> for App {
    /// *Keep all of Maria's* (STORAGE §5.9 step 3): the person's computers are the devices
    /// the fold labels as that person; the store keeps their version where a conflict holds
    /// exactly one from them, and leaves the rest, which the reply names.
    async fn handle(&self, req: ResolveBulk) -> Result<BulkKept, ApiError> {
        let session = self.writer(&req.at)?;
        let handle = session.handle().clone();
        let me = handle.facts().device;
        let devices: BTreeSet<DeviceId> = session
            .read()
            .person_labels(me)
            .iter()
            .filter(|(_, label)| wire::scope::person_label(label).who == req.person)
            .map(|(device, _)| *device)
            .collect();
        let conflicts = req
            .conflicts
            .iter()
            .map(|entry| {
                let id = ConflictId::parse(entry.conflict.as_str()).ok_or_else(bad_token)?;
                let review = entry
                    .review
                    .iter()
                    .map(|token| wire::token::review_token(handle.as_ref(), token))
                    .collect::<Result<Vec<_>, _>>()?;
                if review.is_empty() {
                    return Err(bad_token());
                }
                Ok((id, review))
            })
            .collect::<Result<Vec<_>, ApiError>>()?;
        let command = StoreCommand::resolve_bulk(devices, conflicts).requested_by(req.request);
        match handle.submit(command).await {
            Ok(reply) => Ok(BulkKept {
                ack: self.ack(&reply)?,
                left: reply
                    .skipped
                    .iter()
                    .filter_map(|id| ConflictRef::parse(&id.to_hex()).ok())
                    .collect(),
            }),
            Err(error) => Err(self.not_written(error)),
        }
    }
}
