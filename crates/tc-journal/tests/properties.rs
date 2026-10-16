//! The fold's property tests (§16.1). A team of devices edits one project from stale views
//! and exchanges random subsets (`sim`); each property names the defect that breaks it.
//! Every run counts the states its generator reached and fails when one was never reached.

// Helpers outside `#[test]` functions build known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
mod sim;
mod support;

use common::{Census, run_with_census};
use proptest::prelude::*;
use sim::{Forge, Kept, Mint, Sim, arb_script, choose, mint};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use support::{derived_bytes, same_fold};
use tc_journal::change::{Verified, parse_change};
use tc_journal::fold::{
    Anomaly, BookView, ConflictKind, DerivedFile, FoldInput, FoldState, MergeHow, Placement,
    ProjectKind, Resolved, decode_alignments, decode_decisions,
};
use tc_journal::json::Json;
use tc_journal::record::{BookPresence, CheckValue, CommentTarget, VerseValue};
use tc_journal::{BookId, Cause, Change, ChangeId, DeviceId, Record, RecordKey, ToolId};

/// Cases per property over team scripts. Each case folds every device's view after every
/// step, so a case is a few hundred folds.
const CASES: u32 = 64;

/// `TC_PROPERTY_CASES` runs more cases, for a long sweep.
fn cases() -> u32 {
    std::env::var("TC_PROPERTY_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(CASES)
}

/// The transports a union of changes travels by. The in-memory union is here; `tc-store`
/// runs the same properties over exchange zips and `tc-sync` over the local git server, by
/// implementing this trait.
pub trait Transport {
    fn carry(&self, lines: &[Verified]) -> Vec<FoldInput>;
}

pub struct InMemory;

impl Transport for InMemory {
    fn carry(&self, lines: &[Verified]) -> Vec<FoldInput> {
        lines
            .iter()
            .cloned()
            .map(|line| FoldInput::new(line, None))
            .collect()
    }
}

/// A small deterministic generator for orders and groupings.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

fn kind_name(kind: &ConflictKind) -> &'static str {
    match kind {
        ConflictKind::VerseText => "VerseText",
        ConflictKind::VerseStructure => "VerseStructure",
        ConflictKind::Passage => "Passage",
        ConflictKind::StructureIssue => "StructureIssue",
        ConflictKind::BookHeader => "BookHeader",
        ConflictKind::ChapterFront => "ChapterFront",
        ConflictKind::BookPresence => "BookPresence",
        ConflictKind::BookRemovedWhileEdited { .. } => "BookRemovedWhileEdited",
        ConflictKind::LastBookRemoved => "LastBookRemoved",
        ConflictKind::Alignment => "Alignment",
        ConflictKind::Check => "Check",
        ConflictKind::CheckMoved => "CheckMoved",
        ConflictKind::Comment => "Comment",
        ConflictKind::Pins => "Pins",
        ConflictKind::Setting => "Setting",
        ConflictKind::ProjectMeta => "ProjectMeta",
        ConflictKind::Versification => "Versification",
        ConflictKind::StoryBase => "StoryBase",
        ConflictKind::Frame => "Frame",
        ConflictKind::StoryRef => "StoryRef",
        ConflictKind::CarriedFile => "CarriedFile",
        ConflictKind::PathCase => "PathCase",
        ConflictKind::Unknown => "Unknown",
        ConflictKind::BookLayout => "BookLayout",
        ConflictKind::DeviceProfile => "DeviceProfile",
    }
}

/// What the team scripts reach, over a run.
const TEAM_STATES: &[&str] = &[
    "VerseText",
    "Passage",
    "BookHeader",
    "ChapterFront",
    "BookPresence",
    "BookRemovedWhileEdited",
    "Alignment",
    "Check",
    "CheckMoved",
    "Pins",
    "Setting",
    "ProjectMeta",
    "CarriedFile",
    "PathCase",
    "Unknown",
    "BookLayout",
    "criss-cross bases",
    "incomplete batch",
    "missing parent",
    "duplicate device",
    "quarantined",
    "invalid for burrito",
    "detached",
    "release",
    "settled by rule",
    "merged by units",
    "newer minor",
];

fn census_of(sim: &Sim, state: &FoldState, census: &Census) {
    for conflict in state.conflicts() {
        let name = kind_name(&conflict.kind);
        if TEAM_STATES.contains(&name) {
            census.saw(name);
        }
        if conflict.bases.len() >= 2 {
            census.saw("criss-cross bases");
        }
    }
    for anomaly in state.anomalies() {
        match anomaly {
            Anomaly::QuarantinedChange { .. } => census.saw("quarantined"),
            Anomaly::DuplicateDevice { .. } => census.saw("duplicate device"),
            Anomaly::InvalidForBurrito { .. } => census.saw("invalid for burrito"),
            Anomaly::IncompleteBatch { .. } => census.saw("incomplete batch"),
            Anomaly::MissingParent { .. } => census.saw("missing parent"),
            _ => {}
        }
    }
    for reached in &sim.reached {
        match *reached {
            "incomplete batch" => census.saw("incomplete batch"),
            "missing parent" => census.saw("missing parent"),
            "release" => census.saw("release"),
            _ => {}
        }
    }
    if state.detached().next().is_some() {
        census.saw("detached");
    }
    if sim.recipes.iter().any(|r| r.forge == Forge::Newer) {
        census.saw("newer minor");
    }
    for key in state.record_keys() {
        match state.key_state(&key).map(|k| k.state) {
            Some(Resolved::SettledByRule { .. }) => census.saw("settled by rule"),
            Some(Resolved::Merged {
                how: MergeHow::Units(_),
                ..
            }) => census.saw("merged by units"),
            _ => {}
        }
    }
}

fn team(
    cases: u32,
    extra: &'static [&'static str],
    test: impl Fn(Sim, &Census) -> Result<(), TestCaseError>,
) {
    let states: &'static [&'static str] = if extra.is_empty() {
        TEAM_STATES
    } else {
        Box::leak(
            TEAM_STATES
                .iter()
                .chain(extra.iter())
                .copied()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    };
    run_with_census(
        cases.max(self::cases()),
        arb_script(),
        states,
        |script, census| {
            let mut sim = Sim::run(&script, false);
            sim.sync_all();
            let state = sim.devices[0].state.clone();
            census_of(&sim, &state, census);
            test(sim, census)
        },
    );
}

fn body_of(state: &FoldState, id: &ChangeId) -> Option<tc_journal::ChangeBody> {
    state.body(id)
}

// ---------------------------------------------------------------------------------------
// I1
// ---------------------------------------------------------------------------------------

/// Fails if any rule reads arrival order, a map's iteration order, the clock or the
/// machine: devices holding the same set agree on everything, and so does any order,
/// grouping and duplication of the union.
#[test]
fn fold_converges_under_any_union() {
    team(CASES, &[], |sim, _| {
        let expected = &sim.devices[0].state;
        for device in &sim.devices[1..] {
            same_fold(expected, &device.state, "two devices");
        }
        let lines: Vec<Verified> = sim.items.iter().flatten().cloned().collect();
        let mut rng = Rng(lines.len() as u64 * 7 + 1);
        let mut shuffled = lines.clone();
        for _ in 0..rng.below(lines.len() / 4 + 1) {
            let i = rng.below(lines.len());
            shuffled.push(lines[i].clone());
        }
        rng.shuffle(&mut shuffled);
        let mut state = FoldState::new(ProjectKind::Bible);
        let mut at = 0;
        while at < shuffled.len() {
            let size = 1 + rng.below(12);
            let end = (at + size).min(shuffled.len());
            state.add(InMemory.carry(&shuffled[at..end]));
            at = end;
        }
        same_fold(expected, &state, "shuffled, grouped and repeated");
        Ok(())
    });
}

// ---------------------------------------------------------------------------------------
// I2
// ---------------------------------------------------------------------------------------

/// Fails if a change disappears, or an automated writer names a parent it did not hold.
#[test]
fn conservation() {
    team(CASES, &[], |sim, _| {
        let state = &sim.devices[0].state;
        let candidates: BTreeSet<ChangeId> = state
            .conflicts()
            .flat_map(|c| {
                c.candidates
                    .iter()
                    .flat_map(|x| std::iter::once(x.head).chain(x.also.iter().copied()))
            })
            .collect();
        let incomplete: BTreeSet<tc_journal::BatchId> = state
            .anomalies()
            .filter_map(|a| match a {
                Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
                _ => None,
            })
            .collect();
        let quarantined: BTreeSet<ChangeId> = state
            .anomalies()
            .filter_map(|a| match a {
                Anomaly::QuarantinedChange { change, .. } => Some(*change),
                _ => None,
            })
            .collect();
        let without_book: BTreeSet<BookId> = state
            .anomalies()
            .filter_map(|a| match a {
                Anomaly::RecordsWithoutBook { book, .. } => Some(*book),
                _ => None,
            })
            .collect();
        let detached: BTreeSet<RecordKey> = state.detached().cloned().collect();
        for (label, line) in sim.items.iter().enumerate() {
            let Some(line) = line else { continue };
            let recipe = &sim.recipes[label];
            prop_assert!(
                recipe.parents_held,
                "label {label} named a parent its author did not hold"
            );
            let id = line.id();
            let Verified::Change(change) = line else {
                prop_assert!(quarantined.contains(&id), "quarantined {label} not listed");
                continue;
            };
            let view = state.change(&id);
            prop_assert!(view.is_some(), "change {label} is gone");
            let view = view.unwrap();
            if !view.folded {
                prop_assert!(
                    incomplete.contains(&view.batch),
                    "unfolded {label} not listed"
                );
                continue;
            }
            let key = view.key.clone();
            let heads = state.heads(&key);
            if !heads.contains(&id) {
                // (c): a later change names it.
                prop_assert!(!state.history(&key).is_empty());
                continue;
            }
            let book = book_of(&change.body().record);
            if let Some(book) = book {
                if without_book.contains(&book) {
                    continue; // (e)
                }
                if state.book(book) == BookView::Removed {
                    continue; // (d)
                }
            }
            match state.key_state(&key).map(|k| k.state) {
                Some(Resolved::Conflict(_)) => {
                    prop_assert!(
                        candidates.contains(&id),
                        "head {label} of a conflicted record is no candidate"
                    );
                }
                Some(Resolved::SettledByRule { head, .. }) if head != id => {
                    prop_assert!(
                        matches!(change.body().cause, Cause::Seed),
                        "absorbed {label} is not a seed stub"
                    );
                }
                Some(_) => {
                    // (a): projected, and an attached record is placed or detached.
                    if matches!(key, RecordKey::Alignment(..)) {
                        let placed = state.placement(&key);
                        prop_assert!(
                            placed != Placement::NotAttached || detached.contains(&key),
                            "alignment {label} is on no screen"
                        );
                    }
                }
                None => prop_assert!(false, "head {label} has no state"),
            }
        }
        Ok(())
    });
}

fn book_of(record: &Record) -> Option<BookId> {
    match record {
        Record::BookHeader { book, .. }
        | Record::ChapterFront { book, .. }
        | Record::Verse { book, .. }
        | Record::BookLayout { book, .. }
        | Record::Alignment { book, .. } => Some(*book),
        Record::Check { key, .. } => key.book(),
        Record::Comment { value, .. } => match &value.target {
            CommentTarget::Verse { book, .. } => Some(*book),
            CommentTarget::Check(key) => key.book(),
            CommentTarget::Frame { .. } => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------------------
// Merges
// ---------------------------------------------------------------------------------------

fn value_json(record: &Record) -> Option<Json> {
    let json = tc_journal::canon::encode_record_json(record).ok()?;
    let Json::Obj(map) = json else { return None };
    let (_, inner) = map.into_iter().next()?;
    match inner {
        Json::Obj(mut fields) => fields.remove("value"),
        _ => None,
    }
}

fn is_seed_stub(state: &FoldState, id: &ChangeId) -> bool {
    let Some(body) = body_of(state, id) else {
        return false;
    };
    matches!(body.cause, Cause::Seed)
}

/// Fails if a merge rule picks between two different changes silently.
#[test]
fn concurrent_differences_are_visible() {
    team(CASES, &[], |sim, _| {
        let state = &sim.devices[0].state;
        for key in state.record_keys() {
            let heads = state.heads(&key);
            if heads.len() < 2 {
                continue;
            }
            let values: BTreeSet<Option<Json>> = heads
                .iter()
                .map(|h| body_of(state, h).and_then(|b| value_json(&b.record)))
                .collect();
            if values.len() < 2 {
                continue;
            }
            match state.key_state(&key).map(|k| k.state) {
                Some(Resolved::Conflict(id)) => {
                    prop_assert!(
                        state.conflict(&id).is_some(),
                        "{key:?}: conflict not listed"
                    );
                }
                Some(Resolved::Merged {
                    how: MergeHow::Units(_) | MergeHow::Union,
                    ..
                }) => {}
                Some(Resolved::SettledByRule { head, .. }) => {
                    for h in heads.iter().filter(|h| **h != head) {
                        prop_assert!(
                            is_seed_stub(state, h),
                            "{key:?}: a person's version absorbed"
                        );
                    }
                }
                Some(Resolved::Merged {
                    how: MergeHow::EqualValues,
                    ..
                }) => {
                    // Equal once the seed stubs are absorbed (§7.3).
                    let people: BTreeSet<Option<Json>> = heads
                        .iter()
                        .filter(|h| !is_seed_stub(state, h))
                        .map(|h| body_of(state, h).and_then(|b| value_json(&b.record)))
                        .collect();
                    prop_assert!(
                        people.len() <= 1,
                        "{key:?}: different values merged as equal"
                    );
                }
                other => {
                    let raws: Vec<String> = heads
                        .iter()
                        .map(|h| String::from_utf8_lossy(&state.raw(h).unwrap()).to_string())
                        .collect();
                    prop_assert!(false, "{key:?}: different heads but {other:?}: {raws:#?}")
                }
            }
        }
        Ok(())
    });
}

/// The ancestors of a change on its own record, by the parents stored in the changes.
fn ancestors(state: &FoldState, id: ChangeId) -> BTreeSet<ChangeId> {
    let key = state.change(&id).map(|v| v.key);
    let mut out = BTreeSet::new();
    let mut stack = vec![id];
    while let Some(x) = stack.pop() {
        if !out.insert(x) {
            continue;
        }
        if let Some(view) = state.change(&x) {
            for p in view.parents {
                if state.change(&p).map(|v| v.key) == key {
                    stack.push(p);
                }
            }
        }
    }
    out
}

fn bases(state: &FoldState, heads: &[ChangeId]) -> Vec<ChangeId> {
    let sets: Vec<BTreeSet<ChangeId>> = heads.iter().map(|h| ancestors(state, *h)).collect();
    let mut common = sets[0].clone();
    for s in &sets[1..] {
        common.retain(|c| s.contains(c));
    }
    common
        .iter()
        .copied()
        .filter(|c| {
            !common
                .iter()
                .any(|d| d != c && ancestors(state, *d).contains(c))
        })
        .collect()
}

/// The units of a value as the property sees them: every leaf path, which is finer than
/// any kind's units, so an agreed unit's leaves must each follow the rule.
fn leaves(json: Option<&Json>, path: &mut Vec<String>, out: &mut BTreeMap<String, Option<Json>>) {
    match json {
        Some(Json::Obj(map)) if !map.is_empty() => {
            for (k, v) in map {
                path.push(k.clone());
                leaves(Some(v), path, out);
                path.pop();
            }
        }
        other => {
            out.insert(path.join("/"), other.cloned());
        }
    }
}

/// Fails if the base choice lets one head override another: in a merged value, each leaf
/// is the value every head that changed it (relative to every base) wrote.
#[test]
fn merge_units_sound() {
    team(CASES, &[], |sim, census| {
        let state = &sim.devices[0].state;
        for key in state.record_keys() {
            let Some(Resolved::Merged {
                merged,
                how: MergeHow::Units(_),
            }) = state.key_state(&key).map(|k| k.state)
            else {
                continue;
            };
            let heads = state.heads(&key);
            let base_ids = bases(state, &heads);
            let leaf_map = |id: &ChangeId| {
                let mut out = BTreeMap::new();
                let value = body_of(state, id).and_then(|b| value_json(&b.record));
                leaves(value.as_ref(), &mut Vec::new(), &mut out);
                out
            };
            let head_leaves: Vec<_> = heads.iter().map(leaf_map).collect();
            let base_leaves: Vec<_> = base_ids.iter().map(leaf_map).collect();
            let mut merged_leaves = BTreeMap::new();
            leaves(
                value_json(&merged).as_ref(),
                &mut Vec::new(),
                &mut merged_leaves,
            );
            let paths: BTreeSet<String> = head_leaves
                .iter()
                .chain(base_leaves.iter())
                .chain(std::iter::once(&merged_leaves))
                .flat_map(|m| m.keys().cloned())
                .collect();
            for path in paths {
                // A leaf under an object that one version replaced whole is that object's.
                if paths_shadowed(&path, &head_leaves, &base_leaves) {
                    continue;
                }
                let get = |m: &BTreeMap<String, Option<Json>>| m.get(&path).cloned().flatten();
                let base: BTreeSet<Option<Json>> = base_leaves.iter().map(get).collect();
                let changed: BTreeSet<Option<Json>> = head_leaves
                    .iter()
                    .map(get)
                    .filter(|v| !base.contains(v) || base.len() != 1)
                    .collect();
                let got = get(&merged_leaves);
                let context = || {
                    let show =
                        |id: &ChangeId| body_of(state, id).and_then(|b| value_json(&b.record));
                    format!(
                        "heads {:?}\nbases {:?}\nmerged {:?}",
                        heads.iter().map(show).collect::<Vec<_>>(),
                        base_ids.iter().map(show).collect::<Vec<_>>(),
                        value_json(&merged)
                    )
                };
                if base.len() == 1 && changed.len() == 1 {
                    prop_assert_eq!(
                        &got,
                        changed.iter().next().unwrap(),
                        "{:?} at {}: {}",
                        key,
                        path,
                        context()
                    );
                } else if changed.is_empty() {
                    prop_assert_eq!(
                        &got,
                        base.iter().next().unwrap(),
                        "{:?} at {}: {}",
                        key,
                        path,
                        context()
                    );
                }
            }
            if base_ids.len() >= 2 {
                census.saw("criss-cross bases");
            }
        }
        Ok(())
    });
}

fn paths_shadowed(
    path: &str,
    heads: &[BTreeMap<String, Option<Json>>],
    bases: &[BTreeMap<String, Option<Json>>],
) -> bool {
    let all = heads.iter().chain(bases.iter());
    let parts: Vec<&str> = path.split('/').collect();
    all.into_iter()
        .any(|m| (1..parts.len()).any(|n| m.contains_key(&parts[..n].join("/"))))
}

// ---------------------------------------------------------------------------------------
// I4
// ---------------------------------------------------------------------------------------

/// Fails if the actor uses current heads instead of the token: a screen's token names
/// exactly what the screen displayed, so no head it did not display loses its place.
#[test]
fn stale_tokens_never_supersede_unseen() {
    team(
        CASES,
        &["stale screen", "conflicted screen"],
        |sim, census| {
            let state = &sim.devices[0].state;
            for write in &sim.writes {
                let screen = &write.screen;
                let shown_of = |id: &ChangeId| body_of(state, id).map(|b| b.record);
                if screen.conflicted {
                    census.saw("conflicted screen");
                    for t in &screen.token {
                        prop_assert_eq!(
                            shown_of(t),
                            screen.shown.clone(),
                            "a conflicted token lists a version the screen did not show"
                        );
                    }
                } else if screen.token != screen.heads {
                    prop_assert!(
                        screen.token.len() == 1 && shown_of(&screen.token[0]) == screen.shown,
                        "a settled token is neither every head nor the version shown"
                    );
                }
                let Some(Some(line)) = sim.items.get(write.label) else {
                    continue;
                };
                if let Some(change) = line.as_change() {
                    let mut parents = change.body().parents.clone();
                    parents.sort();
                    let mut token = screen.token.clone();
                    token.sort();
                    prop_assert_eq!(parents, token, "parents are the token");
                }
                if state.heads(&sim.recipes[write.label].key()) != screen.heads {
                    census.saw("stale screen");
                }
            }
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// §7.8
// ---------------------------------------------------------------------------------------

/// Fails if an incremental update forgets a key, a chapter or a book: after every step,
/// every device's state equals a fold of what it holds from scratch.
#[test]
fn incremental_equals_full() {
    run_with_census(cases(), arb_script(), TEAM_STATES, |script, census| {
        let mut sim = Sim::run(&script, true);
        sim.sync_all();
        let full = sim.union_state();
        same_fold(&full, &sim.devices[0].state, "incremental after sync");
        census_of(&sim, &full, census);
        Ok(())
    });
}

// ---------------------------------------------------------------------------------------
// I3
// ---------------------------------------------------------------------------------------

/// The heads, merges and conflicts of a fold, by label, without the provisional choices.
#[derive(Debug, PartialEq, Eq)]
struct Shape {
    heads: BTreeMap<String, BTreeSet<usize>>,
    states: BTreeMap<String, String>,
    conflicts: BTreeSet<(String, BTreeSet<usize>)>,
}

fn shape(state: &FoldState, labels: &HashMap<ChangeId, usize>) -> Shape {
    let label = |id: &ChangeId| labels.get(id).copied().unwrap_or(usize::MAX);
    let mut heads = BTreeMap::new();
    let mut states = BTreeMap::new();
    for key in state.record_keys() {
        let name = format!("{key:?}");
        heads.insert(name.clone(), state.heads(&key).iter().map(label).collect());
        let s = match state.key_state(&key).map(|k| k.state) {
            Some(Resolved::Settled { head }) => format!("settled {}", label(&head)),
            Some(Resolved::SettledByRule { .. }) => "settled by rule".into(),
            Some(Resolved::Merged { how, .. }) => format!("merged {how:?}"),
            Some(Resolved::Conflict(_)) => "conflict".into(),
            None => "none".into(),
        };
        states.insert(name, s);
    }
    let conflicts = state
        .conflicts()
        .filter(|c| {
            // Which book the last-book rule keeps, and which incoherences of the
            // provisional text are listed, are provisional choices (I3).
            !matches!(
                c.kind,
                ConflictKind::LastBookRemoved | ConflictKind::StructureIssue
            )
        })
        .map(|c| {
            (
                format!("{:?}", c.kind),
                c.candidates
                    .iter()
                    .flat_map(|x| std::iter::once(x.head).chain(x.also.iter().copied()))
                    .map(|h| label(&h))
                    .collect(),
            )
        })
        .collect();
    Shape {
        heads,
        states,
        conflicts,
    }
}

fn labels_of(lines: &[Option<Verified>]) -> HashMap<ChangeId, usize> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(l, line)| line.as_ref().map(|line| (line.id(), l)))
        .collect()
}

fn fold_lines(lines: &[Option<Verified>]) -> FoldState {
    FoldState::fold(
        ProjectKind::Bible,
        lines
            .iter()
            .flatten()
            .cloned()
            .map(|line| FoldInput::new(line, None)),
    )
}

/// Fails if a rule reads `at` or the device: the same history with permuted clocks and
/// device ids has the same heads, merges and conflicts.
#[test]
fn no_survival_by_time_or_identity() {
    team(CASES, &[], |sim, _| {
        let original = shape(&sim.devices[0].state, &sim.labels);
        let n = sim.recipes.len();
        let mut rng = Rng(n as u64 + 11);
        let mut times: Vec<i64> = sim.recipes.iter().map(|r| r.at).collect();
        rng.shuffle(&mut times);
        let mut order: Vec<usize> = (0..sim.devices.len()).collect();
        rng.shuffle(&mut order);
        let ids: Vec<DeviceId> = sim.devices.iter().map(|d| d.id).collect();
        let lines = mint(
            &sim.recipes,
            &Mint {
                at: &|label, _| times[label],
                device: &|d| ids[order[d]],
                newer: &|_, forge| forge,
            },
        );
        let state = fold_lines(&lines);
        let permuted = shape(&state, &labels_of(&lines));
        prop_assert_eq!(&original.heads, &permuted.heads);
        prop_assert_eq!(&original.conflicts, &permuted.conflicts);
        for (key, s) in &original.states {
            let other = &permuted.states[key];
            if s.starts_with("settled ") && other.starts_with("settled ") {
                continue;
            }
            prop_assert_eq!(s, other, "{}", key);
        }
        Ok(())
    });
}

/// Fails if an unknown field moves a change to another key or reaches a derived file.
/// Until a build of the previous minor exists to fold the same set, this emulates one: the
/// same history re-written with inert newer-minor fields on a third of its changes and on
/// the changes the team's newer devices wrote (an older device's edit of such a record
/// drops the fields, as it would) keeps every change
/// on its key and the same heads, projects no unknown field, and lists every conflict the
/// plain history lists; a conflict it adds is one whose versions differ in those fields.
#[test]
fn mixed_minor_versions_converge() {
    team(CASES, &[], |sim, _| mixed_minor_case(&sim));
}

fn mixed_minor_case(sim: &Sim) -> Result<(), TestCaseError> {
    let devices: Vec<DeviceId> = sim.devices.iter().map(|d| d.id).collect();
    let lines = mint(
        &sim.recipes,
        &Mint {
            at: &|_, at| at,
            device: &|d| devices[d],
            newer: &|label, forge| {
                if forge == Forge::Plain && label % 3 == 1 {
                    Forge::Newer
                } else {
                    forge
                }
            },
        },
    );
    let state = fold_lines(&lines);
    let labels = labels_of(&lines);
    // The same history written by this minor alone.
    let plain_lines = mint(
        &sim.recipes,
        &Mint {
            at: &|_, at| at,
            device: &|d| devices[d],
            newer: &|_, forge| {
                if forge == Forge::Newer {
                    Forge::Plain
                } else {
                    forge
                }
            },
        },
    );
    let plain = fold_lines(&plain_lines);
    for (label, line) in lines.iter().enumerate() {
        if let Some(Verified::Change(change)) = line {
            prop_assert_eq!(change.body().record.key(), sim.recipes[label].key());
        }
    }
    let a = shape(&plain, &labels_of(&plain_lines));
    let b = shape(&state, &labels);
    prop_assert_eq!(&a.heads, &b.heads);
    let newer_labels: BTreeSet<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            l.as_ref()
                .is_some_and(|l| String::from_utf8_lossy(l.raw()).contains("xNewer"))
        })
        .map(|(label, _)| label)
        .collect();
    // Versions that differ only in a newer field do not merge as equal values (F6 tests
    // equal values first), so such a record joins the passage group it diverges in, and
    // with it every record of that passage (a passage is judged whole, §7.3): a plain group
    // is still listed, grown by records of which at least one differs in a newer field.
    let differs_in_newer = |label: usize| {
        state
            .heads(&sim.recipes[label].key())
            .iter()
            .any(|h| labels.get(h).is_some_and(|l| newer_labels.contains(l)))
    };
    for (kind, heads) in a.conflicts.difference(&b.conflicts) {
        let grown = b.conflicts.iter().any(|(k, h)| {
            k == kind && h.is_superset(heads) && h.difference(heads).any(|l| differs_in_newer(*l))
        });
        prop_assert!(grown, "{kind} {heads:?} is lost with newer fields");
    }
    for conflict in state.conflicts() {
        let heads: BTreeSet<usize> = conflict
            .candidates
            .iter()
            .flat_map(|x| std::iter::once(x.head).chain(x.also.iter().copied()))
            .map(|h| labels.get(&h).copied().unwrap_or(usize::MAX))
            .collect();
        if a.conflicts
            .contains(&(format!("{:?}", conflict.kind), heads.clone()))
        {
            continue;
        }
        if matches!(
            conflict.kind,
            ConflictKind::LastBookRemoved | ConflictKind::StructureIssue
        ) {
            continue;
        }
        let involved = heads
            .iter()
            .copied()
            .chain(conflict.bases.iter().filter_map(|b| labels.get(b).copied()));
        let mut involved = involved;
        prop_assert!(
            involved.any(|l| newer_labels.contains(&l)),
            "{:?} {heads:?} added without a newer field",
            conflict.kind
        );
    }
    for (path, bytes) in derived_bytes(&state) {
        prop_assert!(
            !String::from_utf8_lossy(&bytes).contains("xNewer"),
            "{path} projects an unknown field"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// §8.4
// ---------------------------------------------------------------------------------------

/// Fails if a resolution needs an ancestry condition: every conflict can be resolved,
/// with any choice, until none is left.
#[test]
fn resolution_always_possible() {
    run_with_census(
        cases(),
        (arb_script(), any::<u64>()),
        TEAM_STATES,
        |(script, seed), census| {
            let mut sim = Sim::run(&script, false);
            sim.sync_all();
            census_of(&sim, &sim.devices[0].state.clone(), census);
            let mut rng = Rng(seed);
            for _ in 0..200 {
                let state = sim.devices[0].state.clone();
                let open: Vec<_> = state
                    .conflicts()
                    .filter(|c| c.kind != ConflictKind::Unknown)
                    .cloned()
                    .collect();
                let Some(conflict) = open.first() else { break };
                let (pick, how) = (rng.next() as u8, rng.next() as u8);
                let chosen = choose(&state, conflict, pick, how)
                    .or_else(|| choose(&state, conflict, pick, 0));
                prop_assert!(chosen.is_some(), "no choice for {:?}", conflict.kind);
                let chosen = chosen.unwrap();
                // Each version the person picked is what its record keeps: no clock and no
                // other version of the same side stands in for it.
                for head in &chosen.picks {
                    // A candidate, or an earlier version from its record's History.
                    let picked = state.body(head).map(|b| b.record);
                    prop_assert!(picked.is_some(), "{head:?} is no version");
                    let picked = picked.unwrap();
                    let written = chosen
                        .planned
                        .iter()
                        .find(|p| p.record.key() == picked.key())
                        .map(|p| p.record.clone());
                    match chosen.kept {
                        Kept::Exact => prop_assert_eq!(
                            written.as_ref(),
                            Some(&picked),
                            "{:?}: the picked version is not what is kept",
                            conflict.kind
                        ),
                        Kept::Words => prop_assert_eq!(
                            written.as_ref().and_then(support::words_of),
                            support::words_of(&picked),
                            "{:?} at {:?} {:?} (how {}): the picked words are not what is kept: {:?}; candidates {:?}",
                            conflict.kind,
                            conflict.key,
                            conflict.also_keys,
                            how % 4,
                            chosen.planned.iter().map(|p| &p.record).collect::<Vec<_>>(),
                            conflict
                                .candidates
                                .iter()
                                .map(|c| (c.head, &c.value))
                                .collect::<Vec<_>>()
                        ),
                        Kept::AsItIs => prop_assert!(
                            written.is_none(),
                            "{:?}: the picked file is replaced",
                            conflict.kind
                        ),
                    }
                }
                sim.write_planned(0, chosen.planned);
                let after = &sim.devices[0].state;
                prop_assert!(
                    after.conflict(&conflict.id).is_none(),
                    "{:?} still listed",
                    conflict.kind
                );
            }
            let left: Vec<_> = sim.devices[0]
                .state
                .conflicts()
                .filter(|c| c.kind != ConflictKind::Unknown)
                .map(|c| c.kind)
                .collect();
            prop_assert!(left.is_empty(), "left {left:?}");
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// Projection
// ---------------------------------------------------------------------------------------

fn generated_word(token: &str) -> bool {
    let generated = token.starts_with('d') && token[1..].contains('w');
    let imported = token.starts_with("TIT") && token.contains('v');
    (generated || imported) && token.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Fails if the coherence pass drops a verse, a passage is judged verse by verse, or the
/// layout skips a record: per chapter, the USFM holds exactly the words of the projected
/// texts, and every other word comes from a listed incoherence.
#[test]
fn provisional_usfm_never_drops_words() {
    team(CASES, &["structure issue"], |sim, census| {
        let state = &sim.devices[0].state;
        if !state.book(BookId::Titus).projects() {
            return Ok(());
        }
        let issues: BTreeSet<u16> = state
            .conflicts()
            .filter(|c| c.kind == ConflictKind::StructureIssue)
            .filter_map(|c| match &c.key {
                RecordKey::Verse(_, ch, _) | RecordKey::ChapterFront(_, ch) => Some(ch.get()),
                _ => None,
            })
            .collect();
        if !issues.is_empty() {
            census.saw("structure issue");
        }
        let mut expected: BTreeMap<u16, Vec<String>> = BTreeMap::new();
        for key in state.record_keys() {
            let RecordKey::Verse(BookId::Titus, ch, _) = key else {
                continue;
            };
            if let Some(Record::Verse {
                value: VerseValue::Text { words, .. },
                ..
            }) = state.projected(&key)
            {
                expected.entry(ch.get()).or_default().extend(
                    words
                        .split_whitespace()
                        .filter(|w| generated_word(w))
                        .map(str::to_owned),
                );
            }
        }
        for chapter in 1..=2u16 {
            let usfm = state
                .chapter_usfm(BookId::Titus, chapter)
                .unwrap_or_default();
            let mut got: Vec<String> = usfm
                .split_whitespace()
                .filter(|w| generated_word(w))
                .map(str::to_owned)
                .collect();
            got.sort();
            let mut want = expected.remove(&chapter).unwrap_or_default();
            want.sort();
            if issues.contains(&chapter) {
                for w in &want {
                    prop_assert!(got.contains(w), "chapter {chapter} lost {w}");
                }
            } else {
                prop_assert_eq!(got, want, "chapter {}:\n{}", chapter, usfm);
            }
        }
        Ok(())
    });
}

/// Fails if an attached record is left on an identity no screen shows.
#[test]
fn every_head_reachable_from_a_screen() {
    team(CASES, &[], |sim, _| {
        let state = &sim.devices[0].state;
        let candidates: BTreeSet<ChangeId> = state
            .conflicts()
            .flat_map(|c| {
                c.candidates
                    .iter()
                    .flat_map(|x| std::iter::once(x.head).chain(x.also.iter().copied()))
            })
            .collect();
        let detached: BTreeSet<RecordKey> = state.detached().cloned().collect();
        let decisions: BTreeSet<RecordKey> = state
            .render(&DerivedFile::Decisions(
                ToolId::TranslationNotes,
                BookId::Titus,
            ))
            .map(|bytes| {
                decode_decisions(ToolId::TranslationNotes, BookId::Titus, &bytes)
                    .expect("decodes")
                    .into_iter()
                    .map(|r| r.key())
                    .collect()
            })
            .unwrap_or_default();
        for key in state.record_keys() {
            for head in state.heads(&key) {
                if candidates.contains(&head) || detached.contains(&key) {
                    continue;
                }
                let Some(body) = body_of(state, &head) else {
                    continue;
                };
                let book = book_of(&body.record);
                if book.is_some_and(|b| !state.book(b).projects()) {
                    continue; // History of a removed book, or a book no presence names.
                }
                match &body.record {
                    Record::Alignment { .. } => {
                        prop_assert!(state.placement(&key) != Placement::NotAttached, "{key:?}");
                    }
                    Record::Comment { value, .. } => {
                        if let CommentTarget::Verse { .. } = value.target {
                            prop_assert!(
                                state.placement(&key) != Placement::NotAttached,
                                "{key:?}"
                            );
                        }
                    }
                    Record::Check {
                        value: CheckValue::Decision(_),
                        ..
                    } => {
                        // A decision head merged into a move is shown at the moved check.
                        let shown_here = matches!(
                            state.projected(&key),
                            Some(Record::Check {
                                value: CheckValue::Decision(_),
                                ..
                            })
                        );
                        if shown_here && state.key_state(&key).is_some_and(|k| !matches!(k.state, Resolved::SettledByRule { head: h, .. } if h != head)) {
                            prop_assert!(
                                decisions.contains(&key),
                                "{key:?} is in no decision file: {:?}, projected {:?}, detached {:?}",
                                state.key_state(&key),
                                state.projected(&key),
                                detached
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    });
}

/// Fails if removal visibility ignores a record kind: every change of a removed book that
/// the removers did not hold keeps the book projected and is counted.
#[test]
fn book_removal_hides_nothing_unseen() {
    team(
        CASES,
        &["removed with unseen work", "removed, all seen"],
        |sim, census| {
            let state = &sim.devices[0].state;
            for book in [BookId::Titus, BookId::Ruth] {
                let presence = RecordKey::BookPresence(book);
                let heads = state.heads(&presence);
                let removals: Vec<Vec<tc_journal::ShortId>> = heads
                    .iter()
                    .filter_map(|h| match body_of(state, h).map(|b| b.record) {
                        Some(Record::BookPresence {
                            value: BookPresence::Removed { seen },
                            ..
                        }) => Some(seen),
                        _ => None,
                    })
                    .collect();
                if removals.is_empty() || removals.len() != heads.len() {
                    continue;
                }
                let mut unseen = 0u32;
                for (label, line) in sim.items.iter().enumerate() {
                    let Some(Verified::Change(change)) = line else {
                        continue;
                    };
                    if !sim::belongs_to(&sim.recipes[label], book) {
                        continue;
                    }
                    if !state.change(&change.id()).is_some_and(|v| v.folded) {
                        continue;
                    }
                    let closure_has = |seen: &Vec<tc_journal::ShortId>| {
                        state
                            .history(&change.body().record.key())
                            .iter()
                            .filter(|c| seen.contains(&c.short()))
                            .any(|c| ancestors(state, *c).contains(&change.id()))
                    };
                    if !removals.iter().any(closure_has) {
                        unseen += 1;
                    }
                }
                if unseen == 0 {
                    census.saw("removed, all seen");
                    if !matches!(state.book(book), BookView::KeptAsLast { .. }) {
                        prop_assert_eq!(state.book(book), BookView::Removed);
                    }
                } else {
                    census.saw("removed with unseen work");
                    prop_assert!(
                        matches!(state.book(book), BookView::RemovedWhileEdited { unseen: n, .. } if n == unseen),
                        "{:?}: {} unseen, {:?}",
                        book,
                        unseen,
                        state.book(book)
                    );
                }
            }
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// I5
// ---------------------------------------------------------------------------------------

/// One input of the never-panics generator.
#[derive(Debug, Clone)]
enum Odd {
    /// Arbitrary bytes as a change line's JSON.
    Bytes(Vec<u8>),
    /// An arbitrary body; parents and batch drawn from the set so far.
    Body(Box<tc_journal::ChangeBody>, u8, u8),
    /// Arbitrary bytes as a pack.
    Pack(Vec<u8>),
}

fn arb_odd() -> impl Strategy<Value = Odd> {
    prop_oneof![
        prop::collection::vec(any::<u8>(), 0..200).prop_map(Odd::Bytes),
        (common::arb_body(), any::<u8>(), any::<u8>()).prop_map(|(b, p, n)| Odd::Body(
            Box::new(b),
            p,
            n
        )),
        prop::collection::vec(any::<u8>(), 0..300).prop_map(Odd::Pack),
    ]
}

/// Fails if any step can fail: arbitrary bytes, odd change sets (unknown parents, parents
/// on other records, duplicate devices, giant batches) fold without a panic, every line
/// that is not folded is listed, and the projection builds.
#[test]
fn fold_never_panics() {
    run_with_census(
        256,
        prop::collection::vec(arb_odd(), 1..40),
        &[
            "quarantined",
            "incomplete batch",
            "folded",
            "missing parent",
            "duplicate device",
        ],
        |odds, census| {
            let mut lines: Vec<Verified> = Vec::new();
            let mut ids: Vec<ChangeId> = Vec::new();
            for odd in odds {
                match odd {
                    Odd::Bytes(bytes) => {
                        let raw: std::sync::Arc<[u8]> = bytes.into();
                        lines.push(parse_change(ChangeId::of(&raw), raw));
                    }
                    Odd::Body(mut body, p, n) => {
                        if !ids.is_empty() && p.is_multiple_of(2) {
                            body.parents = vec![ids[usize::from(p) % ids.len()]];
                        }
                        if n.is_multiple_of(5) {
                            body.batch =
                                tc_journal::BatchRef::new(body.batch.id, 1 + u32::from(n) * 1000)
                                    .unwrap_or(body.batch);
                        }
                        if n.is_multiple_of(7)
                            && let Some(c) = lines.iter().rev().find_map(|l| l.as_change())
                        {
                            body.device = c.body().device;
                            body.seq = c.body().seq;
                        }
                        if let Ok(change) = Change::new(*body) {
                            ids.push(change.id());
                            lines.push(Verified::Change(change));
                        }
                    }
                    Odd::Pack(bytes) => {
                        let read = tc_journal::pack::read_pack(
                            "packs/x.tcpack",
                            &bytes,
                            common::project(),
                        );
                        lines.extend(read.entries);
                    }
                }
            }
            let state = FoldState::fold(
                ProjectKind::Bible,
                lines.iter().cloned().map(|l| FoldInput::new(l, None)),
            );
            let _ = state.conflicts().count();
            for file in state.derived_files() {
                prop_assert!(state.render(&file).is_some(), "{file:?}");
            }
            let _ = state.metadata_source((1, 0));
            let incomplete: BTreeSet<tc_journal::BatchId> = state
                .anomalies()
                .filter_map(|a| match a {
                    Anomaly::IncompleteBatch { batch, .. } => Some(*batch),
                    _ => None,
                })
                .collect();
            for line in &lines {
                match line {
                    Verified::Quarantined(q) => {
                        census.saw("quarantined");
                        prop_assert!(
                            state.anomalies().any(|a| matches!(a, Anomaly::QuarantinedChange { change, .. } if *change == q.id)),
                            "a quarantined line is not listed"
                        );
                    }
                    Verified::Change(c) => {
                        let view = state.change(&c.id());
                        prop_assert!(view.is_some());
                        if view.unwrap().folded {
                            census.saw("folded");
                        } else {
                            census.saw("incomplete batch");
                            prop_assert!(incomplete.contains(&c.body().batch.id));
                        }
                    }
                }
            }
            for anomaly in state.anomalies() {
                match anomaly {
                    Anomaly::MissingParent { .. } => census.saw("missing parent"),
                    Anomaly::DuplicateDevice { .. } => census.saw("duplicate device"),
                    _ => {}
                }
            }
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// metadata.json
// ---------------------------------------------------------------------------------------

/// Fails if any value can make `metadata.json` fail: arbitrary JSON as project details,
/// pin slots and book scopes always builds a valid document, and every value it could not
/// use is listed.
#[test]
fn arbitrary_meta_never_blocks_commit() {
    let field = prop_oneof![
        Just(tc_journal::MetaField::Name),
        Just(tc_journal::MetaField::Abbreviation),
        Just(tc_journal::MetaField::Description),
        Just(tc_journal::MetaField::Languages),
        Just(tc_journal::MetaField::Copyright),
        common::arb_meta_field(),
    ];
    let valid = prop::sample::select(vec![
        (
            tc_journal::MetaField::Name,
            sim::tc_obj(&[("en", Json::from("Tito"))]),
        ),
        (
            tc_journal::MetaField::Description,
            sim::tc_obj(&[("es", Json::from("Una traducción"))]),
        ),
        (
            tc_journal::MetaField::Copyright,
            sim::tc_obj(&[(
                "shortStatements",
                Json::Arr(vec![sim::tc_obj(&[("statement", Json::from("CC0"))])]),
            )]),
        ),
    ]);
    let detail = prop_oneof![(field, common::arb_json()), valid];
    let strategy = (
        prop::collection::vec(detail, 0..6),
        prop::collection::vec((common::arb_slot(), common::arb_json()), 0..4),
        common::arb_scope(),
    );
    run_with_census(
        256,
        strategy,
        &["used", "invalid"],
        |(details, slots, scope), census| {
            let mut ana = support::Dev::new(1);
            let mut changes = support::imported_book(&mut ana, &[2]);
            let presence = support::on(&changes, &RecordKey::BookPresence(BookId::Titus));
            changes.push(ana.one(
                Record::BookPresence {
                    book: BookId::Titus,
                    value: BookPresence::Present {
                        scope,
                        how: tc_journal::record::PresentHow::Added,
                    },
                },
                vec![presence.id()],
            ));
            for (field, value) in details {
                changes.push(ana.one(
                    Record::ProjectMeta {
                        field,
                        value: Some(value),
                    },
                    vec![],
                ));
            }
            let mut group = tc_journal::record::PinGroupValue::default();
            for (slot, value) in slots {
                group.0.insert(slot, tc_journal::record::SlotValue(value));
            }
            changes.push(ana.one(
                Record::Pins {
                    group: tc_journal::PinGroup::Primary,
                    value: group,
                },
                vec![],
            ));
            let state = support::fold(&changes);
            // Builds and validates, or panics here.
            let _ = derived_bytes(&state);
            let source = state.metadata_source((1, 0));
            for key in state.record_keys() {
                let RecordKey::ProjectMeta(field) = &key else {
                    continue;
                };
                let Some(Record::ProjectMeta {
                    value: Some(value), ..
                }) = state.projected(&key)
                else {
                    continue;
                };
                let path = match field {
                    tc_journal::MetaField::Name => "identification.name".to_owned(),
                    tc_journal::MetaField::Abbreviation => "identification.abbreviation".to_owned(),
                    tc_journal::MetaField::Description => "identification.description".to_owned(),
                    tc_journal::MetaField::Languages => "languages".to_owned(),
                    tc_journal::MetaField::Copyright => "copyright".to_owned(),
                    tc_journal::MetaField::Extra(path) => path.as_str().to_owned(),
                };
                let used = source.fields.get(&path);
                if used == Some(&serde_json::Value::from(&value)) {
                    census.saw("used");
                } else {
                    census.saw("invalid");
                    prop_assert!(
                        state.anomalies().any(
                            |a| matches!(a, Anomaly::InvalidForBurrito { key: k, .. } if *k == key)
                        ),
                        "{key:?} not used and not listed"
                    );
                }
            }
            Ok(())
        },
    );
}

// ---------------------------------------------------------------------------------------
// Sidecars
// ---------------------------------------------------------------------------------------

/// Fails if an outside-edit decode would invent a change: decoding a rendered sidecar
/// gives back the records it was rendered from.
#[test]
fn sidecar_decode_render() {
    let strategy = (
        prop::collection::vec(common::arb_alignment(), 0..5),
        prop::collection::vec((common::arb_decision(), "[a-z]{1,6}", 1u16..=3), 0..5),
    );
    run_with_census(
        256,
        strategy,
        &["alignment", "decision"],
        |(alignments, decisions), census| {
            let mut ana = support::Dev::new(1);
            let mut changes = support::imported_book(&mut ana, &[5]);
            let mut want_alignments = Vec::new();
            for (i, mut value) in alignments.into_iter().enumerate() {
                let v = i as u16 + 1;
                value.span_end = sim::vno(v);
                let record = Record::Alignment {
                    book: BookId::Titus,
                    chapter: sim::chno(1),
                    verse: sim::vno(v),
                    value,
                };
                if let Ok(change) = Change::new(tc_journal::ChangeBody {
                    v: 0,
                    record: record.clone(),
                    parents: vec![],
                    device: ana.id,
                    seq: tc_journal::DeviceSeq(1_000 + i as u64),
                    at: tc_journal::WallTime(1_760_000_000_000),
                    batch: tc_journal::BatchRef::new(
                        tc_journal::BatchId::from_bytes([i as u8 + 1; 16]),
                        1,
                    )
                    .unwrap(),
                    cause: Cause::Edit,
                }) {
                    changes.push(change);
                    want_alignments.push(record);
                    census.saw("alignment");
                }
            }
            let mut want_decisions = BTreeMap::new();
            for (i, (value, id, v)) in decisions.into_iter().enumerate() {
                let mut key = support::check_key("abcd", 1, v);
                key.check_id = tc_journal::CheckId::parse(&id).unwrap();
                let record = support::check(&key, value);
                if let Ok(change) = Change::new(tc_journal::ChangeBody {
                    v: 0,
                    record: record.clone(),
                    parents: vec![],
                    device: ana.id,
                    seq: tc_journal::DeviceSeq(2_000 + i as u64),
                    at: tc_journal::WallTime(1_760_000_000_000),
                    batch: tc_journal::BatchRef::new(
                        tc_journal::BatchId::from_bytes([100 + i as u8; 16]),
                        1,
                    )
                    .unwrap(),
                    cause: Cause::Edit,
                }) && let std::collections::btree_map::Entry::Vacant(slot) =
                    want_decisions.entry(record.key())
                {
                    slot.insert(record);
                    changes.push(change);
                    census.saw("decision");
                }
            }
            let state = support::fold(&changes);
            if !want_alignments.is_empty() {
                let bytes = state
                    .render(&DerivedFile::Alignments(BookId::Titus))
                    .expect("alignments");
                let mut got = decode_alignments(BookId::Titus, &bytes).expect("decodes");
                got.sort_by_key(|r| format!("{:?}", r.key()));
                prop_assert_eq!(got, want_alignments);
            }
            if !want_decisions.is_empty() {
                let bytes = state
                    .render(&DerivedFile::Decisions(
                        ToolId::TranslationNotes,
                        BookId::Titus,
                    ))
                    .expect("decisions");
                let got: BTreeMap<RecordKey, Record> =
                    decode_decisions(ToolId::TranslationNotes, BookId::Titus, &bytes)
                        .expect("decodes")
                        .into_iter()
                        .map(|r| (r.key(), r))
                        .collect();
                prop_assert_eq!(got, want_decisions);
            }
            Ok(())
        },
    );
}

/// A history `mixed_minor_versions_converge` found: in it, section saves leave verses 3
/// to 6 with two equal heads each, merged in the plain history; with newer fields, three
/// of them differ only in those fields, so they and their whole passage join the passage
/// group. The plain Passage conflict must still be listed, grown, not dropped.
#[test]
fn a_passage_grown_by_newer_fields_keeps_the_plain_conflict() {
    let script = sim::Script {
        devices: 3,
        ops: vec![
            sim::Op::Edit {
                dev: 0,
                at: 20,
                stale: false,
                newer: false,
            },
            sim::Op::BookLayout { dev: 0, seed: 0 },
            sim::Op::Fork {
                a: 0,
                b: 16,
                what: 27,
                at: 1,
            },
            sim::Op::Fork {
                a: 21,
                b: 121,
                what: 4,
                at: 0,
            },
            sim::Op::Comment { dev: 0, on: 0 },
            sim::Op::Fork {
                a: 77,
                b: 202,
                what: 30,
                at: 38,
            },
            sim::Op::Renumber { dev: 33, at: 235 },
            sim::Op::Fork {
                a: 1,
                b: 213,
                what: 255,
                at: 247,
            },
            sim::Op::Resolve {
                dev: 133,
                pick: 14,
                how: 172,
            },
            sim::Op::Drag { dev: 27, at: 196 },
            sim::Op::Renumber { dev: 212, at: 26 },
            sim::Op::Edit {
                dev: 90,
                at: 60,
                stale: false,
                newer: false,
            },
            sim::Op::Resolve {
                dev: 226,
                pick: 171,
                how: 187,
            },
            sim::Op::Decide {
                dev: 244,
                check: 186,
                what: 5,
                stale: true,
            },
            sim::Op::Align {
                dev: 239,
                at: 84,
                current: false,
                newer: false,
            },
            sim::Op::Resolve {
                dev: 57,
                pick: 21,
                how: 75,
            },
            sim::Op::Layout { dev: 61, at: 249 },
            sim::Op::Align {
                dev: 59,
                at: 203,
                current: true,
                newer: false,
            },
            sim::Op::Edit {
                dev: 67,
                at: 134,
                stale: true,
                newer: false,
            },
            sim::Op::Carried {
                dev: 42,
                path: 215,
                value: 135,
            },
            sim::Op::Fork {
                a: 38,
                b: 153,
                what: 194,
                at: 101,
            },
            sim::Op::Align {
                dev: 81,
                at: 164,
                current: false,
                newer: false,
            },
            sim::Op::Setting {
                dev: 175,
                path: 69,
                value: 19,
            },
            sim::Op::Resolve {
                dev: 100,
                pick: 22,
                how: 255,
            },
            sim::Op::Stray {
                dev: 135,
                at: 31,
                covered: true,
            },
            sim::Op::Edit {
                dev: 18,
                at: 194,
                stale: true,
                newer: false,
            },
            sim::Op::Decide {
                dev: 217,
                check: 134,
                what: 198,
                stale: false,
            },
            sim::Op::Restore {
                dev: 6,
                ruth: false,
                text: false,
            },
        ],
    };
    let mut sim = Sim::run(&script, false);
    sim.sync_all();
    if let Err(e) = mixed_minor_case(&sim) {
        panic!("{e}");
    }
}
