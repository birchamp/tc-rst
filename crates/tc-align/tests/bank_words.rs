//! The words the aligner banks are the words the export weaves (tC4 #782, #795 and #796's
//! word list): `TargetVerse` takes them from `tc_usfm::target_words`, the export's own list,
//! and the property here checks the whole path, from the bank through the export and back,
//! on every verse of the bundled ULT and UST and on generated USFM.
//!
//! No test here opens the aligner on content that still holds alignment markup: such a
//! verse is kept as written by the import and does not open in the aligner (tC4 #794). Its
//! bank is tested through [`TargetVerse`], and the aligner opens only split plain text.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use proptest::prelude::*;
use tc_align::{
    Aligner, Alignment, AlignmentGroup, AlignmentValue, Basis, SourceVerse, SourceWord,
    TargetVerse, TargetWord,
};
use tc_core::VerseSpan;
use tc_journal::alignment::split_aligned;
use tc_journal::ids::VerseNo;
use tc_journal::json::JsonMap;
use tc_usfm::Book;

use common::{bundle_entry, codec_verses as verses};

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
    let source =
        SourceVerse::from_book(&Book::parse(&hebrew), 1, VerseSpan::range(1, 1).unwrap()).unwrap();
    assert_eq!(source.words().len(), 7);
    let basis = Basis::run(
        verse_no(1),
        verse_no(1),
        "dcs::unfoldingWord/hbo_uhb@v3.0.0".into(),
        None,
    );
    let aligner =
        Aligner::open(Some(&value), None, TargetVerse::new(&plain), &source, basis).unwrap();
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

/// Splits a bundle verse as the import splits it and opens it against its source; returns
/// its bank, the links the split placed, the links kept at open and the links dropped.
fn open_imported(
    (archive, id): (&str, &str),
    (source_archive, source_id, source_name): (&str, &str, &str),
    book: &str,
    chapter: u16,
    verse: u16,
) -> (Vec<String>, usize, usize, usize) {
    let raw = bundle_entry(archive, &format!("{id}/ingredients/{book}.usfm"));
    let content = verse_of(&raw, &format!("{chapter}:{verse}"));
    let (plain, value) = split_aligned(&content, verse_no(verse)).unwrap();
    let original = bundle_entry(
        source_archive,
        &format!("{source_id}/ingredients/{book}.usfm"),
    );
    let source = SourceVerse::from_book(
        &Book::parse(&original),
        chapter,
        VerseSpan::range(verse, verse).unwrap(),
    )
    .unwrap();
    let basis = Basis::run(verse_no(verse), verse_no(verse), source_name.into(), None);
    let target = TargetVerse::new(&plain);
    let bank = target.words().iter().map(|w| w.word.clone()).collect();
    let aligner = Aligner::open(Some(&value), None, target, &source, basis).unwrap();
    (
        bank,
        value.placed(),
        aligner.value().placed(),
        aligner.dropped().len(),
    )
}

const UGNT: (&str, &str, &str) = (
    "el-x-koine_ugnt_v0.34.sb.zip",
    "el-x-koine_ugnt",
    common::SOURCE_NT,
);

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
        let at = format!("{book} {chapter}:{verse}");
        let (bank, placed, kept, dropped) =
            open_imported(("en_ult_v91.sb.zip", "en_ult"), UGNT, book, chapter, verse);
        assert!(bank.windows(2).any(|pair| pair == apart), "{at}: {bank:?}");
        assert_eq!(
            (bank.len(), placed, kept, dropped),
            (links, links, links, 0),
            "{at}"
        );
    }
}

/// UST EXO 15:21 holds its song in two `\qm1` lines, whose text usfm-js keeps as `content`.
/// The bank holds those words, so all 28 links the import placed are kept at open (before,
/// "Sing to Yahweh … enemies." was hidden from the bank, which held 17 words, and 11 dropped).
#[test]
fn imported_ust_exo_15_21_opens_with_all_its_links() {
    let uhb = (
        "hbo_uhb_v3.0.0.sb.zip",
        "hbo_uhb",
        "dcs::unfoldingWord/hbo_uhb@v3.0.0",
    );
    let (bank, placed, kept, dropped) =
        open_imported(("en_ust_v91.sb.zip", "en_ust"), uhb, "EXO", 15, 21);
    assert!(
        bank.windows(2).any(|pair| pair == ["song", "Sing"]),
        "{bank:?}"
    );
    assert_eq!((bank.len(), placed, kept, dropped), (28, 28, 28, 0));
}

/// The source word of the card that places word `n` of a verse.
fn source(n: usize) -> SourceWord {
    SourceWord {
        word: format!("λ{n}"),
        strong: Some(format!("G{n:04}0")),
        lemma: Some("λόγος".into()),
        morph: None,
        occurrence: 1,
        occurrences: 1,
        extra: JsonMap::new(),
    }
}

/// How the export writes a verse whose every banked word is placed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Export {
    /// It weaves every banked word, and nothing else, as an aligned `\w` word.
    Woven,
    /// The import keeps the verse as written (its alignment does not read apart), so the
    /// export writes its bytes and weaves nothing.
    KeptAsWritten,
    /// Its plain text holds `\w` markup that is not alignment (a glossary word), which weave
    /// refuses, so the export writes its bytes.
    WordMarkup,
}

/// The bank of `content` against the export: the bank of the content as written is the bank
/// of the plain text the import's split gives it, and a value placing each banked word on a
/// card of its own weaves into that plain text, whose aligned `\w` words read back are the
/// bank, word for word with their occurrences. `Err` says what differs.
fn bank_against_export(content: &str) -> Result<Export, String> {
    let bank = triples(TargetVerse::new(content).words());
    let Ok(split) = tc_usfm::parse_aligned(content) else {
        return Ok(Export::KeptAsWritten);
    };
    let split_bank = TargetVerse::new(&split.plain);
    if triples(split_bank.words()) != bank {
        return Err(format!(
            "the split banks {:?}, the verse as written {bank:?}",
            triples(split_bank.words())
        ));
    }
    let value = AlignmentValue {
        span_end: verse_no(1),
        alignments: split_bank
            .words()
            .iter()
            .enumerate()
            .map(|(n, word)| AlignmentGroup {
                top_words: vec![source(n)],
                bottom_words: vec![word.clone()],
                extra: JsonMap::new(),
            })
            .collect(),
        word_bank: Vec::new(),
        target_verse_md5: split_bank.md5(),
        source_version: "".into(),
        source: None,
        done: true,
        marked_invalid: false,
        dismissed: false,
        extra: JsonMap::new(),
    };
    let woven = match tc_align::usfm::weave_verse(&split.plain, &value) {
        Ok(woven) => woven,
        Err(tc_usfm::Error::WordMarkup) => return Ok(Export::WordMarkup),
        Err(e) => return Err(format!("the export refuses the bank {bank:?}: {e}")),
    };
    let back = tc_usfm::parse_aligned(&woven).map_err(|e| format!("{woven:?}: {e}"))?;
    let exported: Vec<(String, u32, u32)> = back
        .groups
        .iter()
        .flat_map(|g| &g.targets)
        .chain(&back.words)
        .map(|w| (w.text.clone(), w.occurrence, w.occurrences))
        .collect();
    let (aligned, all) = exported.split_at(exported.len() - back.words.len());
    if aligned != bank || all != bank {
        return Err(format!(
            "the export writes {aligned:?} aligned of {all:?}, the bank holds {bank:?}"
        ));
    }
    Ok(Export::Woven)
}

/// Verses whose words an earlier shown text banked otherwise than the export weaves them: a
/// note right after a word (before, the bank held "siervode" and the weave failed with
/// `TargetWordNotFound`), a note inside a `\wj` span, the words after a span nested in another
/// ("Lord your God"), a span or paragraph marker right before a letter ("Lords",
/// "Yahwehque", "神愛"), a `\li1` list, a `\qm1` line and the text of markers usfm-js does
/// not know; the attributes of such a marker, which the shown text reads as words; and the
/// text after a heading, which the shown text banked through an unknown marker.
const VECTORS: &[&str] = &[
    "Pablo, siervo\\f + \\ft o esclavo\\f* de Dios.",
    "Pablo, siervo\\f + \\ft o esclavo\\f*de Dios.",
    "a\\fig x|src=\"y\"\\fig*b",
    "Jesus said, \\wj Blessed are the poor\\f + \\ft Or humble\\f* in spirit\\wj* and he went on to teach them.",
    "\\wj a\\f + \\ft n\\f* b\\wj* c",
    "Jesus said, \\wj Worship the \\+nd Lord\\+nd* your God\\wj* and the end.",
    "the \\wj said \\+add the\\+add* go\\wj* the end",
    "the \\add Lord\\add*s house",
    "Yahweh\\q2 que",
    "\\rb 神|かみ\\rb*\\rb 愛|あい\\rb* x",
    "a\n\\li1 b c\n\\li2 d e",
    "a\n\\qm1 b c",
    "a\n\\pp b c",
    "a\n\\zpara b c",
    "a \\zx b\\zx* c",
    "a \\zx b|x-note=\"the hidden word\"\\zx* c",
    "a \\zx b|gloss\\zx*c \\pp d|x-y=\"e\" f",
    "a\n\\s A heading\n\\zx b\\zx* c\n\\p d",
    "a\n\\s A heading \\zx b|x-y=\"z\"\\zx* c\n\\q1 d \\zx e\\zx* f",
    "\\d A psalm \\zx of|x=\"y\"\\zx* David.\n\\q1 Yahweh \\qs Selah\\qs*",
    "a \\w grace|strong=\"G5485\"\\w* b",
];

#[test]
fn the_bank_is_the_export_for_every_vector() {
    let outcomes: Vec<Export> = VECTORS
        .iter()
        .map(|v| bank_against_export(v).unwrap_or_else(|e| panic!("{v:?}: {e}")))
        .collect();
    let (glossary, woven) = outcomes.split_last().unwrap();
    assert_eq!(*glossary, Export::WordMarkup);
    assert!(woven.iter().all(|o| *o == Export::Woven), "{outcomes:?}");
    let words = |v: &str| -> Vec<String> {
        TargetVerse::new(v)
            .words()
            .iter()
            .map(|w| w.word.clone())
            .collect()
    };
    assert_eq!(words(VECTORS[1]), ["Pablo", "siervo", "de", "Dios"]);
    assert_eq!(words(VECTORS[15]), ["a", "b", "c"]);
    assert_eq!(words(VECTORS[17]), ["a", "d"]);
}

/// Every verse of the bundled ULT v91 and UST v91, as the codec cuts a book a tC4 4.0
/// project holds with its alignment inline, and every verse of the plain fixtures and of the
/// drafts of the check-text and shown-text vectors.
#[test]
fn the_bank_is_the_export_for_every_verse_of_the_bundle_and_the_fixtures() {
    let mut books: Vec<(String, String)> = [
        "fixtures/tc4/sample-burrito/ingredients/TIT.usfm",
        "fixtures/tc4/sample-burrito/ingredients/JON.usfm",
        "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm",
        "fixtures/tc4/test-fixtures/usfm-corpus/exotic-partial-book.usfm",
        "fixtures/tc4/test-fixtures/usfm-corpus/sample-JON-span.usfm",
        "fixtures/tc4/test-fixtures/usfm-corpus/sample-TIT-draft.usfm",
        "fixtures/tc4/conformance-fixtures/import/usfm/32-JON.usfm",
        "fixtures/tc4/conformance-fixtures/import/usfm/57-TIT.usfm",
        "fixtures/tc4/conformance-fixtures/import/usfm/no-id.sfm",
        "fixtures/tc4/conformance-fixtures/import/damaged/57-TIT-unclosed.usfm",
    ]
    .iter()
    .map(|f| ((*f).to_owned(), common::read(f)))
    .collect();
    for file in [
        "fixtures/usfm/check-text-vectors.json",
        "fixtures/usfm/shown-text-vectors.json",
    ] {
        let vectors: serde_json::Value = serde_json::from_str(&common::read(file)).unwrap();
        for (draft, raw) in vectors["drafts"].as_object().unwrap() {
            books.push((format!("{file} {draft}"), raw.as_str().unwrap().to_owned()));
        }
    }
    for (archive, id) in [
        ("en_ult_v91.sb.zip", "en_ult"),
        ("en_ust_v91.sb.zip", "en_ust"),
    ] {
        let file = std::fs::File::open(common::fixture(&format!("bundle/en/{archive}"))).unwrap();
        let names: Vec<String> = zip::ZipArchive::new(file)
            .unwrap()
            .file_names()
            .filter(|n| n.starts_with(&format!("{id}/ingredients/")) && n.ends_with(".usfm"))
            .map(str::to_owned)
            .collect();
        for name in names {
            books.push((name.clone(), bundle_entry(archive, &name)));
        }
    }
    let mut wrong = Vec::new();
    let mut outcomes = [0usize; 3];
    for (name, raw) in &books {
        for (reference, content) in verses(raw) {
            match bank_against_export(&content) {
                Ok(outcome) => outcomes[outcome as usize] += 1,
                Err(e) => wrong.push(format!("{name} {reference}: {e}")),
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
    assert_eq!(books.len(), 10 + 19 + 2 * 66);
    // Woven, kept as written, glossary words.
    assert_eq!(outcomes, [48_109, 45, 1]);
}

/// A word, as a verse holds it: Latin, Greek, Hebrew with a word joiner, CJK, a number.
const WORDS: &[&str] = &[
    "a",
    "the",
    "Lord",
    "de",
    "siervo",
    "λόγος",
    "וַ\u{2060}יְהִי",
    "神",
    "7",
];

fn word() -> impl Strategy<Value = String> {
    prop::sample::select(WORDS).prop_map(str::to_owned)
}

/// One piece of a verse: words and the gaps between them, notes, cross references, figures
/// and the other markers whose text is not verse text, character spans (nested, holding a
/// note, before a letter), markers usfm-js does not know with and without attributes,
/// paragraph markers, headings with and without a paragraph after them, a glossary word, and
/// alignments of one to three nested `\zaln` milestones over `\w` words.
fn piece() -> impl Strategy<Value = String> {
    let gap =
        prop::sample::select(&[" ", " ", ", ", "\n", "", "—", ". "][..]).prop_map(str::to_owned);
    let fixed = prop::sample::select(
        &[
            "\\f + \\fr 1:1 \\ft o esclavo\\f*",
            "\\x - \\xo 1:1 \\xt Gen 1:1\\x*",
            "\\fig A picture|src=\"a.jpg\" size=\"col\"\\fig*",
            "\\fe + \\ft an end note\\fe*",
            "\\va 2\\va*",
            "\\vp 1b\\vp*",
            "\\ca 2\\ca*",
            "\\ts\\*",
            "\\rq Gen 1:1\\rq*",
            "\n\\p ",
            "\n\\q1 ",
            "\n\\q2\n",
            "\n\\li1 ",
            "\n\\m ",
            "\n\\qm1 ",
            "\n\\tr \\tc1 ",
            "\n\\pp ",
            "\\zpara ",
            "\n\\s A heading of words\n",
            "\n\\ms Major title\n",
            "\n\\r (Gen 1:1)\n",
            "\n\\s A heading \\zx with|x-y=\"z\"\\zx* more\n",
            "\n\\d A psalm of David\n",
            "\n\\sp The speaker\n",
            "\n\\qa Aleph\n",
            "\\w grace|strong=\"G5485\"\\w*",
        ][..],
    )
    .prop_map(str::to_owned);
    let attributes = prop::sample::select(
        &[
            "",
            "",
            "|x-y=\"z w\"",
            "|lemma=\"the Lord\" x-morph=\"a b\"",
            "|gloss",
        ][..],
    );
    let span = (
        word(),
        word(),
        attributes,
        prop::sample::select(
            &[
                "\\zx {a}{attr}\\zx*",
                "\\zx {a}{attr}",
                "\\add {a}{attr}\\add*",
                "\\nd {a}\\nd*s",
                "\\wj {a} \\+nd {b}\\+nd* {a}\\wj*",
                "\\wj {a}\\f + \\ft n\\f* {b}\\wj* {a}",
                "\\qs {a}\\qs*",
                "\\rb {a}|{b}\\rb*",
                "{a}\\zx*{b}",
            ][..],
        ),
    )
        .prop_map(|(a, b, attr, form)| {
            form.replace("{a}", &a)
                .replace("{b}", &b)
                .replace("{attr}", attr)
        });
    let aligned = (
        1usize..=3,
        prop::collection::vec((word(), prop::sample::select(&[" ", "\n", "\\f + \\ft n\\f*", ", "][..])), 1..4),
    )
        .prop_map(|(depth, words)| {
            let mut out = String::from(" ");
            for level in 0..depth {
                out.push_str(&format!(
                    "\\zaln-s |x-strong=\"G{level}\" x-lemma=\"l\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α{level}\"\\*"
                ));
            }
            for (n, (w, between)) in words.iter().enumerate() {
                if n > 0 {
                    out.push_str(between);
                }
                out.push_str(&format!("\\w {w}|x-occurrence=\"1\" x-occurrences=\"1\"\\w*"));
            }
            out.push_str(&"\\zaln-e\\*".repeat(depth));
            out.push(' ');
            out
        });
    prop_oneof![6 => word(), 6 => gap, 2 => fixed, 2 => span, 2 => aligned]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn the_bank_is_the_export_for_generated_verses(pieces in prop::collection::vec(piece(), 0..24)) {
        let content = pieces.concat();
        if let Err(e) = bank_against_export(&content) {
            prop_assert!(false, "{content:?}: {e}");
        }
    }
}
