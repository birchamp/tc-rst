//! The checking logic behind tC4's Check screens: Translation Words and Translation
//! Notes, for Bible books and for Open Bible Stories.
//!
//! - [`derive`](mod@derive) builds a check list from translation helps: tN notes and tW
//!   word links from `tc-helps`, mapped into the project's versification and filtered to
//!   its scope, with tC4's check identity ([`CheckKey`], the journal's `CheckKey`).
//! - [`selection`] records the words the checker taps, revalidates them against the
//!   verse text, and shows them.
//! - [`status`] holds a stored [`Decision`] and computes whether it still holds, as the
//!   journal's fold does (STORAGE.md §7.7): valid, invalid because the verse text
//!   changed, invalid because the resource changed, or to do.
//! - [`rail`] counts progress and the rail's filter chips, and orders the rail.
//! - [`resolve`] finds the pinned resource a (tool, book) checks against.
//! - [`carry`] plans carrying decisions over to a new resource (an upgrade, a gateway
//!   change, a re-pin) and counts what the person confirms; [`change`] counts what a
//!   change of pins would put at risk before anything is derived.
//! - [`session`] opens a check list with the decision shown on each check.
//! - [`fix`] decides whether a tool can open on this computer and what the guided fix
//!   offers when its resource is missing.
//!
//! The crate does no I/O. Its inputs are plain values; each module says how they map
//! from the journal's records.

pub mod carry;
pub mod change;
pub mod derive;
mod error;
pub mod fix;
pub mod item;
mod js;
pub mod key;
pub mod rail;
pub mod resolve;
pub mod scope;
pub mod selection;
pub mod session;
pub mod status;

pub use error::Error;
pub use item::{Category, CheckItem, NoteCategory, QuoteWord};
pub use key::{CheckId, CheckKey, CheckTarget, Collection, RefPart, ToolId};
pub use resolve::{Coverage, Pin, Pins, RepoPath, Resolution, ResourceRef, Rung, Slot};
pub use selection::Selection;
pub use status::{
    Assessment, CheckState, Decision, Dot, Invalidation, Judgement, TextMd5, UserStatus,
};
