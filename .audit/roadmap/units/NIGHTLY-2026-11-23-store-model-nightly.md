---
id: NIGHTLY-2026-11-23-store-model-nightly
title: Nightly store-model-nightly failed on 2026-11-23
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
updated: 2026-11-23
next: Read the run, reproduce the seed, then fix it or open the card that owns the failing code.
blocked_by: none
---

# NIGHTLY-2026-11-23-store-model-nightly: Nightly store-model-nightly failed on 2026-11-23

## Goal

The failure is reproduced from its seed and fixed, or handed to the card that owns the code.

## Why it matters to a translator

A nightly long run found a history the push gates did not; it may be a way to lose work.

## Before you start

- The run: https://github.com/birchamp/tc-rst/actions/runs/45226668427
- Lines of its log that name a seed or a failure:

    thread 'model::random_histories_lose_no_work_long' (5830) panicked at crates/tc-store/tests/store/model.rs:2483:5:
    Test failed: held ingredients/TIT.usfm: no observed notice leads to its complete copy .tc/external/20261017T000000Z-ingredients%2FTIT.usfm-2
    notices: [FileUnreadable { path: "ingredients/TIT.usfm", reason: "another program has it open, or it is not on this computer yet" }, OutsideEditHeld { path: RepoPath("ingredients/TIT.usfm"), copy: ".tc/external/20261017T000000Z-ingredients%2FTIT.usfm-2" }, MaterializeBlocked { paths: [], reason: Failed { reason: "could not rename /projects/Titus/.tc/heads/metadata.json.bin: FaultFs: the writer was stopped here" } }]
    minimal failing input: history = [
    test model::random_histories_lose_no_work_long ... FAILED
    test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 421 filtered out; finished in 550.61s
    error: test failed, to rerun pass `-p tc-store --test store`

## Funnel card

- **Invariants**: the failing test's own.
- **Checks**: the failing test, rerun with the seed above.

## Steps

### Step 1: Reproduce, then fix or hand over

- Do: rerun the failing test with the seed; fix the cause or open a card for its owner.
- Record: a log line `step 1`.

## Log

- 2026-11-23 | nightly (store-model-nightly) | created | none | the scheduled store-model-nightly job failed | https://github.com/birchamp/tc-rst/actions/runs/45226668427
