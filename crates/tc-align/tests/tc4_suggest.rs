//! Alignment suggestions: tC4's `test/align-suggest.test.ts` (#1, #516) ported, and the
//! engine against tC4's own engine on en_ult and en_ust Titus
//! (`fixtures/align/suggest-vectors.json`): the training verses, every link and
//! confidence of memory-only and boosted predictions, and the growth bench's recall.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::*;
use serde_json::Value;
use tc_align::suggest::{
    self, MIN_BOOST_VERSES, Model, PositionLink, RETRAIN_BUDGET, RawLink, SessionInput, TokenSeed,
    TrainingVerse, bound_corpus, crosses_retrain_budget,
};
use tc_align::{AlignmentValue, SourceVerse, SourceWord, TargetVerse};
use tc_core::Testament;

// ---- the engine against tC4's ----

fn seed_of(v: &Value) -> TokenSeed {
    let s = |k: &str| v[k].as_str().map(str::to_owned);
    let n = |k: &str| u32::try_from(v[k].as_u64().unwrap()).unwrap();
    TokenSeed {
        text: s("text").unwrap(),
        position: usize::try_from(v["position"].as_u64().unwrap()).unwrap(),
        occurrence: n("occurrence"),
        occurrences: n("occurrences"),
        strong: s("strong"),
        lemma: s("lemma"),
        morph: s("morph"),
    }
}

fn positions(v: &Value) -> Vec<usize> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| usize::try_from(p.as_u64().unwrap()).unwrap())
        .collect()
}

fn link_of(v: &Value) -> PositionLink {
    PositionLink {
        source: positions(&v["source"]),
        target: positions(&v["target"]),
    }
}

/// The training verses of a book as tC4's benches read them.
fn training(file: &str) -> Vec<TrainingVerse> {
    verses(file)
        .into_iter()
        .filter_map(|(reference, body)| {
            TrainingVerse::from_aligned_usfm(format!("TIT {reference}"), &body).unwrap()
        })
        .collect()
}

fn suggest_cases(kind: &str) -> Vec<Value> {
    vectors("suggest-vectors.json")
        .into_iter()
        .filter(|c| c["kind"] == kind)
        .collect()
}

fn assert_links(ours: &[RawLink], theirs: &Value, at: &str) {
    let theirs = theirs.as_array().unwrap();
    assert_eq!(ours.len(), theirs.len(), "{at}: {ours:?}");
    for (o, t) in ours.iter().zip(theirs) {
        assert_eq!(o.source, positions(&t["source"]), "{at}");
        assert_eq!(o.target, positions(&t["target"]), "{at}");
        let c = t["confidence"].as_f64().unwrap();
        assert!(
            (o.confidence - c).abs() <= 1e-12 * c.abs().max(1.0),
            "{at}: confidence {} against tC4's {c}",
            o.confidence
        );
    }
}

#[test]
fn en_ult_and_en_ust_read_into_the_training_verses_tc4s_benches_read() {
    for case in suggest_cases("training") {
        let file = case["file"].as_str().unwrap();
        let ours = training(file);
        let theirs = case["verses"].as_array().unwrap();
        assert_eq!(ours.len(), theirs.len(), "{file}");
        for (o, t) in ours.iter().zip(theirs) {
            let at = format!("{file} {}", o.reference());
            assert_eq!(o.reference(), t["ref"].as_str().unwrap());
            let seeds = |v: &Value| {
                v.as_array()
                    .unwrap()
                    .iter()
                    .map(seed_of)
                    .collect::<Vec<_>>()
            };
            assert_eq!(o.source(), seeds(&t["source"]), "{at}");
            assert_eq!(o.target(), seeds(&t["target"]), "{at}");
            let links: Vec<_> = t["links"].as_array().unwrap().iter().map(link_of).collect();
            assert_eq!(o.links(), links, "{at}");
        }
    }
}

#[test]
fn memory_only_predictions_equal_tc4s_link_for_link() {
    let ult = training(ULT);
    let by_ref = |r: &str| ult.iter().find(|v| v.reference() == r).unwrap();
    for case in suggest_cases("memory") {
        let k = usize::try_from(case["train"][1].as_u64().unwrap()).unwrap();
        let mut model = Model::empty(Testament::New);
        for verse in &ult[..k] {
            model.append_verse(verse, true);
        }
        assert_eq!(model.verses() as u64, case["verses"].as_u64().unwrap());
        for probe in case["probes"].as_array().unwrap() {
            let reference = probe["ref"].as_str().unwrap();
            let ours = model.predict(&by_ref(reference).as_question());
            assert_links(
                &ours,
                &probe["links"],
                &format!("memory of {k}, {reference}"),
            );
        }
    }
}

#[test]
fn boosted_predictions_equal_tc4s_under_the_same_seed() {
    let ult = training(ULT);
    let by_ref = |r: &str| ult.iter().find(|v| v.reference() == r).unwrap();
    for case in suggest_cases("boosted") {
        let from = usize::try_from(case["train"][0].as_u64().unwrap()).unwrap();
        let to = usize::try_from(case["train"][1].as_u64().unwrap()).unwrap();
        let seed = u32::try_from(case["seed"].as_u64().unwrap()).unwrap();
        let model = Model::train(Testament::New, &ult[from..to], seed);
        assert_eq!(model.verses() as u64, case["verses"].as_u64().unwrap());
        assert_eq!(model.boosted() as u64, case["boosted"].as_u64().unwrap());
        assert_eq!(model.too_few(), case["tooFew"].as_bool().unwrap());
        for probe in case["probes"].as_array().unwrap() {
            let reference = probe["ref"].as_str().unwrap();
            let verse = by_ref(reference);
            let manual: Vec<_> = probe["manual"]
                .as_array()
                .unwrap()
                .iter()
                .map(link_of)
                .collect();
            let at = format!(
                "trained on {from}..{to}, {reference} with {} placed",
                manual.len()
            );
            let question =
                SessionInput::new(verse.source().to_vec(), verse.target().to_vec(), manual)
                    .unwrap();
            assert_links(&model.predict(&question), &probe["links"], &at);
        }
    }
}

/// The growth bench's protocol on a Titus corpus: recall on the held-out verses for
/// plain memory, a booster frozen at `freezeAt` while the memory grows, and a booster
/// retrained at each checkpoint, for the checkpoints up to `upto` verses.
fn growth(case: &Value, upto: usize) -> Vec<Value> {
    let all = training(case["file"].as_str().unwrap());
    let held: Vec<&str> = case["heldOut"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let held_out: Vec<&TrainingVerse> = all
        .iter()
        .filter(|v| held.contains(&v.reference()))
        .collect();
    let train: Vec<TrainingVerse> = all
        .iter()
        .filter(|v| !held.contains(&v.reference()))
        .cloned()
        .collect();
    let freeze_at = usize::try_from(case["freezeAt"].as_u64().unwrap()).unwrap();
    let seed = u32::try_from(case["seed"].as_u64().unwrap()).unwrap();
    let score = |model: &Model| {
        let key = |source: &[usize], target: &[usize]| {
            let mut s = source.to_vec();
            let mut t = target.to_vec();
            s.sort_unstable();
            t.sort_unstable();
            (s, t)
        };
        let (mut correct, mut gold) = (0, 0);
        for verse in &held_out {
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
        serde_json::json!({ "correct": correct, "gold": gold })
    };
    let mut plain = Model::empty(Testament::New);
    let mut frozen: Option<Model> = None;
    let mut fed = 0;
    let mut rows = Vec::new();
    for checkpoint in [1, 5, 10, 25, train.len()]
        .into_iter()
        .filter(|&c| c <= upto)
    {
        while fed < checkpoint {
            plain.append_verse(&train[fed], true);
            if let Some(f) = frozen.as_mut() {
                f.append_verse(&train[fed], true);
            }
            fed += 1;
        }
        if frozen.is_none() && checkpoint >= freeze_at {
            frozen = Some(Model::train(Testament::New, &train[..checkpoint], seed));
        }
        let retrained = Model::train(Testament::New, &train[..checkpoint], seed);
        rows.push(serde_json::json!({
            "verses": checkpoint,
            "plain": score(&plain),
            "frozen": frozen.as_ref().map(score),
            "retrained": score(&retrained),
            "boosted": retrained.boosted(),
        }));
    }
    rows
}

fn growth_case(file: &str) -> Value {
    suggest_cases("growth")
        .into_iter()
        .find(|c| c["file"] == file)
        .unwrap()
}

#[test]
fn the_growth_bench_on_en_ult_titus_gives_tc4s_recall_up_to_ten_verses() {
    let case = growth_case(ULT);
    let rows = case["rows"].as_array().unwrap();
    assert_eq!(growth(&case, 10), rows[..3].to_vec());
    // tC4's measurement: plain memory answers from the first verse (0.18 recall on the
    // New Testament at one verse; 36 of 199 links here).
    assert_eq!(
        rows[0]["plain"],
        serde_json::json!({ "correct": 36, "gold": 199 })
    );
}

#[test]
#[ignore = "trains boosters on up to 36 verses: over a minute in a debug build; run with --release --ignored"]
fn the_growth_bench_on_en_ult_and_en_ust_titus_gives_tc4s_recall() {
    for file in [ULT, UST] {
        let case = growth_case(file);
        assert_eq!(
            growth(&case, usize::MAX),
            case["rows"].as_array().unwrap().clone(),
            "{file}"
        );
    }
}

// ---- tC4's align-suggest.test.ts ----

fn g(text: &str, strong: &str, lemma: &str, morph: &str) -> SourceWord {
    SourceWord {
        word: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some(morph.to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: tc_journal::json::JsonMap::new(),
    }
}

const V11: &str = "Pablo, siervo de Dios";
const V14: &str = "A Tito, de Dios Padre";

fn orig11() -> Vec<SourceWord> {
    vec![
        g("Παῦλος", "G39720", "Παῦλος", "Gr,N,,,,,NMS,"),
        g("δοῦλος", "G14010", "δοῦλος", "Gr,N,,,,,NMS,"),
        g("Θεοῦ", "G23160", "θεός", "Gr,N,,,,,GMS,"),
    ]
}

fn orig14() -> Vec<SourceWord> {
    vec![
        g("Τίτῳ", "G51030", "Τίτος", "Gr,N,,,,,DMS,"),
        g("Θεοῦ", "G23160", "θεός", "Gr,N,,,,,GMS,"),
        g("Πατρὸς", "G39620", "πατήρ", "Gr,N,,,,,GMS,"),
    ]
}

fn boot(text: &str, orig: Vec<SourceWord>) -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(text);
    (bootstrap(&SourceVerse::new(orig), &verse, basis(1)), verse)
}

fn place(value: &mut AlignmentValue, card: usize, text: &str) {
    let word = value
        .word_bank
        .iter()
        .find(|w| w.word == text)
        .cloned()
        .unwrap();
    value.link(card, &word).unwrap();
}

fn aligned11() -> (AlignmentValue, TargetVerse) {
    let (mut v, verse) = boot(V11, orig11());
    place(&mut v, 0, "Pablo");
    place(&mut v, 1, "siervo");
    place(&mut v, 2, "Dios");
    v.settle(&verse);
    (v, verse)
}

fn aligned14() -> (AlignmentValue, TargetVerse) {
    let (mut v, verse) = boot(V14, orig14());
    place(&mut v, 0, "Tito");
    place(&mut v, 1, "Dios");
    v.settle(&verse);
    (v, verse)
}

fn at(seeds: &[TokenSeed]) -> Vec<String> {
    seeds
        .iter()
        .map(|s| format!("{}@{}", s.text, s.position))
        .collect()
}

fn link(source: &[usize], target: &[usize]) -> PositionLink {
    PositionLink {
        source: source.to_vec(),
        target: target.to_vec(),
    }
}

fn raw(source: &[usize], target: &[usize], confidence: f64) -> RawLink {
    RawLink {
        source: source.to_vec(),
        target: target.to_vec(),
        confidence,
    }
}

fn shown(proposals: &[suggest::Proposal]) -> Vec<String> {
    proposals
        .iter()
        .map(|p| format!("{}:{}", p.card, p.word.word))
        .collect()
}

#[test]
fn training_takes_only_verses_with_a_placed_word_tokens_in_verse_order_links_in_positions() {
    let (v11, t11) = aligned11();
    assert!(
        suggest::training_verse(
            "TIT 1:2",
            &boot("Sin alinear", orig11()).0,
            &TargetVerse::new("Sin alinear")
        )
        .is_none()
    );
    let tv = suggest::training_verse("TIT 1:1", &v11, &t11).unwrap();
    assert_eq!(at(tv.source()), ["Παῦλος@0", "δοῦλος@1", "Θεοῦ@2"]);
    assert_eq!(tv.source()[0].strong.as_deref(), Some("G39720"));
    assert_eq!(at(tv.target()), ["Pablo@0", "siervo@1", "de@2", "Dios@3"]);
    assert_eq!(
        tv.links(),
        [link(&[0], &[0]), link(&[1], &[1]), link(&[2], &[3])]
    );
}

#[test]
fn a_placed_word_the_text_no_longer_holds_is_not_a_training_link() {
    let (v11, _) = aligned11();
    let tv = suggest::training_verse("TIT 1:1", &v11, &TargetVerse::new("Pablo, esclavo de Dios"))
        .unwrap();
    assert_eq!(tv.links(), [link(&[0], &[0]), link(&[2], &[3])]);
    assert!(suggest::training_verse("TIT 1:1", &v11, &TargetVerse::new("")).is_none());
}

#[test]
fn the_session_input_carries_the_placed_links_as_context() {
    let (mut v, verse) = boot(V14, orig14());
    place(&mut v, 2, "Padre");
    let input = suggest::session_input(&v, &verse);
    assert_eq!(input.manual(), [link(&[2], &[4])]);
    let target: Vec<_> = input.target().iter().map(|t| t.text.as_str()).collect();
    assert_eq!(target, ["A", "Tito", "de", "Dios", "Padre"]);
}

#[test]
fn proposals_are_bank_words_each_once_on_the_card_of_the_source_and_never_a_placed_word() {
    let (mut v, verse) = boot(V14, orig14());
    place(&mut v, 2, "Padre");
    let links = suggest::proposals(
        &v,
        &verse,
        &[
            raw(&[1], &[3], 0.9), // Θεοῦ → Dios (bank)
            raw(&[2], &[4], 0.8), // Πατρὸς → Padre (placed: dropped)
            raw(&[0], &[1], 0.7), // Τίτῳ → Tito (bank)
            raw(&[0], &[1], 0.1), // the same target again: dropped
        ],
    );
    assert_eq!(shown(&links), ["1:Dios", "0:Tito"]);
}

#[test]
fn a_prediction_over_two_cards_is_left_out_never_cut_to_the_first() {
    let (v, verse) = boot(V14, orig14());
    let links = suggest::proposals(&v, &verse, &[raw(&[0, 1], &[1], 0.9), raw(&[2], &[4], 0.8)]);
    assert_eq!(shown(&links), ["2:Padre"]);
    assert_eq!(links[0].source.word, "Πατρὸς");
    assert_eq!(links[0].source.occurrence, 1);
}

#[test]
fn rebind_follows_the_predicted_original_word_through_a_split_and_a_merge() {
    let (v, verse) = boot(V14, orig14());
    let mut merged = v.clone();
    merged.merge(0, 1).unwrap();
    let links = suggest::proposals(&merged, &verse, &[raw(&[1], &[3], 0.9)]);
    assert_eq!(shown(&links), ["0:Dios"]);
    assert_eq!(shown(&suggest::rebind(&v, &links)), ["1:Dios"]);

    let links = suggest::proposals(&v, &verse, &[raw(&[1], &[3], 0.9), raw(&[2], &[4], 0.8)]);
    let rebound = suggest::rebind(&merged, &links);
    assert_eq!(shown(&rebound), ["0:Dios", "1:Padre"]);
    // Placing Padre by hand spends its proposal; placing Dios too leaves none.
    let mut placed = merged.clone();
    place(&mut placed, 1, "Padre");
    let left = suggest::rebind(&placed, &rebound);
    assert_eq!(shown(&left), ["0:Dios"]);
    place(&mut placed, 0, "Dios");
    assert!(suggest::rebind(&placed, &left).is_empty());
}

#[test]
fn a_project_with_no_aligned_verses_suggests_nothing() {
    let model = Model::train(Testament::New, &[], 0);
    assert_eq!(model.verses(), 0);
    let (v, verse) = boot(V14, orig14());
    assert!(
        model
            .predict(&suggest::session_input(&v, &verse))
            .is_empty()
    );
}

fn two_verses() -> Vec<TrainingVerse> {
    let (v11, t11) = aligned11();
    let (v14, t14) = aligned14();
    vec![
        suggest::training_verse("TIT 1:1", &v11, &t11).unwrap(),
        suggest::training_verse("TIT 1:4", &v14, &t14).unwrap(),
    ]
}

#[test]
fn trained_on_two_verses_it_proposes_theou_to_dios_and_never_the_placed_padre() {
    let trained = Model::train(Testament::New, &two_verses(), 0);
    assert_eq!(trained.verses(), 2);
    assert!(two_verses().len() < MIN_BOOST_VERSES);
    assert_eq!(trained.boosted(), 0, "no booster below five verses");
    let (mut v, verse) = boot("de Dios Padre", orig14()[1..].to_vec());
    place(&mut v, 1, "Padre");
    let before = v.clone();
    let raw = trained.predict(&suggest::session_input(&v, &verse));
    let links = suggest::proposals(&v, &verse, &raw);
    let dios = links.iter().find(|l| l.word.word == "Dios").unwrap();
    assert_eq!(dios.card, 0, "the Θεοῦ card");
    assert!(links.iter().all(|l| l.word.word != "Padre"));
    // The value is untouched by any of this.
    assert_eq!(v, before);
}

/// A verse of `sources` original words and 300 target words with one link.
fn wide(sources: usize) -> TrainingVerse {
    let words: Vec<SourceWord> = (0..sources)
        .map(|i| g(&format!("a{i}"), "G1", "a", "Gr,N"))
        .collect();
    let (mut value, verse) = boot(&"b ".repeat(300), words);
    place(&mut value, 0, "b");
    suggest::training_verse("wide", &value, &verse).unwrap()
}

#[test]
fn bound_corpus_drops_verses_until_the_complexity_fits_keeping_one() {
    let big = wide(300);
    assert_eq!(big.source().len() * big.target().len(), 90_000);
    assert_eq!(bound_corpus(&[big.clone(), big.clone()], 100_000), 1);
    assert_eq!(bound_corpus(std::slice::from_ref(&big), 10), 1);
}

#[test]
fn every_confirmed_verse_is_in_the_memory_and_the_booster_fits_the_capped_share() {
    let verses = two_verses();
    let small = vec![
        verses[0].clone(),
        verses[1].clone(),
        verses[0].relabeled("TIT 1:1c"),
    ];
    // Two oversize verses: 400 source words over 300 target words each.
    let big = wide(400);
    let all = [small, vec![big.relabeled("big1"), big.relabeled("big2")]].concat();
    let trained = Model::train(Testament::New, &all, 0);
    assert_eq!(trained.verses(), 5);
    assert_eq!(trained.boosted(), 3);
}

#[test]
fn below_five_verses_the_memory_answers_with_at_least_one_proposal() {
    let trained = Model::train(Testament::New, &two_verses(), 0);
    assert_eq!(trained.boosted(), 0);
    let (v, verse) = boot("de Dios Padre", orig14()[1..].to_vec());
    let raw = trained.predict(&suggest::session_input(&v, &verse));
    assert!(!suggest::proposals(&v, &verse, &raw).is_empty());
}

#[test]
fn a_confirmed_save_joins_the_memory_at_once_and_a_resave_keeps_the_count() {
    let (v11, t11) = aligned11();
    let mut trained = Model::train(
        Testament::New,
        &[suggest::training_verse("TIT 1:1", &v11, &t11).unwrap()],
        0,
    );
    assert_eq!(trained.verses(), 1);
    let (v14, t14) = aligned14();
    let tv14 = suggest::training_verse("TIT 1:4", &v14, &t14).unwrap();
    trained.append_verse(&tv14, true);
    assert_eq!(trained.verses(), 2);
    // Τίτῳ → Tito exists only in the appended verse's links.
    let (v, verse) = boot("A Tito", orig14()[..1].to_vec());
    let links = suggest::proposals(
        &v,
        &verse,
        &trained.predict(&suggest::session_input(&v, &verse)),
    );
    assert!(links.iter().any(|l| l.word.word == "Tito"));
    trained.append_verse(&tv14, false);
    assert_eq!(trained.verses(), 2);
}

#[test]
fn the_retrain_budget_is_crossed_only_when_a_step_is_first_reached() {
    assert_eq!(RETRAIN_BUDGET, [10, 25, 50, 100, 250, 500, 1000]);
    assert!(crosses_retrain_budget(9, 10));
    assert!(!crosses_retrain_budget(10, 11));
    assert!(crosses_retrain_budget(8, 12));
    assert!(!crosses_retrain_budget(600, 601));
    assert!(crosses_retrain_budget(999, 1000));
    assert!(!crosses_retrain_budget(1000, 1001));
}

#[test]
fn a_booster_trained_elsewhere_attached_to_a_grown_memory_answers_like_the_trained_model() {
    // tC4's align-suggest-handover.test.ts: the background training's booster joins the
    // memory the answering model built verse by verse.
    let ult = training(ULT);
    let corpus = &ult[..12];
    let trained = Model::train(Testament::New, corpus, 0);
    assert!(trained.boosted() > 0);
    let mut grown = Model::empty(Testament::New);
    for verse in corpus {
        grown.append_verse(verse, true);
    }
    let plain = grown.clone();
    grown.attach_booster(trained.booster().unwrap().clone(), trained.boosted());
    let probes = &ult[20..30];
    for probe in probes {
        assert_eq!(
            grown.predict(&probe.as_question()),
            trained.predict(&probe.as_question())
        );
    }
    assert!(
        probes
            .iter()
            .any(|p| !trained.predict(&p.as_question()).is_empty())
    );
    assert!(
        probes
            .iter()
            .any(|p| plain.predict(&p.as_question()) != trained.predict(&p.as_question()))
    );
}
