//! Listings from Door43 in which one entry this client cannot use is set aside, so a single
//! odd repository, tag or release never hides the rest.

use serde::de::DeserializeOwned;
use serde_json::Value;

/// What a listing call returns: the entries that parsed, and the ones that did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing<T> {
    pub items: Vec<T>,
    pub skipped: Vec<Skipped>,
}

/// An entry Door43 listed that this client could not read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    /// Which entry, as far as it could be told: such as `someone/notes v1` or a tag name.
    pub entry: String,
    pub reason: String,
}

/// The longest `Skipped::entry`; the text comes from the server.
const MAX_LABEL_CHARS: usize = 120;

impl<T> Listing<T> {
    /// Read each JSON value as `R`, then `parse` it. `label` names a value for `skipped`.
    pub(crate) fn from_values<R: DeserializeOwned>(
        route: &str,
        values: Vec<Value>,
        label: impl Fn(&Value) -> String,
        parse: impl Fn(R) -> Result<T, String>,
    ) -> Self {
        let mut items = Vec::with_capacity(values.len());
        let mut skipped = Vec::new();
        for value in values {
            let entry: String = label(&value).chars().take(MAX_LABEL_CHARS).collect();
            let parsed = serde_json::from_value::<R>(value)
                .map_err(|e| format!("not the expected shape: {e}"))
                .and_then(&parse);
            match parsed {
                Ok(item) => items.push(item),
                Err(reason) => {
                    tracing::warn!(%route, %entry, %reason, "door43 listing entry skipped");
                    skipped.push(Skipped { entry, reason });
                }
            }
        }
        Self { items, skipped }
    }
}

/// The string at `field`, or `?`.
pub(crate) fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("?")
}
