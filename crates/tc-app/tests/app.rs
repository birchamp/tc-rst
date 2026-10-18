//! The behaviour of tc-app through its public API. One test binary, so the crate and its
//! dependencies link once.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod app {
    mod app_start;
    pub mod common;
    mod dispatch;
    mod events;
    mod field_store;
    mod flows;
    mod folders;
    mod hello;
    mod hooks;
    mod internet;
    mod jobs;
    mod keychain;
    mod prefs;
    mod projects_root;
    mod replies;
    mod reports;
}
