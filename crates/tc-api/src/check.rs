//! The parts of the Check screen's types that a write's `Ack` and a change scope carry
//! (APP-DESIGN §3.7, AD17). Unit B5a adds the session and item views.
//!
//! A check is not always at `chapter:verse` with numbers. tC4's resources put a Psalms
//! superscription at verse `front`, a note on several verses at `1,3,8,12`, a note on a
//! book's introduction at `front:intro`, and an Open Bible Stories check at a story and a
//! frame (`tC4:src/data/derive.ts` `referenceParts`). [`CheckPlace`] and [`CheckUnit`] carry
//! all of them, as tc-check's `CheckTarget` and `Collection` do.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{IdError, RefPart, StoryNo};
use crate::shown::ConflictMarker;

/// A checking tool, by tC4's tool id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    TranslationNotes,
    TranslationWords,
}

/// What one check list covers: a book, or the fifty stories of Open Bible Stories
/// (tc-check `Collection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CheckUnit {
    Book(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))] BookId),
    Stories,
}

/// One tool's check list for one book, or for the stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CheckListRef {
    pub tool: Tool,
    pub unit: CheckUnit,
}

/// Where a check is (tc-check `CheckTarget`): a verse of a book, with the chapter and the
/// verse as the resource writes them, or a frame of a story.
///
/// On the wire it is text, tC4's `locatorOf` with the book before it, as tc-check prints a
/// target: `PSA 60:front`, `PSA 5:1,3,8,12`, `TIT front:intro`, `OBS 3:4`. A rail holds
/// thousands of these, and an object for each would make the Psalms tN rail about a quarter
/// larger (2.1 MB for 1.65 MB of JSON, on the 7,969 rows of `psalms-vectors.json`).
/// The book is a USFM code, the parts are [`RefPart`]s, and a story's check is `OBS` with
/// the story and the frame as plain numbers (frame 0 is the story's title). The text is
/// read back at the boundary; one that is none of these is refused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum CheckPlace {
    /// `chapter:verse`. Either part can be text: `front`, `intro`, `9-10`, `3,5`, `1a`.
    Verse {
        book: BookId,
        chapter: RefPart,
        verse: RefPart,
    },
    Frame {
        story: StoryNo,
        frame: u16,
    },
}

impl CheckPlace {
    /// The text of a story's check in place of a book's code. No book has this code.
    const STORIES: &'static str = "OBS";
}

impl fmt::Display for CheckPlace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Verse {
                book,
                chapter,
                verse,
            } => write!(f, "{book} {chapter}:{verse}"),
            Self::Frame { story, frame } => {
                write!(f, "{} {}:{frame}", Self::STORIES, story.get())
            }
        }
    }
}

impl FromStr for CheckPlace {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        const WHAT: &str = "check place";
        let refuse = |reason| IdError { what: WHAT, reason };
        let (head, locator) = text
            .split_once(' ')
            .ok_or(refuse("expected `BOOK chapter:verse` or `OBS story:frame`"))?;
        let (first, second) = locator
            .split_once(':')
            .ok_or(refuse("no `:` between the parts"))?;
        if head == Self::STORIES {
            let story = canonical_number::<u8>(first).ok_or(refuse("a story is a number"))?;
            let frame = canonical_number::<u16>(second).ok_or(refuse("a frame is a number"))?;
            return Ok(Self::Frame {
                story: StoryNo::new(story)?,
                frame,
            });
        }
        Ok(Self::Verse {
            book: head.parse().map_err(|_| refuse("not a book"))?,
            chapter: RefPart::parse(first)?,
            verse: RefPart::parse(second)?,
        })
    }
}

/// Digits only and no leading zero, so that one number has one text.
fn canonical_number<T: FromStr>(text: &str) -> Option<T> {
    let digits = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    (digits && (text == "0" || !text.starts_with('0')))
        .then(|| text.parse().ok())
        .flatten()
}

impl TryFrom<String> for CheckPlace {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        text.parse()
    }
}

impl From<CheckPlace> for String {
    fn from(place: CheckPlace) -> Self {
        place.to_string()
    }
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for CheckPlace {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;
    fn arbitrary_with((): ()) -> Self::Strategy {
        use proptest::prelude::*;
        prop_oneof![
            (crate::arb::book(), any::<RefPart>(), any::<RefPart>()).prop_map(
                |(book, chapter, verse)| Self::Verse {
                    book,
                    chapter,
                    verse
                }
            ),
            (any::<StoryNo>(), any::<u16>())
                .prop_map(|(story, frame)| Self::Frame { story, frame }),
        ]
        .boxed()
    }
}

/// The group a check is listed under in By category (tc-check `Category`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Category {
    Note(NoteCategory),
    Word(WordCategory),
}

/// A translationNotes category, as tC4 names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum NoteCategory {
    Discourse,
    Numbers,
    Figures,
    Culture,
    Grammar,
    Other,
}

/// The folder of a translationWords article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum WordCategory {
    Kt,
    Names,
    Other,
}

/// The status dot of a rail row. An invalidated check shows red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Dot {
    Valid,
    Invalid,
    Todo,
}

/// What the rail's filter chips read from a row. Each field is a separate fact a chip
/// tests; together they are not one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RowFlags {
    pub dot: Dot,
    /// tC4's `isDecided` (tc-check `CheckState::is_decided`): progress and "To do" read it.
    pub decided: bool,
    pub bookmarked: bool,
    pub commented: bool,
}

/// One row of the check rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailRow {
    /// The check's place in the session's list.
    pub index: u32,
    pub place: CheckPlace,
    pub title: String,
    pub flags: RowFlags,
    /// The tW term, as an index into the session's terms.
    pub term: Option<u32>,
    pub category: Option<Category>,
    pub marker: Option<ConflictMarker>,
}

/// A rail row a write or a re-assessment changed. It replaces the row with the same index,
/// so the UI patches its rail instead of fetching it again (AD17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailRowPatch {
    pub row: RailRow,
}
