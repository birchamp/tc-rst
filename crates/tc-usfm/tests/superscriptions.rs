//! Verses with a Psalm superscription (`\v 1 \d …`) and the other headings usfm-js shows as
//! verse text (`\sp`, `\qa`, `\lit`), against what tC4 reads from them and writes for them:
//! `fixtures/usfm/superscription-vectors.json`, made with tC4's own code by
//! `crates/tc-usfm/tools/gen-superscription-vectors.mjs`. The published cases are every
//! verse of the ULT v91 and UST v91 Psalms that starts with `\d`; the scale Psalms are the
//! same bytes as the ULT's. The heading cases show where tC4's export can write an aligned
//! word in a heading back, and where it cannot. The acrostic cases show how a bare `\qa`
//! letter at the end of a verse in tC4's cut is woven.

// Helpers outside `#[test]` functions read fixtures and may fail loudly.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use tc_usfm::{
    AlignedVerse, AlignmentGroup, Book, Error, SourceWord, TargetWord, display_text, parse_aligned,
    target_words, weave,
};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vectors(key: &str) -> Vec<Value> {
    let path = repo().join("fixtures/usfm/superscription-vectors.json");
    let vectors: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    vectors[key].as_array().unwrap().clone()
}

fn cases() -> Vec<Value> {
    vectors("cases")
}

fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} in {value}"))
}

fn count(value: &Value) -> u32 {
    u32::try_from(value.as_u64().unwrap()).unwrap()
}

/// A book a case names as `<archive> <entry>`, read from `bundle/en/`.
fn published(source: &str) -> String {
    let (archive, entry) = source.split_once(' ').unwrap();
    let file = std::fs::File::open(repo().join(archive)).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let mut text = String::new();
    archive
        .by_name(entry)
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn words(list: &Value) -> Vec<TargetWord> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|w| TargetWord {
            text: w[0].as_str().unwrap().to_owned(),
            occurrence: count(&w[1]),
            occurrences: count(&w[2]),
        })
        .collect()
}

/// The words of the text the Check screen shows and the aligner splits
/// ([`tc_usfm::shown_text_v1`]).
fn shown_words(plain: &str) -> Vec<TargetWord> {
    tc_usfm::words::words(&tc_usfm::shown_text_v1(plain))
        .into_iter()
        .map(|w| TargetWord {
            text: w.text.to_owned(),
            occurrence: w.occurrence,
            occurrences: w.occurrences,
        })
        .collect()
}

/// Each word of the verse in order, with the original-language words (content and
/// occurrence) of the group it is in: the form of a case's `aligned`.
fn aligned_words(parsed: &AlignedVerse) -> Vec<(String, Vec<(String, u32)>)> {
    let mut sources: HashMap<(&str, u32), Vec<(String, u32)>> = HashMap::new();
    for group in &parsed.groups {
        for target in &group.targets {
            let chain = group
                .sources
                .iter()
                .map(|s| (s.content.clone(), s.occurrence))
                .collect();
            sources.insert((target.text.as_str(), target.occurrence), chain);
        }
    }
    parsed
        .words
        .iter()
        .map(|w| {
            let chain = sources.remove(&(w.text.as_str(), w.occurrence));
            (w.text.clone(), chain.unwrap_or_default())
        })
        .collect()
}

fn expected_aligned(list: &Value) -> Vec<(String, Vec<(String, u32)>)> {
    list.as_array()
        .unwrap()
        .iter()
        .map(|w| {
            let chain = w[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s[0].as_str().unwrap().to_owned(), count(&s[1])))
                .collect();
            (w[0].as_str().unwrap().to_owned(), chain)
        })
        .collect()
}

/// One §5.1 alignment of a draft case as a group: its top words are the sources.
fn group(alignment: &Value) -> AlignmentGroup {
    let optional = |w: &Value, key: &str| w[key].as_str().map(str::to_owned);
    AlignmentGroup {
        sources: alignment["topWords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| SourceWord {
                content: str_of(w, "word").to_owned(),
                strong: optional(w, "strong"),
                lemma: optional(w, "lemma"),
                morph: optional(w, "morph"),
                occurrence: count(&w["occurrence"]),
                occurrences: count(&w["occurrences"]),
                extra: Vec::new(),
            })
            .collect(),
        targets: alignment["bottomWords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| TargetWord {
                text: str_of(w, "word").to_owned(),
                occurrence: count(&w["occurrence"]),
                occurrences: count(&w["occurrences"]),
            })
            .collect(),
    }
}

/// The text and words of each verse are the ones tC4 shows and aligns, with the words of
/// the superscription numbered together with the rest of the verse. A published verse
/// reads apart into the alignment tC4 reads from it and weaves back into the same bytes.
#[test]
fn verses_with_a_superscription_read_as_tc4_reads_them() {
    let mut books: HashMap<String, String> = HashMap::new();
    let (mut published_cases, mut drafts) = (0, 0);
    for case in cases() {
        let source = str_of(&case, "source");
        let text = match case["usfm"].as_str() {
            Some(usfm) => usfm.to_owned(),
            None => books
                .entry(source.to_owned())
                .or_insert_with(|| published(source))
                .clone(),
        };
        let book = Book::parse(&text);
        let chapter: u16 = str_of(&case, "chapter").parse().unwrap();
        let verses = str_of(&case, "verse").parse().unwrap();
        let at = format!("{source} {chapter}:{verses}");
        let body = book.verse(chapter, verses).unwrap().body();
        let parsed = parse_aligned(body).unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!(display_text(body), str_of(&case, "text"), "{at}");
        assert_eq!(display_text(&parsed.plain), str_of(&case, "text"), "{at}");
        assert_eq!(parsed.words, words(&case["words"]), "{at}");
        assert_eq!(target_words(&parsed.plain), parsed.words, "{at}");
        // The Check screen and the aligner show these words too.
        assert_eq!(shown_words(&parsed.plain), parsed.words, "{at}");
        if let Some(aligned) = case.get("aligned") {
            assert_eq!(aligned_words(&parsed), expected_aligned(aligned), "{at}");
            assert!(parsed.renumbered.is_empty(), "{at}");
            assert_eq!(
                weave(&parsed.plain, &parsed.groups).as_deref(),
                Ok(body),
                "{at}"
            );
            published_cases += 1;
        } else {
            let groups: Vec<AlignmentGroup> = case["alignments"]
                .as_array()
                .unwrap()
                .iter()
                .map(group)
                .collect();
            assert_eq!(
                weave(body, &groups).as_deref(),
                Ok(str_of(&case, "woven")),
                "{at}"
            );
            drafts += 1;
        }
    }
    assert_eq!((published_cases, drafts), (102, 8));
}

/// An aligned word in a heading inside a verse is read where tC4's export writes the verse
/// back: in `\d`, `\sp`, `\qa` and `\lit`, whose text usfm-js shows as verse text. In every
/// other heading (`\s`, `\ms`, `\r`, `\d1`…) tC4's export cannot merge the alignment and
/// writes the verse without it, and tc-usfm refuses the verse rather than misplace it.
#[test]
fn an_aligned_heading_is_read_where_tc4_writes_it_back() {
    let (mut read, mut refused) = (Vec::new(), Vec::new());
    for case in vectors("headings") {
        let marker = str_of(&case, "marker").to_owned();
        let content = str_of(&case, "content");
        match str_of(&case, "exported") {
            "same" => {
                let parsed = parse_aligned(content).unwrap_or_else(|e| panic!("\\{marker}: {e}"));
                assert_eq!(parsed.groups.len(), 3, "\\{marker}");
                assert_eq!(
                    weave(&parsed.plain, &parsed.groups).as_deref(),
                    Ok(content),
                    "\\{marker}"
                );
                read.push(marker);
            }
            "unaligned" => {
                assert!(
                    matches!(
                        parse_aligned(content),
                        Err(Error::AlignmentOutsideVerseText(_))
                    ),
                    "\\{marker}"
                );
                refused.push(marker);
            }
            other => panic!("\\{marker}: {other}"),
        }
    }
    assert_eq!(read, ["d", "sp", "qa", "lit"]);
    assert_eq!(refused.len(), 13, "{refused:?}");
}

/// Verse `chapter:verse` of a book as tC4 cuts it to align and export it (`decompose` in
/// tC4's `journal/skeleton.mjs`): from after `\v N ` to the next `\v ` or `\c `.
fn tc4_slot<'a>(book: &'a str, chapter: &str, verse: &str) -> &'a str {
    let chapter_at = book.find(&format!("\\c {chapter}\n")).unwrap();
    let marker = format!("\\v {verse} ");
    let start = chapter_at + book[chapter_at..].find(&marker).unwrap() + marker.len();
    let end = ["\\v ", "\\c "]
        .into_iter()
        .filter_map(|next| book[start..].find(next))
        .min()
        .map_or(book.len(), |end| start + end);
    &book[start..end]
}

/// In tC4's cut, LAM 1:1 ends with the acrostic letter of verse 2 (`\qa BETH` before
/// `\v 2`), which the ULT and UST write bare. usfm-js shows it as the verse's text, so it is
/// the verse's last word. weave writes it as a `\w` word,
/// `\qa \w BETH|x-occurrence="1" x-occurrences="1"\w*`, the form the ULT and UST already
/// give some of their letters, and writes the whole verse as tC4's export writes it once
/// the verse's record banks the letter. tC4's export of the published verse cannot merge
/// the record it reads, which leaves the bare letter out, and writes the verse without its
/// alignment. In the book cut the letter leads into verse 2 and is no word of verse 1.
#[test]
fn an_acrostic_letter_that_ends_a_verse_is_woven_as_tc4_weaves_it() {
    let acrostics = vectors("acrostics");
    for case in &acrostics {
        let source = str_of(case, "source");
        let text = published(source);
        let (chapter, verse) = (str_of(case, "chapter"), str_of(case, "verse"));
        let at = format!("{source} {chapter}:{verse}");
        let slot = tc4_slot(&text, chapter, verse);
        assert!(slot.ends_with("\n\\qa BETH\n\\q1 "), "{at}");
        let parsed = parse_aligned(slot).unwrap_or_else(|e| panic!("{at}: {e}"));
        assert_eq!(display_text(slot), str_of(case, "text"), "{at}");
        assert_eq!(parsed.words, words(&case["words"]), "{at}");
        assert_eq!(shown_words(&parsed.plain), parsed.words, "{at}");
        let held: HashSet<&TargetWord> = parsed.groups.iter().flat_map(|g| &g.targets).collect();
        let bank: Vec<TargetWord> = parsed
            .words
            .iter()
            .filter(|w| !held.contains(w))
            .cloned()
            .collect();
        assert_eq!(bank, words(&case["bank"]), "{at}");

        let woven = weave(&parsed.plain, &parsed.groups).unwrap();
        let wrapped = "\n\\qa \\w BETH|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\\q1 ";
        assert!(woven.ends_with(wrapped), "{at}");
        // tC4's export also writes one more space after the paragraph marker that ends the
        // verse (`\q1  `), which the published verses do not have.
        assert_eq!(format!("{woven} "), str_of(case, "woven"), "{at}");

        let book = Book::parse(&text);
        let unit = book
            .verse(chapter.parse().unwrap(), verse.parse().unwrap())
            .unwrap();
        assert!(!unit.body().contains("\\qa"), "{at}");
        let in_book = parse_aligned(unit.body()).unwrap();
        assert_eq!(
            in_book.words,
            parsed.words[..parsed.words.len() - 1],
            "{at}"
        );
    }
    assert_eq!(acrostics.len(), 2);
}

/// The scale Psalms hold the same superscriptions as the ULT v91 Psalms, byte for byte.
#[test]
fn the_scale_psalms_are_the_ult_v91_psalms() {
    let scale = std::fs::read_to_string(repo().join("fixtures/scale/psa/en_ult/19-PSA.usfm"));
    let ult = published("bundle/en/en_ult_v91.sb.zip en_ult/ingredients/PSA.usfm");
    assert!(scale.unwrap() == ult);
}
