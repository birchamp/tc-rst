//! The preview's paging (`tc_api::model::print`, unit B8) against tC4's (APP-DESIGN §3.8):
//! `paginate` makes tC4's preview pages of `fixtures/export/print-vectors.json`. What the parts
//! of each page show is checked against tC4's markup in tc-export's `tests/vectors.rs`.
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::sync::OnceLock;

use serde_json::Value;
use tc_api::export::{PrintChapter, PrintItem, PrintVerse, Run};
use tc_api::ids::RefPart;
use tc_api::model::print::{Atom, Piece, PrintPart, atoms_of, continues, paginate, parts_of};

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

/// A book case's printed items as the server sends them: the vectors' items, each verse with
/// the text it prints (none when it prints as not yet drafted).
fn items_of(case: &Value) -> Vec<PrintItem> {
    let verse = |c: u64, n: &str| -> PrintVerse {
        let v = case["byChapter"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["key"].as_str().unwrap().parse::<u64>().ok() == Some(c))
            .flat_map(|e| e["verses"].as_array().unwrap())
            .find(|v| v["n"] == n)
            .unwrap();
        let text = v["text"].as_str().unwrap();
        PrintVerse {
            label: RefPart::parse(n).unwrap(),
            text: (v["drafted"].as_bool().unwrap() && !text.is_empty()).then(|| text.to_owned()),
        }
    };
    let n = |v: &Value| u16::try_from(v.as_u64().unwrap()).unwrap();
    case["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            if let Some(gap) = item["gap"].as_array() {
                return PrintItem::Gap(Run {
                    from: n(&gap[0]),
                    to: n(&gap[1]),
                });
            }
            let c = item["c"].as_u64().unwrap();
            PrintItem::Chapter(PrintChapter {
                chapter: n(&item["c"]),
                verses: item["verses"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|label| verse(c, label.as_str().unwrap()))
                    .collect(),
            })
        })
        .collect()
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
        let items = items_of(case);
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
