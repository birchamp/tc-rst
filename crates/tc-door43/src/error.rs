use crate::git_tree::ObjectId;
use crate::names::CommitSha;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Why a Door43 call failed. `route` is the method and path, such as
/// `GET /api/v1/repos/unfoldingWord/en_tn/tags`. A route is built from checked names only,
/// and no variant carries a request header or body, so no error holds a secret.
#[derive(Debug, thiserror::Error)]
pub enum Door43Error {
    /// The internet switch is off. Nothing was sent.
    #[error("{route}: not sent, because the internet is off for this session")]
    Offline { route: String },
    /// No answer: the name did not resolve, the connection was refused or timed out, or
    /// the transfer broke off.
    #[error("{route}: Door43 cannot be reached ({cause})")]
    Unreachable {
        route: String,
        cause: UnreachableCause,
    },
    /// 401 or 403: the password or token was refused, or the token lacks a scope.
    #[error("{route}: Door43 refused the sign-in ({status}): {message}")]
    AuthFailed {
        route: String,
        status: AuthStatus,
        message: String,
    },
    #[error("{route}: not found: {message}")]
    NotFound { route: String, message: String },
    /// 409: a repository with this name exists already.
    #[error("{route}: the name is taken: {message}")]
    NameExists { route: String, message: String },
    /// HTTP 429. `retry_after` is the server's `Retry-After`, sent in seconds or as an HTTP
    /// date (then measured from now, and zero once it has passed).
    #[error("{route}: Door43 asked the app to slow down")]
    RateLimited {
        route: String,
        retry_after: Option<Duration>,
    },
    /// 5xx.
    #[error("{route}: Door43 failed (HTTP {status}): {message}")]
    Server {
        route: String,
        status: u16,
        message: String,
    },
    /// Any other 4xx, such as 400 for a token name in use or 422 for a bad field.
    #[error("{route}: Door43 refused the request (HTTP {status}): {message}")]
    Rejected {
        route: String,
        status: u16,
        message: String,
    },
    /// A success answer this client cannot use: malformed JSON, a missing field, an
    /// address on another server, a redirect to another host.
    #[error("{route}: unexpected answer: {reason}")]
    UnexpectedResponse { route: String, reason: String },
    #[error("{route}: the answer is larger than the {limit}-byte limit")]
    TooLarge { route: String, limit: u64 },
    /// The downloaded bytes are not what was asked for. Nothing from them may be used.
    #[error("{route}: the download failed verification: {source}")]
    Verification {
        route: String,
        #[source]
        source: VerificationError,
    },
    /// The download's watch asked it to stop. Nothing of it was kept.
    #[error("{route}: the download was cancelled")]
    Cancelled { route: String },
    /// A download could not be written to its file, or read back for its check. Nothing of
    /// it was kept.
    #[error("{route}: the download could not be kept in {path}: {source}")]
    Disk {
        route: String,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The HTTP client could not be built (for example, no TLS backend).
    #[error("the Door43 client could not start: {reason}")]
    Setup { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    /// 401: the credentials are wrong, or the token was revoked or expired.
    Unauthorized,
    /// 403: the account or token may not do this, such as a token without a scope.
    Forbidden,
}

impl fmt::Display for AuthStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AuthStatus::Unauthorized => "HTTP 401",
            AuthStatus::Forbidden => "HTTP 403",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnreachableCause {
    /// DNS failed or the connection (TCP or TLS) could not be made.
    Connect,
    Timeout,
    /// The answer started and then broke off.
    Interrupted,
    Other(String),
}

impl fmt::Display for UnreachableCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnreachableCause::Connect => f.write_str("could not connect"),
            UnreachableCause::Timeout => f.write_str("timed out"),
            UnreachableCause::Interrupted => f.write_str("the transfer broke off"),
            UnreachableCause::Other(reason) => f.write_str(reason),
        }
    }
}

/// Why downloaded bytes were refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VerificationError {
    #[error("it is not a readable zip archive: {0}")]
    NotAZip(String),
    #[error("entry {path:?} is corrupt: {reason}")]
    CorruptEntry { path: String, reason: String },
    #[error("an entry name is not UTF-8")]
    NonUtf8Name,
    #[error("entry {0:?} escapes the archive's folder")]
    EscapesRoot(String),
    #[error("two entries unpack to {0:?}")]
    DuplicateEntry(String),
    /// The names differ, but a case-insensitive or normalization-insensitive file system
    /// (the Windows and macOS defaults), or Windows' removal of trailing dots and spaces,
    /// would make them one file or folder, and writing both would lose one.
    #[error("entries {first:?} and {second:?} would be one file on Windows or macOS")]
    CollidingNames { first: String, second: String },
    #[error("entry {path:?} cannot be written on every platform: {reason}")]
    NotPortable { path: String, reason: &'static str },
    #[error("entry {0:?} is a symbolic link, which a burrito never holds")]
    SymlinkInBurrito(String),
    #[error("the archive holds no files")]
    Empty,
    #[error("the archive does not hold its files in one top folder")]
    NotWrapped,
    #[error("the unpacked files exceed the {limit}-byte limit")]
    TooLargeUnpacked { limit: u64 },
    #[error("the archive carries no commit id in its comment")]
    ArchiveCommentMissing,
    #[error("the archive says it holds commit {found:?}, not {expected}")]
    ArchiveCommentMismatch { expected: CommitSha, found: String },
    #[error("entry {path:?} cannot come from a git commit: {reason}")]
    NotGitContent { path: String, reason: &'static str },
    #[error("the files hash to git tree {computed}, but commit {commit} has tree {expected}")]
    TreeMismatch {
        commit: CommitSha,
        expected: ObjectId,
        computed: ObjectId,
    },
    #[error("there is no metadata.json at the burrito root")]
    MetadataMissing,
    #[error("metadata.json is unreadable: {0}")]
    MetadataUnreadable(String),
    #[error("there is no ingredients/ folder")]
    NoIngredients,
    #[error("metadata.json declares no DCS revision")]
    RevisionMissing,
    #[error("metadata.json declares {0} DCS revisions; exactly one is expected")]
    RevisionAmbiguous(usize),
    #[error("the export declares revision {declared:?}, but {expected} was expected")]
    RevisionMismatch {
        expected: CommitSha,
        declared: String,
    },
    #[error("ingredient {path:?} does not match metadata.json: {detail}")]
    IngredientMismatch { path: String, detail: String },
}
