---
id: CHK-2
title: Every written project is validated (P5)
track: A
lane: checks
milestone: M1
state: claimed
owner: checks lane (Claude Code 2198ee19)
branch: track-CHK-2
worktree: .worktrees/track-CHK-2
depends: [S2]
needs_owner: []
proves: [P5]
no_loss: false
size: 1-2 d
merged: none
updated: 2026-10-24
next: step 4
blocked_by: none
---

# CHK-2: Every written project is validated (P5)

## Goal

Every project state any test writes is checked as a Scripture Burrito, without the test asking.
In Rust, every store a `tc-app` test builds carries S2's writer probe and fails its test when a
written state is invalid. In the browser journeys, the kit asks the app to validate every
project folder after each journey. Both fail with one message that names P5.

## Why it matters to a translator

A project that is not a valid Burrito may not open in tC4 or another tool, and Door43 may refuse
it. Today a test that forgets to call `assert_valid_burrito` lets a writer that breaks the
format pass CI.

## Before you start

- `python3 -I .audit/tools/board.py | grep '^| S2 '` shows `done`.
- `grep -n 'pub fn check_burrito\|pub fn burrito_problems' crates/tc-store/src/hooks.rs` shows
  both (S2: lines 108 and 151 at 3b075d7). `grep -n 'probe_written_state' crates/tc-store/src/writer.rs`
  shows the writer probes after a job that wrote files, outside a write group and with no outside
  edit waiting (S2: `writer.rs:575-593`).
- `sed -n 23,42p crates/tc-store/tests/store/common.rs` shows S2's pattern: `Rig::drop` asserts
  `burrito_problems()` is empty unless the thread panics or the FaultFs is dead.
- `grep -n 'fn backend_over\|fn store_world_with\|pub hooks\|fn assert_valid_burrito' crates/tc-app/tests/support/store_world.rs`
  shows the tc-app test world: `backend_over` (line 72 at f9067bc) builds every store, and
  `StoreWorld` holds `hooks: Arc<Hooks>`. Nothing installs a probe there today.
- `grep -rn 'backend_over(\|Store::start(\|StoreBackend::start(' crates/tc-app/tests | wc -l`
  counts the store constructions (37 lines at f9067bc, the definition included;
  `tests/app/store.rs:27` calls `tc_store::Store::start` directly).
- The test routes today (`crates/tc-api/src/lib.rs:220-231`, test-hooks only): quiet, echo,
  crash, hold, release, fieldStore, fieldLog, fieldDraft, keychain, reset, lastEdit, script. Their
  handlers are `impl Handle<..> for App` in `crates/tc-app/src/hooks.rs` (Echo at line 703).
  There is no `test/validate` and no `test/inspect`.

## Funnel card

- **Invariants**:
  - INV-VALID-BURRITO (P5, STORAGE I7): every project state a test writes, outside an open write
    group, is a valid Scripture Burrito: metadata valid, every ingredient present with its listed
    md5 and size, every file under `ingredients/` listed.
- **Wrong states**:
  - A writer leaves `metadata.json` with an old md5 after an ingredient changed: tC4 or Door43
    then sees a damaged project.
  - A new writer (D2's exports, E2's USB files, X's sync intake, I's import) is tested only for
    its own fields, never as a Burrito.
  - A journey ends with the project folder invalid and CI stays green.
- **Code**:
  - Types: `BurritoWatch` (to create) in `crates/tc-app/tests/support/store_world.rs`,
    `#[must_use]`, holding the store's `Arc<Hooks>` and the `FaultFs`. Its `Drop` asserts no
    problems unless the thread panics, the FaultFs is dead, or the test called
    `stop("<reason>")` (which also swaps in a probe that reads nothing, as S2's
    `damaged_on_purpose` does). In `tc-api/src/test.rs`: `Validate` and `ValidateView`
    (to create).
  - One entry point: `backend_over` installs the probe and returns the watch with the backend;
    every store a `tc-app` test builds goes through it. `withApp` in
    `parity/app/journeys/kit.mjs` is the one place a journey ends.
  - Removed: nothing. The explicit `assert_valid_burrito` calls stay: they check states in the
    middle of a test (after a kill, before a reopen).
  - Left to checks, and why a type cannot carry it: validity depends on the bytes on disk.
- **Checks** (all to create):
  - `p5_probe_names_a_planted_bad_md5` and `p5_watch_fails_a_test_whose_store_reported_a_problem`
    in `crates/tc-app/tests/app/p5.rs`; kind: unit; runs in: the CI job that runs
    `cargo test --workspace`.
  - `every_store_a_tc_app_test_builds_is_watched` in the same file; kind: lint (source scan).
  - `test_validate_names_a_project_whose_ingredient_md5_is_wrong` in
    `crates/tc-app/tests/app/hooks.rs`; kind: contract; runs in: `cargo test --workspace`.
  - The kit's call after every journey body; kind: journey; runs in: G5's `app-e2e` job once G5
    has merged, locally until then.
  - Proof it can fail: the seeded tests above, plus one mutation (step 3) that makes the store
    write a wrong md5; record the failing output.
- **Prose**: none.
- **Human**: none.

## Steps

### Step 1: Give every tc-app test store a watch

- Do: in `crates/tc-app/tests/support/store_world.rs` add `BurritoWatch`. `backend_over` calls
  `hooks.check_burrito(...)` with a probe over the FaultFs (copy S2's `burrito_problems` from
  `crates/tc-store/tests/store/common.rs:229-244`: read the folder's files, leave out `.tc/`, run
  `tc_burrito::validate_project`; `assert_valid_burrito_files`, line 336, then calls it instead of
  its own copy) and returns `(StoreBackend, BurritoWatch)`. `StoreWorld` gets
  `pub watch: BurritoWatch`. The `Drop` message is exactly
  `P5/INV-VALID-BURRITO: after <test>, <folder> is not a valid Scripture Burrito: <problems>`,
  with `<test>` from `std::thread::current().name()`; the probe records the folder with each
  problem. Update every caller (`let (backend, watch) = backend_over(...)`, `watch` kept until the
  test ends) and route `tests/app/store.rs:27` through `backend_over`. Write
  `every_store_a_tc_app_test_builds_is_watched`: it fails when a file under `crates/tc-app/tests`
  other than `support/store_world.rs` contains `StoreBackend::start(` or `Store::start(`, or binds
  the watch to `_` (`backend_over(` on a line with `, _)`).
- Files: `crates/tc-app/tests/support/store_world.rs`, the callers the compiler names,
  `crates/tc-app/tests/app/p5.rs` (to create; add `mod p5;` where `tests/app.rs` lists modules).
- Done when: `cargo test -p tc-app --test app p5::` passes and
  `cargo clippy -p tc-app --all-targets -- -D warnings` passes.
- Record: a log line `step 1`.

### Step 2: Prove the watch can fail

- Do: in `tests/app/p5.rs` write `p5_probe_names_a_planted_bad_md5` (build a world; overwrite one
  ingredient's `md5` in `/projects/<folder>/metadata.json` through `world.fs`; the probe function
  of step 1 returns a problem whose text contains `ChecksumMismatch`) and
  `p5_watch_fails_a_test_whose_store_reported_a_problem`
  (`#[should_panic(expected = "P5/INV-VALID-BURRITO")]`: install a probe that always returns one
  problem with `world.hooks.check_burrito(...)`, save one verse through the world, drop the world).
- Files: `crates/tc-app/tests/app/p5.rs`.
- Done when: both pass; with `BurritoWatch::drop` emptied, the second fails ("did not panic").
- Record: a log line `step 2` with that failing output.

### Step 3: Run every tc-app test under the watch and sort the failures

- Do: `cargo test -p tc-app 2>&1 | tee target/chk2-tc-app.log`. For each test that now fails with
  `P5/INV-VALID-BURRITO`: if the test itself made the folder invalid on purpose (an outside edit,
  a damaged or removed file), add `watch.stop("<why>")`; otherwise the store wrote an invalid
  project: that is a blocker. Compare each test binary's time with the log of a run before
  step 1: the probe reads the whole folder after each write job, so the operation-count tests
  (`crates/tc-app/tests/op_counts/`, the Psalms corpus and one ten times larger) may slow down.
  A binary more than 20% slower: stop its corpus stores' watches with a reason and call
  `assert_valid_burrito` once at the end of each such test instead. Then prove the whole chain
  once: in `tc-store`,
  find where an ingredient's md5 is written into `metadata.json`
  (`grep -rn md5 crates/tc-store/src crates/tc-journal/src`), change one hex digit, run
  `cargo test -p tc-app --test app`, see failures that start with `P5/INV-VALID-BURRITO`, and revert.
- Files: the test files that need `stop`.
- Done when: `cargo test -p tc-app` passes; `grep -rn 'watch.stop(' crates/tc-app/tests` lists
  the opt-outs, each with a reason; no binary is more than 20% slower than before step 1.
- Record: a log line `step 3` with the opt-out count, the times before and after, the mutation's
  failing output, and every blocker found (see "Stop and ask when").

### Step 4: Add the test/validate route

- Do: in `crates/tc-api/src/test.rs` add `Validate {}` and `ValidateView { checked: u32, invalid:
  Vec<InvalidProject> }` with `InvalidProject { folder: String, problems: Vec<String> }`, with the
  same derives and serde attributes as `Echo`/`EchoView`. Add
  `query "test/validate": $crate::test::Validate => $crate::test::ValidateView;` to
  `test_hook_table!` (`crates/tc-api/src/lib.rs:220-231`). In `crates/tc-app/src/hooks.rs` add
  `impl Handle<Validate> for App`: for each open project, wait until no open, leave checkpoint
  or close runs (`Sessions::settled`, `crates/tc-app/src/session.rs:832`) and its store is idle
  (`ProjectHandle::wait_idle`, `crates/tc-app/src/backend/mod.rs:1178`, test-hooks only), then run
  `tc_burrito::validate_dir` on each project folder under the app's projects root (`App::projects_root`,
  `crates/tc-app/src/app.rs:538`) and return the invalid ones; on the memory backend `checked` is
  0. Write `test_validate_names_a_project_whose_ingredient_md5_is_wrong` in
  `crates/tc-app/tests/app/hooks.rs` the way that file calls routes (`t.app.call(path, body)`):
  a store-backed app, one project, a wrong md5 written into its `metadata.json`, and the reply
  names that folder with a `ChecksumMismatch`. Add `"test/validate"` to the path list in
  `crates/tc-app/tests/app/hooks.rs` (around line 146) and to `HOOK_TEXT` in
  `crates/tc4/tests/release.rs` (lines 19-35, the words no release binary may hold; raise the
  array length). `HOOK_TEXT` also lacks `"test/script"` today: add it in the same edit.
- Files: `crates/tc-api/src/test.rs`, `crates/tc-api/src/lib.rs`, `crates/tc-app/src/hooks.rs`,
  `crates/tc-app/tests/app/hooks.rs`, `crates/tc4/tests/release.rs`.
- Done when: the new test passes, and fails when the handler returns an empty `invalid`;
  `cargo test -p tc-api` and `cargo test -p tc4 --test release` pass. (Check that
  `crates/tc-api/tests/roundtrip.rs` covers the test service's types; if it does not, add the two
  new types to `every_shared_type_round_trips`.)
- Record: a log line `step 4`.

### Step 5: Validate after every journey

- Do: in `parity/app/journeys/kit.mjs`, make `browserTest` keep the running test's name, and in
  `withApp`, after `body` returns and before `env.close()`, call
  `env.call('test/validate', {})`. Fail with
  `P5/INV-VALID-BURRITO: after <test>, <folder> is not a valid Scripture Burrito: <problems>` for
  each invalid folder, and fail when the store backend checked 0 folders. Add a `withApp` option
  `invalidOnPurpose: '<reason>'` that skips the assertion and prints the reason. Run every journey
  locally (the commands of `units/G5.md` step 1).
- Files: `parity/app/journeys/kit.mjs`, any journey that needs `invalidOnPurpose`.
- Done when: `node --test parity/app/journeys/*.test.mjs` with `TC4_BIN`, `TC4_UI`,
  `TC4_UI_HOOKS` and `TC4_DOOR43_FAKE` set shows no new failure; in one journey, write a wrong md5
  into its project's `metadata.json` at the end of the body: that journey fails with the P5
  message. Revert it.
- Record: a log line `step 5` with the counts and the failing message.

### Step 6: Point the next writers at the hook

- Do: for each of the cards `units/D2.md`, `units/E2.md`, `units/X.md` and `units/I.md` that is
  `todo` with `owner: none`, add one line under "Before you start": "Every store your tests build
  goes through `backend_over` and its `BurritoWatch` (CHK-2); a journey ends with `test/validate`.
  Do not validate by hand only." For a card that has an owner, write a `note` in this card's log
  instead and leave that card alone.
- Files: those cards.
- Done when: `grep -l 'BurritoWatch' .audit/roadmap/units/*.md` lists each card that was free.
- Record: a log line `step 6`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 made it; S2 waits for INT-4, which needs INT-3) on the exact tree you merge. It must print
`gates: all <n> steps passed; receipt ...`. If the script is missing, run these instead:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
3. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
   the same with `--features gallery`, `--features app` and `--features test-hooks`
4. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
6. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
   (needs `gcc-mingw-w64-x86-64` and `nasm`, as `.github/workflows/ci.yml` installs them)

Plus: every browser journey once locally with the four variables set (G5 step 1's commands), and
G5's `app-e2e` job green if G5 has merged.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- Step 3 finds a state the store wrote that is not a valid Burrito: a blocker. Write a red test
  for it in `crates/tc-store/tests/store/`, record it in the log with the failing output, and stop:
  the fix belongs to the storage lane. Do not opt that test out.
- A journey fails the new assertion and the cause is not the journey's own doing.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/done-definition.md` P5 row, DOD-3, DOD-4
- `evidence/2026-10-21/checks-inventory.md` rank 9, finding C7
- `docs/PLAN.md` P5; `docs/STORAGE.md` §16.6 and I7 (S2)
- `crates/tc-store/tests/store/common.rs:23-60`, `crates/tc-store/src/hooks.rs:100-180`,
  `crates/tc-store/tests/store/burrito.rs:27,113` (S2)
- `crates/tc-burrito/src/validate.rs:197-230` (`validate_project`, `validate_dir`)
- Size basis: checks-inventory rank 9 (1-2 days) plus the route and the kit call.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/done-definition.md
- 2026-10-24 06:11 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-CHK-2 from main 33d2378d (S2 done). Overlap noted: C7a's branch edits tc-api's test.rs and lib.rs and tc-app tests/app/sync.rs; FIX-R's edits sources.rs, align_store.rs, upgrades.rs and create.rs among the store builders; whichever merges second resolves the small conflicts | python3 -I .audit/tools/board.py
- 2026-10-24 07:44 EDT | checks lane (Claude Code 2198ee19) | step 1 | none | Every tc-app test store carries a BurritoWatch: backend_over and watch_over install S2's writer probe (metadata.json and ingredients/ only, so the probe costs what it checks), StoreWorld and TestApp hold the watch, and its drop fails with "P5/INV-VALID-BURRITO: after <test>, <folder> is not a valid Scripture Burrito: <problems>". The two direct StoreBackend::start sites in store_scenario.rs call watch_over; app/store.rs's real-folder store is checked with assert_valid_burrito_files. every_store_a_tc_app_test_builds_is_watched scans tests/ for unwatched starts and thrown-away watches | cargo test --locked -p tc-app --test app p5::
- 2026-10-24 07:44 EDT | checks lane (Claude Code 2198ee19) | step 2 | none | p5_probe_names_a_planted_bad_md5 and p5_watch_fails_a_test_whose_store_reported_a_problem pass. With the watch's check emptied, the second fails: "test did not panic as expected at crates/tc-app/tests/app/p5.rs:34:10" | cargo test --locked -p tc-app --test app p5::
- 2026-10-24 07:44 EDT | checks lane (Claude Code 2198ee19) | step 3 | none | No store-written state was invalid: no blocker. Opt-outs: 2 (p5.rs's planted md5; the op-count corpus stores, whose watch stops and whose Scale drop checks each folder once). Times, same machine, load noted: app binary main 252 s (load 7) vs CHK-2 244 s (load 10-26); op_counts main 453 s (load 6-8) vs CHK-2 564-589 s with the watch on (load 9-14) and 511 s with the corpus watch stopped, so the corpus stores take step 3's opt-out. Mutation: one md5 hex digit flipped in tc-store's metadata_entries: 147 app tests fail, first on the older checks (the world's assert_valid_burrito, the store refusing NotABurrito); with the world's assert also off, 106 fail with "P5/INV-VALID-BURRITO: after app::create::a_create_sent_twice_with_one_request_id_makes_one_project, /projects/.staging-create-… is not a valid Scripture Burrito: ChecksumMismatch { path: \"ingredients/JON.usfm\", … }". Both reverted. Found in passing, not CHK-2's: a saved proptest case fails save_machine's C3 on main's code (story reference, Retry after a refused empty text); case and details in the shared notes, reported to the orchestrator | cargo test --locked -p tc-app --test app
