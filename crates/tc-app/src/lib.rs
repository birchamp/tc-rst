//! The application services of the tC4 rebuild (APP-DESIGN §4): the operations behind every
//! screen, the background jobs and their progress, the event stream, the flows that run one
//! at a time, this computer's preferences, the operation reports and the reply cache.
//!
//! The crate does no HTTP. [`App::call`] answers one request as bytes, exactly as the
//! server sends them; the server routes to it. [`backend`] is the storage side; with the
//! feature `fixture-backend` it holds STORAGE §13.2's state seed of a tC4 project.

pub mod account;
pub mod app;
pub mod backend;
pub mod bundle;
pub mod clock;
pub mod config;
pub mod counters;
pub mod dispatch;
pub mod events;
pub mod flows;
pub mod folders;
mod handlers;
#[cfg(feature = "test-hooks")]
pub mod hooks;
pub mod jobs;
pub mod prefs;
pub mod replies;
pub mod reports;
mod seed;
pub mod sessions;
mod startup;
pub mod wire;

pub use app::{Answer, App, AppDeps, CallFailure, StartError, Then};
pub use dispatch::Handle;
pub use seed::RESOURCES_FOLDER;
