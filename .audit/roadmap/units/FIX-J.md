---
id: FIX-J
title: Finish the partial P1 journeys J3, J12 and J13
track: A
lane: app
milestone: M1
state: claimed
owner: C7a session (Claude Code 3058c665)
branch: track-FIX-J
worktree: .worktrees/track-FIX-J
depends: [INT-4]
needs_owner: []
proves: [P1]
no_loss: false
size: 1-1.5 d (basis: C8 wrote the upgrades and source-texts journeys in one session; each item here is one test over code that exists)
merged: none
updated: 2026-10-23
next: Do step 1 (seed ULT Jonah under the fake's parity account) on track-FIX-J.
blocked_by: none
---

# FIX-J: Finish the partial P1 journeys J3, J12 and J13

## Goal

The three tC4 journeys that `evidence/2026-10-21/done-definition.md` §2 marks partial and
assigns to a "C8 follow-up" (a card that does not exist) have an owner and a full test each: J3
downloads a source text from the seeded fake and the pinned version lands on disk; J12 runs an
upgrade through the screens and the moved pin is read from disk; J13 asserts the primary set on
disk after *Change*. CHK-3's coverage ledger maps the three journeys to their tests, not to a
pending card.

## Why it matters to a translator

A translator's first act in a new project is to download a source text; an upgrade moves the
pins their checks rest on. Today the browser tests stop before the disk, so a regression that
downloads the wrong version or leaves the old pin would pass CI.

## Before you start

- `git merge-base --is-ancestor $(git log -1 --format=%H --grep='^Merge INT-4' HEAD) HEAD` shows
  INT-4 merged (or `sed -n 's/^state: //p' .audit/roadmap/units/INT-4.md` prints `done`).
- `grep -n "publishes no resources" parity/app/journeys/source-texts.test.mjs` prints the comment
  this card removes.
- `grep -n "Check for updates asks\|Keep\|Change writes" parity/app/journeys/upgrades.test.mjs`
  prints the three J12 and J13 tests this card extends.
- `grep -n "parity account" crates/tc-door43-fake/src/lib.rs` shows where the fake's seeded
  account is built (`seeded.rs`).

## Funnel card

- **Invariants**:
  - INV-P1-COVERED: J3, J12 and J13 each map to a journey test that reaches the disk (CHK-3's
    `coverage.test.mjs` holds the map).
  - INV-PINS-ON-DISK: after a download or an upgrade through the screens, the project's pin file
    names the version the screen showed; a test reads the file, not the UI.
- **Wrong states**:
  - A download journey that passes while the fake publishes nothing (the test asserts only the
    empty list).
  - An upgrade test that stops at the dialog and never reads the pin it moved.
- **Code**:
  - Types: none new. The fake's `seeded.rs` gains one resource under the parity account (ULT
    Jonah, the version `fixtures/parity-state/en-pins` pins), built from the same fixture the
    Rust tests use, so the fake and the tests cannot disagree about what exists.
  - One entry point: downloads and upgrades already run through `tc-app`'s source-text job; this
    card adds no second path. The tests call it through the screens.
  - Removed: the comment "the fake's parity account publishes no resources" and the test that
    only asserts the empty list.
  - Left to checks, and why: whether the pin on disk equals the version shown is a property of
    a run, not a type; the three tests carry it.
- **Checks**:
  - `j3_a_download_from_the_seeded_fake_installs_the_pinned_version` in
    `parity/app/journeys/source-texts.test.mjs`; kind: browser journey with a disk read through
    `test/read` (test-hooks); runs in: app-e2e (G5) and the local `node --test` run.
  - `j12_an_upgrade_through_the_screens_moves_the_pin_on_disk` in
    `parity/app/journeys/upgrades.test.mjs`; same kind.
  - `j13_change_writes_the_primary_set_to_disk` in `parity/app/journeys/upgrades.test.mjs`; same
    kind.
  - Proof it can fail: make the fake serve version N-1 under the name of N (one line in
    `seeded.rs`); J3 fails naming the version read from disk. Make the upgrade job skip the pin
    write; J12 fails. Revert both.
- **Prose**: `docs/APP-DESIGN.md` J-table rows for J3, J12 and J13 change from "partial" to the
  test names; nothing else.
- **Human**: none.

## Steps

### Step 1: Seed one resource under the fake's parity account

- Do: in `crates/tc-door43-fake/src/seeded.rs`, publish ULT Jonah at the version
  `fixtures/parity-state/en-pins` pins, under the parity account, built from the fixture the Rust
  `sources.rs` tests already use (one function, called by the binary and by the Rust tests).
  Write `the_seeded_fake_publishes_the_en_pins_versions` in `crates/tc-door43-fake/tests/` that
  lists the account's catalog and compares it with the fixture's pins.
- Files: `crates/tc-door43-fake/src/seeded.rs`, `crates/tc-door43-fake/tests/seeded.rs`.
- Done when: `cargo test -p tc-door43-fake seeded` passes, and fails when the resource is
  removed from `seeded.rs`.
- Record: log line `step 1`; set `state: building`; update `updated` and `next`.

### Step 2: J3 end to end

- Do: in `parity/app/journeys/source-texts.test.mjs`, replace the test that asserts the empty
  list with `j3_a_download_from_the_seeded_fake_installs_the_pinned_version`: open the
  `en-pins` scenario with the fake running, go to Source texts, download ULT Jonah, wait for the
  job to finish on the screen, then read the project's pin file through `test/read` and assert
  the version equals the one the screen showed. Delete the comment "the fake's parity account
  publishes no resources".
- Files: `parity/app/journeys/source-texts.test.mjs`.
- Done when: `node --test parity/app/journeys/source-texts.test.mjs` passes, and fails with the
  step 1 mutation (the fake serving N-1 under N's name).
- Record: log line `step 2`.

### Step 3: J12 and J13 read the disk

- Do: in `parity/app/journeys/upgrades.test.mjs`, extend `Check for updates asks…` into
  `j12_an_upgrade_through_the_screens_moves_the_pin_on_disk` (accept the upgrade, then read the
  pin file and assert the new version) and extend `Change writes…` into
  `j13_change_writes_the_primary_set_to_disk` (read the primary-set file after *Change*). Keep
  the `Keep…` test as it is.
- Files: `parity/app/journeys/upgrades.test.mjs`.
- Done when: `node --test parity/app/journeys/upgrades.test.mjs` passes, and J12 fails when the
  upgrade job's pin write is skipped (one line, reverted).
- Record: log line `step 3`.

### Step 4: Update the coverage map and the J-table

- Do: if CHK-3 has merged, change `parity/app/journeys/coverage.json` so J3, J12 and J13 map to
  the three tests with no `pending`; if not, add a `note` to CHK-3's log that FIX-J landed and
  its step 3 maps them to these tests. Change the J3, J12 and J13 rows of `docs/APP-DESIGN.md`'s
  journey table to the test names.
- Files: `parity/app/journeys/coverage.json` (if it exists), `docs/APP-DESIGN.md`.
- Done when: `grep -n "J3\|J12\|J13" docs/APP-DESIGN.md` shows the test names and no "partial";
  `node --test parity/app/journeys/coverage.test.mjs` passes when CHK-3 has merged.
- Record: log line `step 4`; set `state: verifying`; update `next`.

## Gates before merge

The full gate list (`scripts/gates.sh` once INT-3 lands; until then every command in `AGENTS.md`
"Tests" plus the Windows clippy step from `.github/workflows/ci.yml`), plus:

- The three journeys pass under `node --test` with the fake running, and each named mutation
  fails its test (the log quotes the failure line).
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The upgrade job has no pin write a test can skip (the pin is written somewhere else): write
  it in the log and ask the app lane's owner where the pin lands before you write J12.
- CHK-3's `coverage.json` shape differs from what step 4 expects: follow CHK-3's card and log the
  difference; do not invent a second map.

## References

- `evidence/2026-10-21/done-definition.md` §2 (rows J3, J12, J13).
- `units/CHK-3.md` step 3 (the coverage ledger names FIX-J as the pending card until this merges).
- `parity/app/journeys/source-texts.test.mjs`, `parity/app/journeys/upgrades.test.mjs`,
  `crates/tc-door43-fake/src/seeded.rs`, `crates/tc-app/tests/app/sources.rs`,
  `crates/tc-app/tests/app/upgrades.rs`.

## Log

- 2026-10-22 | roadmap session (Cursor, review of 2026-10-22) | created | none | the review (goal-coverage R3) found J3, J12 and J13 assigned to a card that did not exist | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-23 22:40 EDT | C7a session (Claude Code 3058c665) | claim | none | claimed on track-FIX-J from main eaf809d1 (INT-4 done); the preconditions print as the card says. Decision: there is no `test/read` endpoint; the journeys read the pin files from the run's projects folder (`env.projectsDir`, the store scenario's folder on disk), which needs no new test hook | .worktrees/track-FIX-J
