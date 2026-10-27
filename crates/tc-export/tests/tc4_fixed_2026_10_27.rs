//! tC4 Align-and-export issues fixed in tC4 before 2026-10-27, replayed on the rebuild's
//! aligned USFM export: a verse aligned in the aligner as the app opens it, then the book
//! exported with its records as the store holds them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;

use common::fixture;
use tc_align::{
    Aligner, Alignment as _, AlignmentValue, Basis, SourceBook, SourceVerse, SourceWord,
    TargetVerse, VerseState, verse_status,
};
use tc_core::{BookId, VerseSpan};
use tc_export::{
    BookInput, ExportFacts, ExportInput, Produced, Producer, SkipReason, VerseAlignment, produce,
};
use tc_journal::fold::{Writer, decode_alignment_record};
use tc_journal::ids::{ChapterNo, VerseNo};
use tc_usfm::codec::{BookParts, VersePart, decompose, join_verse, recompose};

const SOURCE_NT: &str = "dcs::unfoldingWord/el-x-koine_ugnt@v0.34";

fn sample_book() -> BookParts {
    decompose(&fixture("tc4/sample-burrito/ingredients/TIT.usfm")).unwrap()
}

fn sample_record() -> AlignmentValue {
    let file: serde_json::Value = serde_json::from_str(&fixture(
        "tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    decode_alignment_record(&file["chapters"]["1"]["1"], "TIT 1:1", 1, 1, 1, Writer::Tc4)
        .unwrap()
        .2
}

fn ugnt_1_1() -> SourceVerse {
    SourceBook::parse(&fixture("align/el-x-koine_ugnt-TIT.usfm"))
        .verse(1, VerseSpan::single(VerseNo::new(1).unwrap()))
        .unwrap()
}

fn basis(verse: u16) -> Basis {
    Basis::run(
        VerseNo::new(verse).unwrap(),
        VerseNo::new(verse).unwrap(),
        SOURCE_NT.to_owned(),
        None,
    )
}

fn words_of(parts: &mut BookParts, chapter: u16, verse: u16) -> &mut String {
    match parts
        .chapters
        .get_mut(&chapter)
        .and_then(|c| c.verses.get_mut(&verse))
    {
        Some(VersePart::Text(text)) => &mut text.words,
        _ => panic!("{chapter}:{verse} is not a verse with text"),
    }
}

/// The verse as the store holds it, words and layout, as the app hands it to the aligner.
fn stored_verse(parts: &BookParts, chapter: u16, verse: u16) -> String {
    match parts
        .chapters
        .get(&chapter)
        .and_then(|c| c.verses.get(&verse))
    {
        Some(VersePart::Text(text)) => join_verse(&text.words, &text.layout),
        _ => panic!("{chapter}:{verse} is not a verse with text"),
    }
}

fn export(
    book: BookId,
    parts: BookParts,
    at: (u16, u16),
    value: AlignmentValue,
    pin: Option<SourceVerse>,
) -> (String, ExportFacts) {
    let input = ExportInput::Book(BookInput {
        book,
        parts,
        alignments: BTreeMap::from([(
            (ChapterNo::new(at.0).unwrap(), VerseNo::new(at.1).unwrap()),
            VerseAlignment {
                anchor: VerseNo::new(at.1).unwrap(),
                value,
                pin,
            },
        )]),
        source_sha: None,
    });
    let (out, facts) = produce(Producer::AlignedUsfm, &input).unwrap();
    let Produced::File(bytes) = out else {
        panic!("aligned USFM is one file")
    };
    (String::from_utf8(bytes).unwrap(), facts)
}

/// Every word of the open verse placed, one per card in turn, then marked valid.
fn align_all(text: &str, source: &SourceVerse, verse: u16) -> AlignmentValue {
    let mut aligner =
        Aligner::open(None, None, TargetVerse::new(text), source, basis(verse)).unwrap();
    let cards = aligner.value().alignments.len();
    let mut card = 0;
    while let Some(word) = aligner.value().word_bank.first().cloned() {
        aligner.place(card.min(cards - 1), &word).unwrap();
        card += 1;
    }
    aligner.mark_valid().unwrap();
    aligner.into_value()
}

/// tC4 #794 "Align and export: do not write alignments for a verse that holds imported
/// alignment markup". Steps: the en_ult Titus 1:1 added as a book keeps its 19 `\zaln-s`
/// inline; a record for the verse whose hash matches the export's hash (here the sample's
/// record, stamped so) must leave the verse's imported markup byte-identical, and the export
/// names the verse.
#[test]
fn tc4_794_export_keeps_the_19_inline_zaln_of_titus_1_1_with_a_matching_record() {
    let ult = fixture("tc4/test-fixtures/en_ult/TIT.usfm");
    let aligned = tc_usfm::Book::parse(&ult)
        .verse(1, VerseSpan::single(VerseNo::new(1).unwrap()))
        .unwrap()
        .body()
        .to_owned();
    let mut parts = sample_book();
    *words_of(&mut parts, 1, 1) = aligned.clone();
    let stored = recompose(&parts).usfm;
    let mut record = sample_record();
    record.target_verse_md5 = TargetVerse::new(&stored_verse(&parts, 1, 1)).md5();
    let (out, facts) = export(BookId::Titus, parts, (1, 1), record, Some(ugnt_1_1()));
    assert_eq!(out, stored, "the imported alignment is exported as stored");
    assert_eq!(
        out.matches("\\zaln-s").count(),
        aligned.matches("\\zaln-s").count()
    );
    assert_eq!(aligned.matches("\\zaln-s").count(), 19);
    assert_eq!(facts.skipped.len(), 1);
    assert_eq!(facts.skipped[0].why, SkipReason::InlineAlignment);
}

/// tC4 #795 "the client and the export hash a verse from different text", the `\qs` case:
/// Psalm 3:2 of en_ult as a plain draft ends with a `\qs Selah\qs*` line. Steps: align every
/// word of it in Align and mark it valid; the aligned export must weave it, not drop it.
#[test]
fn tc4_795_a_psalm_verse_with_qs_selah_aligned_in_align_exports_woven() {
    let plain_book =
        tc_usfm::strip_alignment(&fixture("tc4/test-fixtures/en_ult/PSA-3.usfm")).unwrap();
    let parts = decompose(&plain_book).unwrap();
    let text = stored_verse(&parts, 3, 2);
    assert!(text.contains("\\qs"), "{text}");
    let hebrew = |word: &str| SourceWord {
        word: word.into(),
        strong: None,
        lemma: Some(word.into()),
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: Default::default(),
    };
    let source = SourceVerse::new(
        ["רַבִּים", "אֹמְרִים", "לְנַפְשִׁי", "אֵין", "יְשׁוּעָתָה", "סֶלָה"]
            .map(hebrew)
            .to_vec(),
    );
    let value = align_all(&text, &source, 2);
    let verse = TargetVerse::new(&text);
    let run = basis(2).aligned_run(&verse);
    assert_eq!(
        verse_status(Some(&value), Some((&run, &verse)), Some(&source)).state,
        VerseState::Valid
    );
    let (out, facts) = export(BookId::Psalms, parts, (3, 2), value, Some(source));
    assert_eq!(facts, ExportFacts::default(), "nothing was left out");
    assert!(out.contains("\\zaln-s"), "the verse was woven");
    assert!(out.contains("Selah"));
}

/// tC4 #796 "derive the word bank from the words the export checks", the footnote case.
/// Steps: Titus 1:1 drafted as "Pablo, siervo\f + \ft o esclavo\f* de Dios"; align every word
/// and mark the verse valid; the aligned export must weave it with `\zaln` and keep the
/// footnote.
#[test]
fn tc4_796_a_footnote_right_after_siervo_exports_aligned_and_keeps_the_footnote() {
    let mut parts = sample_book();
    *words_of(&mut parts, 1, 1) = "Pablo, siervo\\f + \\ft o esclavo\\f* de Dios".to_owned();
    let text = stored_verse(&parts, 1, 1);
    let value = align_all(&text, &ugnt_1_1(), 1);
    assert_eq!(value.placed(), 4);
    let (out, facts) = export(BookId::Titus, parts, (1, 1), value, Some(ugnt_1_1()));
    assert_eq!(facts, ExportFacts::default(), "nothing was left out");
    let verse_1 = out
        .split("\\v 1 ")
        .nth(1)
        .unwrap()
        .split("\\v 2")
        .next()
        .unwrap();
    assert_eq!(verse_1.matches("\\zaln-s").count(), 4, "{verse_1}");
    assert!(verse_1.contains("\\f + \\ft o esclavo\\f*"), "{verse_1}");
}

/// tC4 #796 "use one review rule everywhere". Steps, on the sample Titus 1:1 and its record:
/// for each case of the issue's table (a stale record, a record that lost a placed word, a
/// word both placed and banked, a record marked invalid whose hash matches, Mark valid with
/// nothing placed then a text edit) the rail, the open verse's notice and the export must
/// give the same answer: the export weaves the verse only when the rail and the notice say
/// it does not need review. Every case but the lost word needs review.
#[test]
fn tc4_796_rail_notice_and_export_give_one_answer_for_each_review_case() {
    let t1 = stored_verse(&sample_book(), 1, 1);
    let pin = ugnt_1_1();
    let edited = t1.replacen("siervo", "esclavo", 1);
    let mut lost = sample_record();
    lost.alignments[0].bottom_words.clear();
    let mut twice = sample_record();
    let pablo = twice.alignments[0].bottom_words[0].clone();
    twice.word_bank.push(pablo);
    let mut flagged = sample_record();
    flagged.marked_invalid = true;
    let mut nothing_placed = sample_record();
    for card in &mut nothing_placed.alignments {
        nothing_placed.word_bank.append(&mut card.bottom_words);
    }
    nothing_placed.done = true;
    let cases: Vec<(&str, AlignmentValue, &str)> = vec![
        ("stale", sample_record(), &edited),
        ("lost a placed word", lost, &t1),
        ("placed and banked", twice, &t1),
        ("marked invalid", flagged, &t1),
        (
            "mark valid, nothing placed, then an edit",
            nothing_placed,
            &edited,
        ),
    ];
    for (name, value, text) in cases {
        let mut parts = sample_book();
        *words_of(&mut parts, 1, 1) = text.to_owned();
        let stored = recompose(&parts).usfm;
        let verse = TargetVerse::new(text);
        let run = basis(1).aligned_run(&verse);
        let rail = verse_status(Some(&value), Some((&run, &verse)), Some(&pin)).state
            == VerseState::Invalid;
        let notice = Aligner::open(Some(&value), None, verse.clone(), &pin, basis(1))
            .unwrap()
            .is_stale();
        let (out, facts) = export(
            BookId::Titus,
            parts,
            (1, 1),
            value.clone(),
            Some(pin.clone()),
        );
        let woven = out != stored;
        assert_eq!(rail, notice, "{name}: the rail and the notice disagree");
        assert_eq!(rail, !woven, "{name}: the rail and the export disagree");
        if name == "lost a placed word" {
            // The rebuild's declared rule: a record holding fewer words than the verse still
            // vouches for its links (tC4 reads a record with no word bank); the lost word is
            // banked in Align and exported unaligned, never dropped from the text.
            assert!(woven, "{name}: {facts:?}");
            assert!(out.contains("Pablo"));
            continue;
        }
        assert!(rail, "{name}: the rail shows no review");
        if value.placed() > 0 {
            assert_eq!(
                facts.skipped.iter().map(|s| s.why).collect::<Vec<_>>(),
                [SkipReason::NeedsReview],
                "{name}: the export names the verse"
            );
        }
    }
}

/// tC4 #796: "A valid record with no placed word exports every word wrapped in `\w`, while an
/// absent record exports the stored bytes." Steps: open the sample Titus 1:1 fresh and Mark
/// valid with nothing placed; the aligned export keeps the verse's stored bytes.
#[test]
fn tc4_796_mark_valid_with_nothing_placed_exports_the_stored_bytes() {
    let parts = sample_book();
    let text = stored_verse(&parts, 1, 1);
    let mut aligner =
        Aligner::open(None, None, TargetVerse::new(&text), &ugnt_1_1(), basis(1)).unwrap();
    aligner.mark_valid().unwrap();
    let value = aligner.into_value();
    assert!(value.done);
    assert_eq!(value.placed(), 0);
    let stored = recompose(&parts).usfm;
    let (out, facts) = export(BookId::Titus, parts, (1, 1), value, Some(ugnt_1_1()));
    assert_eq!(out, stored);
    assert!(!out.contains("\\w "));
    assert_eq!(facts, ExportFacts::default());
}
