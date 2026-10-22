---
id: CHK-6
title: Lint ratchet and fresh P7 captures
track: A
lane: checks
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: [P7]
no_loss: false
size: 1 d
merged: none
updated: 2026-10-21
next: Claim this card after INT-4 merges, then do step 1.
blocked_by: none
---

# CHK-6: Lint ratchet and fresh P7 captures

## Goal

Clippy refuses new bags of booleans, new `unreachable!` without a written reason, and any lint
exemption without a reason; today's sites carry `#[expect(..., reason = "...")]`. Test-only code
leaves the release build. The P7 gate fails when a committed capture was taken from older UI
source than HEAD, or was scripted but labelled as real, or when PLAN's reference commit is not
the baseline's. No test is left out of CI without a line that says which job runs it.

## Why it matters to a translator

P7 promises the screens look like tC4. A gate that reads stale captures can stay green while a
screen changed. The lints keep state in enums, where a translator cannot meet an impossible mix
of flags.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| (INT-4|S2|CHK-1) '`. INT-4 must be `done`.
  Step 2 edits `crates/tc-store` and `crates/tc-journal`: their part waits for S2 `done`. Step 3
  needs `crates/tc-arch` (CHK-1): `test -d crates/tc-arch && echo ok`. Do what you can, then park
  with `next` naming the waiting part.
- `UPDATE_REPORT= scripts/gates.sh --list; echo $?` prints INT-3's refusal and `2`: the gate
  script already refuses `UPDATE_REPORT`, `UPDATE_GOLDEN`, `UPDATE_PREPARE`, `TC_JOURNAL_BLESS`
  and `TC_WRITE_PROBE` (INV-GATES-NEVER-BLESS, `units/INT-3.md` step 1). This card adds no second
  copy of that refusal.
- `grep -n 'workspace.lints' -A8 Cargo.toml`: clippy has `unwrap_used`, `expect_used`, `panic`,
  `dbg_macro`, `todo`; rust has `unsafe_code`. `crates/tc-app/clippy.toml:1-6` copies the root's
  four `allow-*-in-tests` keys because a crate's clippy.toml replaces the root one.
- Today's sites (resume-build-lane at f9067bc, by a script; the compiler's count is the truth):
  `#[allow]` with no reason: 28 in `src` (for example `crates/tc-versification/src/standard.rs:81`,
  `crates/tc-resources/src/dcs.rs:141`, `choice.rs:36`) and 188 in tests; `unreachable!`: 22 in
  `src`, 6 in tests; structs with more than 3 bools: 13 (for example
  `crates/tc-ui/src/app/save/mod.rs:231` `Field`, `crates/tc-app/src/align/actor.rs:104` `Actor`).
- Test-only code in release: `crates/tc-server/src/lib.rs:26` `pub mod stub;` (no cfg);
  `crates/tc-api/src/buffer.rs:73` `FieldKind::Test`, matched at `crates/tc-app/src/drafts.rs:57`,
  `crates/tc-ui/src/app/web/saves.rs:534` and `crates/tc-ui/src/app/save/rules.rs:55`.
- P7: `parity/app/captures.json` `build` is `{ui: 8a97c92765d47ed3, commit: dev, api: 1}`;
  `RustBuild` (`crates/tc-ui/tests/screen_record/inputs.rs:182`) is read by no check. The five
  M+S screens of `docs/APP-DESIGN.md` §7.6 (`open-door43-refused`, `sync-failed`, `sync-ok`,
  `sync-ok-teammates`, `sync-ok-restored`) were captured with backend `store` through
  `test/script` (`parity/app/env-rust.mjs:233,301`), so `ScriptedOverdue`
  (`crates/tc-ui/tests/screen_record/gate.rs:565-569`) never fires for them.
- Recapturing needs this PC (its fingerprint is `captures.json` `machine`), `tc4` with
  `--features test-hooks,fixture-backend`, the UI build and Playwright: `units/G5.md` step 1.

## Funnel card

- **Invariants**:
  - INV-NO-BAGS-OF-BOOLEANS: no struct has more than 3 bools and no fn more than 3 bool
    parameters, except at sites with an `#[expect]` and a reason.
  - INV-PROVEN-PANICS: every `unreachable!`, `expect`, `panic!` and lint exemption in library
    code carries its reason in an `#[expect(..., reason = "...")]`.
  - INV-CAPTURES-FRESH (P7): every committed Rust capture names the git trees of
    `crates/tc-ui/src`, `crates/tc-ui/assets`, `crates/tc-ui/app` and `crates/tc-api/src` it was
    taken from, and they equal HEAD's; a capture that called `test/script` is labelled
    `memory+scripted`; PLAN's reference commit is the baseline's `tc4.commit`.
  - INV-NO-BLESS-IN-CI: a gate run never rewrites what it checks (INT-3's refusal, plus a scan
    that every blessing variable a test reads is in it), and every `#[ignore]` test names the job
    that runs it.
- **Wrong states**:
  - A UI change passes CI while the committed captures show the old screen.
  - E1a lands and the sync screens are never recaptured on real sync.
  - A new flag joins three others on `Field` and makes a state no screen expects.
  - A new `#[ignore]` test silently never runs.
- **Code**:
  - Types: `Capture` (`screen_record/inputs.rs:197`) gains `trees`
    (`BTreeMap<String, String>`); `stub` and `FieldKind::Test` exist only with `test-hooks`.
  - One entry point: capture-rust.mjs is the only writer of `captures.json`; it sets `trees` and
    the scripted label from what happened, not from a flag.
  - Removed: test-only code from release builds.
  - Left to checks, and why a type cannot carry it: whether a capture is current is a fact about
    files and git, checked by a test.
- **Checks**:
  - Workspace lints `clippy::unreachable`, `clippy::allow_attributes_without_reason`,
    `clippy::struct_excessive_bools`, `clippy::fn_params_excessive_bools`; kind: lint; runs in:
    the clippy gate steps.
  - `every_crate_clippy_toml_keeps_the_root_keys` in `crates/tc-arch/tests/clippy_config.rs`
    (to create); kind: architecture; runs in: CI job `test`.
  - `captures_match_the_current_ui_source` in `crates/tc-ui/tests/screen_record.rs` (to create);
    kind: parity gate; runs in: CI job `test`.
  - `PLAN's reference commit is the baseline's` in `parity/tc4-baseline/baseline.test.mjs`
    (to create); kind: contract; runs in: the gate step `node`.
  - `every blessing variable is refused by the gate script` and `every ignored test names its
    job` in `.audit/tools/ignored.test.mjs` (to create), with `scripts/ignored-tests.txt` (to
    create); runs in: the gate step `node` and the gate step `ignored` (step 4).
  - Proof it can fail: each step plants one violation and records the message.
- **Prose**: none.
- **Human**: none.

## Steps

### Step 1: Take test-only code out of release builds

- Do: put `#[cfg(feature = "test-hooks")]` on `pub mod stub;` (`crates/tc-server/src/lib.rs:26`)
  and fix the two doc links to `stub` (`lib.rs:12`, `backend.rs:7`). Put the same cfg on
  `FieldKind::Test` (`crates/tc-api/src/buffer.rs:73`) and on each arm that names it (split an
  or-pattern such as `rules.rs:55` into its own arm).
- Files: those five files.
- Done when: `scripts/gates.sh --step clippy-release-bins`, `--step clippy-release-app`,
  `--step wasm-ui`, `--step wasm-ui-hooks` and `cargo test --locked -p tc-server` pass;
  `cargo build --locked --release -p tc4 -j 2` then
  `TC4_RELEASE_BIN=target/release/tc4 cargo test --locked -p tc4 --test release -j 2 -- --ignored`
  passes (`release_has_no_test_hooks`, `crates/tc4/tests/release.rs:119`).
- Record: a log line `step 1`.

### Step 2: Turn on the lints and give today's sites their reasons

- Do: add to `[workspace.lints.clippy]` `unreachable = "warn"`,
  `allow_attributes_without_reason = "warn"`, `struct_excessive_bools = "warn"`,
  `fn_params_excessive_bools = "warn"`; add `max-struct-bools = 3` and `max-fn-params-bools = 3`
  to `clippy.toml` and to every `crates/*/clippy.toml`. Run
  `cargo clippy --locked --workspace --all-targets -- -D warnings 2>&1 | tee target/chk6-lints.log`.
  In `src`: each site gets `#[expect(<lint>, reason = "<the invariant or why>")]`; an existing
  `#[allow(x)]` becomes `#[expect(x, reason = ...)]`. In tests: the file-level
  `#![allow(clippy::unwrap_used, ...)]` headers keep `allow` (a test file may stop using one) and
  gain `reason = "test code: a failed unwrap is a failed test"`; do it with one scripted
  rewrite and read the diff. A bool struct's reason names the card that will turn it into an
  enum, or why it stays (`Field`: "A4b no-loss code, property-tested; new states go in
  `FieldSave`", health-app §8). Then measure `unreachable_pub`:
  `cargo clippy --locked --workspace --all-targets -- -W unreachable_pub 2>&1 | grep -c 'unreachable .pub. item'`.
  Up to 150: add `unreachable_pub = "warn"` to `[workspace.lints.rust]` and fix them. Over 150:
  leave it out and add a line to `.audit/minors.tsv` with the count.
- Files: `Cargo.toml`, `clippy.toml`, `crates/*/clippy.toml`, every file clippy names.
- Done when: `scripts/gates.sh` passes every clippy step; a new struct with four `bool` fields
  in `crates/tc-check/src/lib.rs` fails clippy with `more than 3 bools in a struct`, and an
  `#[allow(dead_code)]` with no reason fails it too. Revert both.
- Record: a log line `step 2` with the count per lint and the `unreachable_pub` count.

### Step 3: Keep every crate clippy.toml in step with the root

- Do: write `crates/tc-arch/tests/clippy_config.rs`: for each `crates/*/clippy.toml`, every
  top-level key of the root `clippy.toml` except `disallowed-methods` and `disallowed-types` is
  present with the same value. Message:
  `INV-PROVEN-PANICS: crates/<c>/clippy.toml replaces the root clippy.toml but lacks <key> = <value>`.
- Files: `crates/tc-arch/tests/clippy_config.rs` (to create).
- Done when: `cargo test --locked -p tc-arch --test clippy_config` passes; deleting
  `allow-dbg-in-tests` from `crates/tc-app/clippy.toml` fails it naming that key. Revert.
- Record: a log line `step 3`.

### Step 4: No silent blessing and no orphan ignored test

- Do: write `.audit/tools/ignored.test.mjs`. Test 1 scans `crates/**/tests/**/*.rs` and
  `crates/**/src/**/*.rs` for `env::var_os("X")` / `env::var("X")` where X starts with `UPDATE_`,
  ends with `_BLESS` or is `TC_WRITE_PROBE`, and fails for each X missing from the refusal list in
  `scripts/gates.sh`. Create `scripts/ignored-tests.txt`: one line per `#[ignore]` test, its name,
  a tab, and the CI job that runs it or `local: <why>` (today 7 on f9067bc, for example
  `crates/tc-align/tests/suggestion_budget.rs`, `crates/tc4/tests/release.rs`, plus S2's
  `model::random_histories_lose_no_work_long`, job `store-model-nightly`). Write
  `scripts/check-ignored.sh` (to create): it runs
  `cargo test --locked --workspace -- --list --ignored` and fails with
  `INV-NO-BLESS-IN-CI: ignored test <name> has no line in scripts/ignored-tests.txt` for each
  unlisted name, and for each listed name that no longer exists. Add
  `"ignored|scripts/check-ignored.sh"` to `STEPS` and `- run: scripts/gates.sh --step ignored`
  to CI's `test` job after the `test` step.
- Files: `.audit/tools/ignored.test.mjs`, `scripts/ignored-tests.txt`, `scripts/check-ignored.sh`
  (to create); `scripts/gates.sh`; `.github/workflows/ci.yml`.
- Done when: `node --test .audit/tools/ignored.test.mjs` and `scripts/gates.sh --step ignored`
  pass; an `#[ignore]` added to any test fails the step naming it; INT-3's
  `gates-sync.test.mjs` passes.
- Record: a log line `step 4`.

### Step 5: Tie the captures to the UI source and label scripted ones

- Do: in `parity/app/capture-rust.mjs`, give each captured entry (lines 151-157) `trees`: for
  each of `crates/tc-ui/src`, `crates/tc-ui/assets`, `crates/tc-ui/app` and `crates/tc-api/src`,
  the tree the working files will have once committed, so a UI change and its recapture land in
  one commit: with `GIT_INDEX_FILE=target/capture-index` (never `/tmp`), run `git read-tree HEAD`,
  `git add -A -- <the four paths>`, then `git write-tree --prefix=<path>/` per path; delete the
  index file after. Count the `test/script` calls
  the env makes while each screen is captured (`env-rust.mjs:219` `call`); when there is one,
  write `backend: 'memory+scripted'` whatever `--backend` said. In `screen_record/inputs.rs`
  add `trees: Option<BTreeMap<String, String>>` to `Capture` (line 197). Write
  `captures_match_the_current_ui_source`: for every capture, `trees` equals HEAD's; else fail
  `P7/INV-CAPTURES-FRESH: <n> captures were taken at an older UI source (<path> <a>, now <b>):
  <names>; recapture them with parity/app/capture-rust.mjs`. Then recapture every screen and case
  with its own unit and backend (`--unit <its by> --only <names>`, the five M+S screens with
  `--backend memory+scripted`), run `UPDATE_REPORT=1 cargo test --locked -p tc-ui --test
  screen_record` outside the gate, and review the report diff.
- Files: `parity/app/capture-rust.mjs`, `crates/tc-ui/tests/screen_record/inputs.rs`,
  `crates/tc-ui/tests/screen_record.rs`, `parity/app/captures.json`, `parity/app/screens/`,
  `parity/app/dom/`, `parity/app/report.json`.
- Done when: `cargo test --locked -p tc-ui --test screen_record` passes; a one-line comment
  committed in any file under `crates/tc-ui/src` makes `captures_match_the_current_ui_source`
  fail naming the captures (revert that commit); `grep -c '"memory+scripted"' parity/app/captures.json`
  prints at least 5. Tell the app lane in a `note`: from now on a commit that changes those four
  folders recaptures in the same commit.
- Record: a log line `step 5` with the report totals before and after.

### Step 6: Check PLAN's reference against the baseline

- Do: in `parity/tc4-baseline/baseline.test.mjs` (it already reads `docs/PLAN.md`, line 17) add
  `PLAN's reference commit is the baseline's`: the commit in "the reference is tC4 `main` at
  commit `<sha>`" of PLAN's "### The reference version" (line 40 names `d310bf3`) is a prefix of
  `manifest.json` `tc4.commit`. Message:
  `P7/INV-CAPTURES-FRESH: PLAN names reference <a>; the baseline was captured at <b>`.
- Files: `parity/tc4-baseline/baseline.test.mjs`.
- Done when: `node --test parity/tc4-baseline/baseline.test.mjs` passes; changing PLAN's
  `d310bf3` to `a5ce030` fails it. Revert.
- Record: a log line `step 6`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 made it; INT-4 needs INT-3) on the exact tree you merge. It must print
`gates: all <n> steps passed; receipt ...`. If the script is missing, run these instead:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
3. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
   the same with `--features gallery`, `--features app` and `--features test-hooks`
4. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
6. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
   (needs `gcc-mingw-w64-x86-64` and `nasm`, as `.github/workflows/ci.yml` installs them)

Plus: the gate step `ignored`; the recapture of step 5 on this PC; every journey once locally
(`units/G5.md` step 1), since step 1 changes what the test build compiles.

## Stop and ask when

- The recapture shows a screen that now differs from tC4 where it did not before: that is a P7
  finding. Record it with the screen name; fix it here only if the cause is in this card's
  changes, otherwise open a card.
- Step 2 would edit `crates/tc-store` or `crates/tc-journal` while S2 is not `done`.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/checks-inventory.md` ranks 14, 16, 18; findings C12
- `evidence/2026-10-21/done-definition.md` P7 row, DOD-5, DOD-6, DOD-8
- `evidence/2026-10-21/health-domain.md` §6 item 10, F11; `evidence/2026-10-21/health-app.md`
  §5, §7, §8 rows 10 and 13, CH-13
- `docs/APP-DESIGN.md` §7.6 (M+S screens, lines 3028-3032); `units/INT-3.md` step 1
- Size basis: checks-inventory ranks 14 (half a day), 16 (half a day plus a sweep) and 18 (1 h);
  the recapture adds time on this PC.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
