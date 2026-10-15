use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use tc_core::BookId;

/// A three-character book code as versification data writes it, such as `GEN`, `TOB` or
/// `S3Y`.
///
/// Schemes name books outside the 66-book canon that `tc_core::BookId` covers (the `eng`
/// scheme lists 92), so this crate keeps its own code type. A code is three characters
/// from `A-Z` and `0-9`, the grammar tC4 validates `mappedVerses` keys with.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BookCode([u8; 3]);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid book code {0:?}: expected three characters from A-Z and 0-9")]
pub struct InvalidBookCode(pub String);

impl BookCode {
    pub fn new(code: &str) -> Result<Self, InvalidBookCode> {
        match code.as_bytes() {
            &[a, b, c]
                if [a, b, c]
                    .iter()
                    .all(|x| x.is_ascii_uppercase() || x.is_ascii_digit()) =>
            {
                Ok(Self([a, b, c]))
            }
            _ => Err(InvalidBookCode(code.to_owned())),
        }
    }

    pub fn as_str(&self) -> &str {
        // `new` admits ASCII bytes only, so the bytes are always valid UTF-8.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    /// The canonical book this code names, or `None` for a book outside the 66.
    pub fn book_id(self) -> Option<BookId> {
        self.as_str().parse().ok()
    }

    /// The position of this code in proskomma-core's book list, which Proskomma uses to
    /// encode a mapping that crosses into another book. A code missing from the list
    /// cannot be the target of such a mapping.
    pub(crate) fn proskomma_index(self) -> Option<usize> {
        PROSKOMMA_BOOK_CODES
            .iter()
            .position(|c| c.as_bytes() == self.0)
    }
}

/// `bookCodes` in proskomma-core 0.11.3 `src/util/versification.js`, in order.
const PROSKOMMA_BOOK_CODES: [&str; 107] = [
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

impl From<BookId> for BookCode {
    fn from(book: BookId) -> Self {
        // tc-core's codes are a const table of three-letter ASCII codes; the test
        // `canon_books_convert_both_ways` checks every one.
        let b = book.code().as_bytes();
        Self([b[0], b[1], b[2]])
    }
}

impl fmt::Display for BookCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for BookCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BookCode({})", self.as_str())
    }
}

impl FromStr for BookCode {
    type Err = InvalidBookCode;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for BookCode {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BookCode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_parse_strictly() {
        for good in ["GEN", "S3Y", "1MA", "5EZ"] {
            assert_eq!(BookCode::new(good).unwrap().as_str(), good);
        }
        for bad in ["gen", "GE", "GENE", "G-N", "", "ÉXO"] {
            assert!(BookCode::new(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn canon_books_convert_both_ways() {
        for book in BookId::ALL {
            let code = BookCode::from(book);
            assert_eq!(code.as_str(), book.code());
            assert_eq!(code.book_id(), Some(book));
        }
        assert_eq!(BookCode::new("TOB").unwrap().book_id(), None);
    }

    #[test]
    fn proskomma_list_covers_the_canon() {
        for (i, book) in BookId::ALL.into_iter().enumerate() {
            assert_eq!(BookCode::from(book).proskomma_index(), Some(i));
        }
        assert_eq!(BookCode::new("LAO").unwrap().proskomma_index(), Some(106));
        assert_eq!(BookCode::new("XXA").unwrap().proskomma_index(), None);
    }
}
