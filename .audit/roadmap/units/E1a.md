---
id: E1a
title: Sync with Door43 in the app
track: A
lane: app
milestone: M3
state: verifying
owner: C7a session (Claude Code 3058c665)
branch: track-E1a
worktree: .worktrees/track-E1a
depends: [X, C7a, D1, UI-2, UI-3a]
needs_owner: []
later: ["H11 from step 2"]
proves: [P1, P2, P4]
no_loss: true
size: 5-6 sessions (basis: B10 and C0)
merged: none
updated: 2026-10-24
next: Reviewed and ready; the merge waits for D1. The integration lane gates it then.
blocked_by: none
---

# E1a: Sync with Door43 in the app

## Goal

The "Sync with Door43" button on a Home card runs a real sync through `tc-sync` (unit X), in
tests against `tc-door43-fake`. A dropped connection, a save refused after a sync, and three
teammates syncing one project all end with every translator's work kept and visible. "Stay
signed in" uses the computer's keychain once the owner answers H11. The five sync screens are
captured on real data, not on scripted outcomes.

## Why it matters to a translator

Teammates share their work through Door43. A sync must never lose a save, never push over a
teammate's work, and never leave a translator stuck on a screen they cannot leave.

## Before you start

Line numbers are from `resume-build-lane` at f9067bc (main after INT-4); find moved ones by grep.

- `python3 -I .audit/tools/board.py | grep -E '^\| (X|C7a|D1|UI-2|UI-3) \|'` shows `done` in
  all five rows. `ls crates/tc-sync/Cargo.toml` prints the path.
- UI-2: `grep -n "trait SyncPort\|struct UnavailableSync" crates/tc-app/src/ports.rs` prints two
  lines; `grep -n "sync_port_contract!" crates/tc-app/tests/app/port_contract.rs` shows the
  macro and its one invocation, for the scripted port. `AppDeps::production()`
  (`crates/tc-app/src/app.rs:95`) holds `UnavailableSync`: that is the release path to replace.
- UI-3: `AppCtx::run_net`, `AppCtx::leave` and `tc_ui::i18n::refusal_text` exist
  (`grep -rn "fn run_net\|fn leave\|fn refusal_text" crates/tc-ui/src`).
- X-2: `grep -n "BreakOff" crates/tc-door43-fake/src/smart_http.rs` prints a line: the fake
  cuts a git smart-HTTP reply (R13).
- H11: `ls .audit/roadmap/rulings | grep -- -H11` prints a ruling before step 2. The other steps
  do not need it.
- tC4's specs are at tC4 b86270f. `git -C ~/unfoldingword/tc4-sweep cat-file -t b86270f` prints
  `commit`; if not, run SWEEP step 1's two commands (`units/SWEEP.md`). Read with
  `git -C ~/unfoldingword/tc4-sweep show b86270f:e2e/sync-refusal.spec.ts`. Never fetch into
  `~/unfoldingword/translationcore4`; it stays at d310bf3.
- Journey runs need `. ~/.tc-env`, `cargo build -p tc4 --features test-hooks,fixture-backend`,
  `crates/tc-ui/build-app.sh`, `cargo build -p tc-door43-fake`, and `npm ci` in
  `parity/tc4-baseline`. "The three variables" below means
  `TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake`.

## Funnel card

- **Invariants**:
  - INV-A-SYNC-NEVER-LOSES-A-SAVE: every save made before, during or after a sync is on disk
    after the sync ends, however it ends, and the next sync pushes it.
  - INV-NO-REQUEST-WITHOUT-CONSENT: nothing reaches Door43 at launch, while the app is idle, or
    before the person answered the internet question and signed in.
  - INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: a save refused after a sync keeps its text,
    shows one plain sentence, and Home leaves in one click.
- **Wrong states**:
  - Release builds answer `Unavailable(Sync)`; only test builds sync (health-app CH-3).
  - Home does nothing on a click after a save was refused for good: `gate()` returns `Blocked`
    for a field that failed after its one retry (`crates/tc-ui/src/app/save/mod.rs:1211-1214`,
    `crates/tc-ui/src/app/web/saves.rs:461`). tC4 #801, #807; read from code, not reproduced.
  - A save typed during a sync is missing afterwards, or never pushed.
  - A connection cut after intake shows "Nothing changed" although teammates' work came in.
  - A request reaches Door43 at launch or while idle.
  - The toast says "{name}'s alignment replaced yours" (tC4 D108) though nothing was replaced.
- **Code**:
  - Types: `Door43Sync` (to create) implements `SyncPort` over `tc-sync`. `OsKeychain` (to
    create) implements `Keychain` (`crates/tc-app/src/account.rs:61`). FIX-R's `Hold::RefusedForThisOpen(NotSaved)`
    (`units/FIX-R.md` step 2, `enum Hold`, `crates/tc-ui/src/app/save/mod.rs:113`) for a save
    refused for good: a held field never blocks the gate (mod.rs:1207-1209). Create it here only
    if FIX-R is not `done`, in FIX-R's shape and name, and add a `note` to FIX-R's log; never a
    second variant for the same thing. Sync results map onto `SyncFailure` and
    `SyncBlock` (`crates/tc-api/src/refusal.rs`) with exhaustive matches.
  - One entry point: `impl Handle<SyncProject> for App` (`crates/tc-app/src/sync.rs:228`) calls
    the port; `AppDeps::production()` holds `Door43Sync`.
  - Removed: `UnavailableSync` from `AppDeps::production()` (delete the struct if nothing else
    uses it); the scripted Door43 steps of `parity/app/env-rust.mjs` (`scriptSync` :230,
    `pushToDoor43`, `dropNextClone`, `teammatesChangeOneVerse`, `loseFromDoor43` :291-300).
  - Left to checks, and why: "no request at launch or while idle" is the absence of an action
    over time, which no type states; three teammates converging is X's fold seen through the UI.
- **Checks** (all runs in CI `rust` unless named a journey; journeys run in G5's `app-e2e`):
  - `sync_port_contract!(door43, …)` in `crates/tc-app/tests/app/port_contract.rs`: UI-2's cases
    on the real port, plus new cases `a_save_during_a_sync_is_kept_and_pushed_next` and
    `a_story_project_syncs_like_a_bible` (#761) (to create); kind: contract.
  - `no_door43_request_at_launch_or_60_s_idle`, `two_different_alignments_of_one_verse_are_listed_once`,
    `a_failed_branch_lookup_is_a_failed_sync`,
    `a_dropped_connection_keeps_received_work_and_says_so` (to create) in
    `crates/tc-app/tests/app/sync.rs`; kind: unit (paused time; `FakeDoor43::requests()`,
    `crates/tc-door43-fake/src/lib.rs:714`).
  - `stale_scope_save_keeps_text_and_home_leaves_in_one_click`,
    `a_refused_align_save_behind_a_span_save_lets_home_leave` (to create) in
    `crates/tc-ui/tests/app/save.rs`; `failed_open_keeps_the_sync_toast_links` (to create) in
    `crates/tc-ui/tests/app/sync.rs`; kind: unit.
  - Journeys (to create): `parity/app/journeys/sync-refusal.test.mjs` (tC4 J2, J6, J1b) and
    `parity/app/journeys/team-sync.test.mjs` (J26, tests titled `[G1] …` and `[G6] …`, #24).
  - Proof it can fail: each step names a mutation. Run it, see the failure message start with
    the invariant's name, restore, and paste the failing line into the log.
- **Prose**: `docs/PLAN.md` "Where the rebuild differs from tC4 4.0 on purpose", the #791
  paragraph (:152-155): tC4 rc.2 lists "{book} {ref}: {name}'s alignment replaced yours." per
  verse (D108, tC4 `src/views/SyncResult.jsx:98-110`); the rebuild keeps both alignments as a
  conflict, so it shows a count and Review. `docs/TC4-ISSUES.md` rows #801, #791, #678, #724,
  #24, and a new row for #807.
- **Human**: H11, for step 2 only. Tell the owner the D108 difference in one plain sentence in
  the merge report (AGENTS.md "Matching tC4"); it is not a question.

## Steps

### Step 1: Build the real sync port, red first

- Do: Add `sync_port_contract!(door43, <fixture>)` to `port_contract.rs`, with a fixture that
  starts the fake and an app on `Door43Sync`; it fails to compile or fails until the port
  exists. Write `Door43Sync` over `tc-sync`: the account's token, the leave checkpoint first (as
  `sync_job` waits on `sessions.settled`, sync.rs:142), X's sync, then the toast view through
  `synced_view` (sync.rs:67). Add the two new contract cases. Put `Door43Sync` in
  `AppDeps::production()`.
- Files: `crates/tc-app/src/ports/door43.rs` (to create; declare it in `ports.rs`),
  `crates/tc-app/src/app.rs`, `crates/tc-app/tests/app/port_contract.rs`,
  `crates/tc-app/tests/app/common.rs`.
- Done when: `cargo test -p tc-app --test app port_contract` passes, and
  `cargo test -p tc-app --test app port_contract -- --list` lists each case under both
  `scripted` and `door43`; with the save's write skipped in `Door43Sync`,
  `a_save_during_a_sync_is_kept_and_pushed_next` fails with `INV-A-SYNC-NEVER-LOSES-A-SAVE`;
  `cargo clippy -p tc-app --lib -- -D warnings` passes.
- Record: append a log line `step 1` (TRACKING.md format); set `next` to step 2, or step 3 if
  H11 has no ruling; set `updated`.

### Step 2: Keep the token in the computer's keychain (after H11)

- Do: If `ls .audit/roadmap/rulings | grep -- -H11` prints nothing, log a `note`, go to step 3,
  and come back when it exists. If the ruling says `keyring`: add it to
  `[workspace.dependencies]` in `Cargo.toml`; write `OsKeychain` in
  `crates/tc-app/src/account.rs`: "no store on this computer" → `KeychainError::NoKeychain`,
  unreadable bytes → `KeychainError::Unreadable`; use it in `AppDeps::production()` in place of
  `MemKeychain::with_mode(MemKeychainMode::NoKeychain)`; test the mapping with the crate's mock
  store (`keyring::mock`). If the ruling says the shell's `safeStorage`: build nothing; log a
  `note` that G1 (Track B) owns it.
- Files: `Cargo.toml`, `crates/tc-app/Cargo.toml`, `crates/tc-app/src/{account.rs,app.rs}`,
  `crates/tc-app/tests/app/keychain.rs`.
- Done when: `cargo test -p tc-app --test app keychain` passes; with the three variables,
  `node --test parity/app/journeys/j11-sign-in.test.mjs` prints `# fail 0` and `# skipped 0`.
- Record: log line `step 2` (or `note` when skipped); update `next` and `updated`.

### Step 3: Finish the sync button flow and its refusals

- Do: Follow DELTA §3.15.1 (`docs/TC4-DELTA-2026-10-16.md:723-744`; APP-DESIGN.md:3411's
  "DELTA:717-738" is stale): a press while a sync runs does nothing; internet question; sign-in;
  checkpoint; sync; refresh the card and any open screen of that project; on `AuthFailed` forget
  the session and the kept token. Send every Door43 call through `AppCtx::run_net`, and every
  refusal through `refusal_text`. Keep Q36's fixed sentences (`docs/PLAN.md:156-158`). Write
  `no_door43_request_at_launch_or_60_s_idle`, `failed_open_keeps_the_sync_toast_links` (#724:
  Review and Download stay when the open they start fails), `a_failed_branch_lookup_is_a_failed_sync`
  (#724, `docs/TC4-ISSUES.md:532`; inject a `Status` 500 fault on the request X's sync uses to
  find the branch) and
  `two_different_alignments_of_one_verse_are_listed_once` (#791, two real teammates).
- Files: `crates/tc-app/src/sync.rs`, `crates/tc-ui/src/app/sync.rs`,
  `crates/tc-ui/src/views/sync_result.rs`, `crates/tc-app/tests/app/sync.rs`,
  `crates/tc-ui/tests/app/sync.rs`.
- Done when: `cargo test -p tc-app --test app sync` and `cargo test -p tc-ui --test app sync`
  pass, `every_sync_refusal_shows_its_own_sentence_never_door43s_words_or_a_path`
  (`crates/tc-ui/tests/app/sync.rs:240`) included; one Door43 request added to `App::start`
  (`crates/tc-app/src/app.rs:248`) makes `no_door43_request_at_launch_or_60_s_idle` fail with
  `INV-NO-REQUEST-WITHOUT-CONSENT`; with the three variables,
  `node --test parity/app/journeys/sync-button.test.mjs` prints `# fail 0` and `# skipped 0`.
- Record: log line `step 3`; update `next` and `updated`.

### Step 4: Make a refused save never trap the translator (#801, #807)

- Do: Red first. Write the two save-machine tests and port tC4 `sync-refusal.spec.ts` J2 (:215),
  J6 (:409) and J1b (:514, the one-window route of `docs/TC4-ISSUES.md:282`; here the sync joins
  the open session, so assert the book loads and Home leaves in one click) to
  `parity/app/journeys/sync-refusal.test.mjs` (J6 is the control: a save that fails for another
  reason still holds Home over two clicks, FR-32). If both unit tests pass at
  once, #801 does not happen here: keep the tests, log a `note`, skip the fix. Otherwise, in
  `crates/tc-ui/src/app/save/classify.rs` (exhaustive, no wildcard) send a refusal that can
  never succeed to `Hold::RefusedForThisOpen` (FIX-R step 2; see the funnel card's Types), which
  keeps the text as a draft (the declared difference at `docs/TC4-ISSUES.md:403`). This card's
  own work is the port of tC4 `e2e/sync-refusal.spec.ts` and the sync-specific reasons
  (`stale_scope_save_keeps_text_and_home_leaves_in_one_click`); the #801 save-machine tests in
  `crates/tc-ui/tests/app/save.rs` are FIX-R's.
- Files: `crates/tc-ui/src/app/save/{mod.rs,classify.rs}`, `crates/tc-ui/tests/app/save.rs`,
  `parity/app/journeys/sync-refusal.test.mjs`.
- Done when: `cargo test -p tc-ui --test app save` passes; with the `Hold` mapping reverted,
  `stale_scope_save_keeps_text_and_home_leaves_in_one_click` fails with
  `INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR`; the journey prints `# fail 0`, `# skipped 0`.
- Record: log line `step 4` with the failing output; update `next` and `updated`.

### Step 5: Show a dropped connection truthfully (R13)

- Do: Use X-2's `BreakOff` on git smart HTTP during the fetch, during the push, and after
  intake. Write `a_dropped_connection_keeps_received_work_and_says_so`: after intake the toast
  adds `SyncFailure::kept_note()` (`docs/APP-DESIGN.md:520-527`); before intake it says nothing
  changed. The fake binary's control port takes only `Status`, `Delay`, `RateLimited` and `Hold`
  (`FaultKind`, `add_fault`, `crates/tc-door43-fake/src/bin/tc-door43-fake.rs:176-192`): add
  `BreakOff { after }` there. Then add a `sync-failed` case on that fault to `sync-button.test.mjs`.
- Files: `crates/tc-app/tests/app/sync.rs`, `crates/tc-door43-fake/src/bin/tc-door43-fake.rs`,
  `parity/app/journeys/sync-button.test.mjs`, `parity/app/env-rust.mjs`.
- Done when: the test passes; mapping an after-intake failure to the before-intake sentence makes
  it fail with `INV-A-SYNC-NEVER-LOSES-A-SAVE`; the journey prints `# fail 0`.
- Record: log line `step 5`; update `next` and `updated`.

### Step 6: Run J26 with three app-data folders

- Do: Add `withApps(n, { door43 })` (to create) to `parity/app/journeys/kit.mjs`: one fake, n
  `tc4` processes, each with its own run folder (`withApp`, kit.mjs:73, starts one `tc4` and its
  own fake). Each app starts from the `shared` scenario (`fixtures/parity-state/shared`); the
  fake holds that project's repository through `FakeDoor43::add_repo`
  (`crates/tc-door43-fake/src/lib.rs:574`). The control port (`control_router`,
  `crates/tc-door43-fake/src/bin/tc-door43-fake.rs:110-119`) has no route for it: add
  `POST /repos` (to create) that calls `add_repo`. Port tC4 `e2e/j26-team-sync.spec.ts` (b86270f :315, :496 `[G6]`, :517 `[G1]`):
  A, B and C draft, check and sync; A keeps one version of the verse conflict (C7a); all three
  end with the same files. Add the check-conflict step only if the board shows C7b `done`;
  otherwise append a minor to `.audit/minors.tsv`: "J26 check-conflict step waits for C7b".
- Files: `parity/app/journeys/{kit.mjs,team-sync.test.mjs}`,
  `crates/tc-door43-fake/src/bin/tc-door43-fake.rs`.
- Done when: with the three variables, `node --test parity/app/journeys/team-sync.test.mjs`
  prints `# fail 0` and `# skipped 0` three runs in a row; dropping one of B's packs in the fake
  makes `[G1]` fail.
- Record: log line `step 6`; update `next` and `updated`.

### Step 7: Recapture the five sync screens on real data

- Do: Re-implement `pushToDoor43`, `dropNextClone`, `teammatesChangeOneVerse` and
  `loseFromDoor43` in `parity/app/env-rust.mjs` against the fake: real syncs, `BreakOff` through
  the control port (step 5), and teammates' changes pushed by a second `tc4` (step 6's
  `withApps`). Delete `scriptSync`. Add `E1a` to `parity/app/units.json`. Run
  `node parity/app/capture-rust.mjs --unit E1a --only shared-home,sync-failed,sync-ok,sync-ok-teammates,sync-ok-restored`,
  then `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`. Explain each `differs` row in
  `parity/app/residuals.json`.
- Files: `parity/app/{env-rust.mjs,units.json,captures.json,report.json,residuals.json}`,
  `parity/app/screens/`, `parity/app/dom/`.
- Done when: `grep -n scriptSync parity/app/env-rust.mjs` prints nothing;
  `cargo test -p tc-ui --test screen_record` passes; and
  `python3 -I -c "import json; s=json.load(open('parity/app/captures.json'))['screens']; print(sorted((x['name'],x['by'],x['backend']) for x in s if x['name'] in {'shared-home','sync-failed','sync-ok','sync-ok-teammates','sync-ok-restored'}))"`
  prints five rows, each with `'E1a'` and `'store'` (none `memory+scripted`, CHK-6's label).
- Record: log line `step 7`; update `next` and `updated`.

### Step 8: Write the records

- Do: Rewrite the #791 paragraph of `docs/PLAN.md` as the Prose line says. Update
  `docs/TC4-ISSUES.md` rows #801 (:282, :403), #791 (:133, :312, and the #791/D108 row X step 1
  added: it must say a count and Review, not one row per verse), #678 (:296), #724 (:287),
  #681 (:293), #761 (:283) and #24 (:310) with the test names; add a row for #807. In X's `docs/evidence/sync-<date>.md`
  (X step 8 writes the tC4 G1-G8 map), add the app-level tests for G1 and G6.
- Files: `docs/PLAN.md`, `docs/TC4-ISSUES.md`, `docs/evidence/sync-<date>.md`.
- Done when: `grep -n "#807" docs/TC4-ISSUES.md` prints a row; every test name this card added
  to TC4-ISSUES exists (`grep -rn "fn <name>" crates`, or the journey title in
  `parity/app/journeys/`).
- Record: log line `step 8`; set `next` to the gates.

## Gates before merge

The full gate list: `scripts/gates.sh` (to create in INT-3) once INT-3 lands. Until then:
`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace`; `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`
and the same with `--features gallery`, `--features app` and `--features test-hooks`;
`cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. Plus:

- The `sync-button`, `sync-refusal`, `team-sync`, `j11-sign-in` and `conflict-review` journeys
  pass 3 runs in a row with `# skipped 0`, locally and in G5's `app-e2e` job once it exists.
- `cargo test -p tc-ui --test screen_record` passes after the recapture.
- No-loss item: the review runs three verification lenses (FUNNEL.md §8 item 5).

## Stop and ask when

- H11 has no ruling and every other step is done.
- Porting tC4's J2 or J6 would need a new sentence or a new button on screen.
- X's sync cannot say whether intake happened before a failure (the toast needs it).

## References

- `docs/APP-DESIGN.md:3411` (E1a row), §9.2 R10 and R13 (:3263, :3266), Q-3 (:3556).
- `.audit/roadmap/evidence/2026-10-21/app-remaining.md` (E1a; F6, F8, F13, F14);
  `tc4-bugs.md` §2 and §4 (#801, #807, #806/D108); `tc4-coverage.md` rows 46, 47;
  `health-app.md` CH-3, CH-6, CH-8; `done-definition.md` DOD-6, DOD-10.
- `units/UI-2.md` (ports, `sync_port_contract!`), `units/UI-3.md`, `units/X.md` steps 3 and 8.
- tC4 b86270f: `e2e/sync-refusal.spec.ts`, `e2e/j26-team-sync.spec.ts`, `src/views/SyncResult.jsx`.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/app-remaining.md
- 2026-10-24 | C7a session (Claude Code 3058c665) | claim | none | on the orchestrator's word under the fast-release ruling, from track-UI-2 (9a630e9d) with track-X's tip merged (c9a833bc, two commits past the 15e01247 named); X, C7a, D1, UI-2 and UI-3 are not done, so the merge waits for them, and the steps that need D1 or UI-3 come last. Plan for step 1: tc-app gains tc-sync; the backend's `ProjectHandle` gets `door43_sync(self: Arc<Self>, ..)` (the tc-store handle runs `tc_sync::sync` on a blocking thread under its read lock; the memory backend answers none); the toast's received changes are the change ids the fold holds after the sync and not before (record keys and their histories), since tc-store's IntakeReport counts packs, not changes | none
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | `ports/door43.rs` `Door43Sync` over unit X: a sync runs `tc_sync::sync` on the project's tc-store handle (new backend seam `ProjectHandle::door43_sync(self: Arc<Self>, ..)`, on a blocking thread under the handle's read lock; the memory backend answers none), and the toast takes its received count and senders from X's `SyncReport.received_by`; an open runs `tc_sync::join` (seam `ProjectStore::door43_join`, StoreBackend and ScenarioStore); `ProjectHandle::door43_connect` and the test hook `App::connect_door43` connect a project for tests. `AppDeps::production()` holds `Door43Sync`; `UnavailableSync` is deleted. `sync_port_contract!(door43, Door43)` runs every case on the real port against the fake (the sample and the story project each connected to an empty repository of the parity user's), plus the new cases `a_save_during_a_sync_is_kept_and_pushed_next` (the sync held at its repository lookup while Titus 1:1 is saved, then Door43's copy opened as a new project holds it) and `a_story_project_syncs_like_a_bible`; `--list` shows each case under scripted and door43 (13 tests). Proof: with tc-sync's push skipped, `INV-A-SYNC-NEVER-LOSES-A-SAVE: the next sync took the save to Door43`. Clippy clean for tc-app (release and test-hooks) and tc4 | cargo test -p tc-app --test app port_contract
- 2026-10-24 | C7a session (Claude Code 3058c665) | note | none | step 2 waits: `ls .audit/roadmap/rulings | grep -- -H11` prints nothing | ls .audit/roadmap/rulings
- 2026-10-24 | C7a session (Claude Code 3058c665) | note | none | step 3, server half in progress: a sync or an open from Door43 that Door43 answers AuthFailed now forgets both copies of the token (`door43_refused`) and asks for a sign-in. `no_door43_request_at_launch_or_60_s_idle` (paused time, then real time for requests in flight) passes; with an idle poll every 10 s added to `App::start` it fails `INV-NO-REQUEST-WITHOUT-CONSENT: Door43 heard [... /api/v1/repos/a/b ...]`. `a_failed_branch_lookup_is_a_failed_sync` (500 on the repository lookup) passes. Both live in port_contract.rs beside the real-port fixture. Left in step 3: refreshing an open screen after a sync, the UI half (needs UI-3's run_net and refusal_text), `two_different_alignments_of_one_verse_are_listed_once`, `failed_open_keeps_the_sync_toast_links` | cargo test -p tc-app --test app -- no_door43_request a_failed_branch_lookup
- 2026-10-24 | C7a session (Claude Code 3058c665) | note | none | X's tip 2135e6ab merged; 76 sync, share and contract tests pass. Step 5, server half: `a_dropped_connection_keeps_received_work_and_says_so` (port_contract.rs) runs two computers on one fake (B on its own store and device, the sample opened from Door43); A pushes Titus 1:1, B's fetch cut by `BreakOff` fails before intake with nothing changed, B's push cut after intake fails `AfterIntake` and A's words stay on B. Proof: mapping the after-intake failure to before-intake fails `INV-A-SYNC-NEVER-LOSES-A-SAVE: a push cut after intake says the work came in: Failed(Refused(Sync(BeforeIntake(Unreachable))))`. The fake binary's control port takes `{"type":"breakOff","after":n}` (`a_break_off_fault_cuts_a_git_reply`). Both computers sign in as the parity user, and a sign-in replaces that user's token, so each signs in again before it syncs. Left in step 5: the toast's kept note (tc-ui) and the sync-button journey case, which need the real port in test builds (step 7) | cargo test -p tc-app --test app a_dropped_connection
- 2026-10-24 | C7a session (Claude Code 3058c665) | ruling | none | owner, through the orchestrator: a branch deleted on Door43 is put back by the next sync, with a notice tC4 4.0 does not give. tC4 has no words for it (its Upload flow says the upload worked; evidence/2026-10-21/gap-tc4-straggler-sync-safety.md state 3b; no string in tc4-sweep or tc4-rc2), so the approved sentence is the rebuild's: `syncResult.recreated` "The project was missing from Door43, so tC4 put your copy back. Ask your teammates to sync." (i18n-rebuild, pending in the nine other languages). `SyncResultView.recreated` (golden updated); the toast shows it first under the line with a Kindle dot and no button (`a_project_put_back_on_door43_says_so_with_a_kindle_dot_and_no_button`); docs/PLAN.md declares the difference. Asked the storage lane for `SyncReport.recreated` on track-X; until it lands the real port sets false (a shortcut comment in sync.rs) | cargo test -p tc-ui --test app sync
- 2026-10-25 | C7a session (Claude Code 3058c665) | note | none | X's tip 294649c4 merged (`SyncReport.recreated`; main batch 2b inside); conflicts in PLAN, tests/app/sync.rs and the fake's binary test kept both sides. The recreated shortcut is gone: the toast's `recreated` comes from X's report; `a_deleted_branch_is_put_back_and_the_toast_says_so_once` (the fake's branch deleted, the next sync puts it back and says so, the one after does not). Q7/D98 (orchestrator: signing in is the consent; one more sync only when the name is written the first time): the store's sync seam calls `tc_sync::record_username` after a confirmed sync and, when it wrote, syncs once more and adds that sync's received changes to the report; `the_first_sync_sends_the_username_at_once` reads Door43's copy after one sync. Proof: without the extra sync it fails `left: None right: Some(\"facilitator-zq\")`. 80 sync, share and contract tests, the fake's binary tests and tc-ui's sync tests pass; clippy clean | cargo test -p tc-app --test app port_contract
- 2026-10-25 | C7a session (Claude Code 3058c665) | note | none | main 1a0c8469 merged (batch 5: X, UI-3a, FIX-HIDE and FIX-HIDE2 are done); the card's depends now read UI-3a, as main's row does. UI-3a moved the sync button onto `run_net`, so step 3's UI half is done there. Step 4: `stale_scope_save_keeps_text_and_home_leaves_in_one_click` (tc-ui tests/app/save.rs) passes at once: a save refused as stale after a sync keeps its text, and once no open of the project is left the gate does not wait. #801's trap does not happen here, so no fix (FIX-R's `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` holds the general case). Proof: a gate that waits on any refused field fails it with `INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: Home is not held for an open that is gone`. Left: `a_refused_align_save_behind_a_span_save_lets_home_leave`, and every journey (steps 4 to 7), which needs tc4's test builds on the real port against the fake first (step 7's env-rust.mjs rewrite) | cargo test -p tc-ui --features tc-api/test-hooks --test app stale_scope_save_keeps
- 2026-10-25 | C7a session (Claude Code 3058c665) | step 7 (part) | none | journeys can sync for real against the fake: tc4 `--real-sync` (test-hooks builds) picks `Door43Sync` over the scripted port; the fake's control port takes `POST /repos {owner, name}` (an empty repository); `test/connectDoor43 {project, owner, name, branch}` points a project at it (golden added); env-rust's `realSync` (withApp `realSync: true`) makes `pushToDoor43` create and connect the repository instead of scripting a sync. New journey `E1a. A real sync pushes the project to Door43; a cut connection fails and changes nothing` (sync-button.test.mjs). It found a real bug: `Door43Remote::parse_url` read only https, so after a sync with an http Door43 (the fake; a development server) the card said the project was not shared and its button became Share; it reads http too now. sync-button and share-open `# tests 6 # pass 6 # fail 0 # skipped 0`; tc-api (golden), tc4, the fake and tc-app's home, sync, share, contract and golden-endpoint tests pass. Left in step 7: the scripted helpers that need a teammate (`teammatesChangeOneVerse`, `teammatesJoinVerses`, `loseFromDoor43`, `door43Lacks`, `dropNextClone`) and the captures, which need step 6's second app | .enjoy-logs/e1a-journeys.log
- 2026-10-25 | C7a session (Claude Code 3058c665) | step 6 | none | teammates for real: the fake's control port takes `POST /users {login, password, org}` (`FakeDoor43::add_member`: another user who may push in the org); env-rust's `startTeammate` is a second tc4 (own app data, empty projects folder, `--real-sync`) signed in as that user and driven through its API (`join`, `saveVerse`, `sync`, `words`); `withApp` gives a body `teammate(name)`. Shared projects live in the team org `Equipo-Ejemplo` (`DOOR43_TEAM`). Done differently from the card: no `withApps(n)` with n browsers; A is the browser, B and C are API-driven computers, which is what the convergence needs. `parity/app/journeys/team-sync.test.mjs`: `J26 [G1]` (B and C change 1:1 and 1:2 and sync; A's toast counts and names both; after one more round A, B and C hold the same two verses) and `J26 [G6]` (A and B change 1:3: one verse conflict, Review offered). Three runs in a row `# pass 1 # fail 0` for [G1], then both `# pass 2`. Proof: with C's sync left out, [G1] fails `the toast names teammate-c: ... Changes from: teammate-b`. The card's pack-drop proof was not built (the fake has no pack fault); choosing A's version stays C7a's review journeys; the check-conflict step waits for C7b (minors.tsv). sync-button and share-open still `# pass 6` | .enjoy-logs/team.log
- 2026-10-25 | C7a session (Claude Code 3058c665) | note | none | step 7 in progress. Real Door43 steps for `realSync` envs: `dropNextClone` (a `breakOff` on the next reference read), `door43Lacks` (nothing: the fake has no such repository), `teammatesChangeOneVerse` (A's project pushed once through the API, then signed out with the internet off so a capture still asks; two teammates open it, both change 1:1 from the same version, then each syncs), `loseFromDoor43` (the fake's branch moved back to before that teammate's push: control `POST /branch`, `FakeDoor43::set_branch`); a reset starts a fresh repository name (`env.door43Repo`). `capture-rust.mjs --real-sync`. All five screens capture on real syncs (`5 screens captured, 0 failed`) and are labelled E1a/store, but the report shows differences to fix or explain first, so the captures are not committed: (1) `6 changes from teammates` for two verses: X's `received_by` counts each teammate's DeviceProfile (username) record too; (2) the restored note never shows: X's report has no count of changes sent again, and the app passes 0; (3) `1 verse has two alignments to review` appears where tC4 shows none; (4) teammates named by username (Q7, intended, a residual to write); (5) the repository reads `Equipo-Ejemplo/sample_burrito-N` against tC4's `facilitator-zq/sample_burrito` (test setup: needs a collaborator in the fake or the parity user's own repository per reset). sync-button, share-open and team-sync `# tests 8 # pass 8` | .enjoy-logs/capture-e1a2.log
- 2026-10-25 | C7a session (Claude Code 3058c665) | step 7 | none | X's tip 654c5a0b merged: `received_by` leaves DeviceProfile out, and `SyncReport.restored` is wired into the toast like `recreated` (`a_change_door43_lost_is_sent_again_and_counted_once`; with the toast passed 0 it fails `restored: 0`). The captures' repository is now the parity user's own, named as the project (`facilitator-zq/sample_burrito`), replaced at each reset (fake `POST /repos {replace}`, `remove_repo`), with teammates pushing as collaborators (`add_member` on a user). The five screens are recaptured on real syncs (`5 screens captured, 0 failed`), all `('E1a', 'store')`; sync-failed and sync-ok match tC4 in DOM and meaning; sync-ok-teammates and sync-ok-restored differ in DOM and meaning, explained in residuals.json: a teammate's text change also rewrites the verse's alignment (twice tC4's counts), the two alignments are named (#791), teammates by username (Q7). E1a added to units.json. `screen_record` passes (48). `grep scriptSync parity/app/env-rust.mjs` still prints: the scripted steps stay for environments without `realSync` (conflict-review, guided-fix and the other journeys still script); moving those is left | cargo test -p tc-ui --test screen_record
- 2026-10-25 | C7a session (Claude Code 3058c665) | step 2 | none | H11 ruled (keyring, `rulings/2026-10-25-H11.md` on track-RULING-H11). `keyring =4.2.0` and `keyring-core =1.0.1` in `[workspace.dependencies]`. `OsKeychain` (account.rs) keeps the token's secret in one entry ("translationCore 4" / "Door43") through `keyring_core::Entry`; keyring's platform store (Windows Credential Manager, the macOS Keychain, the Secret Service) starts at the first call. No store or no access → `NoKeychain`; bytes that do not open → removed, `Unreadable`; no entry → none kept; anything else → `Failed` with the store's words. A real build's product keychain is `OsKeychain`; a build with the test hooks has none (`product_keychain`, item-level cfg), so no test touches the real store. `tests/os_keychain.rs` on `keyring_core::mock`; mapping `BadEncoding` elsewhere fails it at the unreadable assertion. Clippy clean (workspace target, release lib and bins, Windows gnu target); tc-app's account and keychain tests, tc4's tests and `j11-sign-in.test.mjs` `# tests 8 # pass 8 # fail 0 # skipped 0` | cargo test -p tc-app --test os_keychain
- 2026-10-25 | C7a session (Claude Code 3058c665) | step 8 | none | PLAN's #791 paragraph says the rebuild shows a count and Review, not one row per verse. TC4-ISSUES rows name E1a's tests: #801 (+ `stale_scope_save_keeps_text_and_home_leaves_in_one_click`), #791 both rows covered (`two_different_alignments_of_one_verse_are_listed_once`, new: A and B each change aligned 1:1, the toast counts one verse conflict and one verse with two alignments; journey J26 [G6]), #678 covered (`no_door43_request_at_launch_or_60_s_idle`, the real-sync journey), #724 (the Download journey, `a_failed_branch_lookup_is_a_failed_sync`), #681 covered (J26 [G1]), #761 (`a_story_project_syncs_like_a_bible`), #24 (J26 [G1], [G6]); #807 stays not applicable, so `a_refused_align_save_behind_a_span_save_lets_home_leave` is not written (the rebuild has no alignment-save queue behind a span save). Found and fixed: the toast's Download did nothing (`sync_download` was empty); it now opens the project and its Source texts, and the toast closes once they are open (journey `E1a. Download on the sync toast opens the project's Source texts and then closes the toast`). X's `docs/evidence/sync-<date>.md` does not exist yet (X step 8), so the G1/G6 app tests are named in #24's row instead. Claims, audit tools (39) and board pass; team-sync and sync-button journeys pass | node --test .audit/tools/*.test.mjs
- 2026-10-26 | loss lens (Claude Code subagent of 3058c665) | verify | 0eba9e40 | no blocker; no path loses or overwrites text, alignment, check or comment; saves during a sync take short read locks; AuthFailed keeps received work; a failed join leaves only staging; the token reaches no log or error string; http only for loopback. Major: quit during a sync took the store's write lock on a runtime worker, which on a one-CPU machine left the sync no worker for its network calls (reasoned). Minors: a failed username sync replaced the first sync's success; a name taken between the check and the placing showed as an internal error; the keychain entry might be tC4's; AfterIntake(AuthFailed) shows the sign-in, not the kept note | code reading; port_contract sync 42 passed
- 2026-10-26 | C7a session (Claude Code 3058c665) | fix | none | close takes the write lock on a blocking thread; a failed second (username) sync no longer replaces the first sync's outcome (the name goes out next time); `JoinError::Create(NameExists)` answers `OpenDoor43Refusal::NameExists`; tC4 rc2 has no keyring or credential-store code, so no shared entry. Left as minor: AfterIntake(AuthFailed) shows the sign-in step rather than the kept note | git grep keyring ~/unfoldingword/tc4-rc2
- 2026-10-26 | races and protocol lens (Claude Code subagent of 3058c665) | verify | cdc87260 | no blocker, no major. Lock order read lock then actor; close off the workers; a second sync refused by the FlowGate; door43_refused clears only its own token; keychain calls go through spawn_blocking except the launch read on the main thread; the fake's routes hold no lock across an await. Minors: while close waits for a running sync, a late read of that project waits behind it (quit only); the comment said a launch never touches the store, but the launch read does; remove_repo is blocking (test only) | code reading
- 2026-10-26 | browser and parity lens (Claude Code subagent of 3058c665) | verify | cdc87260 | Blocker: the records treated two alignments as a conflict to review, against ruling H5 (newest wins, the Received sheet says what was replaced). Major: the Download journey's last wait passed vacuously (the toast is drawn only on Home); the residuals' count of 4 claimed tC4's behaviour without evidence. Minors: some sync-button waits loose; J26 [G6] reads no texts back; Download keeps the toast until Source texts opens where tC4 closes it first (#724's own follow-up); with real sync, pushToDoor43 connects an empty repository where the capture steps say main exists | code reading
- 2026-10-26 | C7a session (Claude Code 3058c665) | fix | none | H5: PLAN's #791 paragraph says ruling H5 replaces the two-alignment note (C7b), the two #791 rows are back to before step 8 with the owner named (H5, C7b), `two_different_alignments_of_one_verse_are_listed_once` is dropped (it would hold the pre-H5 behaviour), and the residuals say the note goes when C7b lands. The count residual no longer claims tC4's count. The Download journey goes back Home and checks the toast is gone; with the clearing removed it fails (`# pass 0 # fail 1`). The keychain comment is right now. The close fallback for callers outside a runtime keeps tc-app's lib tests passing (88). Minors left as logged | node --test sync-button.test.mjs
- 2026-10-26 | C7a session (Claude Code 3058c665) | gate | none | hand-off checks with main (1a0c8469) in: fmt, clippy (workspace, release bins and app, Windows gnu), the five wasm clippy runs, `cargo test` of tc-app, tc-api (proptest), tc-ui (tc-api/test-hooks), tc4 and tc-door43-fake, journeys sync-button, share-open, team-sync, j11-sign-in, conflict-review and env-rust `# tests 38 # pass 38 # fail 0 # skipped 0`, screen_record 48, audit tools 39, board 0 errors. Pushed once | .enjoy-logs/handoff{,2,3}.log
- 2026-10-26 13:56 EDT | shell lane (Claude Code b97d18e4) | gate | 9fddd0fb | main (2947d486) merged on track-E1a2 for the next batch; five conflicts kept both sides (X's log, TC4-ISSUES #791 and #790, Cargo.lock's keyring, the captures' ui id); a duplicate git2 dev-dependency removed. scripts/gates.sh --touched passes (17 crates, journeys shell, sync-button and team-sync # pass 14), clippy, wasm-ui-app and clippy-release-app clean; share-open, j11-sign-in, conflict-review and env-rust # tests 32 # pass 32 # fail 0 # skipped 0 | scripts/gates.sh --touched
- 2026-10-26 15:25 EDT | checks lane (Claude Code 70ea6b18) | gate | none | main (86d669b1) merged on track-E1a2 for the next batch; 21 conflicts kept both sides: SyncResultView has recreated and replaced (golden, events and lastSync goldens, every initializer), the i18n keys of both, port_contract's E1a tests and F1's import contract, scripted.rs's imports, units.json lists F1 and E1a; ports.rs imports Feature again for F1's UnavailableImport; E1a's script_synced sets F1's kinds | scripts/gates.sh --touched
