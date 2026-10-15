//! Identifiers for help items and articles, and the small typed values they carry.

use std::fmt;
use std::num::NonZeroU32;

use crate::Error;

fn invalid(what: &'static str, value: &str) -> Error {
    Error::InvalidValue {
        what,
        value: value.to_owned(),
    }
}

/// The `ID` column of a Door43 TSV row, such as `rtc9`. The same id names the same note
/// in every gateway language, so it is the key for carrying decisions across languages.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(String);

impl RowId {
    /// Accepts 1 to 16 ASCII letters and digits. Published rows use 4.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = (1..=16).contains(&text.len()) && text.bytes().all(|b| b.is_ascii_alphanumeric());
        ok.then(|| Self(text.to_owned()))
            .ok_or_else(|| invalid("row id", text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An Aquifer `content_id`. The release JSON writes it as a number or a string; both
/// forms of the same id compare equal here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AquiferContentId(String);

impl AquiferContentId {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = !text.is_empty() && !text.chars().any(|c| c.is_whitespace() || c == '/');
        ok.then(|| Self(text.to_owned()))
            .ok_or_else(|| invalid("Aquifer content id", text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AquiferContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The identity of one note, word link or question.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HelpItemId {
    Row(RowId),
    /// For a word link built from an Aquifer article's passage list, this is the article's
    /// id, so several links share it.
    Aquifer(AquiferContentId),
}

impl HelpItemId {
    /// The Door43 row id behind an item. Aquifer keeps it for some notes, in content ids
    /// of the form `en_tn_TIT_r2gj`.
    pub fn upstream_row_id(&self) -> Option<RowId> {
        match self {
            HelpItemId::Row(id) => Some(id.clone()),
            HelpItemId::Aquifer(id) => {
                let rest = id.as_str().strip_prefix("en_tn_")?;
                let (_book, row) = rest.split_once('_')?;
                RowId::parse(row).ok()
            }
        }
    }
}

/// An article name inside a manual or category, such as `figs-metaphor` or `god`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slug(String);

impl Slug {
    /// Accepts ASCII letters, digits, `-`, `_` and `.`, but not `.` or `..` alone.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = !text.is_empty()
            && text != "."
            && text != ".."
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        ok.then(|| Self(text.to_owned()))
            .ok_or_else(|| invalid("article slug", text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The folder a translation words article lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordCategory {
    /// `kt`
    KeyTerm,
    /// `names`
    Name,
    /// `other`
    Other,
}

impl WordCategory {
    pub fn parse(folder: &str) -> Option<Self> {
        match folder {
            "kt" => Some(Self::KeyTerm),
            "names" => Some(Self::Name),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            Self::KeyTerm => "kt",
            Self::Name => "names",
            Self::Other => "other",
        }
    }
}

/// A translation words article.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordArticleId {
    /// `bible/<category>/<slug>.md` in the RC repository, `payload/<category>/<slug>.md`
    /// in the DCS export.
    Dcs {
        category: WordCategory,
        slug: Slug,
    },
    Aquifer(AquiferContentId),
}

impl WordArticleId {
    /// The category, where the id carries one. Aquifer keeps it only in content ids of
    /// the form `en_tw_kt_iamyahweh`.
    pub fn category(&self) -> Option<WordCategory> {
        match self {
            Self::Dcs { category, .. } => Some(*category),
            Self::Aquifer(id) => {
                let rest = id.as_str().strip_prefix("en_tw_")?;
                WordCategory::parse(rest.split_once('_')?.0)
            }
        }
    }
}

/// The four Translation Academy manuals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Manual {
    Intro,
    Process,
    Translate,
    Checking,
}

impl Manual {
    pub fn parse(folder: &str) -> Option<Self> {
        match folder {
            "intro" => Some(Self::Intro),
            "process" => Some(Self::Process),
            "translate" => Some(Self::Translate),
            "checking" => Some(Self::Checking),
            _ => None,
        }
    }

    pub fn folder(self) -> &'static str {
        match self {
            Self::Intro => "intro",
            Self::Process => "process",
            Self::Translate => "translate",
            Self::Checking => "checking",
        }
    }
}

/// A Translation Academy article.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AcademyArticleId {
    /// `<manual>/<slug>/` in the repository.
    Dcs {
        manual: Manual,
        slug: Slug,
    },
    Aquifer(AquiferContentId),
}

impl AcademyArticleId {
    /// The slug, which tC calls the check's group id. Aquifer keeps it only in content ids
    /// of the form `en_ta_translate_figs-imperative3p`.
    pub fn slug(&self) -> Option<&str> {
        match self {
            Self::Dcs { slug, .. } => Some(slug.as_str()),
            Self::Aquifer(id) => {
                let rest = id.as_str().strip_prefix("en_ta_")?;
                Some(rest.split_once('_')?.1)
            }
        }
    }
}

/// A git commit id: 40 hexadecimal digits, stored in lower case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommitSha(String);

impl CommitSha {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = text.len() == 40 && text.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| Self(text.to_ascii_lowercase()))
            .ok_or_else(|| invalid("commit sha", text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// An original-language quote (or, for Open Bible Stories, a gateway-language phrase),
/// exactly as the resource writes it. ` & ` separates discontinuous spans. The text is kept
/// byte for byte because tC identifies a check decision by it.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Quote(String);

impl Quote {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }

    /// The discontinuous parts of the quote, trimmed, empty parts dropped.
    pub fn spans(&self) -> Vec<&str> {
        self.0
            .split('&')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl fmt::Display for Quote {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which occurrence in the verse of the whole quote an item is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Occurrence {
    /// `0`: the item is not tied to words (introductions, label rows).
    NotTied,
    /// `1`, `2`, …
    Nth(NonZeroU32),
    /// `-1`: every occurrence.
    All,
    /// The source does not record it: an empty TSV cell, or any Aquifer item.
    Unrecorded,
}

impl Occurrence {
    pub fn nth(n: u32) -> Option<Self> {
        NonZeroU32::new(n).map(Self::Nth)
    }

    pub(crate) fn parse_cell(text: &str) -> Option<Self> {
        match text {
            "" => Some(Self::Unrecorded),
            "0" => Some(Self::NotTied),
            "-1" => Some(Self::All),
            _ if text.bytes().all(|b| b.is_ascii_digit()) => text.parse().ok().and_then(Self::nth),
            _ => None,
        }
    }

    pub(crate) fn cell(self) -> String {
        match self {
            Self::NotTied => "0".to_owned(),
            Self::Nth(n) => n.to_string(),
            Self::All => "-1".to_owned(),
            Self::Unrecorded => String::new(),
        }
    }
}

/// A SHA-256 digest of a downloaded file, 64 hexadecimal digits in lower case.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Sha256Digest(String);

impl Sha256Digest {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit());
        ok.then(|| Self(text.to_ascii_lowercase()))
            .ok_or_else(|| invalid("sha256 digest", text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn occurrence_cells_round_trip() {
        for cell in ["", "0", "1", "2", "-1", "17"] {
            assert_eq!(Occurrence::parse_cell(cell).unwrap().cell(), cell);
        }
        for bad in ["-2", "x", "1.0", " 1", "+1"] {
            assert!(Occurrence::parse_cell(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn aquifer_ids_keep_upstream_names_where_they_exist() {
        let note = HelpItemId::Aquifer(AquiferContentId::parse("en_tn_TIT_r2gj").unwrap());
        assert_eq!(note.upstream_row_id().unwrap().as_str(), "r2gj");
        let numeric = HelpItemId::Aquifer(AquiferContentId::parse("140738").unwrap());
        assert_eq!(numeric.upstream_row_id(), None);
        let word = WordArticleId::Aquifer(AquiferContentId::parse("en_tw_kt_iamyahweh").unwrap());
        assert_eq!(word.category(), Some(WordCategory::KeyTerm));
        let ta = AcademyArticleId::Aquifer(
            AquiferContentId::parse("en_ta_translate_figs-imperative3p").unwrap(),
        );
        assert_eq!(ta.slug(), Some("figs-imperative3p"));
    }
}
