//! The Scripture Burrito 1.0.0 JSON schema, embedded and compiled once, fully offline.

use jsonschema::{Draft, Registry, Validator};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

/// Every schema is registered under this base at its path in the bundle. It is the spec's
/// own namespace, so a relative `$ref` resolves the way the spec authors meant.
const BASE: &str = "https://burrito.bible/schema/";

/// The root dispatches on `meta.category` to the source, derived or template schema.
const ROOT: &str = "metadata.schema.json";

macro_rules! bundle {
    ($($file:literal),+ $(,)?) => {
        &[$(($file, include_str!(concat!("../schema/sb-1.0.0/", $file))),)+]
    };
}

/// The whole bundle as shipped, unreachable files included, so the vendored folder and
/// this list can be compared file for file.
pub(crate) const BUNDLE: &[(&str, &str)] = bundle![
    "agencies.schema.json",
    "agency.schema.json",
    "common.schema.json",
    "confidential.schema.json",
    "copyright.schema.json",
    "copyright_constraints.schema.json",
    "derived_meta.schema.json",
    "derived_metadata.schema.json",
    "gloss/text_stories.schema.json",
    "id_authorities.schema.json",
    "identification.schema.json",
    "ingredient.schema.json",
    "ingredients.schema.json",
    "language.schema.json",
    "languages.schema.json",
    "localized_name.schema.json",
    "localized_names.schema.json",
    "meta_comments.schema.json",
    "meta_date_created.schema.json",
    "meta_default_language.schema.json",
    "meta_version.schema.json",
    "metadata.schema.json",
    "normalization.schema.json",
    "numbering_system.schema.json",
    "parascriptural/word_alignment.schema.json",
    "progress.schema.json",
    "promotion.schema.json",
    "recipe.schema.json",
    "recipe_element.schema.json",
    "recipe_section.schema.json",
    "reference_format.schema.json",
    "relationship.schema.json",
    "relationships.schema.json",
    "role.schema.json",
    "scope.schema.json",
    "scripture/audio_translation.schema.json",
    "scripture/common.schema.json",
    "scripture/embossed_braille_scripture.schema.json",
    "scripture/sign_language_video_translation.schema.json",
    "scripture/text_translation.schema.json",
    "scripture/typeset_scripture.schema.json",
    "scripture_flavor_type.schema.json",
    "software_and_user_info.schema.json",
    "source_meta.schema.json",
    "source_metadata.schema.json",
    "target_area.schema.json",
    "target_areas.schema.json",
    "template_meta.schema.json",
    "template_metadata.schema.json",
    "type.schema.json",
    "unm49.schema.json",
    "x_flavor.schema.json",
];

/// Why the embedded schema could not be compiled. The bundle is fixed at build time, so
/// this happens only if the bundle or the schema library changes; a test builds it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaLoadError {
    #[error("schema file {file} is not JSON: {message}")]
    NotJson { file: String, message: String },
    #[error("schema file {file} is not a JSON object")]
    NotAnObject { file: String },
    #[error("a schema refers to {uri}, which the bundle does not contain")]
    MissingReference { uri: String },
    #[error("the schema registry refused the bundle: {message}")]
    Registry { message: String },
    #[error("the schema did not compile: {message}")]
    Compile { message: String },
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Validator>();
};

/// Compiled on first use and shared by every thread. A failure is kept, not retried, and
/// every validation then reports it as a problem, so a broken schema fails closed.
static SB_1_0_0: LazyLock<Result<Validator, SchemaLoadError>> = LazyLock::new(build);

pub(crate) fn validator() -> Result<&'static Validator, &'static SchemaLoadError> {
    SB_1_0_0.as_ref()
}

/// Compiles the embedded schema now, if it is not compiled yet, and reports whether that
/// worked. Call it at startup to pay the compile cost (tens of milliseconds) early.
pub fn schema_ready() -> Result<(), SchemaLoadError> {
    validator().map(|_| ()).map_err(Clone::clone)
}

fn build() -> Result<Validator, SchemaLoadError> {
    compile(&format!("{BASE}{ROOT}"))
}

/// The schema's `languageTag` (`common.schema.json`), compiled from the same bundle and
/// registry as the whole schema, so it judges a tag exactly as a `languages[].tag` is judged.
static LANGUAGE_TAG: LazyLock<Result<Validator, SchemaLoadError>> =
    LazyLock::new(|| compile(&format!("{BASE}common.schema.json#/definitions/languageTag")));

/// Whether `tag` is a language tag a Scripture Burrito can hold: the schema's `languageTag`
/// (BCP 47's form, at least two characters). Door43 lists codes that are not, such as
/// `acm-x-lebarabic`, whose private part is longer than eight letters (tC4 #582). A schema
/// that did not compile answers `false`, so a broken schema fails closed.
pub fn is_language_tag(tag: &str) -> bool {
    LANGUAGE_TAG
        .as_ref()
        .is_ok_and(|validator| validator.is_valid(&Value::String(tag.to_owned())))
}

/// Compiles the schema at `uri`, a schema of the bundle or a part of one, over a registry of
/// every bundle file reachable from the root.
fn compile(uri: &str) -> Result<Validator, SchemaLoadError> {
    let bundle = load_bundle()?;
    let reachable = reachable_from(&bundle, ROOT)?;
    let resources: Vec<(String, Value)> = bundle
        .into_iter()
        .filter(|(file, _)| reachable.contains(file))
        .map(|(file, schema)| (format!("{BASE}{file}"), schema))
        .collect();
    let registry = Registry::new()
        .draft(Draft::Draft7)
        .extend(resources)
        .and_then(|builder| builder.prepare())
        .map_err(|error| SchemaLoadError::Registry {
            message: error.to_string(),
        })?;
    jsonschema::options()
        .with_draft(Draft::Draft7)
        // `meta.dateCreated` is `format: date-time`; ajv-formats asserts it, so must we.
        .should_validate_formats(true)
        // A `$ref` outside the registry fails the build instead of reaching for the network.
        .offline()
        .with_registry(&registry)
        .build(&json!({ "$ref": uri }))
        .map_err(|error| SchemaLoadError::Compile {
            message: error.to_string(),
        })
}

/// Parses every file and sets its `$id` to its absolute URI. The bundle's own ids are
/// relative, and `scripture/*.schema.json` carry ids relative to the wrong folder.
pub(crate) fn load_bundle() -> Result<BTreeMap<&'static str, Value>, SchemaLoadError> {
    BUNDLE
        .iter()
        .map(|(file, text)| {
            let mut schema = parse_lenient(text).map_err(|error| SchemaLoadError::NotJson {
                file: (*file).to_owned(),
                message: error.to_string(),
            })?;
            let object = schema
                .as_object_mut()
                .ok_or_else(|| SchemaLoadError::NotAnObject {
                    file: (*file).to_owned(),
                })?;
            object.insert("$id".to_owned(), Value::String(format!("{BASE}{file}")));
            Ok((*file, schema))
        })
        .collect()
}

/// `scripture_flavor_type.schema.json` has a trailing comma, so a file that does not parse
/// is parsed again with trailing commas removed.
fn parse_lenient(text: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(text)
        .or_else(|first| serde_json::from_str(&strip_trailing_commas(text)).map_err(|_| first))
}

/// Removes every `,` that, outside a string, is followed only by whitespace and then `}` or
/// `]`. Only ASCII commas are removed, so the result is still valid UTF-8.
pub(crate) fn strip_trailing_commas(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
        } else if c == ',' {
            let next = bytes[index + 1..].iter().find(|b| !b.is_ascii_whitespace());
            if matches!(next, Some(b'}' | b']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// The files reachable from `root` through `$ref`. Registering an unreachable file would
/// be harmful: the registry resolves every reference of every file up front, and the
/// bundle's unused `scripture_flavor_type.schema.json` refers to files that do not exist.
pub(crate) fn reachable_from(
    bundle: &BTreeMap<&'static str, Value>,
    root: &str,
) -> Result<BTreeSet<&'static str>, SchemaLoadError> {
    let mut seen = BTreeSet::new();
    let mut pending = vec![root.to_owned()];
    while let Some(file) = pending.pop() {
        let Some((&name, schema)) = bundle.get_key_value(file.as_str()) else {
            return Err(SchemaLoadError::MissingReference {
                uri: format!("{BASE}{file}"),
            });
        };
        if !seen.insert(name) {
            continue;
        }
        let mut refs = Vec::new();
        collect_refs(schema, &mut refs);
        pending.extend(refs.into_iter().filter_map(|r| resolve(name, r)));
    }
    Ok(seen)
}

fn collect_refs<'v>(value: &'v Value, out: &mut Vec<&'v str>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                match (key.as_str(), child) {
                    ("$ref", Value::String(target)) => out.push(target),
                    _ => collect_refs(child, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_refs(item, out)),
        _ => {}
    }
}

/// Resolves a relative file reference against the referring file's folder, as RFC 3986
/// merges paths. Fragment-only references stay inside the file and resolve to nothing.
fn resolve(from: &str, reference: &str) -> Option<String> {
    let target = reference.split('#').next().unwrap_or_default();
    if target.is_empty() {
        return None;
    }
    if let Some(inside) = target.strip_prefix(BASE) {
        return Some(inside.to_owned());
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for segment in target.split('/') {
        match segment {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trailing_commas_are_removed_outside_strings_only() {
        let text = "{\"a\": [1, 2,\n ], \"b\": \"x,}\", \"é\": {\"c\": 1,}, }";
        let stripped = strip_trailing_commas(text);
        assert_eq!(
            stripped,
            "{\"a\": [1, 2\n ], \"b\": \"x,}\", \"é\": {\"c\": 1} }"
        );
        assert!(serde_json::from_str::<Value>(&stripped).is_ok());
    }

    #[test]
    fn a_language_tag_is_the_schemas_language_tag() {
        // Door43 codes: a region subtag, a private part of at most eight letters, a private
        // tag alone; and the forms the schema refuses (#582): a private part of nine letters,
        // an empty first subtag, one letter, an empty private part.
        for tag in ["en", "es-419", "x-ug", "aac-x-siraul", "hbo", "zh-min-nan"] {
            assert!(is_language_tag(tag), "{tag} is a tag");
        }
        for tag in ["acm-x-lebarabic", "-x-", "e", "iba-x-", "", "en--us", "en us"] {
            assert!(!is_language_tag(tag), "{tag} is not a tag");
        }
    }

    #[test]
    fn references_resolve_like_uris() {
        assert_eq!(
            resolve(
                "scripture/text_translation.schema.json",
                "../common.schema.json#/x"
            ),
            Some("common.schema.json".to_owned())
        );
        assert_eq!(
            resolve("type.schema.json", "gloss/text_stories.schema.json"),
            Some("gloss/text_stories.schema.json".to_owned())
        );
        assert_eq!(resolve("common.schema.json", "#/definitions/path"), None);
    }

    #[test]
    fn the_root_reaches_46_files_and_leaves_out_the_dead_ones() {
        let bundle = load_bundle().unwrap();
        let reachable = reachable_from(&bundle, ROOT).unwrap();
        assert_eq!(reachable.len(), 46);
        let left_out: Vec<_> = bundle
            .keys()
            .filter(|file| !reachable.contains(*file))
            .copied()
            .collect();
        assert_eq!(
            left_out,
            [
                "parascriptural/word_alignment.schema.json",
                "recipe_element.schema.json",
                "recipe_section.schema.json",
                "reference_format.schema.json",
                "scripture/common.schema.json",
                "scripture_flavor_type.schema.json",
            ]
        );
    }

    #[test]
    fn the_embedded_list_is_the_whole_vendored_folder() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("schema/sb-1.0.0");
        let mut on_disk = Vec::new();
        let mut pending = vec![dir.clone()];
        while let Some(folder) = pending.pop() {
            for entry in std::fs::read_dir(folder).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let relative = path.strip_prefix(&dir).unwrap().to_str().unwrap();
                    on_disk.push(relative.replace('\\', "/"));
                }
            }
        }
        on_disk.sort();
        let embedded: Vec<&str> = BUNDLE.iter().map(|(file, _)| *file).collect();
        assert_eq!(on_disk, embedded);
    }

    #[test]
    fn every_bundle_file_parses_after_repair() {
        assert_eq!(load_bundle().unwrap().len(), BUNDLE.len());
        let broken = BUNDLE
            .iter()
            .filter(|(_, text)| serde_json::from_str::<Value>(text).is_err())
            .map(|(file, _)| *file)
            .collect::<Vec<_>>();
        assert_eq!(broken, ["scripture_flavor_type.schema.json"]);
    }

    #[test]
    fn the_embedded_schema_compiles_on_another_thread() {
        let ready = std::thread::spawn(schema_ready).join().unwrap();
        assert_eq!(ready, Ok(()));
    }
}
