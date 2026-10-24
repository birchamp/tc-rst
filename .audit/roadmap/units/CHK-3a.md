---
id: CHK-3a
title: Claimed tests must exist: TC4-ISSUES, the done cards, the P-ledger and the tC4 journey map
track: A
lane: checks
milestone: M1
state: gating
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
next: the merge on the orchestrator's turn: merge main in once, scripts/gates.sh --wait-ci CHK-3a, fast-forward main, push, report
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
- 2026-10-24 00:37 EDT | checks lane (Claude Code 2198ee19) | verify | 3cb82ddf | Independent review (a fresh general-purpose agent, read only, ran the node tests: 17 pass; checked board.py --json works in CI and in a shallow clone). 1 Code: the resolver accepted any Rust fn (9,675 names; `new`, `open`, and TC4-ISSUES' "journey `create`" passed through tc-store's fn create). 2 Checks: one (pending: X) in a cell skipped every claim of that cell. 3 Reach: a document claiming protection that does not exist. 4 Siblings: the 29 "FIX-R reconciles" rows are truthful to FIX-R step 5.4; #795 and #507 match its steps 5.1 and 5.3; coverage.json matches tC4 d310bf3 (24 live journeys, 37 specs), five mappings spot-checked. 5 No loss: documents and tests only. 6 Prose: the markers are used only where the cards support them. No blockers; two majors (above); minors recorded | .audit/minors.tsv
- 2026-10-24 00:37 EDT | checks lane (Claude Code 2198ee19) | fix | none | claims.mjs testFns: a Rust fn resolves only from a test file (tests/ or tests.rs), a #[cfg(test)] module, a proptest! block, or under a test attribute; journey files resolve by their stem (`create`). A (pending: ...) marker now excuses only the name it follows (claimsWithPending); #737's from_indices (the product fn its proptest exercises) is unquoted. STATUS.md's "◆" ids are read. Plant: a misspelled name beside #507's pending marker: "INV-CLAIMED-TESTS-EXIST: docs/TC4-ISSUES.md:189 (#507) is covered by a_tile_opens_understand_at_chapter_1_with_the_rails_tooltipz, but no such test exists"; reverted. 14 tests pass | node --test .audit/tools/claims.test.mjs
- 2026-10-24 06:53 EDT | checks lane (Claude Code 2198ee19) | fix | none | Main merged in (29048562: INT-5, INT-7, S2 done); TC4-ISSUES #741 and #731 keep INT-7's text and their FIX-R marker. The check caught the cards done since: P-LEDGER's P4 still pending on S2 (now done: marker dropped); INT-5's Python test (the resolver now reads `def test_` in .audit/tools/*_test.py) and INV-ONE-LIVE-TRACKER (marked not in code: a one-time edit); INT-7's current_lines (named in its Proof item, which names product code to mutate: the units check now skips that item) and INV-BUFFER-REMOVAL-NAMES-TEXT and INV-TORN-LINE-SKIPPED (shell_contract.rs now names each vector's rule). 41 node tests pass; shell_contract passes | node --test .audit/tools/*.test.mjs
