//! Behavior on the real files in `fixtures/`.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::links;
use std::path::{Path, PathBuf};
use tc_core::VerseSpan;
use tc_usfm::{Book, Document, Error, SegmentKind, TargetWord, parse_aligned, weave};

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read(relative: &str) -> String {
    let path = fixtures_dir().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn usfm_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            usfm_files(&path, found);
        } else if path
            .extension()
            .is_some_and(|e| e == "usfm" || e == "sfm" || e == "USFM" || e == "SFM")
        {
            found.push(path);
        }
    }
}

fn all_usfm() -> Vec<(PathBuf, String)> {
    let mut paths = Vec::new();
    usfm_files(&fixtures_dir(), &mut paths);
    paths.sort();
    assert!(paths.len() >= 20, "expected the tC4 USFM fixtures");
    paths
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

fn span(s: &str) -> VerseSpan {
    s.parse().unwrap()
}

#[test]
fn every_fixture_writes_back_byte_for_byte() {
    for (path, text) in all_usfm() {
        assert_eq!(Document::parse(&text).to_usfm(), text, "{}", path.display());
        let book = Book::parse(&text);
        let joined: String = book.segments().iter().map(|s| s.text()).collect();
        assert_eq!(joined, text, "{}", path.display());
    }
}

#[test]
fn rewriting_each_verse_with_its_own_body_changes_nothing() {
    let mut units = 0;
    for (path, text) in all_usfm() {
        let book = Book::parse(&text);
        for unit in book.verses() {
            let (Some(chapter), Some(verses)) = (unit.chapter(), unit.verses()) else {
                continue;
            };
            let same = book.replace_body(chapter, verses, unit.body());
            assert_eq!(
                same.as_deref(),
                Ok(text.as_str()),
                "{} {chapter}:{verses}",
                path.display()
            );
            units += 1;
        }
    }
    assert!(units > 500, "only {units} units checked");
}

#[test]
fn replacing_one_verse_keeps_every_other_byte() {
    let text = read("tc4/test-fixtures/en_ult/TIT.usfm");
    let book = Book::parse(&text);
    let old = book.verse(2, span("7")).unwrap();
    let new_text = book
        .replace_body(2, span("7"), "In all things show yourself an example.\n\n")
        .unwrap();
    let start = old.body().as_ptr() as usize - text.as_ptr() as usize;
    let end = start + old.body().len();
    assert_eq!(&new_text[..start], &text[..start]);
    assert_eq!(
        &new_text[new_text.len() - (text.len() - end)..],
        &text[end..]
    );
    let reparsed = Book::parse(&new_text);
    assert_eq!(
        reparsed.verse(2, span("7")).unwrap().display_text(),
        "In all things show yourself an example."
    );
    assert_eq!(reparsed.segments().len(), book.segments().len());
}

#[test]
fn aligned_verses_weave_back_byte_for_byte() {
    for file in ["en_ult/TIT.usfm", "en_ust/TIT.usfm"] {
        let text = read(&format!("tc4/test-fixtures/{file}"));
        let book = Book::parse(&text);
        let mut verses = 0;
        for unit in book.verses() {
            let aligned = parse_aligned(unit.body()).unwrap();
            assert!(!aligned.plain.contains("\\zaln") && !aligned.plain.contains("\\w "));
            let woven = weave(&aligned.plain, &aligned.groups);
            assert_eq!(
                woven.as_deref(),
                Ok(unit.body()),
                "{file} {:?}:{}",
                unit.chapter(),
                unit.label()
            );
            verses += 1;
        }
        assert_eq!(verses, 46, "{file}");
    }
}

/// Every verse of every fixture reads apart. Each one without other word markup (such as
/// UHB's `\w` words with `lemma`) weaves back into the same plain text and links, with
/// the occurrence numbers the plain text gives.
///
/// One verse is broken alignment markup on purpose: `align/span-source.usfm` wraps a word
/// in each span usfm-js knows, and its `zaln` span is a `\zaln-s` with only `x-content`.
/// That verse is refused, never read as alignment.
#[test]
fn every_fixture_verse_reads_apart_and_weaves_back() {
    let (mut woven, mut word_markup, mut refused) = (0, 0, 0);
    for (path, text) in all_usfm() {
        for unit in Book::parse(&text).verses() {
            let at = format!("{} {:?}:{}", path.display(), unit.chapter(), unit.label());
            let parsed = match parse_aligned(unit.body()) {
                Ok(parsed) => parsed,
                Err(Error::InvalidSourceWord(_))
                    if path.ends_with("align/span-source.usfm")
                        && unit.chapter() == Some(1)
                        && unit.label() == "96" =>
                {
                    refused += 1;
                    continue;
                }
                Err(e) => panic!("{at}: {e}"),
            };
            assert_eq!(tc_usfm::target_words(&parsed.plain), parsed.words, "{at}");
            match weave(&parsed.plain, &parsed.groups) {
                Ok(again) => {
                    let again = parse_aligned(&again).unwrap();
                    assert_eq!(
                        again.plain.replace('\n', " "),
                        parsed.plain.replace('\n', " "),
                        "{at}"
                    );
                    assert_eq!(links(&again.groups), links(&parsed.groups), "{at}");
                    assert_eq!(again.words, parsed.words, "{at}");
                    assert!(again.renumbered.is_empty(), "{at}");
                    woven += 1;
                }
                Err(Error::WordMarkup) => word_markup += 1,
                Err(e) => panic!("{at}: {e}"),
            }
        }
    }
    assert_eq!(
        refused, 1,
        "the broken \\zaln-s in align/span-source.usfm 1:96"
    );
    assert!(woven > 500, "only {woven} verses woven");
    // UHB Jonah's 48 verses, and the copies of it.
    assert!(word_markup >= 48, "{word_markup}");
}

/// The exotic corpus aligns `a servant` but leaves a later `a` bare, so the written
/// `x-occurrences="1"` is stale: the aligned `a` is the first of two.
#[test]
fn a_stale_occurrence_in_the_exotic_fixture_is_renumbered_and_weaves() {
    let text = read("tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm");
    let book = Book::parse(&text);
    let word = |text: &str, occurrence, occurrences| TargetWord {
        text: text.to_owned(),
        occurrence,
        occurrences,
    };
    let unit = book.verse(1, span("3")).unwrap();
    let parsed = parse_aligned(unit.body()).unwrap();
    assert_eq!(
        parsed.plain,
        "Paul a servant, with a footnote \\f + \\fr 1:3 \\ft An aligned verse can carry a footnote too. \\f* after the milestone.\n"
    );
    assert_eq!(parsed.renumbered, vec![(word("a", 1, 1), word("a", 1, 2))]);
    let servant: Vec<&TargetWord> = parsed.groups[1].targets.iter().collect();
    assert_eq!(servant, vec![&word("a", 1, 2), &word("servant", 1, 1)]);
    let woven = weave(&parsed.plain, &parsed.groups).unwrap();
    assert!(woven.contains("\\w a|x-occurrence=\"1\" x-occurrences=\"2\"\\w*"));
    assert!(woven.contains("\\w a|x-occurrence=\"2\" x-occurrences=\"2\"\\w*"));
    let again = parse_aligned(&woven).unwrap();
    assert_eq!(again.plain, parsed.plain);
    assert_eq!(again.groups, parsed.groups);
    assert!(again.renumbered.is_empty());
    // 1:2 holds a `\+w` with alignment attributes inside a footnote: note content, kept.
    let v2 = book.verse(1, span("2")).unwrap();
    let parsed = parse_aligned(v2.body()).unwrap();
    assert_eq!(parsed.plain, v2.body());
    assert!(parsed.groups.is_empty());
    assert_eq!(
        weave(&parsed.plain, &[]).unwrap().matches("\\+w").count(),
        2
    );
}

#[test]
fn titus_1_1_in_the_ult_takes_apart_as_expected() {
    let text = read("tc4/test-fixtures/en_ult/TIT.usfm");
    let book = Book::parse(&text);
    let unit = book.verse(1, span("1")).unwrap();
    let aligned = parse_aligned(unit.body()).unwrap();
    assert_eq!(
        aligned.plain,
        "Paul, a servant of God and an apostle of Jesus Christ, for the faith of the chosen people of God and knowledge of the truth that agrees with godliness,\n"
    );
    assert_eq!(aligned.words.len(), 29);
    let of: Vec<(u32, u32)> = aligned
        .words
        .iter()
        .filter(|w| w.text == "of")
        .map(|w| (w.occurrence, w.occurrences))
        .collect();
    assert_eq!(of, vec![(1, 5), (2, 5), (3, 5), (4, 5), (5, 5)]);
    // ἐκλεκτῶν#1 renders as "of" and, separately, as "the chosen people".
    let chosen: Vec<Vec<&str>> = aligned
        .groups
        .iter()
        .filter(|g| g.sources.iter().any(|s| s.content == "ἐκλεκτῶν"))
        .map(|g| g.targets.iter().map(|t| t.text.as_str()).collect())
        .collect();
    assert_eq!(chosen, vec![vec!["of"], vec!["the", "chosen", "people"]]);
    let paul = &aligned.groups[0];
    assert_eq!(paul.sources[0].strong.as_deref(), Some("G39720"));
    assert_eq!(paul.sources[0].lemma.as_deref(), Some("Παῦλος"));
    assert_eq!(paul.sources[0].morph.as_deref(), Some("Gr,N,,,,,NMS,"));
    // The plain text's own words agree with the \w words of the aligned verse.
    let plain_words: Vec<(String, u32, u32)> = tc_usfm::target_words(&aligned.plain)
        .into_iter()
        .map(|w| (w.text, w.occurrence, w.occurrences))
        .collect();
    let written: Vec<(String, u32, u32)> = aligned
        .words
        .iter()
        .map(|w| (w.text.clone(), w.occurrence, w.occurrences))
        .collect();
    assert_eq!(plain_words, written);
}

#[test]
fn nested_milestones_become_one_group_with_several_sources() {
    let text = read("tc4/test-fixtures/en_ust/TIT.usfm");
    let book = Book::parse(&text);
    let deepest = book
        .verses()
        .flat_map(|u| parse_aligned(u.body()).unwrap().groups)
        .map(|g| g.sources.len())
        .max();
    assert_eq!(deepest, Some(5));
}

#[test]
fn leading_markers_go_to_the_verse_they_introduce() {
    let text = read("tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm");
    let book = Book::parse(&text);
    let leading = |c: u16, v: &str| book.verse(c, span(v)).unwrap().leading();
    assert_eq!(leading(1, "1"), "\\s Prose with footnotes\n\\p\n");
    assert_eq!(leading(1, "3"), "");
    assert_eq!(leading(1, "4-5"), "\\s Poetry\n\\b\n\\q1\n");
    assert_eq!(
        leading(1, "6"),
        "\\b\n\\d A quiet title between the songs\n\\q1\n"
    );
    assert_eq!(leading(2, "1"), "\\s Lists and late prose\n\\p\n");
    assert_eq!(leading(2, "2"), "\\lh\n");
    assert_eq!(leading(2, "3"), "\\p\n");
    let v45 = book.verse(1, span("4-5")).unwrap();
    assert_eq!(
        v45.body(),
        "Sing the first line of the song,\n\\q2 and answer with the second line;\n\\q1 lift the third line higher still,\n\\q2 and let the fourth line rest. \\f + \\fr 1:4 \\ft The song is one span verse. \\f*\n"
    );
    assert_eq!(
        book.verse(2, span("2")).unwrap().body(),
        "The list has these members:\n\\li1 the first member,\n\\li2 a nested member,\n\\li1 the last member.\n\\lf The list ends here.\n"
    );
    let chapters: Vec<&str> = book
        .segments()
        .iter()
        .filter(|s| matches!(s.kind(), SegmentKind::Chapter(_)))
        .map(|s| s.text())
        .collect();
    assert_eq!(chapters, vec!["\\c 1\n", "\\c 2\n"]);
}

#[test]
fn chunk_markers_before_a_chapter_lead_into_the_chapter() {
    let text = read("tc4/test-fixtures/en_ult/TIT.usfm");
    let book = Book::parse(&text);
    let chapters: Vec<&str> = book
        .segments()
        .iter()
        .filter(|s| matches!(s.kind(), SegmentKind::Chapter(_)))
        .map(|s| s.text())
        .collect();
    assert_eq!(
        chapters,
        vec!["\\ts\\*\n\\c 1\n", "\\ts\\*\n\\c 2\n", "\\ts\\*\n\\c 3\n"]
    );
    assert_eq!(book.verse(1, span("4")).unwrap().leading(), "\\ts\\*\n");
    assert_eq!(book.verse(1, span("5")).unwrap().leading(), "\\p\n");
    assert_eq!(
        book.verse(1, span("10")).unwrap().leading(),
        "\\ts\\*\n\\p\n"
    );
    assert!(
        book.verse(1, span("3"))
            .unwrap()
            .body()
            .ends_with("\\zaln-e\\*.\n\n")
    );
    let header = &book.segments()[0];
    assert!(matches!(header.kind(), SegmentKind::Header));
    assert!(header.text().ends_with("\\mt1 Titus\n\n"));
}

#[test]
fn hebrew_alternate_chapter_number_leads_into_its_verse() {
    let text = read("tc4/test-fixtures/hbo_uhb/JON.usfm");
    let book = Book::parse(&text);
    let v17 = book.verse(1, span("17")).unwrap();
    assert_eq!(v17.leading(), "\\ca 2\\ca*\n");
    assert_eq!(v17.marker(), "\\v 17\n");
    assert!(v17.body().starts_with("\\va 1\\va*\n"));
    assert!(!v17.display_text().contains('1'));
    assert!(!v17.display_text().contains('2'));
    // The maqaf (U+05BE) joins two words; it is not part of either.
    let unit = book.verse(1, span("1")).unwrap();
    let v1: Vec<String> = tc_usfm::target_words(unit.body())
        .into_iter()
        .map(|w| w.text)
        .collect();
    assert_eq!(v1[0], "וַֽ\u{2060}יְהִי֙");
    assert!(v1.iter().all(|w| !w.contains('\u{05BE}')));
    assert!(
        unit.display_text()
            .contains(&format!("{}\u{05BE}{}", v1[1], v1[2]))
    );
    assert_eq!(v1.len(), 8);
    assert_eq!(book.verses().count(), 48);
}

#[test]
fn span_verses_are_found_only_by_their_exact_span() {
    let text = read("tc4/test-fixtures/sample-burrito/JON.usfm");
    let book = Book::parse(&text);
    let unit = book.verse(2, span("9-10")).unwrap();
    assert!(unit.display_text().starts_with("Mas yo te ofreceré"));
    assert_eq!(
        book.verse(2, span("9")),
        Err(Error::VerseNotFound {
            chapter: 2,
            verses: span("9")
        })
    );
    assert_eq!(book.verse(1, span("6")).unwrap().display_text(), "___");
}

#[test]
fn verse_text_leaves_out_notes_markup_and_headings() {
    let text = read("tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm");
    let book = Book::parse(&text);
    let plain = |c: u16, v: &str| book.verse(c, span(v)).unwrap().display_text();
    assert_eq!(
        plain(1, "1"),
        "The first verse carries a footnote inside its body."
    );
    assert_eq!(
        plain(1, "2"),
        "The second verse has a keyword footnote and continues after it."
    );
    assert_eq!(
        plain(1, "3"),
        "Paul a servant, with a footnote after the milestone."
    );
    assert_eq!(
        plain(1, "4-5"),
        "Sing the first line of the song, and answer with the second line; lift the third line higher still, and let the fourth line rest."
    );
    let ult = read("tc4/test-fixtures/en_ult/TIT.usfm");
    let ult = Book::parse(&ult);
    assert_eq!(
        ult.verse(1, span("1")).unwrap().display_text(),
        "Paul, a servant of God and an apostle of Jesus Christ, for the faith of the chosen people of God and knowledge of the truth that agrees with godliness,"
    );
}

/// tC4 stores `targetVerseMd5`, the md5 of a verse's plain text, with each alignment;
/// `plain_text_v1` of the verse's content hashes to the same value.
#[test]
fn verse_text_hashes_like_the_stored_alignment_records() {
    use md5::{Digest, Md5};
    let text = read("tc4/sample-burrito/ingredients/TIT.usfm");
    let book = Book::parse(&text);
    let records: serde_json::Value = serde_json::from_str(&read(
        "tc4/sample-burrito/ingredients/checking/alignments/TIT.json",
    ))
    .unwrap();
    let mut checked = 0;
    for (chapter, verses) in records["chapters"].as_object().unwrap() {
        for (verse, record) in verses.as_object().unwrap() {
            let unit = book.verse(chapter.parse().unwrap(), span(verse)).unwrap();
            let digest: String = Md5::digest(tc_usfm::plain_text_v1(unit.body()).as_bytes())
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            assert_eq!(digest, record["targetVerseMd5"].as_str().unwrap());
            checked += 1;
        }
    }
    assert_eq!(checked, 1);
}

/// tC4 hashes the usfm-js top-level text of a verse slot, trimmed (`verseTextMd5`). Our
/// verse text agrees on every verse of the plain drafts and of UHB Jonah; the four verses
/// where it differs are each a case the text module documents.
#[test]
fn verse_text_matches_what_tc4_hashes() {
    let vectors: serde_json::Value =
        serde_json::from_str(&read("usfm/verse-text-vectors.json")).unwrap();
    let mut same = 0;
    let mut documented = Vec::new();
    for case in vectors["cases"].as_array().unwrap() {
        let file = case["book"].as_str().unwrap();
        let text =
            std::fs::read_to_string(fixtures_dir().join(file.trim_start_matches("fixtures/")))
                .unwrap();
        let book = Book::parse(&text);
        let chapter: u16 = case["chapter"].as_str().unwrap().parse().unwrap();
        let verse = case["verse"].as_str().unwrap();
        let ours = book.verse(chapter, span(verse)).unwrap().display_text();
        let tc4 = case["text"].as_str().unwrap();
        if ours == tc4 {
            same += 1;
            continue;
        }
        let name = Path::new(file).file_name().unwrap().to_str().unwrap();
        let key = format!("{name} {chapter}:{verse}");
        match key.as_str() {
            // usfm-js leaves the `\w` words inside `\zaln` out of the top-level text.
            "exotic-poetry-footnotes.usfm 1:3" => {
                assert_eq!(tc4, ", with a footnote after the milestone.");
            }
            // tC4 keeps poetry line breaks, and its slot runs on into the next heading.
            "exotic-poetry-footnotes.usfm 1:4-5" => {
                let tc4 = tc_usfm::text::normalize_whitespace(tc4);
                assert_eq!(tc4, format!("{ours} A quiet title between the songs"));
            }
            "exotic-partial-book.usfm 3:5" => {
                assert_eq!(tc_usfm::text::normalize_whitespace(tc4), ours);
            }
            // usfm-js leaves the text of `\li` list items out.
            "exotic-poetry-footnotes.usfm 2:2" => {
                assert_eq!(tc4, "The list has these members:\nThe list ends here.");
            }
            other => panic!("{other}: ours {ours:?}, tC4 {tc4:?}"),
        }
        documented.push(key);
    }
    assert_eq!(same, 239);
    assert_eq!(documented.len(), 4);
}

#[test]
fn a_partial_book_without_a_final_line_break_is_kept() {
    let text = read("tc4/test-fixtures/usfm-corpus/exotic-partial-book.usfm");
    let book = Book::parse(&text);
    let keys: Vec<String> = book
        .verses()
        .map(|u| format!("{}:{}", u.chapter().unwrap(), u.label()))
        .collect();
    assert_eq!(keys, vec!["3:1", "3:2", "3:3-4", "3:5"]);
    let last = book.verse(3, span("5")).unwrap();
    assert_eq!(
        last.body(),
        "Poetry closes the partial book,\n\\q2 and the file ends without a final line terminator."
    );
    let edited = book.replace_body(3, span("5"), "Fin.").unwrap();
    assert!(edited.ends_with("\\q1\n\\v 5 Fin."));
}

#[test]
fn byte_order_marks_and_crlf_survive_edits() {
    let text = "\u{FEFF}\\id TIT\r\n\\c 1\r\n\\p\r\n\\v 1 One.\r\n\\v 2 Two.\r\n";
    let book = Book::parse(text);
    assert_eq!(book.segments()[0].text(), "\u{FEFF}\\id TIT\r\n");
    let edited = book.replace_body(1, span("1"), "Uno.\r\n").unwrap();
    assert_eq!(
        edited,
        "\u{FEFF}\\id TIT\r\n\\c 1\r\n\\p\r\n\\v 1 Uno.\r\n\\v 2 Two.\r\n"
    );
}

#[test]
fn tokenizer_matches_the_reference_library() {
    let vectors: serde_json::Value =
        serde_json::from_str(&read("usfm/tokenizer-vectors.json")).unwrap();
    let cases = vectors["cases"].as_array().unwrap();
    // 195 real verses, 23 hand-picked edge cases and 500 random strings.
    assert_eq!(cases.len(), 718);
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let expected: Vec<(String, String)> = case["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                (
                    t[0].as_str().unwrap().to_owned(),
                    t[1].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        let actual: Vec<(String, String)> = tc_usfm::words::classify(text)
            .into_iter()
            .map(|t| {
                let kind = match t.kind {
                    tc_usfm::words::TokenKind::Word => "word",
                    tc_usfm::words::TokenKind::Number => "number",
                    tc_usfm::words::TokenKind::Punctuation => "punctuation",
                    tc_usfm::words::TokenKind::Whitespace => "whitespace",
                    tc_usfm::words::TokenKind::Other => "unknown",
                };
                (kind.to_owned(), t.text.to_owned())
            })
            .collect();
        assert_eq!(actual, expected, "{}: {text:?}", case["source"]);
    }
}
