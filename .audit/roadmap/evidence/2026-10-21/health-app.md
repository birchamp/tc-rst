# health-app

## Summary
The app stack in resume-build-lane is well disciplined at the bottom and repetitive in the UI glue. What is good: there is one endpoint table (87 rows); every server write goes through one entry point, `ProjectHandle::submit`; there is one save machine for all 8 text-field kinds; the align save and the draft offers are pure `next()` machines. Library code has no unproven unwrap, expect or panic, because the lints are turned on and CI fails on warnings. Test hooks sit behind cargo features, and CI checks the release build without them.

The size is far over the plan's estimate. PLAN.md:312-314 expected 50,000 to 70,000 lines for the whole rebuild. Product code is about 143,000 non-blank, non-comment lines across 20 crates, and the app stack alone is about 73,000. tc-ui alone is 41,761 code lines, about 2.15 times tC4's views, design system and state put together (19,400 lines).

The extra size is mostly the same pattern written again in each screen:
- 4 copies of the loop that waits for a job to finish
- 6 places that handle "needs internet" or "needs sign-in"
- about 8 places that turn a refusal into words
- 13 hand-made `busy` flags, while the shared `Confirm` type built for double clicks is used only by a test
- a `Dialog` enum plus 7 separate form slots that must be kept in step by hand
- 9 hand-made counters, one per kind of read, that drop stale replies

The units still to build (C7a/b, D1, D2, E1a/b, E2, F1) would copy these patterns again. Each should be built once first: a typed job follower with typed outcomes; a SyncPort, ExchangePort and ImportPort seam with fakes; a typed conflict `Choice`; one `leave()` path for gate then checkpoint; and one way to run a request with the internet, sign-in and double-click rules. Each should come with a source-scan lint like tests/op_counts/lint.rs.

# Code health of the app stack (resume-build-lane snapshot)

Snapshot: `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/snap/resume-build-lane` (shown as `RBL/` below). I ran no cargo commands. The measurement scripts are in `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/codehealth/` (`fnsize.py`, `sloc.py`, `bools.py`, `panics.py`, `cssl.py`). They measure by matching braces and are approximate.

## 1. Size against the plan

| Measure | Value | Source |
|---|---|---|
| PLAN estimate | 50,000–70,000 lines, about 15 crates, "an estimate, not a measurement" | RBL/docs/PLAN.md:312-314 |
| Crates now | 20 | `ls RBL/crates` |
| `.rs` lines, all crates | 341,565 (200,234 under `*/src`) | `wc -l` |
| Product code, all crates (no blanks, comments, in-file test modules or test-only files) | about 142,971 | sloc.py |
| App stack product code | tc-api 8,371 · tc-app 20,842 · tc-server 1,615 · tc-ui 41,761 · tc4 498 = **73,087** | sloc.py |
| Comment lines in the app stack | tc-api 2,224 · tc-app 3,071 · tc-ui 4,406 | sloc.py |
| Feature-gated test or fixture code in src | tc-app 7,302 (hooks, memory backend, scenario, seed) · tc-ui 2,451 (gallery, probe) | sloc.py |
| tC4 for comparison | views 8,000 + design system 4,400 + state 7,000 = 19,400 | PLAN.md:306-308 |

Raw lines inside tc-ui: views 19,528 · ds 6,801 · app pure 9,590 · app/save 1,955 · app/web glue 10,222 · gallery 2,090.

My inference: tc-api (the typed contract) and the journal and store crates are the price of the owner's robustness goal. The extra size that is accidental sits in tc-ui's `app/web` glue and in per-screen state, which together are about 3 times tC4's 7,000 lines of state. The repeated patterns in §3 make up most of it. The app stack alone already passes the estimate for the whole rebuild, and 10 app units plus 6 storage units remain.

## 2. Largest files and functions

Largest files: tc-app/src/backend/store.rs 2,190 (1,531 product, the rest in-file tests) · tc-ui/src/app/save/mod.rs 1,642 · tc-ui/src/views/check/detail.rs 1,351 · tc-app/src/check.rs 1,351 · tc-ui/src/app/web/app_ctx.rs 1,320 (82 fns).

78 functions are 100 lines or longer, and 15 are 200 or longer (out of 3,869):

| Lines | Function |
|---|---|
| 565 | tc-ui/src/ds/forms/select.rs:245 `Dropdown` (ends at :809) |
| 525 | tc-ui/src/ds/primitives/layer.rs:187 `Layer` |
| 424 | tc-ui/src/views/check/detail.rs:522 `item_detail` |
| 390 | tc-ui/src/views/check/rail.rs:308 `check_rail` |
| 307 | tc-ui/src/views/section_editor.rs:86 `SectionEditor` |
| 259 | tc-api/src/model/draft.rs:542 `next` (exhaustive match, by design) |
| 240 | tc-app/src/app.rs:248 `start` |
| 211 | tc-app/src/check.rs:819 `reassess` |

Most of the long functions are Leptos components ported one to one from tC4 components. `clippy::too_many_lines` is not turned on: the workspace lints in RBL/Cargo.toml list only unwrap_used, expect_used, panic, dbg_macro and todo.

## 3. Logic written more than once

| Pattern | Copies | Evidence |
|---|---|---|
| Job-follow loop (read `server.jobs` every 50 ms, `jobs/get` every 1 s) | 4 | app/web/app_ctx.rs:930 `job_end`; app/web/sources.rs:300 `follow_download`; app/web/upgrades.rs:380 `follow_job`; app/web/understand.rs:353 `follow_open` |
| ...each treats Cancelled, a lost job and a wrong outcome differently | — | app_ctx.rs:944,954,1000 map a cancelled job, a lost job and a wrong outcome to `Unavailable(Feature::Sync)`, also for OpenDoor43; understand.rs:369 returns `None`; sources.rs returns `Err(None)` |
| Job outcomes have no types | — | job rows answer `JobId` (RBL/crates/tc-api/src/lib.rs endpoint_table, 7 `job` rows); `JobOutcome` is one flat enum (tc-api/src/jobs.rs:149); callers match with a wildcard, for example app_ctx.rs:902 `Ok(_) => this.open_refused(None, String::new())` |
| NeedsInternet / NeedsSignIn retry | 6 | app_ctx.rs:531, 903-907, 995-998, 1050-1056, 1256, 1277-1282 |
| Transport error relabelled as `BadRequest` | 3 | app_ctx.rs:884-888, 986-990; understand.rs:317-318 |
| Refusal → words (`vars` → `t_dyn(key)`) | 6, plus 2 variants | web/create.rs:31-36, web/sources.rs:31-36, web/understand.rs:135-144, views/modals/sign_in.rs:41-46, views/home.rs:925, app/sync.rs:193; app_ctx.rs:1317 passes `&[]` and drops the placeholders; app_ctx.rs:908-913 joins the var values with spaces |
| Double-submit or busy lifecycle written by hand | 13 `busy`-type bools, 23 `error: Option<String>` | app/forms.rs:74/79, 279/282, 540; app/sources.rs:38/45, 180; app/open_door43.rs:36; app/review.rs:32/37; app/upgrades.rs:44, 209; app/language.rs:15; app/home.rs:170; views/modals/gateway_change.rs:112 |
| ...while the shared type exists and goes unused | — | `Confirm` in tc-ui/src/app/confirm.rs (commit ebe803c) is used only by tests/app/recovery.rs |
| Dialog state kept in two places | `Dialog` enum + 7 form slots | app_ctx.rs:48-72 ("its form is `np`"), fields at :104-133; `close_modal` at :786-797 resets np, ab, im, sh, od, c.fix and st by hand |
| Drop-stale-reply counters | 9 thread-locals | web/check.rs:141-149 (ITEMS, ARTICLES, OPENS), web/align.rs (OPENS, READS, REFRESHES, NAVIGATIONS), web/understand.rs (OPENS, ARTICLES); the query cache has its own `Ticket` (app/cache.rs:91) |
| Navigation gate called directly | 10 product sites | `saves().gate().await` at upgrades.rs:135,268; understand.rs:303,497,516,551; web/mod.rs:643; fix.rs:185; app_ctx.rs:283,305 |

What is already shared and should stay that way: book_rail and story_rail use views/rail_chrome.rs; all 8 text-field kinds go through `Saves` (web/translate.rs:426-432, web/check.rs:303); server errors are mapped once in tc-app/src/wire/error.rs:17; Acks are built once in tc-app/src/wire/write.rs.

## 4. Save and write paths in the UI

1. `Saves`/`SaveMachine` (app/save/mod.rs, web/saves.rs) for Verse, Layout, Section, Comment, CheckComment, Frame, StoryTitle and StoryRef. Property-tested in tests/app/save_props.rs.
2. `DecisionWrites` (app/check_item.rs:328): a lock that lets one check write run at a time, plus a token map, for `check/decide`. Its failures go through `MarkFailure::of` with an `other =>` wildcard (:302-310).
3. Align `EditQueue` with a `Flight` enum (app/align.rs:329). It joins the gate through `add_gate_hook` (web/align.rs:766). Failures go into `EditFailure` with wildcards (web/align.rs:599-632).
4. The conflict keep (`ReviewState.busy/sent/outcome`, web/review.rs:119-172).
5. One-off dialog commands (settings, create, applyPlan, fix*, OpenDoor43, Sync), each with its own lifecycle as listed in §3.

So there are 4 write mechanisms plus the per-form ones. Only path 1 classifies errors with an exhaustive match (app/save/classify.rs:1-4: "no wildcard"). Two models describe a field's lifecycle: `FieldSave` (save/mod.rs:93) and `model::draft::Live` (tc-api/src/model/draft.rs:229). They are kept in step by tests/app/draft_machine.rs.

## 5. State held in bools and Options (RT-30)

25 structs have 2 or more bools (bools.py). The ones that matter:
- `Field` in tc-ui/src/app/save/mod.rs:231: 4 bools and 12 Options next to `state: FieldSave`. `retry_at` is read only while the state is `Saving` (:1349, :1376), and there are 17 direct `.state =` writes.
- tc-app/src/align/actor.rs:104 `Actor`: `mark_refused, asked, suggesting, done` plus 3 Options.
- tc-app/src/align/view.rs:87 `Moment`: 4 bools.
- `ReviewState` in app/review.rs:22: `busy` + `sent` + `outcome`, the shape C7a will extend.
- `SettingsForm` in app/sources.rs:26: `loaded` + `shown: Option`, and `busy` + `request` + `error`.

## 6. The same rule in more than one place

- **Check rail grouping**: tc-api/src/model/rail.rs (UI, 215 lines) and tc-check/src/rail.rs (server, 312 lines). They are held equal by tc-app/tests/support/rail_differential.rs.
- **Read-only**: the server refuses (store `CommandError::ReadOnly`; tc-app/src/align/actor.rs:333). In the UI, `WriteCap` is documented as "editors take `Option<WriteCap>`" (web/saves.rs:114), but no function takes one. It is only tested with `.is_some()` (translate.rs:22,39,60; comment_box.rs:44). The align and check views never consult it.
- **Drafted**: tc-usfm/src/codec.rs:239 `is_drafted` follows tC4 (a verse whose words are on `\q` lines counts as undrafted). It is used by tc-store/src/commands.rs:767, tc-journal/src/fold/project.rs:288 and tc-app/src/check/item.rs:130. tc-app/src/drafted.rs:76-77 counts such a verse as drafted (the declared difference in PLAN.md:146-148). Inference, not run: in Psalms, Check gives such a verse no target text while Home and the rail call it drafted.
- **NFC**: the UI save machine and the server compare normalised text with one pinned crate version (tc-ui/Cargo.toml comment at :37).

## 7. Test hooks, unwrap and panics

- Gating is sound: features `test-hooks` and `fixture-backend`, and CI checks release without them (ci.yml:24,27). Leaks:
  - tc-server/src/lib.rs:26 `pub mod stub;` is not gated (327 lines).
  - `FieldKind::Test` (tc-api/src/buffer.rs:73) is not gated and has release branches at tc-app/src/drafts.rs:57, tc-ui/src/app/web/saves.rs:534 and app/save/rules.rs:55.
  - There are 33 test-hook branches inside function bodies in tc-app (app.rs 14, dispatch.rs 8, upgrades.rs 3, sync.rs:143-146, …). For example, `sync_job`'s release path is `Unavailable(Sync)` and only the test path runs the scripted sync.
- unwrap, expect and panic are linted (Cargo.toml `[workspace.lints.clippy]`, `-D warnings` at ci.yml:21) and allowed only in tests (clippy.toml). Non-test sites: tc-api 7 and tc-app 4, all `unreachable!("<invariant>")` or `#[expect(..., reason)]` (tc-app/src/wire/scope.rs:41-46); tc-ui 2, both in the gated probe.rs. **No unproven sites.** Gap: `clippy::unreachable` is not turned on.
- `SyncPort`, `ExchangePort` and `ImportPort` are designed (APP-DESIGN.md:1002, 1527) but no Rust code defines them.

## 8. What the remaining units should reuse or build first (ranked)

| # | Build once, first | For units | Code | Check (deterministic) | Est. size |
|---|---|---|---|---|---|
| 1 | **Typed jobs**: an `Outcome` type on each job row, and `tc_ui::app::jobs::follow` returning `JobEnd<O> = Done(O) \| Failed(ApiError) \| Cancelled \| Lost`. Delete the 4 loops. | E1a, E1b, E2, F1, D2 | follow is pure, plus one web loop | host test of the follow states; source scan: `js::sleep(50)` and `.jobs.with_untracked(` appear only in the follower | +150 / −200 |
| 2 | **SyncPort / ExchangePort / ImportPort** traits in `AppDeps`. Scripted fakes go under `fixture-backend`, replacing the in-body `cfg(test-hooks)` in sync.rs, share.rs and upgrades.rs. | E1a, E1b, E2, F1 | release and tests run the same function | the contract cases run on the fake and on the real port, as with `store_contract!` (tc-app/tests/store_contract.rs:401-402) | medium |
| 3 | **Typed conflict `Choice`**: `Resolve { keep: ChangeRef }` (tc-api/src/conflicts.rs:377-383) becomes `choice: Choice`; `ReviewState` busy/sent/outcome becomes one enum using `Confirm`. | C7a, C7b | compiler forces tc-app/src/conflicts.rs:581 and the UI to handle each kind | double submit, look-again and one resolve per kind tested through the public API | small, do before C7a |
| 4 | **One `leave(dest)`** path: gate, then checkpoint, then switch. Replaces the 10 direct `gate()` calls. | D1 (and E1a, which waits for D1) | one entry point | source scan: `saves().gate()` only in `leave` | small |
| 5 | **`run_net(pending, call)`**: one place handles NeedsInternet and NeedsSignIn; transport errors stay `Transport`, not `BadRequest`. | E1a, E1b | | exhaustive match on `ApiError` | small |
| 6 | **`Submit` lifecycle on `Confirm`** for the dialogs the remaining units touch: ImportForm, OpenForm, share form, export and USB dialogs | E1b, E2, F1, D2 | a fixed request id per attempt (today OpenDoor43 and Sync draw a new id on each press, app_ctx.rs:877, 981) | `clippy::struct_excessive_bools` with a low cap in clippy.toml | small |
| 7 | **Dialogs own their state**: `Dialog::Import(ImportForm)` and so on, with no parallel `Option<Form>` slots | E1b, E2, F1, D2 | two-dialogs-at-once cannot be represented | none needed: the type does it | small |
| 8 | **`i18n::refusal_text(&dyn Message)`** at all 8 sites | C7, E1, E2, F1, D2 | | a test that every `Refusal` variant renders with no leftover `{` | small |
| 9 | **Enforce `WriteCap`**: align and check edit actions take `WriteCap` | D1 (outside edits), E2 | | compile-time | small |
| 10 | **Lint ratchets**: turn on `clippy::unreachable`, `struct_excessive_bools`, `too_many_lines` (with an allowlist for today's code); more source scans modelled on tc-app/tests/op_counts/lint.rs | all | | CI | small |
| 11 | **A `Latest` generation helper** instead of the 9 thread-local counters | C7a reads, D1 | | | small |
| 12 | **Delete the fixture seed at F1** (APP-DESIGN.md:3419): tc-app/src/backend/seed.rs 1,305 + seed/check.rs 1,034 + records.rs 551 + strict.rs 345 + seed/tests.rs 996 | F1 | −4,231 lines | `memory_seed_equals_tc_import_seed` | — |
| 13 | **Leaks**: gate `tc-server::stub` and `FieldKind::Test` | — | | release clippy | tiny |

Hold, do not refactor now: the internals of `SaveMachine::Field` (A4b no-loss code, property-tested). Rule for the future: no new fields on `Field`; new states go in `FieldSave` variants.

Merge hot spots for a parallel developer (commit counts on resume-build-lane): tc-app/src/lib.rs 38, tc-api/src/lib.rs (endpoint table) 29, tc-app/src/backend/mod.rs 27, backend/store.rs 26, tc-ui/src/app/web/mod.rs 24, app/mod.rs 24, tc-app/src/app.rs 24. Edits to the endpoint table and to the `AppDeps` ports should land in one integration step before the work splits.

Owner decision needed: the two rules for "drafted" (§6) in Check on Psalms.

## Findings
- [CH-1] (medium/medium) The app stack alone is about 73,000 product code lines, more than PLAN's estimate for the whole rebuild (50,000–70,000). All crates together are about 143,000. tc-ui is about 2.15 times tC4's views, design system and state combined.
  - evidence: RBL/docs/PLAN.md:304-314 (tC4 43,000; estimate 50-70k, 'an estimate, not a measurement'); sloc.py: tc-api 8,371, tc-app 20,842, tc-server 1,615, tc-ui 41,761, tc4 498 code lines; all crates 142,971; tc-ui raw lines: views 19,528, ds 6,801, app pure 9,590, app/save 1,955, app/web 10,222
  - rec: Re-baseline the estimate in PLAN.md. Put the cuts where the repetition is: tc-ui app/web glue and per-screen state (CH-2..CH-6). Don't cut the typed contract in tc-api.
- [CH-2] (high/high) The UI has 4 copies of the job-follow loop, and each treats cancelled, lost and wrong-outcome jobs differently. app_ctx shows a cancelled or lost OpenDoor43 or Sync job as Unavailable(Feature::Sync), which means 'not built yet'. Job outcomes have no type: the job rows answer only JobId.
  - evidence: tc-ui/src/app/web/app_ctx.rs:930-962 job_end; :944, :954, :1000 Unavailable(Feature::Sync); tc-ui/src/app/web/sources.rs:300-345 follow_download; tc-ui/src/app/web/upgrades.rs:380-420 follow_job; tc-ui/src/app/web/understand.rs:353-395 follow_open; tc-api/src/jobs.rs:149 flat JobOutcome; tc-api/src/lib.rs endpoint_table: 6 job rows => JobId
  - rec: Before E1a, E1b, E2, F1 and D2: give each job row an Outcome type and build one follow() that returns JobEnd<O> {Done, Failed, Cancelled, Lost}. Delete the 4 loops. Add a source-scan test that only the follower polls jobs.
- [CH-3] (high/high) The SyncPort, ExchangePort and ImportPort seams are designed but do not exist in code. Scripted sync and share outcomes run as cfg(test-hooks) branches inside business functions, so the release path (Unavailable) is never the tested path.
  - evidence: RBL/docs/APP-DESIGN.md:1002 'pub trait SyncPort', :1527 SyncPort/ExchangePort/ImportPort; grep 'SyncPort' over crates: no match; tc-app/src/sync.rs:141-148 sync_job with cfg(test-hooks) at :143; 33 in-body cfg(test-hooks|fixture-backend) lines in tc-app/src (app.rs 14, dispatch.rs 8, upgrades.rs 3)
  - rec: Add the three port traits to AppDeps now, with scripted fakes under fixture-backend. Run shared contract cases on fake and real, as store_contract! does (tc-app/tests/store_contract.rs:401-402). E1a, E1b, E2 and F1 then only add the real implementations.
- [CH-4] (high/high) conflicts/resolve can only express 'keep this ChangeRef', but C7a and C7b need Restore, Dismiss, Detached and per-kind choices. The UI's keep lifecycle is a hand-written busy/sent/outcome trio.
  - evidence: tc-api/src/conflicts.rs:377-383 Resolve { keep: ChangeRef }; tc-ui/src/app/review.rs:22-41 ReviewState busy/sent/outcome; tc-ui/src/app/web/review.rs:119-172 keep_version; RBL/docs/APP-DESIGN.md:3396-3397 C7a/C7b scope
  - rec: Before C7a: replace keep with choice: Choice (an enum), handled exhaustively in tc-app/src/conflicts.rs:581 and the UI. Make ReviewState's keep one enum built on Confirm.
- [CH-5] (high/high) The navigation gate is called directly from 10 product sites. D1 must add a checkpoint at every leave, so without one leave() path it would have to edit each site.
  - evidence: saves().gate().await at app/web/upgrades.rs:135,268; understand.rs:303,497,516,551; web/mod.rs:643; fix.rs:185; app_ctx.rs:283,305; RBL/docs/APP-DESIGN.md:3404 D1 scope (leave triggers, checkpoint at Home, quit flow)
  - rec: Before D1: one leave(dest) that runs gate, then checkpoint, then switch, and a source-scan lint that only leave() calls gate().
- [CH-6] (medium/high) A shared double-submit type (Confirm) exists but only a test uses it. Dialogs hand-write the busy, request and error state: 13 busy-type bools and 23 error: Option<String> fields. OpenDoor43 and Sync draw a fresh request id on every press.
  - evidence: tc-ui/src/app/confirm.rs (commit ebe803c); only user tests/app/recovery.rs:7; app/forms.rs:74/79, 279/282, 540; app/sources.rs:38/45, 180; app/open_door43.rs:36; app/review.rs:32/37; app/upgrades.rs:44, 209; app/language.rs:15; app/home.rs:170; app_ctx.rs:877 and :981 js::random_request_id() per press
  - rec: Build Submit on Confirm. Move ImportForm, OpenForm, the share form and the new export and USB dialogs onto it as E1b, E2, F1 and D2 touch them. Turn on clippy struct_excessive_bools with a low cap.
- [CH-7] (medium/high) Which dialog is open is stored twice: in the Dialog enum and in 7 parallel Option<Form> signals, kept in step by hand in close_modal.
  - evidence: tc-ui/src/app/web/app_ctx.rs:48-72 Dialog ('its form is `np`', etc.); app_ctx.rs:104-133 AppState fields np, ab, im, sh, od, st, src; app_ctx.rs:786-797 close_modal resets 7 slots by hand
  - rec: Make each Dialog variant carry its form (Dialog::Import(ImportForm), …), starting with the dialogs E1b, E2, F1 and D2 add.
- [CH-8] (medium/high) Internet, sign-in and refusal handling is copy-pasted. NeedsInternet/NeedsSignIn is handled at 6 sites; refusal-to-words at 6 sites plus 2 variants that drop or mangle placeholders. Transport errors are relabelled as BadRequest at 3 sites.
  - evidence: app_ctx.rs:531, 903-907, 995-998, 1050-1056, 1256, 1277-1282; web/create.rs:31-36, web/sources.rs:31-36, web/understand.rs:135-144, views/modals/sign_in.rs:41-46, views/home.rs:925, app/sync.rs:193; app_ctx.rs:1317 t_dyn(refusal.key(), &[], None); ShareRefusal::PushFailed has a {reason} var (tc-api/src/refusal.rs); app_ctx.rs:884-888, 986-990; understand.rs:317-318
  - rec: Add run_net(pending, call) and i18n::refusal_text(&dyn Message). Test that every Refusal variant renders with no '{' left over. Do this before E1a and E1b add Door43 flows.
- [CH-9] (medium/high) The UI has 4 separate write mechanisms. Only the text-field save machine classifies errors exhaustively; the check and align write paths use wildcard matches.
  - evidence: tc-ui/src/app/save/classify.rs:1-4 'no wildcard'; tc-ui/src/app/check_item.rs:302-310 MarkFailure::of 'other =>'; tc-ui/src/app/web/align.rs:599-632 EditFailure from error.to_string(); DecisionWrites check_item.rs:328; EditQueue app/align.rs:329; ReviewState web/review.rs:119
  - rec: Don't merge the machines now. Do require an exhaustive match (no wildcard) wherever a write's error is classified, so a new ApiError variant does not compile until every write path decides what to do with it.
- [CH-10] (medium/high) WriteCap was meant as a type-level proof that a project is writable, but no function takes it. Only translate and the comment box check it, and the align and check views rely on server refusals.
  - evidence: tc-ui/src/app/web/saves.rs:114-117 doc 'editors take Option<WriteCap>'; uses only .is_some()/.is_none(): web/translate.rs:22,39,60; views/comment_box.rs:44; server refusal tc-app/src/align/actor.rs:333-335
  - rec: Make align and check edit actions require a WriteCap argument so their controls go off in read-only projects. Needed before D1 and E2 make read-only more common. Check behaviour in the app once.
- [CH-11] (medium/medium) Two rules decide whether a verse is drafted. Home tiles and the rail count words on any line; Check, the store and the fold use tC4's rule, under which a verse whose words are on \q lines is undrafted. Inference: on Psalms, Check gives such verses no target text.
  - evidence: tc-usfm/src/codec.rs:235-242 is_drafted (tC4 rule, doc example \v 1 then \q1); tc-app/src/check/item.rs:130; tc-store/src/commands.rs:767; tc-journal/src/fold/project.rs:288; tc-app/src/drafted.rs:76-77; PLAN.md:146-148 declared difference for tiles only
  - rec: Owner decision: should Check follow the rebuild rule? Verify first with a Psalms fixture in the app. Then rename the functions so the two rules cannot be mixed up.
- [CH-12] (low/high) State is kept in bags of booleans in key structs. The save machine's Field has 4 bools and 12 Options next to its state enum (retry_at is read only while Saving). The align Actor has 4 bools.
  - evidence: tc-ui/src/app/save/mod.rs:231-275 Field; :1349, :1376 retry_at read only in Saving; 17 '.state =' writes; tc-app/src/align/actor.rs:104-127 Actor; tc-app/src/align/view.rs:87 Moment; bools.py: 25 structs with >=2 bools
  - rec: Freeze Field; don't rewrite proven A4b code now. Rule for the future: new states go in FieldSave variants. Turn on clippy::struct_excessive_bools with an allowlist for today's structs.
- [CH-13] (low/high) Test code leaking into release builds is small: tc-server's 327-line stub module and FieldKind::Test are compiled into release. Library code has no unproven unwrap, expect or panic, because the lints are on and CI fails on warnings.
  - evidence: tc-server/src/lib.rs:26 'pub mod stub;' ungated; tc-api/src/buffer.rs:73 FieldKind::Test; branches tc-app/src/drafts.rs:57, tc-ui/src/app/web/saves.rs:534, app/save/rules.rs:55; Cargo.toml [workspace.lints.clippy] unwrap_used/expect_used/panic warn; ci.yml:21 -D warnings; clippy.toml allow-*-in-tests; panics.py: tc-api 7, tc-app 4, all unreachable!(invariant) or #[expect(reason)] e.g. tc-app/src/wire/scope.rs:41-46
  - rec: Put stub and FieldKind::Test behind test-hooks. Turn on clippy::unreachable.
- [CH-14] (low/high) 9 hand-written thread-local counters drop stale replies across check, align and understand, alongside the query cache's own Ticket mechanism.
  - evidence: tc-ui/src/app/web/check.rs:141-151 ITEMS, ARTICLES, OPENS; tc-ui/src/app/web/align.rs thread_local OPENS, READS, REFRESHES, NAVIGATIONS; tc-ui/src/app/web/understand.rs OPENS, ARTICLES; tc-ui/src/app/cache.rs:91 Ticket
  - rec: Add one Latest/generation helper, or send these reads through the query cache. Use it in C7a's review reads and in D1.
- [CH-15] (medium/high) F1 is set to delete about 4,231 lines of fixture seed, a second model of tC4's project format. Commits to the shared registry files are where parallel work will collide.
  - evidence: RBL/docs/APP-DESIGN.md:3419 F1 'the local seed is deleted'; tc-app/src/backend/seed.rs 1,305; seed/check.rs 1,034; seed/records.rs 551; seed/strict.rs 345; seed/tests.rs 996; git log resume-build-lane: tc-app/src/lib.rs 38 commits, tc-api/src/lib.rs 29, backend/mod.rs 27, backend/store.rs 26, tc-ui app/web/mod.rs 24
  - rec: Keep the seed deletion in F1's done-when. Before any split between developers, land the edits to the endpoint table and the AppDeps ports in one integration step.

## Unverified
- No cargo build, test or clippy runs (forbidden here). The lint-clean claims rest on the CI config, not on a run.
- Function and line counts come from brace-matching and line-classifying scripts (fnsize.py, sloc.py in /tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/work/codehealth/). They are approximate; tokei or cargo-count on the branch would give exact numbers.
- Whether tC4's 8,000 lines of 'views' (PLAN.md:306) include its CSS is unknown, so the tc-ui ratio of about 2.15 is rough. Count tC4's src/views and CSS at d310bf3 with the same rules.
- CH-11 (Check showing no target text for Psalms verses on \q lines) is read from the code, not run. To verify: open Psalms in Check with a verse whose words are on a \q1 line and look at the target words.
- CH-10 (align and check controls stay on in read-only projects) is not run in the app. To verify: open a project read-only (lock on another computer) and try an align drag or a check mark.
- Whether account/orgs can answer ShareRefusal::PushFailed, which would show an unfilled {reason}, is not checked. Read tc-app/src/share.rs share_failure.
- Whether a lost reply followed by a second press starts a second OpenDoor43 or Sync job (fresh request id per press) is not tested.
- This covers resume-build-lane only. I did not compare with the s2-proof-executor or storage snapshots, or with main.