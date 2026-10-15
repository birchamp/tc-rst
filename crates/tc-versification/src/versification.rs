//! The typed versification: verse counts per chapter and the mapping table into `org`.

use crate::book::BookCode;
use crate::spec::{
    InvalidMapping, MappedRange, Mapping, UnappliedForm, UnappliedMapping, VerseRef, digits, number,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

/// The most chapters a `.vrs` file may skip, over all its book lines. `vul.vrs` skips
/// 10 (`6EZ 1:63 12:78`). A skipped chapter still takes memory, so without a bound a
/// line as short as `AAA 65535:1` would claim 65,534 of them.
pub const MAX_SKIPPED_CHAPTERS: usize = 1000;

/// A versification scheme as a `vrs.json` or Paratext `.vrs` file describes it.
///
/// Books, chapters and mappings keep the order of the file. Each mapping source appears
/// once: a `.vrs` file that lists one source on several lines gives one mapping with
/// several targets, as Proskomma's `vrs2json` does. There is always at least one book.
///
/// A value read from `vrs.json` also keeps what that document held beyond the tables
/// this crate reads (unknown keys, and which optional tables were absent or `null`), so
/// `to_json` writes it back. Equality compares that too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versification {
    books: Vec<(BookCode, Vec<u16>)>,
    book_index: HashMap<BookCode, usize>,
    mappings: Vec<Mapping>,
    unapplied: Vec<UnappliedMapping>,
    excluded_verses: Vec<VerseRef>,
    partial_verses: Vec<(VerseRef, Vec<String>)>,
    layout: JsonLayout,
}

/// What a `vrs.json` document held beyond the tables this crate reads.
#[derive(Debug, Clone, PartialEq, Eq)]
struct JsonLayout {
    /// Top-level keys this crate does not read (`basedOn`, `mergedVerses`, ...).
    extra: Map<String, Value>,
    mapped: Table,
    excluded: Table,
    partial: Table,
}

impl JsonLayout {
    /// The platform's layout: all four tables, nothing else.
    fn standard() -> Self {
        Self {
            extra: Map::new(),
            mapped: Table::Present,
            excluded: Table::Present,
            partial: Table::Present,
        }
    }
}

/// How a `vrs.json` document wrote one of its optional tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Absent,
    Null,
    Present,
}

impl Table {
    fn of(value: Option<&Value>) -> Self {
        match value {
            None => Self::Absent,
            Some(Value::Null) => Self::Null,
            Some(_) => Self::Present,
        }
    }

    /// Writes the table under `key`. A table with entries is always written.
    fn write(self, root: &mut Map<String, Value>, key: &str, value: Value, empty: bool) {
        match (self, empty) {
            (Self::Absent, true) => {}
            (Self::Null, true) => {
                root.insert(key.to_owned(), Value::Null);
            }
            _ => {
                root.insert(key.to_owned(), value);
            }
        }
    }
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
    #[error("the scheme has no book")]
    NoBooks,
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

/// Why a scheme cannot be written in a format without changing what it says.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("vrs.json has no place for a mapping Proskomma does not apply: `{0}`")]
    UnappliedMapping(String),
    #[error("a .vrs file has no place for the vrs.json keys {0:?}")]
    JsonOnlyKeys(Vec<String>),
    #[error(
        "partial verse {verse}: a .vrs line cannot hold the segment {segment:?} (a comma, a line break, or white space a line loses when it is trimmed)"
    )]
    Segment { verse: VerseRef, segment: String },
}

impl Versification {
    /// Builds a scheme from verse counts (one entry per chapter, in order) and mappings.
    /// Refuses a scheme with no book, a source listed twice, and a source with a
    /// sub-verse letter (which [`crate::reverse_mappings`] can produce).
    pub fn new(
        max_verses: Vec<(BookCode, Vec<u16>)>,
        mappings: Vec<Mapping>,
    ) -> Result<Self, ParseError> {
        if max_verses.is_empty() {
            return Err(ParseError::NoBooks);
        }
        let mut v = Self::empty();
        for (book, counts) in max_verses {
            v.add_book(book, counts)?;
        }
        let mut sources = HashSet::new();
        for mapping in mappings {
            let source = *mapping.source();
            if source.part.is_some() {
                return Err(InvalidMapping::PartSource(source).into());
            }
            if !sources.insert(source) {
                return Err(ParseError::DuplicateSource(source.to_string()));
            }
            v.mappings.push(mapping);
        }
        Ok(v)
    }

    pub fn with_excluded_verses(mut self, verses: Vec<VerseRef>) -> Self {
        self.excluded_verses = verses;
        self.layout.excluded = Table::Present;
        self
    }

    /// Partial verses and their segments. A verse may appear once, since `vrs.json`
    /// keys them by verse.
    pub fn with_partial_verses(
        mut self,
        partial: Vec<(VerseRef, Vec<String>)>,
    ) -> Result<Self, ParseError> {
        let mut seen = HashSet::new();
        for (verse, _) in &partial {
            if !seen.insert(*verse) {
                return Err(ParseError::DuplicatePartialVerse(verse.to_string()));
            }
        }
        self.partial_verses = partial;
        self.layout.partial = Table::Present;
        Ok(self)
    }

    /// Mapping lines a `.vrs` file keeps from Proskomma. They are kept and written back,
    /// and never applied.
    pub fn with_unapplied_mappings(mut self, unapplied: Vec<UnappliedMapping>) -> Self {
        self.unapplied = unapplied;
        self
    }

    fn empty() -> Self {
        Self {
            books: Vec::new(),
            book_index: HashMap::new(),
            mappings: Vec::new(),
            unapplied: Vec::new(),
            excluded_verses: Vec::new(),
            partial_verses: Vec::new(),
            layout: JsonLayout::standard(),
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
    /// one-to-many form. A key that appears twice keeps the last value, as `JSON.parse`
    /// does. A leading byte order mark is skipped. Other top-level keys (`basedOn`,
    /// `mergedVerses`, ...) are kept for `to_json`.
    ///
    /// `maxVerses` counts are digit strings, as tC4 requires, and may have leading
    /// zeros. Unlike tC4 this also accepts `"0"`, a chapter with no verses, which `.vrs`
    /// files use.
    ///
    /// This is stricter than tC4 and the BURRITO-SPEC grammar in a few ways, each a
    /// refusal with a reason and never a silent change:
    ///
    /// - a count above 65535, which no verse number here can reach;
    /// - a `maxVerses` key that is not three characters from `A-Z` and `0-9`. tC4 loads
    ///   a lowercase key, but its lookups upper-case the book, so the key never matches;
    /// - a leading zero in a `mappedVerses` key or target. Proskomma keys its reversed
    ///   table by the exact target text, so `GEN 32:01` and `GEN 32:1` would be two rules
    ///   there and one here;
    /// - a `mappedVerses` key with a sub-verse letter, which neither format can write
    ///   back as a key;
    /// - an `excludedVerses` or `partialVerses` entry that is not a verse reference with
    ///   strings. tC4 never reads these two tables.
    pub fn from_json(text: &str) -> Result<Self, ParseError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let root: Value =
            serde_json::from_str(text).map_err(|e| ParseError::Json(e.to_string()))?;
        let Value::Object(root) = root else {
            return Err(ParseError::Shape("the document"));
        };
        let mut max_verses = None;
        let (mut mapped, mut excluded, mut partial) = (None, None, None);
        let mut extra = Map::new();
        for (key, value) in root {
            match key.as_str() {
                "maxVerses" => max_verses = Some(value),
                "mappedVerses" => mapped = Some(value),
                "excludedVerses" => excluded = Some(value),
                "partialVerses" => partial = Some(value),
                _ => {
                    extra.insert(key, value);
                }
            }
        }
        let mut v = Self::empty();
        v.layout = JsonLayout {
            extra,
            mapped: Table::of(mapped.as_ref()),
            excluded: Table::of(excluded.as_ref()),
            partial: Table::of(partial.as_ref()),
        };

        let Some(Value::Object(max_verses)) = max_verses else {
            return Err(ParseError::Shape("maxVerses"));
        };
        if max_verses.is_empty() {
            return Err(ParseError::NoBooks);
        }
        for (book_text, chapters) in &max_verses {
            let bad = |reason: &str| ParseError::MaxVerses {
                book: book_text.clone(),
                reason: reason.to_owned(),
            };
            let book = BookCode::new(book_text).map_err(|_| bad("not a book code"))?;
            let chapters = chapters.as_array().ok_or_else(|| bad("not an array"))?;
            let counts = chapters
                .iter()
                .map(|c| c.as_str().and_then(digits))
                .collect::<Option<Vec<u16>>>()
                .ok_or_else(|| bad("a verse count is not a digit string up to 65535"))?;
            v.add_book(book, counts)?;
        }

        match mapped {
            None | Some(Value::Null) => {}
            Some(Value::Object(table)) => v.mappings = mappings_from_json(&table)?,
            Some(_) => return Err(ParseError::Shape("mappedVerses")),
        }

        match excluded {
            None | Some(Value::Null) => {}
            Some(Value::Array(list)) => {
                for item in list {
                    let verse = item
                        .as_str()
                        .and_then(|text| text.parse().ok())
                        .ok_or_else(|| ParseError::ExcludedVerse(item.to_string()))?;
                    v.excluded_verses.push(verse);
                }
            }
            Some(_) => return Err(ParseError::Shape("excludedVerses")),
        }

        match partial {
            None | Some(Value::Null) => {}
            Some(Value::Object(table)) => {
                let mut list = Vec::with_capacity(table.len());
                for (key, parts) in table {
                    let bad = || ParseError::PartialVerse(key.clone());
                    let verse = key.parse().map_err(|_| bad())?;
                    let parts = parts
                        .as_array()
                        .and_then(|a| a.iter().map(|p| p.as_str().map(str::to_owned)).collect())
                        .ok_or_else(bad)?;
                    list.push((verse, parts));
                }
                // JSON object keys are distinct, and a verse has one spelling, so no
                // verse repeats.
                v.partial_verses = list;
            }
            Some(_) => return Err(ParseError::Shape("partialVerses")),
        }
        Ok(v)
    }

    /// Parses a Paratext `.vrs` file.
    ///
    /// Lines are trimmed. Blank lines and `#` comment lines are skipped. The other lines
    /// are:
    ///
    /// - `GEN 1:31 2:25 ...`: verse counts, chapters in rising order from 1. A chapter
    ///   the line skips has no verses (`vul.vrs` lists `6EZ 1:63 12:78`). A file may skip
    ///   at most [`MAX_SKIPPED_CHAPTERS`] chapters;
    /// - `GEN 31:55 = GEN 32:1`: a mapping, parsed as Proskomma's `vrs2json` parses it.
    ///   Repeating a source adds a target to it;
    /// - `-GEN 31:51`: an excluded verse;
    /// - `*EXO 28:29,-,a`: the segments of a partial verse;
    /// - `&ACT 19:40-41 = ACT 19:40`: a mapping Proskomma does not read, kept as an
    ///   [`UnappliedMapping`].
    ///
    /// A `#!` line is data that Paratext before 7.3 must not see; Paratext 7.3 and later
    /// read it as the rest of the line. Its excluded and partial verses are read like
    /// any others (`lxx.vrs` gives its partial verses this way). Its mappings are kept
    /// as unapplied mappings, since Proskomma skips them.
    ///
    /// Proskomma skips any other line without a word. This parser refuses it instead,
    /// so a line it cannot read is never dropped in silence.
    pub fn from_vrs(text: &str) -> Result<Self, ParseError> {
        let mut v = Self::empty();
        let mut sources: HashMap<MappedRange, usize> = HashMap::new();
        let mut partial_seen: HashSet<VerseRef> = HashSet::new();
        let mut skipped = 0usize;
        // Proskomma splits on runs of \n and \r; splitting on both and keeping the \n
        // count gives the same lines with useful line numbers.
        let lines = text
            .strip_prefix('\u{feff}')
            .unwrap_or(text)
            .split('\n')
            .enumerate()
            .flat_map(|(i, l)| l.split('\r').map(move |part| (i, part)));
        for (i, raw) in lines {
            let bad = |reason: String| ParseError::VrsLine {
                line: i + 1,
                reason,
            };
            let line = raw.trim();
            let (hidden, line) = match line.strip_prefix("#!") {
                Some(rest) => (true, rest.trim_start()),
                None if line.starts_with('#') => continue,
                None => (false, line),
            };
            if line.is_empty() || (hidden && line.starts_with('#')) {
                continue;
            }
            let parse_mapping = |text: &str| -> Result<(MappedRange, MappedRange), ParseError> {
                let (from, to) = text
                    .split_once(" = ")
                    .ok_or_else(|| bad(format!("{text:?} is not a mapping")))?;
                let source = MappedRange::parse_source(from)
                    .ok_or_else(|| bad(format!("mapping source {from:?} is not a verse range")))?;
                let target = MappedRange::parse_target(to)
                    .ok_or_else(|| bad(format!("mapping target {to:?} is not a verse range")))?;
                Ok((source, target))
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
                if !partial_seen.insert(verse) {
                    return Err(bad(format!("partial verse {verse} is listed twice")));
                }
                v.partial_verses
                    .push((verse, parts.map(str::to_owned).collect()));
            } else if let Some(rest) = line.strip_prefix('&') {
                let (source, target) = parse_mapping(rest)?;
                v.unapplied.push(UnappliedMapping::new(
                    UnappliedForm::Ampersand,
                    source,
                    target,
                )?);
            } else if line.contains(" = ") {
                let (source, target) = parse_mapping(line)?;
                if hidden {
                    v.unapplied.push(UnappliedMapping::new(
                        UnappliedForm::Hidden,
                        source,
                        target,
                    )?);
                    continue;
                }
                match sources.get(&source) {
                    Some(&at) => v.mappings[at].push_target(target),
                    None => {
                        sources.insert(source, v.mappings.len());
                        v.mappings.push(Mapping::new(source, vec![target])?);
                    }
                }
            } else if hidden {
                return Err(bad(format!(
                    "`#!` line {line:?} is not an excluded verse, a partial verse or a mapping"
                )));
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
                    let chapter = usize::from(chapter);
                    if chapter <= counts.len() {
                        return Err(bad(format!("{book} chapter {chapter} is out of order")));
                    }
                    skipped += chapter - 1 - counts.len();
                    if skipped > MAX_SKIPPED_CHAPTERS {
                        return Err(bad(format!(
                            "the file skips more than {MAX_SKIPPED_CHAPTERS} chapters"
                        )));
                    }
                    counts.resize(chapter - 1, 0);
                    counts.push(verses);
                }
                v.add_book(book, counts).map_err(|e| bad(e.to_string()))?;
            }
        }
        if v.books.is_empty() {
            return Err(ParseError::NoBooks);
        }
        Ok(v)
    }

    /// The `vrs.json` form. A mapping with one target is written as a string, as the
    /// standard files write it; one with several targets as an array. Keys this crate
    /// does not read are written back, and an optional table that was absent or `null`
    /// stays so while it is empty, so the result means what the document read meant
    /// ([`crate::canonical_scheme`] agrees). Counts are written without leading zeros.
    ///
    /// Refuses a scheme with unapplied mappings, which `vrs.json` has no place for.
    pub fn to_json(&self) -> Result<String, WriteError> {
        if let Some(m) = self.unapplied.first() {
            return Err(WriteError::UnappliedMapping(m.to_string()));
        }
        let mut root = Map::new();
        let mut max_verses = Map::new();
        for (book, counts) in &self.books {
            let counts = counts
                .iter()
                .map(|c| Value::String(c.to_string()))
                .collect();
            max_verses.insert(book.to_string(), Value::Array(counts));
        }
        root.insert("maxVerses".into(), Value::Object(max_verses));

        let mut mapped = Map::new();
        for m in &self.mappings {
            let value = match m.targets() {
                [one] => Value::String(one.to_string()),
                many => Value::Array(many.iter().map(|t| Value::String(t.to_string())).collect()),
            };
            mapped.insert(m.source().to_string(), value);
        }
        self.layout.mapped.write(
            &mut root,
            "mappedVerses",
            Value::Object(mapped),
            self.mappings.is_empty(),
        );

        let excluded = self
            .excluded_verses
            .iter()
            .map(|v| Value::String(v.to_string()))
            .collect();
        self.layout.excluded.write(
            &mut root,
            "excludedVerses",
            Value::Array(excluded),
            self.excluded_verses.is_empty(),
        );

        let mut partial = Map::new();
        for (verse, parts) in &self.partial_verses {
            let parts = parts.iter().map(|p| Value::String(p.clone())).collect();
            partial.insert(verse.to_string(), Value::Array(parts));
        }
        self.layout.partial.write(
            &mut root,
            "partialVerses",
            Value::Object(partial),
            self.partial_verses.is_empty(),
        );

        for (key, value) in &self.layout.extra {
            root.insert(key.clone(), value.clone());
        }
        Ok(Value::Object(root).to_string())
    }

    /// The Paratext `.vrs` form: verse-count lines, one line per mapping target, the
    /// unapplied mappings behind `#!`, then excluded and partial verses.
    ///
    /// Refuses what a `.vrs` file cannot hold, so that [`Versification::from_vrs`] always
    /// reads back what was written: keys from a `vrs.json` that this crate does not
    /// read, and a partial-verse segment with a comma or a line break, or a last
    /// segment that ends in white space. An absent or `null` table is written, and
    /// read back, as an empty one.
    pub fn to_vrs(&self) -> Result<String, WriteError> {
        if !self.layout.extra.is_empty() {
            return Err(WriteError::JsonOnlyKeys(
                self.layout.extra.keys().cloned().collect(),
            ));
        }
        for (verse, parts) in &self.partial_verses {
            if let Some(segment) = unwritable_segment(parts) {
                return Err(WriteError::Segment {
                    verse: *verse,
                    segment: segment.to_owned(),
                });
            }
        }
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
        for m in &self.unapplied {
            let _ = writeln!(out, "{m}");
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
        Ok(out)
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

    /// Why a verse is not in this scheme, or `None` when it is. This reads the verse
    /// counts only, as tC4 does: a verse in `excluded_verses` is still in the scheme.
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

    /// Whether the verse counts include this verse. See [`Versification::absence`].
    pub fn contains(&self, verse: VerseRef) -> bool {
        self.absence(verse).is_none()
    }

    /// The `mappedVerses` table: this scheme's verses and where they are in `org`.
    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }

    /// Mapping lines from a `.vrs` file that Proskomma does not read. Mapping never
    /// applies them.
    pub fn unapplied_mappings(&self) -> &[UnappliedMapping] {
        &self.unapplied
    }

    /// Verses the scheme numbers but a text in it leaves out.
    pub fn excluded_verses(&self) -> &[VerseRef] {
        &self.excluded_verses
    }

    /// Partial verses and their segment lists, as written.
    pub fn partial_verses(&self) -> &[(VerseRef, Vec<String>)] {
        &self.partial_verses
    }
}

/// The first segment a `.vrs` partial-verse line would not read back as written: the
/// line splits on commas and line breaks, and trimming the line removes white space at
/// the end of its last segment.
fn unwritable_segment(parts: &[String]) -> Option<&str> {
    let breaks = |p: &&String| p.contains([',', '\n', '\r']);
    let trimmed = |p: &&String| p.ends_with(char::is_whitespace);
    parts
        .iter()
        .find(breaks)
        .or_else(|| parts.last().filter(trimmed))
        .map(String::as_str)
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

    fn range(text: &str) -> MappedRange {
        MappedRange::parse_target(text).unwrap()
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
    fn json_accepts_what_tc4_accepts() {
        // tC4's isValidMaxVerses takes any digit string, so leading zeros too.
        let v = Versification::from_json(r#"{"maxVerses":{"TIT":["015","016","0"]}}"#).unwrap();
        assert_eq!(v.verse_counts(book("TIT")), Some([15, 16, 0].as_slice()));
        // A byte order mark, as from_vrs also skips one.
        let v = Versification::from_json("\u{feff}{\"maxVerses\":{\"TIT\":[\"15\"]}}").unwrap();
        assert_eq!(v.last_verse(book("TIT"), 1), Some(15));
    }

    #[test]
    fn json_refuses_malformed_tables() {
        for bad in [
            r#"{"maxVerses":{}}"#,
            r#"{"maxVerses":{"TIT":[]}}"#,
            r#"{"maxVerses":{"TIT":[15]}}"#,
            r#"{"maxVerses":{"TIT":["x"]}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1":[]}}"#,
            r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1":3}}"#,
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
    fn json_refuses_what_it_documents_as_stricter_than_tc4() {
        for (bad, why) in [
            (r#"{"maxVerses":{"TIT":["65536"]}}"#, "count above 65535"),
            (r#"{"maxVerses":{"tit":["15"]}}"#, "lowercase book key"),
            (r#"{"maxVerses":{"TITUS":["15"]}}"#, "long book key"),
            (
                r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 1:01":"TIT 1:2"}}"#,
                "leading zero in a key",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 1:1":"TIT 1:02"}}"#,
                "leading zero in a target",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"mappedVerses":{"TIT 2:1a":"TIT 1:1"}}"#,
                "sub-verse letter in a key",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"excludedVerses":[3]}"#,
                "non-string excluded verse",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"excludedVerses":["TIT 1"]}"#,
                "excluded verse that is not a verse",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"partialVerses":{"TIT 1:1":[1]}}"#,
                "non-string segment",
            ),
            (
                r#"{"maxVerses":{"TIT":["15"]},"partialVerses":{"TIT 1:1":"a"}}"#,
                "segments not in a list",
            ),
        ] {
            assert!(Versification::from_json(bad).is_err(), "{why}: {bad}");
        }
    }

    #[test]
    fn json_keeps_unknown_keys_and_absent_tables() {
        let text = r#"{"maxVerses":{"TIT":["15"]},"basedOn":"org","mergedVerses":{"TIT 1:1-2":"TIT 1:1"},"partialVerses":null}"#;
        let v = Versification::from_json(text).unwrap();
        let written: Value = serde_json::from_str(&v.to_json().unwrap()).unwrap();
        let original: Value = serde_json::from_str(text).unwrap();
        assert_eq!(written, original);
        assert_eq!(Versification::from_json(&v.to_json().unwrap()).unwrap(), v);
        // An absent table that gains entries is written.
        let v = v.with_excluded_verses(vec!["TIT 1:3".parse().unwrap()]);
        let written: Value = serde_json::from_str(&v.to_json().unwrap()).unwrap();
        assert_eq!(written["excludedVerses"], serde_json::json!(["TIT 1:3"]));
        assert_eq!(written["basedOn"], "org");
        // A .vrs file has no place for basedOn.
        assert_eq!(
            v.to_vrs(),
            Err(WriteError::JsonOnlyKeys(vec![
                "basedOn".to_owned(),
                "mergedVerses".to_owned()
            ]))
        );
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
            "# comment\nPSA 1:6 2:12\nPSA 2:0 = PSA 2:1\nPSA 2:0 = PSA 2:2\n-PSA 1:6\n*PSA 2:3,-,a\n",
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
            "PSA 1:6\n&PSA 1:1a = PSA 1:2",
            "PSA 1:6\n#! PSA 1:7",
            "PSA 1:6\n#! -PSA 1",
            "PSA 1:6\n*PSA 1:1,a\n#! *PSA 1:1,b",
            "",
            "# only a comment",
        ] {
            assert!(
                Versification::from_vrs(bad).is_err(),
                "{bad:?} should be refused"
            );
        }
    }

    #[test]
    fn vrs_reads_hidden_lines_as_data_and_never_applies_their_mappings() {
        let v = Versification::from_vrs(
            "PSA 1:6 2:12\n#! -PSA 1:6\n#!*PSA 2:3,-,a \n#! &PSA 1:1 = PSA 1:2\n&PSA 1:3 = PSA 1:4\n#! PSA 2:1 = PSA 2:2\n#!\n#! # a comment\n",
        )
        .unwrap();
        assert_eq!(v.excluded_verses(), ["PSA 1:6".parse().unwrap()]);
        assert_eq!(
            v.partial_verses(),
            [(
                "PSA 2:3".parse().unwrap(),
                vec!["-".to_owned(), "a".to_owned()]
            )]
        );
        assert_eq!(v.mappings(), []);
        let unapplied: Vec<String> = v
            .unapplied_mappings()
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(
            unapplied,
            [
                "#! &PSA 1:1 = PSA 1:2",
                "#! &PSA 1:3 = PSA 1:4",
                "#! PSA 2:1 = PSA 2:2"
            ]
        );
        // The .vrs writer keeps them; vrs.json has no place for them.
        assert_eq!(Versification::from_vrs(&v.to_vrs().unwrap()).unwrap(), v);
        assert!(matches!(
            v.to_json(),
            Err(WriteError::UnappliedMapping(line)) if line == "#! &PSA 1:1 = PSA 1:2"
        ));
    }

    #[test]
    fn vrs_bounds_the_chapters_a_file_may_skip() {
        // Chapters 2 to 1001 are skipped: exactly the bound.
        assert!(Versification::from_vrs("PSA 1:6 1002:1").is_ok());
        let err = Versification::from_vrs("PSA 1:6 1003:1").unwrap_err();
        assert!(err.to_string().contains("skips more than"), "{err}");
        // The bound is over the whole file, not each line.
        let many: String = (0..3).map(|i| format!("AB{i} 400:1\n")).collect();
        assert!(Versification::from_vrs(&many).is_err());
        assert!(Versification::from_vrs("AAA 65535:1").is_err());
    }

    #[test]
    fn vrs_writer_refuses_segments_it_cannot_write() {
        let v = Versification::new(vec![(book("GEN"), vec![31, 25])], vec![]).unwrap();
        let verse: VerseRef = "GEN 1:1".parse().unwrap();
        for segment in ["a\nGEN 1:2 = GEN 2:3", "a,b", "a ", "a\r", " "] {
            let with = v
                .clone()
                .with_partial_verses(vec![(verse, vec![segment.to_owned()])])
                .unwrap();
            assert_eq!(
                with.to_vrs(),
                Err(WriteError::Segment {
                    verse,
                    segment: segment.to_owned()
                }),
                "{segment:?}"
            );
        }
        // Leading spaces, and trailing ones before the last segment, survive a line.
        let fine = vec![" a".to_owned(), "b ".to_owned(), String::new()];
        let with = v.with_partial_verses(vec![(verse, fine)]).unwrap();
        assert_eq!(
            Versification::from_vrs(&with.to_vrs().unwrap()).unwrap(),
            with
        );
    }

    #[test]
    fn constructors_refuse_what_the_writers_cannot_write() {
        assert_eq!(Versification::new(vec![], vec![]), Err(ParseError::NoBooks));
        let reversed = crate::table::reverse_mappings(&[Mapping::new(
            range("ESG 1:2"),
            vec![range("ESG 1:1b")],
        )
        .unwrap()]);
        assert_eq!(
            Versification::new(vec![(book("ESG"), vec![3])], reversed),
            Err(ParseError::Mapping(InvalidMapping::PartSource(range(
                "ESG 1:1b"
            ))))
        );
        let twice = vec![
            Mapping::new(range("ESG 1:1"), vec![range("ESG 1:2")]).unwrap(),
            Mapping::new(range("ESG 1:1"), vec![range("ESG 1:3")]).unwrap(),
        ];
        assert_eq!(
            Versification::new(vec![(book("ESG"), vec![3])], twice),
            Err(ParseError::DuplicateSource("ESG 1:1".to_owned()))
        );
        let verse: VerseRef = "ESG 1:1".parse().unwrap();
        let v = Versification::new(vec![(book("ESG"), vec![3])], vec![]).unwrap();
        assert_eq!(
            v.with_partial_verses(vec![(verse, vec![]), (verse, vec![])]),
            Err(ParseError::DuplicatePartialVerse("ESG 1:1".to_owned()))
        );
    }
}
