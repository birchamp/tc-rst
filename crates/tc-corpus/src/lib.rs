//! The scale corpus of `docs/PERFORMANCE.md`, generated deterministically from a seed.
//!
//! One generator gives every crate the same corpus in two forms:
//!
//! - [`Domain`]: plain inputs without any `tc-journal` type (books with their verse texts,
//!   alignment groups, check rows and the decisions the simulated team made), for domain
//!   crates such as `tc-check` and `tc-align`.
//! - [`Journal`]: the changes a team of devices wrote over three years at the rates of
//!   `docs/STORAGE.md` §14, as sealed packs and pending files, for `tc-journal` and
//!   `tc-store`.
//!
//! Every part says where it came from ([`Origin`]): real data read from `fixtures/scale/`,
//! real data placed elsewhere, or generated. [`Corpus::describe`] prints the labels, so a
//! benchmark never presents generated data as real.
//!
//! [`Spec::psalms`] is the Psalms corpus, [`Spec::scaled`] the same corpus made larger
//! (more books, more history, more devices) for the operation-count tests, and
//! [`Spec::whole_bible`] the whole-Bible corpus of the benchmarks. The crate is for tests
//! and benchmarks only; it reads fixture files and is not published.

pub mod domain;
pub mod fixtures;
pub mod history;
pub mod rng;

pub use domain::{
    AlignmentInput, BookInput, CheckRow, CheckTool, Domain, GroupInput, Origin, OriginalText,
    SourceWordInput, TargetWordInput, VerseInput,
};
pub use history::{Journal, PackFile, Stats};

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use tc_core::BookId;

/// Why a corpus cannot be built: a fixture is missing or unreadable, or the generator made
/// a change the journal refuses (a bug).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CorpusError {
    #[error("fixture {path}: {reason}")]
    Fixture { path: String, reason: String },
    #[error("the generator made an invalid change: {0}")]
    Build(String),
}

/// Where the text, alignments and checks come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// `fixtures/scale/psa/`: Psalms is real; every other book is sampled from it.
    Psalms,
    /// The bundled unfoldingWord releases in `bundle/en/`: the 59 books of the v91
    /// ULT, tN and TWL are real; the seven books those releases lack are sampled from books
    /// of the same testament, with check counts from tC4's v89 density table.
    Releases,
}

/// What to generate. The rates are those of `docs/STORAGE.md` §14.
#[derive(Debug, Clone, PartialEq)]
pub struct Spec {
    pub seed: u64,
    pub source: Source,
    pub books: Vec<BookId>,
    pub devices: usize,
    /// Working days (250 a year).
    pub days: u32,
    /// Verse changes per verse across the team (autosave on blur and after 2 s idle).
    pub verse_saves_per_verse: f64,
    pub alignment_saves_per_verse: f64,
    pub decision_saves_per_check: f64,
    pub comments_per_verse: f64,
    /// The share of verse saves that are section saves of two or three verses: passage
    /// batches (§7.3).
    pub section_rate: f64,
    /// The share of actions that change a record a teammate changed the same day, before
    /// the two exchanged: the source of conflicts.
    pub concurrent_rate: f64,
    /// The share of seen conflicts a device resolves.
    pub resolve_rate: f64,
    /// The chance that a device exchanges with the team at the end of a working day.
    pub sync_rate: f64,
}

impl Spec {
    /// §14: 8 verse saves per verse, 1.5 alignment saves per verse, 1.3 saves per check,
    /// 5,000 comments per 31,102 verses, three devices, three years.
    fn rates(seed: u64, source: Source, books: Vec<BookId>, devices: usize) -> Self {
        Self {
            seed,
            source,
            books,
            devices,
            days: 750,
            verse_saves_per_verse: 8.0,
            alignment_saves_per_verse: 1.5,
            decision_saves_per_check: 1.3,
            comments_per_verse: 5_000.0 / 31_102.0,
            section_rate: 0.1,
            concurrent_rate: 0.004,
            resolve_rate: 0.7,
            sync_rate: 0.8,
        }
    }

    /// The Psalms corpus: Psalms from `fixtures/scale/psa/`, three devices, three years.
    pub fn psalms(seed: u64) -> Self {
        Self::rates(seed, Source::Psalms, vec![BookId::Psalms], 3)
    }

    /// The Psalms corpus made about `factor` times larger: Psalms plus Old Testament books
    /// sampled from it until the verse count is `factor` times that of Psalms, with three
    /// devices per Psalms-sized share of the work (at most 30).
    pub fn scaled(seed: u64, factor: usize) -> Self {
        const PSALMS_VERSES: usize = 2_461;
        let target = PSALMS_VERSES * factor.max(1);
        let mut books = vec![BookId::Psalms];
        let mut verses = PSALMS_VERSES;
        for ordinal in 1..=66u8 {
            if verses >= target {
                break;
            }
            let Some(book) = BookId::from_ordinal(ordinal) else {
                continue;
            };
            if book == BookId::Psalms {
                continue;
            }
            verses += domain::verse_counts(book)
                .iter()
                .map(|&n| usize::from(n))
                .sum::<usize>();
            books.push(book);
        }
        let devices = (3 * factor.max(1)).min(30);
        Self::rates(seed, Source::Psalms, books, devices)
    }

    /// The whole-Bible corpus of §14: 66 books, three devices, three years, about 620,000
    /// changes.
    pub fn whole_bible(seed: u64) -> Self {
        let books = (1..=66u8).filter_map(BookId::from_ordinal).collect();
        Self::rates(seed, Source::Releases, books, 3)
    }

    /// The same corpus with less history: `days` working days and the given rates scaled
    /// by `share`. For quick tests of the generator itself.
    pub fn with_history(mut self, days: u32, share: f64) -> Self {
        self.days = days;
        self.verse_saves_per_verse *= share;
        self.alignment_saves_per_verse *= share;
        self.decision_saves_per_check *= share;
        self.comments_per_verse *= share;
        self
    }
}

/// A plain decision status, without `tc-journal` types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusInput {
    Valid,
    Invalid,
    Todo,
}

/// The last decision the simulated team saved on one check row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionInput {
    /// Index of the book in [`Domain::books`].
    pub book: usize,
    /// Index of the row in that book's [`BookInput::checks`].
    pub check: usize,
    pub status: Option<StatusInput>,
    pub selections: Vec<TargetWordInput>,
    pub nothing_to_select: bool,
    pub comment: Option<String>,
    pub bookmarked: bool,
}

/// A generated corpus.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub spec: Spec,
    pub domain: Domain,
    /// The decisions of the history, in (book, check) order.
    pub decisions: Vec<DecisionInput>,
    pub journal: Journal,
}

/// The repository root, found from this crate's manifest directory.
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

impl Corpus {
    /// Generates the corpus of `spec` from the fixtures of this repository.
    pub fn generate(spec: &Spec) -> Result<Self, CorpusError> {
        Self::generate_from(spec, &repo_root())
    }

    /// Generates the corpus of `spec` from the fixtures under `root`.
    pub fn generate_from(spec: &Spec, root: &Path) -> Result<Self, CorpusError> {
        let domain = Self::domain_from(spec, root)?;
        let rng = rng::Rng::new(spec.seed);
        let out = history::Sim::new(spec, &domain, &rng).run()?;
        Ok(Self {
            spec: spec.clone(),
            decisions: out.decisions,
            journal: out.journal,
            domain,
        })
    }

    /// Only the plain inputs of `spec`, without the history.
    pub fn domain(spec: &Spec) -> Result<Domain, CorpusError> {
        Self::domain_from(spec, &repo_root())
    }

    fn domain_from(spec: &Spec, root: &Path) -> Result<Domain, CorpusError> {
        let rng = rng::Rng::new(spec.seed);
        domain::build(spec, root, &rng)
    }

    /// A summary that names every generated part.
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let stats = &self.journal.stats;
        let _ = writeln!(
            out,
            "tc-corpus seed {} ({:?}): {} books, {} verses, {} checks, {} devices, {} days",
            self.spec.seed,
            self.spec.source,
            self.domain.books.len(),
            self.domain.verse_count(),
            self.domain.check_count(),
            self.spec.devices,
            self.spec.days
        );
        let _ = writeln!(
            out,
            "journal: {} changes in {} batches ({} bytes), {} packs, {} pending files; {} section saves, {} concurrent edits, {} conflicts seen, {} resolved",
            stats.changes,
            stats.batches,
            stats.bytes,
            self.journal.packs.len(),
            self.journal.pending.len(),
            stats.section_saves,
            stats.concurrent_edits,
            stats.conflicts_seen,
            stats.resolutions
        );
        for (kind, count) in &stats.by_kind {
            let _ = writeln!(out, "  {kind}: {count}");
        }
        let _ = writeln!(out, "origins:");
        for (origin, parts) in self.domain.origins() {
            let _ = writeln!(out, "  {origin}: {parts}");
        }
        let _ = writeln!(out, "  {}: the whole journal", history::HISTORY_ORIGIN);
        out
    }
}
