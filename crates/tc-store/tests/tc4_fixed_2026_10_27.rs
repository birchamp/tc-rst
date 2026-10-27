//! tC4 Align issues fixed in tC4 before 2026-10-27, replayed through the store's real
//! commands: a Translate verse save and the alignment it reflows in the same batch, and an
//! alignment saved and read back after the project is closed and opened again.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    reason = "test code: a failed unwrap is a failed test; the shared helpers are larger than \
              what this file uses"
)]

#[path = "store/common.rs"]
mod common;

use common::*;
use tc_align::{
    Aligner, Alignment as _, AlignmentValue, Basis, SourceBook, SourceVerse, TargetVerse,
    VerseState, verse_status,
};
use tc_core::VerseSpan;
use tc_journal::command::{AlignmentEdit, Command};
use tc_journal::fold::{Writer, decode_alignment_record};
use tc_journal::record::{AlignmentGroup, SourceWord, TargetWord, VerseValue};
use tc_journal::{ChapterNo, Record, RecordKey, VerseNo};
use tc_store::{OpenOptions, ProjectHandle};
use unicode_normalization::UnicodeNormalization;

const SOURCE_NT: &str = "dcs::unfoldingWord/el-x-koine_ugnt@v0.34";

fn fixture(rel: &str) -> String {
    std::fs::read_to_string(repo_root().join(rel)).unwrap()
}

fn alignment_key() -> RecordKey {
    RecordKey::Alignment(TIT, ChapterNo::new(1).unwrap(), VerseNo::new(1).unwrap())
}

/// The sample burrito's TIT 1:1 text and its record.
fn sample() -> (String, AlignmentValue) {
    let usfm = fixture("fixtures/tc4/sample-burrito/ingredients/TIT.usfm");
    let text = tc_usfm::Book::parse(&usfm)
        .verse(1, VerseSpan::single(tc_core::VerseNo::new(1).unwrap()))
        .unwrap()
        .body()
        .to_owned();
    let file: serde_json::Value = serde_json::from_str(&fixture(
        "fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let value =
        decode_alignment_record(&file["chapters"]["1"]["1"], "TIT 1:1", 1, 1, 1, Writer::Tc4)
            .unwrap()
            .2;
    (text, value)
}

fn ugnt_1_1() -> SourceVerse {
    SourceBook::parse(&fixture("fixtures/align/el-x-koine_ugnt-TIT.usfm"))
        .verse(1, VerseSpan::single(tc_core::VerseNo::new(1).unwrap()))
        .unwrap()
}

/// 1:1 as the store holds it: words and layout, as the app hands it to the aligner.
fn stored_text(handle: &ProjectHandle) -> String {
    match handle.snapshot().fold().projected(&verse(1, 1)) {
        Some(Record::Verse {
            value: VerseValue::Text { words, layout, .. },
            ..
        }) => format!("{words}{layout}"),
        other => panic!("{other:?}"),
    }
}

fn alignment(handle: &ProjectHandle) -> AlignmentValue {
    match handle.snapshot().fold().projected(&alignment_key()) {
        Some(Record::Alignment { value, .. }) => value,
        other => panic!("{other:?}"),
    }
}

fn save_alignment(
    handle: &ProjectHandle,
    alignments: Vec<AlignmentGroup>,
    bank: Vec<TargetWord>,
    done: bool,
) {
    let shown = handle.mint(&alignment_key(), None).unwrap();
    let verse_shown = handle.mint(&verse(1, 1), None).unwrap();
    handle
        .submit(Command::SaveAlignment {
            shown,
            verse_shown,
            value: AlignmentEdit {
                alignments,
                word_bank: bank,
                done,
            },
        })
        .wait()
        .unwrap();
}

fn card_of(value: &AlignmentValue, word: &str, occurrence: u32) -> usize {
    value
        .alignments
        .iter()
        .position(|g| {
            g.top_words
                .iter()
                .any(|s| s.word == word && s.occurrence == occurrence)
        })
        .unwrap_or_else(|| panic!("no card for {word} {occurrence}"))
}

fn on_card(value: &AlignmentValue, card: usize) -> Vec<String> {
    value.alignments[card]
        .bottom_words
        .iter()
        .map(|w| format!("{}/{}/{}", w.word, w.occurrence, w.occurrences))
        .collect()
}

fn basis() -> Basis {
    Basis::run(
        VerseNo::new(1).unwrap(),
        VerseNo::new(1).unwrap(),
        SOURCE_NT.to_owned(),
        None,
    )
}

/// tC4 #789 "Align: after a verse edit adds or removes an earlier copy of a repeated word, a
/// link moves to another copy with no warning". Steps (sample Titus 1:1 and its record: Θεοῦ¹
/// on the Dios of "siervo de Dios", Θεοῦ² on the Dios of "escogidos de Dios"): in Translate
/// delete "de Dios" after "siervo" and save. The link of the surviving Dios stays on Θεοῦ²,
/// Θεοῦ¹ gets no Dios, and the link of the deleted word is listed by the save.
#[test]
fn tc4_789_translate_save_deleting_de_dios_after_siervo_keeps_dios_on_its_own_theou() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let (t1, record) = sample();
    save_verse(&handle, 1, 1, &t1).unwrap();
    save_alignment(&handle, record.alignments, record.word_bank, false);
    let before = alignment(&handle);
    assert_eq!(on_card(&before, card_of(&before, "Θεοῦ", 1)), ["Dios/1/2"]);
    assert_eq!(on_card(&before, card_of(&before, "Θεοῦ", 2)), ["Dios/2/2"]);

    let reply = save_verse(&handle, 1, 1, &t1.replacen("siervo de Dios", "siervo", 1)).unwrap();
    let after = alignment(&handle);
    assert!(on_card(&after, card_of(&after, "Θεοῦ", 1)).is_empty());
    assert_eq!(on_card(&after, card_of(&after, "Θεοῦ", 2)), ["Dios/1/1"]);
    let listed: Vec<(String, u32, String)> = reply
        .dropped_links
        .iter()
        .flat_map(|d| &d.links)
        .map(|l| {
            (
                l.top_words[0].word.clone(),
                l.top_words[0].occurrence,
                l.word.word.clone(),
            )
        })
        .collect();
    assert_eq!(listed, [("Θεοῦ".to_owned(), 1, "Dios".to_owned())]);
    handle.close().unwrap();
}

/// tC4 #820 "Flag ambiguous repeated-word replacements for alignment review". Steps: give a
/// drafted verse the words "Dios Dios"; in Align place only the first Dios; in Translate save
/// the verse as "Dios Pablo". The link must not survive silently: the verse is flagged, or the
/// link goes back to the bank and the save lists it.
#[test]
fn tc4_820_translate_save_of_dios_dios_as_dios_pablo_keeps_no_unsure_link() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    save_verse(&handle, 1, 1, "Dios Dios").unwrap();
    let theos = |word: &str| SourceWord {
        word: word.into(),
        strong: Some("G23160".into()),
        lemma: Some("θεός".into()),
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: Default::default(),
    };
    let dios = |occurrence| TargetWord {
        word: "Dios".into(),
        occurrence,
        occurrences: 2,
        extra: Default::default(),
    };
    save_alignment(
        &handle,
        vec![
            AlignmentGroup {
                top_words: vec![theos("θεοῦ")],
                bottom_words: vec![dios(1)],
                extra: Default::default(),
            },
            AlignmentGroup {
                top_words: vec![theos("θεὸς")],
                bottom_words: Vec::new(),
                extra: Default::default(),
            },
        ],
        vec![dios(2)],
        false,
    );
    assert_eq!(alignment(&handle).placed(), 1);

    let reply = save_verse(&handle, 1, 1, "Dios Pablo").unwrap();
    let after = alignment(&handle);
    let listed: usize = reply.dropped_links.iter().map(|d| d.links.len()).sum();
    if after.placed() > 0 {
        assert!(
            after.marked_invalid,
            "an unsure link was kept silently: {:?}",
            on_card(&after, 0)
        );
    } else {
        assert_eq!(listed, 1, "the dropped link is listed by the save");
    }
    handle.close().unwrap();
}

/// tC4 #795 "the client and the export hash a verse from different text". Steps: type Titus
/// 1:1 in a decomposed (NFD) form and save it; align every word in the same session and mark
/// it valid; close and open the project again. The verse must read valid, not invalid, and
/// the export's weave must take it.
#[test]
fn tc4_795_an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen() {
    let (rig, folder) = titus();
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let (t1, _) = sample();
    let nfd: String = t1.nfd().collect();
    assert_ne!(nfd, t1, "the verse has a letter NFD writes otherwise");
    save_verse(&handle, 1, 1, &nfd).unwrap();
    let pin = ugnt_1_1();
    let shown = stored_text(&handle);
    let mut aligner = Aligner::open(None, None, TargetVerse::new(&shown), &pin, basis()).unwrap();
    let cards = aligner.value().alignments.len();
    let mut card = 0;
    while let Some(word) = aligner.value().word_bank.first().cloned() {
        aligner.place(card.min(cards - 1), &word).unwrap();
        card += 1;
    }
    aligner.mark_valid().unwrap();
    let value = aligner.into_value();
    save_alignment(&handle, value.alignments, value.word_bank, value.done);
    handle.close().unwrap();

    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    let text = stored_text(&handle);
    let value = alignment(&handle);
    let verse = TargetVerse::new(&text);
    let run = basis().aligned_run(&verse);
    assert_eq!(value.target_verse_md5, verse.md5());
    assert_eq!(
        verse_status(Some(&value), Some((&run, &verse)), Some(&pin)).state,
        VerseState::Valid
    );
    let woven = tc_align::usfm::weave_verse(&text, &value).unwrap();
    assert!(woven.contains("\\zaln-s"));
    handle.close().unwrap();
}
