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
pub mod username;

pub use connect::{ConnectError, connect};
pub use join::{JoinError, join};
pub use share::{ShareError, share};
pub use sync::{Backoff, OtherToolCommit, Pause, SyncReport, sync};
pub use username::{UsernameError, record_username};

/// A change that is bookkeeping, not a translator's work: a device's profile (its Door43
/// username, Q7) and the resource pins an open's upgrade writes (owner ruling 2026-10-26). It
/// joins the project and the Received sheet's rows, but the count of received work leaves it
/// out.
pub fn is_bookkeeping(record: &tc_journal::Record) -> bool {
    matches!(
        record,
        tc_journal::Record::DeviceProfile { .. } | tc_journal::Record::Pins { .. }
    )
}
