//! Verse references and the verse ranges that `mappedVerses` entries are written in.

use crate::book::BookCode;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// A verse in a versification scheme, written `PSA 3:1`.
///
/// Verse 0 is legal: `org`, `lxx` and `vul` number some psalm titles as verse 0, and the
/// mapping arithmetic can land on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseRef {
    pub book: BookCode,
    pub chapter: u16,
    pub verse: u16,
}

impl VerseRef {
    pub fn new(book: BookCode, chapter: u16, verse: u16) -> Self {
        Self {
            book,
            chapter,
            verse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid verse reference {0:?}: expected `BOOK chapter:verse`")]
pub struct InvalidVerseRef(pub String);

impl fmt::Display for VerseRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.verse)
    }
}

impl FromStr for VerseRef {
    type Err = InvalidVerseRef;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || InvalidVerseRef(s.to_owned());
        let (book, rest) = s.split_once(' ').ok_or_else(bad)?;
        let (chapter, verse) = rest.split_once(':').ok_or_else(bad)?;
        Ok(Self {
            book: book.parse().map_err(|_| bad())?,
            chapter: number(chapter).ok_or_else(bad)?,
            verse: number(verse).ok_or_else(bad)?,
        })
    }
}

impl Serialize for VerseRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for VerseRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A sub-verse letter, the `a` in `ESG 1:1a`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Part(u8);

impl Part {
    /// Accepts `a` to `z`.
    pub fn new(letter: char) -> Option<Self> {
        letter.is_ascii_lowercase().then_some(Self(letter as u8))
    }

    pub fn letter(self) -> char {
        char::from(self.0)
    }
}

/// A run of verses in one chapter, as a `mappedVerses` key or target writes it:
/// `GEN 32:1-32`, `PSA 51:0`, `ESG 1:1a`.
///
/// `last` is kept as written. It can be smaller than `first`: the `vul` scheme maps
/// `DAG 3:52-23`. Such a range contains no verse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MappedRange {
    pub book: BookCode,
    pub chapter: u16,
    pub first: u16,
    /// Only a mapping target carries one.
    pub part: Option<Part>,
    /// The verse after the dash, when the range has one.
    pub last: Option<u16>,
}

impl MappedRange {
    pub fn verse(book: BookCode, chapter: u16, verse: u16) -> Self {
        Self {
            book,
            chapter,
            first: verse,
            part: None,
            last: None,
        }
    }

    pub fn span(book: BookCode, chapter: u16, first: u16, last: u16) -> Self {
        Self {
            book,
            chapter,
            first,
            part: None,
            last: Some(last),
        }
    }

    /// The last verse of the run: `last`, or `first` when there is no dash.
    pub fn end(&self) -> u16 {
        self.last.unwrap_or(self.first)
    }

    pub fn contains(&self, verse: VerseRef) -> bool {
        verse.book == self.book
            && verse.chapter == self.chapter
            && self.first <= verse.verse
            && verse.verse <= self.end()
    }

    /// `end - first`, negative for a reversed range.
    pub(crate) fn extent(&self) -> i32 {
        i32::from(self.end()) - i32::from(self.first)
    }

    /// Parses a `mappedVerses` key: `BOOK c:v` or `BOOK c:v-v`.
    pub fn parse_source(s: &str) -> Option<Self> {
        Self::parse(s).filter(|r| r.part.is_none())
    }

    /// Parses a `mappedVerses` target, which may also carry a sub-verse letter:
    /// `BOOK c:v`, `BOOK c:va`, `BOOK c:v-v` or `BOOK c:va-v`.
    pub fn parse_target(s: &str) -> Option<Self> {
        Self::parse(s)
    }

    fn parse(s: &str) -> Option<Self> {
        let (book, rest) = s.split_once(' ')?;
        let (chapter, verses) = rest.split_once(':')?;
        let (head, last) = match verses.split_once('-') {
            Some((head, last)) => (head, Some(number(last)?)),
            None => (verses, None),
        };
        let (first, part) = match head.as_bytes().last() {
            Some(b) if b.is_ascii_lowercase() => {
                (&head[..head.len() - 1], Some(Part::new(char::from(*b))?))
            }
            _ => (head, None),
        };
        Some(Self {
            book: BookCode::new(book).ok()?,
            chapter: number(chapter)?,
            first: number(first)?,
            part,
            last,
        })
    }
}

impl fmt::Display for MappedRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.first)?;
        if let Some(part) = self.part {
            write!(f, "{}", part.letter())?;
        }
        if let Some(last) = self.last {
            write!(f, "-{last}")?;
        }
        Ok(())
    }
}

/// One `mappedVerses` entry: verses of a scheme, and the verses they are in the scheme it
/// maps into. In a scheme file that is always `org`; in a reversed table it is the
/// other way round.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Mapping {
    source: MappedRange,
    targets: Vec<MappedRange>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidMapping {
    #[error("mapping {0} has no target")]
    NoTarget(MappedRange),
}

impl Mapping {
    pub fn new(source: MappedRange, targets: Vec<MappedRange>) -> Result<Self, InvalidMapping> {
        if targets.is_empty() {
            return Err(InvalidMapping::NoTarget(source));
        }
        Ok(Self { source, targets })
    }

    pub fn source(&self) -> &MappedRange {
        &self.source
    }

    /// Never empty. One target is the usual case; several is the one-to-many form.
    pub fn targets(&self) -> &[MappedRange] {
        &self.targets
    }

    pub(crate) fn single(source: MappedRange, target: MappedRange) -> Self {
        Self {
            source,
            targets: vec![target],
        }
    }

    pub(crate) fn push_target(&mut self, target: MappedRange) {
        self.targets.push(target);
    }
}

impl fmt::Display for Mapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} =", self.source)?;
        for (i, t) in self.targets.iter().enumerate() {
            write!(f, "{} {t}", if i == 0 { "" } else { "," })?;
        }
        Ok(())
    }
}

/// A decimal number as the formats write it: digits only, no leading zero except `0`
/// itself, at most 65535. Refusing leading zeros keeps one spelling per value, which
/// matters because Proskomma keys its reverse table by the exact target text.
pub(crate) fn number(s: &str) -> Option<u16> {
    let canonical = match s.as_bytes() {
        [] => false,
        [b'0'] => true,
        [b'0', ..] => false,
        bytes => bytes.iter().all(u8::is_ascii_digit),
    };
    if canonical { s.parse().ok() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_parse_and_print_as_written() {
        for text in ["GEN 32:1-32", "PSA 51:0", "S3Y 1:30-31", "DAG 3:52-23"] {
            let r = MappedRange::parse_source(text).unwrap();
            assert_eq!(r.to_string(), text);
        }
        for text in ["ESG 1:1a", "NUM 25:19b", "GEN 1:1a-3"] {
            assert!(MappedRange::parse_source(text).is_none());
            assert_eq!(MappedRange::parse_target(text).unwrap().to_string(), text);
        }
        let r = MappedRange::parse_target("GEN 32:2-33").unwrap();
        assert_eq!((r.first, r.end()), (2, 33));
        for bad in [
            "GEN 1:01",
            "GEN 01:1",
            "GEN  1:1",
            "GEN 1:1 ",
            "gen 1:1",
            "GEN 1",
            "GEN 1:",
            "GEN 1:1-",
            "GEN 1:1A",
            "GEN 1:1-2a",
            "GEN 1:70000",
            "GEN 1:1-2-3",
        ] {
            assert!(
                MappedRange::parse_target(bad).is_none(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn a_reversed_range_contains_nothing() {
        let r = MappedRange::parse_source("DAG 3:52-23").unwrap();
        for v in 0..60 {
            assert!(!r.contains(VerseRef::new(r.book, 3, v)));
        }
        assert_eq!(r.extent(), -29);
    }

    #[test]
    fn verse_refs_round_trip() {
        let v: VerseRef = "PSA 51:0".parse().unwrap();
        assert_eq!(v.to_string(), "PSA 51:0");
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"PSA 51:0\"");
        assert!("PSA 51:01".parse::<VerseRef>().is_err());
    }
}
