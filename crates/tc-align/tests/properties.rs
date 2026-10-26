//! Properties over random verses and random edit sequences: no edit loses or duplicates a
//! word; the aligner keeps the bank and every card in verse order and changes nothing but
//! the order when a verse opens; a reflow keeps or reports every link; aligned USFM and
//! the tC4 record give back what was written.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::basis;
use proptest::prelude::*;
use std::collections::BTreeMap;
use tc_align::suggest::RawLink;
use tc_align::usfm::weave_verse;
use tc_align::{
    Aligner, AlignmentValue, Click, NoReflow, SourceVerse, SourceWord, TargetVerse, TargetWord,
    reflow,
};
use tc_journal::alignment::split_aligned;
use tc_journal::ids::VerseNo;

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

fn source_word(w: &str) -> SourceWord {
    SourceWord {
        word: w.to_owned(),
        strong: Some("G1".to_owned()),
        lemma: Some(w.to_lowercase()),
        morph: Some("Gr,N,,,,,NMS,".to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: tc_journal::json::JsonMap::new(),
    }
}

fn source_strategy() -> impl Strategy<Value = SourceVerse> {
    prop::collection::vec(prop::sample::select(&SOURCE_WORDS[..]), 1..8)
        .prop_map(|words| SourceVerse::new(words.into_iter().map(source_word).collect()))
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
        let targets = &v.alignments[c % cards].bottom_words;
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

/// One thing a translator does in the Align workspace, with indices folded into range
/// as the screen offers them, plus some out-of-range ones.
#[derive(Debug, Clone)]
enum Step {
    Place(usize, usize),
    PlaceMany(usize, Vec<usize>),
    Unplace(usize, usize),
    Move(usize, usize, usize),
    Merge(usize, bool),
    Split(usize),
    Clear,
    Click(usize, Click),
    PlaceSelection(usize),
    Suggest(usize, usize),
    Accept(usize),
    AcceptAll,
    RejectAll,
}

impl Step {
    fn places(&self) -> bool {
        matches!(
            self,
            Step::Place(..)
                | Step::PlaceMany(..)
                | Step::PlaceSelection(_)
                | Step::Accept(_)
                | Step::AcceptAll
        )
    }
}

fn step_strategy() -> impl Strategy<Value = Step> {
    let i = || 0usize..16;
    let click = prop_oneof![Just(Click::Plain), Just(Click::Toggle), Just(Click::Range)];
    prop_oneof![
        3 => (i(), i()).prop_map(|(c, w)| Step::Place(c, w)),
        2 => (i(), prop::collection::vec(i(), 0..5)).prop_map(|(c, w)| Step::PlaceMany(c, w)),
        2 => (i(), i()).prop_map(|(c, w)| Step::Unplace(c, w)),
        1 => (i(), i(), i()).prop_map(|(a, b, w)| Step::Move(a, b, w)),
        1 => (i(), prop::bool::ANY).prop_map(|(a, up)| Step::Merge(a, up)),
        1 => i().prop_map(Step::Split),
        1 => Just(Step::Clear),
        4 => (i(), click).prop_map(|(w, c)| Step::Click(w, c)),
        2 => i().prop_map(Step::PlaceSelection),
        2 => (i(), i()).prop_map(|(s, t)| Step::Suggest(s, t)),
        1 => i().prop_map(Step::Accept),
        1 => Just(Step::AcceptAll),
        1 => Just(Step::RejectAll),
    ]
}

/// Do `step`; true when it edited the value.
fn step_on(aligner: &mut Aligner, step: &Step) -> bool {
    let cards = aligner.value().alignments.len();
    let bank = aligner.value().word_bank.clone();
    let banked = |w: usize| (!bank.is_empty()).then(|| bank[w % bank.len()].clone());
    let placed = |a: &Aligner, c: usize, w: usize| {
        let targets = &a.value().alignments[c % cards].bottom_words;
        (!targets.is_empty()).then(|| targets[w % targets.len()].clone())
    };
    match step {
        Step::Place(c, w) => {
            banked(*w).is_some_and(|word| aligner.place(c % (cards + 1), &word).is_ok())
        }
        Step::PlaceMany(c, ws) => {
            let words: Vec<TargetWord> = ws.iter().filter_map(|w| banked(*w)).collect();
            aligner.place_many(c % (cards + 1), &words).is_ok()
        }
        Step::Unplace(c, w) => {
            placed(aligner, *c, *w).is_some_and(|word| aligner.unplace(c % cards, &word).is_ok())
        }
        Step::Move(a, b, w) => placed(aligner, *a, *w)
            .is_some_and(|word| aligner.move_word(a % cards, b % cards, &word).is_ok()),
        Step::Merge(a, up) => {
            let a = a % cards;
            let b = if *up { a + 1 } else { a.wrapping_sub(1) };
            aligner.merge(a, b).is_ok()
        }
        Step::Split(c) => aligner.split(c % (cards + 1)).is_ok(),
        Step::Clear => {
            aligner.clear();
            true
        }
        Step::Click(w, click) => {
            let shown = aligner.shown_bank().len();
            aligner.click_bank_word(w % (shown + 1), *click);
            false
        }
        Step::PlaceSelection(c) => aligner.place_selection(c % (cards + 1)).is_ok(),
        Step::Suggest(s, t) => {
            let input = aligner.session_input();
            let (sources, targets) = (input.source().len(), input.target().len());
            aligner.show_suggestions(&[RawLink {
                source: vec![s % sources],
                target: vec![t % targets],
                confidence: 0.9,
            }]);
            false
        }
        Step::Accept(p) => aligner.accept(*p % (aligner.proposals().len() + 1)).is_ok(),
        Step::AcceptAll => aligner.accept_all() > 0,
        Step::RejectAll => {
            aligner.reject_all();
            false
        }
    }
}

/// The aligner's bank and every card are in verse order.
fn assert_shown_in_verse_order(
    aligner: &Aligner,
    verse: &TargetVerse,
) -> Result<(), TestCaseError> {
    prop_assert!(
        in_verse_order(verse, &aligner.value().word_bank),
        "bank {:?}",
        aligner.value().word_bank
    );
    for group in &aligner.value().alignments {
        prop_assert!(
            in_verse_order(verse, &group.bottom_words),
            "card {:?}",
            group.bottom_words
        );
    }
    Ok(())
}

/// Every word has a place in `verse` and they stand in that order.
fn in_verse_order(verse: &TargetVerse, words: &[TargetWord]) -> bool {
    let places: Option<Vec<usize>> = words.iter().map(|w| verse.position(w)).collect();
    places.is_some_and(|p| p.windows(2).all(|w| w[0] < w[1]))
}

/// `part` is `whole` with some words left out, in the same order.
fn is_subsequence(part: &[TargetWord], whole: &[TargetWord]) -> bool {
    let mut rest = whole.iter();
    part.iter().all(|w| rest.any(|x| x == w))
}

fn key(w: &TargetWord) -> (String, u32) {
    (w.word.clone(), w.occurrence)
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
                .top_words
                .iter()
                .map(|s| (s.word.clone(), s.occurrence))
                .collect();
            g.bottom_words
                .iter()
                .map(move |t| (key(t), sources.clone()))
        })
        .collect()
}

fn edited(source: &SourceVerse, text: &str, ops: &[Op]) -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(text);
    let mut value = bootstrap(source, &verse, basis(1));
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
        let mut value = bootstrap(&source, &verse, basis(1));
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
            let flattened: Vec<SourceWord> = value.alignments.iter().flat_map(|g| g.top_words.clone()).collect();
            prop_assert_eq!(&flattened, &source_order);
            prop_assert!(value.alignments.iter().all(|g| !g.top_words.is_empty()));
            if matches!(op, Op::Unlink(..) | Op::Clear) && value != before {
                prop_assert!(in_verse_order(&verse, &value.word_bank), "unlink and clear sort the bank");
            } else {
                prop_assert!(is_subsequence(&value.word_bank, &before.word_bank), "no other edit reorders the bank");
            }
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
                    prop_assert_eq!(&before.top_words, &after.top_words);
                    for word in &after.bottom_words {
                        prop_assert!(before.bottom_words.iter().any(|w| key(w) == key(word)), "no new link");
                    }
                }
                for drop in &next.dropped {
                    // Gone, or it is not clear which copy it is now: the changed verse holds
                    // another number of copies of it, or more than one (#789).
                    let count = new_verse.words().iter().filter(|w| w.word == drop.word.word).count();
                    prop_assert!(new_verse.position(&drop.word).is_none() || count != drop.word.occurrences as usize || count > 1);
                    prop_assert!(value.alignments[drop.card].bottom_words.contains(&drop.word));
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
    fn the_aligner_offers_the_words_of_the_shown_verse_and_accounts_for_every_link(
        source in source_strategy(),
        text in text_strategy(),
        new_text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
        flagged in prop::bool::ANY,
    ) {
        let (mut stored, _) = edited(&source, &text, &ops);
        stored.marked_invalid = flagged;
        let new_verse = TargetVerse::new(&new_text);
        let aligner = Aligner::open(Some(&stored), None, new_verse.clone(), &source, basis(1)).unwrap();
        prop_assert_eq!(
            sorted(aligner.value().all_target_words().cloned()),
            sorted(new_verse.words().to_vec())
        );
        prop_assert_eq!(stored.placed(), aligner.value().placed() + aligner.dropped().len());
        if !aligner.dropped().is_empty() {
            prop_assert!(!aligner.is_changed(), "a lossy carry-over waits for the translator");
        }
    }

    #[test]
    fn edits_after_a_carry_over_keep_every_word_once_and_clear_puts_the_whole_bank_in_verse_order(
        source in source_strategy(),
        text in text_strategy(),
        new_text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
        more in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let (stored, _) = edited(&source, &text, &ops);
        let verse = TargetVerse::new(&new_text);
        let mut value = Aligner::open(Some(&stored), None, verse.clone(), &source, basis(1)).unwrap().into_value();
        for op in &more {
            if apply(&mut value, &verse, op) {
                value.settle(&verse);
            }
            prop_assert_eq!(sorted(value.all_target_words().cloned()), sorted(verse.words().to_vec()));
            prop_assert_eq!(value.well_formed(), Ok(()));
            if matches!(op, Op::Clear) {
                prop_assert_eq!(&value.word_bank, verse.words());
            }
        }
        value.clear(&verse);
        prop_assert_eq!(&value.word_bank, verse.words());
        prop_assert_eq!(value.placed(), 0);
    }

    #[test]
    fn the_aligner_keeps_the_bank_and_every_card_in_verse_order_and_every_word_once(
        source in source_strategy(),
        text in text_strategy(),
        new_text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
        stored in prop::bool::ANY,
        steps in prop::collection::vec(step_strategy(), 0..60),
    ) {
        let (value, _) = edited(&source, &text, &ops);
        let verse = TargetVerse::new(&new_text);
        let mut aligner = Aligner::open(stored.then_some(&value), None, verse.clone(), &source, basis(1)).unwrap();
        let source_order: Vec<SourceWord> = source.words().to_vec();
        assert_shown_in_verse_order(&aligner, &verse)?;
        for step in &steps {
            let before = aligner.value().clone();
            let changed_before = aligner.is_changed();
            let selected = aligner.selection();
            let edited = step_on(&mut aligner, step);
            if edited {
                prop_assert!(aligner.is_changed());
                if step.places() {
                    prop_assert!(aligner.selection().is_empty(), "placing clears the selection");
                }
                if matches!(step, Step::PlaceSelection(_)) {
                    let mut left = before.word_bank.clone();
                    left.retain(|w| !selected.contains(w));
                    prop_assert_eq!(&aligner.value().word_bank, &left, "exactly the shown selection is placed");
                }
            } else {
                prop_assert_eq!(aligner.value(), &before, "a refused edit or a click changes nothing");
                prop_assert_eq!(aligner.is_changed(), changed_before);
            }
            assert_shown_in_verse_order(&aligner, &verse)?;
            prop_assert_eq!(sorted(aligner.value().all_target_words().cloned()), sorted(verse.words().to_vec()));
            prop_assert_eq!(aligner.value().well_formed(), Ok(()));
            let flattened: Vec<SourceWord> = aligner.value().alignments.iter().flat_map(|g| g.top_words.clone()).collect();
            prop_assert_eq!(&flattened, &source_order);
            let shown = aligner.shown_bank();
            let selection = aligner.selection();
            prop_assert!(is_subsequence(&selection, &shown), "the selection is shown, in bank order");
            prop_assert_eq!(aligner.is_armed(), !selection.is_empty());
        }
    }

    #[test]
    fn opening_a_value_made_for_the_verse_changes_only_its_order(
        source in source_strategy(),
        text in text_strategy(),
        ops in prop::collection::vec(op_strategy(), 0..40),
    ) {
        let (mut stored, verse) = edited(&source, &text, &ops);
        stored.word_bank.reverse();
        for group in &mut stored.alignments {
            group.bottom_words.reverse();
        }
        let aligner = Aligner::open(Some(&stored), None, verse.clone(), &source, basis(1)).unwrap();
        prop_assert!(!aligner.is_changed(), "order alone is not saved");
        assert_shown_in_verse_order(&aligner, &verse)?;
        let mut sorted_stored = stored.clone();
        sorted_stored.bank_in_verse_order(&verse);
        sorted_stored.cards_in_verse_order(&verse);
        prop_assert_eq!(aligner.value(), &sorted_stored);
        prop_assert_eq!(sorted(aligner.value().word_bank.clone()), sorted(stored.word_bank.clone()));
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
        let mut read = value.clone();
        // A verse with no link has no markup to take apart.
        if let Some((_, back)) = split_aligned(&woven, VerseNo::new(1).unwrap()) {
            read.alignments = back.alignments;
            read.word_bank = back.word_bank;
        }
        prop_assert_eq!(links(&read), links(&value));
        prop_assert_eq!(read.total(), value.total());
    }
}

/// A verse of words from a small vocabulary, so that words repeat, and an edit that only
/// inserts words or only deletes some: where each old word went, `None` when deleted.
#[derive(Debug, Clone)]
struct OneWayEdit {
    before: Vec<&'static str>,
    after: Vec<&'static str>,
    went: Vec<Option<usize>>,
}

const FEW_WORDS: [&str; 3] = ["de", "Dios", "y"];

fn one_way_edit() -> impl Strategy<Value = OneWayEdit> {
    let words = || prop::collection::vec(prop::sample::select(&FEW_WORDS[..]), 1..10);
    (
        words(),
        prop::bool::ANY,
        prop::collection::vec(
            (
                any::<prop::sample::Index>(),
                prop::sample::select(&FEW_WORDS[..]),
            ),
            0..4,
        ),
        prop::collection::vec(prop::bool::ANY, 10),
    )
        .prop_map(|(before, insert, inserts, keep)| {
            if insert {
                // Each old word is preceded by the words inserted at its place.
                let mut at: BTreeMap<usize, Vec<&'static str>> = BTreeMap::new();
                for (index, word) in inserts {
                    at.entry(index.index(before.len() + 1))
                        .or_default()
                        .push(word);
                }
                let (mut after, mut went) = (Vec::new(), Vec::new());
                for (i, word) in before.iter().enumerate() {
                    after.extend(at.get(&i).into_iter().flatten());
                    went.push(Some(after.len()));
                    after.push(*word);
                }
                after.extend(at.get(&before.len()).into_iter().flatten());
                OneWayEdit {
                    before,
                    after,
                    went,
                }
            } else {
                let (mut after, mut went) = (Vec::new(), Vec::new());
                for (i, word) in before.iter().enumerate() {
                    if keep[i] {
                        went.push(Some(after.len()));
                        after.push(*word);
                    } else {
                        went.push(None);
                    }
                }
                OneWayEdit {
                    before,
                    after,
                    went,
                }
            }
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// An edit that only inserts or only deletes words: every link the reflow keeps is on
    /// the very word it was on, and every link to a deleted word is dropped (#789).
    #[test]
    fn a_link_never_moves_to_another_copy_of_its_word(edit in one_way_edit(), every in 1usize..4) {
        let before = TargetVerse::new(&edit.before.join(" "));
        let after = TargetVerse::new(&edit.after.join(" "));
        let source = SourceVerse::new(
            SOURCE_WORDS[..3].iter().map(|w| source_word(w)).collect(),
        );
        let mut value = bootstrap(&source, &before, basis(1));
        // Link every `every`-th word, to cards in turn; remember which old word each is.
        let mut linked = BTreeMap::new();
        for (i, word) in before.words().iter().enumerate().filter(|(i, _)| i % every == 0) {
            value.link(i % 3, word).unwrap();
            linked.insert((word.word.clone(), word.occurrence), (i, i % 3));
        }
        value.settle(&before);
        let next = match tc_align::reflow_edit(&value, &before, &after) {
            Ok(next) => next,
            Err(NoReflow::NoWords) => { prop_assert!(after.words().is_empty()); return Ok(()); }
            Err(NoReflow::Current) => { prop_assert_eq!(&edit.before, &edit.after); return Ok(()); }
            Err(other) => { prop_assert!(false, "unexpected {:?}", other); return Ok(()); }
        };
        prop_assert_eq!(value.placed(), next.value.placed() + next.dropped.len());
        let mut kept = 0;
        for (card, group) in next.value.alignments.iter().enumerate() {
            for word in &group.bottom_words {
                let j = after.position(word).unwrap();
                let came_from = linked
                    .values()
                    .find(|(i, c)| *c == card && edit.went[*i] == Some(j));
                prop_assert!(came_from.is_some(), "{:?} on card {} is not the word linked there", word, card);
                kept += 1;
            }
        }
        for drop in &next.dropped {
            let (_, card) = linked[&(drop.word.word.clone(), drop.word.occurrence)];
            prop_assert_eq!(card, drop.card);
        }
        prop_assert_eq!(kept + next.dropped.len(), linked.len());
    }
}
