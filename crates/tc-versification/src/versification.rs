//! The typed versification: verse counts per chapter and the mapping table into `org`.

use crate::book::BookCode;
use crate::spec::{InvalidMapping, MappedRange, Mapping, VerseRef, number};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Write as _;

/// A versification scheme as a `vrs.json` or Paratext `.vrs` file describes it.
///
/// Books, chapters and mappings keep the order of the file. Each mapping source appears
/// once: a `.vrs` file that lists one source on several lines gives one mapping with
/// several targets, as Proskomma's `vrs2json` does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versification {
    books: Vec<(BookCode, Vec<u16>)>,
    book_index: HashMap<BookCode, usize>,
    mappings: Vec<Mapping>,
    excluded_verses: Vec<VerseRef>,
    partial_verses: Vec<(VerseRef, Vec<String>)>,
}

/// Why a verse is not in a scheme. The checks run in this order, as tC4's
/// `unplaceableReason` runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Absence {
    /// Verse 0. tC4 never places a reference on verse 0, in any scheme.
    VerseZero,
    /// The scheme has no such book, or the book has no such chapter.
    NoChapter,
    /// The chapter ends before this verse.
    PastChapterEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("not valid JSON: {0}")]
    Json(String),
    #[error("{0} is missing or has the wrong type")]
    Shape(&'static str),
    #[error("maxVerses {book:?}: {reason}")]
    MaxVerses { book: String, reason: String },
    #[error("mappedVerses key {0:?} is not `BOOK c:v` or `BOOK c:v-v`")]
    MappingSource(String),
    #[error("mappedVerses {key:?}: target {target:?} is not a verse range")]
    MappingTarget { key: String, target: String },
    #[error(transparent)]
    Mapping(#[from] InvalidMapping),
    #[error("mappedVerses {0:?} appears twice")]
    DuplicateSource(String),
    #[error("excludedVerses entry {0:?} is not `BOOK c:v`")]
    ExcludedVerse(String),
    #[error("partialVerses entry {0:?} is not `BOOK c:v` with a list of strings")]
    PartialVerse(String),
    #[error("partial verse {0} is listed twice")]
    DuplicatePartialVerse(String),
    #[error("line {line}: {reason}")]
    VrsLine { line: usize, reason: String },
}

impl Versification {
    /// Builds a scheme from verse counts (one entry per chapter, in order) and mappings.
    pub fn new(
        max_verses: Vec<(BookCode, Vec<u16>)>,
        mappings: Vec<Mapping>,
    ) -> Result<Self, ParseError> {
        let mut v = Self::empty();
        for (book, counts) in max_verses {
            v.add_book(book, counts)?;
        }
        for mapping in mappings {
            if v.mappings.iter().any(|m| m.source() == mapping.source()) {
                return Err(ParseError::DuplicateSource(mapping.source().to_string()));
            }
            v.mappings.push(mapping);
        }
        Ok(v)
    }

    pub fn with_excluded_verses(mut self, verses: Vec<VerseRef>) -> Self {
        self.excluded_verses = verses;
        self
    }

    /// Partial verses and their segments. A verse may appear once, since `vrs.json`
    /// keys them by verse.
    pub fn with_partial_verses(
        mut self,
        partial: Vec<(VerseRef, Vec<String>)>,
    ) -> Result<Self, ParseError> {
        self.partial_verses.clear();
        for (verse, parts) in partial {
            self.add_partial(verse, parts)?;
        }
        Ok(self)
    }

    fn add_partial(&mut self, verse: VerseRef, parts: Vec<String>) -> Result<(), ParseError> {
        if self.partial_verses.iter().any(|(v, _)| *v == verse) {
            return Err(ParseError::DuplicatePartialVerse(verse.to_string()));
        }
        self.partial_verses.push((verse, parts));
        Ok(())
    }

    fn empty() -> Self {
        Self {
            books: Vec::new(),
            book_index: HashMap::new(),
            mappings: Vec::new(),
            excluded_verses: Vec::new(),
            partial_verses: Vec::new(),
        }
    }

    fn add_book(&mut self, book: BookCode, counts: Vec<u16>) -> Result<(), ParseError> {
        let bad = |reason: &str| ParseError::MaxVerses {
            book: book.to_string(),
            reason: reason.to_owned(),
        };
        if counts.is_empty() {
            return Err(bad("no chapters"));
        }
        if counts.len() > usize::from(u16::MAX) {
            return Err(bad("too many chapters"));
        }
        if self.book_index.contains_key(&book) {
            return Err(bad("listed twice"));
        }
        self.book_index.insert(book, self.books.len());
        self.books.push((book, counts));
        Ok(())
    }

    /// Parses the platform's `vrs.json` shape:
    /// `{"maxVerses": {"GEN": ["31", ...]}, "mappedVerses": {"GEN 31:55": "GEN 32:1"}, ...}`.
    ///
    /// `mappedVerses` values may be a string or a non-empty array of strings, the
    /// one-to-many form. `maxVerses` counts are digit strings, as tC4 requires; unlike
    /// tC4 this also accepts `"0"`, a chapter with no verses, which `.vrs` files use. A
    /// key that appears twice keeps the last value, as `JSON.parse` does. Keys this crate
    /// does not use (`basedOn`, `mergedVerses`, ...) are ignored.
    pub fn from_json(text: &str) -> Result<Self, ParseError> {
        let root: Value =
            serde_json::from_str(text).map_err(|e| ParseError::Json(e.to_string()))?;
        let root = root.as_object().ok_or(ParseError::Shape("the document"))?;
        let mut v = Self::empty();

        let max_verses = root
            .get("maxVerses")
            .and_then(Value::as_object)
            .filter(|m| !m.is_empty())
            .ok_or(ParseError::Shape("maxVerses"))?;
        for (book_text, chapters) in max_verses {
            let bad = |reason: &str| ParseError::MaxVerses {
                book: book_text.clone(),
                reason: reason.to_owned(),
            };
            let book = BookCode::new(book_text).map_err(|_| bad("not a book code"))?;
            let chapters = chapters.as_array().ok_or_else(|| bad("not an array"))?;
            let counts = chapters
                .iter()
                .map(|c| c.as_str().and_then(number))
                .collect::<Option<Vec<u16>>>()
                .ok_or_else(|| bad("a verse count is not a digit string up to 65535"))?;
            v.add_book(book, counts)?;
        }

        match root.get("mappedVerses") {
            None | Some(Value::Null) => {}
            Some(Value::Object(table)) => v.mappings = mappings_from_json(table)?,
            Some(_) => return Err(ParseError::Shape("mappedVerses")),
        }

        match root.get("excludedVerses") {
            None | Some(Value::Null) => {}
            Some(Value::Array(list)) => {
                for item in list {
                    let text = item.as_str().unwrap_or_default();
                    let verse = text
                        .parse()
                        .map_err(|_| ParseError::ExcludedVerse(item.to_string()))?;
                    v.excluded_verses.push(verse);
                }
            }
            Some(_) => return Err(ParseError::Shape("excludedVerses")),
        }

        match root.get("partialVerses") {
            None | Some(Value::Null) => {}
            Some(Value::Object(table)) => {
                for (key, parts) in table {
                    let bad = || ParseError::PartialVerse(key.clone());
                    let verse = key.parse().map_err(|_| bad())?;
                    let parts = parts
                        .as_array()
                        .and_then(|a| a.iter().map(|p| p.as_str().map(str::to_owned)).collect())
                        .ok_or_else(bad)?;
                    v.add_partial(verse, parts)?;
                }
            }
            Some(_) => return Err(ParseError::Shape("partialVerses")),
        }
        Ok(v)
    }

    /// Parses a Paratext `.vrs` file.
    ///
    /// Lines are trimmed. Blank lines and `#` lines are skipped, including Paratext's `#!`
    /// lines, which Proskomma also ignores. The other lines are:
    ///
    /// - `GEN 1:31 2:25 ...`: verse counts, chapters in rising order from 1. A chapter
    ///   the line skips has no verses (`vul.vrs` lists `6EZ 1:63 12:78`);
    /// - `GEN 31:55 = GEN 32:1`: a mapping, parsed as Proskomma's `vrs2json` parses it.
    ///   Repeating a source adds a target to it;
    /// - `-GEN 31:51`: an excluded verse;
    /// - `*EXO 28:29,-,a`: the segments of a partial verse.
    ///
    /// Proskomma skips any other line without a word. This parser refuses it instead,
    /// so a mapping it cannot read is never dropped in silence.
    pub fn from_vrs(text: &str) -> Result<Self, ParseError> {
        let mut v = Self::empty();
        let mut sources: HashMap<MappedRange, usize> = HashMap::new();
        // Proskomma splits on runs of \n and \r; splitting on both and keeping the \n
        // count gives the same lines with useful line numbers.
        let lines = text
            .strip_prefix('\u{feff}')
            .unwrap_or(text)
            .split('\n')
            .enumerate()
            .flat_map(|(i, l)| l.split('\r').map(move |part| (i, part)));
        for (i, raw) in lines {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let bad = |reason: String| ParseError::VrsLine {
                line: i + 1,
                reason,
            };
            if let Some(rest) = line.strip_prefix('-') {
                let verse = rest
                    .parse()
                    .map_err(|_| bad(format!("excluded verse {rest:?} is not `BOOK c:v`")))?;
                v.excluded_verses.push(verse);
            } else if let Some(rest) = line.strip_prefix('*') {
                let mut parts = rest.split(',');
                let verse = parts
                    .next()
                    .and_then(|r| r.parse().ok())
                    .ok_or_else(|| bad(format!("partial verse {rest:?} is not `BOOK c:v,...`")))?;
                v.add_partial(verse, parts.map(str::to_owned).collect())
                    .map_err(|e| bad(e.to_string()))?;
            } else if let Some((from, to)) = line.split_once(" = ") {
                let source = MappedRange::parse_source(from)
                    .ok_or_else(|| bad(format!("mapping source {from:?} is not a verse range")))?;
                let target = MappedRange::parse_target(to)
                    .ok_or_else(|| bad(format!("mapping target {to:?} is not a verse range")))?;
                match sources.get(&source) {
                    Some(&at) => v.mappings[at].push_target(target),
                    None => {
                        sources.insert(source, v.mappings.len());
                        v.mappings.push(Mapping::new(source, vec![target])?);
                    }
                }
            } else {
                let mut fields = line.split_ascii_whitespace();
                let book = fields
                    .next()
                    .and_then(|b| BookCode::new(b).ok())
                    .ok_or_else(|| bad(format!("unrecognized line {line:?}")))?;
                let mut counts = Vec::new();
                for field in fields {
                    let (chapter, verses) = field
                        .split_once(':')
                        .and_then(|(c, v)| Some((number(c)?, number(v)?)))
                        .ok_or_else(|| bad(format!("{field:?} is not `chapter:verses`")))?;
                    if usize::from(chapter) <= counts.len() {
                        return Err(bad(format!("{book} chapter {chapter} is out of order")));
                    }
                    // `vul.vrs` lists `6EZ 1:63 12:78`. A chapter the line skips has no
                    // verses.
                    counts.resize(usize::from(chapter) - 1, 0);
                    counts.push(verses);
                }
                v.add_book(book, counts).map_err(|e| bad(e.to_string()))?;
            }
        }
        if v.books.is_empty() {
            return Err(ParseError::Shape("verse counts (no book line)"));
        }
        Ok(v)
    }

    /// The `vrs.json` form. A mapping with one target is written as a string, as the
    /// standard files write it; one with several targets as an array.
    pub fn to_json(&self) -> String {
        let mut max_verses = Map::new();
        for (book, counts) in &self.books {
            let counts = counts
                .iter()
                .map(|c| Value::String(c.to_string()))
                .collect();
            max_verses.insert(book.to_string(), Value::Array(counts));
        }
        let mut mapped = Map::new();
        for m in &self.mappings {
            let value = match m.targets() {
                [one] => Value::String(one.to_string()),
                many => Value::Array(many.iter().map(|t| Value::String(t.to_string())).collect()),
            };
            mapped.insert(m.source().to_string(), value);
        }
        let excluded = self
            .excluded_verses
            .iter()
            .map(|v| Value::String(v.to_string()))
            .collect();
        let mut partial = Map::new();
        for (verse, parts) in &self.partial_verses {
            let parts = parts.iter().map(|p| Value::String(p.clone())).collect();
            partial.insert(verse.to_string(), Value::Array(parts));
        }
        let mut root = Map::new();
        root.insert("maxVerses".into(), Value::Object(max_verses));
        root.insert("mappedVerses".into(), Value::Object(mapped));
        root.insert("excludedVerses".into(), Value::Array(excluded));
        root.insert("partialVerses".into(), Value::Object(partial));
        Value::Object(root).to_string()
    }

    /// The Paratext `.vrs` form: verse-count lines, then one line per mapping target.
    pub fn to_vrs(&self) -> String {
        let mut out = String::new();
        for (book, counts) in &self.books {
            out.push_str(book.as_str());
            for (i, count) in counts.iter().enumerate() {
                let _ = write!(out, " {}:{count}", i + 1);
            }
            out.push('\n');
        }
        for m in &self.mappings {
            for t in m.targets() {
                let _ = writeln!(out, "{} = {t}", m.source());
            }
        }
        for verse in &self.excluded_verses {
            let _ = writeln!(out, "-{verse}");
        }
        for (verse, parts) in &self.partial_verses {
            let _ = write!(out, "*{verse}");
            for part in parts {
                let _ = write!(out, ",{part}");
            }
            out.push('\n');
        }
        out
    }

    /// Books in file order.
    pub fn books(&self) -> impl Iterator<Item = BookCode> + '_ {
        self.books.iter().map(|(book, _)| *book)
    }

    /// The verse count of every chapter of a book; chapter `n` is at index `n - 1`.
    pub fn verse_counts(&self, book: BookCode) -> Option<&[u16]> {
        self.book_index
            .get(&book)
            .map(|&i| self.books[i].1.as_slice())
    }

    /// The number of chapters in a book.
    pub fn last_chapter(&self, book: BookCode) -> Option<u16> {
        self.verse_counts(book)
            .and_then(|counts| u16::try_from(counts.len()).ok())
    }

    /// The last verse of a chapter.
    pub fn last_verse(&self, book: BookCode, chapter: u16) -> Option<u16> {
        let counts = self.verse_counts(book)?;
        let index = usize::from(chapter).checked_sub(1)?;
        counts.get(index).copied()
    }

    /// Why a verse is not in this scheme, or `None` when it is.
    pub fn absence(&self, verse: VerseRef) -> Option<Absence> {
        if verse.verse == 0 {
            return Some(Absence::VerseZero);
        }
        match self.last_verse(verse.book, verse.chapter) {
            None => Some(Absence::NoChapter),
            Some(last) if verse.verse > last => Some(Absence::PastChapterEnd),
            Some(_) => None,
        }
    }

    pub fn contains(&self, verse: VerseRef) -> bool {
        self.absence(verse).is_none()
    }

    /// The `mappedVerses` table: this scheme's verses and where they are in `org`.
    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }

    pub fn excluded_verses(&self) -> &[VerseRef] {
        &self.excluded_verses
    }

    /// Partial verses and their segment lists, as written.
    pub fn partial_verses(&self) -> &[(VerseRef, Vec<String>)] {
        &self.partial_verses
    }
}

fn mappings_from_json(table: &Map<String, Value>) -> Result<Vec<Mapping>, ParseError> {
    let mut mappings = Vec::with_capacity(table.len());
    for (key, value) in table {
        let source =
            MappedRange::parse_source(key).ok_or_else(|| ParseError::MappingSource(key.clone()))?;
        let bad_target = |target: &Value| ParseError::MappingTarget {
            key: key.clone(),
            target: target.to_string(),
        };
        let texts: Vec<&Value> = match value {
            Value::Array(list) => list.iter().collect(),
            single => vec![single],
        };
        let targets = texts
            .into_iter()
            .map(|t| {
                t.as_str()
                    .and_then(MappedRange::parse_target)
                    .ok_or_else(|| bad_target(t))
            })
            .collect::<Result<Vec<_>, _>>()?;
        mappings.push(Mapping::new(source, targets)?);
    }
    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(code: &str) -> BookCode {
        BookCode::new(code).unwrap()
    }

    #[test]
    fn json_accepts_both_value_forms() {
        let v = Versification::from_json(
            r#"{"maxVerses":{"TIT":["15","16","14"]},
                "mappedVerses":{"TIT 2:1":"TIT 1:16","TIT 2:2-16":["TIT 2:1-15","TIT 3:1"]}}"#,
        )
        .unwrap();
        assert_eq!(v.mappings().len(), 2);
        assert_eq!(v.mappings()[1].targets().len(), 2);
        assert_eq!(v.last_chapter(book("TIT")), Some(3));
        assert_eq!(v.last_verse(book("TIT"), 2), Some(16));
        assert_eq!(v.last_verse(book("TIT"), 4), None);
        assert_eq!(v.last_verse(book("TIT"), 0), None);
    }

    #[test]
    fn json_refuses_what_tc4_refuses() {
        for bad in [
            r#"{"maxVerses":{}}"#,
            r#"{"maxVerses":{"TIT":[]}}"#,
            r#"{"maxVerses":{"TIT":[15]}}"#,
            r#"{"maxVerses":{"TIT":["x"]}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1":[]}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1":3}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1a":"TIT 1:1"}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":"TIT 2:1"}"#,
            r#"[]"#,
        ] {
            assert!(
                Versification::from_json(bad).is_err(),
                "{bad} should be refused"
            );
        }
    }

    #[test]
    fn absence_follows_tc4_order() {
        let v = Versification::from_json(r#"{"maxVerses":{"TIT":["15","16","14"]}}"#).unwrap();
        let at = |b: &str, c, verse| v.absence(VerseRef::new(book(b), c, verse));
        assert_eq!(at("TIT", 1, 15), None);
        assert_eq!(at("TIT", 1, 16), Some(Absence::PastChapterEnd));
        assert_eq!(at("TIT", 4, 1), Some(Absence::NoChapter));
        assert_eq!(at("TIT", 0, 1), Some(Absence::NoChapter));
        assert_eq!(at("JON", 1, 1), Some(Absence::NoChapter));
        assert_eq!(at("JON", 1, 0), Some(Absence::VerseZero));
    }

    #[test]
    fn vrs_merges_repeated_sources_and_refuses_unknown_lines() {
        let v = Versification::from_vrs(
            "# comment\nPSA 1:6 2:12\n#! &PSA 1:1 = PSA 1:2\nPSA 2:0 = PSA 2:1\nPSA 2:0 = PSA 2:2\n-PSA 1:6\n*PSA 2:3,-,a\n",
        )
        .unwrap();
        assert_eq!(v.mappings().len(), 1);
        assert_eq!(v.mappings()[0].to_string(), "PSA 2:0 = PSA 2:1, PSA 2:2");
        assert_eq!(v.excluded_verses().len(), 1);
        assert_eq!(
            v.partial_verses()[0].1,
            vec!["-".to_owned(), "a".to_owned()]
        );
        for bad in [
            "PSA 2:6 1:12",
            "PSA 0:6",
            "PSA 1:6\nPSA 1:6",
            "PSA 1:6\nPSA 1:1 => PSA 1:2",
            "PSA 1:6\nPSA 1:1a = PSA 1:2",
            "PSA 1:6\n&PSA 1:1 = PSA 1:2",
            "",
        ] {
            assert!(
                Versification::from_vrs(bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }
}
