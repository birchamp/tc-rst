//! Choices (§8.3): each one is a function from the fold and a person's choice to the
//! changes that resolve the conflict, one per record, each naming every current head of
//! its record as parents (§5.9). A resolution needs no ancestry condition, so every
//! conflict can always be resolved (§8.4).

use super::merge::{Unit, ancestors_or_self, record_of, units_for, value_of};
use super::meta::{ChangeIdx, KeyIdx, VerseShape};
use super::passage::rewind;
use super::state::FoldState;
use super::{Candidate, Conflict, ConflictId, ConflictKind, Side, UnitName};
use crate::change::Cause;
use crate::command::{Choice, PlannedChange};
use crate::ids::{ChangeId, ChapterNo, DeviceId, RepoPath, VerseNo};
use crate::json::Json;
use crate::keys::RecordKey;
use crate::record::{
    BookPresence, CarriedValue, ChapterFront, CheckValue, CommentValue, PresentHow, Record, Scope,
    UsfmBytes, VerseValue,
};
use std::collections::{BTreeMap, BTreeSet};
use tc_core::BookId;

/// Why a choice cannot be made into changes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("the conflict is no longer listed; look again")]
    UnknownConflict,
    #[error("the chosen version is not one of the conflict's versions")]
    NotACandidate,
    #[error("{0:?} is not a record of this conflict")]
    NotInConflict(RecordKey),
    #[error("the choice gives no value for {0:?}")]
    MissingValue(RecordKey),
    #[error("this choice does not apply to a conflict of this kind")]
    NotOffered,
    /// A version of `0` could not be read on this computer (an unreadable file): the
    /// person has not seen it, so no choice may replace it yet (§2.6).
    #[error("a version of {0:?} cannot be read on this computer yet")]
    Unreadable(RecordKey),
}

impl FoldState {
    fn listed(&self, id: &ConflictId) -> Result<&Conflict, ResolveError> {
        self.conflict(id).ok_or(ResolveError::UnknownConflict)
    }

    fn idx_of(&self, key: &RecordKey) -> Option<KeyIdx> {
        self.key_idx(key)
    }

    /// Every record a conflict is about: its key and its other keys.
    fn conflict_keys(conflict: &Conflict) -> Vec<RecordKey> {
        std::iter::once(conflict.key.clone())
            .chain(conflict.also_keys.iter().cloned())
            .collect()
    }

    /// One resolving change: `record` with every current head of its record as parents.
    fn resolving(&self, record: Record) -> PlannedChange {
        let parents = self.heads(&record.key());
        PlannedChange {
            record,
            parents,
            cause: Cause::Resolve,
        }
    }

    /// The changes a choice writes (§8.3): *Keep this version* on a candidate (and its
    /// side, for a grouped conflict), or values the person edited, combined or chose unit by
    /// unit, one per record of the conflict.
    pub fn resolution(
        &self,
        id: &ConflictId,
        choice: &Choice,
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?.clone();
        match choice {
            Choice::Values(records) => {
                let keys: BTreeSet<RecordKey> =
                    Self::conflict_keys(&conflict).into_iter().collect();
                for record in records {
                    if !keys.contains(&record.key()) {
                        return Err(ResolveError::NotInConflict(record.key()));
                    }
                }
                Ok(records.iter().cloned().map(|r| self.resolving(r)).collect())
            }
            Choice::Candidate { head } => {
                let candidate = conflict
                    .candidates
                    .iter()
                    .find(|c| c.head == *head || c.also.contains(head))
                    .cloned()
                    .ok_or(ResolveError::NotACandidate)?;
                self.keep_candidate(&conflict, &candidate)
            }
        }
    }

    fn keep_candidate(
        &self,
        conflict: &Conflict,
        candidate: &Candidate,
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        match conflict.kind {
            ConflictKind::Passage => {
                let head = self
                    .change_idx(&candidate.head)
                    .ok_or(ResolveError::NotACandidate)?;
                self.keep_side(conflict, candidate.side.unwrap_or(Side::Others), &[head])
            }
            ConflictKind::StructureIssue => self.keep_structure(conflict, candidate),
            ConflictKind::BookRemovedWhileEdited { .. } => match &candidate.value {
                Record::BookPresence {
                    value: BookPresence::Removed { .. },
                    book,
                } => Ok(vec![self.keep_removed(*book)]),
                _ => self.restore_book(conflict.id),
            },
            ConflictKind::LastBookRemoved => self.restore_book(conflict.id),
            ConflictKind::PathCase => match &candidate.value {
                // Keeping one file at its place removes nothing: the others are renamed by
                // a person. Keeping a candidate here means removing every other file.
                Record::CarriedFile { path, .. } => Ok(Self::conflict_keys(conflict)
                    .into_iter()
                    .filter_map(|k| match k {
                        RecordKey::CarriedFile(p) if p != *path => {
                            Some(self.resolving(Record::CarriedFile {
                                path: p,
                                value: CarriedValue::Removed,
                            }))
                        }
                        _ => None,
                    })
                    .collect()),
                _ => Err(ResolveError::NotOffered),
            },
            _ => Ok(vec![self.resolving(candidate.value.clone())]),
        }
    }

    // ---------------------------------------------------------------------------------
    // Passages
    // ---------------------------------------------------------------------------------

    /// *Keep Maria's passage* (§8.3) for a person who picked `heads`: the first names the
    /// side, and each picks the version of its own record (of any side: a record the kept
    /// side did not write may hold two versions of the others). Every record of the group takes
    /// its value from that side, with every other passage side rewound; a record the side did
    /// not have before becomes absent. A record for which the side still holds several
    /// different versions and no pick names one is left to the person
    /// ([`ResolveError::MissingValue`]): no clock chooses what is kept (I3).
    pub fn keep_passage(
        &self,
        id: &ConflictId,
        heads: &[ChangeId],
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        if conflict.kind != ConflictKind::Passage {
            return Err(ResolveError::NotOffered);
        }
        let mut side = None;
        let mut chosen = Vec::new();
        for head in heads {
            let candidate = conflict
                .candidates
                .iter()
                .find(|c| c.head == *head || c.also.contains(head))
                .ok_or(ResolveError::NotACandidate)?;
            side.get_or_insert(candidate.side.unwrap_or(Side::Others));
            chosen.push(self.change_idx(head).ok_or(ResolveError::NotACandidate)?);
        }
        let side = side.ok_or(ResolveError::NotACandidate)?;
        self.keep_side(conflict, side, &chosen)
    }

    fn keep_side(
        &self,
        conflict: &Conflict,
        side: Side,
        chosen: &[ChangeIdx],
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let sides: BTreeSet<Side> = conflict.candidates.iter().filter_map(|c| c.side).collect();
        let losing: BTreeSet<crate::ids::BatchId> = sides
            .iter()
            .filter_map(|s| match s {
                Side::Batch(b) if *s != side => Some(*b),
                _ => None,
            })
            .collect();
        let mut out = Vec::new();
        for key in Self::conflict_keys(conflict) {
            let Some(k) = self.idx_of(&key) else {
                continue;
            };
            let heads: Vec<ChangeIdx> = self.entry(k).heads.to_vec();
            let changes: Vec<ChangeIdx> = self
                .entry(k)
                .changes
                .iter()
                .copied()
                .filter(|c| self.meta(*c).active)
                .collect();
            let bad: BTreeSet<ChangeIdx> = changes
                .iter()
                .copied()
                .filter(|c| losing.contains(&self.batch(self.meta(*c).batch).id))
                .collect();
            let candidates = rewind(self, &heads, &bad);
            let preferred: Vec<ChangeIdx> = match side {
                Side::Batch(w) => candidates
                    .iter()
                    .copied()
                    .filter(|c| {
                        ancestors_or_self(self, *c)
                            .iter()
                            .any(|a| self.batch(self.meta(*a).batch).id == w)
                    })
                    .collect(),
                Side::Others => candidates
                    .iter()
                    .copied()
                    .filter(|c| {
                        !ancestors_or_self(self, *c).iter().any(|a| {
                            let b = self.meta(*a).batch;
                            sides.contains(&Side::Batch(self.batch(b).id))
                        })
                    })
                    .collect(),
            };
            let pool = if preferred.is_empty() {
                &candidates
            } else {
                &preferred
            };
            let picked: Vec<ChangeIdx> = chosen
                .iter()
                .copied()
                .filter(|c| self.meta(*c).key == k)
                .collect();
            let record = match picked.as_slice() {
                [] => match self.one_value(&key, pool)? {
                    Some(record) => record,
                    None => match &key {
                        RecordKey::Verse(book, chapter, verse) => Record::Verse {
                            book: *book,
                            chapter: *chapter,
                            verse: *verse,
                            value: VerseValue::Absent { moved_to: None },
                        },
                        other => return Err(ResolveError::MissingValue(other.clone())),
                    },
                },
                // The version the person picked, exactly, whichever side it also descends
                // from.
                [one] => {
                    record_of(self, *one).ok_or_else(|| ResolveError::Unreadable(key.clone()))?
                }
                _ => return Err(ResolveError::NotACandidate),
            };
            out.push(self.resolving(record));
        }
        Ok(out)
    }

    /// The one value the versions of `pool` share: `None` when the pool is empty, and
    /// [`ResolveError::MissingValue`] when they differ, for the person to choose.
    fn one_value(
        &self,
        key: &RecordKey,
        pool: &[ChangeIdx],
    ) -> Result<Option<Record>, ResolveError> {
        let mut found: Option<Record> = None;
        for c in pool {
            let record =
                record_of(self, *c).ok_or_else(|| ResolveError::Unreadable(key.clone()))?;
            match &found {
                Some(seen) if *seen != record => {
                    return Err(ResolveError::MissingValue(key.clone()));
                }
                Some(_) => {}
                None => found = Some(record),
            }
        }
        Ok(found)
    }

    // ---------------------------------------------------------------------------------
    // Structure
    // ---------------------------------------------------------------------------------

    fn verse_record(book: BookId, chapter: ChapterNo, verse: VerseNo, value: VerseValue) -> Record {
        Record::Verse {
            book,
            chapter,
            verse,
            value,
        }
    }

    /// The latest text a verse had as a verse of its own (its `last` is itself), else its
    /// latest text: what *Keep separate* writes back (the base before a join). "Latest" is
    /// by ancestry: a text that another one continues is never chosen, so a slow clock
    /// cannot bring back the words a later edit replaced. Concurrent latest texts that
    /// differ are the person's to choose ([`ResolveError::MissingValue`]); no clock picks
    /// one (I3).
    fn own_text(&self, k: KeyIdx, verse: u16) -> Result<Option<VerseValue>, ResolveError> {
        let key = self.entry(k).key.clone();
        let mut seen: BTreeSet<ChangeIdx> = BTreeSet::new();
        for h in self.entry(k).heads.iter() {
            seen.extend(ancestors_or_self(self, *h));
        }
        let texts: Vec<ChangeIdx> = seen
            .into_iter()
            .filter(|c| matches!(self.meta(*c).verse(), Some(VerseShape::Text { .. })))
            .collect();
        let latest = |pool: Vec<ChangeIdx>| -> Vec<ChangeIdx> {
            let mut below: BTreeSet<ChangeIdx> = BTreeSet::new();
            for c in &pool {
                for p in super::merge::key_parents(self, *c) {
                    below.extend(ancestors_or_self(self, p));
                }
            }
            pool.into_iter().filter(|c| !below.contains(c)).collect()
        };
        let own = latest(
            texts
                .iter()
                .copied()
                .filter(|c| matches!(self.meta(*c).verse(), Some(VerseShape::Text { last, .. }) if last == verse))
                .collect(),
        );
        let pool = if own.is_empty() { latest(texts) } else { own };
        let mut found: Option<VerseValue> = None;
        for c in pool {
            let record = record_of(self, c).ok_or_else(|| ResolveError::Unreadable(key.clone()))?;
            let Some(value) = Self::as_verse(record, verse) else {
                continue;
            };
            match &found {
                Some(seen) if *seen != value => return Err(ResolveError::MissingValue(key)),
                Some(_) => {}
                None => found = Some(value),
            }
        }
        Ok(found)
    }

    /// A verse record's text, written as a verse of its own numbered `verse`.
    fn as_verse(record: Record, verse: u16) -> Option<VerseValue> {
        match record {
            Record::Verse {
                value:
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        from,
                        ..
                    },
                ..
            } => Some(VerseValue::Text {
                marker,
                words,
                layout,
                last: VerseNo::new(verse)?,
                from,
            }),
            _ => None,
        }
    }

    /// The versions a person picked in `heads`, by record: each a folded version of a
    /// record of the conflict, a head or an earlier version from its History (the text a
    /// verse had before a join may be two concurrent texts that a later change continued).
    fn picks(
        &self,
        conflict: &Conflict,
        heads: &[ChangeId],
    ) -> Result<BTreeMap<RecordKey, ChangeIdx>, ResolveError> {
        let keys: BTreeSet<RecordKey> = Self::conflict_keys(conflict).into_iter().collect();
        let mut out = BTreeMap::new();
        for head in heads {
            let c = self.change_idx(head).ok_or(ResolveError::NotACandidate)?;
            let k = self.meta(c).key;
            let key = self.entry(k).key.clone();
            if !keys.contains(&key) || !self.meta(c).active {
                return Err(ResolveError::NotACandidate);
            }
            if out.insert(key, c).is_some() {
                return Err(ResolveError::NotACandidate);
            }
        }
        Ok(out)
    }

    /// *Keep 4 and 5 separate* (§8.3): every verse of the range a text of its own, with the
    /// words it had before the join, or the text of the version the person picked for it
    /// in `heads` (needed where a verse has several such texts).
    pub fn keep_separate(
        &self,
        id: &ConflictId,
        heads: &[ChangeId],
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        let picks = self.picks(conflict, heads)?;
        let mut out = Vec::new();
        for key in Self::conflict_keys(conflict) {
            let RecordKey::Verse(book, chapter, verse) = &key else {
                if let RecordKey::ChapterFront(book, chapter) = &key {
                    out.push(self.resolving(Self::default_front(*book, *chapter)));
                }
                continue;
            };
            let picked = match picks.get(&key) {
                Some(c) => {
                    let record =
                        record_of(self, *c).ok_or_else(|| ResolveError::Unreadable(key.clone()))?;
                    Some(Self::as_verse(record, verse.get()).ok_or(ResolveError::NotOffered)?)
                }
                None => match self.idx_of(&key) {
                    Some(k) => self.own_text(k, verse.get())?,
                    None => None,
                },
            };
            let value = picked.unwrap_or(VerseValue::Absent { moved_to: None });
            out.push(self.resolving(Self::verse_record(*book, *chapter, *verse, value)));
        }
        Ok(out)
    }

    /// *Keep the joined verses* (§8.3): one verse of the range keeps a span text and the
    /// verses after it are covered by it. The span is the first span the person picked in
    /// `heads`, at its own verse; else the one version of the range's first verse that
    /// spans; else that verse's own text, spanning the range. A verse of the range before
    /// the span keeps its own text, or the text picked for it in `heads`, as a verse of its
    /// own.
    pub fn keep_joined(
        &self,
        id: &ConflictId,
        heads: &[ChangeId],
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        let picks = self.picks(conflict, heads)?;
        let mut verses: Vec<(BookId, ChapterNo, VerseNo)> = Self::conflict_keys(conflict)
            .iter()
            .filter_map(|k| match k {
                RecordKey::Verse(b, c, v) => Some((*b, *c, *v)),
                _ => None,
            })
            .collect();
        verses.sort();
        let Some(&(book, chapter, first)) = verses.first() else {
            return Err(ResolveError::NotOffered);
        };
        let text_of = |key: &RecordKey, c: ChangeIdx| -> Result<Option<VerseValue>, ResolveError> {
            let record = record_of(self, c).ok_or_else(|| ResolveError::Unreadable(key.clone()))?;
            Ok(match record {
                Record::Verse {
                    value: value @ VerseValue::Text { .. },
                    ..
                } => Some(value),
                _ => None,
            })
        };
        let spans = |v: VerseNo, value: &VerseValue| matches!(value, VerseValue::Text { last, .. } if *last > v);
        // The first picked span, at its own verse; later picks are texts of verses of
        // their own.
        let mut start: Option<(VerseNo, VerseValue)> = None;
        for head in heads {
            let Some(c) = self.change_idx(head) else {
                continue;
            };
            let key = self.entry(self.meta(c).key).key.clone();
            let RecordKey::Verse(_, _, v) = key else {
                continue;
            };
            if let Some(value) = text_of(&key, c)?
                && spans(v, &value)
            {
                start = Some((v, value));
                break;
            }
        }
        let first_key = RecordKey::Verse(book, chapter, first);
        let (start, text) = match start {
            Some(found) => found,
            None => {
                let text = match picks.get(&first_key) {
                    Some(c) => text_of(&first_key, *c)?.ok_or(ResolveError::NotOffered)?,
                    None => {
                        let k = self.idx_of(&first_key).ok_or(ResolveError::NotOffered)?;
                        let mut found: Vec<VerseValue> = Vec::new();
                        for h in self.entry(k).heads.iter() {
                            if let Some(value) = text_of(&first_key, *h)?
                                && spans(first, &value)
                                && !found.contains(&value)
                            {
                                found.push(value);
                            }
                        }
                        match found.len() {
                            0 => self
                                .own_text(k, first.get())?
                                .ok_or(ResolveError::NotOffered)?,
                            1 => found.remove(0),
                            _ => return Err(ResolveError::MissingValue(first_key.clone())),
                        }
                    }
                };
                (first, text)
            }
        };
        let VerseValue::Text {
            marker,
            words,
            layout,
            from,
            last: span_last,
        } = text
        else {
            return Err(ResolveError::NotOffered);
        };
        let last = verses
            .iter()
            .map(|(_, _, v)| *v)
            .max()
            .unwrap_or(start)
            .max(span_last);
        let mut out = Vec::new();
        for (b, c, v) in verses {
            let key = RecordKey::Verse(b, c, v);
            let value = if v == start {
                VerseValue::Text {
                    marker: marker.clone(),
                    words: words.clone(),
                    layout: layout.clone(),
                    last,
                    from,
                }
            } else if v > start {
                VerseValue::Covered { by: start }
            } else {
                // Before the kept span: the verse's own text, or the version picked.
                let own = match picks.get(&key) {
                    Some(p) => text_of(&key, *p)?
                        .and_then(|t| Self::as_verse(Self::verse_record(b, c, v, t), v.get())),
                    None => match self.idx_of(&key) {
                        Some(k) => self.own_text(k, v.get())?,
                        None => None,
                    },
                };
                own.unwrap_or(VerseValue::Absent { moved_to: None })
            };
            out.push(self.resolving(Self::verse_record(b, c, v, value)));
        }
        Ok(out)
    }

    fn default_front(book: BookId, chapter: ChapterNo) -> Record {
        Record::ChapterFront {
            book,
            chapter,
            value: ChapterFront::Present(UsfmBytes(format!("\\c {}\n", chapter.get()))),
        }
    }

    fn keep_structure(
        &self,
        conflict: &Conflict,
        candidate: &Candidate,
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        if let RecordKey::ChapterFront(book, chapter) = &conflict.key {
            return Ok(vec![self.resolving(Self::default_front(*book, *chapter))]);
        }
        match &candidate.value {
            Record::Verse {
                value: VerseValue::Text { last, .. },
                verse,
                ..
            } if last > verse => self.keep_joined(&conflict.id, &[candidate.head]),
            // A text of its own is the version kept for its verse; a covered or absent
            // verse picks nothing, and each verse keeps its own text.
            Record::Verse {
                value: VerseValue::Text { .. },
                ..
            } => self.keep_separate(&conflict.id, &[candidate.head]),
            _ => self.keep_separate(&conflict.id, &[]),
        }
    }

    // ---------------------------------------------------------------------------------
    // Books
    // ---------------------------------------------------------------------------------

    /// *Keep it removed* (§8.3): a removal that saw every current head of every record of
    /// the book, so the unseen work is seen and stays in History.
    pub fn keep_removed(&self, book: BookId) -> PlannedChange {
        let mut seen: Vec<crate::ids::ShortId> = Vec::new();
        if let Some(entry) = self.books.get(&book) {
            for k in &entry.keys {
                for h in self.entry(*k).heads.iter() {
                    seen.push(self.meta(*h).id.short());
                }
            }
        }
        seen.sort();
        seen.dedup();
        self.resolving(Record::BookPresence {
            book,
            value: BookPresence::Removed { seen },
        })
    }

    /// *Restore Jonah* or *Keep Titus* (§8.3): the book present again, with the scope of
    /// its nearest `Present`, restoring its text.
    pub fn restore_book(&self, id: ConflictId) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(&id)?;
        let RecordKey::BookPresence(book) = conflict.key else {
            return Err(ResolveError::NotOffered);
        };
        let scope = self
            .book_scope_change(book)
            .and_then(|c| match self.body(&c).map(|b| b.record) {
                Some(Record::BookPresence {
                    value: BookPresence::Present { scope, .. },
                    ..
                }) => Some(scope),
                _ => None,
            })
            .unwrap_or(Scope::Whole);
        Ok(vec![self.resolving(Record::BookPresence {
            book,
            value: BookPresence::Present {
                scope,
                how: PresentHow::RestoredText,
            },
        })])
    }

    fn book_scope_change(&self, book: BookId) -> Option<ChangeId> {
        match self.book(book) {
            super::BookView::Present { scope } => Some(scope),
            super::BookView::RemovedWhileEdited { scope, .. }
            | super::BookView::KeptAsLast { scope } => scope,
            _ => {
                let presence = self.books.get(&book)?.presence?;
                self.entry(presence)
                    .heads
                    .iter()
                    .flat_map(|h| ancestors_or_self(self, *h))
                    .filter(|c| {
                        matches!(self.meta(*c).shape, super::meta::Shape::Presence(Some(_)))
                    })
                    .max_by_key(|c| (self.meta(*c).at, self.meta(*c).id))
                    .map(|c| self.meta(c).id)
            }
        }
    }

    // ---------------------------------------------------------------------------------
    // Combining
    // ---------------------------------------------------------------------------------

    /// *Combine* for a verse text conflict (§8.3), offered only when the candidates' word
    /// changes against their single base touch disjoint ranges: the combined verse, for the
    /// person to see and confirm.
    pub fn combine(&self, id: &ConflictId) -> Option<Record> {
        let conflict = self.listed(id).ok()?;
        if conflict.kind != ConflictKind::VerseText || conflict.bases.len() != 1 {
            return None;
        }
        let base = self.body(&conflict.bases[0])?.record;
        let words_of = |r: &Record| match r {
            Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            } => Some(words.clone()),
            _ => None,
        };
        let base_words = words_of(&base)?;
        let base_tokens = tokens(&base_words);
        let mut versions: Vec<String> = Vec::new();
        for candidate in &conflict.candidates {
            versions.push(words_of(&candidate.value)?);
        }
        let others: Vec<Vec<&str>> = versions.iter().map(|w| tokens(w)).collect();
        let out = apply_disjoint(&base_tokens, &others)?.concat();
        let mut merged = conflict
            .candidates
            .iter()
            .find(|c| c.head == conflict.provisional)
            .or(conflict.candidates.first())?
            .value
            .clone();
        if let Record::Verse {
            value: VerseValue::Text { words, .. },
            ..
        } = &mut merged
        {
            *words = out;
        }
        Some(merged)
    }

    /// *Combine* for a passage (§8.3), offered when the two sides' word changes against
    /// the passage's base, taken over its text with its verse markers, touch disjoint
    /// ranges: one record per verse of the group, for the person to see and confirm.
    /// ("Juan's change applied to 2:3, where that text now is.")
    pub fn combine_passage(&self, id: &ConflictId) -> Option<Vec<Record>> {
        let conflict = self.listed(id).ok()?;
        if conflict.kind != ConflictKind::Passage {
            return None;
        }
        let sides: BTreeSet<Side> = conflict.candidates.iter().filter_map(|c| c.side).collect();
        if sides.len() != 2 {
            return None;
        }
        let mut keys = Self::conflict_keys(conflict);
        if keys.iter().any(|k| !matches!(k, RecordKey::Verse(..))) {
            return None;
        }
        keys.sort();
        let batches: BTreeSet<crate::ids::BatchId> = sides
            .iter()
            .filter_map(|s| match s {
                Side::Batch(b) => Some(*b),
                Side::Others => None,
            })
            .collect();
        // The passage before either side: the single base of a record both sides wrote,
        // else the record with every passage side rewound.
        let mut base: Vec<Option<Record>> = Vec::new();
        for key in &keys {
            let k = self.idx_of(key)?;
            let heads: Vec<ChangeIdx> = self.entry(k).heads.to_vec();
            let before = if heads.len() > 1 {
                super::merge::ancestry(self, &heads).bases
            } else {
                let bad: BTreeSet<ChangeIdx> = self
                    .entry(k)
                    .changes
                    .iter()
                    .copied()
                    .filter(|c| batches.contains(&self.batch(self.meta(*c).batch).id))
                    .collect();
                rewind(self, &heads, &bad)
            };
            match before.as_slice() {
                [] => base.push(None),
                [one] => base.push(Some(record_of(self, *one)?)),
                _ => return None,
            }
        }
        let mut versions: Vec<Vec<Option<Record>>> = Vec::new();
        for side in &sides {
            let values = self.keep_side(conflict, *side, &[]).ok()?;
            let by_key: BTreeMap<RecordKey, Record> = values
                .into_iter()
                .map(|p| (p.record.key(), p.record))
                .collect();
            versions.push(keys.iter().map(|k| by_key.get(k).cloned()).collect());
        }
        let mut headers: BTreeMap<String, (RecordKey, VerseValue)> = BTreeMap::new();
        let base_text = passage_tokens(&base, &mut headers)?;
        let mut side_texts: Vec<Vec<String>> = Vec::new();
        for version in &versions {
            side_texts.push(passage_tokens(version, &mut headers)?);
        }
        let base_refs: Vec<&str> = base_text.iter().map(String::as_str).collect();
        let side_refs: Vec<Vec<&str>> = side_texts
            .iter()
            .map(|t| t.iter().map(String::as_str).collect())
            .collect();
        let combined = apply_disjoint(&base_refs, &side_refs)?;
        // Back to records: each header starts a verse; words and the layout follow it.
        let mut out: BTreeMap<RecordKey, Record> = BTreeMap::new();
        let mut current: Option<(RecordKey, VerseValue)> = None;
        let finish = |current: Option<(RecordKey, VerseValue)>,
                      out: &mut BTreeMap<RecordKey, Record>|
         -> Option<()> {
            if let Some((RecordKey::Verse(book, chapter, verse), value)) = current {
                let record = Self::verse_record(book, chapter, verse, value);
                if out.insert(record.key(), record).is_some() {
                    return None;
                }
            }
            Some(())
        };
        for token in combined {
            if let Some(header) = headers.get(&token) {
                finish(current.take(), &mut out)?;
                current = Some(header.clone());
            } else if let Some(layout) = token.strip_prefix(LAYOUT_TOKEN) {
                match current.as_mut() {
                    Some((_, VerseValue::Text { layout: l, .. })) => *l = layout.to_owned(),
                    _ => return None,
                }
            } else {
                match current.as_mut() {
                    Some((_, VerseValue::Text { words, .. })) => words.push_str(&token),
                    _ => return None,
                }
            }
        }
        finish(current.take(), &mut out)?;
        Some(
            keys.iter()
                .filter_map(|key| match key {
                    RecordKey::Verse(book, chapter, verse) => {
                        Some(out.remove(key).unwrap_or_else(|| {
                            Self::verse_record(
                                *book,
                                *chapter,
                                *verse,
                                VerseValue::Absent { moved_to: None },
                            )
                        }))
                    }
                    _ => None,
                })
                .collect(),
        )
    }

    /// *Choose per line* (§8.3): the record with every agreed unit and, for each disputed
    /// unit, the value of the head the person picked.
    pub fn per_unit(
        &self,
        id: &ConflictId,
        picks: &BTreeMap<UnitName, ChangeId>,
    ) -> Result<Record, ResolveError> {
        let conflict = self.listed(id)?;
        let k = self
            .idx_of(&conflict.key)
            .ok_or(ResolveError::UnknownConflict)?;
        let heads: Vec<ChangeIdx> = self.entry(k).heads.to_vec();
        let values: Vec<Option<Json>> = heads.iter().map(|h| value_of(self, *h)).collect();
        let versions: Vec<Option<&Json>> = values.iter().map(Option::as_ref).collect();
        let units: Vec<Unit> = units_for(&conflict.key, &versions);
        let mut value = values.first().cloned().flatten();
        for unit in &units {
            let from = match picks.get(&unit.name) {
                Some(pick) => self
                    .change_idx(pick)
                    .filter(|c| heads.contains(c))
                    .ok_or(ResolveError::NotACandidate)?,
                None if conflict.disputed.contains(&unit.name) => {
                    return Err(ResolveError::MissingValue(conflict.key.clone()));
                }
                None => continue,
            };
            let unit_value = unit.read(value_of(self, from).as_ref());
            unit.write(&mut value, unit_value);
        }
        super::merge::rebuild_record(self, heads[0], value)
            .ok_or(ResolveError::MissingValue(conflict.key.clone()))
    }

    /// *Keep both* for two check comments (§8.3): the texts joined with their authors'
    /// names.
    pub fn keep_both_comments(&self, id: &ConflictId) -> Result<Record, ResolveError> {
        let conflict = self.listed(id)?;
        let mut parts: Vec<String> = Vec::new();
        let mut template: Option<Record> = None;
        for candidate in &conflict.candidates {
            let comment = match &candidate.value {
                Record::Check {
                    value: CheckValue::Decision(d),
                    ..
                } => d.comment.clone(),
                Record::Comment {
                    value: CommentValue { text, .. },
                    ..
                } => Some(text.clone()),
                _ => None,
            };
            if let Some(text) = comment {
                let who = candidate.author.as_deref().unwrap_or("Teammate");
                parts.push(format!("{who}: {text}"));
            }
            if template.is_none() {
                template = Some(candidate.value.clone());
            }
        }
        let joined = parts.join("\n\n");
        match template.ok_or(ResolveError::NotOffered)? {
            Record::Check {
                key,
                value: CheckValue::Decision(mut d),
            } => {
                d.comment = Some(joined);
                Ok(Record::Check {
                    key,
                    value: CheckValue::Decision(d),
                })
            }
            Record::Comment { id, value } => Ok(Record::Comment {
                id,
                value: CommentValue {
                    target: value.target,
                    text: joined,
                },
            }),
            _ => Err(ResolveError::NotOffered),
        }
    }

    /// *Apply Pedro's change to the moved check* (§8.3): the new key takes the units the
    /// decision's editor changed relative to the base, and the old key keeps its `MovedTo`.
    pub fn apply_to_moved_check(
        &self,
        id: &ConflictId,
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        if conflict.kind != ConflictKind::CheckMoved {
            return Err(ResolveError::NotOffered);
        }
        let moved = conflict
            .candidates
            .iter()
            .find(|c| {
                matches!(
                    c.value,
                    Record::Check {
                        value: CheckValue::MovedTo { .. },
                        ..
                    }
                )
            })
            .ok_or(ResolveError::NotOffered)?;
        let edited = conflict
            .candidates
            .iter()
            .find(|c| {
                matches!(
                    c.value,
                    Record::Check {
                        value: CheckValue::Decision(_),
                        ..
                    }
                )
            })
            .ok_or(ResolveError::NotOffered)?;
        let Record::Check {
            value: CheckValue::MovedTo { to },
            ..
        } = &moved.value
        else {
            return Err(ResolveError::NotOffered);
        };
        let new_key = RecordKey::Check(to.clone());
        let new_value = self.projected(&new_key);
        let base = conflict
            .bases
            .first()
            .and_then(|b| self.body(b))
            .map(|b| b.record);
        let target = match new_value {
            Some(Record::Check {
                value: CheckValue::Decision(d),
                ..
            }) => d,
            _ => match &edited.value {
                Record::Check {
                    value: CheckValue::Decision(d),
                    ..
                } => d.clone(),
                _ => return Err(ResolveError::NotOffered),
            },
        };
        let (
            Record::Check {
                value: CheckValue::Decision(changed),
                ..
            },
            Some(Record::Check {
                value: CheckValue::Decision(before),
                ..
            }),
        ) = (&edited.value, base)
        else {
            return Err(ResolveError::NotOffered);
        };
        let mut merged = target;
        if changed.judgement != before.judgement
            || changed.imported_invalid != before.imported_invalid
        {
            merged.judgement = changed.judgement.clone();
            merged.imported_invalid = changed.imported_invalid;
        }
        if changed.comment != before.comment {
            merged.comment = changed.comment.clone();
        }
        if changed.bookmarked != before.bookmarked {
            merged.bookmarked = changed.bookmarked;
        }
        Ok(vec![
            self.resolving(Record::Check {
                key: to.clone(),
                value: CheckValue::Decision(merged),
            }),
            self.resolving(moved.value.clone()),
        ])
    }

    /// *Rename* for a path-case conflict (§8.3): the file at a new path, and the old path
    /// removed, in one batch.
    pub fn rename_carried(
        &self,
        id: &ConflictId,
        from: &RepoPath,
        to: RepoPath,
    ) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        let key = RecordKey::CarriedFile(from.clone());
        if !Self::conflict_keys(conflict).contains(&key) {
            return Err(ResolveError::NotInConflict(key));
        }
        let Some(Record::CarriedFile { value, .. }) = self.projected(&key) else {
            return Err(ResolveError::MissingValue(key));
        };
        Ok(vec![
            PlannedChange {
                record: Record::CarriedFile {
                    path: to.clone(),
                    value,
                },
                parents: self.heads(&RecordKey::CarriedFile(to)),
                cause: Cause::Resolve,
            },
            self.resolving(Record::CarriedFile {
                path: from.clone(),
                value: CarriedValue::Removed,
            }),
        ])
    }

    /// A bulk choice (§5.9): for each conflict, the one candidate made on a computer of
    /// `devices`. A conflict where two or more candidates, or none, come from those
    /// computers is left for a choice of its own.
    pub fn bulk_resolution(
        &self,
        devices: &BTreeSet<DeviceId>,
        conflicts: &[ConflictId],
    ) -> (Vec<(ConflictId, Vec<PlannedChange>)>, Vec<ConflictId>) {
        let mut resolved = Vec::new();
        let mut skipped = Vec::new();
        for id in conflicts {
            let Ok(conflict) = self.listed(id) else {
                continue;
            };
            let mine: Vec<&Candidate> = conflict
                .candidates
                .iter()
                .filter(|c| devices.contains(&c.device))
                .collect();
            if mine.len() != 1 {
                skipped.push(*id);
                continue;
            }
            match self.resolution(id, &Choice::Candidate { head: mine[0].head }) {
                Ok(changes) => resolved.push((*id, changes)),
                Err(_) => skipped.push(*id),
            }
        }
        (resolved, skipped)
    }
}

/// Words and the whitespace between them, as alternating tokens, so that joining them gives
/// back the bytes.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut space: Option<bool> = None;
    for (i, ch) in text.char_indices() {
        let is_space = ch.is_whitespace();
        if space.is_some_and(|s| s != is_space) {
            out.push(&text[start..i]);
            start = i;
        }
        space = Some(is_space);
    }
    if start < text.len() {
        out.push(&text[start..]);
    }
    out
}

/// The cells a longest-common-subsequence table may have; a larger middle is one edit.
const LCS_CELLS: usize = 4_000_000;

/// The edits that turn `base` into `other`, as (start, end, replacement) over base token
/// indices: the common prefix and suffix are kept, and the middle is diffed by a longest
/// common subsequence (or replaced whole when it is too large to diff).
fn word_edits(base: &[&str], other: &[&str]) -> Vec<(usize, usize, Vec<String>)> {
    let prefix = base
        .iter()
        .zip(other.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = base[prefix..]
        .iter()
        .rev()
        .zip(other[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let base_mid = &base[prefix..base.len() - suffix];
    let other_mid = &other[prefix..other.len() - suffix];
    if base_mid.is_empty() && other_mid.is_empty() {
        return Vec::new();
    }
    if (base_mid.len() + 1).saturating_mul(other_mid.len() + 1) > LCS_CELLS {
        return vec![(
            prefix,
            prefix + base_mid.len(),
            other_mid.iter().map(|t| (*t).to_owned()).collect(),
        )];
    }
    lcs_edits(base_mid, other_mid)
        .into_iter()
        .map(|(start, end, replacement)| (start + prefix, end + prefix, replacement))
        .collect()
}

/// Applies every side's edits against `base` when no two of them touch the same base
/// tokens: ranges may meet but not overlap, and two insertions at one point are ambiguous.
fn apply_disjoint(base: &[&str], sides: &[Vec<&str>]) -> Option<Vec<String>> {
    let mut edits: Vec<(usize, usize, Vec<String>)> = Vec::new();
    for side in sides {
        edits.extend(word_edits(base, side));
    }
    edits.sort_by_key(|e| (e.0, e.1));
    edits.dedup();
    for pair in edits.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let overlap = b.0 < a.1;
        let same_point = a.0 == a.1 && b.0 == b.1 && a.0 == b.0;
        if overlap || same_point {
            return None;
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut at = 0;
    for (start, end, replacement) in edits {
        out.extend(base[at..start].iter().map(|t| (*t).to_owned()));
        out.extend(replacement);
        at = end;
    }
    out.extend(base[at..].iter().map(|t| (*t).to_owned()));
    Some(out)
}

/// The edits of a longest common subsequence.
fn lcs_edits(base: &[&str], other: &[&str]) -> Vec<(usize, usize, Vec<String>)> {
    let (n, m) = (base.len(), other.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == other[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut edits = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, Vec<String>)> = None;
    while i < n || j < m {
        if i < n && j < m && base[i] == other[j] {
            if let Some((start, replacement)) = open.take() {
                edits.push((start, i, replacement));
            }
            i += 1;
            j += 1;
        } else if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            open.get_or_insert((i, Vec::new()))
                .1
                .push(other[j].to_owned());
            j += 1;
        } else {
            open.get_or_insert((i, Vec::new()));
            i += 1;
        }
    }
    if let Some((start, replacement)) = open.take() {
        edits.push((start, n, replacement));
    }
    edits
}

/// Marks the start of a verse's layout among a passage's tokens; a verse header is a
/// token that starts with [`HEADER_TOKEN`].
const LAYOUT_TOKEN: char = '\u{2}';
const HEADER_TOKEN: char = '\u{1}';

/// A passage as tokens: for each verse, a header naming its key and its fixed fields, its
/// words and whitespace, and its layout. `None` when a word could be read as a marker.
fn passage_tokens(
    records: &[Option<Record>],
    headers: &mut BTreeMap<String, (RecordKey, VerseValue)>,
) -> Option<Vec<String>> {
    let mut out = Vec::new();
    for record in records.iter().flatten() {
        let Record::Verse { value, .. } = record else {
            return None;
        };
        // A verse absent with no destination is no verse: the same as no record, which
        // is what a side that never wrote it has.
        if matches!(value, VerseValue::Absent { moved_to: None }) {
            continue;
        }
        let key = record.key();
        let (template, words, layout) = match value {
            VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            } => (
                VerseValue::Text {
                    marker: marker.clone(),
                    words: String::new(),
                    layout: String::new(),
                    last: *last,
                    from: *from,
                },
                Some(words.as_str()),
                Some(layout.as_str()),
            ),
            other => (other.clone(), None, None),
        };
        let header = format!("{HEADER_TOKEN}{key:?}{template:?}");
        headers.insert(header.clone(), (key, template));
        out.push(header);
        if let Some(words) = words {
            for token in tokens(words) {
                if token.starts_with(HEADER_TOKEN) || token.starts_with(LAYOUT_TOKEN) {
                    return None;
                }
                out.push(token.to_owned());
            }
        }
        if let Some(layout) = layout {
            out.push(format!("{LAYOUT_TOKEN}{layout}"));
        }
    }
    Some(out)
}
