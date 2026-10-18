//! A check is at the place its resource writes, and the types that name a check say so
//! (APP-DESIGN §3.4, §3.7, AD17). On the wire a place is text, `PSA 60:front` or `OBS 3:4`. tC4's resources put a Psalms superscription at verse
//! `front`, a note on several verses at `1,3,8,12`, a note on an introduction at
//! `front:intro`, and an Open Bible Stories note at a story and a frame. An `Ack` that
//! cannot carry the row of such a check leaves its decision looking unsaved, and a rail that
//! cannot read the row drops the check from view.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::{Value, json};
use tc_api::check::{
    CheckListRef, CheckPlace, CheckUnit, Dot, RailRow, RailRowPatch, RowFlags, Tool,
};
use tc_api::event::{ChangeParts, ChangeScope};
use tc_api::ids::{RefPart, StoryNo, ViewSeq};
use tc_api::write::{Ack, Stored};
use tc_core::BookId;

fn fixture(relative: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn json_fixture(relative: &str) -> Value {
    serde_json::from_str(&fixture(relative)).unwrap()
}

fn row(index: usize, place: CheckPlace) -> RailRowPatch {
    RailRowPatch {
        row: RailRow {
            index: u32::try_from(index).unwrap(),
            place,
            title: format!("check {index}"),
            flags: RowFlags {
                dot: Dot::Todo,
                decided: false,
                bookmarked: false,
                commented: false,
            },
            term: None,
            category: None,
            marker: None,
        },
    }
}

fn ack_of(rows: Vec<RailRowPatch>) -> Ack {
    Ack {
        view: ViewSeq::new(1),
        stored: Stored::Written,
        tokens: vec![],
        new_conflicts: vec![],
        saved: vec![],
        rows,
        disk_warning: None,
    }
}

/// The whole write reply, as the server sends it and the UI reads it.
fn assert_the_ack_survives_the_wire(rows: Vec<RailRowPatch>) {
    let ack = ack_of(rows);
    let text = serde_json::to_string(&ack).unwrap();
    let back: Ack = serde_json::from_str(&text).unwrap();
    assert!(
        back == ack,
        "an Ack of {} rows changed on the wire",
        ack.rows.len()
    );
}

fn verse(book: BookId, chapter: &str, verse: &str) -> CheckPlace {
    CheckPlace::Verse {
        book,
        chapter: RefPart::parse(chapter).unwrap(),
        verse: RefPart::parse(verse).unwrap(),
    }
}

/// A reference part as a generated vector holds it: a number, or text.
fn part_text(part: &Value) -> Option<String> {
    match part {
        Value::Number(n) => Some(n.to_string()),
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        // tC4 leaves a part `undefined` when its cell is missing, and an empty cell has no
        // text to place a check by.
        _ => None,
    }
}

/// `[checkId, chapter, verse, occurrence, ..]` rows of a vector file as places in `book`.
fn places_of(rows: &[Value], book: BookId) -> Vec<CheckPlace> {
    rows.iter()
        .filter_map(|item| {
            let chapter = part_text(&item[1])?;
            let verse_part = part_text(&item[2])?;
            Some(verse(book, &chapter, &verse_part))
        })
        .collect()
}

fn verse_parts(place: &CheckPlace) -> (&str, &str) {
    match place {
        CheckPlace::Verse { chapter, verse, .. } => (chapter.as_str(), verse.as_str()),
        CheckPlace::Frame { .. } => panic!("a frame"),
    }
}

#[test]
fn every_check_tc4_derives_for_the_psalms_is_a_row_an_ack_carries() {
    // `psalms-vectors.json` holds what tC4's `deriveForProject` makes of the shipped Psalms
    // tN and TWL tables, in every scope and into both versifications. The numeric
    // `Reference` the first build used refused 277 superscriptions, 39 spans and a verse
    // list among them.
    let vectors = json_fixture("check/psalms-vectors.json");
    let cases = vectors["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18);
    let mut non_numeric = BTreeSet::new();
    let mut checks = 0;
    for case in cases {
        let items = case["items"].as_array().unwrap();
        let places = places_of(items, BookId::Psalms);
        assert_eq!(places.len(), items.len(), "every check has a place");
        checks += places.len();
        for place in &places {
            let (_, verse) = verse_parts(place);
            if !verse.bytes().all(|b| b.is_ascii_digit()) {
                non_numeric.insert(verse.to_owned());
            }
        }
        assert_the_ack_survives_the_wire(
            places
                .into_iter()
                .enumerate()
                .map(|(i, place)| row(i, place))
                .collect(),
        );
    }
    assert_eq!(checks, 31_429);
    // Without these forms the test would pass for the wrong reason.
    for form in ["front", "1,3,8,12", "2-3"] {
        assert!(non_numeric.contains(form), "no check has verse {form:?}");
    }
}

#[test]
fn the_psalms_forms_that_a_numeric_reference_refused_read_back_as_written() {
    // The checks in the finding, as the UI receives them in a write's reply: a
    // superscription, a verse list, a span, an introduction and a story's frame.
    let places = [
        "PSA 60:front",
        "PSA 5:1,3,8,12",
        "PSA 5:2-3",
        "TIT 1:intro",
        "TIT front:intro",
        "OBS 3:4",
    ];
    let rows: Vec<Value> = places
        .iter()
        .map(|place| {
            json!({"row": {
                "index": 0,
                "place": place,
                "title": "a check",
                "flags": {"dot": "todo", "decided": false, "bookmarked": false,
                          "commented": false},
                "term": null, "category": null, "marker": null,
            }})
        })
        .collect();
    let wire = json!({
        "view": 1, "stored": "written", "tokens": [], "newConflicts": [], "saved": [],
        "rows": rows, "diskWarning": null,
    });
    let ack: Ack = serde_json::from_value(wire.clone()).unwrap();
    let read: Vec<String> = ack.rows.iter().map(|r| r.row.place.to_string()).collect();
    assert_eq!(read, places);
    assert_eq!(serde_json::to_value(&ack).unwrap(), wire);
    assert_eq!(ack.rows[0].row.place, verse(BookId::Psalms, "60", "front"));
    assert_eq!(
        ack.rows[5].row.place,
        CheckPlace::Frame {
            story: StoryNo::new(3).unwrap(),
            frame: 4
        }
    );
}

#[test]
fn the_odd_references_tc4_reads_are_rows_too() {
    // `reference-forms-vectors.json`: `1a`, `front `, `1,front`, an en dash, an Arabic-Indic
    // digit, chapter 0 and 151, `front:front`. Whatever has text for both parts is a place.
    let vectors = json_fixture("check/reference-forms-vectors.json");
    let mut places = Vec::new();
    for rows in vectors["rows"].as_object().unwrap().values() {
        places.extend(places_of(rows.as_array().unwrap(), BookId::Psalms));
    }
    assert!(places.len() > 60, "{} places", places.len());
    let verses: BTreeSet<&str> = places.iter().map(|p| verse_parts(p).1).collect();
    for form in ["1a", "front ", "1,front", "1\u{2013}3", "\u{663}", "intro"] {
        assert!(verses.contains(form), "no vector has verse {form:?}");
    }
    assert_the_ack_survives_the_wire(
        places
            .into_iter()
            .enumerate()
            .map(|(i, p)| row(i, p))
            .collect(),
    );
}

/// The `Reference` column of a tC4 resource table.
fn references(relative: &str) -> Vec<String> {
    fixture(relative)
        .lines()
        .skip(1)
        .filter_map(|line| line.split('\t').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_note_on_an_introduction_is_a_row() {
    // en_tn v91 Titus: `front:intro` (the book's introduction) and `1:intro` (chapter 1's).
    let places: Vec<CheckPlace> = references("tc4/test-fixtures/resources/en_tn@v91/TIT.tsv")
        .iter()
        .map(|reference| {
            let (chapter, verse_text) = reference.split_once(':').unwrap();
            verse(BookId::Titus, chapter, verse_text)
        })
        .collect();
    let parts: BTreeSet<(&str, &str)> = places.iter().map(verse_parts).collect();
    assert!(parts.contains(&("front", "intro")));
    assert!(parts.contains(&("1", "intro")));
    assert_the_ack_survives_the_wire(
        places
            .into_iter()
            .enumerate()
            .map(|(i, p)| row(i, p))
            .collect(),
    );
}

#[test]
fn every_open_bible_stories_check_is_a_row_at_its_story_and_frame() {
    // en_obs-tn v13: 2,325 notes in 50 stories; frame 0 is a story's title. tC4 shows a
    // story check as `story:frame` (`locatorOf`), and `CheckTarget::Frame` keys it.
    let frames: Vec<(u8, u16)> = references("tc4/test-fixtures/resources/en_obs-tn@v13/OBS.tsv")
        .iter()
        .map(|reference| {
            let (story, frame) = reference.split_once(':').unwrap();
            (story.parse().unwrap(), frame.parse().unwrap())
        })
        .collect();
    let stories: BTreeSet<u8> = frames.iter().map(|(story, _)| *story).collect();
    assert_eq!(stories, (1..=50).collect());
    assert!(frames.iter().any(|(_, frame)| *frame == 0), "a title check");
    let rows = frames
        .iter()
        .enumerate()
        .map(|(i, (story, frame))| {
            row(
                i,
                CheckPlace::Frame {
                    story: StoryNo::new(*story).unwrap(),
                    frame: *frame,
                },
            )
        })
        .collect();
    assert_the_ack_survives_the_wire(rows);
}

#[test]
fn a_place_that_is_not_one_refuses() {
    for bad in [
        // A part is text, never empty, and never holds the separator.
        json!("PSA :1"),
        json!("PSA 1:"),
        json!("PSA :"),
        json!("PSA 1:2:3"),
        // The book is a book, and a place has a book, a chapter and a verse.
        json!("XYZ 1:2"),
        json!("1:2"),
        json!("PSA1:2"),
        json!("PSA 12"),
        json!(""),
        // A story is one of the fifty and a frame is a number, written the one way.
        json!("OBS 0:1"),
        json!("OBS 51:1"),
        json!("OBS 3:x"),
        json!("OBS 3:-1"),
        json!("OBS 3:65536"),
        json!("OBS 03:4"),
        json!("OBS 3:+4"),
        json!("OBS front:1"),
        json!("OBS 3:"),
        // Not text: the object forms tried and the numbers a resource's cells become.
        json!({"type": "verse", "value": {"book": "PSA", "chapter": "1", "verse": "2"}}),
        json!(1),
        json!(null),
    ] {
        assert!(
            serde_json::from_value::<CheckPlace>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn a_place_is_the_text_tc_check_prints_for_its_target() {
    // `CheckPlace` mirrors tc-check's `CheckTarget`; the server turns one into the other,
    // and a rail row shows the same `c:v` that tc-check's `locator` gives.
    use tc_check::CheckTarget;
    let vectors = json_fixture("check/psalms-vectors.json");
    let whole_book = vectors["cases"][0]["items"].as_array().unwrap();
    let mut parts = BTreeSet::new();
    for item in whole_book {
        parts.insert((part_text(&item[1]).unwrap(), part_text(&item[2]).unwrap()));
    }
    assert!(parts.len() > 150, "{} distinct places", parts.len());
    for (chapter, verse_text) in parts {
        let target = CheckTarget::Verse {
            book: BookId::Psalms,
            chapter: tc_check::RefPart::new(&chapter).unwrap(),
            verse: tc_check::RefPart::new(&verse_text).unwrap(),
        };
        let place = verse(BookId::Psalms, &chapter, &verse_text);
        assert_eq!(place.to_string(), target.to_string());
        assert_eq!(place.to_string().parse::<CheckPlace>().unwrap(), place);
    }
    for (story, frame) in [(1u8, 0u16), (3, 4), (50, 15)] {
        let target = CheckTarget::Frame(tc_helps::ObsRef::new(story, frame).unwrap());
        let place = CheckPlace::Frame {
            story: StoryNo::new(story).unwrap(),
            frame,
        };
        assert_eq!(place.to_string(), target.to_string());
    }
}

#[test]
fn no_book_has_the_code_of_the_stories() {
    // A place that starts with `OBS` is a story's check, so a book must never be called that.
    for book in BookId::ALL {
        assert_ne!(book.code(), "OBS");
    }
}

#[test]
fn a_place_reads_back_as_the_text_it_was_written_as() {
    // The text is the place, so what the server sends is what the UI shows as `c:v`
    // (`locatorOf`), and spacing a resource kept in a part is kept.
    for text in [
        "PSA 60:front",
        "TIT 1:4",
        "JON 2:9-10",
        "PSA  3:1",
        "PSA 3:front ",
        "PSA 1:1\u{2013}3",
        "OBS 50:0",
        "OBS 1:65535",
    ] {
        let place: CheckPlace = serde_json::from_value(json!(text)).unwrap();
        assert_eq!(place.to_string(), text);
        assert_eq!(serde_json::to_value(&place).unwrap(), json!(text));
    }
}

#[test]
fn a_check_list_names_its_book_or_the_stories() {
    // `CheckListRef { tool, book }` had no form for the stories, though a check's tool and
    // its unit are what tc-check keys a list by (`Collection::Stories`).
    let titus: CheckListRef = serde_json::from_value(json!({
        "tool": "translationNotes", "unit": {"type": "book", "value": "TIT"}
    }))
    .unwrap();
    assert_eq!(
        titus,
        CheckListRef {
            tool: Tool::TranslationNotes,
            unit: CheckUnit::Book(BookId::Titus)
        }
    );
    let stories: CheckListRef = serde_json::from_value(json!({
        "tool": "translationWords", "unit": {"type": "stories"}
    }))
    .unwrap();
    assert_eq!(
        stories,
        CheckListRef {
            tool: Tool::TranslationWords,
            unit: CheckUnit::Stories
        }
    );
    // The first form, a book beside the tool, is not read as a list of anything.
    assert!(
        serde_json::from_value::<CheckListRef>(json!({"tool": "translationNotes", "book": "TIT"}))
            .is_err()
    );
}

#[test]
fn a_change_can_name_the_story_check_lists_it_touched() {
    let stories = CheckListRef {
        tool: Tool::TranslationNotes,
        unit: CheckUnit::Stories,
    };
    let titus = CheckListRef {
        tool: Tool::TranslationNotes,
        unit: CheckUnit::Book(BookId::Titus),
    };
    let scope = ChangeScope::Parts(ChangeParts {
        checks: vec![titus, stories],
        ..ChangeParts::default()
    });
    let text = serde_json::to_string(&scope).unwrap();
    let back: ChangeScope = serde_json::from_str(&text).unwrap();
    assert_eq!(back, scope);
}
