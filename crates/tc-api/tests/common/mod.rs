//! Helpers the integration tests share: the interface catalogs, placeholders, and the
//! variant names serde knows for an enum.
#![allow(dead_code)]

pub mod samples;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tc_api::ids::LOCALE_IDS;

pub fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn ui_assets() -> PathBuf {
    crate_dir().join("../tc-ui/assets")
}

pub type Catalog = BTreeMap<String, String>;

fn read_catalog(path: PathBuf) -> Catalog {
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()));
    let value: Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is JSON: {e}", path.display()));
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{} is an object", path.display()));
    object
        .iter()
        .map(|(key, text)| {
            let text = text
                .as_str()
                .unwrap_or_else(|| panic!("{}: {key} is not a string", path.display()));
            (key.clone(), text.to_owned())
        })
        .collect()
}

/// tC4's catalogs, vendored byte for byte, by locale id.
pub fn vendored() -> BTreeMap<&'static str, Catalog> {
    LOCALE_IDS
        .iter()
        .map(|id| {
            (
                *id,
                read_catalog(ui_assets().join(format!("i18n/{id}.json"))),
            )
        })
        .collect()
}

pub fn rebuild_dir() -> PathBuf {
    ui_assets().join("i18n-rebuild")
}

/// The rebuild's own keys, translated, by locale id.
pub fn rebuild() -> BTreeMap<&'static str, Catalog> {
    LOCALE_IDS
        .iter()
        .map(|id| (*id, read_catalog(rebuild_dir().join(format!("{id}.json")))))
        .collect()
}

/// The rebuild keys each language still shows in English. English has none.
pub fn pending() -> BTreeMap<&'static str, BTreeSet<String>> {
    LOCALE_IDS
        .iter()
        .filter(|id| **id != "en")
        .map(|id| {
            let path = rebuild_dir().join(format!("{id}.pending.json"));
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} is readable: {e}", path.display()));
            let keys: Vec<String> = serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("{} is a list of keys: {e}", path.display()));
            let set: BTreeSet<String> = keys.iter().cloned().collect();
            assert_eq!(set.len(), keys.len(), "{} repeats a key", path.display());
            (*id, set)
        })
        .collect()
}

/// The sentence a key shows in a language, as the UI resolves it: tC4's catalog, then the
/// rebuild's translation, then, while it is pending, the rebuild's English.
pub struct Resolver {
    vendored: BTreeMap<&'static str, Catalog>,
    rebuild: BTreeMap<&'static str, Catalog>,
    pending: BTreeMap<&'static str, BTreeSet<String>>,
}

pub enum Resolved<'a> {
    Translated(&'a str),
    /// Pending: shown in English.
    English(&'a str),
    Missing,
}

impl Resolver {
    pub fn load() -> Self {
        Self {
            vendored: vendored(),
            rebuild: rebuild(),
            pending: pending(),
        }
    }

    pub fn resolve(&self, locale: &str, key: &str) -> Resolved<'_> {
        if let Some(text) = self.vendored.get(locale).and_then(|c| c.get(key)) {
            return Resolved::Translated(text);
        }
        if let Some(text) = self.rebuild.get(locale).and_then(|c| c.get(key)) {
            return Resolved::Translated(text);
        }
        let is_pending = self.pending.get(locale).is_some_and(|p| p.contains(key));
        match self.rebuild.get("en").and_then(|c| c.get(key)) {
            Some(text) if is_pending => Resolved::English(text),
            _ => Resolved::Missing,
        }
    }

    pub fn english(&self, key: &str) -> Option<&str> {
        match self.resolve("en", key) {
            Resolved::Translated(text) => Some(text),
            Resolved::English(_) | Resolved::Missing => None,
        }
    }
}

/// The `{name}` placeholders of a message, as tC4's `t()` replaces them.
pub fn placeholders(message: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = message;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        if len > 0 && after[len..].starts_with('}') {
            found.insert(after[..len].to_owned());
            rest = &after[len + 1..];
        } else {
            rest = after;
        }
    }
    found
}

/// Every variant name serde accepts for `T`, read from the error serde gives for an unknown
/// one. It comes from the derive itself, so a variant added to the enum is listed here
/// without anyone listing it by hand.
pub fn variant_names<T: DeserializeOwned>() -> BTreeSet<String> {
    const UNKNOWN: &str = "zzNotAVariant";
    let tries = [json!({ "type": UNKNOWN }), json!(UNKNOWN)];
    for value in tries {
        let Err(error) = serde_json::from_value::<T>(value) else {
            panic!("{UNKNOWN} was accepted as a variant");
        };
        let message = error.to_string();
        if !message.contains("unknown variant") {
            continue;
        }
        // serde writes "expected `a`", "expected `a` or `b`", or "expected one of `a`, `b`".
        let Some((_, list)) = message.split_once("expected ") else {
            continue;
        };
        let list = list.strip_prefix("one of ").unwrap_or(list);
        return list
            .split(", ")
            .flat_map(|part| part.split(" or "))
            .map(|name| name.trim().trim_matches('`').to_owned())
            .collect();
    }
    panic!("serde named no variants for {}", std::any::type_name::<T>());
}

/// The variant name of a serialized enum value.
pub fn variant_of(value: &Value) -> String {
    match value {
        Value::String(name) => name.clone(),
        Value::Object(map) => map
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("not a tagged enum: {value}"))
            .to_owned(),
        other => panic!("not an enum: {other}"),
    }
}

/// Fails unless `samples` hold every variant of `T` at least once.
pub fn assert_every_variant<T: serde::Serialize + DeserializeOwned>(samples: &[T]) {
    let have: BTreeSet<String> = samples
        .iter()
        .map(|s| variant_of(&serde_json::to_value(s).expect("serializes")))
        .collect();
    let all = variant_names::<T>();
    let missing: Vec<_> = all.difference(&have).collect();
    assert!(
        missing.is_empty(),
        "{} has variants with no sample: {missing:?}",
        std::any::type_name::<T>()
    );
}
