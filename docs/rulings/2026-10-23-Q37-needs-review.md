# Q37: does an edit in Align clear "needs review"?

- Asked: 2026-10-23, by the integration lane (SWEEP), through the orchestrator session;
  `docs/TC4-DELTA-2026-10-23.md` §5.
- Options: (a) follow tC4 (#814, D105 point 5): only Mark valid clears it (recommended);
  (b) keep the rebuild's rule: the first edit clears it.
- Answer: "yes, needs-review, raw links are fine, yes orig lang highlights. MIT license new as of https://github.com/unfoldingWord/translationCore4/pull/838", 2026-10-23. Read as (a): "yes, needs-review".

## The rule

A verse marked "needs review" in Align stays marked until the translator clicks Mark valid.
An edit to its alignment does not clear the mark.

## Effect for translators

A verse flagged after a source upgrade or a teammate's change stays red until the translator
says it is right, so a verse cannot look finished when most of it was never checked.

## Unblocks and changes

The rebuild changes: `crates/tc-align/tests/tc4_edit.rs` `an_edit_clears_the_re_review_flag_tc3_or_tc4_set`
is inverted and `is_stale` in `aligner.rs` no longer resets on an edit. Owner card: FIX-R (claimed
by the app lane; `docs/TC4-ISSUES.md` §1 row 23 names the test). `docs/PLAN.md` no longer lists it
as a difference.
