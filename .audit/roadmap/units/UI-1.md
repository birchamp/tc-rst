---
id: UI-1
title: Typed jobs and one job follower
track: A
lane: app
milestone: M1
state: building
owner: C7a session (Claude Code 3058c665)
branch: track-UI-1
worktree: .worktrees/track-UI-1
depends: [INT-4, C7a]
needs_owner: []
proves: []
no_loss: false
size: 1 d (basis: +150/-200 lines estimated in health-app §8)
merged: none
updated: 2026-10-24
next: Step 7: the job journeys and the gates, then one review.
blocked_by: none
---

# UI-1: Typed jobs and one job follower

## Goal

Every job row of the endpoint table names the outcome it ends with, and the UI follows every
job through one function, `follow`, that answers `Done(outcome)`, `Failed(error)`,
`Cancelled`, `Lost` or `Left`. The four hand-written follow loops are gone. One `Latest` helper
replaces the nine thread-local counters that drop stale replies.

## Why it matters to a translator

Today a sync or an open from Door43 that is cancelled or lost tells the translator the feature
"is not available yet", which is false. Sync (E1a), share (E1b), USB exchange (E2), import (F1)
and export (D2) all start jobs: with one follower they all end the same, correct way.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| (INT-4|C7a) '` shows both rows as `done`.
- `grep -n "async fn job_end\|async fn follow_download\|async fn follow_job\|async fn follow_open" crates/tc-ui/src/app/web/*.rs`
  prints four lines (on resume-build-lane: app_ctx.rs:930, sources.rs:301, upgrades.rs:380,
  understand.rs:352). If C7a added a fifth loop, it joins step 4.
- `grep -n '^ *job ' crates/tc-api/src/lib.rs` prints the job rows (seven on resume-build-lane:
  `door43/open`, `door43/sync`, `session/open`, `resources/download`,
  `resources/planUpgrade`, `resources/planGateway`, `resources/fixFetch`).
- `grep -n "Unavailable(tc_api::error::Feature::Sync)" crates/tc-ui/src/app/web/app_ctx.rs`
  prints three lines (944, 954, 1000 on resume-build-lane): the wrong words this item removes.
- `grep -n "static [A-Z_]*: Cell<u64>" crates/tc-ui/src/app/web/*.rs` prints ten lines: the nine
  stale-reply counters (check.rs ITEMS, ARTICLES, OPENS; align.rs OPENS, READS, REFRESHES,
  NAVIGATIONS; understand.rs OPENS, ARTICLES) and fix.rs `NEXT_ID`, which is an id source and
  stays.

## Funnel card

- **Invariants**:
  - INV-ONE-JOB-FOLLOWER: the UI reads a job's state (`server.jobs` and `jobs/get`) in one
    function only, `follow`, and every caller handles each of its ends by an exhaustive match.
  - INV-JOB-OUTCOME-IS-TYPED: a job row's outcome type is fixed by its request type, so the
    follower of a download cannot receive an open's outcome, and the server cannot send one.
  - INV-A-STALE-REPLY-NEVER-SHOWS: a reply to an attempt that a newer attempt replaced changes
    nothing on screen.
- **Wrong states**:
  - A cancelled or lost sync or open from Door43 shown as `Unavailable(Feature::Sync)` ("not
    built yet") (app_ctx.rs:944, :954, :1000), for example when the app's server restarts
    during a sync.
  - A wrong outcome matched with a wildcard: `Ok(_) => this.open_refused(None, String::new())`
    (app_ctx.rs:902), an empty refusal.
  - The open of a book keeps polling when `jobs/get` answers `None`: `follow_open` ignores
    `Ok(None)` (understand.rs:385) and stops only when another open replaces it.
  - Four loops that each treat Cancelled, Lost and wrong outcomes differently; the next units
    would copy a fifth.
- **Code**:
  - Types: a trait `StartsJob: Write` (to create) in `crates/tc-api/src/endpoint.rs` with
    `type Outcome` and `fn outcome(JobOutcome) -> Result<Self::Outcome, JobOutcome>` and
    `fn wrap(Self::Outcome) -> JobOutcome`; the `job` arm of `write_impl!` asserts at compile
    time that every `job` row implements it. In tc-ui, `JobEnd<O>` (to create) with
    `Done(O)`, `Failed(ApiError)`, `Cancelled`, `Lost`, `Left`; a pure `FollowState` with one
    `step` function (to create, `crates/tc-ui/src/app/jobs.rs`). `Latest` and `Attempt` (to
    create, `crates/tc-ui/src/app/latest.rs`).
  - One entry point: `follow::<E: StartsJob>(job, on_progress, wanted)` (to create,
    `crates/tc-ui/src/app/web/jobs.rs`). On the server, one typed start for job rows:
    `Jobs::start_typed::<E>` (to create, next to `start_async` in `crates/tc-app/src/jobs.rs`).
  - Removed: `job_end` (app_ctx.rs), `follow_download` (sources.rs), `follow_job` and
    `follow_plan`'s loop (upgrades.rs), `follow_open` (understand.rs); the nine counters and
    their `next`/`current`/`bump`/`next_open`/`current_open`/`next_read`/`next_navigation`
    helpers.
  - Left to checks, and why: "only the follower polls jobs" is a rule about where code reads a
    signal; no type can stop another module reading `server.jobs`, so a source scan holds it.
- **Checks**:
  - `a_job_ends_once_with_the_state_the_server_gave` and
    `a_job_the_server_no_longer_knows_ends_lost` in `crates/tc-ui/tests/app/jobs.rs` (to
    create); kind: unit (host); runs in: CI job `rust`, step `cargo test --workspace`.
  - `the_follow_of_a_job_ends_with_one_end_on_every_sequence` in the same file (to create);
    kind: property (`proptest` over sequences of `Option<JobState>`); runs in: CI `rust`.
  - `only_the_follower_reads_jobs` in `crates/tc-ui/tests/app/lint.rs` (to create); kind:
    lint (source scan of `crates/tc-ui/src`, modelled on
    `crates/tc-app/tests/op_counts/lint.rs`); runs in: CI `rust`.
  - `a_newer_attempt_makes_an_older_reply_stale` in `crates/tc-ui/tests/app/latest.rs` (to
    create) and `no_hand_made_attempt_counters` in `lint.rs`; kind: unit and lint; runs in:
    CI `rust`.
  - Journeys `source-texts`, `upgrades`, `understand`, `guided-fix`, `sync-button`,
    `share-open` in `parity/app/journeys/`; kind: journey; runs in: the app-e2e job (G5);
    until G5 lands, locally, with the output in the log.
  - Proof it can fail: (1) add `js::sleep(50).await;` plus
    `self.s().server.jobs.with_untracked(|_| ())` to `crates/tc-ui/src/app/web/sources.rs`:
    `only_the_follower_reads_jobs` fails naming sources.rs. (2) In `step`, map
    `JobState::Cancelled` to `Lost`: the unit test fails. (3) Delete the `Ok(None)` arm in the
    follower: `a_job_the_server_no_longer_knows_ends_lost` fails. Record each output.
- **Prose**: a doc comment on `StartsJob` says why the outcome is typed per row (one line).
- **Human**: none. What a lost sync shows is decided in step 4 and recorded in the log.

## Steps

### Step 1: Type each job row's outcome in tc-api

- Do:
  1. Add `pub trait StartsJob: Write { type Outcome; fn outcome(outcome: JobOutcome) ->
     Result<Self::Outcome, JobOutcome>; fn wrap(outcome: Self::Outcome) -> JobOutcome; }` to
     `crates/tc-api/src/endpoint.rs`, exported from `crates/tc-api/src/lib.rs` beside `Write`.
  2. Implement it in `crates/tc-api/src/jobs.rs` for every job row. Outcome types, from what
     the server sends today: `OpenDoor43` -> `ProjectRef` (`JobOutcome::Created`);
     `SyncProject` -> `SyncResultView` (`Synced`); `session::Open` ->
     `Box<OpenedProject>` (`Opened`); `Download` -> `()` (`Finished`); `PlanUpgrade` and
     `PlanGateway` -> `Box<PlanView>` (`PlanReady`); `FixFetch` -> `()` (`Finished`). Confirm
     each with `grep -n "JobOutcome::" crates/tc-app/src/*.rs`.
  3. In the `(job $req:path)` arm of `write_impl!` (`crates/tc-api/src/endpoint.rs:299`),
     add a compile-time assertion that `$req: StartsJob`, so a new job row without an
     outcome type does not compile.
  4. Leave the wire format alone: `JobOutcome` and `JobState` keep their serde shapes.
- Files: `crates/tc-api/src/endpoint.rs`, `crates/tc-api/src/jobs.rs`, `crates/tc-api/src/lib.rs`.
- Done when: `cargo test -p tc-api` passes, including `tests/golden.rs` with no fixture
  change (`git diff --exit-code crates/tc-api/tests` prints nothing); and deleting one
  `impl StartsJob` makes `cargo check -p tc-api` fail with an error that names the trait.
- Record: a log line `step 1`; set `next` to step 2.

### Step 2: Make the server start job rows through the typed start

- Do:
  1. Add `start_typed::<E: StartsJob>(&self, spec: JobSpec, f)` and
     `start_typed_blocking::<E>` next to `start_async` and `start_blocking`
     (`crates/tc-app/src/jobs.rs:360`, `:338`), where `f` returns `Result<E::Outcome,
     JobEnd>`; they call the untyped starts with `E::wrap`.
  2. Move the handlers of the job rows onto them. Find them with
     `grep -rn "Handle<\(OpenDoor43\|SyncProject\|Open\|Download\|PlanUpgrade\|PlanGateway\|FixFetch\)>" crates/tc-app/src`
     (on resume-build-lane: share.rs:170, sync.rs:228, understand.rs:461, resources.rs:354,
     upgrades.rs:808 and :876, fixes.rs:167; the matches in story.rs and check.rs are other
     `Open` types, not job rows). Jobs that no endpoint row starts (seed, summary, train)
     stay on the untyped starts.
- Files: `crates/tc-app/src/jobs.rs` and the handler files the grep names.
- Done when: `cargo test -p tc-app` passes; inside the closures given to `start_typed`, no
  line names `JobOutcome::` (`grep -n "JobOutcome::" crates/tc-app/src/{share,sync,understand,resources,upgrades,fixes}.rs`
  prints only lines outside job closures, such as the test hooks' scripted plans).
- Record: a log line `step 2`; set `next` to step 3.

### Step 3: Write the pure follow state and its tests, red first

- Do:
  1. Create `crates/tc-ui/src/app/jobs.rs` (host-compiled, listed in
     `crates/tc-ui/src/app/mod.rs`): `pub enum JobEnd<O> { Done(O), Failed(ApiError),
     Cancelled, Lost, Left }`; `pub struct FollowState` (milliseconds without news); `pub fn
     step(&mut self, seen: Option<&JobState>, elapsed_ms: u32) -> Step` with `pub enum Step {
     Wait, AskServer, Progress(Progress), End(JobEnd<JobOutcome>) }`. Rules, from the four
     loops: `Done`/`Failed`/`Cancelled` end at once; `Running` reports progress; with no news
     for 1,000 ms, `AskServer`. Add `pub fn asked(&mut self, answer: Option<JobState>) ->
     Option<Step>` where `None` (the server does not know the job) ends `Lost`. `Left` is
     produced only by the web loop when `wanted()` turns false.
  2. Add `pub fn typed<E: StartsJob>(end: JobEnd<JobOutcome>) -> JobEnd<E::Outcome>`: an
     outcome of the wrong kind becomes `Lost` (it can only come from a server of another
     build, which the version check reloads).
  3. Create `crates/tc-ui/tests/app/jobs.rs` and add `mod jobs;` to `crates/tc-ui/tests/app.rs`.
     Write the three tests named in the funnel card. Run them before step 3.1's code exists
     (with `todo`-free stubs that return `Wait`) and see them fail; then implement.
- Files: `crates/tc-ui/src/app/jobs.rs`, `crates/tc-ui/src/app/mod.rs`,
  `crates/tc-ui/tests/app/jobs.rs`, `crates/tc-ui/tests/app.rs`.
- Done when: `cargo test -p tc-ui --test app jobs` passes, and failed before the
  implementation (paste both outputs' last lines in the log).
- Record: a log line `step 3`; set `next` to step 4.

### Step 4: Build the web follower and move the four loops onto it

- Do:
  1. Create `crates/tc-ui/src/app/web/jobs.rs` with `pub(super) async fn follow<E:
     StartsJob>(&self, job: JobId, on_progress: impl FnMut(&Progress), wanted: impl Fn() ->
     bool) -> JobEnd<E::Outcome>`: the only code that reads `server.jobs` for a job, sleeps
     50 ms (`js::sleep(50)`), and calls `jobs/get`. It merges a `jobs/get` answer with
     `crate::app::effects::merge_job` as the loops do today.
  2. Replace `job_end` (app_ctx.rs:930) in `open_door43_now` and `sync_now`. Match `JobEnd`
     exhaustively. `Cancelled`, `Lost` and `Left` never become `Unavailable(Feature::Sync)`:
     for both flows they re-enable the button, show no failure, and call
     `refresh_projects()` (the project list shows whether the open or sync took effect).
     Record this decision in the log.
  3. Replace `follow_download` (sources.rs:301), `follow_job` and `follow_plan`
     (upgrades.rs:370-420, also used by fix.rs:110), and `follow_open` (understand.rs:352).
     Keep each screen's progress text by passing its `on_progress`. Keep each caller's
     current meaning of `Cancelled` (the download's "Cancel" is a declared difference, B9a).
- Files: `crates/tc-ui/src/app/web/jobs.rs`, `crates/tc-ui/src/app/web/mod.rs`,
  `app_ctx.rs`, `sources.rs`, `upgrades.rs`, `fix.rs`, `understand.rs` in
  `crates/tc-ui/src/app/web/`.
- Done when: `grep -rn "js::sleep(50)\|\.jobs\.with_untracked(\|JobsGet {\|jobs::Get {" crates/tc-ui/src/app/web/`
  prints only lines in `web/jobs.rs` and two lines of `read_jobs` in `web/mod.rs` (a reread
  after a gap in the event stream, not a follow);
  `grep -n "Feature::Sync" crates/tc-ui/src/app/web/app_ctx.rs` prints nothing; `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings`
  passes.
- Record: a log line `step 4`; set `next` to step 5.

### Step 5: Add the source scan that keeps one follower

- Do: create `crates/tc-ui/tests/app/lint.rs` (add `mod lint;` to `tests/app.rs`). The test
  `only_the_follower_reads_jobs` reads every `.rs` file under `crates/tc-ui/src` and fails
  when `js::sleep(50)`, `jobs::Get {`, `JobsGet {` or `.jobs.with_untracked(` appears
  outside `src/app/web/jobs.rs`. Its allowlist has one function, `read_jobs` in
  `src/app/web/mod.rs` (it rereads every active job after a gap in the event stream and
  follows none), with that reason. The failure message starts with
  `INV-ONE-JOB-FOLLOWER:` and names the file and line.
- Files: `crates/tc-ui/tests/app/lint.rs`, `crates/tc-ui/tests/app.rs`.
- Done when: `cargo test -p tc-ui --test app lint` passes; the mutation in the funnel card
  makes it fail with `INV-ONE-JOB-FOLLOWER:` and the file name (paste it in the log).
- Record: a log line `step 5`; set `next` to step 6.

### Step 6: Replace the nine counters with one Latest helper

- Do:
  1. Create `crates/tc-ui/src/app/latest.rs` (host-compiled): `pub struct Latest(Cell<u64>)`
     with `begin(&self) -> Attempt` and `is_current(&self, Attempt) -> bool`; `Attempt` is
     `Copy` with no public constructor.
  2. Replace the counters in `web/check.rs` (ITEMS, ARTICLES, OPENS), `web/align.rs` (OPENS,
     READS, REFRESHES, NAVIGATIONS) and `web/understand.rs` (OPENS, ARTICLES) with one
     `Latest` each, held where the counter was. Delete the helper functions listed under
     "Removed". Pass `|| latest.is_current(attempt)` as the follower's `wanted` in
     understand.rs.
  3. Add `a_newer_attempt_makes_an_older_reply_stale` in `crates/tc-ui/tests/app/latest.rs`
     and `no_hand_made_attempt_counters` in `lint.rs`: `Cell<u64>` appears in
     `crates/tc-ui/src/app/web/` only in `fix.rs` (`NEXT_ID`, an id source). Failure message
     starts with `INV-A-STALE-REPLY-NEVER-SHOWS:`.
- Files: `crates/tc-ui/src/app/latest.rs`, `crates/tc-ui/src/app/mod.rs`,
  `crates/tc-ui/src/app/web/check.rs`, `align.rs`, `understand.rs`,
  `crates/tc-ui/tests/app/latest.rs`, `crates/tc-ui/tests/app/lint.rs`, `crates/tc-ui/tests/app.rs`.
- Done when: `grep -c "Cell<u64>" crates/tc-ui/src/app/web/*.rs | grep -v ':0'` prints only
  `crates/tc-ui/src/app/web/fix.rs:1`; `cargo test -p tc-ui --test app` passes.
- Record: a log line `step 6`; set `next` to step 7.

### Step 7: Run the job journeys and the full gates

- Do: build and run the journeys that start jobs (commands under "Gates before merge").
  Then run the full gate list.
- Files: none.
- Done when: the journeys print `# fail 0` and `# skipped 0`; every gate passes.
- Record: a log line `gate` with the journey summary lines; set `state: verifying` and `next`.

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

Plus the journeys that start jobs (Playwright must resolve: `npm ci` in
`parity/tc4-baseline` once, or set `PLAYWRIGHT_MODULE`):

```sh
cargo build -p tc4 --features test-hooks,fixture-backend
cargo build -p tc-door43-fake
crates/tc-ui/build-app.sh
TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
  node --test --test-reporter=tap parity/app/journeys/source-texts.test.mjs \
  parity/app/journeys/upgrades.test.mjs parity/app/journeys/understand.test.mjs \
  parity/app/journeys/guided-fix.test.mjs parity/app/journeys/sync-button.test.mjs \
  parity/app/journeys/share-open.test.mjs | tail -n 9
```

It must print `# fail 0` and `# skipped 0`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A caller's handling of `Cancelled` or `Lost` would change words or a screen a translator
  sees beyond removing the false "not available" (step 4.2). Ask with the options and a
  recommendation.
- C7a's merged code adds a job loop or a counter that this card does not list, and moving it
  would touch C7a's open review findings.
- A gate fails twice and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/health-app.md` §3, §8 items 1 and 11, CH-2, CH-14.
- `crates/tc-ui/src/app/web/app_ctx.rs:930-962` (`job_end`), `sources.rs:301-345`,
  `upgrades.rs:370-420`, `understand.rs:352-395`; `crates/tc-api/src/jobs.rs:149`
  (`JobOutcome`); `crates/tc-api/src/endpoint.rs:101` (`Endpoint`), `:286-302`
  (`write_impl!`); `crates/tc-app/src/jobs.rs:216` (`JobEnd`), `:360` (`start_async`);
  `crates/tc-ui/src/app/cache.rs:91` (`Ticket`, the query cache's own attempt mark).
- `docs/APP-DESIGN.md` §3.6 and §4.6 (jobs).
- `FUNNEL.md` §4 ("One entry point per job").

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/health-app.md
- 2026-10-24 | C7a session (Claude Code 3058c665) | claim | none | cut from track-C7a (d07d6612) on the orchestrator's word under the fast-release ruling (rulings/2026-10-24-H3-H4-H7-fast-release.md); C7a is not done yet, so the merge waits for it. The start checks print as the card says: four follow loops plus `follow_plan` (understand.rs:352, upgrades.rs:370 and :380, app_ctx.rs:930, sources.rs:301), seven job rows, three `Feature::Sync` lines (944, 954, 1000), ten `Cell<u64>` statics; C7a added no loop or counter | grep commands of "Before you start"
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | `StartsJob` in tc-api's endpoint.rs (exported beside `Write`), the seven job rows implemented in jobs.rs through one `starts_job!` table (OpenDoor43 -> ProjectRef, SyncProject -> SyncResultView, Open -> Box<OpenedProject>, Download and FixFetch -> (), PlanUpgrade and PlanGateway -> Box<PlanView>), and the job arm of `write_impl!` asserts `StartsJob` at compile time. With FixFetch's impl removed, `cargo check -p tc-api` fails `the trait bound FixFetch: StartsJob is not satisfied`. tc-api passes, golden fixtures unchanged, native and wasm clippy clean | cargo test -p tc-api --features proptest
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 2 | none | `Jobs::start_typed` and `start_typed_blocking` beside the untyped starts; the seven job rows' handlers (share, sync and its scripted sync, understand's open, resources' download, upgrades' two plans with their scripted plans, fixes) start through them and end with their row's own outcome; no line in those files names `JobOutcome::`. tc-app: 670 of 671 pass; the one failure, `a_successful_save_learns_its_sent_links_without_learning_a_newer_unsaved_edit`, hit its 20 s wall-clock timeout under the full run and passes alone twice (a C7a-base wait main's CHK-4 work already lengthens). Clippy clean with and without test-hooks | cargo test -p tc-app
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 3 | none | `tc_ui::app::jobs`: `JobEnd<O>`, `Step`, `FollowState::step` (a final state ends at once; running passes its progress; once a second the server is asked) and `asked` (unknown job ends `Lost`), and `typed::<E>` (another row's outcome ends `Lost`). Red first against stubs that only wait: `test result: FAILED. 3 passed; 3 failed`; then `test result: ok. 6 passed`. Proof: with Cancelled mapped to Lost, `a_job_ends_once_with_the_state_the_server_gave` fails `left: Some(Lost)` and the property fails too | cargo test -p tc-ui --test app jobs
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 4 | none | `Actions::follow::<E>` in web/jobs.rs is the one follower; `job_end`, `follow_download`, `follow_job` and `follow_open` are gone, and `follow_plan` is a thin generic over it (PlanUpgrade and PlanGateway). Every caller matches `JobEnd` exhaustively. Decision (step 4.2): a Door43 open or a sync that ends Cancelled, Lost or Left shows no failure; the button comes back (the open dialog stops being busy; the sync button is already reset) and the project list is read again, which shows whether it took effect. The download, the fix fetch and the plans keep their old meaning of a stop (`Dl` none, `FixScreen::stopped`, `Planned::Stopped`); the book open's stop ends the overlay, and a job the server forgot now ends it too instead of polling forever. The done-when grep prints only web/jobs.rs and `read_jobs`' two lines in web/mod.rs; no `Feature::Sync` in app_ctx.rs; wasm clippy clean with no feature, app, test-hooks and gallery; tc-ui host tests and clippy pass | cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 5 | none | `only_the_follower_reads_jobs` (tc-ui tests/app/lint.rs) scans src with white space squeezed out, so a call split over lines is found; allowed: app/web/jobs.rs, and app/web/mod.rs inside `read_jobs` only. Proof: a stray `js::sleep(50)` and `.jobs.with_untracked(` in sources.rs fail it with `INV-ONE-JOB-FOLLOWER: ... src/app/web/sources.rs:321 js::sleep(50)` and `:322 .jobs.with_untracked(` | cargo test -p tc-ui --test app lint
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 6 | none | `tc_ui::app::latest::{Latest, Attempt}` (begin, current, is_current; no public `Attempt` constructor) replaces the nine statics in check.rs (ITEMS, ARTICLES, OPENS), align.rs (OPENS, READS, REFRESHES, NAVIGATIONS) and understand.rs (OPENS, ARTICLES) and their helpers; the open's follow passes `|| OPENS.with(|l| l.is_current(attempt))` as `wanted`. The scan also found two attempt counters the card did not list, app_ctx.rs `compose` and `share_flows`: both are `Latest` now. Allowed with reasons: id sources (fix.rs NEXT_ID, mod.rs next_listener, saves.rs next_hook), mod.rs state_frames (dates a hello), app_ctx.rs sign_in_flows (the flow id the sign-in step keeps in its own state). Tests: `a_newer_attempt_makes_an_older_reply_stale`, `no_hand_made_attempt_counters`; proof: a stray `Cell<u64>` in check.rs fails `INV-A-STALE-REPLY-NEVER-SHOWS: ... src/app/web/check.rs:854`. tc-ui host tests, native clippy and the four wasm clippy runs pass | cargo test -p tc-ui --test app
