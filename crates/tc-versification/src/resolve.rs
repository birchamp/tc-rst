//! Which scheme a project is in: tC4's resolution ladder (`resolveProjectScheme` in
//! `src/data/versification.ts`, BURRITO-SPEC §4.3).
//!
//! The burrito does not record the scheme's name, and the platform discards the name it
//! was given at creation, so a project's frame is established from the journal's `vrs`
//! register: the recorded name when the caller has that scheme, else a fingerprint of
//! the `vrs.json` text against the schemes the caller has, else unknown.

use serde_json::{Map, Value};

/// The name the store records when it has no scheme name to give, for a project tC4 did
/// not create. It is never a scheme name.
pub const UNRECORDED_SCHEME: &str = "unrecorded";

/// The project's versification register, as the journal folds it: the scheme name
/// recorded at creation and the exact `vrs.json` text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VrsRegister<'a> {
    pub name: &'a str,
    pub json: &'a str,
}

/// How a project's scheme was established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchemeResolution {
    /// The name recorded at creation, which names a scheme the caller has.
    Recorded(String),
    /// No usable recorded name, but the register's `vrs.json` means the same as this
    /// scheme's document.
    Fingerprint(String),
    /// Neither. A caller does not map an unknown frame, and never reads it as `eng`: most
    /// published burritos carry no versification at all, so absence says nothing.
    Unknown,
}

impl SchemeResolution {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Recorded(name) | Self::Fingerprint(name) => Some(name),
            Self::Unknown => None,
        }
    }
}

/// tC4's `isSchemeName`: any non-empty name except the store's placeholder. The set of
/// schemes is open (the platform serves it), so membership of the six is not the test.
pub fn is_scheme_name(name: &str) -> bool {
    !name.is_empty() && name != UNRECORDED_SCHEME
}

/// Resolves a project's scheme. `known` holds the scheme documents the caller can map
/// with, by name; candidates are tried in name order, so the answer is deterministic.
pub fn resolve_scheme(
    register: Option<VrsRegister<'_>>,
    known: &[(&str, &str)],
) -> SchemeResolution {
    let Some(register) = register else {
        return SchemeResolution::Unknown;
    };
    // A recorded name is trusted only when it names a scheme the caller has: a name no
    // caller can map with is not an answer (tC4 review finding R-E33-1).
    if is_scheme_name(register.name) && known.iter().any(|(name, _)| *name == register.name) {
        return SchemeResolution::Recorded(register.name.to_owned());
    }
    let Some(wanted) = canonical_scheme(register.json) else {
        return SchemeResolution::Unknown;
    };
    let mut candidates: Vec<&(&str, &str)> = known.iter().collect();
    candidates.sort_by_key(|(name, _)| *name);
    candidates
        .into_iter()
        .find(|(_, json)| canonical_scheme(json).as_ref() == Some(&wanted))
        .map_or(SchemeResolution::Unknown, |(name, _)| {
            SchemeResolution::Fingerprint((*name).to_owned())
        })
}

/// A scheme document's meaning, for comparing two documents: tC4's `canonicalizeScheme`.
/// Keys are sorted at every depth, `mappedVerses` values take the array form, and numbers
/// compare by value, as JavaScript reads them. `None` when the text is not JSON.
///
/// Real burritos re-serialize the same scheme with other key orders and whitespace, so
/// bytes identify nothing.
pub fn canonical_scheme(json: &str) -> Option<String> {
    let mut doc: Value = serde_json::from_str(json).ok()?;
    if let Some(Value::Object(table)) = doc.get_mut("mappedVerses") {
        for value in table.values_mut() {
            if !value.is_array() {
                *value = Value::Array(vec![value.take()]);
            }
        }
    }
    Some(canonical(doc).to_string())
}

fn canonical(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(k, v)| (k, canonical(v)))
                    .collect::<Map<_, _>>(),
            )
        }
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        // JavaScript has one number type: `1` and `1.0` are the same value.
        Value::Number(n) => n.as_f64().map_or(Value::Null, Value::from),
        other => other,
    }
}
