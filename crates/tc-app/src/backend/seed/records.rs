//! The records of each tC4 file the seed reads: `metadata.json`'s free fields and the books'
//! scopes, the pins (with the `schemaVersion: 1` migration), the settings, a carried file,
//! and the alignment and decision sidecars, each rewritten into the fields tc-journal's
//! decoders read.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};
use tc_journal::decision::verse_text_md5_v1;
use tc_journal::fold::{FoldState, decode_alignments, decode_decisions};
use tc_journal::pins::{Collection, SetSlot};
use tc_journal::record::{CarriedValue, CheckValue, PinGroupValue, Scope, ScopeRange, ScopeRanges};
use tc_journal::{
    BookId, CheckTarget, GitOid, Md5, MetaField, MetaPath, PinGroup, Record, RepoPath,
    ResourcesKey, ResourcesLevel, SettingKey, ToolId, WallTime,
};

use super::strict::parse_utc_millis;
use super::{
    DeclaredDifference, Item, METADATA, SeedRefusal, book_of, describe_key, json_of, read_json,
};

/// The free fields of `metadata.json` (every path `tc_burrito::is_derived_field` does not
/// name), with `meta`, `idAuthorities`, `identification` and `identification.primary`
/// broken into their members, as the project's records hold them (STORAGE §3.4).
pub(super) fn meta_fields(meta: &Map<String, Value>) -> Result<Vec<(String, Value)>, SeedRefusal> {
    use tc_burrito::is_derived_field;
    let not_object = |path: &str| SeedRefusal::BadValue {
        path: METADATA.into(),
        problem: format!("{path} is not an object"),
    };
    let mut fields: Vec<(Vec<String>, Value)> = Vec::new();
    for (key, value) in meta {
        if is_derived_field(key) {
            continue;
        }
        if !matches!(key.as_str(), "meta" | "idAuthorities" | "identification") {
            fields.push((vec![key.clone()], value.clone()));
            continue;
        }
        let Value::Object(members) = value else {
            return Err(not_object(key));
        };
        for (member, value) in members {
            let path = format!("{key}.{member}");
            if is_derived_field(&path) {
                continue;
            }
            if path != "identification.primary" {
                fields.push((vec![key.clone(), member.clone()], value.clone()));
                continue;
            }
            let Value::Object(primary) = value else {
                return Err(not_object(&path));
            };
            for (authority, value) in primary {
                if !is_derived_field(&format!("{path}.{authority}")) {
                    fields.push((
                        vec![key.clone(), member.clone(), authority.clone()],
                        value.clone(),
                    ));
                }
            }
        }
    }
    // A key with a dot in it would be stored at a dotted path that names another place.
    let mut nested = Map::new();
    let mut dotted = Map::new();
    for (segments, value) in &fields {
        set_segments(&mut nested, segments, value.clone());
        let path = segments.join(".");
        let split: Vec<String> = path.split('.').map(str::to_owned).collect();
        set_segments(&mut dotted, &split, value.clone());
    }
    if nested != dotted {
        return Err(SeedRefusal::BadValue {
            path: METADATA.into(),
            problem: "a key holds a dot, so its field cannot be kept at its own path".into(),
        });
    }
    Ok(fields
        .into_iter()
        .map(|(segments, value)| (segments.join("."), value))
        .collect())
}

fn set_segments(root: &mut Map<String, Value>, segments: &[String], value: Value) {
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut at = root;
    for segment in parents {
        let next = at
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        match next {
            Value::Object(map) => at = map,
            _ => return,
        }
    }
    at.insert(last.clone(), value);
}

/// The record a free field of `metadata.json` is: one of the five modelled fields, or
/// `Extra` at its dotted path.
pub(super) fn meta_field(path: &str) -> Result<MetaField, SeedRefusal> {
    Ok(match path {
        "identification.name" => MetaField::Name,
        "identification.abbreviation" => MetaField::Abbreviation,
        "identification.description" => MetaField::Description,
        "languages" => MetaField::Languages,
        "copyright" => MetaField::Copyright,
        _ => MetaField::Extra(MetaPath::parse(path).map_err(|e| SeedRefusal::BadValue {
            path: METADATA.into(),
            problem: e.to_string(),
        })?),
    })
}

/// Each book's scope from `type.flavorType.currentScope`: `[]` is the whole book, ranges a
/// passage set (D26).
pub(super) fn scopes_of(meta: &Map<String, Value>) -> Result<BTreeMap<BookId, Scope>, SeedRefusal> {
    let bad = |problem: String| SeedRefusal::BadValue {
        path: METADATA.into(),
        problem,
    };
    let Some(scope) = meta
        .get("type")
        .and_then(|t| t.get("flavorType"))
        .and_then(|t| t.get("currentScope"))
    else {
        return Ok(BTreeMap::new());
    };
    let Value::Object(books) = scope else {
        return Err(bad("currentScope is not an object".into()));
    };
    let mut out = BTreeMap::new();
    for (code, ranges) in books {
        let book =
            book_of(code).ok_or_else(|| bad(format!("currentScope names no book {code:?}")))?;
        let Value::Array(ranges) = ranges else {
            return Err(bad(format!("the scope of {code} is not a list")));
        };
        let mut parsed = Vec::with_capacity(ranges.len());
        for range in ranges {
            let text = range
                .as_str()
                .ok_or_else(|| bad(format!("a scope range of {code} is not text")))?;
            let read: ScopeRange = text
                .parse()
                .map_err(|e| bad(format!("{code} {text:?}: {e}")))?;
            // A range written another way would come back in another form.
            if read.to_string() != text {
                return Err(bad(format!(
                    "{code} {text:?} would be written {:?}",
                    read.to_string()
                )));
            }
            parsed.push(read);
        }
        let scope = match ScopeRanges::new(parsed) {
            Some(ranges) => Scope::Ranges(ranges),
            None => Scope::Whole,
        };
        out.insert(book, scope);
    }
    Ok(out)
}

/// The pin records of a `resources.json`, the document the projection must give back, and
/// the migration when there was one.
pub(super) struct PinsSeed {
    pub(super) records: Vec<Record>,
    pub(super) expected: Value,
    pub(super) declared: Option<DeclaredDifference>,
}

pub(super) fn pins_records(path: &str, doc: &Value) -> Result<PinsSeed, SeedRefusal> {
    let bad = |problem: String| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem,
    };
    let Value::Object(top) = doc else {
        return Err(bad("it is not a JSON object".into()));
    };
    let (expected, declared) = match top.get("schemaVersion").and_then(Value::as_u64) {
        Some(2) => (doc.clone(), None),
        Some(1) => (
            migrate_pins_v1(path, top)?,
            Some(DeclaredDifference::ResourcesMigrated { from: 1 }),
        ),
        _ => {
            return Err(SeedRefusal::Schema {
                path: path.to_owned(),
                found: top
                    .get("schemaVersion")
                    .map_or_else(|| "(none)".into(), Value::to_string),
            });
        }
    };
    let Value::Object(top) = &expected else {
        return Err(SeedRefusal::Internal("a migrated resources.json".into()));
    };
    let mut records = Vec::new();
    let group = |name: &str, group: PinGroup, value: &Value| -> Result<Record, SeedRefusal> {
        let value = PinGroupValue::from_json(value)
            .ok_or_else(|| bad(format!("{name} is not a group of pins tC4 could write")))?;
        Ok(Record::Pins { group, value })
    };
    let extra = |level: ResourcesLevel, name: &str, value: &Value| -> Result<Record, SeedRefusal> {
        Ok(Record::ResourcesExtra {
            key: ResourcesKey::new(level, name).map_err(|e| bad(e.to_string()))?,
            value: Some(json_of(path, name, value)?),
        })
    };
    for (name, value) in top {
        match name.as_str() {
            "schemaVersion" => {}
            "languageSets" | "resources" => {
                let Value::Object(members) = value else {
                    return Err(bad(format!("{name} is not an object")));
                };
                let level = if name == "languageSets" {
                    ResourcesLevel::LanguageSets
                } else {
                    ResourcesLevel::Resources
                };
                for (member, value) in members {
                    let at = format!("{name}.{member}");
                    records.push(match (level, member.as_str()) {
                        (ResourcesLevel::LanguageSets, "primary") => {
                            group(&at, PinGroup::Primary, value)?
                        }
                        (ResourcesLevel::LanguageSets, "fallback") => {
                            group(&at, PinGroup::Fallback, value)?
                        }
                        (ResourcesLevel::Resources, "originalLanguage") => {
                            group(&at, PinGroup::OriginalLanguage, value)?
                        }
                        (ResourcesLevel::Resources, "lexicon") => {
                            group(&at, PinGroup::Lexicon, value)?
                        }
                        _ => extra(level, member, value)?,
                    });
                }
            }
            "extraScripture" => records.push(group(
                name,
                PinGroup::ExtraScripture,
                &serde_json::json!({ "extraScripture": value }),
            )?),
            _ => records.push(extra(ResourcesLevel::Top, name, value)?),
        }
    }
    Ok(PinsSeed {
        records,
        expected,
        declared,
    })
}

/// BURRITO-SPEC §5.3's migration of a `schemaVersion: 1` file, the only definition of one
/// (no public tC4 commit wrote it): its `gatewayLanguage` and the set slots under
/// `resources` become the fallback set, `primary` equals `fallback`, a `translationWordsLinks`
/// pin is added when the file has none (the installed English suite's `en_twl`, D97), and
/// `originalLanguage`, `lexicon`, `extraScripture` and every other member carry over.
pub(super) fn migrate_pins_v1(path: &str, top: &Map<String, Value>) -> Result<Value, SeedRefusal> {
    let bad = |problem: &str| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem: format!("schemaVersion 1: {problem}"),
    };
    if top.contains_key("languageSets") {
        return Err(bad("the file already has languageSets"));
    }
    let resources = match top.get("resources") {
        Some(Value::Object(r)) => r.clone(),
        None => Map::new(),
        Some(_) => return Err(bad("resources is not an object")),
    };
    let gateway = match (top.get("gatewayLanguage"), resources.get("gatewayLanguage")) {
        (Some(a), Some(b)) if a != b => {
            return Err(bad("two different gatewayLanguage values"));
        }
        (Some(a), _) | (None, Some(a)) => a.clone(),
        (None, None) => return Err(bad("the file names no gatewayLanguage")),
    };
    let mut set = Map::new();
    set.insert("gatewayLanguage".into(), gateway);
    let mut kept = Map::new();
    for (name, value) in resources {
        if name == "gatewayLanguage" {
            continue;
        }
        if SetSlot::parse(&name).is_some() {
            set.insert(name, value);
        } else {
            kept.insert(name, value);
        }
    }
    if !set.contains_key(SetSlot::TranslationWordsLinks.name()) {
        let twl = serde_json::to_value(tc_resources::suite::english_twl())
            .map_err(|e| SeedRefusal::Internal(e.to_string()))?;
        set.insert(SetSlot::TranslationWordsLinks.name().into(), twl);
    }
    let mut out = Map::new();
    out.insert("schemaVersion".into(), Value::from(2));
    let mut sets = Map::new();
    sets.insert("primary".into(), Value::Object(set.clone()));
    sets.insert("fallback".into(), Value::Object(set));
    out.insert("languageSets".into(), Value::Object(sets));
    if !kept.is_empty() {
        out.insert("resources".into(), Value::Object(kept));
    }
    for (name, value) in top {
        if !matches!(
            name.as_str(),
            "schemaVersion" | "gatewayLanguage" | "resources"
        ) {
            out.insert(name.clone(), value.clone());
        }
    }
    Ok(Value::Object(out))
}

/// One `Setting` record per top-level key of `settings.json` but `schemaVersion`, which
/// must be 1.
pub(super) fn settings_records(path: &str, doc: &Value) -> Result<Vec<Record>, SeedRefusal> {
    let Value::Object(top) = doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    let mut out = Vec::new();
    for (name, value) in top {
        if name == "schemaVersion" {
            continue;
        }
        out.push(Record::Setting {
            key: SettingKey::parse(name).map_err(|e| SeedRefusal::BadValue {
                path: path.to_owned(),
                problem: e.to_string(),
            })?,
            value: Some(json_of(path, name, value)?),
        });
    }
    Ok(out)
}

/// Every sidecar and `settings.json` of tC4 is `schemaVersion` 1.
fn check_schema(path: &str, top: &Map<String, Value>) -> Result<(), SeedRefusal> {
    match top.get("schemaVersion") {
        Some(v) if v.as_u64() == Some(1) => Ok(()),
        other => Err(SeedRefusal::Schema {
            path: path.to_owned(),
            found: other.map_or_else(|| "(none)".into(), Value::to_string),
        }),
    }
}

/// A file the format does not derive, kept by its git blob id (STORAGE §11.4).
pub(super) fn carried_record(path: &str, bytes: &[u8]) -> Result<(Record, GitOid), SeedRefusal> {
    let repo_path = RepoPath::parse(path).map_err(|e| SeedRefusal::BadValue {
        path: path.to_owned(),
        problem: format!("the file cannot be kept under this name: {e}"),
    })?;
    let mut context = ring::digest::Context::new(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY);
    context.update(format!("blob {}\0", bytes.len()).as_bytes());
    context.update(bytes);
    let mut oid = [0u8; 20];
    oid.copy_from_slice(context.finish().as_ref());
    let oid = GitOid::from_bytes(oid);
    let record = Record::CarriedFile {
        path: repo_path,
        value: CarriedValue::Present {
            oid,
            size: bytes.len() as u64,
            md5: Md5::of(bytes),
        },
    };
    Ok((record, oid))
}

/// The records of an alignments sidecar. tC4's `invalid: true` becomes `markedInvalid`
/// (§13.2: `marked_invalid` from `invalid`), which the decoder reads; the decoder ignores
/// `invalid` itself, a flag the fold computes.
pub(super) fn alignment_records(
    path: &str,
    book: BookId,
    bytes: &[u8],
) -> Result<Vec<Record>, SeedRefusal> {
    let mut doc = read_json(path, bytes)?;
    let Value::Object(top) = &mut doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    if let Some(Value::Object(chapters)) = top.get_mut("chapters") {
        for verses in chapters.values_mut() {
            let Value::Object(verses) = verses else {
                continue;
            };
            for record in verses.values_mut() {
                if let Value::Object(record) = record
                    && record.get("invalid") == Some(&Value::Bool(true))
                    && !record.contains_key("markedInvalid")
                {
                    record.insert("markedInvalid".into(), Value::Bool(true));
                }
            }
        }
    }
    let rewritten = serde_json::to_vec(&doc).map_err(|e| SeedRefusal::Internal(e.to_string()))?;
    let records = decode_alignments(book, &rewritten).map_err(|e| SeedRefusal::Unreadable {
        path: path.to_owned(),
        problem: e.to_string(),
    })?;
    no_two_on_one_key(path, &records)?;
    Ok(records)
}

/// tC4 refuses "co-present records on one identity key", and the fold would show only one
/// of them.
fn no_two_on_one_key(path: &str, records: &[Record]) -> Result<(), SeedRefusal> {
    let mut seen = BTreeSet::new();
    for record in records {
        let key = record.key();
        if !seen.insert(key.clone()) {
            return Err(SeedRefusal::DuplicateRecord {
                path: path.to_owned(),
                key: describe_key(&key),
            });
        }
    }
    Ok(())
}

/// The decision records of a decisions sidecar, each with its time: the decision's own
/// `modifiedTimestamp` (tC4 R-8.8.2), which the projection writes back from it.
///
/// Each tC4 record is first rewritten into the fields the decoder reads (the rule of
/// `crates/tc-journal/tests/screens_and_fold.rs`, `seeded`): `importedInvalid` from
/// `invalidated`; `userStatus` from `status` when the record is not invalidated, and on an
/// invalidated one `todo` if tC4 says To do, `invalid` if `userInvalid` says the Invalid is
/// the person's own, else nothing, since tC4's `status` then only repeats the invalidation.
/// `verseTextMd5` is then the hash of the text the target names now (§13.2), read from
/// `interim`, a fold of the project's texts, as the projection reads it.
pub(super) fn decision_records(
    path: &str,
    tool: ToolId,
    book: Option<BookId>,
    bytes: &[u8],
    interim: &FoldState,
    project_at: WallTime,
) -> Result<Vec<Item>, SeedRefusal> {
    let mut doc = read_json(path, bytes)?;
    let Value::Object(top) = &mut doc else {
        return Err(SeedRefusal::BadValue {
            path: path.to_owned(),
            problem: "it is not a JSON object".into(),
        });
    };
    check_schema(path, top)?;
    let placeholder = verse_text_md5_v1("").to_hex();
    let mut times = Vec::new();
    let mut own_hash = Vec::new();
    if let Some(Value::Array(decisions)) = top.get_mut("decisions") {
        for decision in decisions.iter_mut() {
            let Value::Object(m) = decision else {
                times.push(project_at);
                own_hash.push(false);
                continue;
            };
            let invalidated = m.get("invalidated") == Some(&Value::Bool(true));
            let user_invalid = m.get("userInvalid") == Some(&Value::Bool(true));
            let status = m.get("status").and_then(Value::as_str).map(str::to_owned);
            let user_status = match (invalidated, status.as_deref()) {
                (false, status) => status.map(str::to_owned),
                (true, Some("todo")) => Some("todo".to_owned()),
                (true, _) if user_invalid => Some("invalid".to_owned()),
                (true, _) => None,
            };
            m.entry("importedInvalid")
                .or_insert(Value::Bool(invalidated));
            if let Some(user_status) = user_status {
                m.entry("userStatus").or_insert(Value::from(user_status));
            }
            own_hash.push(m.contains_key("verseTextMd5"));
            m.entry("verseTextMd5")
                .or_insert(Value::from(placeholder.clone()));
            times.push(
                m.get("modifiedTimestamp")
                    .and_then(Value::as_str)
                    .and_then(parse_utc_millis)
                    .map_or(project_at, WallTime),
            );
        }
    }
    let rewritten = serde_json::to_vec(&doc).map_err(|e| SeedRefusal::Internal(e.to_string()))?;
    let mut records =
        decode_decisions(tool, book, &rewritten).map_err(|e| SeedRefusal::Unreadable {
            path: path.to_owned(),
            problem: e.to_string(),
        })?;
    if records.len() != times.len() {
        return Err(SeedRefusal::Internal(format!(
            "{path}: {} decisions decoded from {}",
            records.len(),
            times.len()
        )));
    }
    let book_texts = book.map(|b| interim.check_texts(Collection::Book(b)));
    let story_texts = interim.check_texts(Collection::Stories);
    for (record, keep) in records.iter_mut().zip(&own_hash) {
        let Record::Check {
            key,
            value: CheckValue::Decision(decision),
        } = record
        else {
            continue;
        };
        let texts = match (&key.target, book) {
            (CheckTarget::Verse { book: target, .. }, Some(file)) if *target == file => {
                book_texts.as_ref()
            }
            (CheckTarget::Frame { .. }, None) => Some(&story_texts),
            _ => {
                return Err(SeedRefusal::BadValue {
                    path: path.to_owned(),
                    problem: format!("{key} does not belong in this file"),
                });
            }
        };
        if !keep {
            decision.judgement.verse_text_md5 = texts
                .and_then(|t| t.text(&key.target))
                .map_or_else(|| verse_text_md5_v1(""), |t| t.md5());
        }
    }
    no_two_on_one_key(path, &records)?;
    Ok(records
        .into_iter()
        .zip(times)
        .map(|(record, at)| Item {
            record,
            at,
            origin: path.to_owned(),
        })
        .collect())
}
