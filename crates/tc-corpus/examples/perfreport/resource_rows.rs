//! The tc-resources rows (PERFORMANCE.md "Resources" and "Start-up"): the real exports of
//! `bundle/en` installed into a bundled store (what the installer ships), seeded into a
//! machine store as a first launch does, and one book's resources loaded from it.
//!
//! - `seed.first`: the seeding part of the first launch, with the bundled store out of the
//!   page cache; `seed.repeat_reads`: a repeat launch plans and seeds again and must open and
//!   write no content file; `seed.counts`: the per-file rules (each file opened once, each
//!   written once, none read back).
//! - `res.load_psalms`: Psalms loaded from the machine store, the first time with the store
//!   out of the page cache (on Windows each file opened is a Defender scan, so the number
//!   of files is reported beside the time); `res.memory_psalms`: the private bytes the loaded
//!   book keeps.
//! - `lex.load` and `lex.lookup`: the Hebrew lexicon, loaded and looked up with every
//!   `strong` of the UHB Psalms.

use crate::common::{evict_page_cache, header, ms};
use crate::machine::private_mib;
use crate::table::Measured;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tc_core::BookId;
use tc_resources::{
    HelpsChoice, Lexicons, ResourceStore, SeedWatch, Seeding, content_files_opened,
    content_files_written, install_bundle, load_book, open_lexicon, suite, with_opened_paths,
};

/// Where the bundled store (the installer's) and the machine store (the user's) live.
fn bundled_dir(dir: &Path) -> PathBuf {
    dir.join("resources/bundled")
}

fn machine_dir(dir: &Path) -> PathBuf {
    dir.join("resources/machine")
}

struct Quiet;

impl SeedWatch for Quiet {
    fn done(&self, _files: u64) {}

    fn cancelled(&self) -> bool {
        false
    }
}

/// Installs the exports of `bundle/en` into the bundled store, as the packager does.
pub fn setup(dir: &Path) {
    let bundled = bundled_dir(dir);
    if bundled.join("installs").exists()
        || std::fs::read_dir(&bundled).is_ok_and(|mut d| d.next().is_some())
    {
        return;
    }
    std::fs::create_dir_all(&bundled).expect("the bundled store folder");
    let store = ResourceStore::open(&bundled).expect("the bundled store");
    let started = Instant::now();
    let installed = install_bundle(&tc_corpus::repo_root().join("bundle/en"), &store, &|_| true)
        .expect("the bundle installs");
    println!(
        "bundled store: {} installs, {} files in {:.1} s",
        installed.installed.len(),
        installed.files,
        started.elapsed().as_secs_f64()
    );
}

fn under(paths: &[PathBuf], root: &Path) -> usize {
    paths.iter().filter(|p| p.starts_with(root)).count()
}

/// First-launch seeding into a fresh machine store, five times, and then a repeat launch.
pub fn seed(dir: &Path) -> Vec<Measured> {
    header("res-seed");
    setup(dir);
    let (bundled_root, machine_root) = (bundled_dir(dir), machine_dir(dir));
    let bundled = ResourceStore::open_read_only(&bundled_root).expect("the bundled store");
    let mut rows = Vec::new();
    let (mut times, mut planning) = (Vec::new(), Vec::new());
    let mut files = 0;
    let mut violations = 0usize;
    let mut note = String::new();
    for _ in 0..5 {
        let _ = std::fs::remove_dir_all(&machine_root);
        std::fs::create_dir_all(&machine_root).expect("the machine store folder");
        let evicted = evict_page_cache(&bundled_root);
        let machine = ResourceStore::open(&machine_root).expect("the machine store");
        let started = Instant::now();
        let (plan, planned) = with_opened_paths(|| machine.seed_plan(&bundled).expect("a plan"));
        planning.push(ms(started));
        files = plan.files();
        if !planned.is_empty() {
            violations += 1;
        }
        let written_before = content_files_written();
        let started = Instant::now();
        let (seeding, opened) = with_opened_paths(|| machine.seed(plan, &Quiet).expect("seeding"));
        times.push(ms(started) + planning.last().copied().unwrap_or(0.0));
        let Seeding::Finished(report) = seeding else {
            panic!("the seeding finished");
        };
        // Each content file is opened once and written once, and none is read back.
        let distinct: BTreeSet<&PathBuf> = opened.iter().collect();
        let written = content_files_written() - written_before;
        if opened.len() as u64 != files
            || distinct.len() != opened.len()
            || written != files
            || under(&opened, &machine_root) != 0
        {
            violations += 1;
        }
        note = format!(
            "{} installs, {files} content files: opened {}, written {written}, read back {}; \
             planning {:.0} ms; {evicted} files out of the page cache",
            report.added.len(),
            opened.len(),
            under(&opened, &machine_root),
            planning.last().copied().unwrap_or(0.0),
        );
    }
    rows.push(Measured::new("seed.first", times, note).with_fsyncs(files as f64));
    rows.push(Measured::new(
        "seed.counts",
        vec![violations as f64],
        "runs in which a file was opened twice, written twice or read back, or planning opened one",
    ));

    // A repeat launch: plan and seed again; nothing is opened or written.
    let machine = ResourceStore::open(&machine_root).expect("the machine store");
    let (opened_before, written_before) = (content_files_opened(), content_files_written());
    let started = Instant::now();
    let (plan, planned) = with_opened_paths(|| machine.seed_plan(&bundled).expect("a plan"));
    let planned_ms = ms(started);
    let (seeding, opened) = with_opened_paths(|| machine.seed(plan, &Quiet).expect("seeding"));
    let moved =
        (content_files_opened() - opened_before) + (content_files_written() - written_before);
    assert!(matches!(seeding, Seeding::Finished(_)));
    rows.push(Measured::new(
        "seed.repeat_reads",
        vec![moved as f64],
        format!(
            "a repeat launch: plan {planned_ms:.1} ms; content files opened {} and written {}",
            planned.len() + opened.len(),
            content_files_written() - written_before
        ),
    ));
    rows
}

/// Psalms loaded from the machine store: the first time with the store out of the page cache
/// and then warm, and the memory the loaded book keeps.
pub fn load(dir: &Path) -> Vec<Measured> {
    header("res-load");
    let machine_root = machine_dir(dir);
    let store = ResourceStore::open_read_only(&machine_root).expect("the machine store");
    let pins = suite::project_pins();
    let evicted = evict_page_cache(&machine_root);
    let before = private_mib().unwrap_or(f64::NAN);
    let started = Instant::now();
    let (loaded, opened) =
        with_opened_paths(|| load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms));
    let cold = ms(started);
    let after = private_mib().unwrap_or(f64::NAN);
    let articles = opened
        .iter()
        .filter(|p| p.components().any(|c| c.as_os_str() == "en_tw"))
        .count();
    let mut rows = vec![Measured::new(
        "res.load_psalms",
        vec![cold],
        format!(
            "{} content files opened ({articles} tW articles), {evicted} files out of the page \
             cache; {} notes, {} word links",
            opened.len(),
            loaded.helps.notes().len(),
            loaded.helps.word_links().len()
        ),
    )];
    rows.push(Measured::new(
        "res.memory_psalms",
        vec![after - before],
        format!("private bytes {before:.0} MiB before the load, {after:.0} MiB after, book kept"),
    ));
    std::hint::black_box(&loaded);
    let mut warm = Vec::new();
    for _ in 0..21 {
        let started = Instant::now();
        let again = load_book(&store, &pins, &HelpsChoice::Dcs, BookId::Psalms);
        warm.push(ms(started));
        std::hint::black_box(again);
    }
    rows.push(Measured::new(
        "res.load_psalms_warm",
        warm,
        "the same load again, the store in the page cache",
    ));
    rows
}

/// Every `strong` attribute of the UHB Psalms, in text order.
fn psalms_words() -> Vec<String> {
    let path = tc_corpus::repo_root().join("fixtures/scale/psa/hbo_uhb/19-PSA.usfm");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut words = Vec::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find("strong=\"") {
        rest = &rest[at + "strong=\"".len()..];
        let Some(end) = rest.find('"') else {
            break;
        };
        words.push(rest[..end].to_owned());
        rest = &rest[end + 1..];
    }
    words
}

/// The Hebrew lexicon loaded 21 times and every word of Psalms looked up in it.
pub fn lexicon(dir: &Path) -> Vec<Measured> {
    header("res-lexicon");
    let store = ResourceStore::open_read_only(machine_dir(dir)).expect("the machine store");
    let pins = suite::project_pins();
    let pin = pins
        .lexicon
        .ot
        .clone()
        .expect("the project pins the Hebrew lexicon");
    let mut loads = Vec::new();
    for _ in 0..21 {
        let started = Instant::now();
        let lexicon = open_lexicon(&store, &pin).expect("the lexicon opens");
        loads.push(ms(started));
        std::hint::black_box(lexicon);
    }
    let lexicons = Lexicons::open(&store, &pins.lexicon);
    let words = psalms_words();
    let mut lookups = Vec::with_capacity(words.len());
    let opened_before = content_files_opened();
    for word in &words {
        let started = Instant::now();
        let parts = lexicons.lookup(word);
        lookups.push(ms(started));
        std::hint::black_box(parts);
    }
    let opened = content_files_opened() - opened_before;
    vec![
        Measured::new(
            "lex.load",
            loads,
            "en_uhl read and parsed; the first of the process is the first sample",
        ),
        Measured::new(
            "lex.lookup",
            lookups,
            format!(
                "{} Psalms words, each looked up alone; {opened} files opened",
                words.len()
            ),
        ),
    ]
}
