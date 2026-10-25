//! The numbers of a place in a text: chapter, verse, story and frame. Each is defined here
//! once, so every crate holds the same value and none can be built out of range.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::num::NonZeroU16;

/// A chapter number, 1 to 65,535.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct ChapterNo(NonZeroU16);

/// A verse number, 1 to 65,535.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct VerseNo(NonZeroU16);

macro_rules! nonzero_number {
    ($name:ident) => {
        impl $name {
            /// `None` for 0.
            pub fn new(n: u16) -> Option<Self> {
                NonZeroU16::new(n).map(Self)
            }

            pub fn get(self) -> u16 {
                self.0.get()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}
nonzero_number!(ChapterNo);
nonzero_number!(VerseNo);

/// An Open Bible Stories story, 1 to 50 (tC4 `storyNumberError`, `journal/story.mjs:28-31`).
/// On the wire it is a number.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct StoryNo(u8);

/// A number that is not a story.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a story from 1 to 50")]
pub struct NotAStory(pub u8);

impl StoryNo {
    /// The number of stories in Open Bible Stories (tC4 `journal/story.mjs:22`).
    pub const COUNT: u8 = 50;

    /// `None` outside 1 to 50.
    pub fn new(n: u8) -> Option<Self> {
        (1..=Self::COUNT).contains(&n).then_some(Self(n))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// Every story, in order.
    pub fn all() -> impl Iterator<Item = StoryNo> {
        (1..=Self::COUNT).map(StoryNo)
    }
}

impl TryFrom<u8> for StoryNo {
    type Error = NotAStory;
    fn try_from(n: u8) -> Result<Self, NotAStory> {
        Self::new(n).ok_or(NotAStory(n))
    }
}

impl From<StoryNo> for u8 {
    fn from(story: StoryNo) -> Self {
        story.0
    }
}

impl fmt::Display for StoryNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A frame of a story. Frame 0 is the story's title, by name ([`FrameNo::TITLE`]); frames 1
/// and up are its paragraphs (tC4 `frameNumberError`, `journal/story.mjs:32-35`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FrameNo(u16);

impl FrameNo {
    pub const TITLE: FrameNo = FrameNo(0);

    pub fn new(n: u16) -> Self {
        Self(n)
    }

    pub fn get(self) -> u16 {
        self.0
    }

    pub fn is_title(self) -> bool {
        self.0 == 0
    }
}

impl fmt::Display for FrameNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// tc-api's wire types derive `Arbitrary` for their property tests, and a story is one of them.
#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for StoryNo {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;
    fn arbitrary_with((): ()) -> Self::Strategy {
        use proptest::strategy::Strategy;
        (1..=StoryNo::COUNT).prop_map(StoryNo).boxed()
    }
}
