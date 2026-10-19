//! The application's runtime (APP-DESIGN section 6): what every screen stands on.
//!
//! The decisions are plain Rust that compiles on every target and is tested on the host: the
//! parser of the event stream and the reader's rules for repeats ([`events`]), the errors of a
//! call and the boot's token and build checks ([`client`], [`boot`]), what a screen holds while
//! a read is in flight ([`load`]), which reads a change makes stale ([`cache`]), and the
//! shell's decisions: the view and its screen ([`shell`]), the internet question
//! ([`internet`]), the Door43 sign-in step ([`account`]), the Feedback report ([`feedback`]),
//! the App language dialog ([`language`]), the share dialog ([`share`]), Open from Door43
//! ([`open_door43`]), the draft offer and read-only banner ([`offer`]), and the sync's result
//! ([`sync`]). Only the glue that runs them in a browser (`fetch`, the DOM, the page's storage)
//! is in `web`, for wasm32 with the `app` feature, and it holds no decision of its own.
//!
//! The modules index with `get` and never with `[]` (APP-DESIGN 6.5, A-15): a panic in
//! WebAssembly stops the whole UI.
#![deny(clippy::indexing_slicing)]

pub mod account;
pub mod boot;
pub mod buffer;
pub mod cache;
pub mod client;
pub mod confirm;
pub mod effects;
pub mod events;
pub mod feedback;
pub mod forms;
pub mod home;
pub mod internet;
pub mod language;
pub mod load;
pub mod offer;
pub mod open_door43;
pub mod recovery;
pub mod save;
pub mod share;
pub mod shell;
pub mod sources;
pub mod sync;
pub mod translate;
pub mod understand;

#[cfg(all(target_arch = "wasm32", feature = "app"))]
pub mod web;
