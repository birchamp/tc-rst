//! The wire types the tC4 rebuild's server and UI share (APP-DESIGN §3): identifiers,
//! tokens, shown values, write acknowledgements, errors and refusals, events and jobs, and
//! one request type per endpoint. The server and the UI ship as one build, so these types
//! are the whole contract between them.
//!
//! The crate does no I/O and reads no clock, and it builds for `wasm32-unknown-unknown`.
//!
//! On the wire, JSON:
//! - Struct fields are camelCase. A request refuses a field it does not know, at any depth.
//! - An enum whose variants carry nothing is a camelCase string: `"rtl"`.
//! - Any other enum is tagged: `{"type": "<camelCase variant>", "value": <data>}`, with no
//!   `value` for a variant that carries nothing. The data of a variant with named fields is
//!   an object with camelCase fields. Serde's internal tagging (the tag beside the data's
//!   own fields) cannot carry a variant that holds a string or a number, and an enum inside
//!   an enum would put two tags in one object, so every data enum uses this one form.
//! - A value that is one identifier written as text is a string whatever it holds: a book is
//!   `"TIT"`, a check's place `"PSA 60:front"` (a rail repeats it thousands of times).
//! - Every enum is closed: an unknown variant is refused, never read as a fallback.
//! - A reply is the enum [`Reply`]: `{"type": "ok", "value": …}` or `{"type": "err",
//!   "value": <ApiError>}`.
//! - Every identifier checks its format when it is read.

#[cfg(feature = "proptest")]
mod arb;

pub mod account;
pub mod align;
pub mod app;
pub mod buffer;
pub mod check;
pub mod endpoint;
pub mod error;
pub mod event;
pub mod feedback;
pub mod home;
pub mod ids;
pub mod jobs;
pub mod net;
pub mod prefs;
pub mod query;
pub mod refusal;
pub mod shown;
#[cfg(feature = "test-hooks")]
pub mod test;
pub mod token;
pub mod upload;
pub mod write;

pub use endpoint::{
    ALL, API_VERSION, Endpoint, EndpointInfo, Kind, NetKind, Reply, ReplyOf, Write,
};
pub use error::ApiError;
pub use ids::{RequestId, Scope};
pub use refusal::{Message, Refusal};
pub use shown::Shown;
pub use token::{ReviewToken, ShownToken};
pub use write::{Ack, NotSaved};

/// Every product endpoint, one row each, as tokens for a callback macro:
/// `kind [NetKind]? "service/name": RequestType => ReplyType;`, where `kind` is `query`,
/// `command` or `job`. This crate implements `Endpoint` from it and builds `ALL`; the
/// server routes it and the application implements a handler for every row, so an
/// endpoint without a route or a handler does not compile.
///
/// Call it as `endpoint_table!(callback)`; tokens after the callback's name are passed to
/// the callback before the rows.
///
/// The rows are the endpoints of phase 0 (APP-DESIGN §3.9, §10). Each later unit adds the
/// rows of its screens.
#[macro_export]
macro_rules! endpoint_table {
    ($callback:ident $($prefix:tt)*) => {
        $callback! {
            $($prefix)*
            query "app/hello": $crate::app::Hello => $crate::app::HelloView;
            command "app/quit": $crate::app::Quit => ();
            command "app/clientError": $crate::app::ClientError => ();

            command "prefs/setLocale": $crate::prefs::SetLocale => ();
            command "prefs/setDraftUnit": $crate::prefs::SetDraftUnit => ();
            command "prefs/setAlignSuggestions": $crate::prefs::SetAlignSuggestions => ();
            command "prefs/recordPlace": $crate::prefs::RecordPlace => ();

            query "net/state": $crate::net::State => $crate::net::NetView;
            command "net/set": $crate::net::Set => $crate::net::NetView;

            query "jobs/get": $crate::jobs::Get => Option<$crate::jobs::JobView>;
            command "jobs/cancel": $crate::jobs::Cancel => ();

            query "account/state": $crate::account::State => $crate::account::AccountView;
            command [SignIn] "account/signIn": $crate::account::SignIn
                => $crate::account::AccountView;
            command "account/cancelSignIn": $crate::account::CancelSignIn => ();
            command [CheckSignIn] "account/resume": $crate::account::Resume
                => $crate::account::ResumeView;
            command "account/signOut": $crate::account::SignOut => $crate::account::AccountView;
            query [Profile] "account/profileLink": $crate::account::ProfileLink
                => $crate::account::LinkView;
            query [HelpLink] "account/helpLink": $crate::account::HelpLink
                => $crate::account::LinkView;

            query "feedback/compose": $crate::feedback::Compose => $crate::feedback::FeedbackDraft;
        }
    };
}

/// The `test` service's endpoints, in the form of [`endpoint_table!`]. It exists only with
/// the `test-hooks` feature (AD35).
#[cfg(feature = "test-hooks")]
#[macro_export]
macro_rules! test_hook_table {
    ($callback:ident $($prefix:tt)*) => {
        $callback! {
            $($prefix)*
            query "test/quiet": $crate::test::Quiet => $crate::test::QuietView;
            query "test/echo": $crate::test::Echo => $crate::test::EchoView;
            command "test/crash": $crate::test::Crash => ();
            command "test/hold": $crate::test::Hold => ();
            command "test/release": $crate::test::Release => ();
            command "test/fieldStore": $crate::test::FieldStore => $crate::write::Ack;
            command "test/saveVerse": $crate::test::SaveVerse => $crate::write::Ack;
            query "test/fieldLog": $crate::test::FieldLog => $crate::test::FieldLogView;
            command "test/fieldDraft": $crate::test::FieldDraft => ();
            command "test/reset": $crate::test::Reset => ();
        }
    };
}
