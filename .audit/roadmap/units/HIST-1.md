---
id: HIST-1
title: History and Restore on the screens, and the Received sheet's replaced rows
track: A
lane: app
milestone: M3
state: claimed
owner: shell lane (Claude Code e1bb3a0c)
branch: track-HIST-1
worktree: .worktrees/track-HIST-1
depends: [C7b, UI-3b]
needs_owner: []
proves: [P2, P3]
no_loss: true
size: 2-3 sessions (basis: C7a's History view exists; this adds entry points and one list of rows)
merged: none
updated: 2026-10-25
next: Step 1, History and Restore on the Check screen, with the journey half.
blocked_by: none
---

# HIST-1: History and Restore on the screens, and the Received sheet's replaced rows

## Goal

Owner ruling H5 (`rulings/2026-10-24-H5.md`): outside verse text the newest version wins with no
question, and the older one stays in History, where *Restore this version* brings it back. C7b
built the rule, the record-keyed calls (`conflicts/history`, `conflicts/restore`) and the sync's
`replaced` rows. This card gives the translator the way back on the screens: History with
*Restore this version* on the Check screen (a check's decision and comment), in the aligner (a
verse's alignment) and for the project's settings and resource pins, and it draws the Received
sheet's rows ("Juan's alignment of Titus 1:1 replaced yours. Your version is in History.").

## Why it matters to a translator

A teammate's newer check or alignment arrives without a question. The translator sees what it
replaced and can put their own version back in two clicks.

## Before you start

- C7b is `done`: `grep -n '"conflicts/restore"' crates/tc-api/src/lib.rs` prints one line, and
  `grep -n 'pub struct ReplacedView' crates/tc-api/src/jobs.rs` prints one.
- UI-3b is `done` (it changes the check and align views first).
- Read C7a's History view in `crates/tc-ui/src/views/review.rs` (it is reused) and
  `crates/tc-app/tests/app/conflicts.rs`
  `a_record_outside_a_conflict_restores_an_older_version_from_history` (the calls in use).

## Funnel card

- **Invariants**:
  - INV-THE-LOSER-STAYS-IN-HISTORY (C7b): reached from a screen for every newest-wins kind.
  - INV-NO-SILENT-REPLACEMENT: every version a sync replaced with no question is named on the
    Received sheet (`SyncResultView.replaced`).
- **Code**: one History entry point component, given a record's token and `RecordRef`; the
  Check screen, the aligner and the settings and pins screen each place it. The Received sheet
  draws `replaced` rows; catalog keys for the words.
- **Checks**: `history_restores_a_checks_older_decision` (journey, extends C7b's
  `parity/app/journeys/conflict-kinds.test.mjs` with the restore half), and the same for an
  alignment and a setting; `the_received_sheet_names_each_replaced_version` (tc-ui).
- **Prose**: `docs/PLAN.md` declared differences already list History and the Received rows (C7b).
- **Human**: none; the words follow STORAGE §8.5's and H5's example.

## Steps

1. The History entry point on the Check screen, with its journey half. Done when C7b's
   conflict-kinds journey restores Teammate 1's decision and the decision file holds it.
2. The same in the aligner and for settings and pins.
3. The Received sheet's `replaced` rows, words in the catalogs; tests.

## Log

- 2026-10-25 | shell lane (Claude Code e1bb3a0c) | created | none | From C7b's parity lens blocker B1 and major M1; the orchestrator chose option (b): C7b gives the calls, this card the screens; it goes in REL-1's and DONE's depends; the G5 session takes it after UI-3b | .audit/roadmap/evidence/2026-10-24/C7b-lens-parity.md
- 2026-10-25 12:55 EDT | shell lane (Claude Code e1bb3a0c) | claimed | none | The orchestrator asked this lane to take HIST-1 now, cut from track-C7b, before C7b and UI-3b merge; UI-3b (track-UI-3b) also edits web/align.rs, web/check.rs, web/app_ctx.rs and views/check/detail.rs, so whichever lands second merges and adapts
