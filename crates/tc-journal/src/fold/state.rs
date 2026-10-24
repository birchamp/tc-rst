//! The fold's state and its one update path (§7.2, §7.8). A full fold is an empty state
//! with every change added at once; an incremental step adds a few. Both run the same
//! stages over what the new changes made dirty, and each stage recomputes a dirty unit (a
//! key, a passage group, a book, a chapter) from the current set alone, never from a
//! delta. So the incremental state equals a fold from scratch (`incremental_equals_full`).

use super::cow::{ChunkVec, CowMap, PosIndex};
use super::meta::{
    self, BatchIdx, CauseClass, ChangeIdx, ChangeList, DecisionForm, DeviceIdx, FirstParent, Form,
    KeyIdx, Meta, PackSlot, ParentRef, Quarantined, ResourceTable, Resources, Shape, ValueRef,
};
use super::value::{Stored, ValueAt, ValueSource};
use super::{Anomaly, Conflict, ConflictId, Counter, CounterCells, Counters, Frontier};
use crate::change::Verified;
use crate::ids::StoryNo;
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceKind, DeviceSeq, PackId};
use crate::keys::{CheckKey, CheckTarget, RecordKey};
use crate::record::Record;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tc_core::BookId;

/// Changes indexed together by a fold from scratch: large enough that each pass's lookups
/// overlap, small enough that a chunk's prepared inputs are a few megabytes.
const INDEX_CHUNK: usize = 1 << 15;

/// A Bible project keeps one book present (§7.6, the last-book rule); an OBS project has no
/// books.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectKind {
    Bible,
    Stories,
}

/// One input line of the fold: a verified change line and the pack that holds it, if any.
#[derive(Debug, Clone)]
pub struct FoldInput {
    line: Line,
    pack: Option<PackId>,
}

#[derive(Debug, Clone)]
enum Line {
    Verified(Verified),
    /// What the fold keeps of a change, read ahead; the decoded value is gone.
    Prepared {
        id: ChangeId,
        value: Stored,
        summary: meta::Summary,
    },
}

impl FoldInput {
    pub fn new(line: Verified, pack: Option<PackId>) -> Self {
        Self {
            line: Line::Verified(line),
            pack,
        }
    }

    /// The id of the change line.
    pub fn id(&self) -> ChangeId {
        match &self.line {
            Line::Verified(line) => line.id(),
            Line::Prepared { id, .. } => *id,
        }
    }

    /// Reads what the fold keeps of the change and drops its decoded value. A loader that
    /// reads packs on several threads calls this there, so the fold itself only indexes
    /// (a decoded value is many small allocations; reading and freeing them is most of the
    /// cost of a change).
    pub fn prepare(self) -> Self {
        self.prepare_with(|change| Stored::bytes(change.raw().clone()))
    }

    /// [`FoldInput::prepare`] that keeps only where the change's bytes are: the fold reads
    /// its value from its [`ValueSource`] when it needs it (§14).
    pub fn prepare_at(self, at: ValueAt) -> Self {
        self.prepare_with(|change| {
            Stored::located(at, change.raw()).unwrap_or_else(|| Stored::bytes(change.raw().clone()))
        })
    }

    /// The parts of a prepared change: its id, where its value is, and its summary.
    pub(crate) fn prepared(&self) -> Option<(ChangeId, &Stored, &meta::Summary)> {
        match &self.line {
            Line::Prepared { id, value, summary } => Some((*id, value, summary)),
            Line::Verified(_) => None,
        }
    }

    /// A prepared change read back from where it was kept ([`crate::index`]).
    pub(crate) fn from_prepared(
        id: ChangeId,
        value: Stored,
        summary: meta::Summary,
        pack: Option<PackId>,
    ) -> Self {
        Self {
            line: Line::Prepared { id, value, summary },
            pack,
        }
    }

    /// The pack that holds the line, if any.
    pub(crate) fn pack(&self) -> Option<PackId> {
        self.pack
    }

    fn prepare_with(self, store: impl FnOnce(&crate::change::Change) -> Stored) -> Self {
        match self.line {
            Line::Verified(Verified::Change(change)) => {
                let id = change.id();
                let value = store(&change);
                let summary = meta::summarize(change.body(), &mut Resources::default());
                Self {
                    line: Line::Prepared { id, value, summary },
                    pack: self.pack,
                }
            }
            line => Self {
                line,
                pack: self.pack,
            },
        }
    }
}

impl From<Verified> for FoldInput {
    fn from(line: Verified) -> Self {
        Self::new(line, None)
    }
}

impl From<crate::change::Change> for FoldInput {
    fn from(change: crate::change::Change) -> Self {
        Self::new(Verified::Change(change), None)
    }
}

/// A change to append to the change table ([`FoldState::push_meta`]).
struct NewChange {
    id: ChangeId,
    key: KeyIdx,
    batch: BatchIdx,
    parents: SmallVec<[ParentRef; 2]>,
    device: DeviceIdx,
    seq: DeviceSeq,
    at: crate::ids::WallTime,
    cause: CauseClass,
    stub: bool,
    shape: Shape,
    value: Stored,
    pack: PackSlot,
}

/// How a record with several heads merged without disagreement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeHow {
    /// Every head has the same value.
    EqualValues,
    /// Merge units (§7.3) taken from different heads.
    Units(Vec<super::UnitName>),
    /// A grow-only record (`BatchRelease`): the union of the heads.
    Union,
}

/// A key's own resolution, before passage groups (§7.3).
#[derive(Debug, Clone)]
pub(crate) enum Local {
    /// No folded change.
    Empty,
    Settled(ChangeIdx),
    /// Seed stubs absorbed (§7.3 stub rules).
    ByRule(ChangeIdx),
    /// Boxed, like `Conflict`: few records merge or conflict, and every key holds a `Local`.
    Merged(Box<(Arc<Record>, MergeHow)>),
    /// A conflict of this record alone.
    Conflict(Box<LocalConflict>),
    /// Diverges from a passage batch: its group decides (§7.3 Passages).
    Passage,
}

#[derive(Debug, Clone)]
pub(crate) struct LocalConflict {
    pub id: ConflictId,
    /// The head projection shows (F7).
    pub provisional: ChangeIdx,
    /// A head's or base's value could not be read (§2.6): listed until it can.
    pub unreadable: bool,
}

/// What projection shows for a record.
#[derive(Debug, Clone)]
pub(crate) enum Projected {
    Nothing,
    Change(ChangeIdx),
    Merged(Arc<Record>),
}

/// A link F3 ignores: the child, the parent it names, and why.
pub(crate) type Link = (ChangeIdx, ChangeId, Fault);

/// A parent link the fold ignores (F3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fault {
    Missing,
    OtherRecord,
}

#[derive(Debug, Clone)]
pub(crate) struct KeyEntry {
    pub key: RecordKey,
    pub book: Option<BookId>,
    /// Every change of this key the fold holds, folded or held.
    pub changes: ChangeList,
    /// Folded changes with no folded child on this key, sorted by id.
    pub heads: ChangeList,
    pub local: Local,
    /// The passage group this record is in, named by its first record.
    pub group: Option<KeyIdx>,
    pub projected: Projected,
    /// Folded changes of this key.
    pub active: u32,
}

/// A batch (§7.4): 28 bytes for each of half a million batches. Most batches hold one
/// change, so the members of a batch of several live in [`FoldState::members`], and what
/// only a held batch needs in [`FoldState::held`].
#[derive(Debug, Clone)]
pub(crate) struct BatchEntry {
    pub id: BatchId,
    pub n: u32,
    /// Its one valid change, folded or held, while it has one ([`ChangeIdx::NONE`] before).
    first: ChangeIdx,
    /// Its valid changes are in [`FoldState::members`]: it has two or more.
    many: bool,
    pub complete: bool,
    /// An entry in [`FoldState::held`] exists for it.
    pub held: bool,
}

/// What the fold keeps of a batch that is, or was, held.
#[derive(Debug, Clone, Default)]
pub(crate) struct Held {
    /// Its quarantined lines (§4.4): a batch with one is never complete.
    pub quarantined: u32,
    /// The device of a quarantined member, when no valid member names one.
    pub quarantined_device: Option<DeviceId>,
    /// The `IncompleteBatch` anomaly listed for it.
    pub anomaly: Option<Anomaly>,
}

/// A passage group (§7.3), named by its first record.
#[derive(Debug, Clone)]
pub(crate) struct Group {
    pub records: Vec<KeyIdx>,
    pub conflict: Arc<Conflict>,
}

/// A chapter's records and what coherence and placement found there.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChapterEntry {
    pub front: Option<KeyIdx>,
    pub verses: BTreeMap<u16, KeyIdx>,
    /// Alignments keyed by an anchor in this chapter.
    pub alignments: BTreeMap<u16, KeyIdx>,
    /// Verse comments whose anchor is in this chapter.
    pub comments: Vec<KeyIdx>,
    /// Decisions whose target chapter is this chapter.
    pub checks: Vec<KeyIdx>,
    /// The anchors the projected text of this chapter holds: anchor → verse.
    pub anchors: Vec<(super::attach::Anchor, u16)>,
    /// Verses that exist in the projection (a text, or covered by a run).
    pub present: BTreeSet<u16>,
}

/// How a book projects (§7.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BookStatus {
    /// No folded presence change mentions the book.
    Unknown,
    /// Projects, with the scope of this `Present` change.
    Present { scope: ChangeIdx },
    /// Every head is `Removed`, and nothing unseen: projects nothing.
    Removed,
    /// Every head is `Removed` but `unseen` changes were not held by any remover: projects.
    RemovedWhileEdited {
        scope: Option<ChangeIdx>,
        unseen: u32,
    },
    /// The last book of a Bible project, kept present (§7.6).
    KeptAsLast { scope: Option<ChangeIdx> },
}

impl BookStatus {
    pub(crate) fn projects(&self) -> bool {
        matches!(
            self,
            Self::Present { .. } | Self::RemovedWhileEdited { .. } | Self::KeptAsLast { .. }
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BookEntry {
    pub presence: Option<KeyIdx>,
    pub header: Option<KeyIdx>,
    pub layout: Option<KeyIdx>,
    pub chapters: BTreeSet<u16>,
    /// Decisions whose target chapter is not a number (`front`).
    pub other_checks: Vec<KeyIdx>,
    /// Comments of the book filed under no chapter: on a check whose target chapter is not
    /// a number.
    pub other_comments: Vec<KeyIdx>,
    /// Every chapter with a [`ChapterEntry`] of this book: its range reads (R1) visit these.
    pub filed: BTreeSet<u16>,
    /// Every key that belongs to the book (§7.6), for removal visibility.
    pub keys: Vec<KeyIdx>,
    /// Keys of the book with a folded change.
    pub records: u32,
    pub status: BookStatus,
    /// Attached records placed by following `moved_to` (§7.7 rule 3): placed again when
    /// any chapter of the book changes.
    pub chained: BTreeSet<KeyIdx>,
}

impl BookEntry {
    /// The chapters this book files records under, ascending.
    pub(crate) fn chapters_with_records(&self) -> impl Iterator<Item = u16> + '_ {
        self.filed.iter().copied()
    }

    fn new() -> Self {
        Self {
            presence: None,
            header: None,
            layout: None,
            chapters: BTreeSet::new(),
            other_checks: Vec::new(),
            other_comments: Vec::new(),
            filed: BTreeSet::new(),
            keys: Vec::new(),
            records: 0,
            status: BookStatus::Unknown,
            chained: BTreeSet::new(),
        }
    }
}

/// Who owns a listed conflict, so recomputing the owner replaces its conflicts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Owner {
    Key(KeyIdx),
    Group(KeyIdx),
    Chapter(BookId, u16),
    Book(BookId),
    Paths,
}

/// The order of the conflict list: canon book order, then the key, then the kind.
pub(crate) type ConflictSort = (u8, RecordKey, u8, ConflictId);

/// What the fold made dirty and must recompute.
#[derive(Debug, Default)]
pub(crate) struct Dirty {
    pub batches: BTreeSet<BatchIdx>,
    pub keys: BTreeSet<KeyIdx>,
    /// Keys whose only news is one new folded change whose parents are its key's folded
    /// changes: their heads update in O(1).
    pub simple: BTreeMap<KeyIdx, ChangeIdx>,
    /// Keys whose state, projection or group changed.
    pub changed: BTreeSet<KeyIdx>,
    pub books: BTreeSet<BookId>,
    pub chapters: BTreeSet<(BookId, u16)>,
    pub attached: BTreeSet<KeyIdx>,
    /// Passage batches that grew after some of their changes were folded: their passage
    /// grew, so the groups it reaches are rebuilt.
    pub regroup: BTreeSet<BatchIdx>,
    pub pins: bool,
    /// A book's projection appeared, vanished or changed scope.
    pub paths: bool,
    /// A `CarriedFile` record changed.
    pub carried: bool,
    pub unknown: bool,
    /// A change stopped being folded: rebuild from scratch.
    pub rebuild: bool,
    pub added_conflicts: BTreeSet<ConflictId>,
    pub removed_conflicts: BTreeSet<ConflictId>,
    /// Everything is dirty: a fold from scratch. Stages walk their whole tables instead of
    /// dirty sets; each unit is recomputed by the same code either way.
    pub all: bool,
}

/// The fold of a set of changes.
#[derive(Debug, Clone)]
pub struct FoldState {
    pub(crate) kind: ProjectKind,
    pub(crate) metas: ChunkVec<Meta>,
    /// The parents after the first of each change that names several.
    pub(crate) more_parents: CowMap<ChangeIdx, SmallVec<[ParentRef; 2]>>,
    /// The sequence numbers that do not fit in the change table ([`Meta::SEQ_APART`]).
    pub(crate) seqs_apart: CowMap<ChangeIdx, DeviceSeq>,
    /// The canonical bytes of the changes whose bytes the fold keeps ([`Meta::IN_MEMORY`]):
    /// those a command added, or a loader handed over without a place to read them again.
    pub(crate) kept: CowMap<ChangeIdx, Arc<[u8]>>,
    /// The shapes of the changes whose [`Form`] keeps no payload: moves, comments, carried
    /// files and releases.
    pub(crate) shapes: CowMap<ChangeIdx, Shape>,
    /// The ids of parents the fold did not hold when a change named them (`ParentRef`).
    pub(crate) absent: ChunkVec<ChangeId>,
    /// The devices changes name, by [`DeviceIdx`].
    pub(crate) devices: Arc<Vec<DeviceId>>,
    pub(crate) device_index: CowMap<DeviceId, DeviceIdx>,
    /// Changes by id ([`id_hash`]).
    pub(crate) ids: PosIndex,
    /// The packs that hold folded changes, by [`PackSlot`].
    pub(crate) packs: Arc<Vec<PackId>>,
    pub(crate) pack_slots: CowMap<PackId, PackSlot>,
    /// The packs after the first that hold a change.
    pub(crate) more_packs: CowMap<ChangeIdx, SmallVec<[PackSlot; 2]>>,
    pub(crate) keys: ChunkVec<KeyEntry>,
    /// The links F3 ignores, by key. Few keys have any, so they live beside the key table.
    pub(crate) faults: CowMap<KeyIdx, SmallVec<[Link; 1]>>,
    /// The passage batches a record diverges from (§7.3), by key; few records have any.
    pub(crate) divs: CowMap<KeyIdx, SmallVec<[BatchIdx; 1]>>,
    /// Keys by [`key_hash`]; the key table holds the only copy of each key.
    pub(crate) key_ids: PosIndex,
    pub(crate) batches: ChunkVec<BatchEntry>,
    /// The valid changes of each batch that has two or more, in the order the fold met them.
    pub(crate) members: CowMap<BatchIdx, Vec<ChangeIdx>>,
    /// Batches by id ([`batch_hash`]); one per `n` the changes of that id claim (F2).
    pub(crate) batch_ids: PosIndex,
    pub(crate) held: CowMap<BatchIdx, Held>,
    /// The books for which a batch is a passage batch (§7.3); few batches are.
    pub(crate) passages: CowMap<BatchIdx, SmallVec<[BookId; 1]>>,
    /// The changes some folded `BatchRelease` lists, by batch id.
    pub(crate) released: CowMap<BatchId, Arc<BTreeSet<ChangeId>>>,
    pub(crate) quarantined: CowMap<ChangeId, Quarantined>,
    /// Children that name a parent that is unknown or held, by that parent.
    pub(crate) waiting: CowMap<ChangeId, SmallVec<[ChangeIdx; 1]>>,
    /// The first change of each Local device and seq (F5), by [`seq_hash`].
    pub(crate) seqs: PosIndex,
    /// Every change of a Local device and seq that more than one change claims.
    pub(crate) duplicate_seqs: CowMap<(DeviceId, DeviceSeq), SmallVec<[ChangeIdx; 2]>>,
    /// Each book's entry shared on its own, so the first write to a book after a
    /// snapshot copies that book's entry, not every book's.
    pub(crate) books: Arc<BTreeMap<BookId, Arc<BookEntry>>>,
    pub(crate) chapters: CowMap<(BookId, u16), ChapterEntry>,
    pub(crate) groups: CowMap<KeyIdx, Group>,
    pub(crate) unknown_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) carried_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) pin_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) profile_keys: CowMap<DeviceId, KeyIdx>,
    pub(crate) story_keys: Arc<BTreeSet<KeyIdx>>,
    /// Decisions on frames, and comments on frames or on frame checks, by the story they
    /// target: what the OBS decision and comment files list (§3.2). An edit of a story's
    /// text places that story's records again, and no other story's.
    pub(crate) story_attached: Arc<BTreeMap<StoryNo, BTreeSet<KeyIdx>>>,
    pub(crate) setting_keys: Arc<BTreeSet<KeyIdx>>,
    /// Keys whose resolution met a value the source could not read (§2.6); judged again
    /// by [`FoldState::retry_unreadable`].
    pub(crate) unreadable: CowMap<KeyIdx, ()>,
    pub(crate) conflicts: im::OrdMap<ConflictSort, Arc<Conflict>>,
    pub(crate) conflict_ids: im::HashMap<ConflictId, ConflictSort>,
    pub(crate) owned: CowMap<Owner, SmallVec<[ConflictSort; 1]>>,
    pub(crate) anomalies: im::OrdSet<Anomaly>,
    /// The `RecordsWithoutBook` anomaly listed for each book.
    pub(crate) without_book: CowMap<BookId, Anomaly>,
    /// The `UnknownKinds` anomaly listed, if any.
    pub(crate) unknown_anomaly: Option<Anomaly>,
    pub(crate) detached: im::OrdSet<RecordKey>,
    /// Conflict counts by book and kind order.
    pub(crate) counts: super::ConflictCounts,
    /// Anchor (book, chapter, verse) → the verses whose projected text holds it.
    pub(crate) anchors: CowMap<(BookId, u16, u16), BTreeSet<(u16, u16)>>,
    pub(crate) pins: Arc<super::attach::PinsView>,
    /// What each project detail, presence and pin group contributes to `metadata.json`.
    pub(crate) meta_choices: Arc<BTreeMap<KeyIdx, super::metadata::MetaChoice>>,
    /// Carried files placed at a `.case-` path (§3.3).
    pub(crate) case_moves: Arc<BTreeMap<crate::ids::RepoPath, String>>,
    /// The resources decisions name, by the place a [`DecisionForm`] gives.
    pub(crate) resources: ResourceTable,
    /// Reads the values the fold does not keep in memory.
    pub(crate) source: Option<Arc<dyn ValueSource>>,
    /// Records decoded ahead for one rendering (see [`FoldState::render_book`]); `None` in
    /// the state itself.
    pub(crate) records: Option<Arc<super::cow::FxMap<ChangeIdx, Arc<Record>>>>,
    pub(crate) counters: Arc<CounterCells>,
    /// Reads of a value the source could not give (§2.6), counted apart from the counters so
    /// a state given counters of its own keeps counting them.
    pub(crate) unreadable_reads: Arc<std::sync::atomic::AtomicU64>,
}

impl FoldState {
    /// An empty fold.
    pub fn new(kind: ProjectKind) -> Self {
        Self {
            kind,
            metas: ChunkVec::default(),
            more_parents: CowMap::default(),
            seqs_apart: CowMap::default(),
            kept: CowMap::default(),
            shapes: CowMap::default(),
            absent: ChunkVec::default(),
            devices: Arc::new(Vec::new()),
            device_index: CowMap::default(),
            ids: PosIndex::default(),
            packs: Arc::new(Vec::new()),
            pack_slots: CowMap::default(),
            more_packs: CowMap::default(),
            keys: ChunkVec::default(),
            faults: CowMap::default(),
            divs: CowMap::default(),
            key_ids: PosIndex::default(),
            batches: ChunkVec::default(),
            members: CowMap::default(),
            batch_ids: PosIndex::default(),
            held: CowMap::default(),
            passages: CowMap::default(),
            released: CowMap::default(),
            quarantined: CowMap::default(),
            waiting: CowMap::default(),
            seqs: PosIndex::default(),
            duplicate_seqs: CowMap::default(),
            books: Arc::new(BTreeMap::new()),
            chapters: CowMap::default(),
            groups: CowMap::default(),
            unknown_keys: Arc::new(BTreeSet::new()),
            carried_keys: Arc::new(BTreeSet::new()),
            pin_keys: Arc::new(BTreeSet::new()),
            profile_keys: CowMap::default(),
            story_keys: Arc::new(BTreeSet::new()),
            story_attached: Arc::new(BTreeMap::new()),
            setting_keys: Arc::new(BTreeSet::new()),
            unreadable: CowMap::default(),
            conflicts: im::OrdMap::new(),
            conflict_ids: im::HashMap::new(),
            owned: CowMap::default(),
            anomalies: im::OrdSet::new(),
            without_book: CowMap::default(),
            unknown_anomaly: None,
            detached: im::OrdSet::new(),
            counts: BTreeMap::new(),
            anchors: CowMap::default(),
            pins: Arc::new(super::attach::PinsView::default()),
            meta_choices: Arc::new(BTreeMap::new()),
            case_moves: Arc::new(BTreeMap::new()),
            resources: ResourceTable::default(),
            source: None,
            records: None,
            counters: Arc::new(CounterCells::default()),
            unreadable_reads: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    /// Folds a set of changes from scratch (§7.2).
    pub fn fold(kind: ProjectKind, changes: impl IntoIterator<Item = FoldInput>) -> Self {
        Self::fold_from(kind, None, changes)
    }

    /// [`FoldState::fold`] of changes some of whose values `source` reads on demand
    /// (inputs made by [`FoldInput::prepare_at`]).
    pub fn fold_from(
        kind: ProjectKind,
        source: Option<Arc<dyn ValueSource>>,
        changes: impl IntoIterator<Item = FoldInput>,
    ) -> Self {
        Self::fold_from_with_steps(kind, source, changes, &mut |_, _| {})
    }

    /// The stages a fold from scratch runs after the last change is indexed: what
    /// [`FoldState::fold_from_with_steps`] counts.
    pub const FOLD_STEPS: u32 = 11;

    /// [`FoldState::fold_from`] that calls `step(done, total)` after each stage it runs once
    /// the last change is indexed, so an open can show how much of the fold is done (§6.1
    /// "Progress"). `total` is [`FoldState::FOLD_STEPS`].
    pub fn fold_from_with_steps(
        kind: ProjectKind,
        source: Option<Arc<dyn ValueSource>>,
        changes: impl IntoIterator<Item = FoldInput>,
        step: &mut dyn FnMut(u32, u32),
    ) -> Self {
        let mut state = Self::new(kind);
        state.source = source;
        let mut dirty = Dirty {
            all: true,
            ..Dirty::default()
        };
        // Indexed as they come, a chunk at a time: a loader that reads packs on other
        // threads hands changes over while it reads the rest, and the prepared inputs of a
        // whole project are never all alive at once.
        let mut chunk: Vec<FoldInput> = Vec::with_capacity(INDEX_CHUNK);
        for input in changes {
            chunk.push(input);
            if chunk.len() == INDEX_CHUNK {
                state.index_all(&mut chunk, &mut dirty);
            }
        }
        state.index_all(&mut chunk, &mut dirty);
        state.resolve_absent_parents();
        step(1, Self::FOLD_STEPS);
        state.run_counted(&mut dirty, &mut |n| step(1 + n, Self::FOLD_STEPS));
        if dirty.rebuild {
            state.rebuild();
        }
        step(Self::FOLD_STEPS, Self::FOLD_STEPS);
        state
    }

    /// After indexing in chunks: a parent a change named before the parent's own chunk
    /// arrived becomes a link to it, as if the changes had come together.
    fn resolve_absent_parents(&mut self) {
        if self.absent.len() == 0 {
            return;
        }
        for c in 0..self.metas.len() {
            let c = ChangeIdx(c as u32);
            if self.parents(c).all(|p| p.get().is_ok()) {
                continue;
            }
            let refs: SmallVec<[ParentRef; 2]> = self
                .parents(c)
                .map(|p| match p.get() {
                    Ok(_) => p,
                    Err(at) => self
                        .change_idx(&self.absent[at])
                        .map_or(p, ParentRef::known),
                })
                .collect();
            self.set_parents(c, refs);
        }
    }

    /// The parents change `c` names, in its own order.
    pub(crate) fn parents(&self, c: ChangeIdx) -> impl Iterator<Item = ParentRef> + '_ {
        let meta = self.meta(c);
        let more = if meta.has(Meta::MORE_PARENTS) {
            self.more_parents.get(&c).map_or(&[][..], |m| m.as_slice())
        } else {
            &[]
        };
        meta.first_parent
            .get()
            .into_iter()
            .chain(more.iter().copied())
    }

    /// Whether change `c` names no parent.
    pub(crate) fn has_no_parent(&self, c: ChangeIdx) -> bool {
        self.meta(c).first_parent == FirstParent::NONE
    }

    /// Sets the parents change `c` names: the first in its table entry, the others beside.
    fn set_parents(&mut self, c: ChangeIdx, refs: SmallVec<[ParentRef; 2]>) {
        let mut refs = refs.into_iter();
        let first = FirstParent::of(refs.next());
        let more: SmallVec<[ParentRef; 2]> = refs.collect();
        let meta = &mut self.metas[c.get()];
        meta.first_parent = first;
        if more.is_empty() {
            meta.flags &= !Meta::MORE_PARENTS;
            self.more_parents.remove(&c);
        } else {
            meta.flags |= Meta::MORE_PARENTS;
            self.more_parents.insert(c, more);
        }
    }

    /// The sequence number of change `c`.
    pub(crate) fn seq_of(&self, c: ChangeIdx) -> DeviceSeq {
        match self.meta(c).seq {
            Meta::SEQ_APART => self
                .seqs_apart
                .get(&c)
                .copied()
                .unwrap_or(DeviceSeq(u64::from(Meta::SEQ_APART))),
            seq => DeviceSeq(u64::from(seq)),
        }
    }

    /// The shape of change `c`'s value.
    pub(crate) fn shape(&self, c: ChangeIdx) -> Shape {
        match self.meta(c).form {
            Form::Other => Shape::Other,
            Form::Verse(shape) => Shape::Verse(shape),
            Form::Front { present } => Shape::Front { present },
            Form::Presence(how) => Shape::Presence(how),
            Form::Alignment { dismissed } => Shape::Alignment { dismissed },
            // `push_meta` placed the resource in the table, which never forgets one.
            Form::Decision(form) => match self.resources.get(form.resource()) {
                Some(resource) => Shape::Decision {
                    resource: resource.clone(),
                    dismissed: form.dismissed(),
                },
                None => Shape::Other,
            },
            // `push_meta` kept the whole shape of these, and it never removes one.
            Form::MovedTo | Form::Comment | Form::Carried | Form::Release => {
                self.shapes.get(&c).cloned().unwrap_or(Shape::Other)
            }
        }
    }

    /// Where change `c`'s value is, as a loader hands it over.
    fn stored(&self, c: ChangeIdx) -> Stored {
        let meta = self.meta(c);
        if meta.has(Meta::IN_MEMORY)
            && let Some(bytes) = self.kept.get(&c)
        {
            return Stored::bytes(bytes.clone());
        }
        let value = meta.value;
        Stored::At {
            file: value.file,
            offset: value.offset,
            len: value.len,
            fingerprint: value.fingerprint(),
        }
    }

    /// Appends a change to the change table, and what few changes have beside it.
    fn push_meta(&mut self, new: NewChange) -> ChangeIdx {
        let c = ChangeIdx(self.metas.len() as u32);
        let mut flags = if new.stub { Meta::STUB } else { 0 };
        let seq = match u32::try_from(new.seq.0) {
            Ok(seq) if seq != Meta::SEQ_APART => seq,
            _ => {
                self.seqs_apart.insert(c, new.seq);
                Meta::SEQ_APART
            }
        };
        let value = match new.value {
            Stored::At {
                file,
                offset,
                len,
                fingerprint,
            } => ValueRef::new(file, offset, len, fingerprint),
            Stored::Bytes(bytes) => {
                flags |= Meta::IN_MEMORY;
                self.kept.insert(c, *bytes);
                ValueRef::NONE
            }
        };
        let form = match new.shape {
            Shape::Other => Form::Other,
            Shape::Verse(shape) => Form::Verse(shape),
            Shape::Front { present } => Form::Front { present },
            Shape::Presence(how) => Form::Presence(how),
            Shape::Alignment { dismissed } => Form::Alignment { dismissed },
            Shape::Decision {
                resource,
                dismissed,
            } => Form::Decision(DecisionForm::new(
                self.resources.place(&resource),
                dismissed,
            )),
            shape => {
                let form = match shape {
                    Shape::MovedTo { .. } => Form::MovedTo,
                    Shape::Comment { .. } => Form::Comment,
                    Shape::Carried { .. } => Form::Carried,
                    _ => Form::Release,
                };
                self.shapes.insert(c, shape);
                form
            }
        };
        self.metas.push(Meta {
            id: new.id,
            at: new.at,
            key: new.key,
            batch: new.batch,
            first_parent: FirstParent::NONE,
            device: new.device,
            seq,
            pack: new.pack,
            form,
            value,
            cause: new.cause,
            flags,
        });
        self.set_parents(c, new.parents);
        c
    }

    /// Adds changes to the fold and updates what they touch (§7.8). Changes already held
    /// are taken once; adding the same set again changes nothing.
    pub fn add(&mut self, changes: impl IntoIterator<Item = FoldInput>) -> super::Delta {
        let mut dirty = Dirty::default();
        for input in changes {
            self.insert(input, &mut dirty);
        }
        if dirty.keys.is_empty() && dirty.batches.is_empty() && !dirty.unknown {
            return super::Delta::default();
        }
        self.run(&mut dirty);
        if dirty.rebuild {
            return self.rebuild();
        }
        self.delta(&dirty)
    }

    /// Folds this state's change set again from scratch and installs the result. Used
    /// when a change stops being folded (a quarantined line arriving for a complete batch),
    /// which no incremental stage undoes.
    fn rebuild(&mut self) -> super::Delta {
        let mut inputs = Vec::with_capacity(self.metas.len() + self.quarantined.len());
        for c in 0..self.metas.len() {
            let c = ChangeIdx(c as u32);
            let meta = self.meta(c);
            let entry = self.entry(meta.key);
            let batch = self.batch(meta.batch);
            let summary = meta::Summary {
                key: entry.key.clone(),
                parents: self.parent_ids(c),
                device: self.device_of(meta),
                seq: self.seq_of(c),
                at: meta.at,
                batch: (batch.id, batch.n),
                cause: meta.cause,
                stub: meta.stub(),
                shape: self.shape(c),
                book: entry.book,
            };
            let value = self.stored(c);
            let input = |pack| FoldInput {
                line: Line::Prepared {
                    id: meta.id,
                    value: value.clone(),
                    summary: summary.clone(),
                },
                pack,
            };
            let packs = self.packs_of(c);
            if packs.is_empty() {
                inputs.push(input(None));
            } else {
                inputs.extend(packs.into_iter().map(|pack| input(Some(pack))));
            }
        }
        let quarantined: Vec<FoldInput> = self
            .quarantined
            .iter()
            .filter_map(|(_, q)| q.input.clone())
            .collect();
        let counters = self.counters.clone();
        let mut fresh = Self::new(self.kind);
        fresh.counters = counters;
        fresh.source = self.source.clone();
        let mut dirty = Dirty {
            all: true,
            ..Dirty::default()
        };
        let mut all: Vec<FoldInput> = inputs.into_iter().chain(quarantined).collect();
        fresh.index_all(&mut all, &mut dirty);
        fresh.run(&mut dirty);
        *self = fresh;
        super::Delta::everything()
    }

    // ---------------------------------------------------------------------------------
    // F1: index
    // ---------------------------------------------------------------------------------

    /// The key table's index of `key`, if any change has it.
    pub(crate) fn key_idx(&self, key: &RecordKey) -> Option<KeyIdx> {
        self.key_ids
            .find(key_hash(key), |k| self.keys[k as usize].key == *key)
            .map(KeyIdx)
    }

    /// The change table's index of change `id`, if the fold holds it.
    pub(crate) fn change_idx(&self, id: &ChangeId) -> Option<ChangeIdx> {
        self.ids
            .find(id_hash(id), |c| self.metas[c as usize].id == *id)
            .map(ChangeIdx)
    }

    fn intern_key(&mut self, key: RecordKey, book: Option<BookId>, shape: &Shape) -> KeyIdx {
        let hash = key_hash(&key);
        self.intern_hashed_key(key, hash, book, shape)
    }

    fn intern_hashed_key(
        &mut self,
        key: RecordKey,
        hash: u64,
        book: Option<BookId>,
        shape: &Shape,
    ) -> KeyIdx {
        if let Some(found) = self
            .key_ids
            .find(hash, |k| self.keys[k as usize].key == key)
        {
            return KeyIdx(found);
        }
        let k = KeyIdx(self.keys.len() as u32);
        self.index_key(k, &key, book, shape);
        self.key_ids.insert(hash, k.0);
        self.keys.push(KeyEntry {
            key,
            book,
            changes: ChangeList::Empty,
            heads: ChangeList::Empty,
            local: Local::Empty,
            group: None,
            projected: Projected::Nothing,
            active: 0,
        });
        k
    }

    /// Files a new key under its book, chapter or kind.
    fn index_key(&mut self, k: KeyIdx, key: &RecordKey, book: Option<BookId>, shape: &Shape) {
        match key {
            RecordKey::Unknown(_) => {
                Arc::make_mut(&mut self.unknown_keys).insert(k);
            }
            RecordKey::CarriedFile(_) => {
                Arc::make_mut(&mut self.carried_keys).insert(k);
            }
            RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => {
                Arc::make_mut(&mut self.pin_keys).insert(k);
            }
            RecordKey::DeviceProfile(device) => {
                self.profile_keys.insert(*device, k);
            }
            RecordKey::StoryBase(_) | RecordKey::Frame(..) | RecordKey::StoryRef(_) => {
                Arc::make_mut(&mut self.story_keys).insert(k);
            }
            RecordKey::Setting(_) => {
                Arc::make_mut(&mut self.setting_keys).insert(k);
            }
            RecordKey::Check(CheckKey {
                target: CheckTarget::Frame { story, .. },
                ..
            }) => {
                Arc::make_mut(&mut self.story_attached)
                    .entry(*story)
                    .or_default()
                    .insert(k);
            }
            RecordKey::Comment(_) if let Some(story) = comment_story(shape) => {
                Arc::make_mut(&mut self.story_attached)
                    .entry(story)
                    .or_default()
                    .insert(k);
            }
            _ => {}
        }
        let Some(book) = book else {
            return;
        };
        let entry = self.book_or_new(book);
        entry.keys.push(k);
        let filed_under = match key {
            RecordKey::ChapterFront(_, c)
            | RecordKey::Verse(_, c, _)
            | RecordKey::Alignment(_, c, _) => Some(c.get()),
            RecordKey::Check(check) => super::attach::target_chapter(check),
            RecordKey::Comment(_) => comment_chapter(shape),
            _ => None,
        };
        if let Some(c) = filed_under {
            entry.filed.insert(c);
        }
        match key {
            RecordKey::BookPresence(_) => entry.presence = Some(k),
            RecordKey::BookHeader(_) => entry.header = Some(k),
            RecordKey::BookLayout(_) => entry.layout = Some(k),
            RecordKey::ChapterFront(_, c) => {
                entry.chapters.insert(c.get());
                self.chapters
                    .entry_or((book, c.get()), ChapterEntry::default)
                    .front = Some(k);
            }
            RecordKey::Verse(_, c, v) => {
                entry.chapters.insert(c.get());
                self.chapters
                    .entry_or((book, c.get()), ChapterEntry::default)
                    .verses
                    .insert(v.get(), k);
            }
            RecordKey::Alignment(_, c, v) => {
                self.chapters
                    .entry_or((book, c.get()), ChapterEntry::default)
                    .alignments
                    .insert(v.get(), k);
            }
            RecordKey::Check(check) => match super::attach::target_chapter(check) {
                Some(c) => self
                    .chapters
                    .entry_or((book, c), ChapterEntry::default)
                    .checks
                    .push(k),
                None => entry.other_checks.push(k),
            },
            RecordKey::Comment(_) => match comment_chapter(shape) {
                Some(c) => self
                    .chapters
                    .entry_or((book, c), ChapterEntry::default)
                    .comments
                    .push(k),
                None => entry.other_comments.push(k),
            },
            _ => {}
        }
    }

    fn intern_batch(&mut self, (id, n): (BatchId, u32)) -> BatchIdx {
        let hash = batch_hash(&id);
        let found = self.batch_ids.find(hash, |b| {
            let entry = &self.batches[b as usize];
            entry.id == id && entry.n == n
        });
        if let Some(found) = found {
            return BatchIdx(found);
        }
        let b = BatchIdx(self.batches.len() as u32);
        self.batch_ids.insert(hash, b.0);
        self.batches.push(BatchEntry {
            id,
            n,
            first: ChangeIdx::NONE,
            many: false,
            complete: false,
            held: false,
        });
        b
    }

    /// The valid changes of batch `b`, folded or held, in the order the fold met them.
    pub(crate) fn members(&self, b: BatchIdx) -> &[ChangeIdx] {
        let entry = self.batch(b);
        if entry.many {
            self.members.get(&b).map_or(&[], Vec::as_slice)
        } else if entry.first == ChangeIdx::NONE {
            &[]
        } else {
            std::slice::from_ref(&entry.first)
        }
    }

    fn add_member(&mut self, b: BatchIdx, c: ChangeIdx) {
        let entry = &mut self.batches[b.get()];
        if entry.many {
            self.members.entry_or(b, Vec::new).push(c);
        } else if entry.first == ChangeIdx::NONE {
            entry.first = c;
        } else {
            entry.many = true;
            let first = entry.first;
            self.members.insert(b, vec![first, c]);
        }
    }

    /// The quarantined lines of batch `b`.
    fn quarantined_of(&self, b: BatchIdx) -> u32 {
        if self.batch(b).held {
            self.held.get(&b).map_or(0, |h| h.quarantined)
        } else {
            0
        }
    }

    /// F2: a batch is complete when every one of its `n` changes is valid and here.
    fn is_complete(&self, b: BatchIdx) -> bool {
        self.quarantined_of(b) == 0 && self.members(b).len() >= self.batch(b).n as usize
    }

    /// Book `book`'s entry, to change.
    pub(crate) fn book_mut(&mut self, book: BookId) -> Option<&mut BookEntry> {
        Arc::make_mut(&mut self.books)
            .get_mut(&book)
            .map(Arc::make_mut)
    }

    /// Book `book`'s entry, to change; a new one if the book has none.
    pub(crate) fn book_or_new(&mut self, book: BookId) -> &mut BookEntry {
        Arc::make_mut(
            Arc::make_mut(&mut self.books)
                .entry(book)
                .or_insert_with(|| Arc::new(BookEntry::new())),
        )
    }

    // The index types are made only by `insert`, `intern_key` and `intern_batch`, which
    // push onto these append-only tables, so indexing never misses.

    pub(crate) fn meta(&self, c: ChangeIdx) -> &Meta {
        &self.metas[c.get()]
    }

    pub(crate) fn entry(&self, k: KeyIdx) -> &KeyEntry {
        &self.keys[k.get()]
    }

    pub(crate) fn entry_mut(&mut self, k: KeyIdx) -> &mut KeyEntry {
        &mut self.keys[k.get()]
    }

    pub(crate) fn batch(&self, b: BatchIdx) -> &BatchEntry {
        &self.batches[b.get()]
    }

    fn intern_pack(&mut self, pack: PackId) -> PackSlot {
        if let Some(slot) = self.pack_slots.get(&pack) {
            return *slot;
        }
        let slot = PackSlot(self.packs.len() as u32);
        Arc::make_mut(&mut self.packs).push(pack);
        self.pack_slots.insert(pack, slot);
        slot
    }

    /// Records that `pack` also holds change `c`.
    fn add_pack(&mut self, c: ChangeIdx, pack: PackId) {
        let slot = self.intern_pack(pack);
        let first = self.meta(c).pack;
        if first == slot {
            return;
        }
        if first == PackSlot::NONE {
            self.metas[c.get()].pack = slot;
            return;
        }
        let more = self.more_packs.entry_or(c, SmallVec::new);
        if !more.contains(&slot) {
            more.push(slot);
        }
    }

    /// The packs that hold a change, in the order the fold met them.
    pub(crate) fn packs_of(&self, c: ChangeIdx) -> Vec<PackId> {
        let mut out = Vec::new();
        let first = self.meta(c).pack;
        if first == PackSlot::NONE {
            return out;
        }
        let slots =
            std::iter::once(first).chain(self.more_packs.get(&c).into_iter().flatten().copied());
        for slot in slots {
            if let Some(pack) = self.packs.get(slot.0 as usize) {
                out.push(*pack);
            }
        }
        out
    }

    /// F1 for a fold from scratch: the tables [`FoldState::insert`] builds one change at a
    /// time, built in passes that each fill one table. A pass reads its input in order and
    /// does one lookup per change, so the lookups of neighbouring changes overlap; a
    /// whole-Bible index is several times faster this way than change by change.
    /// Takes every input out of `inputs` and leaves it empty with its room, so a fold from
    /// scratch fills one buffer chunk after chunk.
    fn index_all(&mut self, buffer: &mut Vec<FoldInput>, dirty: &mut Dirty) {
        let base = self.metas.len();
        let mut inputs: Vec<FoldInput> = std::mem::take(buffer)
            .into_iter()
            .map(FoldInput::prepare)
            .collect();
        // Ids: each change once, numbered in input order.
        self.ids.reserve(inputs.len());
        let mut fresh: Vec<bool> = Vec::with_capacity(inputs.len());
        let mut copies: Vec<(ChangeIdx, PackId)> = Vec::new();
        // The ids numbered here, whose changes are not in the change table yet.
        let mut numbered: Vec<ChangeId> = Vec::with_capacity(inputs.len());
        for input in &inputs {
            let Line::Prepared { id, .. } = &input.line else {
                fresh.push(false);
                continue;
            };
            let hash = id_hash(id);
            let found = self.ids.find(hash, |c| {
                let c = c as usize;
                match c.checked_sub(base) {
                    Some(new) => numbered[new] == *id,
                    None => self.metas[c].id == *id,
                }
            });
            match found {
                Some(c) => {
                    if let Some(pack) = input.pack {
                        copies.push((ChangeIdx(c), pack));
                    }
                    fresh.push(false);
                }
                None => {
                    self.ids.insert(hash, (base + numbered.len()) as u32);
                    numbered.push(*id);
                    fresh.push(true);
                }
            }
        }
        // The change table, with the record keys and batches set aside for their passes.
        let mut keys: Vec<(u64, RecordKey, Option<BookId>)> = Vec::with_capacity(inputs.len());
        let mut batches: Vec<(BatchId, u32)> = Vec::with_capacity(inputs.len());
        for (input, new) in inputs.drain(..).zip(fresh) {
            let pack = input.pack;
            let (id, value, summary) = match input.line {
                Line::Verified(Verified::Quarantined(q)) => {
                    self.insert(
                        FoldInput {
                            line: Line::Verified(Verified::Quarantined(q)),
                            pack,
                        },
                        dirty,
                    );
                    continue;
                }
                Line::Prepared { id, value, summary } if new => (id, value, summary),
                // A copy of a change already numbered; its pack is noted above.
                Line::Prepared { .. } | Line::Verified(Verified::Change(_)) => continue,
            };
            keys.push((key_hash(&summary.key), summary.key, summary.book));
            batches.push(summary.batch);
            let pack = pack.map_or(PackSlot::NONE, |p| self.intern_pack(p));
            let device = self.intern_device(summary.device);
            let parents: SmallVec<[ParentRef; 2]> = summary
                .parents
                .iter()
                .map(|p| {
                    let found = self.ids.find(id_hash(p), |c| {
                        let c = c as usize;
                        match c.checked_sub(base) {
                            Some(new) => numbered[new] == *p,
                            None => self.metas[c].id == *p,
                        }
                    });
                    match found {
                        Some(c) => ParentRef::known(ChangeIdx(c)),
                        None => ParentRef::absent(self.absent.push(*p)),
                    }
                })
                .collect();
            self.push_meta(NewChange {
                id,
                key: KeyIdx(u32::MAX),
                batch: BatchIdx(u32::MAX),
                parents,
                device,
                seq: summary.seq,
                at: summary.at,
                cause: summary.cause,
                stub: summary.stub,
                shape: summary.shape,
                value,
                pack,
            });
        }
        *buffer = inputs;
        let added = self.metas.len() - base;
        self.count(Counter::ChangesIndexed, added as u64);
        self.key_ids.reserve(added / 2);
        for (i, (hash, key, book)) in keys.into_iter().enumerate() {
            let c = ChangeIdx((base + i) as u32);
            let k = self.intern_hashed_key(key, hash, book, &self.shape(c));
            self.metas[c.get()].key = k;
            self.keys[k.get()].changes.push(c);
            if self.keys[k.get()].changes.len() > 1 {
                self.file_comment(k, c, dirty);
            }
        }
        self.batch_ids.reserve(added);
        for (i, batch) in batches.into_iter().enumerate() {
            let c = ChangeIdx((base + i) as u32);
            let b = self.intern_batch(batch);
            self.metas[c.get()].batch = b;
            self.add_member(b, c);
        }
        self.seqs.reserve(added);
        for c in base..self.metas.len() {
            let c = ChangeIdx(c as u32);
            let (device, seq, id) = {
                let m = self.meta(c);
                (self.device_of(m), self.seq_of(c), m.id)
            };
            if device.kind() == DeviceKind::Local {
                self.note_seq(c, id, device, seq);
            }
        }
        for (c, pack) in copies {
            self.add_pack(c, pack);
        }
    }

    /// The change a Local device wrote with this seq, if the fold holds one (§6.3). Two
    /// changes sharing a seq are a `DuplicateDevice` anomaly; this names the first the fold
    /// met.
    pub fn change_at(&self, device: DeviceId, seq: DeviceSeq) -> Option<ChangeId> {
        let found = self.seqs.find(seq_hash(device, seq), |x| {
            let meta = &self.metas[x as usize];
            self.devices[meta.device.0 as usize] == device && self.seq_of(ChangeIdx(x)) == seq
        })?;
        Some(self.meta(ChangeIdx(found)).id)
    }

    /// F5: files change `c` under its Local device and seq, and lists the devices that two
    /// changes claim.
    fn note_seq(&mut self, c: ChangeIdx, id: ChangeId, device: DeviceId, seq: DeviceSeq) {
        let slot = (device, seq);
        let hash = seq_hash(device, seq);
        let found = self.seqs.find(hash, |x| {
            let meta = &self.metas[x as usize];
            self.devices[meta.device.0 as usize] == device && self.seq_of(ChangeIdx(x)) == seq
        });
        let first = match found {
            None => {
                self.seqs.insert(hash, c.0);
                return;
            }
            Some(first) if first == c.0 => return,
            Some(first) => ChangeIdx(first),
        };
        let all = self
            .duplicate_seqs
            .entry_or(slot, || SmallVec::from_slice(&[first]));
        let before = all.clone();
        all.push(c);
        let all = all.clone();
        let ids_of = |list: &[ChangeIdx]| -> Vec<ChangeId> {
            let mut changes: Vec<ChangeId> = list
                .iter()
                .map(|x| if *x == c { id } else { self.meta(*x).id })
                .collect();
            changes.sort();
            changes
        };
        let old = (before.len() >= 2).then(|| ids_of(&before));
        let new = ids_of(&all);
        self.set_duplicate(device, seq, old, new);
    }

    fn insert(&mut self, input: FoldInput, dirty: &mut Dirty) {
        let (id, value, summary) = match input.line {
            Line::Verified(Verified::Quarantined(q)) => {
                if self.quarantined.contains_key(&q.id) {
                    return;
                }
                let batch = q.batch.map(|b| (b.id, b.n()));
                self.list_anomaly(Anomaly::QuarantinedChange {
                    change: q.id,
                    rule: q.rule.clone(),
                });
                if let Some(batch) = batch {
                    let b = self.intern_batch(batch);
                    if let Some(entry) = self.batches.get_mut(b.get()) {
                        entry.held = true;
                    }
                    let held = self.held.entry_or(b, Held::default);
                    held.quarantined += 1;
                    if held.quarantined_device.is_none() {
                        held.quarantined_device = q.device;
                    }
                    if !dirty.all {
                        dirty.batches.insert(b);
                    }
                }
                let id = q.id;
                self.quarantined.insert(
                    id,
                    Quarantined {
                        input: Some(FoldInput::new(Verified::Quarantined(q), input.pack)),
                    },
                );
                return;
            }
            Line::Verified(Verified::Change(change)) => {
                let value = Stored::bytes(change.raw().clone());
                if self.change_idx(&change.id()).is_some() {
                    (change.id(), value, None)
                } else {
                    let summary = meta::summarize(change.body(), &mut Resources::default());
                    (change.id(), value, Some(summary))
                }
            }
            Line::Prepared { id, value, summary } => (id, value, Some(summary)),
        };
        {
            {
                if let Some(c) = self.change_idx(&id) {
                    if let Some(pack) = input.pack {
                        self.add_pack(c, pack);
                    }
                    return;
                }
                let Some(summary) = summary else {
                    return;
                };
                self.count(Counter::ChangesIndexed, 1);
                let k = self.intern_key(summary.key, summary.book, &summary.shape);
                let b = self.intern_batch(summary.batch);
                let c = ChangeIdx(self.metas.len() as u32);
                if summary.device.kind() == DeviceKind::Local {
                    self.note_seq(c, id, summary.device, summary.seq);
                }
                let pack = input.pack.map_or(PackSlot::NONE, |p| self.intern_pack(p));
                let device = self.intern_device(summary.device);
                let parents: SmallVec<[ParentRef; 2]> = summary
                    .parents
                    .iter()
                    .map(|p| match self.change_idx(p) {
                        Some(pc) => ParentRef::known(pc),
                        None => ParentRef::absent(self.absent.push(*p)),
                    })
                    .collect();
                self.push_meta(NewChange {
                    id,
                    key: k,
                    batch: b,
                    parents,
                    device,
                    seq: summary.seq,
                    at: summary.at,
                    cause: summary.cause,
                    stub: summary.stub,
                    shape: summary.shape,
                    value,
                    pack,
                });
                self.ids.insert(id_hash(&id), c.0);
                self.entry_mut(k).changes.push(c);
                if self.entry(k).changes.len() > 1 {
                    self.file_comment(k, c, dirty);
                }
                self.add_member(b, c);
                if dirty.all {
                    return;
                }
                dirty.batches.insert(b);
                // A known child waited for this change: its link can be read now.
                if let Some(children) = self.waiting.get(&id) {
                    for child in children.clone() {
                        dirty.keys.insert(self.meta(child).key);
                    }
                }
                if matches!(self.entry(k).key, RecordKey::Unknown(_)) {
                    dirty.unknown = true;
                }
            }
        }
    }

    /// Lists an anomaly; each listing or unlisting is counted, and no step reads the
    /// whole list.
    pub(crate) fn list_anomaly(&mut self, anomaly: Anomaly) {
        self.count(Counter::AnomaliesTouched, 1);
        self.anomalies.insert(anomaly);
    }

    pub(crate) fn unlist_anomaly(&mut self, anomaly: &Anomaly) {
        self.count(Counter::AnomaliesTouched, 1);
        self.anomalies.remove(anomaly);
    }

    /// Lists the changes that claim one Local device and seq, replacing the listing of
    /// the `old` ones by its exact value.
    fn set_duplicate(
        &mut self,
        device: DeviceId,
        seq: DeviceSeq,
        old: Option<Vec<ChangeId>>,
        changes: Vec<ChangeId>,
    ) {
        if let Some(old) = old {
            self.unlist_anomaly(&Anomaly::DuplicateDevice {
                device,
                seq,
                changes: old,
            });
        }
        self.list_anomaly(Anomaly::DuplicateDevice {
            device,
            seq,
            changes,
        });
    }

    /// Files comment key `k` under the book and chapter that its change `c` targets too.
    /// Two changes under one comment id (§8.1) may target different books: the key is a
    /// record of each, whichever arrived first, and never moves (I1).
    fn file_comment(&mut self, k: KeyIdx, c: ChangeIdx, dirty: &mut Dirty) {
        if !matches!(self.entry(k).key, RecordKey::Comment(_)) {
            return;
        }
        let shape = self.shape(c);
        let book = meta::book_of(&self.entry(k).key, &shape);
        let active = self.entry(k).active > 0;
        if let Some(book) = book {
            let entry = self.book_or_new(book);
            if !entry.keys.contains(&k) {
                entry.keys.push(k);
                if active {
                    entry.records += 1;
                }
                dirty.books.insert(book);
            }
            if let Some(chapter) = comment_chapter(&shape) {
                let comments = &mut self
                    .chapters
                    .entry_or((book, chapter), ChapterEntry::default)
                    .comments;
                if !comments.contains(&k) {
                    comments.push(k);
                }
            }
        }
    }

    /// The books a key is a record of (§7.6): its own, and for a comment every book a
    /// change under its id targets.
    /// Every decision on a frame and every comment on a frame or a frame check.
    pub(crate) fn story_attached(&self) -> impl Iterator<Item = KeyIdx> + '_ {
        self.story_attached.values().flatten().copied()
    }

    pub(crate) fn books_of(&self, k: KeyIdx) -> SmallVec<[BookId; 1]> {
        let entry = self.entry(k);
        let mut out: SmallVec<[BookId; 1]> = entry.book.into_iter().collect();
        if matches!(entry.key, RecordKey::Comment(_)) && entry.changes.len() > 1 {
            for c in entry.changes.iter() {
                if let Some(book) = meta::book_of(&entry.key, &self.shape(*c))
                    && !out.contains(&book)
                {
                    out.push(book);
                }
            }
        }
        out
    }

    // ---------------------------------------------------------------------------------
    // The stages
    // ---------------------------------------------------------------------------------

    fn run(&mut self, dirty: &mut Dirty) {
        self.run_counted(dirty, &mut |_| {});
    }

    /// The stages, calling `done(n)` after the n-th.
    fn run_counted(&mut self, dirty: &mut Dirty, done: &mut dyn FnMut(u32)) {
        self.stage_batches(dirty);
        done(1);
        if dirty.rebuild {
            return;
        }
        self.stage_keys(dirty);
        done(2);
        super::passage::stage_groups(self, dirty);
        done(3);
        super::attach::stage_local(self, dirty);
        done(4);
        self.stage_meta(dirty);
        done(5);
        super::books::stage_books(self, dirty);
        done(6);
        super::attach::stage_chapters(self, dirty);
        done(7);
        super::attach::stage_attached(self, dirty);
        done(8);
        super::attach::stage_paths(self, dirty);
        done(9);
        if dirty.unknown {
            self.stage_unknown();
        }
    }

    /// F2: batch completeness and releases, to a fixpoint.
    fn stage_batches(&mut self, dirty: &mut Dirty) {
        let mut queue: Vec<BatchIdx> = if dirty.all {
            self.fold_complete_batches(dirty)
        } else {
            dirty.batches.iter().copied().collect()
        };
        while let Some(b) = queue.pop() {
            let (id, complete, size) =
                (self.batch(b).id, self.is_complete(b), self.members(b).len());
            let released = if self.released.len() == 0 {
                None
            } else {
                self.released.get(&id).cloned()
            };
            let mut had_folded = false;
            let mut newly: SmallVec<[ChangeIdx; 2]> = SmallVec::new();
            for i in 0..size {
                let c = self.members(b)[i];
                let meta = self.meta(c);
                had_folded |= meta.active();
                let active = complete || released.as_ref().is_some_and(|r| r.contains(&meta.id));
                if active == meta.active() {
                    continue;
                }
                if !active {
                    // Folded changes only stop being folded when a quarantined line joins
                    // a complete batch; no stage undoes that incrementally.
                    dirty.rebuild = true;
                    return;
                }
                newly.push(c);
            }
            self.batches[b.get()].complete = complete;
            self.anomaly_for_batch(b);
            for &c in &newly {
                self.metas[c.get()].set_active(true);
            }
            if !newly.is_empty() {
                let passage = self.passage_books(b);
                if passage.as_slice() != self.passage_of(b) && !dirty.all {
                    // A release that grows the batch can make it a passage batch: every
                    // record it already wrote is judged again as part of its passage.
                    for i in 0..size {
                        let c = self.members(b)[i];
                        if self.meta(c).active() {
                            let k = self.meta(c).key;
                            dirty.simple.remove(&k);
                            dirty.keys.insert(k);
                        }
                    }
                }
                if had_folded && !passage.is_empty() && !dirty.all {
                    dirty.regroup.insert(b);
                }
                self.set_passage(b, passage);
            }
            for c in newly {
                self.activate(c, dirty, &mut queue);
            }
        }
        // Keys that became dirty for a second reason leave the simple path.
        if !dirty.simple.is_empty() {
            let simple: Vec<KeyIdx> = dirty.simple.keys().copied().collect();
            for k in simple {
                if dirty.keys.contains(&k) {
                    dirty.simple.remove(&k);
                }
            }
        }
    }

    /// F2 for a fold from scratch, in table order: folds every change of a complete batch,
    /// and returns the held batches, which releases may fold in part, for the general loop.
    fn fold_complete_batches(&mut self, dirty: &mut Dirty) -> Vec<BatchIdx> {
        let mut queue: Vec<BatchIdx> = Vec::new();
        for b in 0..self.batches.len() {
            let b = BatchIdx(b as u32);
            let complete = self.is_complete(b);
            self.batches[b.get()].complete = complete;
            if !complete {
                queue.push(b);
            }
            self.anomaly_for_batch(b);
        }
        for c in 0..self.metas.len() {
            let c = ChangeIdx(c as u32);
            if self.batch(self.meta(c).batch).complete && !self.meta(c).active() {
                self.metas[c.get()].set_active(true);
                self.activate(c, dirty, &mut queue);
            }
        }
        for b in 0..self.batches.len() {
            let b = BatchIdx(b as u32);
            if self.batch(b).complete && self.members(b).len() >= 2 {
                let passage = self.passage_books(b);
                self.set_passage(b, passage);
            }
        }
        queue.reverse();
        queue
    }

    /// What a change's becoming folded touches. A fold from scratch registers the links
    /// that wait for a parent after F3 instead (`stage_keys`), from the faults it finds.
    fn activate(&mut self, c: ChangeIdx, dirty: &mut Dirty, queue: &mut Vec<BatchIdx>) {
        let k = self.meta(c).key;
        let id = self.meta(c).id;
        if !dirty.all {
            let simple = self.simple_candidate(c);
            if simple && !dirty.keys.contains(&k) && !dirty.simple.contains_key(&k) {
                dirty.simple.insert(k, c);
            } else {
                dirty.simple.remove(&k);
                dirty.keys.insert(k);
            }
        }
        let entry = &mut self.keys[k.get()];
        entry.active += 1;
        if entry.active == 1 {
            for book in self.books_of(k) {
                if let Some(be) = self.book_mut(book) {
                    be.records += 1;
                }
                if !dirty.all {
                    dirty.books.insert(book);
                }
            }
        }
        if !dirty.all {
            if let Some(children) = self.waiting.remove(&id) {
                for child in children {
                    let ck = self.meta(child).key;
                    dirty.simple.remove(&ck);
                    dirty.keys.insert(ck);
                }
            }
            // Links to parents that are not folded yet wait for them.
            let parents: SmallVec<[ParentRef; 2]> = self.parents(c).collect();
            for p in parents {
                let known_active = self.parent_idx(p).is_some_and(|pi| self.meta(pi).active());
                if !known_active {
                    let id = self.parent_id(p);
                    self.waiting.entry_or(id, SmallVec::new).push(c);
                }
            }
        }
        if self.meta(c).form == Form::Release
            && let Shape::Release { changes } = self.shape(c)
            && let RecordKey::BatchRelease(target) = &self.entry(k).key
        {
            let target = *target;
            let set = Arc::make_mut(self.released.entry_or(target, || Arc::new(BTreeSet::new())));
            let before = set.len();
            set.extend(changes.iter().copied());
            if set.len() != before {
                let mut targets = self.batch_ids.find_all(batch_hash(&target), |b| {
                    self.batches[b as usize].id == target
                });
                targets.sort_unstable();
                queue.extend(targets.into_iter().map(BatchIdx));
            }
        }
    }

    /// A new folded change whose parents are all folded changes of its own key, and that no
    /// known change names as a parent: its key's heads update without a recompute.
    fn simple_candidate(&self, c: ChangeIdx) -> bool {
        let meta = self.meta(c);
        if self.waiting.contains_key(&meta.id) {
            return false;
        }
        self.parents(c).all(|p| {
            self.parent_idx(p)
                .is_some_and(|pi| self.meta(pi).active() && self.meta(pi).key == meta.key)
        })
    }

    /// Lists, or stops listing, a batch that is held (F2).
    fn anomaly_for_batch(&mut self, b: BatchIdx) {
        let entry = self.batch(b);
        if entry.complete && !entry.held {
            return;
        }
        let held = self.held.get(&b).cloned().unwrap_or_default();
        let anomaly = if entry.complete {
            None
        } else {
            self.members(b)
                .iter()
                .map(|c| {
                    let m = self.meta(*c);
                    (self.device_of(m), m.seq, m.id)
                })
                .min()
                .map(|(d, _, _)| d)
                .or(held.quarantined_device)
                .map(|device| Anomaly::IncompleteBatch {
                    batch: entry.id,
                    device,
                    have: self.members(b).len() as u32,
                    of: entry.n,
                })
        };
        if anomaly == held.anomaly {
            return;
        }
        if let Some(old) = &held.anomaly {
            self.unlist_anomaly(old);
        }
        if let Some(new) = &anomaly {
            self.list_anomaly(new.clone());
        }
        self.batches[b.get()].held = true;
        self.held.entry_or(b, Held::default).anomaly = anomaly;
    }

    /// The books for which batch `b` is a passage batch (§7.3): it is not a seed batch and
    /// holds folded changes with parents to two or more verse records of the book.
    fn passage_books(&self, b: BatchIdx) -> SmallVec<[BookId; 1]> {
        let members = self.members(b);
        if members.len() < 2
            || members
                .iter()
                .any(|c| self.meta(*c).cause == CauseClass::Seed)
        {
            return SmallVec::new();
        }
        let mut books: SmallVec<[BookId; 8]> = SmallVec::new();
        for c in members {
            let meta = self.meta(*c);
            if !meta.active() || self.has_no_parent(*c) {
                continue;
            }
            if let RecordKey::Verse(book, ..) = self.entry(meta.key).key {
                books.push(book);
            }
        }
        books.sort();
        let mut out: SmallVec<[BookId; 1]> = SmallVec::new();
        for (i, book) in books.iter().enumerate() {
            if i > 0 && books[i - 1] == *book && out.last() != Some(book) {
                out.push(*book);
            }
        }
        out
    }

    /// F3, F4, F6 for every dirty key.
    fn stage_keys(&mut self, dirty: &mut Dirty) {
        let simple: Vec<(KeyIdx, ChangeIdx)> = dirty.simple.iter().map(|(k, c)| (*k, *c)).collect();
        for (k, c) in simple {
            self.count(Counter::KeysUpdated, 1);
            let parents: SmallVec<[ChangeIdx; 2]> =
                self.parents(c).filter_map(|p| self.parent_idx(p)).collect();
            let mut heads: SmallVec<[ChangeIdx; 1]> = self
                .entry(k)
                .heads
                .iter()
                .copied()
                .filter(|h| !parents.contains(h))
                .collect();
            heads.push(c);
            self.sort_by_id(&mut heads);
            self.entry_mut(k).heads = heads.into_iter().collect();
            self.after_heads(k, dirty);
        }
        let keys: Vec<KeyIdx> = if dirty.all {
            (0..self.keys.len() as u32).map(KeyIdx).collect()
        } else {
            dirty.keys.iter().copied().collect()
        };
        for k in keys {
            self.recompute_links(k);
            if dirty.all {
                // A fold from scratch registers here the links `activate` leaves out: a
                // fault of kind Missing is a link that waits for its parent.
                for (c, p, fault) in self.faults_of(k).to_vec() {
                    if fault == Fault::Missing {
                        self.waiting.entry_or(p, SmallVec::new).push(c);
                    }
                }
            }
            self.after_heads(k, dirty);
        }
        if dirty.all {
            dirty.pins = true;
            dirty.paths = true;
            dirty.carried = true;
            dirty.unknown = !self.unknown_keys.is_empty();
        }
    }

    pub(crate) fn sort_by_id(&self, list: &mut [ChangeIdx]) {
        list.sort_by_key(|c| self.meta(*c).id);
    }

    /// F3 and F4 for one key from its changes alone.
    fn recompute_links(&mut self, k: KeyIdx) {
        self.count(Counter::KeysRecomputed, 1);
        let old_faults = self.faults.remove(&k).unwrap_or_default();
        let mut has_child: SmallVec<[ChangeIdx; 4]> = SmallVec::new();
        let mut faults: SmallVec<[Link; 1]> = SmallVec::new();
        let active: SmallVec<[ChangeIdx; 4]> = self
            .entry(k)
            .changes
            .iter()
            .copied()
            .filter(|c| self.meta(*c).active())
            .collect();
        self.count(Counter::ChangesVisited, active.len() as u64);
        for &c in &active {
            for p in self.parents(c) {
                match self.parent_idx(p) {
                    Some(pi) if self.meta(pi).active() => {
                        if self.meta(pi).key == k {
                            has_child.push(pi);
                        } else {
                            faults.push((c, self.parent_id(p), Fault::OtherRecord));
                        }
                    }
                    _ => faults.push((c, self.parent_id(p), Fault::Missing)),
                }
            }
        }
        let mut heads: SmallVec<[ChangeIdx; 1]> = active
            .iter()
            .copied()
            .filter(|c| !has_child.contains(c))
            .collect();
        self.sort_by_id(&mut heads);
        // Replace this key's link anomalies.
        for (c, p, fault) in old_faults.iter() {
            let anomaly = link_anomaly(self.meta(*c).id, *p, *fault);
            self.unlist_anomaly(&anomaly);
        }
        for (c, p, fault) in faults.iter() {
            let anomaly = link_anomaly(self.meta(*c).id, *p, *fault);
            self.list_anomaly(anomaly);
        }
        self.entry_mut(k).heads = heads.into_iter().collect();
        if !faults.is_empty() {
            self.faults.insert(k, faults);
        }
    }

    /// The change a parent names, if the fold holds it (a parent absent when its child
    /// arrived may have arrived since).
    pub(crate) fn parent_idx(&self, p: ParentRef) -> Option<ChangeIdx> {
        match p.get() {
            Ok(c) => Some(c),
            Err(at) => self.change_idx(&self.absent[at]),
        }
    }

    /// The id of a parent.
    pub(crate) fn parent_id(&self, p: ParentRef) -> ChangeId {
        match p.get() {
            Ok(c) => self.meta(c).id,
            Err(at) => self.absent[at],
        }
    }

    /// The ids of a change's parents, in the order it names them.
    pub(crate) fn parent_ids(&self, c: ChangeIdx) -> SmallVec<[ChangeId; 1]> {
        self.parents(c).map(|p| self.parent_id(p)).collect()
    }

    fn intern_device(&mut self, device: DeviceId) -> DeviceIdx {
        if let Some(found) = self.device_index.get(&device) {
            return *found;
        }
        let idx = DeviceIdx(self.devices.len() as u32);
        Arc::make_mut(&mut self.devices).push(device);
        self.device_index.insert(device, idx);
        idx
    }

    /// The device that wrote a change.
    pub(crate) fn device_of(&self, meta: &Meta) -> DeviceId {
        self.devices[meta.device.0 as usize]
    }

    /// The books for which batch `b` is a passage batch (§7.3).
    pub(crate) fn passage_of(&self, b: BatchIdx) -> &[BookId] {
        self.passages.get(&b).map_or(&[], |p| p.as_slice())
    }

    fn set_passage(&mut self, b: BatchIdx, books: SmallVec<[BookId; 1]>) {
        if books.is_empty() {
            self.passages.remove(&b);
        } else {
            self.passages.insert(b, books);
        }
    }

    /// The links F3 ignores on key `k`.
    pub(crate) fn faults_of(&self, k: KeyIdx) -> &[Link] {
        self.faults.get(&k).map_or(&[], |f| f.as_slice())
    }

    /// The passage batches key `k` diverges from.
    pub(crate) fn div_of(&self, k: KeyIdx) -> &[BatchIdx] {
        self.divs.get(&k).map_or(&[], |d| d.as_slice())
    }

    /// F6 for a key whose heads may have changed.
    fn after_heads(&mut self, k: KeyIdx, dirty: &mut Dirty) {
        let (local, div) = super::merge::resolve_local(self, k);
        if matches!(&local, Local::Conflict(c) if c.unreadable) {
            self.unreadable.insert(k, ());
        } else {
            self.unreadable.remove(&k);
        }
        self.entry_mut(k).local = local;
        if div.is_empty() {
            self.divs.remove(&k);
        } else {
            self.divs.insert(k, div);
        }
        if dirty.all {
            return;
        }
        dirty.changed.insert(k);
        let entry = self.entry(k);
        match &entry.key {
            // `resources.json` holds the extra keys too, so its projection is redone.
            RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => dirty.pins = true,
            RecordKey::CarriedFile(_) => dirty.carried = true,
            _ => {}
        }
        dirty.books.extend(self.books_of(k));
        dirty.attached.insert(k);
    }

    /// The anomaly of unknown kinds (§12): one entry for every folded change of a kind from
    /// a newer minor.
    fn stage_unknown(&mut self) {
        let mut kinds: BTreeSet<Box<str>> = BTreeSet::new();
        let mut changes = 0u32;
        for k in self.unknown_keys.iter() {
            let entry = self.entry(*k);
            if entry.active > 0 {
                kinds.insert(entry.key.kind_name().into());
                changes += entry.active;
            }
        }
        let new = (changes > 0).then(|| Anomaly::UnknownKinds {
            kinds: kinds.into_iter().collect(),
            changes,
        });
        if new == self.unknown_anomaly {
            return;
        }
        if let Some(old) = self.unknown_anomaly.take() {
            self.unlist_anomaly(&old);
        }
        if let Some(new) = new {
            self.list_anomaly(new.clone());
            self.unknown_anomaly = Some(new);
        }
    }

    // ---------------------------------------------------------------------------------
    // Conflicts
    // ---------------------------------------------------------------------------------

    /// Replaces the conflicts `owner` lists.
    pub(crate) fn set_conflicts(
        &mut self,
        owner: Owner,
        conflicts: Vec<Conflict>,
        dirty: &mut Dirty,
    ) {
        let mut old_ids: BTreeSet<ConflictId> = BTreeSet::new();
        if let Some(old) = self.owned.remove(&owner) {
            for sort in old {
                if let Some(c) = self.conflicts.remove(&sort) {
                    self.conflict_ids.remove(&c.id);
                    self.count_conflict(&c, false);
                    old_ids.insert(c.id);
                }
            }
        }
        let new_ids: BTreeSet<ConflictId> = conflicts.iter().map(|c| c.id).collect();
        for id in old_ids.difference(&new_ids) {
            dirty.removed_conflicts.insert(*id);
        }
        for id in new_ids.difference(&old_ids) {
            dirty.added_conflicts.insert(*id);
        }
        if conflicts.is_empty() {
            return;
        }
        let mut sorts = SmallVec::new();
        for conflict in conflicts {
            self.count(Counter::ConflictsBuilt, 1);
            self.count_conflict(&conflict, true);
            let sort = conflict_sort(&conflict);
            self.conflict_ids.insert(conflict.id, sort.clone());
            sorts.push(sort.clone());
            self.conflicts.insert(sort, Arc::new(conflict));
        }
        self.owned.insert(owner, sorts);
    }

    fn count_conflict(&mut self, conflict: &Conflict, add: bool) {
        let slot = (conflict.key.book(), conflict.kind.order());
        let n = self.counts.entry(slot).or_insert(0);
        if add {
            *n += 1;
        } else {
            *n = n.saturating_sub(1);
            if *n == 0 {
                self.counts.remove(&slot);
            }
        }
    }

    fn delta(&self, dirty: &Dirty) -> super::Delta {
        let mut keys: Vec<RecordKey> = dirty
            .changed
            .iter()
            .map(|k| self.entry(*k).key.clone())
            .collect();
        keys.sort();
        let new_conflicts: Vec<ConflictId> = dirty
            .added_conflicts
            .iter()
            .filter(|id| {
                !dirty.removed_conflicts.contains(id) && self.conflict_ids.contains_key(id)
            })
            .copied()
            .collect();
        super::Delta {
            keys,
            books: dirty.books.iter().copied().collect(),
            chapters: dirty.chapters.iter().copied().collect(),
            new_conflicts,
            pins: dirty.pins,
            paths: dirty.paths,
            carried: dirty.carried,
            conflicts_changed: !dirty.added_conflicts.is_empty()
                || !dirty.removed_conflicts.is_empty(),
            everything: false,
        }
    }

    // ---------------------------------------------------------------------------------
    // Reads
    // ---------------------------------------------------------------------------------

    /// Sha256 of the sorted list of every head id (§2.6): names this exact state.
    pub fn frontier(&self) -> Frontier {
        let mut heads: Vec<ChangeId> = self
            .keys
            .iter()
            .flat_map(|e| e.heads.iter().map(|c| self.meta(*c).id))
            .collect();
        heads.sort();
        Frontier::of(&heads)
    }

    /// How much work the fold has done: what the operation-count tests compare
    /// (`docs/PERFORMANCE.md` rule 2).
    pub fn counters(&self) -> Counters {
        self.counters.read()
    }

    /// Gives this state counters of its own, starting at zero, so the work of a snapshot
    /// is counted apart from the state it was taken from.
    pub fn own_counters(&mut self) {
        self.counters = Arc::new(CounterCells::default());
    }

    pub(crate) fn count(&self, counter: Counter, n: u64) {
        self.counters.add(counter, n);
    }

    /// Reads values on demand from `source` from now on (§14).
    pub fn set_source(&mut self, source: Arc<dyn ValueSource>) {
        self.source = Some(source);
    }

    /// The records whose versions could not all be read on this computer (§2.6): each is
    /// listed as a conflict marked unreadable, or holds a conflict candidate that is.
    /// `tc-store` reports them, and derived files are not written while there are any.
    pub fn unreadable_records(&self) -> Vec<RecordKey> {
        let mut keys: BTreeSet<RecordKey> = self
            .unreadable
            .iter()
            .map(|(k, _)| self.entry(*k).key.clone())
            .collect();
        for conflict in self.conflicts() {
            if !conflict.unreadable.is_empty() {
                keys.insert(conflict.key.clone());
                keys.extend(conflict.also_keys.iter().cloned());
            }
        }
        keys.into_iter().collect()
    }

    /// Judges again every record whose versions could not all be read (§2.6), for when
    /// the source can read them again (a file that was locked, a cloud placeholder that
    /// arrived). Costs what those records touch.
    pub fn retry_unreadable(&mut self) -> super::Delta {
        let mut dirty = Dirty::default();
        for key in self.unreadable_records() {
            if let Some(k) = self.key_idx(&key) {
                dirty.keys.insert(k);
            }
        }
        if dirty.keys.is_empty() {
            return super::Delta::default();
        }
        self.run(&mut dirty);
        if dirty.rebuild {
            return self.rebuild();
        }
        self.delta(&dirty)
    }

    /// Whether change `c`'s bytes can be read: kept in memory, or read from the source.
    pub(crate) fn readable(&self, c: ChangeIdx) -> bool {
        if self.meta(c).has(Meta::IN_MEMORY) {
            self.kept.contains_key(&c)
        } else {
            self.bytes(c).is_some()
        }
    }

    /// The canonical bytes of change `c`: kept in memory, or read from the source and
    /// checked against the fingerprint of the bytes that were verified. `None` only when
    /// the source cannot read them, or reads other bytes.
    pub(crate) fn bytes(&self, c: ChangeIdx) -> Option<Arc<[u8]>> {
        let meta = self.meta(c);
        if meta.has(Meta::IN_MEMORY) {
            return self.kept.get(&c).cloned();
        }
        self.count(Counter::ValuesRead, 1);
        let value = meta.value;
        let at = ValueAt {
            offset: u64::from(value.offset),
            file: value.file,
            len: value.len,
        };
        let bytes = self
            .source
            .as_ref()
            .and_then(|source| source.read(at))
            .filter(|bytes| super::value::fingerprint(bytes) == value.fingerprint());
        if bytes.is_none() {
            self.unreadable_reads
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        bytes.map(Arc::from)
    }

    /// How many reads of a value the source could not give so far (§2.6). A projection that
    /// moved it showed a version it could not read as absent, and is not written.
    pub fn values_unreadable(&self) -> u64 {
        self.unreadable_reads
            .load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// The hash [`FoldState::ids`] files a change under: its id's first eight bytes, which are
/// already a uniform hash.
pub(crate) fn id_hash(id: &ChangeId) -> u64 {
    let mut word = [0u8; 8];
    word.copy_from_slice(&id.as_bytes()[..8]);
    u64::from_le_bytes(word)
}

/// The hash [`FoldState::batch_ids`] files a batch under.
pub(crate) fn batch_hash(id: &BatchId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = super::cow::FxHasher::default();
    id.hash(&mut hasher);
    hasher.finish()
}

/// The hash [`FoldState::seqs`] files a Local device's seq under.
pub(crate) fn seq_hash(device: DeviceId, seq: DeviceSeq) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = super::cow::FxHasher::default();
    device.hash(&mut hasher);
    seq.hash(&mut hasher);
    hasher.finish()
}

/// The hash [`FoldState::key_ids`] files a record key under.
pub(crate) fn key_hash(key: &RecordKey) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = super::cow::FxHasher::default();
    key.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn link_anomaly(change: ChangeId, parent: ChangeId, fault: Fault) -> Anomaly {
    match fault {
        Fault::Missing => Anomaly::MissingParent { change, parent },
        Fault::OtherRecord => Anomaly::ParentOnOtherRecord { change, parent },
    }
}

pub(crate) fn conflict_sort(conflict: &Conflict) -> ConflictSort {
    let book = conflict.key.book().map_or(u8::MAX, BookId::ordinal);
    (
        book,
        conflict.key.clone(),
        conflict.kind.order(),
        conflict.id,
    )
}

/// The story a comment on a frame or on a frame check targets.
fn comment_story(shape: &Shape) -> Option<StoryNo> {
    match shape {
        Shape::Comment { target } => match &**target {
            crate::record::CommentTarget::Frame { story, .. } => Some(*story),
            crate::record::CommentTarget::Check(check) => match check.target {
                CheckTarget::Frame { story, .. } => Some(story),
                CheckTarget::Verse { .. } => None,
            },
            crate::record::CommentTarget::Verse { .. } => None,
        },
        _ => None,
    }
}

/// The chapter a comment is filed under: its verse's, or its check's target chapter.
fn comment_chapter(shape: &Shape) -> Option<u16> {
    match shape {
        Shape::Comment { target } => match &**target {
            crate::record::CommentTarget::Verse { chapter, .. } => Some(chapter.get()),
            crate::record::CommentTarget::Check(check) => super::attach::target_chapter(check),
            crate::record::CommentTarget::Frame { .. } => None,
        },
        _ => None,
    }
}

#[cfg(all(test, target_pointer_width = "64"))]
mod tests {
    use std::mem::size_of;

    /// A whole Bible holds 620,000 changes, 550,000 batches and 280,000 records: every byte
    /// of these entries is half a megabyte of the app's steady memory (docs/PERFORMANCE.md,
    /// 150 MiB with the whole Bible open). A field that grows one of them fails here, so the
    /// growth is weighed in review.
    #[test]
    fn the_tables_entries_keep_their_sizes() {
        assert_eq!(size_of::<super::Meta>(), 96);
        assert_eq!(size_of::<super::Form>(), 8);
        assert_eq!(size_of::<super::BatchEntry>(), 28);
        assert_eq!(size_of::<super::KeyEntry>(), 144);
        assert_eq!(size_of::<super::ChangeList>(), 16);
        assert_eq!(size_of::<crate::keys::RecordKey>(), 64);
    }
}
