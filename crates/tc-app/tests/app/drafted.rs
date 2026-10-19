//! The drafted rule of Home's book tiles and the book rail (tC4 #572).

use tc_app::drafted::{BookDraft, usfm_draft};
use tc_usfm::codec::{MarkerTerm, is_drafted};

fn percent(usfm: &str) -> u8 {
    usfm_draft(usfm).expect("the book decomposes").percent()
}

#[test]
fn an_undrafted_span_counts_its_verses() {
    // tC4's vector (test/indexer.test.ts, #572): 1 of 4 verses.
    let usfm = "\\id TST\n\\c 1\n\\p\n\\v 1 uno\n\\v 2-3 ___\n\\v 4 ___\n";
    assert_eq!(
        usfm_draft(usfm).unwrap(),
        BookDraft {
            drafted: 1,
            total: 4
        }
    );
    assert_eq!(percent(usfm), 25);
}

#[test]
fn joining_verses_keeps_the_drafted_percentage() {
    let head = "\\id TST\n\\c 1\n\\p\n";
    assert_eq!(
        percent(&format!("{head}\\v 1 uno\n\\v 2 dos\n\\v 3 tres\n\\v 4 ___\n")),
        75
    );
    assert_eq!(
        percent(&format!("{head}\\v 1 uno\n\\v 2-3 dos tres\n\\v 4 ___\n")),
        75
    );
    assert_eq!(
        percent(&format!("{head}\\v 1 uno\n\\v 2 ___\n\\v 3 ___\n\\v 4 ___\n")),
        25
    );
    assert_eq!(
        percent(&format!("{head}\\v 1 uno\n\\v 2-3 ___\n\\v 4 ___\n")),
        25
    );
}

/// The declared difference: tC4 reads a verse's body up to its first paragraph marker, so a
/// poem verse whose words are on a `\q` line is undrafted there. Here it is drafted. The test
/// asserts both rules, so it fails the day either moves.
#[test]
fn a_poem_verse_with_words_counts_as_drafted() {
    let usfm = "\\id TST\n\\c 1\n\\v 1\n\\q1 Blessed is the man\n\\v 2 ___\n";
    assert_eq!(percent(usfm), 50);
    assert!(!is_drafted(MarkerTerm::Lf, "\\q1 Blessed is the man\n"));
}

#[test]
fn the_samples_are_eleven_and_four_percent() {
    let read = |book: &str| {
        std::fs::read_to_string(format!(
            "{}/../../fixtures/tc4/sample-burrito/ingredients/{book}.usfm",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let titus = usfm_draft(&read("TIT")).unwrap();
    assert_eq!((titus.drafted, titus.total, titus.percent()), (5, 46, 11));
    let jonah = usfm_draft(&read("JON")).unwrap();
    assert_eq!((jonah.drafted, jonah.total, jonah.percent()), (2, 48, 4));
}

#[test]
fn a_book_without_verses_is_zero_percent() {
    assert_eq!(percent("\\id TST\n"), 0);
}
