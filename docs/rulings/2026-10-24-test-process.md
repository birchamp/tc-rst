# Ruling: test where the bugs are found, not where the time goes

- Asked: 2026-10-24, by the owner: "Look back over the testing and see where the most real bugs
  are getting caught, see if that gives us any insight on making the build/test process more
  efficient."
- The evidence: a read of 56 cards' logs, `tracker:minors.tsv` and 438 CI runs, 2026-10-22 to
  2026-10-24.
  - **Real bugs:** of 37 real bugs, the review lenses found 23, including 3 of the 4 blockers. The
    loss lens alone found 12, including 2 blockers. Model and proptest seeds found 2, including 1
    blocker. Journeys, a merge, the owner and an author found 1 each. The CI gate found 1. The
    local full gate found none of its own.
  - **Failed gate and main CI runs (29):** 18 were flaky timing or wrong test expectations, 4 were
    tracker checks, 4 were cross-card compile or lint breaks, 2 were infra, and 1 was a possible
    real bug (nightly model run 40085200557).
  - **Branch CI:** track branches ran 85% of the CI runs, about 80 of 148 machine-hours.
- Answer, in the owner's words: "Apply all six process changes."

## The six changes

1. **Review before the long checks.** Run the card's review lenses, the loss lens first for a
   no-loss card, before any long gate run.
2. **Fast checks before every hand-over.** Run `board.py --check`, the claims and coverage checks,
   `cargo check` and the wasm clippy, with main merged in.
3. **Trial-merge once a day.** The integration lane merges every ready card onto main in a
   scratch worktree and runs `cargo check` and the wasm clippy, to find cross-card breaks before
   a batch.
4. **Lighter CI on track branches.** Only compile and lint run there. The full tests and journeys
   run on `gate/*` and main. Superseded track runs are cancelled; `gate/*` runs never are.
5. **Targeted local runs.** While building, run the touched crates and journeys only. The CI
   receipt on the batch is the full gate.
6. **Fix the flaky test, and catch nightly failures.** Fix `align_save_costs_one_verse` (4
   failures). A failed nightly model or long-seed run writes a card stub to a `nightly/<date>`
   branch, which the integration lane picks up.

The no-loss rules and the three lenses for no-loss cards do not change.

## Effect for translators

None directly. The same reviews still protect their work, and features reach them sooner.
