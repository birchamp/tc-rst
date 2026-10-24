//! tC4's alignment editing tests, ported: `test/align-edit.test.ts` (C2.11, C2.12, #734,
//! #129) and `test/align-done.test.ts` (#271, D73), at d310bf3. The verses and words are
//! tC4's.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;
use tc_align::Alignment;
use tc_align::bootstrap;

use common::{basis, show, tc4_record};
use tc_align::{
    AlignedRun, AlignmentValue, EditError, GitOid, ResourceRef, SourceVerse, SourceWord, Stale,
    TargetVerse, TargetWord, VerseState, verse_status,
};
use tc_journal::json::JsonMap;

fn greek(text: &str, strong: &str, lemma: &str, morph: &str) -> SourceWord {
    SourceWord {
        word: text.to_owned(),
        strong: Some(strong.to_owned()),
        lemma: Some(lemma.to_owned()),
        morph: Some(morph.to_owned()),
        occurrence: 1,
        occurrences: 1,
        extra: JsonMap::new(),
    }
}

fn orig() -> SourceVerse {
    SourceVerse::new(vec![
        greek("Παῦλος", "G39720", "Παῦλος", "Gr,N"),
        greek("δοῦλος", "G14010", "δοῦλος", "Gr,N"),
    ])
}

fn orig3() -> SourceVerse {
    let mut words = orig().words().to_vec();
    words.push(greek("Θεοῦ", "G23160", "θεός", "Gr,N"));
    SourceVerse::new(words)
}

const TARGET: &str = "Pablo, siervo de Dios y de Dios";

fn boot_with(source: &SourceVerse, text: &str) -> (AlignmentValue, TargetVerse) {
    let verse = TargetVerse::new(text);
    (bootstrap(source, &verse, basis(1)), verse)
}

fn boot() -> (AlignmentValue, TargetVerse) {
    boot_with(&orig(), TARGET)
}

fn word(text: &str, occurrence: u32, occurrences: u32) -> TargetWord {
    common::word(text, occurrence, occurrences)
}

fn bank_word(value: &AlignmentValue, text: &str, occurrence: u32) -> TargetWord {
    value
        .word_bank
        .iter()
        .find(|w| w.word == text && w.occurrence == occurrence)
        .cloned()
        .unwrap_or_else(|| panic!("{text} {occurrence} is not banked"))
}

fn sources(value: &AlignmentValue, card: usize) -> Vec<&str> {
    value.alignments[card]
        .top_words
        .iter()
        .map(|w| w.word.as_str())
        .collect()
}

fn targets(value: &AlignmentValue, card: usize) -> Vec<&str> {
    value.alignments[card]
        .bottom_words
        .iter()
        .map(|w| w.word.as_str())
        .collect()
}

// ---- C2.12 bootstrap: an unaligned verse starts usable, not empty ----

#[test]
fn bootstrap_offers_one_card_per_original_word_each_with_no_target_words() {
    let (value, _) = boot();
    assert_eq!(value.alignments.len(), 2);
    assert!(value.alignments.iter().all(|g| g.bottom_words.is_empty()));
    assert_eq!(sources(&value, 0), ["Παῦλος"]);
}

#[test]
fn bootstrap_banks_every_target_word() {
    let (value, _) = boot();
    assert_eq!(value.word_bank.len(), 7);
    assert!(value.word_bank.iter().any(|w| w.word == "Pablo"));
}

#[test]
fn bootstrap_records_the_draft_it_was_built_against() {
    let (value, verse) = boot();
    assert_eq!(value.target_verse_md5, verse.md5());
    assert_eq!(value.target_verse_md5.to_string().len(), 32);
    assert_eq!(&*value.source_version, common::SOURCE_NT);
    assert!(!value.marked_invalid);
    assert!(!value.is_stale_against(&verse));
}

#[test]
fn bootstrap_distinguishes_repeated_words_by_occurrence() {
    let (value, _) = boot();
    let dios: Vec<_> = value
        .word_bank
        .iter()
        .filter(|w| w.word == "Dios")
        .collect();
    assert_eq!(dios.len(), 2);
    assert_eq!(
        dios.iter().map(|w| w.occurrence).collect::<Vec<_>>(),
        [1, 2]
    );
    assert!(dios.iter().all(|w| w.occurrences == 2));
}

#[test]
fn bootstrap_banks_in_tc4_order_every_first_occurrence_then_every_second() {
    let (value, _) = boot_with(&orig(), "de Dios y de Pablo");
    assert_eq!(
        show(&value.word_bank),
        ["de/1/2", "Dios/1/1", "y/1/1", "Pablo/1/1", "de/2/2"]
    );
}

// ---- C2.11 link / unlink: a word is in exactly one place ----

#[test]
fn the_two_dios_occurrences_link_independently() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Dios", 1)).unwrap();
    assert_eq!(
        value.word_bank.iter().filter(|w| w.word == "Dios").count(),
        1
    );
    value.link(1, &bank_word(&value, "Dios", 2)).unwrap();
    assert_eq!(
        value.word_bank.iter().filter(|w| w.word == "Dios").count(),
        0
    );
    assert_eq!(value.alignments[0].bottom_words[0].occurrence, 1);
    assert_eq!(value.alignments[1].bottom_words[0].occurrence, 2);
}

#[test]
fn a_word_that_is_not_banked_cannot_be_linked() {
    let (mut value, _) = boot();
    let pablo = bank_word(&value, "Pablo", 1);
    value.link(0, &pablo).unwrap();
    let once = value.clone();
    assert_eq!(value.link(1, &pablo), Err(EditError::NotInBank(pablo)));
    assert_eq!(value, once);
}

#[test]
fn an_out_of_range_card_is_refused_not_a_crash() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(
        value.link(99, &bank_word(&before, "Pablo", 1)),
        Err(EditError::NoSuchCard(99))
    );
    assert_eq!(value, before);
}

#[test]
fn unlinking_a_word_that_is_not_there_is_refused() {
    let (mut value, verse) = boot();
    let before = value.clone();
    let pablo = bank_word(&value, "Pablo", 1);
    assert!(matches!(
        value.unlink(0, &pablo, &verse),
        Err(EditError::NotOnCard { card: 0, .. })
    ));
    assert_eq!(value, before);
}

// ---- #734 unlink: the bank follows the verse order (align-edit.test.ts:103-137) ----

fn keys(words: &[TargetWord]) -> Vec<String> {
    words
        .iter()
        .map(|w| format!("{}/{}", w.word, w.occurrence))
        .collect()
}

/// The verse's words that `bank` holds, in verse order.
fn verse_order(verse: &TargetVerse, bank: &[TargetWord]) -> Vec<String> {
    let banked = keys(bank);
    keys(verse.words())
        .into_iter()
        .filter(|k| banked.contains(k))
        .collect()
}

#[test]
fn a_repeated_word_returns_to_its_own_occurrence_slot() {
    let (mut value, verse) = boot();
    value.link(0, &bank_word(&value, "de", 2)).unwrap();
    value.link(1, &bank_word(&value, "Dios", 1)).unwrap();
    value.unlink(0, &word("de", 2, 2), &verse).unwrap();
    assert_eq!(
        keys(&value.word_bank),
        verse_order(&verse, &value.word_bank)
    );
    value.unlink(1, &word("Dios", 1, 2), &verse).unwrap();
    assert_eq!(keys(&value.word_bank), keys(verse.words()));
}

#[test]
fn a_bank_stored_out_of_verse_order_comes_back_in_verse_order() {
    let (mut value, verse) = boot();
    value.link(0, &bank_word(&value, "siervo", 1)).unwrap();
    value.word_bank.reverse();
    value.unlink(0, &word("siervo", 1, 1), &verse).unwrap();
    assert_eq!(keys(&value.word_bank), keys(verse.words()));
}

#[test]
fn a_word_the_text_no_longer_has_goes_last_the_others_in_verse_order() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let edited = TargetVerse::new(&TARGET.replace("Pablo", "Saulo"));
    value.unlink(0, &word("Pablo", 1, 1), &edited).unwrap();
    let mut expected: Vec<String> = keys(edited.words())
        .into_iter()
        .filter(|k| k != "Saulo/1")
        .collect();
    expected.push("Pablo/1".to_owned());
    assert_eq!(keys(&value.word_bank), expected);
}

#[test]
fn an_unlink_puts_a_bank_in_bootstrap_order_into_verse_order() {
    // tC4's bootstrap banks every first occurrence before every second one; the first
    // unlink puts the whole bank in verse order, as tC4's `unlinkWord` does.
    let (mut value, verse) = boot_with(&orig(), "de Dios y de Pablo");
    assert_eq!(
        keys(&value.word_bank),
        ["de/1", "Dios/1", "y/1", "Pablo/1", "de/2"]
    );
    value.link(0, &bank_word(&value, "y", 1)).unwrap();
    value.unlink(0, &word("y", 1, 1), &verse).unwrap();
    assert_eq!(
        keys(&value.word_bank),
        ["de/1", "Dios/1", "y/1", "de/2", "Pablo/1"]
    );
}

#[test]
fn an_unlink_from_a_bank_in_verse_order_moves_no_other_bank_word() {
    // The bank as the aligner keeps it: in verse order. A word sent back takes its own
    // place and every other bank word keeps its place relative to the rest.
    let (mut value, verse) = boot();
    value.bank_in_verse_order(&verse);
    let placed = [
        (0, word("de", 2, 2)),
        (1, word("siervo", 1, 1)),
        (0, word("Dios", 2, 2)),
    ];
    for (card, w) in &placed {
        value.link(*card, w).unwrap();
    }
    for (card, w) in [&placed[1], &placed[2], &placed[0]] {
        let before = keys(&value.word_bank);
        value.unlink(*card, w, &verse).unwrap();
        let after = keys(&value.word_bank);
        let key = format!("{}/{}", w.word, w.occurrence);
        let others: Vec<String> = after.iter().filter(|k| **k != key).cloned().collect();
        assert_eq!(others, before, "unlinking {key} moved another bank word");
        assert_eq!(after, verse_order(&verse, &value.word_bank), "{key}");
    }
    assert_eq!(keys(&value.word_bank), keys(verse.words()));
}

// ---- #736 several words placed on one card in one edit ----

#[test]
fn link_many_places_the_banked_words_and_skips_the_rest() {
    let (mut value, _) = boot();
    value.link(1, &bank_word(&value, "y", 1)).unwrap();
    let words = [word("Dios", 2, 2), word("y", 1, 1), word("Pablo", 1, 1)];
    assert_eq!(value.link_many(0, &words), Ok(2), "y is on card 1 already");
    assert_eq!(targets(&value, 0), ["Dios", "Pablo"], "in the order given");
    assert_eq!(targets(&value, 1), ["y"]);
    assert_eq!(value.total(), 7);
}

#[test]
fn link_many_with_nothing_to_place_or_no_such_card_changes_nothing() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let before = value.clone();
    assert_eq!(
        value.link_many(1, &[word("Pablo", 1, 1)]),
        Err(EditError::NothingToPlace)
    );
    assert_eq!(value.link_many(1, &[]), Err(EditError::NothingToPlace));
    assert_eq!(
        value.link_many(99, &[word("Dios", 1, 2)]),
        Err(EditError::NoSuchCard(99))
    );
    assert_eq!(value, before);
}

// ---- #129 merge / split: phrase alignments stay original-anchored ----

#[test]
fn merge_concatenates_sources_and_targets_in_card_order_at_the_lower_card() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.link(1, &bank_word(&value, "siervo", 1)).unwrap();
    let total = value.total();
    value.merge(1, 0).unwrap();
    assert_eq!(value.alignments.len(), 1);
    assert_eq!(sources(&value, 0), ["Παῦλος", "δοῦλος"]);
    assert_eq!(targets(&value, 0), ["Pablo", "siervo"]);
    assert_eq!(value.total(), total, "no word entered or left the verse");
}

#[test]
fn merge_with_the_same_or_an_out_of_range_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(value.merge(0, 0), Err(EditError::NotAdjacent(0, 0)));
    assert_eq!(value.merge(0, 99), Err(EditError::NoSuchCard(99)));
    assert_eq!(value, before);
}

#[test]
fn split_restores_single_cards_with_the_placed_words_on_the_first() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.merge(0, 1).unwrap();
    value.split(0).unwrap();
    assert_eq!(value.alignments.len(), 2);
    assert_eq!(sources(&value, 0), ["Παῦλος"]);
    assert_eq!(targets(&value, 0), ["Pablo"]);
    assert!(value.alignments[1].bottom_words.is_empty());
}

#[test]
fn a_card_field_from_an_import_is_kept_and_no_edit_loses_it() {
    let (value, verse) = boot();
    let mut record = serde_json::json!({
        "alignments": [
            {"topWords": [{"word": "Παῦλος", "strong": "G39720", "lemma": "Παῦλος",
                           "morph": "Gr,N", "occurrence": 1, "occurrences": 1}],
             "bottomWords": []},
            {"topWords": [{"word": "δοῦλος", "strong": "G14010", "lemma": "δοῦλος",
                           "morph": "Gr,N", "occurrence": 1, "occurrences": 1}],
             "bottomWords": [], "note": {"by": "tC3"}}
        ],
        "wordBank": [],
        "invalid": false,
        "targetVerseMd5": value.target_verse_md5.to_hex(),
        "sourceVersion": "dcs::unfoldingWord/el-x-koine_ugnt@v0.34"
    });
    record["wordBank"] = serde_json::Value::Array(
        value
            .word_bank
            .iter()
            .map(|w| serde_json::json!({"word": w.word, "occurrence": w.occurrence, "occurrences": w.occurrences}))
            .collect(),
    );
    let mut read = tc4_record(&record).unwrap();
    let note = tc_journal::json::JsonMap::from([(
        "note".to_owned(),
        tc_journal::json::Json::Obj(tc_journal::json::JsonMap::from([(
            "by".to_owned(),
            tc_journal::json::Json::from("tC3"),
        )])),
    )]);
    assert_eq!(read.alignments[1].extra, note);

    // Merging the card into its neighbour would leave its field on no card.
    let before = read.clone();
    assert_eq!(read.merge(0, 1), Err(EditError::CardFields(1)));
    assert_eq!(read, before);

    // Clearing keeps it on the card's first original word.
    read.link(1, &bank_word(&read, "siervo", 1)).unwrap();
    read.clear(&verse);
    assert_eq!(read.alignments[1].extra, note);
    assert_eq!(read.placed(), 0);
}

#[test]
fn split_on_a_single_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert_eq!(value.split(0), Err(EditError::NotAPhrase(0)));
    assert_eq!(value, before);
}

#[test]
fn move_word_moves_a_placed_word_between_cards_and_leaves_the_bank() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let bank = value.word_bank.clone();
    value.move_word(0, 1, &word("Pablo", 1, 1)).unwrap();
    assert!(value.alignments[0].bottom_words.is_empty());
    assert_eq!(targets(&value, 1), ["Pablo"]);
    assert_eq!(
        value.word_bank, bank,
        "the bank is identical, order included"
    );
}

#[test]
fn a_move_to_a_missing_card_or_onto_itself_changes_nothing() {
    let (mut value, _) = boot();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    let before = value.clone();
    let pablo = word("Pablo", 1, 1);
    assert_eq!(value.move_word(0, 0, &pablo), Err(EditError::SameCard));
    assert_eq!(
        value.move_word(0, 99, &pablo),
        Err(EditError::NoSuchCard(99))
    );
    assert_eq!(value, before);
}

#[test]
fn a_non_adjacent_merge_is_refused_so_phrases_stay_in_verse_order() {
    let (mut value, _) = boot_with(&orig3(), TARGET);
    assert_eq!(value.alignments.len(), 3);
    let before = value.clone();
    assert_eq!(value.merge(0, 2), Err(EditError::NotAdjacent(0, 2)));
    assert_eq!(value.merge(2, 0), Err(EditError::NotAdjacent(2, 0)));
    assert_eq!(value, before);
}

#[test]
fn adjacent_merges_keep_verse_order_whichever_way_they_are_dragged() {
    let (value, _) = boot_with(&orig3(), TARGET);
    let mut down = value.clone();
    down.merge(2, 1).unwrap();
    assert_eq!(sources(&down, 1), ["δοῦλος", "Θεοῦ"]);
    let mut up = value;
    up.merge(0, 1).unwrap();
    assert_eq!(sources(&up, 0), ["Παῦλος", "δοῦλος"]);
}

#[test]
fn move_word_of_a_word_not_on_the_from_card_changes_nothing() {
    let (mut value, _) = boot();
    let before = value.clone();
    assert!(matches!(
        value.move_word(0, 1, &word("Pablo", 1, 1)),
        Err(EditError::NotOnCard { card: 0, .. })
    ));
    assert_eq!(value, before);
}

#[test]
fn clear_starts_the_verse_over_with_the_bank_in_verse_order() {
    let text = "de Dios y de Pablo";
    let (fresh, verse) = boot_with(&orig3(), text);
    let mut value = fresh.clone();
    value.link(2, &bank_word(&value, "de", 2)).unwrap();
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.merge(0, 1).unwrap();
    value.clear(&verse);
    assert_eq!(value.alignments, fresh.alignments);
    assert_eq!(
        value.word_bank,
        verse.words(),
        "verse order, not tC4's bootstrap order"
    );
    assert_ne!(fresh.word_bank, verse.words());
}

// ---- #271 (D73): all aligned is done; Mark valid; any edit takes it back ----

const TEXT: &str = "Pablo siervo";

/// Pablo placed on Παῦλος; "siervo" still in the bank.
fn half_aligned() -> (AlignmentValue, TargetVerse) {
    let (mut value, verse) = boot_with(&orig(), TEXT);
    value.link(0, &bank_word(&value, "Pablo", 1)).unwrap();
    value.target_verse_md5 = verse.md5();
    (value, verse)
}

fn current(verse: &TargetVerse) -> AlignedRun {
    AlignedRun {
        anchor: 1,
        run_start: 1,
        run_end: 1,
        text_md5: verse.md5(),
        source_sha: None,
    }
}

#[test]
fn fully_aligned_only_when_the_bank_is_empty_and_every_card_has_a_word() {
    let (half, _) = half_aligned();
    assert!(!half.is_fully_aligned());
    let mut full = half.clone();
    full.link(1, &bank_word(&full, "siervo", 1)).unwrap();
    assert!(full.is_fully_aligned());
    // A bootstrap (nothing placed) is not aligned even if the bank were empty.
    let (mut empty, _) = boot_with(&orig(), TEXT);
    empty.word_bank.clear();
    assert!(!empty.is_fully_aligned());
}

#[test]
fn placing_the_last_word_sets_done() {
    let (mut value, verse) = half_aligned();
    value.settle(&verse);
    assert!(!value.done);
    value.link(1, &bank_word(&value, "siervo", 1)).unwrap();
    value.settle(&verse);
    assert!(value.done);
}

#[test]
fn mark_done_sets_done_with_words_in_the_bank_and_vouches_for_the_current_text() {
    let (stale, _) = half_aligned();
    let new_text = TargetVerse::new("Pablo, siervo");
    assert!(stale.is_stale_against(&new_text));
    let mut marked = stale.clone();
    marked.mark_done(&new_text);
    assert!(marked.done);
    assert_eq!(marked.word_bank.len(), 1);
    assert!(!marked.is_stale_against(&new_text));
    // A record flagged for re-review is re-reviewed by Mark valid.
    let mut flagged = stale;
    flagged.marked_invalid = true;
    flagged.mark_done(&new_text);
    assert!(!flagged.marked_invalid);
    assert_eq!(
        verse_status(Some(&flagged), Some((&current(&new_text), &new_text)), None).state,
        VerseState::Valid
    );
}

#[test]
fn a_reflow_recomputes_done() {
    let (mut full, verse) = half_aligned();
    full.link(1, &bank_word(&full, "siervo", 1)).unwrap();
    full.settle(&verse);
    assert!(full.done);
    // "siervo" becomes "esclavo": its link drops, the bank is not empty: not done.
    let dropped = tc_align::reflow(&full, &TargetVerse::new("Pablo esclavo")).unwrap();
    assert!(!dropped.value.done);
    // Only punctuation changed: every word still placed: still done.
    let kept = tc_align::reflow(&full, &TargetVerse::new("Pablo, siervo.")).unwrap();
    assert!(kept.value.done);
}

#[test]
fn the_rail_reads_valid_from_done_only_and_staleness_outranks_it() {
    let (half, verse) = half_aligned();
    let now = current(&verse);
    let mut full_no_flag = half.clone();
    full_no_flag
        .link(1, &bank_word(&full_no_flag, "siervo", 1))
        .unwrap();
    let state = |v: Option<&AlignmentValue>, c: Option<(&AlignedRun, &TargetVerse)>| {
        verse_status(v, c, None).state
    };
    let now = (&now, &verse);
    assert_eq!(state(Some(&full_no_flag), Some(now)), VerseState::Todo);
    let mut settled = full_no_flag.clone();
    settled.settle(&verse);
    assert_eq!(state(Some(&settled), Some(now)), VerseState::Valid);
    let mut marked = half.clone();
    marked.mark_done(&verse);
    assert_eq!(state(Some(&marked), Some(now)), VerseState::Valid);
    let other = TargetVerse::new("Pablo siervo de Dios");
    assert_eq!(
        state(Some(&marked), Some((&current(&other), &other))),
        VerseState::Invalid
    );
    let mut flagged = marked;
    flagged.marked_invalid = true;
    assert_eq!(state(Some(&flagged), Some(now)), VerseState::Invalid);
    assert_eq!(state(None, Some(now)), VerseState::Todo);
    assert_eq!(state(None, None), VerseState::Undrafted);
    let status = verse_status(Some(&half), Some(now), None);
    assert_eq!((status.placed, status.total), (1, 2));
}

/// Owner ruling Q37 (tC4 #814, D105 point 5): a verse marked "needs review" stays marked after
/// an edit of its alignment, until the translator clicks Mark valid.
#[test]
fn an_edit_keeps_the_review_mark_until_mark_valid() {
    let (mut flagged, verse) = half_aligned();
    flagged.marked_invalid = true;
    flagged.link(1, &bank_word(&flagged, "siervo", 1)).unwrap();
    flagged.settle(&verse);
    assert!(flagged.marked_invalid, "an edit keeps the mark");
    assert_eq!(
        verse_status(Some(&flagged), Some((&current(&verse), &verse)), None).state,
        VerseState::Invalid
    );
    flagged.mark_done(&verse);
    assert!(!flagged.marked_invalid, "Mark valid clears it");
    assert_eq!(
        verse_status(Some(&flagged), Some((&current(&verse), &verse)), None).state,
        VerseState::Valid
    );
}

#[test]
fn staleness_names_each_reason_a_done_value_no_longer_vouches_for_the_verse() {
    let (mut value, verse) = half_aligned();
    value.mark_done(&verse);
    let pin: GitOid = "0123456789abcdef0123456789abcdef01234567".parse().unwrap();
    let upgrade: GitOid = "89abcdef0123456789abcdef0123456789abcdef".parse().unwrap();
    value.source = Some(ResourceRef {
        repo_path: "git.door43.org/unfoldingWord/el-x-koine_ugnt".into(),
        sha: pin,
    });
    let other = TargetVerse::new("Pablo siervo de Dios");
    fn at(run_end: u16, text: &TargetVerse, sha: Option<&GitOid>) -> AlignedRun {
        AlignedRun {
            anchor: 1,
            run_start: 1,
            run_end,
            text_md5: text.md5(),
            source_sha: sha.copied(),
        }
    }
    let (pin, upgrade) = (&pin, &upgrade);
    // The verse renumbered to 2 with its text: the value is placed at the run 2 and still
    // made for its span (STORAGE.md §7.7).
    let renumbered = AlignedRun {
        anchor: 1,
        run_start: 2,
        run_end: 2,
        ..at(1, &verse, Some(pin))
    };
    let cases = [
        (at(1, &verse, Some(pin)), vec![]),
        (renumbered, vec![]),
        (
            AlignedRun {
                run_end: 3,
                ..renumbered
            },
            vec![Stale::SpanChanged],
        ),
        // Joined into the span 1-2 since.
        (at(2, &verse, Some(pin)), vec![Stale::SpanChanged]),
        (at(1, &other, Some(pin)), vec![Stale::TextChanged]),
        // The original-language text was upgraded, or unpinned (STORAGE.md §5.8).
        (at(1, &verse, Some(upgrade)), vec![Stale::SourceChanged]),
        (at(1, &verse, None), vec![Stale::SourceChanged]),
        (
            at(2, &other, Some(upgrade)),
            vec![Stale::SpanChanged, Stale::TextChanged, Stale::SourceChanged],
        ),
    ];
    for (now, reasons) in &cases {
        assert_eq!(&value.staleness(now), reasons, "{now:?}");
        let text = if now.text_md5 == verse.md5() {
            &verse
        } else {
            &other
        };
        let state = verse_status(Some(&value), Some((now, text)), None).state;
        let expected = if reasons.is_empty() {
            VerseState::Valid
        } else {
            VerseState::Invalid
        };
        assert_eq!(state, expected, "{now:?}");
    }
    let mut flagged = value.clone();
    flagged.marked_invalid = true;
    assert_eq!(
        flagged.staleness(&at(2, &other, None)),
        [
            Stale::SpanChanged,
            Stale::TextChanged,
            Stale::SourceChanged,
            Stale::MarkedInvalid
        ]
    );
    // A value with no pin (an import that could not resolve one) is not stale for its
    // source, whatever the project pins now.
    let mut unpinned = value;
    unpinned.source = None;
    assert!(unpinned.staleness(&at(1, &verse, Some(upgrade))).is_empty());
}
