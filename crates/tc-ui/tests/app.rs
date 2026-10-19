//! The behaviour of the app runtime's pure parts through the public API of tc-ui. One test
//! binary, so the crate and its dependencies link once.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod app {
    mod account;
    mod buffer;
    mod cache;
    mod client;
    mod effects;
    mod feedback;
    mod forms;
    mod home;
    mod indicator;
    mod internet;
    mod language;
    mod recovery;
    mod save;
    pub mod save_kit;
    mod save_props;
    mod save_refresh;
    mod share;
    mod shell;
    mod sse;
}
