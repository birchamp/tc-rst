//! Errors. Each names what it is about, so the app can tell a translator what happened in
//! plain words, and none carries a secret.

use std::io;
use std::path::PathBuf;

use tc_door43::{Door43Error, UnreachableCause, VerificationError};

use crate::pin::Sha256;

/// A value from outside that does not have the expected form.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("{0:?} is not a Door43 repository path (host/owner/repository)")]
    RepoPath(String),
    #[error("{0:?} is not a book coverage record (book codes, or BIBLE alone)")]
    Coverage(String),
    #[error("{0:?} is not a SHA-256 digest")]
    Sha256(String),
    #[error("{0:?} is not a GitHub account or repository name")]
    GithubName(String),
    #[error("{0:?} is not an ISO 639-3 language code")]
    AquiferLanguage(String),
    #[error("{0:?} is not a release asset name")]
    AssetName(String),
    #[error("{0:?} is not a pin slot name or pane id")]
    Slot(String),
    #[error("pins: {0}")]
    Pins(String),
}

/// A pin tC4's §5.3 grammar refuses, found while writing pins: nothing is written. tC4
/// requires a flavor of every pin; a pin from a release tag gets one when its resource
/// is installed ([`crate::Installed::dcs_pin`]).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("slot {slot}: {pin} has no flavor, which every pin needs")]
pub struct UnwritablePin {
    pub slot: String,
    pub pin: String,
}

/// A problem with the resource store on disk.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("{action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    /// An install folder whose receipt is missing or unreadable. The folder is left as it
    /// is: something other than this crate changed it, and nothing in it is trusted.
    #[error("the install at {path} has no readable receipt: {problem}")]
    BadReceipt { path: PathBuf, problem: String },
    /// The folder for this identity holds another install. It is left as it is.
    #[error("the folder {path} holds {holds}, not the resource asked for")]
    Occupied { path: PathBuf, holds: String },
    /// An installed file no longer matches its receipt: something outside the app changed
    /// it. Nothing is repaired in place.
    #[error("the install at {path} is damaged: {file}: {problem}")]
    Damaged {
        path: PathBuf,
        file: String,
        problem: String,
    },
    #[error("{install} holds no file {file:?}")]
    NoSuchFile { install: String, file: String },
}

impl StoreError {
    pub(crate) fn io(action: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            action,
            path: path.into(),
            source,
        }
    }
}

/// Why bytes offered for an install were refused. Nothing from them was installed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// tc-door43's checks of a Door43 download (revision, archive comment, tree,
    /// ingredient checksums, zip safety).
    #[error(transparent)]
    Door43(#[from] VerificationError),
    #[error("the asset's SHA-256 is {found}, but the pin expects {expected}")]
    Sha256Mismatch { expected: Sha256, found: Sha256 },
    #[error("the export names repository {found:?}, but the pin is {expected:?}")]
    WrongRepository { expected: String, found: String },
    #[error("the resource is in language {found:?}, but the pin expects {expected:?}")]
    WrongLanguage { expected: String, found: String },
    #[error("it is not a readable zip archive: {0}")]
    NotAZip(String),
    #[error("entry {path:?} is corrupt: {reason}")]
    CorruptEntry { path: String, reason: String },
    #[error("entry {0:?} escapes the archive's folder")]
    EscapesRoot(String),
    #[error("entry {0:?} is a symbolic link")]
    Symlink(String),
    #[error("two entries unpack to {0:?}")]
    DuplicateEntry(String),
    #[error("the unpacked files exceed the {limit}-byte limit")]
    TooLargeUnpacked { limit: u64 },
    #[error("the archive is not an Aquifer release: {0}")]
    NotAquifer(String),
    #[error("{0}")]
    Unreadable(String),
}

/// Why an install did not happen. In every case nothing was installed, and an install
/// that was already complete is untouched.
#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    /// A Door43 request failed, including `Offline` (nothing was sent) and a download
    /// that failed tc-door43's verification.
    #[error("{0}")]
    Door43(#[from] Door43Error),
    /// A GitHub request for an Aquifer asset failed, including `Offline`.
    #[error("{0}")]
    Aquifer(#[from] AquiferError),
    #[error("{what} was refused: {refusal}")]
    Refused { what: String, refusal: Refusal },
    /// A burrito export is asked for by tag, and the pin names none.
    #[error("{pin} has no version tag, so it can be installed only from the commit's archive")]
    NoTag { pin: String },
    #[error(transparent)]
    Store(#[from] StoreError),
    /// The install stopped part way, at an injected fault (tests only).
    #[error("the install was interrupted")]
    Interrupted,
}

/// Why a GitHub request for an Aquifer release failed. `route` is the method and path.
#[derive(Debug, thiserror::Error)]
pub enum AquiferError {
    /// The internet switch is off. Nothing was sent.
    #[error("{route}: not sent, because the internet is off for this session")]
    Offline { route: String },
    #[error("{route}: GitHub cannot be reached ({cause})")]
    Unreachable {
        route: String,
        cause: UnreachableCause,
    },
    #[error("{route}: not found")]
    NotFound { route: String },
    #[error("{route}: GitHub asked the app to slow down (HTTP {status})")]
    RateLimited { route: String, status: u16 },
    #[error("{route}: GitHub answered HTTP {status}")]
    Http { route: String, status: u16 },
    #[error("{route}: unexpected answer: {reason}")]
    UnexpectedResponse { route: String, reason: String },
    #[error("{route}: the answer is larger than the {limit}-byte limit")]
    TooLarge { route: String, limit: u64 },
    #[error("the GitHub client could not start: {reason}")]
    Setup { reason: String },
}
