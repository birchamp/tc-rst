---
id: CHK-7c
title: The scale tests run at twice Psalms on pushes and gates, ten times at night
track: A
lane: integration
milestone: M1
state: building
owner: integration lane (Claude Code 3d07d5f8)
branch: track-CHK-7c
worktree: .worktrees/track-CHK-7c
depends: [CHK-7b]
needs_owner: []
proves: []
no_loss: false
size: 0.25 d (basis: one helper, four call sites, one gate step)
merged: none
updated: 2026-10-26
next: Time tc-app's op-count tests at 10 and at 2, one verify pass, then the batch.
blocked_by: none
---

# CHK-7c: The scale tests run at twice Psalms on pushes and gates, ten times at night

## Goal

The owner's ruling of 2026-10-26 (`rulings/2026-10-26-test-scale.md`): the operation-count tests
compare Psalms with a corpus `TC_SCALE_FACTOR` times larger, 2 by default and 10 in the nightly
job. `Spec::test_factor()` in tc-corpus is the one switch; tc-journal's, tc-store's and tc-app's
scale tests build their larger corpus with it, and their messages name the factor.

## Why it matters to a translator

The batch gates ran past their time limit; fixes reached translators a day late. The nightly
run still proves a whole Bible's scale.

## Funnel card

- **Invariants**:
  - INV-ONE-SCALE-SWITCH: every scale test's larger corpus is `Spec::scaled(1, Spec::test_factor())`
    (not in code: a grep of `Spec::scaled(1, 10)` in tests finds none).
- **Checks**:
  - `the_larger_corpus_is_the_test_factor_times_larger` (tc-journal); kind: unit.
  - Gate step `scale-long` runs them at 10 in the nightly job.
- **Prose**: the ruling and AGENTS.md (on main with RULING-H11).

## Steps

### Step 1: The switch

- Do: `Spec::test_factor()`, the four call sites, the conflict test's words, the `scale-long`
  nightly step.

## Gates before merge

`scripts/gates.sh --wait-ci <batch>`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done`.

## Log

- 2026-10-26 | integration lane (Claude Code 3d07d5f8) | created | none | from the owner's ruling, assigned by the orchestrator | rulings/2026-10-26-test-scale.md
- 2026-10-26 08:55 EDT | integration lane (Claude Code 3d07d5f8) | step 1 | none | Spec::test_factor reads TC_SCALE_FACTOR (2 by default, at least 2); tc-app's op_counts corpus (twice), tc-journal's operation_counts and tc-store's corpus use it; the conflicts test and the tc-journal size test name the factor; gates.sh NIGHTLY step scale-long runs the three at 10, and the nightly job calls it. Also j07-export: the export journeys settle the store through test/validate before their first snapshot (batch 7's app-e2e saw .tc/materialized.json change during the PDF export) | node --test .audit/tools/*.test.mjs; cargo clippy
