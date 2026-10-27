//! tc-export against what tC4's own print code answers (`fixtures/export/print-vectors.json`,
//! made by `crates/tc-export/tools/gen-print-vectors.mjs` from tC4 at `d310bf3`): the page
//! setup, the printed items and parts of the Titus and Jonah samples and of generated books,
//! the preview's pages, and the print of the OBS sample and of generated story sets. The
//! print is compared as the markup tC4's components render, written from the model by
//! `common::markup`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::collections::BTreeMap;

use common::{
    chapters_of, direction, fixture, markup, picture_sources, setup_of, story_of, vectors,
};
use serde_json::{Value, json};
use tc_api::export::{PrintChapter, PrintItem, Run};
use tc_api::model::print::{
    ChapterOpening, ChapterPart, Columns, ObsLayout, PRINT_LEADING, PageSetup, Paper, Pictures,
    Piece, PrintPart, Spacing, VerseNumbers, atoms_of, parts_of, print_parts,
};
use tc_export::Direction;
use tc_export::book::{PrintedItem, has_undrafted, print_book, print_items, printed_items};
use tc_export::page::{PAGE_MARGIN_MM, PrintVariables, css_size, variables};
use tc_export::stories::print_stories;
use tc_helps::obs::ObsStory;
use tc_obs::print::printed;

fn cases(name: &str) -> &'static [Value] {
    vectors()[name].as_array().unwrap()
}

fn items_json(items: &[PrintedItem<'_>]) -> Value {
    items
        .iter()
        .map(|item| match item {
            PrintedItem::Gap(run) => json!({"gap": [run.from, run.to]}),
            PrintedItem::Chapter(chapter) => json!({
                "c": chapter.number,
                "verses": chapter.verses.iter().map(|v| v.label.as_str()).collect::<Vec<_>>(),
            }),
        })
        .collect()
}

fn parts_json(parts: &[PrintPart<'_>]) -> Value {
    parts
        .iter()
        .map(|part| match part {
            PrintPart::Gap(run) => json!({"gap": [run.from, run.to]}),
            PrintPart::Chapter(part) => json!({
                "c": part.chapter,
                "head": part.head,
                "verses": part.verses.iter().map(|(v, _)| v.label.as_str()).collect::<Vec<_>>(),
            }),
        })
        .collect()
}

#[test]
fn the_page_setup_is_tc4s() {
    let theirs = &vectors()["pageSetup"];
    let default = PageSetup::default();
    assert_eq!(default.columns, Columns::One);
    assert_eq!(default.chapter_opening, ChapterOpening::DropCap);
    assert_eq!(default.verse_numbers, VerseNumbers::Shown);
    assert_eq!(default.spacing, Spacing::Single);
    assert_eq!(default.paper, Paper::A4);
    assert_eq!(default.pictures, Pictures::Shown);
    assert_eq!(default.obs_layout, ObsLayout::Above);
    assert_eq!(
        theirs["default"],
        json!({"columns": 1, "dropCapChapters": true, "verseNumbers": true, "spacing": "single",
               "paper": "a4", "pictures": true, "obsLayout": "above"})
    );
    assert_eq!(theirs["leading"], json!(PRINT_LEADING));
    let mm = |v: &Value| v.as_f64().unwrap();
    for (name, spacing) in [("single", Spacing::Single), ("double", Spacing::Double)] {
        let leading = PageSetup {
            spacing,
            ..PageSetup::default()
        }
        .leading();
        assert_eq!(
            mm(&theirs["spacingFactor"][name]) * PRINT_LEADING,
            leading,
            "{name}"
        );
    }
    for (name, paper) in [("a4", Paper::A4), ("letter", Paper::Letter)] {
        let theirs = &theirs["paperMm"][name];
        assert_eq!((mm(&theirs[0]), mm(&theirs[1])), paper.size_mm());
    }
    let margins = &theirs["marginMm"];
    assert_eq!(
        (
            mm(&margins["top"]),
            mm(&margins["side"]),
            mm(&margins["bottom"])
        ),
        (
            PAGE_MARGIN_MM.top,
            PAGE_MARGIN_MM.side,
            PAGE_MARGIN_MM.bottom
        )
    );
    let cases = theirs["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    for case in cases {
        let s = &case["setup"];
        let setup = PageSetup {
            columns: if s["columns"] == 1 {
                Columns::One
            } else {
                Columns::Two
            },
            spacing: if s["spacing"] == "single" {
                Spacing::Single
            } else {
                Spacing::Double
            },
            paper: if s["paper"] == "a4" {
                Paper::A4
            } else {
                Paper::Letter
            },
            ..PageSetup::default()
        };
        let vars = variables(&setup);
        let theirs = &case["variables"];
        assert_eq!(theirs.as_object().unwrap().len(), 2);
        assert_eq!(
            (
                theirs[PrintVariables::COLUMNS].as_u64(),
                mm(&theirs[PrintVariables::LEADING])
            ),
            (Some(u64::from(vars.columns)), vars.leading),
            "{case}"
        );
        // The rules pdf.ts writes, from the model's values.
        let m = PAGE_MARGIN_MM;
        assert_eq!(
            case["page"],
            format!(
                "@page {{ size: {}; margin: {}mm {}mm {}mm;",
                css_size(setup.paper),
                m.top,
                m.side,
                m.bottom
            )
        );
        assert_eq!(
            case["root"],
            format!(
                ":root {{ {}: {}; {}: {}; }}",
                PrintVariables::COLUMNS,
                vars.columns,
                PrintVariables::LEADING,
                vars.leading
            )
        );
    }
}

#[test]
fn printed_items_and_parts_are_tc4s() {
    let mut compared = 0;
    for case in cases("books") {
        let name = case["name"].as_str().unwrap();
        let chapters = chapters_of(case);
        let items = printed_items(&chapters);
        assert_eq!(items_json(&items), case["items"], "{name}");
        let items = print_items(&items).unwrap();
        assert_eq!(
            parts_json(&parts_of(&atoms_of(&items))),
            case["parts"],
            "{name}"
        );
        compared += 1;
    }
    assert!(compared >= 30, "{compared}");
}

#[test]
fn a_book_with_nothing_to_print_is_the_one_the_pdf_refuses() {
    for case in cases("books") {
        let chapters = chapters_of(case);
        let refused = case["refused"].as_str();
        assert_eq!(
            printed_items(&chapters).is_empty(),
            refused.is_some(),
            "{}",
            case["name"]
        );
        if let Some(code) = refused {
            assert_eq!(code, "export.nothing-drafted");
        }
    }
}

#[test]
fn the_book_print_is_tc4s() {
    let mut compared = 0;
    for case in cases("books") {
        let name = case["name"].as_str().unwrap();
        let chapters = chapters_of(case);
        let items = print_items(&printed_items(&chapters)).unwrap();
        let title = case["title"].as_str().unwrap();
        for variant in case["html"].as_array().unwrap() {
            let view = print_book(
                title,
                &items,
                &setup_of(variant),
                direction(&variant["dir"]),
            );
            assert_eq!(
                markup::book(&view),
                variant["html"].as_str().unwrap(),
                "{name} {variant}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8 * cases("books").len());
}

/// A page of the preview as print parts: each verse found in the book by chapter and label,
/// with the words the page shows of it.
fn page_parts<'a>(page: &'a Value, items: &'a [PrintItem]) -> Vec<PrintPart<'a>> {
    page["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| {
            if let Some(gap) = part["gap"].as_array() {
                let n = |i: usize| u16::try_from(gap[i].as_u64().unwrap()).unwrap();
                return PrintPart::Gap(Run {
                    from: n(0),
                    to: n(1),
                });
            }
            let number = u16::try_from(part["c"].as_u64().unwrap()).unwrap();
            let chapter: &PrintChapter = items
                .iter()
                .find_map(|item| match item {
                    PrintItem::Chapter(c) if c.chapter == number => Some(c),
                    _ => None,
                })
                .unwrap();
            let verses = part["verses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    let label = v["n"].as_str().unwrap();
                    let verse = chapter
                        .verses
                        .iter()
                        .find(|x| x.label.as_str() == label)
                        .unwrap();
                    let text = v["text"].as_str().unwrap();
                    let piece = match v["rest"].as_str() {
                        Some(rest) => Piece::Continued(rest),
                        None if text == verse.text.as_deref().unwrap_or_default() => Piece::Whole,
                        None => Piece::Start(text),
                    };
                    (verse, piece)
                })
                .collect();
            PrintPart::Chapter(ChapterPart {
                chapter: number,
                head: part["head"].as_bool().unwrap(),
                verses,
            })
        })
        .collect()
}

#[test]
fn the_preview_pages_are_tc4s() {
    let (mut pages, mut split) = (0, 0);
    for case in cases("books") {
        let items = print_items(&printed_items(&chapters_of(case))).unwrap();
        for preview in case["preview"].as_array().unwrap() {
            let setup = setup_of(preview);
            for page in preview["pages"].as_array().unwrap() {
                let parts = page_parts(page, &items);
                split += parts
                    .iter()
                    .filter_map(|p| match p {
                        PrintPart::Chapter(c) => Some(c),
                        PrintPart::Gap(_) => None,
                    })
                    .flat_map(|c| &c.verses)
                    .filter(|(_, piece)| *piece != Piece::Whole)
                    .count();
                let blocks = print_parts(&parts, &setup, page["continues"].as_bool().unwrap());
                assert_eq!(
                    markup::flow(&blocks, Direction::Ltr),
                    page["html"].as_str().unwrap(),
                    "{} {page}",
                    case["name"]
                );
                pages += 1;
            }
        }
    }
    // The vectors split verses across pages and continue chapters, or they test little.
    assert!(
        pages > 60 && split > 20,
        "{pages} pages, {split} split verses"
    );
}

#[test]
fn the_incomplete_notice_follows_the_print() {
    for case in cases("books") {
        let chapters = chapters_of(case);
        let drafted_without_text = chapters
            .iter()
            .flat_map(|c| &c.verses)
            .any(|v| v.drafted && v.text.is_empty());
        let theirs = case["undrafted"].as_bool().unwrap();
        // The declared difference: tC4 says nothing when every verse that prints as not yet
        // drafted is drafted with no text to show.
        assert_eq!(
            has_undrafted(&chapters),
            theirs || drafted_without_text,
            "{}",
            case["name"]
        );
    }
    let case = cases("books")
        .iter()
        .find(|c| c["name"] == "drafted-without-text")
        .unwrap();
    assert!(!case["undrafted"].as_bool().unwrap());
    assert!(has_undrafted(&chapters_of(case)));
}

#[test]
fn the_samples_print_as_in_tc4() {
    let titus = cases("books")
        .iter()
        .find(|c| c["name"] == "titus-sample")
        .unwrap();
    let chapters = chapters_of(titus);
    let items = print_items(&printed_items(&chapters)).unwrap();
    // Titus 1 has verses 1-5 drafted; chapters 2 and 3 have none, so they are left out.
    assert_eq!(items.len(), 1);
    let view = print_book("Titus", &items, &PageSetup::default(), Direction::Ltr);
    let html = markup::book(&view);
    assert!(html.contains("<span><sup class=\"print-verse-number\">1</sup>Pablo, siervo de Dios"));
    assert_eq!(html.matches("[ verse not yet drafted ]").count(), 11);
    let jonah = cases("books")
        .iter()
        .find(|c| c["name"] == "jonah-sample")
        .unwrap();
    let chapters = chapters_of(jonah);
    assert_eq!(
        items_json(&printed_items(&chapters)),
        json!([{"c": 2, "verses": ["1", "2", "3", "4", "5", "6", "7", "8", "9-10"]}])
    );
}

/// Every story of a story case, and the stories of the OBS sample as tc-obs reads the files.
fn stories_of(case: &Value) -> Vec<ObsStory> {
    let stories: Vec<ObsStory> = case["stories"]
        .as_array()
        .unwrap()
        .iter()
        .map(story_of)
        .collect();
    if case["name"] == "obs-sample" {
        let read: Vec<ObsStory> = (1..=50)
            .map(|n| {
                tc_obs::story::parse_story(&fixture(&format!(
                    "tc4/sample-burrito-obs/ingredients/content/{n:02}.md"
                )))
                .unwrap()
            })
            .collect();
        assert_eq!(read.len(), stories.len());
        for (ours, theirs) in read.iter().zip(&stories) {
            assert_eq!(ours.number, theirs.number);
            assert_eq!(ours.title, theirs.title);
            assert_eq!(ours.reference, theirs.reference);
            let texts = |s: &ObsStory| s.frames.iter().map(|f| f.text.clone()).collect::<Vec<_>>();
            assert_eq!(texts(ours), texts(theirs));
        }
        return read;
    }
    stories
}

fn pictures_json(pictures: &BTreeMap<u8, BTreeMap<usize, String>>) -> Value {
    pictures
        .iter()
        .map(|(story, frames)| {
            let frames: serde_json::Map<String, Value> = frames
                .iter()
                .map(|(n, src)| (n.to_string(), json!(src)))
                .collect();
            (story.to_string(), Value::Object(frames))
        })
        .collect::<serde_json::Map<String, Value>>()
        .into()
}

#[test]
fn the_story_print_is_tc4s() {
    let mut compared = 0;
    for case in cases("stories") {
        let name = case["name"].as_str().unwrap();
        let stories = stories_of(case);
        let mut asked = Vec::new();
        let mut sources = |story: &ObsStory| {
            asked.push(story.number);
            picture_sources(story)
        };
        let printed = printed(&stories, Some(&mut sources));
        assert_eq!(json!(asked), case["picturesAskedFor"], "{name}");
        assert_eq!(pictures_json(&printed.pictures), case["pictures"], "{name}");
        assert_eq!(json!(printed.undrafted), case["undrafted"], "{name}");
        for variant in case["html"].as_array().unwrap() {
            let view = print_stories(
                &printed.items,
                &printed.pictures,
                &setup_of(variant),
                direction(&variant["dir"]),
            );
            assert_eq!(
                markup::stories(&view),
                variant["html"].as_str().unwrap(),
                "{name} {variant}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 8 * cases("stories").len());
}
