//! A check session as the Check screen opens it: the check list, the decisions placed on
//! each check and the one shown, and each check's computed state (tC4
//! `completedCheckSession`).
//!
//! The decisions are the fold's projection of the (tool, collection)
//! ([`tc_journal::fold::FoldState::decisions`]): each stored decision with the state the
//! fold computed for it, and whether the fold lists it under Detached (§7.7). They are
//! placed on the list with the carry-over's two passes ([`crate::carry`]), as tC4 places
//! them when a session opens. A decision placed on the check it is stored under shows the
//! fold's state, so the rail and the decision files cannot disagree; one shown on a check
//! under another key is assessed against that check's text with the same function
//! ([`tc_journal::decision::assess`]). Placing writes nothing: a decision shown under
//! another key is still stored under its own key until the person decides the check again
//! or the caller records the move. Every stored decision is either placed on one check or
//! listed off the list; tC4 kept only one of two decisions that land on one check, and
//! here both stay reachable.

use tc_journal::fold::ProjectedDecision;
use tc_journal::record::ResourceRef;

use crate::carry::{Outcome, StoredDecision, placements};
use crate::item::CheckItem;
use crate::key::CheckTarget;
use crate::rail::{Progress, progress};
use crate::status::{CheckState, TargetText};

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
    /// Decisions the fold lists under Detached (§7.7): made against another resource than
    /// the (tool, collection) resolves to, or on a target the text no longer has. One can
    /// also be placed on a check, where it shows as invalidated.
    pub detached: Vec<usize>,
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

/// Opens a session on `items` with the fold's `decisions` of the (tool, collection).
/// `resolution` is the resource the (tool, collection) resolves to now and `text` the text
/// of a check's target by the no-text rule of §7.7: they assess a decision shown on a check
/// under another key than its own.
pub fn open<'t>(
    items: Vec<CheckItem>,
    decisions: &[ProjectedDecision],
    resolution: Option<&ResourceRef>,
    text: impl Fn(&CheckTarget) -> Option<&'t TargetText>,
) -> Session {
    let stored: Vec<StoredDecision> = decisions
        .iter()
        .map(|d| StoredDecision::of(d.key.clone(), d.record.clone()))
        .collect();
    let (outcomes, _) = placements(&items, &stored);
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
        .map(|(item, on)| match on.last() {
            None => CheckState::undecided(),
            Some(&s) if decisions[s].key == item.key => {
                CheckState::assessed(&decisions[s].record, decisions[s].assessment)
            }
            Some(&s) => CheckState::of(
                Some(&decisions[s].record),
                resolution,
                text(&item.key.target),
            ),
        })
        .collect();
    let detached = decisions
        .iter()
        .enumerate()
        .filter(|(_, d)| d.detached)
        .map(|(s, _)| s)
        .collect();
    Session {
        items,
        placed,
        states,
        off_list,
        detached,
    }
}
