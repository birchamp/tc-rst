//! The journal codec: a book cut into the records of `docs/STORAGE.md` §2.4 and §13.5, and
//! put back together byte for byte.
//!
//! This cut is not the one [`crate::Book`] makes. Here a verse's content runs from right
//! after its marker to the next verse or chapter marker anywhere, or the end of the file
//! (tC4 R-8.4.1), so the paragraph marker before verse 5 ends verse 4's content. The
//! journal stores that content as `words` (the text) and `layout` (the trailing lines that
//! hold only paragraph-level markers), so a paragraph break added by one translator and a
//! rewording by another touch different fields.
//!
//! - [`split_verse`] and [`join_verse`] split a verse's content into words and layout and
//!   join them again: `join_verse(split_verse(c)) == c` for every string.
//! - [`decompose`] cuts a book into a header, chapter fronts, verse values and, when the
//!   file is not in numeric order, a layout. It refuses what the records cannot hold.
//! - [`recompose`] writes the parts back. `recompose(decompose(x)) == x` for every input
//!   [`decompose`] accepts.
//!
//! A marker here is `\v` or `\c` followed by a space, a tab, a line break or the end of the
//! text. A verse marker is `\v`, spaces or tabs, an ASCII verse number or span such as `4`
//! or `4-5` written without leading zeros, and exactly one terminator (a space, a tab, a CR
//! or an LF). A chapter marker is `\c`, spaces or tabs, an ASCII number, and a space, tab,
//! line break or the end of the text.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// The most verses one book may hold, counting each verse of a span (§13.5). Psalms, the
/// longest book, has 2,461. Each verse is a record, and a span marker of a few bytes such as
/// `\v 1-65535 ` stands for one record per verse, so without a limit a small file could
/// stand for millions of records and stop every device that reads it.
pub const MAX_BOOK_VERSES: usize = 65_535;

/// tC4's `BODY_ENDING_TAGS` (`src/data/usfm/indexer.ts`): the paragraph-level markers that
/// may stand on a layout-only line. A marker matches when its name, lower-cased and with
/// trailing digits removed (unless the name holds a `-`), is in this list.
pub const PARA_MARKERS: &[&str] = &[
    // identification and headers
    "id", "usfm", "ide", "sts", "rem", "h", "toc", "toca", //
    // introductions
    "imt", "is", "ip", "ipi", "im", "imi", "ipq", "imq", "ipr", "iq", "ib", "ili", "iot", "io",
    "ior", "iex", "imte", "ie", //
    // titles and sections
    "mt", "mte", "cl", "cd", "ms", "mr", "s", "sr", "r", "d", "sp", "sd", //
    // chapter presentation
    "cp", "pb", //
    // paragraphs
    "p", "m", "po", "pr", "cls", "pmo", "pm", "pmc", "pmr", "pi", "mi", "nb", "pc", "ph", "b",
    "lit", //
    // poetry
    "q", "qr", "qc", "qa", "qm", "qd", //
    // lists
    "lh", "li", "lf", "lim", //
    // tables
    "tr", "th", "thr", "tc", "tcr", //
    // sidebars and translator's-section milestones
    "esb", "esbe", "ts", "ts-s", "ts-e",
];

// ---------------------------------------------------------------------------------------
// Lines, layout and the verse split
// ---------------------------------------------------------------------------------------

/// The lines of `text`, each with its terminator (`\n` or `\r\n`); the last line has none
/// when the text does not end with `\n`. A lone `\r` is not a terminator.
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split_inclusive('\n')
}

/// The text of a line without its `\n` or `\r\n` terminator.
fn without_terminator(line: &str) -> &str {
    match line.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest),
        None => line,
    }
}

/// True when `name` (a marker name without the backslash) is a paragraph-level marker.
pub fn is_para_marker(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = if lower.contains('-') {
        lower.as_str()
    } else {
        lower.trim_end_matches(|c: char| c.is_ascii_digit())
    };
    PARA_MARKERS.contains(&base)
}

/// True when `line` (without its terminator) is empty or holds only spaces, tabs and
/// paragraph-level markers, each optionally closed at once with `\*` (as in `\ts\*`).
pub fn is_layout_only_line(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b' ' | b'\t' => at += 1,
            b'\\' => {
                let start = at + 1;
                let mut end = start;
                if !bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
                    return false;
                }
                while bytes
                    .get(end)
                    .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'-')
                {
                    end += 1;
                }
                if !is_para_marker(&line[start..end]) {
                    return false;
                }
                at = end;
                if bytes.get(at..at + 2) == Some(b"\\*") {
                    at += 2;
                }
                match bytes.get(at) {
                    None | Some(b' ' | b'\t' | b'\\') => {}
                    Some(_) => return false,
                }
            }
            _ => return false,
        }
    }
    true
}

/// Splits the content of one verse into `(words, layout)` (§2.4): `layout` is the trailing
/// run of layout-only lines, starting with the terminator of the last line that is not
/// layout-only; `words` is everything before it. When every line is layout-only, `words` is
/// empty.
pub fn split_verse(content: &str) -> (&str, &str) {
    let mut words_end = 0;
    let mut offset = 0;
    for line in lines(content) {
        let body = without_terminator(line);
        if !is_layout_only_line(body) {
            words_end = offset + body.len();
        }
        offset += line.len();
    }
    content.split_at(words_end)
}

/// The inverse of [`split_verse`].
pub fn join_verse(words: &str, layout: &str) -> String {
    let mut out = String::with_capacity(words.len() + layout.len());
    out.push_str(words);
    out.push_str(layout);
    out
}

/// The layout rule a stored verse value must pass (§4.5 V3): `layout` is empty, or every
/// line of it is layout-only and, when `words` is not empty, it starts with a line
/// terminator.
pub fn is_valid_layout(words: &str, layout: &str) -> bool {
    if layout.is_empty() {
        return true;
    }
    if !words.is_empty() && !(layout.starts_with('\n') || layout.starts_with("\r\n")) {
        return false;
    }
    lines(layout).all(|line| is_layout_only_line(without_terminator(line)))
}

/// True when `text` holds a verse or chapter marker: `\v` or `\c` followed by a space, a
/// tab, a line break or the end of the text.
pub fn has_verse_or_chapter_marker(text: &str) -> bool {
    marker_positions(text).next().is_some()
}

/// Positions of every `\v` or `\c` that is followed by `[ \t\r\n]` or the end.
fn marker_positions(text: &str) -> impl Iterator<Item = (usize, MarkerKind)> + '_ {
    let bytes = text.as_bytes();
    memchr_backslashes(bytes).filter_map(move |at| {
        let kind = match bytes.get(at + 1) {
            Some(b'v') => MarkerKind::Verse,
            Some(b'c') => MarkerKind::Chapter,
            _ => return None,
        };
        match bytes.get(at + 2) {
            None | Some(b' ' | b'\t' | b'\r' | b'\n') => Some((at, kind)),
            Some(_) => None,
        }
    })
}

fn memchr_backslashes(bytes: &[u8]) -> impl Iterator<Item = usize> + '_ {
    bytes
        .iter()
        .enumerate()
        .filter_map(|(at, b)| (*b == b'\\').then_some(at))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MarkerKind {
    Verse,
    Chapter,
}

// ---------------------------------------------------------------------------------------
// Marker whitespace
// ---------------------------------------------------------------------------------------

/// The character that ends a verse marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MarkerTerm {
    Space,
    Tab,
    Lf,
    Cr,
}

impl MarkerTerm {
    pub fn as_char(self) -> char {
        match self {
            Self::Space => ' ',
            Self::Tab => '\t',
            Self::Lf => '\n',
            Self::Cr => '\r',
        }
    }

    fn of(byte: u8) -> Option<Self> {
        match byte {
            b' ' => Some(Self::Space),
            b'\t' => Some(Self::Tab),
            b'\n' => Some(Self::Lf),
            b'\r' => Some(Self::Cr),
            _ => None,
        }
    }
}

/// The whitespace of a verse marker `\v<lead><label><term>`. `lead` is one or more spaces
/// and tabs. The label is not stored: it follows from the verse and the end of its span.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MarkerWs {
    lead: Box<str>,
    term: MarkerTerm,
}

impl MarkerWs {
    /// Returns `None` unless `lead` is one or more spaces and tabs.
    pub fn new(lead: &str, term: MarkerTerm) -> Option<Self> {
        let ok = !lead.is_empty() && lead.bytes().all(|b| b == b' ' || b == b'\t');
        ok.then(|| Self {
            lead: lead.into(),
            term,
        })
    }

    /// `\v 4 `: one space before the label and one after it.
    pub fn canonical() -> Self {
        Self {
            lead: " ".into(),
            term: MarkerTerm::Space,
        }
    }

    pub fn lead(&self) -> &str {
        &self.lead
    }

    pub fn term(&self) -> MarkerTerm {
        self.term
    }

    /// The whole marker for verse `first` whose span ends at `last`.
    pub fn marker(&self, first: u16, last: u16) -> String {
        let mut out = String::with_capacity(self.lead.len() + 12);
        out.push_str("\\v");
        out.push_str(&self.lead);
        out.push_str(&label(first, last));
        out.push(self.term.as_char());
        out
    }
}

/// `4`, or `4-5` for a span.
pub fn label(first: u16, last: u16) -> String {
    if last > first {
        format!("{first}-{last}")
    } else {
        first.to_string()
    }
}

// ---------------------------------------------------------------------------------------
// Book parts
// ---------------------------------------------------------------------------------------

/// A book as the journal stores it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookParts {
    /// Every byte before the first chapter marker. Verse markers before it stay here.
    pub header: String,
    pub chapters: BTreeMap<u16, ChapterParts>,
    /// The order of chapters and verses when it is not numeric.
    pub layout: Option<Vec<LayoutItem>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChapterParts {
    /// From the chapter marker to the byte before the chapter's first verse marker, or
    /// `None` when the chapter has no front (a projection then writes `\c N\n`).
    pub front: Option<String>,
    pub verses: BTreeMap<u16, VersePart>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersePart {
    /// A verse, or the first verse of a span when `last` is above its number.
    Text(VerseText),
    /// A verse inside the span that starts at `by`.
    Covered { by: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseText {
    pub marker: MarkerWs,
    pub words: String,
    pub layout: String,
    pub last: u16,
}

/// One entry of a book's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LayoutItem {
    Chapter(u16),
    Verse(u16, u16),
}

/// What [`decompose`] refuses, with the 1-based line it found it on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}: {problem}")]
pub struct DecomposeError {
    pub line: usize,
    pub problem: DecomposeProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecomposeProblem {
    #[error("the file is not valid UTF-8")]
    InvalidUtf8,
    #[error("the file holds a U+0000 character")]
    Nul,
    #[error("a chapter marker needs a number, written with the digits 0 to 9")]
    ChapterLabel,
    #[error("chapter 0 does not exist")]
    ChapterZero,
    #[error("chapter {chapter} appears twice")]
    DuplicateChapter { chapter: u16 },
    #[error(
        "a verse marker needs a verse number or span such as 4 or 4-5, then a space or a line break"
    )]
    VerseLabel,
    #[error("the verse label {label:?} has a letter or another character after the number")]
    LetterPart { label: String },
    #[error("the verse label {label:?} starts with 0")]
    LeadingZero { label: String },
    #[error("verse 0 does not exist")]
    VerseZero,
    #[error("the number {label:?} is too large")]
    NumberTooLarge { label: String },
    #[error("the span {first}-{last} does not end after it starts")]
    SpanNotAscending { first: u16, last: u16 },
    #[error("the verse marker at the end of the file has nothing after its number")]
    VerseMarkerAtEnd,
    #[error("verse {chapter}:{verse} appears twice")]
    DuplicateVerse { chapter: u16, verse: u16 },
    #[error(
        "the book holds more than 65,535 verses, counting each verse of a span; the longest book of the Bible has 2,461"
    )]
    TooManyVerses,
}

/// The 1-based line number of byte `at`.
fn line_of(text: &[u8], at: usize) -> usize {
    1 + text[..at.min(text.len())]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

/// [`decompose`] for bytes that may not be UTF-8.
pub fn decompose_bytes(bytes: &[u8]) -> Result<BookParts, DecomposeError> {
    match std::str::from_utf8(bytes) {
        Ok(text) => decompose(text),
        Err(error) => Err(DecomposeError {
            line: line_of(bytes, error.valid_up_to()),
            problem: DecomposeProblem::InvalidUtf8,
        }),
    }
}

/// A parsed marker: where it starts, where its content starts, and what it says.
struct Found {
    at: usize,
    content_start: usize,
    what: Marked,
}

enum Marked {
    Chapter(u16),
    Verse {
        first: u16,
        last: u16,
        marker: MarkerWs,
    },
}

/// Cuts a book into parts (§13.5). See the module documentation for what a marker is.
pub fn decompose(text: &str) -> Result<BookParts, DecomposeError> {
    let bytes = text.as_bytes();
    let fail = |at: usize, problem: DecomposeProblem| DecomposeError {
        line: line_of(bytes, at),
        problem,
    };
    if let Some(at) = bytes.iter().position(|b| *b == 0) {
        return Err(fail(at, DecomposeProblem::Nul));
    }

    let mut found: Vec<Found> = Vec::new();
    let mut seen_chapter = false;
    for (at, kind) in marker_positions(text) {
        match kind {
            MarkerKind::Chapter => {
                let (number, end) = chapter_marker(bytes, at).map_err(|p| fail(at, p))?;
                seen_chapter = true;
                found.push(Found {
                    at,
                    content_start: end,
                    what: Marked::Chapter(number),
                });
            }
            // A verse marker before the first chapter is header text, kept as written.
            MarkerKind::Verse if !seen_chapter => {}
            MarkerKind::Verse => {
                let (first, last, marker, end) = verse_marker(text, at).map_err(|p| fail(at, p))?;
                found.push(Found {
                    at,
                    content_start: end,
                    what: Marked::Verse {
                        first,
                        last,
                        marker,
                    },
                });
            }
        }
    }

    let header_end = found.first().map_or(text.len(), |f| f.at);
    let mut parts = BookParts {
        header: text[..header_end].to_owned(),
        ..BookParts::default()
    };
    let mut order = Vec::new();
    let mut chapter = 0u16;
    let mut verses_in_book = 0usize;
    for (index, marker) in found.iter().enumerate() {
        let end = found.get(index + 1).map_or(text.len(), |next| next.at);
        match &marker.what {
            Marked::Chapter(number) => {
                chapter = *number;
                if parts.chapters.contains_key(number) {
                    return Err(fail(
                        marker.at,
                        DecomposeProblem::DuplicateChapter { chapter: *number },
                    ));
                }
                parts.chapters.insert(
                    *number,
                    ChapterParts {
                        front: Some(text[marker.at..end].to_owned()),
                        verses: BTreeMap::new(),
                    },
                );
                order.push(LayoutItem::Chapter(*number));
            }
            Marked::Verse {
                first,
                last,
                marker: ws,
            } => {
                let Some(parts_of_chapter) = parts.chapters.get_mut(&chapter) else {
                    // Every verse marker after the header follows a chapter marker.
                    continue;
                };
                verses_in_book += usize::from(last - first) + 1;
                if verses_in_book > MAX_BOOK_VERSES {
                    return Err(fail(marker.at, DecomposeProblem::TooManyVerses));
                }
                let verses = &mut parts_of_chapter.verses;
                for verse in *first..=*last {
                    if verses.contains_key(&verse) {
                        return Err(fail(
                            marker.at,
                            DecomposeProblem::DuplicateVerse { chapter, verse },
                        ));
                    }
                }
                let (words, layout) = split_verse(&text[marker.content_start..end]);
                verses.insert(
                    *first,
                    VersePart::Text(VerseText {
                        marker: ws.clone(),
                        words: words.to_owned(),
                        layout: layout.to_owned(),
                        last: *last,
                    }),
                );
                for verse in first + 1..=*last {
                    verses.insert(verse, VersePart::Covered { by: *first });
                }
                order.push(LayoutItem::Verse(chapter, *first));
            }
        }
    }

    if !is_numeric_order(&order) {
        parts.layout = Some(order);
    }
    Ok(parts)
}

/// Chapters ascending, and each chapter's verses ascending.
fn is_numeric_order(order: &[LayoutItem]) -> bool {
    let mut last_chapter = 0u16;
    let mut last_verse = 0u16;
    for item in order {
        match *item {
            LayoutItem::Chapter(c) => {
                if c <= last_chapter {
                    return false;
                }
                last_chapter = c;
                last_verse = 0;
            }
            LayoutItem::Verse(_, v) => {
                if v <= last_verse {
                    return false;
                }
                last_verse = v;
            }
        }
    }
    true
}

/// Parses an ASCII number. Leading zeros are allowed only when `allow_leading_zero`.
fn number(label: &str, allow_leading_zero: bool) -> Result<u16, DecomposeProblem> {
    if label.len() > 1 && label.starts_with('0') && !allow_leading_zero {
        return Err(DecomposeProblem::LeadingZero {
            label: label.to_owned(),
        });
    }
    let value: u32 = label
        .bytes()
        .try_fold(0u32, |acc, b| {
            acc.checked_mul(10)?.checked_add(u32::from(b - b'0'))
        })
        .ok_or_else(|| DecomposeProblem::NumberTooLarge {
            label: label.to_owned(),
        })?;
    u16::try_from(value).map_err(|_| DecomposeProblem::NumberTooLarge {
        label: label.to_owned(),
    })
}

fn skip_blank(bytes: &[u8], mut at: usize) -> usize {
    while matches!(bytes.get(at), Some(b' ' | b'\t')) {
        at += 1;
    }
    at
}

fn skip_digits(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(u8::is_ascii_digit) {
        at += 1;
    }
    at
}

/// `\c N` at `at`: returns the number and the end of the marker (the whitespace after the
/// number stays in the chapter front).
fn chapter_marker(bytes: &[u8], at: usize) -> Result<(u16, usize), DecomposeProblem> {
    let digits = skip_blank(bytes, at + 2);
    let end = skip_digits(bytes, digits);
    if digits == at + 2 || end == digits {
        return Err(DecomposeProblem::ChapterLabel);
    }
    if !matches!(bytes.get(end), None | Some(b' ' | b'\t' | b'\r' | b'\n')) {
        return Err(DecomposeProblem::ChapterLabel);
    }
    let label =
        std::str::from_utf8(&bytes[digits..end]).map_err(|_| DecomposeProblem::ChapterLabel)?;
    match number(label, true)? {
        0 => Err(DecomposeProblem::ChapterZero),
        n => Ok((n, end)),
    }
}

/// `\v N` or `\v N-M` at `at`, with its terminator.
fn verse_marker(text: &str, at: usize) -> Result<(u16, u16, MarkerWs, usize), DecomposeProblem> {
    let bytes = text.as_bytes();
    let lead_end = skip_blank(bytes, at + 2);
    if lead_end == at + 2 {
        return Err(DecomposeProblem::VerseLabel);
    }
    let first_end = skip_digits(bytes, lead_end);
    if first_end == lead_end {
        return Err(DecomposeProblem::VerseLabel);
    }
    let mut label_end = first_end;
    let mut last_digits = None;
    if bytes.get(first_end) == Some(&b'-') {
        let end = skip_digits(bytes, first_end + 1);
        if end == first_end + 1 {
            return Err(DecomposeProblem::LetterPart {
                label: text[lead_end..end].to_owned(),
            });
        }
        last_digits = Some(first_end + 1..end);
        label_end = end;
    }
    let term = match bytes.get(label_end) {
        None => return Err(DecomposeProblem::VerseMarkerAtEnd),
        Some(b) => match MarkerTerm::of(*b) {
            Some(term) => term,
            None => {
                let rest = &text[lead_end..];
                let word_end = rest
                    .find(|c: char| c.is_whitespace() || c == '\\')
                    .unwrap_or(rest.len());
                return Err(DecomposeProblem::LetterPart {
                    label: rest[..word_end].to_owned(),
                });
            }
        },
    };
    let first = number(&text[lead_end..first_end], false)?;
    if first == 0 {
        return Err(DecomposeProblem::VerseZero);
    }
    let last = match last_digits {
        None => first,
        Some(range) => {
            let last = number(&text[range], false)?;
            if last <= first {
                return Err(DecomposeProblem::SpanNotAscending { first, last });
            }
            last
        }
    };
    let marker = MarkerWs {
        lead: text[at + 2..lead_end].into(),
        term,
    };
    Ok((first, last, marker, label_end + 1))
}

// ---------------------------------------------------------------------------------------
// Recompose
// ---------------------------------------------------------------------------------------

/// A place where the parts do not form a coherent book (§7.5). Each one is reported, and
/// [`recompose`] still writes every verse text once.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StructureIssue {
    /// Chapter `chapter` has verses but no front, so `\c N` was written for it.
    MissingChapterFront { chapter: u16 },
    /// Verse `verse` spans to `last`, but only verses up to `end` are covered by it.
    ShortSpan {
        chapter: u16,
        verse: u16,
        last: u16,
        end: u16,
    },
    /// Verse `verse` is covered by `by`, but no span starting at `by` reaches it.
    StrayCovered { chapter: u16, verse: u16, by: u16 },
}

impl fmt::Display for StructureIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingChapterFront { chapter } => {
                write!(f, "chapter {chapter} has verses but no chapter marker")
            }
            Self::ShortSpan {
                chapter,
                verse,
                last,
                end,
            } => write!(
                f,
                "verse {chapter}:{verse} spans to {last}, but only verses up to {end} belong to it"
            ),
            Self::StrayCovered { chapter, verse, by } => write!(
                f,
                "verse {chapter}:{verse} is part of the span at {by}, which does not reach it"
            ),
        }
    }
}

/// The USFM of a book and the issues found while writing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recomposed {
    pub usfm: String,
    pub issues: Vec<StructureIssue>,
}

/// Writes a book from its parts (§7.5): the header, then each chapter with a front or a
/// verse in order, its front (or `\c N` and a line break), then each verse run as
/// `\v` + lead + label + term + words + layout. A verse covered by a run is written inside
/// it; a covered verse that no run reaches writes nothing and is reported.
///
/// The order is numeric, or the layout's order with every chapter and verse the layout does
/// not list placed right after its nearest listed numeric predecessor (first when there is
/// none); layout entries with no part are skipped.
pub fn recompose(parts: &BookParts) -> Recomposed {
    let mut usfm = String::with_capacity(parts.header.len() + 64 * parts.chapters.len());
    let mut issues = Vec::new();
    usfm.push_str(&parts.header);

    let listed_chapters: Vec<u16> = parts.layout.as_ref().map_or_else(Vec::new, |layout| {
        layout
            .iter()
            .filter_map(|item| match item {
                LayoutItem::Chapter(c) => Some(*c),
                LayoutItem::Verse(..) => None,
            })
            .collect()
    });
    let present: Vec<u16> = parts
        .chapters
        .iter()
        .filter(|(_, chapter)| chapter.front.is_some() || !chapter.verses.is_empty())
        .map(|(number, _)| *number)
        .collect();
    let chapter_order = match &parts.layout {
        None => present,
        Some(_) => ordered(&present, &listed_chapters),
    };

    for number in chapter_order {
        let Some(chapter) = parts.chapters.get(&number) else {
            continue;
        };
        match &chapter.front {
            Some(front) => usfm.push_str(front),
            None => {
                usfm.push_str(&format!("\\c {number}\n"));
                issues.push(StructureIssue::MissingChapterFront { chapter: number });
            }
        }
        write_chapter(
            number,
            chapter,
            parts.layout.as_deref(),
            &mut usfm,
            &mut issues,
        );
    }
    Recomposed { usfm, issues }
}

fn write_chapter(
    number: u16,
    chapter: &ChapterParts,
    layout: Option<&[LayoutItem]>,
    usfm: &mut String,
    issues: &mut Vec<StructureIssue>,
) {
    let verses = &chapter.verses;
    // Each run first, so a layout that lists a covered verse before its span cannot make
    // the covered verse look stray.
    let mut run_end: BTreeMap<u16, u16> = BTreeMap::new();
    let mut consumed: BTreeSet<u16> = BTreeSet::new();
    for (verse, part) in verses {
        if let VersePart::Text(text) = part {
            let mut end = *verse;
            while end < text.last {
                let Some(next) = end.checked_add(1) else {
                    break;
                };
                if verses.get(&next) != Some(&VersePart::Covered { by: *verse }) {
                    break;
                }
                end = next;
                consumed.insert(end);
            }
            run_end.insert(*verse, end);
        }
    }

    let all: Vec<u16> = verses.keys().copied().collect();
    let order = match layout {
        None => all,
        Some(layout) => {
            let listed: Vec<u16> = layout
                .iter()
                .filter_map(|item| match item {
                    LayoutItem::Verse(c, v) if *c == number => Some(*v),
                    _ => None,
                })
                .collect();
            ordered(&all, &listed)
        }
    };

    for verse in order {
        match verses.get(&verse) {
            Some(VersePart::Text(text)) => {
                let end = run_end.get(&verse).copied().unwrap_or(verse);
                if end < text.last {
                    issues.push(StructureIssue::ShortSpan {
                        chapter: number,
                        verse,
                        last: text.last,
                        end,
                    });
                }
                usfm.push_str(&text.marker.marker(verse, end));
                usfm.push_str(&text.words);
                usfm.push_str(&text.layout);
            }
            Some(VersePart::Covered { by }) if !consumed.contains(&verse) => {
                issues.push(StructureIssue::StrayCovered {
                    chapter: number,
                    verse,
                    by: *by,
                });
            }
            Some(VersePart::Covered { .. }) | None => {}
        }
    }
}

/// `items` (ascending, unique) in the order of `listed`, with each item `listed` does not
/// name placed right after its nearest listed smaller item, or first when there is none.
/// Listed numbers that are not in `items`, and repeats, are skipped.
fn ordered(items: &[u16], listed: &[u16]) -> Vec<u16> {
    let present: BTreeSet<u16> = items.iter().copied().collect();
    let mut seen = BTreeSet::new();
    let listed: Vec<u16> = listed
        .iter()
        .copied()
        .filter(|n| present.contains(n) && seen.insert(*n))
        .collect();
    let listed_set: BTreeSet<u16> = listed.iter().copied().collect();
    // Each unlisted item hangs after the largest listed item below it (None = first).
    let mut after: BTreeMap<Option<u16>, Vec<u16>> = BTreeMap::new();
    for item in items.iter().copied().filter(|n| !listed_set.contains(n)) {
        let anchor = listed_set.range(..item).next_back().copied();
        after.entry(anchor).or_default().push(item);
    }
    let mut out = Vec::with_capacity(items.len());
    out.extend(after.get(&None).into_iter().flatten().copied());
    for item in listed {
        out.push(item);
        out.extend(after.get(&Some(item)).into_iter().flatten().copied());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poetry_keeps_its_lines_in_words() {
        let content = "line one\n\\q2 line two\n\\q1\n";
        assert_eq!(
            split_verse(content),
            ("line one\n\\q2 line two", "\n\\q1\n")
        );
    }
}
