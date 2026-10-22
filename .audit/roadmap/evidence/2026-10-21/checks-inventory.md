# checks-inventory

## Summary
The Checks layer is in poor shape. The local gate runners work hard, but the one shared gate, GitHub CI, is not doing its job.

1. CI is not a working gate. Main's CI has 163 runs: 20 success, 79 failure, 64 cancelled. The last green run was a52bc0c on 2026-10-19. Main's tip af4eef3 fails at the Windows clippy step because `crates/tc-app/tests/op_counts/harness.rs:179` calls `std::os::unix::fs::symlink` with no `cfg(unix)` guard. So `cargo test --workspace` and the wasm clippy steps were skipped. Earlier failures came from a full runner disk and from new clippy lints on a floating `@stable` toolchain. resume-build-lane carries the same unguarded line.
2. The gate lists have drifted apart. The lane's GATES string, the merge-gate scripts and S2's run-gates.mjs all omit the Windows clippy step. The merge scripts also test only a few crates. Merges are committed first and gated after: B11's merge commit has no recorded pass.
3. No P item has a CI check that can fail on a translator-visible regression. The P1 journeys skip in CI (142 of 212 node tests skipped). P7 compares committed captures only. Rule 5 performance runs only on the storage branch. P5 validation is opt-in per test.
4. Several rules exist only in prose: the dependency and I/O rules, tests never touching the network, and shared dependency versions. All are cheap to enforce. Today they hold, with two exceptions: tc-door43-fake declares futures-util inline, and S2 turns on git2 `https` in tc-store.
5. Determinism and speed: the J15 tests assert 300–400 ms wall-clock windows, and proptest seeds are random, so census checks flake. A full test run takes about 899 s of sequential binaries; the tc-api round-trip binary alone takes 240–291 s.

Top new checks, ranked: get CI green and keep it green; one gate list shared by CI and every local runner; a CI journey job that fails on any skip; a journey coverage manifest; an architecture test built on cargo metadata; I/O and network boundary scans; a universal P5 validator; deterministic time and fixed proptest seeds.

# Checks layer: what exists, what is missing, and what to add

Snapshots: **R** = resume-build-lane (f9067bc), **M** = main (af4eef3), **S2** = s2-proof-executor (3b075d7), **ST** = storage (dd63328). Paths are relative to the snapshot root. Nothing was built or run, apart from `cargo metadata --no-deps --offline`, read-only `gh`/`git`/`journalctl` queries, and log parsing.

## 0. The state of CI (read this first)

| Fact | Evidence |
|---|---|
| Main's CI: 163 runs, 20 success, 79 failure, 64 cancelled | `gh run list --branch claude/focused-ride-5uzj7p --limit 200`, tallied by day |
| Which step failed, across the 79 failures: workspace clippy 58, `cargo test` 11, Windows clippy 6, no step recorded 4 | `gh run view <id> --json jobs`, all 79 |
| Last green run: a52bc0c, 2026-10-19T12:59Z | same listing, `select(.conclusion=="success")` |
| Tip af4eef3 fails at "clippy for Windows": `error[E0433]: cannot find unix in os --> crates/tc-app/tests/op_counts/harness.rs:179`. `cargo test` and the wasm steps were then **skipped** | run 39426828571 |
| The same unguarded line is in R (`harness.rs:179`, with no `cfg` in harness.rs or op_counts.rs) | grep in R |
| Older failures came from the disk: "No space left on device", "Free space left: 35 MB", `ld ... Bus error` | runs 39305042134, 39317068070 |
| Clippy failures were new lints, e.g. "using `chunks_exact` with a constant chunk size". The toolchain floats: "stable updated - rustc 1.99.1 (from rustc 1.99.0)" | runs 39160698131 and another failed run from 2026-10-15; no `rust-toolchain.toml` in any snapshot |
| `concurrency: cancel-in-progress: true` cancels 64 runs, so many commits never get a result | M `.github/workflows/ci.yml:7-9` |
| R and S2 are not pushed, so their checks run only locally | `git rev-parse origin/<b>` returns none |

## (a) Existing checks

D = deterministic, F = fast (under 30 s), B = tests behaviour, N = names the invariant on failure. ✓ yes, ~ partly, ✗ no.

| # | Check | Where | Runs in | D | F | B | N | Notes |
|---|---|---|---|---|---|---|---|---|
| 1 | `cargo fmt --check` | R ci.yml:20 | CI | ✓ | ✓ | n/a | ✓ | |
| 2 | Clippy, workspace, `-D warnings` (unsafe forbidden; unwrap/expect/panic/dbg/todo set to warn) | R ci.yml:21; Cargo.toml:15-23 | CI | ✓ | ✓ | n/a | ✓ | All 20 crates set `[lints] workspace = true`, but no check keeps it so |
| 3 | Root clippy.toml: proxy_watch bans (2 methods, 1 type) | R clippy.toml:7-13 | CI | ✓ | ✓ | n/a | ✓ | reason strings |
| 4 | tc-app clippy.toml: 19 disallowed methods (whole-fold walks, `mint`) | R crates/tc-app/clippy.toml:18-40 | CI | ✓ | ✓ | n/a | ✓ | Replaces the root file (its own lines 1-2 say so) |
| 5 | Whole-fold lint completeness, 3 tests | R crates/tc-app/tests/op_counts/lint.rs:191,209,233 | cargo test | ✓ | ✓ | ~ | ✓ | Parses source text (`impl FoldState {` at column 0, `    pub fn `), so it is brittle |
| 6 | Release-like clippy: tc-server+tc4 bins, tc-app lib | R ci.yml:24,27 | CI | ✓ | ✓ | n/a | ✓ | tc-store lib only on ST/S2 |
| 7 | Clippy for x86_64-pc-windows-gnu | R ci.yml:31-36 | CI only | ✓ | ✗ | n/a | ✓ | **Red on M.** Missing from every local gate list |
| 8 | wasm clippy for tc-ui (4 feature sets) and tc-api | R ci.yml:39-45 | CI | ✓ | ✓ | n/a | ✓ | Skipped on M's tip |
| 9 | `cargo test --workspace`: 3,230 tests, 196 binaries, 7 ignored, about 899 s summed | `.worktrees/resume-build-lane/.enjoy-logs/resume-c5-complete-full.log` | CI (not completed on M since 10-19) and local | ~ | ✗ | ✓ | ~ | About 25% of test asserts carry a message (heuristic count). Test names carry most of the meaning |
| 10 | Op-count tests, Psalms vs ten times larger: tc-app 31–45 tests (172–210 s), tc-journal 8 (20 s), tc-store 7 | R crates/tc-app/tests/op_counts/*, tc-journal/tests/operation_counts.rs, tc-store/tests/store/opcount.rs | cargo test | ✓ | ✗ | ✓ | ✓ | The best checks in the repo |
| 11 | Crash at every write point | R tc-store/tests/store/crash.rs:178, commands.rs:724 | cargo test | ✓ | ~ | ✓ | ✓ | Save and commands only. No sync or import crates yet |
| 12 | tc-journal properties: 18 tests, 17 s; never_panics, loss | R tc-journal/tests/ | cargo test | ~ (random seed) | ✓ | ✓ | ✓ | `resolution_always_possible` flaked 2 times in about 55 runs (R .audit/minors.tsv:55) |
| 13 | Store model: `random_histories_lose_no_work`, 200 cases; `_long`, 5,000 cases, every ledger row | S2 tc-store/tests/store/model.rs:2534,2542 | S2 cargo test; nightly only in S2 ci.yml | ~ | ✗ (store binary 344 s) | ✓ | ✓ | The nightly has **never run**: S2 is unpushed and scheduled workflows run only on the default branch |
| 14 | Wire format pinned | R tc-journal/tests/canonical.rs:264, tokens.rs:94, index.rs:867 | cargo test | ✓ | ✓ | ✓ | ✓ | Part of P10 |
| 15 | tc-api round trips and refusal of unknown keys | R tc-api/tests/roundtrip.rs:61,294,336 | cargo test | ~ | ✗ **240–291 s for 3 tests** | ✓ | ✓ | Fixed `cases: 128`, so `PROPTEST_CASES` cannot lower it |
| 16 | P7 parity gate: 48 tests, 18–19 s, with planted mutations | R crates/tc-ui/tests/screen_record.rs | cargo test | ✓ | ✓ | ~ | ✓ | Checks the **committed** captures only (130 rows: 33 same, 79 differ with a reason, 18 unported) |
| 17 | i18n keys, vendored assets, help pages | R crates/tc-ui/tests/i18n_keys.rs etc. | cargo test | ✓ | ✓ | ✓ | ✓ | |
| 18 | Release build has no test hooks | R crates/tc4/tests/release.rs:67 (runs), :119 (`#[ignore]`, needs TC4_RELEASE_BIN) | cargo test / local | ✓ | ✓ | ✓ | ✓ | The binary scan is local only |
| 19 | Server watchdog: never blocked | R tc-server/tests/never_blocked.rs | cargo test | ~ | ✓ | ✓ | ✓ | Allows for machine load |
| 20 | J15 progress by 300 ms | R tc-store/tests/store/progress.rs:44 (300–400 ms); tc-app/tests/app/store_scenario.rs:734 (300–420 ms) | cargo test | ✗ wall clock | ✓ | ✓ | ✓ | R minors.tsv:12 |
| 21 | alignment-budget: release p95 at most 3 × 50 ms | R ci.yml:47-53; tc-align/tests/suggestion_budget.rs:124 | CI (R only) | ~ | ✗ | ✓ | ✓ | The only budget in CI |
| 22 | parity-baseline node tests | R ci.yml:65-72 | CI | ✓ | ✓ | ✓ | ✓ | M run: 212 tests, **70 pass, 142 skipped** (135 because TC4_BIN is unset) |
| 23 | 29 journey files (Playwright) | R parity/app/journeys/*.test.mjs | local only | ~ | ✗ | ✓ | ✓ | `TC4_PERF=1` budgets (align.test.mjs:267, check-keyboard:227) are local only |
| 24 | Windows VM gates (fmt, clippy, test, wasm) | R .audit/tools/winvm/gates.ps1 | local VM | ✓ | ✗ (17–38 min) | ✓ | ~ | 12 runs; the last was 8d039a8 on 10-19 18:51 (journalctl). That is 71 commits behind M and 160 behind R. Results are not kept |
| 25 | perf job: `perfreport ci --tolerance 3` | ST ci.yml `perf` job | CI (ST only) | ~ | ✗ | ✓ | ✓ | Missing from M, R and S2 |
| 26 | run-gates.mjs and lane-state.mjs receipts | S2 .audit/tools | local | ✓ | n/a | n/a | ✓ | Their test checks its own list (S2 run-gates.test.mjs:39), not ci.yml |
| 27 | Mutation queue (10 cases + 6 mechanisms) | S2 .audit/plans/S2-MUTATION-PROOF.md:4 | manual | — | — | — | — | Its own words: "a queue, not proof that any mutant has been detected" |

## Rules stated only in prose (no check enforces them)

| Rule | Source | State today |
|---|---|---|
| tc-ui depends only on tc-api and tc-core | R docs/ARCHITECTURE.md:88 | Holds (cargo metadata) |
| Only tc-store writes project files; only tc-door43 opens network connections | ARCHITECTURE.md:89 | AGENTS.md:16 also allows tc-resources, so the two documents conflict. **On S2, tc-store links git2 with `https`** (S2 Cargo.toml:64; crates/tc-store/Cargo.toml:36,42) |
| Pure crates do no I/O | ARCHITECTURE.md:90-91 | Holds outside tests (grep). Example: tc-usfm's fs calls are only in `derivations/tests.rs` |
| tc-api: no I/O and no `std::time` | APP-DESIGN.md:277 | Holds (grep) |
| Tests never touch the network | AGENTS.md | Not enforced. CI runners have network |
| Shared dependency versions live in `[workspace.dependencies]` | AGENTS.md | **Broken:** tc-door43-fake/Cargo.toml:13 declares futures-util inline; the workspace has it at Cargo.toml:50 and tc-server uses the workspace entry |
| Every project written by any test is validated (P5, I7) | PLAN.md, STORAGE.md:83 | Opt-in: `assert_valid_burrito` is called in about 20 files |
| CI runs the budgets at 3× (rule 5) | PERFORMANCE.md:40-44 | Only on ST |
| An app-e2e CI job | APP-DESIGN.md §7.9 ("Later (G2)") | Not built |
| The tc-app clippy.toml mirrors the root's four keys | tc-app/clippy.toml:1-2 | Comment only |
| "Stable toolchain"; each commit builds | AGENTS.md | `@stable` floats; merge commits land before their gates (f07fd9e at 11:56:05, gates started 11:56:07, log has no `ALL_GATES_PASSED`) |

## Where CI can be green while a P item regresses

- **P1:** every journey skips in CI, and a skip counts as a pass.
- **P2, P9:** tc-sync and tc-import do not exist, so there is nothing to check.
- **P4:** only save and commands are crash-tested.
- **P5:** a writer whose test forgets to validate passes.
- **P7:** a UI change passes until someone captures again. captures.json says `build.commit: "dev"` and UI build `8a97c927…`. The lane worktree's newest UI build is `d591149e…` (target/tc-ui-app/ui, 10-21 12:11).
- **P8:** the Graphite probe is local only.
- **P10:** no frozen sample projects yet.
- **Performance:** only the alignment budget is checked.
- **Gate integrity:** local gates fake success in two ways. resume-full-gates.sh runs `cargo fmt --all`, which rewrites files instead of checking them. For unit c5 it also runs `UPDATE_REPORT=1` on screen_record before the gate checks that same report.

## Slow and timing-dependent tests

- Slowest binaries, R full run: tc-api roundtrip 240 s, tc-app op_counts 172 s, tc-app app 97 s (628 tests), tc-store 78 s, tc-usfm fixtures 74 s. The merge-b11 run had 291 s, 210 s and 123 s for the first three. S2's tc-store binary takes 344 s (356 tests).
- Sleeps: 114 `sleep(` calls in `crates/*/tests`, and 45 places where a fixed sleep of 10 ms or more is followed by an assert. Examples: tc-app/tests/app/replies.rs:302,506; tc-store/tests/store/tc4.rs:78. There are also 42 `start_paused` tests, which already use virtual time.
- Long waits: lifecycle tests wait 60 s before failing (tc-server/tests/lifecycle.rs:20; minors.tsv:2).
- No fixed proptest seed anywhere. proptest 1.12 reads `PROPTEST_RNG_SEED` (verified in `~/.cargo/registry/.../proptest-1.12.0/src/test_runner/config.rs:40`). Census asserts can flake: tc-journal/tests/common/mod.rs:848, tc-usfm/tests/census/mod.rs:56, tc-store/tests/store/consolidation.rs:95.
- 6 of R's 7 `#[ignore]` tests are never run by CI.

## (b) New checks, ranked

| Rank | Check (invariant) | How to build it | Cost | Failure message |
|---|---|---|---|---|
| 1 | **CI is green and trustworthy** | (1) Wrap harness.rs:179 in `#[cfg(unix)]` and give Windows an alternative, e.g. copy the dir. (2) Add `rust-toolchain.toml` with `channel="1.99.1"`, components rustfmt and clippy, targets wasm32-unknown-unknown and x86_64-pc-windows-gnu. (3) Add `--locked` to every cargo line. (4) Split `rust` into jobs `lint-linux`, `lint-windows`, `lint-wasm` and `test`, each with `timeout-minutes`. (5) Free disk first: `sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc`. (6) The owner makes `ci` a required check | 0.5 day | (the job's own error) |
| 2 | **One gate list** (local pass means CI pass) | Add `scripts/gates.txt`, one command per line. ci.yml, build-lane.js GATES, run-gates.mjs `defaultSteps()` and winvm/gates.ps1 all read it. Add `.audit/tools/gates-sync.test.mjs` (node, in the CI node job): parse the ci.yml `run:` lines and assert they equal gates.txt, ignoring `-j N` | 0.5 day | `gate drift: ci.yml runs "<cmd>" but scripts/gates.txt does not (or the reverse); a local pass would not be a CI pass` |
| 3 | **Merge only on a receipt for the exact tree** | merge-main.js and build-lane.js refuse to update main unless run-gates.mjs has a passing receipt for `git rev-parse HEAD^{tree}` covering every line of gates.txt | 0.5 day | `refusing merge: no passing receipt for tree <sha> (missing: <steps>)` |
| 4 | **P1 runs in CI, with no skips allowed** | Add an `app-e2e` job: `cargo build -p tc4 --features test-hooks,fixture-backend`, `cargo build -p tc-door43-fake`, `crates/tc-ui/build-app.sh` (and `--test-hooks`), `npx playwright@1.56.1 install chromium`, set TC4_BIN/TC4_UI/TC4_UI_HOOKS/TC4_DOOR43_FAKE/PLAYWRIGHT_MODULE, then `node --test --test-reporter=tap parity/app/journeys/*.test.mjs`. Fail if the TAP `# skipped` count is above 0. Gate the DOM, meaning and behaviour layers here; pixels only on the baseline machine (§7.9) | 2 days; CI minutes | `P1: 135 journey tests skipped in app-e2e (set TC4_BIN …); every journey must run here` |
| 5 | **Every P1 journey is mapped** | Add `parity/app/journeys/coverage.json`: each P1 id (J1–J8, J9a, J9c, J9d, J11–J13, J16–J18, J20–J26) and each tC4 spec (37 at d310bf3; the tip adds compare-card and sync-refusal) maps to a file and test titles, or to `{pending:"<unit>"}`. A node test checks each file and title exists, and that a pending unit is not already in units.json `landed` | 0.5 day | `P1: J26 is pending on E1a, but units.json says E1a landed; add its journey` |
| 6 | **Architecture rules** (ARCHITECTURE.md:86-91, AGENTS.md) | New crate `crates/tc-arch` (publish=false) with `tests/architecture.rs`. It runs `$CARGO metadata --no-deps --format-version 1 --offline` and asserts: (1) an allowed-edges table per crate; (2) `reqwest` as a normal dependency only in tc-door43 and tc-resources; (3) git2 `https` only where the owner allows; (4) every member uses edition 2024 and `lints.workspace = true` (read each Cargo.toml); (5) an external dependency that `[workspace.dependencies]` lists is never given an inline version. Today it would fail on tc-door43-fake's futures-util and, after S2, on tc-store's git2 https | 0.5 day | `ARCHITECTURE.md:88: tc-ui may depend only on tc-api, tc-core; crates/tc-ui/Cargo.toml adds tc-store` |
| 7 | **I/O boundaries** (pure crates do no I/O; only tc-store writes project files) | `crates/tc-arch/tests/io_boundaries.rs` scans `crates/{tc-core,tc-usfm,tc-versification,tc-helps,tc-journal,tc-check,tc-align,tc-obs,tc-export,tc-api}/src` (skipping `tests.rs` and `#[cfg(test)]` files) for `std::fs`, `std::net`, `tokio::`, `reqwest`, `SystemTime::now`, `Instant::now`, `std::process`, `std::env::var`. A second scan covers all other `src` for write calls (`fs::write`, `File::create`, `OpenOptions`, `fs::rename`, `remove_*`, `fs::copy`, `create_dir_all`, tokio::fs) against `crates/tc-arch/allow/fs_writes.txt` (file + reason; about 12 files today, listed in §grep) | 0.5 day | `ARCHITECTURE.md:90: pure crate tc-check does I/O at src/x.rs:12 (std::fs::read): pass bytes in from tc-app` |
| 8 | **Tests never reach the network** | In the CI `test` and `app-e2e` jobs, after `cargo fetch` and before the tests: `sudo iptables -A OUTPUT ! -o lo -j REJECT; sudo ip6tables -A OUTPUT ! -o lo -j REJECT`. Remove both rules before the rust-cache post step | 1 h | (the test's own connect error) plus a canary test asserting `TcpStream::connect("1.1.1.1:443")` is refused when `TC_NO_NET=1` |
| 9 | **P5: every written project is valid** | In the tc-store and tc-app test support (`FaultFs` / the Rig in tests/store/common.rs:211, tc-app tests/support/store_world.rs:331), add a Drop guard that runs `tc_burrito::validate_project` on every project folder written in the test, unless a write group is open or the test opts out with a named reason | 1–2 days | `P5/I7: after <test>, <folder> is not a valid Scripture Burrito: <problems>` |
| 10 | **Deterministic time** (rule 4 without the wall clock) | Code layer: inject a monotonic `Ticker` into tc-store progress.rs:109,161 and actor.rs, as `ManualClock` already does for wall time (tc-store/src/clock.rs:26-39). Rewrite progress.rs:44 and store_scenario.rs:734 to use virtual time. Replace fixed sleep-then-assert with Notify/barrier or `start_paused` | 2 days | `J15: first report at virtual 250 ms; rule 4 requires 300 ms` |
| 11 | **Fixed proptest seeds in CI; random seeds nightly** | Set `PROPTEST_RNG_SEED=<fixed>` in the CI test env. Add a nightly job that runs `cargo test` three times with random seeds and prints each seed. Move census asserts to the nightly job, or keep them seeded | 2 h | `nightly seed 9137: tc-journal resolution_always_possible failed; reproduce with PROPTEST_RNG_SEED=9137` |
| 12 | **Test-time budget** | A CI step parses `test result: … finished in Xs` and fails when a binary takes over 300 s. Fixes: tc-api roundtrip with `cases` from env (32 default, 1024 nightly) or `[profile.dev.package.{tc-api,proptest,serde_json}] opt-level=2`; shard the `test` job by crate group | 0.5 day | `slow test binary: tc-api tests/roundtrip.rs took 291 s (limit 300)` |
| 13 | **Rule 5 performance in CI** | Port ST's `perf` job (`perfreport ci --tolerance 3`, `needs: rust`, 90 min) when unit P lands; whole-Bible runs nightly. Run journeys with `TC4_PERF=1` in app-e2e as a nightly job | 1 day after P | `perf: save_verse p95 81 ms > 3×25 ms on CI` |
| 14 | **P7 captures match the source** | capture-rust.mjs writes `git rev-parse HEAD:crates/tc-ui` and `HEAD:crates/tc-api` into captures.json `build`. A screen_record test compares them with the current trees | 0.5 day | `P7: captures taken at tc-ui tree <a>, now <b>: run capture-rust.mjs` |
| 15 | **Doc-claimed tests exist** | A test parses docs/TC4-ISSUES.md rows with status `covered` and checks that each backticked test name without "to add" exists as a Rust `fn` or a node test title. The heuristic count today is 23 missing. Extend the check to PERFORMANCE.md and STORAGE.md | 0.5 day | `TC4-ISSUES.md #568 is "covered" by align_by_keyboard_only, but no such test exists` |
| 16 | **Clippy lints for AGENTS rules** | Add to `[workspace.lints.clippy]`: `allow_attributes_without_reason="warn"` (the "comment that proves the invariant" rule), `struct_excessive_bools="warn"` and `fn_params_excessive_bools="warn"` (no bags of booleans). A test asserts every `crates/*/clippy.toml` contains the root's keys | 0.5 day plus a sweep (count unknown) | clippy's own text plus the reason |
| 17 | **cargo-deny** | Add `deny.toml`: licences compatible with GPL-2.0-or-later, advisories, crates.io as the only source, bans. Run it as its own job (it needs network) | 2 h | `license MIT AND Apache-2.0 … rejected` |
| 18 | **Bless and ignore hygiene** | A CI step fails if UPDATE_REPORT, UPDATE_GOLDEN, UPDATE_PREPARE or TC_JOURNAL_BLESS is set. `cargo test -- --list --ignored` is checked against an allowlist naming the job that runs each ignored test | 1 h | `ignored test release_has_no_test_hooks has no CI job` |
| 19 | **Windows tests in CI** | A nightly `windows-latest` job runs `cargo test --workspace --locked`, or the VM posts a commit status with its gates.ps1 summary | 1 day | gates.ps1 summary |
| 20 | **Mutation testing** | A nightly `cargo mutants` run on tc-journal fold and the tc-store ledger and placement, against a committed list of known misses. Carry out the S2 queue first | 2 days; nightly | `new surviving mutant in tc-store/src/ledger.rs:<line>` |

Funnel note: ranks 6 and 7 are stopgaps. The top layer would be a typed capability: a `ProjectWriter` that only tc-store can build, and a `Net` handle that only tc-door43 can build. Then the wrong call does not compile. Likewise the source-parsing lint.rs (row 5) would be unnecessary if whole-fold walks required a token type that only `CountingSnapshot` holds.

Integration: R and S2 conflict in `.github/workflows/ci.yml`, Cargo.toml, Cargo.lock and the .audit workflows (`git merge-tree --write-tree resume-build-lane s2-proof-executor`). The merged ci.yml must keep the union of jobs: R's Windows, wasm, tc-app/tc-server lib, alignment-budget and journey glob; S2's nightly store model and `.audit/tools/*.test.mjs`; ST's perf job.

## Findings
- [C1] (critical/high) GitHub CI on main is red and has not completed `cargo test --workspace` since a52bc0c (2026-10-19 12:59Z). The tip af4eef3 fails at Windows clippy, so the tests and wasm clippy were skipped.
  - evidence: gh run list (branch claude/focused-ride-5uzj7p, 163 runs): success 20, failure 79, cancelled 64; gh run view 39426828571: 'clippy for Windows' failure; 'cargo test --workspace' skipped; error[E0433] cannot find `unix` in `os` --> crates/tc-app/tests/op_counts/harness.rs:179:14; Failing step across the 79 failures: workspace clippy 58, cargo test 11, Windows clippy 6, none recorded 4; Runs 39305042134 and 39317068070: 'No space left on device', 'Free space left: 35 MB', ld Bus error
  - rec: Guard harness.rs:179 with cfg(unix); split the rust job and free runner disk; pin the toolchain in rust-toolchain.toml; add --locked; make `ci` a required check.
- [C2] (high/high) The local gate lists do not match CI. None includes the Windows-gnu clippy step, and the merge scripts test a subset of crates. So commits that CI rejects pass locally.
  - evidence: R .audit/workflows/build-lane.js:21 GATES has no x86_64-pc-windows-gnu step; S2 .audit/tools/run-gates.mjs defaultSteps() has 10 steps, none for Windows; run-gates.test.mjs:39 asserts its own list, not ci.yml; merge-b11-gates.sh tests only -p tc-api -p tc-app -p tc-ui and 4 hand-picked journeys; resume-full-gates.sh runs `cargo fmt --all` (rewrites files) and, for c5, UPDATE_REPORT=1 before the screen_record gate
  - rec: Keep one scripts/gates.txt read by ci.yml, build-lane.js, run-gates.mjs and gates.ps1, plus a node test asserting ci.yml equals it.
- [C3] (high/high) Merges reach the lane before their gates run, and the B11 merge has no recorded gate pass.
  - evidence: git: f07fd9e (Merge B11) at 2026-10-21 11:56:05; merge-b11-gates.log starts 'start Wed Oct 21 11:56:07' and has no ALL_GATES_PASSED line; no process still running at 13:05; 1a5d5d1 (Merge C4) at 10:26:40; merge-c4-gates.log started 10:27:25 and also has no pass line; R .audit/plans/HANDOFF-resume-lane.md:20 'B11's post-merge records ... then one full gate'
  - rec: The merge step requires a passing run-gates receipt for the exact merge tree SHA before it updates main.
- [C4] (critical/high) P1 has no CI check that can fail: every journey browser test skips in CI.
  - evidence: CI run 39426828571 parity-baseline: tests 212, pass 70, skipped 142 (135 'set TC4_BIN'); R parity/app/journeys/kit.mjs:23 returns skip when TC4_BIN or TC4_UI is unset; R docs/APP-DESIGN.md §7.9 'Later (G2): an app-e2e job' has not been built; R ci.yml:65-72 has no build of tc4, tc-door43-fake or the wasm
  - rec: Add an app-e2e job that builds tc4 with test-hooks,fixture-backend, the fake and the wasm, installs Playwright 1.56.1, runs every journey, and fails if TAP '# skipped' > 0. Add coverage.json mapping each P1 journey id and tC4 spec to tests.
- [C5] (high/high) The ARCHITECTURE and AGENTS dependency, I/O and network rules exist only in prose. They hold today except for one shared-dependency violation, and S2 adds git2 https to tc-store, which conflicts with ARCHITECTURE.md:89.
  - evidence: cargo metadata --no-deps on R: tc-ui deps are tc-api and tc-core; reqwest only in tc-door43 and tc-resources; R crates/tc-door43-fake/Cargo.toml:13 declares futures-util inline, while Cargo.toml:50 has it in [workspace.dependencies]; S2 Cargo.toml:64 sets git2 features [https, vendored-libgit2]; S2 crates/tc-store/Cargo.toml:36,42; R ARCHITECTURE.md:89 says only tc-door43 reaches the network; AGENTS.md:16 also names tc-resources
  - rec: Add crates/tc-arch/tests/architecture.rs (cargo metadata: allowed edges, network-capable deps and features, lints.workspace, edition, workspace versions) and io_boundaries.rs (source scan with an allowlist). The owner decides where git network code lives.
- [C6] (medium/high) Nothing stops tests from touching the network, and the CI runners have open network.
  - evidence: AGENTS.md: 'Tests never touch the network' has no matching check in ci.yml; R ci.yml has no network restriction step
  - rec: In the CI test jobs, block egress with iptables/ip6tables `OUTPUT ! -o lo -j REJECT` after cargo fetch, and remove the rules before the cache post step.
- [C7] (high/high) P5 validation is opt-in per test, so a new writer whose test forgets to validate passes CI.
  - evidence: R assert_valid_burrito is defined in tc-store/tests/store/common.rs:211 and tc-app/tests/support/store_world.rs:331 and called explicitly in about 20 files; PLAN.md P5 says 'Every project state written by any test is validated'
  - rec: Add a Drop guard in the FaultFs/Rig test support that validates every written project folder unless a write group is open or the test opts out with a reason.
- [C8] (medium/high) The J15 progress tests assert wall-clock windows, and fixed sleeps are used for synchronisation. These checks are not deterministic.
  - evidence: R tc-store/tests/store/progress.rs:44-80 asserts 300<=after<=400 ms with real Instant; R tc-app/tests/app/store_scenario.rs:734 asserts 300..=420 ms; R .audit/minors.tsv:12 calls it not deterministic; R tc-store/src/progress.rs:109,161 uses Instant::now with no injectable ticker; clock.rs injects only wall time; 114 sleep( calls in crates/*/tests; 45 sites with a fixed sleep of 10 ms or more followed by an assert
  - rec: Inject a monotonic Ticker into tc-store progress and actor and test on virtual time; replace sleep-then-assert with Notify, barriers or start_paused.
- [C9] (medium/high) Proptest runs with random seeds, and the census assertions make property tests flaky.
  - evidence: No rng_seed or PROPTEST_RNG_SEED anywhere in R crates (grep); proptest 1.12.0 config.rs:40 reads PROPTEST_RNG_SEED; R .audit/minors.tsv:55: resolution_always_possible failed 2 of about 55 runs, 'the generator never reached'; Census asserts at tc-journal/tests/common/mod.rs:848, tc-usfm/tests/census/mod.rs:56, tc-store/tests/store/consolidation.rs:95
  - rec: Use a fixed PROPTEST_RNG_SEED in CI and a nightly run with random seeds that prints each seed.
- [C10] (medium/high) The test suite is slow: about 899 s of sequential binaries, with three binaries over 90 s and S2's store binary at 344 s.
  - evidence: resume-c5-complete-full.log: 196 binaries, 3,230 passed, summed 898.7 s; tc-api roundtrip 240 s (3 tests), tc-app op_counts 172 s, tc-app app 97 s; merge-b11-tests.log: roundtrip 291.51 s, op_counts 210.22 s, app 123.22 s; S2 s2-merge-proof-tests.log: tc-store binary 344.35 s for 356 tests; R tc-api/tests/roundtrip.rs:23 fixes cases: 128
  - rec: Fail CI when a binary takes over 300 s; take roundtrip cases from env (32 default, 1024 nightly) or raise opt-level for tc-api, proptest and serde_json; shard the test job by crate. Do not move op_counts to nextest: it shares one corpus per process.
- [C11] (high/high) The rule 5 performance CI job exists only on the storage branch, so M, R and S2 check only the alignment budget.
  - evidence: ST ci.yml perf job runs `perfreport ci --tolerance 3`; crates/tc-corpus/examples/perfreport exists only in ST; R ci.yml:47-53 alignment-budget is the only budget job; M has none; R docs/PERFORMANCE.md:40-44 rule 5
  - rec: Port ST's perf job when unit P lands; run whole-Bible and TC4_PERF=1 journeys nightly.
- [C12] (medium/medium) The P7 gate checks only the committed captures. Nothing ties those captures to the current UI source.
  - evidence: R crates/tc-ui/tests/screen_record.rs:37-60 rebuilds the report from the committed captures; R parity/app/captures.json build = {ui: 8a97c92765d47ed3, commit: 'dev'}; the newest lane UI build is d591149efce648fb (target/tc-ui-app/ui, 10-21 12:11); report.json totals: 130 rows, 33 same, 79 differs, 18 unported
  - rec: Record the tc-ui and tc-api git tree hashes in captures.json and fail screen_record when they differ from HEAD's.
- [C13] (medium/high) Windows tests run only on a local VM. Their results are not kept, and the last run was on 8d039a8.
  - evidence: journalctl --user: 12 tc-wingate runs, last 2026-10-19T18:51 on 8d039a85df; it is 71 commits behind main and 160 behind resume-build-lane (git rev-list --count); R .audit/HANDOFF.md:54 'reports only problems'; the journal has start and stop lines only
  - rec: Add a nightly windows-latest cargo test job, or have the VM post a commit status.
- [C14] (medium/medium) The docs claim test coverage that may not exist: 'covered' rows in TC4-ISSUES.md name tests that are not in the code.
  - evidence: Heuristic scan of R docs/TC4-ISSUES.md: 103 covered rows; 23 named tests without 'to add' found neither as a Rust fn nor as a node test('...') title (e.g. #568 align_by_keyboard_only); Similar scan: STORAGE.md 62 and APP-DESIGN.md 29 backticked test-like names not found as fns
  - rec: Add a test that parses covered rows and requires each named test to exist; extend it to PERFORMANCE.md and STORAGE.md.
- [C15] (medium/high) S2's model-based nightly and its mutation proof give no CI protection yet.
  - evidence: S2 ci.yml store-model-nightly is a `schedule` job, and scheduled workflows run only on the default branch; S2 is unpushed (no origin/s2-proof-executor); S2 .audit/plans/S2-MUTATION-PROOF.md:4 'This is a queue, not proof that any mutant has been detected'; git merge-tree resume-build-lane s2-proof-executor: CONFLICT in .github/workflows/ci.yml, Cargo.toml, Cargo.lock
  - rec: When integrating, merge ci.yml as the union of R, S2 and ST jobs. Run the mutation queue and record the results. Later add a nightly cargo-mutants run on the fold and the ledger.

## Unverified
- No clippy or test run was possible (hard rule), so the number of new violations each proposed lint would raise (allow_attributes_without_reason, excessive_bools, disallowed std::fs) is unknown. Check by running `cargo clippy --workspace --all-targets -- -W clippy::allow_attributes_without_reason -W clippy::struct_excessive_bools` once the heavy-test slot is free.
- resume-build-lane's CI result is inferred, not observed: the branch is unpushed. The same unguarded symlink at crates/tc-app/tests/op_counts/harness.rs:179 makes a Windows-clippy failure very likely. Confirm with `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu` in a free slot.
- Windows VM gate outcomes are not recorded anywhere visible (journalctl has start and stop lines only). Ask the session that runs the VM, or rerun .audit/tools/winvm/run.sh on af4eef3.
- Whether the B11 merge gates (merge-b11-gates.log) passed or were cut off by the usage limit: the log simply ends mid-journeys at 12:12. The lane session should rerun its full gate.
- The 23 'covered' TC4-ISSUES test names counted as missing come from a heuristic. Some may be journey step titles registered by kit.mjs under another form. Check each by grepping parity/app/journeys.
- Whether captures.json's UI build hash differs from the current UI because the UI changed or only because of TC4_BUILD_COMMIT in the build hash. Rebuild with the same TC4_BUILD_COMMIT and compare.
- Whether the iptables egress block and the disk-freeing steps work on the current GitHub ubuntu-latest image: standard practice, but not tested here.
- The fixed-sleep count (45) and the assert-message share (25%) come from regex heuristics in /tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/checks/asserts.py and grep; they are approximate.