//! Where a help item belongs: a Bible passage, a book or chapter introduction, or an Open
//! Bible Stories frame.

use std::fmt;
use tc_core::{BookId, Reference};

/// A chapter and verse, both counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChapterVerse {
    pub chapter: u16,
    pub verse: u16,
}

impl ChapterVerse {
    /// Returns `None` when either number is zero.
    pub fn new(chapter: u16, verse: u16) -> Option<Self> {
        (chapter >= 1 && verse >= 1).then_some(Self { chapter, verse })
    }
}

/// An inclusive run of verses that may cross chapters, such as `1:30-2:3`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VerseRange {
    start: ChapterVerse,
    end: ChapterVerse,
}

impl VerseRange {
    /// Returns `None` when `end` comes before `start`.
    pub fn new(start: ChapterVerse, end: ChapterVerse) -> Option<Self> {
        (start <= end).then_some(Self { start, end })
    }

    pub fn single(at: ChapterVerse) -> Self {
        Self { start: at, end: at }
    }

    pub fn start(self) -> ChapterVerse {
        self.start
    }

    pub fn end(self) -> ChapterVerse {
        self.end
    }

    pub fn contains(self, at: ChapterVerse) -> bool {
        self.start <= at && at <= self.end
    }

    fn overlaps(self, from: ChapterVerse, to: ChapterVerse) -> bool {
        self.start <= to && from <= self.end
    }
}

/// One or more verse ranges in source order, such as `1:3,5-6`. Never empty.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerseList(Vec<VerseRange>);

impl VerseList {
    /// Returns `None` for an empty list.
    pub fn new(ranges: Vec<VerseRange>) -> Option<Self> {
        (!ranges.is_empty()).then_some(Self(ranges))
    }

    pub fn single(range: VerseRange) -> Self {
        Self(vec![range])
    }

    pub fn ranges(&self) -> &[VerseRange] {
        &self.0
    }
}

/// The part of a book a help item is about.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BibleLocus {
    /// `front:intro` in a TSV, `BB000000` in Aquifer.
    BookIntro,
    /// `N:intro` in a TSV, `BBCCC000` in Aquifer.
    ChapterIntro(u16),
    Verses(VerseList),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BibleRef {
    pub book: BookId,
    pub locus: BibleLocus,
}

impl BibleRef {
    pub fn verse(book: BookId, at: ChapterVerse) -> Self {
        Self {
            book,
            locus: BibleLocus::Verses(VerseList::single(VerseRange::single(at))),
        }
    }

    /// True when this item is about any verse of `passage`. Introductions overlap nothing.
    pub fn overlaps(&self, passage: &Reference) -> bool {
        if self.book != passage.book {
            return false;
        }
        let BibleLocus::Verses(list) = &self.locus else {
            return false;
        };
        let from = ChapterVerse {
            chapter: passage.chapter,
            verse: passage.verses.start(),
        };
        let to = ChapterVerse {
            chapter: passage.chapter,
            verse: passage.verses.end(),
        };
        list.0.iter().any(|r| r.overlaps(from, to))
    }
}

impl fmt::Display for BibleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.book, TsvLocus(&self.locus))
    }
}

/// A frame of an Open Bible Stories story. Frame 0 is the story title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObsRef {
    story: u8,
    frame: u16,
}

impl ObsRef {
    pub const STORIES: u8 = 50;

    /// Returns `None` unless `story` is 1 to 50.
    pub fn new(story: u8, frame: u16) -> Option<Self> {
        (1..=Self::STORIES)
            .contains(&story)
            .then_some(Self { story, frame })
    }

    pub fn story(self) -> u8 {
        self.story
    }

    pub fn frame(self) -> u16 {
        self.frame
    }

    pub fn is_title(self) -> bool {
        self.frame == 0
    }

    /// Reads the TSV form `story:frame`, such as `1:0` or `12:4`.
    pub fn parse(text: &str) -> Option<Self> {
        let (story, frame) = text.split_once(':')?;
        Self::new(number(story)?.try_into().ok()?, number(frame)?)
    }
}

impl fmt::Display for ObsRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.story, self.frame)
    }
}

/// Where any help item belongs.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HelpRef {
    Bible(BibleRef),
    Obs(ObsRef),
}

impl HelpRef {
    pub fn is_introduction(&self) -> bool {
        matches!(
            self,
            HelpRef::Bible(BibleRef {
                locus: BibleLocus::BookIntro | BibleLocus::ChapterIntro(_),
                ..
            })
        )
    }

    /// The text of the TSV `Reference` column for this item.
    pub fn tsv_text(&self) -> String {
        match self {
            HelpRef::Bible(r) => TsvLocus(&r.locus).to_string(),
            HelpRef::Obs(r) => r.to_string(),
        }
    }
}

/// Digits only, with leading zeros allowed: `07` is 7.
pub(crate) fn number(text: &str) -> Option<u16> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// Reads the TSV `Reference` column of a Bible table: `front:intro`, `N:intro`, `C:V`,
/// `C:V-V`, `C:V-C:V`, and comma lists of those such as `1:3,5-6`.
pub(crate) fn parse_tsv_locus(text: &str) -> Option<BibleLocus> {
    if text == "front:intro" {
        return Some(BibleLocus::BookIntro);
    }
    if let Some(chapter) = text.strip_suffix(":intro") {
        let chapter = number(chapter)?;
        return (chapter >= 1).then_some(BibleLocus::ChapterIntro(chapter));
    }
    let mut ranges = Vec::new();
    let mut chapter: Option<u16> = None;
    for item in text.split(',') {
        let (from, to) = match item.split_once('-') {
            Some((a, b)) => (a, Some(b)),
            None => (item, None),
        };
        let start = point(from, chapter)?;
        let end = match to {
            Some(to) => point(to, Some(start.chapter))?,
            None => start,
        };
        ranges.push(VerseRange::new(start, end)?);
        chapter = Some(end.chapter);
    }
    VerseList::new(ranges).map(BibleLocus::Verses)
}

/// `C:V`, or a bare `V` in the chapter `current` when there is one.
fn point(text: &str, current: Option<u16>) -> Option<ChapterVerse> {
    match text.split_once(':') {
        Some((c, v)) => ChapterVerse::new(number(c)?, number(v)?),
        None => ChapterVerse::new(current?, number(text)?),
    }
}

/// Prints a locus the way the TSV `Reference` column writes it.
pub(crate) struct TsvLocus<'a>(pub &'a BibleLocus);

impl fmt::Display for TsvLocus<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            BibleLocus::BookIntro => f.write_str("front:intro"),
            BibleLocus::ChapterIntro(c) => write!(f, "{c}:intro"),
            BibleLocus::Verses(list) => {
                let mut chapter = None;
                for (i, r) in list.0.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    if chapter == Some(r.start.chapter) {
                        write!(f, "{}", r.start.verse)?;
                    } else {
                        write!(f, "{}:{}", r.start.chapter, r.start.verse)?;
                    }
                    if r.end != r.start {
                        if r.end.chapter == r.start.chapter {
                            write!(f, "-{}", r.end.verse)?;
                        } else {
                            write!(f, "-{}:{}", r.end.chapter, r.end.verse)?;
                        }
                    }
                    chapter = Some(r.end.chapter);
                }
                Ok(())
            }
        }
    }
}

/// An Aquifer `BBCCCVVV` key: the book in canonical order 01-66, then chapter and verse.
/// Chapter 0 or verse 0 marks an introduction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Bcv {
    pub book: BookId,
    pub chapter: u16,
    pub verse: u16,
}

impl Bcv {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() != 8 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let book = BookId::from_ordinal(text[0..2].parse().ok()?)?;
        Some(Self {
            book,
            chapter: text[2..5].parse().ok()?,
            verse: text[5..8].parse().ok()?,
        })
    }
}

/// Reads an Aquifer passage key, `BBCCCVVV` or `BBCCCVVV-BBCCCVVV`, as a place in one book.
pub(crate) fn parse_aquifer_ref(text: &str) -> Result<BibleRef, String> {
    let bad = |why: &str| format!("{text:?} {why}");
    let (a, b) = match text.split_once('-') {
        Some((a, b)) => (a, b),
        None => (text, text),
    };
    let start = Bcv::parse(a).ok_or_else(|| bad("is not a BBCCCVVV key"))?;
    let end = Bcv::parse(b).ok_or_else(|| bad("is not a BBCCCVVV key"))?;
    if start.book != end.book {
        return Err(bad("spans two books"));
    }
    aquifer_span(start, end).ok_or_else(|| bad("is not an introduction or a verse range"))
}

pub(crate) fn aquifer_span(start: Bcv, end: Bcv) -> Option<BibleRef> {
    let locus = match (start.chapter, start.verse) {
        // Chapter 0 is the front of the book. The por notes file the Titus introduction's
        // passage as `56000001`, not `56000000`.
        (0, _) if end.chapter == 0 => BibleLocus::BookIntro,
        (c, 0) if c > 0 && end == start => BibleLocus::ChapterIntro(c),
        _ => {
            let from = ChapterVerse::new(start.chapter, start.verse)?;
            let to = ChapterVerse::new(end.chapter, end.verse)?;
            BibleLocus::Verses(VerseList::single(VerseRange::new(from, to)?))
        }
    };
    Some(BibleRef {
        book: start.book,
        locus,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tsv_loci_round_trip() {
        for text in [
            "front:intro",
            "3:intro",
            "1:1",
            "1:1-3",
            "1:30-2:2",
            "1:3,5",
            "1:3,5-6,2:1",
        ] {
            let locus = parse_tsv_locus(text).unwrap();
            assert_eq!(TsvLocus(&locus).to_string(), text);
        }
        for bad in [
            "", "0:1", "1:0", "1", "1:2-1", "front:1", "1:a", "1:1-", ":1", "1:1,",
        ] {
            assert!(parse_tsv_locus(bad).is_none(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn aquifer_keys_map_to_books_in_canonical_order() {
        let r = parse_aquifer_ref("56001001").unwrap();
        assert_eq!(r.to_string(), "TIT 1:1");
        assert_eq!(
            parse_aquifer_ref("56000000").unwrap().locus,
            BibleLocus::BookIntro
        );
        assert_eq!(
            parse_aquifer_ref("56000001").unwrap().locus,
            BibleLocus::BookIntro
        );
        assert_eq!(
            parse_aquifer_ref("56002000").unwrap().locus,
            BibleLocus::ChapterIntro(2)
        );
        assert!(parse_aquifer_ref("56000000-56001001").is_err());
        assert_eq!(
            parse_aquifer_ref("01001003-01002003").unwrap().to_string(),
            "GEN 1:3-2:3"
        );
        assert!(parse_aquifer_ref("67001001").is_err());
        assert!(parse_aquifer_ref("56001001-57001001").is_err());
        assert!(parse_aquifer_ref("5600100").is_err());
    }
}
