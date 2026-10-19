//! What a change of pins would cost, counted before the person confirms it (tC4
//! `consequencesOfGatewayChange` and `uncoveredByChange`, D23a and D30.2). This is the one
//! counter of it: a gateway change, a release upgrade and a re-pin all count here.
//!
//! A decision is at risk when it holds against the resource its (tool, collection)
//! resolves to now and the change moves that resolution elsewhere. A book the new
//! primary does not cover keeps resolving to the fallback it was checked against, so it
//! is not affected. A decision that already does not hold against the current
//! resolution (one a carry-over could not place, which keeps its old resource) is not at
//! risk again: tC4 rewrote such a decision's resource in the file, so it never counted
//! it twice. Here it is counted apart, as revived when the change resolves its unit back
//! to the resource it was made against and as detached otherwise. A change that would
//! leave an affected unit with no usable resource is blocked, because its decisions
//! would then match no rung.
//!
//! Both sides resolve with the journal's ladder ([`Ladder::answer`]) and compare resources
//! as the fold does ([`ResourceRef::same_resource`]).

use tc_journal::pins::{Coverage, Ladder, Unrecorded};
use tc_journal::record::ResourceRef;

use crate::Work;
use crate::carry::{StoredDecision, Summary};
use crate::key::{Collection, ToolId};

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
    /// True when no rung answers it with a usable pin after the change: the change is
    /// refused while any affected unit is blocked.
    pub blocked: bool,
}

/// The cost of a change of pins.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Consequences {
    /// Units with decisions, none of which the change puts at risk.
    pub unaffected: usize,
    pub affected: Vec<Affected>,
    /// Decisions that do not hold now and will hold after the change, because it
    /// resolves their unit back to the resource they were made against.
    pub revived: usize,
    /// Decisions that hold neither now nor after the change: made against a resource
    /// their unit resolves to on neither side. They compute as invalidated (resource
    /// changed) either way and are kept.
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
    current: &dyn Ladder,
    next: &dyn Ladder,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
) -> Consequences {
    consequences_counted(current, next, units, local, &mut Work::default())
}

/// [`consequences`], adding what it touched to `work`: two answers of the ladders per unit
/// with decisions, and each stored decision looked at once (`ladder_answers`,
/// `decisions_visited`).
pub fn consequences_counted(
    current: &dyn Ladder,
    next: &dyn Ladder,
    units: &[Unit<'_>],
    local: impl Fn(&ResourceRef) -> Option<Coverage>,
    work: &mut Work,
) -> Consequences {
    let mut out = Consequences::default();
    let mut answered = 0usize;
    let mut resolve = |pins: &dyn Ladder, unit: &Unit<'_>| {
        answered += 1;
        pins.answer(unit.tool, unit.collection, Unrecorded::Installed(&local))
            .and_then(|a| a.resource().cloned())
    };
    for unit in units {
        if unit.decisions.is_empty() {
            continue;
        }
        let before = resolve(current, unit);
        let after = resolve(next, unit);
        let holds = |d: &StoredDecision, on: Option<&ResourceRef>| {
            on.is_some_and(|r| r.same_resource(&d.decision.resource))
        };
        // One look at each decision: it holds now, or it holds after the change, or it
        // holds on neither side.
        let (mut holding, mut revived) = (0usize, 0usize);
        for d in unit.decisions {
            work.decisions_visited += 1;
            if holds(d, before.as_ref()) {
                holding += 1;
            } else if holds(d, after.as_ref()) {
                revived += 1;
            }
        }
        out.revived += revived;
        out.detached += unit.decisions.len() - holding - revived;
        let moves_away = match (&before, &after) {
            (Some(b), Some(a)) => !b.same_resource(a),
            (b, a) => b.is_some() != a.is_some(),
        };
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
    work.ladder_answers += answered;
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
