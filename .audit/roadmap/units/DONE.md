---
id: DONE
title: Release candidate: P1 to P10 green, the owner's reviews recorded
track: A
lane: integration
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: [ALIGN-LEAVE, AQ1, BURRITO-HOLD, C7a, C7b, CHK-1, CHK-1a, CHK-2, CHK-3, CHK-3a, CHK-4, CHK-4b, CHK-5, CHK-6, CHK-6a, CHK-7, CHK-7b, CHK-7c, CHK-8, D1, D2, DOM-1, DOM-2, DOM-3, E1a, E1b, E1c, E1d, E2, F1, FEEDBACK-PATHS, FIX-791, FIX-HIDE, FIX-HIDE2, FIX-J, FIX-KEPT-EVENT, FIX-R, FIX-RESET-RECENCY, G1, G2, G3, G4, G5, HIST-1, HOME-SETTLE, I, INT-1, INT-2, INT-3, INT-4, INT-5, INT-6, INT-7, INT-8, LIC-1, LINK-DROP, LINK-MARK, LOSS-1, LOSS-2, NIGHTLY-2026-10-28-store-model-nightly, OLH-1, P, PERF-LAPTOP, PKG-HERE, POEM-TAB, PUSH-1, REF1, REF1b, REF1c, REL-1, S2, S3, SAVE-WAIT, SRC-VER, SWEEP, T, TC3-LANG, TOAST-ORDER, UI-1, UI-2, UI-3, UI-3a, UI-3b, USB-VERIFY, V, VRS-GUESS, X]
needs_owner: []
proves: [P1, P2, P3, P4, P5, P6, P7, P8, P9, P10]
no_loss: false
size: 1-2 d
merged: none
updated: 2026-10-24
next: Claim this card when its dependencies merge.
blocked_by: none
---

# DONE: Release candidate: P1 to P10 green, the owner's reviews recorded

## Goal

Every statement P1 to P10 of `docs/PLAN.md` is shown by a CI check on main that is green and that
was shown to fail when its rule was broken. The owner's reviews are recorded as ruling files. A
release built by G3 installs and passes its smoke test on each platform the owner chose. The
documents describe what shipped, and a final evaluation is filed.

## Why it matters to a translator

This is the point where the app can be handed to translators: every promise PLAN makes about
their work (journeys, team work, no loss, crash safety, valid projects, helps, look, scripts,
carry-over, old projects) is held by a check, and the installer has been tried on each system.

## Before you start

- `python3 -I .audit/tools/board.py` shows every card except DONE as `done`, and the last line
  of `python3 -I .audit/tools/board.py --check` reads `<n> cards, 0 errors, <m> warnings`.
- CHK-3's P-item ledger exists and its test passes: `ls docs/P-LEDGER.md` lists the file, and
  `node --test .audit/tools/claims.test.mjs` passes, including
  `INV-CLAIMED-TESTS-EXIST: P-LEDGER` (`units/CHK-3.md` step 4).
- `ls .audit/roadmap/rulings/` lists the rulings this card needs (step 4).
- `gh run list --branch claude/focused-ride-5uzj7p --limit 20` shows the latest runs on main.
- `git -C <tC4 clone> tag -l 'v4.0.0'` shows whether tC4 4.0 has its final tag (on 2026-10-21 the
  newest was `v4.0.0-rc.2`).
- PLAN's size estimate is at `docs/PLAN.md:312-314` (50,000 to 70,000 lines, "an estimate, not
  a measurement"); the 2026-10-21 assessment counted about 143,000 code lines in all crates
  (`evidence/2026-10-21/health-app.md` CH-1).

## Funnel card

- **Invariants**:
  - INV-EVERY-P-ITEM-HAS-A-FAILING-CAPABLE-CHECK: each of P1 to P10 is named in CHK-3's ledger by
    at least one CI job and test that is green on main's tip and that was shown to fail when its
    rule was broken, with the failing output recorded.
- **Wrong states**:
  - A P item shown only by a local run, a document or a skipped test (P1 before G5).
  - A check that cannot fail (it asserts nothing, or always skips).
  - An owner review that was given in chat but not recorded.
  - A release built from another commit than the one the checks passed on.
- **Code**:
  - Types: none new. CHK-3's ledger is the one place that maps a P item to its checks.
  - One entry point: CHK-3's ledger and its test; this card adds no second list.
  - Removed: nothing. `.audit/STATUS.md` and `.audit/PROGRESS.md` stay as history.
  - Left to checks, and why a type cannot carry it: that a check can fail is shown only by
    breaking its rule once and watching it fail; a seeded failure per P item records that.
- **Checks**:
  - `INV-CLAIMED-TESTS-EXIST: P-LEDGER` in `.audit/tools/claims.test.mjs` (created by CHK-3);
    kind: ledger; runs in: the CI job that runs the gate step `node`.
  - `python3 -I .audit/tools/board.py --check`; kind: consistency; runs in: the CI step INT-5 adds
    (gate step `board`).
  - `cargo test --release -p tc-api --test refusals`; kind: contract; runs in: release.yml
    `translations` (G3).
  - release.yml build and smoke jobs on each H3 platform (G3); kind: journey.
  - Proof it can fail: the table of seeded failures, one per P item (step 3).
- **Prose**: `docs/RELEASE.md`, `docs/PLAN.md` and `docs/ARCHITECTURE.md` describe what shipped;
  `docs/evidence/done-<date>.md`.
- **Human**: the rulings of step 4 and the owner's yes before a release tag is pushed.

## Steps

### Step 1: Confirm every dependency is merged and main is green

- Do: run the board and its check on main. Read the latest run of every workflow on main.
- Files: none.
- Done when: the last line of `python3 -I .audit/tools/board.py --check` says `0 errors`; every card but DONE is
  `done`, or `parked` with `next: not doing: <reason>` and the owner's decision in its log
  (`TRACKING.md` "States"); `gh run list --branch claude/focused-ride-5uzj7p --limit 20 --json workflowName,conclusion,headSha`
  shows `success` for `ci` (its push jobs, and its scheduled jobs from CHK-5 and P on their last
  run), `app-e2e` and `shell` on main's tip
  (`git rev-parse HEAD`).
- Record: a log line `step 1` with the run ids. Update `updated` and `next` (`TRACKING.md`).

### Step 2: Fill the P table from the ledger

- Do: for each of P1 to P10, read CHK-3's ledger `docs/P-LEDGER.md`: the CI job, the test names, and the last green
  run on main's tip. Write the table into `docs/evidence/done-<date>.md` (to create): P item, job,
  tests, run id.
- Files: `docs/evidence/done-<date>.md`.
- Done when: the table has ten rows, each with at least one test and a green run id on main's tip,
  plus one extra row `security` naming G1's `window_preferences_isolate_the_renderer` and
  `navigation_outside_the_app_origin_is_refused` and CHK-1's `every_route_is_behind_guard`
  (INV-RENDERER-IS-ISOLATED, INV-LOCAL-SERVER-IS-CLOSED), with their green run;
  `grep -c '(pending:' docs/P-LEDGER.md` prints `0`; `node --test .audit/tools/claims.test.mjs`
  passes.
- Record: a log line `step 2`. Update `updated` and `next` (`TRACKING.md`).

### Step 3: Show each P item's check can fail

- Do: for each P item, find the "Proof it can fail" record in the log of the card that built its
  check (for example G5 for P1, CHK-2 for P5, V for P10). Where a P item has none, make one in
  your own worktree: break the rule (a mutation or a revert of the fix), run the check with the
  command its CI job uses, copy the failing output, then restore the files with
  `git checkout -- <the files you changed>` and confirm `git status` shows them clean. Never
  commit or push the mutation.
- Files: `docs/evidence/done-<date>.md` (a column "seeded failure": the card log line or the
  output).
- Done when: all ten rows have a seeded failure, and each failure message names the invariant or
  the test.
- Record: a log line `step 3`. Update `updated` and `next` (`TRACKING.md`).

### Step 4: Check the owner's reviews are recorded

- Do: confirm these ruling files exist in `.audit/roadmap/rulings/`: the P7 residual review
  (`<date>-p7-review.md`, G2 step 6); H9, format 1.0 frozen (V); H10, the translations received
  and merged by G3's "Receive the translations" step (or the owner's allowlist ruling,
  `ROADMAP.md` H10), and `cargo test --release -p tc-api --test refusals` passing on main; H3,
  the platforms; H4, the signing; H17, whether every speed budget is part of done (`ROADMAP.md` lists it against DONE).
  For each one missing, ask the owner in short plain sentences, with the options and a
  recommendation from `ROADMAP.md` "What needs the owner".
- Files: `.audit/roadmap/rulings/` (only the agent that receives an answer writes the file).
- Done when: `ls .audit/roadmap/rulings/` lists all six and
  `cargo test --release -p tc-api --test refusals` passes.
- Record: a log line `ruling` per answer received, then `step 4`. Update `updated` and `next` (`TRACKING.md`).

### Step 5: Move the reference if tC4 4.0 is final, and sweep once more

- Do: if `v4.0.0` exists in the tC4 clone and `docs/PLAN.md` "The reference version" still names
  another commit, add a card for the move (REF1's steps again) and wait for it to merge. Then run
  SWEEP's procedure (`units/SWEEP.md`) once more against tC4's newest `main` and `release/4.0`.
- Files: a new card if needed; SWEEP's outputs.
- Done when: PLAN's reference is tC4's final 4.0 tag, or no such tag exists; and the sweep's table
  has no row marked "Same bug" without a passing red-first test.
- Record: a log line `step 5` with the sweep's evidence path. Update `updated` and `next` (`TRACKING.md`).

### Step 6: Build the release candidate and smoke-test it

- Do: on main's tip, run `release.yml` by hand: `gh workflow run release.yml --ref claude/focused-ride-5uzj7p`.
  Ask the owner before pushing a release tag; push it only on a clear yes.
- Files: none.
- Done when: every build and smoke job of each H3 platform and `translations` are green, and
  `BUILD-MANIFEST.json`'s commit equals main's tip; G2's evidence file holds the five
  `first_launch_under_60_s` runs against this release candidate's own Windows installer (its run
  id), not an earlier build.
- Record: a log line `step 6` with the run id. Update `updated` and `next` (`TRACKING.md`).

### Step 7: Make the documents describe what shipped

- Do: update `docs/RELEASE.md`, `docs/PLAN.md` and `docs/ARCHITECTURE.md` to what shipped.
  Re-baseline PLAN's size estimate (`docs/PLAN.md:312-314`): run
  `find crates/*/src -name '*.rs' -print0 | xargs -0 grep -hvE '^\s*(//|$)' | wc -l` (non-blank,
  non-comment lines) and write the count, that command and the date in place of the estimate.
- Files: `docs/RELEASE.md`, `docs/PLAN.md`, `docs/ARCHITECTURE.md`.
- Done when: CHK-3's ledger test still passes, and each document names the release's version.
- Record: a log line `step 7`. Update `updated` and `next` (`TRACKING.md`).

### Step 8: Rerun the evaluation, file the report, merge

- Do: rerun the eleven evaluations of `.audit/roadmap/evidence/2026-10-21/` into
  `.audit/roadmap/evidence/<date>/`. Finish `docs/evidence/done-<date>.md`: the P table, the
  rulings, the release run, the open minors count (`wc -l .audit/minors.tsv`), and anything the
  evaluation still finds. A blocker or a major that harms a translator's data or view stops the
  merge: open a card for it.
- Files: `.audit/roadmap/evidence/<date>/`, `docs/evidence/done-<date>.md`, this card.
- Done when: the gates below pass and the board shows every card `done` after the merge.
- Record: a log line `step 8`, then the merge as `TRACKING.md` says. Update `updated` and `next` (`TRACKING.md`).

## Gates before merge

The full gate list (`scripts/gates.sh`, created by INT-3; it holds every command of `AGENTS.md`
"Tests" and the Windows clippy step of `.github/workflows/ci.yml`), plus:

- `python3 -I .audit/tools/board.py --check` prints `0 errors`.
- `cargo test --release -p tc-api --test refusals` passes.
- The release run of step 6 is green on each H3 platform.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A ruling of step 4 is missing: ask the owner; do not decide it.
- A P item has no check that can fail: open a card for the check; DONE waits for it.
- Pushing a tag or publishing a release: ask the owner first.
- The final evaluation finds a blocker.

## References

- `evidence/2026-10-21/done-definition.md` (all of it); `ROADMAP.md` "The goal", M5
- `docs/PLAN.md` P1 to P10, "Scope", "The reference version"
- `evidence/2026-10-21/health-app.md` CH-1; `tc4-bugs.md` §4
- `units/CHK-3.md`, `units/G2.md`, `units/G3.md`, `units/V.md`, `units/SWEEP.md`, `units/REF1.md`

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/done-definition.md
- 2026-10-23 17:47 EDT | integration lane (Claude Code 3d07d5f8) | note | none | depends gains LIC-1 and OLH-1, the two cards added by the owner's rulings of 2026-10-23 | rulings/2026-10-23-tokenizer-relicense.md, rulings/2026-10-23-Q39-original-language-highlight.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
- 2026-10-24 | integration lane (Claude Code 3d07d5f8) | note | none | depends gains CHK-7 | .audit/roadmap/units/CHK-7.md
- 2026-10-26 23:31 EDT | storage lane (Claude Code LOSS-2) | note | none | depends gains LOSS-2 (two store losses the model test excused), on the orchestrator's word: "REL-1 waits for it" | message from the orchestrator, 2026-10-26
