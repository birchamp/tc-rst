//! Import a translationCore 3 project, and Update a book from USFM (unit F1; tC4
//! `src/views/modals/Import.jsx`, STORAGE §13.4 and §13.6). The first release imports the tC3
//! kind only (rulings/2026-10-24-H3-H4-H7-fast-release.md).
//!
//! - `import/review` reads the uploaded zips and writes nothing: what the review step shows.
//! - `import/versions` looks up on Door43 the resource versions the projects name.
//! - `import/run` makes one new project, whole, or none.
//! - `import/updateBook` brings a USFM file into a book of the open project: what comes in
//!   with no question first, then the changes to review, in one job.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{ProjectRef, RequestId, Scope, UploadId};
use crate::refusal::Refusal;

/// A file the UI uploaded (`POST /api/v1/uploads`, flow `import`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportFile {
    pub upload: UploadId,
    /// The name the person's file had, for the refusals and for History.
    pub name: String,
}

/// The Bible name and the language code of the new project: as read from the files, or as the
/// person typed them on the review step. The name is the project's name (#499).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportDetails {
    pub name: String,
    pub code: String,
}

/// `import/review`: what the zips hold. `details` is what the person typed; none takes what the
/// files say.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Review {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub files: Vec<ImportFile>,
    pub details: Option<ImportDetails>,
}

/// The review step. Nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ImportReview {
    pub found: ImportFound,
    /// The details the import would use: the typed ones, else the files'.
    pub details: ImportDetails,
    /// The folder the name makes, when it makes one.
    pub folder: Option<ProjectRef>,
    /// Why Import is off with these details: a name in use (`import.name-exists`, tC4 j09
    /// :367), no name, or a language code a Scripture Burrito cannot hold (#835). The run
    /// refuses the same.
    pub refusal: Option<Refusal>,
}

/// What the files hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ImportFound {
    /// `<language>_<resource>` and the language, as the files name them.
    pub details: ImportDetails,
    /// Each license the files name, in order; `""` is a file with none.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub licenses: Vec<String>,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub contributors: Vec<String>,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub books: Vec<FoundBook>,
}

/// One book of the import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FoundBook {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub chapters: u32,
    pub verses: u32,
    /// Verses with words.
    pub drafted: u32,
    /// Verses with an alignment record.
    pub aligned: u32,
    /// The checking decisions that carry over.
    pub decisions: u32,
    /// What the files hold that the import does not keep, in words: a translationNotes
    /// decision tied to every occurrence (STORAGE §13.4). The zip keeps them.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub not_kept: Vec<String>,
}

/// `import/versions`: look up on Door43 the versions the projects' resources had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Versions {
    pub request: RequestId,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub files: Vec<ImportFile>,
}

/// What the lookup found, slot by slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct VersionsView {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub found: Vec<FoundVersion>,
    /// Slots the projects name a version for that Door43 did not give: the installed
    /// versions are used (`importer.review.resourcesMissing`).
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub unresolved: Vec<VersionSlot>,
}

/// One slot's version, found on Door43. `import/run` sends back the ones the person accepted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FoundVersion {
    pub slot: VersionSlot,
    /// `git.door43.org/unfoldingWord/en_tn`.
    pub repo_path: String,
    pub tag: String,
    /// The commit the tag names, 40 hex characters.
    pub sha: String,
}

/// The resource a version is for (`importer.slot.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum VersionSlot {
    TranslationNotes,
    TranslationWords,
    /// The links that go with a found translationWords version, which tC3 does not record:
    /// the `<language>_twl` release closest in date to it (tC4 `twlFor`, D97 ruling 8). Found
    /// with that version or not at all, and never unresolved on its own.
    TranslationWordsLinks,
    OriginalNewTestament,
    OriginalOldTestament,
}

/// `import/run`: make the new project. The job ends with `JobOutcome::Created`, or fails and
/// leaves no project and no folder at its name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Run {
    pub request: RequestId,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub files: Vec<ImportFile>,
    pub details: ImportDetails,
    /// The versions the person accepted from `import/versions`: the project pins them and
    /// their slots' decisions stay on them. Every other slot takes the installed version, and
    /// its decisions carry over to that version's checks; one with no check there is kept,
    /// marked invalid (tC4 `applyVersions`). Empty when nothing was looked up.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub found: Vec<FoundVersion>,
}

/// `import/updateBook`: "Update a book from USFM" on the open project (STORAGE §13.6). A
/// file that does not read, or whose `\id` names another book, is refused before anything is
/// written. The job ends with `JobOutcome::BookUpdated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateBook {
    pub request: RequestId,
    pub at: Scope,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub file: ImportFile,
}

/// What Update a book did: "Titus: 12 verses imported, 2 to review." No Undo: History keeps
/// every version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UpdateReport {
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    /// Verses the file filled with no question.
    pub imported: u32,
    /// Records that wait in the conflict review, each verse among them; never counted as
    /// imported.
    pub to_review: u32,
    /// Verses left as they were: empty in the file, missing from it, or in another structure.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub left_as_it_was: Vec<VerseAt>,
    /// Verses whose alignment markup did not read apart: imported as text with the markup.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::few()"))]
    pub markup_kept: Vec<VerseAt>,
    /// The project did not have the book, and the file added it.
    pub book_added: bool,
}

/// A verse of the report's book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct VerseAt {
    pub chapter: u16,
    pub verse: u16,
}
