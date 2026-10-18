//! The project folder of the tC4 rebuild (`docs/STORAGE.md` §3, §5, §6).
//!
//! - Every file operation goes through [`Vfs`]: [`StdFs`] in production, [`fault::FaultFs`]
//!   in tests, a model filesystem that can stop the writer at any write point and drop
//!   what a power cut would drop (§16.2). Project files are written only by
//!   [`atomic_write`] and its two siblings (§5.0).
//! - A [`Store`] opens and creates projects. An open project is a [`ProjectHandle`]: one
//!   actor thread writes the folder (§5.0), reads take an O(1) [`Snapshot`] and never wait
//!   behind it, and a command's reply comes only after its batch is durable.
//! - The open (§6.1) reads each journal file once, takes unchanged files from the fold
//!   cache, classifies derived files by stat and scope digest, and reports progress from
//!   300 ms on; seal and consolidation (§5.11, §5.12) and materialization (§5.10) leave the
//!   folder a valid Scripture Burrito between write groups, and a crash at any point leaves
//!   the action's before or after state (§6.4).
//!
//! Git, sync and outside-edit recording are unit S2's.

pub mod actor;
pub mod appdata;
pub mod atomic;
mod carried;
mod checkpoint;
mod classify;
pub mod clock;
mod commands;
pub mod counters;
pub mod device;
pub mod drafts;
pub mod error;
pub mod fault;
mod focus;
pub mod git;
pub mod handle;
mod history;
pub mod hooks;
pub mod layout;
pub mod load;
pub mod manifest;
pub mod materialize;
pub mod notices;
pub mod open;
mod outside;
mod pace;
mod packs;
pub mod platform;
pub mod progress;
mod project;
pub mod random;
mod repack;
pub mod reply;
mod scan;
pub mod seal;
pub mod seed;
pub mod source;
pub mod store;
mod temps;
pub mod training;
pub mod vfs;
mod watch;
mod writer;

pub use atomic::{DirSet, SyncDir, WriteError, atomic_create_dirs, atomic_remove, atomic_write};
pub use clock::{Clock, ManualClock, SystemClock};
pub use counters::StoreCounters;
pub use drafts::OfferedDraft;
pub use error::StoreError;
pub use handle::{
    BookRead, CheckpointReason, CheckpointReport, CheckpointSkip, Leave, Mode, Pending,
    ProjectFacts, ProjectHandle, Snapshot, StoreEvent, Work,
};
pub use hooks::{Failure, Hooks, Point};
pub use notices::StoreNotice;
pub use open::{LockHolder, OpenError, OpenOptions};
pub use platform::{FakePlatform, FolderFacts, Platform, StdPlatform, UnsafeFolder};
pub use progress::{Phase, Progress, ProgressSink};
pub use reply::{CommandError, FocusReport, ReadOnlyReason, Refusal, StoreReply, Stored, TakenIn};
pub use store::{CreateError, NewProject, RemovedStaging, StagingKind, Store, StoreConfig};
pub use training::{EditorText, TrainingVerse};
pub use vfs::{StdFs, Vfs};
