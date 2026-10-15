//! A check session as the Check screen opens it: the check list, the decision shown on
//! each check, and each check's computed state (tC4 `completedCheckSession`).
//!
//! Decisions are placed on the list with the carry-over's two passes ([`crate::carry`]),
//! as tC4 places them when a session opens. Placing writes nothing: a decision shown on
//! a check under another key is still stored under its own key until the person
//! decides the check again or the caller records the move.

use crate::carry::{Outcome, StoredDecision, placements};
use crate::item::CheckItem;
use crate::key::CheckTarget;
use crate::rail::{Progress, progress};
use crate::resolve::ResourceRef;
use crate::status::CheckState;

/// An open check session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub items: Vec<CheckItem>,
    /// For each check, the stored decision shown on it: an index into the decisions the
    /// session was opened with.
    pub shown: Vec<Option<usize>>,
    /// For each check, its computed state.
    pub states: Vec<CheckState>,
    /// Checks that more than one decision re-attaches to. The later decision is shown,
    /// as in tC4, and the screen offers the others.
    pub contested: Vec<usize>,
    /// Decisions no check on the list carries. The screen lists them as checks from a
    /// previous resource, kept until the person dismisses them.
    pub off_list: Vec<usize>,
}

impl Session {
    /// Decided checks over the checks on the list.
    pub fn progress(&self) -> Progress {
        progress(&self.states)
    }
}

/// Opens a session on `items`. `resolution` is the resource the (tool, collection)
/// resolves to now; `verse_text` gives the plain text of a check's verse or frame.
pub fn open<'t>(
    items: Vec<CheckItem>,
    stored: &[StoredDecision],
    resolution: Option<&ResourceRef>,
    verse_text: impl Fn(&CheckTarget) -> Option<&'t str>,
) -> Session {
    let (outcomes, _) = placements(&items, stored);
    let mut shown: Vec<Option<usize>> = vec![None; items.len()];
    let mut contested = Vec::new();
    let mut off_list = Vec::new();
    for (s, outcome) in outcomes.iter().enumerate() {
        match outcome {
            Outcome::Stays { item, .. } | Outcome::Moves { item, .. } => {
                if shown[*item].replace(s).is_some() && !contested.contains(item) {
                    contested.push(*item);
                }
            }
            Outcome::Unplaced => off_list.push(s),
        }
    }
    let states = items
        .iter()
        .zip(&shown)
        .map(|(item, s)| {
            CheckState::of(
                s.map(|s| &stored[s].decision),
                resolution,
                verse_text(&item.key.target),
            )
        })
        .collect();
    Session {
        items,
        shown,
        states,
        contested,
        off_list,
    }
}
