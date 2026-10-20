//! PERFORMANCE's suggestion budget on ULT v91's NT, Genesis and Psalms alignments.
//! Run the release probe with `cargo test -p tc-align --release --test suggestion_budget
//! -- --ignored --nocapture`. The fixture and training are outside the prediction timing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::time::{Duration, Instant};

use tc_align::suggest::{Model, TrainingVerse};
use tc_core::{BookId, Testament};
use tc_usfm::Book;

fn corpus() -> (Vec<TrainingVerse>, Vec<TrainingVerse>) {
    let archive = std::fs::File::open(common::fixture("bundle/en/en_ult_v91.sb.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(archive).unwrap();
    let mut books = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).unwrap();
        let Some(stem) = file
            .name()
            .rsplit('/')
            .next()
            .unwrap()
            .strip_suffix(".usfm")
        else {
            continue;
        };
        let Ok(book) = stem.rsplit('-').next().unwrap().parse::<BookId>() else {
            continue;
        };
        if book.testament() != Testament::New && !matches!(book, BookId::Genesis | BookId::Psalms) {
            continue;
        }
        let mut text = String::new();
        file.read_to_string(&mut text).unwrap();
        books.push((book, text));
    }
    books.sort_by_key(|(book, _)| *book);
    let present: BTreeSet<_> = books.iter().map(|(book, _)| *book).collect();
    assert_eq!(
        present.len(),
        29,
        "the fixture must contain every NT book, GEN and PSA"
    );
    let mut old = Vec::new();
    let mut new = Vec::new();
    for (book, text) in books {
        let parsed = Book::parse(&text);
        let target = if book.testament() == Testament::New {
            &mut new
        } else {
            &mut old
        };
        for unit in parsed.verses() {
            let (Some(chapter), Some(verse)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            let reference = format!("{} {chapter}:{verse}", book.code());
            if let Some(verse) = TrainingVerse::from_aligned_usfm(&reference, unit.body())
                .unwrap_or_else(|error| panic!("{reference}: {error}"))
            {
                target.push(verse);
            }
        }
    }
    assert!(old.len() > 3_000, "{} OT aligned verses", old.len());
    assert!(new.len() > 6_000, "{} NT aligned verses", new.len());
    (old, new)
}

#[test]
#[ignore = "release performance probe on the full NT + GEN + PSA fixture"]
#[allow(
    clippy::assertions_on_constants,
    reason = "The timing probe must refuse a run without --release."
)]
fn suggestions_meet_the_full_training_corpus_budget() {
    assert!(!cfg!(debug_assertions), "measure the release build");
    let (old, new) = corpus();
    for (testament, verses) in [(Testament::Old, old), (Testament::New, new)] {
        let question = verses
            .iter()
            .filter(|verse| testament != Testament::Old || verse.reference().starts_with("PSA "))
            .max_by_key(|verse| verse.source().len() * verse.target().len())
            .unwrap();
        let input = question.as_question();
        let started = Instant::now();
        let model = Model::train(testament, &verses, 0);
        let training = started.elapsed();
        assert_eq!(
            model.verses(),
            verses.len(),
            "every aligned verse must train"
        );
        let warm = model.predict(&input);
        assert!(
            !warm.is_empty(),
            "{} must produce a real suggestion",
            question.reference()
        );
        std::hint::black_box(warm);
        let mut times = Vec::new();
        for _ in 0..20 {
            let started = Instant::now();
            std::hint::black_box(model.predict(&input));
            times.push(started.elapsed());
        }
        times.sort();
        let median = (times[9] + times[10]) / 2;
        let p95 = times[18];
        println!(
            "{testament:?}: {} aligned verses, question {}, source={} target={}, training={:.3}s median={:.3}ms p95={:.3}ms",
            verses.len(),
            question.reference(),
            input.source().len(),
            input.target().len(),
            training.as_secs_f64(),
            median.as_secs_f64() * 1000.0,
            p95.as_secs_f64() * 1000.0
        );
        assert!(
            p95 <= Duration::from_millis(150),
            "CI threshold is three times the 50 ms budget: {p95:?}"
        );
    }
}
