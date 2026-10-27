---
id: SAVE-WAIT
title: The save-machine tests fail with a message instead of hanging
track: A
lane: checks
milestone: M5
state: building
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
next: The three lenses (loss first), --touched, then hand to the integration lane.
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
  - INV-WAITS-CATCH-HANGS: a save-machine test that waits longer than its limit fails and names
    the wait, including the store watch's wait when the test ends; no hold outlives a run's settle.
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

### What the hang was (2026-10-27)

Reproduced under load (6 copies on 4 cores; 1 hang in about 65 runs) and caught with gdb. The
test thread sat in `BurritoWatch::drop` → `StoreBackend::idle_waiter` → `wait_idle`, after the
case had ended; the store actor sat at `Point::BeforeDurablePending` in a save's `commit`. A
`Hold` step arms that point; `settle` released only holds a save had reached; a save the page
had dropped (a reload or a cut keeps the call running) reached the store after the run settled,
stopped at the armed hold, and nothing was left to release it. The watch's wait has no limit and
runs on the test's own thread, so the case's 120 s timeout never fired and nextest killed the
test at 300 s without a word. c3_reload (ci 40423604313) runs the same `World`.

No translator can reach it: the hold is a test hook (`test-hooks`, absent from a release
build), and a product save never waits at it. The app needs no change; a person sees nothing.

## Log

- 2026-10-26 | reset-recency session (Claude Code d4604680) | created | none | from the orchestrator's message: the two unbounded waits found while looking for the 18:31 hang; after the release, since no translator can reach either | side-session-notes 2026-10-26-tc-app-hang-1831.md
- 2026-10-27 01:10 EDT | save-wait session (Claude Code 749c177d) | claim | none | a release card now: REL-1 waits for it, and it is no_loss, since save-path tests keep timing out in CI (production_reload_anywhere at 300 s on gate/batch-30, ci 40449554417; before it c3_reload and FIX-KEPT-EVENT's test); its REL-1 dependency is turned round | gh run view 40449554417 --log
- 2026-10-27 01:55 EDT | save-wait session (Claude Code 749c177d) | step 1 | none | settle waits for the calls a reload or cut dropped (releasing a hold each reaches) and leaves no hold armed; deliver's await and settle's release loop stop after a minute with "hung: …"; the store watch's idle wait runs off the test thread and fails after a minute. New test a_save_left_at_a_hold_fails_the_store_watch_instead_of_hanging hangs without the watch fix (killed at 151 s) and passes with it (60 s); app::save_machine 20/20 pass | /tmp/claude-1000/-home-bot-tc-rst/749c177d-e0a2-4ed0-b871-e79b599ba496/scratchpad/loop1/c6-r7.txt (gdb stacks of the hang)
