//! The behaviour of the app runtime's pure parts through the public API of tc-ui. One test
//! binary, so the crate and its dependencies link once.
#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod app {
    mod account;
    mod buffer;
    mod cache;
    mod client;
    mod draft_machine;
    mod effects;
    mod feedback;
    mod fix;
    mod forms;
    mod home;
    mod indicator;
    mod internet;
    mod jobs;
    mod language;
    mod latest;
    mod lint;
    mod offer;
    mod print;
    mod recovery;
    mod review;
    mod save;
    pub mod save_kit;
    mod save_props;
    mod save_refresh;
    mod section;
    mod share;
    mod shell;
    mod shell_contract;
    mod sources;
    mod sse;
    mod story;
    mod sync;
    mod translate;
    mod understand;
    mod upgrades;
}
