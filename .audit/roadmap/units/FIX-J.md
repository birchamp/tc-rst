---
id: FIX-J
title: Finish the partial P1 journeys J3, J12 and J13
track: A
lane: app
milestone: M1
state: done
owner: C7a session (Claude Code 3058c665)
branch: track-FIX-J
worktree: .worktrees/track-FIX-J
depends: [INT-4]
needs_owner: []
proves: [P1]
no_loss: false
size: 1-1.5 d (basis: C8 wrote the upgrades and source-texts journeys in one session; each item here is one test over code that exists)
merged: f1b865b9
updated: 2026-10-24
next: none
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
- 2026-10-23 23:50 EDT | C7a session (Claude Code 3058c665) | step 1 | none | `tc_door43_fake::seeded::unfolding_word_resources` holds unfoldingWord's releases (en_tn v86, v91, v92 pre-release; en_ult v89; en_twl v86), called by `seed()` (start_seeded) and by the fake binary, so the browser tests and the Rust tests share one set of releases. Decision: not ULT Jonah at en-pins' version, as the card guessed: the scenarios pin Door43's own commits (en_twl v91 never existed in the fake), and a fake can only serve its own commits. `the_seeded_fake_publishes_the_en_pins_versions` (crates/tc-door43-fake/tests/binary.rs) checks that the binary and start_seeded publish the same unfoldingWord releases; without the binary's call it failed `the binary publishes unfoldingWord's resources` | cargo test -p tc-door43-fake --test binary -- the_seeded_fake
- 2026-10-23 23:50 EDT | C7a session (Claude Code 3058c665) | step 2 | none | J3 `j3_a_download_from_the_seeded_fake_installs_the_pinned_version`: the sample pins Door43's commits, which the fake cannot serve, and the download rightly refuses another commit under a pinned version (probe: `the export declares revision c7277…, but e5867… was expected`); so the test first moves the text's pin to the fake's release through the upgrade screen, removes the install from the run's app data, downloads the English package, and reads the receipt from disk: the pinned commit and version. The empty-list test became the internet-question test it already was (D95, Not now sends nothing, then one catalog read); the comment "publishes no resources" is gone. Proof: with the download wanting no pin and taking the text as present, J3 failed `timed out after 60000 ms waiting for the source text is installed`. (A first break, `present = true` alone, did not fail it: a wanted pin installs regardless.) | node --test parity/app/journeys/source-texts.test.mjs
- 2026-10-23 23:50 EDT | C7a session (Claude Code 3058c665) | step 3 | none | J12 `j12_an_upgrade_through_the_screens_moves_the_pin_on_disk` (the text upgrade of en_ult through the screens, the pin file's extraScripture entry then names the fake's commit and tag) and J13 `j13_change_writes_the_primary_set_to_disk` (the pin file's primary set is English after Change); the Keep test is unchanged. Proof: with ApplyPlan answering Applied without submitting, J12 failed `timed out after 30000 ms waiting for the pin moved on disk`. Finding: the offer reads "v91 → v89" because tC4's rule offers any release whose commit differs (D58); logged as a minor | node --test parity/app/journeys/upgrades.test.mjs
- 2026-10-23 23:50 EDT | C7a session (Claude Code 3058c665) | step 4 | none | CHK-3 has not merged: a note on its log names the three tests; APP-DESIGN's J3 and J12/J13 rows name them (the table had no "partial" left) | docs/APP-DESIGN.md
- 2026-10-24 01:45 EDT | C7a session (Claude Code 3058c665) | test | none | the fake-using journeys ran 40 of 41; the one failure (guided-fix's file of another commit) was this worktree lacking `parity/tc4-baseline/node_modules` for fflate, not FIX-J. With it linked from the main checkout (git-ignored), guided-fix passes 3 of 3; the fake's tests, fmt and clippy pass | .enjoy-logs/fixj-guided.log
- 2026-10-24 02:30 EDT | C7a session (Claude Code 3058c665) | verify | none | browser-and-parity lens (subagent review of afdd5718). Fixed: J3 read the error line as soon as en_ult's receipt appeared, before the package's notes and word links finished; it now waits for `sources-done` or `sources-error` and states the error text. Fixed: the dropped "no package for this book" check is back as its own journey (Español on the fake publishes nothing). Kept as a limit, noted in J3: the fake has one en_ult release, so J3 cannot tell the pinned version from the newest (a fake route that adds a release would). Minor, logged before: J3 and J12 upgrade en_ult to a lower label (D58). Other journeys see nothing new from the added releases. source-texts 4 of 4, guided-fix 3 of 3 | .enjoy-logs/fixj-sources.log
- 2026-10-24 18:35 EDT | integration lane (Claude Code 3d07d5f8) | merge | f1b865b9 | merged into main in batch 2b (INT-6 a4dc42af, FIX-J f1b865b9, G1 137164e8, LIC-1 819efb32, CHK-2 a9c3710f, CHK-1 64ad5158, I 22f77f0d, RULING-H5 8bd03b4b), gated by scripts/gates.sh --wait-ci batch-2b on the batch's last commit. | scripts/gates.sh --wait-ci batch-2b
