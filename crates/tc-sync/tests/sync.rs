//! tc-sync's tests: the machine against a model of Door43 and a project, and the real sync
//! against `tc-door43-fake`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "sync/model.rs"]
mod model;

#[path = "sync/door43.rs"]
mod door43;

#[path = "sync/exchange.rs"]
mod exchange;
