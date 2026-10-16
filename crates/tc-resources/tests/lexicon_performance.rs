//! Loading a lexicon and looking words up, against the budgets in docs/PERFORMANCE.md
//! (Resources: load the Hebrew lexicon, 8,674 entries, in 100 ms once per session; a lookup
//! in memory, under 1 ms). Run
//! `cargo test --release -p tc-resources --test lexicon_performance -- --nocapture` to see
//! times; a release build fails only above three times a budget (rule 5).
//!
//! The words are every `strong` attribute of the UHB Psalms (`fixtures/scale/psa/`). A
//! lookup reads no file and probes one entry per numbered part, so the count tests run the
//! same words against the real Hebrew lexicon and against one ten times larger, and the
//! counts must be equal.

// Test helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::bundled::{UGL, UHL, lock_from_the_zips};
use serde_json::{Map, Value};
use tc_resources::{
    LEXICON_FILE, Lexicon, LexiconInstall, Lexicons, PartEntry, ResourceStore, Testaments,
    content_files_opened, install_lexicon_zip, lexicon_entries_probed, open_lexicon,
};

const LOAD_BUDGET: Duration = Duration::from_millis(100);
const LOOKUP_BUDGET: Duration = Duration::from_millis(1);

/// Every `strong` attribute of the UHB Psalms, in text order.
fn psalms_words() -> Vec<String> {
    let text = String::from_utf8(common::fixture("scale/psa/hbo_uhb/19-PSA.usfm")).unwrap();
    let mut words = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("strong=\"") {
        rest = &rest[at + "strong=\"".len()..];
        let end = rest.find('"').unwrap();
        words.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
    words
}

/// A store holding both bundled lexicons, as the first run seeds them, and their pins.
fn store_with_both() -> (tempfile::TempDir, ResourceStore, Testaments) {
    let dir = tempfile::tempdir().unwrap();
    let store = ResourceStore::open(dir.path()).unwrap();
    let lock = lock_from_the_zips();
    for lexicon in [UGL, UHL] {
        let installed =
            install_lexicon_zip(&store, &lexicon.pin(), &lock, &lexicon.zip(), 1 << 30).unwrap();
        assert!(matches!(installed, LexiconInstall::Installed(_)));
    }
    let pins = Testaments {
        nt: Some(UGL.pin()),
        ot: Some(UHL.pin()),
        ..Testaments::default()
    };
    (dir, store, pins)
}

/// The Hebrew lexicon with nine more copies of every entry, numbered above the real ones.
fn ten_times_larger() -> Lexicon {
    let mut file: Value = serde_json::from_slice(UHL.repackaged()).unwrap();
    let entries = file["entries"].as_object().unwrap().clone();
    let mut larger = Map::new();
    for copy in 0..10u32 {
        for (number, entry) in &entries {
            let number: u32 = number.parse().unwrap();
            larger.insert((number + copy * 100_000).to_string(), entry.clone());
        }
    }
    file["entries"] = Value::Object(larger);
    let lexicon = Lexicon::parse(&serde_json::to_vec(&file).unwrap()).unwrap();
    assert_eq!(lexicon.len(), 10 * UHL.entries);
    lexicon
}

/// Entries probed and content files opened while every word is looked up.
fn lookup_counts(lexicons: &Lexicons, words: &[String]) -> (u64, u64, usize) {
    let (probed, opened) = (lexicon_entries_probed(), content_files_opened());
    let mut found = 0;
    for word in words {
        found += lexicons
            .lookup(word)
            .iter()
            .filter(|p| matches!(p.entry, PartEntry::Found(_)))
            .count();
    }
    (
        lexicon_entries_probed() - probed,
        content_files_opened() - opened,
        found,
    )
}

#[test]
fn a_lookup_costs_the_same_in_a_lexicon_ten_times_larger() {
    let words = psalms_words();
    let numbered: u64 = words
        .iter()
        .map(|w| {
            tc_resources::strongs::strongs_parts(w)
                .iter()
                .filter(|p| p.number.is_some())
                .count() as u64
        })
        .sum();
    let real = Lexicons::from_parts(Ok(UGL.lexicon()), Ok(UHL.lexicon()));
    let larger = Lexicons::from_parts(Ok(UGL.lexicon()), Ok(ten_times_larger()));

    let (probed, opened, found) = lookup_counts(&real, &words);
    assert_eq!(probed, numbered, "one probe per numbered part");
    assert_eq!(opened, 0, "a lookup reads no file");
    assert_eq!(found as u64, numbered, "every Psalms word has its entry");
    assert_eq!(
        lookup_counts(&larger, &words),
        (probed, 0, found),
        "the same words cost the same in a lexicon ten times larger"
    );
    println!(
        "{} Psalms words, {numbered} numbered parts: {probed} probes in both lexicons",
        words.len()
    );
}

#[test]
fn a_session_opens_each_lexicon_file_once() {
    let (_dir, store, pins) = store_with_both();
    let before = content_files_opened();
    let lexicons = Lexicons::open(&store, &pins);
    assert_eq!(content_files_opened() - before, 2, "one file per lexicon");
    let before = content_files_opened();
    lookup_counts(&lexicons, &psalms_words());
    assert_eq!(content_files_opened(), before);
}

fn median_and_p95(mut times: Vec<Duration>) -> (Duration, Duration) {
    times.sort();
    let p95 = times[(times.len() * 95).div_ceil(100) - 1];
    (times[times.len() / 2], p95)
}

/// The time `f` takes, not counting the drop of what it returns: a session keeps its
/// lexicons until it ends.
fn timed<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let value = f();
    (start.elapsed(), value)
}

#[test]
fn loading_the_hebrew_lexicon_meets_the_budget() {
    let (_dir, store, _) = store_with_both();
    let pin = UHL.pin();
    let (first, lexicon) = timed(|| open_lexicon(&store, &pin).unwrap());
    assert_eq!(lexicon.len(), UHL.entries);
    // A debug build only checks that the code runs.
    let runs = if cfg!(debug_assertions) { 1 } else { 21 };
    let (mut loads, mut reads, mut parses) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..runs {
        let (load, lexicon) = timed(|| open_lexicon(&store, &pin).unwrap());
        drop(lexicon);
        loads.push(load);
        // The two parts of a load: reading the file, and parsing it.
        let installed = store.dcs(&pin).unwrap().unwrap();
        let (read, bytes) = timed(|| installed.read(LEXICON_FILE).unwrap());
        let (parse, lexicon) = timed(|| Lexicon::parse(&bytes).unwrap());
        drop(lexicon);
        reads.push(read);
        parses.push(parse);
    }
    let (median, p95) = median_and_p95(loads);
    println!(
        "load en_uhl ({} bytes, {} entries): first {first:?}, median {median:?}, p95 {p95:?} of {runs}; \
         read median {:?}, parse median {:?}",
        UHL.repackaged().len(),
        UHL.entries,
        median_and_p95(reads).0,
        median_and_p95(parses).0,
    );
    let (ugl, _) = timed(|| open_lexicon(&store, &UGL.pin()).unwrap());
    println!(
        "load en_ugl ({} bytes, {} entries): {ugl:?}",
        UGL.repackaged().len(),
        UGL.entries,
    );
    if !cfg!(debug_assertions) {
        assert!(
            first.max(p95) <= LOAD_BUDGET * 3,
            "first {first:?}, p95 {p95:?}"
        );
    }
}

#[test]
fn looking_up_every_word_of_psalms_meets_the_budget() {
    let words = psalms_words();
    let lexicons = Lexicons::from_parts(Ok(UGL.lexicon()), Ok(UHL.lexicon()));
    let start = Instant::now();
    let times: Vec<Duration> = words
        .iter()
        .map(|word| {
            let start = Instant::now();
            let parts = lexicons.lookup(word);
            let elapsed = start.elapsed();
            assert!(!parts.is_empty());
            elapsed
        })
        .collect();
    let total = start.elapsed();
    let max = times.iter().max().copied().unwrap();
    let (median, p95) = median_and_p95(times);
    println!(
        "{} lookups: total {total:?}, median {median:?}, p95 {p95:?}, max {max:?}",
        words.len()
    );
    if !cfg!(debug_assertions) {
        assert!(p95 <= LOOKUP_BUDGET * 3, "p95 {p95:?}");
    }
}
