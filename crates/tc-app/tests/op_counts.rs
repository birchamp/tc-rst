//! The operation-count tests of tc-app (PERFORMANCE rule 2, APP-DESIGN §8.1, §8.2, AD32): each
//! action runs on tc-corpus Psalms and on the corpus ten times larger, through a real `App`
//! over the memory backend and over the store (C1a), and fails when a count grows. One test binary, so the corpora are
//! generated and folded once.
// Tests read whole folds to check results; the lint on whole-fold walks guards the product's
// paths (crates/tc-app/clippy.toml).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

#[path = "app/common.rs"]
mod common;
#[path = "support/rail_differential.rs"]
mod rail_differential;
#[path = "support/store_world.rs"]
mod store_world;

mod op_counts {
    mod check;
    pub mod corpus;
    pub mod harness;
    mod home;
    mod lint;
    mod plans;
    mod probes;
    mod section;
    mod session;
    mod understand;
}
