//! The application services of the tC4 rebuild (APP-DESIGN §4): the operations behind every
//! screen, the background jobs and their progress, the event stream, the flows that run one
//! at a time, this computer's preferences, the operation reports and the reply cache.
//!
//! The crate does no HTTP. [`App::call`] answers one request as bytes, exactly as the
//! server sends them; the server routes to it. [`backend`] is the storage seam; with the
//! feature `fixture-backend` it holds the memory backend and STORAGE §13.2's state seed of a
//! tC4 project. [`session`] keeps one session per open folder, [`counters`] counts what the
//! reads of a project cost, and [`wire`] converts the seam's types into tc-api's.

pub mod account;
pub mod app;
pub mod backend;
pub mod bundle;
pub mod clock;
pub mod config;
pub mod counters;
pub mod dispatch;
pub mod drafted;
pub mod drafts;
pub mod events;
pub mod feedback;
pub mod flows;
pub mod folders;
mod handlers;
pub mod home;
#[cfg(feature = "test-hooks")]
pub mod hooks;
pub mod jobs;
mod js;
pub mod language_search;
pub mod prefs;
pub mod replies;
pub mod reports;
mod seed;
pub mod session;
mod startup;
pub mod wire;

pub use app::{Answer, App, AppDeps, CallFailure, FilesChoice, StartError, StoreChoice, Then};
pub use dispatch::Handle;
pub use seed::RESOURCES_FOLDER;
