//! tC4 issues fixed upstream in October 2026, replayed on the rebuild (report 2026-10-27, area
//! helps-check): #765 and #773, the Check list's titles and the ULT pane's highlight, which the
//! rebuild takes from the gateway ULT's alignment.
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use tc_check::CheckItem;
use tc_check::derive::{BookSpec, CheckList, notes_for_book, word_links_for_book};
use tc_core::BookId;
use tc_helps::highlight::{AlignedVerse, SourceWord, highlight_quote};
use tc_helps::tsv::{TsvScope, parse_notes, parse_word_links};
use tc_helps::{BibleLocus, HelpRef, Occurrence};
use tc_usfm::{Book, SegmentKind};
use tc_versification::StandardScheme;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// One table of a bundled burrito export: `("en_tn_v91", "GEN")`.
fn bundled_tsv(export: &str, book: &str) -> String {
    let path = repo_root().join(format!("bundle/en/{export}.sb.zip"));
    let mut archive = zip::ZipArchive::new(std::fs::File::open(&path).unwrap()).unwrap();
    let repo = export.rsplit_once('_').unwrap().0;
    let mut text = String::new();
    archive
        .by_name(&format!("{repo}/ingredients/{book}.tsv"))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    text
}

fn genesis_spec() -> BookSpec<'static> {
    BookSpec {
        book: BookId::Genesis,
        resource_frame: StandardScheme::Eng.scheme(),
        project_frame: Some(StandardScheme::Eng.scheme()),
        scope: &[],
    }
}

/// Genesis's Translation Notes checks from en_tn v91, as the Check tool derives them.
fn genesis_notes() -> CheckList {
    let notes = parse_notes(
        &bundled_tsv("en_tn_v91", "GEN"),
        TsvScope::Book(BookId::Genesis),
    )
    .unwrap()
    .notes;
    notes_for_book(&notes, &genesis_spec())
}

/// Genesis's Translation Words checks from en_twl v91.
fn genesis_words() -> CheckList {
    let links = parse_word_links(
        &bundled_tsv("en_twl_v91", "GEN"),
        TsvScope::Book(BookId::Genesis),
    )
    .unwrap();
    word_links_for_book(&links, None, &genesis_spec())
}

/// The verses of an en_ult fixture, read as the app's source pane reads them
/// (`tc_app::sources::pane_book`): each verse's display tokens, every word pointing at the
/// alignment group around it.
fn ult(file: &str) -> BTreeMap<(u16, u16), AlignedVerse> {
    let usfm = std::fs::read_to_string(
        repo_root().join(format!("fixtures/tc4/test-fixtures/en_ult/{file}")),
    )
    .unwrap();
    let book = Book::parse(&usfm);
    let mut out = BTreeMap::new();
    let mut chapter = None;
    for segment in book.segments() {
        match segment.kind() {
            SegmentKind::Chapter(mark) => chapter = mark.number(),
            SegmentKind::Verse(unit) => {
                let c = unit.chapter().or(chapter).unwrap();
                let Ok(v) = unit.label().parse::<u16>() else {
                    continue;
                };
                let display = tc_usfm::display_tokens(unit.body());
                let mut verse = AlignedVerse::new();
                let ids: Vec<_> = display
                    .groups
                    .iter()
                    .map(|g| {
                        verse.add_alignment(
                            g.iter()
                                .map(|s| SourceWord::new(s.content.clone(), s.occurrence))
                                .collect(),
                        )
                    })
                    .collect();
                for token in &display.tokens {
                    if token.word {
                        verse
                            .push_word(token.text.clone(), token.group.map(|g| ids[g]))
                            .unwrap();
                    } else {
                        verse.push_text(token.text.clone());
                    }
                }
                out.insert((c, v), verse);
            }
            _ => {}
        }
    }
    out
}

/// The single verse the resource places `item` at.
fn place(item: &CheckItem) -> Option<(u16, u16)> {
    let HelpRef::Bible(r) = &item.source else {
        return None;
    };
    let BibleLocus::Verses(list) = &r.locus else {
        return None;
    };
    let [range] = list.ranges() else {
        return None;
    };
    let at = range.start();
    (at == range.end()).then(|| (at.chapter().get(), at.verse().get()))
}

fn row_id(item: &CheckItem) -> String {
    item.help_id
        .upstream_row_id()
        .map_or_else(String::new, |r| r.as_str().to_owned())
}

fn item<'a>(list: &'a CheckList, id: &str) -> &'a CheckItem {
    list.items
        .iter()
        .find(|i| row_id(i) == id)
        .unwrap_or_else(|| panic!("no check {id}"))
}

/// The title the rail and the detail header show: `CheckItem::title` over the ULT verse.
fn title(ult: &BTreeMap<(u16, u16), AlignedVerse>, item: &CheckItem) -> String {
    item.title(place(item).and_then(|at| ult.get(&at)))
}

fn hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c))
}

/// The titles of `list`'s checks in Genesis 1 that show Hebrew; how many checks there are on
/// one verse of Genesis 1, and how many of those quote a word with a maqaf.
fn hebrew_titles_in_genesis_1(
    list: &CheckList,
    ult: &BTreeMap<(u16, u16), AlignedVerse>,
) -> (Vec<String>, usize, usize) {
    let in_1: Vec<&CheckItem> = list
        .items
        .iter()
        .filter(|i| place(i).is_some_and(|(c, _)| c == 1))
        .collect();
    let maqaf = in_1
        .iter()
        .filter(|i| i.quote.as_str().contains('\u{05BE}'))
        .count();
    let wrong = in_1
        .iter()
        .map(|i| (i, title(ult, i)))
        .filter(|(_, t)| hebrew(t))
        .map(|(i, t)| format!("{} {:?}: {t}", row_id(i), place(i)))
        .collect();
    (wrong, in_1.len(), maqaf)
}

/// tC4 #765 "tN/tW check list shows Hebrew original-language quotes instead of
/// gateway-language text for some items".
///
/// Steps: open Check → Translation Notes for Genesis (en_tn v91, an aligned en_ult), sort By
/// verse and read the list at Gen 1:5–1:7; then the same in Translation Words (en_twl v91).
/// Every title of an aligned verse is the ULT's words: Gen 1:5 xxe6 "Then there was evening
/// and there was morning", 1:7 xfx7 "And it was so", tW 1:28 vzz2 "the earth", each lit in
/// the ULT pane; titles without a maqaf stay (1:5 c9yz "And God called", xmx5 "one day").
/// No title of Genesis 1 shows Hebrew.
#[test]
fn tc4_765_maqaf_quotes_show_english_titles_in_the_genesis_1_lists() {
    let ult = ult("GEN-1.usfm");
    assert_eq!(ult.len(), 31, "the fixture is the whole of Genesis 1");
    let notes = genesis_notes();
    let words = genesis_words();

    let (wrong, checks, maqaf) = hebrew_titles_in_genesis_1(&notes, &ult);
    assert!(
        checks > 30 && maqaf > 0,
        "{checks} notes, {maqaf} with a maqaf"
    );
    assert!(
        wrong.is_empty(),
        "tN titles in Hebrew:\n{}",
        wrong.join("\n")
    );
    let (wrong, checks, maqaf) = hebrew_titles_in_genesis_1(&words, &ult);
    assert!(
        checks > 30 && maqaf > 0,
        "{checks} words, {maqaf} with a maqaf"
    );
    assert!(
        wrong.is_empty(),
        "tW titles in Hebrew:\n{}",
        wrong.join("\n")
    );

    let vzz2 = item(&words, "vzz2");
    assert!(vzz2.quote.as_str().contains('\u{05BE}'));
    for (check, want) in [
        (
            item(&notes, "xxe6"),
            "Then there was evening and there was morning",
        ),
        (item(&notes, "xfx7"), "And it was so"),
        (vzz2, "the earth"),
        (item(&notes, "c9yz"), "And God called"),
        (item(&notes, "xmx5"), "one day"),
    ] {
        let id = row_id(check);
        assert_eq!(title(&ult, check), want, "{id}");
        // The ULT pane lights exactly the title's words.
        let verse = &ult[&place(check).unwrap()];
        let occurrence = Occurrence::nth(check.key.occurrence).unwrap_or(Occurrence::NotTied);
        let lit = highlight_quote(verse, &check.quote, occurrence);
        assert_eq!(lit.texts(verse).join(" "), want, "{id} lit");
    }
}

/// tC4 #773 "Check: a title and the ULT highlight pick up words from elsewhere in the verse
/// when the quote's word order differs from the ULT".
///
/// Steps: open Check → Translation Notes for Genesis (en_tn and en_ult v91) and find Gen 33:5
/// mix3, Gen 24:48 gyfv and Gen 9:11 r2u9. Each title is the quote's own phrase, and the ULT
/// pane lights only that phrase's words, never words that the quote's common particles (אֶת,
/// כָּל) name elsewhere in the verse.
#[test]
fn tc4_773_reordered_quotes_title_and_light_only_their_own_phrase() {
    let ult = ult("GEN-773.usfm");
    let notes = genesis_notes();
    let mut wrong = Vec::new();
    for (id, at, want) in [
        (
            "mix3",
            (33, 5),
            "whom God has graciously given to your servant",
        ),
        ("gyfv", (24, 48), "the daughter of my master s brother"),
        (
            "r2u9",
            (9, 11),
            "that never again will all flesh be cut off",
        ),
    ] {
        let check = item(&notes, id);
        assert_eq!(place(check), Some(at), "{id}");
        let got = title(&ult, check);
        let verse = &ult[&at];
        let occurrence = Occurrence::nth(check.key.occurrence).unwrap_or(Occurrence::NotTied);
        let lit = highlight_quote(verse, &check.quote, occurrence);
        let phrase: BTreeSet<&str> = want.split(' ').collect();
        let stray: Vec<&str> = lit
            .texts(verse)
            .into_iter()
            .flat_map(|t| t.split_whitespace())
            .filter(|w| !phrase.contains(w))
            .collect();
        if got != want || !stray.is_empty() {
            wrong.push(format!(
                "{id} {}:{}: title {got:?}, lit beyond the phrase {stray:?}; tC4 {want:?}",
                at.0, at.1
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
