---
id: UI-3b
title: Dialogs that own their forms, WriteCap enforced
track: A
lane: app
milestone: M1
state: done
owner: REF1 session (Claude Code 56b3f56d)
branch: track-UI-3b
worktree: /home/bot/tc-rst/.worktrees/track-REF1
depends: [UI-3a]
needs_owner: []
proves: []
no_loss: true
size: 1 d (steps 5-6 of UI-3; step 5 built on track-UI-3b)
merged: 8fbc3c1f
updated: 2026-10-25
next: none
blocked_by: none
---

# UI-3b: Dialogs that own their forms, WriteCap enforced

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

Steps 5-6 of UI-3, which split on 2026-10-24; steps 1-4 are UI-3a.

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

- 2026-10-24 16:24 EDT | checks lane (Claude Code 2198ee19) | step 5 | none | First part, as the card orders: Dialog carries its form for Share, Open from Door43 and Import, and DialogKind (Copy) is what the modal host and every comparison switch on, so typing does not rebuild a dialog. The slots sh, od and im are gone; views read the form through AppState::share_form, open_door43_form and import_form, and write through the patch helpers. The sign-in step holds the dialog it covers (Dialog::SignIn { under }): Open from Door43's form comes back as the person left it when the open goes on after a sign-in, and Cancel closes both, as before. All five tc-ui clippy runs and every tc-ui test pass | cargo test -p tc-ui
- 2026-10-24 16:32 EDT | checks lane (Claude Code 2198ee19) | step 5 | d2206dd4 | Journeys on the first part (share, open, import): 218 passed, 0 failed, 0 skipped | node --test parity/app/journeys/*.test.mjs
- 2026-10-24 16:32 EDT | checks lane (Claude Code 2198ee19) | step 5 | none | Second part: New Bible and New Open Bible Stories, Add a book, Project settings and the guided fix carry their state in the dialog too; the slots np, ab, st and c.fix are gone. Code that read or patched a slot uses a FormSlot over the dialog (np_slot, ab_slot, st_slot, fix_slot), views a derived signal (new_project_form, add_book_form, settings_form, fix_form). close_modal is two lines (the dialog, and the share flow's end). grep for a slot read prints nothing. Leaving a tool still closes its guided fix (reset_check). All five tc-ui clippy runs and every tc-ui test pass | cargo test -p tc-ui
- 2026-10-24 17:24 EDT | checks lane (Claude Code 2198ee19) | claim | none | Split from UI-3 (orchestrator, 2026-10-24). Step 5 is built and committed on track-UI-3b at f0d8e801 (both parts; 218/218 journeys pass on it; not reviewed, not pushed). Resumes after D1 | none
- 2026-10-25 10:50 EDT | REF1 session (Claude Code 56b3f56d) | takeover | none | From the checks lane (Claude Code 2198ee19), on the orchestrator's word and as the checks lane's handover note says (UI-3b handed to this session 2026-10-25); its three step 5 log lines carried here from UI-3.md. track-UI-3b f0d8e801 with main 4956ac52 merged in (UI-3a is done); conflicts: one doc comment in app_ctx.rs (kept), UI-3.md (main's parked card). Step 5 already makes the sign-in dialog carry the dialog it covers (Dialog::SignIn { under }), so Open from Door43's form survives the sign-in step. In REF1's worktree, on branch track-UI-3b | git log --oneline -3 track-UI-3b
- 2026-10-25 12:40 EDT | REF1 session (Claude Code 56b3f56d) | step 6 | none | align_edit, keep_align_reflow and mark take a WriteCap first; Saves::watch_write_cap is the tracked read and Actions::write_cap gives views the open project's cap. The aligner (bank words, placed words, suggested chips, card headers, card click, key and drop, the suggestion buttons, Mark valid) and the check marks (valid, invalid, todo, bookmark) draw off with no drag start when it is None. New journey read-only.test.mjs: a project another computer holds open (its .tc/lock) opens read-only; every align and check control is off and no align/edit or check/decide request leaves the page: 2 of 2 pass. Proof mutation (4), watch_write_cap always Some: both fail (AssertionError, fail 2). Journeys align 15/15, check-writes 7/7, check-keyboard 4/4; wasm app clippy clean. The check comment keeps its 6.4 behavior (typed text stays in the buffer) | node --test parity/app/journeys/{read-only,align,check-writes,check-keyboard}.test.mjs
- 2026-10-25 14:05 EDT | REF1 session (Claude Code 56b3f56d) | lens loss | f436a61f | Independent reviewer, steps 5 and 6: no blocker, no major. Minors: the sign-in step kept only Open from Door43 under it (a New Bible typed while a sync asked for the sign-in was dropped), and Cancel on the step dropped the dialog it covered; nit: the aligner redrew on every read of the open sessions | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-loss.md
- 2026-10-25 14:05 EDT | REF1 session (Claude Code 56b3f56d) | lens races | f436a61f | Independent reviewer: no blocker, no major. Minors: the aligner's untracked-memo redraw (as above); a late New Bible or Add book reply acted on another dialog opened after Cancel; Open's success closed whatever dialog showed (a sync's sign-in step over it); a step over a step dropped the covered Open form, and open_door43_now's restore was redundant; WriteCap's doc should say it gates the click, not the send; no journey for a switch to read-only while open | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-races.md
- 2026-10-25 14:05 EDT | REF1 session (Claude Code 56b3f56d) | lens parity | f436a61f | Independent reviewer: no blocker. Majors: off controls still looked clickable (tC4 draws cursor default, opacity .55); a read-only open no longer said why (the click's refusal was the only sentence, and C6's banner is not drawn yet); the check comment editor stayed on. Minors: hints for actions the control cannot do; the journey's drop carried no payload, its header loop could assert nothing, no negative control | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-parity.md
- 2026-10-25 14:30 EDT | REF1 session (Claude Code 56b3f56d) | fix | none | All lens findings fixed but one. Any dialog stays under the sign-in step (a step over a step keeps the one under both) and shows again after Sign in and after Cancel (declared difference from tC4's closeModal, in APP-DESIGN 6.7); Open's success closes only its own dialog; the redundant restore is gone; New Bible's late reply lands only on the form that sent it (NewProjectForm::awaits, tested) and Add book's only on its project's form. The aligner reads the cap through a Memo. Off controls get tC4's off look, drop their action hints, and the aligner and the check say why in the banner's words (align-read-only, check-read-only) until C6 draws the banner; the check comment is read-only and Delete off. The journey gains a negative control, a real drop payload, placed words and suggestion buttons, the cursor, the sentence and the comment. Not fixed, decided: a journey that switches an open project to read-only, since nothing a test can reach flips the mode mid-session (the only live trigger is a second computer writing the folder, SecondWriter) and no test hook sets it; the views read the same tracked Memo as the verse comment box | cargo test -p tc-ui --features tc-api/test-hooks
- 2026-10-25 21:10 EDT | REF1 session (Claude Code 56b3f56d) | verify | 036494ac | Loss lens (an independent general-purpose reviewer, read only, over steps 5 and 6 at f436a61f; its findings fixed in 036494ac). 1 Code: each dialog holds its form in its Dialog variant and the sign-in step holds the dialog under it (Dialog::SignIn { under }), so no form outlives its dialog; align and check edits take a WriteCap. 2 Checks: tc-ui's forms tests (NewProjectForm::awaits) and the read-only journey, which prints INV-READ-ONLY-HAS-NO-WRITE-CONTROLS; all 239 journey tests and cargo test -p tc-ui pass. 3 Reach: a New Bible typed while a sync asks the sign-in, and Cancel on the step, both reachable; both kept the form only for Open before, now for every dialog. 4 Siblings: every dialog that can sit under the step searched (Open, New Bible, Add book, Settings, Share). 5 No loss: no blocker or major; the two form-loss minors fixed. 6 Prose: the Cancel difference from tC4's closeModal declared in APP-DESIGN 6.7 | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-loss.md
- 2026-10-25 21:10 EDT | REF1 session (Claude Code 56b3f56d) | verify | 036494ac | Races and protocol lens (an independent reviewer at f436a61f; fixed in 036494ac). 1 Code: New Bible's late reply lands only on the form that sent it (NewProjectForm::awaits), Add book's only on its project's form, Open's success closes only its own dialog. 2 Checks: the forms tests of the late reply, and the journeys above. 3 Reach: Cancel then a new dialog before the reply, reachable. 4 Siblings: the other replies that patch a dialog searched (orgs, settings read). 5 No loss: no blocker or major. 6 Prose: WriteCap's doc says it gates the click. Not done, decided: a journey that turns an open project read-only, which nothing a test can reach does (only a second computer writing the folder, SecondWriter) | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-races.md
- 2026-10-25 21:10 EDT | REF1 session (Claude Code 56b3f56d) | verify | 036494ac | Browser and parity lens (an independent reviewer in Chromium against tC4 b86270f; fixed in 036494ac). 1 Code: off controls take tC4's off look (cursor default, opacity .55) and drop their action hints; the check comment is read-only and Delete off. 2 Checks: read-only.test.mjs gains a negative control, a real drop payload, placed words, suggestion buttons, the cursor, the sentence and the comment, and passes 3 of 3. 3 Reach: a read-only open (another computer holds the lock). 4 Siblings: every align and check control listed in the journey. 5 No loss: the three majors (clickable look, no reason shown, comment editable) fixed. 6 Prose: the aligner and the check say why in the banner's words until C6 draws the banner | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/UI-3b-lens-parity.md
- 2026-10-25 21:10 EDT | REF1 session (Claude Code 56b3f56d) | note | none | The read-only journey's two nothing-sent assertions name INV-READ-ONLY-HAS-NO-WRITE-CONTROLS, which the claims check needs printed; the journey passes 3 of 3 | node --test parity/app/journeys/read-only.test.mjs
- 2026-10-25 22:49 EDT | integration lane (Claude Code 3d07d5f8) | merge | 8fbc3c1f | merged into main in batch 8 (UI-3b 8fbc3c1f, X's follow-up 654c5a0b, RULING-H11), gated by scripts/gates.sh --wait-ci batch-8 | scripts/gates.sh --wait-ci batch-8
