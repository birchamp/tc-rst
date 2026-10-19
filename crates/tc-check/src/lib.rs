//! The checking logic behind tC4's Check screens: Translation Words and Translation
//! Notes, for Bible books and for Open Bible Stories.
//!
//! - [`derive`](mod@derive) builds a check list from translation helps: tN notes and tW
//!   word links from `tc-helps`, mapped into the project's versification and filtered to
//!   its scope, with tC4's check identity ([`CheckKey`], the journal's `CheckKey`).
//! - [`selection`] records the words the checker taps and shows them; the journal
//!   revalidates them against the verse text.
//! - [`status`] shows a stored [`DecisionRecord`] with the state the journal computes for
//!   it (STORAGE.md §7.7): valid, invalid because the verse text changed, invalid because
//!   the resource changed, or to do; and makes the journal's edit from a Check screen
//!   button.
//! - [`rail`] counts progress and the rail's filter chips, and orders the rail.
//! - The pinned resource a (tool, book) checks against is `tc-journal`'s D30 ladder
//!   ([`Ladder`]), the one the fold resolves with.
//! - [`carry`] plans carrying decisions over to a new resource (an upgrade, a gateway
//!   change, a re-pin) and counts what the person confirms; [`change`] counts what a
//!   change of pins would put at risk before anything is derived.
//! - [`session`] opens a check list with the fold's decisions shown on each check.
//! - [`fix`] decides whether a tool can open on this computer and what the guided fix
//!   offers when its resource is missing.
//!
//! The crate does no I/O. Its types and rules are `tc-journal`'s: the check identity, the
//! decision record and its edit, the computed state of a decision, the pins and the D30
//! ladder. It holds no second copy of any of them (STORAGE.md §17).

pub mod carry;
pub mod change;
pub mod derive;
mod error;
pub mod fix;
pub mod item;
mod js;
pub mod key;
pub mod rail;
pub mod scope;
pub mod selection;
pub mod session;
pub mod status;
mod work;

pub use error::Error;
pub use item::{Category, CheckItem, NoteCategory, QuoteWord};
pub use key::{CheckId, CheckKey, CheckTarget, Collection, RefPart, ToolId};
pub use status::{
    Assessment, CheckState, DecisionRecord, Dot, Invalidation, Judgement, Selection, TargetText,
    UserStatus,
};
pub use tc_journal::pins::{
    Answer, Coverage, Ladder, LanguageSets, Pin, Rung, SetSlot, SlotPin, Unrecorded, UnusablePin,
};
pub use tc_journal::record::ResourceRef;
pub use work::Work;
