//! The validator: the SB 1.0.0 schema, then the rules the schema misses, then the files.
//!
//! Every check reads the document as untyped JSON, so it works on any input, and returns
//! problems instead of stopping at the first one. An empty list means the burrito is valid.

use crate::files::{DirProject, FileState, ProjectFiles};
use crate::ingredient::is_schema_mime_type;
use crate::json::{StrictJsonError, parse_strict};
use crate::model::Metadata;
use crate::path::{IngredientPath, PathRule, check as check_path};
use crate::schema::validator;
use crate::time::{TimestampError, check_date_time, is_schema_date};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::Path;

/// A reason a burrito is not valid. Problems name the place with a JSON Pointer or an
/// ingredient path, so a caller can map each one to a message or a refusal code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Problem {
    #[error("the project has no metadata.json")]
    MissingMetadata,
    #[error("{path} could not be read: {message}")]
    Unreadable { path: String, message: String },
    #[error("metadata.json is not JSON: {message}")]
    NotJson { message: String },
    #[error("metadata.json has the key at {pointer:?} more than once")]
    DuplicateKey { pointer: String },
    /// The embedded schema did not compile. Every document is then refused.
    #[error("the Scripture Burrito schema could not be loaded: {message}")]
    SchemaUnavailable { message: String },
    /// The SB 1.0.0 JSON schema refused the value at `pointer`.
    #[error("{message} (at {pointer:?}, schema keyword {keyword})")]
    Schema {
        pointer: String,
        keyword: String,
        schema_path: String,
        message: String,
    },
    /// The 1.0.0 schema means to require `languages` for scripture and gloss burritos, but
    /// its condition never matches (spec issue #343).
    #[error("languages is missing; a scripture or gloss burrito must list its languages")]
    MissingLanguages,
    #[error("idAuthorities label {label:?} must be lower-case letters, digits and '-'")]
    InvalidAuthorityLabel { label: String },
    #[error(
        "identification.primary uses the authority {authority:?}, which idAuthorities does not declare"
    )]
    UndeclaredAuthority { authority: String },
    #[error("identification.primary revision {value:?} of {id:?} is not a revision string")]
    InvalidRevision { id: String, value: String },
    #[error("{field} {value:?}: {reason}")]
    InvalidTimestamp {
        field: TimestampField,
        value: String,
        reason: TimestampError,
    },
    #[error("ingredient path {path:?}: {rule}")]
    UnsafePath { path: String, rule: PathRule },
    #[error(
        "ingredient paths {first:?} and {second:?} differ only in letter case, so one overwrites the other on Windows and macOS"
    )]
    PathCaseCollision { first: String, second: String },
    #[error(
        "ingredient {path:?} has mimeType {value:?}, which is not lower-case type/subtype without dots or parameters"
    )]
    InvalidMimeType { path: String, value: String },
    #[error("ingredient {path:?} has no checksum.md5")]
    MissingChecksum { path: String },
    #[error("currentScope lists {book}, but no text ingredient has it in its scope")]
    UncoveredBook { book: String },
    #[error(
        "copyright needs exactly one of publicDomain: true, shortStatements and licenses; it has {found:?}"
    )]
    CopyrightNotExactlyOne { found: Vec<CopyrightBasis> },
    #[error("ingredient {path:?} is listed but there is no file")]
    MissingIngredient { path: String },
    #[error("ingredient {path:?} is a folder, a link or another special file")]
    NotAFile { path: String },
    #[error("ingredient {path:?} lists md5 {listed}, but the file's md5 is {actual}")]
    ChecksumMismatch {
        path: String,
        listed: String,
        actual: String,
    },
    #[error("ingredient {path:?} lists size {listed}, but the file has {actual} bytes")]
    SizeMismatch {
        path: String,
        listed: u64,
        actual: u64,
    },
    #[error("{path:?} is in the ingredients folder, but metadata.json does not list it")]
    UnlistedFile { path: String },
    /// The document passed every other check but does not read into [`Metadata`], so the
    /// app could not open it; for example, a `size` written as `1166.0`.
    #[error("the metadata passes every other check but does not fit the burrito model: {message}")]
    ModelMismatch { message: String },
}

/// Which timestamp a [`Problem::InvalidTimestamp`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimestampField {
    /// `meta.dateCreated`: an RFC 3339 date-time with a timezone.
    DateCreated,
    /// `identification.primary.*.*.timestamp`: an RFC 3339 date-time with a timezone, or
    /// the date the schema intends (`YYYY`, `YYYY-MM` or `YYYY-MM-DD`).
    PrimaryTimestamp,
}

impl fmt::Display for TimestampField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DateCreated => "meta.dateCreated",
            Self::PrimaryTimestamp => "identification.primary timestamp",
        })
    }
}

/// One of the three ways `copyright` can state its terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CopyrightBasis {
    PublicDomain,
    ShortStatements,
    Licenses,
}

/// Schema messages repeat the refused value, which can be the whole document.
const MAX_MESSAGE_CHARS: usize = 400;

/// Checks a `metadata.json` document on its own: the schema and the rules the schema
/// misses. It does not look at files; see [`validate_project`].
pub fn validate_metadata(document: &Value) -> Vec<Problem> {
    let mut problems = schema_problems(document);
    if let Some(root) = document.as_object() {
        check_languages(root, &mut problems);
        check_authorities(root, &mut problems);
        check_date_created(root, &mut problems);
        check_ingredient_entries(root, &mut problems);
        check_scope_coverage(root, &mut problems);
        check_copyright(root, &mut problems);
    }
    if problems.is_empty()
        && let Err(error) = Metadata::deserialize(document)
    {
        problems.push(Problem::ModelMismatch {
            message: error.to_string(),
        });
    }
    problems
}

/// Parses `bytes` strictly (a key written twice is a problem) and checks the document.
pub fn validate_metadata_bytes(bytes: &[u8]) -> Vec<Problem> {
    match parse_document(bytes) {
        Ok(document) => validate_metadata(&document),
        Err(problem) => vec![problem],
    }
}

/// Checks `metadata.json` bytes together with the project's files: every listed ingredient
/// exists and has the listed md5 and size, and every file under `ingredients/` is listed.
pub fn validate_project(metadata: &[u8], files: &(impl ProjectFiles + ?Sized)) -> Vec<Problem> {
    let document = match parse_document(metadata) {
        Ok(document) => document,
        Err(problem) => return vec![problem],
    };
    let mut problems = validate_metadata(&document);
    check_files(&document, files, &mut problems);
    problems
}

/// Checks the project folder at `root`.
pub fn validate_dir(root: &Path) -> Vec<Problem> {
    match std::fs::read(root.join("metadata.json")) {
        Ok(bytes) => validate_project(&bytes, &DirProject::new(root)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => vec![Problem::MissingMetadata],
        Err(error) => vec![Problem::Unreadable {
            path: "metadata.json".to_owned(),
            message: error.to_string(),
        }],
    }
}

fn parse_document(bytes: &[u8]) -> Result<Value, Problem> {
    parse_strict(bytes).map_err(|error| match error {
        StrictJsonError::Syntax(error) => Problem::NotJson {
            message: error.to_string(),
        },
        StrictJsonError::DuplicateKey { pointer } => Problem::DuplicateKey { pointer },
    })
}

fn schema_problems(document: &Value) -> Vec<Problem> {
    match validator() {
        Ok(validator) => validator
            .iter_errors(document)
            .map(|error| Problem::Schema {
                pointer: error.instance_path().to_string(),
                keyword: error.kind().keyword().to_owned(),
                schema_path: error.schema_path().to_string(),
                message: truncate(error.to_string()),
            })
            .collect(),
        Err(error) => vec![Problem::SchemaUnavailable {
            message: error.to_string(),
        }],
    }
}

fn truncate(mut message: String) -> String {
    if let Some((cut, _)) = message.char_indices().nth(MAX_MESSAGE_CHARS) {
        message.truncate(cut);
        message.push('…');
    }
    message
}

fn flavor_type(root: &Map<String, Value>) -> Option<&Map<String, Value>> {
    root.get("type")?.get("flavorType")?.as_object()
}

fn check_languages(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let needs_languages = flavor_type(root)
        .and_then(|flavor_type| flavor_type.get("name"))
        .and_then(Value::as_str)
        .is_some_and(|name| name == "scripture" || name == "gloss");
    if needs_languages && !root.contains_key("languages") {
        out.push(Problem::MissingLanguages);
    }
}

/// The schema's `idAuthorityLabel` pattern, `^[a-z][a-z0-9\-]*[a-z0-9]$`, which its
/// mis-nested `propertyNames` never applies.
fn is_authority_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() >= 2
        && first.is_ascii_lowercase()
        && (last.is_ascii_lowercase() || last.is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// The schema's `revisionString` pattern,
/// `^[0-9A-Za-z]([0-9A-Za-z_.:\-]{0,62}[0-9A-Za-z])?$`, which it never applies to
/// `identification.primary`.
fn is_revision_string(revision: &str) -> bool {
    let bytes = revision.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };
    bytes.len() <= 64
        && first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
}

fn check_authorities(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let declared = root.get("idAuthorities").and_then(Value::as_object);
    if let Some(declared) = declared {
        for label in declared.keys() {
            if !is_authority_label(label) {
                out.push(Problem::InvalidAuthorityLabel {
                    label: label.clone(),
                });
            }
        }
    }
    let primary = root
        .get("identification")
        .and_then(|identification| identification.get("primary"))
        .and_then(Value::as_object);
    let Some(primary) = primary else { return };
    for (authority, ids) in primary {
        if declared.is_some_and(|declared| !declared.contains_key(authority)) {
            out.push(Problem::UndeclaredAuthority {
                authority: authority.clone(),
            });
        }
        let Some(ids) = ids.as_object() else { continue };
        for (id, entry) in ids {
            if let Some(revision) = entry.get("revision").and_then(Value::as_str)
                && !is_revision_string(revision)
            {
                out.push(Problem::InvalidRevision {
                    id: id.clone(),
                    value: revision.to_owned(),
                });
            }
            if let Some(timestamp) = entry.get("timestamp").and_then(Value::as_str)
                && !is_schema_date(timestamp)
                && let Err(reason) = check_date_time(timestamp)
            {
                out.push(Problem::InvalidTimestamp {
                    field: TimestampField::PrimaryTimestamp,
                    value: timestamp.to_owned(),
                    reason,
                });
            }
        }
    }
}

fn check_date_created(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let created = root
        .get("meta")
        .and_then(|meta| meta.get("dateCreated"))
        .and_then(Value::as_str);
    if let Some(created) = created
        && let Err(reason) = check_date_time(created)
    {
        out.push(Problem::InvalidTimestamp {
            field: TimestampField::DateCreated,
            value: created.to_owned(),
            reason,
        });
    }
}

fn check_ingredient_entries(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let Some(ingredients) = root.get("ingredients").and_then(Value::as_object) else {
        return;
    };
    let mut by_folded_case: BTreeMap<String, &str> = BTreeMap::new();
    for (path, entry) in ingredients {
        if let Err(rule) = check_path(path) {
            out.push(Problem::UnsafePath {
                path: path.clone(),
                rule,
            });
        }
        if let Some(first) = by_folded_case.insert(path.to_lowercase(), path) {
            out.push(Problem::PathCaseCollision {
                first: first.to_owned(),
                second: path.clone(),
            });
        }
        let Some(entry) = entry.as_object() else {
            continue;
        };
        if let Some(mime) = entry.get("mimeType").and_then(Value::as_str)
            && !is_schema_mime_type(mime)
        {
            out.push(Problem::InvalidMimeType {
                path: path.clone(),
                value: mime.to_owned(),
            });
        }
        let has_md5 = entry
            .get("checksum")
            .and_then(Value::as_object)
            .is_some_and(|checksum| checksum.contains_key("md5"));
        if !has_md5 {
            out.push(Problem::MissingChecksum { path: path.clone() });
        }
    }
}

/// For a scripture burrito, every book of `currentScope` must appear in the scope of an
/// ingredient. For `textTranslation` the ingredient must be text (`text/*`), so an
/// alignment or check file in JSON does not stand in for a missing USFM book. Coverage is
/// by book; chapter and verse ranges are not compared.
fn check_scope_coverage(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let Some(flavor_type) = flavor_type(root) else {
        return;
    };
    if flavor_type.get("name").and_then(Value::as_str) != Some("scripture") {
        return;
    }
    let text_only = flavor_type
        .get("flavor")
        .and_then(|flavor| flavor.get("name"))
        .and_then(Value::as_str)
        == Some("textTranslation");
    let Some(current) = flavor_type.get("currentScope").and_then(Value::as_object) else {
        return;
    };
    let ingredients = root.get("ingredients").and_then(Value::as_object);
    let covers = |entry: &Value, book: &str| {
        let in_scope = entry
            .get("scope")
            .and_then(Value::as_object)
            .is_some_and(|scope| scope.contains_key(book));
        let is_text = entry
            .get("mimeType")
            .and_then(Value::as_str)
            .is_some_and(|mime| mime.starts_with("text/"));
        in_scope && (is_text || !text_only)
    };
    for book in current.keys() {
        let covered = ingredients
            .is_some_and(|ingredients| ingredients.values().any(|entry| covers(entry, book)));
        if !covered {
            out.push(Problem::UncoveredBook { book: book.clone() });
        }
    }
}

fn check_copyright(root: &Map<String, Value>, out: &mut Vec<Problem>) {
    let Some(copyright) = root.get("copyright").and_then(Value::as_object) else {
        return;
    };
    let mut found = Vec::new();
    if copyright.get("publicDomain") == Some(&Value::Bool(true)) {
        found.push(CopyrightBasis::PublicDomain);
    }
    if copyright.contains_key("shortStatements") {
        found.push(CopyrightBasis::ShortStatements);
    }
    if copyright.contains_key("licenses") {
        found.push(CopyrightBasis::Licenses);
    }
    if found.len() != 1 {
        out.push(Problem::CopyrightNotExactlyOne { found });
    }
}

fn check_files(document: &Value, files: &(impl ProjectFiles + ?Sized), out: &mut Vec<Problem>) {
    let Some(ingredients) = document.get("ingredients").and_then(Value::as_object) else {
        return;
    };
    for (key, entry) in ingredients {
        // An unsafe path was reported above and is never joined to the project root.
        let Ok(path) = IngredientPath::parse(key) else {
            continue;
        };
        let facts = match files.inspect(&path) {
            FileState::Present(facts) => facts,
            FileState::Missing => {
                out.push(Problem::MissingIngredient { path: key.clone() });
                continue;
            }
            FileState::NotAFile => {
                out.push(Problem::NotAFile { path: key.clone() });
                continue;
            }
            FileState::Unreadable(message) => {
                out.push(Problem::Unreadable {
                    path: key.clone(),
                    message,
                });
                continue;
            }
        };
        let actual = facts.md5.to_string();
        if let Some(listed) = entry
            .get("checksum")
            .and_then(|checksum| checksum.get("md5"))
            .and_then(Value::as_str)
            // Upper-case hex is already a schema problem; it is not also a mismatch.
            && !listed.eq_ignore_ascii_case(&actual)
        {
            out.push(Problem::ChecksumMismatch {
                path: key.clone(),
                listed: listed.to_owned(),
                actual,
            });
        }
        if let Some(listed) = entry.get("size").and_then(Value::as_u64)
            && listed != facts.size
        {
            out.push(Problem::SizeMismatch {
                path: key.clone(),
                listed,
                actual: facts.size,
            });
        }
    }
    match files.ingredient_dir_files() {
        Ok(found) => {
            for path in found {
                if !ingredients.contains_key(&path) {
                    out.push(Problem::UnlistedFile { path });
                }
            }
        }
        Err(message) => out.push(Problem::Unreadable {
            path: crate::files::INGREDIENTS_DIR.to_owned(),
            message,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn problems_serialize_as_tagged_camel_case_objects() {
        let problems = [
            Problem::MissingLanguages,
            Problem::UnsafePath {
                path: "c:/x".to_owned(),
                rule: PathRule::ForbiddenChar(':'),
            },
            Problem::Schema {
                pointer: "/meta".to_owned(),
                keyword: "required".to_owned(),
                schema_path: "/required".to_owned(),
                message: "m".to_owned(),
            },
            Problem::InvalidTimestamp {
                field: TimestampField::DateCreated,
                value: "x".to_owned(),
                reason: TimestampError::MissingTimezone,
            },
        ];
        assert_eq!(
            serde_json::to_value(problems).unwrap(),
            json!([
                {"kind": "missingLanguages"},
                {"kind": "unsafePath", "path": "c:/x", "rule": {"forbiddenChar": ":"}},
                {"kind": "schema", "pointer": "/meta", "keyword": "required",
                 "schemaPath": "/required", "message": "m"},
                {"kind": "invalidTimestamp", "field": "dateCreated", "value": "x",
                 "reason": "missingTimezone"},
            ])
        );
    }
}
