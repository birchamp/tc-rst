use crate::error::ValueError;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// The rule an ingredient path breaks.
///
/// The SB 1.0 schema only refuses a leading `/` and the characters `: ? * " < > |`. These
/// rules add what keeps a path safe to join to the project root on every platform, and
/// what Pankosmia and tC4 need to round-trip the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, thiserror::Error)]
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
    #[error("the path contains {0:?}, which the burrito schema forbids")]
    ForbiddenChar(char),
    #[error("a segment is a device name Windows reserves")]
    ReservedName,
    #[error("metadata.json describes the burrito and cannot be one of its ingredients")]
    MetadataFile,
}

/// The path of an ingredient, relative to the project root, with `/` between segments,
/// such as `ingredients/TIT.usfm`. A value of this type is always safe to join to the
/// project root: it cannot be absolute, climb out with `..`, or name a hidden file.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IngredientPath(String);

const FORBIDDEN: [char; 7] = [':', '?', '*', '"', '<', '>', '|'];

const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

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
    if text.is_empty() {
        return Err(PathRule::Empty);
    }
    if text.contains('\\') {
        return Err(PathRule::Backslash);
    }
    if text.starts_with('/') {
        return Err(PathRule::Absolute);
    }
    for c in text.chars() {
        if c.is_control() {
            return Err(PathRule::Control);
        }
        if c.is_whitespace() {
            return Err(PathRule::Whitespace);
        }
        if FORBIDDEN.contains(&c) {
            return Err(PathRule::ForbiddenChar(c));
        }
    }
    for segment in text.split('/') {
        if segment.is_empty() {
            return Err(PathRule::EmptySegment);
        }
        if segment == ".." {
            return Err(PathRule::ParentDir);
        }
        if segment.starts_with('.') {
            return Err(PathRule::Hidden);
        }
        if segment.ends_with('.') {
            return Err(PathRule::TrailingDot);
        }
        let device = segment.split('.').next().unwrap_or(segment);
        if RESERVED
            .iter()
            .any(|name| name.eq_ignore_ascii_case(device))
        {
            return Err(PathRule::ReservedName);
        }
    }
    if text == "metadata.json" {
        return Err(PathRule::MetadataFile);
    }
    Ok(())
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
            ("metadata.json", PathRule::MetadataFile),
        ];
        for (text, rule) in cases {
            assert_eq!(check(text), Err(rule), "{text:?}");
        }
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
