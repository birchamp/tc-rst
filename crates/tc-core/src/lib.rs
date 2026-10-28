//! Identifiers shared by every crate: books, chapter, verse, story and frame numbers, verse
//! spans and references, and the two digests every layer records (a git object id and an
//! MD5).

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

mod books;
mod digest;
mod ids;
pub mod js;
mod path;
mod zipdir;

pub use books::{BookId, Testament};
pub use digest::{DigestError, GitOid, Md5, Md5Stream};
pub use ids::{ChapterNo, FrameNo, NotANumber, NotAStory, StoryNo, VerseNo, digits};
pub use path::{PathProblem, check_path};
pub use zipdir::central_directory_records;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unknown book code {0:?}")]
    UnknownBook(String),
    #[error("invalid verse label {0:?}")]
    InvalidVerse(String),
    #[error("invalid reference {0:?}")]
    InvalidReference(String),
}

/// A verse or a span of verses, such as `3` or `1-2`. A span covers `start..=end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseSpan {
    start: u16,
    end: u16,
}

impl VerseSpan {
    pub fn single(verse: VerseNo) -> Self {
        Self {
            start: verse.get(),
            end: verse.get(),
        }
    }

    /// Returns `None` when `end < start` or `start` is zero.
    pub fn range(start: u16, end: u16) -> Option<Self> {
        (start >= 1 && end >= start).then_some(Self { start, end })
    }

    pub fn start(self) -> u16 {
        self.start
    }

    pub fn end(self) -> u16 {
        self.end
    }

    pub fn is_span(self) -> bool {
        self.end > self.start
    }

    pub fn contains(self, verse: u16) -> bool {
        (self.start..=self.end).contains(&verse)
    }

    pub fn verses(self) -> impl Iterator<Item = u16> {
        self.start..=self.end
    }
}

impl fmt::Display for VerseSpan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_span() {
            write!(f, "{}-{}", self.start, self.end)
        } else {
            write!(f, "{}", self.start)
        }
    }
}

impl FromStr for VerseSpan {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ParseError::InvalidVerse(s.to_owned());
        let number = |part: &str| digits(part).ok_or_else(bad);
        match s.split_once('-') {
            None => {
                let v = number(s)?;
                Self::range(v, v).ok_or_else(bad)
            }
            Some((a, b)) => Self::range(number(a)?, number(b)?).ok_or_else(bad),
        }
    }
}

impl Serialize for VerseSpan {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for VerseSpan {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A book, chapter and verse span, written `TIT 1:2` or `TIT 1:1-2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reference {
    book: BookId,
    chapter: ChapterNo,
    verses: VerseSpan,
}

impl Reference {
    pub fn new(book: BookId, chapter: ChapterNo, verses: VerseSpan) -> Self {
        Self {
            book,
            chapter,
            verses,
        }
    }

    pub fn book(&self) -> BookId {
        self.book
    }

    pub fn chapter(&self) -> ChapterNo {
        self.chapter
    }

    pub fn verses(&self) -> VerseSpan {
        self.verses
    }
}

impl fmt::Display for Reference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.verses)
    }
}

impl FromStr for Reference {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ParseError::InvalidReference(s.to_owned());
        let (book, rest) = s.split_once(' ').ok_or_else(bad)?;
        let (chapter, verses) = rest.split_once(':').ok_or_else(bad)?;
        let chapter: ChapterNo = chapter.parse().map_err(|_| bad())?;
        Ok(Self {
            book: book.parse()?,
            chapter,
            verses: verses.parse()?,
        })
    }
}

impl Serialize for Reference {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Reference {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verse_spans_parse_and_print() {
        assert_eq!(
            "3".parse::<VerseSpan>().unwrap(),
            VerseSpan::single(VerseNo::new(3).unwrap())
        );
        let span: VerseSpan = "9-10".parse().unwrap();
        assert_eq!((span.start(), span.end()), (9, 10));
        assert_eq!(span.to_string(), "9-10");
        for bad in ["", "0", "2-1", "a", "1-", "-1", "1-2-3", "+1", " 1"] {
            assert!(
                bad.parse::<VerseSpan>().is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn chapter_and_verse_numbers_are_plain_nonzero_digits() {
        for (text, n) in [("1", 1), ("007", 7), ("65535", 65535)] {
            assert_eq!(text.parse::<ChapterNo>().map(ChapterNo::get), Ok(n));
            assert_eq!(text.parse::<VerseNo>().map(VerseNo::get), Ok(n));
        }
        for bad in [
            "", "0", "000", "+1", "-1", " 1", "1 ", "1a", "65536", "\u{661}",
        ] {
            assert_eq!(bad.parse::<ChapterNo>(), Err(NotANumber), "{bad:?}");
            assert_eq!(bad.parse::<VerseNo>(), Err(NotANumber), "{bad:?}");
        }
        assert_eq!(digits("000"), Some(0));
        assert_eq!(digits("0065535"), Some(65535));
    }

    #[test]
    fn references_round_trip_through_text_and_json() {
        for text in ["TIT 1:1", "JON 2:9-10", "3JN 1:15"] {
            let r: Reference = text.parse().unwrap();
            assert_eq!(r.to_string(), text);
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(serde_json::from_str::<Reference>(&json).unwrap(), r);
        }
        assert_eq!(
            "TIT 01:1".parse::<Reference>().unwrap().to_string(),
            "TIT 1:1"
        );
        for bad in ["TIT 0:1", "TIT +1:1", "TIT  1:1", "TIT 65536:1", "TIT :1"] {
            assert!(bad.parse::<Reference>().is_err(), "{bad:?}");
        }
        assert!("XYZ 1:1".parse::<Reference>().is_err());
    }
}
