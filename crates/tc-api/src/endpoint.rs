//! Endpoints (APP-DESIGN §3.3, AD9). A request type is its endpoint: it knows its path, its
//! kind, the internet question it needs and its reply type. Every endpoint is one row of
//! [`endpoint_table!`](crate::endpoint_table), from which this crate implements
//! [`Endpoint`] and builds [`ALL`], and from which the server builds its routes and the
//! application its handlers, so the three cannot drift.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::ids::{RequestId, Scope};
use crate::jobs::JobOutcome;
#[cfg(feature = "test-hooks")]
use crate::test_hook_table;

/// `POST /api/v1/{path}` with the header `X-Tc-Api: {API_VERSION}`.
pub const API_VERSION: u32 = 1;

/// The header that carries [`API_VERSION`].
pub const API_VERSION_HEADER: &str = "X-Tc-Api";

/// The header that carries the launch's token on every request (APP-DESIGN §5.2).
pub const TOKEN_HEADER: &str = "X-Tc-Token";

/// `GET`, the event stream of §5.4: `?after=<seq>` asks for the frames after one the reader
/// holds.
pub const EVENTS_PATH: &str = "/api/v1/events";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// Reads; never changes state. Repeating one is always safe.
    Query,
    /// Changes state. It carries a `RequestId`, so a retry after a lost reply gets the first
    /// reply instead of running again.
    Command,
    /// Starts a background job and answers with its id. It carries a `RequestId`, so a
    /// retry gets the first job.
    StartJob,
}

/// The internet question a request needs first: tC4's `net.ask.<kind>` (AD25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum NetKind {
    Share,
    /// tC4's catalogs have its question, but no endpoint asks it: the owner's ruling Q2
    /// removed the Upload changes dialog.
    Upload,
    SignIn,
    CheckSignIn,
    Sources,
    Fix,
    ImportVersions,
    Profile,
    RepoLink,
    HelpLink,
    Feedback,
    Sync,
    Open,
}

impl NetKind {
    pub const ALL: [NetKind; 13] = [
        Self::Share,
        Self::Upload,
        Self::SignIn,
        Self::CheckSignIn,
        Self::Sources,
        Self::Fix,
        Self::ImportVersions,
        Self::Profile,
        Self::RepoLink,
        Self::HelpLink,
        Self::Feedback,
        Self::Sync,
        Self::Open,
    ];

    /// The catalog key of what the question says the step does.
    pub fn ask_key(self) -> &'static str {
        match self {
            Self::Share => "net.ask.share",
            Self::Upload => "net.ask.upload",
            Self::SignIn => "net.ask.signIn",
            Self::CheckSignIn => "net.ask.checkSignIn",
            Self::Sources => "net.ask.sources",
            Self::Fix => "net.ask.fix",
            Self::ImportVersions => "net.ask.importVersions",
            Self::Profile => "net.ask.profile",
            Self::RepoLink => "net.ask.repoLink",
            Self::HelpLink => "net.ask.helpLink",
            Self::Feedback => "net.ask.feedback",
            Self::Sync => "net.ask.sync",
            Self::Open => "net.ask.open",
        }
    }
}

/// One request type and what it answers.
pub trait Endpoint: Serialize + DeserializeOwned + 'static {
    /// The path under `/api/v1/`: `service/name`.
    const PATH: &'static str;
    const KIND: Kind;
    /// The internet question the UI asks before it sends this request (AD25). The server
    /// still answers `NeedsInternet` when the switch went off meanwhile.
    const NET: Option<NetKind>;
    type Ok: Serialize + DeserializeOwned;
}

/// A request that changes state: every `Command` and `StartJob` implements it, which
/// `endpoint_table!` checks when it compiles.
pub trait Write: Endpoint {
    fn request(&self) -> RequestId;
    /// The open project this write applies to; `None` for a write that is not
    /// project-scoped. The server checks it before the reply cache, so a write against a
    /// closed or replaced open is refused and never answered from the cache of another.
    fn at(&self) -> Option<&Scope>;
}

/// A `job` row. The outcome its job ends with is typed per request, so the follower of a
/// download cannot be handed an open's outcome, and the server cannot send one.
pub trait StartsJob: Write {
    type Outcome;
    /// The outcome of this row's job, or the outcome back when it is of another kind.
    fn outcome(outcome: JobOutcome) -> Result<Self::Outcome, JobOutcome>;
    fn wrap(outcome: Self::Outcome) -> JobOutcome;
}

/// Whether a write row is project-scoped. Every `command` and `job` row needs one of
/// [`scoped!`] or [`unscoped!`], so a unit that adds a write must say which it is.
trait ScopeOf {
    fn scope_of(&self) -> Option<&Scope>;
}

/// Writes whose `at` names the open project they apply to.
macro_rules! scoped {
    ($($req:path),* $(,)?) => {
        $(
            impl ScopeOf for $req {
                fn scope_of(&self) -> Option<&Scope> {
                    Some(&self.at)
                }
            }
        )*
    };
}

/// Writes that apply to no project.
macro_rules! unscoped {
    ($($req:path),* $(,)?) => {
        $(
            impl ScopeOf for $req {
                fn scope_of(&self) -> Option<&Scope> {
                    None
                }
            }
        )*
    };
}

scoped!(
    crate::editor::AddComment,
    crate::story::SaveFrame,
    crate::story::SaveRef,
    crate::editor::SaveVerse,
    crate::editor::SaveLayout,
    crate::editor::SaveSection,
    crate::editor::Draft,
    crate::editor::DiscardDraft,
    crate::prefs::SetDraftUnit,
    crate::prefs::SetAlignSuggestions,
    crate::prefs::SetRailFolded,
    crate::prefs::RecordPlace,
    crate::session::Close,
    crate::session::Drafts,
    crate::session::KeepDraft,
    crate::session::DiscardDraft,
    crate::session::CopyProject,
    crate::session::OtherComputerClosed,
    crate::resources::Download,
    crate::check::Decide,
    crate::resources::PlanUpgrade,
    crate::resources::PlanGateway,
    crate::resources::ApplyPlan,
    crate::resources::FixFetch,
    crate::resources::FixSideload,
    crate::resources::FixRepin,
    crate::align::Open,
    crate::align::Edit,
    crate::align::KeepReflow,
    crate::align::Suggest,
    crate::align::Flush,
    crate::align::Close,
    crate::conflicts::Resolve,
    crate::conflicts::ResolveBulk,
);
unscoped!(
    crate::app::Quit,
    crate::app::ClientError,
    crate::prefs::SetLocale,
    crate::net::Set,
    crate::jobs::Cancel,
    crate::account::SignIn,
    crate::account::CancelSignIn,
    crate::account::Resume,
    crate::account::SignOut,
    crate::session::Open,
    crate::door43::OpenDoor43,
    crate::door43::SyncProject,
    crate::settings::Save,
    crate::create::Bible,
    crate::create::Obs,
    crate::create::AddBlankBooks,
    crate::create::AddUsfmBooks,
);
#[cfg(feature = "test-hooks")]
unscoped!(
    crate::test::Crash,
    crate::test::Hold,
    crate::test::Release,
    crate::test::FieldStore,
    crate::test::FieldDraft,
    crate::test::Keychain,
    crate::test::Reset,
    crate::test::LastEdit,
    crate::test::Script,
);

/// What an endpoint answers inside an HTTP 200 (AD9). On the wire it follows the crate's
/// rule for enums: `{"type": "ok", "value": <reply>}` or `{"type": "err", "value":
/// <ApiError>}`. Serde's own form of `Result`, `{"Ok": …}`, would break that rule at the top
/// of every reply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum Reply<T> {
    Ok(T),
    Err(ApiError),
}

/// The reply of endpoint `E`.
pub type ReplyOf<E> = Reply<<E as Endpoint>::Ok>;

impl<T> Reply<T> {
    /// The reply as a `Result`, for `?` and one exhaustive match.
    pub fn into_result(self) -> Result<T, ApiError> {
        match self {
            Self::Ok(ok) => Ok(ok),
            Self::Err(error) => Err(error),
        }
    }
}

impl<T> From<Result<T, ApiError>> for Reply<T> {
    fn from(result: Result<T, ApiError>) -> Self {
        match result {
            Ok(ok) => Self::Ok(ok),
            Err(error) => Self::Err(error),
        }
    }
}

/// One row of [`ALL`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointInfo {
    pub path: &'static str,
    pub kind: Kind,
    pub net: Option<NetKind>,
}

#[doc(hidden)]
#[macro_export]
macro_rules! __kind {
    (query) => {
        $crate::endpoint::Kind::Query
    };
    (command) => {
        $crate::endpoint::Kind::Command
    };
    (job) => {
        $crate::endpoint::Kind::StartJob
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __net {
    () => {
        None
    };
    ($net:ident) => {
        Some($crate::endpoint::NetKind::$net)
    };
}

macro_rules! write_impl {
    (query $req:path) => {};
    (command $req:path) => {
        impl Write for $req {
            fn request(&self) -> RequestId {
                self.request
            }

            fn at(&self) -> Option<&Scope> {
                ScopeOf::scope_of(self)
            }
        }
    };
    (job $req:path) => {
        write_impl!(command $req);
        // Every job row names the outcome its job ends with.
        const _: fn() = || {
            fn starts_job<T: StartsJob>() {}
            starts_job::<$req>();
        };
    };
}

macro_rules! define_endpoints {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        $(
            impl Endpoint for $req {
                const PATH: &'static str = $path;
                const KIND: Kind = $crate::__kind!($kind);
                const NET: Option<NetKind> = $crate::__net!($($net)?);
                type Ok = $ok;
            }
            write_impl!($kind $req);
        )*
    };
}

macro_rules! endpoint_infos {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        &[$(
            EndpointInfo {
                path: $path,
                kind: $crate::__kind!($kind),
                net: $crate::__net!($($net)?),
            },
        )*]
    };
}

crate::endpoint_table!(define_endpoints);
#[cfg(feature = "test-hooks")]
crate::test_hook_table!(define_endpoints);

/// Every endpoint the server routes, in table order: the test hooks too when the
/// `test-hooks` feature is on.
#[cfg(not(feature = "test-hooks"))]
pub const ALL: &[EndpointInfo] = crate::endpoint_table!(endpoint_infos);
#[cfg(feature = "test-hooks")]
pub const ALL: &[EndpointInfo] = crate::endpoint_table!(test_hook_table endpoint_infos);
