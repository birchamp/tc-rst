---
id: CHK-3a
title: Claimed tests must exist: TC4-ISSUES, the done cards, the P-ledger and the tC4 journey map
track: A
lane: checks
milestone: M1
state: verifying
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-3
worktree: .worktrees/track-CHK-3
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (steps 1-4 and 3b of CHK-3, already built)
merged: none
updated: 2026-10-24
next: the review's verify line, then the merge on the orchestrator's turn: merge main in once, scripts/gates.sh --wait-ci CHK-3a, fast-forward main, push
blocked_by: none
---

# CHK-3a: Claimed tests must exist: TC4-ISSUES, the done cards, the P-ledger and the tC4 journey map

## Goal

The parts of CHK-3 that need no other card, split off so they reach main now (the
orchestrator's yes of 2026-10-24): CHK-3 steps 1-4 and 3b. Step 5 (STORAGE.md section 15) stays in CHK-3 until S2 merges.

## Why it matters to a translator

The documents that say what protects a translator's work name only tests that exist, or the unfinished card that will write them.

## Before you start

- `units/CHK-3.md`: its steps 1-4 and 3b and their log lines are this card's work.

## Funnel card

- **Invariants**:
  - INV-CLAIMED-TESTS-EXIST, INV-INVARIANT-NAMES-ARE-USED, INV-P1-COVERED: as `units/CHK-3.md` states them.
- **Wrong states**:
  - A covered row, a done card or the P-ledger names a renamed or never-written test; a live tC4 journey or spec maps to nothing.
- **Code**:
  - See `units/CHK-3.md` "Code"; this card holds the parts its steps 1-4 and 3b built.
- **Checks**:
  - `.audit/tools/claims.test.mjs` (TC4-ISSUES, units, P-LEDGER, the resolver) and `parity/app/journeys/coverage.test.mjs` (three tests); kind: ledger; runs in: the gate step `node`.
  - `.audit/tools/gates-sync.test.mjs`'s INV-CI-IS-THE-GATE, INV-BUILDS-ON-WINDOWS and INV-JOURNEYS-RUN tests.
  - Proof it can fail: CHK-3's log, the planted failures of its steps 1-4 and 3b.
- **Prose**: none beyond CHK-3's.
- **Human**: none.

## Steps

### Step 1: Done on track-CHK-3

- Do: CHK-3 steps 1-4 and 3b (`units/CHK-3.md`).
- Done when: CI is green on the branch; the review's verify line is written.
- Record: CHK-3's log holds each step; this card's log holds the split, the review and the merge.

## Gates before merge

`scripts/gates.sh --wait-ci CHK-3a` on the merge commit (INT-8), and the review gate of
`FUNNEL.md` §8: a `verify` log line before `done`.

## Stop and ask when

- A gate fails twice in a row and you cannot tell why.

## Log

- 2026-10-24 00:31 EDT | checks lane (Claude Code 2198ee19) | created | f0714d9d | Split from CHK-3 on the orchestrator's yes of 2026-10-24: CHK-3 steps 1-4 and 3b (done, CI green) reach main now; Step 5 (STORAGE.md section 15) stays in CHK-3 until S2 merges. | .audit/roadmap/units/CHK-3.md
