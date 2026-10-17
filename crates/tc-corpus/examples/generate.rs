//! Generates a corpus and prints what it holds and how long it took:
//! `cargo run --release -p tc-corpus --example generate -- bible [seed]`
//! (`psalms`, `scaled <factor>` or `bible`).

use std::time::Instant;
use tc_corpus::{Corpus, Spec};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let seed = |at: usize| args.get(at).and_then(|s| s.parse().ok()).unwrap_or(1);
    let spec = match args.first().map(String::as_str) {
        Some("scaled") => Spec::scaled(seed(2), seed(1) as usize),
        Some("bible") => Spec::whole_bible(seed(1)),
        _ => Spec::psalms(seed(1)),
    };
    let started = Instant::now();
    let domain = match Corpus::domain(&spec) {
        Ok(domain) => domain,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let domain_time = started.elapsed();
    let started = Instant::now();
    match Corpus::generate(&spec) {
        Ok(corpus) => {
            print!("{}", corpus.describe());
            println!(
                "plain inputs alone: {domain_time:?}; whole corpus: {:?} ({} verses)",
                started.elapsed(),
                domain.verse_count()
            );
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
