//! What a change of pins would cost, counted before the person confirms it (tC4
//! `consequencesOfGatewayChange` and `uncoveredByChange`, D23a and D30.2).
//!
//! A decision is at risk when it holds against the resource its (tool, collection)
//! resolves to now and the change moves that resolution elsewhere. A book the new
//! primary does not cover keeps resolving to the fallback it was checked against, so it
//! is not affected. A decision that already does not hold against the current
//! resolution (one a carry-over could not place, which keeps its old resource) is not at
//! risk again: tC4 rewrote such a decision's resource in the file, so it never counted
//! it twice, and here it is counted apart as detached. A change that would leave an
//! affected unit with no resource at all is blocked, because its decisions would then
//! match no rung.

use crate::carry::{StoredDecision, Summary};
use crate::key::{Collection, ToolId};
use crate::resolve::{Coverage, Pins, ResourceRef, resolve_with};

/// The stored decisions of one (tool, collection).
#[derive(Debug, Clone, Copy)]
pub struct Unit<'a> {
    pub tool: ToolId,
    pub collection: Collection,
    pub decisions: &'a [StoredDecision],
}

/// A (tool, collection) whose decisions the change moves away from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Affected {
    pub tool: ToolId,
    pub collection: Collection,
    /// The resource it resolves to before the change, which its decisions at risk were
    /// made against.
    pub checked_against: ResourceRef,
    /// How many of its decisions hold against `checked_against` and would no longer.
    pub decisions: usize,
    /// True when no rung covers it after the change: the change is refused while any
    /// affected unit is blocked.
    pub blocked: bool,
}

/// The cost of a change of pins.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consequences {
    /// Units with decisions, none of which the change puts at risk.
    pub unaffected: usize,
    pub affected: Vec<Affected>,
    /// Decisions that already do not hold against their unit's current resolution. They
    /// compute as invalidated before and after the change and are kept either way.
    pub detached: usize,
}

impl Consequences {
    /// Decisions that hold now against a resource the change moves away from. Some
    /// carry over by meaning; the rest come back as checks to do.
    pub fn at_risk(&self) -> usize {
        self.affected.iter().map(|a| a.decisions).sum()
    }

    /// True when nothing that holds now is disturbed.
    pub fn harmless(&self) -> bool {
        self.affected.is_empty()
    }

    /// True when the change must be refused.
    pub fn blocked(&self) -> bool {
        self.affected.iter().any(|a| a.blocked)
    }
}

/// Counts what changing the pins from `current` to `next` would cost. `local` gives the
/// books an installed copy of a pin holds, for pins that record none.
pub fn consequences(
    current: &Pins,
    next: &Pins,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
) -> Consequences {
    let mut out = Consequences::default();
    for unit in units {
        if unit.decisions.is_empty() {
            continue;
        }
        let before =
            resolve_with(current, unit.tool, unit.collection, &local).map(|r| r.pin.resource);
        let after = resolve_with(next, unit.tool, unit.collection, &local);
        let holding = unit
            .decisions
            .iter()
            .filter(|d| before.as_ref() == Some(&d.decision.resource))
            .count();
        out.detached += unit.decisions.len() - holding;
        let moves_away = after.as_ref().map(|r| &r.pin.resource) != before.as_ref();
        match before {
            Some(checked_against) if holding > 0 && moves_away => out.affected.push(Affected {
                tool: unit.tool,
                collection: unit.collection,
                checked_against,
                decisions: holding,
                blocked: after.is_none(),
            }),
            _ => out.unaffected += 1,
        }
    }
    out
}

impl Summary {
    /// The totals of several units' summaries: the whole upgrade's numbers.
    pub fn total(summaries: impl IntoIterator<Item = Summary>) -> Summary {
        summaries
            .into_iter()
            .fold(Summary::default(), |a, b| Summary {
                carried: a.carried + b.carried,
                decided_after: a.decided_after + b.decided_after,
                invalidated: a.invalidated + b.invalidated,
                still_unplaced: a.still_unplaced + b.still_unplaced,
                contested: a.contested + b.contested,
                undecided: a.undecided + b.undecided,
            })
    }
}
