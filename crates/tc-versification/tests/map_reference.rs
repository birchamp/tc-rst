//! tC4's measured cases for `mapReference` (tC4 `test/mapReference.test.ts`), against the
//! bundled schemes.

// Test helpers fail the test by panicking.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use tc_core::{Reference, VerseNo, VerseSpan};
use tc_versification::{
    BookCode, MAX_SPAN_VERSES, MapOutcome, RefPart, RowReference, Scheme, SchemeReference,
    StandardScheme, Unplaceable, Versification, map_reference,
};

use StandardScheme::{Eng, Lxx, Org, Rsc, Rso, Vul};

fn r(text: &str) -> SchemeReference {
    text.parse::<Reference>().unwrap().into()
}

fn map(from: StandardScheme, to: StandardScheme, text: &str) -> MapOutcome {
    map_reference(from.scheme(), to.scheme(), &r(text).into())
}

/// A resource row's reference, its chapter and verse read as tC4's `refPart` reads them.
fn row(book: &str, chapter: &str, verse: &str) -> RowReference {
    RowReference {
        book: BookCode::new(book).unwrap(),
        chapter: RefPart::parse(chapter),
        verse: RefPart::parse(verse),
    }
}

fn map_row(from: StandardScheme, to: StandardScheme, reference: &RowReference) -> MapOutcome {
    map_reference(from.scheme(), to.scheme(), reference)
}

fn mapped(text: &str) -> MapOutcome {
    MapOutcome::Mapped(r(text))
}

fn reason(outcome: &MapOutcome) -> Option<Unplaceable> {
    match outcome {
        MapOutcome::Unplaceable { reason, .. } => Some(*reason),
        _ => None,
    }
}

#[test]
fn the_same_frame_never_maps() {
    // A composed eng -> org -> eng round trip would move NEH 7:68 to 7:69.
    assert_eq!(
        map(Eng, Eng, "NEH 7:68"),
        MapOutcome::SameFrame(r("NEH 7:68").into())
    );
    assert_eq!(
        map(Eng, Eng, "JON 2:9-10"),
        MapOutcome::SameFrame(r("JON 2:9-10").into())
    );
}

#[test]
fn single_verses_cross_frames() {
    // eng JON 1 has 17 verses and every other scheme 16: the frame discriminator.
    assert_eq!(map(Eng, Rsc, "JON 1:17"), mapped("JON 2:1"));
    assert_eq!(map(Eng, Lxx, "GEN 31:55"), mapped("GEN 32:1"));
    // Converted, though the numbers come out the same: eng MAL 4:1 -> org 3:19 -> vul 4:1.
    assert_eq!(map(Eng, Vul, "MAL 4:1"), mapped("MAL 4:1"));
}

#[test]
fn refusals_each_a_measured_failure() {
    assert_eq!(
        reason(&map(Eng, Rsc, "PSA 116:10")),
        Some(Unplaceable::VerseZero)
    );
    assert_eq!(
        reason(&map(Eng, Rsc, "ACT 19:41")),
        Some(Unplaceable::PastChapterEnd)
    );
    assert_eq!(
        reason(&map(Eng, Vul, "EST 1:1")),
        Some(Unplaceable::NoChapter)
    );
}

#[test]
fn spans() {
    assert_eq!(map(Eng, Rsc, "JHN 1:1-3"), mapped("JHN 1:1-3"));
    // The psalm title is verse 1 in lxx, so the whole span shifts.
    assert_eq!(map(Eng, Lxx, "PSA 3:1-2"), mapped("PSA 3:2-3"));
    let split = reason(&map(Eng, Rsc, "PSA 116:9-10"));
    assert!(matches!(
        split,
        Some(Unplaceable::SpanSplit | Unplaceable::VerseZero)
    ));
    // A span's interior is kept: 11-13 is not the gapped pair {11, 13}.
    assert_eq!(map(Eng, Rsc, "1TH 3:11-13"), mapped("1TH 3:11-13"));
    assert!(reason(&map(Eng, Rsc, "ACT 19:40-41")).is_some());
}

#[test]
fn a_contiguous_fan_out_becomes_a_span_and_a_gapped_one_is_refused() {
    let out = map(Eng, Rsc, "NEH 7:67");
    assert!(matches!(out, MapOutcome::Mapped(_)), "{out:?}");
    assert_eq!(
        reason(&map(Eng, Rso, "PSA 87:1")),
        Some(Unplaceable::Ambiguous)
    );
    // en_tn PSA 11:1-3 lands in vul at 10:1, 10:3 and 10:4; claiming 10:2 would be a
    // fabrication.
    match map(Eng, Vul, "PSA 11:1-3") {
        MapOutcome::Unplaceable {
            reason: Unplaceable::Ambiguous,
            candidates,
        } => {
            let verses: Vec<u16> = candidates.iter().map(|c| c.verse).collect();
            assert_eq!(verses, vec![1, 3, 4]);
        }
        other => panic!("expected ambiguous, got {other:?}"),
    }
}

#[test]
fn span_work_is_bounded_by_the_reference() {
    let big = Scheme::new(
        "big",
        Versification::from_json(r#"{"maxVerses":{"GEN":["65535"]}}"#).unwrap(),
    )
    .unwrap();
    let eng = Eng.scheme();
    let span = |last| -> RowReference {
        SchemeReference {
            book: "GEN".parse().unwrap(),
            chapter: 1,
            verses: VerseSpan::range(1, last).unwrap(),
        }
        .into()
    };
    assert_eq!(
        reason(&map_reference(&big, eng, &span(MAX_SPAN_VERSES + 1))),
        Some(Unplaceable::MalformedReference)
    );
    // At the bound the span is mapped, then refused by the target's real limits.
    assert_eq!(
        reason(&map_reference(&big, eng, &span(MAX_SPAN_VERSES))),
        Some(Unplaceable::PastChapterEnd)
    );
    assert!(matches!(map(Rsc, Eng, "JON 2:2-3"), MapOutcome::Mapped(_)));
}

#[test]
fn chapter_zero_is_malformed() {
    let input = SchemeReference {
        book: "GEN".parse().unwrap(),
        chapter: 0,
        verses: VerseSpan::single(VerseNo::new(1).unwrap()),
    };
    assert_eq!(
        map_reference(Eng.scheme(), Rsc.scheme(), &input.into()),
        MapOutcome::Unplaceable {
            reason: Unplaceable::MalformedReference,
            candidates: vec![]
        }
    );
}

#[test]
fn verse_zero_is_malformed_across_frames() {
    // tC4 refuses a verse below 1 before any arithmetic. Mapped, eng PSA 51:0 would land
    // on org PSA 51:2 and be written as that check's identity for good.
    let malformed = MapOutcome::Unplaceable {
        reason: Unplaceable::MalformedReference,
        candidates: vec![],
    };
    // A verse 0 cannot be built as a `VerseSpan`; written in a row, it is malformed.
    for verse in ["0", "00", "0-2"] {
        assert_eq!(map_row(Eng, Org, &row("PSA", "51", verse)), malformed);
    }
}

#[test]
fn other_verse_forms_pass_through_one_frame_and_are_malformed_across_frames() {
    // en_tn has PSA `N:front` notes, comma lists and letter verses; an eng project keeps
    // them, so they must pass through untouched.
    for (chapter, verse) in [
        ("3", "front"),
        ("5", "1,3"),
        ("1", "1a"),
        ("front", "intro"),
        ("1", "3-2"),
        ("1", "70000"),
    ] {
        let input = row("PSA", chapter, verse);
        assert_eq!(
            map_row(Eng, Eng, &input),
            MapOutcome::SameFrame(input.clone())
        );
        assert_eq!(
            map_row(Eng, Rsc, &input),
            MapOutcome::Unplaceable {
                reason: Unplaceable::MalformedReference,
                candidates: vec![]
            },
            "{input}"
        );
    }
    let front = row("PSA", "3", "front");
    assert_eq!(front.to_string(), "PSA 3:front");
    // tC4 reads plain digits as numbers, leading zeros and all.
    assert_eq!(
        map_row(Eng, Rsc, &row("JON", "1", "017")),
        mapped("JON 2:1")
    );
    assert_eq!(
        map_row(Eng, Rsc, &row("JON", "1", "016-017")),
        map(Eng, Rsc, "JON 1:16-17")
    );
}

#[test]
fn a_one_verse_span_checks_its_endpoints_in_the_source() {
    // rsc JON 1 has 16 verses. As a verse, 17 passes through the tables unchanged and
    // eng has it; as the span 17-17, tC4 first checks it in rsc and refuses it.
    assert_eq!(
        map_row(Rsc, Eng, &row("JON", "1", "17")),
        mapped("JON 1:17")
    );
    assert_eq!(
        reason(&map_row(Rsc, Eng, &row("JON", "1", "17-17"))),
        Some(Unplaceable::PastChapterEnd)
    );
    // Inside the chapter, a one-verse span maps like the verse.
    assert_eq!(
        map_row(Eng, Rsc, &row("JON", "1", "17-17")),
        mapped("JON 2:1")
    );
}

#[test]
fn deuterocanonical_books_map_like_any_other() {
    // eng S3Y (the Song of the Three) is part of vul's DAG (Greek Daniel).
    let s3y = SchemeReference {
        book: "S3Y".parse().unwrap(),
        chapter: 1,
        verses: VerseSpan::single(VerseNo::new(1).unwrap()),
    };
    let dag = SchemeReference {
        book: "DAG".parse().unwrap(),
        chapter: 3,
        verses: VerseSpan::single(VerseNo::new(24).unwrap()),
    };
    assert_eq!(
        map_reference(Eng.scheme(), Vul.scheme(), &s3y.into()),
        MapOutcome::Mapped(dag)
    );
    assert_eq!(dag.to_reference(), None);
    assert_eq!(
        r("DAN 3:24").to_reference(),
        Some("DAN 3:24".parse().unwrap())
    );
}
