---
id: UI-2
title: Ports for sync, share, exchange and import
track: A
lane: app
milestone: M1
state: gating
owner: C7a session (Claude Code 3058c665)
branch: track-UI-2
worktree: .worktrees/track-UI-2
depends: [UI-1]
needs_owner: []
proves: [P2, P9]
no_loss: false
size: 1-2 d
merged: none
updated: 2026-10-24
next: Merge after UI-1: merge main, gate the merge commit (batch or --wait-ci UI-2), fast-forward main.
blocked_by: none
---

# UI-2: Ports for sync, share, exchange and import

## Goal

`AppDeps` holds one port per outside job: `SyncPort` (sync, share and open from Door43, as
`docs/APP-DESIGN.md:1002` designs it), `ExchangePort` and `ImportPort`. Release builds get
ports that answer `Unavailable`; tests and journeys get a scripted port. No business function
holds a `cfg(feature = "test-hooks")` branch for these jobs any more, so release and tests run
the same code. E1a, E1b, E2 and F1 then only add real ports and their methods.

## Why it matters to a translator

Team work (P2) and carrying projects over (P9) run through these ports. Today the tested sync
path is a test-only branch, and the release path is never run by a test. With ports, the code a
translator runs is the code the tests run.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| UI-1 '` shows `done`.
- `grep -rn "SyncPort\|ExchangePort\|ImportPort" crates/` prints nothing (no port exists yet).
- `grep -n "cfg(feature = \"test-hooks\")" crates/tc-app/src/sync.rs` shows the in-body branch
  in `sync_job` (line 143 on resume-build-lane).
- `grep -n "struct ScriptedOpenDoor43\|struct ScriptedSync" crates/tc-app/src/*.rs` prints
  `share.rs` and `sync.rs`: the two scripted doubles. `ScriptedOpenDoor43::next` is compiled
  into release builds and answers `Unavailable(Sync)` there (share.rs:150-154).
- `grep -rn "^\s\{8,\}#\[cfg(.*\(test-hooks\|fixture-backend\)" crates/tc-app/src | awk -F: '{print $1}' | sort | uniq -c`
  prints today's in-body branches per file. On resume-build-lane: 33 lines (app.rs 15,
  dispatch.rs 8, upgrades.rs 3, align/suggest.rs 2, seed.rs 2, fixes.rs 1, sync.rs 1,
  understand.rs 1). Write the numbers you see in the log.
- `grep -n "pub struct Exchange\|pub struct Import\|\"exchange/\|\"import/" crates/tc-api/src/*.rs`
  prints nothing: no exchange or import endpoint or wire type exists yet.

## Funnel card

- **Invariants**:
  - INV-RELEASE-AND-TESTS-RUN-THE-SAME-CODE: no function body in `crates/tc-app/src` holds a
    `cfg(feature = "test-hooks")` or `cfg(feature = "fixture-backend")` branch, except the
    ones listed with a reason in the allowlist, which may only shrink.
  - INV-ONE-PORT-PER-JOB: sync, share, open from Door43, exchange and import each reach the
    outside through one `AppDeps` port; a scripted double exists only as a port.
- **Wrong states**:
  - A test passes on a scripted branch that a release build never runs (sync.rs:143-147).
  - A test double compiled into release (`ScriptedOpenDoor43`, share.rs:142-160).
  - A unit that adds a real sync or import edits business functions instead of adding a port,
    and copies the test branch again.
- **Code**:
  - Types: traits `SyncPort`, `ExchangePort`, `ImportPort` (to create,
    `crates/tc-app/src/ports.rs`); release structs `UnavailableSync`, `UnavailableExchange`,
    `UnavailableImport` (to create): `UnavailableSync` answers
    `ApiError::Unavailable(Feature::Sync)`, the other two answer `Feature::Exchange` and
    `Feature::Import` from their first methods (E2, F1); `ScriptedSyncPort` (to create,
    `crates/tc-app/src/ports/scripted.rs`, `#[cfg(feature = "test-hooks")]` at module level).
    Share has no port of its own: the design puts it in `SyncPort` (APP-DESIGN.md:1002,
    "sync, share, join").
  - One entry point: `App` keeps `Arc<dyn SyncPort>` (and the two others) from `AppDeps`;
    the `door43/sync` and `door43/open` handlers call only the port.
  - Removed: `ScriptedSync` (sync.rs:116-160), `ScriptedOpenDoor43` (share.rs:142-160), the
    in-body branch of `sync_job`, and the `open_door43`/`sync` fields of the app's inner
    state (app.rs:194, :197, :468, :470).
  - Left to checks, and why: "no test branch inside a business function" is about how code
    is written; a type cannot see a `cfg` attribute, so a source scan holds it.
  - Not in this item: `ExchangePort` and `ImportPort` get no methods yet. No exchange or
    import endpoint or wire type exists, so any signature now would be a guess. E2 and F1 add
    their methods, fakes and contract cases to the same files.
- **Checks**:
  - `no_test_hook_branch_inside_a_business_function` in
    `crates/tc-app/tests/app/test_hook_branches.rs` (to create); kind: lint (counts lines
    indented 8 or more spaces that start `#[cfg(` and name `test-hooks` or
    `fixture-backend`, per file under `crates/tc-app/src`, against an allowlist with a reason
    per file; `sync.rs` and `share.rs` are 0); runs in: CI job `rust`, `cargo test --workspace`.
  - `scripted_doubles_live_only_in_ports` in the same file (to create); kind: lint
    (`struct Scripted` appears under `crates/tc-app/src` only in `src/ports/`); runs in: CI
    `rust`.
  - Contract cases in `crates/tc-app/tests/app/port_contract.rs` (to create), a macro
    `sync_port_contract!(name, fixture)` modelled on `store_contract!`
    (`crates/tc-app/tests/store_contract.rs:401-402`), invoked once for the scripted port; E1a
    adds the real port's line:
    `a_sync_that_fails_before_intake_changes_nothing`,
    `an_open_from_door43_answers_a_project_the_list_holds`,
    `a_refused_open_from_door43_adds_no_project`,
    `with_the_internet_off_the_port_is_never_called`; kind: contract; runs in: CI `rust`.
  - The existing tests `crates/tc-app/tests/app/sync.rs` and `crates/tc-app/tests/app/share.rs`
    pass with no change to their test functions; journeys `sync-button` and `share-open`.
  - Proof it can fail: (1) put `#[cfg(feature = "test-hooks")]` back inside `sync_job`: the
    lint fails naming `sync.rs`. (2) Make the scripted port write the teammates' change before it
    answers a failure before intake: `a_sync_that_fails_before_intake_changes_nothing` fails. (3) Make
    dispatch's internet check answer `None` (`crates/tc-app/src/dispatch.rs:107-108`):
    `with_the_internet_off_the_port_is_never_called` fails (the scripted port counts its
    calls). Record each output.
- **Prose**: one doc line on each trait naming the unit that adds its real implementation
  (E1a, E1b, E2, F1).
- **Human**: none.

## Steps

### Step 1: Write the lint first and see it fail

- Do:
  1. Create `crates/tc-app/tests/app/test_hook_branches.rs` and add `mod test_hook_branches;`
     to `crates/tc-app/tests/app.rs`. Write `no_test_hook_branch_inside_a_business_function`
     as the funnel card says. Fill its allowlist from the counts in "Before you start", one
     entry per file with its reason (for example `dispatch.rs`: routes the test service's
     endpoints), and set `sync.rs` and `share.rs` to 0.
  2. Write `scripted_doubles_live_only_in_ports`.
  3. Failure messages start with `INV-RELEASE-AND-TESTS-RUN-THE-SAME-CODE:` and
     `INV-ONE-PORT-PER-JOB:` and name the file and line.
- Files: `crates/tc-app/tests/app/test_hook_branches.rs`, `crates/tc-app/tests/app.rs`.
- Done when: `cargo test -p tc-app --test app test_hook_branches` fails on `sync.rs` (one
  line) and on `share.rs` and `sync.rs` for `struct Scripted`; paste the two messages in the
  log. Do not commit a red test: commit steps 1 to 3 together, once the lint passes
  (`AGENTS.md`: every commit builds and passes its tests).
- Record: a log line `step 1`; set `next` to step 2.

### Step 2: Define the ports and their release implementations

- Do:
  1. Create `crates/tc-app/src/ports.rs` (listed in `crates/tc-app/src/lib.rs`). `SyncPort:
     Send + Sync` gets the two jobs that exist today, with the outcome types UI-1 gave their
     rows and the crate's `BoxFuture` (`crates/tc-app/src/backend/mod.rs:47`):
     `fn sync(&self, app: App, folder: ProjectRef) -> BoxFuture<Result<SyncResultView, JobEnd>>`
     and `fn open(&self, app: App, repo: RepoSource, name: ProjectName) ->
     BoxFuture<Result<ProjectRef, ApiError>>` (the fields of `tc_api::door43::OpenDoor43`).
     The port takes a clone of `App` because the scripted sync writes the teammates' changes
     through the app's store and sessions (sync.rs:170-220), as the real sync's intake will. Share's push is E1b's to add.
     `ExchangePort` and `ImportPort` are declared with no methods.
  2. Add `UnavailableSync`, `UnavailableExchange`, `UnavailableImport`.
  3. Add `pub sync: Arc<dyn SyncPort>`, `pub exchange: Arc<dyn ExchangePort>`,
     `pub import: Arc<dyn ImportPort>` to `AppDeps` (`crates/tc-app/src/app.rs:65`) and the
     three `Unavailable…` values to `AppDeps::production` (app.rs:96).
- Files: `crates/tc-app/src/ports.rs`, `crates/tc-app/src/lib.rs`, `crates/tc-app/src/app.rs`.
- Done when: `cargo clippy -p tc-app --lib -- -D warnings` passes (the release build, without
  test hooks); `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings` passes.
- Record: a log line `step 2`; set `next` to step 3.

### Step 3: Move the scripted sync and open into a scripted port

- Do:
  1. Create `crates/tc-app/src/ports/scripted.rs`, declared in `ports.rs` with
     `#[cfg(feature = "test-hooks")] pub mod scripted;`. Move into `ScriptedSyncPort` what
     `ScriptedSync` and `scripted_sync` (sync.rs:116-224) and `ScriptedOpenDoor43`
     (share.rs:142-160) do today, unchanged in behavior (`self` becomes the `app` argument).
     The store methods it calls, `teammates_edit_verse` and `synced`
     (`crates/tc-app/src/backend/mod.rs:1087`, `:1100`), stay test hooks.
  2. Reach the scripted port from the test hooks without a new in-body `cfg`: give
     `SyncPort` a trait method `#[cfg(feature = "test-hooks")] fn scripted(&self) ->
     Option<&ScriptedSyncPort> { None }` that the scripted port overrides. Keep
     `App::script_sync` and `App::script_open_door43` (item-level `cfg`) as one-line
     forwards, so `tests/app/share.rs` and the `test/script` handler (sync.rs:248) keep
     working.
  3. Choose the scripted port where `AppDeps` is built, never inside `App::start`: in
     `crates/tc4/src/lib.rs`, next to the test keychain (around line 226), when
     `args.test_hooks` is set; and in `crates/tc-app/tests/app/common.rs`, the only test that
     builds `AppDeps`.
  4. Make `sync_job` (sync.rs:141) and the `door43/open` job (share.rs:200-210) call
     `self.inner.sync` (the port). Delete `ScriptedSync`, `ScriptedOpenDoor43`, and the inner
     fields at app.rs:194, :197, :468, :470.
- Files: `crates/tc-app/src/ports.rs`, `crates/tc-app/src/ports/scripted.rs`,
  `crates/tc-app/src/sync.rs`, `crates/tc-app/src/share.rs`, `crates/tc-app/src/app.rs`,
  `crates/tc4/src/lib.rs`, `crates/tc-app/tests/app/common.rs`.
- Done when: `cargo test -p tc-app --test app test_hook_branches` passes;
  `cargo test -p tc-app --test app sync share` passes; and
  `git diff <base> -- crates/tc-app/tests/app/sync.rs crates/tc-app/tests/app/share.rs`
  prints nothing (`<base>` is the commit you claimed on).
- Record: a log line `step 3`; set `next` to step 4.

### Step 4: Add the port contract cases

- Do: create `crates/tc-app/tests/app/port_contract.rs` with `macro_rules!
  sync_port_contract` holding the four cases of the funnel card, each driven through the
  public API of `App` (post a `door43/sync` or `door43/open`, follow the job, read the
  project list or the project's records). Invoke it once:
  `sync_port_contract!(scripted, crate::app::common::scripted_sync_port)`. Leave a comment
  saying E1a adds the real port's invocation.
- Files: `crates/tc-app/tests/app/port_contract.rs`, `crates/tc-app/tests/app.rs`,
  `crates/tc-app/tests/app/common.rs`.
- Done when: `cargo test -p tc-app --test app port_contract` passes; each proof mutation of
  the funnel card makes its case fail (paste the outputs in the log).
- Record: a log line `step 4`; set `next` to step 5.

### Step 5: Run the team journeys and the full gates

- Do: run the full gate list, then the journeys below.
- Files: none.
- Done when: every gate passes; the journeys print `# fail 0` and `# skipped 0`.
- Record: a log line `gate`; set `state: verifying` and `next`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 creates it; it lands before this item can start). If it is
missing, run every command of `AGENTS.md` "Tests" and the Windows step:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, and
  again with `--features gallery`, `--features app` and `--features test-hooks`
- `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
- `cargo clippy -p tc-app --lib -- -D warnings`
- `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
- `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`

Plus the journeys of sync and open from Door43 (Playwright must resolve: `npm ci` in
`parity/tc4-baseline` once, or set `PLAYWRIGHT_MODULE`):

```sh
cargo build -p tc4 --features test-hooks,fixture-backend
cargo build -p tc-door43-fake
crates/tc-ui/build-app.sh
TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
  node --test --test-reporter=tap parity/app/journeys/sync-button.test.mjs \
  parity/app/journeys/share-open.test.mjs | tail -n 9
```

It must print `# fail 0` and `# skipped 0`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A port method would need a type from `tc-sync` or `tc-import`, which do not exist yet:
  do not invent it; leave that method to E1a or F1 and log a `note`.
- Moving a scripted outcome changes what a journey shows.
- A gate fails twice and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/health-app.md` §7, §8 item 2, CH-3.
- `docs/APP-DESIGN.md:1001-1004` (the three port traits), `:1527` (the services that use
  them), `:3411` (E1a "the real `SyncPort`").
- `crates/tc-app/src/sync.rs:116-160`, `:248-260` (`test/script`); `crates/tc-app/src/share.rs:142-210`;
  `crates/tc-app/src/app.rs:65-102` (`AppDeps`), `:248` (`start`); `crates/tc4/src/lib.rs:213-236`;
  `crates/tc-app/tests/store_contract.rs:401-402`.
- `FUNNEL.md` §5 ("Contract test").

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/health-app.md
- 2026-10-24 | C7a session (Claude Code 3058c665) | claim | none | cut from track-UI-1 (efe271cd) on the orchestrator's word under the fast-release ruling; UI-1 is not done yet, so the merge waits for it. Start checks: no port exists; sync.rs's in-body branch at 143; `ScriptedOpenDoor43` share.rs:145, `ScriptedSync` sync.rs:118; in-body branches 33 (app.rs 15, dispatch.rs 8, upgrades.rs 3, align/suggest.rs 2, seed.rs 2, fixes.rs 1, sync.rs 1, understand.rs 1); no exchange or import endpoint | grep commands of "Before you start"
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | `no_test_hook_branch_inside_a_business_function` and `scripted_doubles_live_only_in_ports` (tests/app/test_hook_branches.rs), red first: `INV-RELEASE-AND-TESTS-RUN-THE-SAME-CODE: ... src/sync.rs: 1 in-body test branches (lines [143]), 0 allowed` and `INV-ONE-PORT-PER-JOB: ... src/share.rs:145, src/sync.rs:118, src/upgrades.rs:123`. upgrades.rs's `ScriptedResources` is the resources service's double (C8), not one of this card's jobs: allowed by name with that reason. Committed with steps 2 to 4 | cargo test -p tc-app --test app test_hook_branches
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 2 | none | `crates/tc-app/src/ports.rs`: `SyncPort` (sync and open, outcome types from UI-1), `ExchangePort` and `ImportPort` (no methods: E2 and F1 add them), `UnavailableSync/Exchange/Import`; `AppDeps` holds the three ports and `production()` the unavailable ones. Release clippy of tc-app, tc-server and tc4 pass | cargo clippy -p tc-app --lib -- -D warnings
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 3 | none | `ports/scripted.rs` `ScriptedSyncPort` holds the scripted syncs and opens and counts its calls; `ScriptedSync` and `ScriptedOpenDoor43` and the inner `open_door43`/`sync` fields are gone; `sync_job` and the `door43/open` job call only the port. Decision: the scripted sync's intake (`App::scripted_sync`, the teammates' writes and the reread) stays an item-level test-hooks method in sync.rs that the port calls, so sync.rs's private helpers stay private; it is not an in-body branch. tc4 picks the scripted port beside the test keychain when `--test-hooks` is on; the tests' common setup picks it too. In-body branches now 32 (app.rs 15 to 14, sync.rs 1 to 0); the allowlist holds today's counts. tests/app/sync.rs and share.rs unchanged | cargo test -p tc-app --test app -- test_hook_branches sync share
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 4 | none | `sync_port_contract!` (tests/app/port_contract.rs) with the four cases, invoked for the scripted port; E1a's line is a comment. Proofs: (1) a cfg branch back in `sync_job` fails the lint at sync.rs:130; (2) the scripted port writing two teammates' versions before answering a failure fails `a_sync_that_fails_before_intake_changes_nothing` (Titus conflicts 1, not 0); (3) dispatch's internet check answering `None` fails `with_the_internet_off_the_port_is_never_called` (`left: Err(NeedsSignIn(NotSignedIn)) right: Err(NeedsInternet(Sync))`, before the call count) | cargo test -p tc-app --test app port_contract
- 2026-10-24 | C7a session (Claude Code 3058c665) | gate | none | tc-app: every test binary passes (app 677); clippy clean; journeys sync-button and share-open `# tests 5 # pass 5 # fail 0 # skipped 0`. The full gate list runs on the merge commit after UI-1 | .enjoy-logs/ui2-journeys.log
- 2026-10-24 | races and protocol lens (Claude Code subagent of 3058c665) | verify | 7e609bd4 | no blocker. Release answers as before; with the feature on and `--test-hooks` off tc4 gets `UnavailableSync`, and `--scenario` journeys get the scripted port; reset still clears scripts. Majors, all in the tests and the script hook: the open case passes for a port that writes nothing (the scripted open answers a project already listed); the sync-fail and refused-open cases match any failure, so a dropped script passes; `test/script` answers Ok when the port is not scripted; `calls()` reads 0 for any other port. Minors: the allowlist is a ceiling, not exact; `is_branch` misses `cfg!` and tabs | cargo test -p tc-app --test app -- port_contract test_hook_branches sync share
- 2026-10-24 | C7a session (Claude Code 3058c665) | fix | none | each fixture call now returns the exact error its case expects, and the cases compare it (`JobState::Failed(error)`); `OPEN_WRITES` makes a writing port prove the project was not listed before (the scripted port answers a listed one); `test/script` refuses a Door43 script with BadRequest when the port is not scripted (`scripting_a_port_that_is_not_scripted_is_refused`; without the refusal it fails `Ok(())`); `calls()` expects the scripted port. The lint's counts are exact and it catches `cfg!(` and tab indents; the exact count found my log's slip: app.rs still said 15 for 14 branches, now 14. 65 targeted tests pass, clippy clean | cargo test -p tc-app --test app -- port_contract test_hook_branches sync share hooks
