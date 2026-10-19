//! Get source texts (M8 steps 1 and 2, unit B9a; `tC4:src/views/modals/SourceTexts.jsx`): the
//! gateway-language packages, one organization's package for a book as Door43's catalog lists
//! it, and the download of the rows the person keeps. Every way into the dialog opens the
//! project first (tC4 `manageSettingsSources`, `openSources`), so each request names the open
//! project. The release upgrades of its first step and the gateway change of its second are
//! unit C8's.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::ids::{RequestId, Scope};
use crate::shown::Direction;

/// A gateway-language package: a language and the Door43 organization that publishes it
/// (tC4 `gatewayKey`: `language::owner`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayRef {
    pub language: String,
    pub owner: String,
}

impl GatewayRef {
    /// tC4's `gatewayKey`, the key of a package's option card.
    pub fn key(&self) -> String {
        format!("{}::{}", self.language, self.owner)
    }
}

/// One package of tC4's registry of gateway languages (`data/gateways.ts`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct GatewayPackage {
    pub gateway: GatewayRef,
    /// The English name.
    pub name: String,
    /// The language's own name, which every screen shows first (D94, #579).
    pub autonym: String,
    pub direction: Direction,
    /// The releases of its notes, words and academy that tC4 saw (`seen`).
    pub seen: Seen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Seen {
    pub tn: String,
    pub tw: String,
    pub ta: String,
}

/// `resources/sources`: the dialog's first step for the open project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sources {
    pub at: Scope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SourcesView {
    /// Every package, in the registry's order, each with how many of its books this app session
    /// downloaded (tC4 `installedCountFor`, which counts the session's downloads).
    pub gateways: Vec<SourceGateway>,
    /// The open project's checking language, from its pins.
    pub current: Option<GatewayRef>,
    /// The packages whose whole suite is installed on this computer, which the project could
    /// check in (tC4 `refreshCheckable`).
    pub checkable: FromInstalls<GatewayRef>,
    /// Whether the project has language sets: tC4 draws "Pinned resources" only then.
    pub pinned: bool,
}

/// A list worked out from what is installed on this computer, or why the installs could not be
/// read: a failed read is stated, never shown as an empty list (tC4's D30 sweep).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum FromInstalls<T> {
    Listed(Vec<T>),
    Failed { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SourceGateway {
    pub package: GatewayPackage,
    pub installed: u32,
}

/// `resources/catalog` [sources]: what one organization publishes that tC4 can use for one book
/// (tC4 `readPackage`, `packageRows`), read from Door43's catalog. Every row carries the
/// `tc-ready` topic, and a book-scoped row covers the book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Catalog {
    pub at: Scope,
    pub gateway: GatewayRef,
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct CatalogView {
    pub rows: Vec<PackageRow>,
}

/// One row of a package (tC4's `{k, name, repo, desc, fixed, books}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PackageRow {
    pub role: PackageRole,
    /// The repository's name, such as `en_tn`.
    pub repo: String,
    /// What the catalog says the repository is.
    pub desc: String,
}

impl PackageRow {
    /// tC4's row key, `role:repo`, which the person's choices are kept under.
    pub fn key(&self) -> String {
        format!("{}:{}", self.role.key(), self.repo)
    }
}

/// The part a repository plays in a package (tC4 `roleOf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum PackageRole {
    /// A Bible: always included.
    Text,
    Notes,
    Questions,
    WordLinks,
    Words,
    Academy,
    /// Open Bible Stories: always included.
    ObsText,
    ObsNotes,
    ObsWords,
    ObsImages,
}

impl PackageRole {
    /// tC4's role key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Notes => "notes",
            Self::Questions => "questions",
            Self::WordLinks => "wordLinks",
            Self::Words => "words",
            Self::Academy => "academy",
            Self::ObsText => "obs",
            Self::ObsNotes => "obs-notes",
            Self::ObsWords => "obs-words",
            Self::ObsImages => "obs-images",
        }
    }

    /// A row the person cannot leave out: the package's text.
    pub fn fixed(self) -> bool {
        matches!(self, Self::Text | Self::ObsText)
    }
}

/// `resources/download` [sources] (job `Download`): installs the named repositories of one
/// organization on this computer, one after the other, each at the open project's pin when the
/// project pins it and this computer lacks that version, else as installed, else at its latest
/// release. A cancelled job installed nothing it had not finished; it never changes the
/// project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Download {
    pub request: RequestId,
    pub at: Scope,
    pub gateway: GatewayRef,
    /// The book the package was chosen for, which step 1 counts.
    #[cfg_attr(feature = "proptest", proptest(strategy = "crate::arb::book()"))]
    pub book: BookId,
    pub repos: Vec<String>,
}
