//! The Align workspace on Psalms (`fixtures/scale/psa/`), the book docs/PERFORMANCE.md
//! measures: every verse opens with its bank and cards in verse order, left-to-right
//! English aligned to right-to-left Hebrew and the Hebrew itself as the translation; and
//! opening and editing the longest verse meet the aligner's budgets. Run
//! `cargo test --release -p tc-align --test psalms -- --nocapture` to see times.
//!
//! The aligner takes one verse and nothing of the project, so its cost cannot grow with
//! the project; the budgets bound the cost of the largest verse.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::{basis, in_tc4_source_order, read};
use std::time::{Duration, Instant};
use tc_align::{Aligner, AlignmentValue, SourceVerse, TargetVerse, TargetWord, reflow};
use tc_journal::alignment::tc4_cards;
use tc_usfm::Book;

const ULT_PSA: &str = "fixtures/scale/psa/en_ult/19-PSA.usfm";
const UHB_PSA: &str = "fixtures/scale/psa/hbo_uhb/19-PSA.usfm";

/// One verse of Psalms: its reference, the Hebrew words behind it and, for the ULT, its
/// aligned body.
struct Verse {
    reference: String,
    source: SourceVerse,
    body: String,
}

/// Every verse of `file`, with the UHB words behind it.
fn psalms(file: &str) -> Vec<Verse> {
    let (text, uhb) = (read(file), read(UHB_PSA));
    let (book, uhb) = (Book::parse(&text), Book::parse(&uhb));
    book.verses()
        .filter_map(|unit| {
            let (chapter, verses) = (unit.chapter()?, unit.verses()?);
            Some(Verse {
                reference: format!("{chapter}:{verses}"),
                source: SourceVerse::from_book(&uhb, chapter, verses).unwrap(),
                body: unit.body().to_owned(),
            })
        })
        .collect()
}

/// A ULT verse as tC4 reads an aligned verse (word-aligner's `unmerge`), made against the
/// verse's text, with its bank and every card's words reversed: a value saved in another
/// order, as old tC4 saves and imports have.
fn stored(verse: &Verse) -> (AlignmentValue, TargetVerse) {
    let aligned = tc_usfm::parse_aligned(&verse.body).unwrap();
    let target = TargetVerse::new(&aligned.plain);
    let mut value = bootstrap(&verse.source, &target, basis(1));
    let cards = tc4_cards(&aligned.groups);
    let placed: Vec<&TargetWord> = cards.iter().flat_map(|c| &c.bottom_words).collect();
    value.word_bank = target
        .words()
        .iter()
        .filter(|w| !placed.contains(w))
        .cloned()
        .collect();
    value.alignments = in_tc4_source_order(&verse.source, cards);
    value.word_bank.reverse();
    for group in &mut value.alignments {
        group.bottom_words.reverse();
    }
    (value, target)
}

/// Every word has a place in `verse` and they stand in that order.
fn in_verse_order(verse: &TargetVerse, words: &[TargetWord]) -> bool {
    let places: Option<Vec<usize>> = words.iter().map(|w| verse.position(w)).collect();
    places.is_some_and(|p| p.windows(2).all(|w| w[0] < w[1]))
}

fn shown_in_verse_order(aligner: &Aligner) -> bool {
    let (value, verse) = (aligner.value(), aligner.verse());
    in_verse_order(verse, &value.word_bank)
        && value
            .alignments
            .iter()
            .all(|g| in_verse_order(verse, &g.bottom_words))
}

/// The cards that hold links, each as it prints, in a fixed order.
fn linked_cards(value: &AlignmentValue) -> Vec<String> {
    let mut cards: Vec<String> = value
        .alignments
        .iter()
        .filter(|g| !g.bottom_words.is_empty())
        .map(|g| format!("{g:?}"))
        .collect();
    cards.sort();
    cards
}

#[test]
fn every_aligned_ult_verse_opens_in_verse_order_with_nothing_to_save() {
    let verses = psalms(ULT_PSA);
    assert_eq!(verses.len(), 2461);
    let mut spare_cards = 0;
    for verse in &verses {
        let at = &verse.reference;
        let (stored, target) = stored(verse);
        let aligner = Aligner::open(Some(&stored), None, target, &verse.source, basis(1))
            .unwrap_or_else(|e| panic!("{at}: {e}"));
        assert!(shown_in_verse_order(&aligner), "{at}");
        assert!(!aligner.is_changed(), "{at}: order alone is not saved");
        assert!(aligner.dropped().is_empty(), "{at}");
        let mut expected = stored.clone();
        expected.bank_in_verse_order(aligner.verse());
        expected.cards_in_verse_order(aligner.verse());
        assert_eq!(aligner.value().word_bank, expected.word_bank, "{at}");
        assert_eq!(
            linked_cards(aligner.value()),
            linked_cards(&expected),
            "{at}: every link is kept"
        );
        // A word the ULT writes in another order of its marks than the UHB has one card, not
        // an empty one beside the linked one (#790).
        assert!(
            aligner.value().alignments.len() <= stored.alignments.len(),
            "{at}"
        );
        spare_cards += stored.alignments.len() - aligner.value().alignments.len();
    }
    assert_eq!(spare_cards, 324);
}

#[test]
fn every_hebrew_verse_as_the_translation_opens_and_edits_in_reading_order() {
    let verses = psalms(UHB_PSA);
    assert_eq!(verses.len(), 2461);
    let (mut regrouped, mut repeated) = (0, 0);
    for verse in &verses {
        let at = &verse.reference;
        let target = TargetVerse::new(&verse.body);
        let words = target.words().to_vec();
        if target.bank_order() != words {
            regrouped += 1;
        }
        if words.iter().any(|w| w.occurrences > 1) {
            repeated += 1;
        }
        let mut aligner = Aligner::open(None, None, target, &verse.source, basis(1))
            .unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!(aligner.value().word_bank, words, "{at}: the first view");

        // Every word dropped on the first card, last word first.
        let mut reversed = words.clone();
        reversed.reverse();
        assert_eq!(aligner.place_many(0, &reversed), Ok(words.len()), "{at}");
        assert_eq!(aligner.value().alignments[0].bottom_words, words, "{at}");

        // Every other word back to the bank, last word first.
        for word in reversed.iter().skip(1).step_by(2) {
            aligner.unplace(0, word).unwrap();
            assert!(shown_in_verse_order(&aligner), "{at}");
        }
        aligner.clear();
        assert_eq!(aligner.value().word_bank, words, "{at}: after clear");
    }
    // Each verse with a repeated word is one that tC4's bootstrap banks out of verse order.
    assert_eq!((regrouped, repeated), (122, 122));
}

/// The ULT verse of Psalms with the most words.
fn longest(verses: &[Verse]) -> &Verse {
    verses
        .iter()
        .max_by_key(|v| tc_usfm::parse_aligned(&v.body).unwrap().words.len())
        .unwrap()
}

/// The median and p95 of `runs` timings that `f` takes and returns, after one untimed
/// run.
fn time(runs: usize, mut f: impl FnMut() -> Duration) -> (Duration, Duration) {
    f();
    let mut times: Vec<Duration> = (0..runs).map(|_| f()).collect();
    times.sort();
    (
        times[times.len() / 2],
        times[(times.len() * 95).div_ceil(100) - 1],
    )
}

/// How long `f` takes.
fn timed<T>(f: impl FnOnce() -> T) -> Duration {
    let start = Instant::now();
    std::hint::black_box(f());
    start.elapsed()
}

#[test]
fn opening_editing_and_reflowing_the_longest_psalms_verse_meet_the_budgets() {
    let verses = psalms(ULT_PSA);
    let verse = longest(&verses);
    let (stored, target) = stored(verse);
    let words = target.words().len();
    // A debug build only checks that the code runs.
    let runs = if cfg!(debug_assertions) { 1 } else { 21 };

    let open =
        || Aligner::open(Some(&stored), None, target.clone(), &verse.source, basis(1)).unwrap();
    let (open_median, open_p95) = time(runs, || timed(open));

    // The edits a translator makes on the verse: every word back to the bank and onto
    // one card in one drop, then the verse cleared.
    let placed: Vec<TargetWord> = stored
        .alignments
        .iter()
        .flat_map(|g| g.bottom_words.clone())
        .collect();
    let (edit_median, edit_p95) = time(runs, || {
        let mut aligner = open();
        timed(|| {
            for (card, group) in stored.alignments.iter().enumerate() {
                for word in &group.bottom_words {
                    aligner.unplace(card, word).unwrap();
                }
            }
            aligner.place_many(0, &placed).unwrap();
            aligner.clear();
            aligner
        })
    });

    // A one-word edit of the verse text: the alignment carried over.
    let edited = TargetVerse::new(&target.text().replacen("the", "a", 1));
    let mut saved = open();
    saved.mark_valid().unwrap();
    let saved = saved.into_value();
    let (reflow_median, reflow_p95) = time(runs, || timed(|| reflow(&saved, &edited).unwrap()));

    eprintln!(
        "PSA {} ({words} words, {} cards): open median {open_median:?} p95 {open_p95:?}; \
         {} edits median {edit_median:?} p95 {edit_p95:?}; reflow median {reflow_median:?} \
         p95 {reflow_p95:?}",
        verse.reference,
        stored.alignments.len(),
        placed.len() + 2,
    );
    // Budgets: open 30 ms, reflow 2 ms (docs/PERFORMANCE.md, Aligning). Tests fail only
    // above three times a budget, because test machines vary (rule 5).
    if !cfg!(debug_assertions) {
        assert!(
            open_p95 < Duration::from_millis(3 * 30),
            "open p95 {open_p95:?}"
        );
        assert!(
            reflow_p95 < Duration::from_millis(3 * 2),
            "reflow p95 {reflow_p95:?}"
        );
    }
}
