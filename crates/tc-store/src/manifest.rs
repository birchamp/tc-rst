//! `.tc/materialized.json`, the manifest (`docs/STORAGE.md` §3.6, §6.2): what the app last
//! wrote at each derived or carried path, and the md5 each pack is listed with in
//! `metadata.json`. An open tests each path against its entry before it reads the file.
//!
//! `.tc/materialize.intent.json` names the writes of a materialization while it runs (§5.10
//! M3), and `.tc/heads/<escaped path>.bin` the heads each written file stands for.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tc_journal::format::DERIVATIONS_V0;
use tc_journal::{ChangeId, FORMAT_MINOR, RecordKey};

/// What the app last wrote at a path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub md5: String,
    pub size: u64,
    pub mtime_ns: i64,
    /// The bytes were read back after their timestamp tick was over (§6.2).
    pub settled: bool,
    /// The scope digest the bytes were projected from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Bytes the app holds at the path without writing over them (§6.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Held>,
    /// A carried file's git blob id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oid: Option<String>,
    /// For a USFM or story file: whether the bytes are in NFC, so `metadata.json` says
    /// `normalization` without projecting every book again (§3.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nfc: Option<bool>,
    /// The git blob that holds exactly these bytes, once a checkpoint has stored it (§9.4).
    /// A new entry has none: a changed file is stored at the next checkpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

/// Bytes held at a path (§6.2): what they are, why they are held, and where their copy is,
/// so the notice of the hold comes back with its own words after a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Held {
    pub md5: String,
    pub cause: HeldCause,
    /// The byte copy in `.tc/external/`, relative to the project folder.
    pub copy: String,
}

/// Why a path is held (§6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HeldCause {
    /// A program changed the file, or the app cannot tell: `OutsideEditHeld`.
    Edit,
    /// The file changed although its size and time did not: `StorageDamage`.
    Damage,
}

impl Entry {
    /// An entry for bytes the app has just read or written at a path, settled or not.
    pub fn of(md5: String, size: u64, mtime_ns: i64, settled: bool) -> Self {
        Self {
            md5,
            size,
            mtime_ns,
            settled,
            scope: None,
            held: None,
            oid: None,
            nfc: None,
            blob: None,
        }
    }
}

/// What `metadata.json` lists for a pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackEntry {
    pub md5: String,
    pub size: u64,
    /// The 64-bit fingerprint (§2.6) the md5 was computed for, as 16 hex digits: an open
    /// fingerprints every pack anyway (O8), so a pack listed here is never hashed again.
    pub fingerprint: String,
    /// The git blob that holds the pack's bytes, once stored (§4.4, §5.11 K3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub minor: u16,
    pub derivations: BTreeMap<String, u64>,
    pub paths: BTreeMap<String, Entry>,
    pub packs: BTreeMap<String, PackEntry>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            minor: FORMAT_MINOR,
            derivations: current_derivations(),
            paths: BTreeMap::new(),
            packs: BTreeMap::new(),
        }
    }
}

pub fn current_derivations() -> BTreeMap<String, u64> {
    DERIVATIONS_V0
        .iter()
        .map(|(name, v)| ((*name).to_owned(), *v))
        .collect()
}

impl Manifest {
    /// Reads the manifest; `None` when it is missing or unreadable (every path is then
    /// read and classified as having no entry, §6.2).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// The projection versions in the manifest are this app's.
    pub fn versions_current(&self) -> bool {
        self.minor == FORMAT_MINOR && self.derivations == current_derivations()
    }
}

/// `.tc/materialize.intent.json` (§5.10 M3).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    pub writes: Vec<IntentWrite>,
    pub removes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentWrite {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_md5: Option<String>,
    pub new_md5: String,
}

impl Intent {
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap_or_default()
    }

    /// The old and new md5 the intent names for `path`, if any.
    pub fn md5s_of(&self, path: &str) -> Vec<&str> {
        self.writes
            .iter()
            .filter(|w| w.path == path)
            .flat_map(|w| w.old_md5.as_deref().into_iter().chain([w.new_md5.as_str()]))
            .collect()
    }

    pub fn names(&self, path: &str) -> bool {
        self.writes.iter().any(|w| w.path == path) || self.removes.iter().any(|r| r == path)
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.removes.is_empty()
    }

    /// Everything either intent names. A path named twice keeps every md5 it was named with,
    /// since any of them may be on disk (a write that stopped, then a retry of it).
    pub fn merged(&self, other: &Self) -> Self {
        let mut out = self.clone();
        for write in &other.writes {
            if !out.writes.contains(write) {
                out.writes.push(write.clone());
            }
        }
        for remove in &other.removes {
            if !out.removes.contains(remove) {
                out.removes.push(remove.clone());
            }
        }
        out
    }

    /// Names `new_md5` and nothing else for `path`: the write went through, so no earlier
    /// bytes of the path are on disk (§5.10).
    pub fn narrow_to(&mut self, path: &str, new_md5: &str) {
        self.writes.retain(|w| w.path != path);
        self.writes.push(IntentWrite {
            path: path.to_owned(),
            old_md5: None,
            new_md5: new_md5.to_owned(),
        });
    }

    /// What the intent names about the paths `done` says are not settled yet.
    pub fn remaining(&self, done: impl Fn(&str) -> bool) -> Self {
        Self {
            writes: self
                .writes
                .iter()
                .filter(|w| !done(&w.path))
                .cloned()
                .collect(),
            removes: self.removes.iter().filter(|r| !done(r)).cloned().collect(),
        }
    }
}

/// A heads file (§3.6): for every record a derived file was built from, the heads its
/// written bytes stand for by the token rule (§2.7), and, for a book's USFM, a digest of how
/// the written file shows the record (§11.1 X3, D10), so reconciling an outside edit looks at
/// the records whose digest changed and no others. Postcard behind a magic prefix; the
/// format without digests (unit S1's) still parses.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadsFile {
    pub records: Vec<HeadsRecord>,
}

/// One record of a heads file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadsRecord {
    /// The record's canonical key.
    pub key: String,
    pub heads: Vec<[u8; 32]>,
    /// How the written file showed the record, when the file kind keeps digests.
    pub shown: Option<[u8; 16]>,
}

/// The format without digests.
#[derive(Deserialize)]
struct HeadsFileV1 {
    records: Vec<(String, Vec<[u8; 32]>)>,
}

/// Marks a heads file that keeps digests.
const HEADS_MAGIC: &[u8; 4] = b"TCH\x02";

impl HeadsFile {
    pub fn of(
        heads: &[(RecordKey, Vec<ChangeId>)],
        digests: &[(RecordKey, tc_journal::outside::ShownDigest)],
    ) -> Self {
        let shown: std::collections::HashMap<&RecordKey, &[u8; 16]> =
            digests.iter().map(|(k, d)| (k, d)).collect();
        Self {
            records: heads
                .iter()
                .map(|(key, ids)| HeadsRecord {
                    key: tc_journal::canon::encode_key(key)
                        .map(|j| j.to_canonical_string())
                        .unwrap_or_default(),
                    heads: ids.iter().map(|id| *id.as_bytes()).collect(),
                    shown: shown.get(key).map(|d| **d),
                })
                .collect(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = HEADS_MAGIC.to_vec();
        out.extend(postcard::to_allocvec(&self.records).unwrap_or_default());
        out
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if let Some(rest) = bytes.strip_prefix(HEADS_MAGIC.as_slice())
            && let Ok(records) = postcard::from_bytes(rest)
        {
            return Some(Self { records });
        }
        let old: HeadsFileV1 = postcard::from_bytes(bytes).ok()?;
        Some(Self {
            records: old
                .records
                .into_iter()
                .map(|(key, heads)| HeadsRecord {
                    key,
                    heads,
                    shown: None,
                })
                .collect(),
        })
    }
}
