---
id: FIX-KEPT-EVENT
title: Lost work kept just after a project opens reaches the pages without a reload
track: A
lane: app
milestone: M5
state: done
owner: kept-event session (Claude Code 2e51a251)
branch: fix-kept-event
worktree: /home/bot/tc-rst/.worktrees/fix-kept-event
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.25 d (basis: SAVE-WAIT, a change of the same size)
merged: ab1e2ca9
updated: 2026-10-26
next: none
blocked_by: none
---

# FIX-KEPT-EVENT: Lost work kept just after a project opens reaches the pages without a reload

## Goal

The session's event pump compares each look at the files with the lists of outside edits read
when the session opened, before any reply carried them. A copy kept after the open reply always
reaches the pages as an `OutsideEdits` event.

## Why it matters to a translator

When the app keeps a translator's lost work as a copy, the pages say so. Before this fix, a copy
kept in the moment between the open and the pump's start was never told: the translator saw it
only after a reload. That hides kept work, against the no-loss rule.

## Before you start

- CI run 40438997652 (gate/batch-27b): `app::sessions::lost_work_kept_at_a_look_reaches_the_pages_without_a_reload`
  failed with "hung: the event comes: Elapsed". On bd836694 the test failed 9 times in 8,808 runs
  (8 at once for 10 minutes under tc-heavy).

## Funnel card

- **Invariants**:
  - INV-KEPT-TOLD: every change of the outside-edit lists after the open reply reaches the pages
    as an `OutsideEdits` event.
- **Wrong states**:
  - `pump` (crates/tc-app/src/session.rs) read its first lists when the spawned task first ran.
    A keep between the open reply and that read matched the lists, so the write after it told
    nothing.
- **Code**:
  - `SessionRegistry::opened` reads the lists right after `subscribe`, before the slot opens,
    and hands them to `pump`. A keep between that read and the reply is told twice; the second
    telling changes nothing on the pages.
  - The pump's first snapshot (`previous`, which tells when conflict counts changed) is read
    there too, for the same reason. No test: new conflicts are told through
    `delta.new_conflicts` whatever the snapshot; only a resolution in the moment before the
    pump's first poll would be missed, and no translator can make one then.
  - Left to checks: none.
- **Checks**:
  - `lost_work_kept_at_a_look_reaches_the_pages_without_a_reload` in
    `crates/tc-app/tests/app/sessions.rs` now runs on a current-thread runtime with one task per
    tick, so the pump starts only after the test has kept the copy.
  - Proof it can fail: with the fix reverted it fails 3 of 3 runs with "hung: the event comes".
- **Human**: none.

## Steps

### Step 1: Read the lists before the pump starts

- Done when: the test passes, and fails with the fix reverted.

## Log

- 2026-10-26 | kept-event session (Claude Code 2e51a251) | created | none | from the orchestrator's message: the CI timeout of run 40438997652; the app, not the test, drops the event; REL-1 waits for it | crates/tc-app/src/session.rs
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | step 1 | none | the pump's first lists are read in `opened`; the test fails 3/3 without the fix and passes with it; SAVE-WAIT's two waits play no part (another test in the same binary; the failure was this test's own 60 s limit) | crates/tc-app/tests/app/sessions.rs
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | verify | 801e7a0e | loss lens (subagent, read-only): no blocker, no major. 1 Code: the baseline is read in `opened` after `subscribe`, before every reply that carries the lists (open_screen, open_sessions via hello), so a keep lands in the baseline or in a queued event. 2 Checks: the test fails 3/3 without the fix. 3 Reach: a watcher or focus look keeping lost work as the pump starts. 4 Siblings: `previous` moved too; `checks().begin` keeps its meaning (a commit before the read is in it, one after is noted). 5 No loss: this lens. 6 Prose: one comment, a reason. One older, unreachable point (Opened published before the slot opens) goes to minors | .audit/minors.tsv
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | verify | 801e7a0e | races-and-protocol lens (subagent, read-only): approve. The repeat of a keep between the baseline and the reply is harmless (the page replaces its lists, tc-ui state.rs:135); a close racing the open no longer has the pump read a closed handle; the test is deterministic on a current-thread runtime with one task per tick; no other spawned task keeps a late baseline. One older point (publish_outside unlocked against the pump) goes to minors | .audit/minors.tsv
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | verify | 801e7a0e | browser-and-parity lens (subagent, read-only): no blocker. The pages keep state (memos over the lists), so a repeated OutsideEdits shows nothing twice; APP-DESIGN difference (g) already says kept work reaches the pages without a reload; no journey covers this timing, the tc-app test does | crates/tc-ui/src/app/web/state.rs
- 2026-10-26 | kept-event session (Claude Code 2e51a251) | note | 801e7a0e | scripts/gates.sh --touched passed (1,042 tests); after the fix, 3,640 runs of the sessions file under tc-heavy (8 at once, 7 minutes) failed none | scripts/gates.sh --touched
- 2026-10-26 23:55 EDT | integration lane (Claude Code 3d07d5f8) | merge | ab1e2ca9 | merged into main in batch 29, gated by scripts/gates.sh --wait-ci batch-29 | scripts/gates.sh --wait-ci batch-29
