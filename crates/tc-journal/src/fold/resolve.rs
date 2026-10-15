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
}

impl FoldState {
    fn listed(&self, id: &ConflictId) -> Result<&Conflict, ResolveError> {
        self.conflict(id).ok_or(ResolveError::UnknownConflict)
    }

    fn idx_of(&self, key: &RecordKey) -> Option<KeyIdx> {
        self.key_ids.get(key).copied()
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
                self.keep_side(conflict, candidate.side.unwrap_or(Side::Others))
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

    /// *Keep Maria's passage* (§8.3): every record of the group takes its value from that
    /// side, with every other passage side rewound; a record the side did not have before
    /// becomes absent.
    pub fn keep_side(
        &self,
        conflict: &Conflict,
        side: Side,
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
            let chosen = pool.iter().copied().max_by_key(|c| self.meta(*c).order());
            let record = match chosen.and_then(|c| record_of(self, c)) {
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
            };
            out.push(self.resolving(record));
        }
        Ok(out)
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

    /// The newest text a verse ever had as a verse of its own (its `last` is itself), else
    /// its newest text: what *Keep separate* writes back (the base before a join).
    fn own_text(&self, k: KeyIdx, verse: u16) -> Option<VerseValue> {
        let mut seen: BTreeSet<ChangeIdx> = BTreeSet::new();
        for h in self.entry(k).heads.iter() {
            seen.extend(ancestors_or_self(self, *h));
        }
        let texts: Vec<ChangeIdx> = seen
            .into_iter()
            .filter(|c| matches!(self.meta(*c).verse(), Some(VerseShape::Text { .. })))
            .collect();
        let own = texts
            .iter()
            .copied()
            .filter(|c| matches!(self.meta(*c).verse(), Some(VerseShape::Text { last, .. }) if last == verse))
            .max_by_key(|c| (self.meta(*c).at, self.meta(*c).id));
        let any = texts
            .iter()
            .copied()
            .max_by_key(|c| (self.meta(*c).at, self.meta(*c).id));
        let c = own.or(any)?;
        match record_of(self, c)? {
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

    /// *Keep 4 and 5 separate* (§8.3): every verse of the range a text of its own, with the
    /// words it had before the join.
    pub fn keep_separate(&self, id: &ConflictId) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        let mut out = Vec::new();
        for key in Self::conflict_keys(conflict) {
            let RecordKey::Verse(book, chapter, verse) = &key else {
                if let RecordKey::ChapterFront(book, chapter) = &key {
                    out.push(self.resolving(Self::default_front(*book, *chapter)));
                }
                continue;
            };
            let value = self
                .idx_of(&key)
                .and_then(|k| self.own_text(k, verse.get()))
                .unwrap_or(VerseValue::Absent { moved_to: None });
            out.push(self.resolving(Self::verse_record(*book, *chapter, *verse, value)));
        }
        Ok(out)
    }

    /// *Keep the joined verses* (§8.3): the first verse of the range keeps its span text,
    /// the others are covered by it.
    pub fn keep_joined(&self, id: &ConflictId) -> Result<Vec<PlannedChange>, ResolveError> {
        let conflict = self.listed(id)?;
        let keys = Self::conflict_keys(conflict);
        let verses: Vec<(BookId, ChapterNo, VerseNo)> = keys
            .iter()
            .filter_map(|k| match k {
                RecordKey::Verse(b, c, v) => Some((*b, *c, *v)),
                _ => None,
            })
            .collect();
        let Some(&(book, chapter, first)) = verses.iter().min_by_key(|(_, _, v)| *v) else {
            return Err(ResolveError::NotOffered);
        };
        let last = verses.iter().map(|(_, _, v)| *v).max().unwrap_or(first);
        let head = self
            .projected(&RecordKey::Verse(book, chapter, first))
            .and_then(|r| match r {
                Record::Verse {
                    value: VerseValue::Text { .. },
                    ..
                } => Some(r),
                _ => None,
            });
        let text = match head {
            Some(Record::Verse {
                value:
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        from,
                        ..
                    },
                ..
            }) => VerseValue::Text {
                marker,
                words,
                layout,
                last,
                from,
            },
            _ => self
                .idx_of(&RecordKey::Verse(book, chapter, first))
                .and_then(|k| self.own_text(k, first.get()))
                .map(|v| match v {
                    VerseValue::Text {
                        marker,
                        words,
                        layout,
                        from,
                        ..
                    } => VerseValue::Text {
                        marker,
                        words,
                        layout,
                        last,
                        from,
                    },
                    other => other,
                })
                .ok_or(ResolveError::NotOffered)?,
        };
        let mut out = vec![self.resolving(Self::verse_record(book, chapter, first, text))];
        for (b, c, v) in verses {
            if v != first {
                out.push(self.resolving(Self::verse_record(
                    b,
                    c,
                    v,
                    VerseValue::Covered { by: first },
                )));
            }
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
            } if last > verse => self.keep_joined(&conflict.id),
            _ => self.keep_separate(&conflict.id),
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
        let base_tokens: Vec<&str> = base_words.split_inclusive(char::is_whitespace).collect();
        let mut edits: Vec<(usize, usize, Vec<String>)> = Vec::new();
        for candidate in &conflict.candidates {
            let words = words_of(&candidate.value)?;
            let tokens: Vec<&str> = words.split_inclusive(char::is_whitespace).collect();
            edits.extend(word_edits(&base_tokens, &tokens));
        }
        edits.sort_by_key(|(start, end, _)| (*start, *end));
        // Disjoint: no two edits touch the same or adjacent base tokens.
        for pair in edits.windows(2) {
            if pair[1].0 <= pair[0].1 {
                return None;
            }
        }
        let mut out = String::new();
        let mut at = 0;
        for (start, end, replacement) in &edits {
            out.extend(base_tokens[at..*start].iter().copied());
            out.extend(replacement.iter().map(String::as_str));
            at = *end;
        }
        out.extend(base_tokens[at..].iter().copied());
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
                    .ids
                    .get(pick)
                    .filter(|c| heads.contains(c))
                    .copied()
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

/// The edits that turn `base` into `other`, as (start, end, replacement) over base token
/// indices, from a longest common subsequence.
fn word_edits(base: &[&str], other: &[&str]) -> Vec<(usize, usize, Vec<String>)> {
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
