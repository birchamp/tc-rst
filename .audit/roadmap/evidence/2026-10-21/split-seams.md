# split-seams

## Summary
Best handoff: Package B, "Shell and release": G1 (the Electronite shell), the Windows, macOS and Linux release pipelines and installers, the OBS image fetch, the P8 Graphite probe, and a new CI job that runs the browser journeys. Its interface is already a written, tested contract: the `tc4 serve` command line and output lines (APP-DESIGN §5.1, crates/tc4/tests/lifecycle.rs) and `window.tc4Shell` (§5.7). The page side of the bridge is already built (crates/tc-ui/src/app/web/bridge.rs). No shell code exists outside spikes/. CI runs only on ubuntu. No macOS build happens anywhere. The pieces to port exist: the G0 spike shell (765 lines) and tC4's own packaging (package-desktop.yml 588 lines, package-desktop.zsh 1,115 lines). Conflict risk is near zero if B adds new files only and never edits crates/, Cargo.*, or ci.yml. One plan change is needed: G1 is listed as waiting for D2, but only printPdf needs D2, so G1 can start now.

Second choice: Package A′, "Import core": a pure `tc-import` library plus the P9 tests. The store write (R29), the import screens (F1) and the tC4 team move (§13.3) stay with the main team. tc-journal already has every import variant, its public changes are additive on both branches, and the §13.2 seed already works in memory in tc-app. A′ can start now, in parallel with X, which shortens the storage chain. Its data risk is high, so the main team must review it adversarially.

Do not hand off Exchange (X with E1a, E1b, E2). It depends on tc-store internals that S2 and the next journal-files machine are still rewriting, and `commit_received` does not exist yet. Export (D2) is small and touches the busiest app files.

# How to split off a package for another developer (evaluator: split dimension)

Snapshots: **lane** = resume-build-lane f9067bc; **s2** = s2-proof-executor 3b075d7; **main** = af4eef3. No builds or tests were run. Sizes are estimates, and the basis for each is given.

## 1. The facts behind the split

| Fact | Evidence |
|---|---|
| Lane is 89 commits ahead of main and 0 behind, so main can fast-forward. s2 is 77 ahead and 541 behind. | `git rev-list --count` (run) |
| A trial merge of lane and s2 conflicts in tc-store `handle.rs`, `hooks.rs` and `open.rs`, two tc-store test files, `Cargo.toml`, `Cargo.lock` and `.github/workflows/ci.yml`. | `git merge-tree --write-tree resume-build-lane s2-proof-executor` |
| S2 is still being validated: long-model failures and new mutation recipes appeared on 2026-10-21. The journal-files state machine comes next, estimated at 2–3 weeks. | s2 `.audit/plans/S2-PROOF-2026-10-21.md:11-43`; `JOURNAL-MACHINE-ASSESSMENT.md:43-50` |
| tc-store more than doubles in size on s2 (11,869 → 26,046 src lines, with new `git`, `ledger`, `checkpoint` and `outside` modules). | `wc` of `crates/tc-store/src` in both snapshots |
| `commit_received`, the intake path that sync needs (STORAGE §5.0, §9.3 T2, §10.2 R7), is not in tc-store on either branch. | grep returned nothing |
| tc-journal's public changes since the merge base 7e62455 are additive only. `record.rs` is unchanged. | `git diff` of `pub` lines on both branches |
| The import variants already exist: `ImportSource::{Tc4, Tc4Reconcile, Tc3, Usfm, Burrito}` and `DeviceKind::{Tc4Actor, Tc3User, …}`. | lane `tc-journal/src/change.rs:77-91`, `ids.rs:126-138` |
| The §13.2 seed is already built and checked in memory: it seals packs, folds them and compares bytes. | lane `tc-app/src/backend/seed.rs:1-31` (1,305 lines, plus check/records/strict 1,930, tests 996) |
| Unit I currently runs after X: its task says "tc-journal, tc-store and tc-sync are committed". | s2 `.audit/workflows/storage-chain-B.js:35` |
| G1 is listed as waiting for D2, and D2 waits for S2. | lane `.audit/STATUS.md` rows G1 and D2; `APP-DESIGN.md:3425` |
| No shell exists outside spikes. The spike shell already has the single-instance lock, spawn, filter, mirror and `printToPDF`. | lane `spikes/electronite/shell/main.cjs:89,128,193,418-432` (535 lines), `lib.cjs` 202, `preload.cjs` 28 |
| CI has only ubuntu jobs. The browser journeys are skipped in CI. | lane `.github/workflows/ci.yml:12,48,66`; `parity/app/journeys/kit.mjs:5-12` (30 files) |
| Two lane files contain macOS-only code (`tc-app/src/config.rs`, `tc-app/tests/app/feedback.rs`) that is never compiled. | grep `target_os = "macos"` |
| tC4 already has a three-OS pipeline: windows-2025, macos-15, ubuntu-24.04. It uses Inno Setup, Azure Artifact Signing through GitHub OIDC, and an unsigned macOS arm64 pkg. | `~/unfoldingword/translationcore4/.github/workflows/package-desktop.yml:72,92,254`; `docs/PACKAGING.md:333-345,545-600,917` |
| A precedent exists for work on another machine: the outside session works only on named branches, never pushes main or storage, and main merges after checks. | lane `.audit/tasks/windows-track.md` "Rules" |

## 2. The candidates compared

| | A: I + F1 + P9 | **A′: Import core only** | **B: Shell and release** | C: X + E1a + E1b + E2 | D: D2 + Burrito zip |
|---|---|---|---|---|---|
| Size (estimate) | Core 5–8k lines plus F1 1.5–3k. Basis: STORAGE §13 is about 210 spec lines (s2 `STORAGE.md:4389-4599`); tC4 `src/data/import` is 1,039 TS lines; the existing seed is 3.2k src lines | 5–8k lines (same basis) | 4–6k lines. Basis: spike 765 lines plus tests; tC4 scripts to port about 3.2k lines (package-desktop.yml 588, package-desktop.zsh 1,115, sign-windows.ps1 162, smoke-api.cjs 336, desktop-bootstrap.cjs 466, desktop-main.cjs 183, plus small ones) | Largest, at least the size of S1 (11.9k src). Spec §9–§10 is about 420 lines, plus 3 UI units and journeys J11, J17, J24, J26 and USB | 2–3k lines. tc-export is 750 src lines today |
| Needs from the rest | tc-store write entry (R29, "no" owner today, `APP-DESIGN.md` §9.2), S2 git; F1 needs tc-api/tc-app/tc-ui | tc-journal types, `pack::seal`, the fold and projection, `tc_usfm::decompose`/`recompose`, `alignment::split_aligned`, `tc_burrito` (all stable or additive) | The `tc4 serve` contract, the bridge §5.7, `build-app.sh`, `bundle_store`, `fetched.lock` (all on main and tested) | New tc-store actor steps (intake, integrate, merge commit) in the files S2 is rewriting | S2 checkpoint, plus §10.1's verify (X) |
| Files shared with other work | Medium-high: Cargo.toml, tc-app seed, hot API/app/UI files | Low: one Cargo.toml member line | ≈0 if limited to new files | Very high: tc-store, tc-door43, tc-app, tc-ui | Medium-high: tc-app/tc-ui |
| Skills and hardware | Rust; reading TS; Leptos for F1 | Rust; reading TS | Node/Electron, CI YAML, PowerShell/zsh, **a Mac**, Windows | Rust, git internals, libgit2 | Rust, Leptos |
| Risk to translators' data | High | High: imported history becomes permanent at format freeze. Owner: keep I lean (rebuild.tsv ruling 2026-10-18T13:25Z) | Medium: the mirror, close guard and quit hold unsaved text | Highest (P2, P3, P4) | Low-medium |
| Can the developer verify it alone? | Partly | Yes: `cargo test -p tc-import` on `fixtures/tc4/conformance-fixtures/{import,team-sync}` (20 + 19 files) | Yes: node tests, Electronite Linux under Xvfb (G0), GitHub runners | No: needs live storage | Partly |
| Verdict | Split it into A′ | **Second** | **Best** | Keep in the main team | Keep in the lane |

## 3. Recommended: Package B "Shell and release" (units G1, G3, G4, G5)

### 3.1 Work to do before the handoff (main team, one small PR, about one day — inference)

1. Fast-forward main to lane (≥ f9067bc), so the developer starts from the newest app.
2. Add `docs/contracts/SHELL.md`, version 1, holding the frozen points in §3.2.
3. Add **`fixtures/shell/buffer-vectors.json`**, input lines with expected output. A Rust test in tc-ui must assert that `tc_ui::app::buffer::current_lines` (lane `buffer.rs:396`) produces it. The Playwright stub `Bridge.current()` (lane `crates/tc-ui/app/no-loss.mjs:213-229`) already ignores the "removal names a text" rule. That is test-only, but it shows the drift that shared vectors prevent.
4. Add **`docs/contracts/shell-bridge.json`**, the method names. A host test reads the `method("…")` calls in `bridge.rs` and compares them with this file.
5. Change APP-DESIGN §10: G1 no longer depends on D2, because `printPdf(url)` is part of the contract. Add Phase 6 rows **G3** (release pipeline and installers, plus the OBS fetch), **G4** (P8 probe) and **G5** (app-e2e CI, moved out of G2). `status.py` only finds dependencies matching `[A-GJ]\d+[a-b]?` (lane `.audit/tools/status.py:20-38`), so new ids must be `G<n>`.

### 3.2 The frozen interface (SHELL.md v1)

| # | Item | Source |
|---|---|---|
| 1 | `tc4 serve --app-data <dir> --ui <dir> [--projects <dir>] [--door43 <url>] [--bundle <dir>]`. A release build has no test flags. | lane `crates/tc4/src/args.rs:37-38`; `tests/release.rs` `release_has_no_test_hooks` |
| 2 | The first stdout line is `{"url":"http://127.0.0.1:PORT/","token":"<43 base64url>"}`, or `{"error":{"type":"appLockHeld","value":{"pid":…}}}` with exit 3, or `{"error":{"type":"start","value":{"reason":"…"}}}` with exit 4. Usage errors exit 2; server errors exit 1. | `APP-DESIGN.md` §5.1; `tc4/src/lib.rs:29-32` |
| 3 | Writing `{"quit":true}` to stdin gets `{"closed":true}` back. Closing stdin or SIGTERM quits too. A close takes at most 5 s. | §5.1; `tests/lifecycle.rs` (`quit_message_answers_closed_then_exits`, `sigterm_quits_like_the_quit_message`) |
| 4 | Headers are `X-Tc-Token` and `X-Tc-Api`. The shell's filter adds the token only on paths outside `/api/` (AD11). The page sends it on `/api/` calls. | `tc-api/src/endpoint.rs:19,22`; `tc-ui/app/boot.js:161` |
| 5 | `window.tc4Shell` has `printPdf`, `chooseFolder`, `chooseFile`, `openExternal`, `setCanClose`, `onCloseRequested` (5 s), `onDownloadDone`, `feedbackSend` → `{ok, reason?: offline\|timeout\|refused\|not-configured}`, `bufferChanged`, `takeBuffered`, `platform` | §5.7; `bridge.rs:1-20` |
| 6 | `drafts-buffer.log` holds one `BufferLine` per line (`tc_api::buffer`). It is fsynced at most once a second, off the main thread. The reduction is `current_lines`. A torn last line is skipped. | §5.7; §3.10; G0 notes in `APP-DESIGN.md:48-88` |
| 7 | `TC4_BUILD_COMMIT` is set for both the server and the wasm builds. The UI comes from `crates/tc-ui/build-app.sh` (wasm-bindgen 0.2.129). | §3.10; lane HANDOFF "Tools" |
| 8 | The bundle is `cargo run -p tc-app --example bundle_store -- --from bundle/en --out <dir>`. The OBS pack `uW/obs_images_360@7146d5b5` must match sha256 `d8170e18…` and size 37,766,473 in `fetched.lock`. A build missing any pinned file fails. | `bundle/en/README.md` "Fetched when the app is packaged"; `fetched.lock` |
| 9 | Electronite v37.1.0-graphite, pinned zips with sha256 (win32-x64 `8146CA21…`) | tC4 `PACKAGING.md:917`; lane `.audit/tasks/g0-windows.md` |

### 3.3 Deliverables

- **G1 shell** (no-loss ◆). It goes in `shell/` (`main.cjs`, `preload.cjs`, `lib.cjs`) and implements everything in the G1 row (`APP-DESIGN.md:3425`). The lifecycle is one explicit state machine: Starting, Preparing, Running, Restarting, Closing, Quitting. One pure `next()` function, with I/O kept in an executor. This follows the owner's robustness rule (HANDOFF "Robustness").
- **G3 release pipeline**. Add `.github/workflows/release.yml` with jobs for windows-2025, macos-15 (arm64) and ubuntu-24.04. It produces installers (Inno Setup and pkg, as tC4 does), runs a post-install smoke test (a port of tC4 `scripts/smoke-api.cjs`), writes `BUILD-MANIFEST.json`, and signs only on manual runs through OIDC with no stored secret (tC4 D99), and only once the owner decides to. Docs go in `docs/RELEASE.md`.
- **G4 P8 probe**. Start from the spike's `page/probe.js:639-706`, which already measures ascent and descent. Run it in Electronite on all three runners. The job fails when Graphite shaping falls back.
- **G5 app-e2e**. Add `.github/workflows/app-e2e.yml`. It builds `tc4` with `test-hooks,fixture-backend`, `build-app.sh` with and without hooks, and `tc-door43-fake`, then `npm ci` in `parity/tc4-baseline`. It sets `TC4_BIN`, `TC4_UI`, `TC4_UI_HOOKS` and `TC4_DOOR43_FAKE` (kit.mjs:5-12) and runs `node --test parity/app/journeys/*.test.mjs`. **The job fails if any test is skipped.**
- Optional, only with the main team's agreement because it touches Cargo.toml: an OS keychain implementation of `tc_app::account::Keychain` using `keyring` (Q-3, `APP-DESIGN.md:3556`). It is part of E1a today.

### 3.4 Acceptance tests, all runnable by the developer

- `node --test shell/**/*.test.cjs`: argument parsing, filter patterns, token redaction, the buffer vectors, bridge names against the contract, and a port of tC4 `scripts/desktop-prepare.test.cjs` (96 lines).
- Electronite end-to-end tests under Xvfb, and on Windows and macOS runners:
  - `second_launch_focuses_the_running_window_and_starts_no_server`
  - `ending_tc4_restarts_it_and_reloads_home`
  - `ending_the_shell_ends_tc4_within_5s`
  - `hard_kill_while_typing_offers_the_text_at_relaunch`
  - `close_with_unsaved_text_asks`
  - `same_origin_help_link_stays_in_app`
  - `header_filter_reaches_every_request_type` (G0's 12 request types)
  - a timing check that `/api/` p95 is not raised by mirror writes (G0 baseline 7–11 ms)
- Release: the installer builds and the smoke test passes on all three operating systems; the shipped binary passes `release_has_no_test_hooks`; the OBS pack check passes; a build that leaves out a pinned file fails.
- The P8 probe passes on all three operating systems, and the G5 job is green with zero skips.

### 3.5 Branch and merge protocol

1. The developer works on a fork or in `ext/shell/*` branches on `birchamp/tc-rst`, never on main, storage or resume-build-lane. This follows `windows-track.md`.
2. Each PR covers one deliverable (G1, G3, G4, G5) as one coherent commit series (AGENTS "Commits").
3. The developer merges main into the branch at least weekly and never rebases pushed commits. Before a PR, `git merge-tree --write-tree origin/main HEAD` must show no conflicts.
4. The main team gates each PR the way the owner already requires: merge in a separate worktree, then run every AGENTS check (fmt; clippy for the workspace, `-p tc-app --lib` and `-p tc-server -p tc4 --lib --bins`; `test --workspace`; the wasm clippies), plus the node tests, `.audit/tools/winvm/gates.ps1`, and the release workflow green on the PR. One verification pass follows: fix blockers and the majors that harm data or what the translator sees; minors go to `.audit/minors.tsv`.
5. The merge commit is titled `Merge G1: …` (so `status.py` counts it), and a `merge` line goes into `.audit/rebuild.tsv`.

### 3.6 Communication and tracking

- A 30-minute call each week, plus `.audit/ext/G-shell.md` on the developer's branch, listing each deliverable's state with commit hashes.
- A contract change bumps the version in `docs/contracts/SHELL.md` and needs both sides to approve. The vector and name tests make any drift fail CI.
- Questions for the owner are written in plain words with a recommendation, as the owner asks.

### 3.7 Boundaries while B is out

**Main must not change:** items 1–9 of §3.2; the assertions in `crates/tc4/tests/lifecycle.rs` and `release.rs`; the startup protocol of `crates/tc-ui/app/prepare.html`; the `ui/<build>/` folder layout; the `BufferLine` format; the `bundle_store` command line; the `fetched.lock` format. New bridge calls are allowed only from the §5.7 list.

**B must not edit:** `crates/**`, `Cargo.toml`, `Cargo.lock`, `.github/workflows/ci.yml` (it already conflicts between lane and s2), `docs/STORAGE.md`, the generated `.audit/STATUS.md`, or `spikes/` (reference only). A change B needs inside `crates/` is filed as a request to the main team.

### 3.8 The enforcement funnel for B

| Layer | What B does there |
|---|---|
| Code | One lifecycle state machine; one entry point for each bridge method |
| Checks | Vectors and names contract tests, end-to-end tests, the release smoke tests, the zero-skip journeys job |
| Prose | `SHELL.md` and `RELEASE.md` |
| Human | Signing identity: reuse uW's Azure profile? macOS notarization or Developer ID; GitHub runner minutes; help-desk token secret (tC4 `scripts/desktop-feedback.cjs:3-5`); who the developer is and what access they get |

## 4. Second choice: Package A′ "Import core" (unit I, split)

- **Contract to freeze first** (main team writes the skeleton crate and its types):
  - tc-import depends on tc-core, tc-usfm, tc-burrito, tc-journal and zip. **It does not depend on tc-store.**
  - Functions: `identify(&SourceTree) -> SourceKind`, `review()` (the dry run of R11), `convert(…, progress, cancel) -> ImportPlan`, `verify(&ImportPlan, &SourceTree)` (P9: fold, project, `tc_burrito::check`, byte comparison).
  - `ImportPlan` holds `project_id`, `name`, `packs: Vec<EncodedPack>`, carried files, the `project.json` fields, `git: Fresh | AdoptFrom(path)` (for §13.1 T1/T9), the commit message and trailers, the paths to remove, the `.bak` files, and the report.
  - `trait TagResolver`: the tC3 pin lookup; the network work happens in tc-app through tc-door43.
  - `pub mod tc4::convert_segments`, which the §13.3 bridge in X reuses.
- **Main team keeps:** `Store::create_imported` (R29, built after S2 merges, with the crash-injection test of P4), F1, §13.3 and V.
- **Deliverables:** §13.1 T2–T8, §13.2 (moved out of `tc-app/src/backend/seed*`), §13.4, §13.5, the four P9 tC4 cases, named replays of E1–E6, L2, L9–L11 and L16 (storage-chain-B.js:35), and `memory_seed_equals_tc_import_seed`.
- **Moving target:** tC4's tip changed `journalingStore.ts`, `seal.ts` (8 MiB segment limit, D104) and `tc3.ts` after d310bf3 (`git diff --stat` in the tip clone). The P9 reference moves to the v4.0.0 tag when it exists (PLAN:40-41).
- **Tracking:** keep id `I` in the STORAGE table, with its sub-deliverables listed in `.audit/ext/I-import.md`. Merge it as `Merge storage: unit I`.

## 5. What the plan must say

1. Do B now. Do A′ once S2 has merged or the main team has frozen `ImportPlan`.
2. Remove "tc-sync committed" from I's prerequisites. Only §10.2 R2 and §13.3 need X.
3. Keep X, E1a, E1b, E2, D1, D2, P, T and V in the main team.

## Findings
- [S1] (high/high) Shell and release (G1, plus the release pipelines and installers, the P8 probe and the app-e2e CI job) is the best package to hand off: its interface is already a written, tested contract and it touches almost no shared files.
  - evidence: lane docs/APP-DESIGN.md §5.1 and §5.7 (lines 1608-1664, 1895-1935); lane crates/tc4/tests/lifecycle.rs: the_address_line_comes_first_and_names_loopback, quit_message_answers_closed_then_exits; lane crates/tc-ui/src/app/web/bridge.rs:1-20 (the page side is built); git diff --stat main..lane: bridge.rs +5 lines, tc4/src/join.rs +10 lines (small drift); No shell code outside spikes/ (find *.cjs)
  - rec: Hand off B to a developer who has Windows and a Mac. Freeze docs/contracts/SHELL.md v1 first.
- [S2] (medium/high) G1 is listed as waiting for D2 (which waits for S2), but only printPdf depends on D2, and the spike already implements printToPDF.
  - evidence: lane .audit/STATUS.md row G1 'Waits for D2'; lane APP-DESIGN.md:3425 G1 depends on 'A2, A4b, A9, C1b, D2, G0'; lane spikes/electronite/shell/main.cjs:422-432 shell:printPdf using printToPDF
  - rec: Remove D2 from G1's dependencies and add G3, G4 and G5 rows to APP-DESIGN §10 Phase 6 (ids must match status.py's [A-GJ]\d+ pattern).
- [S3] (high/high) Exchange (X with E1a, E1b, E2) should not be handed off: it needs tc-store actor paths that do not exist yet, inside code that S2 and the next journal-files machine are still rewriting.
  - evidence: grep 'commit_received' in tc-store/src on lane and s2 found nothing; s2 .audit/plans/S2-PROOF-2026-10-21.md:11-43 (new long-model failures on 2026-10-21); s2 .audit/plans/JOURNAL-MACHINE-ASSESSMENT.md:22-50 (next storage unit, 2-3 weeks estimated); git merge-tree lane s2: conflicts in tc-store handle.rs, hooks.rs, open.rs; s2 STORAGE.md §9.1 S1/S6/S7 and §10.2 R7 are [actor] steps
  - rec: Keep X, E1a, E1b and E2 with the main storage team.
- [S4] (high/high) Import can be split into a pure library (tc-import, with no tc-store dependency) and a store write step plus UI that stay with the main team; the pure part can start now.
  - evidence: lane tc-app/src/backend/seed.rs:1-31 (seals packs, folds and checks bytes in memory); lane tc-journal/src/change.rs:77-91 ImportSource variants; ids.rs:126-138 DeviceKind; Public tc-journal changes since 7e62455 are additive on lane and s2 (git diff of pub lines); lane APP-DESIGN §9.2 R29 owner I, 'In a task text: no'
  - rec: Make A′ the second package. The main team freezes ImportPlan and the TagResolver trait first, then builds Store::create_imported after S2 merges.
- [S5] (medium/medium) Unit I is scheduled after X, but only §10.2 R2 and §13.3 actually need tc-sync, so I-core could run in parallel and shorten the storage chain.
  - evidence: s2 .audit/workflows/storage-chain-B.js:35 (I task: 'tc-journal, tc-store and tc-sync are committed'); s2 STORAGE.md §13.1-13.5 need only tc-journal, tc-usfm and tc-burrito plus a store write
  - rec: Change I's prerequisites. The §13.3 team move and its bridge stay in X, calling tc_import::tc4::convert_segments.
- [S6] (high/high) Import's data risk is high and its reference is still moving, so it needs the main team's adversarial review even if someone else builds it.
  - evidence: PLAN.md 'Rigor': import is listed as high rigor; rebuild.tsv ruling 2026-10-18T13:25Z 'keep unit I lean'; tC4 tip diff d310bf3..b86270f: journalingStore.ts, seal.ts (5fefba87 segment limit 8 MiB), tc3.ts changed
  - rec: Merge gate for A′: P9 byte-for-byte tests, a determinism test, and a review by several models before merge.
- [S7] (low/high) Export (D2 plus the Burrito zip) is a poor handoff: it is small, waits for S2's checkpoint, touches the busiest tc-app and tc-ui files, and its Burrito zip is X's §10.1 export.
  - evidence: lane crates/tc-export/src is 750 lines; lane STATUS D2 waits for S2; s2 STORAGE.md §10.1 (the J7 Burrito export is the same file as the exchange export)
  - rec: Keep D2 in the main build lane.
- [S8] (high/high) CI never runs the browser journeys and never builds on macOS, so P1 and P8 are not yet checks that can fail.
  - evidence: lane .github/workflows/ci.yml: all jobs runs-on ubuntu-latest; the parity-baseline comment says browser tests skip; lane parity/app/journeys/kit.mjs:5-12 (skips without TC4_BIN, TC4_UI, Playwright); 30 journey files; Two lane files with target_os = "macos" (tc-app/src/config.rs, tc-app/tests/app/feedback.rs)
  - rec: Make app-e2e (G5) and macOS builds part of B. The app-e2e job must fail when any test is skipped.
- [S9] (medium/medium) The shell's buffer reduction has no shared vectors: the Playwright stub already differs from tc-ui's current_lines.
  - evidence: lane crates/tc-ui/src/app/buffer.rs:391-422 (a removal that names a text ends the entry only while it holds that text); lane crates/tc-ui/app/no-loss.mjs:213-229 Bridge.current() deletes on any removal
  - rec: Add fixtures/shell/buffer-vectors.json, checked by a Rust test and by the shell's node test.
- [S10] (medium/high) Most of the release pipeline can be ported from tC4, which already packages an Electronite app with a native server on three operating systems.
  - evidence: tC4 .github/workflows/package-desktop.yml:72,92,254 (ubuntu-24.04, macos-15, windows-2025), 588 lines; tC4 scripts/package-desktop.zsh 1115 lines, sign-windows.ps1 162, smoke-api.cjs 336; tC4 docs/PACKAGING.md:545-600 (Azure Artifact Signing via OIDC, Inno Setup), 333-345 (unsigned macOS arm64 pkg), 917 (Electronite sha256 pins)
  - rec: G3 ports tC4's pipeline into a new release.yml. Signing is used only after the owner decides.
- [S11] (medium/high) Branch and merge tracking only works if new units use G<n> ids and merge commits are titled 'Merge <id>:'.
  - evidence: lane .audit/tools/status.py:20-38 (deps regex [A-GJ]\d+[a-b]?|S1|S2|C|X|I|P); status.py:49-56 (done is read from 'Merge|Build <id>' commit subjects)
  - rec: Name B's units G1, G3, G4, G5. Keep I as one storage id, with its sub-deliverables tracked in .audit/ext/I-import.md.
- [S12] (medium/high) A protocol for outside sessions already exists and can be reused: work only on named branches, never push main or storage, main merges after checks.
  - evidence: lane .audit/tasks/windows-track.md 'Rules'; lane .audit/HANDOFF.md 'Merge into claude/focused-ride-5uzj7p in a separate worktree, run the full gates'
  - rec: The outside developer pushes only ext/* branches and merges main into them weekly. The main team runs every AGENTS gate plus the VM gates before merging.
- [S13] (medium/high) Edits to shared build files would cause conflicts: ci.yml, Cargo.toml and Cargo.lock already conflict between the two live branches.
  - evidence: git merge-tree --write-tree resume-build-lane s2-proof-executor: CONFLICT in .github/workflows/ci.yml, Cargo.toml, Cargo.lock
  - rec: B only adds new workflow files and never edits ci.yml or Cargo.*. The OS keychain item is optional and needs an agreed slot.
- [S14] (medium/high) The main team should fast-forward main to the lane before the handoff; otherwise the developer starts 89 commits behind the newest app.
  - evidence: git rev-list --count main..resume-build-lane = 89, and lane is 0 behind main
  - rec: Set the handoff base to main at ≥ f9067bc.

## Unverified
- Whether birchamp/tc-rst has GitHub Actions minutes for windows-2025 and macos-15 runners. Check the repository's billing and settings.
- Whether the rebuild may use uW's Azure Artifact Signing profile (tC4 D99), and whether an Apple Developer ID exists for notarization. Ask the owner or uW.
- Whether Electronite v37.1.0-graphite on darwin-arm64 shapes Graphite fonts. G0 covered only Windows and Linux. Run the spike probe on a Mac.
- All sizes are estimates from line counts of the spec, the tC4 sources and existing code. Nothing was measured, built or tested; builds were forbidden in this assessment.
- When S2 will merge into main, and how long the journal-files machine will take (the critics estimated 2-3 weeks). Read the s2 HANDOFF as it is updated.
- Whether tC4's post-d310bf3 changes (8 MiB segment limit, tc3.ts) affect the tC4 4.0 journal validator. Review git diff d310bf3..b86270f of src/data/journal and src/data/import.
- Whether the lane's next units (C7a, C7b, D1, D2, E1a, E1b, E2) will need new shell bridge calls beyond §5.7. This is inferred from their APP-DESIGN scope rows; ask the lane session.
- Whether s2's new tc-journal sidecar decoders (fold/decode.rs) duplicate tc-app/src/backend/seed/records.rs. Compare the two before tc-import moves the seed.