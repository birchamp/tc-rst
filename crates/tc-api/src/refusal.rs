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
    /// The diagnosis shown before the sentence, when the sentence has no place for it. A flow
    /// whose tC4 banner shows the sentence alone, sync and share, has none.
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
    Settings(SettingsRefusal),
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
    Upload(UploadRefusal),
}

impl Refusal {
    fn message(&self) -> &dyn Message {
        match self {
            Self::Open(r) => r,
            Self::Create(r) => r,
            Self::Settings(r) => r,
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
            Self::Upload(r) => r,
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

/// Opening a project refuses only for these (STORAGE §6.1). A lock another process holds
/// refuses the open and names the holder (O1; translationCore runs once per computer, so
/// no other window of this app can hold it); an unsafe folder opens read-only instead.
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
    /// Another process holds the project's lock (O1). `holder` is the process the lock
    /// names, or `None` when the platform will not read the lock's record. tC4 has no such
    /// state: it runs once per computer and has no project lock.
    LockHeld {
        holder: Option<LockHolder>,
    },
}

/// The process that holds a project's lock, as the lock names it (tc-store's
/// `LockHolder`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LockHolder {
    pub host: String,
    pub pid: u32,
}

impl Message for OpenRefusal {
    fn code(&self) -> Option<&'static str> {
        match self {
            Self::NotAProject => Some("open.not-a-project"),
            Self::NewerFormat { .. } | Self::Unreadable { .. } | Self::LockHeld { .. } => None,
        }
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NewerFormat { .. } => "refusal.open.newer-format",
            Self::NotAProject => "refusal.open.not-a-project",
            Self::Unreadable { .. } => "home.openError",
            Self::LockHeld { holder: Some(_) } => "refusal.open.lock-held",
            Self::LockHeld { holder: None } => "refusal.open.lock-held-unnamed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NewerFormat { format } => vec![("format", format.to_string())],
            Self::LockHeld {
                holder: Some(holder),
            } => vec![
                ("pid", holder.pid.to_string()),
                ("host", holder.host.clone()),
            ],
            Self::NotAProject | Self::Unreadable { .. } | Self::LockHeld { holder: None } => {
                Vec::new()
            }
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Unreadable { reason } => Some(reason),
            Self::NewerFormat { .. } | Self::NotAProject | Self::LockHeld { .. } => None,
        }
    }
}

/// Creating a project and adding books (unit C6).
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
    /// The form's name is empty.
    NameRequired,
    /// The form's language code is empty.
    CodeRequired,
    /// Neither the name nor the code has a letter or a digit of the Latin alphabet, so no
    /// folder name can be made of them.
    AbbrRequired,
    /// A Scripture Burrito cannot hold the language code (a part over eight letters, #582).
    /// Nothing was created.
    CodeNotHeld {
        code: String,
        reason: String,
    },
    Failed {
        reason: String,
    },
    /// A copy of a read-only project would open read-only for the same reason: the projects
    /// folder itself is on a drive that cannot keep a project safe, or a cloud client copies
    /// it, and the local projects folder is no safer (C1b, Q-30). No copy is made.
    CopyUnsafe {
        reason: crate::write::ReadOnlyReason,
    },
}

impl Message for CreateRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NameInUse { .. } => "wizard.nameInUse",
            Self::NameRequired => "wizard.nameRequired",
            Self::CodeRequired => "wizard.codeRequired",
            Self::AbbrRequired => "wizard.abbrRequired",
            Self::CodeNotHeld { .. } => "wizard.codeNotHeld",
            Self::Failed { .. } => "wizard.error",
            Self::CopyUnsafe { .. } => "readOnly.copyUnsafe",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NameInUse { abbr } => vec![("abbr", abbr.clone())],
            Self::CodeNotHeld { code, .. } => vec![("code", code.clone())],
            Self::NameRequired
            | Self::CodeRequired
            | Self::AbbrRequired
            | Self::Failed { .. }
            | Self::CopyUnsafe { .. } => Vec::new(),
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Failed { reason } | Self::CodeNotHeld { reason, .. } => Some(reason),
            Self::NameInUse { .. }
            | Self::NameRequired
            | Self::CodeRequired
            | Self::AbbrRequired
            | Self::CopyUnsafe { .. } => None,
        }
    }
}

/// Saving a project's settings from Home (unit C6). With R18's fallback the direction and the
/// font are two batches, so a refusal says which was written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SettingsRefusal {
    /// The project's settings are not what the dialog showed (a teammate's work arrived, or
    /// another program changed the file): nothing was saved. The dialog reads them again.
    Changed,
    /// The direction was not saved, and the font was not tried.
    DirectionNotSaved { reason: String },
    /// The font was not saved; `direction_saved` says whether the direction was.
    FontNotSaved {
        reason: String,
        direction_saved: bool,
    },
}

impl Message for SettingsRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::Changed => "settings.changed",
            Self::DirectionNotSaved { .. } => "settings.directionNotSaved",
            Self::FontNotSaved {
                direction_saved: false,
                ..
            } => "settings.fontNotSaved",
            Self::FontNotSaved {
                direction_saved: true,
                ..
            } => "settings.fontNotSavedAfterDirection",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Changed => None,
            Self::DirectionNotSaved { reason } | Self::FontNotSaved { reason, .. } => Some(reason),
        }
    }
}

/// Importing a project and Update a book (unit F1). The tC3 refusals are tc-import's, one
/// variant each, in tC4's `importer.tc3.*` words; tC4 gives a code to three of them only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ImportRefusal {
    WriteFailed,
    NameExists,
    /// The zip ends partway (tC4 `import.damaged.truncated`).
    NotZip {
        file: String,
    },
    /// A part of the zip does not match its checksum.
    CorruptEntry {
        file: String,
        entry: String,
    },
    NoManifest {
        file: String,
    },
    NoText {
        file: String,
    },
    /// The projects are in different languages.
    Languages {
        languages: Vec<String>,
    },
    /// Two files hold one book.
    Twice {
        code: String,
    },
    /// A part of a project does not read; `problem` says where.
    Unreadable {
        file: String,
        problem: String,
    },
    /// The imported text would not be the source's byte for byte (P9).
    TextChanged {
        book: String,
    },
    /// Update a book: the file does not read at `line`.
    UpdateUnreadable {
        file: String,
        line: u32,
        problem: String,
    },
    /// Update a book: the file's `\id` names another book than the one chosen.
    UpdateOtherBook {
        file: String,
        named: String,
        chosen: String,
    },
    /// Update a book wrote nothing.
    UpdateNotWritten {
        reason: String,
    },
    /// Update a book wrote what comes in with no question, and not the changes to review. A
    /// second run with the same file finishes it.
    UpdateReviewNotWritten {
        reason: String,
    },
}

impl Message for ImportRefusal {
    fn code(&self) -> Option<&'static str> {
        match self {
            Self::WriteFailed => Some("import.write-failed"),
            Self::NameExists => Some("import.name-exists"),
            Self::NotZip { .. } => Some("import.damaged.truncated"),
            Self::CorruptEntry { .. } => Some("import.damaged.corrupt-entry"),
            Self::NoManifest { .. } => Some("import.damaged.no-manifest"),
            Self::NoText { .. }
            | Self::Languages { .. }
            | Self::Twice { .. }
            | Self::Unreadable { .. }
            | Self::TextChanged { .. }
            | Self::UpdateUnreadable { .. }
            | Self::UpdateOtherBook { .. }
            | Self::UpdateNotWritten { .. }
            | Self::UpdateReviewNotWritten { .. } => None,
        }
    }
    fn key(&self) -> &'static str {
        match self {
            Self::WriteFailed => "refusal.import.write-failed",
            Self::NameExists => "refusal.import.name-exists",
            Self::NotZip { .. } => "importer.tc3.notZip",
            Self::CorruptEntry { .. } => "importer.tc3.corruptEntry",
            Self::NoManifest { .. } => "importer.tc3.noManifest",
            Self::NoText { .. } => "importer.tc3.noText",
            Self::Languages { .. } => "importer.tc3.languages",
            Self::Twice { .. } => "importer.tc3.twice",
            Self::Unreadable { .. } => "importer.tc3.unreadable",
            Self::TextChanged { .. } => "importer.tc3.textChanged",
            Self::UpdateUnreadable { .. } => "updateBook.unreadable",
            Self::UpdateOtherBook { .. } => "updateBook.otherBook",
            Self::UpdateNotWritten { .. } => "updateBook.notWritten",
            Self::UpdateReviewNotWritten { .. } => "updateBook.reviewNotWritten",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        let file = |file: &String| ("file", file.clone());
        match self {
            Self::NotZip { file: f } | Self::NoManifest { file: f } | Self::NoText { file: f } => {
                vec![file(f)]
            }
            Self::CorruptEntry { file: f, entry } => vec![file(f), ("entry", entry.clone())],
            Self::Languages { languages } => vec![("languages", languages.join(", "))],
            Self::Twice { code } => vec![("code", code.clone())],
            Self::Unreadable { file: f, .. } => vec![file(f)],
            Self::TextChanged { book } => vec![("book", book.clone())],
            Self::UpdateUnreadable { file: f, line, .. } => {
                vec![file(f), ("line", line.to_string())]
            }
            Self::UpdateOtherBook {
                file: f,
                named,
                chosen,
            } => vec![
                file(f),
                ("named", named.clone()),
                ("chosen", chosen.clone()),
            ],
            Self::WriteFailed
            | Self::NameExists
            | Self::UpdateNotWritten { .. }
            | Self::UpdateReviewNotWritten { .. } => Vec::new(),
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::Unreadable { problem, .. } | Self::UpdateUnreadable { problem, .. } => {
                Some(problem)
            }
            Self::UpdateNotWritten { reason } | Self::UpdateReviewNotWritten { reason } => {
                Some(reason)
            }
            Self::WriteFailed
            | Self::NameExists
            | Self::NotZip { .. }
            | Self::CorruptEntry { .. }
            | Self::NoManifest { .. }
            | Self::NoText { .. }
            | Self::Languages { .. }
            | Self::Twice { .. }
            | Self::TextChanged { .. }
            | Self::UpdateOtherBook { .. } => None,
        }
    }
}

/// Exporting a book or the stories (unit D2). As tC4's export refusals do, each that tC4
/// words with a diagnosis carries it (`facts.error`), shown before the sentence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ExportRefusal {
    CheckpointFailed {
        error: String,
    },
    ReadFailed {
        error: String,
    },
    /// The file could not be written into the downloads folder, such as on a full disk.
    WriteFailed {
        error: String,
    },
    PrintFailed,
    PrintFailedPictures,
    /// tC4's "{title} has no drafted verse yet, so the PDF has nothing to print."
    NothingDrafted {
        error: String,
    },
}

impl Message for ExportRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::CheckpointFailed { .. } => "export.checkpoint-failed",
            Self::ReadFailed { .. } => "export.read-failed",
            Self::WriteFailed { .. } => "export.write-failed",
            Self::PrintFailed => "export.print-failed",
            Self::PrintFailedPictures => "export.print-failed-pictures",
            Self::NothingDrafted { .. } => "export.nothing-drafted",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::CheckpointFailed { .. } => "refusal.export.checkpoint-failed",
            Self::ReadFailed { .. } => "refusal.export.read-failed",
            Self::WriteFailed { .. } => "refusal.export.write-failed",
            Self::PrintFailed => "refusal.export.print-failed",
            Self::PrintFailedPictures => "refusal.export.print-failed-pictures",
            Self::NothingDrafted { .. } => "refusal.export.nothing-drafted",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        Vec::new()
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::CheckpointFailed { error }
            | Self::ReadFailed { error }
            | Self::WriteFailed { error }
            | Self::NothingDrafted { error } => Some(error),
            Self::PrintFailed | Self::PrintFailedPictures => None,
        }
    }
}

/// A sync that did not complete, by when it stopped. Whether the teammates' work had been
/// received decides what the person reads (PLAN Q8), so a refusal that can only come before
/// intake cannot be paired with "after intake", and no banner says both "Nothing changed"
/// and "received and kept".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SyncFailure {
    /// The project and the repository cannot sync as they are. Found at the gate or by
    /// reading the repository's format and project, before anything is taken (STORAGE §9.1
    /// S0, §9.2 N1, N2): nothing changed.
    Blocked(SyncBlock),
    /// A step failed before the teammates' work was received: nothing changed.
    BeforeIntake(SyncRefusal),
    /// A step failed after the teammates' work was received and integrated (STORAGE §9.3):
    /// the person sees it and it stays.
    AfterIntake(SyncRefusal),
}

/// Why a project and its repository cannot sync (DELTA §3.15.2; tC4 `SYNC_REFUSALS`,
/// `tC4:src/views/SyncResult.jsx:14-51`). Each is found before intake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SyncBlock {
    /// The project has no Door43 repository (S0).
    NotShared,
    /// The repository holds another project (N2).
    ProjectMismatch,
    /// A teammate uses a newer project format (N1).
    NeedsNewerApp { major: u16, min_reader_minor: u16 },
    /// The repository is still in tC4's team format; the team move is offered (N1).
    Tc4TeamRepository,
}

/// Why a sync step failed, before or after intake (STORAGE §9.1, §9.5; DELTA §3.15.2; tC4
/// `SYNC_REFUSALS`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SyncRefusal {
    Offline,
    NotSignedIn,
    AuthFailed,
    NoWriteAccess {
        /// Door43's words, for the operation report. No banner shows them: tC4's sync toast shows the
        /// code's sentence alone (`tC4:src/views/SyncResult.jsx:47-51`, DELTA §3.15.2).
        server_message: String,
    },
    /// Door43 refused the push for a reason of its own. Its words are for the operation
    /// report only: a sync's message never shows a raw reply or a file path (Q36).
    RefRejected {
        server_message: String,
    },
    /// Teammates pushed during every attempt.
    Busy {
        attempts: u8,
    },
    RemoteMissing,
    Unreachable,
    ReadOnly,
    /// A step failed for a reason no other variant names, such as a full disk. The reason is
    /// for the operation report only (Q36).
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
        (matches!(self, Self::AfterIntake(_)) && !says_kept).then_some("syncResult.receivedKept")
    }

    /// The failed step's refusal and whether the received work was kept, or the block.
    fn stage(&self) -> Result<(&SyncRefusal, bool), &SyncBlock> {
        match self {
            Self::Blocked(block) => Err(block),
            Self::BeforeIntake(refusal) => Ok((refusal, false)),
            Self::AfterIntake(refusal) => Ok((refusal, true)),
        }
    }
}

impl Message for SyncFailure {
    fn code(&self) -> Option<&'static str> {
        use SyncRefusal as R;
        match self.stage() {
            Err(SyncBlock::NotShared) => Some("sync.not-shared"),
            Err(
                SyncBlock::ProjectMismatch
                | SyncBlock::NeedsNewerApp { .. }
                | SyncBlock::Tc4TeamRepository,
            ) => None,
            Ok((R::Offline, _)) => Some("sync.offline"),
            Ok((R::AuthFailed, _)) => Some("sync.auth-failed"),
            Ok((R::NoWriteAccess { .. }, _)) => Some("sync.no-write-access"),
            Ok((R::Busy { .. }, _)) => Some("sync.non-fast-forward"),
            Ok((R::Unreachable | R::RemoteMissing, false)) => Some("sync.clone-failed"),
            Ok((R::Unreachable | R::RemoteMissing, true))
            | Ok((R::NotSignedIn | R::RefRejected { .. } | R::ReadOnly | R::Io { .. }, _)) => None,
        }
    }

    // DELTA §3.15.2's table; after intake, a sentence that would say "nothing changed"
    // gives way to one that says the received work was kept (PLAN Q8).
    fn key(&self) -> &'static str {
        use SyncRefusal as R;
        match self.stage() {
            Err(SyncBlock::NotShared) => "syncResult.error.notShared",
            Err(SyncBlock::ProjectMismatch) => "syncResult.error.projectMismatch",
            Err(SyncBlock::NeedsNewerApp { .. }) => "syncResult.error.needsNewerApp",
            Err(SyncBlock::Tc4TeamRepository) => "syncResult.error.tc4TeamRepository",
            Ok((R::Offline, _)) => "syncResult.error.offline",
            Ok((R::NotSignedIn | R::AuthFailed, _)) => "syncResult.error.authFailed",
            Ok((R::NoWriteAccess { .. }, _)) => "syncResult.error.noWriteAccess",
            Ok((R::Busy { .. }, _)) => "syncResult.error.nonFastForward",
            Ok((R::Unreachable | R::RemoteMissing, false)) => "syncResult.error.cloneFailed",
            Ok((R::Unreachable | R::RemoteMissing, true)) => "syncResult.error.notSent",
            Ok((R::RefRejected { .. } | R::Io { .. }, false)) => "syncResult.error.stopped",
            Ok((R::RefRejected { .. } | R::Io { .. }, true)) => "syncResult.error.keptStopped",
            Ok((R::ReadOnly, _)) => "syncResult.error.readOnly",
        }
    }

    fn vars(&self) -> Vec<(&'static str, String)> {
        use SyncRefusal as R;
        match self {
            Self::Blocked(SyncBlock::NeedsNewerApp { major, .. }) => {
                vec![("format", major.to_string())]
            }
            Self::Blocked(
                SyncBlock::NotShared | SyncBlock::ProjectMismatch | SyncBlock::Tc4TeamRepository,
            ) => Vec::new(),
            // Q36: Door43's words and an I/O reason, which can hold a path on this computer,
            // never reach the message; the refusal keeps them for the operation report.
            Self::BeforeIntake(refusal) | Self::AfterIntake(refusal) => match refusal {
                R::RefRejected { .. }
                | R::Io { .. }
                | R::Offline
                | R::NotSignedIn
                | R::AuthFailed
                | R::NoWriteAccess { .. }
                | R::Busy { .. }
                | R::RemoteMissing
                | R::Unreachable
                | R::ReadOnly => Vec::new(),
            },
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
    /// The project is already connected to a Door43 repository: a second share would move it
    /// to another one (a second tab's dialog, or a resend after a lost reply).
    AlreadyShared,
    Offline,
    AuthFailed,
    CreateRejected {
        /// Door43's words, for the operation report. The dialog shows the code's sentence alone
        /// (`tC4:src/views/modals/ShareDialog.jsx:28-29`).
        server_message: String,
    },
    PushFailed {
        reason: String,
    },
}

impl Message for ShareRefusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::NameExists => "share.name-exists",
            Self::AlreadyShared => "share.already-shared",
            Self::Offline => "share.offline",
            Self::AuthFailed => "share.auth-failed",
            Self::CreateRejected { .. } => "share.create-rejected",
            Self::PushFailed { .. } => "share.push-failed",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NameExists => "shareDialog.error.name-exists",
            Self::AlreadyShared => "refusal.share.already-shared",
            Self::Offline => "shareDialog.error.offline",
            Self::AuthFailed => "shareDialog.error.auth-failed",
            Self::CreateRejected { .. } => "shareDialog.error.create-rejected",
            Self::PushFailed { .. } => "shareDialog.error.push-failed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::PushFailed { reason } => vec![("reason", reason.clone())],
            Self::NameExists
            | Self::AlreadyShared
            | Self::Offline
            | Self::AuthFailed
            | Self::CreateRejected { .. } => Vec::new(),
        }
    }
}

/// Open a project from Door43 (unit E1b).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OpenDoor43Refusal {
    /// The project of that repository is on this computer as `project` (ruling H14): there is
    /// never a second copy, and the dialog offers to open this one. Nothing was sent.
    AlreadyHere {
        project: crate::ids::ProjectRef,
    },
    NameExists,
    NoJournal,
    /// An open that did not finish before the app closed; its part copy was removed at the
    /// next start (STORAGE §6.4 N1).
    Removed,
}

impl Message for OpenDoor43Refusal {
    fn code(&self) -> Option<&'static str> {
        Some(match self {
            Self::AlreadyHere { .. } => "open-door43.already-here",
            Self::NameExists => "open-door43.name-exists",
            Self::NoJournal => "open-door43.no-journal",
            Self::Removed => "open-door43.removed",
        })
    }
    fn key(&self) -> &'static str {
        match self {
            Self::AlreadyHere { .. } => "refusal.open-door43.already-here",
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
    /// The check for updates could not be made (tC4 #833.1): one fixed sentence per cause,
    /// never the client's own text.
    UpdateCheckFailed {
        why: UpdateCheckFailure,
    },
    UpgradeFailed {
        reason: String,
    },
    /// The change was not planned or not written; `reason` is the cause, which the dialog shows
    /// after "The change was not applied." (tC4 `gateway.failed`).
    GatewayFailed {
        reason: String,
    },
    /// `books` would lose their checking resource.
    GatewayBlocked {
        books: String,
    },
    /// The download finished but the project's pins were not updated.
    AdoptFailed {
        error: String,
    },
}

/// Why a check for updates could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckFailure {
    /// No answer from Door43: the connection failed, timed out or broke off.
    Unreachable,
    /// Door43 answered but refused the request (a sign-in, a missing repository, a limit).
    Refused,
    /// Door43 answered with an error, or something this app cannot read.
    Server,
}

impl UpdateCheckFailure {
    pub fn key(self) -> &'static str {
        match self {
            Self::Unreachable => "upgrade.checkFailedUnreachable",
            Self::Refused => "upgrade.checkFailedRefused",
            Self::Server => "upgrade.checkFailedServer",
        }
    }
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
            Self::UpdateCheckFailed { why } => why.key(),
            Self::UpgradeFailed { .. } => "upgrade.failed",
            Self::GatewayFailed { .. } => "gateway.failed",
            Self::GatewayBlocked { .. } => "gateway.blockedError",
            Self::AdoptFailed { .. } => "sources.adoptFailed",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::CatalogUnreadable { reason }
            | Self::FixFailed { reason }
            | Self::UpgradeFailed { reason } => vec![("reason", reason.clone())],
            Self::UpdateCheckFailed { .. } => Vec::new(),
            Self::SomeFailed { list } => vec![("list", list.clone())],
            Self::GatewayBlocked { books } => vec![("books", books.clone())],
            Self::AdoptFailed { error } => vec![("error", error.clone())],
            Self::GatewayFailed { .. } => Vec::new(),
        }
    }
    fn detail(&self) -> Option<&str> {
        match self {
            Self::GatewayFailed { reason } => Some(reason),
            _ => None,
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
    /// The file holds another project (R2): "This file is for {name}."
    OtherProject {
        name: String,
    },
    /// The file holds no translationCore project at all (R2's fourth case): nothing to
    /// receive, and Import is the way in for other files.
    NotTranslationCore,
    /// A new project's name is in use on this computer; nothing was written.
    NameExists,
    /// The intake failed after some of the file's packs were folded: what was received
    /// stays in the project (PLAN Q8), so the person is never told nothing changed.
    PartlyReceived {
        reason: String,
    },
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
            Self::OtherProject { .. } => "exchange.error.otherProject",
            Self::NotTranslationCore => "exchange.error.notTranslationCore",
            Self::NameExists => "exchange.error.nameExists",
            Self::PartlyReceived { .. } => "exchange.error.partlyReceived",
            Self::NotWritten { .. } => "exchange.error.notWritten",
            Self::Unreadable { .. } => "exchange.error.unreadable",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NewerFormat { format } => vec![("format", format.to_string())],
            Self::OtherProject { name } => vec![("name", name.clone())],
            Self::NotWritten { reason }
            | Self::Unreadable { reason }
            | Self::PartlyReceived { reason } => vec![("reason", reason.clone())],
            Self::Damaged | Self::VerifyFailed | Self::NotTranslationCore | Self::NameExists => {
                Vec::new()
            }
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

/// A file the UI uploaded could not be stored (APP-DESIGN §5.5). The server keeps no part
/// of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UploadRefusal {
    /// The disk refused the bytes: full, read-only or gone.
    NotWritten { reason: String },
}

impl Message for UploadRefusal {
    fn code(&self) -> Option<&'static str> {
        None
    }
    fn key(&self) -> &'static str {
        match self {
            Self::NotWritten { .. } => "upload.notWritten",
        }
    }
    fn vars(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::NotWritten { reason } => vec![("reason", reason.clone())],
        }
    }
}
