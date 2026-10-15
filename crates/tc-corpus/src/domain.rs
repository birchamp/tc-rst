//! The plain inputs domain crates take, without any `tc-journal` type: the books with their
//! chapters and verse texts, the alignments, the check rows and the original-language text.
//! Every value says where it came from ([`Origin`]).

use crate::fixtures::{self, HelpsRow, Psalms, Releases, UltBook, UltVerse};
use crate::rng::Rng;
use crate::{CorpusError, Source, Spec};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use tc_core::{BookId, Testament};
use tc_versification::{BookCode, StandardScheme};

/// Where a part of the corpus came from. Benchmark output prints these, so nobody mistakes
/// generated data for real data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Origin {
    /// Read from a real file, named by its repository path.
    Real(&'static str),
    /// Real data from the named file, placed somewhere else (another book, another verse).
    Sampled(&'static str),
    /// Made up by the generator; the text says what it stands in for.
    Generated(&'static str),
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Real(path) => write!(f, "real, from {path}"),
            Self::Sampled(path) => write!(f, "sampled from {path}"),
            Self::Generated(what) => write!(f, "generated: {what}"),
        }
    }
}

/// A source (original-language) word of an alignment group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceWordInput {
    pub word: String,
    pub strong: Option<String>,
    pub lemma: Option<String>,
    pub morph: Option<String>,
    pub occurrence: u32,
    pub occurrences: u32,
}

/// A target word: its text and its occurrence in the verse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetWordInput {
    pub word: String,
    pub occurrence: u32,
    pub occurrences: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupInput {
    pub sources: Vec<SourceWordInput>,
    pub targets: Vec<TargetWordInput>,
}

/// One verse (or the first verse of a span) of a book: the USFM content after its marker,
/// split as §2.4 of STORAGE.md splits it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseInput {
    pub book: BookId,
    pub chapter: u16,
    pub verse: u16,
    /// The last verse of the span; `verse` for a single verse.
    pub last: u16,
    pub words: String,
    pub layout: String,
    pub origin: Origin,
}

/// The alignment of one verse (or span) to the original language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlignmentInput {
    pub book: BookId,
    pub chapter: u16,
    pub verse: u16,
    pub groups: Vec<GroupInput>,
    pub origin: Origin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CheckTool {
    TranslationNotes,
    TranslationWords,
}

/// One check item: a translationNotes row or a translationWords link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckRow {
    pub tool: CheckTool,
    pub book: BookId,
    /// tC4's reference strings: `1`, `front`.
    pub chapter: String,
    /// `1`, `1-3`, `intro`.
    pub verse: String,
    /// The row id; unique per tool and book.
    pub id: String,
    /// tC4's group id: the Translation Academy article or the word.
    pub group: String,
    /// tC4's category: `translate` for notes, `kt`, `names` or `other` for words.
    pub category: String,
    /// The original-language quote.
    pub quote: String,
    pub occurrence: u32,
    pub note: String,
    pub origin: Origin,
}

impl CheckRow {
    /// The chapter and first verse the row names, when both are numbers.
    pub fn verse_number(&self) -> Option<(u16, u16)> {
        let chapter = self.chapter.parse().ok()?;
        let verse = self.verse.split('-').next()?.parse().ok()?;
        Some((chapter, verse))
    }
}

/// The original-language text of a book.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginalText {
    pub usfm: String,
    pub origin: Origin,
}

/// A book of the corpus.
#[derive(Debug, Clone)]
pub struct BookInput {
    pub book: BookId,
    /// Every byte before `\c 1`.
    pub header: String,
    /// Verse counts per chapter; chapter `n` at index `n - 1`.
    pub chapters: Vec<u16>,
    /// The chapter fronts (`\c N` up to the first verse), chapter `n` at index `n - 1`.
    pub fronts: Vec<String>,
    pub verses: Vec<VerseInput>,
    pub alignments: Vec<AlignmentInput>,
    pub checks: Vec<CheckRow>,
    /// The Hebrew of an Old Testament book or the Greek of a New Testament book; `None`
    /// for a book sampled from Psalms.
    pub original: Option<OriginalText>,
    /// Where the text and alignments came from.
    pub origin: Origin,
}

impl BookInput {
    pub fn verse_count(&self) -> usize {
        self.chapters.iter().map(|&n| usize::from(n)).sum()
    }
}

/// The plain inputs of a corpus.
#[derive(Debug, Clone)]
pub struct Domain {
    pub books: Vec<BookInput>,
    /// Alternative words for edits: the WEB Psalms, real but unrelated text.
    pub edit_words: Vec<String>,
}

impl Domain {
    pub fn verse_count(&self) -> usize {
        self.books.iter().map(BookInput::verse_count).sum()
    }

    pub fn check_count(&self) -> usize {
        self.books.iter().map(|b| b.checks.len()).sum()
    }

    pub fn book(&self, book: BookId) -> Option<&BookInput> {
        self.books.iter().find(|b| b.book == book)
    }

    /// Every origin in the corpus with the parts that have it.
    pub fn origins(&self) -> BTreeMap<String, String> {
        let mut parts: BTreeMap<String, BTreeMap<&'static str, usize>> = BTreeMap::new();
        let mut add = |origin: Origin, what: &'static str| {
            *parts
                .entry(origin.to_string())
                .or_default()
                .entry(what)
                .or_default() += 1;
        };
        for book in &self.books {
            add(book.origin, "books");
            for v in &book.verses {
                add(v.origin, "verse texts");
            }
            for a in &book.alignments {
                add(a.origin, "alignments");
            }
            for c in &book.checks {
                add(c.origin, "check rows");
            }
            if let Some(original) = &book.original {
                add(original.origin, "original-language books");
            }
        }
        parts
            .into_iter()
            .map(|(origin, counts)| {
                let text = counts
                    .into_iter()
                    .map(|(what, n)| format!("{n} {what}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                (origin, text)
            })
            .collect()
    }
}

/// Verse counts per chapter of `book` in the English versification.
pub fn verse_counts(book: BookId) -> Vec<u16> {
    BookCode::new(book.code())
        .ok()
        .and_then(|code| {
            StandardScheme::Eng
                .scheme()
                .versification()
                .verse_counts(code)
                .map(<[u16]>::to_vec)
        })
        .unwrap_or_default()
}

/// Check rows per book, from `fixtures/tc4/conformance-fixtures/check-density.json`.
pub const DENSITY_PATH: &str = "fixtures/tc4/conformance-fixtures/check-density.json";

struct Density {
    notes: BTreeMap<BookId, u32>,
    words: BTreeMap<BookId, u32>,
}

impl Density {
    fn load(root: &Path) -> Result<Self, CorpusError> {
        let fail = |reason: String| CorpusError::Fixture {
            path: DENSITY_PATH.into(),
            reason,
        };
        let text =
            std::fs::read_to_string(root.join(DENSITY_PATH)).map_err(|e| fail(e.to_string()))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|e| fail(e.to_string()))?;
        let table = |name: &str| -> Result<BTreeMap<BookId, u32>, CorpusError> {
            let map = value
                .get(name)
                .and_then(|v| v.as_object())
                .ok_or_else(|| fail(format!("no {name} table")))?;
            let mut out = BTreeMap::new();
            for (code, count) in map {
                let book: BookId = code.parse().map_err(|_| fail(format!("book {code}")))?;
                let count = count
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| fail(format!("count of {code}")))?;
                out.insert(book, count);
            }
            Ok(out)
        };
        Ok(Self {
            notes: table("translationNotes")?,
            words: table("translationWords")?,
        })
    }
}

const PSA_ULT: Origin = Origin::Real(fixtures::PSA_ULT);
const PSA_ULT_SAMPLED: Origin = Origin::Sampled(fixtures::PSA_ULT);
const PSA_TN: Origin = Origin::Real(fixtures::PSA_TN);
const PSA_TN_SAMPLED: Origin = Origin::Sampled(fixtures::PSA_TN);
const PSA_TWL: Origin = Origin::Real(fixtures::PSA_TWL);
const PSA_TWL_SAMPLED: Origin = Origin::Sampled(fixtures::PSA_TWL);
const PSA_UHB: Origin = Origin::Real(fixtures::PSA_UHB);
const ULT_V91: Origin = Origin::Real(fixtures::ULT_ZIP);
const ULT_V91_SAMPLED: Origin = Origin::Sampled(fixtures::ULT_ZIP);
const TN_V91: Origin = Origin::Real(fixtures::TN_ZIP);
const TN_V91_SAMPLED: Origin = Origin::Sampled(fixtures::TN_ZIP);
const TWL_V91: Origin = Origin::Real(fixtures::TWL_ZIP);
const TWL_V91_SAMPLED: Origin = Origin::Sampled(fixtures::TWL_ZIP);
const UHB_V3: Origin = Origin::Real(fixtures::UHB_ZIP);
const UGNT_V034: Origin = Origin::Real(fixtures::UGNT_ZIP);
const GENERATED_FRAME: Origin = Origin::Generated(
    "the header, chapter fronts and verse frame of a book the fixtures lack, from the English versification",
);

pub(crate) fn build(spec: &Spec, root: &Path, rng: &Rng) -> Result<Domain, CorpusError> {
    let density = Density::load(root)?;
    let psalms = Psalms::load(root)?;
    let edit_words = psalms
        .web
        .values()
        .flat_map(|text| text.split(' '))
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| !w.is_empty() && w.chars().all(char::is_alphabetic))
        .map(str::to_owned)
        .collect();
    let books = match spec.source {
        Source::Psalms => psalms_books(spec, &psalms, &density, rng),
        Source::Releases => {
            let releases = fixtures::load_releases(root)?;
            release_books(spec, &releases, &density, rng)
        }
    };
    Ok(Domain { books, edit_words })
}

/// What a real book is made of.
struct RealBook<'a> {
    ult: &'a UltBook,
    notes: &'a [HelpsRow],
    words: &'a [HelpsRow],
    hebrew: Option<&'a str>,
    origins: [Origin; 4],
}

fn psalms_books(spec: &Spec, psalms: &Psalms, density: &Density, rng: &Rng) -> Vec<BookInput> {
    let pool = Pool {
        verses: psalms
            .ult
            .verses
            .iter()
            .filter(|v| v.last == v.verse)
            .collect(),
        notes: &psalms.notes,
        words: &psalms.words,
        origins: [PSA_ULT_SAMPLED, PSA_TN_SAMPLED, PSA_TWL_SAMPLED],
    };
    spec.books
        .iter()
        .map(|&book| {
            let mut rng = rng.fork(book.code());
            if book == BookId::Psalms {
                real_book(
                    book,
                    &RealBook {
                        ult: &psalms.ult,
                        notes: &psalms.notes,
                        words: &psalms.words,
                        hebrew: Some(&psalms.hebrew),
                        origins: [PSA_ULT, PSA_TN, PSA_TWL, PSA_UHB],
                    },
                )
            } else {
                sampled_book(book, &pool, density, &mut rng)
            }
        })
        .collect()
}

fn release_books(spec: &Spec, releases: &Releases, density: &Density, rng: &Rng) -> Vec<BookInput> {
    let pool_of = |testament: Testament| Pool {
        verses: releases
            .books
            .iter()
            .filter(|(b, _)| b.testament() == testament)
            .flat_map(|(_, r)| r.ult.verses.iter())
            .filter(|v| v.last == v.verse)
            .collect(),
        notes: &[],
        words: &[],
        origins: [ULT_V91_SAMPLED, TN_V91_SAMPLED, TWL_V91_SAMPLED],
    };
    let old = pool_of(Testament::Old);
    let new = pool_of(Testament::New);
    // Rows to sample from: every row of the same testament's real books.
    let rows_of = |testament: Testament, words: bool| -> Vec<&HelpsRow> {
        releases
            .books
            .iter()
            .filter(|(b, _)| b.testament() == testament)
            .flat_map(|(_, r)| {
                if words {
                    r.words.iter()
                } else {
                    r.notes.iter()
                }
            })
            .collect()
    };
    let old_rows = (
        rows_of(Testament::Old, false),
        rows_of(Testament::Old, true),
    );
    let new_rows = (
        rows_of(Testament::New, false),
        rows_of(Testament::New, true),
    );
    spec.books
        .iter()
        .map(|&book| {
            let mut rng = rng.fork(book.code());
            let original = releases.originals.get(&book).map(|usfm| OriginalText {
                usfm: usfm.clone(),
                origin: match book.testament() {
                    Testament::Old => UHB_V3,
                    Testament::New => UGNT_V034,
                },
            });
            let mut built = match releases.books.get(&book) {
                Some(release) => real_book(
                    book,
                    &RealBook {
                        ult: &release.ult,
                        notes: &release.notes,
                        words: &release.words,
                        hebrew: None,
                        origins: [ULT_V91, TN_V91, TWL_V91, UHB_V3],
                    },
                ),
                None => {
                    let (pool, rows) = match book.testament() {
                        Testament::Old => (&old, &old_rows),
                        Testament::New => (&new, &new_rows),
                    };
                    sampled_book_with(book, pool, density, &mut rng, rows)
                }
            };
            built.original = original;
            built
        })
        .collect()
}

fn real_book(book: BookId, real: &RealBook<'_>) -> BookInput {
    let [text_origin, notes_origin, words_origin, original_origin] = real.origins;
    let mut chapters: Vec<u16> = Vec::new();
    for verse in &real.ult.verses {
        let index = usize::from(verse.chapter).saturating_sub(1);
        if chapters.len() <= index {
            chapters.resize(index + 1, 0);
        }
        chapters[index] = chapters[index].max(verse.last);
    }
    let fronts = (1..=chapters.len())
        .map(|c| {
            real.ult
                .fronts
                .get(&(c as u16))
                .cloned()
                .unwrap_or_else(|| format!("\\c {c}\n"))
        })
        .collect();
    let verses = real
        .ult
        .verses
        .iter()
        .map(|v| verse_input(book, v.chapter, v.verse, v, text_origin))
        .collect();
    let alignments = real
        .ult
        .verses
        .iter()
        .map(|v| AlignmentInput {
            book,
            chapter: v.chapter,
            verse: v.verse,
            groups: v.groups.clone(),
            origin: text_origin,
        })
        .collect();
    let mut checks: Vec<CheckRow> = real
        .notes
        .iter()
        .map(|row| check_row(CheckTool::TranslationNotes, book, row, notes_origin))
        .collect();
    checks.extend(
        real.words
            .iter()
            .map(|row| check_row(CheckTool::TranslationWords, book, row, words_origin)),
    );
    BookInput {
        book,
        header: real.ult.header.clone(),
        chapters,
        fronts,
        verses,
        alignments,
        checks,
        original: real.hebrew.map(|usfm| OriginalText {
            usfm: usfm.to_owned(),
            origin: original_origin,
        }),
        origin: text_origin,
    }
}

/// Real data to place in a book the fixtures lack.
struct Pool<'a> {
    /// Single verses (no spans) to copy.
    verses: Vec<&'a UltVerse>,
    notes: &'a [HelpsRow],
    words: &'a [HelpsRow],
    /// Text, notes, words.
    origins: [Origin; 3],
}

fn sampled_book(book: BookId, pool: &Pool<'_>, density: &Density, rng: &mut Rng) -> BookInput {
    let rows = (
        pool.notes.iter().collect::<Vec<_>>(),
        pool.words.iter().collect::<Vec<_>>(),
    );
    sampled_book_with(book, pool, density, rng, &rows)
}

/// A book with the English versification's verse frame, each verse a copy of a random
/// pool verse, and as many check rows as tC4's density table gives, each a copy of a
/// random pool row placed on a random verse.
fn sampled_book_with(
    book: BookId,
    pool: &Pool<'_>,
    density: &Density,
    rng: &mut Rng,
    rows: &(Vec<&HelpsRow>, Vec<&HelpsRow>),
) -> BookInput {
    let [text_origin, notes_origin, words_origin] = pool.origins;
    let chapters = verse_counts(book);
    let header = format!(
        "\\id {code} EN_ULT en_English_ltr generated by tc-corpus\n\\usfm 3.0\n\\ide UTF-8\n\\h {name}\n\\toc1 The Book of {name}\n\\toc2 {name}\n\\toc3 {code}\n\\mt1 {name}\n\n",
        code = book.code(),
        name = book.english_name()
    );
    let fronts = (1..=chapters.len())
        .map(|c| format!("\\c {c}\n\\p\n"))
        .collect();
    let mut verses = Vec::new();
    let mut alignments = Vec::new();
    for (index, &count) in chapters.iter().enumerate() {
        let chapter = index as u16 + 1;
        for verse in 1..=count {
            let Some(source) = rng.pick(&pool.verses).copied() else {
                continue;
            };
            verses.push(verse_input(book, chapter, verse, source, text_origin));
            alignments.push(AlignmentInput {
                book,
                chapter,
                verse,
                groups: source.groups.clone(),
                origin: text_origin,
            });
        }
    }
    let mut checks = Vec::new();
    for (tool, table, from, origin, prefix) in [
        (
            CheckTool::TranslationNotes,
            &density.notes,
            &rows.0,
            notes_origin,
            'n',
        ),
        (
            CheckTool::TranslationWords,
            &density.words,
            &rows.1,
            words_origin,
            'w',
        ),
    ] {
        let count = table.get(&book).copied().unwrap_or(0);
        for n in 0..count {
            let (Some(row), Some(v)) = (rng.pick(from), rng.pick(&verses)) else {
                break;
            };
            let mut placed = check_row(tool, book, row, origin);
            placed.chapter = v.chapter.to_string();
            placed.verse = v.verse.to_string();
            placed.id = format!("{prefix}{n:05x}");
            checks.push(placed);
        }
    }
    BookInput {
        book,
        header,
        chapters,
        fronts,
        verses,
        alignments,
        checks,
        original: None,
        origin: GENERATED_FRAME,
    }
}

fn verse_input(
    book: BookId,
    chapter: u16,
    verse: u16,
    source: &UltVerse,
    origin: Origin,
) -> VerseInput {
    VerseInput {
        book,
        chapter,
        verse,
        last: verse + (source.last - source.verse),
        words: source.words.clone(),
        layout: source.layout.clone(),
        origin,
    }
}

fn check_row(tool: CheckTool, book: BookId, row: &HelpsRow, origin: Origin) -> CheckRow {
    CheckRow {
        tool,
        book,
        chapter: row.chapter.clone(),
        verse: row.verse.clone(),
        id: row.id.clone(),
        group: row.group.clone(),
        category: row.category.clone(),
        quote: row.quote.clone(),
        occurrence: row.occurrence,
        note: row.note.clone(),
        origin,
    }
}
