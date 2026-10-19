//! Operation counts for the Align workspace (`docs/PERFORMANCE.md` rule 2, rows
//! "Aligning"): opening a verse, every edit of it, a reflow after a text edit and a
//! suggestion cost what the verse holds, whatever the project or the model has grown to.
//!
//! The aligner takes one verse and nothing of the project, so these pin the work inside that
//! verse: the words each sort puts in order, the words a reflow places and the memory a
//! suggestion reads.

// Helpers build fixed, known-valid values and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{basis, read};
use std::io::Read;
use tc_align::suggest::{Model, TrainingVerse, Work};
use tc_align::usfm::from_aligned;
use tc_align::words::position_lookups;
use tc_align::{
    Aligner, Alignment, AlignmentValue, SourceVerse, TargetVerse, TargetWord, bootstrap, reflow,
};
use tc_core::{BookId, Testament};
use tc_journal::alignment::verse_order_lookups;
use tc_usfm::Book;

const ULT_PSA: &str = "fixtures/scale/psa/en_ult/19-PSA.usfm";
const UHB_PSA: &str = "fixtures/scale/psa/hbo_uhb/19-PSA.usfm";

/// A Psalms verse as the Align workspace opens it: its stored value with the bank and every
/// card in an order other than the verse's, the Hebrew behind it and its text.
struct Verse {
    reference: String,
    source: SourceVerse,
    target: TargetVerse,
    stored: AlignmentValue,
}

fn verses() -> &'static [Verse] {
    static VERSES: std::sync::OnceLock<Vec<Verse>> = std::sync::OnceLock::new();
    VERSES.get_or_init(build)
}

fn build() -> Vec<Verse> {
    let (ult, uhb) = (read(ULT_PSA), read(UHB_PSA));
    let (ult, uhb) = (Book::parse(&ult), Book::parse(&uhb));
    ult.verses()
        .filter_map(|unit| {
            let (chapter, verses) = (unit.chapter()?, unit.verses()?);
            let source = SourceVerse::from_book(&uhb, chapter, verses).ok()?;
            let aligned = tc_usfm::parse_aligned(unit.body()).ok()?;
            let target = TargetVerse::new(&aligned.plain);
            let extracted = from_aligned(&aligned, Some(&source));
            let mut stored = bootstrap(&source, &target, basis(1));
            stored.alignments = extracted.alignments;
            stored.word_bank = extracted.word_bank;
            stored.word_bank.reverse();
            for group in &mut stored.alignments {
                group.bottom_words.reverse();
            }
            Some(Verse {
                reference: format!("{chapter}:{verses}"),
                source,
                target,
                stored,
            })
        })
        .collect()
}

/// The verses to measure: the longest, the shortest and a spread between.
fn sample(all: &[Verse]) -> Vec<&Verse> {
    let words = |v: &Verse| v.target.words().len();
    let longest = all.iter().max_by_key(|v| words(v)).unwrap();
    let shortest = all
        .iter()
        .filter(|v| v.stored.placed() > 0)
        .min_by_key(|v| words(v))
        .unwrap();
    let mut out = vec![longest, shortest];
    out.extend(all.iter().step_by(all.len() / 40));
    out
}

fn open(verse: &Verse) -> Aligner {
    Aligner::open(
        Some(&verse.stored),
        verse.target.clone(),
        &verse.source,
        basis(1),
    )
    .unwrap()
}

#[test]
fn opening_the_aligner_sorts_only_the_verse() {
    let all = verses();
    assert_eq!(all.len(), 2461);
    let mut ratios = Vec::new();
    for verse in sample(all) {
        let words = verse.target.words().len() as u64;
        let before = verse_order_lookups();
        let aligner = open(verse);
        let looked = verse_order_lookups() - before;
        std::hint::black_box(aligner);
        // The bank and the cards are each put in verse order once: every word of the verse
        // looked up at most once for each, and nothing else.
        assert!(
            looked <= 2 * words,
            "{}: {looked} lookups for {words} words",
            verse.reference
        );
        ratios.push(looked as f64 / words as f64);
    }
    // The cost per word is the same in the longest verse and the shortest.
    let (min, max) = ratios
        .iter()
        .fold((f64::MAX, 0.0f64), |(lo, hi), &r| (lo.min(r), hi.max(r)));
    assert!(max <= 2.0 && min > 0.0, "ratios {min}..{max}");
}

#[test]
fn an_aligner_edit_sorts_only_the_verse() {
    let all = verses();
    let longest = sample(all)[0];
    let words = longest.target.words().len() as u64;
    let placed: Vec<TargetWord> = longest
        .stored
        .alignments
        .iter()
        .flat_map(|g| g.bottom_words.clone())
        .collect();
    let mut aligner = open(longest);
    let mut worst = 0;
    let mut edits = 0;
    let mut measure = |f: &mut dyn FnMut()| {
        let before = verse_order_lookups();
        f();
        worst = worst.max(verse_order_lookups() - before);
        edits += 1;
    };
    // Every word back to the bank, one edit each; then all onto one card; then clear.
    for (card, group) in longest.stored.alignments.iter().enumerate() {
        for word in &group.bottom_words {
            measure(&mut || aligner.unplace(card, word).unwrap());
        }
    }
    measure(&mut || {
        aligner.place_many(0, &placed).unwrap();
    });
    measure(&mut || aligner.clear());
    assert_eq!(edits, placed.len() + 2);
    // One edit sorts the bank and the cards: each word of the verse looked up at most twice.
    assert!(
        worst <= 2 * words,
        "an edit looked up {worst} positions in a verse of {words} words"
    );
}

#[test]
fn a_reflow_costs_the_verse() {
    let all = verses();
    for verse in sample(all) {
        let mut aligner = open(verse);
        if aligner.mark_valid().is_err() {
            continue;
        }
        let saved = aligner.into_value();
        let edited = TargetVerse::new(&verse.target.text().replacen("the", "a", 1));
        let words = edited.words().len() as u64;
        let before = position_lookups();
        let carried = reflow(&saved, &edited);
        let looked = position_lookups() - before;
        drop(carried);
        // Each placed word is found in the new text once.
        assert!(
            looked <= saved.placed() as u64 + words,
            "{}: {looked} lookups, {} placed words in {words}",
            verse.reference,
            saved.placed()
        );
    }
}

/// The aligned verses of the ULT v91 export for `books`, except the one being aligned.
fn training(books: &[BookId], held_out: &str) -> Vec<TrainingVerse> {
    let path = common::fixture("bundle/en/en_ult_v91.sb.zip");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut verses = Vec::new();
    for &book in books {
        let mut text = String::new();
        archive
            .by_name(&format!("en_ult/ingredients/{}.usfm", book.code()))
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let parsed = Book::parse(&text);
        for unit in parsed.verses() {
            let (Some(c), Some(v)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            let reference = format!("{} {c}:{v}", book.code());
            if reference == held_out {
                continue;
            }
            if let Ok(Some(verse)) = TrainingVerse::from_aligned_usfm(reference, unit.body()) {
                verses.push(verse);
            }
        }
    }
    verses
}

fn memory_of(verses: &[TrainingVerse]) -> Model {
    let mut model = Model::empty(Testament::Old);
    for verse in verses {
        model.append_verse(verse, true);
    }
    model
}

#[test]
fn a_suggestion_costs_the_verse_not_the_corpus() {
    let held = "PSA 42:4";
    let small = training(&[BookId::Psalms], held);
    let large = training(
        &[
            BookId::Genesis,
            BookId::Exodus,
            BookId::Leviticus,
            BookId::Psalms,
        ],
        held,
    );
    assert!(large.len() > 2 * small.len());
    let asked = {
        let path = common::fixture("bundle/en/en_ult_v91.sb.zip");
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        let mut text = String::new();
        archive
            .by_name("en_ult/ingredients/PSA.usfm")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let parsed = Book::parse(&text);
        let unit = parsed
            .verses()
            .find(|u| {
                u.chapter() == Some(42) && u.verses().map(|v| v.to_string()) == Some("4".into())
            })
            .unwrap();
        TrainingVerse::from_aligned_usfm(held, unit.body())
            .unwrap()
            .unwrap()
    };

    // The memory of a model that learned from a corpus twice as large (and more) is read as
    // often as the small one's: the n-grams of the verse, and its candidates.
    let (small_memory, large_memory) = (memory_of(&small), memory_of(&large));
    let work = |model: &Model| -> Work { model.predict_counted(&asked.as_question()).1 };
    let (a, b) = (work(&small_memory), work(&large_memory));
    assert_eq!(a, b, "the corpus changed what a suggestion reads");
    assert!(a.candidates > 1_000, "{a:?}");
    // And it is the verse's: source n-grams (3 lengths) times target n-grams (5 lengths)
    // and the empty one.
    let (s, t) = (asked.source().len(), asked.target().len());
    let ngrams = |n: usize, max: usize| (1..=max.min(n)).map(|len| n - len + 1).sum::<usize>();
    assert_eq!(
        a.candidates,
        ngrams(s, 3) * (ngrams(t, 5) + 1),
        "{s} source and {t} target words"
    );
    assert!(a.memory_reads <= 2 * a.candidates + 2 * (ngrams(s, 3) + ngrams(t, 5) + 1));

    // A booster walks each tree once for each candidate.
    let boosted = Model::train(Testament::Old, &small[..40], 0);
    assert!(boosted.boosted() > 0, "a booster fitted on 40 verses");
    let (_, w) = boosted.predict_counted(&asked.as_question());
    assert_eq!(
        w.tree_walks,
        w.candidates * boosted.booster().unwrap().trees()
    );
}
