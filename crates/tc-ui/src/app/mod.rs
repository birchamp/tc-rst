//! The application's runtime (APP-DESIGN section 6): what every screen stands on.
//!
//! The decisions are plain Rust that compiles on every target and is tested on the host: the
//! parser of the event stream and the reader's rules for repeats ([`events`]), the errors of a
//! call and the boot's token and build checks ([`client`], [`boot`]), what a screen holds while
//! a read is in flight ([`load`]), and which reads a change makes stale ([`cache`]). Only the
//! glue that runs them in a browser (`fetch`, the DOM, the page's storage) is in `web`, for
//! wasm32 with the `app` feature, and it holds no decision of its own.
//!
//! The modules index with `get` and never with `[]` (APP-DESIGN 6.5, A-15): a panic in
//! WebAssembly stops the whole UI.
#![deny(clippy::indexing_slicing)]

pub mod boot;
pub mod buffer;
pub mod cache;
pub mod client;
pub mod effects;
pub mod events;
pub mod load;

#[cfg(all(target_arch = "wasm32", feature = "app"))]
pub mod web;
