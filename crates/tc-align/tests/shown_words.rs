//! The words the aligner banks are every word of the verse shown, and the words the export
//! weaves (tC4 #782, #795 and #796's word list).
//!
//! No test here opens the aligner on content that still holds alignment markup: such a
//! verse is kept as written by the import and does not open in the aligner (tC4 #794). Its
//! bank is tested through [`TargetVerse`], and the aligner opens only split plain text.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;
use std::io::Read;
use tc_align::{
    Aligner, Alignment, AlignmentGroup, AlignmentValue, Basis, SourceVerse, SourceWord,
    TargetVerse, TargetWord,
};
use tc_core::VerseSpan;
use tc_journal::alignment::split_aligned;
use tc_journal::ids::VerseNo;
use tc_journal::json::JsonMap;
use tc_usfm::{Book, Document, Node, codec};

fn bundle_entry(archive: &str, entry: &str) -> String {
    let file = std::fs::File::open(common::fixture(&format!("bundle/en/{archive}"))).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut text = String::new();
    zip.by_name(entry)
        .unwrap_or_else(|e| panic!("{archive} {entry}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

/// Every verse of a book as the codec cuts it: `chapter:verse` and its content.
fn verses(raw: &str) -> Vec<(String, String)> {
    let parts = codec::decompose(raw).unwrap();
    let mut out = Vec::new();
    for (c, chapter) in &parts.chapters {
        for (v, part) in &chapter.verses {
            if let codec::VersePart::Text(verse) = part {
                out.push((
                    format!("{c}:{}", codec::label(*v, verse.last)),
                    codec::join_verse(&verse.words, &verse.layout),
                ));
            }
        }
    }
    out
}

fn verse_of(raw: &str, reference: &str) -> String {
    verses(raw)
        .into_iter()
        .find(|(r, _)| r == reference)
        .unwrap_or_else(|| panic!("no {reference}"))
        .1
}

fn verse_no(n: u16) -> VerseNo {
    VerseNo::new(n).unwrap()
}

fn triples(words: &[TargetWord]) -> Vec<(String, u32, u32)> {
    words
        .iter()
        .map(|w| (w.word.clone(), w.occurrence, w.occurrences))
        .collect()
}

fn export_words(plain: &str) -> Option<Vec<(String, u32, u32)>> {
    let found = tc_usfm::target_words(plain).ok()?;
    Some(
        found
            .into_iter()
            .map(|w| (w.text, w.occurrence, w.occurrences))
            .collect(),
    )
}

/// TC4-ISSUES row 2's "tC4-imported verse with nested `\zaln`": Genesis 1:1 as a tC4 4.0
/// book added from aligned USFM stores it, with its markup inline. Its bank holds every word
/// of the verse, "the heavens" included; split as the import splits it, it opens against the
/// Hebrew with every link in place.
#[test]
fn a_tc4_imported_verse_with_nested_zaln_banks_every_word() {
    let content = verse_of(
        &common::read("fixtures/tc4/test-fixtures/en_ult/GEN-1-1.usfm"),
        "1:1",
    );
    let verse = TargetVerse::new(&content);
    assert_eq!(
        verse.text(),
        "In the beginning God created the heavens and the earth."
    );
    let words: Vec<(&str, u32, u32)> = verse
        .words()
        .iter()
        .map(|w| (w.word.as_str(), w.occurrence, w.occurrences))
        .collect();
    assert_eq!(
        words,
        [
            ("In", 1, 1),
            ("the", 1, 3),
            ("beginning", 1, 1),
            ("God", 1, 1),
            ("created", 1, 1),
            ("the", 2, 3),
            ("heavens", 1, 1),
            ("and", 1, 1),
            ("the", 3, 3),
            ("earth", 1, 1),
        ]
    );
    let (plain, value) = split_aligned(&content, verse_no(1)).unwrap();
    let hebrew = common::read("fixtures/tc4/test-fixtures/hbo_uhb/GEN-1-1.usfm");
    let source = SourceVerse::from_book(&Book::parse(&hebrew), 1, VerseSpan::single(1)).unwrap();
    assert_eq!(source.words().len(), 7);
    let basis = Basis::run(
        verse_no(1),
        verse_no(1),
        "dcs::unfoldingWord/hbo_uhb@v3.0.0".into(),
        None,
    );
    let aligner = Aligner::open(Some(&value), TargetVerse::new(&plain), &source, basis).unwrap();
    assert_eq!(value.placed(), 10);
    assert_eq!(aligner.value().placed(), 10);
    assert!(aligner.dropped().is_empty());
}

/// Psalm 34:4 of the ULT is kept as written by the import (its markup does not read back
/// byte for byte), so the verse keeps its markup and its bank comes from it: "Yahweh" sits
/// in an alignment nested inside another.
#[test]
fn a_verse_kept_as_written_shows_yahweh() {
    let content = verse_of(
        &common::read("fixtures/scale/psa/en_ult/19-PSA.usfm"),
        "34:4",
    );
    assert!(split_aligned(&content, verse_no(4)).is_none());
    let verse = TargetVerse::new(&content);
    assert_eq!(
        verse.text(),
        "I sought Yahweh, and he answered me,\nand from all of my fears he delivered me."
    );
    assert!(triples(verse.words()).contains(&("Yahweh".to_owned(), 1, 1)));
}

/// TC4-ISSUES row 3: after the import's split, the ULT's MAT 5:22, 5:44 and 1TH 3:2 hold a
/// footnote right after a word. The words on either side stay apart, so every link the
/// import placed is kept when the verse opens (before, "brotherwill" dropped 2 of 44).
#[test]
fn imported_mat_5_22_opens_with_all_44_links() {
    for (book, chapter, verse, links, apart) in [
        ("MAT", 5, 22, 44, ["brother", "will"]),
        ("MAT", 5, 44, 17, ["enemies", "and"]),
        ("1TH", 3, 2, 24, ["God", "in"]),
    ] {
        let raw = bundle_entry(
            "en_ult_v91.sb.zip",
            &format!("en_ult/ingredients/{book}.usfm"),
        );
        let content = verse_of(&raw, &format!("{chapter}:{verse}"));
        let (plain, value) = split_aligned(&content, verse_no(verse)).unwrap();
        let greek = bundle_entry(
            "el-x-koine_ugnt_v0.34.sb.zip",
            &format!("el-x-koine_ugnt/ingredients/{book}.usfm"),
        );
        let source =
            SourceVerse::from_book(&Book::parse(&greek), chapter, VerseSpan::single(verse))
                .unwrap();
        let basis = Basis::run(
            verse_no(verse),
            verse_no(verse),
            common::SOURCE_NT.into(),
            None,
        );
        let target = TargetVerse::new(&plain);
        let shown: Vec<&str> = target.words().iter().map(|w| w.word.as_str()).collect();
        assert!(
            shown.windows(2).any(|pair| pair == apart),
            "{book} {chapter}:{verse}: {:?}",
            target.text()
        );
        assert_eq!(target.words().len(), links, "{book} {chapter}:{verse}");
        let aligner = Aligner::open(Some(&value), target, &source, basis).unwrap();
        assert_eq!(value.placed(), links, "{book} {chapter}:{verse}");
        assert_eq!(aligner.value().placed(), links, "{book} {chapter}:{verse}");
        assert!(
            aligner.dropped().is_empty(),
            "{book} {chapter}:{verse}: {:?}",
            aligner.dropped()
        );
    }
}

/// TC4-ISSUES row 5: a footnote right after "siervo". The bank holds "siervo" and "de", the
/// export's words, so a value that places every word weaves (before, the bank held
/// "siervode" and the weave failed with `TargetWordNotFound`, dropping the verse's alignment
/// from the export).
#[test]
fn a_footnote_right_after_a_word_banks_both_words_and_weaves() {
    let plain = "Pablo, siervo\\f + \\ft o esclavo\\f* de Dios.";
    let verse = TargetVerse::new(plain);
    assert_eq!(Some(triples(verse.words())), export_words(plain));
    let card = |(n, word): (usize, &TargetWord)| AlignmentGroup {
        top_words: vec![SourceWord {
            word: format!("λ{n}"),
            strong: Some(format!("G{n:04}0")),
            lemma: Some("λόγος".into()),
            morph: None,
            occurrence: 1,
            occurrences: 1,
            extra: JsonMap::new(),
        }],
        bottom_words: vec![word.clone()],
        extra: JsonMap::new(),
    };
    let value = AlignmentValue {
        span_end: verse_no(1),
        alignments: verse.words().iter().enumerate().map(card).collect(),
        word_bank: Vec::new(),
        target_verse_md5: verse.md5(),
        source_version: "".into(),
        source: None,
        done: true,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    };
    let woven = tc_align::usfm::weave_verse(plain, &value).unwrap();
    assert!(woven.contains("\\f + \\ft o esclavo\\f*"), "{woven}");
    assert!(
        woven.contains("\\w siervo|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
        "{woven}"
    );
    assert!(
        woven.contains("\\w de|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"),
        "{woven}"
    );
    assert_eq!(tc_usfm::parse_aligned(&woven).unwrap().plain, plain);
}

/// The aligner's bank ([`TargetVerse`]) is the export's word list for every verse of the
/// shown-text vectors' drafts with no word markup: notes, cross references, figures and
/// other markers between words.
#[test]
fn the_bank_is_the_export_word_list_for_every_note_draft() {
    let vectors: serde_json::Value =
        serde_json::from_str(&common::read("fixtures/usfm/shown-text-vectors.json")).unwrap();
    let mut differ = Vec::new();
    let mut compared = 0;
    for (name, raw) in vectors["drafts"].as_object().unwrap() {
        for (reference, content) in verses(raw.as_str().unwrap()) {
            let Some(export) = export_words(&content) else {
                continue;
            };
            compared += 1;
            if triples(TargetVerse::new(&content).words()) != export {
                differ.push(format!("{name} {reference}"));
            }
        }
    }
    assert_eq!(differ, Vec::<String>::new());
    assert_eq!(compared, 23);
}

/// Notes, cross references and figures: their words are not verse text.
const NOTES: &[&str] = &["f", "fe", "ef", "x", "ex", "fig"];

/// The `\w` words of a verse's content in file order, notes left out.
fn raw_words(content: &str) -> Vec<String> {
    fn walk(nodes: &[Node<'_>], out: &mut Vec<String>) {
        for node in nodes {
            let Node::Span(span) = node else { continue };
            if NOTES.contains(&span.name()) {
                continue;
            }
            if span.name() != "w" {
                walk(&span.children, out);
                continue;
            }
            let text: String = span
                .children
                .iter()
                .filter_map(|c| match c {
                    Node::Text(t) => Some(*t),
                    _ => None,
                })
                .collect();
            let word = text.split('|').next().unwrap_or("");
            out.extend(
                tc_usfm::words::words(word)
                    .iter()
                    .map(|w| w.text.to_owned()),
            );
        }
    }
    let mut out = Vec::new();
    walk(Document::parse(content).nodes(), &mut out);
    out
}

/// What the shown text does to every verse of the ULT v91 and UST v91 with its markup inline,
/// for `docs/evidence/`. Run
/// `cargo test --release -p tc-align --test shown_words -- --ignored --nocapture`.
#[test]
#[ignore = "a census for the evidence file; prints, asserts nothing"]
fn shown_text_census_of_the_bundle() {
    for (archive, id) in [
        ("en_ult_v91.sb.zip", "en_ult"),
        ("en_ust_v91.sb.zip", "en_ust"),
    ] {
        let names: Vec<String> = {
            let file = std::fs::File::open(common::fixture(&format!("bundle/en/{archive}")));
            let zip = zip::ZipArchive::new(file.unwrap()).unwrap();
            zip.file_names()
                .filter(|n| n.ends_with(".usfm"))
                .map(str::to_owned)
                .collect()
        };
        let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
        let mut differ = Vec::new();
        for name in names {
            let book = name.rsplit('/').next().unwrap().trim_end_matches(".usfm");
            let mut row: BTreeMap<&str, usize> = BTreeMap::new();
            for (reference, content) in verses(&bundle_entry(archive, &name)) {
                *row.entry("verses").or_default() += 1;
                if !content.contains("\\zaln-s") {
                    continue;
                }
                *row.entry("with markup").or_default() += 1;
                let shown: Vec<String> = TargetVerse::new(&content)
                    .words()
                    .iter()
                    .map(|w| w.word.clone())
                    .collect();
                let raw = raw_words(&content);
                let mut rest = shown.iter();
                let lost_raw = !raw.iter().all(|w| rest.any(|s| s == w));
                if lost_raw {
                    *row.entry("lose \\w words").or_default() += 1;
                }
                let split = tc_usfm::parse_aligned(&content);
                let lost_split = match &split {
                    Ok(split) => {
                        let plain: Vec<String> = TargetVerse::new(&split.plain)
                            .words()
                            .iter()
                            .map(|w| w.word.clone())
                            .collect();
                        if Some(triples(TargetVerse::new(&split.plain).words()))
                            != export_words(&split.plain)
                        {
                            *row.entry("split: bank is not export").or_default() += 1;
                            differ.push(format!("{id} {book} {reference}"));
                        }
                        plain != shown
                    }
                    Err(_) => {
                        *row.entry("split refused").or_default() += 1;
                        false
                    }
                };
                if lost_split {
                    *row.entry("differ from split").or_default() += 1;
                }
                let Some(n) = reference
                    .split(':')
                    .nth(1)
                    .and_then(|v| v.split('-').next_back())
                    .and_then(|v| v.parse::<u16>().ok())
                    .and_then(VerseNo::new)
                else {
                    continue;
                };
                if split_aligned(&content, n).is_none() {
                    *row.entry("kept as written").or_default() += 1;
                    if lost_raw || lost_split {
                        *row.entry("kept and lose words").or_default() += 1;
                    }
                }
            }
            println!("{id} {book}: {row:?}");
            for (k, v) in row {
                *totals.entry(k).or_default() += v;
            }
        }
        println!("{id} total: {totals:?}");
        println!("{id} split plains whose bank is not the export's: {differ:?}");
    }
}
