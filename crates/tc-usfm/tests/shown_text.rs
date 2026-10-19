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

/// A marker the shown text leaves out right before a letter keeps the words on either side
/// apart, as the export splits them, though usfm-js keeps no character after its end marker:
/// a space is put back between two characters that would read as one word or number, and
/// nothing between a word and punctuation (a declared difference: tC4, PR #800 too, shows
/// "siervode", which the export cannot weave, #796). So does the start or end of a character
/// span or a paragraph marker: tC4 shows "Lords", "Yahwehque" and "神愛".
#[test]
fn a_skipped_marker_right_before_a_letter_leaves_a_space() {
    for (content, shown) in [
        ("the \\add Lord\\add*s house", "the Lord s house"),
        ("the \\nd Lord\\nd*s house", "the Lord s house"),
        ("x\\add y\\add* z", "x y z"),
        ("Yahweh\\q2 que", "Yahweh que"),
        ("\\rb 神|かみ\\rb*\\rb 愛|あい\\rb* x", "神 愛 x"),
        (
            "Pablo, siervo\\f + \\ft o esclavo\\f*de Dios.",
            "Pablo, siervo de Dios.",
        ),
        ("a\\fig x|src=\"y\"\\fig*b", "a b"),
        ("Je\\ts\\*sus", "Je sus"),
        ("año 12\\f + \\ft n\\f*3", "año 12 3"),
        ("siervo\\f + \\ft o\\f*, de", "siervo, de"),
        ("siervo,\\f + \\ft o\\f*de", "siervo,de"),
        ("siervo\\f + \\ft o\\f*\\x - \\xt R\\x*de", "siervo de"),
    ] {
        assert_eq!(shown_text_v1(content), shown, "{content}");
    }
}

/// Words of a character span after a span nested in it are shown, in file order (a declared
/// difference: tC4 shows only the outer span's text before the nested span, so it hides
/// "Lord your God" from Check and the Align bank while the export counts those words).
#[test]
fn words_after_a_nested_character_span_show() {
    for (content, shown) in [
        (
            "Jesus said, \\wj Worship the \\+nd Lord\\+nd* your God\\wj* and the end.",
            "Jesus said, Worship the Lord your God and the end.",
        ),
        (
            "\\wj a \\+nd b \\+add c\\+add* d\\+nd* e\\wj* f",
            "a b c d e f",
        ),
        ("x \\qs Selah \\+nd x\\+nd* y\\qs*", "x Selah x y"),
        ("the \\add a \\+nd b\\+nd* c\\add* d", "the a b c d"),
    ] {
        assert_eq!(shown_text_v1(content), shown, "{content}");
    }
}

/// The words after a character span that holds a note, a cross reference or a figure show,
/// nested or not, and so does the text after a stray end marker; the note's words stay hidden
/// (a declared difference: usfm-js keeps that text as the `content` of the span's end marker,
/// which tC4 hides from Check and the Align bank while the export counts it).
#[test]
fn words_after_a_span_that_holds_a_note_show() {
    for (content, shown) in [
        (
            "Jesus said, \\wj Blessed are the poor\\f + \\ft Or humble\\f* in spirit\\wj* and he went on to teach them.",
            "Jesus said, Blessed are the poor in spirit and he went on to teach them.",
        ),
        (
            "Jesus said, \\wj Truly I say\\f + \\ft Some manuscripts add: amen\\f* to you.\\wj* Then he left the house.",
            "Jesus said, Truly I say to you. Then he left the house.",
        ),
        ("\\wj a\\f + \\ft n\\f* b\\wj* c", "a b c"),
        ("\\add a\\f + \\ft n\\f*b\\add* c", "a b c"),
        ("\\wj a \\fig x|src=\"y\"\\fig* c\\wj* d e f", "a c d e f"),
        ("\\nd a\\x - \\xt R\\x* b\\nd* c", "a b c"),
        ("\\qt a \\f + \\ft n\\f* b\\qt* c", "a  b c"),
        (
            "\\add a \\+nd b\\f + \\ft n\\f* c\\+nd* d\\add* e",
            "a b c d e",
        ),
        ("\\wj a\\fig x|src=\"y\"\\fig*b\\wj*c", "a b c"),
        (
            "\\wj a\\f + \\fq b\\fq* c \\fq d\\fq* e\\f* g\\wj* h",
            "a g h",
        ),
        ("\\wj a\\f + \\ft b \\+wj c\\+wj* d\\f* e\\wj* f", "a e f"),
        ("\\wj a\\f + \\ft n\\wj* b", "a b"),
        ("a\\nd* b", "a b"),
    ] {
        assert_eq!(shown_text_v1(content), shown, "{content}");
    }
}

/// The verses on which the rebuild keeps words apart that tC4 (PR #800 included) joins, with
/// tC4's joined text and the rebuild's: a marker other than a note, a cross reference or a
/// figure between "siervo" and "de", and notes that usfm-js misreads inside an alignment run,
/// where it keeps no character after the note.
const SPACED: &[(&str, &str, &str)] = &[
    ("hidden-va 1:1", "siervode", "siervo de"),
    ("hidden-vp 1:1", "siervode", "siervo de"),
    ("hidden-ca 1:1", "siervode", "siervo de"),
    ("hidden-rq 1:1", "siervode", "siervo de"),
    ("hidden-cat 1:1", "siervode", "siervo de"),
    ("hidden-ts 1:1", "siervode", "siervo de"),
    ("aligned-notes 1:1", "onetwo", "one two"),
    (
        "bundle/en/en_ult_v91.sb.zip en_ult/ingredients/2SA.usfm 23:8",
        "spearagainst",
        "spear against",
    ),
];

/// Compares the shown text of every case of a vector file in the format of
/// `fixtures/usfm/shown-text-vectors.json` with tC4's, and the codec's verses of each book
/// with usfm-js's; returns how many cases held a text and how many an md5.
fn check_vectors(file: &serde_json::Value) -> (usize, usize) {
    use std::collections::{BTreeMap, BTreeSet};
    let mut by_book: BTreeMap<&str, Vec<&serde_json::Value>> = BTreeMap::new();
    for case in file["cases"].as_array().unwrap() {
        by_book
            .entry(case["book"].as_str().unwrap())
            .or_default()
            .push(case);
    }
    let key = |case: &serde_json::Value| {
        format!(
            "{}:{}",
            case["chapter"].as_str().unwrap(),
            case["verse"].as_str().unwrap()
        )
    };
    let mut wrong = Vec::new();
    let (mut texts, mut digests) = (0, 0);
    for (book, cases) in &by_book {
        let ours: BTreeMap<String, String> = verses(&vector_book(file, book)).into_iter().collect();
        let keys: BTreeSet<String> = cases.iter().map(|c| key(c)).collect();
        if keys != ours.keys().cloned().collect::<BTreeSet<_>>() {
            wrong.push(format!("{book}: the codec's verses are not usfm-js's"));
            continue;
        }
        for case in cases {
            let key = key(case);
            let shown = shown_text_v1(&ours[&key]);
            let spaced = SPACED
                .iter()
                .find(|(at, ..)| *at == format!("{book} {key}"));
            match (case["text"].as_str(), case["md5"].as_str()) {
                (Some(theirs), None) => {
                    texts += 1;
                    let expected = spaced.map_or_else(
                        || theirs.to_owned(),
                        |(_, joined, apart)| theirs.replacen(joined, apart, 1),
                    );
                    if shown != expected || spaced.is_some() && shown == theirs {
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
    (texts, digests)
}

/// What tC4 shows of every verse in `fixtures/usfm/shown-text-vectors.json`, made by
/// `tools/gen-shown-text-vectors.mjs` with tC4's own `verseText` at PR #800's head 5dcd147:
/// tC4's aligned fixtures, drafts with notes and other markers between words, and the md5 of
/// every verse of the ULT and UST v91 Genesis and Psalms. The codec finds the verses usfm-js
/// finds. The differences are the declared ones ([`SPACED`]). `main`, tC4 main's text where
/// it differs, holds exactly the drafts where PR #800's space is the only change.
#[test]
fn the_shown_text_is_what_tc4_reads() {
    use std::collections::BTreeSet;
    let file: serde_json::Value =
        serde_json::from_str(&read("fixtures/usfm/shown-text-vectors.json")).unwrap();
    assert_eq!(file["tc4"], "5dcd1472f9a3dec36bbc2f4e811c40d1387e7068");
    assert_eq!(file["main"], "9ad4d5c506466819e5b10f255bbb4a36d394e498");
    assert_eq!(check_vectors(&file), (160, 7983));
    let with_main: BTreeSet<&str> = file["cases"]
        .as_array()
        .unwrap()
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

/// The long parity run: every verse of every book of the ULT v91 and UST v91 (47,723 verses),
/// against a dump of tC4's text made by `tools/dump-shown-text.mjs` and named by
/// `TC_USFM_SHOWN_TEXT_DUMP` (about 12 MB, so not committed).
#[test]
#[ignore = "needs a dump of tC4's text; see the doc comment"]
fn the_shown_text_is_what_tc4_reads_on_a_dump() {
    let Some(path) = std::env::var_os("TC_USFM_SHOWN_TEXT_DUMP") else {
        return;
    };
    let file: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (texts, digests) = check_vectors(&file);
    eprintln!("{texts} verses as tC4 {} shows them", file["tc4"]);
    assert_eq!((texts, digests), (47_723, 0));
}

const ARCHIVES: &[(&str, &str)] = &[
    ("bundle/en/en_ult_v91.sb.zip", "en_ult"),
    ("bundle/en/en_ust_v91.sb.zip", "en_ust"),
];

/// The `.usfm` entries of a bundle archive.
fn bundle_books(archive: &str) -> Vec<String> {
    let file = std::fs::File::open(root().join(archive)).unwrap();
    let zip = zip::ZipArchive::new(file).unwrap();
    zip.file_names()
        .filter(|n| n.ends_with(".usfm"))
        .map(str::to_owned)
        .collect()
}

fn target_words(plain: &str) -> Option<Vec<(String, u32, u32)>> {
    let found = tc_usfm::target_words(plain).ok()?;
    Some(
        found
            .into_iter()
            .map(|w| (w.text, w.occurrence, w.occurrences))
            .collect(),
    )
}

/// Every verse of the ULT v91 and UST v91 Genesis and Psalms, with its alignment markup
/// inline as a tC4 4.0 book added from aligned USFM holds it, keeps every word:
///
/// 1. the `\w` words of the file appear in its shown words in order (tC4's `holdsInOrder`);
/// 2. the import's split reads it apart, and the words shown of the split plain text are the
///    same, so a verse the import keeps as written banks what its split form banks;
/// 3. the export's words of the split plain text (`target_words`) are those words too.
///
/// Before the fix, 633 + 236 ULT and 1,305 + 1,346 UST verses failed 1 or 2.
#[test]
fn every_genesis_and_psalms_verse_of_the_bundle_keeps_every_word() {
    let mut counts = Vec::new();
    let mut wrong = Vec::new();
    for (archive, id) in ARCHIVES {
        for book in ["GEN", "PSA"] {
            let raw = bundle_entry(archive, &format!("{id}/ingredients/{book}.usfm"));
            let all = verses(&raw);
            counts.push(all.len());
            for (reference, content) in &all {
                let at = format!("{id} {book} {reference}");
                let shown = words(&shown_text_v1(content));
                if !holds_in_order(&shown, &raw_words(content)) {
                    wrong.push(format!(
                        "{at}: loses \\w words: {:?}",
                        shown_text_v1(content)
                    ));
                }
                let Ok(split) = tc_usfm::parse_aligned(content) else {
                    wrong.push(format!("{at}: the split refuses it"));
                    continue;
                };
                let plain = words(&shown_text_v1(&split.plain));
                if plain != shown {
                    wrong.push(format!("{at}: split {plain:?}, as written {shown:?}"));
                }
                if target_words(&split.plain).as_ref() != Some(&plain) {
                    wrong.push(format!("{at}: the export splits it otherwise"));
                }
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
    assert_eq!(counts, [1533, 2461, 1533, 2456]);
}

/// The verses whose text tC4's `verseText` hides while the export counts it, which this
/// check therefore names (TC4-ISSUES §1, "Found by FIX-text"): the items of a `\li` list and
/// a line of a marker usfm-js does not know (`\qm1`), whose words are top-level siblings of
/// the marker in the aligned form but its `content` in the plain one.
const HIDDEN_BY_TC4: &[&str] = &[
    "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm 2:2",
    "check-text-vectors lists 1:1",
    "check-text-vectors lists 1:2",
    "en_ust EXO 15:21",
];

/// The tC4 #796 word list, tc-usfm's part: the words the aligner banks (the shown text's)
/// are the words the export weaves (`target_words`) for every verse with no word markup of
/// every plain fixture, every draft of the check-text and shown-text vectors, and the split
/// plain text of every verse of the ULT v91 and UST v91, but for [`HIDDEN_BY_TC4`].
#[test]
fn the_bank_and_the_export_split_every_plain_verse_into_the_same_words() {
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
    .map(|f| ((*f).to_owned(), read(f)))
    .collect();
    for (file, name) in [
        (
            "fixtures/usfm/check-text-vectors.json",
            "check-text-vectors",
        ),
        (
            "fixtures/usfm/shown-text-vectors.json",
            "shown-text-vectors",
        ),
    ] {
        let vectors: serde_json::Value = serde_json::from_str(&read(file)).unwrap();
        for (draft, raw) in vectors["drafts"].as_object().unwrap() {
            books.push((format!("{name} {draft}"), raw.as_str().unwrap().to_owned()));
        }
    }
    let mut differ = Vec::new();
    let (mut compared, mut marked_up) = (0, 0);
    for (name, raw) in &books {
        for (reference, content) in verses(raw) {
            let Some(export) = target_words(&content) else {
                marked_up += 1;
                continue;
            };
            compared += 1;
            if words(&shown_text_v1(&content)) != export {
                differ.push(format!("{name} {reference}"));
            }
        }
    }
    let (mut plains, mut refused) = (0, 0);
    for (archive, id) in ARCHIVES {
        for entry in bundle_books(archive) {
            let book = entry.rsplit('/').next().unwrap().trim_end_matches(".usfm");
            for (reference, content) in verses(&bundle_entry(archive, &entry)) {
                let Ok(split) = tc_usfm::parse_aligned(&content) else {
                    refused += 1;
                    continue;
                };
                plains += 1;
                let export = target_words(&split.plain);
                if export.is_none() || words(&shown_text_v1(&split.plain)) != export.unwrap() {
                    differ.push(format!("{id} {book} {reference}"));
                }
            }
        }
    }
    assert_eq!(differ, HIDDEN_BY_TC4);
    assert_eq!((compared, marked_up), (427, 5));
    assert_eq!((plains, refused), (47_680, 43));
}
