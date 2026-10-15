//! Format versioning (`docs/STORAGE.md` §12): what this app reads and writes, the project's
//! `ingredients/tc/format.json`, the gate an open, a sync intake and a file import pass, and
//! the paths each minor derives.

use crate::json::{Json, JsonMap};
use std::collections::BTreeMap;
use std::fmt;

/// The project format major this app reads and writes.
pub const FORMAT_MAJOR: u16 = 1;
/// The newest project format minor this app knows. A change's `v` above it comes from a
/// newer app.
pub const FORMAT_MINOR: u16 = 0;

/// Where `format.json` lives.
pub const FORMAT_PATH: &str = "ingredients/tc/format.json";

/// A path pattern of `derivedPaths`: `/`-separated segments where `*` matches any run of
/// characters inside one segment and a `**` segment matches one or more whole segments.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathPattern(Box<str>);

impl PathPattern {
    /// Any non-empty pattern without empty segments.
    pub fn parse(text: &str) -> Option<Self> {
        (!text.is_empty() && text.split('/').all(|s| !s.is_empty())).then(|| Self(text.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// True when `path` matches. Letter case and Unicode normalization count, so a caller
    /// that compares paths the way Windows and macOS do passes collision forms to
    /// [`PathPattern::matches_folded`].
    pub fn matches(&self, path: &str) -> bool {
        let pattern: Vec<&str> = self.0.split('/').collect();
        let path: Vec<&str> = path.split('/').collect();
        match_segments(&pattern, &path)
    }

    /// [`PathPattern::matches`] on a path already in collision form, against the pattern's
    /// collision form.
    pub fn matches_folded(&self, folded_path: &str) -> bool {
        let folded = tc_burrito::collision_form(&self.0);
        Self(folded.into()).matches(folded_path)
    }
}

impl fmt::Display for PathPattern {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Patterns come from `format.json`, which any app or person may have written, so matching
/// takes time proportional to the pattern's length times the path's, never exponential.
fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    // reach[j]: the pattern so far matches the first j segments of the path.
    let mut reach = vec![false; path.len() + 1];
    reach[0] = true;
    for segment in pattern {
        let mut next = vec![false; path.len() + 1];
        if *segment == "**" {
            let mut earlier = false;
            for (j, slot) in next.iter_mut().enumerate() {
                *slot = earlier;
                earlier |= reach[j];
            }
        } else {
            for (j, part) in path.iter().enumerate() {
                if reach[j] && glob(segment.as_bytes(), part.as_bytes()) {
                    next[j + 1] = true;
                }
            }
        }
        reach = next;
    }
    reach[path.len()]
}

/// `*` matches any run of bytes; every other byte matches itself. Only the last `*` seen
/// is ever retried, which is enough because a later `*` can absorb whatever an earlier one
/// would have.
fn glob(pattern: &[u8], text: &[u8]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut retry: Option<(usize, usize)> = None;
    while t < text.len() {
        if pattern.get(p) == Some(&b'*') {
            retry = Some((p, t));
            p += 1;
        } else if pattern.get(p) == Some(&text[t]) {
            p += 1;
            t += 1;
        } else if let Some((star, from)) = retry {
            p = star + 1;
            t = from + 1;
            retry = Some((star, from + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|b| *b == b'*')
}

const DERIVED_V0: &[&str] = &[
    "ingredients/*.usfm",
    "ingredients/vrs.json",
    "ingredients/checking/**",
    "ingredients/content/*.md",
    "ingredients/tc/comments/*.json",
    "ingredients/tc/conflicts.json",
    "ingredients/tc/format.json",
];

/// The paths format minor `minor` derives, for every minor up to this app's. Minor 0 is the
/// only one so far; a later minor that derives more paths extends this table, and a minor
/// above this app's derives what this app's minor derives.
pub fn derived_paths(_minor: u16) -> Vec<PathPattern> {
    DERIVED_V0
        .iter()
        .filter_map(|p| PathPattern::parse(p))
        .collect()
}

/// The derivation functions and their versions (§7.7, §12.2 rule 3).
pub const DERIVATIONS_V0: &[(&str, u64)] = &[
    ("plainText", 1),
    ("selections", 1),
    ("tokenizer", 1),
    ("verseSplit", 1),
];

/// `ingredients/tc/format.json`. Fields a newer minor adds are kept and written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDoc {
    /// Read as any non-negative integer, so a number beyond this app's types is still
    /// compared and written back as it was.
    pub major: u64,
    pub minor: u64,
    pub min_reader_minor: u64,
    /// Derivation name → version, including names a newer minor added.
    pub derivations: BTreeMap<String, u64>,
    pub derived_paths: Vec<PathPattern>,
    /// Members this app does not model.
    pub extra: JsonMap,
}

/// Why a `format.json` cannot be read. Open restores it from the HEAD tree (§6.1 O4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    #[error("format.json is not JSON")]
    NotJson,
    #[error("format.json is not a translationCore project format document: {0}")]
    Shape(String),
}

/// What this app may do with a project of a given format (§12.2 rules 1 and 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatGate {
    /// Open and edit.
    Writable,
    /// `minReaderMinor` is above this app's minor: open, show and export only.
    ReadOnly { min_reader_minor: u64 },
    /// A larger major: refused before anything is written.
    Refused { major: u64 },
    /// A smaller major: migrated by an explicit, confirmed step that first writes a backup.
    NeedsMigration { major: u64 },
}

impl FormatDoc {
    /// The document this app writes for a new project.
    pub fn current() -> Self {
        Self {
            major: u64::from(FORMAT_MAJOR),
            minor: u64::from(FORMAT_MINOR),
            min_reader_minor: 0,
            derivations: DERIVATIONS_V0
                .iter()
                .map(|(name, version)| ((*name).to_owned(), *version))
                .collect(),
            derived_paths: derived_paths(FORMAT_MINOR),
            extra: JsonMap::new(),
        }
    }

    /// Reads `format.json` written by any app version. Formatting is not checked: the file
    /// is derived and rewritten in [`FormatDoc::to_bytes`] form.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|_| FormatError::NotJson)?;
        let json = Json::try_from(&value).map_err(|e| FormatError::Shape(e.to_string()))?;
        let Json::Obj(mut map) = json else {
            return Err(FormatError::Shape("not an object".into()));
        };
        let shape = |text: &str| FormatError::Shape(text.to_owned());
        if map
            .remove("format")
            .and_then(|f| f.as_str().map(str::to_owned))
            .as_deref()
            != Some("tc-project")
        {
            return Err(shape("format is not \"tc-project\""));
        }
        let mut number = |key: &str| -> Result<u64, FormatError> {
            match map.remove(key) {
                Some(Json::Int(n)) => {
                    u64::try_from(n.get()).map_err(|_| shape(&format!("{key} is negative")))
                }
                _ => Err(shape(&format!("{key} is not an integer"))),
            }
        };
        let major = number("major")?;
        let minor = number("minor")?;
        let min_reader_minor = number("minReaderMinor")?;
        let derivations = match map.remove("derivations") {
            Some(Json::Obj(items)) => items
                .into_iter()
                .map(|(name, version)| match version {
                    Json::Int(n) => u64::try_from(n.get())
                        .map(|v| (name, v))
                        .map_err(|_| shape("a derivation version is negative")),
                    _ => Err(shape("a derivation version is not an integer")),
                })
                .collect::<Result<_, _>>()?,
            _ => return Err(shape("derivations is not an object")),
        };
        let derived_paths = match map.remove("derivedPaths") {
            Some(Json::Arr(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .and_then(PathPattern::parse)
                        .ok_or_else(|| shape("a derived path is not a path pattern"))
                })
                .collect::<Result<_, _>>()?,
            _ => return Err(shape("derivedPaths is not an array")),
        };
        Ok(Self {
            major,
            minor,
            min_reader_minor,
            derivations,
            derived_paths,
            extra: map,
        })
    }

    fn to_json(&self) -> Json {
        let mut map = self.extra.clone();
        map.insert("format".into(), Json::from("tc-project"));
        map.insert("major".into(), Json::from(self.major));
        map.insert("minor".into(), Json::from(self.minor));
        map.insert("minReaderMinor".into(), Json::from(self.min_reader_minor));
        map.insert(
            "derivations".into(),
            Json::Obj(
                self.derivations
                    .iter()
                    .map(|(k, v)| (k.clone(), Json::from(*v)))
                    .collect(),
            ),
        );
        map.insert(
            "derivedPaths".into(),
            Json::Arr(
                self.derived_paths
                    .iter()
                    .map(|p| Json::from(p.as_str()))
                    .collect(),
            ),
        );
        Json::Obj(map)
    }

    /// The file's bytes: keys sorted, two-space indentation, one trailing line feed, the
    /// form `metadata.json` is written in. Equal documents give equal bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        tc_burrito::to_canonical_json(&serde_json::Value::from(&self.to_json()))
    }

    /// What this app may do with a project of this format.
    pub fn gate(&self) -> FormatGate {
        if self.major > u64::from(FORMAT_MAJOR) {
            FormatGate::Refused { major: self.major }
        } else if self.major < u64::from(FORMAT_MAJOR) {
            FormatGate::NeedsMigration { major: self.major }
        } else if self.min_reader_minor > u64::from(FORMAT_MINOR) {
            FormatGate::ReadOnly {
                min_reader_minor: self.min_reader_minor,
            }
        } else {
            FormatGate::Writable
        }
    }

    /// The document after an intake of `other` (§12.1): `minor` and `minReaderMinor` by
    /// maximum; derivations, derived paths and unknown fields from the document with the
    /// larger minor. Commutative, associative and idempotent, so teammates converge.
    pub fn merge(&self, other: &Self) -> Self {
        let pick = |a: &Self, b: &Self| -> bool {
            // Ties between documents of one minor fall to the larger canonical text, so the
            // result never depends on which side is local.
            (a.minor, a.rest_bytes()) >= (b.minor, b.rest_bytes())
        };
        let source = if pick(self, other) { self } else { other };
        Self {
            major: self.major.max(other.major),
            minor: self.minor.max(other.minor),
            min_reader_minor: self.min_reader_minor.max(other.min_reader_minor),
            derivations: source.derivations.clone(),
            derived_paths: source.derived_paths.clone(),
            extra: source.extra.clone(),
        }
    }

    fn rest_bytes(&self) -> Vec<u8> {
        let mut doc = self.clone();
        doc.minor = 0;
        doc.min_reader_minor = 0;
        doc.major = 0;
        doc.to_json().to_canonical()
    }

    /// Rule 6: writing a change kind introduced in minor `v` raises `minor` to at least `v`.
    /// Never lowers anything.
    pub fn raised_to(&self, v: u16) -> Self {
        let mut doc = self.clone();
        doc.minor = doc.minor.max(u64::from(v));
        doc
    }

    /// True when `path` is derived by the project's format but not by this app (§12.2
    /// rule 4): such a file is kept verbatim, never rewritten, removed or carried.
    pub fn is_newer_derived(&self, path: &str) -> bool {
        let own = derived_paths(FORMAT_MINOR);
        self.derived_paths.iter().any(|p| p.matches(path)) && !own.iter().any(|p| p.matches(path))
    }
}

/// The gate for raw `format.json` bytes, or why they cannot be read.
pub fn gate(bytes: &[u8]) -> Result<FormatGate, FormatError> {
    FormatDoc::parse(bytes).map(|doc| doc.gate())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_segments() {
        let usfm = PathPattern::parse("ingredients/*.usfm").unwrap();
        assert!(usfm.matches("ingredients/TIT.usfm"));
        assert!(!usfm.matches("ingredients/audio/TIT.usfm"));
        assert!(!usfm.matches("ingredients/TIT.usfm.bak"));
        let checking = PathPattern::parse("ingredients/checking/**").unwrap();
        assert!(checking.matches("ingredients/checking/resources.json"));
        assert!(checking.matches("ingredients/checking/alignments/TIT.json"));
        assert!(!checking.matches("ingredients/checking"));
        assert!(!checking.matches("ingredients/checkingx/a"));
        assert!(usfm.matches_folded(&tc_burrito::collision_form("Ingredients/tit.USFM")));
        assert!(PathPattern::parse("a//b").is_none());
        let both = PathPattern::parse("a/**/b/**").unwrap();
        assert!(both.matches("a/x/b/y"));
        assert!(both.matches("a/x/y/b/z/w"));
        assert!(!both.matches("a/b/y"));
        assert!(!both.matches("a/x/b"));
        let stars = PathPattern::parse("*a*b*").unwrap();
        assert!(stars.matches("ab") && stars.matches("xaybz") && !stars.matches("ba"));
        assert!(PathPattern::parse("*").unwrap().matches(""));
    }

    #[test]
    fn hostile_patterns_match_in_polynomial_time() {
        let started = std::time::Instant::now();
        let stars = PathPattern::parse(&format!("{}b", "*a".repeat(40))).unwrap();
        assert!(!stars.matches(&"a".repeat(4000)));
        let deep = PathPattern::parse(&format!("{}x", "**/".repeat(40))).unwrap();
        assert!(!deep.matches(&["a"; 400].join("/")));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
