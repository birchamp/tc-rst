//! The store's tests, in one binary: the contract of the handle (APP-DESIGN R4 to R25), the
//! crash matrix (`docs/STORAGE.md` §16.2), the loss cases of §15.3, the open's progress and
//! operation counts, and the tC4 tests ported (`docs/TC4-DELTA-2026-10-16.md` §4).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "store/common.rs"]
mod common;

#[path = "store/contract.rs"]
mod contract;

#[path = "store/corpus.rs"]
mod corpus;

#[path = "store/crash.rs"]
mod crash;

#[path = "store/loss.rs"]
mod loss;

#[path = "store/opcount.rs"]
mod opcount;

#[path = "store/progress.rs"]
mod progress;

#[path = "store/reflow.rs"]
mod reflow;

#[path = "store/tc4.rs"]
mod tc4;

#[path = "store/consolidation.rs"]
mod consolidation;

#[path = "store/commands.rs"]
mod commands;

#[path = "store/drafts.rs"]
mod drafts;

#[path = "store/review.rs"]
mod review;

#[path = "store/held.rs"]
mod held;

#[path = "store/writer.rs"]
mod writer;

#[path = "store/groups.rs"]
mod groups;
