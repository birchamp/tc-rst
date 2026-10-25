---
id: REF1c
title: Notes by figure, with groups that fold (tC4 #769)
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [REF1]
needs_owner: []
proves: [P1, P7]
no_loss: false
size: 2-3 sessions (basis: tC4's state.jsx and Check.jsx changed by about 800 lines)
merged: none
updated: 2026-10-24
next: Claim this card after REF1 merges, then do step 1.
blocked_by: none
---

# REF1c: Notes by figure, with groups that fold (tC4 #769)

## Goal

In Check, Translation Notes sort "By figure": one group per Translation Academy article, headed
by its title, in the order of each group's first note; notes without an article go under
"Other notes". Every group heading of a grouped sort (Notes by figure, Words by term, Open Bible
Stories) folds, shows resolved/total, and "Fold all" and "Unfold all" sit in the list header.
The fold state is kept per computer.

## Why it matters to a translator

A checker works through one kind of note at a time (all metaphors, then all idioms). Folding
the groups they finished keeps the long list short. tC4 rc.2 has it, and the owner chose to
match rc.2.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| REF1 '` shows REF1 `done` (it re-vendors the
  catalogs that hold `check.sort.byFigure`, `check.foldAll`, `check.unfoldAll`,
  `check.group.other`).
- Until this card lands, "By category" keeps its words through
  `crates/tc-ui/assets/i18n-rebuild/*.json` `check.sort.byCategory` (REF1 step 2). This card
  removes those ten keys.
- Read `docs/TC4-DELTA-2026-10-19.md` §3.12.1 item 2 and C43, and tC4's `src/views/Check.jsx`
  and `src/state.jsx` `setRailFolded` at d25b7871.

## Funnel card

- **Invariants**:
  - INV-THE-ACTIVE-GROUP-IS-OPEN: the group of the check on screen is never folded; Next or
    Previous into a folded group unfolds it; Fold all keeps it open.
  - INV-GROUP-COUNTS-IGNORE-THE-FILTER: a heading's resolved/total counts every check of the
    group, whatever the filter shows.
- **Wrong states**:
  - The open check hidden in a folded group; counts that change with the filter.
- **Code**:
  - Types: tc-check `SortMode::ByCategory` becomes `ByFigure` with the tA article id; tc-helps
    reads a tA title; the fold state is a set of `(tool, group id)` in the app's prefs
    (`railFolded`), written through the prefs writer.
  - One entry point: tc-check's `groups()` gives groups with counts; the rail renders them.
  - Removed: `SortMode::ByCategory` and the ten `check.sort.byCategory` rebuild keys.
- **Checks**:
  - `rail_groups_by_figure_in_first_note_order`, `group_counts_ignore_the_filter`,
    `fold_survives_a_restart`, `next_into_a_folded_group_unfolds_it`,
    `fold_all_keeps_the_active_group_open` (the #769 row's tests), red first.
  - Journeys in `parity/app/journeys/check-picker.test.mjs` or `check-keyboard.test.mjs`:
    headings with `aria-expanded`, the list one Tab stop, Left (Right in right-to-left) to the
    heading, rows skip folded rows.
- **Prose**: `docs/APP-DESIGN.md` §3.8 and §4.9.
- **Human**: none.

## Steps

### Step 1: By figure in tc-check and tc-helps

- Do: `ByFigure` grouping in first-note order with counts; tA titles; the first test above.
- Record: a log line `step 1`.

### Step 2: Folding, saved per computer

- Do: the fold state in prefs; the rail's fold headings, Fold all and Unfold all, keys; the
  other four tests and the journeys.
- Record: a log line `step 2`.

### Step 3: Recapture the Check screens

- Do: recapture the Check baseline screens (REF1 step 3's harness); remove the ten
  `check.sort.byCategory` rebuild keys.
- Record: a log line `step 3`; `state: verifying`.

## Gates before merge

`scripts/gates.sh` (journeys included), plus the review gate of `FUNNEL.md` §8: a `verify` log
line, and a `fix` line for every blocker and every major that harms a translator's view.

## Stop and ask when

- A note has an article tC4 groups differently than its title suggests: follow tC4.

## References

- tC4 #769 (572521c, d5a2b4b, PR #779)
- `docs/TC4-DELTA-2026-10-19.md` C43, §3.12.1

## Log

- 2026-10-24 15:15 EDT | REF1 session (Claude Code 56b3f56d) | created | none | split from REF1 step 4.1 on the orchestrator's word: the next free lane builds it; REF1 keeps "By category" until it lands | .audit/roadmap/units/REF1.md
