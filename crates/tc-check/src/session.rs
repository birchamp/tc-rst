//! A check session as the Check screen opens it: the check list, the decisions placed on
//! each check and the one shown, and each check's computed state (tC4
//! `completedCheckSession`).
//!
//! Decisions are placed on the list with the carry-over's two passes ([`crate::carry`]),
//! as tC4 places them when a session opens. Placing writes nothing: a decision shown on
//! a check under another key is still stored under its own key until the person
//! decides the check again or the caller records the move. Every stored decision is
//! either placed on one check or listed off the list; tC4 kept only one of two
//! decisions that land on one check, and here both stay reachable.

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
    /// For each check, every stored decision placed on it, in the order the decisions
    /// were given: indices into the decisions the session was opened with. A check
    /// holding two or more is contested ([`Session::contested`]).
    pub placed: Vec<Vec<usize>>,
    /// For each check, the computed state of the decision shown on it.
    pub states: Vec<CheckState>,
    /// Decisions no check on the list carries. The screen lists them as checks from a
    /// previous resource, kept until the person dismisses them.
    pub off_list: Vec<usize>,
}

impl Session {
    /// Decided checks over the checks on the list.
    pub fn progress(&self) -> Progress {
        progress(&self.states)
    }

    /// The decision shown on check `item`: the later of the decisions placed on it, as in
    /// tC4.
    pub fn shown(&self, item: usize) -> Option<usize> {
        self.placed.get(item)?.last().copied()
    }

    /// The checks more than one decision is placed on. The screen shows the later
    /// decision and offers the others from [`Session::placed`], so the person chooses.
    pub fn contested(&self) -> impl Iterator<Item = usize> + '_ {
        self.placed
            .iter()
            .enumerate()
            .filter(|(_, placed)| placed.len() > 1)
            .map(|(item, _)| item)
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
    let mut placed: Vec<Vec<usize>> = vec![Vec::new(); items.len()];
    let mut off_list = Vec::new();
    for (s, outcome) in outcomes.iter().enumerate() {
        match outcome {
            Outcome::Stays { item, .. } | Outcome::Moves { item, .. } => placed[*item].push(s),
            Outcome::Unplaced => off_list.push(s),
        }
    }
    let states = items
        .iter()
        .zip(&placed)
        .map(|(item, on)| {
            CheckState::of(
                on.last().map(|&s| &stored[s].decision),
                resolution,
                verse_text(&item.key.target),
            )
        })
        .collect();
    Session {
        items,
        placed,
        states,
        off_list,
    }
}
