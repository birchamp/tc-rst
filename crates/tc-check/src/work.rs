//! What a step of the checking logic touched: the counters its operation-count tests read
//! (`docs/PERFORMANCE.md` rule 2). A step costs what it touches, and a test that pins the
//! count fails the day a step starts to cost the size of the list instead (the rail's
//! `contains` per term, a carry-over's scan of every open check per decision).
//!
//! The `*_counted` functions of [`crate::rail`], [`crate::carry`] and [`crate::change`]
//! add to one of these; their plain forms drop it. The counters are integer additions,
//! made where the work is done, so they cannot drift from it.

/// Counters of the work one step did. A counter a step does not use stays zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Work {
    /// Rail: checks the filter looked at.
    pub rows_tested: usize,
    /// Rail: sort keys read from the checks shown (one per row, once).
    pub keys_read: usize,
    /// Rail: comparisons the sorts made, of rows and of group labels.
    pub comparisons: usize,
    /// Rail: rows put into their group.
    pub bucketed: usize,
    /// Carry-over: probes of the identity pass, one per check and one per decision.
    pub identity_probes: usize,
    /// Carry-over: decisions looked up by check id in the second pass.
    pub id_probes: usize,
    /// Carry-over: quotes compared word by word ([`crate::carry::same_quote`]).
    pub quote_comparisons: usize,
    /// Carry-over: decisions looked up by place, quote and occurrence.
    pub place_probes: usize,
    /// Carry-over: checks the place lookups returned, to be told apart by group id.
    pub place_candidates: usize,
    /// Carry-over: decisions looked up in the contested test, one per move.
    pub contest_probes: usize,
    /// Consequences: stored decisions looked at.
    pub decisions_visited: usize,
    /// Consequences: answers asked of the ladders.
    pub ladder_answers: usize,
}
