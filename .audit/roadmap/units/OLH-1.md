---
id: OLH-1
title: Highlight a note's words in the original-language pane
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: [P1]
no_loss: false
size: 2-3 sessions (basis: tC4 #840's scope; the gateway highlighter exists in tc-helps)
merged: none
updated: 2026-10-23
next: Claim this card, then do step 1.
blocked_by: none
---

# OLH-1: Highlight a note's words in the original-language pane

## Goal

With a translation note current, the Greek or Hebrew pane marks the note's quote words, in the
original tab of Draft and Understand and in the original row of the check card. The gateway
highlight is unchanged. The owner ruled to build this now, ahead of tC4 (#840).

## Why it matters to a translator

A note's quote is written in the original language, so a checker looks for it there first.
Today that pane stays plain, in tC4 4.0 and in the rebuild.

## Before you start

- `ls .audit/roadmap/rulings/ | grep Q39` lists `2026-10-23-Q39-original-language-highlight.md`.
- `gh issue view 840 -R unfoldingWord/translationcore4` (read only): the rule and the
  acceptance criteria.
- `grep -n 'pub fn' crates/tc-helps/src/highlight.rs`: today's highlighter reads the gateway's
  `\zaln` milestones (`AlignedVerse`, `SourceWord`).
- The original row of the check card: `crates/tc-ui/src/views/check/detail.rs`.

## Funnel card

- **Invariants**:
  - INV-ORIGINAL-HIGHLIGHT-MATCHES-THE-QUOTE: with a note current, the original-language pane
    marks exactly the words of the note's quote at the note's occurrence, matched against the
    verse's `\w` words; a quote that does not match in full marks nothing; with no current note
    nothing is marked.
- **Wrong states**:
  - The pane marks a different occurrence of the same word.
  - A partial match marks some of the quote's words.
  - The check card's original row stays plain while Draft marks the words.
- **Code**:
  - Types: reuse tc-helps' `SourceWord` (content plus occurrence) for the quote's words.
  - One entry point: one pure function in tc-helps from (verse words, quote, occurrence) to the
    marked word indexes, called by every original-language pane.
  - Removed: nothing.
  - Left to checks, and why: which words a quote names is a matching rule over text.
- **Checks**:
  - `the_original_pane_marks_the_quote_at_its_occurrence` in `crates/tc-helps/tests/` (to create),
    with a Greek (TIT, UGNT) and a Hebrew (UHB) verse from `fixtures/`; kind: unit; runs in: CI.
  - `a_quote_the_verse_lacks_marks_nothing_in_the_original_pane` (to create); kind: unit.
  - A journey step in the Check and Understand journeys that the original row marks the words
    (to create); kind: journey; runs in: G5's app-e2e job.
  - Proof it can fail: mark the first occurrence instead of the note's; the first test fails.
- **Prose**: a declared difference in `docs/PLAN.md` until tC4 ships #840.
- **Human**: none; ruled (`rulings/2026-10-23-Q39-original-language-highlight.md`). New pixels
  in the Check and Understand captures are reviewed with the P7 report (G2).

## Steps

### Step 1: The matcher

- Do: claim the card. Write the two unit tests first, then the pure function in tc-helps,
  following tC4's gateway rule (`matchQuote`: the quote's words in order, the note's
  occurrence, a partial match marks nothing), over `\w` words compared as tc-helps'
  `normalize_word` compares them.
- Files: `crates/tc-helps/src/highlight.rs`, `crates/tc-helps/tests/`.
- Done when: both tests pass, and fail with the occurrence ignored.
- Record: a log line `step 1`.

### Step 2: The panes

- Do: call the function from the original tab of Draft and Understand and from the check
  card's original row; mark the words as the gateway panes mark theirs. Add the journey steps;
  recapture the affected screens; add the declared difference to `docs/PLAN.md` and set the
  #840 row of `docs/TC4-ISSUES.md` to covered with the test names.
- Files: `crates/tc-ui/src/views/**`, `parity/app/journeys/*.test.mjs`, captures, `docs/PLAN.md`,
  `docs/TC4-ISSUES.md`.
- Done when: the journeys pass with zero skips; `screen_record` passes with the new captures.
- Record: a log line `step 2`.

### Step 3: Verify and merge

- Do: one verify pass (`FUNNEL.md` §8), then merge as `TRACKING.md` says, titled
  `Merge OLH-1: a note's words are marked in the Greek and Hebrew text`, trailer `Unit: OLH-1`.
- Record: log lines `verify` and `merge`.

## Gates before merge

`scripts/gates.sh --journeys` and `scripts/gates.sh --check-receipt` on the exact commit main
moves to.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The mark would change what a translator sees beyond the original-language panes.

## References

- tC4 #840; `rulings/2026-10-23-Q39-original-language-highlight.md`;
  `rulings/2026-10-23-replace-tc4.md`.
- `crates/tc-helps/src/highlight.rs`; tC4 `src/data/sourceHighlight.ts` (`matchQuote`).

## Log

- 2026-10-23 | integration lane (Claude Code 3d07d5f8) | created | none | from the owner's Q39 answer of 2026-10-23 | rulings/2026-10-23-Q39-original-language-highlight.md
