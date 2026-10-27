---
id: SAVE-WAIT
title: The save-machine tests fail with a message instead of hanging
track: A
lane: checks
milestone: M5
state: claimed
owner: save-wait session (Claude Code 749c177d)
branch: track-SAVE-WAIT
worktree: .worktrees/track-SAVE-WAIT
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.25 d (basis: FIX-RESET-RECENCY, a test-only change of the same size)
merged: none
updated: 2026-10-27
next: Find why production_reload_anywhere ran past 300 s on gate/batch-30 (ci 40449554417), then step 1.
blocked_by: none
---

# SAVE-WAIT: The save-machine tests fail with a message instead of hanging

## Goal

Every wait in `crates/tc-app/tests/app/save_machine.rs` has a limit and ends with a "hung: …"
message that names what it waited for.

## Why it matters to a translator

It doesn't, directly: both waits are in test code, and no translator can reach them. A hung test
holds a heavy-run slot until tc-heavy stops it after 5 minutes, and the report then names only
the thread, not the wait.

## Before you start

- A tc-app run hung at 18:31 on 2026-10-26 on CHK-6's tree (`/tmp/tc-heavy-hang-20261026-183145.txt`).
  Its live test thread was `app::save_machi…`. The 20 `app::align::a_v` threads in the report
  were most likely rayon's global pool: it starts 20 unnamed threads on a 20-core machine, and
  Linux gives each the name of the thread that first used rayon.
- 92 nextest loops of `app::save_machine::*` (20 minutes) did not reproduce it. The hung run was
  `cargo test`, with every test in one process.
- Notes: side-session-notes `2026-10-26-tc-app-hang-1831.md`.

## Funnel card

- **Invariants**:
  - A save-machine test that waits longer than its limit fails and names the wait.
- **Wrong states**:
  - `World::deliver` (save_machine.rs, about line 680): after a minute's poll, `flight.handle.await`
    waits with no limit when the store neither finished nor reached a hold.
  - `World::settle` (save_machine.rs, about line 853): `while self.t.app.holding() > 0 { release;
    sleep 1 ms }` has no cap.
- **Code**:
  - A timeout round each wait, with a "hung: …" message, as `store_world::for_a_minute` and the
    other waits in the file do.
- **Checks**:
  - The save-machine tests pass as before; a deliberately held store makes each wait fail with
    its message instead of hanging.
- **Human**: none.

## Steps

### Step 1: Bound both waits

## Log

- 2026-10-26 | reset-recency session (Claude Code d4604680) | created | none | from the orchestrator's message: the two unbounded waits found while looking for the 18:31 hang; after the release, since no translator can reach either | side-session-notes 2026-10-26-tc-app-hang-1831.md
- 2026-10-27 01:10 EDT | save-wait session (Claude Code 749c177d) | claim | none | a release card now: REL-1 waits for it, and it is no_loss, since save-path tests keep timing out in CI (production_reload_anywhere at 300 s on gate/batch-30, ci 40449554417; before it c3_reload and FIX-KEPT-EVENT's test); its REL-1 dependency is turned round | gh run view 40449554417 --log
