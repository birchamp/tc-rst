---
id: CHK-7b
title: One command runs what a branch's change reaches before it is handed over
track: A
lane: integration
milestone: M1
state: gating
owner: integration lane (Claude Code 3d07d5f8)
branch: track-CHK-7b
worktree: .worktrees/track-CHK-7b
depends: [CHK-7]
needs_owner: []
proves: []
no_loss: false
size: 0.25 d (basis: one node selector, one gates.sh mode)
merged: none
updated: 2026-10-25
next: Batch 7, with C7b.
blocked_by: none
---

# CHK-7b: One command runs what a branch's change reaches before it is handed over

## Goal

Since CHK-7 a track push runs only the lints, so a lane's own tests must run before hand-over.
`scripts/gates.sh --touched [base]` runs them: the fast checks, the tests of every crate the
branch changed and of every workspace crate that depends on one, tc-api's endpoint and golden
lists, and every browser journey that names a changed endpoint or test id. CI's test step runs
with `--no-fail-fast`, so one failing crate no longer hides the rest.

## Why it matters to a translator

Batch 6's first gate failed on a REF1c endpoint missing from tc-api's list and a journey D2
changed the meaning of; each cost a 45-minute gate. Catching them on the branch gets fixes to
translators sooner.

## Funnel card

- **Invariants**:
  - INV-TOUCHED-RUNS-WHAT-A-CHANGE-REACHES: `--touched` runs every changed crate and its
    dependents, and every journey that names a changed endpoint or test id.
- **Checks**:
  - `.audit/tools/touched.test.mjs`; kind: unit; runs in CI's node step.
  - Proof it can fail: drop the dependents loop, or the token match, and its two cases fail; on
    batch 6 against batch 5 it picks `community-checking.test.mjs`, the journey that broke.
- **Prose**: TRACKING's "How a card gets ready" item 3 names `scripts/gates.sh --touched`.

## Steps

### Step 1: The selector, the mode and the rule

- Do: `.audit/tools/touched.mjs`, `scripts/gates.sh --touched`, `--no-fail-fast` on the test
  step, TRACKING item 3.

## Gates before merge

`scripts/gates.sh --wait-ci <batch>`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done`.

## Log

- 2026-10-25 | integration lane (Claude Code 3d07d5f8) | created | none | the orchestrator's brief after batch 6's first gate | message from the orchestrator
- 2026-10-25 12:26 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | touched.mjs selects crates (with dependents from cargo metadata path deps) and journeys (changed files, or naming a quoted endpoint path or test id from a changed line of tc-ui, tc-api, tc-app, tc-server or tc4 src); gates.sh --touched runs board, claims, coverage, touched.test, tc-api endpoints and golden, cargo test --no-fail-fast on the crates, and journeys() on the files; the test step and the VM agree on --no-fail-fast. On batch 6 against batch 5 it picks 16 crates and 15 journey files, among them community-checking.test.mjs; against batch 6 itself, 0 and 0, and its fast checks and tc-api lists pass | scripts/gates.sh --touched batch/6
- 2026-10-25 12:28 EDT | integration lane (Claude Code 3d07d5f8) | verify | 450f98ba | one pass by a fresh read-only reviewer agent: base and diff ranges right; crate names match packages; dependents include dev-dependencies; --package= expands; journeys() still globs all with no arguments; board, node and touched lines fail under set -e, cargo and journey failures add up; --no-fail-fast changes only how much a failure reports. §8: code a heuristic selector; checks exist and run in CI's node step; reach N/A (tooling); siblings: run_all fine; no-loss N/A; prose fine. One major: under `journeys ... || code=1` bash turns set -e off, so a failed build let the journeys run against an older one. Minors: one-word and format! test ids, changes with no quoted token, paths outside crates/ and untracked files pick nothing; a deleted file's removed lines were not read | reviewer report in this session
- 2026-10-25 12:28 EDT | integration lane (Claude Code 3d07d5f8) | fix | none | each build in journeys() returns 1 on failure; a deleted file keeps its `---` path, with a test case; the other three minors are in .audit/minors.tsv (the batch gate runs everything) | node --test .audit/tools/touched.test.mjs
