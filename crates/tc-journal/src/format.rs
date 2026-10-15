//! Format versioning (`docs/STORAGE.md` §12): what this app reads and writes, the project's
//! `ingredients/tc/format.json`, the gate an open, a sync intake and a file import pass, and
//! the paths each minor derives.

use serde_json::value::RawValue;
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

/// A version number of `format.json`: a non-negative integer of any size, compared as a
/// number and written back with the digits it was read with (§12.1). A newer app's numbers
/// may not fit any integer type of this one, and reading such a document as damaged would
/// overwrite it (§6.1 O4, §9.2 N1) instead of refusing a newer major.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct FormatNumber(Box<str>);

impl FormatNumber {
    /// A JSON integer literal that is not negative: `0`, or digits without a leading zero.
    pub fn parse(text: &str) -> Option<Self> {
        let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        (digits && (text == "0" || !text.starts_with('0'))).then(|| Self(text.into()))
    }

    /// The number, when it fits a `u64`.
    pub fn get(&self) -> Option<u64> {
        self.0.parse().ok()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<u64> for FormatNumber {
    fn from(value: u64) -> Self {
        Self(value.to_string().into())
    }
}

impl Ord for FormatNumber {
    /// Without leading zeros, a longer number is a larger one.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (self.0.len(), self.0.as_bytes()).cmp(&(other.0.len(), other.0.as_bytes()))
    }
}

impl PartialOrd for FormatNumber {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq<u64> for FormatNumber {
    fn eq(&self, other: &u64) -> bool {
        self.get() == Some(*other)
    }
}

impl PartialOrd<u64> for FormatNumber {
    fn partial_cmp(&self, other: &u64) -> Option<std::cmp::Ordering> {
        Some(self.cmp(&Self::from(*other)))
    }
}

impl fmt::Display for FormatNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for FormatNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A JSON value of `format.json` that this app does not model, kept exactly: numbers keep
/// the text they were written with, so a member a newer minor adds is written back as it
/// came, whatever kind of number it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatJson {
    Null,
    Bool(bool),
    /// A JSON number literal, as written.
    Number(Box<str>),
    String(String),
    Array(Vec<FormatJson>),
    Object(BTreeMap<String, FormatJson>),
}

/// The deepest nesting of a member this app reads; any app writes far less.
const MAX_MEMBER_DEPTH: usize = 64;

impl FormatJson {
    /// Reads a JSON value whose syntax serde_json has checked.
    fn read(raw: &RawValue, depth: usize) -> Result<Self, FormatError> {
        let text = raw.get();
        let shape = |problem: &str| FormatError::Shape(problem.to_owned());
        if depth > MAX_MEMBER_DEPTH {
            return Err(shape("a member nests deeper than 64 levels"));
        }
        Ok(match text.as_bytes().first() {
            Some(b'{') => Self::Object(
                serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(text)
                    .map_err(|_| shape("a member is not JSON"))?
                    .into_iter()
                    .map(|(key, value)| Ok((key, Self::read(&value, depth + 1)?)))
                    .collect::<Result<_, FormatError>>()?,
            ),
            Some(b'[') => Self::Array(
                serde_json::from_str::<Vec<Box<RawValue>>>(text)
                    .map_err(|_| shape("a member is not JSON"))?
                    .iter()
                    .map(|value| Self::read(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            Some(b'"') => Self::String(
                serde_json::from_str(text).map_err(|_| shape("a string is not Unicode text"))?,
            ),
            Some(b't') => Self::Bool(true),
            Some(b'f') => Self::Bool(false),
            Some(b'n') => Self::Null,
            _ => Self::Number(text.into()),
        })
    }

    /// Writes the value the way `tc_burrito::to_canonical_json` writes `metadata.json`:
    /// keys sorted, two-space indentation, strings escaped as serde_json escapes them.
    fn write(&self, out: &mut Vec<u8>, depth: usize) {
        let newline = |out: &mut Vec<u8>, depth: usize| {
            out.push(b'\n');
            out.extend(std::iter::repeat_n(b' ', depth * 2));
        };
        match self {
            Self::Null => out.extend_from_slice(b"null"),
            Self::Bool(true) => out.extend_from_slice(b"true"),
            Self::Bool(false) => out.extend_from_slice(b"false"),
            Self::Number(text) => out.extend_from_slice(text.as_bytes()),
            Self::String(text) => {
                // serde_json writes a string with exactly these escapes.
                let quoted = tc_burrito::to_canonical_json(&serde_json::Value::from(text.as_str()));
                out.extend_from_slice(&quoted[..quoted.len() - 1]);
            }
            Self::Array(items) if items.is_empty() => out.extend_from_slice(b"[]"),
            Self::Array(items) => {
                out.push(b'[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(b',');
                    }
                    newline(out, depth + 1);
                    item.write(out, depth + 1);
                }
                newline(out, depth);
                out.push(b']');
            }
            Self::Object(map) if map.is_empty() => out.extend_from_slice(b"{}"),
            Self::Object(map) => {
                out.push(b'{');
                for (index, (key, item)) in map.iter().enumerate() {
                    if index > 0 {
                        out.push(b',');
                    }
                    newline(out, depth + 1);
                    Self::String(key.clone()).write(out, depth + 1);
                    out.extend_from_slice(b": ");
                    item.write(out, depth + 1);
                }
                newline(out, depth);
                out.push(b'}');
            }
        }
    }
}

/// `ingredients/tc/format.json`. Fields a newer minor adds are kept and written back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatDoc {
    pub major: FormatNumber,
    pub minor: FormatNumber,
    pub min_reader_minor: FormatNumber,
    /// Derivation name → version, including names a newer minor added.
    pub derivations: BTreeMap<String, FormatNumber>,
    pub derived_paths: Vec<PathPattern>,
    /// Members this app does not model.
    pub extra: BTreeMap<String, FormatJson>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatGate {
    /// Open and edit.
    Writable,
    /// `minReaderMinor` is above this app's minor: open, show and export only.
    ReadOnly { min_reader_minor: FormatNumber },
    /// A larger major: refused before anything is written.
    Refused { major: FormatNumber },
    /// A smaller major: migrated by an explicit, confirmed step that first writes a backup.
    NeedsMigration { major: FormatNumber },
}

impl FormatDoc {
    /// The document this app writes for a new project.
    pub fn current() -> Self {
        Self {
            major: u64::from(FORMAT_MAJOR).into(),
            minor: u64::from(FORMAT_MINOR).into(),
            min_reader_minor: 0.into(),
            derivations: DERIVATIONS_V0
                .iter()
                .map(|(name, version)| ((*name).to_owned(), (*version).into()))
                .collect(),
            derived_paths: derived_paths(FORMAT_MINOR),
            extra: BTreeMap::new(),
        }
    }

    /// Reads `format.json` written by any app version. Formatting is not checked: the file
    /// is derived and rewritten in [`FormatDoc::to_bytes`] form.
    pub fn parse(bytes: &[u8]) -> Result<Self, FormatError> {
        // Raw values, so that no number is read into a type it might not fit.
        let whole: Box<RawValue> =
            serde_json::from_slice(bytes).map_err(|_| FormatError::NotJson)?;
        let shape = |text: &str| FormatError::Shape(text.to_owned());
        let mut map: BTreeMap<String, Box<RawValue>> =
            serde_json::from_str(whole.get()).map_err(|_| shape("not an object"))?;
        let number = |key: &str, raw: Option<Box<RawValue>>| -> Result<FormatNumber, FormatError> {
            let text = raw.as_ref().map_or("", |raw| raw.get());
            FormatNumber::parse(text).ok_or_else(|| match text.strip_prefix('-') {
                Some(rest) if FormatNumber::parse(rest).is_some() => {
                    shape(&format!("{key} is negative"))
                }
                _ => shape(&format!("{key} is not an integer")),
            })
        };
        let format: Option<String> = map
            .remove("format")
            .and_then(|raw| serde_json::from_str(raw.get()).ok());
        if format.as_deref() != Some("tc-project") {
            return Err(shape("format is not \"tc-project\""));
        }
        let major = number("major", map.remove("major"))?;
        let minor = number("minor", map.remove("minor"))?;
        let min_reader_minor = number("minReaderMinor", map.remove("minReaderMinor"))?;
        let derivations = map
            .remove("derivations")
            .and_then(|raw| serde_json::from_str::<BTreeMap<String, Box<RawValue>>>(raw.get()).ok())
            .ok_or_else(|| shape("derivations is not an object"))?
            .into_iter()
            .map(|(name, version)| Ok((name, number("a derivation version", Some(version))?)))
            .collect::<Result<_, FormatError>>()?;
        let derived_paths = map
            .remove("derivedPaths")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw.get()).ok())
            .ok_or_else(|| shape("derivedPaths is not an array of strings"))?
            .iter()
            .map(|item| {
                PathPattern::parse(item)
                    .ok_or_else(|| shape("a derived path is not a path pattern"))
            })
            .collect::<Result<_, _>>()?;
        let extra = map
            .into_iter()
            .map(|(key, raw)| Ok((key, FormatJson::read(&raw, 1)?)))
            .collect::<Result<_, FormatError>>()?;
        Ok(Self {
            major,
            minor,
            min_reader_minor,
            derivations,
            derived_paths,
            extra,
        })
    }

    fn to_json(&self) -> FormatJson {
        let number = |n: &FormatNumber| FormatJson::Number(n.as_str().into());
        let mut map = self.extra.clone();
        map.insert("format".into(), FormatJson::String("tc-project".into()));
        map.insert("major".into(), number(&self.major));
        map.insert("minor".into(), number(&self.minor));
        map.insert("minReaderMinor".into(), number(&self.min_reader_minor));
        map.insert(
            "derivations".into(),
            FormatJson::Object(
                self.derivations
                    .iter()
                    .map(|(name, version)| (name.clone(), number(version)))
                    .collect(),
            ),
        );
        map.insert(
            "derivedPaths".into(),
            FormatJson::Array(
                self.derived_paths
                    .iter()
                    .map(|p| FormatJson::String(p.as_str().to_owned()))
                    .collect(),
            ),
        );
        FormatJson::Object(map)
    }

    /// The file's bytes: keys sorted, two-space indentation, one trailing line feed, the
    /// form `metadata.json` is written in. Equal documents give equal bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.to_json().write(&mut out, 0);
        out.push(b'\n');
        out
    }

    /// What this app may do with a project of this format.
    pub fn gate(&self) -> FormatGate {
        let major = u64::from(FORMAT_MAJOR);
        if self.major > major {
            FormatGate::Refused {
                major: self.major.clone(),
            }
        } else if self.major < major {
            FormatGate::NeedsMigration {
                major: self.major.clone(),
            }
        } else if self.min_reader_minor > u64::from(FORMAT_MINOR) {
            FormatGate::ReadOnly {
                min_reader_minor: self.min_reader_minor.clone(),
            }
        } else {
            FormatGate::Writable
        }
    }

    /// The document after an intake of `other` (§12.1): `minor` and `minReaderMinor` by
    /// maximum; derivations, derived paths and unknown fields from the document with the
    /// larger minor. Commutative, associative and idempotent, so teammates converge.
    pub fn merge(&self, other: &Self) -> Self {
        // Ties between documents of one minor fall to the larger canonical text, so the
        // result never depends on which side is local.
        let source = if (&self.minor, self.rest_bytes()) >= (&other.minor, other.rest_bytes()) {
            self
        } else {
            other
        };
        Self {
            major: self.major.clone().max(other.major.clone()),
            minor: self.minor.clone().max(other.minor.clone()),
            min_reader_minor: self
                .min_reader_minor
                .clone()
                .max(other.min_reader_minor.clone()),
            derivations: source.derivations.clone(),
            derived_paths: source.derived_paths.clone(),
            extra: source.extra.clone(),
        }
    }

    fn rest_bytes(&self) -> Vec<u8> {
        let mut doc = self.clone();
        doc.minor = 0.into();
        doc.min_reader_minor = 0.into();
        doc.major = 0.into();
        doc.to_bytes()
    }

    /// Rule 6: writing a change kind introduced in minor `v` raises `minor` to at least `v`.
    /// Never lowers anything.
    pub fn raised_to(&self, v: u16) -> Self {
        let mut doc = self.clone();
        doc.minor = doc.minor.max(u64::from(v).into());
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
