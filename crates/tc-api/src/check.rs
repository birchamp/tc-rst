//! The Check screen's types (APP-DESIGN §3.7, AD17): the parts a write's `Ack` and a change
//! scope carry (unit A1), and the tool picker, the rail and the guided fix (unit B5a). B5b adds
//! the item view.
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

use crate::ids::{CheckSessionId, IdError, RefPart, Scope, StoryNo, ViewSeq};
use crate::shown::ConflictMarker;

/// A checking tool, by tC4's tool id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    TranslationNotes,
    TranslationWords,
}

/// What one check list covers: a book, or one story of Open Bible Stories. A story's session
/// is scoped to the open story, as a book's is to the open book (tC4 `deriveObsItems`); its
/// decisions are the collection `Stories`'s (tc-check `Collection`), one file for the fifty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum CheckUnit {
    Book(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))] BookId),
    Story(StoryNo),
}

/// One tool's check list for one book, or for one story.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
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
    /// The decision no longer holds (tC4's `invalidated`): the dot is red whatever the status.
    pub invalidated: bool,
    /// The status shown is Invalid (tC4's `status === 'invalid'`), what the Invalid chip
    /// tests: an invalidated decision the person set to To do shows a red dot and is not
    /// under Invalid.
    pub invalid: bool,
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
    /// tC4's `checkId`: the help row the check comes from, which the row's `data-check-id`
    /// names.
    pub check_id: String,
    pub place: CheckPlace,
    pub title: String,
    pub flags: RowFlags,
    /// The check's group id (a tW term, a tN Translation Academy slug), as an index into the
    /// session's terms: By word groups by it, and the row's tooltip names it.
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

// ---------------------------------------------------------------------------------------
// The tool picker (unit B5a): `check/picker`
// ---------------------------------------------------------------------------------------

/// `check/picker`: the tool picker of a book or a story (tC4's `runPreflight` and
/// `loadPickerProgress`): whether each checking tool can open, and how far each has come.
/// Reading it writes nothing (tC4 #501). A tool's list is derived once and kept for its
/// session (`check/open`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Picker {
    pub at: Scope,
    pub unit: CheckUnit,
}

/// The picker's cards: the two helps tools in tC4's order (Translation Words, then
/// Translation Notes), then Align's, which a story has none of (tC4 D74 §8: the entry is
/// absent, not disabled).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PickerView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub tools: Vec<ToolCard>,
    pub align: Option<CardCount<AlignProgress>>,
    /// The project's pre-release words-links pin, named once above the cards (tC4
    /// `OldTwlNotice`, D97 ruling 6): its repository path.
    pub old_twl: Option<String>,
}

/// One helps tool's card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ToolCard {
    pub tool: Tool,
    pub state: ToolState,
    /// The pin the card cites (tC4 `ToolNeeds`): the one this computer lacks, else the one the
    /// tool resolved to.
    pub pin: Option<PinCitation>,
    /// The rung of the resolved pin.
    pub rung: Option<Rung>,
    /// The primary pin that is not on this computer while the tool opens on the English
    /// fallback (tC4 `unavailablePrimary`, B20): the card warns and offers to download it.
    pub fallback_for: Option<PinCitation>,
    /// How far the tool has come; only a ready tool has a count.
    pub progress: Option<CardCount<CheckProgress>>,
}

/// What opening a tool needs (tC4's preflight states).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ToolState {
    Ready,
    /// A pinned resource is not on this computer: download it (D88: always this, never
    /// "unavailable", since the download asks before it uses the internet).
    Fetch,
    /// No language set pins the tool.
    Unpinned,
    /// The pins do not cover the book.
    NotCovered,
    /// The words-links pin is the pre-release one-repository form: the project must be
    /// created again.
    OldTwl,
    /// The rung that answers holds a pin with no commit (D59): the pins must be repaired.
    /// tC4 has no such pin; its card reads as "No resources pinned".
    UnusablePin,
}

/// A pinned resource as the cards and the guided fix cite it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PinCitation {
    /// `git.door43.org/es-419_gl/es-419_twl`.
    pub repo_path: String,
    /// The release tag, when the pin names one.
    pub version: Option<String>,
    /// The commit, 40 hex digits.
    pub sha: String,
}

/// A rung of the language-set ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum Rung {
    Primary,
    Fallback,
}

/// A card's progress, or why it could not be counted (tC4 `progressError`): a failed count
/// never blocks opening the tool, and is never shown as 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CardCount<T> {
    Counted(T),
    Failed { reason: String },
}

/// A helps tool's count, from the same session the tool opens (tC4 `pickerEntryFromSession`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CheckProgress {
    pub decided: u32,
    pub total: u32,
    pub dropped: Option<DroppedNote>,
    /// The first check not decided.
    pub next: Option<NextCheck>,
}

/// "Next: “{title}” · {c}:{v}".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct NextCheck {
    pub title: String,
    pub place: CheckPlace,
}

/// The Align card's count (tC4 `alignPickerEntry`): drafted verses, the fully aligned ones,
/// and the first left to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct AlignProgress {
    pub done: u32,
    pub total: u32,
    /// `c:v` of the first drafted verse to do or to review again.
    pub next: Option<String>,
}

/// Checks the project's numbering has no verse for (tC4 #15): stated beside every count they
/// leave out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct DroppedNote {
    pub count: u32,
    /// The project's scheme, when it is known.
    pub scheme: Option<String>,
}

// ---------------------------------------------------------------------------------------
// A tool's session and its rail (unit B5a): `check/open`, `check/rows`
// ---------------------------------------------------------------------------------------

/// `check/open { since }`: a tool's check session (AD17). The rail arrives whole once; a UI
/// that still holds a rail of the same session names it and its view, and gets only the rows
/// changed since (A-12).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Open {
    pub at: Scope,
    pub list: CheckListRef,
    pub since: Option<RailSince>,
}

/// The rail a UI holds: its session and the view it is current to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RailSince {
    pub session: CheckSessionId,
    pub view: ViewSeq,
}

/// An open check session (tC4's `checkSession`). A new derivation of the list (another
/// resolved resource, numbering or scope) is a new session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CheckSessionView {
    pub session: CheckSessionId,
    pub list: CheckListRef,
    /// `check/rows { since: view }` patches the rail from here.
    pub view: ViewSeq,
    pub rows: RailRows,
    /// The group ids of the list's checks (tW terms, tN Translation Academy slugs), in the
    /// order By word lists them; a row's `term` indexes it.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub terms: Vec<String>,
    pub progress: RailProgress,
    /// The resource the list derives from (tC4 `cs.resource`).
    pub citation: Option<ResourceCitation>,
    pub dropped: Option<DroppedNote>,
    /// Set when the tool has no check to show (tC4 `CheckEmpty`).
    pub empty: Option<CheckEmpty>,
    /// The check the session opens at: the index of the remembered check, else the first not
    /// decided, else the first (tC4 `checkStartIndex`).
    pub start: u32,
}

/// The rail: every row in the order By verse lists them, or the rows changed since the view
/// the UI named.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RailRows {
    Full(#[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))] Vec<RailRow>),
    Since(
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
        Vec<RailRowPatch>,
    ),
}

/// Decided checks over the checks on the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RailProgress {
    pub decided: u32,
    pub total: u32,
}

/// What a session's list derives from: `{repo} · {version} · {rung}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ResourceCitation {
    pub pin: PinCitation,
    pub rung: Rung,
}

/// Why a tool shows no check (tC4 `CheckEmpty`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum CheckEmpty {
    /// The resource lists no check in the book.
    None,
    /// Every check fell outside the project's numbering.
    AllDropped,
    /// The pinned resource has no file for the book.
    Missing,
    /// The project's scheme is known but its mapping is not here.
    VersificationUnavailable,
    /// The project's scheme could not be established.
    VersificationUnknown,
}

/// `check/rows { since }`: the rows of an open session changed since `since`. A `Changed`
/// event naming the session's list asks it; a write's `Ack` carries its own rows (AD17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rows {
    pub at: Scope,
    pub list: CheckListRef,
    pub since: RailSince,
}

/// The rows changed since the view asked for, or a new session when the list was derived
/// again (its resource, numbering or scope changed): the UI then opens it whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RowsView {
    Changed {
        view: ViewSeq,
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
        rows: Vec<RailRowPatch>,
        progress: RailProgress,
    },
    Reopen,
}

// ---------------------------------------------------------------------------------------
// The guided fix (M11, unit B5a): `resources/fixState`
// ---------------------------------------------------------------------------------------

/// `resources/fixState`: what the guided fix offers for a tool whose pinned resource is not on this
/// computer (tC4 `openFix`): the pin to get and the installed versions of its repository a
/// re-pin could move to. Reading it changes nothing. C8 builds the three ways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixState {
    pub at: Scope,
    pub list: CheckListRef,
}

/// The guided fix of one tool; `None` when nothing is missing, or the pin is one no file can
/// satisfy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FixView {
    pub tool: Tool,
    pub pin: PinCitation,
    pub rung: Rung,
    /// Installed commits of the same repository: the re-pin choices.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub candidates: Vec<PinCitation>,
}

// ---------------------------------------------------------------------------------------
// A story's check (unit B7): `check/storyItem`
// ---------------------------------------------------------------------------------------

/// `check/storyItem`: one check of an open story session, as the item detail shows it.
/// Reading it writes nothing. B5b's `check/item` carries decisions, the target's selectable
/// words and the notices; until C4 writes a story's decisions this is the story card alone
/// (the compare card's gateway row, tC4 `StoryPane`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoryItem {
    pub at: Scope,
    pub list: CheckListRef,
    pub index: u32,
}

/// One check of a story.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct StoryItemView {
    pub index: u32,
    pub place: CheckPlace,
    /// The quote the check is about, in the gateway language: the item's title.
    pub title: String,
    /// The note's text as blocks (Translation Notes); empty for a words link.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub note: Vec<crate::model::ArticleBlock>,
    /// The article the check links to: the note's Translation Academy article, or the word's.
    pub article: Option<crate::helps::ArticleLink>,
    /// The gateway story's frame (the title for frame 0): tC4's `StoryPane`.
    pub gateway: StoryPane,
    /// The frame's text in the project; empty when it is not drafted.
    pub target: String,
}

/// The compare card's gateway row of a story check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum StoryPane {
    /// The frame's gateway text as tokens (words and what lies between them), with the
    /// indices of the tokens that render the check's quote (tC4's `tokenizePlain` and
    /// `matchPlainQuote`).
    Text {
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
        tokens: Vec<String>,
        #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
        lit: Vec<u32>,
    },
    /// The gateway story is not on this computer, or has no text for the frame
    /// (`storyDraft.sourceMissing`).
    Missing,
}
