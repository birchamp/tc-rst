---
id: CHK-7
title: CI runs the full tests only on gates and main; fast checks before a gate; nightly failures become cards
track: A
lane: integration
milestone: M1
state: verifying
owner: integration lane (Claude Code 3d07d5f8)
branch: track-CHK-7
worktree: .worktrees/track-CHK-7
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: three workflow conditions, one script step, one stub script)
merged: none
updated: 2026-10-24
next: One verify pass, then the batch after batch 3. Item 4 (the daily trial merge) is a standing duty of the integration lane, not code; item 5 is answered in the log.
blocked_by: none
---

# CHK-7: CI runs the full tests only on gates and main; fast checks before a gate; nightly failures become cards

## Goal

The owner's test-process ruling of 2026-10-24 (`rulings/2026-10-24-test-process.md`) holds in
the workflows and scripts: on a track branch CI runs only the compile and lint jobs; the full
tests, the journeys and app-e2e run on `gate/*` and main; a superseded track run is cancelled,
a gate or main run never is. `scripts/gates.sh --wait-ci` runs the board check, the claims
check and the journey coverage check locally before it pushes, and stops on a failure. A failed
nightly run pushes a card stub to a `nightly/<date>-<job>` branch for the integration lane.

## Why it matters to a translator

A translator never sees it. It keeps CI's runners free for the runs that gate a merge, so fixes
reach translators sooner, and no nightly failure goes unread.

## Before you start

- `grep -n 'cancel-in-progress' .github/workflows/*.yml` shows UI-3a's rule (gate/* and main
  never cancelled) on all three workflows.
- `sed -n '/^wait_ci()/,/^}/p' scripts/gates.sh` prints the gate's push and wait.

## Funnel card

- **Invariants**:
  - INV-FULL-TESTS-GATE-MERGES: the `test`, `alignment-budget` and `parity-baseline` jobs of
    `ci.yml` and the `journeys` job of `app-e2e.yml` run on main, on `gate/*` and on request,
    and not on other pushes.
  - INV-NIGHTLY-FAILURE-IS-A-CARD: a failed scheduled nightly job pushes a card stub naming
    its run.
- **Wrong states**:
  - Every push to a track branch runs the 75-minute test job and the journeys, so the runs
    that gate merges wait in the queue.
  - A gate push carries a card that contradicts git, found only after 30 minutes of CI.
  - A nightly failure is seen by nobody.
- **Code**:
  - Types: none; workflow conditions and a script.
  - One entry point: `scripts/gates.sh --wait-ci` for every merge gate.
  - Removed: `pull_request` from app-e2e (journeys run on gates).
  - Left to checks, and why: workflow text is not a type; `gates-sync.test.mjs` holds it.
- **Checks**:
  - `INV-FULL-TESTS-GATE-MERGES` case in `.audit/tools/gates-sync.test.mjs`; kind: lint over
    the workflow text; runs in CI's parity-baseline job (on gates).
  - The `INV-JOURNEYS-RUN` case asserts app-e2e's push branches are main and `gate/**`.
  - Proof it can fail: drop the condition from the `test` job; the new case fails.
- **Prose**: TRACKING's merge checklist already says it (PROCESS-6).
- **Human**: none; ruled.

## Steps

### Step 1: The workflow conditions

- Do: give `test`, `alignment-budget` and `parity-baseline` in `ci.yml` the condition "main,
  `gate/*` or a dispatch"; limit app-e2e's push trigger to main and `gate/**` and drop its
  `pull_request`; extend `gates-sync.test.mjs`.
- Done when: `node --test .audit/tools/*.test.mjs` passes and fails with a condition removed.
- Record: a log line `step 1`.

### Step 2: Fast checks before a gate

- Do: `wait_ci` in `scripts/gates.sh` runs `board.py --check`, `claims.test.mjs` and
  `coverage.test.mjs` before the push and returns 1 on a failure.
- Done when: with a card set to `building` and `owner: none`, `--wait-ci` stops before pushing.
- Record: a log line `step 2`.

### Step 3: Nightly failures become cards

- Do: `scripts/nightly-stub.sh <job> <log>` writes `units/NIGHTLY-<date>-<job>.md` (state
  todo, the run's URL, the failing seed lines) and pushes `nightly/<date>-<job>`; the nightly
  jobs tee their log and call it on a scheduled failure, with `contents: write`.
- Done when: run locally with a fake log in a scratch clone, it writes a card `board.py --check`
  accepts.
- Record: a log line `step 3`.

### Step 4: Verify and merge

- Do: one verify pass, then a merge batch.

## Gates before merge

`scripts/gates.sh --wait-ci <batch>`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done`, and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A lane needs the full tests on its track branch for a reason a gate cannot serve.

## References

- `rulings/2026-10-24-test-process.md`; TRACKING "Checklist: merge an item".

## Log

- 2026-10-24 | integration lane (Claude Code 3d07d5f8) | created | none | from the owner's test-process ruling, assigned by the orchestrator | rulings/2026-10-24-test-process.md
- 2026-10-24 19:26 EDT | integration lane (Claude Code 3d07d5f8) | claim | none | claimed on track-CHK-7, cut from batch/3 (UI-3a's cancel rule and PROCESS-6 are there); merges in the batch after 3 | python3 -I -B .audit/tools/board.py
- 2026-10-24 19:28 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | ci.yml's test, alignment-budget and parity-baseline run only on main, gate/* and on request; app-e2e pushes only on main and gate/** (pull_request dropped); the cancel rule from UI-3a stays. gates-sync: INV-JOURNEYS-RUN checks the branches, new INV-FULL-TESTS-GATE-MERGES case; 39 pass. With the test job's condition removed: `INV-FULL-TESTS-GATE-MERGES: ci.yml job test runs on every push` | node --test .audit/tools/*.test.mjs
- 2026-10-24 19:28 EDT | integration lane (Claude Code 3d07d5f8) | step 2 | none | wait_ci runs board.py --check, claims.test.mjs and coverage.test.mjs before it pushes. With C7b set to building and owner none (scratch commit, undone): `gates: --wait-ci stops before pushing: board.py --check or the claims and coverage checks fail; run them to see why`, exit 1, and no gate/probe-chk7 branch was pushed | scripts/gates.sh --wait-ci probe-chk7
- 2026-10-24 19:32 EDT | integration lane (Claude Code 3d07d5f8) | step 3 | none | scripts/nightly-stub.sh writes units/NIGHTLY-<date>-<job>.md (state todo, the run URL, the log lines naming a seed or a failure), adds it to DONE's depends, commits on nightly/<date>-<job> and pushes; the two nightly jobs tee their log (bash with pipefail) and call it on a scheduled failure, with contents: write for that job only. In a scratch clone with a fake log and --no-push: the card is written with the seed and panic lines, and board.py --check shows 0 errors | scripts/nightly-stub.sh model-long <log> --no-push
- 2026-10-24 19:32 EDT | integration lane (Claude Code 3d07d5f8) | note | none | item 5 of the orchestrator's brief: op_counts::align::align_save_costs_one_verse and its _while_model_training_is_held variant pass in every main and gate ci run since batch 1 (40127991235, 40133930168, 40136425572, 40139145271: 2 ok each); fix-book-removal-unseen's hold of draft writes fixed it | gh run view <id> --log
