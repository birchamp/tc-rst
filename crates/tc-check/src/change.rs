//! What a change of pins would cost, counted before the person confirms it (tC4
//! `consequencesOfGatewayChange` and `uncoveredByChange`, D23a and D30.2).
//!
//! Only the books whose decisions were made against a resource the change moves away
//! from are affected: a book the new primary does not cover keeps resolving to the
//! fallback it was checked against. A change that would leave an affected book with
//! no resource at all is blocked, because its decisions would then match no rung.

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
    /// The resources its decisions were made against, each once, in first-seen order.
    pub checked_against: Vec<ResourceRef>,
    /// How many of its decisions were made against another resource than the one it
    /// resolves to after the change.
    pub decisions: usize,
    /// True when no rung covers it after the change: the change is refused while any
    /// affected unit is blocked.
    pub blocked: bool,
}

/// The cost of a change of pins.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consequences {
    /// Units with decisions that keep resolving to what they were checked against.
    pub unaffected: usize,
    pub affected: Vec<Affected>,
}

impl Consequences {
    /// Decisions made against a resource the change moves away from. Some carry over
    /// by meaning; the rest come back as checks to do.
    pub fn at_risk(&self) -> usize {
        self.affected.iter().map(|a| a.decisions).sum()
    }

    /// True when nothing anyone decided is disturbed.
    pub fn harmless(&self) -> bool {
        self.affected.is_empty()
    }

    /// True when the change must be refused.
    pub fn blocked(&self) -> bool {
        self.affected.iter().any(|a| a.blocked)
    }
}

/// Counts what changing the pins to `next` would cost. `local` gives the books an
/// installed copy of a pin holds, for pins that record none.
pub fn consequences(
    next: &Pins,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
) -> Consequences {
    let mut out = Consequences::default();
    for unit in units {
        if unit.decisions.is_empty() {
            continue;
        }
        let after = resolve_with(next, unit.tool, unit.collection, &local);
        let moved: Vec<&StoredDecision> = unit
            .decisions
            .iter()
            .filter(|d| after.as_ref().map(|r| &r.pin.resource) != Some(&d.decision.resource))
            .collect();
        if moved.is_empty() {
            out.unaffected += 1;
            continue;
        }
        let mut checked_against: Vec<ResourceRef> = Vec::new();
        for d in &moved {
            if !checked_against.contains(&d.decision.resource) {
                checked_against.push(d.decision.resource.clone());
            }
        }
        out.affected.push(Affected {
            tool: unit.tool,
            collection: unit.collection,
            checked_against,
            decisions: moved.len(),
            blocked: after.is_none(),
        });
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
