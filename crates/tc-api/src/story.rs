//! The `story` service's reads (APP-DESIGN §3.7, §3.9, unit B7): the stories of the open Open
//! Bible Stories project, and one story with its gateway story and its pictures. The story's
//! helps are `helps/story` (B3's `PassageView`, with the story as the chapter and the frame as
//! the verse); its check lists are `check/*` over `CheckUnit::Story`. The saves (`saveFrame`,
//! `saveRef`) come with C3.
//!
//! Frame 0 is the story's title. A story's number, its title line and its frames' image lines
//! are the project's file; this service shows them and writes nothing.

use serde::{Deserialize, Serialize};

use crate::ids::{Scope, StoryNo, WallMs};
use crate::shown::Known;

/// `story/list`: the stories the project holds, as the story rail lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {
    pub at: Scope,
}

/// The stories in number order. Each row carries what Home's summary knows of the story;
/// `story/open` has the open story's own, which the rail prefers (tC4's `StoryRail`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryList {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub stories: Vec<StoryRow>,
}

/// One row of the story rail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryRow {
    pub story: StoryNo,
    /// The drafted title, else the pinned gateway story's when it is on this computer
    /// (tC4 #328); empty when neither is known.
    pub title: String,
    /// The share of the story's frames drafted; unknown until Home's summary has read it.
    pub drafted: Known<u8>,
}

/// `story/open`: one story of the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub at: Scope,
    pub story: StoryNo,
}

/// A story as the story screens show it (tC4's `readObsStoryPresentation`): the project's
/// story, the pinned gateway story beside it and each frame's picture. Reading it writes
/// nothing, neither gateway text nor pictures, into the project's story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryView {
    pub story: StoryNo,
    /// The drafted title (frame 0); empty when none is drafted.
    pub title: String,
    /// The reference line's text between its underscores; empty when there is none.
    pub reference: String,
    /// Frames 1 to n, in file order.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub frames: Vec<FrameView>,
    /// The pinned gateway story, when it is on this computer and holds this story.
    pub gateway: Option<GatewayStory>,
    /// Why the gateway story is absent or unusable; none when it was read and matches.
    pub source: Option<SourceNotice>,
    /// The version of the project's `obs` pin (`v9`), named under the source column's head
    /// whatever the gateway story's state.
    pub source_version: Option<String>,
    /// Set only when the story has image lines and no picture was found for any.
    pub image_note: Option<ImageNote>,
    /// The user comments on the story's frames, oldest first.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub comments: Vec<FrameComment>,
}

/// One frame of the project's story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FrameView {
    /// The frame's paragraph; empty when it is not drafted.
    pub text: String,
    /// Where the frame's picture is served (`/media/obs/…`); none when no picture was found
    /// (a missing picture is never a frame error, tC4 #289).
    pub image: Option<String>,
}

/// The gateway story's text: the source column of Translate and Understand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct GatewayStory {
    pub title: String,
    pub reference: String,
    /// One text per frame, as many as the gateway story holds.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub frames: Vec<String>,
}

/// Why the gateway story is absent or unusable (tC4's `ObsSourceState`). Only `NotInstalled`
/// earns the offer to get it; a mismatch keeps the gateway story and each frame its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SourceNotice {
    /// The project pins no gateway story.
    NoPin,
    /// The pinned gateway story is not on this computer: `repo@commit`.
    NotInstalled { pin: String },
    /// The gateway story's frames are not the project's.
    Mismatch { message: String },
    /// The gateway story could not be read.
    Error { message: String },
}

/// Why no frame has a picture: how many frames wanted one, and what each pack consulted held
/// (tC4's `ObsImageNote`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ImageNote {
    pub wanted: u32,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub packs: Vec<PackReport>,
}

/// One picture pack consulted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PackReport {
    /// `repo@commit`.
    pub pin: String,
    /// Where the pack is installed on this computer; none when it is not.
    pub installed_at: Option<String>,
    /// How its files were listed; none when they could not be.
    pub via: Option<PackVia>,
    pub files: u32,
    pub error: Option<String>,
}

/// Where a pack's file names came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum PackVia {
    /// The real file listing.
    Paths,
    /// The ingredient table of its `metadata.json`, read because the listing could not be.
    Metadata,
}

/// A user comment on a frame (0 is the title).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FrameComment {
    pub frame: u16,
    pub text: String,
    pub at: WallMs,
}
