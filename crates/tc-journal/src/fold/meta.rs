//! What the fold keeps of each change: its links, its batch, its order key and a summary of
//! its value's shape. The value itself stays in the change's stored bytes and is decoded on
//! demand (§14: values are read on demand), so a whole-Bible fold holds 96 bytes per change
//! in its change table.

use super::cow::FxMap;
use crate::canon;
use crate::change::{Cause, ChangeBody, ImportSource};
use crate::ids::{BatchId, ChangeId, DeviceId, DeviceSeq, GitOid, WallTime};
use crate::json;
use crate::keys::{CheckKey, RecordKey};
use crate::record::{
    BookPresence, CarriedValue, ChapterFront, ChapterVerse, CheckValue, CommentTarget, PresentHow,
    Record, ResourceRef, VerseValue,
};
use smallvec::SmallVec;
use std::sync::Arc;
use tc_core::BookId;

/// A change's place in the fold's change table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChangeIdx(pub(crate) u32);

impl ChangeIdx {
    /// No change: the fold's tables never hold this many.
    pub(crate) const NONE: Self = Self(u32::MAX);

    pub fn get(self) -> usize {
        self.0 as usize
    }
}

/// A list of changes that most often holds one: a record's changes and its heads. 16 bytes,
/// where a small vector with room for one took 24, and no allocation for one change.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) enum ChangeList {
    #[default]
    Empty,
    One(ChangeIdx),
    // A boxed Vec is a thin pointer, so the list stays 16 bytes.
    #[allow(clippy::box_collection)]
    Many(Box<Vec<ChangeIdx>>),
}

impl ChangeList {
    pub(crate) fn push(&mut self, c: ChangeIdx) {
        match self {
            Self::Empty => *self = Self::One(c),
            Self::One(first) => *self = Self::Many(Box::new(vec![*first, c])),
            Self::Many(list) => list.push(c),
        }
    }
}

impl std::ops::Deref for ChangeList {
    type Target = [ChangeIdx];

    fn deref(&self) -> &[ChangeIdx] {
        match self {
            Self::Empty => &[],
            Self::One(c) => std::slice::from_ref(c),
            Self::Many(list) => list,
        }
    }
}

impl FromIterator<ChangeIdx> for ChangeList {
    fn from_iter<I: IntoIterator<Item = ChangeIdx>>(iter: I) -> Self {
        let mut list = Self::Empty;
        for c in iter {
            list.push(c);
        }
        list
    }
}

/// A record key's place in the fold's key table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct KeyIdx(pub(crate) u32);

impl KeyIdx {
    pub(crate) fn get(self) -> usize {
        self.0 as usize
    }
}

/// A parent a change names: that change's place in the change table, or, when the fold
/// did not hold it as the child arrived, the place of its id in the fold's table of absent
/// parents (it may arrive later: [`super::FoldState`] looks such an id up again).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParentRef(u32);

const ABSENT: u32 = 1 << 31;
const NO_PARENT: u32 = u32::MAX;

impl ParentRef {
    pub(crate) fn known(c: ChangeIdx) -> Self {
        Self(c.0)
    }

    pub(crate) fn absent(at: usize) -> Self {
        Self(at as u32 | ABSENT)
    }

    /// The change, or the place of the absent id.
    pub(crate) fn get(self) -> Result<ChangeIdx, usize> {
        if self.0 & ABSENT == 0 {
            Ok(ChangeIdx(self.0))
        } else {
            Err((self.0 & !ABSENT) as usize)
        }
    }
}

/// The first parent a change names, or none; the change table keeps the others apart
/// ([`super::FoldState::parents`]), since nearly every change names one at most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FirstParent(u32);

impl FirstParent {
    pub(crate) const NONE: Self = Self(NO_PARENT);

    pub(crate) fn of(first: Option<ParentRef>) -> Self {
        first.map_or(Self::NONE, |p| Self(p.0))
    }

    pub(crate) fn get(self) -> Option<ParentRef> {
        (self != Self::NONE).then_some(ParentRef(self.0))
    }
}

/// A device's place in the fold's device table: a project has a handful of devices, and
/// every change names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeviceIdx(pub(crate) u32);

/// A batch's place in the fold's batch table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BatchIdx(pub(crate) u32);

impl BatchIdx {
    pub(crate) fn get(self) -> usize {
        self.0 as usize
    }
}

/// The part of a cause the fold's rules read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CauseClass {
    Edit,
    Resolve,
    Restore,
    CarryOver,
    External { unknown_base: bool },
    Import { unknown_base: bool },
    Seed,
    StartOver,
    System,
    Newer,
}

impl CauseClass {
    fn of(cause: &Cause) -> Self {
        match cause {
            Cause::Edit => Self::Edit,
            Cause::Resolve => Self::Resolve,
            Cause::Restore { .. } => Self::Restore,
            Cause::CarryOver { .. } => Self::CarryOver,
            Cause::External { unknown_base, .. } => Self::External {
                unknown_base: *unknown_base,
            },
            Cause::Import(source) => Self::Import {
                unknown_base: matches!(
                    source,
                    ImportSource::Usfm {
                        unknown_base: true,
                        ..
                    }
                ),
            },
            Cause::Seed => Self::Seed,
            Cause::StartOver => Self::StartOver,
            Cause::System(_) => Self::System,
            Cause::Newer { .. } => Self::Newer,
        }
    }
}

/// The structure of a verse value: what coherence and placement read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerseShape {
    Text {
        last: u16,
        from: Option<ChapterVerse>,
    },
    Covered {
        by: u16,
    },
    Absent {
        moved_to: Option<ChapterVerse>,
    },
}

/// A summary of a value: the parts the fold's rules read without decoding the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Shape {
    Other,
    Verse(VerseShape),
    Front {
        present: bool,
    },
    Presence(Option<PresentHow>),
    Alignment {
        dismissed: bool,
    },
    Decision {
        resource: Arc<ResourceRef>,
        dismissed: bool,
    },
    MovedTo {
        to: Arc<CheckKey>,
    },
    Comment {
        target: Arc<CommentTarget>,
    },
    Carried {
        oid: Option<GitOid>,
    },
    Release {
        changes: Arc<Vec<ChangeId>>,
    },
}

/// What the fold keeps of one change: 96 bytes for each of the 620,000 changes of a whole
/// Bible. What few changes have (a second parent, a sequence number past `u32`, bytes kept
/// in memory, the target of a move, a comment's target, a carried file's blob, a release's
/// list) the fold keeps beside the table, so every change does not pay for it.
#[derive(Debug, Clone)]
pub(crate) struct Meta {
    pub id: ChangeId,
    pub at: WallTime,
    pub key: KeyIdx,
    pub batch: BatchIdx,
    pub(crate) first_parent: FirstParent,
    /// In [`super::FoldState`]'s device table.
    pub device: DeviceIdx,
    /// The sequence number, or [`Meta::SEQ_APART`] when it does not fit in 32 bits.
    pub(crate) seq: u32,
    /// The first pack that holds it, in [`super::FoldState`]'s pack table; others are in
    /// its `more_packs`.
    pub pack: PackSlot,
    pub form: Form,
    /// Where to read its canonical JSON, unless [`Meta::IN_MEMORY`] says the fold keeps it.
    pub(crate) value: ValueRef,
    pub cause: CauseClass,
    pub(crate) flags: u8,
}

/// A pack's place in the fold's pack table, or none (a change only in a pending file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PackSlot(pub(crate) u32);

impl PackSlot {
    pub(crate) const NONE: Self = Self(u32::MAX);
}

impl Meta {
    /// A seed stub (§7.3): starting text no person wrote.
    pub(crate) const STUB: u8 = 1;
    /// Folded: its batch is complete or a release lists it (F2).
    pub(crate) const ACTIVE: u8 = 2;
    /// It names more than one parent.
    pub(crate) const MORE_PARENTS: u8 = 4;
    /// Its bytes are kept in memory, not read from the fold's source.
    pub(crate) const IN_MEMORY: u8 = 8;
    /// The sequence number of a change whose number is kept apart.
    pub(crate) const SEQ_APART: u32 = u32::MAX;

    pub(crate) fn active(&self) -> bool {
        self.flags & Self::ACTIVE != 0
    }

    pub(crate) fn set_active(&mut self, active: bool) {
        if active {
            self.flags |= Self::ACTIVE;
        } else {
            self.flags &= !Self::ACTIVE;
        }
    }

    pub(crate) fn stub(&self) -> bool {
        self.flags & Self::STUB != 0
    }

    pub(crate) fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    /// 0 for an outside change or an import recorded without a base (§11.1 X3, §13.6 U3), 1
    /// for every other: such a change is never chosen as provisional over another (F7).
    pub(crate) fn rank(&self) -> u8 {
        u8::from(!matches!(
            self.cause,
            CauseClass::External { unknown_base: true } | CauseClass::Import { unknown_base: true }
        ))
    }

    /// The order of provisional choices: largest (rank, at, id) wins (F7).
    pub(crate) fn order(&self) -> (u8, WallTime, ChangeId) {
        (self.rank(), self.at, self.id)
    }

    pub(crate) fn verse(&self) -> Option<VerseShape> {
        match self.form {
            Form::Verse(shape) => Some(shape),
            _ => None,
        }
    }
}

/// What the change table keeps of a [`Shape`]: 8 bytes. A decision names its resource by
/// its place in the fold's resource table; the kinds few changes have keep their whole shape
/// beside the table ([`super::FoldState::shape`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    Other,
    Verse(VerseShape),
    Front { present: bool },
    Presence(Option<PresentHow>),
    Alignment { dismissed: bool },
    Decision(DecisionForm),
    MovedTo,
    Comment,
    Carried,
    Release,
}

/// A decision's resource, as its place in the fold's resource table, and whether it was
/// dismissed, in one word: the table never holds 2³¹ resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecisionForm(u32);

const DISMISSED: u32 = 1 << 31;

impl DecisionForm {
    pub(crate) fn new(resource: u32, dismissed: bool) -> Self {
        Self((resource & !DISMISSED) | if dismissed { DISMISSED } else { 0 })
    }

    pub(crate) fn resource(self) -> u32 {
        self.0 & !DISMISSED
    }

    pub(crate) fn dismissed(self) -> bool {
        self.0 & DISMISSED != 0
    }
}

/// Where a change's canonical JSON lies in the fold's source (20 bytes): the loader's file,
/// the span, and the [`super::value::fingerprint`] of the bytes it verified, in two halves so
/// the change table needs no padding for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValueRef {
    pub file: u32,
    pub offset: u32,
    pub len: u32,
    fingerprint: [u32; 2],
}

impl ValueRef {
    /// The place of a value the fold keeps in memory.
    pub(crate) const NONE: Self = Self {
        file: 0,
        offset: 0,
        len: 0,
        fingerprint: [0; 2],
    };

    pub(crate) fn new(file: u32, offset: u32, len: u32, fingerprint: u64) -> Self {
        Self {
            file,
            offset,
            len,
            fingerprint: [fingerprint as u32, (fingerprint >> 32) as u32],
        }
    }

    pub(crate) fn fingerprint(&self) -> u64 {
        u64::from(self.fingerprint[0]) | (u64::from(self.fingerprint[1]) << 32)
    }
}

/// The resources the fold's decisions name, each once, numbered in the order the fold met
/// them: a whole Bible's 280,000 decisions name a handful. Shared, so a snapshot copies it
/// only when a step meets a new resource.
#[derive(Debug, Clone, Default)]
pub(crate) struct ResourceTable {
    list: Arc<Vec<Arc<ResourceRef>>>,
    places: Arc<FxMap<Arc<ResourceRef>, u32>>,
}

impl ResourceTable {
    /// The place of `resource`, numbered now if the table does not hold it yet.
    pub(crate) fn place(&mut self, resource: &Arc<ResourceRef>) -> u32 {
        if let Some(at) = self.places.get(resource) {
            return *at;
        }
        let at = self.list.len() as u32;
        Arc::make_mut(&mut self.list).push(resource.clone());
        Arc::make_mut(&mut self.places).insert(resource.clone(), at);
        at
    }

    pub(crate) fn get(&self, at: u32) -> Option<&Arc<ResourceRef>> {
        self.list.get(at as usize)
    }
}

/// Interns resource refs so the decisions of one file a loader reads share a handful of
/// values; the fold keeps its own [`ResourceTable`].
#[derive(Debug, Clone, Default)]
pub(crate) struct Resources(Vec<Arc<ResourceRef>>);

impl Resources {
    pub(crate) fn intern(&mut self, resource: &ResourceRef) -> Arc<ResourceRef> {
        if let Some(found) = self.0.iter().find(|r| ***r == *resource) {
            return found.clone();
        }
        let new = Arc::new(resource.clone());
        // Bound the list: a project pins a few resources; a forged set of thousands only
        // loses sharing, never correctness.
        if self.0.len() < 256 {
            self.0.push(new.clone());
        }
        new
    }
}

/// A seed stub (§7.3): a `Seed` change on a verse with no words (`___`, empty, or no text
/// at all), a book header, a chapter front or a book layout.
fn is_stub(cause: CauseClass, record: &Record) -> bool {
    if cause != CauseClass::Seed {
        return false;
    }
    match record {
        Record::Verse { value, .. } => match value {
            VerseValue::Text { words, .. } => words == "___" || words.is_empty(),
            VerseValue::Covered { .. } | VerseValue::Absent { .. } => true,
        },
        Record::BookHeader { .. } | Record::ChapterFront { .. } | Record::BookLayout { .. } => true,
        _ => false,
    }
}

fn shape_of(record: &Record, resources: &mut Resources) -> Shape {
    match record {
        Record::Verse { value, .. } => Shape::Verse(match value {
            VerseValue::Text { last, from, .. } => VerseShape::Text {
                last: last.get(),
                from: *from,
            },
            VerseValue::Covered { by } => VerseShape::Covered { by: by.get() },
            VerseValue::Absent { moved_to } => VerseShape::Absent {
                moved_to: *moved_to,
            },
        }),
        Record::ChapterFront { value, .. } => Shape::Front {
            present: matches!(value, ChapterFront::Present(_)),
        },
        Record::BookPresence { value, .. } => Shape::Presence(match value {
            BookPresence::Present { how, .. } => Some(*how),
            BookPresence::Removed { .. } => None,
        }),
        Record::Alignment { value, .. } => Shape::Alignment {
            dismissed: value.dismissed,
        },
        Record::Check { value, .. } => match value {
            CheckValue::Decision(decision) => Shape::Decision {
                resource: resources.intern(&decision.resource),
                dismissed: decision.dismissed,
            },
            CheckValue::MovedTo { to } => Shape::MovedTo {
                to: Arc::new(to.clone()),
            },
        },
        Record::Comment { value, .. } => Shape::Comment {
            target: Arc::new(value.target.clone()),
        },
        Record::CarriedFile { value, .. } => Shape::Carried {
            oid: match value {
                CarriedValue::Present { oid, .. } => Some(*oid),
                CarriedValue::Removed => None,
            },
        },
        Record::BatchRelease { value, .. } => Shape::Release {
            changes: Arc::new(value.changes.clone()),
        },
        _ => Shape::Other,
    }
}

/// The book a record belongs to for §7.6: the key's book, or a comment's target book.
pub(crate) fn book_of(key: &RecordKey, shape: &Shape) -> Option<BookId> {
    match (key, shape) {
        (RecordKey::Comment(_), Shape::Comment { target }) => match &**target {
            CommentTarget::Verse { book, .. } => Some(*book),
            CommentTarget::Check(check) => check.book(),
            CommentTarget::Frame { .. } => None,
        },
        _ => key.book(),
    }
}

/// The parts of a decoded change the fold keeps.
#[derive(Debug, Clone)]
pub(crate) struct Summary {
    pub key: RecordKey,
    pub parents: SmallVec<[ChangeId; 1]>,
    pub device: DeviceId,
    pub seq: DeviceSeq,
    pub at: WallTime,
    pub batch: (BatchId, u32),
    pub cause: CauseClass,
    pub stub: bool,
    pub shape: Shape,
    pub book: Option<BookId>,
}

pub(crate) fn summarize(body: &ChangeBody, resources: &mut Resources) -> Summary {
    let key = body.record.key();
    let cause = CauseClass::of(&body.cause);
    let shape = shape_of(&body.record, resources);
    let book = book_of(&key, &shape);
    Summary {
        parents: body.parents.iter().copied().collect(),
        device: body.device,
        seq: body.seq,
        at: body.at,
        batch: (body.batch.id, body.batch.n()),
        cause,
        stub: is_stub(cause, &body.record),
        shape,
        book,
        key,
    }
}

/// Decodes the change body of stored canonical bytes that passed the per-change rules when
/// they were read. `None` only if the bytes changed since, which a fold never does.
pub(crate) fn decode(raw: &[u8]) -> Option<ChangeBody> {
    let tape = json::parse_tape(raw).ok()?;
    canon::decode_body_view(tape.root()).ok()
}

/// [`decode`]'s record alone: the other fields of the change are not decoded.
pub(crate) fn decode_record(raw: &[u8]) -> Option<crate::record::Record> {
    let tape = json::parse_tape(raw).ok()?;
    canon::decode_stored_record(tape.root())
}

/// The JSON of a change's record value (`record.<kind>.value`), `None` when the record has
/// no value member (a `None` setting, project detail, layout or story reference).
pub(crate) fn value_json(raw: &[u8]) -> Option<json::Json> {
    let tape = json::parse_tape(raw).ok()?;
    let record = tape.root().as_obj()?.get("record")?.as_obj()?;
    let (_, inner) = record.iter().next()?;
    inner.as_obj()?.get("value").map(json::View::to_json)
}

/// Why a quarantined change was set aside, with what could be read of its origin, and the
/// line itself so a rebuild can fold it again.
#[derive(Debug, Clone)]
pub(crate) struct Quarantined {
    pub input: Option<super::state::FoldInput>,
}
