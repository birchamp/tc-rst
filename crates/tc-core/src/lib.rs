//! Identifiers shared by every crate: books, chapters, verse spans and references.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

mod books;

pub use books::{BookId, Testament};

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
    pub fn single(verse: u16) -> Self {
        Self {
            start: verse,
            end: verse,
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
        let number = |part: &str| -> Result<u16, ParseError> {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
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
    pub book: BookId,
    pub chapter: u16,
    pub verses: VerseSpan,
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
        let chapter: u16 = chapter.parse().map_err(|_| bad())?;
        if chapter == 0 {
            return Err(bad());
        }
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
        assert_eq!("3".parse::<VerseSpan>().unwrap(), VerseSpan::single(3));
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
    fn references_round_trip_through_text_and_json() {
        for text in ["TIT 1:1", "JON 2:9-10", "3JN 1:15"] {
            let r: Reference = text.parse().unwrap();
            assert_eq!(r.to_string(), text);
            let json = serde_json::to_string(&r).unwrap();
            assert_eq!(serde_json::from_str::<Reference>(&json).unwrap(), r);
        }
        assert!("TIT 0:1".parse::<Reference>().is_err());
        assert!("XYZ 1:1".parse::<Reference>().is_err());
    }
}
