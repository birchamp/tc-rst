//! The tC4 journal (`docs/STORAGE.md`): the data model and the codecs. No I/O.
//!
//! - Identifiers ([`ids`]), record keys ([`keys`]) and records with their values
//!   ([`record`]) are the types of §2.1 to §2.3. Identity strings are parsed at the
//!   boundary and refused, never repaired, when they break their grammar.
//! - A [`Change`] is one immutable version of one record. [`Change::new`] writes its
//!   canonical JSON ([`json`], [`canon`]) and checks it exactly as a reader will; its id is
//!   the SHA-256 of those bytes.
//! - [`line`] frames a change as `<json> TAB <sha256> LF`. [`pending`] and [`pack`] read and
//!   write pending batch files and packs; every reader is total, salvages what verifies
//!   from a damaged file, and reports why it is damaged.
//! - [`rules`] holds the per-change rules of §4.5; a change that breaks one is a
//!   [`QuarantinedChange`].
//! - [`consolidate`] decides which packs to merge and which packs are redundant.
//! - [`index`] is the fold cache, `.tc/index.bin`: each file's prepared changes, so an open
//!   with the same file takes them instead of verifying and parsing its lines again.
//! - [`format`] is §12: the format version, `format.json`, the open gate and the paths each
//!   minor derives.
//! - [`fold`] holds the fold's output types, [`token`] the shown tokens and [`command`] the
//!   commands of §2.7.

pub mod canon;
pub mod change;
pub mod command;
pub mod consolidate;
pub mod fold;
pub mod format;
pub mod ids;
pub mod index;
pub mod json;
pub mod keys;
pub mod line;
pub mod pack;
pub mod pending;
pub mod record;
pub mod rules;
mod sha;
mod text;
pub mod token;

pub use change::{
    Cause, Change, ChangeBody, ChangeError, ExternalSource, ImportSource, QuarantinedChange,
    SystemReason, Verified, parse_change,
};
pub use format::{FORMAT_MAJOR, FORMAT_MINOR, FormatDoc, FormatGate, FormatJson, FormatNumber};
pub use ids::{
    BatchId, BatchRef, ChangeId, ChapterNo, CommentId, DeviceId, DeviceKind, DeviceSeq, FrameNo,
    GitOid, Md5, PackId, PackName, ProjectId, RepoPath, ShortId, StoryNo, VerseNo, WallTime,
};
pub use json::{Json, JsonMap};
pub use keys::{
    CheckId, CheckKey, CheckTarget, MetaField, MetaPath, PinGroup, RecordKey, RefPart, SettingKey,
    SlotName, ToolId, UnknownKey,
};
pub use record::Record;
pub use rules::{Rule, RuleViolation};
pub use tc_core::BookId;
pub use token::{ShownToken, TokenKey};
