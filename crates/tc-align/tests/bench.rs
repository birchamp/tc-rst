//! The suggestion engine's ramp-up and timing bench, tC4's
//! `test/align-suggest-growth-bench.test.ts` protocol: a seeded held-out set of 100
//! verses (10 for Titus), the rest fed one verse at a time in canonical order, and at each
//! checkpoint the exact-position recall of plain wordMAP memory and of a booster retrained
//! there, with the time each step takes.
//!
//! Opt-in. With `TC_ALIGN_BENCH_CORPUS` naming a folder of aligned USFM books (such as an
//! unfoldingWord en_ult release), it reads every book there; without it, en_ult Titus.
//! Boosters are retrained up to `TC_ALIGN_BENCH_BOOST_MAX` verses (default 250). Run:
//!
//! ```sh
//! TC_ALIGN_BENCH_CORPUS=/path/to/en_ult cargo test -p tc-align --release --test bench -- --ignored --nocapture
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::Instant;
use tc_align::suggest::{Model, TrainingVerse};
use tc_core::{BookId, Testament};
use tc_usfm::Book;

/// mulberry32, as tC4's bench draws its held-out set.
fn mulberry32(seed: u32) -> impl FnMut() -> f64 {
    let mut a = seed;
    move || {
        a = a.wrapping_add(0x6d2b_79f5);
        let mut t = a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

fn corpus() -> (String, Vec<TrainingVerse>) {
    let mut files: Vec<(BookId, String)> = match std::env::var("TC_ALIGN_BENCH_CORPUS") {
        Ok(dir) => std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| {
                let path = e.unwrap().path();
                let name = path.file_stem()?.to_str()?.to_owned();
                let code = name.rsplit('-').next()?.to_uppercase();
                let book: BookId = code.parse().ok()?;
                Some((book, std::fs::read_to_string(&path).ok()?))
            })
            .collect(),
        Err(_) => vec![(BookId::Titus, common::read(common::ULT))],
    };
    files.sort_by_key(|(book, _)| *book);
    let label = files
        .iter()
        .map(|(b, _)| b.code())
        .collect::<Vec<_>>()
        .join(" ");
    let mut verses = Vec::new();
    for (book, text) in &files {
        if book.testament() != Testament::New {
            continue;
        }
        let parsed = Book::parse(text);
        for unit in parsed.verses() {
            let (Some(c), Some(v)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            if let Ok(Some(verse)) =
                TrainingVerse::from_aligned_usfm(format!("{} {c}:{v}", book.code()), unit.body())
            {
                verses.push(verse);
            }
        }
    }
    (label, verses)
}

#[test]
#[ignore = "a bench: run with --release --ignored --nocapture"]
fn suggestion_growth_bench() {
    let (label, all) = corpus();
    let held_count = if all.len() > 1000 { 100 } else { 10 };
    let boost_max: usize = std::env::var("TC_ALIGN_BENCH_BOOST_MAX")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(250);
    let mut draw = mulberry32(1);
    let mut drawn: Vec<(usize, f64)> = (0..all.len()).map(|i| (i, draw())).collect();
    drawn.sort_by(|a, b| a.1.total_cmp(&b.1));
    let held: Vec<&str> = drawn[..held_count]
        .iter()
        .map(|&(i, _)| all[i].reference())
        .collect();
    let held_out: Vec<&TrainingVerse> = all
        .iter()
        .filter(|v| held.contains(&v.reference()))
        .collect();
    let training: Vec<TrainingVerse> = all
        .iter()
        .filter(|v| !held.contains(&v.reference()))
        .cloned()
        .collect();
    println!(
        "[bench {label}] {} aligned verses, {} held out, {} to train on",
        all.len(),
        held_out.len(),
        training.len()
    );
    let recall = |model: &Model| {
        let started = Instant::now();
        let (mut correct, mut gold) = (0usize, 0usize);
        for verse in &held_out {
            let key = |s: &[usize], t: &[usize]| {
                let (mut s, mut t) = (s.to_vec(), t.to_vec());
                s.sort_unstable();
                t.sort_unstable();
                (s, t)
            };
            let got: Vec<_> = model
                .predict(&verse.as_question())
                .iter()
                .map(|l| key(&l.source, &l.target))
                .collect();
            correct += verse
                .links()
                .iter()
                .filter(|g| got.contains(&key(&g.source, &g.target)))
                .count();
            gold += verse.links().len();
        }
        let per_verse = started.elapsed().as_secs_f64() * 1000.0 / held_out.len() as f64;
        (correct as f64 / gold as f64, per_verse)
    };
    println!(
        "| verses aligned | plain wordMAP | retrained booster | retrain time | append ms/verse | ms per suggestion plain / boosted |"
    );
    println!("|---|---|---|---|---|---|");
    let mut plain = Model::empty(Testament::New);
    let mut fed = 0;
    let mut append_seconds = 0.0;
    let checkpoints = [
        1,
        5,
        10,
        25,
        50,
        100,
        250,
        500,
        1000,
        2500,
        5000,
        training.len(),
    ];
    for checkpoint in checkpoints.into_iter().filter(|&c| c <= training.len()) {
        while fed < checkpoint {
            let started = Instant::now();
            plain.append_verse(&training[fed], true);
            append_seconds += started.elapsed().as_secs_f64();
            fed += 1;
        }
        let (plain_recall, plain_ms) = recall(&plain);
        let boosted = (checkpoint <= boost_max).then(|| {
            let started = Instant::now();
            let model = Model::train(Testament::New, &training[..checkpoint], 0);
            let seconds = started.elapsed().as_secs_f64();
            let (r, ms) = recall(&model);
            (r, seconds, ms)
        });
        let (boost_recall, boost_time, boost_ms) = match boosted {
            Some((r, s, ms)) => (format!("{r:.3}"), format!("{s:.1} s"), format!("{ms:.0}")),
            None => ("–".into(), "–".into(), "–".into()),
        };
        println!(
            "| {checkpoint} | {plain_recall:.3} | {boost_recall} | {boost_time} | {:.3} | {plain_ms:.0} / {boost_ms} |",
            append_seconds * 1000.0 / fed as f64
        );
    }
}
