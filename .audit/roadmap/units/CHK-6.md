---
id: CHK-6
title: Lint ratchet and fresh P7 captures
track: A
lane: checks
milestone: M1
state: parked
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-6
worktree: .worktrees/track-CHK-6
depends: [INT-4]
needs_owner: []
proves: [P7]
no_loss: false
size: 1 d
merged: none
updated: 2026-10-24
next: step 2 after C7a, FIX-R and S2 merge (the lint pass edits their crates); step 3 after CHK-1 merges (crates/tc-arch); step 5 after the app lane's UI merges (recapture); then the review and a merge turn. Branch track-CHK-6
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
  - Workspace lints `clippy::unreachable`, `clippy::allow_attributes_without_reason`
    (INV-EVERY-ALLOW-HAS-A-REASON), `clippy::struct_excessive_bools`,
    `clippy::fn_params_excessive_bools` (INV-NO-BAGS-OF-BOOLEANS); kind: lint; runs in:
    the clippy gate steps. The invariant names sit as comments on the lint lines (step 2).
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
  to `clippy.toml` and to every `crates/*/clippy.toml`. Each lint line in `Cargo.toml` and each
  `max-*-bools` line carries a comment naming the invariant it enforces, so `board.py --invariants`
  and a reader can find it: `# INV-NO-BAGS-OF-BOOLEANS` above the two `*_excessive_bools` lines
  and the two `max-*-bools` keys; `# INV-EVERY-ALLOW-HAS-A-REASON` above
  `allow_attributes_without_reason`. Run
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

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

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
- 2026-10-23 23:50 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-CHK-6 from main 325244a1. Order decided: steps 1, 4 and 6 now (their files overlap no active branch); step 2's lint pass edits nearly every crate while C7a and FIX-R change tc-api, tc-app and tc-ui and S2 changes tc-store and tc-journal, so it waits for those merges; step 3 needs crates/tc-arch (CHK-1, parked); step 5 recaptures the screens and waits for the app lane's UI merges, or its trees go stale at once | git diff --name-only claude/focused-ride-5uzj7p...track-C7a
- 2026-10-24 00:05 EDT | checks lane (Claude Code 2198ee19) | step 1 | none | `pub mod stub;` in tc-server is under cfg(feature = "test-hooks"), its two doc links plain text. Decided, against the card: FieldKind::Test stays in every build. With the variant under cfg, the workspace clippy and the Windows clippy failed ("E0004: non-exhaustive patterns: `tc_api::buffer::FieldKind::Test` not covered" in tc-ui): cargo unifies tc-api's test-hooks feature across the workspace, so tc-ui's host library sees the variant while its own feature is off, and a crate cannot test another crate's feature. Instead the variant carries `#[cfg_attr(not(feature = "test-hooks"), serde(skip))]`: a release build refuses a test field on the wire. clippy, clippy-release-bins, clippy-release-app, clippy-windows, wasm-ui, wasm-ui-app, wasm-ui-hooks, wasm-api, wasm-ui-gallery all exit 0; tc-server and tc-api tests pass; the release build and release_has_no_test_hooks pass | .enjoy-logs/chk6-step1b.log
- 2026-10-24 00:05 EDT | checks lane (Claude Code 2198ee19) | step 4 | none | scripts/check-ignored.sh (gate step `ignored`, after `test` in CI's test job) lists the workspace's ignored tests with cargo and fails on one with no line in scripts/ignored-tests.txt, or a line naming no ignored test. The list holds today's 7: suggestions_meet_the_full_training_corpus_budget in job alignment-budget, six local with their reason. .audit/tools/ignored.test.mjs: every blessing variable a source reads is refused by gates.sh, and every list line names a workflow job or `local: <why>`. Plants, each reverted: #[ignore] on tc-versification's an_ordinary_shift_is_lossless: "INV-NO-BLESS-IN-CI: ignored test an_ordinary_shift_is_lossless has no line in scripts/ignored-tests.txt"; a read of UPDATE_SNAPSHOTS: "INV-NO-BLESS-IN-CI: crates/tc-versification/tests/losses.rs reads UPDATE_SNAPSHOTS, which scripts/gates.sh does not refuse; add it to the refusal list". gates-sync passes | scripts/gates.sh --step ignored
- 2026-10-24 00:05 EDT | checks lane (Claude Code 2198ee19) | step 6 | none | baseline.test.mjs "PLAN's reference commit is the baseline's": PLAN's "the reference is tC4 `main` at commit `<sha>`" is a prefix of manifest.json tc4.commit. Plant: PLAN's d310bf3 as a5ce030: "P7/INV-CAPTURES-FRESH: PLAN names reference a5ce030; the baseline was captured at d310bf304cfee784f4ca45dee474a3faf0d5616e"; reverted | node --test parity/tc4-baseline/baseline.test.mjs
- 2026-10-24 00:05 EDT | checks lane (Claude Code 2198ee19) | park | none | Parked: steps 2, 3 and 5 wait (see next) | python3 -I .audit/tools/board.py
- 2026-10-24 00:29 EDT | checks lane (Claude Code 2198ee19) | fix | none | CI on 95d6badd (run 40060957519) failed app::save_machine::c3_reload_anywhere_keeps_each_saved_field_once: "Elapsed" at the scenario's 30 s wall-clock bound (a Reference field, not the test field step 1 touched). Raised to 120 s with a "hung" message, as S2's branch already does for the verse scenario two hunks above (so the edits merge); ruling 2026-10-23 | gh run view 40060957519 --log-failed
