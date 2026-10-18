//! How far an OBS project is drafted, and which story a project opens on (tC4
//! `src/state.jsx`, D74, #289, #328).

use serde_json::Value;
use tc_helps::obs::ObsStory;

use crate::js::{round, to_number};

/// A percentage of frames drafted. Any drafted frame shows as at least 1%: one frame of
/// the ~600 rounds to 0, and a first save must move the tile (`storyPercent`,
/// `state.jsx:921`).
pub fn story_percent(drafted: usize, frames: usize) -> u8 {
    if drafted == 0 || frames == 0 {
        return 0;
    }
    let percent = round(drafted as f64 / frames as f64 * 100.0);
    // `drafted` never exceeds `frames`, so the percentage is at most 100.
    (percent as u8).max(1)
}

/// One story's Home tile (#328).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryTile {
    pub number: u8,
    /// The drafted title, else the gateway story's title, else empty.
    pub title: String,
    /// `None` when the story could not be read: unknown, never a false 0% (D30).
    pub percent: Option<u8>,
    pub frames: usize,
    pub drafted: usize,
}

/// Per-story progress and the project's percentage over every frame of the fifty stories;
/// the title (frame 0) and the reference line do not count (`obsStoryProgress`,
/// `state.jsx:929-958`; `obsDraftPercent`, `:917-919`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObsProgress {
    /// `None` when a story could not be read, or the stories could not be listed.
    pub percent: Option<u8>,
    pub stories: Vec<StoryTile>,
}

impl ObsProgress {
    /// The progress of a project whose stories could not be listed.
    pub fn unknown() -> Self {
        Self {
            percent: None,
            stories: Vec::new(),
        }
    }
}

/// `stories` are the listed story numbers in order, each with the story as read, or
/// `None` when the read failed. `gateway_title` gives the gateway story's title for a
/// story with no drafted title, or `None` when it cannot.
pub fn obs_progress<'a>(
    stories: impl IntoIterator<Item = (u8, Option<&'a ObsStory>)>,
    mut gateway_title: impl FnMut(u8) -> Option<String>,
) -> ObsProgress {
    let mut tiles = Vec::new();
    let (mut frames, mut drafted, mut unknown) = (0, 0, false);
    for (number, story) in stories {
        let Some(story) = story else {
            unknown = true;
            tiles.push(StoryTile {
                number,
                title: String::new(),
                percent: None,
                frames: 0,
                drafted: 0,
            });
            continue;
        };
        let done = story.frames.iter().filter(|f| !f.text.is_empty()).count();
        frames += story.frames.len();
        drafted += done;
        let title = if story.title.is_empty() {
            gateway_title(number).unwrap_or_default()
        } else {
            story.title.clone()
        };
        tiles.push(StoryTile {
            number,
            title,
            percent: Some(story_percent(done, story.frames.len())),
            frames: story.frames.len(),
            drafted: done,
        });
    }
    ObsProgress {
        percent: (!unknown).then(|| story_percent(drafted, frames)),
        stories: tiles,
    }
}

/// JavaScript's `Number(value)` for a JSON value: what the app compares with the story
/// numbers of a listing.
fn js_number(value: &Value) -> f64 {
    match value {
        Value::Null => 0.0,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => to_number(s),
        Value::Array(items) => match items.as_slice() {
            [] => 0.0,
            [Value::Null] => 0.0,
            [one @ (Value::Number(_) | Value::String(_) | Value::Bool(_))] => match one {
                Value::Bool(b) => to_number(if *b { "true" } else { "false" }),
                other => js_number(other),
            },
            _ => f64::NAN,
        },
        Value::Object(_) => f64::NAN,
    }
}

fn listed(numbers: &[u8], n: f64) -> Option<u8> {
    numbers
        .iter()
        .copied()
        .find(|&number| f64::from(number) == n)
}

/// The story an OBS project opens on: the story a Home tile names, else the story this
/// computer last had open in the project, else the first listed; `None` when none is
/// listed (`openProjectContent`, `state.jsx:2031-2034`). A book code is never read as a
/// story: `GEN` names no listed story, so the open falls back.
pub fn first_story(numbers: &[u8], asked: Option<&str>, remembered: Option<&Value>) -> Option<u8> {
    let asked = asked.map_or(f64::NAN, to_number);
    let remembered = remembered.map_or(f64::NAN, js_number);
    listed(numbers, asked)
        .or_else(|| listed(numbers, remembered))
        .or_else(|| numbers.first().copied())
}

/// The story an open request may open: an integer that the listing holds, or any story
/// number while the listing is empty (`obsStoryOpenContext`, `state.jsx:2052-2061`).
pub fn story_to_open(numbers: &[u8], asked: &Value) -> Option<u8> {
    let n = js_number(asked);
    if n.fract() != 0.0 || !n.is_finite() {
        return None;
    }
    if numbers.is_empty() {
        return (0.0..=255.0).contains(&n).then_some(n as u8);
    }
    listed(numbers, n)
}
