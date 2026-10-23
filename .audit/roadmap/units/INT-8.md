---
id: INT-8
title: CI runs are the gate receipt
track: A
lane: checks
milestone: M0
state: gating
owner: checks lane (Claude Code 2198ee19)
branch: track-INT-8
worktree: .worktrees/track-INT-8
depends: [INT-3, G5]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: INT-3's receipt code and tests, which this extends)
merged: none
updated: 2026-10-23
next: merge by the CI route once G5 is done: push the merge commit to gate/INT-8, --wait-ci INT-8, fast-forward main, push, tell the lanes
blocked_by: none
---

# INT-8: CI runs are the gate receipt

## Goal

A green GitHub run counts as the gate receipt: `scripts/gates.sh --check-receipt` accepts, besides
the local receipt, a completed, successful run of both the `ci` and the `app-e2e` workflows on a
commit whose tree is the tree being merged. `scripts/gates.sh --wait-ci <card>` pushes the merge
commit to `gate/<card>`, waits for both workflows, and runs the check. Lanes stop queueing for
this PC's heavy-test slot.

## Why it matters to a translator

A translator never sees it. Four lanes were waiting hours for one PC's test slot; GitHub runs
the same list in about 25 to 30 minutes with its jobs in parallel, so fixes reach main sooner
and every merge is still tested on its exact tree.

## Before you start

- The owner asked for it on 2026-10-23 (relayed by the audit session): "make a green GitHub CI run count
  as the gate receipt, so the lanes stop queueing for this PC's one heavy-test slot. GitHub Actions
  minutes are free (the repository is public) and a push run finishes in about 25-30 minutes".
- `gh auth status` succeeds and `gh api "repos/{owner}/{repo}/actions/runs?head_sha=<sha>"`
  lists runs by commit with `name`, `status`, `conclusion` and `head_sha`.
- G5's gate change (`track-G5-gate`, 36666b59) makes the plain run include the journeys and the
  receipt require `journeys yes`; this card starts from that tip so `scripts/gates.sh` does not
  conflict.

## Funnel card

- **Invariants**:
  - INV-NO-UNGATED-MERGE (extended): main moves only to a commit whose tree has a local receipt
    from a full clean run, or a green `ci` and `app-e2e` run on a commit with that same tree.
  - INV-LOCAL-PASS-MEANS-CI-PASS: unchanged; the drift tests keep `ci.yml`, `app-e2e.yml` and
    `scripts/gates.sh` in step.
- **Wrong states**:
  - A merge commit tested by CI on a branch, then re-made with a different tree before main moves.
  - A run that is cancelled or still in progress counted as green.
- **Code**:
  - Types: none; one function `ci_receipt` in `scripts/gates.sh`.
  - One entry point: `check_receipt` tries the local receipt, then `ci_receipt`.
  - Removed: the requirement of a local full run before a merge (TRACKING.md step 1).
  - Left to checks, and why: whether a run is green is GitHub's data; the script reads it.
- **Checks**:
  - `INV-NO-UNGATED-MERGE: a green ci and app-e2e run on the same tree is a receipt` in
    `.audit/tools/gates-script.test.mjs` (a fake `gh` on PATH answers canned runs); kind: unit;
    runs in: the gate step `node`. Proof it can fail: the fake answers only a `ci` run, or a run
    on another commit: refused.
- **Prose**: README.md push rule (`gate/*` branches); TRACKING.md merge checklist step 1.
- **Human**: none; the owner asked for it.

## Steps

### Step 1: The CI receipt and `--wait-ci`

- Do: in `scripts/gates.sh`: `ci_receipt` queries the runs for HEAD's sha with `gh api`, keeps
  completed successful runs of `ci` and `app-e2e`, requires `<head_sha>^{tree}` to equal
  `HEAD^{tree}`, prints `gates: CI receipt: ci run <id>, app-e2e run <id> on <sha>` and returns 0;
  else the INV-NO-UNGATED-MERGE line with the reason. `check_receipt` accepts either receipt.
  `--wait-ci <card>`: refuses a dirty tree, pushes `HEAD` to `refs/heads/gate/<card>`
  (updating it), polls every 60 s until both workflows have a completed run for HEAD (90 min
  bound), then runs `check_receipt`. Tests in `gates-script.test.mjs` with a fake `gh`.
- Files: `scripts/gates.sh`, `.audit/tools/gates-script.test.mjs`.
- Done when: `node --test .audit/tools/*.test.mjs` passes; the fake-gh refusals are seen.
- Record: a log line `step 1`.

### Step 2: Prose and merge

- Do: README.md push rule: `gate/<card>` branches may be pushed for this purpose; TRACKING.md
  "Checklist: merge an item" step 1: the CI route; ROADMAP.md M0 row. Merge this card by its own
  route: push the merge commit to `gate/INT-8`, wait for green, `--check-receipt`, fast-forward
  main, push. Tell the other lane sessions.
- Files: `.audit/roadmap/README.md`, `TRACKING.md`, `ROADMAP.md`.
- Done when: main is at the merge commit, CI green on it, the sessions told.
- Record: log lines `step 2`, `gate`, `merge`.

## Gates before merge

`scripts/gates.sh --check-receipt` exits 0 on the merge commit (local or CI receipt); the node
tests pass.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done`, and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- GitHub refuses the `gate/*` push or has no runs for a pushed commit after 90 minutes.

## Log

- 2026-10-23 14:03 EDT | checks lane (Claude Code 2198ee19) | created | none | the owner's request of 2026-10-23, relayed by the audit session, quoted in "Before you start" | .audit/roadmap/units/INT-8.md
- 2026-10-23 14:03 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-INT-8 in .worktrees/track-INT-8, branched from track-G5-gate (36666b59, which holds main d293375a) so the gates.sh edits do not conflict with G5's; note at the claim: G5 is not done yet, so the merge waits for it | python3 -I .audit/tools/board.py
- 2026-10-23 14:05 EDT | checks lane (Claude Code 2198ee19) | step 1 | none | scripts/gates.sh: check_receipt takes the local receipt (journeys yes) or ci_receipt, which asks gh for the runs of HEAD's sha, keeps completed successful ci and app-e2e runs whose commit has HEAD's tree, and prints `gates: CI receipt: ci run <id>, app-e2e run <id> on <sha>`; --wait-ci <card> refuses a dirty tree, pushes HEAD to gate/<card>, polls every 60 s up to 90 min for completed runs of both workflows, then runs check_receipt. gates-script.test.mjs: a fake gh on PATH; green ci+app-e2e on HEAD is accepted, ci alone refused, both on another tree refused; node tests 10 pass | node --test .audit/tools/*.test.mjs
- 2026-10-23 14:05 EDT | checks lane (Claude Code 2198ee19) | step 2 | none | README.md push rule: gate/<card> branches; TRACKING.md merge checklist step 1: the CI route, no local full run needed; ROADMAP.md M0 row; DONE depends on INT-8 | python3 -I .audit/tools/board.py --check
