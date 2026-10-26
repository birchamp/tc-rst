//! Lexicons as one JSON file each, read whole into memory (`bundle/en/README.md`,
//! "Repackaged as one JSON file per lexicon"; owner ruling 2026-10-15).
//!
//! A lexicon archive holds thousands of tiny files, which are slow to install, copy and
//! scan. [`repackage_lexicon`] turns a verified archive into one `tc-lexicon` file. It runs
//! when the app is packaged ([`crate::install_lexicon_zip`]) and whenever a lexicon is
//! installed later ([`crate::install_dcs`]), so there is one format and one code path; an
//! installed lexicon is that one file, [`LEXICON_FILE`]. [`Lexicons::open`] reads a
//! project's two lexicons once per session, and every lookup after that is in memory.
//!
//! The file is UTF-8 JSON without spaces, ending in a newline, its keys in this order:
//!
//! - `format`: `"tc-lexicon"`; `version`: 1.
//! - `source`: `repo` (`owner/name`), `commit` and `archiveSha256`, the SHA-256 of the
//!   zip the files came from.
//! - `metadata`: the archive's `metadata.json`, unchanged as a value. The `uW` lexicons'
//!   metadata vouches for nothing (an ingredient table that lists only `README.md`, and in
//!   `en_uhl` the invalid timestamp `2025-11-24T55:20:15.878Z`); it is kept for its license
//!   and credits, never checked.
//! - `entries`: every `ingredients/content/<n>.json`, unchanged as a value, keyed by `<n>`
//!   in ascending numeric order. `<n>` is a decimal number without leading zeros that fits
//!   in 32 bits. An entry is an object; its `brief` and `long`, when present, are text, and
//!   its other fields are kept.
//! - `index`: `ingredients/index.json` unchanged, only when the archive has one.
//! - `files`: every other file, its text byte for byte, keyed by its path below the
//!   archive's top folder, so the notes and attribution the publisher ships travel with the
//!   entries (as tC4 keeps every source file of a lexicon, #740).
//!
//! The same input always gives the same bytes. A JSON file with a key twice has no single
//! value, so it is refused, as is a symbolic link, a file that is not UTF-8 text and an
//! archive without entries. A reader refuses a format it does not know and a version it
//! does not read, in words: the app shows the lexicon as unavailable rather than guess.

#![expect(
    clippy::result_large_err,
    reason = "a LexiconUnavailable is made at most once per lexicon and session and carries the pin it names, so its size costs nothing; boxing it would only make every match harder to read"
)]

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};

use serde::Deserialize;
use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};
use tc_door43::{FileMode, GitOid, RepoRef, UnpackedFile, VerifiedArchive};

use crate::error::{LexiconError, LexiconUnavailable};
use crate::pin::{ResourcePin, Sha256};
use crate::sets::Testaments;
use crate::store::{Origin, Packaging, ResourceStore};
use crate::strongs::{LexiconId, StrongsNumber, StrongsPart, strongs_parts};

pub const LEXICON_FORMAT: &str = "tc-lexicon";
/// The newest version this app reads and the one it writes.
pub const LEXICON_VERSION: u64 = 1;
/// An installed lexicon's one content file.
pub const LEXICON_FILE: &str = "lexicon.json";
/// tC4 `resourceArchive.mjs:20` repackages an archive whose metadata declares this flavor.
pub(crate) const LEXICON_FLAVOR: &str = "x-lexicon";

const METADATA: &str = "metadata.json";
const INDEX: &str = "ingredients/index.json";
const CONTENT: &str = "ingredients/content/";

/// Where a repackaged lexicon came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconSource {
    pub repo: RepoRef,
    pub commit: GitOid,
    /// Of the zip the archive's files came from.
    pub archive_sha256: Sha256,
}

/// Turns a verified lexicon archive of `repo` into a `tc-lexicon` file (see the module
/// documentation). `archive_sha256` is the digest of the zip `archive` was unpacked from.
pub fn repackage_lexicon(
    repo: &RepoRef,
    archive: &VerifiedArchive,
    archive_sha256: Sha256,
) -> Result<Vec<u8>, LexiconError> {
    let mut metadata = None;
    let mut index = None;
    let mut entries = BTreeMap::new();
    let mut files = BTreeMap::new();
    for file in &archive.files {
        if file.mode == FileMode::Symlink {
            return Err(LexiconError::Symlink {
                path: file.path.clone(),
            });
        }
        let path = file.path.as_str();
        if path == METADATA {
            metadata = Some(json_of(file)?);
        } else if path == INDEX {
            index = Some(json_of(file)?);
        } else if let Some(number) = entry_number(path) {
            let entry = json_of(file)?;
            if let Some(problem) = entry_problem(&entry) {
                return Err(LexiconError::BadEntry {
                    path: path.to_owned(),
                    problem,
                });
            }
            entries.insert(number, entry);
        } else {
            let text = std::str::from_utf8(&file.bytes).map_err(|_| LexiconError::NotText {
                path: path.to_owned(),
            })?;
            files.insert(path, text);
        }
    }
    let metadata = metadata.ok_or(LexiconError::NoMetadata)?;
    let flavor = flavor_name(&metadata);
    if flavor != Some(LEXICON_FLAVOR) {
        return Err(LexiconError::NotALexicon {
            found: flavor.map(str::to_owned),
        });
    }
    if entries.is_empty() {
        return Err(LexiconError::NoEntries);
    }

    let mut source = Map::new();
    source.insert("repo".into(), repo.to_string().into());
    source.insert("commit".into(), archive.commit.to_hex().into());
    source.insert("archiveSha256".into(), archive_sha256.to_hex().into());
    let mut out = Map::new();
    out.insert("format".into(), LEXICON_FORMAT.into());
    out.insert("version".into(), LEXICON_VERSION.into());
    out.insert("source".into(), Value::Object(source));
    out.insert("metadata".into(), metadata);
    out.insert(
        "entries".into(),
        Value::Object(
            entries
                .into_iter()
                .map(|(number, entry)| (number.to_string(), entry))
                .collect(),
        ),
    );
    if let Some(index) = index {
        out.insert("index".into(), index);
    }
    out.insert(
        "files".into(),
        Value::Object(
            files
                .into_iter()
                .map(|(path, text)| (path.to_owned(), text.into()))
                .collect(),
        ),
    );
    let mut bytes = serde_json::to_vec(&Value::Object(out))
        .map_err(|e| LexiconError::Malformed(e.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Whether an archive's metadata declares the lexicon flavor.
pub(crate) fn is_lexicon_archive(files: &[UnpackedFile]) -> bool {
    files
        .iter()
        .find(|f| f.path == METADATA)
        .and_then(|f| json_of(f).ok())
        .is_some_and(|metadata| flavor_name(&metadata) == Some(LEXICON_FLAVOR))
}

/// tC4 `resourceArchive.mjs:20`: `meta.type?.flavorType?.flavor?.name`.
fn flavor_name(metadata: &Value) -> Option<&str> {
    metadata
        .pointer("/type/flavorType/flavor/name")
        .and_then(Value::as_str)
}

/// The entry number of `ingredients/content/<n>.json`.
fn entry_number(path: &str) -> Option<u32> {
    let name = path.strip_prefix(CONTENT)?.strip_suffix(".json")?;
    canonical_number(name)
}

fn canonical_number(name: &str) -> Option<u32> {
    let canonical = !name.is_empty()
        && name.bytes().all(|b| b.is_ascii_digit())
        && (name == "0" || !name.starts_with('0'));
    canonical.then(|| name.parse().ok()).flatten()
}

fn json_of(file: &UnpackedFile) -> Result<Value, LexiconError> {
    let bytes = file
        .bytes
        .strip_prefix(b"\xEF\xBB\xBF")
        .unwrap_or(&file.bytes);
    serde_json::from_slice::<Strict>(bytes)
        .map(|strict| strict.0)
        .map_err(|e| LexiconError::NotJson {
            path: file.path.clone(),
            reason: e.to_string(),
        })
}

/// Why `value` is not an entry: an object whose `brief` and `long` are text when present.
fn entry_problem(value: &Value) -> Option<String> {
    let Value::Object(fields) = value else {
        return Some("it is not a JSON object".to_owned());
    };
    ["brief", "long"].into_iter().find_map(|key| {
        fields
            .get(key)
            .filter(|v| !v.is_string())
            .map(|_| format!("its {key:?} is not text"))
    })
}

/// One entry. Its `long` is the HTML tC4's word details show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexiconEntry {
    pub brief: Option<String>,
    pub long: Option<String>,
    /// Fields besides `brief` and `long`, as the publisher wrote them.
    pub other: Map<String, Value>,
}

thread_local! {
    static ENTRIES_COMPARED: Cell<u64> = const { Cell::new(0) };
}

/// How many times this thread has compared an entry's number with another, in any
/// lexicon: the operation count the lookup tests assert (docs/PERFORMANCE.md rule 2). A
/// lookup that finds its entry by hash compares about one; one that walks the entries
/// compares one per entry it passes.
pub fn lexicon_entries_compared() -> u64 {
    ENTRIES_COMPARED.with(Cell::get)
}

/// An entry number as the key of [`Entries`], counting each comparison in
/// [`lexicon_entries_compared`]. The map's own probing calls `eq`, so whatever the map
/// visits is counted, not only the calls to [`Lexicon::entry`].
#[derive(Debug, Clone, Copy, Eq)]
struct EntryKey(u32);

impl PartialEq for EntryKey {
    fn eq(&self, other: &Self) -> bool {
        ENTRIES_COMPARED.with(|compared| compared.set(compared.get() + 1));
        self.0 == other.0
    }
}

impl Hash for EntryKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

// A fixed hasher makes the comparisons a lookup counts the same in every run, so the count
// tests are deterministic (PERFORMANCE.md rule 2). A random seed would guard against keys
// chosen to collide, but the keys are the entry numbers of a lexicon the app installed from
// a pinned archive, and a lexicon that collided on purpose would cost at worst a scan.
type EntryMap = HashMap<EntryKey, LexiconEntry, BuildHasherDefault<DefaultHasher>>;

/// A lexicon in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexicon {
    source: LexiconSource,
    metadata: Value,
    entries: EntryMap,
    index: Option<Value>,
    files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Header {
    format: String,
    version: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLexicon {
    // Read by `Header` first.
    #[serde(rename = "format")]
    _format: IgnoredAny,
    #[serde(rename = "version")]
    _version: IgnoredAny,
    source: RawSource,
    metadata: Strict,
    entries: Entries,
    #[serde(default)]
    index: Option<Strict>,
    files: Files,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawSource {
    repo: String,
    commit: String,
    archive_sha256: String,
}

impl Lexicon {
    /// Reads a `tc-lexicon` file. The format and version are read first, so a file of
    /// another format or of a newer version is refused for that reason, in words.
    pub fn parse(bytes: &[u8]) -> Result<Self, LexiconError> {
        let malformed = |e: serde_json::Error| LexiconError::Malformed(e.to_string());
        let header: Header = serde_json::from_slice(bytes).map_err(malformed)?;
        if header.format != LEXICON_FORMAT {
            return Err(LexiconError::UnknownFormat {
                found: header.format,
            });
        }
        match header.version {
            LEXICON_VERSION => {}
            0 => return Err(LexiconError::NoSuchVersion(0)),
            found => {
                return Err(LexiconError::NewerVersion {
                    found,
                    reads: LEXICON_VERSION,
                });
            }
        }
        let raw: RawLexicon = serde_json::from_slice(bytes).map_err(malformed)?;
        let bad = |what: &str, e: String| LexiconError::Malformed(format!("{what}: {e}"));
        let source = LexiconSource {
            repo: raw
                .source
                .repo
                .parse()
                .map_err(|e: tc_door43::ParseError| bad("source.repo", e.to_string()))?,
            commit: raw
                .source
                .commit
                .parse()
                .map_err(|e: tc_core::DigestError| bad("source.commit", e.to_string()))?,
            archive_sha256: raw
                .source
                .archive_sha256
                .parse()
                .map_err(|e: crate::ParseError| bad("source.archiveSha256", e.to_string()))?,
        };
        if raw.entries.0.is_empty() {
            return Err(LexiconError::NoEntries);
        }
        Ok(Self {
            source,
            metadata: raw.metadata.0,
            entries: raw.entries.0,
            index: raw.index.map(|i| i.0),
            files: raw.files.0,
        })
    }

    pub fn source(&self) -> &LexiconSource {
        &self.source
    }

    /// The archive's `metadata.json`: the license and credits to show.
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }

    /// `ingredients/index.json`, when the archive had one.
    pub fn index(&self) -> Option<&Value> {
        self.index.as_ref()
    }

    /// The archive's other files, by path: today its `ingredients/README.md`.
    pub fn files(&self) -> &BTreeMap<String, String> {
        &self.files
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry with this number.
    pub fn entry(&self, number: u32) -> Option<&LexiconEntry> {
        self.entries.get(&EntryKey(number))
    }

    /// Every entry number, ascending.
    pub fn numbers(&self) -> Vec<u32> {
        let mut numbers: Vec<u32> = self.entries.keys().map(|key| key.0).collect();
        numbers.sort_unstable();
        numbers
    }
}

/// Reads the installed lexicon of `pin` into memory: one content file, read once.
pub fn open_lexicon(
    store: &ResourceStore,
    pin: &ResourcePin,
) -> Result<Lexicon, LexiconUnavailable> {
    let damaged = |problem: String| LexiconUnavailable::Damaged {
        pin: pin.clone(),
        problem,
    };
    let installed = store
        .dcs(pin)
        .map_err(|e| damaged(e.to_string()))?
        .ok_or_else(|| LexiconUnavailable::NotInstalled(pin.clone()))?;
    if !matches!(
        installed.origin(),
        Origin::Dcs {
            packaging: Packaging::Lexicon,
            ..
        }
    ) {
        return Err(LexiconUnavailable::NotRepackaged(pin.clone()));
    }
    let bytes = installed
        .read(LEXICON_FILE)
        .map_err(|e| damaged(e.to_string()))?;
    Lexicon::parse(&bytes).map_err(|error| LexiconUnavailable::Unreadable {
        pin: pin.clone(),
        error,
    })
}

/// A project's two lexicons, read once for the session. A lexicon that cannot be read
/// stays unavailable, with its reason, until the session opens them again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lexicons {
    ugl: Result<Lexicon, LexiconUnavailable>,
    uhl: Result<Lexicon, LexiconUnavailable>,
}

/// What one part of a `strong` attribute finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartEntry<'a> {
    Found(&'a LexiconEntry),
    /// The part names no entry: a prefix such as `c`, or no number (tC4's id 0).
    NoNumber,
    /// The lexicon has no entry with the part's number.
    NotInLexicon,
    /// The part's lexicon cannot be shown, for this reason.
    Unavailable(&'a LexiconUnavailable),
}

/// One part of a `strong` attribute and what it finds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartLookup<'a> {
    pub part: StrongsPart<'a>,
    pub entry: PartEntry<'a>,
}

impl Lexicons {
    /// Reads the lexicons `pins` names (a project's `lexicon` group): `ugl` from the `nt`
    /// pin and `uhl` from the `ot` pin.
    pub fn open(store: &ResourceStore, pins: &Testaments) -> Self {
        let open = |id: LexiconId| match pins.get(id.testament()) {
            Some(pin) => open_lexicon(store, pin),
            None => Err(LexiconUnavailable::NotPinned(id)),
        };
        Self {
            ugl: open(LexiconId::Ugl),
            uhl: open(LexiconId::Uhl),
        }
    }

    /// Two lexicons already in memory, or unavailable.
    pub fn from_parts(
        ugl: Result<Lexicon, LexiconUnavailable>,
        uhl: Result<Lexicon, LexiconUnavailable>,
    ) -> Self {
        Self { ugl, uhl }
    }

    pub fn get(&self, id: LexiconId) -> Result<&Lexicon, &LexiconUnavailable> {
        match id {
            LexiconId::Ugl => self.ugl.as_ref(),
            LexiconId::Uhl => self.uhl.as_ref(),
        }
    }

    /// Each part of a word's `strong` attribute and the entry it names, in the order of the
    /// parts: what tC4's `lookupStrongsNumbers` finds, read per part as its word details
    /// read it (`WordLexiconDetails.jsx:162-182`). Reads no file.
    pub fn lookup<'a>(&'a self, strong: &'a str) -> Vec<PartLookup<'a>> {
        strongs_parts(strong)
            .into_iter()
            .map(|part| {
                let entry = match part.number {
                    None => PartEntry::NoNumber,
                    Some(number) => match self.get(part.lexicon) {
                        Err(why) => PartEntry::Unavailable(why),
                        Ok(lexicon) => match number {
                            StrongsNumber::Entry(n) => lexicon
                                .entry(n)
                                .map_or(PartEntry::NotInLexicon, PartEntry::Found),
                            StrongsNumber::OutOfRange => PartEntry::NotInLexicon,
                        },
                    },
                };
                PartLookup { part, entry }
            })
            .collect()
    }
}

/// A JSON value read with every object's keys checked to be distinct: a key twice gives
/// a document no single value, and serde_json would keep the last silently.
struct Strict(Value);

impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(StrictVisitor).map(Strict)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Number(v.into()))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("a number JSON cannot hold"))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(Strict(item)) = seq.next_element()? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            match object.entry(key) {
                serde_json::map::Entry::Occupied(taken) => return Err(twice(taken.key())),
                serde_json::map::Entry::Vacant(free) => {
                    free.insert(map.next_value::<Strict>()?.0);
                }
            }
        }
        Ok(Value::Object(object))
    }
}

fn twice<E: de::Error>(key: &str) -> E {
    E::custom(format!("the key {key:?} appears twice"))
}

impl<'de> Deserialize<'de> for LexiconEntry {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_map(EntryVisitor)
    }
}

struct EntryVisitor;

impl<'de> Visitor<'de> for EntryVisitor {
    type Value = LexiconEntry;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an entry: a JSON object whose brief and long are text")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<LexiconEntry, A::Error> {
        let mut entry = LexiconEntry {
            brief: None,
            long: None,
            other: Map::new(),
        };
        while let Some(key) = map.next_key::<String>()? {
            let slot = match key.as_str() {
                "brief" => &mut entry.brief,
                "long" => &mut entry.long,
                _ => match entry.other.entry(key) {
                    serde_json::map::Entry::Occupied(taken) => return Err(twice(taken.key())),
                    serde_json::map::Entry::Vacant(free) => {
                        free.insert(map.next_value::<Strict>()?.0);
                        continue;
                    }
                },
            };
            if slot.replace(map.next_value()?).is_some() {
                return Err(twice(&key));
            }
        }
        Ok(entry)
    }
}

/// `entries`: each key an entry number, written without leading zeros.
struct Entries(EntryMap);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_map(EntriesVisitor)
    }
}

struct EntriesVisitor;

impl<'de> Visitor<'de> for EntriesVisitor {
    type Value = Entries;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("entries by number")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
        let mut entries = EntryMap::default();
        while let Some(key) = map.next_key::<String>()? {
            let number = canonical_number(&key).ok_or_else(|| {
                de::Error::custom(format!(
                    "{key:?} is not an entry number (a decimal number without leading zeros)"
                ))
            })?;
            match entries.entry(EntryKey(number)) {
                std::collections::hash_map::Entry::Occupied(_) => return Err(twice(&key)),
                std::collections::hash_map::Entry::Vacant(free) => {
                    free.insert(map.next_value::<LexiconEntry>()?);
                }
            }
        }
        Ok(Entries(entries))
    }
}

/// `files`: text by path.
struct Files(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for Files {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_map(FilesVisitor)
    }
}

struct FilesVisitor;

impl<'de> Visitor<'de> for FilesVisitor {
    type Value = Files;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("files: text by path")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Files, A::Error> {
        let mut files = BTreeMap::new();
        while let Some(path) = map.next_key::<String>()? {
            match files.entry(path) {
                std::collections::btree_map::Entry::Occupied(taken) => {
                    return Err(twice(taken.key()));
                }
                std::collections::btree_map::Entry::Vacant(free) => {
                    free.insert(map.next_value::<String>()?);
                }
            }
        }
        Ok(Files(files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, bytes: &[u8]) -> UnpackedFile {
        UnpackedFile {
            path: path.to_owned(),
            mode: FileMode::Regular,
            bytes: bytes.to_vec(),
        }
    }

    const METADATA_JSON: &str = r#"{"format":"scripture burrito","identification":{"primary":{"uWBurritos":{"TA":{"revision":"1","timestamp":"2025-11-24T55:20:15.878Z"}}}},"type":{"flavorType":{"name":"peripheral","flavor":{"name":"x-lexicon"}}},"ingredients":{"README.md":{"size":24}}}"#;

    fn archive(files: Vec<UnpackedFile>) -> VerifiedArchive {
        VerifiedArchive {
            commit: "72df5ac25acf9d51e826b20e3ad883a5a657ef4e".parse().unwrap(),
            tree: "ad07f5e41f7052598d90eb8dc2b9ecad811b0b17"
                .parse::<GitOid>()
                .unwrap(),
            files,
        }
    }

    fn sample() -> Vec<UnpackedFile> {
        vec![
            file("ingredients/README.md", b"# Lexicon\r\n\n"),
            file(
                "ingredients/content/1.json",
                br#"{"brief":"father","long":"<i>Meaning:</i> father"}"#,
            ),
            file(
                "ingredients/content/10.json",
                br#"{"long":"ten","brief":"x","strongs":"H10"}"#,
            ),
            file(
                "ingredients/content/2.json",
                br#"{"brief":"two","long":"2"}"#,
            ),
            file(
                "ingredients/index.json",
                r#"[{"id":"H1","name":"אָב"}]"#.as_bytes(),
            ),
            file("metadata.json", METADATA_JSON.as_bytes()),
        ]
    }

    fn repo() -> RepoRef {
        "uW/en_uhl".parse().unwrap()
    }

    fn repackage(files: Vec<UnpackedFile>) -> Result<Vec<u8>, LexiconError> {
        repackage_lexicon(&repo(), &archive(files), Sha256::of(b"zip"))
    }

    #[test]
    fn entries_are_keyed_in_numeric_order_and_values_are_kept() {
        let bytes = repackage(sample()).unwrap();
        let text = String::from_utf8(bytes.clone()).unwrap();
        let at = |needle: &str| text.find(needle).unwrap();
        assert!(at(r#""1":"#) < at(r#""2":"#) && at(r#""2":"#) < at(r#""10":"#));
        assert!(text.starts_with(r#"{"format":"tc-lexicon","version":1,"source":{"repo":"uW/en_uhl","commit":"72df5ac25acf9d51e826b20e3ad883a5a657ef4e","archiveSha256":""#));
        assert!(text.ends_with("}\n"));

        let lexicon = Lexicon::parse(&bytes).unwrap();
        assert_eq!(lexicon.numbers(), [1, 2, 10]);
        let ten = lexicon.entry(10).unwrap();
        assert_eq!(ten.long.as_deref(), Some("ten"));
        assert_eq!(ten.other.get("strongs"), Some(&Value::from("H10")));
        assert_eq!(
            lexicon
                .metadata()
                .pointer("/identification/primary/uWBurritos/TA/timestamp"),
            Some(&Value::from("2025-11-24T55:20:15.878Z")),
            "the invalid timestamp is kept"
        );
        assert_eq!(
            serde_json::to_string(lexicon.metadata()).unwrap(),
            METADATA_JSON,
            "metadata keeps its key order"
        );
        assert_eq!(
            lexicon
                .files()
                .get("ingredients/README.md")
                .map(String::as_str),
            Some("# Lexicon\r\n\n")
        );
        assert_eq!(lexicon.index().unwrap()[0]["name"], "אָב");
        assert_eq!(lexicon.source().repo, repo());
    }

    #[test]
    fn the_order_of_the_archive_does_not_change_the_bytes() {
        let mut reversed = sample();
        reversed.reverse();
        assert_eq!(repackage(sample()).unwrap(), repackage(reversed).unwrap());
    }

    #[test]
    fn an_archive_without_index_has_no_index_key() {
        let files = sample().into_iter().filter(|f| f.path != INDEX).collect();
        let bytes = repackage(files).unwrap();
        assert!(
            !String::from_utf8(bytes.clone())
                .unwrap()
                .contains(r#""index""#)
        );
        assert_eq!(Lexicon::parse(&bytes).unwrap().index(), None);
    }

    #[test]
    fn what_cannot_be_kept_faithfully_is_refused_in_words() {
        let with = |path: &str, bytes: &[u8]| {
            let mut files: Vec<UnpackedFile> =
                sample().into_iter().filter(|f| f.path != path).collect();
            files.push(file(path, bytes));
            repackage(files).unwrap_err().to_string()
        };
        assert_eq!(
            with("ingredients/README.md", b"\xff"),
            "ingredients/README.md is not UTF-8 text"
        );
        assert!(
            with(
                "ingredients/content/2.json",
                br#"{"brief":"a","brief":"b"}"#
            )
            .contains("appears twice")
        );
        assert_eq!(
            with("ingredients/content/2.json", br#"["two"]"#),
            "entry ingredients/content/2.json: it is not a JSON object"
        );
        assert_eq!(
            with("ingredients/content/2.json", br#"{"brief":2}"#),
            "entry ingredients/content/2.json: its \"brief\" is not text"
        );
        assert!(
            with(
                "metadata.json",
                br#"{"type":{"flavorType":{"flavor":{"name":"x-bcvnotes"}}}}"#
            )
            .contains("\"x-bcvnotes\"")
        );
        assert!(
            with("ingredients/index.json", b"[").starts_with("ingredients/index.json is not JSON")
        );

        let mut no_entries: Vec<UnpackedFile> = sample()
            .into_iter()
            .filter(|f| !f.path.starts_with(CONTENT))
            .collect();
        assert_eq!(
            repackage(no_entries.clone()).unwrap_err(),
            LexiconError::NoEntries
        );
        no_entries.retain(|f| f.path != METADATA);
        assert_eq!(repackage(no_entries).unwrap_err(), LexiconError::NoMetadata);

        let mut link = sample();
        link[0].mode = FileMode::Symlink;
        assert_eq!(
            repackage(link).unwrap_err().to_string(),
            "ingredients/README.md is a symbolic link"
        );
    }

    #[test]
    fn a_content_file_not_named_by_a_number_is_kept_as_a_file() {
        let mut files = sample();
        files.push(file("ingredients/content/01.json", b"{}"));
        files.push(file("ingredients/content/notes.txt", b"n"));
        let lexicon = Lexicon::parse(&repackage(files).unwrap()).unwrap();
        assert_eq!(lexicon.numbers(), [1, 2, 10]);
        assert_eq!(
            lexicon.files().keys().collect::<Vec<_>>(),
            [
                "ingredients/README.md",
                "ingredients/content/01.json",
                "ingredients/content/notes.txt"
            ]
        );
    }

    fn leaf() -> impl proptest::strategy::Strategy<Value = Value> {
        use proptest::prelude::*;
        prop_oneof![
            any::<String>().prop_map(Value::from),
            any::<i64>().prop_map(Value::from),
            any::<bool>().prop_map(Value::from),
            Just(Value::Null),
        ]
    }

    /// An entry as a publisher might write it: `brief` and `long` or not, other fields, in
    /// any order.
    fn entry() -> impl proptest::strategy::Strategy<Value = Map<String, Value>> {
        use proptest::prelude::*;
        let other = proptest::collection::vec(
            (
                "[a-z]{1,8}",
                prop_oneof![
                    leaf(),
                    proptest::collection::vec(leaf(), 0..3).prop_map(Value::from)
                ],
            ),
            0..3,
        );
        (
            proptest::option::of(any::<String>()),
            proptest::option::of(any::<String>()),
            other,
            any::<bool>(),
        )
            .prop_map(|(brief, long, other, long_first)| {
                let mut fields: Vec<(String, Value)> = other
                    .into_iter()
                    .filter(|(k, _)| k != "brief" && k != "long")
                    .collect();
                let mut text = vec![("brief", brief), ("long", long)];
                if long_first {
                    text.reverse();
                }
                for (key, value) in text {
                    if let Some(value) = value {
                        fields.insert(0, (key.to_owned(), value.into()));
                    }
                }
                let mut map = Map::new();
                for (key, value) in fields {
                    map.entry(key).or_insert(value);
                }
                map
            })
    }

    /// Entries by number, other files by path, and the index.
    type LexiconFiles = (
        BTreeMap<u32, Map<String, Value>>,
        BTreeMap<String, String>,
        Option<Value>,
    );

    fn lexicon_files() -> impl proptest::strategy::Strategy<Value = LexiconFiles> {
        use proptest::prelude::*;
        (
            proptest::collection::btree_map(any::<u32>(), entry(), 1..12),
            proptest::collection::btree_map("ingredients/[a-z]{1,6}\\.md", any::<String>(), 0..3),
            proptest::option::of(proptest::collection::vec(leaf(), 0..4).prop_map(Value::from)),
        )
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(64))]

        #[test]
        fn any_lexicon_reads_back_as_it_was_written_whatever_the_archive_order(
            (entries, others, index) in lexicon_files(),
            order in proptest::collection::vec(proptest::prelude::any::<u64>(), 16),
        ) {
            let mut files = vec![file(METADATA, METADATA_JSON.as_bytes())];
            for (number, entry) in &entries {
                let bytes = serde_json::to_vec(entry).unwrap();
                files.push(file(&format!("{CONTENT}{number}.json"), &bytes));
            }
            for (path, text) in &others {
                files.push(file(path, text.as_bytes()));
            }
            if let Some(index) = &index {
                files.push(file(INDEX, &serde_json::to_vec(index).unwrap()));
            }
            let bytes = repackage(files.clone()).unwrap();
            // The same files in another order give the same bytes.
            let mut shuffled: Vec<(u64, UnpackedFile)> =
                order.iter().copied().cycle().zip(files).collect();
            shuffled.sort_by_key(|(key, _)| *key);
            let again = repackage(shuffled.into_iter().map(|(_, f)| f).collect()).unwrap();
            proptest::prop_assert_eq!(&again, &bytes);

            let lexicon = Lexicon::parse(&bytes).unwrap();
            proptest::prop_assert_eq!(lexicon.numbers(), entries.keys().copied().collect::<Vec<_>>());
            for (number, written) in &entries {
                let read = lexicon.entry(*number).unwrap();
                let mut whole = read.other.clone();
                if let Some(brief) = &read.brief {
                    whole.insert("brief".into(), brief.as_str().into());
                }
                if let Some(long) = &read.long {
                    whole.insert("long".into(), long.as_str().into());
                }
                let mut expected = written.clone();
                expected.sort_keys();
                whole.sort_keys();
                proptest::prop_assert_eq!(whole, expected);
            }
            let read_files: BTreeMap<String, String> = lexicon.files().clone();
            proptest::prop_assert_eq!(read_files, others);
            proptest::prop_assert_eq!(lexicon.index(), index.as_ref());
            proptest::prop_assert_eq!(
                serde_json::to_string(lexicon.metadata()).unwrap(),
                METADATA_JSON
            );
        }
    }

    #[test]
    fn the_count_grows_with_the_entries_a_lookup_compares() {
        let mut value: Value = serde_json::from_slice(&repackage(sample()).unwrap()).unwrap();
        let entries: Map<String, Value> = (1..=1000u32)
            .map(|n| (n.to_string(), serde_json::json!({ "brief": n.to_string() })))
            .collect();
        value["entries"] = Value::Object(entries);
        let lexicon = Lexicon::parse(&serde_json::to_vec(&value).unwrap()).unwrap();
        let compared = |look: &dyn Fn()| {
            let before = lexicon_entries_compared();
            look();
            lexicon_entries_compared() - before
        };

        let by_hash = compared(&|| {
            for n in 1..=1000 {
                assert!(lexicon.entry(n).is_some());
            }
        });
        assert!((1000..=1100).contains(&by_hash), "{by_hash}");
        assert!(compared(&|| assert!(lexicon.entry(1001).is_none())) <= 2);
        // What a lookup that walked the entries would cost: the count must show it.
        let walked = compared(&|| {
            let found = lexicon
                .entries
                .iter()
                .position(|(key, _)| *key == EntryKey(500));
            assert!(found.is_some());
        });
        let passed = lexicon.entries.keys().position(|key| key.0 == 500).unwrap() as u64;
        assert_eq!(walked, passed + 1);
    }

    #[test]
    fn a_reader_refuses_another_format_or_a_newer_version_in_words() {
        let bytes = repackage(sample()).unwrap();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        let refusal = |value: &Value| {
            Lexicon::parse(&serde_json::to_vec(value).unwrap())
                .unwrap_err()
                .to_string()
        };
        value["version"] = 2.into();
        assert_eq!(
            refusal(&value),
            "the file is version 2 of the lexicon format, and this app reads only version 1: a newer app wrote it"
        );
        value["format"] = "tc-lexicon-2".into();
        assert_eq!(
            refusal(&value),
            "the file's format is \"tc-lexicon-2\", not \"tc-lexicon\", so this app cannot read it"
        );
        value["format"] = LEXICON_FORMAT.into();
        value["version"] = 0.into();
        assert_eq!(
            refusal(&value),
            "version 0 is not a version of the lexicon format"
        );
        value["version"] = 1.into();
        value["extra"] = true.into();
        assert!(refusal(&value).contains("unknown field `extra`"));
        assert!(
            Lexicon::parse(b"{}")
                .unwrap_err()
                .to_string()
                .contains("missing field `format`")
        );
    }
}
