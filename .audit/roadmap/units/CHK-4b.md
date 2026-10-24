---
id: CHK-4b
title: tc-app's tests wait for events, never for a short clock
track: A
lane: checks
milestone: M1
state: done
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-4b
worktree: .worktrees/track-CHK-4b
depends: [CHK-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d
merged: 0bbd6d51
updated: 2026-10-24
next: none
blocked_by: none
---

# CHK-4b: tc-app's tests wait for events, never for a short clock

## Goal

No tc-app test fails because a loaded machine was slow. Every wait for something to happen
waits for the event itself, with a limit of about a minute that only catches a hang and names
the step that hung (rule 1 of `rulings/2026-10-23-tests-prove-behavior-not-speed.md`).

## Why it matters to a translator

Flaky tests hold every merge, so fixes reach translators later. A test that fails on speed also
hides real failures among false ones.

## Before you start

- The orchestrator asked for this card on 2026-10-24: tc-app's `test` job failed in a different
  timing test on almost every CI run (the align timer, understand's open, save_machine's C3).
- CHK-4 (done) raised the waits that had flaked by then; this sweeps the rest of tc-app's tests.

## Funnel card

- **Invariants**:
  - INV-WAITS-CATCH-HANGS: a tc-app test that waits for something to happen waits for the event
    with a limit of about a minute, and its failure says the step hung; no test asserts how fast a
    step ran (ruling 2026-10-23, rule 1).
- **Wrong states**:
  - CI fails because a loaded runner took 1.2 s where a loop allowed 1000 sleeps of 1 ms.
  - A timeout of 5 s fails a save that took 6 s and nobody can tell it from a real hang.
- **Code**:
  - `for_a_minute()` in `crates/tc-app/tests/support/store_world.rs`: the turns of a polling loop.
  - Removed: the 2 s timer limit of app_data's frozen-runtime test, the 5 s quit limit of seed's
    stop test, the 420 ms upper limit of store_scenario's first progress report.
  - Left to checks, and why a type cannot carry it: how a test waits is a property of test code.
- **Checks**: the existing tests, which keep their behavior; a review of the sweep.
- **Prose**: none.
- **Human**: none.

## Steps

### Step 1: Sweep the waits

- Do: in `crates/tc-app/tests`, raise every deadline under a minute that waits for something
  to happen (`tokio::time::timeout`, `recv_timeout`, an `Instant` deadline, a loop of a few
  thousand 1 ms sleeps) to a minute, and make its failure start with `hung:` and name the step.
  A loop of sleeps uses `store_world::for_a_minute()`, which ends as soon as the condition holds.
  Keep the waits that check something has not happened yet (a 20 ms, 100 ms or 300 ms window):
  slowness can only make those pass. Remove assertions that a step took less than some time.
- Files: `crates/tc-app/tests/**`.
- Done when: `grep` finds no wait under a minute for something to happen, and
  `cargo test --locked -p tc-app` passes with no slower suite.
- Record: a log line `step 1` with what changed.

### Step 2: Gate and merge

- Do: `scripts/gates.sh --wait-ci CHK-4b` on the merge commit, on the orchestrator's turn.

## Log

- 2026-10-24 10:11 EDT | checks lane (Claude Code 2198ee19) | claim | none | Created and claimed on the orchestrator's request, on branch track-CHK-4b from main d8176c40 | python3 -I .audit/tools/board.py
- 2026-10-24 10:21 EDT | checks lane (Claude Code 2198ee19) | step 1 | none | 45 deadlines under a minute that wait for something to happen are now a minute with a "hung:" message naming the step; 22 polling loops capped at 100 to 5000 sleeps of 1-50 ms (0.1 to 10 s) now run for_a_minute() and still end as soon as their condition holds, among them the align timer test's 1000 x 1 ms loop and save_machine's 200 x 1 ms wait for a delivery to finish or hold; dispatch's 50 ms sleep before reading the preferences is now a wait for them. Removed speed limits: app_data's 2 s timer (a frozen runtime cannot end its sleep before the release, so the hang bound proves it), seed's 5 s quit (now a hang bound), store_scenario's 420 ms upper limit on the first progress report (the 300 ms lower limit stays: it is behavior). Kept: the 20, 100 and 300 ms windows and the sleeps before asserting that something has not happened yet, and drain loops that stop on an event. cargo test --locked -p tc-app passes: app 667 in 191 s, op_counts 45 in 364 s (before: 162-252 s and 404-466 s on this machine) | cargo test --locked -p tc-app
- 2026-10-24 10:39 EDT | checks lane (Claude Code 2198ee19) | verify | none | Review (correctness lens, one subagent, read only). Blocker: dispatch's new wait looked at the first of three queued preferences, then read the last, which a write's fsync could still hold back. Major: align_suggestions' four 1 s timeouts run on a paused clock, where they check order (before the 2 s save timer and the 5 s reply), not speed, so raising them removed the checks; and four "sleep 30 ms so the second open joins the held one" sites (sessions, create, store_contract twice) still raced the join. Minors: failures after for_a_minute() loops did not say hung; three messages read backwards after the prefix; jobs' wait for the slow read had no bound; seed's quit had none | review report in the session transcript
- 2026-10-24 10:39 EDT | checks lane (Claude Code 2198ee19) | fix | none | dispatch waits for the place, the last message in the writer's queue. align_suggestions' four paused-clock bounds are 1 s again, with a comment saying why. The four join sites poll the joining open once before the release (store_world::poll_once): an open registers on its first poll, so it has joined without any sleep, and the "one open" checks would fail if it had not. Post-loop failures start with "hung:"; the three messages read the right way; jobs' slow-read wait and seed's quit have a minute's bound. cargo test --locked -p tc-app passes (app 667 in 140 s, op_counts 45 in 415 s, store_contract 42); clippy passes for Linux and Windows | cargo test --locked -p tc-app; cargo clippy --locked -p tc-app --tests [--target x86_64-pc-windows-gnu] -- -D warnings
- 2026-10-24 11:14 EDT | checks lane (Claude Code 2198ee19) | merge | 0bbd6d51 | Merge commit `Merge CHK-4b: tc-app's tests wait for events, never for a short clock`, on the orchestrator's turn after DOM-1 (f878719f). Main merged into the branch once (0bbd6d51), no conflict; DOM-1 changed no tc-app test | python3 -I .audit/tools/board.py --check
- 2026-10-24 12:02 EDT | checks lane (Claude Code 2198ee19) | fix | none | The gate's CI run on 0bbd6d51's merge (255b5cfa, run 40106091901) failed only the node step: INV-INVARIANT-NAMES-ARE-USED found INV-WAITS-CATCH-HANGS named in this card and nowhere in the code. The helper every polling wait uses, for_a_minute(), now names it. All node tests pass locally on the amended merge | node --test .audit/tools/*.test.mjs
