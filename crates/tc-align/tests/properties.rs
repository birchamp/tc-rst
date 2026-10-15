//! Properties over random verses and random edit sequences: no edit loses, duplicates or
//! reorders a word; a reflow keeps or reports every link; aligned USFM and the tC4 record
//! give back what was written.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::basis;
use proptest::prelude::*;
use std::collections::BTreeMap;
use tc_align::usfm::{from_aligned, weave_verse};
use tc_align::{
    AlignmentValue, NoReflow, SourceVerse, SourceWord, TargetVerse, TargetWord, record, reflow,
};

const TARGET_WORDS: [&str; 7] = ["a", "b", "c", "de", "Dios", "λόγος", "וַ\u{2060}יְהִי"];
const SOURCE_WORDS: [&str; 5] = ["Α", "Β", "Γ", "Θεοῦ", "דְּבַר"];

fn text_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(
        (prop::sample::select(&TARGET_WORDS[..]), prop::bool::ANY),
        1..12,
    )
    .prop_map(|words| {
        let mut out = String::new();
        for (i, (w, comma)) in words.iter().enumerate() {
            if i > 0 {
                out.push_str(if *comma { ", " } else { " " });
            }
            out.push_str(w);
        }
        out.push('.');
        out
    })
}

fn source_strategy() -> impl Strategy<Value = SourceVerse> {
    prop::collection::vec(prop::sample::select(&SOURCE_WORDS[..]), 1..8).prop_map(|words| {
        SourceVerse::new(
            words
                .into_iter()
                .map(|w| SourceWord {
                    content: w.to_owned(),
                    strong: Some("G1".to_owned()),
                    lemma: Some(w.to_lowercase()),
                    morph: Some("Gr,N,,,,,NMS,".to_owned()),
                    occurrence: 1,
                    occurrences: 1,
                    extra: Vec::new(),
                })
                .collect(),
        )
    })
}

#[derive(Debug, Clone)]
enum Op {
    Link(usize, usize),
    Unlink(usize, usize),
    Move(usize, usize, usize),
    Merge(usize, usize),
    Split(usize),
    Clear,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    let i = || 0usize..16;
    prop_oneof![
        4 => (i(), i()).prop_map(|(c, w)| Op::Link(c, w)),
        2 => (i(), i()).prop_map(|(c, w)| Op::Unlink(c, w)),
        2 => (i(), i(), i()).prop_map(|(a, b, w)| Op::Move(a, b, w)),
        2 => (i(), prop::bool::ANY).prop_map(|(a, up)| Op::Merge(a, if up { a + 1 } else { a.wrapping_sub(1) })),
        1 => i().prop_map(Op::Split),
        1 => Just(Op::Clear),
    ]
}

/// Apply `op` with its indices folded into range, as a UI would only offer real cards
/// and words, plus some out-of-range ones to check refusals.
fn apply(value: &mut AlignmentValue, verse: &TargetVerse, op: &Op) -> bool {
    let cards = value.alignments.len();
    let placed = |v: &AlignmentValue, c: usize, w: usize| {
        let targets = &v.alignments[c % cards].targets;
        (!targets.is_empty()).then(|| targets[w % targets.len()].clone())
    };
    match op {
        Op::Link(c, w) if !value.word_bank.is_empty() => {
            let word = value.word_bank[w % value.word_bank.len()].clone();
            value.link(c % (cards + 1), &word).is_ok()
        }
        Op::Unlink(c, w) => match placed(value, *c, *w) {
            Some(word) => value.unlink(c % cards, &word, verse).is_ok(),
            None => false,
        },
        Op::Move(a, b, w) => match placed(value, *a, *w) {
            Some(word) => value.move_word(a % cards, b % cards, &word).is_ok(),
            None => false,
        },
        Op::Merge(a, b) => value.merge(*a % cards, *b).is_ok(),
        Op::Split(c) => value.split(c % (cards + 1)).is_ok(),
        Op::Clear => {
            value.clear(verse);
            true
        }
        Op::Link(..) => false,
    }
}

fn key(w: &TargetWord) -> (String, u32) {
    (w.text.clone(), w.occurrence)
}

fn sorted(words: impl IntoIterator<Item = TargetWord>) -> Vec<TargetWord> {
    let mut words: Vec<TargetWord> = words.into_iter().collect();
    words.sort_by_key(key);
    words
}

/// Each placed word and the source words of its card.
fn links(value: &AlignmentValue) -> BTreeMap<(String, u32), Vec<(String, u32)>> {
    value
        .alignments
        .iter()
        .flat_map(|g| {
            let sources: Vec<_> = g
                .sources
                .iter()
                .map(|s| (s.content.clone(), s.occurrence))
                .collect();
            g.targets.iter().map(move |t| (key(t), sources.clone()))
        })
        .collect()
}

fn edited(source: &SourceVerse, text: &str, ops: &[Op]) -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(text);
    let mut value = AlignmentValue::bootstrap(source, &verse, basis(1));
    for op in ops {
        if apply(&mut value, &verse, op) {
            value.settle(&verse);
        }
    }
    (value, verse)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn edits_keep_every_word_once_and_the_source_order(
        source in source_strategy(),
        text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let verse = TargetVerse::new(&text);
        let mut value = AlignmentValue::bootstrap(&source, &verse, basis(1));
        let source_order: Vec<SourceWord> = source.words().to_vec();
        for op in &ops {
            let before = value.clone();
            if apply(&mut value, &verse, op) {
                value.settle(&verse);
                prop_assert_eq!(value.done, value.is_fully_aligned());
            } else {
                prop_assert_eq!(&value, &before, "a refused edit changes nothing");
            }
            prop_assert_eq!(sorted(value.all_target_words().cloned()), sorted(verse.words().to_vec()));
            let flattened: Vec<SourceWord> = value.alignments.iter().flat_map(|g| g.sources.clone()).collect();
            prop_assert_eq!(&flattened, &source_order);
            prop_assert!(value.alignments.iter().all(|g| !g.sources.is_empty()));
            let order: Vec<_> = value.word_bank.iter().map(|w| (w.occurrence, verse.position(w))).collect();
            prop_assert!(order.windows(2).all(|p| p[0] <= p[1]), "the bank stays in bootstrap order");
        }
    }

    #[test]
    fn a_reflow_keeps_or_reports_every_link_and_makes_none(
        source in source_strategy(),
        text in text_strategy(),
        new_text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let (value, _) = edited(&source, &text, &ops);
        let new_verse = TargetVerse::new(&new_text);
        match reflow(&value, &new_verse) {
            Ok(next) => {
                prop_assert_eq!(value.placed(), next.value.placed() + next.dropped.len());
                prop_assert_eq!(sorted(next.value.all_target_words().cloned()), sorted(new_verse.words().to_vec()));
                for (before, after) in value.alignments.iter().zip(&next.value.alignments) {
                    prop_assert_eq!(&before.sources, &after.sources);
                    for word in &after.targets {
                        prop_assert!(before.targets.iter().any(|w| key(w) == key(word)), "no new link");
                    }
                }
                for drop in &next.dropped {
                    prop_assert!(new_verse.position(&drop.word).is_none());
                    prop_assert!(value.alignments[drop.card].targets.contains(&drop.word));
                }
                prop_assert!(!next.value.is_stale_against(&new_verse));
                prop_assert_eq!(next.value.done, next.value.is_fully_aligned());
            }
            Err(NoReflow::Current) => prop_assert_eq!(&text, &new_text),
            Err(NoReflow::NothingPlaced) => prop_assert_eq!(value.placed(), 0),
            Err(other) => prop_assert!(false, "unexpected {:?}", other),
        }
    }

    #[test]
    fn aligned_usfm_reads_back_the_links_and_the_text(
        source in source_strategy(),
        text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let (value, _) = edited(&source, &text, &ops);
        let woven = weave_verse(&text, &value).unwrap();
        let aligned = tc_usfm::parse_aligned(&woven).unwrap();
        prop_assert_eq!(&aligned.plain, &text);
        let back = from_aligned(&aligned, None);
        let mut read = value.clone();
        read.alignments = back.alignments;
        read.word_bank = back.word_bank;
        prop_assert_eq!(links(&read), links(&value));
        prop_assert_eq!(read.total(), value.total());
    }

    #[test]
    fn the_tc4_record_reads_back_the_same_value(
        source in source_strategy(),
        text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
        invalid in prop::bool::ANY,
    ) {
        let (value, _) = edited(&source, &text, &ops);
        let json = record::to_record(&value, invalid);
        prop_assert_eq!(json["invalid"].as_bool(), Some(invalid));
        let mut back = record::from_record(&json, basis(1)).unwrap();
        back.marked_invalid = value.marked_invalid;
        prop_assert_eq!(back, value);
    }
}
