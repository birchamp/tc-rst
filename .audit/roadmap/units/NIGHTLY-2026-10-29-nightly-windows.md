---
id: NIGHTLY-2026-10-29-nightly-windows
title: Nightly nightly-windows failed on 2026-10-29
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
updated: 2026-10-29
next: Read the run, reproduce the seed, then fix it or open the card that owns the failing code.
blocked_by: none
---

# NIGHTLY-2026-10-29-nightly-windows: Nightly nightly-windows failed on 2026-10-29

## Goal

The failure is reproduced from its seed and fixed, or handed to the card that owns the code.

## Why it matters to a translator

A nightly long run found a history the push gates did not; it may be a way to lose work.

## Before you start

- The run: https://github.com/birchamp/tc-rst/actions/runs/40871810177
- Lines of its log that name a seed or a failure:

            PASS [  31.187s] ( 287/3100) tc-align::tc4_suggest boosted_predictions_equal_tc4s_under_the_same_seed
            PASS [   0.042s] ( 549/3100) tc-corpus rng::tests::one_seed_gives_one_stream
            PASS [   2.549s] ( 552/3100) tc-corpus::corpus one_seed_gives_one_corpus
            PASS [   4.593s] ( 680/3100) tc-door43::git a_refused_token_is_auth_failed_and_offered_once
        test a_git_fetch_goes_through_the_proxy_the_finder_names ... FAILED
        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 6 filtered out; finished in 0.06s
        thread 'a_git_fetch_goes_through_the_proxy_the_finder_names' (7924) panicked at crates\tc-door43\tests\proxy.rs:119:5:
        AuthFailed { route: "POST /unfoldingWord/en_tn.git/git-upload-pack", status: Forbidden, message: "request failed with status code: 403" }
        test a_403_on_push_is_an_http_error_and_makes_no_branch ... FAILED
        test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 4 filtered out; finished in 4.42s
        thread 'a_403_on_push_is_an_http_error_and_makes_no_branch' (9044) panicked at crates\tc-door43\tests\push.rs:339:5:
        assertion `left == right` failed

## Funnel card

- **Invariants**: the failing test's own.
- **Checks**: the failing test, rerun with the seed above.

## Steps

### Step 1: Reproduce, then fix or hand over

- Do: rerun the failing test with the seed; fix the cause or open a card for its owner.
- Record: a log line `step 1`.

## Log

- 2026-10-29 | nightly (nightly-windows) | created | none | the scheduled nightly-windows job failed | https://github.com/birchamp/tc-rst/actions/runs/40871810177
