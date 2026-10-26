//! Property tests: both formats round-trip, and mapping into `org` and back is the
//! identity for every verse no lossy entry touches.

// Test helpers fail the test by panicking.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use proptest::prelude::*;
use std::collections::BTreeSet;
use tc_versification::{
    BookCode, MappedRange, Mapping, Part, Scheme, StandardScheme, UnappliedForm, UnappliedMapping,
    VerseRef, Versification, WriteError, map_verse,
};

const SOURCE_BOOKS: [&str; 6] = ["GEN", "PSA", "TIT", "S3Y", "DAG", "XYZ"];
/// Targets that cross books must name a book Proskomma can encode.
const TARGET_BOOKS: [&str; 5] = ["GEN", "PSA", "TIT", "S3Y", "DAG"];

fn book_from(pool: &'static [&'static str]) -> impl Strategy<Value = BookCode> {
    prop::sample::select(pool).prop_map(|b| BookCode::new(b).unwrap())
}

fn verse_ref() -> impl Strategy<Value = VerseRef> {
    (book_from(&SOURCE_BOOKS), 1..8u16, 0..40u16).prop_map(|(b, c, v)| VerseRef::new(b, c, v))
}

/// A range as files write them, including reversed ranges and sub-verse letters.
fn range(books: &'static [&'static str], letters: bool) -> impl Strategy<Value = MappedRange> {
    (
        book_from(books),
        0..8u16,
        0..40u16,
        prop::option::of(0..45u16),
        prop::option::weighted(0.2, prop::char::range('a', 'c')),
    )
        .prop_map(move |(book, chapter, first, last, part)| MappedRange {
            book,
            chapter,
            first,
            part: part.filter(|_| letters).and_then(Part::new),
            last,
        })
}

fn mapping() -> impl Strategy<Value = Mapping> {
    (
        range(&SOURCE_BOOKS, false),
        prop::collection::vec(range(&TARGET_BOOKS, true), 1..4),
    )
        .prop_map(|(source, targets)| Mapping::new(source, targets).unwrap())
}

/// Segments as files write them, and the characters a `.vrs` line cannot hold: commas,
/// line breaks, and white space that trimming a line removes.
fn segment() -> impl Strategy<Value = String> {
    "[a-c ,\n\r\t-]{0,3}"
}

fn unapplied() -> impl Strategy<Value = UnappliedMapping> {
    (
        any::<bool>(),
        range(&SOURCE_BOOKS, false),
        range(&TARGET_BOOKS, true),
    )
        .prop_map(|(ampersand, source, target)| {
            let form = if ampersand {
                UnappliedForm::Ampersand
            } else {
                UnappliedForm::Hidden
            };
            UnappliedMapping::new(form, source, target).unwrap()
        })
}

/// Any scheme the parsers accept, lossy or not.
fn versification() -> impl Strategy<Value = Versification> {
    (
        prop::collection::btree_map(
            book_from(&SOURCE_BOOKS),
            prop::collection::vec(0..40u16, 1..8),
            1..5,
        ),
        prop::collection::vec(mapping(), 0..10),
        prop::collection::vec(verse_ref(), 0..3),
        prop::collection::btree_map(verse_ref(), prop::collection::vec(segment(), 0..3), 0..3),
        prop::collection::vec(unapplied(), 0..3),
    )
        .prop_map(|(books, mappings, excluded, partial, unapplied)| {
            let mut seen = BTreeSet::new();
            let mappings = mappings
                .into_iter()
                .filter(|m| seen.insert(m.source().to_string()))
                .collect();
            Versification::new(books.into_iter().collect(), mappings)
                .unwrap()
                .with_excluded_verses(excluded)
                .with_partial_verses(partial.into_iter().collect())
                .unwrap()
                .with_unapplied_mappings(unapplied)
        })
}

/// Whether a `.vrs` partial-verse line would read this segment back differently.
fn breaks_a_vrs_line(parts: &[String], segment: &str) -> bool {
    segment.contains([',', '\n', '\r'])
        || (parts.last().map(String::as_str) == Some(segment)
            && segment.ends_with(char::is_whitespace))
}

/// A scheme with no lossy entry: some runs of verses move, whole and one-to-one, into
/// chapters (100 and up) that nothing else uses, possibly in another book. Its books
/// are ones Proskomma can encode, since the reversed table crosses back into them.
fn lossless_scheme() -> impl Strategy<Value = Versification> {
    let book = (
        book_from(&TARGET_BOOKS),
        prop::collection::vec(
            // A chapter: its verse count, and the cut points and moves of its runs.
            (
                1..30u16,
                prop::collection::vec(
                    (1..30u16, any::<bool>(), book_from(&TARGET_BOOKS), 0..5u16),
                    0..4,
                ),
            ),
            1..5,
        ),
    );
    prop::collection::vec(book, 1..4).prop_map(|books| {
        let mut max_verses = Vec::new();
        let mut mappings = Vec::new();
        let mut fresh_chapter = 100u16;
        let mut seen = BTreeSet::new();
        for (book, chapters) in books {
            if !seen.insert(book) {
                continue;
            }
            let mut counts = Vec::new();
            for (i, (count, runs)) in chapters.into_iter().enumerate() {
                let chapter = u16::try_from(i + 1).unwrap();
                counts.push(count);
                let mut cuts: Vec<u16> = runs.iter().map(|r| r.0.min(count)).collect();
                cuts.push(count);
                cuts.sort_unstable();
                cuts.dedup();
                let mut first = 1;
                for (cut, (_, moves, target_book, offset)) in cuts.into_iter().zip(runs) {
                    if cut >= first && moves {
                        let source = MappedRange::span(book, chapter, first, cut);
                        let target = MappedRange::span(
                            target_book,
                            fresh_chapter,
                            first + offset,
                            cut + offset,
                        );
                        mappings.push(Mapping::new(source, vec![target]).unwrap());
                        fresh_chapter += 1;
                    }
                    first = cut + 1;
                }
            }
            max_verses.push((book, counts));
        }
        Versification::new(max_verses, mappings).unwrap()
    })
}

/// The verses of a scheme: every verse 0..=last of every chapter.
fn verses(v: &Versification) -> Vec<VerseRef> {
    let mut out = Vec::new();
    for book in v.books() {
        for (i, &last) in v.verse_counts(book).unwrap().iter().enumerate() {
            let chapter = u16::try_from(i + 1).unwrap();
            out.extend((0..=last).map(|n| VerseRef::new(book, chapter, n)));
        }
    }
    out
}

fn verses_of(r: &MappedRange) -> impl Iterator<Item = VerseRef> + '_ {
    (r.first..=r.end()).map(|n| VerseRef::new(r.book, r.chapter, n))
}

fn overlap(a: &MappedRange, b: &MappedRange) -> bool {
    (a.book, a.chapter) == (b.book, b.chapter) && a.first <= b.end() && b.first <= a.end()
}

/// A static reading of the table, independent of the mapping code: the verses that a
/// lossy entry touches. An entry is lossy when it has several targets, a sub-verse
/// letter, a target of a different length, a reversed source, a source that overlaps
/// another source, or a target that overlaps another target or the unmoved position of
/// a verse no entry moves.
fn touched_by_lossy_entry(v: &Versification) -> BTreeSet<VerseRef> {
    let all = verses(v);
    let entries = v.mappings();
    let moved: BTreeSet<VerseRef> = entries.iter().flat_map(|m| verses_of(m.source())).collect();
    let unmoved: BTreeSet<VerseRef> = all.iter().copied().filter(|x| !moved.contains(x)).collect();
    let targeted: BTreeSet<VerseRef> = entries
        .iter()
        .flat_map(|m| m.targets().iter().flat_map(verses_of))
        .collect();
    let extent = |r: &MappedRange| i32::from(r.end()) - i32::from(r.first);
    let lossy: Vec<&Mapping> = entries
        .iter()
        .enumerate()
        .filter(|&(i, m)| {
            let s = m.source();
            let [t] = m.targets() else { return true };
            t.part.is_some()
                || s.end() < s.first
                || extent(t) != extent(s)
                || verses_of(t).any(|x| unmoved.contains(&x))
                || entries.iter().enumerate().any(|(j, o)| {
                    i != j
                        && (overlap(s, o.source()) || o.targets().iter().any(|ot| overlap(t, ot)))
                })
        })
        .map(|(_, m)| m)
        .collect();
    all.into_iter()
        .filter(|x| {
            lossy.iter().any(|m| m.source().contains(*x))
                || (unmoved.contains(x) && targeted.contains(x))
        })
        .collect()
}

fn assert_round_trips(scheme: &Scheme, skip: &BTreeSet<VerseRef>) -> Result<usize, TestCaseError> {
    let mut checked = 0;
    for x in verses(scheme.versification()) {
        if skip.contains(&x) {
            continue;
        }
        let forward = scheme.to_org(x);
        prop_assert_eq!(forward.target_count(), 1, "{} to org", x);
        let back = scheme.from_org(forward.first());
        prop_assert_eq!(
            back.targets().collect::<Vec<_>>(),
            vec![x],
            "{} -> {} -> back",
            x,
            forward.first()
        );
        checked += 1;
    }
    Ok(checked)
}

proptest! {
    #[test]
    fn json_round_trips_or_refuses_unapplied_mappings(v in versification()) {
        match v.to_json() {
            Ok(text) => prop_assert_eq!(Versification::from_json(&text).unwrap(), v),
            Err(e) => {
                prop_assert!(matches!(e, WriteError::UnappliedMapping(_)), "{}", e);
                prop_assert!(!v.unapplied_mappings().is_empty());
            }
        }
    }

    #[test]
    fn vrs_round_trips_or_refuses_a_segment_it_cannot_write(v in versification()) {
        match v.to_vrs() {
            Ok(text) => prop_assert_eq!(Versification::from_vrs(&text).unwrap(), v),
            Err(WriteError::Segment { verse, segment }) => {
                let parts = v
                    .partial_verses()
                    .iter()
                    .find(|(at, _)| *at == verse)
                    .map(|(_, parts)| parts.as_slice())
                    .unwrap_or_default();
                prop_assert!(breaks_a_vrs_line(parts, &segment), "{:?} in {:?}", segment, parts);
            }
            Err(e) => prop_assert!(false, "unexpected refusal: {}", e),
        }
    }

    #[test]
    fn vrs_writes_every_segment_it_can(v in versification()) {
        let writable = v
            .partial_verses()
            .iter()
            .all(|(_, parts)| parts.iter().all(|p| !breaks_a_vrs_line(parts, p)));
        prop_assert_eq!(v.to_vrs().is_ok(), writable);
    }

    #[test]
    fn a_scheme_with_no_lossy_entry_maps_to_org_and_back_unchanged(v in lossless_scheme()) {
        prop_assert!(touched_by_lossy_entry(&v).is_empty());
        let scheme = Scheme::new("gen", v).unwrap();
        assert_round_trips(&scheme, &BTreeSet::new())?;
        for x in verses(scheme.versification()).into_iter().filter(|x| x.verse > 0) {
            let m = map_verse(&scheme, &scheme, x);
            prop_assert_eq!(m.exact(), Some(x), "{:?}", m.losses);
        }
    }

    #[test]
    fn verses_no_lossy_entry_touches_map_to_org_and_back_unchanged(v in versification()) {
        let Ok(scheme) = Scheme::new("gen", v) else {
            // A random table can exceed what Proskomma encodes; that refusal is tested
            // elsewhere.
            return Ok(());
        };
        let skip = touched_by_lossy_entry(scheme.versification());
        assert_round_trips(&scheme, &skip)?;
    }
}

#[test]
fn standard_schemes_map_to_org_and_back_unchanged_outside_lossy_entries() {
    for which in StandardScheme::ALL {
        let scheme = which.scheme();
        let skip = touched_by_lossy_entry(scheme.versification());
        let checked = assert_round_trips(scheme, &skip).unwrap();
        assert!(
            checked > 30_000,
            "{}: only {checked} verses checked",
            which.name()
        );
    }
}

/// A value that parses prints a text that parses back to it (INV-IDS-ROUND-TRIP).
fn round_trips<T: std::str::FromStr + std::fmt::Display + PartialEq + std::fmt::Debug>(
    text: &str,
) -> Result<(), TestCaseError> {
    if let Ok(value) = text.parse::<T>() {
        let printed = value.to_string();
        prop_assert_eq!(
            printed.parse::<T>().ok(),
            Some(value),
            "{:?} printed {:?}",
            text,
            printed
        );
    }
    Ok(())
}

proptest! {
    #[test]
    fn ids_round_trip(
        verse in "[A-Za-z1-3]{3} [0-9]{1,3}:[0-9]{1,3}",
        book in "[A-Za-z0-9]{3}",
        part in "[0-9a-z,+ -]{0,5}",
    ) {
        round_trips::<VerseRef>(&verse)?;
        round_trips::<BookCode>(&book)?;
        round_trips::<tc_versification::RefPart>(&part)?;
    }
}
