//! Reading the project files back into records (§11.1 X2): `resources.json`,
//! `settings.json`, `vrs.json`, the comments files and `metadata.json`. Each decoder is the
//! inverse of the projection in `project.rs` (`sidecar_decode_render` proves it): rendering
//! a file and decoding it gives every record of its scope again, so any byte a person
//! changes maps to a record change or to a derived field.
//!
//! A file no decoder can read is an error in words (`SidecarError`), which the store turns
//! into a byte copy and a notice; nothing is recorded from it.

use super::sidecar::{SidecarError, fail};
use crate::canon;
use crate::ids::CommentId;
use crate::json::Json;
use crate::keys::{
    MetaField, MetaPath, PinGroup, ResourcesKey, ResourcesLevel, SettingKey, SlotName,
};
use crate::record::{PinGroupValue, Record, SlotValue, VersificationValue};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

fn json(v: &Value, at: &str) -> Result<Json, SidecarError> {
    Json::try_from(v).map_err(|e| fail(at, e.to_string()))
}

fn document(bytes: &[u8]) -> Result<Map<String, Value>, SidecarError> {
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| fail("the file", e.to_string()))?;
    match doc {
        Value::Object(map) => Ok(map),
        _ => Err(fail("the file", "expected an object")),
    }
}

/// A group of `resources.json` read back: the object of its slots.
fn group(value: &Value, at: &str) -> Result<PinGroupValue, SidecarError> {
    let Value::Object(map) = value else {
        return Err(fail(at, "expected an object of slots"));
    };
    let mut slots = BTreeMap::new();
    for (name, slot) in map {
        let slot_name =
            SlotName::parse(name).map_err(|_| fail(at, format!("{name} is not a slot name")))?;
        slots.insert(slot_name, SlotValue(json(slot, &format!("{at}.{name}"))?));
    }
    Ok(PinGroupValue(slots))
}

/// Every record of a `resources.json`: the five pin groups, and one `ResourcesExtra` for
/// each member no group holds. A group the file lacks is not a record here; the caller
/// compares with what the project holds.
pub fn decode_resources(bytes: &[u8]) -> Result<Vec<Record>, SidecarError> {
    let doc = document(bytes)?;
    match doc.get("schemaVersion").and_then(Value::as_u64) {
        Some(2) => {}
        _ => return Err(fail("schemaVersion", "expected 2")),
    }
    let mut out = Vec::new();
    let mut extra = |level: ResourcesLevel, map: &Map<String, Value>| -> Result<(), SidecarError> {
        for (name, value) in map {
            if level.owned().contains(&name.as_str()) {
                continue;
            }
            let key =
                ResourcesKey::new(level, name).map_err(|e| fail(name.as_str(), e.to_string()))?;
            out.push(Record::ResourcesExtra {
                key,
                value: Some(json(value, name)?),
            });
        }
        Ok(())
    };
    extra(ResourcesLevel::Top, &doc)?;
    let mut groups: Vec<(PinGroup, PinGroupValue)> = Vec::new();
    for (level, member, members) in [
        (
            ResourcesLevel::LanguageSets,
            "languageSets",
            [
                ("primary", PinGroup::Primary),
                ("fallback", PinGroup::Fallback),
            ],
        ),
        (
            ResourcesLevel::Resources,
            "resources",
            [
                ("originalLanguage", PinGroup::OriginalLanguage),
                ("lexicon", PinGroup::Lexicon),
            ],
        ),
    ] {
        match doc.get(member) {
            None | Some(Value::Null) => {}
            Some(Value::Object(map)) => {
                extra(level, map)?;
                for (name, pin_group) in members {
                    if let Some(v) = map.get(name).filter(|v| !v.is_null()) {
                        groups.push((pin_group, group(v, &format!("{member}.{name}"))?));
                    }
                }
            }
            Some(_) => return Err(fail(member, "expected an object")),
        }
    }
    if let Some(list) = doc.get("extraScripture").filter(|v| !v.is_null()) {
        // One slot holds the ordered list (§5.6).
        let slot = SlotName::parse("extraScripture")
            .map_err(|_| fail("extraScripture", "no slot holds the list"))?;
        let mut slots = BTreeMap::new();
        slots.insert(slot, SlotValue(json(list, "extraScripture")?));
        groups.push((PinGroup::ExtraScripture, PinGroupValue(slots)));
    }
    for (pin_group, value) in groups {
        out.push(Record::Pins {
            group: pin_group,
            value,
        });
    }
    Ok(out)
}

/// Every `Setting` of a `settings.json`: one per top-level member but `schemaVersion`.
pub fn decode_settings(bytes: &[u8]) -> Result<Vec<Record>, SidecarError> {
    let doc = document(bytes)?;
    let mut out = Vec::new();
    for (name, value) in &doc {
        if name == "schemaVersion" {
            continue;
        }
        let key = SettingKey::parse(name).map_err(|e| fail(name.as_str(), e.to_string()))?;
        out.push(Record::Setting {
            key,
            value: Some(json(value, name)?),
        });
    }
    Ok(out)
}

/// The `Versification` record of a `vrs.json`: its bytes verbatim. The file holds no name,
/// so the caller supplies the one the project has.
pub fn decode_versification(bytes: &[u8], name: Option<&str>) -> Result<Record, SidecarError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| fail("the file", "the versification is not UTF-8 text"))?;
    Ok(Record::Versification {
        value: VersificationValue {
            name: name.map(Into::into),
            bytes: text.to_owned(),
        },
    })
}

/// One entry of a comments file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedComment {
    /// `Some` when the file's `id` is a comment id.
    pub id: Option<CommentId>,
    pub target: crate::record::CommentTarget,
    pub text: String,
}

/// The comments of `tc/comments/<BOOK>.json` or `OBS.json`. `placed`, `author` and `at` are
/// computed and ignored.
pub fn decode_comments(bytes: &[u8]) -> Result<Vec<DecodedComment>, SidecarError> {
    let doc = document(bytes)?;
    let list = doc
        .get("comments")
        .and_then(Value::as_array)
        .ok_or_else(|| fail("the file", "no comments"))?;
    let mut out = Vec::with_capacity(list.len());
    for (i, entry) in list.iter().enumerate() {
        let at = format!("comment {}", i + 1);
        let Value::Object(m) = entry else {
            return Err(fail(&at, "expected an object"));
        };
        let target = m
            .get("target")
            .ok_or_else(|| fail(&at, "no target"))
            .and_then(|t| json(t, &at))
            .and_then(|t| {
                canon::decode_comment_target(&t).ok_or_else(|| fail(&at, "the target is not valid"))
            })?;
        let text = m
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| fail(&at, "text is not a string"))?
            .to_owned();
        let id = m
            .get("id")
            .and_then(Value::as_str)
            .and_then(CommentId::parse);
        out.push(DecodedComment { id, target, text });
    }
    Ok(out)
}

/// What a `metadata.json` says: the fields the records hold, and the fields the builder
/// derives (compared by the caller with the document it would write).
#[derive(Debug, Clone, PartialEq)]
pub struct DecodedMetadata {
    /// The records' fields.
    pub fields: Vec<(MetaField, Json)>,
    /// Every derived field found, by dotted path.
    pub derived: BTreeMap<String, Value>,
    /// Members that no record can hold (a name with a dot in it), by dotted path.
    pub unrecordable: Vec<String>,
}

/// Where a field of `metadata.json` is recorded: the dotted path of its record, or `None`
/// when the path is derived (§3.4).
fn field_of(path: &str) -> Option<MetaField> {
    Some(match path {
        "identification.name" => MetaField::Name,
        "identification.abbreviation" => MetaField::Abbreviation,
        "identification.description" => MetaField::Description,
        "languages" => MetaField::Languages,
        "copyright" => MetaField::Copyright,
        _ => MetaField::Extra(MetaPath::parse(path).ok()?),
    })
}

/// The members of `metadata.json` read back at the granularity the fold writes: a top-level
/// member is one record, except the ones that hold records of their own (`meta`,
/// `idAuthorities`, `identification` and its `primary`).
pub fn decode_metadata(bytes: &[u8]) -> Result<DecodedMetadata, SidecarError> {
    let doc = document(bytes)?;
    let mut out = DecodedMetadata {
        fields: Vec::new(),
        derived: BTreeMap::new(),
        unrecordable: Vec::new(),
    };
    let leaf = |path: String, value: &Value, out: &mut DecodedMetadata| {
        if tc_burrito::is_derived_field(&path) {
            out.derived.insert(path, value.clone());
            return Ok::<(), SidecarError>(());
        }
        match (field_of(&path), Json::try_from(value)) {
            (Some(field), Ok(j)) => out.fields.push((field, j)),
            _ => out.unrecordable.push(path),
        }
        Ok(())
    };
    for (name, value) in &doc {
        let nested: &[&str] = match name.as_str() {
            "meta" => &["meta"],
            "idAuthorities" => &["idAuthorities"],
            "identification" => &["identification"],
            _ => &[],
        };
        match (nested.first(), value) {
            (Some(&top), Value::Object(map)) => {
                for (member, v) in map {
                    let path = format!("{top}.{member}");
                    // `identification.primary` holds one record per authority.
                    if path == "identification.primary"
                        && let Value::Object(authorities) = v
                    {
                        for (authority, a) in authorities {
                            leaf(format!("{path}.{authority}"), a, &mut out)?;
                        }
                    } else {
                        leaf(path, v, &mut out)?;
                    }
                }
            }
            _ => leaf(name.clone(), value, &mut out)?,
        }
    }
    out.fields.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}
