---
id: CHK-4
title: Deterministic and fast tests
track: A
lane: checks
milestone: M1
state: fixing
owner: checks lane (Claude Code 2198ee19)
branch: chk-4-fix
worktree: .worktrees/chk-4-fix
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (cut to the flaky waits by the ruling of 2026-10-23)
merged: a522eadd
updated: 2026-10-23
next: merge the fix (still this lane's turn), then INT-8
blocked_by: none
---

# CHK-4: Deterministic and fast tests

## Goal

Cut by the ruling `rulings/2026-10-23-tests-prove-behavior-not-speed.md`: a test asserts what the
app does, never how fast. This card raises or removes the short waits in the tests that have
flaked under load, so a wait only catches a hang (about a minute) and says so, and nothing more
until a real flake appears. Virtual time, fixed seeds and per-binary time budgets are not built.
The steps below the ruling line are the old plan, kept as history.

## Why it matters to a translator

A translator never sees it. It keeps the checks that protect their work trustworthy: a flaky
test teaches people to rerun until green, and a slow suite makes people skip it.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| (INT-4|S2) '`. INT-4 must be `done`. Step 5
  edits `crates/tc-store`: do it only when S2 is `done`; until then do steps 1-4 and 6 and park
  with `next: do step 5 after S2 merges`.
- `scripts/gates.sh --list` prints the steps; the step `test` is
  `cargo test --locked --workspace`. CI's job `test` runs `scripts/gates.sh --step test`.
- No seed is fixed: `grep -rn PROPTEST_RNG_SEED crates scripts .github` prints nothing.
  proptest 1.12 reads `PROPTEST_RNG_SEED` in `Config::default()`
  (`~/.cargo/registry/src/*/proptest-1.12.0/src/test_runner/config.rs:40,139`).
- The J15 wall-clock windows: `crates/tc-store/tests/store/progress.rs:44-74` (300-400 ms) and
  `crates/tc-app/tests/app/store_scenario.rs:733-766` (300-420 ms); `.audit/minors.tsv` line 12
  (C1a) calls them not deterministic. `crates/tc-store/src/progress.rs:84-161` times reports with
  `Instant::now`; `crates/tc-store/src/clock.rs` injects only wall time (`Clock`, `ManualClock`).
- The census assertions: `crates/tc-journal/tests/common/mod.rs:846-849`,
  `crates/tc-usfm/tests/census/mod.rs:54-57`, `crates/tc-store/tests/store/consolidation.rs:93-96`.
  `resolution_always_possible` failed 2 of about 55 runs on one (minors.tsv line 55, C6).
- `crates/tc-api/tests/roundtrip.rs:23` fixes `cases: 128`; that binary took 240-291 s
  (checks-inventory C10).
- `grep -rnE 'sleep\(Duration::from_(millis\([1-9][0-9]+|secs)' crates/*/tests | wc -l` prints 63
  on resume-build-lane at f9067bc (the evidence counts 45 that an assert follows).
- `crates/tc-server/tests/lifecycle.rs:20` `BROKEN_AFTER` is 60 s; `patient_server` (line 125)
  uses it as the drain and close limits too (minors.tsv line 2, FIX-quit).

## Funnel card

- **Invariants**:
  - INV-DETERMINISTIC: a gate run's verdict depends only on the tree: proptest runs from the
    fixed `PROPTEST_RNG_SEED` the gate script sets; no test compares elapsed real time with a
    window; no test sleeps and then asserts a state it could wait for.
  - INV-FAST: no test binary in the gate step `test` runs longer than 300 s.
- **Wrong states**:
  - CI fails on a busy runner because the first progress report came at 421 ms.
  - A census assert fails one run in forty and people learn to rerun.
  - The tc-api round trip grows past 300 s and the whole job times out.
- **Code**:
  - Types: `Ticker` (to create) in `crates/tc-store/src/clock.rs`: monotonic time the store reads
    for progress, with `SystemTicker` (today's `Instant`) and `ManualTicker` (moves only on
    `advance`; `advance` returns after every waiter whose deadline it passed has run). Injected
    through `StoreConfig`, as `clock` is.
  - One entry point: `crate::progress::start` takes the ticker; `open_project`
    (`crates/tc-store/src/open.rs:201` on S2) passes the store's.
  - Removed: the wall-clock windows of both J15 tests; the fixed `cases: 128`.
  - Left to checks, and why a type cannot carry it: whether a test sleeps or reads the clock is a
    property of test code; the time limit is a property of a run.
- **Checks**:
  - `open_shows_determinate_progress_by_300_ms` rewritten on `ManualTicker` (to change) in
    `crates/tc-store/tests/store/progress.rs`; kind: unit; runs in: CI job `test`.
  - Gate step `test-budget` (to create): `.audit/tools/test-budget.mjs` reads the `test` step's log
    and fails on a binary over 300 s; its tests `.audit/tools/test-budget.test.mjs` (to create;
    the gate step `node` runs them); kind: lint over a run.
  - `the_gates_fix_the_proptest_seed` in `.audit/tools/test-budget.test.mjs` (to create, step 1):
    reads `scripts/gates.sh` and `.audit/tools/winvm/gates.ps1` and fails unless each sets
    `PROPTEST_RNG_SEED` before any `cargo test` line (INV-DETERMINISTIC's seed half); kind:
    lint over a file; runs in: the gate step `node`.
  - Proof it can fail: a planted log line `finished in 301.00s` fails the budget test; a reverted
    ticker fix fails the progress test (step 5); deleting the `export PROPTEST_RNG_SEED` line
    fails the seed test (step 1).
- **Prose**: a comment above the seed in `scripts/gates.sh`: why it is fixed and how the nightly
  varies it (CHK-5).
- **Human**: none.

## Steps

### Step 1: Fix the proptest seed and make the census checks seed-proof

- Do: in `scripts/gates.sh`, after the blessing refusal, add
  `export PROPTEST_RNG_SEED="${PROPTEST_RNG_SEED:-20261021}"` with the comment of the Prose line;
  in `.audit/tools/winvm/gates.ps1` add `$env:PROPTEST_RNG_SEED = '20261021'`. Write
  `the_gates_fix_the_proptest_seed` in `.audit/tools/test-budget.test.mjs` (create the file now;
  step 4's budget tests join it): it reads both gate scripts and fails unless `PROPTEST_RNG_SEED`
  is set before the first `cargo test`; prove it fails by deleting the line once. For each census
  assert (tc-journal and tc-usfm here; tc-store's in step 5) add a temporary
  `eprintln!("census {counts:?}");` just before the assert (the counts are printed today only
  when it fails), run its test with 20 seeds
  (`for s in $(seq 1 20); do PROPTEST_RNG_SEED=$s cargo test --locked -p tc-journal --test properties resolution_always_possible -- --nocapture 2>&1 | grep '^census'; done`),
  take the rarest state's share `p` per case, remove the `eprintln!`, and raise that property's cases until
  `(1-p)^cases` is below one in a million, as `VIEW_CASES` already does
  (`crates/tc-journal/tests/properties.rs:29-34`). If that makes its binary slower than 60 s,
  instead run the census assert only when `TC_CENSUS` is set (the nightly sets it, CHK-5).
- Files: `scripts/gates.sh`, `.audit/tools/winvm/gates.ps1`, the census test files.
- Done when: `scripts/gates.sh --step test` passes twice in a row; each changed property's
  measured miss chance is in the log.
- Record: a log line `step 1` with the seed, the census shares and the new case counts.

### Step 2: Fail a gate run when a test binary is too slow

- Do: in `scripts/gates.sh`, send each step's output also to
  `${CARGO_TARGET_DIR:-target}/gates/<name>.log` (`tee`, with `set -o pipefail` so the exit code
  is the step's). Write `.audit/tools/test-budget.mjs` (Node built-ins only): it pairs each
  `Running <path> (<binary>)` or `Doc-tests <crate>` line with the next
  `test result: ... finished in <s>s`, prints `INV-FAST: <path> took <s> s (limit <n> s)` for each
  binary over the limit, and exits 1 then, or when the log has no `test result:` line. Add
  `"test-budget|node .audit/tools/test-budget.mjs \"${CARGO_TARGET_DIR:-target}/gates/test.log\" 300"`
  to `STEPS`, and `- run: scripts/gates.sh --step test-budget` after the `test` step in CI's
  `test` job. Write `test-budget.test.mjs` with inline log samples (one at 301.00 s, one with no
  result line, one at 12.5 s). In `crates/tc-api/tests/roundtrip.rs:23` read the cases from
  `TC_API_ROUNDTRIP_CASES` (to create; default 32), and time the binary
  (`time cargo test --locked -p tc-api --test roundtrip`). If it still takes over 60 s, add
  `[profile.dev.package.tc-api]`, `[profile.dev.package.proptest]` and
  `[profile.dev.package.serde_json]` with `opt-level = 2` to `Cargo.toml`, with a comment that
  gives the measured times, as the profile entries there do.
- Files: `scripts/gates.sh`, `.audit/tools/test-budget.mjs`, `.audit/tools/test-budget.test.mjs`
  (to create), `.github/workflows/ci.yml`, `crates/tc-api/tests/roundtrip.rs`, maybe `Cargo.toml`.
- Done when: `node --test .audit/tools/test-budget.test.mjs` passes, and fails when you make the
  script ignore the limit; `scripts/gates.sh --step test && scripts/gates.sh --step test-budget`
  passes; INT-3's `gates-sync.test.mjs` passes; the round-trip time before and after is in the log.
- Record: a log line `step 2` with the five slowest binaries and their times.

### Step 3: Replace sleep-then-assert with waits on a signal

- Do: list the sites: `grep -rnE -A4 'sleep\(Duration::from_(millis\([1-9][0-9]+|secs)' crates/*/tests`,
  leaving out `crates/tc-store` (step 5), tests under `#[tokio::test(start_paused = true)]`
  (virtual time already), and sleeps that only pace a fake and are not followed by an assert. For
  each other site: wait on the thing itself: a store hold (`hooks.wait_reached(Point::..., n)`,
  as `crates/tc-store/tests/store/tc4.rs` does with
  `hooks.wait_reached(Point::BeforeStagingRemoval, 1)`), `tokio::sync::Notify`, a `Barrier`, or a
  polling helper that returns as soon as the condition holds and fails after a 10 s bound with
  `INV-DETERMINISTIC: waited 10 s for <what>`. An assert that something has NOT happened yet after
  a sleep cannot become a wait: hold the other side at a named point instead, or keep the sleep
  with a comment that says why. Start with the files that have the most sites
  (`crates/tc-app/tests/app/sessions.rs` 8, `crates/tc-server/tests/never_blocked.rs` 6,
  `crates/tc-app/tests/app/replies.rs` 5, at f9067bc).
- Files: the test files named by the grep, outside `crates/tc-store`.
- Done when: the grep, filtered as above, lists only sites whose comment says why they stay;
  `cargo test --locked --workspace` passes three times in a row.
- Record: a log line `step 3` with sites fixed and sites kept.

### Step 4: Shorten waits a failing run really reaches

- Do: in `crates/tc-server/tests/lifecycle.rs`, give `patient_server` (lines 125-131) its own
  drain and close limits, 10 s, separate from `BROKEN_AFTER`, so a broken drain fails at the
  drain limit. For each other wait of 30 s or more (`grep -rnE 'from_secs\((3|4|5|6)[0-9]\)' crates/*/tests`,
  for example `crates/tc4/tests/seed.rs:95,175`, `crates/tc-app/tests/app/upgrades.rs:663`),
  check that a passing run returns at once; such a bound stays.
- Files: `crates/tc-server/tests/lifecycle.rs`.
- Done when: with the mutation of minors.tsv line 2 (`lifecycle::gate` refuses only in the
  closing phase), `time cargo test --locked -p tc-server --test lifecycle a_request_whose_body_arrives_during_the_drain_still_gets_its_reply`
  fails in under 15 s; with the mutation reverted it passes.
- Record: a log line `step 4` with both times.

### Step 5: Put the store's progress on a ticker and the J15 tests on virtual time

- Do: only when S2 is `done`. Add `Ticker`, `SystemTicker` and `ManualTicker` to
  `crates/tc-store/src/clock.rs`; a `ticker` field on `StoreConfig`; `progress::start` and its
  report loop (`progress.rs:84-161`) read and wait on the ticker instead of `Instant`. Add a
  FaultFs read delay that advances a `ManualTicker` instead of sleeping (beside
  `set_read_delay`, `crates/tc-store/src/fault.rs:722` on S2). Rewrite the three tests of
  `tests/store/progress.rs` on it: the first report's `at` lies in `[300 ms, 300 ms + one read
  delay]` of virtual time. In `crates/tc-app/tests/app/store_scenario.rs`, delete the
  `300..=420 ms` window and keep the order checks (determinate reports, phases forward, counts
  never fall), with a comment that tc-store's progress test owns the timing. Do steps 1 and 3 for
  `crates/tc-store/tests` too (`consolidation.rs:93-97`; the sleeps at `tests/store/tc4.rs:79,286`, S2 lines). Look at
  `src/actor.rs` (MATERIALIZE_AFTER) and `src/drafts.rs:155` (S2 lines): convert them only if a test
  asserts their timing; otherwise log them as a `note`.
- Files: `crates/tc-store/src/clock.rs`, `progress.rs`, `open.rs`, `store.rs` (StoreConfig),
  `fault.rs`; `crates/tc-store/tests/store/progress.rs`; `crates/tc-app/tests/app/store_scenario.rs`
  and every `StoreConfig { .. }` the compiler names.
- Done when: `cargo test --locked -p tc-store --test store progress::` passes 20 times in a row
  while another `cargo build` runs (load); with `report_loop` waiting 20 ms past the
  deadline the progress test fails naming the report time; `grep -n 'Instant' crates/tc-store/tests/store/progress.rs`
  prints nothing.
- Record: a log line `step 5` with the failing output.

### Step 6: Gate and hand on

- Do: `scripts/gates.sh`. Write a `note` for CHK-5: the seed, `TC_API_ROUNDTRIP_CASES=1024`
  and (if used) `TC_CENSUS=1` for the nightly job.
- Files: none.
- Done when: `scripts/gates.sh` ends with its pass line.
- Record: a log line `gate`; set `state: verifying` (or `parked` until step 5) and `next`.

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

Plus: the gate step `test-budget` passes; the whole `test` step passes twice in a row with the
fixed seed.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A census, timing or sleep fix shows the product is wrong (a report really comes late, a state
  really is reached only sometimes): that is a finding, not a test fix. Record it, and open a card
  if it harms a translator's data or view.
- Step 5 would edit `crates/tc-store` while S2 is not `done`.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/checks-inventory.md` "Slow and timing-dependent tests", ranks 10-12,
  findings C8, C9, C10
- `.audit/minors.tsv` lines 2, 12 and 55 (resume-build-lane)
- `FUNNEL.md` §5 "Rules for every check"; `units/INT-3.md` (the gate script)
- Size basis: checks-inventory ranks 10 (2 days), 11 (2 h) and 12 (half a day).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
- 2026-10-23 14:11 EDT | checks lane (Claude Code 2198ee19) | ruling | 3cd1ac6a | The owner's ruling of 2026-10-23 cuts this card to the flaky waits: no virtual time, no fixed seed, no per-binary budget; a wait only catches a hang, about a minute, and says "hung" | .audit/roadmap/rulings/2026-10-23-tests-prove-behavior-not-speed.md
- 2026-10-23 14:11 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-CHK-4 in .worktrees/track-CHK-4 from main 3cd1ac6a, ahead of INT-8's merge on the orchestrator's word, because main stays red on these waits. Scope: the two app-e2e journeys that flaked (a_focused_help_lights_its_words_in_the_passage, a_section_card_opened_after_a_reload_during_its_save_saves_each_verse), kit.mjs's 10 s default wait, upgrades.test.mjs's Settings wait, plan_cost_follows_affected_books' 120 s, the reset-hold and job-reporter waits, the HomeCard 5 s wait, and op_counts::story's record drift if it is a wait | python3 -I .audit/tools/board.py
- 2026-10-23 14:13 EDT | checks lane (Claude Code 2198ee19) | step A | none | Waits raised to hang-only bounds that say "hung": kit.mjs until() default 10 s to 60 s (covers the two app-e2e journeys and upgrades.test.mjs's Settings wait) and browserTest's per-test timeout 120 s to 600 s; plans.rs plan wait 120 s to 600 s; home.rs HomeCard 5 s to 60 s; jobs.rs reporter test steps paused time until the first report (60 s bound) instead of a fixed 300 ms and no longer asserts the report's time. op-count drift: the harness settle() now also waits for the app's jobs to end (60 s bound), since a job's record read entered a save's count under load; op-count asserts unchanged. The reset-hold test is not a wait but a race in the Reset test hook: minor recorded, test unchanged | git diff e47f5bba -- parity crates
- 2026-10-23 14:20 EDT | checks lane (Claude Code 2198ee19) | step B | none | ci.yml and app-e2e.yml: cancel-in-progress is false on main, true elsewhere. Main's runs on 3cd1ac6a (40001100232, 40001100230) and d293375a were cancelled by the next push, so main had no finished result while every merge waited for one | gh api repos/{owner}/{repo}/actions/runs?head_sha=3cd1ac6a
- 2026-10-23 14:25 EDT | checks lane (Claude Code 2198ee19) | interrupt | a156a38c | Usage limit. Targeted runs on 0e15cabd (shared target of .worktrees/track-INT-4): app home:: 21 ok, jobs::the_reporter ok, op_counts story 8 ok, plans ok, section ok, comments 3 ok; op_counts translate: 4 FAILED, all panicking at corpus.rs:190 (the shared corpus setup, not the settle change; check whether another run rebuilt the corpus in that target at the same time). Next: rerun `cargo test --locked -p tc-app --test op_counts translate` alone in this worktree's own target; if it passes, run scripts/gates.sh --wait-ci CHK-4 after INT-8 lands (or scripts/gates.sh), then merge. Main's runs on eaf809d1 were in progress | .enjoy-logs/chk4-tests.log
- 2026-10-23 16:48 EDT | checks lane (Claude Code 2198ee19) | step C | none | Main's ci on eaf809d1 failed on app::check_item::a_check_comment_typed_before_the_app_ended_is_offered_and_kept_beside_the_mark ("the comment is offered: OfferedDrafts { offers: [] }"). Not a short wait: the Draft reply comes before the draft writer puts the draft in the file (handle.rs submit), and STORAGE §5.14 calls a draft best effort, so a crash right after typing may lose it (allowed by the 2-minute ruling). The four check_item tests that crash after typing a comment now wait for the store, draft writer included (drafts_written, which calls wait_idle), before the crash; one replaced a 1.5 s sleep. In this worktree's own target: check_item:: 20 ok; op_counts translate 4 ok (the earlier 4 failures came from two runs sharing one target's corpus folder); fmt and clippy -p tc-app --all-targets clean. Owner rule recorded: one merge into main at a time (TRACKING.md, ROADMAP.md) | .enjoy-logs/chk4-tests-2.log
- 2026-10-23 16:58 EDT | checks lane (Claude Code 2198ee19) | verify | a5f87180 | Independent review (a fresh general-purpose agent, read only, ran the node tests). 1 Code: bounds and messages only, plus settle() and drafts_written. 2 Checks: no assertion weakened beyond the ruling; the 300 ms rule stays proven by two other jobs.rs tests. 3 Reach: test code and CI only. 4 Siblings: drafts.rs already waits after each draft; no other Draft-then-crash test can flake toward failure. 5 No loss: no product code changed. 6 Prose: comments give reasons. One blocker: settle() waited for every live job, so op_counts::align::align_save_costs_one_verse_while_model_training_is_held, which holds a Train job during cost(), would hang 60 s and fail; no majors; minors recorded | .audit/minors.tsv
- 2026-10-23 16:58 EDT | checks lane (Claude Code 2198ee19) | fix | none | settle() waits while live jobs exceed the test's held ones (App::holding), so a job held at a hold point on purpose is not waited for; relaunch's doc comment back on relaunch. op_counts align 5 ok (the held-training test among them), op_counts home 3 ok, app check_item:: 20 ok; fmt and clippy clean | .enjoy-logs/chk4-tests-3.log
- 2026-10-23 16:58 EDT | checks lane (Claude Code 2198ee19) | merge | a522eadd | Merge commit `Merge CHK-4: tests wait for what the app does, not for a deadline`, on the orchestrator's turn; gated by scripts/gates.sh --journeys in .worktrees/chk-4-merge and by CI on gate/CHK-4, then main fast-forwards and its CI must finish green before the turn passes | scripts/gates.sh --check-receipt
- 2026-10-23 18:22 EDT | checks lane (Claude Code 2198ee19) | fix | none | Main's own CI on 9982556a (runs 40030654585, 40030654998) failed on two races, not slowness, under the new 60 s bounds: (1) app::home::one_summary_job_runs_at_a_time waited for a_copy's HomeCard event, then b_copy's, but its helper discards other frames, so when the job finished b_copy first the second wait hung ("hung: no HomeCard event for b_copy in 60 s"); now cards_event waits for both in any order (the app promises no order). (2) section.test.mjs a_section_card_opened_after_a_reload_during_its_save_saves_each_verse checked the pill's visibility once while the reloaded page still loaded, found nothing, and went looking for Home's Titus button while the app restored Translate (Playwright's 30 s click timeout); it now waits for either before it chooses. No sibling of either race found (grep). home:: 21 ok; clippy clean. INT-8's local gate run was stopped; its merge is redone on this fix | gh run view 40030654585 --log-failed
- 2026-10-23 19:02 EDT | checks lane (Claude Code 2198ee19) | fix | none | CI on the fix merge e4914b59 (run 40033229973) still drifted three op-counts on the store by 1-2 app record reads (align_save_costs_one_verse 0 vs 1; story saving_a_reference 1 vs 2; translate save_layout 2 vs 4); local gates passed. Cause found in code: the session's event pump (session.rs pump) handles each store commit after the action returns and reads records to label it (writer_of, comment_target), on the counter op-counts read, so on a slow runner its reads fell on either side of the count. The harness settle() now also waits until the session's view has reached the store's newest seq (60 s bound, "hung"). All 45 op_counts pass with 8 test threads beside a 12-job workspace build | .enjoy-logs/opc-under-load.log (chk-4-fix-merge)
- 2026-10-23 19:36 EDT | checks lane (Claude Code 2198ee19) | fix | none | CI on the fix merge 16ff363a (run 40037285872) failed on app::hooks::reset_keeps_a_write_in_flight_so_its_retry_still_waits_for_it; local gates passed (receipt). Cause in the Reset test hook (hooks.rs, test-hooks only): it released the holds first and forgot settled replies later, so the held write could finish in between and be forgotten, and its retry ran again. Its own doc says a write still running at the reset is kept; forget_settled now runs before release_all. hooks:: 18 ok five times beside a 12-job build; replies:: 17 ok; clippy (all targets and the release-like lib) clean | crates/tc-app/src/hooks.rs
- 2026-10-23 20:13 EDT | checks lane (Claude Code 2198ee19) | fix | none | On merge 5afdc631 CI was fully green (ci 40040632243, app-e2e 40040632067), but the local journeys failed home.test.mjs a_card_reads_again_on_a_new_home_visit_not_on_a_redraw ("a preview reads no card", 1 !== 0): the Home summary job can finish after the tiles show and its card event makes the page read the card, inside the counted window. The journey now waits for the server's test/quiet (live jobs) and then the page before it counts, as internet-consent.test.mjs does | .enjoy-logs/gates-3.log (chk-4-fix-merge)
