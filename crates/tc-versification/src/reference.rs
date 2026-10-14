//! tC4's frame rule: turn a reference from a resource's scheme into the project's scheme,
//! or refuse it with a reason (tC4 `src/data/mapReference.ts`, BURRITO-SPEC §5.2).
//!
//! A mapped reference becomes a check's identity key and is written once, so this refuses
//! rather than guesses. It reproduces tC4's outcomes exactly; the oracle test runs it
//! against tC4's own code.

use crate::book::BookCode;
use crate::scheme::Scheme;
use crate::spec::VerseRef;
use crate::versification::Absence;
use std::fmt;
use tc_core::{Reference, VerseSpan};

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
    /// Chapter 0, or a span wider than `MAX_SPAN_VERSES`.
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

/// A reference in a scheme's numbering. Its book can lie outside the 66-book canon:
/// schemes number deuterocanonical books, and a mapping can land in a book such as
/// `S3Y`. `From<Reference>` converts a canonical reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemeReference {
    pub book: BookCode,
    pub chapter: u16,
    pub verses: VerseSpan,
}

impl From<Reference> for SchemeReference {
    fn from(r: Reference) -> Self {
        Self {
            book: r.book.into(),
            chapter: r.chapter,
            verses: r.verses,
        }
    }
}

impl SchemeReference {
    /// The canonical reference, when the book is one of the 66.
    pub fn to_reference(&self) -> Option<Reference> {
        Some(Reference {
            book: self.book.book_id()?,
            chapter: self.chapter,
            verses: self.verses,
        })
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
    /// unchecked (tC4 `mapped: false`). Composing `eng` to `org` and back would move 3
    /// verses, so the same frame must not be mapped.
    SameFrame(SchemeReference),
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
/// span maps verse by verse and must land in one chapter as an unbroken run. Every
/// verse of the result must exist in `to`.
///
/// tC4 also refuses an unknown frame; here both schemes are known by type. tC4 treats
/// the verse `5` and the span `5-5` differently (only a span checks its endpoints in the
/// source scheme); `VerseSpan` cannot tell them apart and takes the single-verse path.
pub fn map_reference(from: &Scheme, to: &Scheme, reference: &SchemeReference) -> MapOutcome {
    if from.name() == to.name() {
        return MapOutcome::SameFrame(*reference);
    }
    let span = reference.verses;
    let width = u32::from(span.end()) - u32::from(span.start()) + 1;
    if reference.chapter == 0 || width > u32::from(MAX_SPAN_VERSES) {
        return refuse(Unplaceable::MalformedReference, Vec::new());
    }
    let book = reference.book;
    let chapter = reference.chapter;

    if !span.is_span() {
        let landing = match hop(from, to, VerseRef::new(book, chapter, span.start())) {
            Ok(landing) => landing,
            Err(reason) => return refuse(reason, Vec::new()),
        };
        if landing.verses.len() > 1 && !contiguous(&landing.verses) {
            return refuse(
                Unplaceable::Ambiguous,
                candidates(std::slice::from_ref(&landing)),
            );
        }
        return finish(to, landing);
    }

    for endpoint in [span.start(), span.end()] {
        if let Some(absence) = from
            .versification()
            .absence(VerseRef::new(book, chapter, endpoint))
        {
            return refuse(absence.into(), Vec::new());
        }
    }
    let mut landings = Vec::new();
    for verse in span.verses() {
        match hop(from, to, VerseRef::new(book, chapter, verse)) {
            Ok(landing) => landings.push(landing),
            Err(reason) => return refuse(reason, Vec::new()),
        }
    }
    let reached = candidates(&landings);
    // A span has at least two verses, so there is a first landing.
    let (first_book, first_chapter) = (landings[0].book, landings[0].chapter);
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

/// Where one source verse lands, in tC4's shape: one book, one chapter, sorted verses.
struct TcLanding {
    book: BookCode,
    chapter: u16,
    verses: Vec<u16>,
}

fn hop(from: &Scheme, to: &Scheme, verse: VerseRef) -> Result<TcLanding, Unplaceable> {
    let forward = from.to_org(verse);
    let &[pivot] = forward.targets.as_slice() else {
        return Err(Unplaceable::Ambiguous);
    };
    let back = to.from_org(pivot);
    let chapter = back.targets[0].chapter;
    if back.targets.iter().any(|t| t.chapter != chapter) {
        return Err(Unplaceable::Ambiguous);
    }
    let mut verses: Vec<u16> = back.targets.iter().map(|t| t.verse).collect();
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
    let (lo, hi) = (landing.verses[0], landing.verses[landing.verses.len() - 1]);
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
