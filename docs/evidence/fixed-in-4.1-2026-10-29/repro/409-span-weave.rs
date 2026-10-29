// Goes at crates/tc-align/tests/repro_409.rs (its own test target; uses tests/common).
//! tC4 #409 repro: a verse span (TIT 1:1-3) aligned fresh in the rebuild's aligner, then woven
//! as the aligned USFM export weaves it. The issue wants `x-ref="C:V"` on every `\zaln-s` and
//! occurrences counted per source verse.
//! Run: tc-heavy cargo test -p tc-align --test repro_409 -- --nocapture
#![allow(clippy::unwrap_used, reason = "test code")]
mod common;
use common::*;
use tc_align::usfm::weave_verse;
use tc_align::{Aligner, TargetVerse};

#[test]
fn a_span_aligned_here_weaves_without_x_ref_and_with_span_wide_counts() {
    let source = source_verse(UGNT, "1:1-3");
    let text = "Paul, a servant of God, revealed his word at the right time by the command of God";
    let mut aligner = Aligner::open(None, None, TargetVerse::new(text), &source, basis(3)).unwrap();
    let theou: Vec<_> = source.words().iter().filter(|w| w.word == "Θεοῦ").map(|w| (w.occurrence, w.occurrences)).collect();
    println!("Θεοῦ in UGNT TIT 1:1-3, as the span numbers it: {theou:?}");
    for (on, at, said, n) in [("Παῦλος", 1, "Paul", 1), ("Θεοῦ", 1, "God", 1), ("Θεοῦ", theou.len() as u32, "God", 2)] {
        let card = aligner.value().alignments.iter()
            .position(|g| g.top_words.iter().any(|w| w.word == on && w.occurrence == at)).unwrap();
        let word = aligner.value().word_bank.iter().find(|w| w.word == said && w.occurrence == n).cloned().unwrap();
        aligner.place(card, &word).unwrap();
    }
    let woven = weave_verse(text, aligner.value()).unwrap();
    let milestones: Vec<&str> = woven.split("\\zaln-s ").skip(1).map(|m| m.split("\\*").next().unwrap()).collect();
    for m in &milestones {
        println!("\\zaln-s {m}");
    }
    assert!(milestones.iter().all(|m| !m.contains("x-ref")), "no x-ref is written");
    assert!(milestones.iter().any(|m| m.contains("x-content=\"Θεοῦ\"") && !m.contains("x-occurrences=\"1\"")),
        "Θεοῦ is counted across the span, not per verse");
}
