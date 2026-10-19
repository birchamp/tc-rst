//! The rulings docs/PLAN.md lists under "Where the rebuild differs from tC4 4.0 on purpose", read
//! one way by both parity gates: `screen_record` checks a mask's ruling against them and
//! `gallery_record` a residual's. A ruling is written in parentheses, alone or before a comma:
//! an owner's answer to the tC4 delta (`(Q3)`, `(Q1, AGENTS.md)`) or a tC4 issue the rebuild
//! decides otherwise under AGENTS.md's "Matching tC4" (`(#701)`).

use std::collections::BTreeSet;

use regex::Regex;

/// The rulings in `plan`, the text of docs/PLAN.md; none when it has no such section.
pub fn plan_rulings(plan: &str) -> BTreeSet<String> {
    let mut lines = plan.lines();
    lines
        .by_ref()
        .find(|l| l.starts_with("### Where the rebuild differs from tC4 4.0 on purpose"));
    let section: String = lines
        .take_while(|l| !l.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    // A constant pattern.
    let ruling = Regex::new(r"\((Q\d+|#\d+)[,)]").unwrap();
    ruling
        .captures_iter(&section)
        .map(|c| c[1].to_owned())
        .collect()
}
