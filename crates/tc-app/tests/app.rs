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
#[path = "support/rail_differential.rs"]
mod rail_differential;
#[path = "support/store_world.rs"]
mod store_world;

mod app {
    mod account;
    mod align;
    mod app_data;
    mod app_start;
    mod check;
    mod check_item;
    mod check_story;
    pub mod common;
    mod conflicts;
    mod create;
    mod dispatch;
    mod drafted;
    mod drafts;
    mod events;
    mod export;
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
    mod port_contract;
    mod prefs;
    mod projects_root;
    mod replies;
    mod reports;
    mod save_machine;
    pub mod scenario;
    mod section;
    mod seed;
    mod sessions;
    mod share;
    mod sources;
    pub mod state_seed;
    mod store;
    pub mod store_scenario;
    mod story;
    pub mod sync;
    mod test_hook_branches;
    mod translate;
    pub mod understand;
    mod upgrades;
    mod wire;
}
