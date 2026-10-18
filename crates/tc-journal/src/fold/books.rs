//! F8, books (§7.6): presence, removal that never hides unseen work, records without a
//! book, and the last-book rule.

use super::merge::{ancestors_or_self, record_of};
use super::meta::{ChangeIdx, Form, KeyIdx};
use super::state::{BookStatus, Dirty, FoldState, Owner, ProjectKind};
use super::{Anomaly, Conflict, ConflictId, ConflictKind, Counter};
use crate::ids::{ChangeId, ShortId};
use crate::record::{BookPresence, PresentHow, Record};
use std::collections::{BTreeMap, BTreeSet};
use tc_core::BookId;

pub(crate) fn stage_books(state: &mut FoldState, dirty: &mut Dirty) {
    let books: Vec<BookId> = if dirty.all {
        state.books.keys().copied().collect()
    } else {
        dirty.books.iter().copied().collect()
    };
    let mut changed = false;
    for book in books {
        let before = state
            .books
            .get(&book)
            .map(|b| b.status.clone())
            .unwrap_or(BookStatus::Unknown);
        let after = status_of(state, book, dirty);
        records_anomaly(state, book);
        let presence_changed = state
            .books
            .get(&book)
            .and_then(|b| b.presence)
            .is_some_and(|p| dirty.changed.contains(&p));
        if after != before || presence_changed {
            changed = true;
            if let Some(entry) = state.book_mut(book) {
                entry.status = after;
            }
            mark_book(state, book, dirty);
        }
        // A restore race lists the verses typed after the losing *Start empty* (§7.6):
        // its conflict follows the book's verses.
        if !dirty.all
            && let Some(presence) = state.books.get(&book).and_then(|b| b.presence)
            && matches!(
                state.entry(presence).local,
                super::state::Local::Conflict(_)
            )
            && !race_batches(state, book).is_empty()
        {
            super::attach::relist(state, presence, dirty);
        }
    }
    if changed || dirty.all || dirty.books.iter().any(|b| removed(state, *b)) {
        last_book(state, dirty);
    }
}

fn removed(state: &FoldState, book: BookId) -> bool {
    matches!(
        state.books.get(&book).map(|b| &b.status),
        Some(BookStatus::Removed) | Some(BookStatus::KeptAsLast { .. })
    )
}

/// Every chapter and path of a book whose projection appeared, vanished or changed scope.
fn mark_book(state: &FoldState, book: BookId, dirty: &mut Dirty) {
    dirty.paths = true;
    if let Some(entry) = state.books.get(&book) {
        for c in &entry.chapters {
            dirty.chapters.insert((book, *c));
        }
        dirty.attached.extend(entry.keys.iter().copied());
    }
}

/// The presence of `book` from its `BookPresence` heads.
fn status_of(state: &mut FoldState, book: BookId, dirty: &mut Dirty) -> BookStatus {
    state.count(Counter::BooksChecked, 1);
    let Some(entry) = state.books.get(&book) else {
        return BookStatus::Unknown;
    };
    let Some(presence) = entry.presence else {
        state.set_conflicts(Owner::Book(book), Vec::new(), dirty);
        return BookStatus::Unknown;
    };
    let heads: Vec<ChangeIdx> = state.entry(presence).heads.to_vec();
    if heads.is_empty() {
        state.set_conflicts(Owner::Book(book), Vec::new(), dirty);
        return BookStatus::Unknown;
    }
    let present: Vec<ChangeIdx> = heads
        .iter()
        .copied()
        .filter(|h| matches!(state.meta(*h).form, Form::Presence(Some(_))))
        .collect();
    if let Some(scope) = present
        .iter()
        .copied()
        .max_by_key(|h| (state.meta(*h).at, state.meta(*h).id))
    {
        state.set_conflicts(Owner::Book(book), Vec::new(), dirty);
        return BookStatus::Present { scope };
    }
    // Every head is a removal: does it hide anything its remover had not seen?
    let unseen = unseen(state, book, presence, &heads);
    let scope = nearest_present(state, &heads);
    if unseen.is_empty() {
        state.set_conflicts(Owner::Book(book), Vec::new(), dirty);
        return BookStatus::Removed;
    }
    let count = unseen.len() as u32;
    let mut ids: Vec<ChangeId> = heads.iter().map(|h| state.meta(*h).id).collect();
    ids.extend(unseen.iter().map(|c| state.meta(*c).id));
    let kind = ConflictKind::BookRemovedWhileEdited { unseen: count };
    let conflict = book_conflict(state, presence, &heads, kind, &ids);
    state.set_conflicts(Owner::Book(book), vec![conflict], dirty);
    BookStatus::RemovedWhileEdited {
        scope,
        unseen: count,
    }
}

/// The `Present` ancestor of the removal heads with the largest (at, id).
fn nearest_present(state: &FoldState, heads: &[ChangeIdx]) -> Option<ChangeIdx> {
    let mut best: Option<ChangeIdx> = None;
    for h in heads {
        for a in ancestors_or_self(state, *h) {
            if matches!(state.meta(a).form, Form::Presence(Some(_)))
                && best.is_none_or(|b| {
                    (state.meta(a).at, state.meta(a).id) > (state.meta(b).at, state.meta(b).id)
                })
            {
                best = Some(a);
            }
        }
    }
    best
}

/// The changes of the book's records that no removal head's remover held (§7.6).
fn unseen(
    state: &FoldState,
    book: BookId,
    presence: KeyIdx,
    heads: &[ChangeIdx],
) -> Vec<ChangeIdx> {
    let seen_lists: Vec<BTreeSet<ShortId>> = heads
        .iter()
        .map(|h| match record_of(state, *h) {
            Some(Record::BookPresence {
                value: BookPresence::Removed { seen },
                ..
            }) => seen.into_iter().collect(),
            _ => BTreeSet::new(),
        })
        .collect();
    let Some(entry) = state.books.get(&book) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &k in &entry.keys {
        if k == presence {
            continue;
        }
        // A comment id held by changes that target several books (§8.1) is a record of
        // each, by the changes that target it.
        let comment = matches!(state.entry(k).key, crate::keys::RecordKey::Comment(_));
        let changes: Vec<ChangeIdx> = state
            .entry(k)
            .changes
            .iter()
            .copied()
            .filter(|c| state.meta(*c).active())
            .filter(|c| {
                !comment
                    || super::meta::book_of(&state.entry(k).key, &state.shape(*c)) == Some(book)
            })
            .collect();
        if changes.is_empty() {
            continue;
        }
        state.count(Counter::ChangesVisited, changes.len() as u64);
        // closure_r(k): the changes on k whose short id r saw, and their ancestors.
        let closures: Vec<BTreeSet<ChangeIdx>> = seen_lists
            .iter()
            .map(|seen| {
                let mut closure = BTreeSet::new();
                for c in &changes {
                    if seen.contains(&state.meta(*c).id.short()) {
                        closure.extend(ancestors_or_self(state, *c));
                    }
                }
                closure
            })
            .collect();
        for c in changes {
            if closures.iter().all(|closure| !closure.contains(&c)) {
                out.push(c);
            }
        }
    }
    out
}

fn book_conflict(
    state: &FoldState,
    presence: KeyIdx,
    heads: &[ChangeIdx],
    kind: ConflictKind,
    ids: &[ChangeId],
) -> Conflict {
    let tag = kind.tag().unwrap_or("book");
    let (candidates, unreadable) = super::attach::candidates_of(state, heads, |_| None);
    let provisional = heads
        .iter()
        .copied()
        .max_by_key(|h| state.meta(*h).order())
        .map(|h| state.meta(h).id)
        .unwrap_or(ChangeId::from_bytes([0; 32]));
    let batches: BTreeSet<crate::ids::BatchId> = heads
        .iter()
        .map(|h| state.batch(state.meta(*h).batch).id)
        .collect();
    Conflict {
        id: ConflictId::tagged(tag, ids),
        key: state.entry(presence).key.clone(),
        also_keys: Vec::new(),
        kind,
        bases: Vec::new(),
        candidates,
        agreed: Vec::new(),
        disputed: Vec::new(),
        provisional,
        batches: batches.into_iter().collect(),
        incomplete_history: super::attach::continues_missing(state, heads),
        unreadable,
    }
}

/// `RecordsWithoutBook`: records of a book that no folded presence change mentions. The
/// anomaly listed for each book is kept beside the list, so a step replaces it by its exact
/// value instead of looking for it among every anomaly.
fn records_anomaly(state: &mut FoldState, book: BookId) {
    let new = state.books.get(&book).and_then(|entry| {
        let mentioned = entry.presence.is_some_and(|p| state.entry(p).active > 0);
        (!mentioned && entry.records > 0).then_some(Anomaly::RecordsWithoutBook {
            book,
            records: entry.records,
        })
    });
    if state.without_book.get(&book) == new.as_ref() {
        return;
    }
    if let Some(old) = state.without_book.remove(&book) {
        state.unlist_anomaly(&old);
    }
    if let Some(new) = new {
        state.list_anomaly(new.clone());
        state.without_book.insert(book, new);
    }
}

/// The last-book rule (§7.6): in a Bible project where no book would project, the book
/// whose removal head has the largest (at, id) projects.
fn last_book(state: &mut FoldState, dirty: &mut Dirty) {
    if state.kind != ProjectKind::Bible {
        return;
    }
    // Undo an earlier choice first.
    let kept: Vec<BookId> = state
        .books
        .iter()
        .filter(|(_, b)| matches!(b.status, BookStatus::KeptAsLast { .. }))
        .map(|(id, _)| *id)
        .collect();
    for book in kept {
        if let Some(entry) = state.book_mut(book) {
            entry.status = BookStatus::Removed;
        }
        state.set_conflicts(Owner::Book(book), Vec::new(), dirty);
        mark_book(state, book, dirty);
    }
    if state.books.values().any(|b| b.status.projects()) {
        return;
    }
    let mut best: Option<(crate::ids::WallTime, ChangeId, BookId, ChangeIdx)> = None;
    for (book, entry) in state.books.iter() {
        if entry.status != BookStatus::Removed {
            continue;
        }
        let Some(presence) = entry.presence else {
            continue;
        };
        for h in state.entry(presence).heads.iter() {
            let meta = state.meta(*h);
            let candidate = (meta.at, meta.id, *book, *h);
            if best
                .as_ref()
                .is_none_or(|b| (b.0, b.1) < (candidate.0, candidate.1))
            {
                best = Some(candidate);
            }
        }
    }
    let Some((_, _, book, _)) = best else {
        return;
    };
    let Some(presence) = state.books.get(&book).and_then(|b| b.presence) else {
        return;
    };
    let heads: Vec<ChangeIdx> = state.entry(presence).heads.to_vec();
    let scope = nearest_present(state, &heads);
    let ids: Vec<ChangeId> = heads.iter().map(|h| state.meta(*h).id).collect();
    let conflict = book_conflict(state, presence, &heads, ConflictKind::LastBookRemoved, &ids);
    state.set_conflicts(Owner::Book(book), vec![conflict], dirty);
    if let Some(entry) = state.book_mut(book) {
        entry.status = BookStatus::KeptAsLast { scope };
    }
    mark_book(state, book, dirty);
}

/// For a book whose presence is in conflict between *Restore the text* and *Start empty*
/// (§7.6): the *Start empty* batches of the race, whichever is provisional. Read no clock.
pub(crate) fn race_batches(state: &FoldState, book: BookId) -> Vec<crate::ids::BatchId> {
    let Some(presence) = state.books.get(&book).and_then(|b| b.presence) else {
        return Vec::new();
    };
    let heads = &state.entry(presence).heads;
    if heads.len() < 2
        || !heads.iter().any(|h| {
            matches!(
                state.meta(*h).form,
                Form::Presence(Some(PresentHow::RestoredText))
            )
        })
    {
        return Vec::new();
    }
    let mut out: BTreeMap<crate::ids::BatchId, ()> = BTreeMap::new();
    for h in heads.iter() {
        if matches!(
            state.meta(*h).form,
            Form::Presence(Some(PresentHow::StartedEmpty))
        ) {
            out.insert(state.batch(state.meta(*h).batch).id, ());
        }
    }
    out.into_keys().collect()
}

/// The *Start empty* batches projection rewinds (§7.6): those of the race, when the
/// provisional presence head (the `Present` head with the largest `(at, id)`) restored the
/// text.
pub(crate) fn rewound_batches(state: &FoldState, book: BookId) -> Vec<crate::ids::BatchId> {
    let race = race_batches(state, book);
    if race.is_empty() {
        return race;
    }
    let Some(presence) = state.books.get(&book).and_then(|b| b.presence) else {
        return Vec::new();
    };
    let provisional = state
        .entry(presence)
        .heads
        .iter()
        .copied()
        .filter(|h| matches!(state.meta(*h).form, Form::Presence(Some(_))))
        .max_by_key(|h| (state.meta(*h).at, state.meta(*h).id));
    match provisional.map(|h| state.meta(h).form) {
        Some(Form::Presence(Some(PresentHow::RestoredText))) => race,
        _ => Vec::new(),
    }
}
