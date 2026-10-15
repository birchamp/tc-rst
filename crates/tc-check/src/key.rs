//! The identity of a check: tC4's decision identity, the same fields as `CheckKey` in
//! `docs/STORAGE.md` §2.2.
//!
//! These types mirror the journal's so this crate does not depend on `tc-journal`. They
//! convert field by field: [`ToolId`] is `ToolId`, [`CheckId`] is `CheckId`,
//! [`RefPart`] is `RefPart` (tC4's chapter or verse string, compared as a string), and
//! [`CheckTarget::Frame`] holds the story and frame numbers that `CheckTarget::Frame`
//! holds as `StoryNo` and `FrameNo`.

use std::fmt;

use serde_json::Value;
use tc_core::BookId;
use tc_helps::ObsRef;
use unicode_normalization::is_nfc;

use crate::Error;

/// The tool a check belongs to. Only Translation Words and Translation Notes derive
/// check lists; Translation Questions is read-only help, but tC3 and tC4 files can hold
/// decisions for it, so the identity carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ToolId {
    TranslationWords,
    TranslationNotes,
    TranslationQuestions,
}

impl ToolId {
    /// The tool's name in tC4 files: `translationWords`, `translationNotes`,
    /// `translationQuestions`.
    pub fn name(self) -> &'static str {
        match self {
            Self::TranslationWords => "translationWords",
            Self::TranslationNotes => "translationNotes",
            Self::TranslationQuestions => "translationQuestions",
        }
    }

    pub fn parse(name: &str) -> Result<Self, Error> {
        match name {
            "translationWords" => Ok(Self::TranslationWords),
            "translationNotes" => Ok(Self::TranslationNotes),
            "translationQuestions" => Ok(Self::TranslationQuestions),
            other => Err(Error::Tool(other.to_owned())),
        }
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The resource's id for a check: the TSV `ID` column, such as `rtc9`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckId(Box<str>);

impl CheckId {
    /// Accepts non-empty NFC text without `|` or `:`, as the journal does.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let ok = !text.is_empty() && !text.contains(['|', ':']) && is_nfc(text);
        ok.then(|| Self(text.into()))
            .ok_or_else(|| Error::CheckId(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CheckId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A chapter or verse of a check reference as tC4 writes it: `1`, `9-10`, `front`,
/// `intro`, `3,5`. Two parts are equal when their text is equal.
///
/// tC4 reads a TSV reference part with `refPart`: plain digits become a number and
/// everything else stays text. [`RefPart::from_cell`] gives the text of that value, so
/// `05` becomes `5`. In a tC4 file the part is a JSON number exactly when the text is
/// digits ([`RefPart::to_json`]).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RefPart(Box<str>);

impl RefPart {
    /// Accepts any non-empty text without `:`, kept as written.
    pub fn new(text: &str) -> Result<Self, Error> {
        (!text.is_empty() && !text.contains(':'))
            .then(|| Self(text.into()))
            .ok_or_else(|| Error::RefPart(text.to_owned()))
    }

    pub fn number(n: u16) -> Self {
        Self(n.to_string().into())
    }

    /// tC4's `refPart`: plain digits are read as a number, so leading zeros go.
    pub fn from_cell(cell: &str) -> Result<Self, Error> {
        if !cell.is_empty() && cell.bytes().all(|b| b.is_ascii_digit()) {
            let trimmed = cell.trim_start_matches('0');
            return Ok(Self(if trimmed.is_empty() { "0" } else { trimmed }.into()));
        }
        Self::new(cell)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The part as a tC4 file stores it: a number for digits, a string otherwise.
    pub fn to_json(&self) -> Value {
        if self.0.bytes().all(|b| b.is_ascii_digit())
            && let Ok(n) = self.0.parse::<u64>()
        {
            return Value::from(n);
        }
        Value::from(self.as_str())
    }

    /// The part as `tc_versification` reads it for mapping.
    pub fn to_versification(&self) -> tc_versification::RefPart {
        tc_versification::RefPart::parse(&self.0)
    }
}

impl fmt::Display for RefPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&tc_versification::RefPart> for RefPart {
    fn from(part: &tc_versification::RefPart) -> Self {
        Self(part.to_string().into())
    }
}

/// Where a check is: a verse of a book, or a frame of an Open Bible Stories story.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckTarget {
    Verse {
        book: BookId,
        chapter: RefPart,
        verse: RefPart,
    },
    Frame(ObsRef),
}

impl CheckTarget {
    /// The two locator parts the screens show and sort by: chapter and verse, or story
    /// and frame (tC4 `referenceParts`).
    pub fn parts(&self) -> (String, String) {
        match self {
            Self::Verse { chapter, verse, .. } => (chapter.to_string(), verse.to_string()),
            Self::Frame(r) => (r.story().to_string(), r.frame().to_string()),
        }
    }

    /// `c:v`, or `story:frame` (tC4 `locatorOf`).
    pub fn locator(&self) -> String {
        let (a, b) = self.parts();
        format!("{a}:{b}")
    }

    /// The collection the target is in.
    pub fn collection(&self) -> Collection {
        match self {
            Self::Verse { book, .. } => Collection::Book(*book),
            Self::Frame(_) => Collection::Stories,
        }
    }

    /// tC4's `reference` object: `{bookId, chapter, verse}` with the book in lower case,
    /// or `{story, frame}`.
    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        match self {
            Self::Verse {
                book,
                chapter,
                verse,
            } => {
                map.insert("bookId".into(), book.code().to_ascii_lowercase().into());
                map.insert("chapter".into(), chapter.to_json());
                map.insert("verse".into(), verse.to_json());
            }
            Self::Frame(r) => {
                map.insert("story".into(), r.story().into());
                map.insert("frame".into(), r.frame().into());
            }
        }
        Value::Object(map)
    }
}

impl fmt::Display for CheckTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verse { book, .. } => write!(f, "{book} {}", self.locator()),
            Self::Frame(r) => write!(f, "OBS {r}"),
        }
    }
}

/// tC4's decision identity: tool, check id, place and occurrence. The quote is not part
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CheckKey {
    pub tool: ToolId,
    pub check_id: CheckId,
    pub target: CheckTarget,
    pub occurrence: u32,
}

impl fmt::Display for CheckKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {} #{}",
            self.tool, self.target, self.check_id, self.occurrence
        )
    }
}

/// What a check list and a decisions file cover: one book, or the fifty stories of Open
/// Bible Stories. With a tool, it is tC4's resolution unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Collection {
    Book(BookId),
    Stories,
}

impl fmt::Display for Collection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Book(book) => write!(f, "{book}"),
            Self::Stories => f.write_str("OBS"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_ids_follow_the_journal_rule() {
        assert!(CheckId::parse("rtc9").is_ok());
        for bad in ["", "a|b", "a:b", "e\u{301}"] {
            assert!(CheckId::parse(bad).is_err(), "{bad:?}");
        }
        assert!(CheckId::parse("\u{e9}").is_ok());
    }

    #[test]
    fn ref_parts_read_as_tc4_ref_part_reads_them() {
        let cell = |s: &str| RefPart::from_cell(s).unwrap();
        assert_eq!(cell("05").as_str(), "5");
        assert_eq!(cell("0").as_str(), "0");
        assert_eq!(cell("9-10").as_str(), "9-10");
        assert_eq!(cell("front").as_str(), "front");
        assert_eq!(cell("12").to_json(), Value::from(12));
        assert_eq!(cell("9-10").to_json(), Value::from("9-10"));
        assert!(RefPart::from_cell("").is_err());
        assert!(RefPart::new("1:2").is_err());
    }

    #[test]
    fn targets_print_as_tc4_writes_them() {
        let target = CheckTarget::Verse {
            book: BookId::Titus,
            chapter: RefPart::number(1),
            verse: RefPart::new("9-10").unwrap(),
        };
        assert_eq!(target.locator(), "1:9-10");
        assert_eq!(
            target.to_json().to_string(),
            r#"{"bookId":"tit","chapter":1,"verse":"9-10"}"#
        );
        let frame = CheckTarget::Frame(ObsRef::new(1, 0).unwrap());
        assert_eq!(frame.to_json().to_string(), r#"{"story":1,"frame":0}"#);
        assert_eq!(frame.locator(), "1:0");
    }
}
