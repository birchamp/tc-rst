//! Quick timing of a full fold of a corpus.
#![allow(clippy::expect_used)]
use std::time::Instant;
use tc_corpus::{Corpus, Spec};
use tc_journal::fold::{FoldInput, FoldState, ProjectKind};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let spec = match args.first().map(String::as_str) {
        Some("bible") => Spec::whole_bible(1),
        Some("scaled") => Spec::scaled(1, 10),
        _ => Spec::psalms(1),
    };
    let t = Instant::now();
    let corpus = Corpus::generate(&spec).expect("corpus");
    eprintln!(
        "generated {} changes in {:?}",
        corpus.journal.stats.changes,
        t.elapsed()
    );
    let t = Instant::now();
    let lines = corpus.journal.lines();
    eprintln!("read {} lines in {:?}", lines.len(), t.elapsed());
    let t = Instant::now();
    let prepared: Vec<FoldInput> = lines
        .into_iter()
        .map(|l| FoldInput::from(l).prepare())
        .collect();
    eprintln!("prepared in {:?}", t.elapsed());
    let t = Instant::now();
    let state = FoldState::fold(ProjectKind::Bible, prepared);
    eprintln!(
        "folded in {:?}: {} conflicts, {} anomalies, {} detached",
        t.elapsed(),
        state.conflict_count(),
        state.anomalies().count(),
        state.detached().count()
    );
    eprintln!("{:?}", state.counters());
}
