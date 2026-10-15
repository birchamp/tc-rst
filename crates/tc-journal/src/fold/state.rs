//! The fold's state and its one update path (§7.2, §7.8). A full fold is an empty state
//! with every change added at once; an incremental step adds a few. Both run the same
//! stages over what the new changes made dirty, and each stage recomputes a dirty unit (a
//! key, a passage group, a book, a chapter) from the current set alone, never from a
//! delta. So the incremental state equals a fold from scratch (`incremental_equals_full`).

use super::cow::{ChunkVec, CowMap};
use super::meta::{
    self, BatchIdx, CauseClass, ChangeIdx, KeyIdx, Meta, Quarantined, Resources, Shape,
};
use super::{Anomaly, Conflict, ConflictId, Counter, CounterCells, Counters, Frontier};
use crate::change::Verified;
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceKind, DeviceSeq, PackId};
use crate::keys::RecordKey;
use crate::record::Record;
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tc_core::BookId;

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
        raw: Arc<[u8]>,
        summary: Box<meta::Summary>,
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
        match self.line {
            Line::Verified(Verified::Change(change)) => {
                let id = change.id();
                let raw = change.raw().clone();
                let summary = meta::summarize(change.body(), &mut Resources::default());
                Self {
                    line: Line::Prepared {
                        id,
                        raw,
                        summary: Box::new(summary),
                    },
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
    Merged(Arc<Record>, MergeHow),
    /// A conflict of this record alone; `provisional` is the head projection shows (F7).
    Conflict {
        id: ConflictId,
        provisional: ChangeIdx,
    },
    /// Diverges from a passage batch: its group decides (§7.3 Passages).
    Passage,
}

/// What projection shows for a record.
#[derive(Debug, Clone)]
pub(crate) enum Projected {
    Nothing,
    Change(ChangeIdx),
    Merged(Arc<Record>),
}

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
    pub changes: SmallVec<[ChangeIdx; 2]>,
    /// Folded changes with no folded child on this key, sorted by id.
    pub heads: SmallVec<[ChangeIdx; 1]>,
    pub faults: SmallVec<[(ChangeIdx, ChangeId, Fault); 0]>,
    pub local: Local,
    /// The passage batches this record diverges from (§7.3).
    pub div: SmallVec<[BatchIdx; 1]>,
    /// The passage group this record is in, named by its first record.
    pub group: Option<KeyIdx>,
    pub projected: Projected,
    /// Folded changes of this key.
    pub active: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct BatchEntry {
    pub id: BatchId,
    pub n: u32,
    /// Its valid changes, folded or held.
    pub members: Vec<ChangeIdx>,
    pub quarantined: u32,
    /// The device of a quarantined member, when no valid member names one.
    pub quarantined_device: Option<DeviceId>,
    pub complete: bool,
    /// The books for which it is a passage batch (§7.3).
    pub passage: SmallVec<[BookId; 1]>,
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
    fn new() -> Self {
        Self {
            presence: None,
            header: None,
            layout: None,
            chapters: BTreeSet::new(),
            other_checks: Vec::new(),
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
    pub pins: bool,
    pub paths: bool,
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
    pub(crate) ids: CowMap<ChangeId, ChangeIdx>,
    pub(crate) keys: ChunkVec<KeyEntry>,
    pub(crate) key_ids: CowMap<RecordKey, KeyIdx>,
    pub(crate) batches: ChunkVec<BatchEntry>,
    pub(crate) batch_ids: CowMap<(BatchId, u32), BatchIdx>,
    pub(crate) batches_by_id: CowMap<BatchId, SmallVec<[BatchIdx; 1]>>,
    /// The changes some folded `BatchRelease` lists, by batch id.
    pub(crate) released: CowMap<BatchId, Arc<BTreeSet<ChangeId>>>,
    pub(crate) quarantined: CowMap<ChangeId, Quarantined>,
    /// Children that name a parent that is unknown or held, by that parent.
    pub(crate) waiting: CowMap<ChangeId, SmallVec<[ChangeIdx; 1]>>,
    /// Changes by Local device and seq (F5).
    pub(crate) seqs: CowMap<(DeviceId, DeviceSeq), SmallVec<[ChangeId; 1]>>,
    pub(crate) books: Arc<BTreeMap<BookId, BookEntry>>,
    pub(crate) chapters: CowMap<(BookId, u16), ChapterEntry>,
    pub(crate) groups: CowMap<KeyIdx, Group>,
    pub(crate) unknown_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) carried_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) pin_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) profile_keys: CowMap<DeviceId, KeyIdx>,
    pub(crate) story_keys: Arc<BTreeSet<KeyIdx>>,
    pub(crate) conflicts: im::OrdMap<ConflictSort, Arc<Conflict>>,
    pub(crate) conflict_ids: im::HashMap<ConflictId, ConflictSort>,
    pub(crate) owned: CowMap<Owner, SmallVec<[ConflictSort; 1]>>,
    pub(crate) anomalies: im::OrdSet<Anomaly>,
    pub(crate) detached: im::OrdSet<RecordKey>,
    /// Conflict counts by book and kind order.
    pub(crate) counts: BTreeMap<(Option<BookId>, u8), u32>,
    /// Anchor (book, chapter, verse) → the verses whose projected text holds it.
    pub(crate) anchors: CowMap<(BookId, u16, u16), BTreeSet<(u16, u16)>>,
    pub(crate) pins: Arc<super::attach::PinsView>,
    /// What each project detail, presence and pin group contributes to `metadata.json`.
    pub(crate) meta_choices: Arc<BTreeMap<KeyIdx, super::metadata::MetaChoice>>,
    /// Carried files placed at a `.case-` path (§3.3).
    pub(crate) case_moves: Arc<BTreeMap<crate::ids::RepoPath, String>>,
    pub(crate) resources: Resources,
    pub(crate) counters: Arc<CounterCells>,
}

impl FoldState {
    /// An empty fold.
    pub fn new(kind: ProjectKind) -> Self {
        Self {
            kind,
            metas: ChunkVec::default(),
            ids: CowMap::default(),
            keys: ChunkVec::default(),
            key_ids: CowMap::default(),
            batches: ChunkVec::default(),
            batch_ids: CowMap::default(),
            batches_by_id: CowMap::default(),
            released: CowMap::default(),
            quarantined: CowMap::default(),
            waiting: CowMap::default(),
            seqs: CowMap::default(),
            books: Arc::new(BTreeMap::new()),
            chapters: CowMap::default(),
            groups: CowMap::default(),
            unknown_keys: Arc::new(BTreeSet::new()),
            carried_keys: Arc::new(BTreeSet::new()),
            pin_keys: Arc::new(BTreeSet::new()),
            profile_keys: CowMap::default(),
            story_keys: Arc::new(BTreeSet::new()),
            conflicts: im::OrdMap::new(),
            conflict_ids: im::HashMap::new(),
            owned: CowMap::default(),
            anomalies: im::OrdSet::new(),
            detached: im::OrdSet::new(),
            counts: BTreeMap::new(),
            anchors: CowMap::default(),
            pins: Arc::new(super::attach::PinsView::default()),
            meta_choices: Arc::new(BTreeMap::new()),
            case_moves: Arc::new(BTreeMap::new()),
            resources: Resources::default(),
            counters: Arc::new(CounterCells::default()),
        }
    }

    /// Folds a set of changes from scratch (§7.2).
    pub fn fold(kind: ProjectKind, changes: impl IntoIterator<Item = FoldInput>) -> Self {
        let mut state = Self::new(kind);
        let mut dirty = Dirty {
            all: true,
            ..Dirty::default()
        };
        for input in changes {
            state.insert(input, &mut dirty);
        }
        state.run(&mut dirty);
        if dirty.rebuild {
            state.rebuild();
        }
        state
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
        for meta in self.metas.iter() {
            if let crate::change::Verified::Change(change) =
                crate::change::parse_change(meta.id, meta.raw.clone())
            {
                let packs = meta.packs.clone();
                if packs.is_empty() {
                    inputs.push(FoldInput::new(Verified::Change(change), None));
                } else {
                    for pack in packs {
                        inputs.push(FoldInput::new(Verified::Change(change.clone()), Some(pack)));
                    }
                }
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
        let mut dirty = Dirty {
            all: true,
            ..Dirty::default()
        };
        for input in inputs.into_iter().chain(quarantined) {
            fresh.insert(input, &mut dirty);
        }
        fresh.run(&mut dirty);
        *self = fresh;
        super::Delta::everything()
    }

    // ---------------------------------------------------------------------------------
    // F1: index
    // ---------------------------------------------------------------------------------

    fn intern_key(&mut self, key: RecordKey, book: Option<BookId>, shape: &Shape) -> KeyIdx {
        if let Some(k) = self.key_ids.get(&key) {
            return *k;
        }
        let k = KeyIdx(self.keys.len() as u32);
        self.index_key(k, &key, book, shape);
        self.key_ids.insert(key.clone(), k);
        self.keys.push(KeyEntry {
            key,
            book,
            changes: SmallVec::new(),
            heads: SmallVec::new(),
            faults: SmallVec::new(),
            local: Local::Empty,
            div: SmallVec::new(),
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
            RecordKey::Pins(_) => {
                Arc::make_mut(&mut self.pin_keys).insert(k);
            }
            RecordKey::DeviceProfile(device) => {
                self.profile_keys.insert(*device, k);
            }
            RecordKey::StoryBase(_) | RecordKey::Frame(..) | RecordKey::StoryRef(_) => {
                Arc::make_mut(&mut self.story_keys).insert(k);
            }
            _ => {}
        }
        let Some(book) = book else {
            return;
        };
        let books = Arc::make_mut(&mut self.books);
        let entry = books.entry(book).or_insert_with(BookEntry::new);
        entry.keys.push(k);
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
            RecordKey::Comment(_) => {
                let chapter = match shape {
                    Shape::Comment { target } => match &**target {
                        crate::record::CommentTarget::Verse { chapter, .. } => Some(chapter.get()),
                        crate::record::CommentTarget::Check(check) => {
                            super::attach::target_chapter(check)
                        }
                        crate::record::CommentTarget::Frame { .. } => None,
                    },
                    _ => None,
                };
                if let Some(c) = chapter {
                    self.chapters
                        .entry_or((book, c), ChapterEntry::default)
                        .comments
                        .push(k);
                }
            }
            _ => {}
        }
    }

    fn intern_batch(&mut self, batch: (BatchId, u32)) -> BatchIdx {
        if let Some(b) = self.batch_ids.get(&batch) {
            return *b;
        }
        let b = BatchIdx(self.batches.len() as u32);
        self.batches.push(BatchEntry {
            id: batch.0,
            n: batch.1,
            members: Vec::new(),
            quarantined: 0,
            quarantined_device: None,
            complete: false,
            passage: SmallVec::new(),
            anomaly: None,
        });
        self.batch_ids.insert(batch, b);
        self.batches_by_id.entry_or(batch.0, SmallVec::new).push(b);
        b
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

    fn insert(&mut self, input: FoldInput, dirty: &mut Dirty) {
        let (id, raw, summary) = match input.line {
            Line::Verified(Verified::Quarantined(q)) => {
                if self.quarantined.contains_key(&q.id) {
                    return;
                }
                let batch = q.batch.map(|b| (b.id, b.n()));
                self.anomalies.insert(Anomaly::QuarantinedChange {
                    change: q.id,
                    rule: q.rule.clone(),
                });
                if let Some(batch) = batch {
                    let b = self.intern_batch(batch);
                    if let Some(entry) = self.batches.get_mut(b.get()) {
                        entry.quarantined += 1;
                        if entry.quarantined_device.is_none() {
                            entry.quarantined_device = q.device;
                        }
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
                if self.ids.contains_key(&change.id()) {
                    (change.id(), change.raw().clone(), None)
                } else {
                    let summary = meta::summarize(change.body(), &mut self.resources);
                    (change.id(), change.raw().clone(), Some(summary))
                }
            }
            Line::Prepared { id, raw, summary } => (id, raw, Some(*summary)),
        };
        {
            {
                if let Some(c) = self.ids.get(&id).copied() {
                    if let Some(pack) = input.pack
                        && let Some(m) = self.metas.get_mut(c.get())
                        && !m.packs.contains(&pack)
                    {
                        m.packs.push(pack);
                    }
                    return;
                }
                let Some(mut summary) = summary else {
                    return;
                };
                self.count(Counter::ChangesIndexed, 1);
                if let Shape::Decision { resource, .. } = &mut summary.shape {
                    *resource = self.resources.intern(resource);
                }
                let k = self.intern_key(summary.key, summary.book, &summary.shape);
                let b = self.intern_batch(summary.batch);
                let c = ChangeIdx(self.metas.len() as u32);
                if summary.device.kind() == DeviceKind::Local {
                    let ids = self
                        .seqs
                        .entry_or((summary.device, summary.seq), SmallVec::new);
                    ids.push(id);
                    if ids.len() > 1 {
                        let mut changes: Vec<ChangeId> = ids.to_vec();
                        changes.sort();
                        let device = summary.device;
                        let seq = summary.seq;
                        self.set_duplicate(device, seq, changes);
                    }
                }
                self.metas.push(Meta {
                    id,
                    key: k,
                    batch: b,
                    parents: summary.parents,
                    device: summary.device,
                    seq: summary.seq,
                    at: summary.at,
                    cause: summary.cause,
                    stub: summary.stub,
                    shape: summary.shape,
                    raw,
                    packs: input.pack.into_iter().collect(),
                    active: false,
                });
                self.ids.insert(id, c);
                self.entry_mut(k).changes.push(c);
                if let Some(entry) = self.batches.get_mut(b.get()) {
                    entry.members.push(c);
                }
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

    fn set_duplicate(&mut self, device: DeviceId, seq: DeviceSeq, changes: Vec<ChangeId>) {
        let stale: Vec<Anomaly> = self
            .anomalies
            .iter()
            .filter(|a| {
                matches!(a, Anomaly::DuplicateDevice { device: d, seq: s, .. } if *d == device && *s == seq)
            })
            .cloned()
            .collect();
        for anomaly in stale {
            self.anomalies.remove(&anomaly);
        }
        self.anomalies.insert(Anomaly::DuplicateDevice {
            device,
            seq,
            changes,
        });
    }

    // ---------------------------------------------------------------------------------
    // The stages
    // ---------------------------------------------------------------------------------

    fn run(&mut self, dirty: &mut Dirty) {
        self.stage_batches(dirty);
        if dirty.rebuild {
            return;
        }
        self.stage_keys(dirty);
        super::passage::stage_groups(self, dirty);
        super::attach::stage_local(self, dirty);
        self.stage_meta(dirty);
        super::books::stage_books(self, dirty);
        super::attach::stage_chapters(self, dirty);
        super::attach::stage_attached(self, dirty);
        super::attach::stage_paths(self, dirty);
        if dirty.unknown {
            self.stage_unknown();
        }
    }

    /// F2: batch completeness and releases, to a fixpoint.
    fn stage_batches(&mut self, dirty: &mut Dirty) {
        let mut queue: Vec<BatchIdx> = if dirty.all {
            (0..self.batches.len() as u32).rev().map(BatchIdx).collect()
        } else {
            dirty.batches.iter().copied().collect()
        };
        while let Some(b) = queue.pop() {
            let (id, complete, members) = {
                let entry = self.batch(b);
                let complete = entry.quarantined == 0 && entry.members.len() >= entry.n as usize;
                (entry.id, complete, entry.members.clone())
            };
            let released = self.released.get(&id).cloned();
            let mut newly: Vec<ChangeIdx> = Vec::new();
            for &c in &members {
                let meta = self.meta(c);
                let active = complete || released.as_ref().is_some_and(|r| r.contains(&meta.id));
                if active == meta.active {
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
                self.metas[c.get()].active = true;
            }
            if !newly.is_empty() {
                let passage = self.passage_books(b);
                self.batches[b.get()].passage = passage;
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

    /// What a change's becoming folded touches.
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
        if entry.active == 1
            && let Some(book) = entry.book
        {
            if let Some(be) = Arc::make_mut(&mut self.books).get_mut(&book) {
                be.records += 1;
            }
            if !dirty.all {
                dirty.books.insert(book);
            }
        }
        if let Some(children) = self.waiting.remove(&id)
            && !dirty.all
        {
            for child in children {
                let ck = self.meta(child).key;
                dirty.simple.remove(&ck);
                dirty.keys.insert(ck);
            }
        }
        // Links to parents that are not folded yet wait for them.
        let parents = self.meta(c).parents.clone();
        for p in parents {
            let known_active = self.ids.get(&p).is_some_and(|pi| self.meta(*pi).active);
            if !known_active {
                self.waiting.entry_or(p, SmallVec::new).push(c);
            }
        }
        if let Shape::Release { changes } = &self.meta(c).shape {
            let changes = changes.clone();
            if let RecordKey::BatchRelease(target) = &self.entry(k).key {
                let target = *target;
                let set =
                    Arc::make_mut(self.released.entry_or(target, || Arc::new(BTreeSet::new())));
                let before = set.len();
                set.extend(changes.iter().copied());
                if set.len() != before
                    && let Some(targets) = self.batches_by_id.get(&target)
                {
                    queue.extend(targets.iter().copied());
                }
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
        meta.parents.iter().all(|p| {
            self.ids
                .get(p)
                .is_some_and(|pi| self.meta(*pi).active && self.meta(*pi).key == meta.key)
        })
    }

    /// Lists, or stops listing, a batch that is held (F2).
    fn anomaly_for_batch(&mut self, b: BatchIdx) {
        let entry = self.batch(b);
        let anomaly = if entry.complete {
            None
        } else {
            entry
                .members
                .iter()
                .map(|c| {
                    let m = self.meta(*c);
                    (m.device, m.seq, m.id)
                })
                .min()
                .map(|(d, _, _)| d)
                .or(entry.quarantined_device)
                .map(|device| Anomaly::IncompleteBatch {
                    batch: entry.id,
                    device,
                    have: entry.members.len() as u32,
                    of: entry.n,
                })
        };
        if anomaly == entry.anomaly {
            return;
        }
        if let Some(old) = entry.anomaly.clone() {
            self.anomalies.remove(&old);
        }
        if let Some(new) = &anomaly {
            self.anomalies.insert(new.clone());
        }
        self.batches[b.get()].anomaly = anomaly;
    }

    /// The books for which batch `b` is a passage batch (§7.3): it is not a seed batch and
    /// holds folded changes with parents to two or more verse records of the book.
    fn passage_books(&self, b: BatchIdx) -> SmallVec<[BookId; 1]> {
        let entry = self.batch(b);
        if entry
            .members
            .iter()
            .any(|c| self.meta(*c).cause == CauseClass::Seed)
        {
            return SmallVec::new();
        }
        let mut per_book: BTreeMap<BookId, u32> = BTreeMap::new();
        for c in &entry.members {
            let meta = self.meta(*c);
            if !meta.active || meta.parents.is_empty() {
                continue;
            }
            if let RecordKey::Verse(book, ..) = self.entry(meta.key).key {
                *per_book.entry(book).or_default() += 1;
            }
        }
        per_book
            .into_iter()
            .filter(|(_, n)| *n >= 2)
            .map(|(book, _)| book)
            .collect()
    }

    /// F3, F4, F6 for every dirty key.
    fn stage_keys(&mut self, dirty: &mut Dirty) {
        let simple: Vec<(KeyIdx, ChangeIdx)> = dirty.simple.iter().map(|(k, c)| (*k, *c)).collect();
        for (k, c) in simple {
            self.count(Counter::KeysUpdated, 1);
            let parents: SmallVec<[ChangeIdx; 2]> = self
                .meta(c)
                .parents
                .iter()
                .filter_map(|p| self.ids.get(p).copied())
                .collect();
            let mut heads: SmallVec<[ChangeIdx; 1]> = self
                .entry(k)
                .heads
                .iter()
                .copied()
                .filter(|h| !parents.contains(h))
                .collect();
            heads.push(c);
            self.sort_by_id(&mut heads);
            self.entry_mut(k).heads = heads;
            self.after_heads(k, dirty);
        }
        let keys: Vec<KeyIdx> = if dirty.all {
            (0..self.keys.len() as u32).map(KeyIdx).collect()
        } else {
            dirty.keys.iter().copied().collect()
        };
        for k in keys {
            self.recompute_links(k);
            self.after_heads(k, dirty);
        }
        if dirty.all {
            dirty.pins = true;
            dirty.paths = true;
            dirty.unknown = !self.unknown_keys.is_empty();
        }
    }

    pub(crate) fn sort_by_id(&self, list: &mut [ChangeIdx]) {
        list.sort_by_key(|c| self.meta(*c).id);
    }

    /// F3 and F4 for one key from its changes alone.
    fn recompute_links(&mut self, k: KeyIdx) {
        self.count(Counter::KeysRecomputed, 1);
        let old_faults = std::mem::take(&mut self.entry_mut(k).faults);
        let mut has_child: SmallVec<[ChangeIdx; 4]> = SmallVec::new();
        let mut faults: SmallVec<[(ChangeIdx, ChangeId, Fault); 0]> = SmallVec::new();
        let active: SmallVec<[ChangeIdx; 4]> = self
            .entry(k)
            .changes
            .iter()
            .copied()
            .filter(|c| self.meta(*c).active)
            .collect();
        self.count(Counter::ChangesVisited, active.len() as u64);
        for &c in &active {
            for p in self.meta(c).parents.iter() {
                match self.ids.get(p) {
                    Some(pi) if self.meta(*pi).active => {
                        if self.meta(*pi).key == k {
                            has_child.push(*pi);
                        } else {
                            faults.push((c, *p, Fault::OtherRecord));
                        }
                    }
                    _ => faults.push((c, *p, Fault::Missing)),
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
            self.anomalies
                .remove(&link_anomaly(self.meta(*c).id, *p, *fault));
        }
        for (c, p, fault) in faults.iter() {
            self.anomalies
                .insert(link_anomaly(self.meta(*c).id, *p, *fault));
        }
        let e = self.entry_mut(k);
        e.heads = heads;
        e.faults = faults;
    }

    /// F6 for a key whose heads may have changed.
    fn after_heads(&mut self, k: KeyIdx, dirty: &mut Dirty) {
        let (local, div) = super::merge::resolve_local(self, k);
        let entry = self.entry_mut(k);
        entry.local = local;
        entry.div = div;
        if dirty.all {
            return;
        }
        dirty.changed.insert(k);
        let entry = self.entry(k);
        match &entry.key {
            RecordKey::Pins(_) => dirty.pins = true,
            RecordKey::CarriedFile(_) => dirty.paths = true,
            _ => {}
        }
        if let Some(book) = entry.book {
            dirty.books.insert(book);
        }
        dirty.attached.insert(k);
    }

    /// The anomaly of unknown kinds (§12): one entry for every folded change of a kind from
    /// a newer minor.
    fn stage_unknown(&mut self) {
        let stale: Vec<Anomaly> = self
            .anomalies
            .iter()
            .filter(|a| matches!(a, Anomaly::UnknownKinds { .. }))
            .cloned()
            .collect();
        for anomaly in stale {
            self.anomalies.remove(&anomaly);
        }
        let mut kinds: BTreeSet<Box<str>> = BTreeSet::new();
        let mut changes = 0u32;
        for k in self.unknown_keys.iter() {
            let entry = self.entry(*k);
            if entry.active > 0 {
                kinds.insert(entry.key.kind_name().into());
                changes += entry.active;
            }
        }
        if changes > 0 {
            self.anomalies.insert(Anomaly::UnknownKinds {
                kinds: kinds.into_iter().collect(),
                changes,
            });
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

    pub(crate) fn count(&self, counter: Counter, n: u64) {
        self.counters.add(counter, n);
    }
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
