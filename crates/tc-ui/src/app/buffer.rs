//! The draft buffer in the browser's storage (APP-DESIGN 6.6, AD21): one item per field, its
//! key [`BUFFER_ITEM_PREFIX`] and the field's [`BufferKey`] as JSON, its value one
//! [`BufferEntry`].
//!
//! Every function here works on a [`Store`], which is `localStorage` in the browser and a map
//! in tests, and every one of its calls may fail: storage can be blocked, full or cleared. The
//! rules that keep text:
//!
//! - An item this build cannot read (another build's, a damaged one, a key not in the exact
//!   form serde writes) is skipped and never removed or rewritten.
//! - A rewrite keeps the keys of the stored entry that this build does not know (3.10).
//! - An entry is removed after a save only while its text is still the text that save stored:
//!   another tab, or `boot.js` on a keystroke, may have written newer text since.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use tc_api::buffer::{BUFFER_ITEM_PREFIX, BufferEntry, BufferKey, BufferLine};
use tc_api::ids::{ProjectId, ProjectRef};
use unicode_normalization::UnicodeNormalization;

/// The browser refused a write, or an entry could not be written as JSON.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("the browser's storage refused: {0}")]
    Refused(String),
    #[error("a draft could not be written as JSON: {0}")]
    Encode(String),
}

/// The browser's storage, or a map in tests.
pub trait Store {
    /// Every item's key. Empty when the storage cannot be read.
    fn keys(&self) -> Vec<String>;
    fn get(&self, item: &str) -> Option<String>;
    fn set(&mut self, item: &str, value: &str) -> Result<(), StoreError>;
    fn remove(&mut self, item: &str) -> Result<(), StoreError>;
}

/// A [`Store`] in memory, for tests and for the in-process page of tc-app's tests.
#[derive(Debug, Clone, Default)]
pub struct MemStore {
    pub items: BTreeMap<String, String>,
    /// While true, every `set` fails, as a full or blocked storage does.
    pub fail_writes: bool,
    /// How many times `set` was called, failed calls included.
    pub sets: usize,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Store for MemStore {
    fn keys(&self) -> Vec<String> {
        self.items.keys().cloned().collect()
    }

    fn get(&self, item: &str) -> Option<String> {
        self.items.get(item).cloned()
    }

    fn set(&mut self, item: &str, value: &str) -> Result<(), StoreError> {
        self.sets += 1;
        if self.fail_writes {
            return Err(StoreError::Refused("the storage is full".to_owned()));
        }
        self.items.insert(item.to_owned(), value.to_owned());
        Ok(())
    }

    fn remove(&mut self, item: &str) -> Result<(), StoreError> {
        self.items.remove(item);
        Ok(())
    }
}

/// The one NFC of the UI, the same crate and version as the server's (`unicode-normalization`
/// in the workspace), so a text the server stored compares equal here. The browser's own
/// `normalize` follows its ICU's Unicode version, which can differ for new characters, and a
/// field that never compared equal would save again every two seconds.
pub fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// Whether two texts are one text in NFC.
pub fn same_text(a: &str, b: &str) -> bool {
    a == b || nfc(a) == nfc(b)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String, StoreError> {
    serde_json::to_string(value).map_err(|error| StoreError::Encode(error.to_string()))
}

/// The item key of a field.
pub fn item_key(key: &BufferKey) -> Result<String, StoreError> {
    Ok(format!("{BUFFER_ITEM_PREFIX}{}", encode(key)?))
}

/// The field an item key names. `None` for an item that is not a buffer item, does not parse,
/// or is not in the exact form serde writes: such an item is left as it is.
pub fn parse_item_key(item: &str) -> Option<BufferKey> {
    let text = item.strip_prefix(BUFFER_ITEM_PREFIX)?;
    let key: BufferKey = serde_json::from_str(text).ok()?;
    (encode(&key).ok()? == text).then_some(key)
}

/// An item's value, if this build can read it.
pub fn parse_entry(text: &str) -> Option<BufferEntry> {
    serde_json::from_str(text).ok()
}

/// The entry of a field, if there is one this build can read.
pub fn read(store: &dyn Store, key: &BufferKey) -> Option<BufferEntry> {
    parse_entry(&store.get(&item_key(key).ok()?)?)
}

/// `entry` as the JSON text to store, with every key of `old` that this build does not know.
fn merged_text(old: Option<&str>, entry: &BufferEntry) -> Result<String, StoreError> {
    let Value::Object(fresh) =
        serde_json::to_value(entry).map_err(|error| StoreError::Encode(error.to_string()))?
    else {
        return Err(StoreError::Encode("an entry is not an object".to_owned()));
    };
    let mut merged: Map<String, Value> = match old.map(serde_json::from_str::<Value>) {
        Some(Ok(Value::Object(old))) => old,
        _ => Map::new(),
    };
    for (name, value) in fresh {
        merged.insert(name, value);
    }
    encode(&Value::Object(merged))
}

/// Writes `entry` over the field's item, keeping every key of the stored entry this build does
/// not know (3.10). Returns the JSON text written, for the mirror's line.
pub fn write(
    store: &mut dyn Store,
    key: &BufferKey,
    entry: &BufferEntry,
) -> Result<String, StoreError> {
    let item = item_key(key)?;
    let text = merged_text(store.get(&item).as_deref(), entry)?;
    store.set(&item, &text)?;
    Ok(text)
}

/// What [`remove`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Removal {
    Removed,
    /// The item holds other text than the save stored, or text this build cannot read: it
    /// stays.
    Kept,
    Absent,
}

/// Removes the field's item when its text is `if_text` in NFC, or whatever it holds when
/// `if_text` is `None` (Cancel of a section card discards it on purpose).
pub fn remove(
    store: &mut dyn Store,
    key: &BufferKey,
    if_text: Option<&str>,
) -> Result<Removal, StoreError> {
    let item = item_key(key)?;
    let Some(stored) = store.get(&item) else {
        return Ok(Removal::Absent);
    };
    if let Some(expected) = if_text {
        match parse_entry(&stored) {
            Some(entry) if same_text(&entry.text, expected) => {}
            _ => return Ok(Removal::Kept),
        }
    }
    store.remove(&item)?;
    Ok(Removal::Removed)
}

/// Every entry this build can read, by key. Items it cannot read are skipped.
pub fn entries(store: &dyn Store) -> Vec<(BufferKey, BufferEntry)> {
    store
        .keys()
        .iter()
        .filter_map(|item| {
            let key = parse_item_key(item)?;
            let entry = parse_entry(&store.get(item)?)?;
            Some((key, entry))
        })
        .collect()
}

/// The entries of one folder with one project id (A-9): a folder made again under the same
/// name has another id, and its records never meet the old folder's text.
pub fn entries_of(
    store: &dyn Store,
    project: &ProjectRef,
    id: &ProjectId,
) -> Vec<(BufferKey, BufferEntry)> {
    entries(store)
        .into_iter()
        .filter(|(key, _)| key.project == *project && key.id == *id)
        .collect()
}

/// The mirror's line for a change, from the entry's JSON text as it was stored (with the keys
/// this build does not know), byte for byte the line `boot.js` builds.
pub fn changed_line(key: &BufferKey, entry_json: &str) -> Result<String, StoreError> {
    Ok(format!(
        r#"{{"type":"changed","value":{{"key":{},"entry":{entry_json}}}}}"#,
        encode(key)?
    ))
}

/// The mirror's line for a field that became clean.
pub fn removed_line(key: &BufferKey) -> Result<String, StoreError> {
    encode(&BufferLine::Removed { key: key.clone() })
}

/// The page's entries merged with what the shell's mirror gave (`takeBuffered`): the newest
/// `at` per field, the page's own on a tie. A line that does not parse (a torn last line) and
/// a removal add nothing.
pub fn merge(
    local: Vec<(BufferKey, BufferEntry)>,
    mirror: &[String],
) -> Vec<(BufferKey, BufferEntry)> {
    let mut newest: BTreeMap<BufferKey, BufferEntry> = local.into_iter().collect();
    for line in mirror {
        let Ok(BufferLine::Changed { key, entry }) = serde_json::from_str::<BufferLine>(line)
        else {
            continue;
        };
        match newest.get(&key) {
            Some(kept) if kept.at >= entry.at => {}
            _ => {
                newest.insert(key, entry);
            }
        }
    }
    newest.into_iter().collect()
}

/// The copy of a read-only project takes its folder's entries (A-10, 6.4): every item of
/// `from` that this build can read is written under `to`, as it is, and only when all are written are the old items
/// removed, so a failure leaves both and never neither. Returns how many moved.
pub fn rekey(
    store: &mut dyn Store,
    from: (&ProjectRef, &ProjectId),
    to: (&ProjectRef, &ProjectId),
) -> Result<usize, StoreError> {
    let mut moved = Vec::new();
    for item in store.keys() {
        let Some(key) = parse_item_key(&item) else {
            continue;
        };
        if key.project != *from.0 || key.id != *from.1 {
            continue;
        }
        let Some(text) = store.get(&item).filter(|text| parse_entry(text).is_some()) else {
            continue;
        };
        let new_key = BufferKey {
            project: to.0.clone(),
            id: to.1.clone(),
            field: key.field,
        };
        store.set(&item_key(&new_key)?, &text)?;
        moved.push(item);
    }
    for item in &moved {
        store.remove(item)?;
    }
    Ok(moved.len())
}
