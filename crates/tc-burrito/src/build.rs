//! `metadata.json` of a tC4 project, built from the project's records (`docs/STORAGE.md`
//! §3.4). The fold hands over each field's value already checked on its own with
//! [`check_field`], so the document always passes; a value that would not is never
//! projected, and the fold says so in words.

use crate::ingredient::{Md5, MimeType};
use crate::json::to_canonical_json;
use crate::model::ProjectKind;
use crate::path::IngredientPath;
use crate::validate::{Problem, validate_metadata};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// Paths the builder derives itself. A field the records hold at one of these, or inside
/// one, is not written: the derived value wins and the record stays in History.
pub const DERIVED_FIELDS: &[&str] = &[
    "format",
    "meta.version",
    "meta.category",
    "meta.generator",
    "meta.normalization",
    "idAuthorities.local",
    "idAuthorities.dcs",
    "identification.primary.local",
    "type",
    "relationships",
    "ingredients",
];

/// Whether `path` is a derived field or inside one.
pub fn is_derived_field(path: &str) -> bool {
    DERIVED_FIELDS.iter().any(|d| {
        path == *d
            || path
                .strip_prefix(d)
                .is_some_and(|rest| rest.starts_with('.'))
    })
}

/// What `metadata.json` is built from, besides the files.
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataSource {
    pub kind: ProjectKind,
    /// The project format version: `generator.softwareVersion` is `tc-project-<major>.<minor>`.
    pub format: (u64, u64),
    /// Field values by their dotted path in the document (`identification.name`,
    /// `languages`, `localizedNames`, `idAuthorities.ebible`, `agencies`, …), each
    /// checked with [`check_field`]. Paths of [`DERIVED_FIELDS`] are ignored.
    pub fields: BTreeMap<String, Value>,
    /// `type.flavorType.currentScope`: the present books' scopes (Bible), or the template
    /// table (stories).
    pub scope: Value,
    /// The pins mirrored as relationships (tC4 `relationshipsFromPins`).
    pub relationships: Vec<Value>,
    /// Every USFM and story ingredient is in NFC.
    pub nfc: bool,
}

/// One file `metadata.json` lists, with the facts of the exact bytes on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngredientEntry {
    pub path: IngredientPath,
    pub md5: Md5,
    pub size: u64,
    pub role: Option<String>,
    /// The entry's scope, as `{BOOK: [ranges]}`.
    pub scope: Option<Value>,
}

const DEFAULT_CREATED: &str = "2026-01-01T00:00:00.000Z";

/// The defaults of fields a project always sets at creation, used only when the records
/// hold no valid value at all.
fn default_of(path: &str, locale: &str) -> Option<Value> {
    Some(match path {
        "meta.defaultLocale" => json!("en"),
        "meta.dateCreated" => json!(DEFAULT_CREATED),
        "identification.name" => json!({ locale: "translationCore project" }),
        "languages" => json!([{ "tag": "und", "name": { "en": "Undetermined" } }]),
        "copyright" => json!({ "shortStatements": [{ "statement": "Not stated" }] }),
        "confidential" => json!(false),
        "localizedNames" => json!({}),
        _ => return None,
    })
}

fn set_path(root: &mut Map<String, Value>, path: &str, value: Value) {
    let mut segments: Vec<&str> = path.split('.').collect();
    let Some(last) = segments.pop() else {
        return;
    };
    let mut at = root;
    for segment in segments {
        let next = at
            .entry(segment.to_owned())
            .or_insert_with(|| Value::Object(Map::new()));
        if !next.is_object() {
            *next = Value::Object(Map::new());
        }
        match next {
            Value::Object(map) => at = map,
            _ => return,
        }
    }
    at.insert(last.to_owned(), value);
}

fn get_path<'a>(root: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    let mut segments = path.split('.');
    let mut at = root.get(segments.next()?)?;
    for segment in segments {
        at = at.as_object()?.get(segment)?;
    }
    Some(at)
}

/// The local identifier: the abbreviation in the default locale, else the first one, else
/// `project`, made a valid identifier.
fn local_id(fields: &BTreeMap<String, Value>, locale: &str) -> String {
    let abbreviation = fields
        .get("identification.abbreviation")
        .and_then(|v| match v {
            Value::Object(map) => map
                .get(locale)
                .or_else(|| map.values().next())
                .and_then(Value::as_str),
            Value::String(s) => Some(s.as_str()),
            _ => None,
        });
    let cleaned: String = abbreviation
        .unwrap_or("project")
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "project".into()
    } else {
        cleaned
    }
}

fn document(source: &MetadataSource, ingredients: &[IngredientEntry]) -> Value {
    let fields: BTreeMap<String, Value> = source
        .fields
        .iter()
        .filter(|(path, _)| !is_derived_field(path))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let locale = fields
        .get("meta.defaultLocale")
        .and_then(Value::as_str)
        .unwrap_or("en")
        .to_owned();
    let created = fields
        .get("meta.dateCreated")
        .and_then(Value::as_str)
        .unwrap_or(DEFAULT_CREATED)
        .to_owned();
    let mut root = Map::new();
    root.insert("format".into(), json!("scripture burrito"));
    let mut meta = json!({
        "version": "1.0.0",
        "category": "source",
        "generator": {
            "softwareName": "translationCore",
            "softwareVersion": format!("tc-project-{}.{}", source.format.0, source.format.1),
        },
        "defaultLocale": locale,
        "dateCreated": created,
    });
    if source.nfc
        && let Value::Object(m) = &mut meta
    {
        m.insert("normalization".into(), json!("NFC"));
    }
    root.insert("meta".into(), meta);
    let mut authorities = Map::new();
    authorities.insert(
        "local".into(),
        json!({ "id": "http://_local_", "name": { "en": "Local Project" } }),
    );
    if !source.relationships.is_empty() {
        authorities.insert(
            "dcs".into(),
            json!({ "id": "https://git.door43.org", "name": { "en": "Door43 Content Service" } }),
        );
    }
    root.insert("idAuthorities".into(), Value::Object(authorities));
    root.insert(
        "identification".into(),
        json!({
            "primary": { "local": { local_id(&fields, &locale): { "revision": "1", "timestamp": created } } },
        }),
    );
    let flavor_type = match source.kind {
        ProjectKind::BibleText => json!({
            "name": "scripture",
            "flavor": {
                "name": "textTranslation",
                "usfmVersion": "3.0",
                "translationType": "firstTranslation",
                "audience": "common",
                "projectType": "standard",
            },
            "currentScope": source.scope,
        }),
        ProjectKind::OpenBibleStories => json!({
            "name": "gloss",
            "flavor": { "name": "textStories" },
            "currentScope": source.scope,
        }),
    };
    root.insert("type".into(), json!({ "flavorType": flavor_type }));
    for path in [
        "identification.name",
        "languages",
        "copyright",
        "confidential",
        "localizedNames",
    ] {
        if !fields.contains_key(path)
            && let Some(value) = default_of(path, &locale)
        {
            set_path(&mut root, path, value);
        }
    }
    for (path, value) in &fields {
        set_path(&mut root, path, value.clone());
    }
    if !source.relationships.is_empty() {
        root.insert(
            "relationships".into(),
            Value::Array(source.relationships.clone()),
        );
    }
    let mut listed = Map::new();
    for entry in ingredients {
        let mut item = Map::new();
        item.insert("checksum".into(), json!({ "md5": entry.md5.to_string() }));
        item.insert(
            "mimeType".into(),
            json!(MimeType::for_path(&entry.path).as_str()),
        );
        item.insert("size".into(), json!(entry.size));
        if let Some(role) = &entry.role {
            item.insert("role".into(), json!(role));
        }
        if let Some(scope) = &entry.scope {
            item.insert("scope".into(), scope.clone());
        }
        listed.insert(entry.path.as_str().to_owned(), Value::Object(item));
    }
    root.insert("ingredients".into(), Value::Object(listed));
    Value::Object(root)
}

/// The bytes of `metadata.json` (§3.4): the canonical writer's form (keys sorted, two-space
/// indent, one trailing line feed). The document is checked before it is returned; a
/// problem here is a bug, reported, and nothing is written.
pub fn build_metadata(
    source: &MetadataSource,
    ingredients: &[IngredientEntry],
) -> Result<Vec<u8>, Vec<Problem>> {
    let document = document(source, ingredients);
    let problems = validate_metadata(&document);
    if problems.is_empty() {
        Ok(to_canonical_json(&document))
    } else {
        Err(problems)
    }
}

/// A document every check passes, to try one field in.
fn baseline(kind: ProjectKind) -> MetadataSource {
    MetadataSource {
        kind,
        format: (1, 0),
        fields: BTreeMap::new(),
        scope: match kind {
            ProjectKind::BibleText => json!({ "TIT": [] }),
            ProjectKind::OpenBibleStories => json!({ "GEN": [] }),
        },
        relationships: Vec::new(),
        nfc: false,
    }
}

fn baseline_ingredient() -> Vec<IngredientEntry> {
    match IngredientPath::parse("ingredients/TIT.usfm") {
        Ok(path) => vec![IngredientEntry {
            path,
            md5: Md5::of(b""),
            size: 0,
            role: None,
            scope: Some(json!({ "TIT": [] })),
        }],
        Err(_) => Vec::new(),
    }
}

/// Checks one field's value on its own (§3.4): the value set at `path` in an otherwise
/// valid document passes every check. `Err` says why, in words.
pub fn check_field(kind: ProjectKind, path: &str, value: &Value) -> Result<(), String> {
    if is_derived_field(path) {
        return Err(format!(
            "{path} is written by translationCore, not by a person"
        ));
    }
    let mut source = baseline(kind);
    source.fields.insert(path.to_owned(), value.clone());
    let document = document(&source, &baseline_ingredient());
    // The value must land where it was put, unchanged: a path through a non-object would
    // replace it.
    if let Value::Object(root) = &document
        && get_path(root, path) != Some(value)
    {
        return Err(format!("{path} cannot hold a value there"));
    }
    first_problem(validate_metadata(&document))
}

/// Checks a book's scope ranges as `currentScope` would hold them.
pub fn check_scope(kind: ProjectKind, book: &str, ranges: &[String]) -> Result<(), String> {
    let mut source = baseline(kind);
    let mut scope = Map::new();
    scope.insert(book.to_owned(), json!(ranges));
    source.scope = Value::Object(scope);
    first_problem(validate_metadata(&document(
        &source,
        &baseline_ingredient(),
    )))
}

/// Checks one relationship as `relationships` would hold it.
pub fn check_relationship(kind: ProjectKind, relationship: &Value) -> Result<(), String> {
    let mut source = baseline(kind);
    source.relationships = vec![relationship.clone()];
    first_problem(validate_metadata(&document(
        &source,
        &baseline_ingredient(),
    )))
}

fn first_problem(problems: Vec<Problem>) -> Result<(), String> {
    match problems.into_iter().next() {
        None => Ok(()),
        Some(problem) => Err(problem.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_document_passes_and_a_bad_field_is_named() {
        let mut source = baseline(ProjectKind::BibleText);
        source
            .fields
            .insert("identification.name".into(), json!({ "en": "Tito" }));
        let bytes = build_metadata(&source, &baseline_ingredient()).expect("valid");
        assert!(crate::validate_metadata_bytes(&bytes).is_empty());
        assert!(check_field(ProjectKind::BibleText, "copyright", &json!("All rights")).is_err());
        assert!(
            check_field(
                ProjectKind::BibleText,
                "copyright",
                &json!({ "shortStatements": [{ "statement": "CC BY-SA 4.0" }] })
            )
            .is_ok()
        );
        assert!(check_field(ProjectKind::BibleText, "meta.generator", &json!({})).is_err());
        assert!(check_scope(ProjectKind::BibleText, "TIT", &["1:1-9".into()]).is_ok());
        assert!(check_scope(ProjectKind::BibleText, "TIT", &["one".into()]).is_err());
    }
}
