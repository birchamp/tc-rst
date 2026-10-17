//! The frame set of a story collection: which stories it has and how many frames each
//! (tC4 `src/data/obsFrameSet.ts`, BURRITO-SPEC §10.7.5).

use tc_helps::obs::ObsStory;

/// The frame set: each story's number and frame count, by story number
/// (`obsFrameSet`, `obsFrameSet.ts:12-15`).
pub fn frame_set<'a>(stories: impl IntoIterator<Item = &'a ObsStory>) -> Vec<(u8, usize)> {
    let mut set: Vec<(u8, usize)> = stories
        .into_iter()
        .map(|story| (story.number, story.frames.len()))
        .collect();
    set.sort_by_key(|&(number, _)| number);
    set
}

/// Why a replacement source cannot stand for the current one, or `None` when it can.
/// Text, references and image addresses may differ; story numbers and frame counts may
/// not, because the project's frames have nowhere else to attach
/// (`obsFrameSetMismatch`, `obsFrameSet.ts:17-31`).
pub fn frame_set_mismatch<'a>(
    current: impl IntoIterator<Item = &'a ObsStory>,
    incoming: impl IntoIterator<Item = &'a ObsStory>,
) -> Option<String> {
    let before = frame_set(current);
    let after = frame_set(incoming);
    if before.len() != after.len() {
        return Some(format!(
            "source has {} stories; project has {}",
            after.len(),
            before.len()
        ));
    }
    for (&(number, frames), &(source_number, source_frames)) in before.iter().zip(&after) {
        if number != source_number {
            return Some(format!(
                "source story {source_number} does not match project story {number}"
            ));
        }
        if frames != source_frames {
            return Some(format!(
                "story {number} has {source_frames} source frames; project has {frames}"
            ));
        }
    }
    None
}
