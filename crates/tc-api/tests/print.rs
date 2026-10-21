//! The preview's print model (`tc_api::model::print`, unit B8) against tc-export's and tC4's
//! (APP-DESIGN §3.8): `print_parts` says what tc-export's says on every preview page of
//! `fixtures/export/print-vectors.json`, and `paginate` makes tC4's pages from them.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::OnceLock;

use serde_json::Value;
use tc_api::export::{PrintChapter, PrintItem, PrintVerse, Run};
use tc_api::ids::RefPart;
use tc_api::model::print::{
    Atom, ChapterOpening, FlowBlock, Opening, PageSetup, Paper, Piece, PrintPart, Spacing,
    VerseLine, VerseNumbers, atoms_of, continues, paginate, parts_of, print_parts,
};
use tc_export::book as export;

fn vectors() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| {
        let path = common::crate_dir().join("../../fixtures/export/print-vectors.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    })
}

fn books() -> &'static [Value] {
    vectors()["books"].as_array().unwrap()
}

/// A book case's chapters in both models: tc-export's input, and the wire's verses as the
/// server sends them (a verse with no text to print has none).
fn chapters(case: &Value) -> (Vec<export::BookChapter>, Vec<PrintChapter>) {
    let by_chapter = case["byChapter"].as_array().unwrap();
    let mut theirs = Vec::new();
    let mut ours = Vec::new();
    for c in case["chapterNums"].as_array().unwrap() {
        let number = u16::try_from(c.as_u64().unwrap()).unwrap();
        let verses: Vec<export::BookVerse> = by_chapter
            .iter()
            .filter(|entry| entry["key"].as_str().unwrap().parse::<u16>().ok() == Some(number))
            .flat_map(|entry| entry["verses"].as_array().unwrap())
            .map(|v| export::BookVerse {
                label: v["n"].as_str().unwrap().into(),
                drafted: v["drafted"].as_bool().unwrap(),
                text: v["text"].as_str().unwrap().into(),
            })
            .collect();
        ours.push(PrintChapter {
            chapter: number,
            verses: verses
                .iter()
                .map(|v| PrintVerse {
                    label: RefPart::parse(&v.label).unwrap(),
                    text: v.printed_text().map(str::to_owned),
                })
                .collect(),
        });
        theirs.push(export::BookChapter { number, verses });
    }
    (theirs, ours)
}

/// tc-export's printed items as the server sends them.
fn wire_items(items: &[export::PrintedItem<'_>], ours: &[PrintChapter]) -> Vec<PrintItem> {
    items
        .iter()
        .map(|item| match item {
            export::PrintedItem::Gap(run) => PrintItem::Gap(Run {
                from: run.from,
                to: run.to,
            }),
            export::PrintedItem::Chapter(chapter) => PrintItem::Chapter(
                ours.iter()
                    .find(|c| c.chapter == chapter.number)
                    .unwrap()
                    .clone(),
            ),
        })
        .collect()
}

fn setup_of(preview: &Value) -> PageSetup {
    PageSetup {
        chapter_opening: if preview["dropCapChapters"].as_bool().unwrap() {
            ChapterOpening::DropCap
        } else {
            ChapterOpening::Heading
        },
        verse_numbers: if preview["verseNumbers"].as_bool().unwrap() {
            VerseNumbers::Shown
        } else {
            VerseNumbers::Hidden
        },
        ..PageSetup::default()
    }
}

fn their_setup(setup: &PageSetup) -> tc_export::page::PageSetup {
    tc_export::page::PageSetup {
        chapter_opening: match setup.chapter_opening {
            ChapterOpening::DropCap => tc_export::page::ChapterOpening::DropCap,
            ChapterOpening::Heading => tc_export::page::ChapterOpening::Heading,
        },
        verse_numbers: match setup.verse_numbers {
            VerseNumbers::Shown => tc_export::page::VerseNumbers::Shown,
            VerseNumbers::Hidden => tc_export::page::VerseNumbers::Hidden,
        },
        ..tc_export::page::PageSetup::default()
    }
}

/// A page's parts as the vectors write them, in tc-export's model and in ours.
fn page_parts<'a>(
    page: &Value,
    theirs: &'a [export::BookChapter],
    ours: &'a [PrintChapter],
) -> (Vec<export::PrintPart<'a>>, Vec<PrintPart<'a>>) {
    let mut out = (Vec::new(), Vec::new());
    for part in page["parts"].as_array().unwrap() {
        if let Some(gap) = part["gap"].as_array() {
            let n = |i: usize| u16::try_from(gap[i].as_u64().unwrap()).unwrap();
            out.0.push(export::PrintPart::Gap(tc_export::Run {
                from: n(0),
                to: n(1),
            }));
            out.1.push(PrintPart::Gap(Run {
                from: n(0),
                to: n(1),
            }));
            continue;
        }
        let number = u16::try_from(part["c"].as_u64().unwrap()).unwrap();
        let head = part["head"].as_bool().unwrap();
        let their_chapter = theirs.iter().find(|c| c.number == number).unwrap();
        let our_chapter = ours.iter().find(|c| c.chapter == number).unwrap();
        let mut their_verses = Vec::new();
        let mut our_verses = Vec::new();
        for v in part["verses"].as_array().unwrap() {
            let label = v["n"].as_str().unwrap();
            let at = their_chapter
                .verses
                .iter()
                .position(|x| x.label == label)
                .unwrap();
            let (verse, our_verse) = (&their_chapter.verses[at], &our_chapter.verses[at]);
            let text = v["text"].as_str().unwrap();
            let (their_piece, our_piece) = match v["rest"].as_str() {
                Some(rest) => {
                    let rest = shown_part(&verse.text, rest);
                    (export::Piece::Continued(rest), Piece::Continued(rest))
                }
                None if text == verse.text => (export::Piece::Whole, Piece::Whole),
                None => {
                    let start = shown_part(&verse.text, text);
                    (export::Piece::Start(start), Piece::Start(start))
                }
            };
            their_verses.push(export::VersePiece {
                verse,
                piece: their_piece,
            });
            our_verses.push((our_verse, our_piece));
        }
        out.0.push(export::PrintPart::Chapter(export::ChapterPart {
            chapter: number,
            head,
            verses: their_verses,
        }));
        out.1
            .push(PrintPart::Chapter(tc_api::model::print::ChapterPart {
                chapter: number,
                head,
                verses: our_verses,
            }));
    }
    out
}

/// `part`, which the vectors hold as a copy, as the slice of `text` it is: a piece of a verse
/// is always a run of the verse's own text.
fn shown_part<'a>(text: &'a str, part: &str) -> &'a str {
    let at = text
        .find(part)
        .unwrap_or_else(|| panic!("{part:?} is not in {text:?}"));
    &text[at..at + part.len()]
}

/// tc-export's blocks and ours, written out in one form.
fn their_lines(blocks: &[export::FlowBlock<'_>]) -> Vec<String> {
    blocks
        .iter()
        .map(|block| match block {
            export::FlowBlock::Gap(tc_export::PrintText::ChaptersNotDrafted(run)) => {
                format!("gap {}-{}", run.from, run.to)
            }
            export::FlowBlock::Gap(other) => panic!("a gap of {other:?}"),
            export::FlowBlock::Chapter(c) => format!(
                "chapter {} {} {} {:?}",
                c.chapter,
                match c.opening {
                    None => "-".to_owned(),
                    Some(export::Opening::DropCap) => "dropcap".to_owned(),
                    Some(export::Opening::Heading(tc_export::PrintText::ChapterHeading {
                        chapter,
                    })) => format!("heading {chapter}"),
                    Some(export::Opening::Heading(other)) => panic!("a heading of {other:?}"),
                },
                c.continues,
                c.verses
                    .iter()
                    .map(|v| match v {
                        export::VerseLine::Text { number, text } => {
                            format!("{number:?} {text}")
                        }
                        export::VerseLine::NotDrafted { number } => format!("undrafted {number}"),
                    })
                    .collect::<Vec<_>>()
            ),
        })
        .collect()
}

fn our_lines(blocks: &[FlowBlock<'_>]) -> Vec<String> {
    blocks
        .iter()
        .map(|block| match block {
            FlowBlock::Gap(run) => format!("gap {}-{}", run.from, run.to),
            FlowBlock::Chapter(c) => format!(
                "chapter {} {} {} {:?}",
                c.chapter,
                match c.opening {
                    None => "-".to_owned(),
                    Some(Opening::DropCap) => "dropcap".to_owned(),
                    Some(Opening::Heading) => format!("heading {}", c.chapter),
                },
                c.continues,
                c.verses
                    .iter()
                    .map(|v| match v {
                        VerseLine::Text { number, text } => format!("{number:?} {text}"),
                        VerseLine::NotDrafted { number } => format!("undrafted {number}"),
                    })
                    .collect::<Vec<_>>()
            ),
        })
        .collect()
}

#[test]
fn print_parts_is_tc_exports_on_every_preview_page() {
    let (mut pages, mut split, mut headings) = (0, 0, 0);
    for case in books() {
        let (theirs, ours) = chapters(case);
        for preview in case["preview"].as_array().unwrap() {
            let setup = setup_of(preview);
            for page in preview["pages"].as_array().unwrap() {
                let (their_parts, our_parts) = page_parts(page, &theirs, &ours);
                let carries = page["continues"].as_bool().unwrap();
                let theirs_shown = their_lines(&export::print_parts(
                    &their_parts,
                    &their_setup(&setup),
                    carries,
                ));
                let ours_shown = our_lines(&print_parts(&our_parts, &setup, carries));
                assert_eq!(ours_shown, theirs_shown, "{} {page}", case["name"]);
                split += our_parts
                    .iter()
                    .filter_map(|p| match p {
                        PrintPart::Chapter(c) => Some(c),
                        PrintPart::Gap(_) => None,
                    })
                    .flat_map(|c| &c.verses)
                    .filter(|(_, piece)| *piece != Piece::Whole)
                    .count();
                headings += usize::from(setup.chapter_opening == ChapterOpening::Heading);
                pages += 1;
            }
        }
    }
    // The vectors split verses across pages, continue chapters and turn drop caps off, or
    // they test little.
    assert!(
        pages > 60 && split > 20 && headings > 30,
        "{pages} pages, {split} split verses, {headings} pages with headings"
    );
}

/// What the generator's `fits` counts: `atomChars` (`gen-print-vectors.mjs`), in UTF-16 units
/// as JavaScript's `length`.
fn chars(atom: &Atom<'_>) -> usize {
    match atom {
        Atom::Gap(_) => 40,
        Atom::Verse {
            head, verse, piece, ..
        } => {
            let shown = match piece {
                Piece::Whole => verse.text.as_deref().unwrap_or_default(),
                Piece::Start(part) | Piece::Continued(part) => part,
            };
            usize::from(*head) * 24 + 6 + shown.encode_utf16().count()
        }
    }
}

/// The word segments of `text` as ICU's word breaks (`Intl.Segmenter`) give them on the
/// vectors' text: a run of letters, digits and combining marks, a run of white space, or one
/// other character. ICU breaks Thai by a dictionary, which this does not; no page of the
/// vectors breaks inside Thai words.
fn words(text: &str) -> Vec<usize> {
    fn class(c: char) -> u8 {
        let mark = matches!(u32::from(c),
            0x0300..=0x036F | 0x0591..=0x05C7 | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670
            | 0x06D6..=0x06ED | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E);
        if c.is_whitespace() {
            0
        } else if c.is_alphanumeric() || mark {
            1
        } else {
            2
        }
    }
    let mut ends = Vec::new();
    let mut last: Option<u8> = None;
    for (at, c) in text.char_indices() {
        let k = class(c);
        if let Some(prev) = last
            && (prev != k || k == 2)
        {
            ends.push(at);
        }
        last = Some(k);
    }
    if !text.is_empty() {
        ends.push(text.len());
    }
    ends
}

/// A page's parts written as the vectors write them.
fn written(parts: &[PrintPart<'_>]) -> Value {
    Value::Array(
        parts
            .iter()
            .map(|part| match part {
                PrintPart::Gap(run) => serde_json::json!({ "gap": [run.from, run.to] }),
                PrintPart::Chapter(c) => serde_json::json!({
                    "c": c.chapter,
                    "head": c.head,
                    "verses": c.verses.iter().map(|(verse, piece)| {
                        let n = verse.label.as_str();
                        match piece {
                            Piece::Whole => serde_json::json!({ "n": n }),
                            Piece::Start(text) => serde_json::json!({ "n": n, "text": text }),
                            Piece::Continued(rest) => serde_json::json!({ "n": n, "rest": rest }),
                        }
                    }).collect::<Vec<_>>(),
                }),
            })
            .collect(),
    )
}

/// The vectors' page parts in the same form: a piece's `text` only where a break cut it.
fn their_written(page: &Value, case: &Value) -> Value {
    let by_label = |c: u64, n: &str| -> String {
        case["byChapter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["key"].as_str().unwrap().parse::<u64>().ok() == Some(c))
            .flat_map(|e| e["verses"].as_array().unwrap())
            .find(|v| v["n"] == n)
            .map(|v| v["text"].as_str().unwrap().to_owned())
            .unwrap()
    };
    Value::Array(
        page["parts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|part| {
                if part.get("gap").is_some() {
                    return part.clone();
                }
                let c = part["c"].as_u64().unwrap();
                serde_json::json!({
                    "c": c,
                    "head": part["head"],
                    "verses": part["verses"].as_array().unwrap().iter().map(|v| {
                        let n = v["n"].as_str().unwrap();
                        if let Some(rest) = v["rest"].as_str() {
                            serde_json::json!({ "n": n, "rest": rest })
                        } else if v["text"].as_str().unwrap() == by_label(c, n) {
                            serde_json::json!({ "n": n })
                        } else {
                            serde_json::json!({ "n": n, "text": v["text"] })
                        }
                    }).collect::<Vec<_>>(),
                })
            })
            .collect(),
    )
}

#[test]
fn paginate_makes_tc4s_preview_pages() {
    const PAGE_CHARS: [usize; 2] = [260, 420];
    let (mut compared, mut split) = (0, 0);
    for case in books() {
        let (theirs, ours) = chapters(case);
        let items = wire_items(&export::printed_items(&theirs), &ours);
        let atoms = atoms_of(&items);
        let pages = paginate(&atoms, &mut |text| words(text), &mut |page, atoms, _| {
            atoms.iter().map(chars).sum::<usize>() <= PAGE_CHARS[page.min(1)]
        });
        let expected = case["preview"][0]["pages"].as_array().unwrap();
        assert_eq!(pages.len(), expected.len(), "{} page count", case["name"]);
        for (i, (page, theirs)) in pages.iter().zip(expected).enumerate() {
            split += page
                .iter()
                .filter(|a| matches!(a, Atom::Verse { piece, .. } if *piece != Piece::Whole))
                .count();
            let next = pages.get(i + 1).map(Vec::as_slice);
            assert_eq!(
                continues(next),
                theirs["continues"].as_bool().unwrap(),
                "{} page {i}",
                case["name"]
            );
            assert_eq!(
                written(&parts_of(page)),
                their_written(theirs, case),
                "{} page {i}",
                case["name"]
            );
            compared += 1;
        }
    }
    assert!(
        compared > 40 && split > 10,
        "{compared} pages, {split} split verses"
    );
}

#[test]
fn the_page_setup_is_tc_exports() {
    let theirs = tc_export::page::PageSetup::default();
    let ours = PageSetup::default();
    assert_eq!(their_setup(&ours), theirs);
    assert_eq!(ours.columns.count(), theirs.columns.count());
    assert!(ours.spacing == Spacing::Single && ours.paper == Paper::A4);
    for (spacing, their_spacing) in [
        (Spacing::Single, tc_export::page::Spacing::Single),
        (Spacing::Double, tc_export::page::Spacing::Double),
    ] {
        let leading = PageSetup {
            spacing,
            ..PageSetup::default()
        }
        .leading();
        let their_leading = tc_export::page::PageSetup {
            spacing: their_spacing,
            ..theirs
        }
        .leading();
        assert!((leading - their_leading).abs() < f64::EPSILON);
    }
    for (paper, their_paper) in [
        (Paper::A4, tc_export::page::Paper::A4),
        (Paper::Letter, tc_export::page::Paper::Letter),
    ] {
        let size = their_paper.size_mm();
        assert_eq!(paper.size_mm(), (size.width, size.height));
    }
    let margins = tc_export::page::PAGE_MARGIN_MM;
    assert_eq!(
        tc_api::model::print::PAGE_MARGIN_MM,
        (margins.top, margins.side, margins.bottom)
    );
}
