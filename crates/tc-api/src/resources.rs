//! Get source texts (M8 steps 1 and 2, unit B9a; `tC4:src/views/modals/SourceTexts.jsx`): the
//! gateway-language packages, one organization's package for a book as Door43's catalog lists
//! it, and the download of the rows the person keeps. Every way into the dialog opens the
//! project first (tC4 `manageSettingsSources`, `openSources`), so each request names the open
//! project. The release upgrades of its first step and the gateway change of its second are
//! unit C8's.

use serde::{Deserialize, Serialize};
use tc_core::BookId;

use crate::check::{CheckUnit, Tool};
use crate::ids::{PlanId, RequestId, Scope, UploadId};
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

// ---------------------------------------------------------------------------------------
// Release upgrades (unit C8; tC4 `ResourceUpdates.jsx`, `UpgradeSet.jsx`)
// ---------------------------------------------------------------------------------------

/// A language set of a project: the one it checks in, or the English one it falls back on
/// (tC4's rung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum SetRung {
    Primary,
    Fallback,
}

/// One repository moving from one release to another: what an offer row and the confirmation
/// list (tC4 `upgrade.row`, `upgrade.move`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct RepoMove {
    pub repo: String,
    /// The release the project pins, or the first 12 digits of its commit when it has no tag.
    pub from: String,
    pub to: String,
    /// When the release was published, as Door43 gave it.
    pub published: Option<String>,
}

/// `resources/checkUpdates` [sources]: asks Door43, on demand, what is newer than what the
/// open project pins. Nothing is installed and nothing is written (tC4 `checkForUpdates`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckUpdates {
    pub at: Scope,
}

/// What `resources/checkUpdates` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UpdatesView {
    /// One offer for each language set the project pins; a set with no moves is up to date.
    pub sets: Vec<SetOffer>,
    /// The scripture texts with a newer release (#258).
    pub texts: Vec<TextOffer>,
    /// Why the texts could not be asked about, said beside the set offers, which stand.
    pub texts_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SetOffer {
    pub rung: SetRung,
    /// The set's language id, as tC4 names the set (`Primary set · es-419`).
    pub language: String,
    pub moves: Vec<RepoMove>,
}

/// The two kinds of scripture text a project pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum TextKind {
    /// The original-language text the alignments point at.
    Original,
    /// A gateway Bible a source pane shows.
    Gateway,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct TextOffer {
    pub kind: TextKind,
    pub change: RepoMove,
}

/// What a plan is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OfferRef {
    /// Every newer repository of one language set.
    Set { rung: SetRung },
    /// One scripture text, by its repository's name.
    Text { repo: String },
}

/// `resources/planUpgrade` [sources] (job `UpgradePlan`): installs the release the offer names,
/// all of it or none, then plans what moving the pins costs. The pins move only when the
/// person confirms the plan ([`ApplyPlan`]). The job's outcome is `PlanReady`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanUpgrade {
    pub request: RequestId,
    pub at: Scope,
    pub offer: OfferRef,
}

/// `resources/planGateway` (job `GatewayPlan`): plans changing the project's checking language
/// to a package whose suite is installed here. Nothing is downloaded, so it asks no internet
/// question (tC4 `previewGatewayChange`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanGateway {
    pub request: RequestId,
    pub at: Scope,
    pub gateway: GatewayRef,
}

/// `resources/applyPlan`: writes a plan the person confirmed, as one batch. Only the id is
/// sent: the server holds the plan it showed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyPlan {
    pub request: RequestId,
    pub at: Scope,
    pub plan: PlanId,
}

/// What a confirmed plan came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlanApplied {
    Applied,
    /// A teammate's work arrived between the plan and the confirmation, so the plan the person
    /// read no longer describes the project. Nothing was written. This is the plan made again
    /// from the project as it is now, to read and confirm (tC4 shows the dialog again).
    Replanned(Box<PlanView>),
}

/// A plan the person reads before anything is written (tC4 `UpgradeSet.jsx`,
/// `GatewayChange.jsx`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub plan: PlanId,
    pub what: PlanWhat,
    /// Decisions made against what the change leaves. None: nothing already checked is
    /// affected.
    pub decisions_at_risk: u32,
    /// One row for each (tool, book) the change affects.
    pub rows: Vec<PlanRow>,
    /// Units the change cannot be applied to; the plan cannot be confirmed while there are any.
    pub blocked: Vec<BlockedUnit>,
    /// For an original-language text: the verses whose alignments will be marked for review.
    /// Every alignment is kept.
    pub alignments: Option<u32>,
}

impl PlanView {
    /// Nothing the person has already checked is disturbed.
    pub fn harmless(&self) -> bool {
        self.rows.is_empty() && self.blocked.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PlanWhat {
    /// A release upgrade of a language set.
    Set {
        rung: SetRung,
        language: String,
        moves: Vec<RepoMove>,
    },
    /// The guided fix moves a set's pins to a version installed here.
    Repin {
        rung: SetRung,
        language: String,
        moves: Vec<RepoMove>,
    },
    /// A scripture text's release.
    Text { kind: TextKind, change: RepoMove },
    /// A new checking language.
    Gateway(Box<GatewayChange>),
}

/// A checking-language change, each side named by its autonym (#579).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct GatewayChange {
    pub from: Option<GatewayPackage>,
    pub to: GatewayPackage,
    /// A story project's pictures, when the package changes them.
    pub images: Option<ImageChange>,
}

/// The picture pack a story project reads, before and after a gateway change; `None` is the
/// bundled default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ImageChange {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// What the change does to one (tool, book)'s decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PlanRow {
    pub tool: Tool,
    pub unit: CheckUnit,
    /// Its decisions made against the resource it leaves.
    pub decisions: u32,
    /// The repository it was checked against.
    pub against: String,
    /// How the new list takes them; `None` when it could not be derived.
    pub carry: Option<Carry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct Carry {
    /// Decisions that find their check in the new list and count as decided there.
    pub carried: u32,
    /// Decisions with no check in the new list: kept, and the checks come back as work.
    pub check_again: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BlockedUnit {
    pub tool: Tool,
    pub unit: CheckUnit,
    pub reason: BlockReason,
}

/// Why a change cannot be applied to a unit (tC4's `gateway.blocked*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum BlockReason {
    /// The new pins leave the unit's decisions with no usable resource.
    NoResource,
    /// The numbering data to carry the decisions over could not be loaded; trying again may
    /// work.
    VersificationUnavailable,
    /// The project's versification could not be identified, so its decisions cannot be carried
    /// over safely.
    VersificationUnknown,
}

// ---------------------------------------------------------------------------------------
// The guided fix (tC4 `GuidedFix.jsx`)
// ---------------------------------------------------------------------------------------

/// `resources/fixState`: what a check tool's missing resource is and the ways to get it.
/// Nothing is written by asking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixState {
    pub at: Scope,
    pub tool: Tool,
    pub unit: CheckUnit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FixView {
    /// The pin to get: its repository, release and commit.
    pub pin: PinLabel,
    pub rung: SetRung,
    /// Other commits of the same repository installed here, to move the pin to.
    pub candidates: Vec<PinLabel>,
    /// Why the installed copies could not be read, said instead of "none installed".
    pub candidates_error: Option<String>,
}

/// A pinned resource as the guided fix names it: `es-419_tn · v66` and its commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PinLabel {
    pub repo_path: String,
    pub version: Option<String>,
    pub sha: String,
}

/// `resources/fixFetch` [fix] (job `FixFetch`): downloads the pinned resource itself, checked
/// against the pinned commit before it is installed. It changes no pin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixFetch {
    pub request: RequestId,
    pub at: Scope,
    pub tool: Tool,
    pub unit: CheckUnit,
}

/// `resources/fixSideload`: installs a file the person chose, only when the file declares
/// exactly the pinned commit (tC4 `verifySideload`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixSideload {
    pub request: RequestId,
    pub at: Scope,
    pub tool: Tool,
    pub unit: CheckUnit,
    pub upload: UploadId,
}

/// `resources/fixRepin`: plans moving the pins that name the missing commit to an installed
/// commit of the same repository. The plan is confirmed like a release upgrade
/// ([`ApplyPlan`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixRepin {
    pub request: RequestId,
    pub at: Scope,
    pub tool: Tool,
    pub unit: CheckUnit,
    /// The installed commit to move to, one of [`FixView::candidates`].
    pub sha: String,
}
