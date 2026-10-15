use crate::error::ValueError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;
use tc_core::BookId;

/// The book codes the SB 1.0 schema accepts as scope keys (`common.schema.json`,
/// `definitions.bookId`), in its order: the 66 books of `tc_core::BookId`, then the
/// deuterocanon and other books. A test checks this list against the embedded schema.
pub(crate) const SCOPE_BOOKS: [&str; 107] = [
    "GEN", "EXO", "LEV", "NUM", "DEU", "JOS", "JDG", "RUT", "1SA", "2SA", "1KI", "2KI", "1CH",
    "2CH", "EZR", "NEH", "EST", "JOB", "PSA", "PRO", "ECC", "SNG", "ISA", "JER", "LAM", "EZK",
    "DAN", "HOS", "JOL", "AMO", "OBA", "JON", "MIC", "NAM", "HAB", "ZEP", "HAG", "ZEC", "MAL",
    "MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP", "COL", "1TH",
    "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE", "1JN", "2JN", "3JN", "JUD",
    "REV", "TOB", "JDT", "ESG", "WIS", "SIR", "BAR", "LJE", "S3Y", "SUS", "BEL", "1MA", "2MA",
    "3MA", "4MA", "1ES", "2ES", "MAN", "PS2", "ODA", "PSS", "JSA", "JDB", "TBS", "SST", "DNT",
    "BLT", "EZA", "5EZ", "6EZ", "DAG", "PS3", "2BA", "LBA", "JUB", "ENO", "1MQ", "2MQ", "3MQ",
    "REP", "4BA", "LAO",
];

/// A book a burrito scope may name. It covers every book the schema allows, including the
/// deuterocanon, which `tc_core::BookId` does not model. Books sort in canonical order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeBook(u8);

impl ScopeBook {
    /// Accepts the exact upper-case code, as the schema does: `tit` and `FRT` are refused.
    pub fn parse(code: &str) -> Result<Self, ValueError> {
        SCOPE_BOOKS
            .iter()
            .position(|known| *known == code)
            .and_then(|index| u8::try_from(index).ok())
            .map(Self)
            .ok_or_else(|| ValueError::Book(code.to_owned()))
    }

    pub fn code(self) -> &'static str {
        // Only `parse` and `From<BookId>` make a ScopeBook, and both store a position in
        // SCOPE_BOOKS, so the index is in bounds.
        SCOPE_BOOKS[usize::from(self.0)]
    }

    /// The book as `tc_core` knows it, or `None` for a deuterocanonical book.
    pub fn book(self) -> Option<BookId> {
        BookId::from_ordinal(self.0.checked_add(1)?)
    }

    pub fn all() -> impl Iterator<Item = ScopeBook> {
        (0..SCOPE_BOOKS.len()).filter_map(|index| u8::try_from(index).ok().map(Self))
    }
}

impl From<BookId> for ScopeBook {
    fn from(book: BookId) -> Self {
        // SCOPE_BOOKS starts with the 66 books in BookId order (a test checks it), and an
        // ordinal is 1 to 66.
        Self(book.ordinal() - 1)
    }
}

impl fmt::Debug for ScopeBook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Display for ScopeBook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for ScopeBook {
    type Err = ValueError;

    fn from_str(code: &str) -> Result<Self, Self::Err> {
        Self::parse(code)
    }
}

impl Serialize for ScopeBook {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.code())
    }
}

impl<'de> Deserialize<'de> for ScopeBook {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = String::deserialize(d)?;
        Self::parse(&code).map_err(serde::de::Error::custom)
    }
}

/// One entry of a book's scope list, in the five forms the schema allows. Numbers are never
/// zero and are written without leading zeros, so text and value map one to one. Ranges
/// are kept as written; they are not checked against a versification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeRange {
    /// `3`
    Chapter(NonZeroU32),
    /// `3-5`
    Chapters(NonZeroU32, NonZeroU32),
    /// `3:1`
    Verse(NonZeroU32, NonZeroU32),
    /// `3:1-9`
    Verses(NonZeroU32, NonZeroU32, NonZeroU32),
    /// `3:1-4:2`
    Span(NonZeroU32, NonZeroU32, NonZeroU32, NonZeroU32),
}

impl FromStr for ScopeRange {
    type Err = ValueError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = || ValueError::ScopeRange(text.to_owned());
        let number = |part: &str| -> Result<NonZeroU32, ValueError> {
            let mut bytes = part.bytes();
            let leading_ok = matches!(bytes.next(), Some(b'1'..=b'9'));
            if !leading_ok || !bytes.all(|b| b.is_ascii_digit()) {
                return Err(bad());
            }
            part.parse().map_err(|_| bad())
        };
        let (start, end) = match text.split_once('-') {
            Some((start, end)) => (start, Some(end)),
            None => (text, None),
        };
        let start = match start.split_once(':') {
            Some((chapter, verse)) => (number(chapter)?, Some(number(verse)?)),
            None => (number(start)?, None),
        };
        let end = match end {
            None => None,
            Some(end) => Some(match end.split_once(':') {
                Some((chapter, verse)) => (Some(number(chapter)?), number(verse)?),
                None => (None, number(end)?),
            }),
        };
        Ok(match (start, end) {
            ((chapter, None), None) => Self::Chapter(chapter),
            ((first, None), Some((None, last))) => Self::Chapters(first, last),
            ((chapter, Some(verse)), None) => Self::Verse(chapter, verse),
            ((chapter, Some(first)), Some((None, last))) => Self::Verses(chapter, first, last),
            ((c1, Some(v1)), Some((Some(c2), v2))) => Self::Span(c1, v1, c2, v2),
            ((_, None), Some((Some(_), _))) => return Err(bad()),
        })
    }
}

impl fmt::Display for ScopeRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Chapter(c) => write!(f, "{c}"),
            Self::Chapters(a, b) => write!(f, "{a}-{b}"),
            Self::Verse(c, v) => write!(f, "{c}:{v}"),
            Self::Verses(c, a, b) => write!(f, "{c}:{a}-{b}"),
            Self::Span(c1, v1, c2, v2) => write!(f, "{c1}:{v1}-{c2}:{v2}"),
        }
    }
}

impl Serialize for ScopeRange {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ScopeRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// What a burrito or an ingredient contains, by book. An empty list means the whole book.
/// The order of a list is kept, because tC4 copies the OBS template's lists byte for byte.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Scope(BTreeMap<ScopeBook, Vec<ScopeRange>>);

impl Scope {
    pub fn new() -> Self {
        Self::default()
    }

    /// A scope of whole books.
    pub fn whole_books(books: impl IntoIterator<Item = impl Into<ScopeBook>>) -> Self {
        Self(
            books
                .into_iter()
                .map(|book| (book.into(), Vec::new()))
                .collect(),
        )
    }

    pub fn insert(&mut self, book: impl Into<ScopeBook>, ranges: Vec<ScopeRange>) {
        self.0.insert(book.into(), ranges);
    }

    pub fn remove(&mut self, book: impl Into<ScopeBook>) -> Option<Vec<ScopeRange>> {
        self.0.remove(&book.into())
    }

    pub fn get(&self, book: impl Into<ScopeBook>) -> Option<&[ScopeRange]> {
        self.0.get(&book.into()).map(Vec::as_slice)
    }

    pub fn contains_book(&self, book: impl Into<ScopeBook>) -> bool {
        self.0.contains_key(&book.into())
    }

    /// Books in canonical order.
    pub fn books(&self) -> impl Iterator<Item = ScopeBook> + '_ {
        self.0.keys().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ScopeBook, &[ScopeRange])> {
        self.0
            .iter()
            .map(|(book, ranges)| (*book, ranges.as_slice()))
    }

    /// The schema refuses an empty scope (`minProperties: 1`).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_66_scope_books_are_tc_core_books_in_order() {
        for book in BookId::ALL {
            let scope_book = ScopeBook::from(book);
            assert_eq!(scope_book.code(), book.code());
            assert_eq!(scope_book.book(), Some(book));
        }
        assert_eq!(ScopeBook::parse("TOB").unwrap().book(), None);
        assert_eq!(ScopeBook::all().count(), 107);
    }

    #[test]
    fn scope_books_are_the_schema_book_ids_in_order() {
        let bundle = crate::schema::load_bundle().unwrap();
        let book_ids = &bundle["common.schema.json"]["definitions"]["bookId"]["enum"];
        let codes: Vec<&str> = book_ids
            .as_array()
            .unwrap()
            .iter()
            .map(|code| code.as_str().unwrap())
            .collect();
        assert_eq!(codes, SCOPE_BOOKS);
    }

    #[test]
    fn scope_books_refuse_what_the_schema_refuses() {
        for bad in ["tit", "FRT", "BAK", "GLO", "INT", "XXA", "", "TITUS"] {
            assert!(ScopeBook::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scope_ranges_parse_the_five_forms() {
        for text in ["3", "44-46", "3:15", "9:1-7", "24:1-25:26", "3-1"] {
            let range: ScopeRange = text.parse().unwrap();
            assert_eq!(range.to_string(), text);
        }
        for bad in [
            "",
            "0",
            "01",
            "1:0",
            "1-",
            "-1",
            "1:1-",
            "1-2:3",
            "1:2:3",
            "1-2-3",
            "a",
            " 1",
            "+1",
            "4294967296",
        ] {
            assert!(bad.parse::<ScopeRange>().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn scopes_sort_books_canonically_and_keep_range_order() {
        let scope: Scope =
            serde_json::from_str(r#"{"MAT": ["44-46", "1"], "GEN": [], "TOB": ["2"]}"#).unwrap();
        let books: Vec<_> = scope.books().map(ScopeBook::code).collect();
        assert_eq!(books, ["GEN", "MAT", "TOB"]);
        assert_eq!(
            serde_json::to_value(&scope).unwrap()["MAT"],
            serde_json::json!(["44-46", "1"])
        );
    }
}
