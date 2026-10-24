//! What `metadata.json` is built from (§3.4): every value that feeds the document is
//! checked on its own, an invalid one is never projected, and the fold lists it as
//! `InvalidForBurrito` in words. So no record can make the document invalid.

use super::merge::{ancestors_or_self, record_of};
use super::meta::{ChangeIdx, KeyIdx};
use super::state::{Dirty, FoldState, ProjectKind, Projected};
use super::{Anomaly, Counter};
use crate::json::Json;
use crate::keys::{MetaField, PinGroup, RecordKey};
use crate::record::{BookPresence, PinGroupValue, Record, Scope};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use tc_burrito::{MetadataSource, check_field, check_relationship, check_scope};

/// What a record contributes to `metadata.json`, and why a value was not used.
#[derive(Debug, Clone, Default)]
pub(crate) struct MetaChoice {
    /// The value used: the field's JSON, a book's scope ranges, or a group's
    /// relationships.
    pub used: Option<Value>,
    pub anomaly: Option<Anomaly>,
}

fn burrito_kind(kind: ProjectKind) -> tc_burrito::ProjectKind {
    match kind {
        ProjectKind::Bible => tc_burrito::ProjectKind::BibleText,
        ProjectKind::Stories => tc_burrito::ProjectKind::OpenBibleStories,
    }
}

/// The dotted path a project detail projects to.
pub(crate) fn field_path(field: &MetaField) -> String {
    match field {
        MetaField::Name => "identification.name".into(),
        MetaField::Abbreviation => "identification.abbreviation".into(),
        MetaField::Description => "identification.description".into(),
        MetaField::Languages => "languages".into(),
        MetaField::Copyright => "copyright".into(),
        MetaField::Extra(path) => path.as_str().to_owned(),
    }
}

/// The changes to try for a record, best first: its projected head (or merged value), then
/// every ancestor of its heads, newest first by (at, id).
fn candidates(state: &FoldState, k: KeyIdx) -> (Option<Record>, Vec<ChangeIdx>) {
    let entry = state.entry(k);
    let first = match &entry.projected {
        Projected::Change(c) => record_of(state, *c),
        Projected::Merged(record) => Some((**record).clone()),
        Projected::Nothing => None,
    };
    let mut older: Vec<ChangeIdx> = Vec::new();
    for h in entry.heads.iter() {
        older.extend(ancestors_or_self(state, *h));
    }
    older.sort_by_key(|c| std::cmp::Reverse((state.meta(*c).at, state.meta(*c).id)));
    older.dedup();
    if let Projected::Change(c) = &entry.projected {
        older.retain(|x| x != c);
    }
    (first, older)
}

fn projected_change(state: &FoldState, k: KeyIdx) -> Option<ChangeIdx> {
    match &state.entry(k).projected {
        Projected::Change(c) => Some(*c),
        _ => state
            .entry(k)
            .heads
            .iter()
            .copied()
            .max_by_key(|h| state.meta(*h).order()),
    }
}

/// A value from one record, tried newest first: `read` takes the record's value, `check`
/// says whether the document can hold it.
fn choose(
    state: &FoldState,
    k: KeyIdx,
    read: impl Fn(&Record) -> Option<Value>,
    check: impl Fn(&Value) -> Result<(), String>,
) -> MetaChoice {
    state.count(Counter::ValuesDecoded, 1);
    let (first, older) = candidates(state, k);
    let mut first_error: Option<String> = None;
    let mut tried = first
        .into_iter()
        .map(Some)
        .chain(older.into_iter().map(|c| {
            state.count(Counter::ValuesDecoded, 1);
            record_of(state, c)
        }));
    let mut used = None;
    for record in tried.by_ref() {
        let Some(value) = record.as_ref().and_then(&read) else {
            if first_error.is_none() {
                // A removed value (no value at all) is used as it is.
                break;
            }
            continue;
        };
        match check(&value) {
            Ok(()) => {
                used = Some(value);
                break;
            }
            Err(reason) => {
                if first_error.is_none() {
                    first_error = Some(reason);
                }
            }
        }
    }
    let anomaly = first_error.and_then(|reason| {
        projected_change(state, k).map(|c| Anomaly::InvalidForBurrito {
            key: state.entry(k).key.clone(),
            change: state.meta(c).id,
            reason: reason.into(),
        })
    });
    MetaChoice { used, anomaly }
}

fn scope_ranges(scope: &Scope) -> Vec<String> {
    match scope {
        Scope::Whole => Vec::new(),
        Scope::Ranges(ranges) => ranges.as_slice().iter().map(ToString::to_string).collect(),
    }
}

/// The relationships a pin group mirrors (tC4 `relationshipsFromPins`): one per slot that
/// names a repository and a flavor.
pub(crate) fn relationships(value: &PinGroupValue) -> Vec<Value> {
    let mut out = Vec::new();
    let mut push = |slot: &Json| {
        let Some(map) = slot.as_obj() else {
            return;
        };
        let (Some(repo), Some(flavor)) = (
            map.get("repoPath").and_then(Json::as_str),
            map.get("flavor").and_then(Json::as_str),
        ) else {
            return;
        };
        let id = repo.split_once('/').map_or(repo, |(_, rest)| rest);
        // tC4's `relationshipsFromPins`: only these types have a relationship the Burrito
        // schema accepts; any other (`gloss/textStories`, the OBS source text) has no row.
        let Some((relation, flavor)) = flavor.split_once('/').filter(|(_, f)| !f.is_empty()) else {
            return;
        };
        let relation = match relation {
            "scripture" => "source",
            "parascriptural" | "peripheral" => relation,
            _ => return,
        };
        let mut r = serde_json::Map::new();
        r.insert("relationType".into(), json!(relation));
        r.insert("flavor".into(), json!(flavor));
        r.insert("id".into(), json!(format!("dcs::{id}")));
        if let Some(version) = map.get("version").and_then(Json::as_str) {
            r.insert("revision".into(), json!(version));
        }
        out.push(Value::Object(r));
    };
    // tC4 reads the slots in its `resources.json` order.
    let mut slots: Vec<_> = value.0.iter().collect();
    slots.sort_by_key(|(name, _)| (super::project::slot_rank(name.as_str()), name.as_str()));
    for (_, slot) in slots {
        match &slot.0 {
            Json::Arr(items) => items.iter().for_each(&mut push),
            other => push(other),
        }
    }
    out
}

/// tC4's `relationshipsFromPins` order (`journal/relationships.mjs:24-31`).
const GROUP_ORDER: [PinGroup; 5] = [
    PinGroup::OriginalLanguage,
    PinGroup::ExtraScripture,
    PinGroup::Primary,
    PinGroup::Fallback,
    PinGroup::Lexicon,
];

impl FoldState {
    fn meta_choice(&self, k: KeyIdx) -> MetaChoice {
        let kind = burrito_kind(self.kind);
        match &self.entry(k).key {
            RecordKey::ProjectMeta(field) => {
                let path = field_path(field);
                choose(
                    self,
                    k,
                    |r| match r {
                        Record::ProjectMeta { value, .. } => value.as_ref().map(Value::from),
                        _ => None,
                    },
                    |v| check_field(kind, &path, v),
                )
            }
            RecordKey::BookPresence(book) => {
                let book = *book;
                choose(
                    self,
                    k,
                    |r| match r {
                        Record::BookPresence {
                            value: BookPresence::Present { scope, .. },
                            ..
                        } => Some(json!(scope_ranges(scope))),
                        _ => None,
                    },
                    |v| {
                        let ranges: Vec<String> = v
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default();
                        check_scope(kind, book.code(), &ranges)
                    },
                )
            }
            RecordKey::Pins(_) => choose(
                self,
                k,
                |r| match r {
                    Record::Pins { value, .. } => Some(Value::Array(relationships(value))),
                    _ => None,
                },
                |v| match v.as_array() {
                    Some(items) => items.iter().try_for_each(|r| check_relationship(kind, r)),
                    None => Ok(()),
                },
            ),
            _ => MetaChoice::default(),
        }
    }

    /// The values `metadata.json` uses, rechecked for the records that changed.
    pub(crate) fn stage_meta(&mut self, dirty: &Dirty) {
        let keys: Vec<KeyIdx> = if dirty.all {
            (0..self.keys.len() as u32)
                .map(KeyIdx)
                .filter(|k| self.feeds_metadata(*k))
                .collect()
        } else {
            dirty
                .changed
                .iter()
                .copied()
                .filter(|k| self.feeds_metadata(*k))
                .collect()
        };
        for k in keys {
            let choice = self.meta_choice(k);
            let old = self.meta_choices.get(&k).and_then(|c| c.anomaly.clone());
            if let Some(old) = old {
                self.unlist_anomaly(&old);
            }
            if let Some(new) = &choice.anomaly {
                self.list_anomaly(new.clone());
            }
            Arc::make_mut(&mut self.meta_choices).insert(k, choice);
        }
    }

    fn feeds_metadata(&self, k: KeyIdx) -> bool {
        matches!(
            self.entry(k).key,
            RecordKey::ProjectMeta(_) | RecordKey::BookPresence(_) | RecordKey::Pins(_)
        )
    }

    /// What `metadata.json` is built from (§3.4), besides its files: pass it with the
    /// ingredient table to `tc_burrito::build_metadata`. For an OBS project the caller sets
    /// `scope` to the template table.
    pub fn metadata_source(&self, format: (u64, u64)) -> MetadataSource {
        self.metadata_source_nfc(format, self.text_is_nfc())
    }

    /// [`Self::metadata_source`] with `nfc` given by the caller: whether every USFM and story
    /// file is in NFC. A store knows it from the bytes it wrote, so it never projects every
    /// book again to build `metadata.json` (§5.10 M2, §6.2).
    pub fn metadata_source_nfc(&self, format: (u64, u64), nfc: bool) -> MetadataSource {
        let mut fields: BTreeMap<String, Value> = BTreeMap::new();
        let mut scope = serde_json::Map::new();
        let mut groups: BTreeMap<PinGroup, Vec<Value>> = BTreeMap::new();
        for (k, choice) in self.meta_choices.iter() {
            let Some(used) = &choice.used else {
                continue;
            };
            match &self.entry(*k).key {
                RecordKey::ProjectMeta(field) => {
                    fields.insert(field_path(field), used.clone());
                }
                RecordKey::BookPresence(book) => {
                    if self.book(*book).projects() {
                        scope.insert(book.code().to_owned(), used.clone());
                    }
                }
                RecordKey::Pins(group) => {
                    if let Some(items) = used.as_array() {
                        groups.insert(*group, items.clone());
                    }
                }
                _ => {}
            }
        }
        // A book that projects without a valid scope record (kept as the last book, or
        // shown while removed) covers the whole book.
        for book in self.projected_books() {
            scope
                .entry(book.code().to_owned())
                .or_insert_with(|| json!([]));
        }
        let mut relationships = Vec::new();
        for group in GROUP_ORDER {
            // One row per repository, the first one wins, as tC4.
            for r in groups.remove(&group).unwrap_or_default() {
                if !relationships
                    .iter()
                    .any(|seen: &Value| seen["id"] == r["id"])
                {
                    relationships.push(r);
                }
            }
        }
        MetadataSource {
            kind: burrito_kind(self.kind),
            format,
            fields,
            scope: match self.kind {
                ProjectKind::Bible => Value::Object(scope),
                ProjectKind::Stories => Value::Null,
            },
            relationships,
            nfc,
        }
    }

    /// Whether every projected USFM and story text is in NFC (`meta.normalization`).
    fn text_is_nfc(&self) -> bool {
        use unicode_normalization::is_nfc;
        for book in self.projected_books() {
            if let Some(usfm) = self.book_usfm(book)
                && !is_nfc(&usfm)
            {
                return false;
            }
        }
        for k in self.story_keys.iter() {
            if let Some(record) = match &self.entry(*k).projected {
                Projected::Change(c) => record_of(self, *c),
                _ => None,
            } {
                let text = match record {
                    Record::StoryBase { value, .. } | Record::Frame { value, .. } => value,
                    Record::StoryRef { value, .. } => value.unwrap_or_default(),
                    _ => String::new(),
                };
                if !is_nfc(&text) {
                    return false;
                }
            }
        }
        true
    }
}
