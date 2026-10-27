---
id: CHK-6
title: Lint ratchet and fresh P7 captures
track: A
lane: checks
milestone: M1
state: verifying
owner: checks lane (Claude Code 93835106)
branch: track-CHK-6
worktree: .worktrees/track-CHK-6
depends: [INT-4, CHK-6a]
needs_owner: []
proves: [P7]
no_loss: false
size: 1 d
merged: none
updated: 2026-10-26
next: step 5 is ready for a merge batch run by the integration lane (steps 2 and 3 landed in batch 21; steps 1, 4 and 6 as CHK-6a). The batch's merge tree must keep the captures' trees: a batch that also changes crates/tc-ui/src, crates/tc-ui/assets, crates/tc-ui/app or crates/tc-api/src recaptures every screen on it. Branch track-CHK-6
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
- 2026-10-24 00:30 EDT | checks lane (Claude Code 2198ee19) | note | none | Split: steps 1, 4 and 6 and the C3 bound are card CHK-6a, merged on its own; this card keeps steps 2, 3 and 5 and depends on CHK-6a | .audit/roadmap/units/CHK-6a.md
- 2026-10-24 01:44 EDT | checks lane (Claude Code 2198ee19) | note | none | On the orchestrator's option A (ruling 2026-10-23, rule 1): the alignment-budget job reports and does not fail in CI. suggestions_meet_the_full_training_corpus_budget asserts its p95 only with TC_ALIGN_BUDGET_ENFORCE set; its suggestion check still fails in CI. Main's ci 40065010453 had failed it at p95 150.76 ms against the 150 ms CI limit. The change rides with CHK-1a | crates/tc-align/tests/suggestion_budget.rs
- 2026-10-26 15:22 EDT | checks lane (Claude Code 2414b2c7) | resume | none | Resumed on main 86d669b1 (INT-4, S2, C7a, FIX-R and CHK-1 done; steps 1, 4 and 6 merged as CHK-6a): steps 2 and 3 now; step 5 waits until E1a, E1b and E1c merge, so its trees are not stale at once (orchestrator, 2026-10-26) | python3 -I .audit/tools/board.py
- 2026-10-26 17:54 EDT | checks lane (Claude Code 2414b2c7) | step 2 | none | Workspace lints unreachable, allow_attributes_without_reason, struct_excessive_bools and fn_params_excessive_bools are on, max-struct-bools and max-fn-params-bools = 3 in the root and the three crate clippy.toml files, each line under its invariant comment. Sites: 246 lint exemptions without a reason (237 host, 8 wasm-only, 1 Windows-only): test headers keep allow with a reason, written by a script; library ones became #[expect] with their old comment as the reason; 3 #[allow(clippy::too_many_lines)] were deleted (the lint is off, they allowed nothing). unreachable!: 21 #[expect] in library code carry each site's proof; the 10 in tests became panic!. 17 structs and 1 component fn (Layer) carry #[expect]/#[allow] saying why the flags stay independent (Field: A4b no-loss code, new states go in FieldSave). unreachable_pub counts 934, over 150: not turned on, a line in .audit/minors.tsv. Plant in tc-check/src/lib.rs, reverted: a four-bool struct gave "more than 3 bools in a struct", an #[allow(dead_code)] gave "`allow` attribute without specifying a reason". Steps clippy, clippy-release-bins, clippy-release-app, clippy-windows, wasm-ui, wasm-ui-gallery, wasm-ui-app, wasm-ui-hooks and wasm-api exit 0 | scripts/gates.sh --step clippy
- 2026-10-26 17:54 EDT | checks lane (Claude Code 2414b2c7) | step 3 | none | crates/tc-arch/tests/clippy_config.rs: every crates/*/clippy.toml holds each top-level key of the root clippy.toml but disallowed-methods and disallowed-types, with the same value. Plant: allow-dbg-in-tests deleted from crates/tc-app/clippy.toml gave "INV-PROVEN-PANICS: crates/tc-app/clippy.toml replaces the root clippy.toml but lacks allow-dbg-in-tests = true"; reverted | cargo test --locked -p tc-arch --test clippy_config
- 2026-10-26 17:59 EDT | checks lane (Claude Code 2414b2c7) | verify | none | Independent reviewer (a fresh general-purpose agent, steps 2 and 3, diff 654c083e..b8d99e5d): no blocker, no major. 1 Code: no type can carry a lint rule; clippy is the structure, and each bool struct says why it stays. 2 Checks: the four lints run in every clippy gate step, clippy_config runs in CI's test job in 0.02 s, each has a recorded failing plant. 3 Reach: build-time only, nothing to drop. 4 Siblings: src, tests and examples swept; it found one never-compiled reasonless allow in tc-server's stub. 5 No loss: not a no-loss card; library code only gained attributes, tests only swapped unreachable! for panic!. 6 Prose and Human: reasons give the why except four minors, all fixed in the next commit (two swapped bench reasons, seven bench headers called test code, drafts.rs's reason not proving its site, the dead cfg(not(test-hooks)) test_rows in a test-hooks-only module deleted); no owner questions | git diff 654c083e..HEAD
- 2026-10-26 19:21 EDT | checks lane (Claude Code 2414b2c7) | note | none | Main 2d4bea68 merged in; its one new reasonless allow (allow(clippy::type_complexity) on tc-store kept.rs prepare_import) allowed nothing and is deleted. Host, Windows and the four wasm clippy runs show no site left | scripts/gates.sh --step clippy
- 2026-10-26 19:21 EDT | checks lane (Claude Code 2414b2c7) | note | none | The first --touched run was stopped by tc-heavy as hung in tc-app's app tests, with app::save_machine::c3_reload_anywhere_keeps_each_saved_field_once the one test unfinished; alone it passes in 12 s, the whole app binary passed on the next run (758 in 276 s). Intermittent, not from this card (tc-app gained only attributes); the integration lane is told | target/chk6-touched.log (local)
- 2026-10-26 19:21 EDT | checks lane (Claude Code 2414b2c7) | gate | none | scripts/gates.sh --touched exits 0 on the branch with main 2d4bea68 merged | scripts/gates.sh --touched
- 2026-10-26 19:21 EDT | checks lane (Claude Code 2414b2c7) | park | none | Steps 2 and 3 go to the integration lane for a batch; step 5 waits until E1a, E1b and E1c merge (orchestrator, 2026-10-26) | python3 -I .audit/tools/board.py
- 2026-10-26 19:45 EDT | integration lane (Claude Code 3d07d5f8) | merge | 19c2c330 | steps 2 and 3 merged into main in batch 21; the card stays open for step 5. In the batch the new lints caught code that landed after the branch: tc-api door43.rs's unreachable! (a loop now), two allows without a reason (tc-sync share.rs, tc-app os_keychain.rs), and app.rs's expect(dead_code) on exchange, which E2 now uses; workspace, wasm, release and Windows clippy pass | scripts/gates.sh --wait-ci batch-21
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | resume | none | Resumed on main 8f76f29a (E1a, E1b and E1c merged) for step 5, the orchestrator's go | python3 -I .audit/tools/board.py
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | step 5 | d1ae6d86 | capture-rust.mjs writes on each captures.json entry `trees` (the git trees of crates/tc-ui/src, crates/tc-ui/assets, crates/tc-ui/app and crates/tc-api/src as they will be once committed, through a private index under target/) and labels a shot memory+scripted when the env called test/script during it; screen_record.rs's captures_match_the_current_ui_source fails while a capture's trees are not HEAD's. All 123 captures retaken with their own unit and layers (E1a and E1b with --real-sync); 13 come out memory+scripted on their own (B11's conflict-review cases, C8's upgrade and gateway-change cases, home-checkpoint-error), the sync and open screens store since E1a and E1b. Plant: a committed comment in crates/tc-ui/src/lib.rs gave "P7/INV-CAPTURES-FRESH: 123 captures were taken at an older UI source (crates/tc-ui/src df4ff54a..., now f6cf5333...): home, understand-tit1, ..."; reverted. Report totals: before 36 same, 87 differ, 7 unported, 0 failed; after 26 same, 97 differ, 7 unported, 0 failed (130 rows) | cargo test --locked -p tc-ui --test screen_record
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | note | d1ae6d86 | P7 finding, by the stop rule: main's UI had changed without a recapture, and the retake failed the gate 58 times. On the orchestrator's option A each change was checked against the card that made it and residuals.json names it: E2's "Receive work from a file" on every Home card and as Add a project's fifth card (it also puts Share under an open dialog's panel, so homeCards reads none), HIST-1's History toggle, FIX-R step 1's dir wrappers (no text line lost or added across the 123 DOMs) and #832.2's language name, G4 step 6's lang, F1's disabled importers (ruling H3-H4-H7); C7b's and E1a's stale sentences corrected. q2-obs-card widened to the moved button; a new mask conflict-mine-date covers the run's own clock. home-empty could not be captured: its hand reply lacked HomeView's notices and importRemoved (E1b, F1), added. Findings: .audit side note CHK-6-step5-recapture-findings.md | parity/app/residuals.json
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | note | none | Home card order leaks between gallery cases in one env: license, open-progress-state and once open-progress-prepare were taken with Tito y Jonás first after the open-progress cases opened it; each taken alone has tC4's order. The pixels+dom residual of open-progress-prepare hid it from the gate. Told the orchestrator | node parity/app/capture-rust.mjs --unit B7 --case license
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | verify | d1ae6d86 | Independent reviewer (a fresh general-purpose agent, diff d1ae6d86): no blocker. Major: the trees come from the working files, not from the build that took the shots, so a recapture without a rebuild would pass with old screens. Minors: the branch needs a recapture after each merge that touches the four folders; two runs share one index file; q2-obs-card's rectangle is the box round two areas (checked: no title or summary under it). The test/script count per shot, a capture with no trees failing, and five residual sentences checked against the DOMs hold | git show d1ae6d86
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | fix | none | The major: capture-rust.mjs refuses to start while tc4 is older than crates/tc-api/src or the UI build (its index.html, written afresh by build-app.sh) is older than any file of the four folders. Plant: touching crates/tc-ui/src/lib.rs gave "a build is older than the source its captures would name; rebuild first: .../target/tc-ui-app is older than crates/tc-ui/src, ...: crates/tc-ui/build-app.sh". The index file carries the process id | node parity/app/capture-rust.mjs --unit B2 --case home-empty
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | note | none | Main 2fc90cc1 (batch 21) merged in; its lint fix in crates/tc-api/src/door43.rs changed that tree, so every capture was retaken on the merge: only the loopback ports and the masked clock moved | cargo test --locked -p tc-ui --test screen_record
- 2026-10-26 21:30 EDT | checks lane (Claude Code 93835106) | note | none | To the app lane: from now on a commit that changes crates/tc-ui/src, crates/tc-ui/assets, crates/tc-ui/app or crates/tc-api/src recaptures every screen and case in the same commit (rebuild tc4 and the UI first, or capture-rust.mjs refuses), and so does a merge that changes them; captures_match_the_current_ui_source names the stale ones. A capture that differs then gets its residual naming the card, as this step did | crates/tc-ui/tests/screen_record.rs
- 2026-10-26 21:55 EDT | checks lane (Claude Code 93835106) | gate | ca26f869 | scripts/gates.sh --touched exits 0 on the branch with main 2fc90cc1 merged | scripts/gates.sh --touched
