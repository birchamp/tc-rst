---
id: CHK-1a
title: Architecture, I/O and network checks in tc-arch, and ARCHITECTURE.md matching the code
track: A
lane: checks
milestone: M1
state: gating
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-1
worktree: .worktrees/track-CHK-1
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (steps 1-4 and 6 of CHK-1, already built)
merged: none
updated: 2026-10-24
next: the merge on the orchestrator's turn: merge main in once, scripts/gates.sh --wait-ci CHK-1a, fast-forward main, push, report
blocked_by: none
---

# CHK-1a: Architecture, I/O and network checks in tc-arch, and ARCHITECTURE.md matching the code

## Goal

The parts of CHK-1 that need no other card, split off so they reach main now (the
orchestrator's yes of 2026-10-24): CHK-1 steps 1-4 and 6. Step 5 (clippy.toml in tc-store and tc-journal) stays in CHK-1 until S2 merges.

## Why it matters to a translator

A translator's project is written only through the crash-safe path and the network only behind the internet switch; tc-arch fails a change that breaks either, naming the rule.

## Before you start

- `units/CHK-1.md`: its steps 1-4 and 6 and their log lines are this card's work.

## Funnel card

- **Invariants**:
  - INV-ARCHITECTURE, INV-PURE-CRATES-DO-NO-IO, INV-ATOMIC-WRITES (the source scan), INV-ONE-PARSER-OWNER, INV-LOCAL-SERVER-IS-CLOSED: as `units/CHK-1.md` states them.
- **Wrong states**:
  - A new crate edge, an I/O call in a pure crate, an unlisted file write, a parser internal used outside its crate, or a route outside the server's guard.
- **Code**:
  - See `units/CHK-1.md` "Code"; this card holds the parts its steps 1-4 and 6 built.
- **Checks**:
  - `crates/tc-arch/tests/architecture.rs` (six tests) and `crates/tc-arch/tests/io_boundaries.rs` (four tests); kind: architecture; runs in: CI job `test`.
  - `request_without_token_is_401` in `crates/tc-server/tests/security.rs`, now over every registered path.
  - Proof it can fail: CHK-1's log, the planted failures of its steps 1-4 and 6.
- **Prose**: none beyond CHK-1's.
- **Human**: none.

## Steps

### Step 1: Done on track-CHK-1

- Do: CHK-1 steps 1-4 and 6 (`units/CHK-1.md`).
- Done when: CI is green on the branch; the review's verify line is written.
- Record: CHK-1's log holds each step; this card's log holds the split, the review and the merge.

## Gates before merge

`scripts/gates.sh --wait-ci CHK-1a` on the merge commit (INT-8), and the review gate of
`FUNNEL.md` §8: a `verify` log line before `done`.

## Stop and ask when

- A gate fails twice in a row and you cannot tell why.

## Log

- 2026-10-24 00:31 EDT | checks lane (Claude Code 2198ee19) | created | 98eca717 | Split from CHK-1 on the orchestrator's yes of 2026-10-24: CHK-1 steps 1-4 and 6 (done, CI green) reach main now; Step 5 (clippy.toml in tc-store and tc-journal) stays in CHK-1 until S2 merges. | .audit/roadmap/units/CHK-1.md
- 2026-10-24 00:36 EDT | checks lane (Claude Code 2198ee19) | verify | 84759382 | Independent review (a fresh general-purpose agent, read only, ran cargo test -p tc-arch: 16 pass). 1 Code: the five common write and I/O forms asked about are caught; File::options( was missed (a form tc-door43 uses). 2 Checks: the edge table equals cargo metadata's normal member edges (all 21), target-specific dependencies counted as normal. 3 Reach: a non-atomic project write passing CI. 4 Siblings: three allow-list reasons read and true; none writes in a project. 5 No loss: test code and docs only. 6 Prose: ARCHITECTURE.md matches. Blocker (probable, unverified): tests/common.rs's canonicalize() gives Windows' \\?\ root, so rel() would not strip manifest paths and the allow-list tests would fail on the nightly Windows job. Major: File::options( missing from scan_writes. Minors recorded | .audit/minors.tsv
- 2026-10-24 00:36 EDT | checks lane (Claude Code 2198ee19) | fix | none | tests/common.rs takes the root from cargo metadata's workspace_root (no canonicalize); scan_writes also flags File::options(. cargo test -p tc-arch: 16 pass. The Windows half is proven by CI's nightly-windows job dispatched on the branch | crates/tc-arch/tests/common.rs
- 2026-10-24 01:22 EDT | checks lane (Claude Code 2198ee19) | gate | a3102411 | The review's probable Windows blocker is cleared: CI run 40062995961 dispatched on the branch ran every job green, nightly-windows included (cargo test --workspace on windows-latest, tc-arch's allow-list tests among them) | gh run view 40062995961
- 2026-10-24 01:44 EDT | checks lane (Claude Code 2198ee19) | note | none | On the orchestrator's option A, this merge also carries the alignment budget's CI change: suggestion_budget.rs asserts its p95 only with TC_ALIGN_BUDGET_ENFORCE set (cards P and G2 set it on the reference laptop); notes in CHK-6, P and G2. Main merged in once (589720e3) | crates/tc-align/tests/suggestion_budget.rs
