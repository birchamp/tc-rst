//! Domain refusals in tC4's words (APP-DESIGN §3.5).
//!
//! Every refusal names the catalog key of the sentence the person reads, tC4's refusal code
//! where tC4 has one, and the values of the sentence's placeholders. The UI builds the
//! banner as tC4's `failureText` does: the diagnosis, then the sentence
//! (`tC4:src/state.jsx:1781-1788`). Keys that tC4's catalogs lack live in the rebuild key
//! files, `crates/tc-ui/assets/i18n-rebuild/`. The tests check that every key resolves in
//! every language with the placeholders its variant fills, that every tC4 refusal code has a
//! variant or is listed as unused, and that no two sync refusals read the same.
//!
//! Each family belongs to one flow. A unit that builds a flow adds the refusals the flow
//! meets, with their keys.

use serde::{Deserialize, Serialize};

/// The words of a refusal.
pub trait Message {
    /// tC4's refusal code (`sync.offline`), where tC4 has one. "Ask for help" puts it in
    /// the report.
    fn code(&self) -> Option<&'static str>;
    /// The catalog key of the sentence the person reads.
    fn key(&self) -> &'static str;
    /// The values of the sentence's `{name}` placeholders.
    fn vars(&self) -> Vec<(&'static str, String)>;
    /// The diagnosis shown before the sentence, when the sentence has no place for it.
    fn detail(&self) -> Option<&str> {
        None
    }
}

/// A refusal, by the flow it belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Refusal {
    Open(OpenRefusal),
    Create(CreateRefusal),
    Import(ImportRefusal),
    Export(ExportRefusal),
    Sync(SyncFailure),
    Share(ShareRefusal),
    OpenDoor43(OpenDoor43Refusal),
    SignIn(SignInRefusal),
    Resources(ResourceRefusal),
    Exchange(ExchangeRefusal),
    Feedback(FeedbackRefusal),
    Prefs(PrefsRefusal),
}

impl Refusal {
    fn message(&self) -> &dyn Message {
        match self {
            Self::Open(r) => r,
            Self::Create(r) => r,
            Self::Import(r) => r,
            Self::Export(r) => r,
            Self::Sync(r) => r,
            Self::Share(r) => r,
            Self::OpenDoor43(r) => r,
            Self::SignIn(r) => r,
            Self::Resources(r) => r,
            Self::Exchange(r) => r,
            Self::Feedback(r) => r,
            Self::Prefs(r) => r,
        }
    }
}

impl Message for Refusal {
    fn code(&self) -> Option<&'static str> {
        self.message().code()
    }
    fn key(&self) -> &'static str {
        self.message().key()
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        self.message().vars()
    }
    fn detail(&self) -> Option<&str> {
        self.message().detail()
    }
}

/// Opening a project refuses only for these (STORAGE §6.1); a held lock or an unsafe
/// folder opens read-only instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpenRefusal {
    /// The project's format major is newer than this app's (O4).
    NewerFormat {
        format: u16,
    },
    NotAProject,
    /// The folder cannot be read.
    Unreadable {
        reason: String,
    },
}

impl Message for OpenRefusal {
    fn code(&self) -> Option<&'static str> {
        match self {
            Self::NotAProject => Some("open.not-a-project"),
            Self::NewerFormat { .. } | Self::Unreadable { .. } => None,
        }
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NewerFormat { .. } => "refusal.open.newer-format",
            Self::NotAProject => "refusal.open.not-a-project",
            Self::Unreadable { .. } => "home.openError",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NewerFormat { format } => vec![("format", format.to_string())],
            Self::NotAProject | Self::Unreadable { .. } => Vec::new(),
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Unreadable { reason } => Some(reason),
            Self::NewerFormat { .. } | Self::NotAProject => None,
        }
    }
}

/// Creating a project (unit C6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CreateRefusal {
    /// A folder with the project's short name exists.
    NameInUse {
        abbr: String,
    },
    Failed {
        reason: String,
    },
}

impl Message for CreateRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NameInUse { .. } => "wizard.nameInUse",
            Self::Failed { .. } => "wizard.error",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NameInUse { abbr } => vec![("abbr", abbr.clone())],
            Self::Failed { .. } => Vec::new(),
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Failed { reason } => Some(reason),
            Self::NameInUse { .. } => None,
        }
    }
}

/// Importing a project (unit F1 adds the importer's refusals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ImportRefusal {
    WriteFailed,
    NameExists,
}

impl Message for ImportRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::WriteFailed => "import.write-failed",
            Self::NameExists => "import.name-exists",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::WriteFailed => "refusal.import.write-failed",
            Self::NameExists => "refusal.import.name-exists",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// Exporting a book or the stories (unit D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ExportRefusal {
    CheckpointFailed,
    ReadFailed,
    PrintFailed,
    PrintFailedPictures,
    NothingDrafted,
}

impl Message for ExportRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::CheckpointFailed => "export.checkpoint-failed",
            Self::ReadFailed => "export.read-failed",
            Self::PrintFailed => "export.print-failed",
            Self::PrintFailedPictures => "export.print-failed-pictures",
            Self::NothingDrafted => "export.nothing-drafted",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::CheckpointFailed => "refusal.export.checkpoint-failed",
            Self::ReadFailed => "refusal.export.read-failed",
            Self::PrintFailed => "refusal.export.print-failed",
            Self::PrintFailedPictures => "refusal.export.print-failed-pictures",
            Self::NothingDrafted => "refusal.export.nothing-drafted",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// A sync that did not complete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct SyncFailure {
    pub refusal: SyncRefusal,
    /// The teammates' work had already been received and kept when the sync stopped (PLAN
    /// Q8). The person must not read "nothing changed" then.
    pub after_intake: bool,
}

/// Why a sync stopped (STORAGE §9.7, DELTA §3.15.2; tC4 `SYNC_REFUSALS`,
/// `tC4:src/views/SyncResult.jsx:14-51`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SyncRefusal {
    Offline,
    NotSignedIn,
    AuthFailed,
    NotShared,
    NoWriteAccess {
        server_message: String,
    },
    RefRejected {
        server_message: String,
    },
    /// Teammates pushed during every attempt.
    Busy {
        attempts: u8,
    },
    RemoteMissing,
    Unreachable,
    /// The repository holds another project (STORAGE §9.2 N2).
    ProjectMismatch,
    /// A teammate uses a newer project format (N1).
    NeedsNewerApp {
        major: u16,
        min_reader_minor: u16,
    },
    /// The repository is still in tC4's team format; the team move is offered (N1).
    Tc4TeamRepository,
    ReadOnly,
    Io {
        reason: String,
    },
}

impl SyncFailure {
    /// The second sentence a failure after intake shows, so the person learns that the
    /// received work was kept (PLAN Q8). `None` when the first sentence already says so, or
    /// when nothing had been received.
    pub fn kept_note(&self) -> Option<&'static str> {
        let says_kept = matches!(
            self.key(),
            "syncResult.error.notSent" | "syncResult.error.keptStopped"
        );
        (self.after_intake && !says_kept).then_some("syncResult.receivedKept")
    }
}

impl Message for SyncFailure {
    fn code(&self) -> Option<&'static str> {
        use SyncRefusal as R;
        match (&self.refusal, self.after_intake) {
            (R::Offline, _) => Some("sync.offline"),
            (R::AuthFailed, _) => Some("sync.auth-failed"),
            (R::NotShared, _) => Some("sync.not-shared"),
            (R::NoWriteAccess { .. }, _) => Some("sync.no-write-access"),
            (R::Busy { .. }, _) => Some("sync.non-fast-forward"),
            (R::Unreachable | R::RemoteMissing, false) => Some("sync.clone-failed"),
            (R::Unreachable | R::RemoteMissing, true)
            | (
                R::NotSignedIn
                | R::RefRejected { .. }
                | R::ProjectMismatch
                | R::NeedsNewerApp { .. }
                | R::Tc4TeamRepository
                | R::ReadOnly
                | R::Io { .. },
                _,
            ) => None,
        }
    }

    // DELTA §3.15.2's table; after intake, a sentence that would say "nothing changed"
    // gives way to one that says the received work was kept (PLAN Q8).
    fn key(&self) -> &'static str {
        use SyncRefusal as R;
        match (&self.refusal, self.after_intake) {
            (R::Offline, _) => "syncResult.error.offline",
            (R::NotSignedIn | R::AuthFailed, _) => "syncResult.error.authFailed",
            (R::NotShared, _) => "syncResult.error.notShared",
            (R::NoWriteAccess { .. }, _) => "syncResult.error.noWriteAccess",
            (R::Busy { .. }, _) => "syncResult.error.nonFastForward",
            (R::Unreachable | R::RemoteMissing, false) => "syncResult.error.cloneFailed",
            (R::Unreachable | R::RemoteMissing, true) => "syncResult.error.notSent",
            (R::RefRejected { .. }, _) | (R::Io { .. }, false) => "syncResult.error.other",
            (R::Io { .. }, true) => "syncResult.error.keptStopped",
            (R::ProjectMismatch, _) => "syncResult.error.projectMismatch",
            (R::NeedsNewerApp { .. }, _) => "syncResult.error.needsNewerApp",
            (R::Tc4TeamRepository, _) => "syncResult.error.tc4TeamRepository",
            (R::ReadOnly, _) => "syncResult.error.readOnly",
        }
    }

    fn vars(&self) -> Vec<(&'static str, String)> {
        use SyncRefusal as R;
        match &self.refusal {
            R::RefRejected { server_message } => vec![("reason", server_message.clone())],
            R::Io { reason } => vec![("reason", reason.clone())],
            R::NeedsNewerApp { major, .. } => vec![("format", major.to_string())],
            R::Offline
            | R::NotSignedIn
            | R::AuthFailed
            | R::NotShared
            | R::NoWriteAccess { .. }
            | R::Busy { .. }
            | R::RemoteMissing
            | R::Unreachable
            | R::ProjectMismatch
            | R::Tc4TeamRepository
            | R::ReadOnly => Vec::new(),
        }
    }

    fn detail(&self) -> Option<&str> {
        match &self.refusal {
            SyncRefusal::NoWriteAccess { server_message } => Some(server_message),
            _ => None,
        }
    }
}

/// The first share of a project (STORAGE §9.6; units B9b and E1b).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ShareRefusal {
    NameExists,
    Offline,
    AuthFailed,
    CreateRejected { server_message: String },
    PushFailed { reason: String },
}

impl Message for ShareRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::NameExists => "share.name-exists",
            Self::Offline => "share.offline",
            Self::AuthFailed => "share.auth-failed",
            Self::CreateRejected { .. } => "share.create-rejected",
            Self::PushFailed { .. } => "share.push-failed",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NameExists => "shareDialog.error.name-exists",
            Self::Offline => "shareDialog.error.offline",
            Self::AuthFailed => "shareDialog.error.auth-failed",
            Self::CreateRejected { .. } => "shareDialog.error.create-rejected",
            Self::PushFailed { .. } => "shareDialog.error.push-failed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::PushFailed { reason } => vec![("reason", reason.clone())],
            Self::NameExists | Self::Offline | Self::AuthFailed | Self::CreateRejected { .. } => {
                Vec::new()
            }
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::CreateRejected { server_message } => Some(server_message),
            _ => None,
        }
    }
}

/// Open a project from Door43 (unit E1b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum OpenDoor43Refusal {
    NameExists,
    NoJournal,
    /// An open that did not finish before the app closed; its part copy was removed at the
    /// next start (STORAGE §6.4 N1).
    Removed,
}

impl Message for OpenDoor43Refusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::NameExists => "open-door43.name-exists",
            Self::NoJournal => "open-door43.no-journal",
            Self::Removed => "open-door43.removed",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NameExists => "refusal.open-door43.name-exists",
            Self::NoJournal => "refusal.open-door43.no-journal",
            Self::Removed => "refusal.open-door43.removed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// A Door43 sign-in (`tC4:src/data/share/session.ts`, `refusalForSignIn`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SignInRefusal {
    /// Door43 answered 401 or 403.
    AuthFailed,
    /// Door43 did not answer.
    Offline,
    /// Door43 answered 5xx.
    ServerUnavailable,
    /// Any other answer: not about the password, in Door43's words.
    Rejected { reason: String },
}

impl Message for SignInRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::AuthFailed => "share.auth-failed",
            Self::Offline => "share.offline",
            Self::ServerUnavailable => "share.server-unavailable",
            Self::Rejected { .. } => "share.sign-in-rejected",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::AuthFailed => "signIn.error.auth-failed",
            Self::Offline => "signIn.error.offline",
            Self::ServerUnavailable => "signIn.error.server-unavailable",
            Self::Rejected { .. } => "signIn.error.sign-in-rejected",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Rejected { reason } => vec![("reason", reason.clone())],
            Self::AuthFailed | Self::Offline | Self::ServerUnavailable => Vec::new(),
        }
    }
}

/// Installing, upgrading and choosing resources (units B9a and C8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ResourceRefusal {
    CatalogUnreadable {
        reason: String,
    },
    /// Some downloads did not install; `list` names them.
    SomeFailed {
        list: String,
    },
    FixFailed {
        reason: String,
    },
    UpdateCheckFailed {
        reason: String,
    },
    UpgradeFailed {
        reason: String,
    },
    GatewayFailed,
    /// `books` would lose their checking resource.
    GatewayBlocked {
        books: String,
    },
    /// The download finished but the project's pins were not updated.
    AdoptFailed {
        error: String,
    },
}

impl Message for ResourceRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::CatalogUnreadable { .. } => "sources.catalogError",
            Self::SomeFailed { .. } => "sources.someFailed",
            Self::FixFailed { .. } => "fix.failed",
            Self::UpdateCheckFailed { .. } => "upgrade.checkFailed",
            Self::UpgradeFailed { .. } => "upgrade.failed",
            Self::GatewayFailed => "gateway.failed",
            Self::GatewayBlocked { .. } => "gateway.blockedError",
            Self::AdoptFailed { .. } => "sources.adoptFailed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::CatalogUnreadable { reason }
            | Self::FixFailed { reason }
            | Self::UpdateCheckFailed { reason }
            | Self::UpgradeFailed { reason } => vec![("reason", reason.clone())],
            Self::SomeFailed { list } => vec![("list", list.clone())],
            Self::GatewayBlocked { books } => vec![("books", books.clone())],
            Self::AdoptFailed { error } => vec![("error", error.clone())],
            Self::GatewayFailed => Vec::new(),
        }
    }
}

/// Saving work for a teammate and receiving it from a file (STORAGE §10; unit E2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExchangeRefusal {
    /// The zip's central directory cannot be read (R1).
    Damaged,
    /// The file's project format is newer than this app's (R3).
    NewerFormat {
        format: u16,
    },
    /// The written file did not read back from the drive; it was deleted (E5).
    VerifyFailed,
    NotWritten {
        reason: String,
    },
    Unreadable {
        reason: String,
    },
}

impl Message for ExchangeRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::Damaged => "exchange.error.damaged",
            Self::NewerFormat { .. } => "exchange.error.newerFormat",
            Self::VerifyFailed => "exchange.error.verifyFailed",
            Self::NotWritten { .. } => "exchange.error.notWritten",
            Self::Unreadable { .. } => "exchange.error.unreadable",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NewerFormat { format } => vec![("format", format.to_string())],
            Self::NotWritten { reason } | Self::Unreadable { reason } => {
                vec![("reason", reason.clone())]
            }
            Self::Damaged | Self::VerifyFailed => Vec::new(),
        }
    }
}

/// Why a feedback report was not sent. The shell sends reports (owner's answer to Q-2) and
/// answers with one of tC4's reasons, which [`FeedbackRefusal::from_shell_reason`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum FeedbackRefusal {
    /// A browser, with no shell to send from.
    NoDesktop,
    /// A build without the help desk's key.
    NotConfigured,
    Offline,
    Timeout,
    Refused,
}

impl FeedbackRefusal {
    /// The reason the shell's bridge answers (`tC4:scripts/desktop-feedback.cjs`).
    pub fn from_shell_reason(reason: &str) -> Option<Self> {
        Some(match reason {
            "no-desktop" => Self::NoDesktop,
            "not-configured" => Self::NotConfigured,
            "offline" => Self::Offline,
            "timeout" => Self::Timeout,
            "refused" => Self::Refused,
            _ => return None,
        })
    }
}

impl Message for FeedbackRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NoDesktop => "feedback.reason.no-desktop",
            Self::NotConfigured => "feedback.reason.not-configured",
            Self::Offline => "feedback.reason.offline",
            Self::Timeout => "feedback.reason.timeout",
            Self::Refused => "feedback.reason.refused",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}

/// A preference write the person waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum PrefsRefusal {
    /// The app language was not saved; the dialog stays open
    /// (`tC4:e2e/app-language.spec.ts:263`).
    LocaleNotSaved,
}

impl Message for PrefsRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::LocaleNotSaved => "language.saveFailed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
}
