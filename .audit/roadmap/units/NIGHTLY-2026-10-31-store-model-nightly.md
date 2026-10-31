---
id: NIGHTLY-2026-10-31-store-model-nightly
title: Nightly store-model-nightly failed on 2026-10-31
track: A
lane: integration
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: unknown (a nightly failure, not yet read)
merged: none
updated: 2026-10-31
next: Read the run, reproduce the seed, then fix it or open the card that owns the failing code.
blocked_by: none
---

# NIGHTLY-2026-10-31-store-model-nightly: Nightly store-model-nightly failed on 2026-10-31

## Goal

The failure is reproduced from its seed and fixed, or handed to the card that owns the code.

## Why it matters to a translator

A nightly long run found a history the push gates did not; it may be a way to lose work.

## Before you start

- The run: https://github.com/birchamp/tc-rst/actions/runs/41197788313
- Lines of its log that name a seed or a failure:

    thread 'model::random_histories_lose_no_work_long' (5565) panicked at crates/tc-store/tests/store/model.rs:2483:5:
    Test failed: the revert of ingredients/TIT.usfm: not in the folder or complete projection, no notified full copy in .tc/external
    minimal failing input: history = [
    test model::random_histories_lose_no_work_long ... FAILED
    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 421 filtered out; finished in 118.66s
    error: test failed, to rerun pass `-p tc-store --test store`

## Funnel card

- **Invariants**: the failing test's own.
- **Checks**: the failing test, rerun with the seed above.

## Steps

### Step 1: Reproduce, then fix or hand over

- Do: rerun the failing test with the seed; fix the cause or open a card for its owner.
- Record: a log line `step 1`.

## Log

- 2026-10-31 | nightly (store-model-nightly) | created | none | the scheduled store-model-nightly job failed | https://github.com/birchamp/tc-rst/actions/runs/41197788313
