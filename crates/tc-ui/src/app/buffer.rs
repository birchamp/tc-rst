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
//! - An entry of another server run is never written over or removed by this run: it waits
//!   for the offer at its project's next open (AD4).
//! - A rewrite keeps the keys of the stored entry that this build does not know (3.10).
//! - An entry is removed after a save only while its text is still the text that save stored:
//!   another tab, or `boot.js` on a keystroke, may have written newer text since.

use std::collections::BTreeMap;

use serde_json::{Map, Value};
use tc_api::buffer::{BUFFER_ITEM_PREFIX, BufferEntry, BufferKey, BufferLine};
use tc_api::ids::{ProjectId, ProjectRef, ServerInstance, WallMs};
use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

/// The browser refused a write, or an entry could not be written as JSON.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StoreError {
    #[error("the browser's storage refused: {0}")]
    Refused(String),
    #[error("a draft could not be written as JSON: {0}")]
    Encode(String),
    /// The field's item holds an entry of another server run, or one this build cannot read.
    /// It waits for the offer at its project's next open and is never written over.
    #[error("the field's buffer holds another run's text")]
    Foreign,
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

/// Whether two texts are one text in NFC. Runs on every keystroke: two texts that are already
/// in NFC (the quick check, one pass and no allocation, says so for nearly all text) and differ
/// are different, so only text that may not be in NFC is normalized.
pub fn same_text(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let normalized = |text: &str| is_nfc_quick(text.chars()) == IsNormalized::Yes;
    if normalized(a) && normalized(b) {
        return false;
    }
    nfc(a) == nfc(b)
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

/// Whether a stored item may be replaced by an entry of `instance`: it is an entry this build
/// reads, of the same run.
fn is_own(stored: &str, instance: ServerInstance) -> bool {
    parse_entry(stored).is_some_and(|entry| entry.instance == instance)
}

/// Writes `entry` over the field's item, keeping every key of the stored entry this build does
/// not know (3.10). Returns the JSON text written, for the mirror's line. An item of another
/// run, or one this build cannot read, is not written over ([`StoreError::Foreign`]).
pub fn write(
    store: &mut dyn Store,
    key: &BufferKey,
    entry: &BufferEntry,
) -> Result<String, StoreError> {
    let item = item_key(key)?;
    let old = store.get(&item);
    if old
        .as_deref()
        .is_some_and(|old| !is_own(old, entry.instance))
    {
        return Err(StoreError::Foreign);
    }
    let text = merged_text(old.as_deref(), entry)?;
    store.set(&item, &text)?;
    Ok(text)
}

/// What `boot.js`'s capture listener does on a keystroke, before the WebAssembly sees it
/// (6.6, A-15), in Rust for the tests that play the page: the field's entry gets the new text
/// and time, or, when it has none, the editor's `template` becomes its entry. Returns the
/// mirror's line, which goes to the shell even when the storage refused the write. An entry of
/// another run or one this build cannot read is left alone and gives no line.
pub fn input_to_entry(
    store: &mut dyn Store,
    key: &BufferKey,
    text: &str,
    at: WallMs,
    template: &BufferEntry,
) -> Option<String> {
    let item = item_key(key).ok()?;
    let mut value = match store.get(&item) {
        Some(stored) => {
            if !is_own(&stored, template.instance) {
                return None;
            }
            serde_json::from_str::<Value>(&stored).ok()?
        }
        None => serde_json::to_value(template).ok()?,
    };
    let object = value.as_object_mut()?;
    object.insert("text".to_owned(), Value::String(text.to_owned()));
    object.insert("at".to_owned(), Value::from(at.0));
    let entry_json = encode(&value).ok()?;
    let line = changed_line(key, &entry_json).ok();
    // A refused write still sends the line: the shell's mirror then holds the keystroke.
    let _ = store.set(&item, &entry_json);
    line
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

/// Removes the field's item when it is an entry of `instance` whose text is `if_text` in NFC,
/// or any text for `None` (Cancel of a section card discards it on purpose). An entry of
/// another run, or one this build cannot read, is kept.
pub fn remove(
    store: &mut dyn Store,
    key: &BufferKey,
    if_text: Option<&str>,
    instance: ServerInstance,
) -> Result<Removal, StoreError> {
    let item = item_key(key)?;
    let Some(stored) = store.get(&item) else {
        return Ok(Removal::Absent);
    };
    match parse_entry(&stored) {
        Some(entry)
            if entry.instance == instance
                && if_text.is_none_or(|expected| same_text(&entry.text, expected)) => {}
        _ => return Ok(Removal::Kept),
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
/// `at` per field and server run, the page's own on a tie. A line that does not parse (a torn
/// last line) and a removal add nothing. Two runs' entries of one field are both kept: one
/// typed before a restart and one typed after it, behind a reused port, before the offer
/// settled the first, are two texts, and the offer shows both.
pub fn merge(
    local: Vec<(BufferKey, BufferEntry)>,
    mirror: &[String],
) -> Vec<(BufferKey, BufferEntry)> {
    let mut newest: BTreeMap<(BufferKey, ServerInstance), BufferEntry> = local
        .into_iter()
        .map(|(key, entry)| ((key, entry.instance), entry))
        .collect();
    for line in mirror {
        let Ok(BufferLine::Changed { key, entry }) = serde_json::from_str::<BufferLine>(line)
        else {
            continue;
        };
        let slot = (key, entry.instance);
        match newest.get(&slot) {
            Some(kept) if kept.at >= entry.at => {}
            _ => {
                newest.insert(slot, entry);
            }
        }
    }
    newest
        .into_iter()
        .map(|((key, _), entry)| (key, entry))
        .collect()
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
