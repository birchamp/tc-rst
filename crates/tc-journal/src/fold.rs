//! The fold (`docs/STORAGE.md` §7): the set of changes turned into the project state.
//!
//! [`FoldState::fold`] folds a set of changes from scratch and [`FoldState::add`] adds
//! changes to a fold; both run the same stages (§7.2 F1 to F10) over what the changes
//! touched, so an incremental step costs what it touches and equals a fold from scratch
//! (§7.8). The state lives in shared containers, so a clone is a snapshot.
//!
//! The fold reads no clock, device or author to decide what survives (I3), never fails
//! (I5), and lists every defect as an [`Anomaly`].

mod attach;
mod books;
mod cow;
mod decode;
mod merge;
pub(crate) mod meta;
mod metadata;
mod passage;
mod people;
mod project;
mod ranges;
mod reads;
mod resolve;
mod scope;
mod sidecar;
mod state;
pub(crate) mod value;

pub use decode::{
    DecodedComment, DecodedMetadata, decode_comments, decode_metadata, decode_resources,
    decode_settings, decode_versification,
};
pub use meta::ChangeIdx;
pub use people::PersonLabel;
pub use project::{CarriedPlacement, CheckTexts, DerivedFile, ProjectedDecision, RenderedFile};
pub use ranges::{ChapterKeys, DeviceStat};
pub use reads::{BookView, ChangeView, Placement};
pub use resolve::ResolveError;
pub use scope::ScopeDigest;
pub use sidecar::{
    SidecarError, Writer, decode_alignment_record, decode_alignments, decode_decisions,
    records_from_usfm,
};
pub use state::{FoldInput, FoldState, MergeHow, ProjectKind};
pub use value::{ValueAt, ValueSource};

pub(crate) use merge::{record_of, value_of};

use crate::change::Cause;
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceSeq, GitOid, RepoPath, WallTime};
use crate::json::Json;
use crate::keys::RecordKey;
use crate::pack::PackError;
use crate::record::Record;
use crate::rules::RuleViolation;
use crate::sha::sha256;
use crate::text::hex;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use tc_core::BookId;

/// SHA-256 of a sorted set of head ids: the 32-byte ids, ascending, concatenated.
fn digest_of(heads: &[ChangeId]) -> [u8; 32] {
    let mut sorted: Vec<ChangeId> = heads.to_vec();
    sorted.sort();
    sorted.dedup();
    let mut bytes = Vec::with_capacity(sorted.len() * 32);
    for id in &sorted {
        bytes.extend_from_slice(id.as_bytes());
    }
    sha256(&bytes)
}

/// Names one exact state: the digest of the sorted set of every head id.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frontier([u8; 32]);

impl Frontier {
    pub fn of(heads: &[ChangeId]) -> Self {
        Self(digest_of(heads))
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }
}

impl fmt::Debug for Frontier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Frontier({})", self.to_hex())
    }
}

/// A conflict's identity: the digest of its sorted head ids, the same on every device.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictId([u8; 32]);

impl ConflictId {
    pub fn of(heads: &[ChangeId]) -> Self {
        Self(digest_of(heads))
    }

    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    pub fn parse(text: &str) -> Option<Self> {
        crate::text::unhex::<32>(text).map(Self)
    }
}

impl fmt::Debug for ConflictId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConflictId({})", self.to_hex())
    }
}

impl fmt::Display for ConflictId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// Conflict counts by book (`None` for records of no book) and kind order
/// ([`ConflictKind::order`]).
pub type ConflictCounts = std::collections::BTreeMap<(Option<BookId>, u8), u32>;

/// A record's heads and how they resolve (§2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyState {
    /// Sorted.
    pub heads: Vec<ChangeId>,
    pub state: Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Settled {
        head: ChangeId,
    },
    /// Several heads; the projection uses `head`, no conflict is listed (§7.3 stub rules,
    /// ruling H5). Every head stays in History.
    SettledByRule {
        head: ChangeId,
        rule: AutoRule,
    },
    /// Several heads merged without disagreement. Computed, never stored.
    Merged {
        merged: Box<Record>,
        how: MergeHow,
    },
    /// The record is in this listed conflict (its own, or a grouped one).
    Conflict(ConflictId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRule {
    SeedStubs,
    /// The head with the largest (rank, at, id) of a kind that never conflicts (ruling H5).
    Newest,
}

/// A merge unit (§7.3): a group of fields taken together, named by its JSON path in the
/// record's value, such as `/words` or `/judgement`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitName(pub Box<str>);

/// A unit's value as stored, unknown fields of a newer minor included; `None` when the
/// unit's fields are absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitValue(pub Option<Json>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub id: ConflictId,
    pub key: RecordKey,
    /// The other records of a grouped conflict (Passage, StructureIssue).
    pub also_keys: Vec<RecordKey>,
    pub kind: ConflictKind,
    /// Maximal common ancestors of the heads (§7.3).
    pub bases: Vec<ChangeId>,
    /// One per distinct head value.
    pub candidates: Vec<Candidate>,
    /// Units that merged cleanly.
    pub agreed: Vec<(UnitName, UnitValue)>,
    /// Empty for whole-value kinds.
    pub disputed: Vec<UnitName>,
    /// The head with the largest (rank, at, id), or the passage side (§7.2 F7).
    pub provisional: ChangeId,
    /// Batches of the heads, for grouping (§8.1).
    pub batches: Vec<BatchId>,
    /// A head the conflict lists, as a candidate, in a candidate's `also` or as unreadable,
    /// has a MissingParent link (§7.4, §8.1).
    pub incomplete_history: bool,
    /// Versions (heads, or the bases a merge needs) whose values this computer could not
    /// read (§2.6). They are kept, and no choice may replace them until they can be read;
    /// an unreadable head is listed here, not as a candidate.
    pub unreadable: Vec<ChangeId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub head: ChangeId,
    /// Other heads with an identical value.
    pub also: Vec<ChangeId>,
    pub device: DeviceId,
    /// From DeviceProfile.
    pub author: Option<Box<str>>,
    pub at: WallTime,
    pub cause: Cause,
    pub value: Record,
    /// For a `Passage` conflict: the side this head belongs to (§7.3).
    pub side: Option<Side>,
}

/// A side of a `Passage` conflict: a passage batch, or every other change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Side {
    Batch(BatchId),
    Others,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConflictKind {
    VerseText,
    VerseStructure,
    Passage,
    StructureIssue,
    BookHeader,
    ChapterFront,
    BookPresence,
    BookRemovedWhileEdited {
        unseen: u32,
    },
    LastBookRemoved,
    Alignment,
    Check,
    CheckMoved,
    Comment,
    Pins,
    Setting,
    ProjectMeta,
    Versification,
    StoryBase,
    Frame,
    StoryRef,
    CarriedFile,
    PathCase,
    Unknown,
    /// Two orders of an imported book's chapters and verses.
    BookLayout,
    /// Two versions of one device's profile (only one device writes it, so only a
    /// duplicated device makes this).
    DeviceProfile,
}

impl ConflictKind {
    /// A verse conflict: verse text, verse structure, a passage or a structure issue. The
    /// sync result counts these apart from the others, and its review link opens them
    /// (tC4's toast, `docs/TC4-DELTA-2026-10-16.md` §3.9, §8.5).
    pub fn is_verse(&self) -> bool {
        matches!(
            self,
            Self::VerseText | Self::VerseStructure | Self::Passage | Self::StructureIssue
        )
    }

    /// The order of kinds within one key in the conflict list.
    pub fn order(&self) -> u8 {
        match self {
            Self::VerseText => 0,
            Self::VerseStructure => 1,
            Self::Passage => 2,
            Self::StructureIssue => 3,
            Self::BookHeader => 4,
            Self::ChapterFront => 5,
            Self::BookPresence => 6,
            Self::BookRemovedWhileEdited { .. } => 7,
            Self::LastBookRemoved => 8,
            Self::Alignment => 9,
            Self::Check => 10,
            Self::CheckMoved => 11,
            Self::Comment => 12,
            Self::Pins => 13,
            Self::Setting => 14,
            Self::ProjectMeta => 15,
            Self::Versification => 16,
            Self::StoryBase => 17,
            Self::Frame => 18,
            Self::StoryRef => 19,
            Self::CarriedFile => 20,
            Self::PathCase => 21,
            Self::Unknown => 22,
            Self::BookLayout => 23,
            Self::DeviceProfile => 24,
        }
    }

    /// Kinds that are not one record's own heads take their id over a tag too, so two
    /// conflicts over the same heads (a book's presence conflict and its
    /// removed-while-edited conflict) never share an id.
    pub(crate) fn tag(&self) -> Option<&'static str> {
        match self {
            Self::StructureIssue => Some("structureIssue"),
            Self::BookRemovedWhileEdited { .. } => Some("bookRemovedWhileEdited"),
            Self::LastBookRemoved => Some("lastBookRemoved"),
            Self::PathCase => Some("pathCase"),
            _ => None,
        }
    }
}

impl ConflictId {
    /// The id of a derived conflict: SHA-256 of the tag and the sorted head ids.
    pub(crate) fn tagged(tag: &str, heads: &[ChangeId]) -> Self {
        let mut sorted: Vec<ChangeId> = heads.to_vec();
        sorted.sort();
        sorted.dedup();
        let mut bytes = Vec::with_capacity(tag.len() + 1 + sorted.len() * 32);
        bytes.extend_from_slice(tag.as_bytes());
        bytes.push(0);
        for id in &sorted {
            bytes.extend_from_slice(id.as_bytes());
        }
        Self(sha256(&bytes))
    }
}

/// What one call of [`FoldState::add`] changed: what `tc-store` marks dirty (§5.0 B5) and
/// what the reply to a command says (B6).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Delta {
    /// Records whose state or projected value may have changed, sorted.
    pub keys: Vec<RecordKey>,
    pub books: Vec<BookId>,
    pub chapters: Vec<(BookId, u16)>,
    /// Conflicts that did not exist before this step.
    pub new_conflicts: Vec<ConflictId>,
    /// A pin group changed: decision sidecars and `resources.json` change.
    pub pins: bool,
    /// A book's projection appeared, vanished or changed scope (`books` names it): its files
    /// are written or removed.
    pub paths: bool,
    /// A `CarriedFile` record changed: carried files are placed or removed.
    pub carried: bool,
    /// A conflict was listed or unlisted: `conflicts.json` changes.
    pub conflicts_changed: bool,
    /// The fold was rebuilt from scratch: every derived file may have changed.
    pub everything: bool,
}

impl Delta {
    pub(crate) fn everything() -> Self {
        Self {
            everything: true,
            ..Self::default()
        }
    }
}

/// Work counters for the operation-count tests (`docs/PERFORMANCE.md` rule 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Counter {
    ChangesIndexed,
    KeysUpdated,
    KeysRecomputed,
    ChangesVisited,
    ValuesDecoded,
    GroupsBuilt,
    BooksChecked,
    ChaptersCohered,
    VersesCohered,
    AttachedPlaced,
    ConflictsBuilt,
    RecordsProjected,
    ValuesRead,
    AnomaliesTouched,
    RecordsListed,
}

const COUNTERS: usize = 15;

#[derive(Debug, Default)]
pub(crate) struct CounterCells([AtomicU64; COUNTERS]);

impl CounterCells {
    pub(crate) fn add(&self, counter: Counter, n: u64) {
        self.0[counter as usize].fetch_add(n, Ordering::Relaxed);
    }

    pub(crate) fn read(&self) -> Counters {
        let get = |c: Counter| self.0[c as usize].load(Ordering::Relaxed);
        Counters {
            changes_indexed: get(Counter::ChangesIndexed),
            keys_updated: get(Counter::KeysUpdated),
            keys_recomputed: get(Counter::KeysRecomputed),
            changes_visited: get(Counter::ChangesVisited),
            values_decoded: get(Counter::ValuesDecoded),
            groups_built: get(Counter::GroupsBuilt),
            books_checked: get(Counter::BooksChecked),
            chapters_cohered: get(Counter::ChaptersCohered),
            verses_cohered: get(Counter::VersesCohered),
            attached_placed: get(Counter::AttachedPlaced),
            conflicts_built: get(Counter::ConflictsBuilt),
            records_projected: get(Counter::RecordsProjected),
            values_read: get(Counter::ValuesRead),
            anomalies_touched: get(Counter::AnomaliesTouched),
            records_listed: get(Counter::RecordsListed),
        }
    }
}

/// How much work a fold has done since it was made. A snapshot shares its counters with
/// the state it was taken from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counters {
    /// Changes added to the index (F1).
    pub changes_indexed: u64,
    /// Keys whose heads were updated in O(1).
    pub keys_updated: u64,
    /// Keys whose links and heads were recomputed from their changes.
    pub keys_recomputed: u64,
    /// Changes visited by those recomputations and by ancestry walks.
    pub changes_visited: u64,
    /// Values decoded from stored bytes.
    pub values_decoded: u64,
    /// Passage groups built.
    pub groups_built: u64,
    /// Books whose presence and removal visibility were computed.
    pub books_checked: u64,
    /// Chapters whose coherence was computed.
    pub chapters_cohered: u64,
    /// Verse records those passes read.
    pub verses_cohered: u64,
    /// Alignments, comments and decisions placed (§7.7).
    pub attached_placed: u64,
    /// Conflicts built.
    pub conflicts_built: u64,
    /// Records read to project derived bytes.
    pub records_projected: u64,
    /// Values read from the fold's source (§14).
    pub values_read: u64,
    /// Anomalies listed or unlisted.
    pub anomalies_touched: u64,
    /// Records visited to list derived files.
    pub records_listed: u64,
}

impl Counters {
    /// The counters' change from `earlier` to `self`.
    pub fn since(&self, earlier: &Self) -> Self {
        Self {
            changes_indexed: self.changes_indexed - earlier.changes_indexed,
            keys_updated: self.keys_updated - earlier.keys_updated,
            keys_recomputed: self.keys_recomputed - earlier.keys_recomputed,
            changes_visited: self.changes_visited - earlier.changes_visited,
            values_decoded: self.values_decoded - earlier.values_decoded,
            groups_built: self.groups_built - earlier.groups_built,
            books_checked: self.books_checked - earlier.books_checked,
            chapters_cohered: self.chapters_cohered - earlier.chapters_cohered,
            verses_cohered: self.verses_cohered - earlier.verses_cohered,
            attached_placed: self.attached_placed - earlier.attached_placed,
            conflicts_built: self.conflicts_built - earlier.conflicts_built,
            records_projected: self.records_projected - earlier.records_projected,
            values_read: self.values_read - earlier.values_read,
            anomalies_touched: self.anomalies_touched - earlier.anomalies_touched,
            records_listed: self.records_listed - earlier.records_listed,
        }
    }
}

/// Change-level defects. A function of the change set, so every device with the same
/// changes lists the same anomalies.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Anomaly {
    QuarantinedChange {
        change: ChangeId,
        rule: RuleViolation,
    },
    IncompleteBatch {
        batch: BatchId,
        device: DeviceId,
        have: u32,
        of: u32,
    },
    MissingParent {
        change: ChangeId,
        parent: ChangeId,
    },
    ParentOnOtherRecord {
        change: ChangeId,
        parent: ChangeId,
    },
    DuplicateDevice {
        device: DeviceId,
        seq: DeviceSeq,
        changes: Vec<ChangeId>,
    },
    /// Changes of record kinds from a newer minor format (§12).
    UnknownKinds {
        kinds: Vec<Box<str>>,
        changes: u32,
    },
    /// Records of a book that no `BookPresence` change makes present (§7.6): kept, listed.
    RecordsWithoutBook {
        book: BookId,
        records: u32,
    },
    /// A value that would make `metadata.json` invalid; an older valid value is used (§3.4).
    InvalidForBurrito {
        key: RecordKey,
        change: ChangeId,
        reason: Box<str>,
    },
}

/// What a pack missing from or damaged in the working tree was (§4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PackLoss {
    Missing,
    Damaged,
}

/// Why this folder got a new Local device (§6.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RotationReason {
    /// `.tc/device.json` is missing, or its fingerprint differs (a copied or moved folder).
    FolderCopied,
    /// The ledger says this device wrote more than the folder holds (an older backup).
    BackupRestored,
    /// Two machines saved as this device.
    DuplicateDevice,
    /// A file of saved changes in the folder could not be read when the project opened, so
    /// it may hold sequence numbers this device already used (§6.3).
    PendingUnreadable,
}

/// Why a materialization or a commit is blocked (R16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// Another program holds the file and the rename kept failing (a scanner, Paratext).
    FileBusy,
    /// The file could not be read to compare it before it is written over (§4.4).
    FileUnreadable,
    /// `records` values of the fold cannot be read on this computer yet (§2.6): nothing is
    /// written from a value that is not read.
    ValuesUnreadable { records: u32 },
    /// An expected pack is missing (§4.4): nothing derived from it is removed.
    PackMissing,
    /// The commit's tree is not a valid Scripture Burrito (§9.4): no commit was made.
    TreeInvalid { problem: Box<str> },
    /// A write failed for another reason.
    Failed { reason: Box<str> },
}

/// Notices depend on the files and the machine, so `tc-store` computes them outside the
/// fold, and they are not part of the convergence comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    QuarantinedPack {
        path: RepoPath,
        reason: PackError,
        salvaged: u32,
    },
    QuarantinedPending {
        file: Box<str>,
        reason: PackError,
        salvaged: u32,
    },
    /// Carried, not read.
    NewerPack {
        path: RepoPath,
        tcpack: u64,
    },
    /// `at` is more than 24 h after this machine's clock.
    ClockAhead {
        device: DeviceId,
        by_ms: i64,
    },
    CarriedBytesMissing {
        path: RepoPath,
        oid: GitOid,
    },
    DeviceRotated {
        old: DeviceId,
        new: DeviceId,
        reason: RotationReason,
    },
    RestoredBackup {
        missing: u64,
        last_at: WallTime,
    },
    /// A file changed outside the app was recorded as `changes` changes in one batch
    /// (§11.1). `unknown_base`: they were recorded without parents, so each is a conflict
    /// with the project's text (the review offers *Accept all from the file*, *Keep
    /// translationCore's text* and *Review one by one*); otherwise a toast with *Undo*.
    OutsideEditRecorded {
        path: RepoPath,
        changes: u32,
        batch: BatchId,
        unknown_base: bool,
    },
    /// The file no longer shows `records` alignments, decisions or comments it was written
    /// with. A file cannot remove them (no removal value exists, §11.1 X3): they stay, and
    /// the file is written again with them.
    OutsideRemovalKept {
        path: RepoPath,
        records: u32,
    },
    /// `metadata.json` was changed in fields translationCore writes itself (§11.1 X2); they
    /// are written again from the project.
    DerivedFieldsEdited {
        path: RepoPath,
        fields: u32,
    },
    /// `project.json`, `format.json` or `conflicts.json` was changed outside the app (§3.5):
    /// it is written again, and `copy` holds the bytes found.
    FixedFileRestored {
        path: RepoPath,
        copy: Box<str>,
    },
    /// `.git` was damaged and was built again from the project (§6.4 G4); `broken` is where
    /// the damaged one was kept.
    HistoryRebuilt {
        broken: Box<str>,
    },
    /// The folder's history cannot be opened (for example another account owns `.git`); the
    /// session runs without checkpoints (§6.1 O3).
    HistoryUnavailable {
        reason: Box<str>,
    },
    /// A carried file over 256 MiB is not recorded and not committed (§11.4).
    CarriedTooLarge {
        path: RepoPath,
        size: u64,
    },
    /// A save the disk refused leaves its file behind; the person saved again, so the open
    /// kept the earlier attempt as a copy instead of folding it beside their later work
    /// (§6.1 O2, §5.0 B3).
    UnsavedAttemptKept {
        copy: Box<str>,
        changes: u32,
        at: WallTime,
    },
    OutsideEditUnreadable {
        path: RepoPath,
        copy: Box<str>,
        reason: Box<str>,
    },
    /// Not a valid ingredient path.
    FileMoved {
        from: Box<str>,
        to: Box<str>,
    },
    MissingFileRebuilt {
        path: RepoPath,
    },
    /// A pack missing from the working tree or damaged there, written back from a valid
    /// blob in the object database (§4.4).
    PackRestored {
        path: RepoPath,
        was: PackLoss,
    },
    /// A pending file of this session's saves was deleted or cut short by another program
    /// before the seal took it. The session held the `changes` saves it lost in memory, and the
    /// seal wrote them into its pack (§5.11 K1): nothing is lost.
    PendingWrittenAgain {
        file: Box<str>,
        changes: u32,
    },
    /// A file that could not be read. Never treated as damage; retried later.
    FileUnreadable {
        path: Box<str>,
        reason: Box<str>,
    },
    /// Size and mtime equal the manifest but the bytes differ: storage damage, not an edit.
    StorageDamage {
        path: RepoPath,
        copy: Box<str>,
    },
    /// A derived file changed outside the app. It is held, unchanged and unrecorded, and a
    /// copy is kept, until the edit is recorded as changes (§11.1) or the person restores
    /// the file from the project; nothing is written over it meanwhile (I8).
    OutsideEditHeld {
        path: RepoPath,
        copy: Box<str>,
    },
    /// `metadata.json` held bytes the app did not write (a project detail edited in a text
    /// editor, or any bytes when no manifest says what the app wrote). The app writes the file
    /// from the project's records, since it must list the files as they are, so the bytes
    /// found are kept in `copy` first (I8, §5.10). `metadata.json` is no ingredient, so no
    /// [`RepoPath`] names it.
    MetadataEditedOutside {
        copy: Box<str>,
    },
    /// Derived files (§5.10) or a commit (§5.13) could not be written, and the work is saved:
    /// "translationCore can't update TIT.usfm because another program has it open. Your
    /// work is saved." The paths stay dirty and are tried again; the notice is listed once for
    /// each reason and path set, and goes when the blockage clears (R16).
    MaterializeBlocked {
        paths: Vec<RepoPath>,
        reason: Blocked,
    },
    /// An expected pack (§4.4) is missing from the working tree and no copy of it could be
    /// restored: nothing derived from it is removed, and no tree is built, until it is back.
    PackMissing {
        path: RepoPath,
    },
    /// Line-ending or trailing-whitespace differences in an outside edit, not recorded.
    WhitespaceOnlyEdit {
        path: RepoPath,
        keys: u32,
    },
    /// Another installation writes into this folder.
    SecondWriter {
        device: DeviceId,
    },
    /// A save's file was renamed into place but the disk reported an error after it.
    SavedWithDiskError {
        reason: Box<str>,
    },
    ImportUnfinished {
        source: Box<str>,
    },
    /// A book file or story changed outside the app while the project is open read-only:
    /// nothing is kept and nothing is asked ("Titus was changed outside translationCore. Open
    /// the project normally to import it.", delta §2.11, H22 part 4).
    ChangedWhileReadOnly {
        path: RepoPath,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_ids_ignore_order_and_repeats() {
        let a = ChangeId::of(b"a");
        let b = ChangeId::of(b"b");
        assert_eq!(ConflictId::of(&[a, b]), ConflictId::of(&[b, a, b]));
        assert_ne!(ConflictId::of(&[a, b]), ConflictId::of(&[a]));
        assert_eq!(Frontier::of(&[]).to_hex(), hex(&sha256(b"")));
        let mut both = Vec::new();
        let (first, second) = if a < b { (a, b) } else { (b, a) };
        both.extend_from_slice(first.as_bytes());
        both.extend_from_slice(second.as_bytes());
        assert_eq!(Frontier::of(&[b, a]).to_hex(), hex(&sha256(&both)));
        let id = ConflictId::of(&[a]);
        assert_eq!(ConflictId::parse(&id.to_hex()), Some(id));
    }
}
