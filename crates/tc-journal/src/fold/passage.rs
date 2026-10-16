//! Passages (§7.3): a save that rewrote several verses at once against a concurrent change
//! of any of them is one conflict over the whole passage, shown wholly from one side.

use super::merge::{ancestors_or_self, ancestry, div, key_parents};
use super::meta::{BatchIdx, ChangeIdx, KeyIdx};
use super::state::{Dirty, FoldState, Group, Local, Owner, Projected};
use super::{Candidate, Conflict, ConflictId, ConflictKind, Counter, Side};
use crate::ids::ChangeId;
use crate::keys::RecordKey;
use std::collections::{BTreeMap, BTreeSet};
use tc_core::BookId;

/// The verse records of `book` that batch `b` wrote (its passage).
fn passage_records(state: &FoldState, b: BatchIdx, book: BookId) -> Vec<KeyIdx> {
    let mut out: Vec<KeyIdx> = state
        .batch(b)
        .members
        .iter()
        .filter(|c| state.meta(**c).active)
        .map(|c| state.meta(*c).key)
        .filter(|k| matches!(state.entry(*k).key, RecordKey::Verse(bk, ..) if bk == book))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Recomputes the passage groups the dirty verse records belong to, before and after.
pub(crate) fn stage_groups(state: &mut FoldState, dirty: &mut Dirty) {
    let mut todo: BTreeSet<KeyIdx> = if dirty.all {
        (0..state.keys.len() as u32)
            .map(KeyIdx)
            .filter(|k| matches!(state.entry(*k).key, RecordKey::Verse(..)))
            .collect()
    } else {
        let mut todo: BTreeSet<KeyIdx> = dirty
            .changed
            .iter()
            .copied()
            .filter(|k| matches!(state.entry(*k).key, RecordKey::Verse(..)))
            .collect();
        for &b in &dirty.regroup {
            for book in state.passage_of(b).to_vec() {
                todo.extend(passage_records(state, b, book));
            }
        }
        todo
    };
    if todo.is_empty() {
        return;
    }
    // Dissolve the groups the dirty records were in; their records are recomputed too.
    let mut queue: Vec<KeyIdx> = todo.iter().copied().collect();
    let mut dissolved: BTreeSet<KeyIdx> = BTreeSet::new();
    while let Some(k) = queue.pop() {
        if let Some(g) = state.entry(k).group
            && dissolved.insert(g)
            && let Some(group) = state.groups.remove(&g)
        {
            state.set_conflicts(Owner::Group(g), Vec::new(), dirty);
            for r in group.records {
                state.entry_mut(r).group = None;
                if todo.insert(r) {
                    queue.push(r);
                }
            }
        }
    }
    // Build the groups from every dirty record that diverges; a group may reach records
    // of other groups, which dissolve and join.
    let mut grouped: BTreeSet<KeyIdx> = BTreeSet::new();
    let seeds: Vec<KeyIdx> = todo.iter().copied().collect();
    for seed in seeds {
        if grouped.contains(&seed) || !matches!(state.entry(seed).local, Local::Passage) {
            continue;
        }
        let records = component(state, seed, dirty, &mut todo);
        grouped.extend(records.iter().copied());
        build_group(state, records, dirty);
    }
    for k in todo {
        if !dirty.all {
            dirty.changed.insert(k);
        }
        if state.entry(k).group.is_none() {
            let projected = local_projection(state, k);
            state.entry_mut(k).projected = projected;
        }
    }
}

/// The records connected to `seed` through the passage batches they diverge from.
fn component(
    state: &mut FoldState,
    seed: KeyIdx,
    dirty: &mut Dirty,
    todo: &mut BTreeSet<KeyIdx>,
) -> Vec<KeyIdx> {
    let mut members: BTreeSet<KeyIdx> = BTreeSet::new();
    let mut queue = vec![seed];
    while let Some(r) = queue.pop() {
        if !members.insert(r) {
            continue;
        }
        // A record of a group that was not dirty joins this one: dissolve its group.
        if let Some(g) = state.entry(r).group
            && let Some(group) = state.groups.remove(&g)
        {
            state.set_conflicts(Owner::Group(g), Vec::new(), dirty);
            for other in group.records {
                state.entry_mut(other).group = None;
                todo.insert(other);
                queue.push(other);
            }
        }
        todo.insert(r);
        let RecordKey::Verse(book, ..) = state.entry(r).key else {
            continue;
        };
        for b in state.div_of(r).to_vec() {
            for other in passage_records(state, b, book) {
                if !members.contains(&other) {
                    queue.push(other);
                }
            }
        }
    }
    members.into_iter().collect()
}

/// A record's projection from its own resolution.
pub(crate) fn local_projection(state: &FoldState, k: KeyIdx) -> Projected {
    match &state.entry(k).local {
        Local::Empty => Projected::Nothing,
        Local::Settled(c) | Local::ByRule(c) => Projected::Change(*c),
        Local::Merged(merged) => Projected::Merged(merged.0.clone()),
        Local::Conflict(conflict) => Projected::Change(conflict.provisional),
        // Not in a group (its batches' passages could not be read): its newest head.
        Local::Passage => {
            let heads = state.entry(k).heads.to_vec();
            Projected::Change(super::merge::provisional(state, &heads))
        }
    }
}

/// The side order: largest (rank, at, id), then the side itself.
type SideTime = ((u8, crate::ids::WallTime, ChangeId), Side);

fn build_group(state: &mut FoldState, mut records: Vec<KeyIdx>, dirty: &mut Dirty) {
    state.count(Counter::GroupsBuilt, 1);
    // The order records were discovered in depends on arrival; the conflict must not.
    records.sort_by(|a, b| state.entry(*a).key.cmp(&state.entry(*b).key));
    let Some(&first) = records.iter().min_by_key(|k| state.entry(**k).key.clone()) else {
        return;
    };
    let book = match state.entry(first).key {
        RecordKey::Verse(book, ..) => book,
        _ => return,
    };
    // Div sets and sides.
    let mut sides: BTreeMap<Side, (u8, crate::ids::WallTime, ChangeId)> = BTreeMap::new();
    let mut side_of: BTreeMap<ChangeIdx, Vec<Side>> = BTreeMap::new();
    let mut bases: BTreeSet<ChangeId> = BTreeSet::new();
    for &r in &records {
        let heads = state.entry(r).heads.to_vec();
        if heads.len() < 2 {
            continue;
        }
        let anc = ancestry(state, &heads);
        bases.extend(anc.bases.iter().map(|b| state.meta(*b).id));
        let div_set = div(&anc);
        // The changes of passage batches on this record that are in its Div set: a
        // passage batch every head descends from is history, not a side.
        let batch_changes: Vec<(BatchIdx, ChangeIdx)> = div_set
            .iter()
            .copied()
            .map(|c| (state.meta(c).batch, c))
            .filter(|(b, _)| state.passage_of(*b).contains(&book))
            .collect();
        for x in div_set {
            let anc_x = ancestors_or_self(state, x);
            let mut belongs: Vec<Side> = batch_changes
                .iter()
                .filter(|(_, c)| anc_x.contains(c))
                .map(|(b, _)| Side::Batch(state.batch(*b).id))
                .collect();
            belongs.sort();
            belongs.dedup();
            if belongs.is_empty() {
                belongs.push(Side::Others);
            }
            let order = state.meta(x).order();
            for side in &belongs {
                let time = sides.entry(*side).or_insert(order);
                if order > *time {
                    *time = order;
                }
            }
            side_of.insert(x, belongs);
        }
    }
    let winner: Option<SideTime> = sides.iter().map(|(side, time)| (*time, *side)).max();
    let (w_time, w) = match winner {
        Some(pair) => pair,
        None => return,
    };
    let losing: BTreeSet<BatchId> = sides
        .keys()
        .filter_map(|s| match s {
            Side::Batch(id) if *s != w => Some(*id),
            _ => None,
        })
        .collect();
    // The provisional value of every record.
    let mut all_heads: Vec<ChangeId> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut batches: BTreeSet<BatchId> = BTreeSet::new();
    let mut incomplete = false;
    for &r in &records {
        let heads = state.entry(r).heads.to_vec();
        let projected = provisional_after_rewind(state, r, &heads, &losing, w);
        state.entry_mut(r).projected = projected;
        state.entry_mut(r).group = Some(first);
        if !dirty.all {
            dirty.changed.insert(r);
        }
        for h in &heads {
            let meta = state.meta(*h);
            all_heads.push(meta.id);
            batches.insert(state.batch(meta.batch).id);
            if state
                .faults_of(r)
                .iter()
                .any(|(c, _, f)| c == h && *f == super::state::Fault::Missing)
            {
                incomplete = true;
            }
            let side = side_of
                .get(h)
                .and_then(|s| s.first().copied())
                .or_else(|| {
                    // A single-head record of the passage: the side whose change on it the
                    // head is or descends from.
                    let anc = ancestors_or_self(state, *h);
                    anc.iter()
                        .map(|a| Side::Batch(state.batch(state.meta(*a).batch).id))
                        .filter(|s| sides.contains_key(s))
                        .min()
                })
                .unwrap_or(Side::Others);
            if let Some(candidate) = super::attach::candidate(state, *h, Vec::new(), Some(side)) {
                candidates.push(candidate);
            }
        }
    }
    candidates.sort_by(|a, b| {
        (a.value.key(), std::cmp::Reverse((a.at, a.head)))
            .cmp(&(b.value.key(), std::cmp::Reverse((b.at, b.head))))
    });
    let mut also_keys: Vec<RecordKey> = records
        .iter()
        .filter(|k| **k != first)
        .map(|k| state.entry(*k).key.clone())
        .collect();
    also_keys.sort();
    let id = ConflictId::of(&all_heads);
    let conflict = Conflict {
        id,
        key: state.entry(first).key.clone(),
        also_keys,
        kind: ConflictKind::Passage,
        bases: bases.into_iter().collect(),
        candidates,
        agreed: Vec::new(),
        disputed: Vec::new(),
        provisional: w_time.2,
        batches: batches.into_iter().collect(),
        incomplete_history: incomplete,
    };
    let shared = std::sync::Arc::new(conflict.clone());
    state.groups.insert(
        first,
        Group {
            records,
            conflict: shared,
        },
    );
    state.set_conflicts(Owner::Group(first), vec![conflict], dirty);
}

use crate::ids::BatchId;

/// The provisional value of record `r` of a group (§7.3): its heads with every losing
/// passage side rewound, then the candidate of the winning side if it wrote `r`.
fn provisional_after_rewind(
    state: &FoldState,
    r: KeyIdx,
    heads: &[ChangeIdx],
    losing: &BTreeSet<BatchId>,
    winner: Side,
) -> Projected {
    let changes: Vec<ChangeIdx> = state
        .entry(r)
        .changes
        .iter()
        .copied()
        .filter(|c| state.meta(*c).active)
        .collect();
    let bad_roots: BTreeSet<ChangeIdx> = changes
        .iter()
        .copied()
        .filter(|c| losing.contains(&state.batch(state.meta(*c).batch).id))
        .collect();
    let candidates = rewind(state, heads, &bad_roots);
    if candidates.is_empty() {
        return Projected::Nothing;
    }
    let preferred: Vec<ChangeIdx> = match winner {
        Side::Batch(w) => {
            let w_changes: Vec<ChangeIdx> = changes
                .iter()
                .copied()
                .filter(|c| state.batch(state.meta(*c).batch).id == w)
                .collect();
            candidates
                .iter()
                .copied()
                .filter(|c| {
                    let anc = ancestors_or_self(state, *c);
                    w_changes.iter().any(|w| anc.contains(w))
                })
                .collect()
        }
        Side::Others => Vec::new(),
    };
    let pool = if preferred.is_empty() {
        &candidates
    } else {
        &preferred
    };
    pool.iter()
        .copied()
        .max_by_key(|c| state.meta(*c).order())
        .map_or(Projected::Nothing, Projected::Change)
}

/// Rewinding (§7.3): the maximal changes among the ancestors-or-self of `heads` that
/// neither are nor descend from a change in `bad`.
pub(crate) fn rewind(
    state: &FoldState,
    heads: &[ChangeIdx],
    bad: &BTreeSet<ChangeIdx>,
) -> Vec<ChangeIdx> {
    if bad.is_empty() {
        return heads.to_vec();
    }
    let mut reached: BTreeSet<ChangeIdx> = BTreeSet::new();
    let mut stack: Vec<ChangeIdx> = heads.to_vec();
    let mut good: BTreeSet<ChangeIdx> = BTreeSet::new();
    while let Some(x) = stack.pop() {
        if !reached.insert(x) {
            continue;
        }
        let anc = ancestors_or_self(state, x);
        if anc.iter().any(|a| bad.contains(a)) {
            stack.extend(key_parents(state, x));
        } else {
            good.insert(x);
        }
    }
    // Drop every candidate that is an ancestor of another.
    let mut below: BTreeSet<ChangeIdx> = BTreeSet::new();
    for g in &good {
        for p in key_parents(state, *g) {
            below.extend(ancestors_or_self(state, p));
        }
    }
    let mut out: Vec<ChangeIdx> = good.into_iter().filter(|g| !below.contains(g)).collect();
    state.sort_by_id(&mut out);
    out
}
