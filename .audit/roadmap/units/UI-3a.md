---
id: UI-3a
title: One leave path, one network-call helper, one refusal text, Submit
track: A
lane: app
milestone: M1
state: done
owner: checks lane (Claude Code 2198ee19)
branch: track-UI-3a
worktree: .worktrees/track-UI-3a
depends: [UI-1]
needs_owner: []
proves: []
no_loss: true
size: 1 d (steps 1-4 of UI-3, built)
merged: 51e5edbc
updated: 2026-10-24
next: none
blocked_by: none
---

# UI-3a: One leave path, one network-call helper, one refusal text, Submit

## Goal

The UI glue gets one way to do each of six things the remaining units would otherwise copy:
leave a screen (`leave`: save gate, then checkpoint, then switch), make a network call
(`run_net`), put a refusal into words (`refusal_text`), send a dialog's request once per
attempt (`Submit`), hold a dialog's form (inside its `Dialog` variant), and edit a project
(only with a `WriteCap`). Each is held by a type or a source-scan test.

## Why it matters to a translator

Leaving a screen must never lose typed text, and D1 adds a checkpoint at every leave. A
double click or a lost reply must never start a second open or sync. Every refusal must read
as a whole sentence, never with `{reason}` left in it. In a read-only project, align and check
controls must be off instead of failing after the click.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| UI-1 '` shows `done`.
- `grep -rn "\.gate()" crates/tc-ui/src/app/web/`: nine product sites (on resume-build-lane
  app_ctx.rs:283, :305; upgrades.rs:135, :268; understand.rs:303, :497, :516, :551;
  fix.rs:185), the test-hooks probe page (mod.rs:643, probe.rs:249) and `saves.rs` itself.
- `grep -n "KeepPhase" crates/tc-ui/src/app/review.rs` prints C7a's enum (`Idle`,
  `InFlight(SentKeep)`, …); step 2b generalises it.
- `grep -rn "ApiError::NeedsInternet\|ApiError::NeedsSignIn" crates/tc-ui/src/app/web/`: 17
  lines (app_ctx.rs 10, sources.rs 3, upgrades.rs 3, fix.rs 1).
- `grep -rn "ApiError::BadRequest {" crates/tc-ui/src/app/web/`: transport errors relabelled
  at app_ctx.rs:885, :987, understand.rs:318 and sources.rs:46 (`api_error`).
- `grep -rn "\.vars()" crates/tc-ui/src`: seven refusal-to-words sites (web/create.rs:31,
  web/sources.rs:31, web/understand.rs:135, web/app_ctx.rs:910, views/modals/sign_in.rs:41,
  views/home.rs:925, app/sync.rs:193); app_ctx.rs:1317 passes no vars at all.
- `grep -rn "Confirm::new" crates/tc-ui/src` prints nothing (only
  `crates/tc-ui/tests/app/recovery.rs` uses `Confirm`); `grep -rn "write_cap" crates/tc-ui/src`
  prints only `saves.rs`, `translate.rs` and `views/comment_box.rs`.

## Funnel card

- **Invariants**:
  - INV-ONE-LEAVE-PATH: every move away from a screen or project runs `leave`, which runs the
    save gate, then the checkpoint, then the switch; no other product code calls `gate()`.
  - INV-NET-REFUSALS-HANDLED-ONCE: `NeedsInternet` and `NeedsSignIn` are turned into the
    internet question and the sign-in step in one function, and a failure below the API stays
    a transport failure.
  - INV-EVERY-REFUSAL-RENDERS: every refusal's words have every placeholder filled.
  - INV-ONE-REQUEST-PER-ATTEMPT: one attempt of a dialog or button sends one request id; a
    resend after a lost reply reuses it, so the server's reply cache answers it.
  - INV-READ-ONLY-HAS-NO-WRITE-CONTROLS: align and check edit actions cannot be called
    without a `WriteCap`, so their controls are off in a read-only project (not in code: the `WriteCap` type holds it; a call without one does not compile).
- **Wrong states**:
  - D1 adds a checkpoint at one leave site and misses another (nine sites today).
  - A lost reply to Open from Door43 or Sync, then a second press, starts a second job (a new
    id per press, app_ctx.rs:878, :981).
  - A refusal with a placeholder shown through `share_problem` (app_ctx.rs:1317) shows a raw
    `{…}` (E1b's `ShareRefusal::PushFailed` has `{reason}`, refusal.rs:582); Open from Door43
    shows the var values joined by spaces in place of the sentence (app_ctx.rs:906-913).
  - The server not answering is shown as a bad request.
  - Two dialogs' forms set at once, or a form left behind after its dialog closed
    (`close_modal` resets seven slots by hand, app_ctx.rs:786-797).
  - In a read-only project an align drag or a check mark is taken on screen, then refused by
    the server (`crates/tc-app/src/align/actor.rs:333`).
- **Code**:
  - Types: `LeaveTo` (to create; variants for each leave: a tab, Home, a book, a chapter, a
    story, an open, a plan); `NetEnd<T>` (to create; `Ok(T)`, `Asked`, `Failed(ApiError)`,
    `Transport(Transport)`, `Dropped`); `Submit` (to create, `crates/tc-ui/src/app/submit.rs`,
    built on `Confirm`); `Dialog` carries its form (`Dialog::Import(ImportForm)`, …) and a
    `DialogKind` (the old `Copy` enum) for the outer switch; `WriteCap` as an argument.
  - One entry point: `AppCtx::leave(LeaveTo, then)`; `AppCtx::run_net(pending, request)`;
    `tc_ui::i18n::refusal_text(&dyn Message)`.
  - Removed: every site listed under "Before you start"; the `busy` of `OpenForm` and
    `ImportForm`; the slots `np`, `ab`, `im`, `sh`, `od`, `st` and `close_modal`'s resets.
  - Left alone: `SaveMachine`'s `Field` internals (`crates/tc-ui/src/app/save/mod.rs:231`,
    A4b no-loss code under property tests). `leave` calls `Saves::gate` as it is. `src`
    (Get source texts) stays its own slot: tC4 keeps that form across openings.
  - Left to checks, and why: where `gate()` and `NeedsInternet` may appear is a rule about
    call sites; no type can stop a call, so source scans hold it.
- **Checks**:
  - `only_leave_calls_the_gate`, `only_run_net_handles_net_refusals`,
    `only_refusal_text_reads_vars` in `crates/tc-ui/tests/app/lint.rs` (UI-1 created it);
    kind: lint; runs in: CI job `rust`, `cargo test --workspace`.
  - `every_refusal_renders_with_no_placeholder_left` in `crates/tc-ui/tests/i18n_runtime.rs`
    (to create); kind: property (`any::<Refusal>()` with var values that hold no `{`, every
    locale's catalog installed); runs in: CI `rust`.
  - `a_lost_reply_resends_the_same_request` and `a_refused_attempt_takes_a_new_request` in
    `crates/tc-ui/tests/app/submit.rs` (to create); kind: unit (host); runs in: CI `rust`.
  - `parity/app/journeys/read-only.test.mjs` (to create), beside the compile-time rule; kind:
    journey; runs in: the app-e2e job (G5); until G5 lands, locally, output in the log.
  - The existing no-loss checks pass unchanged: `crates/tc-ui/tests/app/save_props.rs`,
    `save.rs`, `save_refresh.rs`, and the journeys `translate`, `section`, `shell`.
  - Proof it can fail: (1) put `this.0.runtime.saves().gate().await` back into `go`:
    `only_leave_calls_the_gate` fails naming app_ctx.rs. (2) Make `refusal_text` pass `&[]`
    as vars: the property test fails on a `ShareRefusal::PushFailed`. (3) Make `Submit::lost`
    draw a new id: `a_lost_reply_resends_the_same_request` fails. (4) Draw the mark buttons
    enabled whatever the cap: the read-only journey fails. Record each output.
- **Prose**: none beyond doc comments on the new types.
- **Human**: none. Controls off in a read-only project is `docs/APP-DESIGN.md` §6.4's design
  (`WriteCap`, A-10); tC4 has no read-only mode.

## Steps

Steps 1-4 of UI-3, which split on 2026-10-24 (orchestrator): D1, E1a and E2 need these, while the
dialog refactor and WriteCap (UI-3b) are still being built.

### Step 1: Make one leave path, red first

- Do:
  1. Add `only_leave_calls_the_gate` to `crates/tc-ui/tests/app/lint.rs`: `.gate()` appears
     under `crates/tc-ui/src/app/web/` only in the file that holds `leave`, in `saves.rs`
     (the gate itself and the window-close guard, which D1's quit flow takes over), and in the
     test-hooks probe page (`mod.rs` `ProbeRoot`, `probe.rs`), each with its reason. Message
     starts with `INV-ONE-LEAVE-PATH:`. Run it: it fails on the nine sites.
  2. Create `crates/tc-ui/src/app/web/leave.rs` with `pub enum LeaveTo` and
     `pub(crate) fn leave(&self, to: LeaveTo, then: impl FnOnce(&AppCtx) + 'static)`: spawn,
     `saves().gate().await`, stop if false, then `checkpoint(&to)` (an empty function with a
     doc line "D1 fills this in"), then `then(self)`.
  3. Move the nine sites onto it: `go` and `back_to_projects` (app_ctx.rs:279-310),
     `open` (understand.rs:290), `open_book`, `set_chapter`, `open_story` (understand.rs
     around :497, :516, :551), the plan starts in upgrades.rs (:135, :268) and `fix_repin`
     (fix.rs:185). The plan starts use `LeaveTo::Plan`: a plan reads the saved project, so
     everything is saved first.
- Files: `crates/tc-ui/src/app/web/` (leave.rs, mod.rs, app_ctx.rs, understand.rs,
  upgrades.rs, fix.rs), `crates/tc-ui/tests/app/lint.rs`.
- Done when: `cargo test -p tc-ui --test app lint` passes and failed before step 1.3 (paste
  both in the log); `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings`
  passes; the journeys `shell`, `translate`, `section` and `understand` pass.
- Record: a log line `step 1`; set `next` to step 2.

### Step 2: Make one network-call helper

- Do:
  1. In `crates/tc-ui/src/app/web/app_ctx.rs` (beside `ask_again`, :504, and
     `needs_sign_in`, :1145), add `pub(super) fn net_refusal(&self, pending: Pending, error:
     ApiError) -> Option<ApiError>`: `NeedsInternet` runs `ask_again(pending)`,
     `NeedsSignIn` runs `needs_sign_in(pending)`, both answer `None`; any other error comes
     back. Add `pub(super) async fn run_net<E: Endpoint>(&self, pending: Pending, request:
     &E) -> NetEnd<E::Ok>` on top of it: `CallError::Transport` stays `NetEnd::Transport`.
     Match `ApiError` and `CallError` without a wildcard.
  2. Replace the 17 hand-made arms (see "Before you start") with `run_net`, or with
     `net_refusal` where the refusal comes from a job's `JobEnd::Failed` (sources.rs:284,
     fix.rs:123, upgrades.rs:175). Delete `api_error` (sources.rs:43-50) and the three other
     `BadRequest` relabels; a transport failure shows the screen's existing "could not reach
     the app" text, or the transport error's own words where the screen has none today.
  2b. `review.rs`: C7a's `KeepPhase::InFlight/ReplyLost/Answered` (the first network-reply
     state in the UI, `units/C7a.md` step 1) holds a `NetEnd<KeepOutcome>` instead of its own
     outcome shape; keep C7a's rule that a lost reply resends the same request (A-27);
     `grep -n "CallError::Transport" crates/tc-ui/src/app/review.rs` prints nothing afterwards.
     C7a's log has a `note` that UI-3 absorbs `KeepPhase`, so this is expected, not a rewrite.
  3. Add `only_run_net_handles_net_refusals` to `lint.rs`: the strings
     `ApiError::NeedsInternet` and `ApiError::NeedsSignIn` appear under
     `crates/tc-ui/src/app/web/` only in `net_refusal`'s file. Message starts with
     `INV-NET-REFUSALS-HANDLED-ONCE:`.
- Files: `crates/tc-ui/src/app/web/` (app_ctx.rs, sources.rs, upgrades.rs, fix.rs,
  understand.rs), `crates/tc-ui/tests/app/lint.rs`.
- Done when: `grep -rn "ApiError::BadRequest {" crates/tc-ui/src/app/web/` prints nothing;
  `cargo test -p tc-ui --test app lint` passes; the journeys `internet-consent`,
  `j11-sign-in`, `account-menu`, `source-texts`, `upgrades`, `guided-fix`, `sync-button`,
  `share-open` pass.
- Record: a log line `step 2`; set `next` to step 3.

### Step 3: Make one refusal text, and test that every refusal renders

- Do:
  1. Add `pub fn refusal_text(message: &dyn tc_api::refusal::Message) -> String` to
     `crates/tc-ui/src/i18n.rs`: `t_dyn(message.key(), <all of message.vars()>, None)`.
  2. Use it at the seven `.vars()` sites and at app_ctx.rs:1317. Each caller keeps what it
     adds today (`detail()` in create.rs and understand.rs, `kept_note` in app/sync.rs).
     app_ctx.rs:906-913 (Open from Door43) passes `refusal_text(&refusal)` to `open_refused`
     instead of the var values joined by spaces.
  3. Add `every_refusal_renders_with_no_placeholder_left` to
     `crates/tc-ui/tests/i18n_runtime.rs`: for each id of `LOCALE_IDS`, install its catalog
     as that file's `catalog(id)` helper builds it, then for 256 cases of `any::<Refusal>()`
     whose var values hold no `{`, assert `!refusal_text(&r).contains('{')`. Message starts
     with `INV-EVERY-REFUSAL-RENDERS:` and names the key and the locale.
  4. Add `only_refusal_text_reads_vars` to `lint.rs`: `.vars()` appears under
     `crates/tc-ui/src` only in `i18n.rs`.
- Files: `crates/tc-ui/src/i18n.rs`, the seven sites' files, app_ctx.rs,
  `crates/tc-ui/tests/i18n_runtime.rs`, `crates/tc-ui/tests/app/lint.rs`.
- Done when: `cargo test -p tc-ui --test i18n_runtime --test app` passes; proof mutation (2)
  fails it (paste the message in the log).
- Record: a log line `step 3`; set `next` to step 4.

### Step 4: Build Submit on Confirm and send one request per attempt

- Do:
  1. Create `crates/tc-ui/src/app/submit.rs` (host-compiled): `Submit` holds a `Confirm` and
     the problem to show. `press() -> Option<RequestId>`; `lost() -> RequestId` (the same
     id, resent); `answered()`; `refused(problem, next: RequestId)` shows the problem and
     makes the next press a new attempt with `next`. Write
     `crates/tc-ui/tests/app/submit.rs` (`a_lost_reply_resends_the_same_request`,
     `a_refused_attempt_takes_a_new_request`, and `a_second_press_in_flight_sends_nothing`),
     red first.
  2. Put `OpenForm` (`crates/tc-ui/src/app/open_door43.rs:31`, drop its `busy`), the Sync
     button (`syncing`, app_ctx.rs:141, `sync_now` at :971) and `ImportForm`
     (`crates/tc-ui/src/app/forms.rs:537`, drop its `busy`) on `Submit`. A transport failure
     (`NetEnd::Transport`) calls `lost()`, so the next press resends the same id.
  3. Leave `NewProjectForm`, `AddBookForm` and `SettingsForm` as they are: they already keep
     one id per press (`press(js::random_request_id)`, web/create.rs:78, :182, :280, :340)
     and no remaining unit touches them. Append one line for them to `.audit/minors.tsv`.
     E1b builds the share dialog's press on `Submit`.
- Files: `crates/tc-ui/src/app/` (submit.rs, mod.rs, open_door43.rs, forms.rs,
  web/app_ctx.rs), `crates/tc-ui/tests/app/submit.rs`, `crates/tc-ui/tests/app.rs`,
  `.audit/minors.tsv`.
- Done when: `cargo test -p tc-ui --test app submit` passes and failed before 4.1's code;
  `grep -n "random_request_id" crates/tc-ui/src/app/web/app_ctx.rs` no longer lists the
  lines inside `open_door43_now` and `sync_now`; journeys `share-open` and `sync-button` pass.
- Record: a log line `step 4`; set `next` to step 5.


### Step 7: Run all journeys and the full gates

- Do: run the full gate list and every journey (below).
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

Plus every journey, since the leave path and the network helper touch every screen
(Playwright must resolve: `npm ci` in `parity/tc4-baseline` once, or set `PLAYWRIGHT_MODULE`):

```sh
cargo build -p tc4 --features test-hooks,fixture-backend
cargo build -p tc-door43-fake
crates/tc-ui/build-app.sh
crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks
TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
  TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
  node --test --test-reporter=tap parity/app/journeys/*.test.mjs | tail -n 9
```

It must print `# fail 0` and `# skipped 0`. This is a no-loss item: the review runs the
three verification lenses of `FUNNEL.md` §8 item 5, and `cargo test -p tc-ui --test app
save_props` must pass unchanged.

## Stop and ask when

- A step would change `SaveMachine`'s `Field` or `FieldSave` (frozen A4b code).
- The read-only journey shows a control a translator uses in a read-only project today (for
  example a check's comment box or the align suggestions switch) that this card would turn
  off: ask, with the options and a recommendation.
- A screen's transport-failure text would change words a translator sees beyond replacing a
  "bad request" message.
- A gate fails twice and you cannot tell why.

## References

- `.audit/roadmap/evidence/2026-10-21/health-app.md` §3, §6 ("Read-only"), §8 items 4-9,
  CH-5, CH-6, CH-7, CH-8, CH-10.
- `crates/tc-ui/src/app/web/app_ctx.rs` (Dialog :47-72, AppState :104-133, close_modal
  :786-797, open_door43_now :859, sync_now :971, share_problem :1315); `saves.rs:114`
  (`WriteCap`), `:431`, `:454` (`gate`); `crates/tc-ui/src/app/confirm.rs`;
  `crates/tc-api/src/refusal.rs:17` (`Message`).
- `docs/APP-DESIGN.md` §6.4 (`WriteCap`, :2089-2096), §6.5; `FUNNEL.md` §4.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/health-app.md
- 2026-10-24 15:03 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on the orchestrator's assignment (UI-3 is on the critical path for D1, E1a and E2), on branch track-UI-3 from origin/track-UI-1 at efe271cd (UI-1 done, in merge batch 2). UI files only; the app lane does UI-2 in server files at the same time | python3 -I .audit/tools/board.py
- 2026-10-24 16:07 EDT | checks lane (Claude Code 2198ee19) | step 1 | c2944312 | only_leave_calls_the_gate failed first on the nine sites (app_ctx.rs:284, :306; fix.rs:193; understand.rs:292, :456, :475, :510; upgrades.rs:137, :270), then passed once each went through Actions::leave(LeaveTo) in web/leave.rs (gate, then checkpoint, which D1 fills). Declared change from the card: leave is an async fn answering whether to switch, not a closure taker, since several sites do work when the gate holds them back and most go on with async calls; every site keeps its own switch after it. The four tc-ui wasm32 clippy runs pass. Journeys (all 218, as the card's step 7 runs them): 215 passed; the 3 that failed passed in two reruns of their files (guided-fix needed fflate, missing from this worktree's node_modules; understand's under-300-ms overlay check and a section draft check under a loaded machine) | cargo test -p tc-ui --test app lint; node --test parity/app/journeys/*.test.mjs
- 2026-10-24 16:07 EDT | checks lane (Claude Code 2198ee19) | step 2 | c2944312 | Actions::net_refusal (every ApiError variant named) and run_net (NetEnd: Ok, Asked, Failed, Transport, Dropped; NetEnd lives in app/net.rs so host code uses it) replace the hand-made NeedsInternet and NeedsSignIn arms in app_ctx.rs, sources.rs, upgrades.rs and fix.rs, and the job ends of sources, upgrades, fix, Open from Door43 and Sync. api_error and the BadRequest relabels are gone: a transport failure shows its own words (said_call, NetEnd::said), and Sync shows its fixed "unexpected" sentence through a new SyncEnd::Unreached. 2b: review.rs's keep_reply takes a NetEnd; KeepPhase keeps its states (A-27's resend is unchanged); grep CallError::Transport in review.rs prints nothing. only_run_net_handles_net_refusals passes, and fails naming sources.rs when a screen matches ApiError::NeedsInternet itself. The remaining grep hit for "ApiError::BadRequest {" is net_refusal's match arm, not a relabel | cargo test -p tc-ui --test app lint
- 2026-10-24 16:07 EDT | checks lane (Claude Code 2198ee19) | step 3 | none | i18n::refusal_text is the one reader of Message::vars; the seven sites and share_problem use it, and Open from Door43 shows the sentence instead of the values joined by spaces. every_refusal_renders_with_no_placeholder_left passes (256 cases, every locale); with refusal_text passing no values it fails: "INV-EVERY-REFUSAL-RENDERS: upload.notWritten in en: \"The file could not be copied into translationCore: {reason}. Nothing was changed.\"". only_refusal_text_reads_vars passes | cargo test -p tc-ui --test i18n_runtime every_refusal
- 2026-10-24 16:07 EDT | checks lane (Claude Code 2198ee19) | step 4 | none | app/submit.rs: Submit on Confirm (press, lost, not_sent, next, refused, answered). The three tests failed first (no module), then pass; with lost drawing a new id, a_lost_reply_resends_the_same_request fails ("left: Some(RequestId(0909…)) right: Some(RequestId(0101…))"). OpenForm and ImportForm hold a Submit (their busy flags are gone; OpenForm::new and ImportForm::open take the first attempt's id, so no dialog shares a default id); Sync keeps one Submit per project. A transport failure leaves the next press with the same request; the internet question, the sign-in step, a refusal and a run that ended start a new attempt, since the server caches a refusal too. random_request_id still appears in open_door43_now and sync_now, for the next attempt only. New Bible, Add a book and Settings stay as they are (minors.tsv) | cargo test -p tc-ui --test app submit
- 2026-10-24 16:24 EDT | checks lane (Claude Code 2198ee19) | verify | none | Review of steps 1-4 (no-loss lens first, one subagent, read only): no blocker, no major. The nine leave sites keep their hold-back code and switch only after the gate passes; every network site keeps its question and sign-in behaviour; no request id is reused after an answered refusal. Minors: an edit after a lost reply kept the request id, so a changed body could go under the old id (RequestReused, or two projects); after a lost reply the callout still says "No project was added"; the leave lint proves no other gate caller, not that every move leaves (open_check_tool, close_check_tool, start_aligning never gate, as before); the net-refusal lint misses glob imports and plain call sites; the vars lint misses a key rendered with no values (feedback.rs:230, harmless today); the property test covers Refusal only, not UnusableReason; Import's Submit has nothing to send until F1 | review report in the session transcript
- 2026-10-24 16:24 EDT | checks lane (Claude Code 2198ee19) | fix | none | Submit::edited: a change to Open from Door43's form while nothing is in flight makes the next press a new attempt (Actions::edit_od); a_changed_form_after_a_lost_reply_is_a_new_attempt. The other minors stay: the lint gaps and the leave moves that never gated are for D1 to meet, and the lost-reply wording predates this card | cargo test -p tc-ui --test app submit
- 2026-10-24 17:24 EDT | checks lane (Claude Code 2198ee19) | note | none | Split from UI-3 on the orchestrator's word: this card holds steps 1-4 and the review's fix (Submit::edited, Actions::edit_od, a_changed_form_after_a_lost_reply_is_a_new_attempt); UI-3b holds steps 5-6, built in part on track-UI-3b (f0d8e801). This branch also makes CI cancel a superseded run on a track branch but never on main or a gate/* branch (orchestrator, CI congestion). Main is not merged in: UI-1, its base, is not on main yet, and the conflicts (minors.tsv, tc-app upgrades.rs) are UI-1's batch's | none
- 2026-10-24 18:45 EDT | checks lane (Claude Code 2198ee19) | verify | none | Races and protocol (second lens, one subagent, read only): Submit against the reply cache's rules (handler refusals and job handles cached, pre-start NeedsInternet not, RequestReused on a changed body), every NetEnd and JobEnd exit of Open from Door43 and Sync, presses while the question or sign-in step is up, late replies after a dialog closed, share's order with end_share, leave pressed twice, server restart. No blocker, no major. Minors: after a lost Sync reply a much later press replayed the old job (a stale "Synced" toast, new work unsynced); after a lost Open reply a refreshed Home list can turn Open off before the cached answer is fetched (inferred); two quick leaves run two checkpoints once D1 fills them in | review report in the session transcript
- 2026-10-24 18:45 EDT | checks lane (Claude Code 2198ee19) | verify | none | Browser and parity (third lens, one subagent): built tc4 and both UI bundles; journeys shell, translate, section, understand, story, internet-consent, j11-sign-in, account-menu, source-texts, upgrades, guided-fix, sync-button, share-open: 102 pass, 0 fail, 0 skipped; scratch Playwright journeys dropped the replies of Sync and Open from Door43 (the same request went again, an edit or an answer took a new one), held Switch project behind a failing save and waited for a slow one. Words: only "bad request:" prefixes go and refusal placeholders are filled; nothing new to declare against tC4. No blocker, no major. Found outside the diff: after a held leave in Translate by verse, the typed verse is not on the page until Retry (reported to the orchestrator for its own card) | scratchpad j/lost.test.mjs, j/leave.test.mjs
- 2026-10-24 18:45 EDT | checks lane (Claude Code 2198ee19) | fix | none | Sync starts a new attempt after a lost reply instead of resending the same request, so a much later press can never replay an old sync and show it as done (the races lens's first minor; a second sync of the same work is harmless). The other minors go to minors.tsv | cargo clippy -p tc-ui --target wasm32-unknown-unknown --features app
- 2026-10-24 20:08 EDT | integration lane (Claude Code 3d07d5f8) | merge | 51e5edbc | merged into main in batch 3 (UI-3a 51e5edbc, REF1 1ad7e1f8, G4 578dd3f0, PROCESS-6 f0e0c399, shell.yml's cancel rule), gated by scripts/gates.sh --wait-ci batch-3 on the batch's last commit. In the batch ci.yml and app-e2e.yml keep main's jobs with UI-3a's cancel rule, and shell.yml takes it too. | scripts/gates.sh --wait-ci batch-3
