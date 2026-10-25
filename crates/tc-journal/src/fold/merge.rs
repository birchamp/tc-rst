//! F6 for one record (§7.3): equal values, the stub rules, divergence from a passage
//! batch, and the three-way merge on merge units.

use super::meta::{BatchIdx, ChangeIdx, Form, KeyIdx, Shape, VerseShape};
use super::state::{FoldState, Local, MergeHow};
use super::{ConflictKind, UnitName, UnitValue};
use crate::json::{Json, JsonMap};
use crate::keys::RecordKey;
use crate::record::{BatchReleaseValue, Record};
use smallvec::SmallVec;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// The folded parents of `c` on its own key.
pub(crate) fn key_parents(state: &FoldState, c: ChangeIdx) -> SmallVec<[ChangeIdx; 2]> {
    let meta = state.meta(c);
    state
        .parents(c)
        .filter_map(|p| state.parent_idx(p))
        .filter(|p| {
            let pm = state.meta(*p);
            pm.active() && pm.key == meta.key
        })
        .collect()
}

/// `c` and its ancestors on its key.
pub(crate) fn ancestors_or_self(state: &FoldState, c: ChangeIdx) -> BTreeSet<ChangeIdx> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![c];
    while let Some(x) = stack.pop() {
        if seen.insert(x) {
            stack.extend(key_parents(state, x));
        }
    }
    seen
}

/// The ancestry of a record's heads: each head's ancestors-or-self, the common ancestors
/// and the bases (§7.3).
pub(crate) struct Ancestry {
    pub of_head: Vec<BTreeSet<ChangeIdx>>,
    /// Ancestors of every head.
    pub common: BTreeSet<ChangeIdx>,
    /// The maximal elements of `common`.
    pub bases: Vec<ChangeIdx>,
}

pub(crate) fn ancestry(state: &FoldState, heads: &[ChangeIdx]) -> Ancestry {
    let of_head: Vec<BTreeSet<ChangeIdx>> =
        heads.iter().map(|h| ancestors_or_self(state, *h)).collect();
    let mut common: BTreeSet<ChangeIdx> = of_head.first().cloned().unwrap_or_default();
    for set in of_head.iter().skip(1) {
        common.retain(|c| set.contains(c));
    }
    // A head is never an ancestor of another head, so with two or more heads `common`
    // holds no head.
    let mut below: BTreeSet<ChangeIdx> = BTreeSet::new();
    for c in &common {
        for p in key_parents(state, *c) {
            below.extend(ancestors_or_self(state, p));
        }
    }
    let mut bases: Vec<ChangeIdx> = common
        .iter()
        .copied()
        .filter(|c| !below.contains(c))
        .collect();
    state.sort_by_id(&mut bases);
    Ancestry {
        of_head,
        common,
        bases,
    }
}

/// Div(k) of §7.3: changes that are, or are ancestors of, some head but not ancestors of
/// every head.
pub(crate) fn div(ancestry: &Ancestry) -> BTreeSet<ChangeIdx> {
    let mut out = BTreeSet::new();
    for set in &ancestry.of_head {
        out.extend(set.iter().copied().filter(|c| !ancestry.common.contains(c)));
    }
    out
}

/// A change whose bytes the fold's source cannot read on this computer (§2.6): its value
/// is unknown, which is never the same as a value that is absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unreadable;

/// The record value of a change as JSON (`Ok(None)` when the record has no value member).
pub(crate) fn value_of(state: &FoldState, c: ChangeIdx) -> Result<Option<Json>, Unreadable> {
    state.count(super::Counter::ValuesDecoded, 1);
    let bytes = state.bytes(c).ok_or(Unreadable)?;
    Ok(super::meta::value_json(&bytes))
}

/// The record of a change, decoded from its stored bytes.
pub(crate) fn record_of(state: &FoldState, c: ChangeIdx) -> Option<Record> {
    if let Some(record) = state.records.as_ref().and_then(|r| r.get(&c)) {
        return Some((**record).clone());
    }
    decode_record(state, c)
}

/// [`record_of`] shared: a record decoded ahead is not copied.
pub(crate) fn record_arc(state: &FoldState, c: ChangeIdx) -> Option<Arc<Record>> {
    if let Some(record) = state.records.as_ref().and_then(|r| r.get(&c)) {
        return Some(record.clone());
    }
    decode_record(state, c).map(Arc::new)
}

fn decode_record(state: &FoldState, c: ChangeIdx) -> Option<Record> {
    state.count(super::Counter::ValuesDecoded, 1);
    super::meta::decode_record(&state.bytes(c)?)
}

fn seed_stub_kind(key: &RecordKey) -> bool {
    matches!(
        key,
        RecordKey::Verse(..)
            | RecordKey::BookHeader(_)
            | RecordKey::ChapterFront(..)
            | RecordKey::BookLayout(_)
    )
}

/// F6 for key `k`: its own resolution and the passage batches it diverges from.
///
/// A head or base whose value cannot be read (§2.6) is never taken as equal to another or
/// as absent: the record is not merged and not settled by equal values, but listed as a
/// conflict marked unreadable until its values can be read and it is judged again. Rules
/// that read no value (the stub rules, divergence from a passage batch) still apply.
pub(crate) fn resolve_local(state: &FoldState, k: KeyIdx) -> (Local, SmallVec<[BatchIdx; 1]>) {
    let entry = state.entry(k);
    resolve_heads(state, &entry.key, entry.heads.to_vec())
}

/// [`resolve_local`] for `heads` of `key`, which need not be its current heads: what a
/// screen showed when they were.
pub(crate) fn resolve_heads(
    state: &FoldState,
    key: &RecordKey,
    heads: Vec<ChangeIdx>,
) -> (Local, SmallVec<[BatchIdx; 1]>) {
    match heads.len() {
        0 => return (Local::Empty, SmallVec::new()),
        1 => return (Local::Settled(heads[0]), SmallVec::new()),
        _ => {}
    }
    if let RecordKey::BatchRelease(batch) = key {
        // Grow-only: the union of what every release lists (§7.4).
        let mut changes: BTreeSet<crate::ids::ChangeId> = BTreeSet::new();
        for h in &heads {
            if let Shape::Release { changes: listed } = state.shape(*h) {
                changes.extend(listed.iter().copied());
            }
        }
        let record = Record::BatchRelease {
            batch: *batch,
            value: BatchReleaseValue {
                changes: changes.into_iter().collect(),
            },
        };
        return (
            Local::Merged(Box::new((Arc::new(record), MergeHow::Union))),
            SmallVec::new(),
        );
    }
    let values: Vec<Result<Option<Json>, Unreadable>> =
        heads.iter().map(|h| value_of(state, *h)).collect();
    let mut unreadable = values.iter().any(Result::is_err);
    if !unreadable && values.iter().all(|v| *v == values[0]) {
        return equal_values(state, &key, &heads);
    }
    // Stub rules: a seed stub is absorbed by any head that is not one; if every head is a
    // stub, the smallest id stays.
    let mut heads = heads;
    let mut values = values;
    if seed_stub_kind(key) {
        let stubs: Vec<bool> = heads.iter().map(|h| state.meta(*h).stub()).collect();
        if stubs.iter().all(|s| *s) {
            return (
                Local::ByRule {
                    head: heads[0],
                    rule: super::AutoRule::SeedStubs,
                    unreadable: false,
                },
                SmallVec::new(),
            );
        }
        if stubs.iter().any(|s| *s) {
            let kept: Vec<(ChangeIdx, Result<Option<Json>, Unreadable>)> = heads
                .iter()
                .zip(values)
                .zip(&stubs)
                .filter(|(_, stub)| !**stub)
                .map(|(pair, _)| (pair.0.to_owned(), pair.1))
                .collect();
            if kept.len() == 1 {
                return (
                    Local::ByRule {
                        head: kept[0].0,
                        rule: super::AutoRule::SeedStubs,
                        unreadable: false,
                    },
                    SmallVec::new(),
                );
            }
            heads = kept.iter().map(|(h, _)| *h).collect();
            values = kept.into_iter().map(|(_, v)| v).collect();
            unreadable = values.iter().any(Result::is_err);
            if !unreadable && values.iter().all(|v| *v == values[0]) {
                return equal_values(state, &key, &heads);
            }
        }
    }
    let ancestry = ancestry(state, &heads);
    if let RecordKey::Verse(book, ..) = &key {
        let mut diverges: SmallVec<[BatchIdx; 1]> = SmallVec::new();
        for c in div(&ancestry) {
            let b = state.meta(c).batch;
            if state.passage_of(b).contains(book) && !diverges.contains(&b) {
                diverges.push(b);
            }
        }
        if !diverges.is_empty() {
            diverges.sort();
            return (Local::Passage, diverges);
        }
    }
    let base_values: Result<Vec<Option<Json>>, Unreadable> =
        ancestry.bases.iter().map(|b| value_of(state, *b)).collect();
    let values: Result<Vec<Option<Json>>, Unreadable> = values.into_iter().collect();
    let (Ok(values), Ok(base_values)) = (values, base_values) else {
        return conflict(state, &key, &heads, true);
    };
    let outcome = merge_units(key, &values, &base_values);
    match outcome.merged {
        Some(merged) => match rebuild_record(state, heads[0], merged) {
            Some(record) => (
                Local::Merged(Box::new((
                    Arc::new(record),
                    MergeHow::Units(outcome.changed),
                ))),
                SmallVec::new(),
            ),
            // A merged value that does not decode is a conflict; so is a template whose
            // bytes could not be read again, until they can.
            None => conflict(state, &key, &heads, state.bytes(heads[0]).is_none()),
        },
        None => conflict(state, &key, &heads, false),
    }
}

/// Several heads with equal values: merged, standing for all of them.
fn equal_values(
    state: &FoldState,
    key: &RecordKey,
    heads: &[ChangeIdx],
) -> (Local, SmallVec<[BatchIdx; 1]>) {
    match record_of(state, heads[0]) {
        Some(record) => (
            Local::Merged(Box::new((Arc::new(record), MergeHow::EqualValues))),
            SmallVec::new(),
        ),
        // Read a moment ago, unreadable now: judged again when it can be read.
        None => conflict(state, key, heads, true),
    }
}

/// Owner ruling H5: only text conflicts. For these kinds the newest head wins with no
/// question, and the others stay in History, where Restore brings one back.
pub(crate) fn newest_wins(key: &RecordKey) -> bool {
    matches!(
        key,
        RecordKey::Alignment(..)
            | RecordKey::Check(_)
            | RecordKey::Comment(_)
            | RecordKey::Pins(_)
            | RecordKey::ResourcesExtra(_)
            | RecordKey::Setting(_)
            | RecordKey::ProjectMeta(_)
            | RecordKey::Versification
            | RecordKey::DeviceProfile(_)
            | RecordKey::BookLayout(_)
            // The newest keeps the name; the other versions stay beside it (carried_files).
            | RecordKey::CarriedFile(_)
    )
}

/// *Restore the text* against *Start empty*: one of them empties verse text, so the person
/// decides (H5 keeps verse text a conflict).
fn restore_race(state: &FoldState, heads: &[ChangeIdx]) -> bool {
    let how = |wanted| {
        heads
            .iter()
            .any(|h| state.meta(*h).form == super::meta::Form::Presence(Some(wanted)))
    };
    how(crate::record::PresentHow::RestoredText) && how(crate::record::PresentHow::StartedEmpty)
}

fn conflict(
    state: &FoldState,
    key: &RecordKey,
    heads: &[ChangeIdx],
    unreadable: bool,
) -> (Local, SmallVec<[BatchIdx; 1]>) {
    let provisional = provisional(state, heads);
    // An outside edit recorded without a known base (every comment edited in a file, §11.1)
    // never wins by itself, and the person's own edit is never dropped unasked: the outside
    // edit's review (*Accept all from the file*) decides, so it stays a conflict.
    let ranks: BTreeSet<u8> = heads.iter().map(|h| state.meta(*h).rank()).collect();
    let outside_without_base = ranks.len() > 1;
    if !outside_without_base
        && (newest_wins(key)
            || (matches!(key, RecordKey::BookPresence(_)) && !restore_race(state, heads)))
    {
        // A check that moved holds no decision at its old place: a move there wins over a
        // decision, newer or not, so the decision a teammate saved meanwhile is listed in the
        // History of the check it moved to (moved_into) and never shown as a second copy.
        let moves: Vec<ChangeIdx> = heads
            .iter()
            .copied()
            .filter(|h| matches!(state.shape(*h), Shape::MovedTo { .. }))
            .collect();
        let head = if matches!(key, RecordKey::Check(_)) && !moves.is_empty() {
            self::provisional(state, &moves)
        } else {
            provisional
        };
        return (
            Local::ByRule {
                head,
                rule: super::AutoRule::Newest,
                unreadable,
            },
            SmallVec::new(),
        );
    }
    let ids: Vec<crate::ids::ChangeId> = heads.iter().map(|h| state.meta(*h).id).collect();
    (
        Local::Conflict(Box::new(super::state::LocalConflict {
            id: super::ConflictId::of(&ids),
            provisional,
            unreadable,
        })),
        SmallVec::new(),
    )
}

/// F7: the head with the largest (rank, at, id).
pub(crate) fn provisional(state: &FoldState, heads: &[ChangeIdx]) -> ChangeIdx {
    heads
        .iter()
        .copied()
        .max_by_key(|h| state.meta(*h).order())
        .unwrap_or(ChangeIdx(0))
}

/// The record of change `template` with its value replaced by `value`.
pub(crate) fn rebuild_record(
    state: &FoldState,
    template: ChangeIdx,
    value: Option<Json>,
) -> Option<Record> {
    let raw = state.bytes(template)?;
    let parsed = crate::json::parse(&raw).ok()?;
    let record = parsed.as_obj()?.get("record")?.as_obj()?;
    let (kind, inner) = record.iter().next()?;
    let mut inner = inner.as_obj()?.clone();
    match value {
        Some(v) => {
            inner.insert("value".into(), v);
        }
        None => {
            inner.remove("value");
        }
    }
    let mut outer = JsonMap::new();
    outer.insert(kind.clone(), Json::Obj(inner));
    crate::canon::decode_record_lenient(&Json::Obj(outer))
}

// ---------------------------------------------------------------------------------------
// Merge units
// ---------------------------------------------------------------------------------------

/// A unit: where it is in the value, and, for a group of members taken together, which.
#[derive(Debug, Clone)]
pub(crate) struct Unit {
    pub name: UnitName,
    pub path: Vec<String>,
    pub members: Option<Vec<String>>,
}

impl Unit {
    fn at(path: &[&str]) -> Self {
        Self {
            name: UnitName(format!("/{}", path.join("/")).into()),
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            members: None,
        }
    }

    fn group(name: &str, path: &[&str], members: &[&str]) -> Self {
        Self {
            name: UnitName(name.into()),
            path: path.iter().map(|s| (*s).to_owned()).collect(),
            members: Some(members.iter().map(|s| (*s).to_owned()).collect()),
        }
    }

    fn whole() -> Self {
        Self {
            name: UnitName("".into()),
            path: Vec::new(),
            members: None,
        }
    }

    /// This unit's value in `value`.
    pub(crate) fn read(&self, value: Option<&Json>) -> Option<Json> {
        let mut at = value?;
        for segment in &self.path {
            at = at.as_obj()?.get(segment)?;
        }
        match &self.members {
            None => Some(at.clone()),
            Some(members) => {
                let map = at.as_obj()?;
                let picked: JsonMap = members
                    .iter()
                    .filter_map(|m| map.get(m).map(|v| (m.clone(), v.clone())))
                    .collect();
                Some(Json::Obj(picked))
            }
        }
    }

    /// Writes `unit` into `value`.
    pub(crate) fn write(&self, value: &mut Option<Json>, unit: Option<Json>) {
        if self.path.is_empty() && self.members.is_none() {
            *value = unit;
            return;
        }
        let Some(root) = value.as_mut() else {
            return;
        };
        let (last, prefix) = match &self.members {
            Some(_) => (None, &self.path[..]),
            None => match self.path.split_last() {
                Some((last, prefix)) => (Some(last), prefix),
                None => return,
            },
        };
        let mut at = root;
        for segment in prefix {
            let Json::Obj(map) = at else {
                return;
            };
            at = map.get_or_insert_with(segment.clone(), || Json::Obj(JsonMap::new()));
        }
        let Json::Obj(map) = at else {
            return;
        };
        match (&self.members, last) {
            (Some(members), _) => {
                for m in members {
                    map.remove(m);
                }
                if let Some(Json::Obj(picked)) = unit {
                    map.extend(picked);
                }
            }
            (None, Some(last)) => match unit {
                Some(v) => {
                    map.insert(last.clone(), v);
                }
                None => {
                    map.remove(last);
                }
            },
            (None, None) => {}
        }
    }
}

fn members<'a>(
    values: impl Iterator<Item = Option<&'a Json>>,
    path: &[&str],
) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for value in values {
        let mut at = value?;
        for segment in path {
            at = at.as_obj()?.get(segment)?;
        }
        out.extend(at.as_obj()?.keys().cloned());
    }
    Some(out)
}

/// The units of a record kind for these versions (heads and bases), or one whole-value unit
/// when the kind's precondition fails.
pub(crate) fn units_for(key: &RecordKey, versions: &[Option<&Json>]) -> Vec<Unit> {
    let all = || versions.iter().copied();
    match key {
        RecordKey::Verse(..) => {
            // Every version a text, with equal last, marker and from.
            let texts: Option<Vec<&JsonMap>> =
                all().map(|v| v?.as_obj()?.get("text")?.as_obj()).collect();
            let Some(texts) = texts else {
                return vec![Unit::whole()];
            };
            let fixed = ["last", "marker", "from"];
            let same = texts.iter().all(|t| {
                fixed
                    .iter()
                    .all(|f| t.get(f) == texts.first().and_then(|first| first.get(f)))
            });
            if !same {
                return vec![Unit::whole()];
            }
            let names: BTreeSet<&String> = texts.iter().flat_map(|t| t.keys()).collect();
            names
                .into_iter()
                .filter(|n| !fixed.contains(&n.as_str()))
                .map(|n| Unit::at(&["text", n.as_str()]))
                .collect()
        }
        RecordKey::Check(_) => {
            // A moved check (`movedTo`) in any version makes the value one unit.
            let Some(names) = members(all(), &["decision"]) else {
                return vec![Unit::whole()];
            };
            let Some(extra) = extra_keys(versions) else {
                return vec![Unit::whole()];
            };
            let mut units = vec![
                Unit::group(
                    "/decision/judgement",
                    &["decision"],
                    &["judgement", "importedInvalid"],
                ),
                Unit::at(&["decision", "comment"]),
                Unit::at(&["decision", "bookmarked"]),
                Unit::at(&["decision", "dismissed"]),
                Unit::group(
                    "/decision/basis",
                    &["decision"],
                    &["context", "category", "resource"],
                ),
            ];
            for name in &extra {
                units.push(Unit::at(&["decision", "extra", name.as_str()]));
            }
            const KNOWN: [&str; 9] = [
                "judgement",
                "importedInvalid",
                "comment",
                "bookmarked",
                "dismissed",
                "context",
                "category",
                "resource",
                "extra",
            ];
            for name in names.iter().filter(|n| !KNOWN.contains(&n.as_str())) {
                units.push(Unit::at(&["decision", name.as_str()]));
            }
            units
        }
        RecordKey::Pins(_) | RecordKey::DeviceProfile(_) => match members(all(), &[]) {
            Some(names) => names.iter().map(|n| Unit::at(&[n.as_str()])).collect(),
            None => vec![Unit::whole()],
        },
        RecordKey::Setting(_) | RecordKey::ProjectMeta(_) | RecordKey::ResourcesExtra(_) => {
            if versions.iter().any(Option::is_none) {
                return vec![Unit::whole()];
            }
            let mut units = Vec::new();
            leaf_units(versions, &mut Vec::new(), &mut units);
            units
        }
        RecordKey::Alignment(..) => match members(all(), &[]) {
            Some(names) => {
                let rest: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .filter(|n| *n != "dismissed")
                    .collect();
                vec![
                    Unit::at(&["dismissed"]),
                    Unit::group("/alignment", &[], &rest),
                ]
            }
            None => vec![Unit::whole()],
        },
        _ => vec![Unit::whole()],
    }
}

/// The keys of the decisions' `extra` maps.
fn extra_keys(versions: &[Option<&Json>]) -> Option<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for v in versions {
        let decision = (*v)?.as_obj()?.get("decision")?.as_obj()?;
        if let Some(extra) = decision.get("extra") {
            out.extend(extra.as_obj()?.keys().cloned());
        }
    }
    Some(out)
}

/// Each leaf path of JSON values: objects recurse while every version that has the path
/// has an object there; arrays and scalars are leaves.
fn leaf_units(versions: &[Option<&Json>], path: &mut Vec<String>, out: &mut Vec<Unit>) {
    let at: Vec<Option<&Json>> = versions
        .iter()
        .map(|v| {
            let mut at = (*v)?;
            for segment in path.iter() {
                at = at.as_obj()?.get(segment)?;
            }
            Some(at)
        })
        .collect();
    let present: Vec<&Json> = at.iter().flatten().copied().collect();
    let all_objects = !present.is_empty() && present.iter().all(|j| j.as_obj().is_some());
    if !all_objects {
        out.push(Unit {
            name: UnitName(format!("/{}", path.join("/")).into()),
            path: path.clone(),
            members: None,
        });
        return;
    }
    let names: BTreeSet<String> = present
        .iter()
        .filter_map(|j| j.as_obj())
        .flat_map(|m| m.keys().cloned())
        .collect();
    if names.is_empty() {
        out.push(Unit {
            name: UnitName(format!("/{}", path.join("/")).into()),
            path: path.clone(),
            members: None,
        });
        return;
    }
    for name in names {
        path.push(name);
        leaf_units(versions, path, out);
        path.pop();
    }
}

/// What the rule per unit decided.
pub(crate) struct UnitOutcome {
    /// The merged value when no unit is disputed.
    pub merged: Option<Option<Json>>,
    pub agreed: Vec<(UnitName, UnitValue)>,
    pub disputed: Vec<UnitName>,
    /// Units whose heads differed but merged.
    pub changed: Vec<UnitName>,
}

/// The rule per unit of §7.3 over the heads' and bases' values.
pub(crate) fn merge_units(
    key: &RecordKey,
    heads: &[Option<Json>],
    bases: &[Option<Json>],
) -> UnitOutcome {
    let versions: Vec<Option<&Json>> = heads
        .iter()
        .chain(bases.iter())
        .map(Option::as_ref)
        .collect();
    let units = units_for(key, &versions);
    let mut agreed = Vec::new();
    let mut disputed = Vec::new();
    let mut changed = Vec::new();
    let mut chosen: Vec<(Unit, Option<Json>)> = Vec::new();
    for unit in units {
        let values: Vec<Option<Json>> = heads.iter().map(|h| unit.read(h.as_ref())).collect();
        let first = values.first().cloned().flatten();
        if values.iter().all(|v| *v == first) {
            agreed.push((unit.name.clone(), UnitValue(first.clone())));
            chosen.push((unit, first));
            continue;
        }
        let base_values: Vec<Option<Json>> = bases.iter().map(|b| unit.read(b.as_ref())).collect();
        let decided = match base_values.split_first() {
            Some((base, rest)) if rest.iter().all(|b| b == base) => {
                let mut changes: Vec<&Option<Json>> =
                    values.iter().filter(|v| *v != base).collect();
                changes.sort();
                changes.dedup();
                match changes.as_slice() {
                    [only] => Some((*only).clone()),
                    _ => None,
                }
            }
            _ => None,
        };
        match decided {
            Some(value) => {
                agreed.push((unit.name.clone(), UnitValue(value.clone())));
                changed.push(unit.name.clone());
                chosen.push((unit, value));
            }
            None => disputed.push(unit.name.clone()),
        }
    }
    let merged = if disputed.is_empty() {
        let mut value = heads.first().cloned().flatten();
        for (unit, unit_value) in &chosen {
            unit.write(&mut value, unit_value.clone());
        }
        if let Some(root) = value.as_mut() {
            let heads: Vec<Option<&Json>> = heads.iter().map(Option::as_ref).collect();
            prune_emptied(root, &mut Vec::new(), &heads);
        }
        Some(value)
    } else {
        None
    };
    UnitOutcome {
        merged,
        agreed,
        disputed,
        changed,
    }
}

/// Removes the objects that unit writes emptied and no head has empty: when one head
/// removed a whole object and another kept it unchanged, the merge removes it, rather than
/// leaving an empty object nobody wrote.
pub(crate) fn prune_emptied(value: &mut Json, path: &mut Vec<String>, heads: &[Option<&Json>]) {
    let Json::Obj(map) = value else {
        return;
    };
    let names: Vec<String> = map.keys().cloned().collect();
    for name in names {
        path.push(name.clone());
        if let Some(child) = map.get_mut(&name) {
            prune_emptied(child, path, heads);
            let emptied = child.as_obj().is_some_and(JsonMap::is_empty);
            let some_head_has_it_empty = heads.iter().any(|h| {
                let mut at = *h;
                for segment in path.iter() {
                    at = at.and_then(|j| j.as_obj()).and_then(|m| m.get(segment));
                }
                at.and_then(Json::as_obj).is_some_and(JsonMap::is_empty)
            });
            if emptied && !some_head_has_it_empty {
                map.remove(&name);
            }
        }
        path.pop();
    }
}

/// The conflict kind of a record's own conflict (§8.1).
pub(crate) fn conflict_kind(
    state: &FoldState,
    key: &RecordKey,
    heads: &[ChangeIdx],
) -> ConflictKind {
    match key {
        RecordKey::Verse(..) => {
            let shapes: Vec<Option<VerseShape>> =
                heads.iter().map(|h| state.meta(*h).verse()).collect();
            let lasts: BTreeSet<Option<u16>> = shapes
                .iter()
                .map(|s| match s {
                    Some(VerseShape::Text { last, .. }) => Some(*last),
                    _ => None,
                })
                .collect();
            if lasts.len() == 1 && !lasts.contains(&None) {
                ConflictKind::VerseText
            } else {
                ConflictKind::VerseStructure
            }
        }
        RecordKey::BookPresence(_) => ConflictKind::BookPresence,
        RecordKey::BookHeader(_) => ConflictKind::BookHeader,
        RecordKey::ChapterFront(..) => ConflictKind::ChapterFront,
        RecordKey::BookLayout(_) => ConflictKind::BookLayout,
        RecordKey::Alignment(..) => ConflictKind::Alignment,
        RecordKey::Check(_) => {
            let moved = heads
                .iter()
                .any(|h| matches!(state.meta(*h).form, Form::MovedTo));
            if moved {
                ConflictKind::CheckMoved
            } else {
                ConflictKind::Check
            }
        }
        RecordKey::Comment(_) => ConflictKind::Comment,
        RecordKey::Pins(_) | RecordKey::ResourcesExtra(_) => ConflictKind::Pins,
        RecordKey::Setting(_) => ConflictKind::Setting,
        RecordKey::ProjectMeta(_) => ConflictKind::ProjectMeta,
        RecordKey::Versification => ConflictKind::Versification,
        RecordKey::DeviceProfile(_) => ConflictKind::DeviceProfile,
        RecordKey::StoryBase(_) => ConflictKind::StoryBase,
        RecordKey::Frame(..) => ConflictKind::Frame,
        RecordKey::StoryRef(_) => ConflictKind::StoryRef,
        RecordKey::CarriedFile(_) => ConflictKind::CarriedFile,
        RecordKey::BatchRelease(_) | RecordKey::Unknown(_) => ConflictKind::Unknown,
    }
}

/// The agreed and disputed units of a record's own conflict, for the review (§8.3); none
/// while a value cannot be read.
pub(crate) fn units_of_conflict(
    state: &FoldState,
    key: &RecordKey,
    heads: &[ChangeIdx],
    bases: &[ChangeIdx],
) -> (Vec<(UnitName, UnitValue)>, Vec<UnitName>) {
    let values: Result<Vec<Option<Json>>, Unreadable> =
        heads.iter().map(|h| value_of(state, *h)).collect();
    let base_values: Result<Vec<Option<Json>>, Unreadable> =
        bases.iter().map(|b| value_of(state, *b)).collect();
    let (Ok(values), Ok(base_values)) = (values, base_values) else {
        return (Vec::new(), Vec::new());
    };
    let outcome = merge_units(key, &values, &base_values);
    let whole = outcome.disputed.len() == 1 && outcome.disputed[0].0.is_empty();
    if whole {
        // A whole-value kind lists no units.
        return (Vec::new(), Vec::new());
    }
    (outcome.agreed, outcome.disputed)
}

/// Values of distinct heads, grouped: (value bytes → heads), for candidates; and the heads
/// whose values cannot be read, which group with nothing.
pub(crate) fn group_by_value(
    state: &FoldState,
    heads: &[ChangeIdx],
) -> (Vec<Vec<ChangeIdx>>, Vec<ChangeIdx>) {
    let mut groups: BTreeMap<Option<Vec<u8>>, Vec<ChangeIdx>> = BTreeMap::new();
    let mut unreadable = Vec::new();
    for h in heads {
        match value_of(state, *h) {
            Ok(value) => groups
                .entry(value.map(|v| v.to_canonical()))
                .or_default()
                .push(*h),
            Err(Unreadable) => unreadable.push(*h),
        }
    }
    (groups.into_values().collect(), unreadable)
}
