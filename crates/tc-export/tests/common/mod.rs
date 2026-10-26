// Each test file uses a different subset of these helpers, and test helpers may panic.
#![allow(
    dead_code,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test code: a failed unwrap is a failed test, and not every test binary uses each shared helper"
)]

pub mod markup;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde_json::Value;
use tc_api::model::print::{ChapterOpening, ObsLayout, PageSetup, Pictures, VerseNumbers};
use tc_export::Direction;
use tc_export::book::{BookChapter, BookVerse};
use tc_helps::obs::{ObsFrame, ObsImage, ObsStory};

pub fn repo_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

pub fn fixture(rel: &str) -> String {
    let path = repo_path("fixtures").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `fixtures/export/print-vectors.json`, made by `crates/tc-export/tools/gen-print-vectors.mjs`.
pub fn vectors() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(&fixture("export/print-vectors.json")).unwrap())
}

pub fn direction(v: &Value) -> Direction {
    match v.as_str().unwrap() {
        "ltr" => Direction::Ltr,
        "rtl" => Direction::Rtl,
        other => panic!("direction {other}"),
    }
}

/// The page setup of a variant: tC4's default with the variant's choices.
pub fn setup_of(variant: &Value) -> PageSetup {
    let mut setup = PageSetup::default();
    if let Some(drop_cap) = variant["dropCapChapters"].as_bool() {
        setup.chapter_opening = if drop_cap {
            ChapterOpening::DropCap
        } else {
            ChapterOpening::Heading
        };
    }
    if let Some(numbers) = variant["verseNumbers"].as_bool() {
        setup.verse_numbers = if numbers {
            VerseNumbers::Shown
        } else {
            VerseNumbers::Hidden
        };
    }
    if let Some(pictures) = variant["pictures"].as_bool() {
        setup.pictures = if pictures {
            Pictures::Shown
        } else {
            Pictures::Hidden
        };
    }
    if let Some(layout) = variant["obsLayout"].as_str() {
        setup.obs_layout = match layout {
            "above" => ObsLayout::Above,
            "wrapped" => ObsLayout::Wrapped,
            other => panic!("layout {other}"),
        };
    }
    setup
}

fn verse_of(v: &Value) -> BookVerse {
    BookVerse {
        label: v["n"].as_str().unwrap().into(),
        drafted: v["drafted"].as_bool().unwrap(),
        text: v["text"].as_str().unwrap().into(),
    }
}

/// A book case's book model as the rebuild holds it: chapters by number, in `chapterNums`
/// order. tC4 keys `byChapter` by the `\c` token as written; a chapter's number is that
/// token read as a number, so `\c 01` is chapter 1.
pub fn chapters_of(case: &Value) -> Vec<BookChapter> {
    let by_chapter = case["byChapter"].as_array().unwrap();
    case["chapterNums"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            let number = u16::try_from(c.as_u64().unwrap()).unwrap();
            let verses = by_chapter
                .iter()
                .filter(|entry| entry["key"].as_str().unwrap().parse::<u16>().ok() == Some(number))
                .flat_map(|entry| entry["verses"].as_array().unwrap().iter().map(verse_of))
                .collect();
            BookChapter { number, verses }
        })
        .collect()
}

/// A story of a story case, with a picture line tc-export never reads.
pub fn story_of(v: &Value) -> ObsStory {
    ObsStory {
        number: u8::try_from(v["number"].as_u64().unwrap()).unwrap(),
        title: v["title"].as_str().unwrap().into(),
        frames: v["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|text| ObsFrame {
                image: ObsImage {
                    alt: "OBS Image".into(),
                    url: String::new(),
                },
                text: text.as_str().unwrap().into(),
            })
            .collect(),
        reference: v["ref"].as_str().map(str::to_owned),
    }
}

/// The pictures the generator gives a story's frames (`pictureSources` in
/// `gen-print-vectors.mjs`).
pub fn picture_sources(story: &ObsStory) -> BTreeMap<usize, String> {
    (1..=story.frames.len())
        .filter(|n| n % 4 != 0)
        .map(|n| {
            let src = if n % 5 == 0 {
                String::new()
            } else {
                format!("/media/obs/{}-{n}.jpg?pin=a&size=\"360\"", story.number)
            };
            (n, src)
        })
        .collect()
}
