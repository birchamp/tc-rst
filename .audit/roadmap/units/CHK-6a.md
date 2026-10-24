---
id: CHK-6a
title: Release builds without the test server, every ignored test named, PLAN's reference checked
track: A
lane: checks
milestone: M1
state: verifying
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-6
worktree: .worktrees/track-CHK-6
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (steps 1, 4 and 6 of CHK-6, already built)
merged: none
updated: 2026-10-24
next: the review's verify line, then the merge on the orchestrator's turn: merge main in once, scripts/gates.sh --wait-ci CHK-6a, fast-forward main, push
blocked_by: none
---

# CHK-6a: Release builds without the test server, every ignored test named, PLAN's reference checked

## Goal

The parts of CHK-6 that need no other card, split off so they reach main now (the
orchestrator's ruling of 2026-10-24): CHK-6 steps 1, 4 and 6, and one CI flake found on the way. The
lint pass, the clippy.toml test and the fresh captures stay in CHK-6.

## Why it matters to a translator

A release build cannot be driven through the test server, a test left out of CI says so, and
the screenshots that stand for "looks like tC4" are tied to the tC4 commit PLAN names.

## Before you start

- `units/CHK-6.md`: its steps 1, 4 and 6 and their log lines are this card's work.

## Funnel card

- **Invariants**:
  - INV-NO-BLESS-IN-CI: a gate run never rewrites what it checks (every blessing variable a
    source reads is refused by scripts/gates.sh), and every `#[ignore]` test names the job that
    runs it.
  - INV-CAPTURES-FRESH (its PLAN half): PLAN's reference commit is the baseline's.
- **Wrong states**:
  - A release build answers the test server's routes or reads a test field off the wire.
  - A new `#[ignore]` test silently never runs.
  - PLAN names one tC4 commit and the screenshots were taken at another.
- **Code**:
  - Types: `pub mod stub` in tc-server only with `test-hooks`; `FieldKind::Test` is skipped by
    serde without `test-hooks` (a cfg on the variant breaks exhaustive matches under cargo's
    feature unification: CHK-6's log, step 1).
  - One entry point: `scripts/check-ignored.sh`, gate step `ignored`.
  - Removed: test-only code from release builds.
  - Left to checks, and why: which tests are ignored, and which variables bless, are facts
    about source text.
- **Checks**:
  - Gate step `ignored` (`scripts/check-ignored.sh` with `scripts/ignored-tests.txt`); kind: lint
    over a run; runs in: CI job `test`.
  - `.audit/tools/ignored.test.mjs` (its two tests); kind: lint; runs in: the gate step `node`.
  - `PLAN's reference commit is the baseline's` in `parity/tc4-baseline/baseline.test.mjs`;
    kind: contract; runs in: the gate step `node`.
  - Proof it can fail: CHK-6's log, steps 4 and 6.
- **Prose**: none.
- **Human**: none.

## Steps

### Step 1: Done on track-CHK-6

- Do: CHK-6 steps 1, 4 and 6 (`units/CHK-6.md`), and the C3 reload scenario's hang-only bound.
- Done when: CI is green on the branch; the review's verify line is written.
- Record: CHK-6's log holds each step; this card's log holds the split, the review and the merge.

## Gates before merge

`scripts/gates.sh --wait-ci CHK-6a` on the merge commit (INT-8), and the review gate of
`FUNNEL.md` §8: a `verify` log line before `done`.

## Stop and ask when

- A gate fails twice in a row and you cannot tell why.

## Log

- 2026-10-24 00:30 EDT | checks lane (Claude Code 2198ee19) | created | 019f3889 | Split from CHK-6 on the orchestrator's yes of 2026-10-24: CHK-6 steps 1, 4 and 6 (done, CI green on 95d6badd except one flake) and the C3 reload scenario's bound (019f3889) reach main now; steps 2, 3 and 5 stay in CHK-6 | .audit/roadmap/units/CHK-6.md
