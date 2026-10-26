//! Story and frame numbers, and the paths of an OBS project.

use serde_json::Value;
use tc_helps::ObsRef;

pub use tc_core::{FrameNo, StoryNo};

/// The number of stories in Open Bible Stories (tC4 `journal/story.mjs:22`).
pub const STORY_COUNT: u8 = StoryNo::COUNT;

/// The tc-helps and tc-check address of one frame of a story.
#[expect(
    clippy::unreachable,
    reason = "the story is 1 to 50 by construction, which is all ObsRef::new asks"
)]
pub fn obs_ref(story: StoryNo, frame: FrameNo) -> ObsRef {
    ObsRef::new(story.get(), frame.get())
        .unwrap_or_else(|| unreachable!("story {story} is 1 to 50"))
}

/// The story and frame of a tc-helps address.
#[expect(
    clippy::unreachable,
    reason = "an ObsRef's story is 1 to 50 by construction"
)]
pub fn story_frame(at: ObsRef) -> (StoryNo, FrameNo) {
    let story = StoryNo::new(at.story())
        .unwrap_or_else(|| unreachable!("an ObsRef's story {} is 1 to 50", at.story()));
    (story, FrameNo::new(at.frame()))
}

/// `content/NN.md`, the story's path below `ingredients/` (tC4 `storyIpath`,
/// `journal/story.mjs:37`; BURRITO-SPEC R-10.2.2).
pub fn story_ipath(story: StoryNo) -> String {
    format!("content/{:02}.md", story.get())
}

/// The two-digit number of a `content/NN.md` path, else `None` (tC4 `storyNumberOf`,
/// `src/data/journal/runtime.ts:154-157`). Like tC4 it answers 0 for `content/00.md` and
/// 51 to 99 for the paths above story 50, so a listing names those files and reading them
/// fails, rather than skipping them unseen.
pub fn story_number_of(ipath: &str) -> Option<u8> {
    let digits = ipath.strip_prefix("content/")?.strip_suffix(".md")?;
    let b = digits.as_bytes();
    (b.len() == 2 && b.iter().all(u8::is_ascii_digit)).then(|| (b[0] - b'0') * 10 + (b[1] - b'0'))
}

/// The story numbers of a project's file listing, in order (tC4 `listStoriesOf`,
/// `src/data/httpStore.ts:423-429`).
pub fn list_stories<'a>(ipaths: impl IntoIterator<Item = &'a str>) -> Vec<u8> {
    let mut out: Vec<u8> = ipaths.into_iter().filter_map(story_number_of).collect();
    out.sort_unstable();
    out
}

/// The OBS files the app never derives and never writes: copied from the gateway source
/// at creation, byte for byte (tC4 `isObsBaseIngredient`,
/// `src/data/journal/runtime.ts:169-170`; R-10.2.1).
pub fn is_obs_base_ingredient(ipath: &str) -> bool {
    ipath == "LICENSE.md"
        || ipath.starts_with("content/front/")
        || ipath.starts_with("content/back/")
}

/// Whether a `metadata.json` document is an OBS project's: flavor type `gloss`, flavor
/// `textStories` (tC4 `isObsMetadata`, `src/data/journal/runtime.ts:161-164`; R-10.1.1).
pub fn is_obs_metadata(metadata: &Value) -> bool {
    let flavor_type = &metadata["type"]["flavorType"];
    flavor_type["name"] == "gloss" && flavor_type["flavor"]["name"] == "textStories"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn story_paths_round_trip() {
        for story in StoryNo::all() {
            assert_eq!(story_number_of(&story_ipath(story)), Some(story.get()));
        }
        assert_eq!(story_ipath(StoryNo::new(7).unwrap()), "content/07.md");
        assert_eq!(story_number_of("content/00.md"), Some(0));
        assert_eq!(story_number_of("content/99.md"), Some(99));
        for other in [
            "content/1.md",
            "content/100.md",
            "content/0a.md",
            "x/content/01.md",
            "content/01.md.bak",
            "content/front/title.md",
        ] {
            assert_eq!(story_number_of(other), None, "{other}");
        }
    }
}
