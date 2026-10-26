---
id: NIGHTLY-2026-10-26-nightly-windows
title: Nightly nightly-windows failed on 2026-10-26
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
updated: 2026-10-26
next: Read the run, reproduce the seed, then fix it or open the card that owns the failing code.
blocked_by: none
---

# NIGHTLY-2026-10-26-nightly-windows: Nightly nightly-windows failed on 2026-10-26

## Goal

The failure is reproduced from its seed and fixed, or handed to the card that owns the code.

## Why it matters to a translator

A nightly long run found a history the push gates did not; it may be a way to lose work.

## Before you start

- The run: https://github.com/birchamp/tc-rst/actions/runs/40320912191
- Lines of its log that name a seed or a failure:

    test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
    test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.79s
    test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 45.96s
    test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
    test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
    test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.36s
    test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
    test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
    test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
    test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
    test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
    test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

## Funnel card

- **Invariants**: the failing test's own.
- **Checks**: the failing test, rerun with the seed above.

## Steps

### Step 1: Reproduce, then fix or hand over

- Do: rerun the failing test with the seed; fix the cause or open a card for its owner.
- Record: a log line `step 1`.

## Log

- 2026-10-26 | nightly (nightly-windows) | created | none | the scheduled nightly-windows job failed | https://github.com/birchamp/tc-rst/actions/runs/40320912191
