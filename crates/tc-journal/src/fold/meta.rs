//! What the fold keeps of each change: its links, its batch, its order key and a summary of
//! its value's shape. The value itself stays in the change's stored bytes and is decoded on
//! demand (§14: values are read on demand), so a whole-Bible fold holds about a hundred
//! bytes per change besides the bytes.

use crate::canon;
use crate::change::{Cause, ChangeBody};
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
    pub fn get(self) -> usize {
        self.0 as usize
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
    Import,
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
            Cause::Import(_) => Self::Import,
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
        changes: Arc<[ChangeId]>,
    },
}

/// What the fold keeps of one change.
#[derive(Debug, Clone)]
pub(crate) struct Meta {
    pub id: ChangeId,
    pub key: KeyIdx,
    pub batch: BatchIdx,
    pub parents: SmallVec<[ChangeId; 1]>,
    pub device: DeviceId,
    pub seq: DeviceSeq,
    pub at: WallTime,
    pub cause: CauseClass,
    /// A seed stub (§7.3): starting text no person wrote.
    pub stub: bool,
    pub shape: Shape,
    /// The canonical JSON exactly as stored, or where to read it.
    pub value: super::value::Stored,
    /// The first pack that holds it, in [`super::FoldState`]'s pack table; others are in
    /// its `more_packs`.
    pub pack: PackSlot,
    /// Folded: its batch is complete or a release lists it (F2).
    pub active: bool,
}

/// A pack's place in the fold's pack table, or none (a change only in a pending file).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PackSlot(pub(crate) u32);

impl PackSlot {
    pub(crate) const NONE: Self = Self(u32::MAX);
}

impl Meta {
    /// 0 for an outside change recorded without a base (§11.1 X3), 1 for every other: such
    /// a change is never chosen as provisional over another (F7).
    pub(crate) fn rank(&self) -> u8 {
        u8::from(!matches!(
            self.cause,
            CauseClass::External { unknown_base: true }
        ))
    }

    /// The order of provisional choices: largest (rank, at, id) wins (F7).
    pub(crate) fn order(&self) -> (u8, WallTime, ChangeId) {
        (self.rank(), self.at, self.id)
    }

    pub(crate) fn verse(&self) -> Option<VerseShape> {
        match self.shape {
            Shape::Verse(shape) => Some(shape),
            _ => None,
        }
    }
}

/// Interns resource refs so a hundred thousand decisions share a handful of values.
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
            changes: value.changes.clone().into(),
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
    let value = json::parse(raw).ok()?;
    canon::decode_body(&value).ok()
}

/// The JSON of a change's record value (`record.<kind>.value`), `None` when the record has
/// no value member (a `None` setting, project detail, layout or story reference).
pub(crate) fn value_json(raw: &[u8]) -> Option<json::Json> {
    let value = json::parse(raw).ok()?;
    let record = value.as_obj()?.get("record")?.as_obj()?;
    let (_, inner) = record.iter().next()?;
    inner.as_obj()?.get("value").cloned()
}

/// Why a quarantined change was set aside, with what could be read of its origin, and the
/// line itself so a rebuild can fold it again.
#[derive(Debug, Clone)]
pub(crate) struct Quarantined {
    pub input: Option<super::state::FoldInput>,
}
