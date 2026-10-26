---
id: CHK-7c
title: The scale tests run at twice Psalms on pushes and gates, ten times at night
track: A
lane: integration
milestone: M1
state: gating
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
next: The batch gate (scripts/gates.sh --wait-ci), then a verify line.
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
- 2026-10-26 10:55 EDT | integration lane (Claude Code 3d07d5f8) | note | none | the two possible bugs at 2 are the tests' assumptions, not the app's: (1) a_bulk_keep_costs_its_conflicts: on the store, the person taken from the first conflict's first version holds 0, 2 or 3 versions in each of the larger corpus's 11 text conflicts and never exactly one, so C7a's rule (keep only where the person holds exactly one) leaves all 11 and stores Unchanged; at 2 on Psalms the same person keeps 3 and leaves 4. Fix: pick a person who holds exactly one version in some conflict of each corpus. (2) translate_text_touches_one_book subtracts the smaller corpus's marked conflicts from the larger's and underflows when the larger holds fewer; compare signed. The four "same count" failures (+1..6 fold values on Psalms) are next: compare each corpus against its own expected size | TC_SCALE_FACTOR=2 run with a temporary print, not committed
- 2026-10-26 12:10 EDT | integration lane (Claude Code 3d07d5f8) | step 2 | none | the six op-count tests hold at 2. The larger corpus's Psalms is not the Psalms corpus record for record (6 devices at 2: verses merged 2 against 0, notes merged 2 against 0, conflicts 11 against 5); every +1..6 fold count matched those keys (decoded = the book's records not merged), none the project's size. A Psalms-alone corpus (the larger journal without other books' records and comments, packs and pending files re-sealed; its Psalms keys' states checked equal) is now the baseline: book_costs_what_it_touches for the rail, the notes open, the book open and Translate's text (now OnStore::Everything; the token-read arithmetic dropped, it did not hold at 2: 0 against -6), cost_alone for the plan. The bulk keep picks a person who holds exactly one version of some conflict | TC_SCALE_FACTOR=2 and 10 tc-app op_counts 48/48; tc-journal operation_counts and tc-store store opcount/groups/checkpoint/outside at 2; clippy; fmt
