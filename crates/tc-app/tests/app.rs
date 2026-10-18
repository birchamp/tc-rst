//! The behaviour of tc-app through its public API. One test binary, so the crate and its
//! dependencies link once.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod app {
    pub mod common;
    mod events;
    mod flows;
    mod folders;
    mod jobs;
    mod keychain;
    mod projects_root;
    mod reports;
}
