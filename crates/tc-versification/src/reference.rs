//! tC4's frame rule: turn a reference from a resource's scheme into the project's scheme,
//! or refuse it with a reason (tC4 `src/data/mapReference.ts`, BURRITO-SPEC §5.2).
//!
//! A mapped reference becomes a check's identity key and is written once, so this refuses
//! rather than guesses. It reproduces tC4's outcomes exactly; the oracle tests run it
//! against tC4's own code.

use crate::book::BookCode;
use crate::scheme::Scheme;
use crate::spec::{VerseRef, digits, number};
use crate::versification::Absence;
use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;
use tc_core::{ChapterNo, Reference, VerseSpan};

/// The widest span `map_reference` maps. No chapter comes near it (PSA 119 has 176
/// verses); it bounds the work a malformed resource row can cause.
pub const MAX_SPAN_VERSES: u16 = 1000;

/// Why tC4 refuses to place a reference in the project's scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Unplaceable {
    /// The mapping arithmetic reached verse 0 (`eng PSA 116:10` to `rsc PSA 115:0`).
    VerseZero,
    /// The target scheme has no such book or chapter.
    NoChapter,
    /// Past the last verse of the chapter (`eng ACT 19:41` to `rsc`).
    PastChapterEnd,
    /// Several targets that cannot be one reference: a one-to-many step into `org`,
    /// targets in more than one chapter, or a gapped set of verses.
    Ambiguous,
    /// The verses of a span land in different chapters.
    SpanSplit,
    /// The row's reference is not one tC4 can map: a chapter that is not a number from
    /// 1, a verse 0, a span that is reversed, starts at 0 or is wider than
    /// `MAX_SPAN_VERSES`, or any other verse form (`front`, `1,3`, `1a`).
    MalformedReference,
}

impl From<Absence> for Unplaceable {
    fn from(absence: Absence) -> Self {
        match absence {
            Absence::VerseZero => Self::VerseZero,
            Absence::NoChapter => Self::NoChapter,
            Absence::PastChapterEnd => Self::PastChapterEnd,
        }
    }
}

/// One part of a resource row's reference, its chapter or its verse, read as tC4's
/// `refPart` reads it: plain digits become a number, and anything else stays text.
///
/// Within one frame, `map_reference` passes every form through as it is. Across frames
/// it maps a number or a span and refuses the rest.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RefPart {
    /// Plain digits: `5`, and `05`, which tC4 also reads as 5. Can be 0.
    Number(u16),
    /// `first-last` without leading zeros: `9-10`, and also `5-5` and `10-9`, which tC4
    /// keeps as text.
    Span { first: u16, last: u16 },
    /// Any other text, kept as written: `front`, `intro`, a comma list (`1,3`), a letter
    /// verse (`1a`), a span with a leading zero (`01-03`), or digits past 65535.
    Other(String),
}

impl RefPart {
    pub fn parse(text: &str) -> Self {
        if let Some(n) = digits(text) {
            return Self::Number(n);
        }
        if let Some((a, b)) = text.split_once('-')
            && let (Some(first), Some(last)) = (number(a), number(b))
        {
            return Self::Span { first, last };
        }
        Self::Other(text.to_owned())
    }
}

impl FromStr for RefPart {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::parse(s))
    }
}

impl fmt::Display for RefPart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Number(n) => write!(f, "{n}"),
            Self::Span { first, last } => write!(f, "{first}-{last}"),
            Self::Other(text) => f.write_str(text),
        }
    }
}

impl From<VerseSpan> for RefPart {
    fn from(span: VerseSpan) -> Self {
        if span.is_span() {
            Self::Span {
                first: span.start(),
                last: span.end(),
            }
        } else {
            Self::Number(span.start())
        }
    }
}

/// A reference as a resource row gives it, in some scheme's numbering: what tC4's
/// `mapReference` takes. The book code is upper case, as tC4 makes it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RowReference {
    pub book: BookCode,
    pub chapter: RefPart,
    pub verse: RefPart,
}

impl From<SchemeReference> for RowReference {
    fn from(r: SchemeReference) -> Self {
        Self {
            book: r.book,
            chapter: RefPart::Number(r.chapter),
            verse: r.verses.into(),
        }
    }
}

impl From<Reference> for RowReference {
    fn from(r: Reference) -> Self {
        SchemeReference::from(r).into()
    }
}

impl fmt::Display for RowReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.verse)
    }
}

/// A mapped reference: a chapter and a verse or span from 1, in a scheme's numbering.
/// Its book can lie outside the 66-book canon: schemes number deuterocanonical books,
/// and a mapping can land in a book such as `S3Y`. `From<Reference>` converts a
/// canonical reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemeReference {
    pub book: BookCode,
    pub chapter: u16,
    pub verses: VerseSpan,
}

impl From<Reference> for SchemeReference {
    fn from(r: Reference) -> Self {
        Self {
            book: r.book().into(),
            chapter: r.chapter().get(),
            verses: r.verses(),
        }
    }
}

impl SchemeReference {
    /// The canonical reference, when the book is one of the 66.
    pub fn to_reference(&self) -> Option<Reference> {
        Some(Reference::new(
            self.book.book_id()?,
            ChapterNo::new(self.chapter)?,
            self.verses,
        ))
    }
}

impl fmt::Display for SchemeReference {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}:{}", self.book, self.chapter, self.verses)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapOutcome {
    /// Both schemes have the same name: the reference passes through unchanged and
    /// unchecked, whatever its form (tC4 `mapped: false`). Composing `eng` to `org` and
    /// back would move 3 verses, so the same frame must not be mapped.
    SameFrame(RowReference),
    /// The reference was converted (tC4 `mapped: true`). The numbers can be unchanged.
    Mapped(SchemeReference),
    /// No usable reference. `candidates` are the target-scheme verses the mapping
    /// reached, for diagnostics only; they are never identities.
    Unplaceable {
        reason: Unplaceable,
        candidates: Vec<VerseRef>,
    },
}

/// Converts a reference from scheme `from` into scheme `to`, as tC4's `mapReference`
/// does.
///
/// Each verse goes forward into `org` and back out through the target scheme. A verse
/// that becomes several `org` verses is ambiguous. One that becomes several target
/// verses in one chapter is kept when they form an unbroken run, written as a span. A
/// span maps verse by verse and must land in one chapter as an unbroken run; its
/// endpoints, unlike a single verse, must exist in `from`. Every verse of the result must
/// exist in `to`. Like tC4, this reads verse counts only, never `excludedVerses`.
///
/// tC4 also refuses an unknown frame; here both schemes are known by type. tC4 reads a
/// verse past 65535 as a number and refuses it as past the chapter end (or in no
/// chapter); `RefPart` cannot hold that number, so this refuses it as malformed.
pub fn map_reference(from: &Scheme, to: &Scheme, reference: &RowReference) -> MapOutcome {
    if from.name() == to.name() {
        return MapOutcome::SameFrame(reference.clone());
    }
    let (RefPart::Number(chapter @ 1..), Some(verses)) =
        (&reference.chapter, tc4_verses(&reference.verse))
    else {
        return refuse(Unplaceable::MalformedReference, Vec::new());
    };
    let (book, chapter) = (reference.book, *chapter);

    let (first, last) = match verses {
        Verses::Single(verse) => {
            let landing = match hop(from, to, VerseRef::new(book, chapter, verse)) {
                Ok(landing) => landing,
                Err(reason) => return refuse(reason, Vec::new()),
            };
            if !contiguous(&landing.verses) {
                return refuse(
                    Unplaceable::Ambiguous,
                    candidates(std::slice::from_ref(&landing)),
                );
            }
            return finish(to, landing);
        }
        Verses::Span(first, last) => (first, last),
    };

    for endpoint in [first, last] {
        if let Some(absence) = from
            .versification()
            .absence(VerseRef::new(book, chapter, endpoint))
        {
            return refuse(absence.into(), Vec::new());
        }
    }
    let mut landings = Vec::new();
    for verse in first..=last {
        match hop(from, to, VerseRef::new(book, chapter, verse)) {
            Ok(landing) => landings.push(landing),
            Err(reason) => return refuse(reason, Vec::new()),
        }
    }
    let reached = candidates(&landings);
    #[expect(
        clippy::unreachable,
        reason = "tc4_verses gives a span only when first <= last, so the loop ran"
    )]
    let Some(&TcLanding {
        book: first_book,
        chapter: first_chapter,
        ..
    }) = landings.first()
    else {
        unreachable!("a span with no verse");
    };
    if landings
        .iter()
        .any(|l| l.book != first_book || l.chapter != first_chapter)
    {
        return refuse(Unplaceable::SpanSplit, reached);
    }
    let mut verses: Vec<u16> = landings
        .iter()
        .flat_map(|l| l.verses.iter().copied())
        .collect();
    verses.sort_unstable();
    verses.dedup();
    if !contiguous(&verses) {
        return refuse(Unplaceable::Ambiguous, reached);
    }
    finish(
        to,
        TcLanding {
            book: first_book,
            chapter: first_chapter,
            verses,
        },
    )
}

/// A verse field tC4 can map: one verse, or `first..=last`.
#[derive(Debug, Clone, Copy)]
enum Verses {
    Single(u16),
    Span(u16, u16),
}

/// tC4's `parseVerse` and its `malformedVerse` test: a verse from 1, or a span from 1
/// that is not reversed and at most `MAX_SPAN_VERSES` wide. Text a caller kept as
/// `Other` is read the way `parseVerse` reads a string, so `01-03` is the span 1-3.
fn tc4_verses(part: &RefPart) -> Option<Verses> {
    let (first, last) = match part {
        RefPart::Number(verse) => return (*verse >= 1).then_some(Verses::Single(*verse)),
        RefPart::Span { first, last } => (*first, *last),
        RefPart::Other(text) => {
            if let Some(verse) = digits(text) {
                return (verse >= 1).then_some(Verses::Single(verse));
            }
            let (a, b) = text.split_once('-')?;
            (digits(a)?, digits(b)?)
        }
    };
    let fits = 1 <= first && first <= last && last - first < MAX_SPAN_VERSES;
    fits.then_some(Verses::Span(first, last))
}

/// Where one source verse lands, in tC4's shape: one book, one chapter, and the target
/// verses, sorted, distinct and never empty.
struct TcLanding {
    book: BookCode,
    chapter: u16,
    verses: Vec<u16>,
}

fn hop(from: &Scheme, to: &Scheme, verse: VerseRef) -> Result<TcLanding, Unplaceable> {
    let forward = from.to_org(verse);
    if forward.target_count() != 1 {
        return Err(Unplaceable::Ambiguous);
    }
    let back = to.from_org(forward.first());
    let chapter = back.first().chapter;
    if back.targets().any(|t| t.chapter != chapter) {
        return Err(Unplaceable::Ambiguous);
    }
    let mut verses: Vec<u16> = back.targets().map(|t| t.verse).collect();
    verses.sort_unstable();
    verses.dedup();
    Ok(TcLanding {
        book: back.book(),
        chapter,
        verses,
    })
}

fn finish(to: &Scheme, landing: TcLanding) -> MapOutcome {
    let reached = || candidates(std::slice::from_ref(&landing));
    for &verse in &landing.verses {
        if let Some(absence) =
            to.versification()
                .absence(VerseRef::new(landing.book, landing.chapter, verse))
        {
            return refuse(absence.into(), reached());
        }
    }
    #[expect(
        clippy::unreachable,
        reason = "a landing holds every target of a Hop, which always has one"
    )]
    let (Some(&lo), Some(&hi)) = (landing.verses.first(), landing.verses.last()) else {
        unreachable!("a landing with no verse");
    };
    // Verse 0 was refused above, so `lo >= 1` and the range is valid.
    match VerseSpan::range(lo, hi) {
        Some(verses) => MapOutcome::Mapped(SchemeReference {
            book: landing.book,
            chapter: landing.chapter,
            verses,
        }),
        None => refuse(Unplaceable::VerseZero, reached()),
    }
}

fn contiguous(sorted: &[u16]) -> bool {
    sorted.windows(2).all(|w| w[0].checked_add(1) == Some(w[1]))
}

fn candidates(landings: &[TcLanding]) -> Vec<VerseRef> {
    landings
        .iter()
        .flat_map(|l| {
            l.verses
                .iter()
                .map(|&v| VerseRef::new(l.book, l.chapter, v))
        })
        .collect()
}

fn refuse(reason: Unplaceable, candidates: Vec<VerseRef>) -> MapOutcome {
    MapOutcome::Unplaceable { reason, candidates }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_parse_as_tc4_ref_part_reads_them() {
        assert_eq!(RefPart::parse("5"), RefPart::Number(5));
        assert_eq!(RefPart::parse("05"), RefPart::Number(5));
        assert_eq!(RefPart::parse("0"), RefPart::Number(0));
        assert_eq!(RefPart::parse("5-5"), RefPart::Span { first: 5, last: 5 });
        assert_eq!(RefPart::parse("10-9"), RefPart::Span { first: 10, last: 9 });
        for other in [
            "front", "1,3", "1a", "01-03", "70000", "", " 3", "+3", "1-2-3",
        ] {
            let part = RefPart::parse(other);
            assert_eq!(part, RefPart::Other(other.to_owned()));
            assert_eq!(part.to_string(), other);
        }
    }

    #[test]
    fn verse_fields_tc4_can_map() {
        let single = |p: RefPart| match tc4_verses(&p) {
            Some(Verses::Single(v)) => Some(v),
            _ => None,
        };
        let span = |p: RefPart| match tc4_verses(&p) {
            Some(Verses::Span(a, b)) => Some((a, b)),
            _ => None,
        };
        assert_eq!(single(RefPart::Number(3)), Some(3));
        assert_eq!(single(RefPart::Other("3".into())), Some(3));
        assert_eq!(span(RefPart::parse("5-5")), Some((5, 5)));
        assert_eq!(span(RefPart::parse("01-03")), Some((1, 3)));
        assert_eq!(span(RefPart::parse("1-1000")), Some((1, 1000)));
        for bad in ["0", "0-2", "3-2", "1-1001", "front", "1,3", "1a", "70000"] {
            assert!(tc4_verses(&RefPart::parse(bad)).is_none(), "{bad:?}");
        }
    }
}
