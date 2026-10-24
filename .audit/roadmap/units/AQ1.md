---
id: AQ1
title: Aquifer helps in the app (P6)
track: A
lane: app
milestone: M3
state: parked
owner: none
branch: none
worktree: none
depends: [INT-4, UI-1]
needs_owner: []
later: ["H6 from step 3"]
proves: [P6]
no_loss: false
size: 3-5 sessions (estimate; basis B9a for the download job and B3 for helps panes)
merged: none
updated: 2026-10-24
next: Parked until after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md).
blocked_by: none
---

# AQ1: Aquifer helps in the app (P6)

## Goal

PLAN P6 is true and shown: the same helps screens (Notes, Questions, Words and the Academy
article) work with Door43 resources and with Aquifer resources, pinned and offline, on recorded
real data. A translator can install an Aquifer package from Source texts. Aquifer's Portuguese
Bible, whose word links were made by AI, shows as a reading pane with a clear label, and its
links are never used (owner answer Q34 (b)). A project that carries a tC4 Aquifer pin opens
and keeps it (Q33).

## Why it matters to a translator

Translators who read Portuguese, Hindi, Indonesian, Spanish, French or Arabic get notes and
questions in their own language from Aquifer, offline, in screens they already know. An
AI-made text is never mistaken for a reviewed one.

## Before you start

- `git merge-base --is-ancestor f9067bc HEAD && echo landed` prints `landed` (INT-4 is in).
- `sed -n 1505,1512p docs/APP-DESIGN.md` says nothing in the app installs an Aquifer asset.
- `ls crates/tc-helps/tests/aquifer.rs crates/tc-resources/tests/aquifer_install.rs
  crates/tc-resources/tests/aquifer_proxy.rs crates/tc-resources/tests/common/aquifer_fake.rs`
  lists four files: Aquifer is tested only in the libraries.
- `ls fixtures/parity-state` prints `README.md common en-pins shared`: no scenario has Aquifer
  pins. `fixtures/aquifer/README.md` records the BibleAquifer data at tag `v2026-10-12` (Titus
  notes and questions, five Words articles and eight Academy articles, in eng and por).
- `grep -n 'pub async fn download_asset' crates/tc-resources/src/aquifer.rs` prints about 343:
  it returns the whole asset as `Vec<u8>`, with no progress and no cancel.
- `grep -n 'Download,' crates/tc-api/src/jobs.rs` prints the `Download` job kind (about 38) that
  B9a built for Door43 packages.
- `sed -n 518,547p docs/TC4-DELTA-2026-10-19.md` shows the answers to Q33 and Q34.
- `python3 -I .audit/tools/board.py | grep -E '^\| UI-1 '`: if its State column says `done`,
  step 3 follows the install job with UI-1's follower; if not, follow it the way the Door43
  package install in `crates/tc-app/src/resources.rs` does, and say so in the log.

## Funnel card

- **Invariants**:
  - INV-ONE-HELPS-MODEL-FOR-BOTH-SOURCES: every helps pane reads the one tc-helps model; the
    only thing that differs between a Door43 and an Aquifer source is the source's name shown.
  - INV-AI-MADE-TEXT-IS-ALWAYS-LABELLED: the AI-linked Portuguese Bible is drawn only with its
    label, and its word links never reach alignment, suggestions or checks.
  - INV-OFFLINE-AFTER-INSTALL: once installed, Aquifer helps open with the internet switch off
    and make no network request.
  - INV-A-TC4-AQUIFER-PIN-IS-KEPT: the app never writes an Aquifer pin into a project's
    language sets, and a project that carries one keeps it byte for byte (Q33 (a)).
- **Wrong states**:
  - An Aquifer package is offered, and its 16.2 MB notes asset is read whole into memory with
    no progress and no cancel (APP-DESIGN about 1505).
  - Aquifer helps appear in another screen, or with other controls, than Door43 helps.
  - The Portuguese pane shows without its label, or its links feed suggestions.
  - Aquifer helps need the internet after install.
  - The app writes an Aquifer pin that tC4 4.1 would read differently, or drops a tC4 one.
- **Code**:
  - Types: a source label on each helps slot (Door43 or Aquifer, for the name only); a type for
    the AI-linked text that has no conversion into what alignment, suggestions or checks take.
  - One entry point: the one `Download` job kind for both sources, streaming (A11's pattern);
    tc-resources' load for both sources into the tc-helps model.
  - Removed: `AquiferClient::download_asset`'s whole-in-memory read; the test-only copy of the
    fake in `crates/tc-resources/tests/common/aquifer_fake.rs`.
  - Left to checks, and why: "no request after install" is about runtime behavior, not types;
    a fake server that fails the test on any request checks it.
- **Checks**:
  - `an_aquifer_install_streams_with_progress_and_cancel` (to create) in
    `crates/tc-app/tests/app/sources.rs`; unit through `App::request`; CI.
  - `aquifer_helps_open_with_the_internet_switch_off` (to create), same file; CI.
  - `the_ai_linked_bible_is_labelled_and_never_feeds_alignment` (to create) in
    `crates/tc-app/tests/app/sources.rs`; unit; CI.
  - `a_project_with_a_tc4_aquifer_pin_opens_and_keeps_it` (to create) in
    `crates/tc-app/tests/app/sources.rs`; unit; CI.
  - `the_same_helps_journey_on_dcs_and_on_aquifer_offline` (to create) in
    `parity/app/journeys/aquifer.test.mjs` (to create); journey; local until G5 runs journeys
    in CI.
  - `every_scenario_is_assembled_from_committed_bytes` (`crates/tc-app/tests/app/scenario.rs`,
    exists) covers the new scenario too; CI.
  - Proof it can fail: for each, stash the fix (`git stash push -- <file>`), run the test,
    paste the failure into the log, `git stash pop`.
- **Prose**: `docs/APP-DESIGN.md` about 1505-1512: replace "nothing in the app installs an
  Aquifer asset" with what this item built.
- **Human**: H6 (`ROADMAP.md`): how Aquifer helps appear, and the AI-made Portuguese pane.
  Recommendation: approve the packet step 1 sends. Steps 3 to 6 wait for its ruling file;
  step 2 (the fake crate) does not need it.

## Steps

### Step 1: Prepare the design packet and send it to the owner (H6)

- Do:
  1. Find the Aquifer repository that holds the AI-linked Portuguese Bible:
     `gh repo list BibleAquifer --limit 200 --json name,description`. Record its name and
     newest release tag. If none fits, say so in the packet.
  2. Make three mock-ups, each in English and in Arabic: (a) Source texts listing Aquifer
     packages beside Door43 ones; (b) the Notes, Questions, Words and Academy panes filled from
     `fixtures/aquifer` with the source named; (c) the Portuguese reading pane with its label,
     for example "Word links made by AI and not reviewed". Make (b) as screenshots of the app
     with routed replies (the pattern of `parity/app/journeys/understand.test.mjs` about
     282-290). Make (a) and (c) as view-gallery cases (`parity/view-gallery/README.md`).
     Save the PNGs under `docs/evidence/aquifer-design-<date>/` (to create).
  3. Write `docs/evidence/aquifer-design-<date>.md` (to create): the pictures, then one plain
     question per choice (where Aquifer packages are offered; how the label reads), each with
     two or three options, what each means for a translator, and a recommendation.
  4. Send the packet's path and the questions to the owner in short plain sentences.
- Files: `docs/evidence/aquifer-design-<date>.md` and its folder.
- Done when: `ls docs/evidence/aquifer-design-<date>/*.png | wc -l` prints 6 or more; every
  question in the packet has options and a recommendation; the owner was sent it.
- Record: log line `step 1`; set `needs_owner: [H6]` and
  `next: Do step 2; it needs no ruling. Step 3 waits for rulings/<date>-H6.md.`

### Step 2: Promote the fake Aquifer server into its own crate

- Do:
  1. Create `crates/tc-aquifer-fake` (to create) with a library and a binary, laid out as
     `crates/tc-door43-fake` is (`src/lib.rs`, `src/bin/tc-door43-fake.rs`). Move
     `FakeGithub` and its types from `crates/tc-resources/tests/common/aquifer_fake.rs`, and
     the zip builders `aquifer_zip`, `aquifer_zip_with` and `aquifer_commit` from
     `crates/tc-resources/tests/common/mod.rs`. The binary serves only what `fixtures/aquifer`
     holds, on loopback.
  2. Point `crates/tc-resources/tests/aquifer_install.rs` and `aquifer_proxy.rs` at the crate
     (a dev-dependency). Delete the old test copy.
  3. The workspace takes `crates/*`, so the root `Cargo.toml` needs no new member.
- Files: `crates/tc-aquifer-fake/` (to create), `crates/tc-resources/Cargo.toml`,
  `crates/tc-resources/tests/common/{mod.rs,aquifer_fake.rs}`,
  `crates/tc-resources/tests/aquifer_install.rs`, `crates/tc-resources/tests/aquifer_proxy.rs`.
- Done when: `cargo build -p tc-aquifer-fake` succeeds;
  `cargo test -p tc-resources --test aquifer_install --test aquifer_proxy` passes;
  `ls crates/tc-resources/tests/common/aquifer_fake.rs` fails.
- Record: log line `step 2`. If `ls .audit/roadmap/rulings/ | grep H6` prints nothing, add a
  log line `block`, set `state: blocked`,
  `blocked_by: waiting for the owner's H6 ruling on docs/evidence/aquifer-design-<date>.md` and
  `next: When the H6 ruling file exists, remove H6, set state building, and do step 3.`
  Otherwise remove H6 from `needs_owner` and set `next` to step 3.

### Step 3: Install Aquifer packages with a streaming Download job

- Do:
  1. Red first: `an_aquifer_install_streams_with_progress_and_cancel` in
     `crates/tc-app/tests/app/sources.rs`, against `tc-aquifer-fake`: progress events arrive;
     a cancel leaves no install and no staging folder.
  2. Make `AquiferClient` stream an asset to the store's staging folder with byte progress
     and cancel, as A11 did for Door43 (`docs/evidence/streaming-downloads-2026-10-17.md`).
     Replace `download_asset` (`crates/tc-resources/src/aquifer.rs` about 343), which reads
     the whole asset into a `Vec<u8>`.
  3. Offer the Aquifer packages the ruling names in Source texts through the existing
     `Download` job kind (`crates/tc-api/src/jobs.rs` about 38), followed as "Before you start"
     says. Install them as app resources. Never write an Aquifer pin into a
     project's `resources.json`.
- Files: `crates/tc-resources/src/aquifer.rs`, `crates/tc-app/src/resources.rs`,
  `crates/tc-ui/src/views/modals/source_texts.rs`, `crates/tc-app/tests/app/sources.rs`.
- Done when: `cargo test -p tc-app --test app -- aquifer_install` passes and fails with the fix
  stashed; `cargo test -p tc-resources --test aquifer_install` still passes.
- Record: log line `step 3`; `next` to step 4.

### Step 4: Draw the helps panes from Aquifer through the one model

- Do:
  1. Red first: `aquifer_helps_open_with_the_internet_switch_off`: install the Titus notes and
     questions from the fake, turn the switch off, open Titus 1:1, and assert the helps reply
     holds Aquifer's notes and questions in the same slot shapes as Door43's, and the fake
     saw no request after the install.
  2. Load Aquifer installs through tc-resources into the tc-helps model (tc-helps already
     converts Aquifer JSON: `crates/tc-helps/src/aquifer.rs`). Add the source's name to the slot.
  3. Add no second pane and no source branch in `crates/tc-ui/src/views/helps_panel.rs`
     beyond showing the source's name.
  4. Build the Portuguese pane as the ruling says, with the AI type of the funnel card. Write
     `the_ai_linked_bible_is_labelled_and_never_feeds_alignment`.
- Files: `crates/tc-resources/src/load.rs`, `crates/tc-app/src/understand.rs`,
  `crates/tc-api/src/helps.rs`, `crates/tc-ui/src/views/helps_panel.rs`,
  `crates/tc-app/tests/app/sources.rs`.
- Done when: `cargo test -p tc-app --test app -- aquifer_helps ai_linked` passes and each new
  test fails with its fix stashed.
- Record: log line `step 4`; `next` to step 5.

### Step 5: Run the same helps journey on Door43 and on Aquifer, offline

- Do:
  1. Add `--aquifer <url>` (to create) to `tc4 serve` beside `--door43`
     (`crates/tc4/src/args.rs` about 134); it builds the client with
     `AquiferClient::with_servers`. Add `startAquiferFake` (to create) in
     `parity/app/env-rust.mjs` beside `startDoor43Fake`, read from `TC4_AQUIFER_FAKE` (to
     create), and an `aquifer` option on `withApp` in `parity/app/journeys/kit.mjs`.
  2. Add the scenario `aquifer-pins` (to create): add it to `SCENARIOS` in
     `parity/tc4-baseline/fixtures/build-state.mjs` (about 377), run
     `node parity/app/state.mjs --scenario aquifer-pins --out <scratch dir>`, copy only the
     files it changes into `fixtures/parity-state/aquifer-pins/`, run
     `node parity/app/state.mjs --record`, and add a row to `fixtures/parity-state/README.md`.
     Never write those files by hand.
  3. Write `parity/app/journeys/aquifer.test.mjs` (to create) with
     `the_same_helps_journey_on_dcs_and_on_aquifer_offline`: one helps journey (open Titus
     1:1, read a note, a question, a Words article and an Academy article) as a function, run
     once with `scenario: 'en-pins'` and once with `scenario: 'aquifer-pins'`, both after the
     internet switch is off; fail on any request to either fake. Add the Portuguese pane's
     label check.
- Files: `crates/tc4/src/args.rs`, `parity/app/env-rust.mjs`, `parity/app/journeys/kit.mjs`,
  `parity/tc4-baseline/fixtures/build-state.mjs`, `fixtures/parity-state/`,
  `parity/app/state.sha256.json`, `parity/app/journeys/aquifer.test.mjs`.
- Done when: `cargo test -p tc-app --test app -- every_scenario_is_assembled_from_committed_bytes`
  passes and fails if one byte of a new scenario file changes; with `TC4_BIN`, `TC4_UI` and
  `TC4_AQUIFER_FAKE=target/debug/tc-aquifer-fake` set, `node --test
  parity/app/journeys/aquifer.test.mjs` passes and fails when step 4's slot loading is stashed.
- Record: log line `step 5` with the run's summary line; `next` to step 6.

### Step 6: Keep a tC4 Aquifer pin (Q33)

- Do:
  1. Read tC4 #712 (`gh issue view 712 -R unfoldingWord/translationcore4 --comments`). If tC4
     has settled its pin format, make a fixture project whose `resources.json` holds such a pin.
     If not, use an entry with an unknown source kind and say so in the test's comment.
  2. Write `a_project_with_a_tc4_aquifer_pin_opens_and_keeps_it`: the project opens, the
     pin's bytes are unchanged after open, a save and a close, and the guided fix is offered
     (Q33's answer).
  3. In `docs/TC4-ISSUES.md`, set the #711 row to covered with this test and the #720 rows
     to covered with step 4's tests. Update `docs/APP-DESIGN.md` about 1505-1512.
- Files: `crates/tc-app/tests/app/sources.rs`, a fixture under `fixtures/` (to create),
  `docs/TC4-ISSUES.md`, `docs/APP-DESIGN.md`.
- Done when: `cargo test -p tc-app --test app -- tc4_aquifer_pin` passes, and fails if the open
  rewrites `resources.json`.
- Record: log line `step 6`; set `state: verifying`.

## Gates before merge

The full gate list: `scripts/gates.sh` once INT-3 lands. Until then, each must pass:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus: `parity/app/journeys/aquifer.test.mjs` run locally (step 5) with its summary in the log;
no test touches the network (the fakes listen on loopback only).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The H6 ruling is missing: do not start step 3.
- The ruling asks for a screen the packet did not show. Send a new picture first.
- tC4 settles an Aquifer pin format that would make the rebuild write pins (Q33 (b)). That is
  a new owner decision; do not write pins.

## References

- `docs/PLAN.md` P6 (about 270); `docs/APP-DESIGN.md` about 1505-1512
- `docs/TC4-DELTA-2026-10-19.md` Q33 and Q34 (about 518-547)
- `.audit/roadmap/evidence/2026-10-21/done-definition.md` DOD-13;
  `tc4-coverage.md` F4; `app-remaining.md` F9; `tc4-bugs.md` T13
- `fixtures/aquifer/README.md`; `crates/tc-resources/src/aquifer.rs`

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/done-definition.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
