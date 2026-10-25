---
id: E1b
title: Share and open from Door43
track: A
lane: app
milestone: M3
state: building
owner: REF1 session (Claude Code 56b3f56d)
branch: track-E1b
worktree: .worktrees/track-REF1
depends: [E1a]
needs_owner: []
later: ["H14 from step 6"]
proves: [P1, P2]
no_loss: true
size: 3-4 sessions (basis: B9b)
merged: none
updated: 2026-10-25
next: Step 2: the real share and open in Door43Sync over X's share and join.
blocked_by: none
---

# E1b: Share and open from Door43

## Goal

Share creates the project's Door43 repository, pushes the project, and ends on a done step with
the team-access line for every project, Bible and Open Bible Stories alike. Open from Door43
brings a shared project to another computer; a private repository asks the sign-in once and
tries again. An open the app could not finish is removed at the next start, and Home says so
once. Journeys J11, J17 and J24 run end to end on the fake Door43.

## Why it matters to a translator

A facilitator shares the project so the team can work on it, and a translator on a new computer
opens it. Share must push exactly what the translator saw, and an interrupted open must never
leave a broken project under the project's name.

## Before you start

Line numbers are from `resume-build-lane` at f9067bc (main after INT-4); find moved ones by grep.

- `python3 -I .audit/tools/board.py | grep -E '^\| E1a \|'` shows `done`.
- UI-2 put share and open on `SyncPort` (`crates/tc-app/src/ports.rs`; APP-DESIGN.md:1002 "sync,
  share, join"); it has `sync` and `open`, and "Share's push is E1b's to add" (`units/UI-2.md`
  step 2). E1a put `Door43Sync` in `AppDeps::production()`.
- `grep -n '"door43/' crates/tc-api/src/lib.rs` shows only `door43/open` and `door43/sync`
  (lib.rs:122-123). `JobKind::Share` exists (`crates/tc-api/src/jobs.rs:32`); `enum Phase`
  (jobs.rs:90) has no share phase.
- B9b built the dialog up to the check step: `ShareStep { Target, Check }`
  (`crates/tc-ui/src/app/share.rs:55`), view `crates/tc-ui/src/views/modals/share.rs`.
- X step 6 (X-5) built share, join and connect in `crates/tc-sync/src/{share.rs,join.rs}`, with
  `sync_private_repository` (Q3), `team_sync_obs_project` (#761) and
  `a_join_cut_off_leaves_only_a_staging_folder_the_next_start_removes`.
- The four unported cases are in `parity/view-gallery/cases.mjs:497-511`
  (`share-progress-preparing`, `share-progress-create`, `share-progress-push`, `share-done`),
  with `domUnit` and `pixelsUnit` `E1b` in `parity/app/cases.json`; their rows in
  `parity/app/report.json` say `unported`.
- The start cleanup reports a removed `.staging-join-…` folder (`staging_report`,
  `crates/tc-app/src/backend/store.rs:819`; test `a_staging_folder_removed_at_start_is_reported`,
  `crates/tc-app/tests/app/store.rs:303`; tc-store's `Store::removed_at_start`). Home shows
  nothing of it today.
- `ls .audit/roadmap/rulings | grep -- -H14` tells whether H14 (#837) is answered (step 6).
- tC4 specs at b86270f: fetch as `units/E1a.md` says, then read `e2e/j11-share.spec.ts` and
  `e2e/j17-open-from-door43.spec.ts`. "The three variables" are as in `units/E1a.md`.

## Funnel card

- **Invariants**:
  - INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW: after a share, Door43's `main` equals the local
    `main` made by the leave checkpoint, and the check step listed exactly its books or stories.
  - INV-A-HALF-JOIN-NEVER-SITS-AT-THE-PROJECT-NAME: an open from Door43 is built in a staging
    folder and placed whole; an open cut short leaves nothing at the project's name, and Home
    says so once at the next start.
  - INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE: a 401 or 404 on an open without a sign-in asks the
    sign-in once and tries once more; a second 401 or 404 is the refusal, never a second ask.
- **Wrong states**:
  - Door43's `main` differs from the local one after a share (a push before the checkpoint).
  - A push over an existing repository of that name, a tC3-era one included (tC4 RISKS #6, #7).
  - A half project at the project's name after `tc4` ended during an open.
  - A sign-in loop on a private repository.
  - No team-access line on the done step of an OBS project (#724).
  - A project opened from Door43 with no pins stays without sources (#766).
  - The card shows the old title after an open under a new name (#837).
- **Code**:
  - Types: `SharePhase { Preparing, Create, Push }` (to create, a `Phase` variant in
    `crates/tc-api/src/jobs.rs`); the share dialog's state gains `Running(SharePhase)` and
    `Done` after `ShareStep::Check`, sent through UI-3's `Submit` (no `busy` bool); an open
    attempt enum `OpenTry { Anonymous, SignedIn }` (to create) so a retry after the sign-in
    cannot ask again; `StartNotice` (to create, `crates/tc-api/src/home.rs`), typed from
    `staging_report`, for Home's words about removed work.
  - One entry point: `SyncPort::share` (to create) behind the `door43/share` job (to create), and
    `SyncPort::open` behind `door43/open` (`crates/tc-app/src/share.rs:170`). The create step
    refuses a name in use before any push (`ShareRefusal::NameExists`,
    `crates/tc-api/src/refusal.rs:547`).
  - Removed: the env step `door43Lacks` (`parity/app/env-rust.mjs:301`), which scripts an open
    failure through `test/script`; a real fake answer replaces it.
  - Left to checks, and why: equality of Door43's `main` and the local `main` needs git on both
    sides; a test compares them.
- **Checks** (CI `rust` unless named a journey; journeys run in G5's `app-e2e`):
  - The `j14-join-isolation` case in `parity/app/journeys/open-door43.test.mjs` (to create, step
    4; the port of tC4's spec); journey.
  - In `crates/tc-app/tests/app/port_contract.rs`, new `sync_port_contract!` cases (to create),
    run on the scripted and the real port: `share_pushes_the_checkpointed_main`,
    `open_from_door43_equals_main` (`FakeDoor43::tree_of`, `crates/tc-door43-fake/src/lib.rs:742`);
    kind: contract.
  - `share_name_taken_on_door43_is_refused_before_any_push` (RISKS #7),
    `open_name_in_use_refused_before_request`, `open_private_repo_signs_in_and_retries` (to
    create) in `crates/tc-app/tests/app/share.rs`; kind: unit on the fake's `requests()`.
  - `a_join_cut_short_is_removed_and_home_says_so_once` and
    `opening_under_a_new_name_shows_that_name_on_its_card` (to create) in
    `crates/tc-app/tests/app/home.rs`; kind: unit.
  - `the_done_step_names_team_access_for_every_project` (to create) in
    `crates/tc-ui/tests/app/share.rs`; kind: unit.
  - Journeys (to create): `parity/app/journeys/share.test.mjs` (J11, J24) and
    `parity/app/journeys/open-door43.test.mjs` (J17).
  - Proof it can fail: each step names a mutation; paste the failing line into the log.
- **Prose**: `docs/TC4-ISSUES.md` rows #677 (:297), #372 (:330), #766 (:267, :302), #724 (:287),
  #761 (:283), and a row for #837.
- **Human**: H14 (#837: only the card shows the new name, or the project's own name too;
  ROADMAP recommends only the card). Step 6 waits for its ruling; the other steps do not.

## Steps

### Step 1: Add the share job with the scripted port, progress and done step

- Do: Add `door43/share` to the endpoint table (`crates/tc-api/src/lib.rs`, near :122) with
  `SharePhase`, add `"door43/share"` to `PHASE_0` in `crates/tc-api/tests/endpoints.rs`, and
  write its golden file `crates/tc-api/tests/golden/endpoints/door43/share.json` with
  `UPDATE_GOLDEN=1 cargo test -p tc-api --test golden`. Add `share` to `SyncPort`
  and to UI-2's `ScriptedSyncPort` (`crates/tc-app/src/ports/scripted.rs`). Draw the progress
  line (preparing, create, push) and the done step with the team-access line for every project,
  OBS included (#724; B9b D4). Follow the job with UI-1's `follow`. Add a `rust:` route to the
  four cases in `parity/view-gallery/cases.mjs`, add `E1b` to `parity/app/units.json`, run
  `node parity/app/capture-rust.mjs --unit E1b --case share-progress-preparing,share-progress-create,share-progress-push,share-done`,
  then `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`.
- Files: `crates/tc-api/src/{lib.rs,jobs.rs,door43.rs}`, `crates/tc-api/tests/endpoints.rs`,
  `crates/tc-api/tests/golden/endpoints/door43/`, `crates/tc-app/src/{ports.rs,share.rs}`,
  `crates/tc-app/src/ports/scripted.rs`, `crates/tc-ui/src/app/share.rs`,
  `crates/tc-ui/src/views/modals/share.rs`, `crates/tc-ui/tests/app/share.rs`,
  `parity/view-gallery/cases.mjs`, `parity/app/*`.
- Done when: `cargo test -p tc-api` and `cargo test -p tc-ui --test app share` pass;
  `cargo test -p tc-ui --test screen_record` passes and the four rows of `parity/app/report.json`
  are no longer `unported`; drawing the team-access line only for Bible projects fails
  `the_done_step_names_team_access_for_every_project`.
- Record: append a log line `step 1` (TRACKING.md format); update `next` and `updated`.

### Step 2: Build the real share and open

- Do: Implement `share` and finish `open` in `Door43Sync` over X's `share` and `join`. Share:
  checkpoint, create the repository (refuse `share.name-exists` before any push), push without
  force, done. Open: the name check before any request (share.rs:170-200 does the local half),
  a 401 or 404 asks the sign-in once through `AppCtx::run_net`, then one retry (`OpenTry`);
  staging, then place; creation pins at the first open when the repository has none (#766; the
  rule of `open_writes_creation_pins_only_when_none_exist`,
  `crates/tc-app/tests/app/store_scenario.rs:538`). Write the tests named in the Checks.
- Files: `crates/tc-app/src/ports/door43.rs` (E1a's `Door43Sync`), `crates/tc-app/src/share.rs`,
  `crates/tc-app/tests/app/{port_contract.rs,share.rs}`, `crates/tc-ui/src/app/open_door43.rs`.
- Done when: `cargo test -p tc-app --test app port_contract` and `cargo test -p tc-app --test app share`
  pass; running the push before the checkpoint fails `share_pushes_the_checkpointed_main` with
  `INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW`; removing the `OpenTry` check fails
  `open_private_repo_signs_in_and_retries` with `INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE`.
- Record: log line `step 2`; update `next` and `updated`.

### Step 3: Run J11 and J24 end to end

- Do: Port tC4 `e2e/j11-share.spec.ts` (b86270f) test 1 (:478, the remote `main` equals the
  local `main`), test 6 (:827, refusals, nothing pushed) and test 11 (:890, an OBS project: J24)
  to `parity/app/journeys/share.test.mjs`. The other j11 cases are in `j11-sign-in.test.mjs` and
  `share-open.test.mjs`, or do not apply: ruling Q2 removed the Upload changes dialog
  (`docs/APP-DESIGN.md:392-393`).
- Files: `parity/app/journeys/share.test.mjs`.
- Done when: with the three variables, `node --test parity/app/journeys/share.test.mjs` prints
  `# fail 0` and `# skipped 0` three runs in a row.
- Record: log line `step 3`; update `next` and `updated`.

### Step 4: Run J17, the private repository and the join cleanup

- Do: Port tC4 `e2e/j17-open-from-door43.spec.ts` (b86270f) :95 (open, then sync), :209 (`[G2]`
  an invalid journal file refuses the open and writes nothing) and :226 (#766) to
  `parity/app/journeys/open-door43.test.mjs`; port tC4 `e2e/j14-join-isolation.spec.ts` into
  the same file as the join-cleanup case (a second person who joins a team project sees none of
  the first person's local-only state; CHK-3's coverage map names this file for that spec); add a private-repository case (seed the repository
  with `private: true`; without a token the fake's git route answers 401 and its API 404,
  `crates/tc-door43-fake/src/api.rs:1553`, :925-935). Build `StartNotice`: `home/list` carries
  it in the launch whose start removed a `.staging-join-…` folder; Home shows `refusal.open-door43.removed`
  (`crates/tc-ui/assets/i18n/en.json:1030`) until dismissed; the next launch shows nothing. tC4
  says it when the person opens that project (`src/data/journal/opsLog.ts:317-318`); this app's
  join never puts a half project at the name, so Home says it (APP-DESIGN.md:3412). If F1
  already added `StartNotice`, add the join case to it. Replace `door43Lacks` with a real fake
  answer and update `share-open.test.mjs`.
- Files: `crates/tc-api/src/home.rs`, `crates/tc-app/src/home.rs`,
  `crates/tc-app/src/backend/store.rs`, `crates/tc-ui/src/views/home.rs`,
  `crates/tc-app/tests/app/home.rs`, `parity/app/env-rust.mjs`, `parity/app/journeys/`.
- Done when: `cargo test -p tc-app --test app home` passes; showing the notice on every launch
  fails `a_join_cut_short_is_removed_and_home_says_so_once` with
  `INV-A-HALF-JOIN-NEVER-SITS-AT-THE-PROJECT-NAME`; with the three variables,
  `node --test parity/app/journeys/open-door43.test.mjs parity/app/journeys/share-open.test.mjs`
  prints `# fail 0` and `# skipped 0`.
- Record: log line `step 4`; update `next` and `updated`.

### Step 5: Recapture the three Door43 screens on real data

- Do: Run `node parity/app/capture-rust.mjs --unit E1b --only share-dialog,share-dialog-check,open-door43-refused`,
  then `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`. Explain each remaining
  `differs` row in `parity/app/residuals.json`.
- Files: `parity/app/{captures.json,report.json,residuals.json}`, `parity/app/screens/`,
  `parity/app/dom/`.
- Done when: `cargo test -p tc-ui --test screen_record` passes, and
  `python3 -I -c "import json; s=json.load(open('parity/app/captures.json'))['screens']; print(sorted((x['name'],x['by'],x['backend']) for x in s if x['name'] in {'share-dialog','share-dialog-check','open-door43-refused'}))"`
  prints three rows, each with `'E1b'` and `'store'` (none `memory+scripted`).
- Record: log line `step 5`; update `next` and `updated`.

### Step 6: Show the new name on the card (#837), then write the records

- Do: Run `ls .audit/roadmap/rulings | grep -- -H14`. If it prints nothing, stop and ask (below)
  and do only the records. With a ruling, build what it says. For the recommended answer, Home's
  card shows the name typed at the open, and the project's own `identification.name` stays as
  Door43 has it (Home reads that today, `crates/tc-app/src/home.rs:207-213`). Write
  `opening_under_a_new_name_shows_that_name_on_its_card`. Update the TC4-ISSUES rows of the Prose
  line and add a row for #837.
- Files: `crates/tc-app/src/home.rs`, `crates/tc-app/tests/app/home.rs`, `docs/TC4-ISSUES.md`.
- Done when: the test passes, and fails with the old name source restored;
  `grep -n "#837" docs/TC4-ISSUES.md` prints a row.
- Record: log line `step 6` (and `ruling` when it arrives); set `next` to the gates.

## Gates before merge

The full gate list: `scripts/gates.sh` (to create in INT-3) once INT-3 lands. Until then:
`cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`;
`cargo test --workspace`; `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`
and the same with `--features gallery`, `--features app` and `--features test-hooks`;
`cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. Plus:

- The `share`, `open-door43`, `share-open`, `j11-sign-in` and `sync-button` journeys pass 3 runs
  in a row with `# skipped 0`, locally and in G5's `app-e2e` job once it exists.
- `cargo test -p tc-ui --test screen_record` passes.
- No-loss item: three verification lenses in review (FUNNEL.md §8 item 5).

## Stop and ask when

- Step 6 is reached and H14 has no ruling: ask H14 as ROADMAP.md words it, with its
  recommendation; merge without step 6's code only if the owner says so.
- The done step's team-access line would need words tC4 does not have.
- X's join cannot say which file it could not read (J17's `[G2]` case needs it).

## References

- `docs/APP-DESIGN.md:3412` (E1b row), :1002 (ports), :392-393 (Upload changes removed).
- `.audit/roadmap/evidence/2026-10-21/app-remaining.md` (E1b; F6, F14); `tc4-bugs.md` §2, §4
  (#837), §5 (RISKS #5-#8); `tc4-coverage.md` rows 44-46; `health-app.md` CH-6, CH-8.
- `units/UI-2.md`, `units/E1a.md`, `units/X.md` step 6.
- tC4 b86270f: `e2e/j11-share.spec.ts`, `e2e/j17-open-from-door43.spec.ts`, `docs/RISKS.md`
  rows 6 and 7.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/app-remaining.md
- 2026-10-25 16:10 EDT | REF1 session (Claude Code 56b3f56d) | claim | none | Claimed at the audit session's request before E1a merges: cut from track-E1a at e5b30962 (journeys sync for real against the fake), so E1a's later commits come in by merge; the app lane owns share-open.test.mjs and env-rust.mjs until it says otherwise | git log --oneline -1 e5b30962
- 2026-10-25 18:40 EDT | REF1 session (Claude Code 56b3f56d) | step 1 | c55e1eb5 | door43/share with Phase::Share(Preparing, Create, Push) and JobOutcome::Shared on SyncPort::share (scripted queue; the real port answers Unavailable until step 2); the dialog runs through UI-3's Submit with Running(SharePhase) and Done(SharedRepo), no footer or close while it runs, a refusal back on the check step. Merged track-UI-3b (dialogs in their Dialog variant) and track-E1a at 85f11a81. The four share cases are captured on the store (DOM same as tC4's, pixels only the Home cards' glyph anti-aliasing, residuals as share-dialog-check's). Declared difference: the team-access line on every project (tC4 left it off OBS, #724). Proof: drawing it only for Books fails `the_done_step_names_team_access_for_every_project`: 'Stories: teammates need write access to sync, stories too (#724)' | cargo test -p tc-ui --features tc-api/test-hooks --test app share; cargo test -p tc-ui --test screen_record
