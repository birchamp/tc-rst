---
id: FIX-KEPT-EVENT
title: Lost work kept just after a project opens reaches the pages without a reload
track: A
lane: app
milestone: M5
state: verifying
owner: kept-event session (Claude Code 2e51a251)
branch: fix-kept-event
worktree: /home/bot/tc-rst/.worktrees/fix-kept-event
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.25 d (basis: SAVE-WAIT, a change of the same size)
merged: none
updated: 2026-10-26
next: The integration lane takes it into the next batch.
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
