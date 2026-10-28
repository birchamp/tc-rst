use crate::error::ValueError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

/// The rule an ingredient path breaks.
///
/// The SB 1.0 schema only refuses a leading `/` and the characters `: ? * " < > |`. These
/// rules add what keeps a path safe to join to the project root on every platform, and
/// what Pankosmia and tC4 need to round-trip the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
pub enum PathRule {
    #[error("the path is empty")]
    Empty,
    #[error("the path starts with '/'; ingredient paths are relative to the project root")]
    Absolute,
    #[error("the path contains '\\'; ingredient paths use '/' as the separator")]
    Backslash,
    #[error("the path has an empty segment")]
    EmptySegment,
    #[error("the path has a '..' segment, which leaves the project folder")]
    ParentDir,
    #[error("a segment starts with '.'; '.' segments and hidden files are not ingredients")]
    Hidden,
    #[error("a segment ends with '.', which Windows drops")]
    TrailingDot,
    #[error("the path contains whitespace, which Pankosmia and tC4 refuse")]
    Whitespace,
    #[error("the path contains a control character")]
    Control,
    /// Zero-width characters and direction overrides (General Category Cf). HFS+ ignores
    /// several of them when it compares names, and an override can disguise a name.
    #[error("the path contains an invisible format character, such as a zero-width space")]
    FormatChar,
    #[error("the path contains {0:?}, which the burrito schema forbids")]
    ForbiddenChar(char),
    #[error("a segment is a device name Windows reserves")]
    ReservedName,
    /// Windows can reach a file by an 8.3 short name such as `INGRED~1`, so a segment of
    /// that shape may name another ingredient's file.
    #[error("a segment has '~' followed by a digit, the shape of a Windows short name")]
    ShortName,
    /// Any spelling that a case-insensitive file system resolves to `metadata.json`.
    #[error("metadata.json describes the burrito and cannot be one of its ingredients")]
    MetadataFile,
}

/// The path of an ingredient, relative to the project root, with `/` between segments,
/// such as `ingredients/TIT.usfm`. A value of this type is always safe to join to the
/// project root: it cannot be absolute, climb out with `..`, name a hidden file, or name
/// `metadata.json` under any spelling a Windows or macOS file system resolves to it.
/// Two different paths can still name one file on those systems (`a.usfm` and `A.usfm`);
/// the validator reports such pairs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IngredientPath(String);

const FORBIDDEN: [char; 7] = [':', '?', '*', '"', '<', '>', '|'];

/// Windows device names. Windows reads the superscript digits ¹ ² ³ as digits in `COM` and
/// `LPT` names, and Microsoft's naming rules also list `COM0` and `LPT0`.
const RESERVED: [&str; 32] = [
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4",
    "COM5", "COM6", "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3",
    "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

pub(crate) const METADATA_FILE: &str = "metadata.json";

impl IngredientPath {
    pub fn parse(text: &str) -> Result<Self, ValueError> {
        check(text)
            .map(|()| Self(text.to_owned()))
            .map_err(|rule| ValueError::Path {
                path: text.to_owned(),
                rule,
            })
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    /// The last segment, such as `TIT.usfm`.
    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or(&self.0)
    }

    /// The text after the last `.` of the file name, such as `usfm`.
    pub fn extension(&self) -> Option<&str> {
        self.file_name()
            .rsplit_once('.')
            .map(|(_, extension)| extension)
    }

    /// The file name without its extension, such as `TIT`.
    pub fn file_stem(&self) -> &str {
        let name = self.file_name();
        name.rsplit_once('.').map_or(name, |(stem, _)| stem)
    }
}

/// Returns the first rule `text` breaks.
pub(crate) fn check(text: &str) -> Result<(), PathRule> {
    broken_rules(text).into_iter().next().map_or(Ok(()), Err)
}

/// Checks the path of any file of a project, ingredient or not (`README.md`,
/// `.gitignore`, `LICENSE copy.md`): only the rules that keep it inside the project
/// folder, and writable, on every platform.
pub fn check_project_path(text: &str) -> Result<(), PathRule> {
    broken_rules(text)
        .into_iter()
        .find(|rule| {
            matches!(
                rule,
                PathRule::Empty
                    | PathRule::Absolute
                    | PathRule::Backslash
                    | PathRule::EmptySegment
                    | PathRule::ParentDir
                    | PathRule::TrailingDot
                    | PathRule::Control
                    | PathRule::ForbiddenChar(_)
                    | PathRule::ReservedName
            )
        })
        .map_or(Ok(()), Err)
}

/// Every rule `text` breaks, in the order [`check`] reports them.
fn broken_rules(text: &str) -> Vec<PathRule> {
    if text.is_empty() {
        return vec![PathRule::Empty];
    }
    let mut out = Vec::new();
    if text.contains('\\') {
        out.push(PathRule::Backslash);
    }
    if text.starts_with('/') {
        out.push(PathRule::Absolute);
    }
    for c in text.chars() {
        if c.is_control() {
            out.push(PathRule::Control);
        }
        if c.is_whitespace() {
            out.push(PathRule::Whitespace);
        }
        if get_general_category(c) == GeneralCategory::Format {
            out.push(PathRule::FormatChar);
        }
        if FORBIDDEN.contains(&c) {
            out.push(PathRule::ForbiddenChar(c));
        }
    }
    for segment in text.split('/') {
        if segment.is_empty() {
            out.push(PathRule::EmptySegment);
            continue;
        }
        if segment == ".." {
            out.push(PathRule::ParentDir);
        }
        if segment.starts_with('.') {
            out.push(PathRule::Hidden);
        }
        if segment.ends_with('.') {
            out.push(PathRule::TrailingDot);
        }
        if segment
            .as_bytes()
            .windows(2)
            .any(|pair| matches!(pair, [b'~', digit] if digit.is_ascii_digit()))
        {
            out.push(PathRule::ShortName);
        }
        let device = segment.split('.').next().unwrap_or(segment);
        if RESERVED
            .iter()
            .any(|name| name.eq_ignore_ascii_case(device))
        {
            out.push(PathRule::ReservedName);
        }
    }
    if fold(text) == METADATA_FILE {
        out.push(PathRule::MetadataFile);
    }
    out
}

/// The form under which Windows and macOS see two names as one file: NFC, then every
/// character upper-cased and lower-cased again, so `É` (NFD) meets `é`, `ſ` meets `s` and
/// `ς` meets `σ`. Folding more than a given file system does only finds more collisions.
/// Two paths collide when their collision forms are equal (`docs/STORAGE.md` §3.3).
pub fn collision_form(text: &str) -> String {
    fold(text)
}

pub(crate) fn fold(text: &str) -> String {
    text.nfc()
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
        .nfc()
        .collect()
}

impl fmt::Display for IngredientPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for IngredientPath {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

impl From<IngredientPath> for String {
    fn from(path: IngredientPath) -> Self {
        path.0
    }
}

impl AsRef<str> for IngredientPath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Serialize for IngredientPath {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for IngredientPath {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_paths_tc4_writes() {
        for ok in [
            "ingredients/TIT.usfm",
            "ingredients/checking/alignments/TIT.json",
            "ingredients/content/01.md",
            "ingredients/LICENSE.md",
            "ingredients/Jonás.usfm",
            "LICENSE",
            "ingredients/metadata.json",
            "ingredients/a~b.usfm",
            "ingredients/COM10.usfm",
        ] {
            assert!(IngredientPath::parse(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn names_the_rule_a_path_breaks() {
        let cases = [
            ("", PathRule::Empty),
            ("/abs.usfm", PathRule::Absolute),
            ("ingredients\\x.usfm", PathRule::Backslash),
            ("c:\\x.usfm", PathRule::Backslash),
            ("ingredients//x.usfm", PathRule::EmptySegment),
            ("ingredients/", PathRule::EmptySegment),
            ("../escape.usfm", PathRule::ParentDir),
            ("ingredients/../../x", PathRule::ParentDir),
            ("./x.usfm", PathRule::Hidden),
            (".git/config", PathRule::Hidden),
            ("ingredients/.DS_Store", PathRule::Hidden),
            ("ingredients/x.", PathRule::TrailingDot),
            ("ingredients/my file.usfm", PathRule::Whitespace),
            ("ingredients/x\u{a0}y", PathRule::Whitespace),
            ("ingredients/x\ny", PathRule::Control),
            ("c:/x.usfm", PathRule::ForbiddenChar(':')),
            ("a?b", PathRule::ForbiddenChar('?')),
            ("ingredients/con.usfm", PathRule::ReservedName),
            ("ingredients/LPT1", PathRule::ReservedName),
            ("ingredients/COM¹.usfm", PathRule::ReservedName),
            ("ingredients/lpt³", PathRule::ReservedName),
            ("ingredients/CONIN$", PathRule::ReservedName),
            ("ingredients/conout$.txt", PathRule::ReservedName),
            ("ingredients/lpt0.txt", PathRule::ReservedName),
            ("ingredients/TIT\u{202e}nosj.usfm", PathRule::FormatChar),
            ("ingredients/a\u{200b}b.usfm", PathRule::FormatChar),
            ("metadata\u{200c}.json", PathRule::FormatChar),
            ("ingredients/\u{feff}TIT.usfm", PathRule::FormatChar),
            ("INGRED~1/TIT.usfm", PathRule::ShortName),
            ("METADA~1.JSO", PathRule::ShortName),
            ("metadata.json", PathRule::MetadataFile),
            ("Metadata.json", PathRule::MetadataFile),
            ("METADATA.JSON", PathRule::MetadataFile),
            ("metadata.JSON", PathRule::MetadataFile),
            // U+017F LATIN SMALL LETTER LONG S upper-cases to S.
            ("metadata.j\u{17f}on", PathRule::MetadataFile),
        ];
        for (text, rule) in cases {
            assert_eq!(check(text), Err(rule), "{text:?}");
        }
    }

    /// A project file may be hidden, spaced or named like the metadata; it may never leave
    /// the folder or be unwritable on Windows, even when a rule `check` reports first
    /// (whitespace here) hides the one that matters.
    #[test]
    fn a_project_path_is_checked_only_for_what_keeps_it_inside_the_folder() {
        for ok in [
            ".gitignore",
            "README.md",
            "LICENSE copy.md",
            "metadata.json",
            "a/b~1.txt",
        ] {
            assert_eq!(check_project_path(ok), Ok(()), "{ok}");
        }
        for (text, rule) in [
            ("a\\..\\..\\evil", PathRule::Backslash),
            ("my file/../../evil", PathRule::ParentDir),
            ("c:evil", PathRule::ForbiddenChar(':')),
            ("/etc/passwd", PathRule::Absolute),
            ("a//b", PathRule::EmptySegment),
            ("dir./x", PathRule::TrailingDot),
            ("con.txt", PathRule::ReservedName),
            ("a\nb", PathRule::Control),
            ("", PathRule::Empty),
        ] {
            assert_eq!(check_project_path(text), Err(rule), "{text:?}");
        }
    }

    #[test]
    fn folding_meets_case_and_normalization_variants() {
        let same = [
            (
                "ingredients/Jon\u{e1}s.usfm",
                "ingredients/jona\u{301}s.USFM",
            ),
            ("ingredients/TIT.usfm", "INGREDIENTS/tit.usfm"),
            ("ingredients/\u{3a3}", "ingredients/\u{3c2}"),
        ];
        for (a, b) in same {
            assert_eq!(fold(a), fold(b), "{a:?} {b:?}");
        }
        assert_ne!(fold("ingredients/TIT.usfm"), fold("ingredients/TIT.usf"));
    }

    #[test]
    fn splits_names_and_extensions() {
        let path = IngredientPath::parse("ingredients/checking/TIT.json").unwrap();
        assert_eq!(path.file_name(), "TIT.json");
        assert_eq!(path.file_stem(), "TIT");
        assert_eq!(path.extension(), Some("json"));
        let bare = IngredientPath::parse("LICENSE").unwrap();
        assert_eq!(bare.extension(), None);
        assert_eq!(bare.file_stem(), "LICENSE");
    }
}
