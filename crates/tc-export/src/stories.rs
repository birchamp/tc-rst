//! The print of an Open Bible Stories project (tC4 #360, #454, layouts #11), over tc-obs'
//! print model: [`tc_obs::print::printed`] says which stories and frames print and finds
//! the pictures of the drafted frames; [`print_stories`] says what each shows under a page
//! setup (`src/views/print/PrintStories.jsx`).
//!
//! Each printed story opens with its title on a page of its own. Then each drafted frame is
//! its picture and its text, and each run of undrafted frames is one line with no picture.
//! Then the reference line, when the story has one. A run of undrafted stories between two
//! printed ones is one line. With no story drafted, the print is one line that says so.

use std::collections::BTreeMap;

use tc_obs::print::{PrintedFrame, PrintedStory};

use crate::{Direction, PrintText, Run};
use tc_api::model::print::{ObsLayout, PageSetup, Pictures};

/// A physical side of the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A frame's picture. `float` is the side it sits at in the wrapped layout, the frame's
/// start corner: left for a left-to-right project, right for right-to-left. It is
/// physical, so the print states it rather than leaving it to the text direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FramePicture<'a> {
    pub src: &'a str,
    /// [`PrintText::FramePicture`].
    pub alt: PrintText,
    pub float: Option<Side>,
}

/// A drafted frame: its picture, when it has one and pictures are on, above or beside its
/// text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameView<'a> {
    /// 1 is the story's first frame.
    pub frame: usize,
    pub picture: Option<FramePicture<'a>>,
    pub text: &'a str,
}

/// A block of a printed story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBlock<'a> {
    /// The line for a run of undrafted frames, [`PrintText::FramesNotDrafted`].
    Gap(PrintText),
    Frame(FrameView<'a>),
}

/// A story's title: its own, or [`PrintText::StoryNumber`] when it has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoryTitle<'a> {
    Own(&'a str),
    Numbered(PrintText),
}

/// A printed story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoryView<'a> {
    pub number: u8,
    pub title: StoryTitle<'a>,
    pub frames: Vec<FrameBlock<'a>>,
    /// The reference line, when the story has one with text.
    pub reference: Option<&'a str>,
}

/// A block of the print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoryBlock<'a> {
    /// The line for a run of undrafted stories, [`PrintText::StoriesNotDrafted`].
    Gap(PrintText),
    Story(StoryView<'a>),
}

/// An OBS project's print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoriesView<'a> {
    pub dir: Direction,
    pub layout: ObsLayout,
    /// Empty when no story is drafted: see [`StoriesView::nothing_drafted`].
    pub blocks: Vec<StoryBlock<'a>>,
}

impl StoriesView<'_> {
    /// The one line the print has when no story is drafted.
    pub fn nothing_drafted(&self) -> Option<PrintText> {
        self.blocks.is_empty().then_some(PrintText::NothingDrafted)
    }
}

/// The pictures of the drafted frames of each printed story, by story number and then
/// frame number (1 is the first frame), as [`tc_obs::print::Printed`] holds them.
pub type StoryPictures = BTreeMap<u8, BTreeMap<usize, String>>;

/// What `items` print under `setup` (`PrintStories`, `src/views/print/PrintStories.jsx:34-60`).
/// A drafted frame whose picture `pictures` does not hold, or holds as an empty source,
/// prints no picture; with pictures off, none does.
pub fn print_stories<'a>(
    items: &'a [PrintedStory<'_>],
    pictures: &'a StoryPictures,
    setup: &PageSetup,
    dir: Direction,
) -> StoriesView<'a> {
    let float = match setup.obs_layout {
        ObsLayout::Above => None,
        ObsLayout::Wrapped => Some(match dir {
            Direction::Ltr => Side::Left,
            Direction::Rtl => Side::Right,
        }),
    };
    let blocks = items
        .iter()
        .map(|item| match item {
            PrintedStory::Gap(from, to) => StoryBlock::Gap(PrintText::StoriesNotDrafted(Run {
                from: *from,
                to: *to,
            })),
            PrintedStory::Story { story, frames } => {
                let sources = (setup.pictures == Pictures::Shown)
                    .then(|| pictures.get(&story.number))
                    .flatten();
                StoryBlock::Story(StoryView {
                    number: story.number,
                    title: if story.title.is_empty() {
                        StoryTitle::Numbered(PrintText::StoryNumber {
                            story: story.number,
                        })
                    } else {
                        StoryTitle::Own(&story.title)
                    },
                    frames: frames
                        .iter()
                        .map(|frame| frame_block(frame, sources, float))
                        .collect(),
                    reference: story.reference.as_deref().filter(|r| !r.is_empty()),
                })
            }
        })
        .collect();
    StoriesView {
        dir,
        layout: setup.obs_layout,
        blocks,
    }
}

/// A frame of a printed story. `sources` are the story's pictures, or `None` with pictures
/// off.
fn frame_block<'a>(
    frame: &'a PrintedFrame,
    sources: Option<&'a BTreeMap<usize, String>>,
    float: Option<Side>,
) -> FrameBlock<'a> {
    match frame {
        PrintedFrame::Gap(from, to) => FrameBlock::Gap(PrintText::FramesNotDrafted(Run {
            from: *from,
            to: *to,
        })),
        PrintedFrame::Drafted { n, text } => FrameBlock::Frame(FrameView {
            frame: *n,
            picture: sources
                .and_then(|sources| sources.get(n))
                .filter(|src| !src.is_empty())
                .map(|src| FramePicture {
                    src,
                    alt: PrintText::FramePicture { frame: *n },
                    float,
                }),
            text,
        }),
    }
}
