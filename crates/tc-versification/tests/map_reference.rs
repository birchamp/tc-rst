//! tC4's measured cases for `mapReference` (tC4 `test/mapReference.test.ts`), against the
//! bundled schemes.

// Test helpers fail the test by panicking.
#![allow(clippy::unwrap_used, clippy::panic)]

use tc_core::{Reference, VerseSpan};
use tc_versification::{
    MAX_SPAN_VERSES, MapOutcome, Scheme, SchemeReference, StandardScheme, Unplaceable,
    Versification, map_reference,
};

use StandardScheme::{Eng, Lxx, Rsc, Rso, Vul};

fn r(text: &str) -> SchemeReference {
    text.parse::<Reference>().unwrap().into()
}

fn map(from: StandardScheme, to: StandardScheme, text: &str) -> MapOutcome {
    map_reference(from.scheme(), to.scheme(), &r(text))
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
        MapOutcome::SameFrame(r("NEH 7:68"))
    );
    assert_eq!(
        map(Eng, Eng, "JON 2:9-10"),
        MapOutcome::SameFrame(r("JON 2:9-10"))
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
    let span = |last| SchemeReference {
        book: "GEN".parse().unwrap(),
        chapter: 1,
        verses: VerseSpan::range(1, last).unwrap(),
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
        verses: VerseSpan::single(1),
    };
    assert_eq!(
        map_reference(Eng.scheme(), Rsc.scheme(), &input),
        MapOutcome::Unplaceable {
            reason: Unplaceable::MalformedReference,
            candidates: vec![]
        }
    );
}

#[test]
fn deuterocanonical_books_map_like_any_other() {
    // eng S3Y (the Song of the Three) is part of vul's DAG (Greek Daniel).
    let s3y = SchemeReference {
        book: "S3Y".parse().unwrap(),
        chapter: 1,
        verses: VerseSpan::single(1),
    };
    let dag = SchemeReference {
        book: "DAG".parse().unwrap(),
        chapter: 3,
        verses: VerseSpan::single(24),
    };
    assert_eq!(
        map_reference(Eng.scheme(), Vul.scheme(), &s3y),
        MapOutcome::Mapped(dag)
    );
    assert_eq!(dag.to_reference(), None);
    assert_eq!(
        r("DAN 3:24").to_reference(),
        Some("DAN 3:24".parse().unwrap())
    );
}
