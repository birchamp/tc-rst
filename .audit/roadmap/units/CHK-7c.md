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
next: Make the six op-count tests hold at 2 (each corpus's own expected size); find whether the bulk keep that writes nothing at 2 is a bug.
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
- 2026-10-26 10:51 EDT | integration lane (Claude Code 3d07d5f8) | verify | 82872631 | one pass by a fresh read-only reviewer agent: test_factor defaults to 2 and is at least 2; all four Spec::scaled test sites use it; the other 10x uses are outside the ruling; the nightly job runs scale-long at 10 and gates-sync passes; every j07 unchanged call passes env. §8: code held by a grep; checks: the renamed size test; reach N/A; siblings: the major; no-loss N/A; prose fine. One major: nine failure messages still said "ten times". Minors: a bad TC_SCALE_FACTOR silently ran at 2; psalms_x10 kept its name; the ruling arrives with RULING-H11; scale-long runs the whole tc-store binary at night | reviewer report in this session
- 2026-10-26 10:51 EDT | integration lane (Claude Code 3d07d5f8) | fix | none | the nine messages name the factor; a set but bad TC_SCALE_FACTOR stops the run; psalms_x10 is psalms_larger. The tc-arch fix for tc-sync (pack_sealer) and the export journeys' snapshot fix went to main in batch 8 | cargo clippy; cargo test -p tc-arch
- 2026-10-26 10:51 EDT | integration lane (Claude Code 3d07d5f8) | note | none | dropped from batch 8 (orchestrator): at 2, six tc-app op-count tests fail (CI run 40328011229): four "same count on both" comparisons see +1..6 fold values, understand.rs:144 subtracts a smaller conflict count from a larger one and underflows (the larger corpus at 2 holds fewer conflicts in that book), and a bulk keep stores Unchanged. tc-app op_counts: 534 s at 10, 102 s at 2 (this PC, 4 threads) | CI run 40328011229
