//! The shown text ([`tc_usfm::shown_text_v1`]): what tC4's Translate, Check and Align screens
//! show of a verse, and the words the Check screen selects in and the aligner banks. tC4 #782
//! (words inside nested alignments) is ported from tC4 9ad4d5c's
//! `test/bookModel-nested-alignment.test.ts`, on its own fixtures.
//!
//! The #782 cases read a verse with its alignment markup inline, as a tC4 4.0 book added from
//! aligned USFM stores it and as the import keeps a verse it cannot split: on the split plain
//! text nothing is nested, so those cases would pass before the fix and prove nothing.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use tc_usfm::{Document, Node, codec, shown_text_v1};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Every verse of a book as the codec cuts it: `chapter:verse` and its content (`words +
/// layout`, the USFM after `\v N `).
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

/// The content of one verse of a fixture book.
fn verse(rel: &str, reference: &str) -> String {
    verses(&read(rel))
        .into_iter()
        .find(|(r, _)| r == reference)
        .unwrap_or_else(|| panic!("{rel} has no {reference}"))
        .1
}

const TC4: &str = "fixtures/tc4/test-fixtures";

/// The words of `text` with their occurrences, as the aligner banks them.
fn words(text: &str) -> Vec<(String, u32, u32)> {
    tc_usfm::words::words(text)
        .into_iter()
        .map(|w| (w.text.to_owned(), w.occurrence, w.occurrences))
        .collect()
}

/// Notes, cross references and figures: their words are not verse text.
const NOTES: &[&str] = &["f", "fe", "ef", "x", "ex", "fig"];

/// The `\w` words of a verse's content in file order, read from the USFM tree without
/// usfm-js (tC4's `rawWords`), notes left out, each split into words.
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

/// True when `raw` appears in `shown` in order, each word taken once (tC4's `holdsInOrder`).
fn holds_in_order(shown: &[(String, u32, u32)], raw: &[String]) -> bool {
    let mut rest = shown.iter();
    raw.iter().all(|w| rest.any(|(s, _, _)| s == w))
}

/// tC4's way 1: Genesis 1:1 of the en_ult aligns "the heavens" to one Hebrew word inside
/// another `\zaln`, two levels down; the one-level walk read "created  and the earth.".
#[test]
fn shown_text_reads_words_inside_nested_alignments() {
    let sentence = "In the beginning God created the heavens and the earth.";
    let content = verse(&format!("{TC4}/en_ult/GEN-1-1.usfm"), "1:1");
    assert!(content.contains("\\zaln-s"));
    assert_eq!(shown_text_v1(&content), sentence);
    let found = words(&shown_text_v1(&content));
    assert_eq!(found.len(), 10);
    let the: Vec<_> = found.iter().filter(|w| w.0 == "the").collect();
    assert_eq!(
        the.iter().map(|w| (w.1, w.2)).collect::<Vec<_>>(),
        [(1, 3), (2, 3), (3, 3)]
    );
    assert!(found.contains(&("heavens".to_owned(), 1, 1)));
    // tC4's "Translate reads the same text" case, on the chapter.
    let chapter = format!("{TC4}/en_ult/GEN-1.usfm");
    assert_eq!(shown_text_v1(&verse(&chapter, "1:1")), sentence);
    assert_eq!(
        shown_text_v1(&verse(&chapter, "1:4")),
        "And God saw the light, that {it was} good. Then God separated between the light and the darkness."
    );
}

/// tC4's ways 1 and 4: every verse of the en_ult Genesis 1 holds its `\w` words in file
/// order, each once.
#[test]
fn every_verse_of_genesis_1_holds_its_words_in_file_order() {
    let all = verses(&read(&format!("{TC4}/en_ult/GEN-1.usfm")));
    assert_eq!(all.len(), 31);
    let mut wrong = Vec::new();
    for (reference, content) in &all {
        let shown = words(&shown_text_v1(content));
        let raw = raw_words(content);
        assert!(!raw.is_empty(), "{reference}");
        // Every word of the en_ult is a `\w` word, so the counts say no word came twice.
        if !holds_in_order(&shown, &raw) || shown.len() != raw.len() {
            wrong.push(format!("{reference}: {:?}", shown_text_v1(content)));
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// tC4's way 1 on Psalm 3:2, whose aligned "Selah" the en_ult wraps in `\qs … \qs*`.
#[test]
fn selah_inside_qs_shows() {
    let content = verse(&format!("{TC4}/en_ult/PSA-3.usfm"), "3:2");
    assert!(content.contains("\\qs\n\\zaln-s"));
    assert_eq!(
        shown_text_v1(&content),
        "{There are} many sayers about me,\n“There is no salvation for him in God.”\nSelah"
    );
}

/// tC4's way 2: walking deeper does not bring footnote text or a section heading into the
/// verse.
#[test]
fn footnotes_and_headings_stay_out_of_the_shown_text() {
    let book = read(&format!("{TC4}/usfm-corpus/exotic-poetry-footnotes.usfm"));
    let all = verses(&book);
    for reference in ["1:1", "1:2", "1:3", "1:4-5"] {
        let (_, content) = all.iter().find(|(r, _)| r == reference).unwrap();
        let shown = shown_text_v1(content);
        for hidden in [
            "manuscripts",
            "explained",
            "carry a footnote",
            "one span verse",
            "Poetry",
        ] {
            assert!(!shown.contains(hidden), "{reference}: {shown:?}");
        }
    }
}

/// The book a vector case names: a draft of the vector file, an entry of a bundle archive
/// (`"<archive> <entry>"`), or a file under the repository root.
fn vector_book(file: &serde_json::Value, book: &str) -> String {
    if let Some(draft) = file["drafts"].get(book) {
        return draft.as_str().unwrap().to_owned();
    }
    if let Some((archive, entry)) = book.split_once(' ') {
        return bundle_entry(archive, entry);
    }
    read(book)
}

fn bundle_entry(archive: &str, entry: &str) -> String {
    use std::io::Read;
    let file = std::fs::File::open(root().join(archive)).unwrap();
    let mut zip = zip::ZipArchive::new(file).unwrap();
    let mut text = String::new();
    zip.by_name(entry)
        .unwrap_or_else(|e| panic!("{archive} {entry}: {e}"))
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn md5_hex(text: &str) -> String {
    use md5::{Digest, Md5};
    Md5::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// tC4 PR #800's last case: what Translate and Check show for a verse whose footnote follows
/// a word.
#[test]
fn a_footnote_between_words_keeps_its_space() {
    assert_eq!(
        shown_text_v1("Pablo, siervo\\f + \\ft o esclavo\\f* de Dios."),
        "Pablo, siervo de Dios."
    );
    assert_eq!(
        shown_text_v1("Pablo, siervo\\f + \\fr 1:1 \\ft o esclavo\\f* de Dios.\n"),
        "Pablo, siervo de Dios."
    );
}

/// tC4 PR #800's cases on the sample Titus 1:1: a footnote, a cross reference, an extended
/// note or a figure right after a word keeps the space after it; a note after a space, two
/// notes, a note before punctuation and a note at either end of the verse add nothing.
#[test]
fn a_note_of_any_kind_between_words_keeps_its_space() {
    let content = verse("fixtures/tc4/sample-burrito/ingredients/TIT.usfm", "1:1");
    let plain = shown_text_v1(&content);
    assert!(plain.starts_with("Pablo, siervo de Dios y apóstol"));
    let with = |find: &str, replace: &str| {
        assert!(content.contains(find), "{find}");
        shown_text_v1(&content.replacen(find, replace, 1))
    };
    for note in [
        "siervo\\f + \\fr 1:1 \\ft o esclavo\\f* de",
        "siervo\\x - \\xt Rom 1:1\\x* de",
        "siervo\\fe + \\ft o esclavo\\fe* de",
        "siervo\\ef + \\ft o esclavo\\ef* de",
        "siervo\\fig a|src=\"b\"\\fig* de",
        "siervo \\f + \\ft o esclavo\\f* de",
        "siervo\\f + \\ft a\\f*\\f + \\ft b\\f* de",
        "siervo\\f + \\ft a\\f* \\f + \\ft b\\f* de",
    ] {
        assert_eq!(with("siervo de", note), plain, "{note}");
    }
    assert_eq!(
        with("siervo de", "siervo\\f + \\ft a\\f*, de"),
        plain.replacen("siervo de", "siervo, de", 1)
    );
    assert_eq!(with("Pablo", "\\f + \\ft a\\f* Pablo"), plain);
    assert_eq!(with("piedad,", "piedad,\\f + \\ft a\\f*"), plain);
}

/// The rebuild's rule (a declared difference): every marker the shown text leaves out keeps
/// the space after it when it stands between two words, where tC4's PR #800 joins them for
/// all but notes, cross references and figures. A no-break space after a note is already a
/// space, so nothing is added.
#[test]
fn any_skipped_marker_between_words_keeps_its_space() {
    for marker in [
        "\\va 2\\va*",
        "\\vp 1b\\vp*",
        "\\ca 3\\ca*",
        "\\rq Rom 1:1\\rq*",
        "\\cat x\\cat*",
        "\\ndx x\\ndx*",
        "\\pro x\\pro*",
        "\\xt x\\xt*",
        "\\fdc x\\fdc*",
        "\\ts\\*",
    ] {
        assert_eq!(
            shown_text_v1(&format!("siervo{marker} de Dios")),
            "siervo de Dios",
            "{marker}"
        );
    }
    assert_eq!(shown_text_v1("a\\f + \\ft n\\f*\u{a0}b"), "a\u{a0}b");
}

/// The drafts on which the rebuild keeps a space that tC4 (PR #800 included) drops: a marker
/// other than a note, a cross reference or a figure between "siervo" and "de".
const HIDDEN: &[&str] = &[
    "hidden-va",
    "hidden-vp",
    "hidden-ca",
    "hidden-rq",
    "hidden-cat",
    "hidden-ts",
];

/// What tC4 shows of every verse in `fixtures/usfm/shown-text-vectors.json`, made by
/// `tools/gen-shown-text-vectors.mjs` with tC4's own `verseText` at PR #800's head 5dcd147:
/// tC4's aligned fixtures, drafts with notes and other markers between words, and the md5 of
/// every verse of the ULT and UST v91 Genesis and Psalms. The codec finds the verses usfm-js
/// finds. The one difference is the declared one ([`HIDDEN`]). `main`, tC4 main's text where
/// it differs, holds exactly the drafts where PR #800's space is the only change.
#[test]
fn the_shown_text_is_what_tc4_reads() {
    use std::collections::{BTreeMap, BTreeSet};
    let file: serde_json::Value =
        serde_json::from_str(&read("fixtures/usfm/shown-text-vectors.json")).unwrap();
    assert_eq!(file["tc4"], "5dcd1472f9a3dec36bbc2f4e811c40d1387e7068");
    assert_eq!(file["main"], "9ad4d5c506466819e5b10f255bbb4a36d394e498");
    let cases = file["cases"].as_array().unwrap();
    let mut by_book: BTreeMap<&str, Vec<&serde_json::Value>> = BTreeMap::new();
    for case in cases {
        by_book
            .entry(case["book"].as_str().unwrap())
            .or_default()
            .push(case);
    }
    let mut wrong = Vec::new();
    let (mut texts, mut digests) = (0, 0);
    for (book, cases) in &by_book {
        let ours: BTreeMap<String, String> =
            verses(&vector_book(&file, book)).into_iter().collect();
        let keys: BTreeSet<String> = cases
            .iter()
            .map(|c| {
                format!(
                    "{}:{}",
                    c["chapter"].as_str().unwrap(),
                    c["verse"].as_str().unwrap()
                )
            })
            .collect();
        if keys != ours.keys().cloned().collect::<BTreeSet<_>>() {
            wrong.push(format!("{book}: the codec's verses are not usfm-js's"));
            continue;
        }
        for case in cases {
            let key = format!(
                "{}:{}",
                case["chapter"].as_str().unwrap(),
                case["verse"].as_str().unwrap()
            );
            let shown = shown_text_v1(&ours[&key]);
            match (case["text"].as_str(), case["md5"].as_str()) {
                (Some(theirs), None) if HIDDEN.contains(book) => {
                    texts += 1;
                    if shown == theirs || shown != theirs.replacen("siervode", "siervo de", 1) {
                        wrong.push(format!("{book} {key}: {shown:?} against tC4's {theirs:?}"));
                    }
                }
                (Some(theirs), None) => {
                    texts += 1;
                    if shown != theirs {
                        wrong.push(format!("{book} {key}: {shown:?} against tC4's {theirs:?}"));
                    }
                }
                (None, Some(theirs)) => {
                    digests += 1;
                    if md5_hex(&shown) != theirs {
                        wrong.push(format!("{book} {key}: {shown:?}"));
                    }
                }
                _ => panic!("{book} {key}: a case has a text or an md5"),
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
    assert_eq!((texts, digests), (160, 7983));
    let with_main: BTreeSet<&str> = cases
        .iter()
        .filter(|c| c.get("main").is_some())
        .map(|c| c["book"].as_str().unwrap())
        .collect();
    assert_eq!(
        with_main,
        BTreeSet::from([
            "aligned-notes",
            "note-astral",
            "note-ef",
            "note-f",
            "note-fe",
            "note-fig",
            "note-line",
            "note-newline",
            "note-poetry",
            "note-rtl",
            "note-x",
            "two-notes",
        ])
    );
}
