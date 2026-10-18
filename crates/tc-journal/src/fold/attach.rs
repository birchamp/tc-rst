//! F9 and F10: verse coherence per chapter (§7.5), the placement of attached records and
//! the Detached list (§7.7), path collisions of carried files (§3.3), and the conflicts
//! of single records.

use super::books::rewound_batches;
use super::merge::{ancestry, conflict_kind, group_by_value, record_of, units_of_conflict};
use super::meta::{ChangeIdx, Form, KeyIdx, Shape, VerseShape};
use super::passage::{local_projection, rewind};
use super::state::{Dirty, Fault, FoldState, Local, Owner, Projected};
use super::{Candidate, Conflict, ConflictId, ConflictKind, Counter, Side};
use crate::ids::{ChangeId, DeviceId, RepoPath, StoryNo};
use crate::json::Json;
use crate::keys::{CheckKey, CheckTarget, PinGroup, RecordKey, ToolId};
use crate::pins::{Answer, Collection, Ladder, LanguageSets};
use crate::record::{
    ChapterVerse, CheckValue, CommentTarget, DeviceProfileValue, PinGroupValue, Record,
    ResourceRef, VerseValue,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tc_core::BookId;

/// An anchor: the chapter and verse a text had before any renumber (§7.7).
pub(crate) type Anchor = (u16, u16);

/// The chapter a decision's target names, when it is a number.
pub(crate) fn target_chapter(check: &CheckKey) -> Option<u16> {
    match &check.target {
        CheckTarget::Verse { chapter, .. } => chapter.number(),
        CheckTarget::Frame { .. } => None,
    }
}

/// The chapter and first verse a decision's target names, when the chapter is a number and
/// the verse a number or a span of numbers.
fn target_verse(check: &CheckKey) -> Option<(u16, u16)> {
    match &check.target {
        CheckTarget::Verse { chapter, verse, .. } => Some((chapter.number()?, verse.span()?.0)),
        CheckTarget::Frame { .. } => None,
    }
}

// ---------------------------------------------------------------------------------------
// What a verse shows
// ---------------------------------------------------------------------------------------

/// The changes of verse record `k` that projection rewinds (§7.6): its changes of the
/// `StartOver` batches in `rewound`, when its projected value is, or descends from, one of
/// them. Empty when the record shows its own projected value.
pub(crate) fn rewound_here(
    state: &FoldState,
    k: KeyIdx,
    rewound: &[crate::ids::BatchId],
) -> BTreeSet<ChangeIdx> {
    if rewound.is_empty() {
        return BTreeSet::new();
    }
    let entry = state.entry(k);
    let bad: BTreeSet<ChangeIdx> = entry
        .changes
        .iter()
        .copied()
        .filter(|x| {
            state.meta(*x).active() && rewound.contains(&state.batch(state.meta(*x).batch).id)
        })
        .collect();
    if bad.is_empty() {
        return bad;
    }
    let shown: Vec<ChangeIdx> = match &entry.projected {
        Projected::Nothing => Vec::new(),
        Projected::Change(c) => vec![*c],
        Projected::Merged(_) => entry.heads.to_vec(),
    };
    let descends = shown.iter().any(|c| {
        super::merge::ancestors_or_self(state, *c)
            .iter()
            .any(|a| bad.contains(a))
    });
    if descends { bad } else { BTreeSet::new() }
}

/// The value projection shows for a verse record: its projected value, with every change
/// that is or descends from a `StartOver` change of a *Start empty* that lost to *Restore
/// the text* rewound (§7.6, §7.3).
pub(crate) fn shown_verse(
    state: &FoldState,
    k: KeyIdx,
    rewound: &[crate::ids::BatchId],
) -> Option<ShownVerse> {
    let bad = rewound_here(state, k, rewound);
    if !bad.is_empty() {
        let heads = state.entry(k).heads.to_vec();
        let back = rewind(state, &heads, &bad);
        return back
            .iter()
            .copied()
            .max_by_key(|x| state.meta(*x).order())
            .and_then(|x| state.meta(x).verse().map(|s| ShownVerse::Change(s, x)));
    }
    match &state.entry(k).projected {
        Projected::Nothing => None,
        Projected::Merged(record) => match &**record {
            Record::Verse { value, .. } => {
                Some(ShownVerse::Merged(shape_of_value(value), record.clone()))
            }
            _ => None,
        },
        Projected::Change(c) => state.meta(*c).verse().map(|s| ShownVerse::Change(s, *c)),
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ShownVerse {
    Change(VerseShape, ChangeIdx),
    Merged(VerseShape, Arc<Record>),
}

impl ShownVerse {
    pub(crate) fn shape(&self) -> VerseShape {
        match self {
            Self::Change(s, _) | Self::Merged(s, _) => *s,
        }
    }
}

fn shape_of_value(value: &VerseValue) -> VerseShape {
    match value {
        VerseValue::Text { last, from, .. } => VerseShape::Text {
            last: last.get(),
            from: *from,
        },
        VerseValue::Covered { by } => VerseShape::Covered { by: by.get() },
        VerseValue::Absent { moved_to } => VerseShape::Absent {
            moved_to: *moved_to,
        },
    }
}

fn anchor_of(cv: Option<ChapterVerse>) -> Option<Anchor> {
    cv.map(|cv| (cv.chapter.get(), cv.verse.get()))
}

// ---------------------------------------------------------------------------------------
// Coherence (§7.5)
// ---------------------------------------------------------------------------------------

/// A structure issue found in a chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Issue {
    /// The chapter has verses but no chapter marker.
    MissingFront,
    /// The text at `verse` spans to `last`, but only verses up to `end` are covered by it.
    ShortSpan { verse: u16, last: u16 },
    /// A covered verse no run reaches.
    StrayCovered { verse: u16, by: u16 },
}

/// The runs of a chapter: what coherence and projection both read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Runs {
    /// run start → run end.
    pub ends: BTreeMap<u16, u16>,
    /// Covered verses a run consumes.
    pub consumed: BTreeSet<u16>,
    pub issues: Vec<Issue>,
}

pub(crate) fn runs(shapes: &BTreeMap<u16, VerseShape>, front: bool) -> Runs {
    let mut out = Runs::default();
    if !front
        && shapes
            .values()
            .any(|s| !matches!(s, VerseShape::Absent { .. }))
    {
        out.issues.push(Issue::MissingFront);
    }
    for (&verse, shape) in shapes {
        if let VerseShape::Text { last, .. } = shape {
            let mut end = verse;
            while end < *last {
                let Some(next) = end.checked_add(1) else {
                    break;
                };
                if shapes.get(&next) != Some(&VerseShape::Covered { by: verse }) {
                    break;
                }
                end = next;
                out.consumed.insert(end);
            }
            if end < *last {
                out.issues.push(Issue::ShortSpan { verse, last: *last });
            }
            out.ends.insert(verse, end);
        }
    }
    for (&verse, shape) in shapes {
        if let VerseShape::Covered { by } = shape
            && !out.consumed.contains(&verse)
        {
            out.issues.push(Issue::StrayCovered { verse, by: *by });
        }
    }
    out
}

pub(crate) fn stage_chapters(state: &mut FoldState, dirty: &mut Dirty) {
    if dirty.all {
        dirty
            .chapters
            .extend(state.chapters.iter().map(|(key, _)| *key));
    }
    for k in dirty.changed.iter() {
        match &state.entry(*k).key {
            RecordKey::Verse(b, c, _) | RecordKey::ChapterFront(b, c) => {
                dirty.chapters.insert((*b, c.get()));
            }
            _ => {}
        }
    }
    let chapters: Vec<(BookId, u16)> = dirty.chapters.iter().copied().collect();
    let mut rewound: BTreeMap<BookId, Vec<crate::ids::BatchId>> = BTreeMap::new();
    for (book, chapter) in chapters {
        let rewound = rewound
            .entry(book)
            .or_insert_with(|| rewound_batches(state, book))
            .clone();
        cohere(state, book, chapter, &rewound, dirty);
    }
}

fn cohere(
    state: &mut FoldState,
    book: BookId,
    chapter: u16,
    rewound: &[crate::ids::BatchId],
    dirty: &mut Dirty,
) {
    state.count(Counter::ChaptersCohered, 1);
    let projects = state.books.get(&book).is_some_and(|b| b.status.projects());
    let Some(entry) = state.chapters.get(&(book, chapter)).cloned() else {
        return;
    };
    let mut shapes: BTreeMap<u16, VerseShape> = BTreeMap::new();
    let mut issues = Vec::new();
    let mut anchors: Vec<(Anchor, u16)> = Vec::new();
    let mut present: BTreeSet<u16> = BTreeSet::new();
    if projects {
        state.count(Counter::VersesCohered, entry.verses.len() as u64);
        for (&verse, &k) in &entry.verses {
            if let Some(shown) = shown_verse(state, k, rewound) {
                shapes.insert(verse, shown.shape());
            }
        }
        let front = entry
            .front
            .is_some_and(|f| match &state.entry(f).projected {
                Projected::Change(c) => {
                    matches!(state.meta(*c).form, Form::Front { present: true })
                }
                Projected::Merged(record) => matches!(
                    &**record,
                    Record::ChapterFront {
                        value: crate::record::ChapterFront::Present(_),
                        ..
                    }
                ),
                Projected::Nothing => false,
            });
        let found = runs(&shapes, front);
        for (&verse, shape) in &shapes {
            match shape {
                VerseShape::Text { from, .. } => {
                    present.insert(verse);
                    anchors.push((anchor_of(*from).unwrap_or((chapter, verse)), verse));
                }
                VerseShape::Covered { .. } if found.consumed.contains(&verse) => {
                    present.insert(verse);
                }
                _ => {}
            }
        }
        issues = found.issues;
    }
    // Anchors: replace this chapter's, and place again what they move.
    let old: BTreeSet<(Anchor, u16)> = entry.anchors.iter().copied().collect();
    let new: BTreeSet<(Anchor, u16)> = anchors.iter().copied().collect();
    let mut moved: BTreeSet<Anchor> = BTreeSet::new();
    for (anchor, verse) in old.difference(&new) {
        moved.insert(*anchor);
        if let Some(set) = state.anchors.get_mut(&(book, anchor.0, anchor.1)) {
            set.remove(&(chapter, *verse));
        }
    }
    for (anchor, verse) in new.difference(&old) {
        moved.insert(*anchor);
        state
            .anchors
            .entry_or((book, anchor.0, anchor.1), BTreeSet::new)
            .insert((chapter, *verse));
    }
    for anchor in moved {
        if let Some(c) = state.chapters.get(&(book, anchor.0)) {
            if let Some(k) = c.alignments.get(&anchor.1) {
                dirty.attached.insert(*k);
            }
            dirty.attached.extend(c.comments.iter().copied());
        }
    }
    let present_changed = present != entry.present;
    if let Some(e) = state.chapters.get_mut(&(book, chapter)) {
        e.anchors = anchors;
        e.present = present;
    }
    dirty.attached.extend(entry.alignments.values().copied());
    dirty.attached.extend(entry.comments.iter().copied());
    if present_changed {
        dirty.attached.extend(entry.checks.iter().copied());
    }
    if let Some(b) = state.books.get(&book) {
        dirty.attached.extend(b.chained.iter().copied());
    }
    // Structure issues are conflicts.
    let conflicts: Vec<Conflict> = issues
        .iter()
        .filter_map(|issue| issue_conflict(state, book, chapter, &entry, issue))
        .collect();
    state.set_conflicts(Owner::Chapter(book, chapter), conflicts, dirty);
}

fn issue_conflict(
    state: &FoldState,
    book: BookId,
    chapter: u16,
    entry: &super::state::ChapterEntry,
    issue: &Issue,
) -> Option<Conflict> {
    let ch = crate::ids::ChapterNo::new(chapter)?;
    let (key, others): (RecordKey, Vec<u16>) = match issue {
        Issue::MissingFront => (RecordKey::ChapterFront(book, ch), Vec::new()),
        Issue::ShortSpan { verse, last } => (
            RecordKey::Verse(book, ch, crate::ids::VerseNo::new(*verse)?),
            (verse + 1..=*last).collect(),
        ),
        Issue::StrayCovered { verse, by } => (
            RecordKey::Verse(book, ch, crate::ids::VerseNo::new(*verse)?),
            vec![*by],
        ),
    };
    let mut keys: Vec<KeyIdx> = Vec::new();
    if let Some(k) = state.key_idx(&key) {
        keys.push(k);
    }
    // Only verses with a folded version are records of the range: a verse whose only
    // change is held in an incomplete batch has nothing to show or resolve (and filing it
    // would make the listing depend on when the held change arrived).
    for v in others {
        if let Some(k) = entry.verses.get(&v)
            && !state.entry(*k).heads.is_empty()
        {
            keys.push(*k);
        }
    }
    let mut heads: Vec<ChangeIdx> = Vec::new();
    for k in &keys {
        heads.extend(state.entry(*k).heads.iter().copied());
    }
    let ids: Vec<ChangeId> = heads.iter().map(|h| state.meta(*h).id).collect();
    let kind = ConflictKind::StructureIssue;
    let tag = format!(
        "{}\0{}",
        kind.tag().unwrap_or(""),
        crate::canon::encode_key(&key)
            .map(|j| j.to_canonical_string())
            .unwrap_or_default()
    );
    let (candidates, unreadable) = candidates_of(state, &heads, |_| None);
    let mut also_keys: Vec<RecordKey> = keys
        .iter()
        .map(|k| state.entry(*k).key.clone())
        .filter(|k| *k != key)
        .collect();
    also_keys.sort();
    let batches: BTreeSet<crate::ids::BatchId> = heads
        .iter()
        .map(|h| state.batch(state.meta(*h).batch).id)
        .collect();
    Some(Conflict {
        id: ConflictId::tagged(&tag, &ids),
        key,
        also_keys,
        kind,
        bases: Vec::new(),
        candidates,
        agreed: Vec::new(),
        disputed: Vec::new(),
        provisional: heads
            .iter()
            .copied()
            .max_by_key(|h| state.meta(*h).order())
            .map_or(ChangeId::from_bytes([0; 32]), |h| state.meta(h).id),
        batches: batches.into_iter().collect(),
        incomplete_history: continues_missing(state, &heads),
        unreadable,
    })
}

/// The candidates of `heads`, one each, and the heads whose values cannot be read, which
/// are listed apart (§2.6).
pub(crate) fn candidates_of(
    state: &FoldState,
    heads: &[ChangeIdx],
    side: impl Fn(ChangeIdx) -> Option<Side>,
) -> (Vec<Candidate>, Vec<ChangeId>) {
    let mut candidates = Vec::new();
    let mut unreadable = Vec::new();
    for h in heads {
        match candidate(state, *h, Vec::new(), side(*h)) {
            Some(c) => candidates.push(c),
            None => unreadable.push(state.meta(*h).id),
        }
    }
    unreadable.sort();
    (candidates, unreadable)
}

/// Whether a head of a conflict continues a change that has not arrived (§7.4): its
/// conflict is marked incomplete history.
pub(crate) fn continues_missing(state: &FoldState, heads: &[ChangeIdx]) -> bool {
    heads.iter().any(|h| {
        state
            .faults_of(state.meta(*h).key)
            .iter()
            .any(|(c, _, f)| c == h && *f == Fault::Missing)
    })
}

// ---------------------------------------------------------------------------------------
// Conflicts of single records
// ---------------------------------------------------------------------------------------

/// Lists or clears each changed record's own conflict, and sets the projection of records
/// outside passage groups.
pub(crate) fn stage_local(state: &mut FoldState, dirty: &mut Dirty) {
    let keys: Vec<KeyIdx> = if dirty.all {
        (0..state.keys.len() as u32).map(KeyIdx).collect()
    } else {
        dirty.changed.iter().copied().collect()
    };
    let mut profiles = false;
    for k in keys {
        if matches!(state.entry(k).key, RecordKey::DeviceProfile(_)) {
            profiles = true;
        }
        if state.entry(k).group.is_some() {
            state.set_conflicts(Owner::Key(k), Vec::new(), dirty);
            continue;
        }
        let projected = local_projection(state, k);
        state.entry_mut(k).projected = projected;
        let conflict = match &state.entry(k).local {
            Local::Conflict(c) => Some(local_conflict(state, k, c)),
            _ => None,
        };
        state.set_conflicts(Owner::Key(k), conflict.into_iter().collect(), dirty);
    }
    if profiles {
        refresh_authors(state);
    }
}

/// Lists record `k`'s own conflict again (its candidates may follow other records).
pub(crate) fn relist(state: &mut FoldState, k: KeyIdx, dirty: &mut Dirty) {
    let conflict = match &state.entry(k).local {
        Local::Conflict(c) if state.entry(k).group.is_none() => local_conflict(state, k, c),
        _ => return,
    };
    state.set_conflicts(Owner::Key(k), vec![conflict], dirty);
}

impl FoldState {
    /// The conflict list and its counts built again instead of read from the list each
    /// step keeps: every record's own conflict is built anew from its heads (its
    /// candidates' values read again), and the conflicts of passage groups, chapters, books
    /// and carried paths are gathered from their owners. A background check compares it
    /// with [`FoldState::conflicts`] and [`FoldState::conflict_counts`] (§7.8).
    pub fn conflicts_rebuilt(&self) -> (Vec<Conflict>, super::ConflictCounts) {
        let mut out: Vec<Conflict> = Vec::new();
        for (i, entry) in self.keys.iter().enumerate() {
            if let (Local::Conflict(c), None) = (&entry.local, entry.group) {
                out.push(local_conflict(self, KeyIdx(i as u32), c));
            }
        }
        for (owner, sorts) in self.owned.iter() {
            if matches!(owner, super::state::Owner::Key(_)) {
                continue;
            }
            out.extend(
                sorts
                    .iter()
                    .filter_map(|sort| self.conflicts.get(sort))
                    .map(|c| (**c).clone()),
            );
        }
        out.sort_by_key(super::state::conflict_sort);
        let mut counts = super::ConflictCounts::new();
        for conflict in &out {
            *counts
                .entry((conflict.key.book(), conflict.kind.order()))
                .or_insert(0) += 1;
        }
        (out, counts)
    }
}

fn local_conflict(state: &FoldState, k: KeyIdx, local: &super::state::LocalConflict) -> Conflict {
    let entry = state.entry(k);
    let heads: Vec<ChangeIdx> = entry.heads.to_vec();
    let anc = ancestry(state, &heads);
    let (agreed, disputed) = units_of_conflict(state, &entry.key, &heads, &anc.bases);
    let mut candidates: Vec<Candidate> = Vec::new();
    let (groups, unreadable_heads) = group_by_value(state, &heads);
    let mut unreadable: Vec<ChangeId> =
        unreadable_heads.iter().map(|h| state.meta(*h).id).collect();
    for group in groups {
        let Some(&best) = group.iter().max_by_key(|h| state.meta(**h).order()) else {
            continue;
        };
        let mut also: Vec<ChangeId> = group
            .iter()
            .filter(|h| **h != best)
            .map(|h| state.meta(*h).id)
            .collect();
        also.sort();
        match candidate(state, best, also, None) {
            Some(c) => candidates.push(c),
            None => unreadable.extend(group.iter().map(|h| state.meta(*h).id)),
        }
    }
    if local.unreadable {
        // A base the merge needed and could not read.
        unreadable.extend(
            anc.bases
                .iter()
                .filter(|b| state.bytes(**b).is_none())
                .map(|b| state.meta(*b).id),
        );
    }
    // A restore race (§7.6): a verse changed after *Start empty* is rewound for projection
    // while *Restore the text* is provisional, so it is a candidate here whichever is (its
    // `StartOver` changes are the *Start empty* candidate's).
    let mut also_keys: Vec<RecordKey> = Vec::new();
    if let RecordKey::BookPresence(book) = &entry.key {
        let race = super::books::race_batches(state, *book);
        if !race.is_empty()
            && let Some(b) = state.books.get(book)
        {
            for &v in &b.keys {
                if !matches!(state.entry(v).key, RecordKey::Verse(..)) {
                    continue;
                }
                let bad: BTreeSet<ChangeIdx> = state
                    .entry(v)
                    .changes
                    .iter()
                    .copied()
                    .filter(|x| {
                        state.meta(*x).active()
                            && race.contains(&state.batch(state.meta(*x).batch).id)
                    })
                    .collect();
                let mut listed = false;
                for h in state.entry(v).heads.iter() {
                    if bad.contains(h)
                        || !super::merge::ancestors_or_self(state, *h)
                            .iter()
                            .any(|a| bad.contains(a))
                    {
                        continue;
                    }
                    listed = true;
                    match candidate(state, *h, Vec::new(), None) {
                        Some(c) => candidates.push(c),
                        None => unreadable.push(state.meta(*h).id),
                    }
                }
                if listed {
                    also_keys.push(state.entry(v).key.clone());
                }
            }
        }
    }
    also_keys.sort();
    unreadable.sort();
    unreadable.dedup();
    let (id, provisional) = (local.id, local.provisional);
    candidates.sort_by_key(|c| std::cmp::Reverse((c.at, c.head)));
    let batches: BTreeSet<crate::ids::BatchId> = heads
        .iter()
        .map(|h| state.batch(state.meta(*h).batch).id)
        .collect();
    let incomplete_history = continues_missing(state, &heads);
    Conflict {
        id,
        key: entry.key.clone(),
        also_keys,
        kind: conflict_kind(state, &entry.key, &heads),
        bases: anc.bases.iter().map(|b| state.meta(*b).id).collect(),
        candidates,
        agreed,
        disputed,
        provisional: state.meta(provisional).id,
        batches: batches.into_iter().collect(),
        incomplete_history,
        unreadable,
    }
}

/// A candidate of a conflict: the head, its value and who made it.
pub(crate) fn candidate(
    state: &FoldState,
    head: ChangeIdx,
    also: Vec<ChangeId>,
    side: Option<Side>,
) -> Option<Candidate> {
    let meta = state.meta(head);
    let body = super::meta::decode(&state.bytes(head)?)?;
    state.count(Counter::ValuesDecoded, 1);
    Some(Candidate {
        head: meta.id,
        also,
        device: state.device_of(meta),
        author: author(state, state.device_of(meta)),
        at: meta.at,
        cause: body.cause,
        value: body.record,
        side,
    })
}

/// The name a device's profile shows: its Door43 username, else its display name.
pub(crate) fn author(state: &FoldState, device: DeviceId) -> Option<Box<str>> {
    let k = state.profile_keys.get(&device)?;
    let record = match &state.entry(*k).projected {
        Projected::Change(c) => record_of(state, *c)?,
        Projected::Merged(record) => (**record).clone(),
        Projected::Nothing => return None,
    };
    let Record::DeviceProfile {
        value:
            DeviceProfileValue {
                display_name,
                door43_username,
                ..
            },
        ..
    } = record
    else {
        return None;
    };
    door43_username.map(|u| u.as_str().into()).or(display_name)
}

/// A device profile changed: every candidate shows its author again.
fn refresh_authors(state: &mut FoldState) {
    let sorts: Vec<super::state::ConflictSort> = state.conflicts.keys().cloned().collect();
    for sort in sorts {
        let Some(conflict) = state.conflicts.get(&sort).cloned() else {
            continue;
        };
        let mut updated = (*conflict).clone();
        for c in &mut updated.candidates {
            c.author = author(state, c.device);
        }
        if updated != *conflict {
            state.conflicts.insert(sort, Arc::new(updated));
        }
    }
}

// ---------------------------------------------------------------------------------------
// Placement and Detached (§7.7)
// ---------------------------------------------------------------------------------------

/// Where an attached record is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    /// At the run starting at this verse.
    Run(u16, u16),
    /// Nowhere: listed under Detached.
    Nowhere,
}

/// Places an anchor in a book (§7.7 rules 1 to 3), and says whether it followed a
/// `moved_to`.
pub(crate) fn place(state: &FoldState, book: BookId, anchor: Anchor) -> (Place, bool) {
    let mut at = anchor;
    let mut chained = false;
    for _ in 0..=64 {
        if let Some(set) = state.anchors.get(&(book, at.0, at.1))
            && let Some(first) = set.iter().next()
        {
            return (Place::Run(first.0, first.1), chained);
        }
        let Some(entry) = state.chapters.get(&(book, at.0)) else {
            return (Place::Nowhere, chained);
        };
        let Some(&k) = entry.verses.get(&at.1) else {
            return (Place::Nowhere, chained);
        };
        let rewound = Vec::new();
        match shown_verse(state, k, &rewound).map(|s| s.shape()) {
            Some(VerseShape::Covered { by }) if entry.present.contains(&at.1) => {
                return (Place::Run(at.0, by), chained);
            }
            Some(VerseShape::Absent {
                moved_to: Some(next),
            }) => {
                chained = true;
                at = (next.chapter.get(), next.verse.get());
            }
            _ => return (Place::Nowhere, chained),
        }
    }
    (Place::Nowhere, chained)
}

/// Whether a check's target verse exists in the projection (a number that is a text or
/// covered by a run; a target that is not a number always does).
pub(crate) fn target_exists(state: &FoldState, check: &CheckKey) -> bool {
    let Some(book) = check.book() else {
        return true;
    };
    match target_verse(check) {
        Some((c, v)) => state
            .chapters
            .get(&(book, c))
            .is_some_and(|e| e.present.contains(&v)),
        None => true,
    }
}

/// The projected pins, for `resolution(tool, collection)` (§7.7).
#[derive(Debug, Clone, Default)]
pub(crate) struct PinsView {
    pub groups: BTreeMap<PinGroup, PinGroupValue>,
    /// `resolution(tool, collection)` of every tool and collection, by [`resolved_slot`].
    resolved: Vec<Option<Answer>>,
}

const TOOLS: [ToolId; 3] = [
    ToolId::TranslationNotes,
    ToolId::TranslationWords,
    ToolId::TranslationQuestions,
];

/// A collection's place in the table: the books by ordinal, then the stories.
fn collection_slot(collection: Collection) -> usize {
    match collection {
        Collection::Book(book) => usize::from(book.ordinal()).saturating_sub(1),
        Collection::Stories => BookId::ALL.len(),
    }
}

fn resolved_slot(tool: ToolId, collection: Collection) -> usize {
    let tool = match tool {
        ToolId::TranslationNotes => 0,
        ToolId::TranslationWords => 1,
        ToolId::TranslationQuestions => 2,
    };
    tool * (BookId::ALL.len() + 1) + collection_slot(collection)
}

impl PinsView {
    pub(crate) fn of(state: &FoldState) -> Self {
        let mut groups = BTreeMap::new();
        for k in state.pin_keys.iter() {
            let record = match &state.entry(*k).projected {
                Projected::Change(c) => record_of(state, *c),
                Projected::Merged(record) => Some((**record).clone()),
                Projected::Nothing => None,
            };
            if let Some(Record::Pins { group, value }) = record {
                groups.insert(group, value);
            }
        }
        let sets = LanguageSets {
            primary: groups.get(&PinGroup::Primary).cloned(),
            fallback: groups.get(&PinGroup::Fallback).cloned(),
        };
        let collections = BookId::ALL
            .into_iter()
            .map(Collection::Book)
            .chain([Collection::Stories]);
        let mut resolved = vec![None; TOOLS.len() * (BookId::ALL.len() + 1)];
        for tool in TOOLS {
            for collection in collections.clone() {
                if let Some(slot) = resolved.get_mut(resolved_slot(tool, collection)) {
                    *slot = sets.resolution(tool, collection);
                }
            }
        }
        Self { groups, resolved }
    }

    /// `resolution(tool, collection)`, from the table [`PinsView::of`] fills: where tC4's D30
    /// ladder stops over the projected pins alone ([`Ladder::resolution`]).
    pub(crate) fn answer(&self, tool: ToolId, collection: Collection) -> Option<&Answer> {
        self.resolved.get(resolved_slot(tool, collection))?.as_ref()
    }

    /// The usable pin `resolution(tool, collection)` names, which a decision must record.
    pub(crate) fn resolved(&self, tool: ToolId, collection: Collection) -> Option<&ResourceRef> {
        self.answer(tool, collection).and_then(Answer::resource)
    }

    /// The current original-language pin's sha for a book's testament.
    pub(crate) fn original_sha(&self, book: BookId) -> Option<crate::ids::GitOid> {
        let value = self.groups.get(&PinGroup::OriginalLanguage)?;
        let slot = match book.testament() {
            tc_core::Testament::Old => "ot",
            tc_core::Testament::New => "nt",
        };
        value
            .slot(slot)?
            .as_obj()?
            .get("sha")
            .and_then(Json::as_str)
            .and_then(crate::ids::GitOid::parse)
    }
}

pub(crate) fn stage_attached(state: &mut FoldState, dirty: &mut Dirty) {
    if dirty.pins {
        state.pins = Arc::new(PinsView::of(state));
    }
    if dirty.pins && !dirty.all {
        let books = state.books.values().flat_map(|b| b.keys.iter().copied());
        for k in books.chain(state.story_attached()) {
            if matches!(state.entry(k).key, RecordKey::Check(_)) {
                dirty.attached.insert(k);
            }
        }
    }
    if !dirty.all {
        // A story's text decides where its comments are placed.
        let stories: BTreeSet<StoryNo> = dirty
            .changed
            .iter()
            .filter_map(|k| match state.entry(*k).key {
                RecordKey::StoryBase(story) | RecordKey::Frame(story, _) => Some(story),
                _ => None,
            })
            .collect();
        for story in stories {
            if let Some(keys) = state.story_attached.get(&story) {
                dirty.attached.extend(keys.iter().copied());
            }
        }
    }
    let keys: Vec<KeyIdx> = if dirty.all {
        (0..state.keys.len() as u32)
            .map(KeyIdx)
            .filter(|k| {
                matches!(
                    state.entry(*k).key,
                    RecordKey::Alignment(..) | RecordKey::Check(_) | RecordKey::Comment(_)
                )
            })
            .collect()
    } else {
        dirty.attached.iter().copied().collect()
    };
    // Placing reads the state alone: a fold from scratch places its hundreds of thousands
    // of attached records on several threads, then files what they found.
    let found: Vec<(KeyIdx, Attached)> = if dirty.all {
        let state: &FoldState = state;
        let chunks: Vec<&[KeyIdx]> = keys.chunks(4096).collect();
        super::project::on_threads(&chunks, |chunk| {
            chunk
                .iter()
                .map(|k| (*k, attached(state, *k)))
                .collect::<Vec<_>>()
        })
        .into_iter()
        .flatten()
        .collect()
    } else {
        keys.iter().map(|k| (*k, attached(state, *k))).collect()
    };
    for (k, found) in found {
        if let Some((book, chained)) = found.chained {
            set_chained(state, book, k, chained);
        }
        if found.detached {
            let key = state.entry(k).key.clone();
            state.detached.insert(key);
        } else if !dirty.all {
            let key = state.entry(k).key.clone();
            state.detached.remove(&key);
        }
    }
}

/// What placing an attached record found (§7.7).
struct Attached {
    /// Listed under Detached.
    detached: bool,
    /// The book whose `moved_to` chains it was placed through, and whether it was.
    chained: Option<(BookId, bool)>,
}

fn projects(state: &FoldState, book: Option<BookId>) -> bool {
    book.is_some_and(|b| state.books.get(&b).is_some_and(|e| e.status.projects()))
}

/// Whether record `k` is listed under Detached: an alignment, comment or decision of a
/// projected book that has no place on any screen and was not dismissed.
fn attached(state: &FoldState, k: KeyIdx) -> Attached {
    let mut chained = None;
    let detached = detached(state, k, &mut chained);
    Attached { detached, chained }
}

fn detached(state: &FoldState, k: KeyIdx, chained_in: &mut Option<(BookId, bool)>) -> bool {
    state.count(Counter::AttachedPlaced, 1);
    let entry = state.entry(k);
    let shape = match &entry.projected {
        Projected::Change(c) => state.shape(*c),
        Projected::Merged(record) => merged_shape(record),
        Projected::Nothing => return false,
    };
    let key = entry.key.clone();
    match &key {
        RecordKey::Alignment(book, c, v) => {
            if !projects(state, Some(*book))
                || matches!(shape, Shape::Alignment { dismissed: true })
            {
                return false;
            }
            let (place, chained) = place(state, *book, (c.get(), v.get()));
            *chained_in = Some((*book, chained));
            place == Place::Nowhere
        }
        RecordKey::Check(check) => {
            let Shape::Decision {
                resource,
                dismissed,
            } = shape
            else {
                return false;
            };
            if dismissed {
                return false;
            }
            let collection = check.target.collection();
            if let Collection::Book(book) = collection
                && !projects(state, Some(book))
            {
                return false;
            }
            let resolved = state.pins.resolved(check.tool, collection);
            !resolved.is_some_and(|r| r.same_resource(&resource)) || !target_exists(state, check)
        }
        RecordKey::Comment(_) => {
            let Shape::Comment { target } = shape else {
                return false;
            };
            match &*target {
                CommentTarget::Verse {
                    book,
                    chapter,
                    verse,
                } => {
                    if !projects(state, Some(*book)) {
                        return false;
                    }
                    let (place, chained) = place(state, *book, (chapter.get(), verse.get()));
                    *chained_in = Some((*book, chained));
                    place == Place::Nowhere
                }
                CommentTarget::Check(check) => {
                    let live = follow_moves(state, check);
                    match live.target {
                        CheckTarget::Frame { story, frame } => {
                            state.frame_text(story, frame).is_none()
                        }
                        CheckTarget::Verse { .. } => {
                            projects(state, check.book()) && !target_exists(state, &live)
                        }
                    }
                }
                // Placed where `tc/comments/OBS.json` places it: at a frame with a text.
                CommentTarget::Frame { story, frame } => state.frame_text(*story, *frame).is_none(),
            }
        }
        _ => false,
    }
}

fn set_chained(state: &mut FoldState, book: BookId, k: KeyIdx, chained: bool) {
    let has = state
        .books
        .get(&book)
        .is_some_and(|b| b.chained.contains(&k));
    if has != chained
        && let Some(entry) = state.book_mut(book)
    {
        if chained {
            entry.chained.insert(k);
        } else {
            entry.chained.remove(&k);
        }
    }
}

/// Follows `MovedTo` from a check key to the key its decision lives at now (at most 64
/// steps, §7.7).
pub(crate) fn follow_moves(state: &FoldState, check: &CheckKey) -> CheckKey {
    let mut at = check.clone();
    for _ in 0..64 {
        let Some(k) = state.key_idx(&RecordKey::Check(at.clone())) else {
            break;
        };
        let next = match &state.entry(k).projected {
            Projected::Change(c) => match state.shape(*c) {
                Shape::MovedTo { to } => Some((*to).clone()),
                _ => None,
            },
            Projected::Merged(record) => match &**record {
                Record::Check {
                    value: CheckValue::MovedTo { to },
                    ..
                } => Some(to.clone()),
                _ => None,
            },
            Projected::Nothing => None,
        };
        match next {
            Some(to) => at = to,
            None => break,
        }
    }
    at
}

fn merged_shape(record: &Record) -> Shape {
    match record {
        Record::Alignment { value, .. } => Shape::Alignment {
            dismissed: value.dismissed,
        },
        Record::Check {
            value: CheckValue::Decision(d),
            ..
        } => Shape::Decision {
            resource: Arc::new(d.resource.clone()),
            dismissed: d.dismissed,
        },
        Record::Check {
            value: CheckValue::MovedTo { to },
            ..
        } => Shape::MovedTo {
            to: Arc::new(to.clone()),
        },
        Record::Comment { value, .. } => Shape::Comment {
            target: Arc::new(value.target.clone()),
        },
        _ => Shape::Other,
    }
}

// ---------------------------------------------------------------------------------------
// Paths (§3.3)
// ---------------------------------------------------------------------------------------

/// The derived paths of a book that projects.
pub(crate) fn book_paths(book: BookId) -> Vec<String> {
    let code = book.code();
    vec![
        format!("ingredients/{code}.usfm"),
        format!("ingredients/checking/alignments/{code}.json"),
        format!("ingredients/checking/translationNotes/{code}.json"),
        format!("ingredients/checking/translationWords/{code}.json"),
        format!("ingredients/checking/translationQuestions/{code}.json"),
        format!("ingredients/tc/comments/{code}.json"),
    ]
}

/// The path a carried file with this oid takes when it collides (§3.3).
pub(crate) fn moved_path(path: &str, oid: &crate::ids::GitOid, infix: &str) -> String {
    let short = &oid.to_hex()[..8];
    let (dir, name) = match path.rsplit_once('/') {
        Some((dir, name)) => (Some(dir), name),
        None => (None, path),
    };
    let renamed = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem}.{infix}-{short}.{ext}"),
        _ => format!("{name}.{infix}-{short}"),
    };
    match dir {
        Some(dir) => format!("{dir}/{renamed}"),
        None => renamed,
    }
}

/// PathCase conflicts and the places colliding carried files take.
pub(crate) fn stage_paths(state: &mut FoldState, dirty: &mut Dirty) {
    if !dirty.paths {
        return;
    }
    let mut derived: BTreeMap<String, String> = BTreeMap::new();
    for (book, entry) in state.books.iter() {
        if entry.status.projects() {
            for path in book_paths(*book) {
                derived.insert(tc_burrito::collision_form(&path), path);
            }
        }
    }
    for path in [
        "ingredients/vrs.json",
        "ingredients/checking/resources.json",
        "ingredients/checking/settings.json",
        "metadata.json",
    ] {
        derived.insert(tc_burrito::collision_form(path), path.to_owned());
    }
    // Carried files with a present head, by collision form. Any present head counts, not
    // only the provisional one, so whether two paths collide never reads a clock (I3).
    let mut carried: BTreeMap<String, Vec<(RepoPath, KeyIdx, Option<crate::ids::GitOid>)>> =
        BTreeMap::new();
    for k in state.carried_keys.iter() {
        let RecordKey::CarriedFile(path) = &state.entry(*k).key else {
            continue;
        };
        let present = state
            .entry(*k)
            .heads
            .iter()
            .any(|h| matches!(state.shape(*h), Shape::Carried { oid: Some(_) }));
        if !present {
            continue;
        }
        let shown = match &state.entry(*k).projected {
            Projected::Change(c) => match state.shape(*c) {
                Shape::Carried { oid: Some(oid) } => Some(oid),
                _ => None,
            },
            _ => None,
        };
        carried
            .entry(path.collision_form())
            .or_default()
            .push((path.clone(), *k, shown));
    }
    let mut moves: BTreeMap<RepoPath, String> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for (form, mut files) in carried {
        let derived_here = derived.contains_key(&form);
        if files.len() < 2 && !derived_here {
            continue;
        }
        files.sort_by(|a, b| a.0.as_str().as_bytes().cmp(b.0.as_str().as_bytes()));
        let keep = if derived_here { 0 } else { 1 };
        let moved: Vec<&(RepoPath, KeyIdx, Option<crate::ids::GitOid>)> =
            files.iter().skip(keep).collect();
        for (path, _, oid) in &moved {
            if let Some(oid) = oid {
                moves.insert(path.clone(), moved_path(path.as_str(), oid, "case"));
            }
        }
        let keys: Vec<KeyIdx> = files.iter().map(|f| f.1).collect();
        let mut heads: Vec<ChangeIdx> = Vec::new();
        for k in &keys {
            heads.extend(state.entry(*k).heads.iter().copied());
        }
        let ids: Vec<ChangeId> = heads.iter().map(|h| state.meta(*h).id).collect();
        let Some(first) = moved.first() else {
            continue;
        };
        let key = RecordKey::CarriedFile(first.0.clone());
        let mut also_keys: Vec<RecordKey> = files
            .iter()
            .map(|f| RecordKey::CarriedFile(f.0.clone()))
            .filter(|k| *k != key)
            .collect();
        also_keys.sort();
        let tag = format!("pathCase\0{form}");
        let (candidates, unreadable) = candidates_of(state, &heads, |_| None);
        conflicts.push(Conflict {
            id: ConflictId::tagged(&tag, &ids),
            key,
            also_keys,
            kind: ConflictKind::PathCase,
            bases: Vec::new(),
            candidates,
            agreed: Vec::new(),
            disputed: Vec::new(),
            provisional: heads
                .iter()
                .copied()
                .max_by_key(|h| state.meta(*h).order())
                .map_or(ChangeId::from_bytes([0; 32]), |h| state.meta(h).id),
            batches: heads
                .iter()
                .map(|h| state.batch(state.meta(*h).batch).id)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
            incomplete_history: continues_missing(state, &heads),
            unreadable,
        });
    }
    state.case_moves = Arc::new(moves);
    state.set_conflicts(Owner::Paths, conflicts, dirty);
}
