//! Door43 sync and USB exchange (`docs/STORAGE.md` §9, §10).
//!
//! [`machine`] holds the steps of one sync as a pure, total function; the executors drive it
//! with the store and the Door43 client, so what a step may do after a failure is decided in
//! one place.

pub mod connect;
pub mod exchange;
pub mod join;
pub mod machine;
pub mod share;
pub mod sync;

pub use connect::{ConnectError, connect};
pub use join::{JoinError, join};
pub use share::{ShareError, share};
pub use sync::{Backoff, OtherToolCommit, Pause, SyncReport, sync};
