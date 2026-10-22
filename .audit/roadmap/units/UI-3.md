---
id: UI-3
title: One leave path, one network-call helper, one refusal text, Submit, dialogs that own their forms, WriteCap enforced
track: A
lane: app
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [UI-1]
needs_owner: []
proves: []
no_loss: true
size: 2 d
merged: none
updated: 2026-10-21
next: Claim this card after UI-1 merges, then do step 1.
blocked_by: none
---

# UI-3: One leave path, one network-call helper, one refusal text, Submit, dialogs that own their forms, WriteCap enforced

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
    without a `WriteCap`, so their controls are off in a read-only project.
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

### Step 5: Make each dialog own its form

- Do:
  1. In `crates/tc-ui/src/app/web/app_ctx.rs:47-72`, rename today's `Dialog` to
     `DialogKind` (it stays `Copy`) and make a new `Dialog` whose variants carry their state:
     `NewProject(NewProjectForm)`, `NewObs(NewProjectForm)`, `AddBook(AddBookForm)`,
     `Import(ImportForm)`, `OpenDoor43(OpenForm)`, `Share(ShareForm)`,
     `GuidedFix(<the type of c.fix>)`, `Settings(SettingsForm)`; `Feedback`, `SignIn`,
     `AddProject`, `Sources` carry nothing new. Add `Dialog::kind(&self) -> DialogKind`.
  2. The modal host switches on a `Memo<Option<DialogKind>>`, never on the whole `Dialog`,
     so typing in a form does not rebuild the dialog (a rebuilt input loses focus). Forms are
     read and patched through `layers.dialog` with small helpers like today's `patch_od`.
  3. Delete the slots `np`, `ab`, `im`, `sh`, `od`, `st` from `AppState`
     (app_ctx.rs:117-127) and the dialog part of `c.fix`; `close_modal` (:786) sets the
     dialog to `None` and nothing else. Do this order: `sh`, `od`, `im` (the ones E1b and F1
     touch) first, each in its own commit, then `np`, `ab`, `st`, `c.fix`.
- Files: `crates/tc-ui/src/app/web/` (app_ctx.rs, create.rs, check.rs, fix.rs, sources.rs)
  and the views that read the slots:
  `grep -rn "\.np\b\|\.ab\b\|\.im\b\|\.sh\b\|\.od\b\|\.st\b" crates/tc-ui/src`.
- Done when: that grep prints no slot read; `close_modal` is three lines or fewer; all four
  `tc-ui` wasm32 clippy runs pass; journeys `create`, `share-open`, `home`, `guided-fix`,
  `source-texts` pass.
- Record: a log line `step 5` per commit; set `next`.

### Step 6: Require a WriteCap for align and check edits

- Do:
  1. Change `align_edit(&self, edit)` (web/align.rs:516), `keep_align_reflow`
     (web/align.rs:641) and `mark(&self, edit)` (web/check.rs:739) to take `cap: WriteCap`
     as their first argument.
  2. Add a tracked read of the cap for views (today `Saves::write_cap`, saves.rs:431, reads
     the sessions untracked): a switch to read-only must turn the controls off at once
     (APP-DESIGN §6.4). The views that call these actions
     (`crates/tc-ui/src/views/align/session.rs`, `crates/tc-ui/src/views/check/detail.rs`;
     find every caller with `grep -rn "align_edit(\|\.mark(\|keep_align_reflow(" crates/tc-ui/src/views`)
     get `Option<WriteCap>` and draw the control off (`disabled`, and no drag start) when it
     is `None`.
  3. Fix the doc of `WriteCap` (saves.rs:114) to say which actions take it.
  4. Create `parity/app/journeys/read-only.test.mjs`. Make the project read-only a way a
     translator meets: projects inside a cloud-synced folder (`cloud_client`,
     `crates/tc-store/src/platform.rs:106`, e.g. `$HOME/Google Drive`) if the kit lets the
     test set `HOME`, else another computer's lock (`OpenOnAnotherComputer` in
     `crates/tc-store/src/open.rs`; `grep -rn "const LOCK" crates/tc-store/src`). Assert:
     align cards and bank take no drag or click, check mark buttons are disabled, and no
     `align/edit` or `check/decide` request leaves the page (`page.on('request')`).
- Files: `crates/tc-ui/src/app/web/align.rs`, `check.rs`, `saves.rs`,
  `crates/tc-ui/src/views/align/session.rs`, `crates/tc-ui/src/views/check/detail.rs`,
  `parity/app/journeys/read-only.test.mjs`.
- Done when: the journey passes, and fails with proof mutation (4) (paste both in the log);
  journeys `align`, `check-writes`, `check-keyboard` still pass.
- Record: a log line `step 6`; set `next` to step 7.

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
