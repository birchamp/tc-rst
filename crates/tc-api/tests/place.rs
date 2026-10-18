//! `prefs/recordPlace` carries every observation of the place that tC4 makes
//! (`tC4:src/state.jsx` `rememberPlace` and its callers; `tC4:test/place.test.ts`), so a Home
//! tile can return where tC4 returns.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use tc_api::check::Tool;
use tc_api::ids::{RefPart, StoryNo};
use tc_api::prefs::{PlaceChange, PlaceMode, PlaceUpdate, RecordPlace};
use tc_core::BookId;

fn record(place: Value) -> Value {
    json!({
        "at": { "project": "kr_ulb", "epoch": 3 },
        "request": "06".repeat(16),
        "place": place,
    })
}

/// The place a request carries, or why it does not read.
fn read(place: Value) -> Result<PlaceUpdate, String> {
    serde_json::from_value::<RecordPlace>(record(place))
        .map(|r| r.place)
        .map_err(|e| e.to_string())
}

fn verse(text: &str) -> PlaceChange<RefPart> {
    PlaceChange::Set(RefPart::parse(text).unwrap())
}

fn book(
    mode: Option<PlaceMode>,
    chapter: Option<u16>,
    verse: PlaceChange<RefPart>,
    tool: Option<Tool>,
) -> PlaceUpdate {
    PlaceUpdate::Book {
        book: BookId::Titus,
        mode,
        chapter,
        verse,
        tool,
    }
}

#[test]
fn an_edit_of_a_bridged_verse_records_its_key() {
    // `editVerse` records the verse key, `4-5` for a bridge (state.jsx:7191).
    let place = read(json!({
        "type": "book",
        "value": {"book": "TIT", "mode": "draft", "chapter": 2,
                  "verse": {"type": "set", "value": "4-5"}},
    }));
    assert_eq!(
        place,
        Ok(book(Some(PlaceMode::Draft), Some(2), verse("4-5"), None))
    );
}

#[test]
fn a_check_item_records_its_reference_verse_as_written() {
    // `recordCheckLastEdit` records `referenceParts(ref).v` (state.jsx:3874): an intro or
    // front note, a verse list or a letter verse (D60) as well as a number.
    for text in ["5", "intro", "front", "3,5", "1a", "9-10"] {
        let place = read(json!({
            "type": "book",
            "value": {"book": "TIT", "mode": "check", "chapter": 1,
                      "verse": {"type": "set", "value": text},
                      "tool": "translationNotes"},
        }));
        assert_eq!(
            place,
            Ok(book(
                Some(PlaceMode::Check),
                Some(1),
                verse(text),
                Some(Tool::TranslationNotes)
            )),
            "{text}"
        );
    }
}

#[test]
fn a_mode_switch_keeps_the_verse_and_a_chapter_click_clears_it() {
    // place.test.ts: `{ mode: 'read' }` keeps chapter 2 verse '5'; `{ chapter: 3, verse:
    // null }` clears the verse (`setChapter`, state.jsx:7150).
    let switch = read(json!({
        "type": "book",
        "value": {"book": "TIT", "mode": "read", "verse": {"type": "keep"}},
    }));
    assert_eq!(
        switch,
        Ok(book(Some(PlaceMode::Read), None, PlaceChange::Keep, None))
    );
    let click = read(json!({
        "type": "book",
        "value": {"book": "TIT", "chapter": 3, "verse": {"type": "clear"}},
    }));
    assert_eq!(click, Ok(book(None, Some(3), PlaceChange::Clear, None)));
}

#[test]
fn a_story_records_its_frame_and_a_frame_click_keeps_the_mode() {
    // place.test.ts: `{ verse: 4 }` on story 3; `rememberPlaceForStory` records frame 1;
    // frame 0 is the title.
    for frame in [4u16, 1, 0] {
        let place = read(json!({
            "type": "story",
            "value": {"story": 3, "frame": {"type": "set", "value": frame}},
        }));
        assert_eq!(
            place,
            Ok(PlaceUpdate::Story {
                story: StoryNo::new(3).unwrap(),
                mode: None,
                frame: PlaceChange::Set(frame),
                tool: None,
            })
        );
    }
}

#[test]
fn every_place_reads_back_as_written() {
    let places = [
        book(Some(PlaceMode::Draft), Some(2), verse("4-5"), None),
        book(
            Some(PlaceMode::Check),
            Some(1),
            verse("front"),
            Some(Tool::TranslationWords),
        ),
        book(None, Some(3), PlaceChange::Clear, None),
        PlaceUpdate::Story {
            story: StoryNo::new(50).unwrap(),
            mode: Some(PlaceMode::Check),
            frame: PlaceChange::Keep,
            tool: Some(Tool::TranslationNotes),
        },
    ];
    for place in places {
        let text = serde_json::to_value(&place).unwrap();
        assert_eq!(read(text.clone()), Ok(place), "{text}");
    }
}

#[test]
fn a_place_that_is_not_one_refuses() {
    for bad in [
        // A verse that holds the chapter, or none at all.
        json!({"type": "book", "value": {"book": "TIT", "verse": {"type": "set", "value": "1:4"}}}),
        json!({"type": "book", "value": {"book": "TIT", "verse": {"type": "set", "value": ""}}}),
        // A story is one of the fifty.
        json!({"type": "story", "value": {"story": 0, "frame": {"type": "keep"}}}),
        json!({"type": "story", "value": {"story": 51, "frame": {"type": "keep"}}}),
        // A story's frame is a number, and a book has a verse, not a frame.
        json!({"type": "story", "value": {"story": 3, "frame": {"type": "set", "value": "4"}}}),
        json!({"type": "story", "value": {"story": 3, "verse": {"type": "set", "value": "4"}, "frame": {"type": "keep"}}}),
        json!({"type": "book", "value": {"book": "TIT", "frame": {"type": "keep"}, "verse": {"type": "keep"}}}),
        // The verse always says what happens to it.
        json!({"type": "book", "value": {"book": "TIT", "mode": "read"}}),
    ] {
        assert!(read(bad.clone()).is_err(), "{bad}");
    }
}

#[test]
fn a_place_refuses_a_field_it_does_not_know_wherever_it_stands() {
    // The envelope and body of the place and of the verse or frame inside it, for every
    // variant of both: an extra key must not be read and dropped.
    let places = [
        json!({"type": "book", "value": {"book": "TIT", "verse": {"type": "keep"}}}),
        json!({"type": "book", "value": {"book": "TIT", "verse": {"type": "clear"}}}),
        json!({"type": "book", "value": {"book": "TIT", "mode": "check", "chapter": 1,
               "verse": {"type": "set", "value": "4"}, "tool": "translationWords"}}),
        json!({"type": "story", "value": {"story": 3, "frame": {"type": "keep"}}}),
        json!({"type": "story", "value": {"story": 3, "frame": {"type": "clear"}}}),
        json!({"type": "story", "value": {"story": 3, "mode": "read",
               "frame": {"type": "set", "value": 4}}}),
    ];
    for place in places {
        let request = record(place);
        let accepted = common::unknown_key_accepted_in::<RecordPlace>(&request).unwrap();
        assert!(
            accepted.is_empty(),
            "{request} reads an unknown key in {accepted:?}"
        );
    }
}
