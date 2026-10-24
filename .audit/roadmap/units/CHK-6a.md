---
id: CHK-6a
title: Release builds without the test server, every ignored test named, PLAN's reference checked
track: A
lane: checks
milestone: M1
state: done
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-6
worktree: .worktrees/track-CHK-6
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (steps 1, 4 and 6 of CHK-6, already built)
merged: 673e8df2
updated: 2026-10-24
next: none
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
- 2026-10-24 00:35 EDT | checks lane (Claude Code 2198ee19) | verify | 9f2cea4c | Independent review (a fresh general-purpose agent, read only, ran the node tests: 9 and 12 pass). 1 Code: FieldKind::Test is in every build; without test-hooks serde refuses it, and every reader parses entry by entry (tc-ui buffer.rs parse_item_key, current_lines, merge; tc-app drafts.rs offer), so an old stored entry loses only itself; stub is used only by tc-server's own tests. 2 Checks: the ignored step lists exactly the ignored tests, sits after test in CI's test job; PLAN's regex matches line 40. 3 Reach: a release build reading an old stored test entry: that entry is skipped. 4 Siblings: no other link to stub. 5 No loss: nothing can lose or rewrite a translator's data. 6 Prose: comments give reasons. No blockers, no majors; minors: proptest-derive ignores serde(skip), so cargo test -p tc-ui alone generated Test and failed; check-ignored.sh hid cargo's errors; bare test names; PLAN's wrap | .audit/minors.tsv
- 2026-10-24 00:35 EDT | checks lane (Claude Code 2198ee19) | fix | none | FieldKind::Test also carries proptest(skip) without test-hooks: item_keys_are_unambiguous_for_any_folder_name passes with cargo test -p tc-ui alone; check-ignored.sh lets cargo's errors through. clippy, clippy-release-app, wasm-ui, wasm-ui-hooks and the ignored step pass; the other two minors are in minors.tsv | crates/tc-api/src/buffer.rs
- 2026-10-24 00:44 EDT | checks lane (Claude Code 2198ee19) | merge | 673e8df2 | Merge commit `Merge CHK-6a: no test server in release builds, every ignored test named, PLAN's reference checked`, on the orchestrator's turn after INT-7 (1675b644); no conflict (minors.tsv auto-merged). Gated by scripts/gates.sh --wait-ci CHK-6a | scripts/gates.sh --check-receipt
