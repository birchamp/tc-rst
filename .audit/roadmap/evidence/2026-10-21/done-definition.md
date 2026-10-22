# done-definition

## Summary
None of P1 to P10 is "shown by a check in CI that can fail" yet. What CI runs today on resume-build-lane (.github/workflows/ci.yml):
- the rust job (fmt, clippy, `cargo test --workspace`, which includes the P7 record gate `screen_record.rs` and the journal/store property and crash tests);
- the alignment-budget job;
- a Node job that only loads the 29 browser journey files. Every browser test skips there because TC4_BIN, TC4_UI and Playwright are missing (kit.mjs:21-39).

The browser journeys run only locally, through an untracked script (.enjoy-logs/resume-full-gates.sh) and in subsets. The largest run in the logs is 49 tests; there are about 200 static browserTest sites.

P1 coverage of the 24 live journeys:
- 10 reach a browser-checked end state with disk assertions.
- 7 are partial (J3, J8, J11, J12, J13, J17, J18).
- 7 have no test at all (J7, J9a, J9c, J9d, J23, J24, J26).

P2, P9 and P10 have no checks, because units X, I, V, E1a, E1b, E2 and F1 do not exist yet. P3 and P4 are strong at the journal and store level but have nothing for sync or import. P5's validate-every-write hook exists only on the s2 branch. P6's Aquifer half cannot be reached in the app and needs the owner's Q34 decision. P7's gate reads only committed captures: it cannot see stale captures and it does not enforce the E1a recapture of the 5 scripted sync screens. P8's Graphite probe is an informational spike with no owner unit.

Highest-leverage additions:
1. An app-e2e CI job now, split out of G2, that fails on any skipped test.
2. Burrito validation at the end of every journey.
3. Machine-checked ledgers: P-items to tests, §15 rows to tests, journeys to tests.
4. Mark scripted captures as scripted.
5. Union the three diverged ci.yml files when the branches are integrated.

# Definition of done (PLAN P1 to P10): which checks exist, where they run, and what is missing

All paths are in the `resume-build-lane` snapshot (f9067bc) unless marked [s2] (`s2-proof-executor`, 3b075d7) or [storage] (dd63328). "CI" means `.github/workflows/ci.yml` on GitHub.

## 0. What CI runs today

| Branch | Jobs in ci.yml | Evidence |
|---|---|---|
| lane | `rust` (fmt; clippy for the workspace, release libs, x86_64-pc-windows-gnu and wasm32 tc-ui with none, gallery, app and test-hooks, and tc-api; `cargo test --workspace`); `alignment-budget`; `parity-baseline` (`node --test` over baseline, mock-server, state, observe, env-rust, view-gallery, recovery and `parity/app/journeys/*.test.mjs`) | ci.yml:11-72 |
| main (af4eef3) | the same, without `alignment-budget` | diff of the two ci.yml files |
| s2 | adds `schedule` (cron `17 3 * * *`) and `store-model-nightly` (`random_histories_lose_no_work_long`), and adds `.audit/tools/*.test.mjs`. It lacks the Windows clippy, tc-ui test-hooks and tc-api wasm steps | [s2] ci.yml |
| storage | adds a `perf` job (`perfreport ci --tolerance 3`). It lacks the Windows clippy | [storage] ci.yml |

**The browser journeys run nowhere in CI.**
- kit.mjs:21-23 returns `skip` unless `TC4_BIN` and `TC4_UI` are set. kit.mjs:24-29 returns `skip` unless Playwright resolves.
- The parity-baseline job sets neither variable and runs no `npm ci` (ci.yml:65-72).
- kit.mjs:10-11 says so itself: "CI's node job runs these files with no browser".

**Where the journeys do run.**
- Locally only, through `/home/bot/tc-rst/.worktrees/resume-build-lane/.enjoy-logs/resume-full-gates.sh`. That file is untracked: 0 files under `enjoy-logs` in `git ls-tree resume-build-lane`. Each merge runs only a subset.
- The largest runs in the logs: `resume-c2-browser.log` with 49 tests (167 s) and `merge-c4b-gates.log` with 32 tests.
- There are 200 static `browserTest(` sites in 29 files. No log shows a full run.

**Other places checks run.**
- The Windows VM runs fmt, clippy, test and wasm clippy on each new main (`.audit/tools/winvm/gates.ps1:8-13`). It runs no journeys.
- A nightly schedule only runs from the default branch's workflow. `origin/HEAD` is `claude/focused-ride-5uzj7p`, so s2's nightly does nothing until it is merged there. This is an inference from GitHub's documented behavior.
- `resume-build-lane` and `s2-proof-executor` have no upstream, so GitHub has run none of their CI.

## 1. Table: P-item, checks, gaps, and the check to add

| P | Checks that exist (file, where it runs) | Gaps | Check to add, owner, and whether GitHub CI can run it |
|---|---|---|---|
| **P1 Journeys** | 29 files in `parity/app/journeys/`. They run locally only and skip in CI. Each starts `tc4`, Chromium and optionally the fake Door43, and fails on page errors and foreign requests (kit.mjs:73-107). Section 2 maps the journeys. | 7 of 24 live journeys have no test; 7 are partial. No journey validates the Burrito. APP-DESIGN §7.7 names a `tc-parity-assert` helper and a `test/inspect` endpoint; neither exists (grep finds them only in APP-DESIGN.md:3042, and the test routes are boom, crash, echo, hold, keychain, quiet, release, reset). There is no RTL-project run (tC4's J10 axis on J2, J4, J5 and J7). tC4's j14 join-isolation has no counterpart. | (a) An **`app-e2e` job now**, owned by a new unit "CI-E2E" split out of G2, because G2 waits for P (APP-DESIGN:3426). Steps: wasm32 target; `cargo install wasm-bindgen-cli --version 0.2.129` (build-app.sh:22-23); Node 22; `npm ci` in `parity/tc4-baseline` (playwright 1.56.1); `npx playwright install --with-deps chromium`; `cargo build -p tc4 --features test-hooks,fixture-backend`; `cargo build -p tc-door43-fake`; `build-app.sh` and `build-app.sh --test-hooks --out target/tc-ui-app-hooks`; then `node --test parity/app/journeys/*.test.mjs`. GitHub CI: yes, on Linux. Timing budgets stay off, because they are gated by `TC4_PERF=1` (align.test.mjs:267,491; check-keyboard.test.mjs:227). (b) `TC4_REQUIRE_BROWSER=1` in kit.mjs turns a skip into a failure, so a misconfigured job cannot pass green. (c) A journey ledger `parity/app/journeys.json` (J-number to test names), with a Node test in parity-baseline that fails when a live J in tC4's JOURNEYS.md index has no entry or names a missing test. |
| **P2 Team work** | Nothing runs end to end. These are scripted or partial: `conflict-review.test.mjs` (J18 over `test/script` sync; header lines 1-8), `sync-button.test.mjs` (scripted), and `share-open.test.mjs` (the open "ends in its refusal", lines 11-13). The server is the Rust `tc-door43-fake`: a Gitea-API fake plus git smart HTTP through `git http-backend` (tc-door43-fake/src/lib.rs:1-40). Its own tests run in CI's `cargo test`. | No sync (X, E1a), share push (E1b), USB (E2) or check-conflict resolution (C7b). `withApp` starts one `tc4` and its own fake (kit.mjs:73-79), so the harness cannot run three teammates against one server. No recorded live Door43 answers exist (fixtures/door43/README.md, last paragraph). | J17, J26 (with A, B and C as three app-data folders and the C7b step) and `usb_both_directions`, owned by X, E1a, E1b, E2 and C7b. Harness: `withApps(n, {door43})` sharing one fake, owned by E1a. GitHub CI: yes, on Linux; git is on the runner. Recording real Door43 answers needs a machine that can reach qa.door43.org, which is a human step. |
| **P3 No silent loss** | `tc-journal/tests/properties.rs`: a team of 3 to 5 devices (sim/mod.rs:1); `fold_converges_under_any_union`, `conservation`, `concurrent_differences_are_visible`, `merge_units_sound`, `resolution_always_possible`; 64 cases, or 256 for view properties (properties.rs:28-34). `tc-journal/tests/loss.rs` has 60+ named `loss_*` tests; `tc-store/tests/store/loss.rs` covers S1's cases. All run in CI's `cargo test`. | The properties use only the in-memory `Transport` (properties.rs:44-56). The zip and git transports belong to X. My name check of STORAGE §15 rows against `fn` names: lane has 102 rows, 148 named tests, 64 absent; [s2] has 143 rows, 219 names, 49 absent, of which 28 are sync, USB or import names (`sync_*`, `usb_*`, `loss_f4_*`, `team_*`, `import_*`). No check ties §15 to code. No nightly long sweep exists for `TC_PROPERTY_CASES`. | (a) A test that parses STORAGE §15. It fails when a named test does not exist, unless the row names its pending owner unit. Owner: process, now. CI: yes. (b) X implements `Transport` for exchange zips and for local git. (c) A nightly `TC_PROPERTY_CASES=4096` run, in ci.yml on the default branch. |
| **P4 Crash safety** | lane `tc-store/tests/store/crash.rs`: 9 tests (save verse, section save, draft, checkpoint seal and materialize, months folder, materialize, create, open repairs, unflushed pack) under FaultFs with kill, power cut and both power-loss models. [s2] crash.rs has 16 tests and adds Git G1-G3, the materialize ledger and held saves. [s2] `model.rs` `random_histories_lose_no_work` runs 200 cases on every push, and `_long` runs 5,000 cases with `every_row` nightly ([s2] model.rs:2533-2547). All run in `cargo test` except the nightly. | There are no write points for sync or import, because X and I do not exist, so STORAGE §6.4 rows Y2 and I2 and tC4's G8 are unchecked. s2's nightly is not on the default branch. The real NTFS power-cut probe (STORAGE §16.5 item 5) has no check. | Crash rows for sync intake, push, USB export and receive (owner X), and for import (owner I), all through the same `kill_everywhere` helper. CI: yes. The NTFS power cut needs the release pipeline's Windows machine; GitHub-hosted runners cannot cut power. This is human or VM work. |
| **P5 Valid burritos** | `tc-burrito` has its own tests (samples, mutations, properties). Lane: explicit `assert_valid_burrito` calls in tc-store tests (about 41 calls in 7 files) and in tc-app tests (about 30 calls in 10 files), plus `scenario_store.rs:386` validating the seeded parity folders. [s2] adds a writer hook: `Rig::drop` fails any test whose store wrote an invalid state ([s2] tests/store/common.rs:23-50, src/hooks.rs:108), plus `every_commit_tree_is_a_valid_burrito`. All run in `cargo test`. | The lane has no `check_burrito` hook (0 matches in src/hooks.rs). tc-app tests only check when a test asks. Browser journeys never validate; J1 and J5 check selected fields or md5s by hand (create.test.mjs:64-90, align.test.mjs:80-90). There are no exports (D2), USB files (E2), sync intake (X) or imports (I) to validate. | (a) After S2 integrates, install the hook in tc-app's `StoreWorld`. Owner: the S2 merge. (b) A `test/validate` route behind test-hooks that runs `tc_burrito::validate_dir` on every project folder. `withApp` calls it after `body` and asserts `[]`. Owner: CI-E2E. (c) D2, E2, X and I write through the same hook. CI: yes. |
| **P6 Helps sources** | DCS helps reach the screens on the `en-pins` scenario (understand, check-detail and story journeys; local only). Aquifer is tested only in libraries: `tc-helps/tests/aquifer.rs` (18 tests), `tc-resources/tests/aquifer_install.rs` (13) and `aquifer_proxy.rs` (2), over real BibleAquifer fixtures at tag v2026-10-12 (fixtures/aquifer/README.md). These run in CI's `cargo test`. | No screen can reach Aquifer: "nothing in the app installs an Aquifer asset" (APP-DESIGN:1509-1512). That waits for Q34 and a design. There is no parity scenario with Aquifer pins (`fixtures/parity-state` has `common`, `en-pins` and `shared`). No unit owns P6. | Human first: the owner decides Q34, meaning how Aquifer is offered. Then a unit adds an `aquifer-pins` scenario and runs Understand and the Check reading panes on it, offline, against a fake Aquifer server (`tc-resources/tests/common/aquifer_fake.rs` promoted to a binary). CI: yes. |
| **P7 Same look** | `parity/tc4-baseline`: 62 screens, of which 6 are in `ar` and 56 in `en`; 49 complete and 13 incomplete; tC4 d310bf3, 4.0.0-rc.1, Chromium 141 (manifest.json). `screen_record.rs` recomputes `report.json` from the committed captures in `cargo test` on CI. Report: 130 rows. Screens: 51 differ, 11 same. Gallery cases: 28 differ, 22 same, 18 unported, waiting on G1, F1, E1b and D1. `residuals.json` explains 79 rows plus 10 general items. `gallery_record.rs` (ds-gallery, 53 `ar` cases) and `baseline.test.mjs` (parity-baseline job) also run in CI. | (1) The gate cannot see stale captures. `captures.json` has `build.commit: "dev"`, and the gate never reads `build` (only report.rs:587 copies it). (2) The 5 M+S sync screens were captured through `test/script` (env-rust.mjs:233,291) but are labelled `store`, and `ScriptedOverdue` fires only for `MemoryScripted` (gate.rs:565-569). So E1a and E1b are never forced to recapture them. (3) No human review is recorded: residual rows carry only `layers`, `why` and `until`. (4) Only 6 of 62 screens have an `ar` baseline. (5) tC4's release/4.0 is now rc.2, 38 commits after d310bf3, with 7 files under `src/views` and 10 under `src/i18n` changed. PLAN:40-41 moves the reference to v4.0.0 when it is tagged. | (a) CI-E2E recaptures the DOM and meaning layers on every push and fails on any difference from the committed captures. Pixels compare only on a machine whose fingerprint matches (APP-DESIGN §7.9). (b) Fix now: `capture-rust.mjs` marks any capture that called `test/script` as scripted, and the gate enforces its recapture unit. (c) G2: add a `reviewed: {by, date}` stamp to each residual; the gate requires it once `units.json` lists G2. (d) G2: capture `ar` for all 62 tC4 screens on the owner's machine (not in CI). (e) A baseline test that PLAN's reference commit equals `manifest.tc4.commit`. |
| **P8 Scripts** | Arabic interface RTL: app-language h and j, Home h, `arabic_helps_panes_are_rtl_in_an_english_interface`, Arabic IME text into a Spanish project (translate.test.mjs:187-211). All local. Also 6 `ar` baseline screens and 53 `ar` ds-gallery cases, through the CI record gates. Graphite: `spikes/electronite`, excluded from the workspace (Cargo.toml:4-6). | The Graphite probe is "a smoke test, not the P8 probe" (electronite-spike-linux evidence §5). It asserts only `fontLoaded` (page.test.cjs:98) and has no limits on width, ascent or descent. No unit in APP-DESIGN §10 owns it (G1 and G2 rows, lines 3425-3426). No Hebrew text is edited or rendered in any journey. There is no RTL target project. | (a) A new **P8-probe** unit, or an addition to G1's scope: the probe inside the shell, asserting the joined width (<480 px is the spike's rule) and the vertical extent within limits. CI: Linux yes, with an Electronite 37.1.0 download and Xvfb, as G0 did here; Windows and macOS need release-pipeline runners. (b) An RTL fixture project (`ar` or `he` target) with J2, J4 and J5 run on it, and Hebrew original text in Align on Jonah. Owner: a parity unit. CI: yes, with Playwright. |
| **P9 Carry-over** | None. There is no importer: unit I and F1 are "to do" (STATUS.md). The fixtures `fixtures/tc4/conformance-fixtures/import/MANIFEST.json` hold 16 cases: tC3 2 accept and 3 refuse, USFM 2 accept and 2 refuse, Burrito 5 accept (one is the `fake` parser) and 2 refuse. No Rust or journey test reads the MANIFEST; only usfm vectors read the `usfm/` files. | PLAN P9's four tC4 4.0 sub-cases (a synced team project with leftover sync folders, a stopped sync apply refused per Q23, a torn `metadata.json`, platform stamps) have no fixtures on any branch. There is no byte-equality check. | Unit I: a table-driven test over the MANIFEST plus the four new fixtures, asserting a valid Burrito, scripture bytes identical, and refusal codes. F1: J9a, J9c and J9d journeys. CI: yes. |
| **P10 Old projects** | Partial pins in `tc-journal`: `every_kind_has_the_pinned_wire_form` (canonical.rs:264; 64 lines, 18 kinds), `the_encoding_is_pinned` (index.rs:867), `tokens_have_the_pinned_byte_form_and_key` (tokens.rs:94), `format_json_is_written_in_one_byte_form` and forward-compatibility tests over format-1.3, 1.4 and 2.0 fixtures (versioning.rs). These run in CI. | Unit V is "to do". There are no frozen sample projects. `TC_JOURNAL_BLESS=1` rewrites the pins with no check that the format version moved. Packs, the manifest and `materialized.json` are not pinned. `tc4` has only `serve` (args.rs:108), so an old build can only be driven over HTTP. The format is not frozen yet (STORAGE §16.5). | Unit V: `fixtures/capsules/<major.minor>/*` (read-only, sha256-pinned), and a `FORMAT_PINS` table keyed by version, so any pinned-byte change also needs a new version row and new capsules. A CI matrix job checks out the last frozen tag, builds it, and opens, edits and validates folders the current build wrote. CI: yes. |

## 2. P1 journey map (tC4 JOURNEYS.md index lines 28-56, against lane journeys)

| J | Rust test (file: test) | State | Missing piece (owner) |
|---|---|---|---|
| J1 | create: `J1: a Bible is created…` | browser + disk | Burrito validation (CI-E2E) |
| J2 | section (20 tests), translate (11) | browser + disk | RTL run |
| J3 | guided-fix (3), source-texts (2); the download itself in tc-app `tests/app/sources.rs` | partial: "the fake's parity account publishes no resources" | a browser download against the seeded fake (C8 follow-up) |
| J4 | check-writes: `j4_…one_decision_record_on_disk` | browser + disk | RTL run |
| J5 | align: `j5_align_steps_that_do_not_save` + `expectTwoStoredLinks` | browser + disk | RTL run |
| J6 | check-writes: `j6_…` | browser + disk | — |
| J7 | none (community-checking: `…runs_none_until_d2`) | none | D2 (PDF needs G1) |
| J8 | check-keyboard `resume_and_tab_click…`, check-picker `resume_into…` | partial | D1 (checkpoint on leave) |
| J9a, J9c, J9d | none | none | I, F1 |
| J11 | j11-sign-in (8), share-open (3) | up to the check step | E1b |
| J12 | upgrades: `Check for updates asks…`; the upgrade in tc-app `upgrades.rs` | partial | C8 follow-up |
| J13 | upgrades: `Keep…`, `Change writes…` | browser; no disk read | add a disk assertion |
| J16 | comments: `j16_…` | browser + disk | — |
| J17 | share-open: `Open a project from Door43…refusal` | refusal only | X, E1b |
| J18 | conflict-review (3) over a scripted sync | partial | C7a, E1a |
| J20 | create: `J20: …` | browser + disk | — |
| J21 | story-writes: `j21_…` (+4) | browser + disk | — |
| J22 | check-writes `j22_…`, story `j22_…` | browser + disk | — |
| J23 | none | none | D2 |
| J24 | none | none | E1b |
| J25 | story `j25_…` (2), comments `j25_…` | browser + disk | — |
| J26 | none (sync-button is scripted) | none | X, E1a, C7b, multi-app kit |

Totals: 10 reach browser and disk, 7 are partial, 7 are missing. tC4 specs with no Rust counterpart: `bundled-upgrade`, `j10-rtl`, `j14-join-isolation`.

## 3. Integration notes for the plan

- When the branches meet, the three ci.yml files must be **unioned**: the lane's jobs, s2's nightly and `.audit/tools` tests, and storage's `perf` job. Then they must be pushed to the default branch for `schedule` to run.
- Commit a tracked journey runner, for example `scripts/journeys.sh`, built from `resume-full-gates.sh`. Then local runs, the Windows VM and CI share one entry point.
- The lane's STATUS.md is stale by one unit. It was generated at 3eac396, before the B11 merge f07fd9e.

## Findings
- [DOD-1] (critical/high) No browser journey runs in any CI job: parity-baseline only loads the 29 journey files, and every browserTest skips without TC4_BIN, TC4_UI and Playwright.
  - evidence: resume-build-lane .github/workflows/ci.yml:65-72 (no env, no npm ci); parity/app/journeys/kit.mjs:10-11, 21-39 (skip logic); resume-full-gates.sh exists only in the untracked /home/bot/tc-rst/.worktrees/resume-build-lane/.enjoy-logs/; largest local run: resume-c2-browser.log, 49 tests; 200 static browserTest sites in 29 files
  - rec: Add an app-e2e job now as a new unit CI-E2E, split from G2, which waits for P. Add TC4_REQUIRE_BROWSER=1 so a skip fails the job. Commit a tracked runner script.
- [DOD-2] (critical/high) P1 coverage: of 24 live journeys, 10 have browser plus on-disk end-state tests, 7 are partial (J3, J8, J11, J12, J13, J17, J18) and 7 have none (J7, J9a, J9c, J9d, J23, J24, J26). No RTL-project run exists.
  - evidence: ~/unfoldingword/translationcore4/docs/JOURNEYS.md:28-56; grep of J-numbers and browserTest names in parity/app/journeys/*.test.mjs; share-open.test.mjs:11-13 (open ends in refusal); source-texts.test.mjs header (fake publishes no resources)
  - rec: Add parity/app/journeys.json mapping J-numbers to tests, checked by a Node test against JOURNEYS.md in the parity-baseline job. Missing journeys go to D1, D2, E1a, E1b, I, F1 and C7a.
- [DOD-3] (high/high) Browser journeys never validate the Burrito end state, and the test/inspect endpoint and tc-parity-assert helper named in APP-DESIGN §7.7 do not exist.
  - evidence: APP-DESIGN.md:3040-3043; tc-server and tc-app test routes found: boom, crash, echo, hold, keychain, quiet, release, reset (no inspect); create.test.mjs:64-90 checks fields by hand only
  - rec: Add a test/validate route behind test-hooks that runs tc_burrito::validate_dir per project folder. kit.withApp asserts it returns no problems after every body.
- [DOD-4] (high/high) The writer hook that validates every written state exists only on s2. The lane's tc-store has no check_burrito hook, and tc-app tests validate only where a test asks.
  - evidence: [s2] crates/tc-store/tests/store/common.rs:23-50 (Rig::drop asserts); [s2] crates/tc-store/src/hooks.rs:108; lane crates/tc-store/src/hooks.rs: 0 matches for check_burrito; lane crates/tc-app/tests/support/store_world.rs:330-350 (explicit helper)
  - rec: After S2 integrates, install the hook in tc-app StoreWorld so every tc-app test validates every write. D2, E2, X and I use the same hook.
- [DOD-5] (high/high) The P7 gate cannot detect stale Rust captures: captures.json records build.commit "dev" and the gate never compares the build with the source.
  - evidence: parity/app/captures.json build = {ui: 8a97c92765d47ed3, commit: dev}; crates/tc-ui/tests/screen_record/inputs.rs:174 (field only), report.rs:587 (copied, never checked)
  - rec: CI-E2E recaptures the DOM and meaning layers on every push and fails on a difference. Pixels compare only on a fingerprint-equal machine.
- [DOD-6] (medium/high) The 5 scripted sync screens are labelled 'store', so the gate's ScriptedOverdue rule will not force E1a or E1b to recapture them on real sync.
  - evidence: parity/app/report.json: sync-ok, sync-failed, open-door43-refused have backend 'store'; parity/app/env-rust.mjs:233,291 (test/script drives pushToDoor43 and teammates); crates/tc-ui/tests/screen_record/gate.rs:551-569 (only MemoryScripted triggers)
  - rec: capture-rust.mjs labels any capture that used test/script as scripted, and the gate enforces pending_until. Small fix, now or in E1a.
- [DOD-7] (medium/high) P7's human review is not recorded anywhere machine-checkable, and only 6 of 62 baseline screens have an Arabic interface.
  - evidence: parity/app/residuals.json: 79 row entries with keys {layers, why, until} only; 10 general; parity/tc4-baseline/manifest.json: locale ar on 6 screens; totals 62 (49 complete, 13 incomplete); APP-DESIGN.md:3426 (G2: 'a person reviewed the residuals')
  - rec: G2 adds a reviewed {by, date} stamp per residual, enforced once units.json lists G2, and captures ar for all 62 tC4 screens.
- [DOD-8] (medium/high) The tC4 baseline is at rc.1 (d310bf3). release/4.0 is now rc.2, 38 commits later, with 7 src/views and 10 src/i18n files changed. PLAN moves the reference to v4.0.0 when it is tagged.
  - evidence: parity/tc4-baseline/manifest.json tc4.commit d310bf3, version 4.0.0-rc.1; tc4-tip: git rev-list --count d310bf3..origin/release/4.0 = 38; diff stat over src/; PLAN.md:40-41
  - rec: Add a baseline test that PLAN's reference commit equals manifest.tc4.commit. Plan a recapture when v4.0.0 is tagged.
- [DOD-9] (high/high) No P8 Graphite probe exists as a check and no unit owns it. The spike asserts only that the font loaded and is excluded from the workspace. No journey renders or edits Hebrew or an RTL target project.
  - evidence: spikes/electronite/test/page.test.cjs:98; docs/evidence/electronite-spike-linux-2026-10-17.md §5 ('smoke test, not the P8 probe'); Cargo.toml:4-6 (spikes excluded); APP-DESIGN.md:3425-3426 (G1 and G2 rows do not mention the probe)
  - rec: Create a P8-probe unit with width and vertical-extent thresholds, run under Xvfb with Electronite 37.1.0 on Linux CI and on the Windows and macOS release runners. Add an RTL project fixture with J2, J4 and J5 runs.
- [DOD-10] (critical/high) P2 has no end-to-end team journey. Only scripted pieces exist, and the journey kit runs one tc4 per test with its own fake Door43, so it cannot run the three-teammate J26.
  - evidence: parity/app/journeys/conflict-review.test.mjs:1-8 (scripted sync); parity/app/journeys/kit.mjs:73-79; crates/tc-door43-fake/src/lib.rs:1-40 (git http-backend fake); fixtures/door43/README.md (no recorded live answers)
  - rec: X, E1a, E1b, E2 and C7b deliver J17, J26 and USB both ways. E1a adds withApps(n) with a shared fake. These can run on GitHub Linux runners.
- [DOD-11] (high/medium) STORAGE §15 names regression tests that do not exist as functions: 64 of 148 on the lane and 49 of 219 on s2, of which 28 are sync, USB or import tests. Nothing checks §15 against the code.
  - evidence: name check script /tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/dod/s15.py run on each snapshot; docs/STORAGE.md:4169-4173 ('Each row is a named regression test')
  - rec: Add a test that parses §15 and fails on a named test that is absent unless the row names its pending owner unit. Run it in cargo test.
- [DOD-12] (high/high) P3 and P4 are strong at the journal and store level but cover no sync or import write points. s2's nightly 5,000-history model run will not fire until its ci.yml reaches the default branch.
  - evidence: crates/tc-journal/tests/properties.rs:28-56 (in-memory Transport only); lane crates/tc-store/tests/store/crash.rs (9 tests); [s2] crash.rs (16 tests incl. Git G1-G3); [s2] ci.yml schedule plus store-model-nightly; origin/HEAD -> claude/focused-ride-5uzj7p
  - rec: X and I add crash rows and Transport implementations for zip and git. Merge s2's nightly into the default branch's ci.yml. Add a nightly TC_PROPERTY_CASES sweep.
- [DOD-13] (high/high) P6's Aquifer half cannot be reached in the app ('nothing in the app installs an Aquifer asset') and waits for owner decision Q34. Aquifer has only library tests, and no parity scenario has Aquifer pins.
  - evidence: docs/APP-DESIGN.md:1505-1512; crates/tc-helps/tests/aquifer.rs (18 tests), crates/tc-resources/tests/aquifer_install.rs (13); fixtures/parity-state: common, en-pins, shared
  - rec: Ask the owner to decide Q34 and the design. Then a unit adds an aquifer-pins scenario and helps-screen journeys against a fake Aquifer server.
- [DOD-14] (high/high) P9 has no check: there is no importer, the 16-case import MANIFEST is read by no test, and P9's four tC4 4.0 sub-case fixtures do not exist on any branch.
  - evidence: fixtures/tc4/conformance-fixtures/import/MANIFEST.json (16 entries); grep: no crates or parity test reads MANIFEST.json or the tc3 zips; only parity/view-gallery/states.mjs:215 names one; STATUS.md: I and F1 to do
  - rec: Unit I adds a table-driven MANIFEST test plus four new fixtures, checking byte equality and validity. F1 adds J9a, J9c and J9d journeys.
- [DOD-15] (medium/high) P10 is partly pinned (change-line wire form, format.json, tokens, index encoding), but there are no frozen capsules, TC_JOURNAL_BLESS can rewrite pins without a version bump, and no CI job builds an old version.
  - evidence: crates/tc-journal/tests/canonical.rs:264-266; crates/tc-journal/tests/versioning.rs:281; fixtures/journal/README.md; crates/tc4/src/args.rs:108 (serve only)
  - rec: Unit V adds sha-pinned capsules per version, a FORMAT_PINS table tied to the version, and a CI matrix job that builds the last frozen tag and opens the current build's projects.

## Unverified
- Whether GitHub Actions is enabled for birchamp/tc-rst, and the latest CI result on af4eef3: no network check was made (gh or the Actions UI would show it).
- Whether all 200 browser tests pass together, and how long a full run takes on a GitHub runner: the rules forbid running them here, and no full-run log exists. Run `node --test parity/app/journeys/*.test.mjs` with the five env variables on the lane worktree once the heavy-test slot is free.
- Whether geometry and font-dependent journeys (check-keyboard 'geometry within 1 px', app-language j) are stable on ubuntu-latest fonts. Try it with a first CI-E2E run.
- Whether the §15 names my script reports missing are renamed tests or really absent: the script matches only `fn name(` and journey test titles. Review /tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/dod/s15.py output row by row.
- The claim that GitHub runs `schedule` only from the default branch's workflow comes from GitHub's documentation, not from a run seen here.
- Whether an Electronite 37.1.0-graphite build can be downloaded and run headless under Xvfb on a GitHub Linux runner. G0 ran it in this container, not on GitHub.
- Whether tC4's v4.0.0 tag will land on release/4.0 at 9f06f99 or later. Today there is no v4.0.0 tag (git tag -l shows rc.2 as the newest).