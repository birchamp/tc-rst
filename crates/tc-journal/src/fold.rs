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
mod merge;
pub(crate) mod meta;
mod metadata;
mod passage;
mod people;
mod project;
mod reads;
mod resolve;
mod sidecar;
mod state;
pub(crate) mod value;

pub use meta::ChangeIdx;
pub use people::PersonLabel;
pub use project::{CarriedPlacement, CheckTexts, DerivedFile, ProjectedDecision};
pub use reads::{BookView, ChangeView, Placement};
pub use resolve::ResolveError;
pub use sidecar::{SidecarError, decode_alignments, decode_decisions, records_from_usfm};
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
    /// Several heads; the projection uses `head`, no conflict is listed (§7.3 stub rules).
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
    /// Carried or derived paths may have moved.
    pub paths: bool,
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
    OutsideEditRecorded {
        path: RepoPath,
        changes: u32,
        batch: BatchId,
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
