//! Home (APP-DESIGN §3.7, §4.5): the project cards and their states (A-30), and the reads of
//! Home and its dialogs (unit B2).

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::Tool;
use crate::ids::{ProjectRef, RefPart, StoryNo, WallMs};
use crate::prefs::PlaceMode;
use crate::refusal::Message;
use crate::shown::{Direction, Known};

/// How the project came to this computer: an imported project carries the "Imported"
/// badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ProjectOrigin {
    Created,
    Imported,
}

/// Whether the project is on Door43. A shared project, Bible or OBS, offers "Sync with
/// Door43" (PLAN, Q2); one that is not offers Share.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Door43Card {
    Local,
    Shared { place: RepoPlace },
}

/// A Door43 repository: `owner/name`, on `server` when that is not the production server.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RepoPlace {
    pub owner: String,
    pub name: String,
    /// The host, set only when the app talks to a server other than production and the
    /// repository is on it (a development build on the QA server, tC4 #506).
    pub server: Option<String>,
}

// ---------------------------------------------------------------------------------------
// Home (unit B2): `home/list`, `home/card`, `home/versifications`, `home/languages`
// ---------------------------------------------------------------------------------------

/// `home/list`: the whole Home, read once per Home visit (`AppTopic::Projects`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct List {}

/// Home: the cards newest first, the folders that cannot be opened, and the Resume record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HomeView {
    pub cards: Vec<HomeCard>,
    /// Folders that hold a project's files but cannot be opened, by name (§4.5, AD12): a
    /// declared difference from tC4, whose list leaves them off.
    pub unusable: Vec<UnusableCard>,
    pub resume: Option<LastEditView>,
}

/// `home/card`: one card, read by each card when it is drawn (`AppTopic::HomeCard`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Card {
    pub project: ProjectRef,
}

/// A project's card on Home. Every count the server could not compute yet is `Unknown`, so
/// the card shows "—", never a false 0 (tC4 D30).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct HomeCard {
    pub project: ProjectRef,
    pub name: String,
    /// The language tag; `Unknown` when `metadata.json` could not be read.
    pub tag: Known<String>,
    pub dir: Direction,
    pub kind: ProjectKind,
    pub door43: Door43Card,
    /// A Bible's books in canonical order with their progress; `Known(vec![])` for a story
    /// project, `Unknown` when `metadata.json` could not be read.
    pub books: Known<Vec<BookTileView>>,
    /// The fifty stories of a story project.
    pub stories: Option<StoriesView>,
    /// The verse conflicts of a Bible project (tC4 #679). Unknown or 0: no chip.
    pub conflicts: Known<u32>,
}

/// What a project holds: a Bible's books, or Open Bible Stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ProjectKind {
    Bible,
    Stories,
}

/// One book tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BookTileView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    /// The drafted percentage (tC4 #572).
    pub drafted: Known<u8>,
    /// The book's verse conflicts; 0 while unknown (no line).
    pub conflicts: u32,
}

/// A story project's stories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoriesView {
    /// The project's drafted percentage.
    pub drafted: Known<u8>,
    /// Every story, in number order; empty until the stories are read.
    pub stories: Vec<StoryTileView>,
    /// The stories this computer edited most recently, newest first.
    pub recent: Vec<RecentStory>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryTileView {
    pub story: StoryNo,
    /// The story's title: the drafted one, else the gateway's; empty when neither is known.
    pub title: String,
    pub drafted: Known<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RecentStory {
    pub story: StoryNo,
    pub at: WallMs,
}

/// A folder that holds a project's files but cannot be opened, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UnusableCard {
    /// The folder's name, with any byte that is not UTF-8 replaced.
    pub name: String,
    pub reason: UnusableReason,
}

/// Why a folder that holds a project's files cannot be opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UnusableReason {
    /// The folder holds a project's files but no `ingredients/tc/project.json`.
    NoProjectFile,
    /// Its `project.json` could not be read.
    ProjectFileUnreadable { reason: String },
    /// Its `project.json` is one the store refuses.
    ProjectFileDamaged { reason: String },
    /// A link to a folder elsewhere.
    Link,
    /// A name the app cannot carry (`:`, `\`, `/`, or only dots and spaces).
    NameNotAllowed,
    /// A name that is not UTF-8.
    NameNotText,
    /// The folder's files could not be read for its card.
    FolderUnreadable { reason: String },
}

impl Message for UnusableReason {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NoProjectFile => "home.unusable.noProjectFile",
            Self::ProjectFileUnreadable { .. } => "home.unusable.projectFileUnreadable",
            Self::ProjectFileDamaged { .. } => "home.unusable.projectFileDamaged",
            Self::Link => "home.unusable.link",
            Self::NameNotAllowed => "home.unusable.nameNotAllowed",
            Self::NameNotText => "home.unusable.nameNotText",
            Self::FolderUnreadable { .. } => "home.unusable.folderUnreadable",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::ProjectFileUnreadable { reason }
            | Self::ProjectFileDamaged { reason }
            | Self::FolderUnreadable { reason } => vec![("reason", reason.clone())],
            Self::NoProjectFile | Self::Link | Self::NameNotAllowed | Self::NameNotText => {
                Vec::new()
            }
        }
    }
}

/// The Resume card: the last place a change was made on this computer, shown only while its
/// project is on Home and holds that place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LastEditView {
    pub project: ProjectRef,
    pub place: ResumePlace,
    pub mode: PlaceMode,
    pub tool: Option<Tool>,
    /// The start of the text, for the card; empty for a cleared verse.
    pub snippet: String,
}

/// Where the last change was made. tC4 records a story's number as the chapter and its
/// frame as the verse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ResumePlace {
    Book {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
        book: BookId,
        chapter: u16,
        verse: RefPart,
    },
    Story {
        story: StoryNo,
        frame: u16,
    },
}

/// `home/versifications`: the versifications New Bible offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Versifications {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct VersificationsView {
    /// The standard versifications' names, `eng` first.
    pub names: Vec<String>,
}

/// `home/languages`: the languages that match what the person typed (AD27, tC4 #492),
/// ranked on the server from the list the app ships. Nothing goes to the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Languages {
    pub query: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LanguagesView {
    /// Best first, 100 at most.
    pub suggestions: Vec<LanguageSuggestion>,
}

/// One suggested language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LanguageSuggestion {
    /// The anglicized name, or the language's own name where Door43 has none.
    pub name: String,
    /// The language's own name, when it differs from `name`.
    pub own: Option<String>,
    /// The code the Code box gets: a language tag a project can hold. `None` when none can be
    /// made from Door43's code (owner's answer to Q35).
    pub code: Option<String>,
    /// Door43's code, when it differs from `code`.
    pub door43: Option<String>,
    pub dir: Direction,
}
