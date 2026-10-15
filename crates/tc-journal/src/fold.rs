//! The fold's output types (`docs/STORAGE.md` §2.6). The fold itself (§7) fills them.

use crate::change::{Cause, Change};
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceSeq, GitOid, PackId, RepoPath, WallTime};
use crate::json::Json;
use crate::keys::RecordKey;
use crate::pack::PackError;
use crate::record::Record;
use crate::rules::RuleViolation;
use crate::text::{hex, sha256};
use smallvec::SmallVec;
use std::fmt;
use tc_core::BookId;

/// A handle to a change in a [`ChangeIndex`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangeIdx(u32);

impl ChangeIdx {
    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// A folded change and the packs that hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedChange {
    pub change: Change,
    /// Empty for a change that is only in a pending file.
    pub packs: SmallVec<[PackId; 1]>,
}

/// Every folded change, by handle and by id. Persistent, so a snapshot is O(1).
#[derive(Debug, Clone, Default)]
pub struct ChangeIndex {
    by_idx: im::Vector<IndexedChange>,
    by_id: im::HashMap<ChangeId, ChangeIdx>,
}

impl ChangeIndex {
    /// Adds a change, or notes one more pack holding a change already here (two lines with
    /// one id have the same bytes, so nothing is lost).
    pub fn insert(&mut self, change: Change, pack: Option<PackId>) -> ChangeIdx {
        if let Some(idx) = self.by_id.get(&change.id()).copied() {
            if let Some(pack) = pack
                && let Some(entry) = self.by_idx.get_mut(idx.get())
                && !entry.packs.contains(&pack)
            {
                entry.packs.push(pack);
            }
            return idx;
        }
        // A project holds far fewer than 2³² changes (§14 estimates 620,000 in three years).
        let idx = ChangeIdx(u32::try_from(self.by_idx.len()).unwrap_or(u32::MAX));
        self.by_id.insert(change.id(), idx);
        self.by_idx.push_back(IndexedChange {
            change,
            packs: pack.into_iter().collect(),
        });
        idx
    }

    pub fn get(&self, idx: ChangeIdx) -> Option<&IndexedChange> {
        self.by_idx.get(idx.get())
    }

    pub fn find(&self, id: &ChangeId) -> Option<ChangeIdx> {
        self.by_id.get(id).copied()
    }

    pub fn len(&self) -> usize {
        self.by_idx.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_idx.is_empty()
    }
}

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

/// The result of folding.
#[derive(Debug, Clone)]
pub struct FoldState {
    pub changes: ChangeIndex,
    pub keys: im::HashMap<RecordKey, KeyState>,
    /// Sorted by key: canon book order, then numeric.
    pub conflicts: Vec<Conflict>,
    /// Sorted by kind, then key.
    pub anomalies: Vec<Anomaly>,
    /// Heads with no place on any screen, listed under Detached (§7.7).
    pub detached: Vec<RecordKey>,
    pub frontier: Frontier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyState {
    pub heads: SmallVec<[ChangeIdx; 1]>,
    pub state: Resolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    Settled {
        head: ChangeIdx,
    },
    /// Several heads; the projection uses `head`, no conflict is listed (§7.3 stub rules).
    SettledByRule {
        head: ChangeIdx,
        rule: AutoRule,
    },
    /// Several heads merged without disagreement. Computed, never stored.
    Merged {
        merged: Box<Record>,
        how: MergeHow,
    },
    Conflict(ConflictId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRule {
    SeedStubs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeHow {
    EqualValues,
    Units(Vec<UnitName>),
}

/// A merge unit (§7.3): a group of fields taken together, named by its JSON path in the
/// record's value, such as `/words` or `/judgement`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitName(pub Box<str>);

/// A unit's value as stored, unknown fields of a newer minor included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitValue(pub Json);

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
    /// A head has a MissingParent link (§8.1).
    pub incomplete_history: bool,
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
    BookRemovedWhileEdited { unseen: u32 },
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
