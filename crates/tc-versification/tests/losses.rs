//! Every way a mapping loses information is reported. Each case here is a real entry of
//! a standard scheme.

// Test helpers fail the test by panicking.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::path::PathBuf;
use tc_versification::{
    Absence, Loss, Scheme, StandardScheme, VerseMapping, VerseRef, Versification, map_verse,
};

fn verse(s: &str) -> VerseRef {
    s.parse().unwrap()
}

fn std_map(from: StandardScheme, to: StandardScheme, v: &str) -> VerseMapping {
    map_verse(from.scheme(), to.scheme(), verse(v))
}

/// Proskomma's own `.vrs` copy of a scheme, which keeps one-to-many entries.
fn proskomma(name: &str) -> Scheme {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../fixtures/versification/vrs/proskomma/{name}.vrs"
    ));
    let text = std::fs::read_to_string(path).unwrap();
    Scheme::new(name, Versification::from_vrs(&text).unwrap()).unwrap()
}

fn has(m: &VerseMapping, pred: impl Fn(&Loss) -> bool) -> bool {
    m.losses.iter().any(pred)
}

#[test]
fn an_ordinary_shift_is_lossless() {
    let m = std_map(StandardScheme::Eng, StandardScheme::Rsc, "JON 1:17");
    assert_eq!(m.losses, vec![]);
    assert_eq!(m.exact(), Some(verse("JON 2:1")));
    assert_eq!(m.landings[0].pivot, verse("JON 2:1"));
}

#[test]
fn verse_zero_is_reported_missing() {
    let m = std_map(StandardScheme::Eng, StandardScheme::Rsc, "PSA 116:10");
    assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("PSA 115:0")]);
    assert!(m.losses.contains(&Loss::Missing {
        verse: verse("PSA 115:0"),
        absence: Absence::VerseZero
    }));
    assert_eq!(m.exact(), None);
}

#[test]
fn past_the_chapter_end_is_reported_missing() {
    let m = std_map(StandardScheme::Eng, StandardScheme::Rsc, "ACT 19:41");
    assert!(m.losses.contains(&Loss::Missing {
        verse: verse("ACT 19:41"),
        absence: Absence::PastChapterEnd
    }));
}

#[test]
fn a_missing_book_is_reported() {
    let m = std_map(StandardScheme::Eng, StandardScheme::Vul, "EST 1:1");
    assert!(has(&m, |l| matches!(
        l,
        Loss::Missing {
            absence: Absence::NoChapter,
            ..
        }
    )));
}

#[test]
fn many_to_one_is_not_reversible() {
    // eng PSA 13:5 and 13:6 both become org PSA 13:6. Going back, org PSA 13:6 can only
    // give one of them; the other mapping says it cannot be reversed.
    let five = std_map(StandardScheme::Eng, StandardScheme::Org, "PSA 13:5");
    let six = std_map(StandardScheme::Eng, StandardScheme::Org, "PSA 13:6");
    for m in [&five, &six] {
        assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("PSA 13:6")]);
    }
    let back = map_verse(
        StandardScheme::Org.scheme(),
        StandardScheme::Eng.scheme(),
        verse("PSA 13:6"),
    );
    let back = back.targets().collect::<Vec<_>>();
    assert_eq!(back.len(), 1);
    let (kept, lost) = if back[0] == five.source {
        (&five, &six)
    } else {
        (&six, &five)
    };
    assert!(!has(kept, |l| matches!(l, Loss::NotReversible { .. })));
    assert!(lost.losses.contains(&Loss::NotReversible { back }));
}

#[test]
fn composing_a_scheme_with_itself_can_move_a_verse_and_says_so() {
    // tC4's reason for never mapping the same frame.
    let m = std_map(StandardScheme::Eng, StandardScheme::Eng, "NEH 7:68");
    assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("NEH 7:69")]);
    assert!(has(&m, |l| matches!(l, Loss::NotReversible { .. })));
}

#[test]
fn one_to_many_is_a_fan_out() {
    // Proskomma's eng.vrs lists `PSA 51:0 = PSA 51:1` and `PSA 51:0 = PSA 51:2`.
    let eng = proskomma("eng");
    let org = StandardScheme::Org.scheme();
    let m = map_verse(&eng, org, verse("PSA 51:0"));
    assert_eq!(m.landings.len(), 2);
    assert_eq!(
        m.targets().collect::<Vec<_>>(),
        vec![verse("PSA 51:1"), verse("PSA 51:2")]
    );
    assert!(m.losses.contains(&Loss::FanOut { count: 2 }));
}

#[test]
fn a_partial_verse_target_is_reported() {
    // eng `ESG 1:2 = ESG 1:1b`: the whole of org ESG 1:1 is used.
    let m = std_map(StandardScheme::Eng, StandardScheme::Org, "ESG 1:2");
    assert!(has(
        &m,
        |l| matches!(l, Loss::PartialVerse { rule } if rule.to_string() == "ESG 1:2 = ESG 1:1b")
    ));
}

#[test]
fn a_length_mismatch_is_reported() {
    // rsc `PSA 141:0 = PSA 142:0-1`: Proskomma lands on 142:0 only.
    let m = std_map(StandardScheme::Rsc, StandardScheme::Org, "PSA 141:0");
    assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("PSA 142:0")]);
    assert!(has(
        &m,
        |l| matches!(l, Loss::LengthMismatch { rule } if rule.to_string() == "PSA 141:0 = PSA 142:0-1")
    ));
}

#[test]
fn a_shadowed_rule_is_reported() {
    // org `S3Y 1:1-29 = DAG 3:24-52` and `S3Y 1:30-31 = DAG 3:52-53`: going back from
    // DAG 3:52 Proskomma takes the first and never reaches S3Y 1:30.
    let org = StandardScheme::Org.scheme();
    let m = map_verse(org, org, verse("S3Y 1:30"));
    assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("S3Y 1:29")]);
    assert!(has(
        &m,
        |l| matches!(l, Loss::Shadowed { rule } if rule.to_string() == "DAG 3:52-53 = S3Y 1:30-31")
    ));
    assert!(has(&m, |l| matches!(l, Loss::NotReversible { .. })));
}

#[test]
fn a_source_verse_the_scheme_lacks_is_reported() {
    // eng has no EZR 14; Proskomma maps the verse to itself anyway.
    let m = std_map(StandardScheme::Eng, StandardScheme::Lxx, "EZR 14:18");
    assert_eq!(m.targets().collect::<Vec<_>>(), vec![verse("EZR 14:18")]);
    assert!(m.losses.contains(&Loss::SourceMissing {
        absence: Absence::NoChapter
    }));
    assert_eq!(m.exact(), None);
    let m = std_map(StandardScheme::Eng, StandardScheme::Org, "JON 1:18");
    assert!(m.losses.contains(&Loss::SourceMissing {
        absence: Absence::PastChapterEnd
    }));
    let m = std_map(StandardScheme::Org, StandardScheme::Org, "PSA 3:0");
    assert!(m.losses.contains(&Loss::SourceMissing {
        absence: Absence::VerseZero
    }));
}

#[test]
fn excluded_verses_are_reported_on_both_sides() {
    // Proskomma's lxx.vrs excludes GEN 31:51.
    let lxx = proskomma("lxx");
    let org = StandardScheme::Org.scheme();
    assert!(lxx.is_excluded(verse("GEN 31:51")));
    assert!(lxx.versification().contains(verse("GEN 31:51")));
    let into = map_verse(org, &lxx, verse("GEN 31:51"));
    assert!(into.losses.contains(&Loss::Excluded {
        verse: verse("GEN 31:51")
    }));
    assert_eq!(into.exact(), None);
    let out = map_verse(&lxx, org, verse("GEN 31:51"));
    assert!(out.losses.contains(&Loss::SourceExcluded));
    assert_eq!(out.exact(), None);
    // Every excluded verse of lxx is reported, as a source and wherever org lands on it.
    let mut landed = 0usize;
    for &x in lxx.versification().excluded_verses() {
        assert!(
            map_verse(&lxx, org, x)
                .losses
                .contains(&Loss::SourceExcluded),
            "lxx {x}"
        );
        for pivot in lxx.to_org(x).targets() {
            let m = map_verse(org, &lxx, pivot);
            if m.targets().any(|t| t == x) {
                assert!(
                    m.losses.contains(&Loss::Excluded { verse: x }),
                    "org {pivot} -> lxx {x}"
                );
                landed += 1;
            }
        }
    }
    assert!(landed > 200, "only {landed} landings on excluded verses");
}

#[test]
fn every_lossy_verse_of_every_standard_pair_says_why() {
    // The contract: a mapping with no reported loss lands on exactly one verse that exists
    // in the target and maps back to the source alone.
    let mut lossy = 0usize;
    for from in StandardScheme::ALL {
        for to in StandardScheme::ALL {
            let (f, t) = (from.scheme(), to.scheme());
            let v = f.versification();
            for book in v.books() {
                for (i, &last) in v.verse_counts(book).unwrap().iter().enumerate() {
                    let chapter = u16::try_from(i + 1).unwrap();
                    // Verse 0 and the verse past the end are not in the source scheme.
                    for missing in [0, last + 1] {
                        let m = map_verse(f, t, VerseRef::new(book, chapter, missing));
                        assert!(
                            m.losses
                                .iter()
                                .any(|l| matches!(l, Loss::SourceMissing { .. })),
                            "{from:?}>{to:?} {}",
                            m.source
                        );
                    }
                    for n in 1..=last {
                        let m = map_verse(f, t, VerseRef::new(book, chapter, n));
                        if !m.is_lossless() {
                            lossy += 1;
                            continue;
                        }
                        let targets: Vec<VerseRef> = m.targets().collect();
                        assert_eq!(targets.len(), 1, "{from:?}>{to:?} {}", m.source);
                        assert!(t.versification().contains(targets[0]));
                        let mut back: Vec<VerseRef> =
                            map_verse(t, f, targets[0]).targets().collect();
                        back.dedup();
                        assert_eq!(back, vec![m.source], "{from:?}>{to:?} {}", m.source);
                    }
                }
            }
        }
    }
    assert!(lossy > 1000, "only {lossy} lossy mappings");
}
