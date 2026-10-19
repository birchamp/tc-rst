//! The behaviour of tc-app through its public API. One test binary, so the crate and its
//! dependencies link once.
// Tests read whole folds to check results; the lint on whole-fold walks guards the product's
// paths (crates/tc-app/clippy.toml).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

#[path = "support/counting_store.rs"]
mod counting_store;
#[path = "support/store_world.rs"]
mod store_world;

mod app {
    mod account;
    mod app_data;
    mod app_start;
    pub mod common;
    mod dispatch;
    mod drafted;
    mod drafts;
    mod events;
    mod feedback;
    mod field_store;
    mod flows;
    mod folders;
    mod hello;
    mod home;
    mod hooks;
    mod internet;
    mod jobs;
    mod keychain;
    mod language_search;
    mod memory;
    mod prefs;
    mod projects_root;
    mod replies;
    mod reports;
    mod save_machine;
    pub mod scenario;
    mod seed;
    mod sessions;
    mod share;
    pub mod state_seed;
    mod store;
    mod store_scenario;
    mod understand;
    mod wire;
}
