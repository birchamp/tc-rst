//! The `session` service (APP-DESIGN §3.9, unit B3): opening a project for the screens and
//! closing it at Home.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::app::OpenSession;
use crate::check::Tool;
use crate::home::ProjectKind;
use crate::ids::{ProjectRef, RequestId, Scope, StoryNo};
use crate::prefs::{DraftUnit, PlaceMode};
use crate::shown::Direction;

/// `session/open`: opens a project for the screens. A job: its `Opened` outcome carries the
/// session and what the screens need first. Opening any project stops the Home summary that
/// runs (§4.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub request: RequestId,
    pub project: ProjectRef,
    /// The tile the person chose, whose remembered place the outcome carries; none for the
    /// project's first book.
    pub unit: Option<OpenUnit>,
}

/// A book or a story of the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OpenUnit {
    Book(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))] BookId),
    Story(StoryNo),
}

/// What an open gives the screens: the session, the project's chip, its books and the place
/// the person last worked in the unit asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct OpenedProject {
    pub session: OpenSession,
    /// The name in the project's default language, as Home's card names it.
    pub name: String,
    /// The project language's tag.
    pub tag: String,
    /// The direction of the project's script.
    pub dir: Direction,
    pub kind: ProjectKind,
    /// A Bible's books in canonical order; empty for a story project.
    #[cfg_attr(
        feature = "proptest",
        proptest(strategy = "proptest::collection::vec(crate::arb::book(), 0..4)")
    )]
    pub books: Vec<BookId>,
    /// Section or Verse, as this computer last showed the project (Section until set).
    pub draft_unit: DraftUnit,
    /// The place kept for the unit asked for, if any: a Home tile returns there.
    pub place: Option<KeptPlace>,
}

/// A place a Home tile returns to: the mode, the chapter (or story) and the Check tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct KeptPlace {
    pub mode: PlaceMode,
    pub chapter: u16,
    /// The tool open when the person left Check (tC4's `resumeCheckTool`, B5a).
    pub tool: Option<Tool>,
}

/// `session/close`: the person went Home. The screen's session closes after its "leaving the
/// project" checkpoint, behind the reply (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Close {
    pub request: RequestId,
    pub at: Scope,
}
