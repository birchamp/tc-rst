//! The print model of an OBS project: what the Community Checking preview and the OBS PDF
//! show, by one rule (tC4 `src/data/storyModel.ts`, #454).

use std::collections::BTreeMap;

use tc_helps::obs::ObsStory;

/// A drafted frame (`n` counts from 1), or one run of undrafted frames `from`–`to`
/// (`PrintedFrame`, `storyModel.ts:9`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintedFrame {
    Drafted { n: usize, text: String },
    Gap(usize, usize),
}

/// A drafted story with its frames, or one run of undrafted stories `from`–`to`
/// (`PrintedStory`, `storyModel.ts:12`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrintedStory<'a> {
    Story {
        story: &'a ObsStory,
        frames: Vec<PrintedFrame>,
    },
    Gap(u8, u8),
}

/// A story is drafted when its title, a frame or its reference line has text; the seed
/// form of each is empty (`storyDrafted`, `storyModel.ts:20-21`; R-10.2.4).
pub fn story_drafted(story: &ObsStory) -> bool {
    !story.title.is_empty()
        || story.reference.as_deref().is_some_and(|r| !r.is_empty())
        || story.frames.iter().any(|frame| !frame.text.is_empty())
}

/// Each drafted frame, and one gap for each run of undrafted frames, at the start and the
/// end of the story too (`printedFrames`, `storyModel.ts:25-35`).
pub fn printed_frames(story: &ObsStory) -> Vec<PrintedFrame> {
    let mut out: Vec<PrintedFrame> = Vec::new();
    for (i, frame) in story.frames.iter().enumerate() {
        let n = i + 1;
        if !frame.text.is_empty() {
            out.push(PrintedFrame::Drafted {
                n,
                text: frame.text.clone(),
            });
        } else if let Some(PrintedFrame::Gap(_, to)) = out.last_mut() {
            *to = n;
        } else {
            out.push(PrintedFrame::Gap(n, n));
        }
    }
    out
}

/// What the preview and the PDF show, in the order given: each drafted story with its
/// frames, and one gap for each run of undrafted stories between two drafted ones.
/// Undrafted stories before the first drafted story or after the last are left out
/// (`printedStories`, `storyModel.ts:43-55`).
pub fn printed_stories(stories: &[ObsStory]) -> Vec<PrintedStory<'_>> {
    let drafted: Vec<bool> = stories.iter().map(story_drafted).collect();
    let (Some(first), Some(last)) = (
        drafted.iter().position(|d| *d),
        drafted.iter().rposition(|d| *d),
    ) else {
        return Vec::new();
    };
    let mut items: Vec<PrintedStory<'_>> = Vec::new();
    for (story, drafted) in stories[first..=last].iter().zip(&drafted[first..=last]) {
        if *drafted {
            items.push(PrintedStory::Story {
                story,
                frames: printed_frames(story),
            });
        } else if let Some(PrintedStory::Gap(_, to)) = items.last_mut() {
            *to = story.number;
        } else {
            items.push(PrintedStory::Gap(story.number, story.number));
        }
    }
    items
}

/// Every story of a project as printed items, in number order, with the picture sources
/// of the drafted frames of each printed story (`readPrintedStories`,
/// `storyModel.ts:65-83`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed<'a> {
    pub items: Vec<PrintedStory<'a>>,
    /// By story number, then frame number (1 is the first frame).
    pub pictures: BTreeMap<u8, BTreeMap<usize, String>>,
    /// Whether any frame of the project has no text.
    pub undrafted: bool,
}

/// One story's picture sources by frame number (1 is the first frame).
pub type PictureSources<'f> = &'f mut dyn FnMut(&ObsStory) -> BTreeMap<usize, String>;

/// `stories` must be in story number order, as the project lists them. `pictures` gives
/// one story's picture sources by frame number; it is asked only for printed stories with
/// a drafted frame, and only the drafted frames' sources are kept.
pub fn printed<'a>(
    stories: &'a [ObsStory],
    mut pictures: Option<PictureSources<'_>>,
) -> Printed<'a> {
    let items = printed_stories(stories);
    let undrafted = stories
        .iter()
        .any(|story| story.frames.iter().any(|frame| frame.text.is_empty()));
    let mut by_story = BTreeMap::new();
    if let Some(pictures) = pictures.as_mut() {
        for item in &items {
            let PrintedStory::Story { story, frames } = item else {
                continue;
            };
            let drafted: Vec<usize> = frames
                .iter()
                .filter_map(|f| match f {
                    PrintedFrame::Drafted { n, .. } => Some(*n),
                    PrintedFrame::Gap(..) => None,
                })
                .collect();
            if drafted.is_empty() {
                continue;
            }
            let mut sources = pictures(story);
            let kept = drafted
                .into_iter()
                .filter_map(|n| sources.remove(&n).map(|s| (n, s)))
                .filter(|(_, s)| !s.is_empty())
                .collect();
            by_story.insert(story.number, kept);
        }
    }
    Printed {
        items,
        pictures: by_story,
        undrafted,
    }
}
