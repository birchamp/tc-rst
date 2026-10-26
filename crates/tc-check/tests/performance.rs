//! Opening the Psalms check lists: reading the tN and TWL v91 tables of Psalms
//! (`fixtures/scale/psa/`) and deriving both check lists for a whole-book `eng` project,
//! against the Check screen's budget of 250 ms (docs/PERFORMANCE.md). Run
//! `cargo test --release -p tc-check --test performance -- --nocapture` to see times.
//!
//! The derivation takes one book's rows and nothing of the project, so its cost cannot
//! grow with the project; the count test shows it visits each row once.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeSet;
use std::io::Read;
use std::time::{Duration, Instant};

use common::{eng, fixture};
use tc_check::derive::{CheckList, notes_for_book, word_links_for_book};
use tc_core::BookId;
use tc_helps::WordArticleId;
use tc_helps::metadata::parse_burrito_metadata;
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};

const PSA: TsvScope = TsvScope::Book(BookId::Psalms);

/// The articles of the en_tw v91 pin, which gate the tW checks.
fn tw_articles() -> BTreeSet<WordArticleId> {
    let path = common::fixture_path("../bundle/en/en_tw_v91.sb.zip");
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut text = String::new();
    archive
        .by_name("en_tw/metadata.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    parse_burrito_metadata(&text).unwrap().word_articles()
}

/// Reads both Psalms tables and derives both lists, as the Check screen does on open.
fn open(tn: &str, twl: &str, articles: &BTreeSet<WordArticleId>) -> (CheckList, CheckList) {
    let spec = eng(BookId::Psalms);
    let notes = parse_notes(tn, PSA).unwrap().notes;
    let links = parse_word_links(twl, PSA).unwrap();
    (
        notes_for_book(&notes, &spec),
        word_links_for_book(&links, Some(articles), &spec),
    )
}

#[test]
fn each_psalms_row_is_visited_once() {
    let (tn, twl) = (
        fixture("scale/psa/en_tn/tn_PSA.tsv"),
        fixture("scale/psa/en_twl/twl_PSA.tsv"),
    );
    let (notes, words) = open(&tn, &twl, &tw_articles());
    for (list, rows) in [(&notes, 8212), (&words, 7683)] {
        assert_eq!(
            list.items.len() + list.skipped.len() + list.dropped.len(),
            rows
        );
    }
    assert_eq!((notes.items.len(), words.items.len()), (7969, 7683));
}

#[test]
fn opening_the_psalms_check_lists_meets_the_budget() {
    let (tn, twl) = (
        fixture("scale/psa/en_tn/tn_PSA.tsv"),
        fixture("scale/psa/en_twl/twl_PSA.tsv"),
    );
    let articles = tw_articles();
    // The first run is the first open of a session: it loads the versification scheme
    // and touches fresh memory for the rows, which the later runs reuse.
    let start = Instant::now();
    open(&tn, &twl, &articles);
    let first = start.elapsed();
    // A debug build only checks that the code runs.
    let runs = if cfg!(debug_assertions) { 1 } else { 21 };
    let mut times: Vec<Duration> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            let lists = open(&tn, &twl, &articles);
            let elapsed = start.elapsed();
            std::hint::black_box(lists);
            elapsed
        })
        .collect();
    times.sort();
    let median = times[times.len() / 2];
    let p95 = times[(times.len() * 95).div_ceil(100) - 1];
    eprintln!(
        "Psalms tN ({} bytes) and TWL ({} bytes), read and derived: first {first:?}, \
         {runs} runs median {median:?}, p95 {p95:?}",
        tn.len(),
        twl.len()
    );
    // The budget is 250 ms on the reference laptop; tests fail only above three times it,
    // because test machines vary (docs/PERFORMANCE.md rule 5).
    if !cfg!(debug_assertions) {
        let limit = Duration::from_millis(3 * 250);
        assert!(first < limit && p95 < limit, "first {first:?}, p95 {p95:?}");
    }
}
