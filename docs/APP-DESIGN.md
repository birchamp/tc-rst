# The application layer of the tC4 rebuild

This is the design for `tc-api`, `tc-app`, `tc-server`, the server side of the `tc4` binary, and
the port of tC4's screens into `tc-ui`. It merges three competing designs (core-first,
parity-first, slice-first) after two judges scored them. Its claims were checked against the
code on 2026-10-17; §13 lists what was read. A red team attacked it the same day (the
"Red team" section at the end). This revision answers every finding: a change made in answer to
finding RT-n is marked *(A-n)*, and "How each point was answered", after the red team, says
what changed or why the finding was rejected.

---

## 0. How to read this document

- **Paths.** Plain paths are in `/home/user/tc-rst`, branch `claude/focused-ride-5uzj7p` (HEAD
  `21fef4a`). `storage:path` is the same path on branch `storage` (HEAD `2dcfe6f`, worktree
  `.claude/worktrees/storage`). `tC4:path:n` is in `/home/user/unfoldingword/translationcore4`
  at `d310bf3`.
- **STORAGE:n** is line n of `storage:docs/STORAGE.md` (4,225 lines). The file is byte-identical
  at `225091b` and `2dcfe6f`. Unit C is editing it; up to line 3,445 it is identical to the copy
  at `6bb20d4`. The copy of STORAGE.md on main is older. Use the storage branch copy.
- **ARCH, PERF, PLAN, DELTA** are `docs/ARCHITECTURE.md`, `docs/PERFORMANCE.md`, `docs/PLAN.md`
  and `docs/TC4-DELTA-2026-10-16.md`.
- **Screen ids.** S3–S16 are screens, M1–M18 dialogs, X1–X14 shared pieces, as in the screen map
  that the design run collected from tC4. In this document S1 and S2 always mean storage units;
  the Home and Understand screens are called by name. The 62 baseline screens are listed in
  `parity/tc4-baseline/README.md:271-334`.
- **Storage units.** C, S1, S2, P, X and I are the units of `.audit/workflows/storage-chain-B.js`:
  C converges main with tc-journal; S1 builds tc-store's files, actor, commands and open; S2
  materialize, outside edits and git; P scale; X tc-sync; I tc-import. Unit C is in progress:
  the merge of main has landed on `storage` (`a6f1ffd`); the spec decisions, one owner per
  shared type and the differential test are still to come.
- **This design's ids.** AD-n are decisions (§2). R-n are requests to the storage team (§9). A1 to
  G2 are work units (§10). Q-n are questions for the owner (§12). RT-n are the red team's
  findings, and *(A-n)* marks the change that answers RT-n.
- **Names.** Names in `code` that this document introduces are proposals. Names that exist are
  cited with a file and line.

---

## 1. The answer in short

1. **Base.** The design starts from core-first, the first judge's winner: typed, safe values on
   the save path. It takes the second judge's winner, parity-first, for transport, the backend
   before tc-store, the parity gate and the one-to-one port of tC4's views. It takes slice-first's
   endpoint table, its project reference and its tests. Both judges' graft lists point to this
   same shape.
2. **tc-api** holds wire types only. It depends on `tc-core`, `serde` and `thiserror`, and builds
   for wasm32. Tokens are opaque: `ShownToken` for an edit and `ReviewToken` (every head) for a
   resolution, kept apart by type down to the storage seam and by a prefix on the wire
   *(A-7)*. Every editable value arrives as `Shown<T>`. Every write carries a `RequestId`
   and gets an `Ack` only after STORAGE's B3 step. A retry is matched to its first attempt by
   id and by a digest of its body *(A-2)*.
3. **tc-app** is the only caller of storage, sync, import and export. It talks to storage through
   one seam, `ProjectStore` and `ProjectHandle`. Before S1 a read-only `MemoryBackend` folds the
   fixture projects with tc-journal's real fold. It executes no command. After S1 a
   `StoreBackend` adapts tc-store's actor to the seam; it is a unit of its own, sized by what
   S1 provides *(A-5)*. One contract suite runs on both.
4. **tc-server** is axum on `127.0.0.1`. Every request carries the token in a header, as
   ARCH:42-44 says: the UI's own `fetch` calls send it, and the shell (or Playwright) adds it to
   loads the browser makes by itself. Events come on one server-sent-events stream that the UI
   reads with `fetch`.
5. **The UI** ports tC4's views file by file behind a `use_app()` façade with tC4's names. The
   state behind it has three sources: server data, unsaved drafts, and view state. Saves follow
   a port of tC4's save scheduler, with tC4's timing per field: the section editor writes only
   on Save *(A-1)*, a user comment only on blur *(A-18)*, and the server saves alignments
   *(A-4)*. Typed text is in a browser buffer on every keystroke, mirrored by the shell to a
   file in app data, and in `.tc/drafts.json` within a second. A crash of the server, the
   window or the whole app loses at most the keystroke being handled *(A-3)*.
6. **Parity** is checked in four layers (pixels, DOM, meaning, behaviour) against the committed
   tC4 record, plus a tC4 view-state gallery for states with no baseline. A cargo test gates the
   report, as `gallery_record.rs` gates the design system.
7. **Performance** is proved with operation counts on the Psalms corpus and on ten times Psalms,
   from the first data unit on, and with release budgets measured per screen.
8. **Before tc-store exists,** all 62 baseline screens can be built and compared: 57 on real
   fixture data, 5 on scripted job outcomes that are captured again when tc-sync lands. Every
   journey that writes waits for S1 or a later unit.
9. **The work** is 47 units in seven phases *(A-16, A-17)*. Phase 0 (14 units) can start now on
   main; one of them, the Electronite spike G0, must be run on Windows by the owner or a Windows
   runner. The seed spike B1a can start now on a branch cut from `storage`. Phase 1 (14 more
   units) starts when unit C merges. Phase 2 starts with the store adapter C0, whose size
   depends on whether S1 provides R1, R4, R5, R14 and R16 (§9.2).
10. **Open questions.** 28 points need the owner (§12). The most urgent is Q-23: five requests
    this design depends on are in no storage unit's task text, and only the owner can add them
    *(A-5)*. The others with the most weight are feedback sending (Q-2), checkpoints on every
    mode switch (Q-5), alignment reflow after a verse edit (Q-6), the Understand comment box
    over append-only comments (Q-24) and the Windows run of G0 (Q-26).

---

## 2. Decisions

### 2.1 The decisions

| Id | Decision | Why | Rejected |
|---|---|---|---|
| AD1 | Core-first is the base; parity-first and slice-first supply grafts (§2.2 lists each). | Judge 1 scored no-loss highest and picked core-first; judge 2 scored parity and delivery highest and picked parity-first. Their graft lists converge. | Parity-first as the base: one opaque token type, no write capability, no retry dedupe. Slice-first: no retry dedupe, list round trips inside 16 ms budgets. |
| AD2 | `tc-api` holds wire types only and does not depend on `tc-journal`. `tc-app::wire` converts. | The UI must never reach storage types (ARCH:88), and the wire must not move when the journal changes. | Re-exporting journal types through tc-api. |
| AD3 | Two opaque token types, `ShownToken` and `ReviewToken`, with no constructor and no accessor. The seam keeps them apart too: `mint` returns an `EditToken` and `mint_review` a `ReviewToken`, two tc-app types around tc-journal's one token, and `tc-app::wire` converts each to its own wire type only. The wire forms carry the prefixes `st1.` and `rt1.`; parsing refuses the wrong prefix. R19 asks tc-journal to put the purpose under the MAC, so the actor also refuses a review token in a save and an edit token in a `Resolve`. Tokens never enter the DOM; the DOM test looks for the two prefixes. *(A-7)* | STORAGE:756-765: a conflict's edit token lists only the shown candidate; `Resolve` needs every head (STORAGE:1610-1616). tc-journal has one `ShownToken` for both (`storage:crates/tc-journal/src/token.rs:54`) and its MAC does not say which kind it is, so a review token used in a save would supersede every head and end the conflict unseen. | One `ShownTokenText` for both; separate types in tc-api only, which stop at the wire. |
| AD4 | Every write carries a `RequestId`. The browser buffer entry keeps the id with the text that was sent (`sent`) beside the current text. A per-session `ReplyCache` (1,024 entries, memory only) keys each id with a digest of the request body: a retry of a finished request gets the first reply, a retry of a request still running waits for its reply, and a retry whose body differs gets `ApiError::RequestReused`, never the cached `Ack`. A field is clean only when `Ack.saved` equals the NFC form of its current text. A new `ServerInstance` means the cache is gone: the UI goes Home and resends nothing. Each buffered entry is offered at the next open of its project, beside the entries of `drafts.json`, unless the stored value already equals its text; a comment entry is dropped when the newest comment at its target has its text (tC4's rule, `tC4:src/data/journal/journalingStore.ts:3048-3058`). *(A-2, A-3)* | A retry rebuilt from the same token after a lost reply would otherwise make a person conflict with their own save (STORAGE:1422-1428, 1447). A retry matched by id alone could carry newer text and be answered with the older text's `Ack`, or run twice while the first attempt waits on the actor. `AddComment` has no token and mints a new id each time (STORAGE:1536). After a restart the old page cannot reach the server on its new port (§5.2), so an in-place resend cannot happen. | An echo-only request tag; matching retries by id alone; resending in place after a server restart. |
| AD5 | `Ack` arrives only after B3 and says exactly what was stored: `written`, the NFC text, the new tokens, new conflicts, `ViewSeq`, and the disk warning. | STORAGE:1415-1428. | An optimistic "Saved". |
| AD6 | One seam to storage. Before S1, `MemoryBackend` executes no command: it answers `Unavailable`, or a scripted outcome under the `test-hooks` feature. | No second copy of STORAGE's B1–B2 rules. Save behaviour in the UI is still testable before S1. | Core-first's temporary command builder, whose deletion depended on S1 publishing a pure `build_batch`. |
| AD7 | The fixture seed is STORAGE §13.2's state seed, with its checks, written as `tc_app::backend::seed::tc4_state_seed`. When unit I lands, `memory_seed_equals_tc_import_seed` runs and the copy is deleted. | §13.2 (STORAGE:3433-3442) requires byte-for-byte USFM and `vrs.json` and field-for-field sidecars. A checked seed cannot make a screen look right for the wrong reason. | A new `tc-import` crate now (unit I owns it); a seed without the check. |
| AD8 | The open's progress is tc-store's reporter, forwarded unchanged. tc-app's own reporter times only tc-app's jobs. | STORAGE:1870-1891 gives the clock to the store, and S1's `open_shows_determinate_progress_by_300_ms` tests it. | A tc-app clock for the open and an invented `Preparing` phase. |
| AD9 | Typed `POST /api/v1/<service>/<name>` with JSON. HTTP 200 carries `Reply<E> = Result<E::Ok, ApiError>`. Statuses are used only below the API: 401 token, 403 Host or Origin, 409 API version, 413 upload cap. | One exhaustive Rust match in the UI. A proxy or auth failure cannot be mistaken for a domain refusal. | 422 for every error; a result type per area without one error enum. |
| AD10 | Events come on one SSE stream, `GET /api/v1/events`, read with `fetch` and the token header. Frames carry a sequence number; a ring of 4,096 frames serves reconnects; a gap gives `Resync`. | Works the same in the shell, in Playwright and in a developer's browser, whether or not header injection reaches it. | `EventSource` (needs injection or a cookie); WebSocket (no headers; opaque to Playwright); polling. |
| AD11 | ARCH:42-44 stays as written. The shell adds `X-Tc-Token` through `session.webRequest.onBeforeSendHeaders`, with a URL filter that lists only the server's paths outside `/api/`: `/`, `/ui/*`, `/help/*`, `/media/*`, `/download/*`, `/print/*` and `/prepare.html`. The UI's own `fetch` calls set the header themselves, so no API request or event-stream read passes through the main process. Playwright uses `extraHTTPHeaders`. Only debug builds accept a one-time `?dev-token=` cookie. Capability URLs are the fallback if Electronite misses a request type (G0 probes every type). *(A-14)* | No amendment to the fixed security decision; images, iframes, downloads and the print window carry the header, and the aligner's round trip does not pay for a hop through the main process. | A production launch cookie (core-first); public static files plus capability URLs (slice-first); a filter over every request. |
| AD12 | A project is named by `ProjectRef`, its folder under the projects root. `ProjectId` is data. Scopes, sessions, Home cards and summaries are keyed by folder. `prefs/<projectId>.json` stays as STORAGE §3.7 lists it. | Two imports of one tC4 project get one id (STORAGE:3374, T4), and a copied folder is expected (STORAGE:2015-2036, `FolderCopied`). Two folders must be two cards. | Keying by `ProjectId`. |
| AD13 | Home never folds a project. A card reads `metadata.json`, `.tc/sync.json`, `ingredients/tc/conflicts.json` and a summary file stamped by the names of the pack and pending files. A missing or stale summary is rebuilt in the background from the derived USFM files and `conflicts.json`, with no lock and no fold, one project at a time, cancelled when the person opens any project. | PERF rule 1. tC4 computes the drafted percent from USFM too (`tC4:src/state.jsx:2742-2752`). Reading derived files needs no `.tc/lock`, so a person's open is never pushed into read-only mode by a background job (STORAGE:1790). | A background read-only fold (it holds the lock and a whole project's memory); a lockless "peek" open requested from S1 (not needed). |
| AD14 | Settings opened from Home show values from the derived files. Save opens the project with progress, checks that the fold's projected values equal what the dialog showed, mints tokens for those values, and saves. A difference shows "This project changed. Look again." The open goes through the registry's single open per folder (AD38), so a card click during the save shares the session instead of finding the lock held. *(A-8)* | The dialog does not fold a whole Bible to appear. The equality check keeps I4: the minted heads stand for exactly what was shown. | `session.open{purpose: Settings}`. |
| AD15 | A bulk resolution is one batch, confirmed with its count first, or it is not offered. Until the command exists (R7), the review offers only the choices the command set carries, and "Later". | STORAGE:1619-1626. | One `Resolve` per conflict: partial results, and tc-app would reimplement the device-set rule. |
| AD16 | Switching chapter sends no request. `editor/book` returns the book head with the focus chapter; `editor/chapters` returns the rest. Events re-query only the chapters they name. | PERF:73, 75. A 16 ms switch cannot include a round trip and a render. | A request per chapter. |
| AD17 | The check rail arrives once per check session as compact rows with flags and group keys. After that it is patched row by row: an `Ack` carries the rows its write changed, and a `Changed` event names the rows in `ChangeScope.records`. A cached open sends only the rows changed since the `ViewSeq` of the rail the UI still holds *(A-12)*. The UI masks, orders and groups with `tc_api::model::rail`, which buckets rows by the term index in one pass *(A-13)*. `tc-app` proves it equals `tc_check::rail::groups` for every filter and mode on Titus and Psalms. Category groups are ordered after the mask, by first appearance among shown rows. | `tc_check::rail::groups` filters, then sorts, then orders categories by first appearance among the kept rows (`crates/tc-check/src/rail.rs:113-172`); a group order computed before the filter is wrong under a filter. Filter changes send no request (PERF:78). The Psalms tN rail is about 1.2 MB of JSON (estimate), so a refetch per decision or per cached open misses the 20 ms budget (PERF:77). tC4's and tc-check's grouping filter every row once per term (`tC4:src/views/Check.jsx:889-898`, `rail.rs:138-156`): about 3.7 million comparisons on Psalms TWL's 7,683 rows and 485 terms. | Precomputed group starts; a copy of tc-check's rail in tc-ui; windowed rail requests; refetching the rail on every change. |
| AD18 | The aligner runs on the server. `tc_align::Aligner` holds the selection (`click_bank_word`, `selection`, `is_armed`, `crates/tc-align/src/aligner.rs:337-358`). Every click is a round trip; the UI queues clicks in order and sends the next when the previous reply arrives, so a fast click is never refused or lost *(A-14)*. `align/open` on a verse whose session is live returns that session. The server owns the alignment save: 2 s after the last link change, on leaving the verse, when the session is replaced or closed, and before the quit checkpoint; its state feeds the save indicator and the close guard *(A-4)*. The view is built by a pure function of the `Aligner` that depends only on tc-align and tc-api, so Q-13's fallback is a switch, not a rewrite *(A-14)*. A2 measures the loopback round trip through a header-adding proxy; G0 measures it in Electronite on Windows before B6 starts *(A-14)*. | One source of truth for the selection; unsaved links survive a reload; nobody but the server knows that links are unsaved. | Armed words in UI state; tc-align in wasm (needs an exception to ARCH:88, Q-13); a UI timer for the alignment save. |
| AD19 | A verse edit writes nothing on its alignment. The aligner opens the verse with the reflowed value, and a lossless reflow is saved on leaving the verse. This is how `tc-align` is built (`crates/tc-align/src/reflow.rs:9-17`, `aligner.rs:85-130`). | STORAGE §0 item 4 and §5.2 item 4 (STORAGE:1482-1485). | Writing the reflow at the verse save (tC4 #213); owner choice, Q-6. |
| AD20 | A per-field save state machine ports tC4's `SaveScheduler`, FR-32 included, with tC4's timing per field (§6.5). Verse, layout and story text save on blur or after 2 s idle. The section editor writes only on Save; blur and the timer write only its draft *(A-1)*. A user comment saves on blur; the timer writes only its draft *(A-18)*. A check comment saves on Done and on leaving the item *(A-10)*. Every decision change saves at once. Alignments are saved by the server (AD18). | tC4 flushes a decision at once (`tC4:src/state.jsx:5683-5685`); STORAGE's "1 s after the last change" (STORAGE:1522) is a ceiling this meets. tC4's section card writes nothing until Save (`tC4:src/views/SectionEditor.jsx:4-6`), and each section save is a passage batch that can join, split or renumber verses. STORAGE comments are append-only (STORAGE:1534-1541), so a save per pause would leave a comment per pause. | Decisions after 1 s; one blur-or-2 s rule for every text field. |
| AD21 | The browser draft buffer is `localStorage`, written synchronously on every input. In the shell, the preload also sends each change to the main process, which writes it to `drafts-buffer.json` in app data at once and fsyncs it at most once a second *(A-3)*. A `Draft` command follows at most once a second. Within a launch the buffer restores fields after a reload. After any crash, and across launches, the shell's mirror and `.tc/drafts.json` are offered at the next open of the project. Entries are keyed by folder and project id *(A-9)*. | An origin is scheme, host and port, and every launch has a new port (ARCH:12). Chromium may hold `localStorage` writes in memory before it commits them, so even a stable origin would not keep the last keystrokes through a crash; a file the main process has written survives a crash of any process in the operating system's cache. | Reusing the previous launch's port (fragile; the port may be taken); relying on a stable origin (Q-1) for no-loss. |
| AD22 | Open sessions live in the server. `app/hello` returns them, so a reload after a panic resumes without reopening the project. A live aligner session keeps its unsaved links, and the server saves them (AD18) *(A-4)*. The screen that panicked is not restored: the reload shows the view-error callout in its place, and its buffered fields stay dirty until they are saved or offered *(A-15)*. | A reopen costs 3–10 s (PERF:89-90) and would drop the live aligner session. Restoring the state that panicked can panic again. | Reopening on reload; restoring the editor that panicked. |
| AD23 | `use_app()` returns tC4's names: `s` for state and `actions` for actions. Server data in `s` is a read-only view of the query cache; only view state is writable; drafts live in the save machine. | A builder ports `Check.jsx` top to bottom, and each value has one owner. | One writable signal per tC4 field; a three-input model with no tC4 names. |
| AD24 | Logic both sides run lives in `tc_api::model`: the section buffer, the drag range, text truncation, rail masking and grouping, change-scope matching. Differential tests in `tc-app` compare each with its domain twin. | `tc-app` depends on both `tc-api` and the domain crates, so a separate test crate adds nothing. | A `tc-crosscheck` crate; ports in `tc-ui` with twins elsewhere. |
| AD25 | The UI asks the internet question before it sends any request whose endpoint declares `NET`. The server still refuses with `NeedsInternet` if the switch went off meanwhile, and the UI asks once more. | tC4's "Not now sends nothing" (ARCH:49-54); the server enforces the switch (`crates/tc-door43/src/internet.rs:19-45`). | Hand-wired kinds at call sites. |
| AD26 | Uploads are raw request bodies, one file each, streamed to app data. "Receive work from a file" takes a path from the shell in the desktop app. | No multipart parser; bytes never pass through wasm memory; a 4 GiB exchange file is not copied. | Multipart; uploads for USB files. |
| AD27 | Language suggestions run on the server (`home/languages`), a port of `tC4:src/data/languageSearch.ts`. | The 1 MB language list stays off the wasm heap, and the ranking has one copy. | Shipping `langnames.json` to the UI. |
| AD28 | The port of `renderArticleBlocks` (`tC4:src/data/articles.ts:142`) lives in `tc-helps`. | Pure logic belongs in a pure crate. | tc-app. |
| AD29 | Help pages are vendored from tc-website `cdfdf9b7…` with a per-file sha list and a licence record. | They are not in tC4's git (`parity/tc4-baseline/README.md:18-19, 41-46`); `vendor-tc4.sh` lists files with `git ls-tree` of a tC4 commit (`crates/tc-ui/vendor-tc4.sh:29-33`). | Extending `vendor-tc4.sh` with blob ids. |
| AD30 | A panic hook stores a crash marker and reports the error; `boot.js`, not wasm, reloads the page and counts the page loads that ended in a panic in `sessionStorage`. Three within 10 s stop the reloads; `boot.js` then draws tC4's view-error callout as static DOM and posts every buffered entry as a `Draft` with a plain `fetch`. A capture listener in `boot.js` writes each keystroke to the buffer before wasm handles it. *(A-15)* | wasm aborts on panic (`Cargo.toml:28`), after which closures it registered trap again and a reload it scheduled may never run; tC4 shows `data-testid="view-error"` (`tC4:src/App.jsx:163-190`). | Reload forever; no recovery; counting traps. |
| AD31 | Sync, gateway change, share flow, sign-in and import each run one at a time across the app (`FlowGate`). USB export and receive run one at a time per project, never beside a sync of the same project. *(A-31)* | DELTA:660-661; tC4's flags (`tC4:src/state.jsx:176-192`). STORAGE does not say that a receive and a sync of one project may run together. | — |
| AD32 | Operation-count tests start with the first data unit (B1), on `MemoryBackend` folds of tc-corpus Psalms and ten times Psalms. | Count rows do not wait for S1 or P. | Counts only after P. |
| AD33 | The app track builds `tc-export`: the print model early (B8), the producers after S2 (D2). | No storage unit owns it; the Community Checking baseline needs the print model. | Waiting for an owner. |
| AD34 | Checkpoint on every mode switch, as tC4 does (`tC4:src/state.jsx:4031-4038`), until the owner and the storage team rule (Q-5). Before R17 is adopted, unit P measures save latency while a checkpoint runs (R9), and the storage team is asked to keep the long part of a checkpoint off the actor (R21). If the 25 ms save budget still fails, Q-5 offers one checkpoint per mode switch at most every two minutes. *(A-11)* | tC4's J8 asserts it (`tC4:e2e/j08-resume.spec.ts:194`); STORAGE lists only leaving Translate (STORAGE:1723). A checkpoint is an actor step with a 500 ms budget for a whole Bible (STORAGE:1725-1735, 2720; PERF:92), so a checker who switches after each decision would wait behind one each time. | Silently following either. |
| AD35 | Test hooks exist only under the cargo features `test-hooks` and `fixture-backend`. `release_has_no_test_hooks` checks the release binary. | No hook may ship. | Runtime flags. |
| AD36 | Teammate changes in parity scenarios are written only through tc-journal's encoders (phase 1) and the store's sealing API (from C1), never by hand. | The scenario must exercise the real format. | Hand-made packs. |
| AD37 | An open editor holds its value and its token as one pair, taken when it opened. A refresh replaces the pair only while the editor has had no input and no composition since then; otherwise the field keeps its text and token and shows the "changed while you were typing" notice. A `Changed` whose cause is the field's own request shows nothing. Nothing is ever written into a focused field, and texts are compared in NFC. *(A-6)* | A value and a token that come apart make the next save conflict with itself, or revert a change unseen (STORAGE:1449-1453). The `Ack` and the `Changed` event of one write may arrive in either order (§5.4). NFC reorders Hebrew points and Arabic marks (STORAGE:1445), and writing into a focused field moves the caret and breaks an IME composition. | Following the query in an open editor; never refreshing an open editor. |
| AD38 | `SessionRegistry` runs one open per folder at a time; a second caller waits for it and shares the session. The screen session closes when the person goes Home, with tC4's "leaving the project" checkpoint. *(A-8, A-25)* | A second open finds `.tc/lock` held and opens read-only with a false message (STORAGE:1790). tC4 closes the project at Home (`tC4:src/state.jsx:7286-7328`), and J8 asserts that commit. | Waiting only for a running leave-checkpoint; closing the session only when another project opens. |
| AD39 | The shell supervises `tc4`. When `tc4` exits without being asked, the shell starts it again and loads Home at the new address; the new page reads the shell's buffer mirror (AD21). `tc4` treats a closed stdin as a quit. A `tc4` that finds `app.lock` held waits up to 10 s for the old process to finish closing, then names it (the lock records its pid). *(A-3)* | Every launch has a new port, so the old page cannot reach a restarted server (§5.2). An orphaned `tc4` would hold `app.lock` and `.tc/lock` and refuse the next launch. When the shell dies, the pipe to `tc4`'s stdin closes. | A quit endpoint for orphans (it would need the old token); restarting in place. |

### 2.2 Disagreements and how they were resolved

| # | Point | Positions | Ruling |
|---|---|---|---|
| 1 | Which design is the base | Judge 1: core-first. Judge 2: parity-first. | AD1. Core-first types, parity-first transport, backend and gate. |
| 2 | Before S1, may the fixture backend write? | Core-first: a temporary builder. Others: read only. | AD6, read only with scripted outcomes. |
| 3 | Where the seed lives | Parity-first: tc-app, handed to unit I. Slice-first: a new tc-import crate. | AD7, tc-app, deleted after the differential test. |
| 4 | Who owns the open's 300 ms clock | Core-first R5: tc-app. Others: tc-store. | AD8, tc-store. |
| 5 | Bulk resolve before a batch command | Core-first: one Resolve per conflict. Parity-first: offer what exists. | AD15, offer what exists. |
| 6 | Token on browser-native loads | Core-first: a production cookie. Parity-first: header injection. Slice-first: capability URLs. | AD11, header injection; capability URLs as fallback. |
| 7 | Event stream reader | Core-first: `EventSource` with the cookie. Others: `fetch`. | AD10, `fetch`. |
| 8 | Error encoding | 200 with ok or err; 422; statuses plus per-area results. | AD9. One `ApiError` whose `Refused` variant holds per-family refusal enums. |
| 9 | Project key | Core-first and parity-first: `ProjectId`. Slice-first: folder. | AD12, folder. |
| 10 | Home summaries | Fold in a read-only job; a lockless peek requested from S1. | AD13, derived files, no fold, no lock, no request to S1. |
| 11 | Settings from Home | Open a session to show it; show from files and open on Save. | AD14, files, open on Save, with an equality check. |
| 12 | Chapter switch | A request per chapter; the whole book in two replies. | AD16, two replies. |
| 13 | Rail grouping | Judge 2: server group starts are equivalent to tc-check. Judge 1: wrong under a filter. | The code agrees with judge 1 (`rail.rs:157-172`). AD17 groups after the mask, with a differential test. |
| 14 | Aligner selection | Core-first: UI view state. Slice-first: the server's `Aligner`. | AD18, the server. |
| 15 | Reflow after a verse edit | Core-first Q-9 asked; others missed it. | The code already decides it (AD19). The visible difference goes to the owner (Q-6). |
| 16 | Decision timing | Core-first: 1 s. Parity-first: at once. Slice-first: status at once, the rest after 1 s. | AD20, every decision change at once, as `recordDecision` does; the other fields keep tC4's own timing, field by field *(A-1, A-10, A-18)*. |
| 17 | Browser buffer across launches | Core-first: reuse the port. Parity-first: assumed it survives. Slice-first: only `drafts.json`. | AD21, the shell's mirror and `drafts.json` across launches and crashes *(A-3)*; Q-1 no longer decides whether typed text survives. |
| 18 | Recovery after a panic | Parity-first: reopen. Others: sessions in the server. | AD22. |
| 19 | UI state shape | One signal per tC4 field; three inputs with new names. | AD23, tC4 names over three sources. |
| 20 | Shared logic | Parity-first: ports in tc-ui plus tc-crosscheck. Core-first: tc-api models. | AD24, tc-api models with differential tests in tc-app. |
| 21 | Internet question | Core-first: server refuses first. Others: UI asks first. | AD25, UI first; server refusal as backstop. |
| 22 | Checkpoint per mode switch | Parity-first and slice-first adopted tC4's rule without asking. | AD34, tC4's rule now, Q-5 to the owner, R17 to the storage team. |
| 23 | Uploads | Core-first: multipart and `fromPaths`. Parity-first: raw bodies. | AD26. |
| 24 | Language search | Parity-first: in the UI. Core-first and slice-first: on the server. | AD27, the server. |
| 25 | `renderArticleBlocks` | tc-app; tc-helps. | AD28, tc-helps. |
| 26 | Help pages | Extend `vendor-tc4.sh`; vendor from tc-website. | AD29, tc-website with a sha list. |
| 27 | When `tc-export` is built | Core-first: early. Parity-first: after S2. | AD33, print model early, producers after S2. |
| 28 | When the "Recommended" count is added | Core-first: after X. Parity-first: with the share dialog. Slice-first: no unit. | With the share-dialog capture, unit B9b (R12). |
| 29 | Scripted screens | Parity-first: a `pending_until` gate. Others: none. | Adopted (§7.5). |
| 30 | Oversized units | Parity-first's B5 and B9; the red team's B1, A4, A6, B4, C7, E1 and C1. | Split into B5a/B5b and B9a/B9b; then B1a/B1b, A4a/A4b, A6a/A6b, B4a/B4b, C7a/C7b, E1a/E1b and C0/C1, with the non-UI parts of B8 and B9a moved to phase 0 as A10 and A11 *(A-5, A-16, A-17)*. |
| 31 | Start screens before unit C finishes | Judge 2's optional graft. | Not by default: unit C is already in progress. Kept as an option in Q-16. |
| 32 | Proof that the harness split changed nothing | Parity-first said the record pins hashes. | It pins sizes and byte counts (`parity/tc4-baseline/baseline.test.mjs:45-59`). A sha check is added (A6). |

---

## 3. tc-api

### 3.1 Rules for the crate

- Builds for the host and for `wasm32-unknown-unknown`. CI adds
  `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`.
- Dependencies: `tc-core` (which already serializes `BookId`, `VerseSpan` and `Reference`,
  `crates/tc-core/src/books.rs:227-237`, `lib.rs:93-145`), `serde`, `thiserror`.
- No I/O and no `std::time`. Times are `WallMs(i64)`.
- Every identifier is a newtype whose `Deserialize` validates its format. Requests use
  `#[serde(deny_unknown_fields)]`. Enums are internally tagged with camelCase names.
- Every enum on the wire is closed. A `proptest` round trip covers every type.
- Pure logic only where both sides must agree (§3.8).

### 3.2 Identifiers and scope

```rust
// crates/tc-api/src/ids.rs
pub struct ProjectRef(String);       // the folder name under the projects root: one path segment, validated
pub struct ProjectId(String);        // tc/project.json `projectId` (STORAGE:1028-1036); data, never a key
pub struct Epoch(u64);               // one open of one folder in this server run; never reused
pub struct Scope { pub project: ProjectRef, pub epoch: Epoch }
pub struct ViewSeq(u64);             // per session: +1 for every fold delta the session applies
pub struct RequestId([u8; 16]);      // made by the UI, one per save attempt of one text
pub struct ServerInstance([u8; 16]); // random per launch; a change means the server restarted
pub struct JobId(u64);
pub struct UploadId(String);         // 32 hex
pub struct DownloadId(String);       // 32 hex
pub struct PlanId(u64);
pub struct ProposalId(u64);
pub struct CheckSessionId(u64);
pub struct AlignerId(u64);
pub struct ConflictRef(String);      // 64 hex, tc-journal ConflictId
pub struct ChangeRef(String);        // 64 hex, tc-journal ChangeId
pub struct RecordRef(String);        // canonical JSON of a RecordKey (STORAGE:823-869); opaque to the UI
pub struct ReportId(u64);
pub struct LocaleId(String);         // one of the 10 ids of tc-ui's LOCALES
pub struct WallMs(i64);
```

### 3.3 Endpoints

```rust
// crates/tc-api/src/endpoint.rs
pub enum Kind { Query, Command, StartJob }
pub enum NetKind { Share, Upload, SignIn, CheckSignIn, Sources, Fix, ImportVersions,
                   Profile, RepoLink, HelpLink, Feedback, Sync, Open }   // tC4's `net.ask.<kind>`

/// One request. The request type is `Self`; the path is POST /api/v1/{PATH}.
pub trait Endpoint: Serialize + DeserializeOwned + 'static {
    const PATH: &'static str;              // "editor/saveVerse"
    const KIND: Kind;                      // a Query never changes state; a Command carries a RequestId
    const NET: Option<NetKind> = None;     // the UI asks the internet question first (AD25)
    type Ok: Serialize + DeserializeOwned;
}
pub type Reply<E> = Result<<E as Endpoint>::Ok, ApiError>;

// endpoints! { app::Hello, prefs::SetLocale, editor::SaveVerse, ... }
// generates `ALL: &[(&str, Kind, Option<NetKind>)]`. tc-server builds its routes from the same
// list, and tc-app must implement `Handle<E>` for each, so a request without a handler does not
// compile.
pub const API_VERSION: u32 = 1;           // header `X-Tc-Api`
```

`NetKind::Upload` stays because tC4's catalogs have the key, but no endpoint uses it: ruling Q2
removes the Upload changes dialog (DELTA:1137, PLAN:49-50).

### 3.4 Shown values, writes and acknowledgements

```rust
// token.rs. Deserialize checks the prefix ("st1." for ShownToken, "rt1." for ReviewToken), then
// the base64url alphabet and length. No constructor, no accessor, no Display; Debug prints
// "ShownToken(..)". *(A-7)*
#[serde(transparent)] pub struct ShownToken(String);   // STORAGE:745-767
#[serde(transparent)] pub struct ReviewToken(String);  // every head of a record (STORAGE:1610-1616)

// shown.rs
pub struct Shown<T> { pub value: T, pub token: ShownToken, pub marker: Option<ConflictMarker> }
pub struct ConflictMarker { pub conflict: ConflictRef, pub versions: u8 }  // "2 versions · Review"
pub struct PersonLabel { pub who: Who, pub computer: Option<String> }
pub enum Who { Mine, Username(String), DisplayName(String), Teammate(u16) }  // STORAGE:2686-2691
pub enum Known<T> { Known(T), Unknown }        // "—", never a false 0 (tC4:src/views/Home.jsx:259-264)
pub enum Direction { Ltr, Rtl }

// write.rs. Every write request carries `at: Scope`, `request: RequestId` and the token(s) of
// what was displayed. Examples:
pub struct SaveVerse { pub at: Scope, pub request: RequestId, pub shown: ShownToken, pub words: String }
pub struct SaveLayout { pub at: Scope, pub request: RequestId, pub shown: ShownToken, pub layout: LayoutChoice }
pub struct SaveSection { pub at: Scope, pub request: RequestId, pub book: BookId,
                         pub displayed: Vec<ShownToken>,      // every verse and chapter front shown (STORAGE:1467-1473)
                         pub layout: Option<ShownToken>,      // the BookLayout token when the book has one
                         pub buffer: model::SectionBuffer }
pub struct SaveDecision { pub at: Scope, pub request: RequestId, pub session: CheckSessionId,
                          pub item: u32, pub shown: ShownToken, pub verse_shown: ShownToken,
                          pub patch: DecisionPatch }
pub struct AddComment { pub at: Scope, pub request: RequestId, pub target: CommentTargetRef, pub text: String }
pub struct Resolve { pub at: Scope, pub request: RequestId, pub conflict: ConflictRef,
                     pub review: Vec<ReviewToken>, pub decide: Decide }  // Keep(candidate) | Proposal(id)

pub struct Ack {
    pub view: ViewSeq,                    // the snapshot that holds this write (read-your-writes)
    pub stored: Stored,                   // *(A-30)*
    pub tokens: Vec<TokenUpdate>,         // the next save of the field uses these (STORAGE:1455)
    pub new_conflicts: Vec<ConflictBrief>,// "Juan changed this verse while you were editing…"
    pub saved: Vec<SavedValue>,           // the stored NFC text; the field is clean only if it
                                          // equals the NFC form of the field's current text *(A-2)*
    pub rows: Vec<RailRowPatch>,          // check rail rows this write changed *(A-12)*
    pub disk_warning: Option<String>,     // WrittenNotDurable → SavedWithDiskError (STORAGE:1424-1428)
}
pub enum Stored { Written, Unchanged }    // Unchanged: equal to the displayed value (STORAGE:1447)
pub struct TokenUpdate { pub record: RecordRef, pub token: ShownToken }
pub struct ConflictBrief { pub conflict: ConflictRef, pub record: RecordRef, pub by: PersonLabel }
pub enum NotSaved {                       // the text stays in the editor: "Not saved: <reason>. Retry"
    Refused(SaveRefusal),                 // B1 (STORAGE:1411-1413)
    Disk(String),                         // B3 NotWritten (STORAGE:1422-1424)
    ReadOnly(ReadOnlyReason),             // O1, O4, O5, SecondWriter
    LookAgain,                            // Resolve over moved heads (STORAGE:1615-1616)
    PlanStale,                            // an upgrade or gateway plan over moved heads (STORAGE:1603-1606)
    StaleScope,                           // the project was closed or reopened under the field; the
                                          // UI reattaches and resends with the field's base token *(A-19)*
    BadToken,                             // a forged token, or a token of the wrong kind: a bug, reported.
                                          // A draft whose token no longer verifies never gets here:
                                          // it is offered as text (§6.6) *(A-9)*
}
```

### 3.5 Errors and refusals

```rust
pub enum ApiError {
    Version { server: u32 },                    // a stale UI after an update: reload
    BadRequest { field: String, reason: String },// a boundary parse failure: a bug, logged
    StaleScope { current: Option<Epoch> },      // a query's project closed or reopened: drop the reply.
                                                // A write never gets this: it gets NotSaved::StaleScope *(A-19)*
    StaleSession,                               // a check or align session was replaced
    RequestReused,                              // a RequestId came back with another body: a bug,
                                                // reported; the field stays dirty and saves anew *(A-2)*
    NotSaved(NotSaved),                         // writes only
    Refused(Refusal),                           // a domain refusal in tC4's words
    NeedsInternet(NetKind),                     // nothing was sent
    NeedsSignIn(SignInReason),                  // NotSignedIn | TokenRejected | PrivateRepository (Q3)
    Busy(Flow),                                 // a one-at-a-time flow runs (AD31)
    Unavailable(Feature),                       // the backend cannot do this yet (before S1, X, I)
    Internal { report: ReportId },              // a bug; "Ask for help" carries the report
}

/// Closed, grouped by family. Each family enum implements `code()` (tC4's code where one
/// exists), `key()` (the catalog key) and `vars()` (placeholder values).
pub enum Refusal {
    Open(OpenRefusal),          // NewerFormat, NotAProject, Unreadable
    Create(CreateRefusal),
    Import(ImportRefusal),      // refusal.import.*, importer.*
    Export(ExportRefusal),      // refusal.export.* (5 keys)
    Sync(SyncFailure),          // { refusal: SyncRefusal, after_intake: bool } (Q8)
    Share(ShareRefusal),
    OpenDoor43(OpenDoor43Refusal),
    SignIn(SignInRefusal),      // signIn.error.*
    Resources(ResourceRefusal),
    Exchange(ExchangeRefusal),
    Feedback(FeedbackRefusal),
    Prefs(PrefsRefusal),        // a failed app-language save keeps the dialog (tC4:e2e/app-language.spec.ts:263)
}
pub enum SyncRefusal {          // DELTA:740-800; tC4 SYNC_REFUSALS (tC4:src/views/SyncResult.jsx:14-51)
    Offline, NotSignedIn, AuthFailed, NotShared, NoWriteAccess { server_message: String },
    RefRejected { server_message: String }, Busy { attempts: u8 }, RemoteMissing, Unreachable,
    ProjectMismatch, NeedsNewerApp { major: u16, min_reader_minor: u16 }, Tc4TeamRepository,
    ReadOnly, Io { reason: String },
}
```

The UI builds a sentence the way tC4's `failureText` does: diagnosis, then the recovery sentence
(`tC4:src/state.jsx:1781-1788`). "Ask for help" shows when the sentence contains `feedback.ask`
(`tC4:src/state.jsx:1792-1793`).

Tests in `tc-api`:

- `every_refusal_has_a_key_in_every_locale`: each variant's key resolves in all 10 vendored
  catalogs or in the rebuild key files, with matching placeholders. A rebuild key whose text is
  still English is listed in `assets/i18n-rebuild/<id>.pending.json`; it passes in debug builds
  and fails the release check `release_has_no_pending_translation`, so an Arabic screen cannot
  ship in English unnoticed (Q-10). *(A-32)*
- `no_two_sync_refusals_share_a_message` (DELTA:797-799).
- `no_tc4_code_without_a_variant`: every `refusal.*` code in the vendored catalogs has a variant
  or a line in a pinned "vendored and unused" list (DELTA:790-797).
- `rebuild_keys_never_shadow_vendored_keys`.

### 3.6 Events, jobs and progress

```rust
// event.rs
pub struct EventFrame { pub seq: u64, pub instance: ServerInstance, pub event: Event }
pub enum Event {
    Resync,                                              // a gap: read every live query again
    Internet { on: bool },
    Account(AccountState),
    Job { id: JobId, kind: JobKind, project: Option<ProjectRef>, state: JobState },
    ProjectsChanged,                                     // create, import, join
    HomeCard { project: ProjectRef },                    // a summary was computed or changed
    Session { at: Scope, event: SessionEvent },
    Startup(StartupProgress),                            // first-launch seeding
}
pub enum SessionEvent {
    Opened, Closed,
    Mode(SessionMode),                                   // Writable | ReadOnly(reason)
    Changed { view: ViewSeq, scope: ChangeScope, cause: ChangeCause },
    Notice(NoticeView),                                  // STORAGE:680-735 anomalies and notices
    Checkpoint(CheckpointOutcome),                       // Committed | Unchanged | Failed(Refusal)
    MaterializeBlocked { paths: Vec<String> },           // "can't update TIT.usfm…" (STORAGE §5.10)
    Received(ReceivedSheet),                             // STORAGE:2681-2684
    Suggest { testament: Testament, status: SuggestStatus },
    AlignSave { aligner: AlignerId, state: AlignSave },  // the server's alignment save (AD18) *(A-4)*
}
pub enum ChangeCause { LocalSave { request: RequestId }, Intake { from: Vec<PersonLabel> },
                       OutsideEdit { path: String }, Upgrade, Resolve, Import, System }
pub enum ChangeScope { Parts(ChangeParts), Everything } // a list over its cap gives Everything *(A-30)*
pub struct ChangeParts {                                 // tc-app's form of the fold's Delta (STORAGE:628-629)
    pub chapters: Vec<(BookId, u16)>, pub books: Vec<BookId>, pub stories: Vec<u8>,
    pub checks: Vec<(Tool, BookId)>,                     // an open rail asks `check/rows { since }`
                                                         // for the rows that changed; never the whole rail *(A-12)*
    pub records: Vec<(RecordRef, Option<PersonLabel>)>,  // capped at 256; who wrote each
    pub aspects: Vec<Aspect>,                            // Pins | Settings | Meta | Conflicts *(A-30)*
}
impl QueryKey { pub fn depends_on(&self, scope: &ChangeScope) -> bool }  // one rule for the UI and the tests

// job.rs
pub enum JobKind { Open, Create, Join, Sync, Share, ExchangeExport, ExchangeReceive, Import,
                   ImportVersions, AddUsfmBooks, Download, UpgradePlan, GatewayPlan, FixFetch,
                   Export, HomeSummary, Train, Seed }
pub enum JobState { Queued, Running(Progress), Done(JobOutcome), Failed(ApiError), Cancelling, Cancelled }
pub struct Progress { pub phase: Phase, pub done: u64, pub total: Option<u64>, pub item: Option<String> }
pub enum Phase {
    Open(OpenPhase),   // FinishingWork | ReadingChanges | Folding | CheckingFiles | WritingFiles (STORAGE:1870-1881)
    Sync(SyncPhase),   // Checkpoint | Fetch | Intake | Integrate | MergeCommit | Push | Confirm (STORAGE:2709-2747)
    Share(SharePhase), Exchange(ExchangePhase), Install { repo: String }, Import(ImportPhase),
    Export(ExportPhase), Plan { book: Option<BookId> }, Train, Seed, Summary,
}
```

`JobOutcome` is typed per kind: `Opened(OpenedSession)`, `Synced(SyncResultView)`,
`ExportReady { download, file_name }`, `PlanReady { plan, consequences }`, `Created { project }`.
A final outcome stays readable through `jobs/get` for 10 minutes.

No screen shows `Train` or `HomeSummary` progress. Their `Job` frames go out once a second at
most; only their end (`Done`, `Failed`, `Cancelled`) goes out at once. The other jobs keep the
100 ms rate of §4.6, because a screen shows them. *(A-29)*

### 3.7 View types

The view types follow the shapes tC4's views read, so each screen ports one to one. A value that
is a state is an enum, as AGENTS.md asks *(A-30)*. The main ones:

```rust
// text.rs: tC4's bookModel verse entry (tC4:src/data/bookModel.js:30-37) as a Shown value.
pub struct BookHead { pub book: BookId, pub chapters: Vec<u16>, pub focus: ChapterModel,
                      pub draft_pct: Known<u8>, pub layout: Option<ShownToken>, pub cross_frame: bool }
pub struct ChapterModel { pub chapter: u16, pub front: Option<Shown<String>>,
                          pub verses: Vec<VerseRow>, pub sections: Vec<SectionRange> }
pub struct VerseRow {
    pub n: VerseLabel,                 // "4" or "4-5"
    pub drafted: bool, pub para: bool, pub format: Option<ParaFormat>,
    pub text: String,                  // display text
    pub shown: Shown<VerseView>,       // what the editor edits (STORAGE `words`) with its token
    pub last_by: Option<PersonLabel>,  // "Juan changed this verse while you were typing"
}
pub enum VerseView { Text { words: String, layout: LayoutView, last: u16 }, Covered { by: u16 }, Absent }

// source panes carry token runs and highlight word indices; the UI never matches quotes
// (tc_helps::highlight_quote, crates/tc-helps/src/highlight.rs:268).
pub struct SourcePane { pub id: String, pub name: String, pub version: String, pub dir: Direction,
                        pub chapters: Vec<SourceChapter> }

// helps.rs
pub enum Slot<T> { Ready(Vec<T>), None, Unavailable(Why), Fetch { pin: String }, Missing(Why),
                   OldTwl, Versification(Why), Error(Failure) }   // tC4:src/views/HelpsPanel.jsx:109-137
pub enum ArticleView { Found { title: String, blocks: Vec<ArticleBlock>, dir: Direction }, Missing, Error(Failure) }
pub struct ArticleBlock { pub kind: BlockKind, pub quote: u8, pub spans: Vec<Span> }   // H | Li | P

// check.rs: the rail arrives once per check session, then by row (AD17).
pub struct CheckSessionView { pub session: CheckSessionId, pub tool: Tool, pub unit: CheckUnit,
                              pub view: ViewSeq,                 // `check/rows { since: view }` patches from here *(A-12)*
                              pub rows: RailRows,                // Full(Vec<RailRow>) | Since(Vec<RailRowPatch>) *(A-12)*
                              pub terms: Vec<String>,            // tW terms in locale order (ByWord)
                              pub progress: Progress3, pub citation: ResourceCitation,
                              pub dropped: u32, pub empty: Option<CheckEmpty>, pub start: u32 }
pub struct RailRow { pub index: u32, pub reference: Reference, pub title: String,
                     pub flags: RowFlags,                        // decided, status, invalid, bookmarked, commented
                     pub term: Option<u32>, pub category: Option<Category>,
                     pub marker: Option<ConflictMarker> }
pub struct CheckItemView { pub index: u32, pub reference: Reference, pub quote_title: String,
                           pub note: Vec<ArticleBlock>, pub article: Option<HelpLink>,
                           pub notices: Vec<CheckNotice>, pub orig: Option<SourceVerseView>,
                           pub gateway: Option<SourceVerseView>, pub target: Shown<TargetWords>,
                           pub decision: Shown<DecisionView> }

// align.rs: the session lives on the server (AD18); every edit returns the whole view.
pub struct AlignerView { pub aligner: AlignerId, pub reference: Reference, pub counter: (u32, u32),
                         pub bank: Vec<BankChip>, pub selected: Vec<u32>, pub armed: bool,
                         pub cards: Vec<WordCard>, pub placed: (u32, u32), pub suggest: SuggestView,
                         pub stale: bool, pub dropped: Vec<DroppedLinkView>, pub blocked: Option<AlignBlocked>,
                         pub changed: bool }

// home.rs
pub struct HomeCard { pub project: ProjectRef, pub id: ProjectId, pub name: String, pub tag: String,
                      pub dir: Direction, pub kind: ProjectKind, pub imported: bool,
                      pub door43: Option<Door43Place>, pub syncs: bool,
                      pub books: Vec<BookTile>, pub stories: Option<StoryTiles>,
                      pub conflicts: Known<ConflictCounts>, pub in_progress: Known<u32> }
```

Every optional part is a `Slot`, `Known` or `Result`, so "could not be read" never looks like
"absent" (tC4's D30 rule).

### 3.8 Shared pure models (`tc_api::model`)

| Model | Port of | Twin checked by a differential test in tc-app |
|---|---|---|
| `SectionBuffer`: parse, serialize, can drop, drop pin | `tC4:src/views/sectionDraft.js:147,244,263,282` | `tc-app` turns the buffer into `SectionValue`s with the same code; proptest `parse(serialize(b)) == b` |
| `drag_range(base, from, to)` | `tC4:src/views/Check.jsx:567-666` (#737) | `tc_check::selection::from_indices` on the saved indices |
| `ArticleBlock::take_spans` | `tC4:src/data/articles.ts:127-136` | tc-helps' block renderer |
| `rail::mask` and `rail::groups` | `tC4:src/views/checkFilters.js:15-28`, `Check.jsx:889-905` | `tc_check::rail::groups` for every filter and mode on Titus and Psalms tN and tW |
| `QueryKey::depends_on` | — | the server's own invalidation tests |

### 3.9 Endpoints by service

`[kind]` marks an endpoint that declares `NET`. "Job" marks a `StartJob`.

| Service | Endpoints |
|---|---|
| `app` | `hello` → `{api, build, instance, internet, account, sessions, prefs, startup}`; `quit`; `clientError` |
| `prefs` | `setLocale`, `setDraftUnit`, `setAlignSuggestions`, `recordPlace` |
| `net` | `state`, `set` |
| `account` | `state` (sends nothing to Door43), `signIn` [signIn], `cancelSignIn`, `resume` [checkSignIn], `signOut`, `orgs` [share], `link` [profile, repoLink or helpLink] |
| `home` | `list`, `card`, `nameFree`, `versifications`, `languages` |
| `session` | `open` (job), `close`, `leave { mode \| book }`, `focus` (outside-edit check, STORAGE §11.1), `notices`, `dismissNotice`, `keepDraft`, `discardDraft`, `retryCheckpoint` |
| `editor` | `book`, `chapters`, `sources`, `original`, `saveVerse`, `saveLayout`, `saveSection`, `draft`, `addComment` |
| `helps` | `passage`, `story`, `article` |
| `story` | `list`, `open`, `saveFrame`, `saveRef` |
| `check` | `picker`, `open`, `item`, `decide`, `close` |
| `align` | `index`, `open`, `edit`, `keepReflow`, `save`, `markValid`, `suggest`, `accept`, `acceptAll`, `rejectAll`, `close` |
| `conflicts` | `summary`, `list`, `detail`, `propose`, `resolve`, `history`, `restore`, `dismissDetached`, `releaseBatch` |
| `resources` | `sources`, `catalog` [sources], `download` [sources] (job), `checkUpdates` [sources], `planUpgrade` (job), `applyPlan`, `planGateway` (job), `fixState`, `fixFetch` [fix] (job), `fixSideload`, `fixRepin` |
| `settings` | `view`, `viewFromHome`, `save` |
| `create` | `bible` (job), `obs` (job), `addBlankBooks`, `reviewUsfm`, `addUsfmBooks` (job) |
| `door43` | `shareTargets` [share], `shareCheck` [share], `share` [share] (job), `sync` [sync] (job), `open` [open] (job) |
| `exchange` | `exportFile` (job), `identify`, `receive` (job) |
| `import` | `review`, `versions` [importVersions], `run` (job) |
| `export` | `preflight`, `run` (job) |
| `feedback` | `compose`, `send` [feedback] |
| `jobs` | `get`, `cancel` |
| `test` (feature `test-hooks`) | `quiet`, `hold`, `release`, `clock`, `script`, `door43`, `inspect`, `writeTeammate`, `counters`, `reset` |

### 3.10 Versioning

- `API_VERSION = 1`, paths under `/api/v1/`, header `X-Tc-Api: 1`.
- The UI and the server ship as one build. `index.html` is `no-store`; every other UI file has a
  content-hashed name. A `build.commit` in `hello` that differs from the wasm's compile-time
  commit forces one reload.
- A new `ServerInstance` on the event stream means the server restarted. The UI drops every
  scope, resends unacknowledged comments by the rule of AD4, and goes Home. *(RT-3)*
- Persisted formats carry their own version: the browser buffer key `tc4.drafts.v1`, and
  `schemaVersion` in each app-data file. Unknown keys are kept on rewrite.

---

## 4. tc-app

### 4.1 Module map

| Module | Owns |
|---|---|
| `lib.rs`, `config.rs` | `App::start(AppConfig)`; `AppConfig { app_data, projects_root, ui_dir, door43: ServerUrl, backend, test_hooks }` |
| `backend/{mod,memory,store,seed}.rs` | the seam (§4.2) |
| `session.rs` | `SessionRegistry`, `ProjectSession`, epochs, the request guard, `ReplyCache` |
| `jobs.rs`, `flows.rs`, `events.rs` | `JobRegistry`, `CancelToken`, `ProgressSink`; `FlowGate`; `EventBus` (broadcast plus the ring) |
| `prefs.rs`, `reports.rs` | `PrefsWriter`, the one writer of app-data files; `ReportLog`, the last 50 operation reports |
| `home.rs` | `ProjectRegistry`, `SummaryCache` (§4.5) |
| `editor.rs`, `story.rs`, `helps.rs` | chapter and book models, section conversion, drafts, comments, OBS, helps and articles |
| `check.rs`, `align.rs` | preflight, `CheckListCache`, check sessions, decisions; aligner sessions, training |
| `conflicts.rs` | review model, proposals, labels, review tokens, history |
| `resources.rs` | the `ResourceStore`, book resources, lexicons, installs, plans, guided fix |
| `account.rs`, `door43.rs`, `exchange.rs`, `import.rs`, `export.rs`, `create.rs`, `settings.rs`, `feedback.rs` | one flow each |
| `language_search.rs` | port of `tC4:src/data/languageSearch.ts` |
| `wire/` | pure conversions between domain types and tc-api types, each tested on fixtures |
| `counters.rs` | `AppCounters` and `CountingSnapshot` (§8.1) |

### 4.2 The storage seam

```rust
// crates/tc-app/src/backend/mod.rs
pub trait ProjectStore: Send + Sync + 'static {
    /// Folders with ingredients/tc/project.json. No lock, no fold.
    fn list(&self) -> Result<Vec<ProjectEntry>, StoreError>;
    /// Derived files only: metadata.json, .tc/sync.json, tc/conflicts.json, the names of the
    /// pack and pending files, materialized.json's size and mtime. No lock, no fold.
    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError>;
    /// O1–O13 (STORAGE:1787-1868). Blocking: called on a job thread. Progress goes to `progress`
    /// from tc-store's own reporter thread (AD8).
    fn open(&self, folder: &ProjectRef, progress: ProgressSink) -> Result<Arc<dyn ProjectHandle>, OpenFailure>;
    /// Staging folder, initial changes, no-replace rename (R2). `Unavailable` before S1.
    fn create(&self, spec: NewProject, progress: ProgressSink, cancel: CancelToken) -> Result<ProjectRef, CreateFailure>;
}

pub trait ProjectHandle: Send + Sync {
    fn facts(&self) -> ProjectFacts;                                // kind, mode, device, presentation
    fn snapshot(&self) -> Snapshot;                                 // { fold: FoldState (O(1) clone), seq }
    fn mint(&self, key: &RecordKey, displayed: Option<ChangeId>) -> ShownToken;  // FoldState::token
    fn mint_review(&self, key: &RecordKey) -> ShownToken;           // ShownToken::mint over FoldState::heads *(RT-7)*
    fn parse(&self, text: &str) -> Result<ShownToken, TokenError>;  // refuses a bad MAC
    fn submit(&self, cmd: Command) -> CommandFuture;                // resolves after B6; never blocks the caller
    fn subscribe(&self) -> Receiver<StoreEvent>;                    // Delta + cause + notices + mode
    fn offered_drafts(&self) -> Vec<OfferedDraft>;                  // O12 (STORAGE:1857)
    fn leave(&self, what: Leave);                                   // materialize trigger (STORAGE:1639-1640)
    fn checkpoint(&self, reason: CheckpointReason) -> CheckpointFuture;
    fn counters(&self) -> StoreCounters;
    fn close(self: Arc<Self>) -> CloseFuture;                       // materialize at close, release the lock
}
pub struct StoreReply { pub tokens: Vec<(RecordKey, ShownToken)>, pub new_conflicts: Vec<ConflictId>,
                        pub written: bool, pub disk_error: Option<String>, pub seq: u64 }
pub enum CommandError { Refused(RuleRefusal), NotWritten(String), LookAgain, PlanStale,
                        ReadOnly(ReadOnlyReason), BadToken, Unavailable }

/// Jobs owned by crates that do not exist yet. Each has a scripted double until its crate lands.
pub trait SyncPort: Send + Sync { /* sync, share, join */ }
pub trait ImportPort: Send + Sync { /* review, versions, run */ }
pub trait ExchangePort: Send + Sync { /* export, identify, receive */ }
```

The seam uses tc-journal's own `ShownToken`; `tc-app::wire` turns it into tc-api's opaque text.
The token key is derived from the installation secret and the project id
(`storage:crates/tc-journal/src/token.rs:36`), so two folders with one id share a key. The scope
check (§4.3) keeps a token inside the session it came from.

**`MemoryBackend`** (feature `fixture-backend`, never in release builds):

- **Projects.** It reads the parity state layout (`parity/tc4-baseline/fixtures/build-state.mjs:9-13`)
  and runs `seed::tc4_state_seed` on each project: `ProjectMeta`, `Versification`, books through
  the codec, pins from `resources.json` with the schema 1 → 2 migration, settings per top-level
  key, decisions and alignments per record, OBS story records. It uses tc-journal's decoders:
  `decode_alignments` (`storage:crates/tc-journal/src/fold/sidecar.rs:132`), `decode_decisions`
  (`:229`) and `records_from_usfm` (`:340`), plus `tc_obs::decode_story`. It encodes the batches
  with `encode_pending` and folds them with `FoldState::fold`, so the fold takes real verified
  changes. Then it applies §13.2's check: the projection must reproduce every USFM file and
  `vrs.json` byte for byte, and every sidecar record field for field. A mismatch fails the
  scenario load.
- **Tokens.** Minted with a fixed test installation secret, so they are deterministic.
- **Commands.** `Unavailable`. Under `test-hooks`, `test/script` queues outcomes: a fresh token
  for the displayed value (the fold does not change), `NotWritten`, `WrittenNotDurable`, a delay,
  a switch to read-only, or a teammate's head added before the reply (a change encoded by
  tc-journal for another device and given to `FoldState::add`). Drafts and leaves are no-ops.
- **Teammates.** The `shared` scenario adds two teammates' heads on Titus 1:1 the same way, so
  the conflict chip and the sync toast read a real fold conflict.
- **Resources.** It installs every `repos/_local_/_sideloaded_/*` burrito of the scenario into a
  temporary `tc-resources` store, with the pins of the scenario's install records. It never seeds
  `bundle/en/`, which would change what the screens show (`parity/tc4-baseline/README.md:224-246`).
- **Scale.** It also folds tc-corpus Psalms and ten times Psalms for the operation-count tests.

**`StoreBackend`** (unit C1) wraps tc-store's `ProjectActor`. When it lands, the parity scenario
projects are written to disk through S1's create and sealing API with the same seed. The memory
backend and its feature are deleted once the last phase-1 unit has moved to the store.

**Contract suite.** `tc-app/tests/store_contract.rs` defines `store_contract!(make_store)`. Each
case is named after the rule it protects.

| Case | Runs on | Rule |
|---|---|---|
| `chapter_view_equals_seed_usfm` | both | the projection matches the source bytes |
| `minted_token_parses`, `forged_token_is_refused` | both | a screen cannot forge a token (STORAGE:745-754) |
| `review_token_lists_every_head` | both | STORAGE:1610-1616 |
| `snapshot_clone_costs_nothing` | both | STORAGE:574-577 |
| `unchanged_save_writes_nothing` | store | STORAGE:1447 |
| `reply_token_lists_only_the_new_change` | store | STORAGE:1455 |
| `stale_token_after_intake_makes_a_conflict_never_a_revert` | store | STORAGE:1449-1453, I4 |
| `retry_after_lost_reply_is_idempotent` | store (through tc-app) | AD4 |
| `read_your_writes` | store | `StoreReply.seq` (R16) |
| `section_save_checks_only_displayed_values` | store | STORAGE:1474-1481 |
| `resolve_with_moved_heads_is_look_again` | store | STORAGE:1615-1616 |
| `not_written_keeps_seq_and_retry_uses_the_same_token` | store | STORAGE:1422-1424 |
| `written_not_durable_is_saved_with_warning` | store | STORAGE:1424-1428 |
| `read_only_refuses_every_command_and_writes_nothing` | store | STORAGE:1893-1899 |
| `drafts_offered_after_reopen` | store | STORAGE:1750-1759 |

### 4.3 Sessions, epochs and the request guard

- `SessionRegistry` maps a `ProjectRef` to one `ProjectSession`. Each session counts its users:
  the screen, a sync, a USB export. It closes when its last user leaves and its checkpoint is
  done. An open of the same folder while its leave-checkpoint runs waits for it, as tC4 waits for
  `leaveCheckpoints` (`tC4:src/state.jsx:199, 2177`). *(RT-8)*
- One screen session at a time, as in tC4. Opening another project first closes the screen
  session. A sync or USB export from Home holds a side session (Q-15 on memory). *(RT-8, RT-25)*
- Each open gets a new `Epoch`. Every project-scoped request is checked before any work: a
  closed session or another epoch gives `StaleScope` and does nothing. Check and align sessions
  get ids inside an epoch, and a replaced one answers `StaleSession`. This replaces tC4's sequence
  counters (`tC4:src/state.jsx:592-622, 3592-3631`).
- `ProjectSession` holds the handle, epoch, mode, presentation, `ReplyCache`, caches (§4.4),
  check sessions, aligner sessions, plans and proposals.
- **Queries never touch the actor.** They run on the latest snapshot on a rayon pool. No query
  calls `FoldState::record_keys` (`storage:crates/tc-journal/src/fold/reads.rs:82-91`), which
  lists every key and sorts them. The one exception is `ProjectIndex::build`, which runs once per
  open in the background and then follows each delta, until R1 removes it. *(RT-21)*

### 4.4 Caches

| Cache | Key | Owner | Filled | Invalidated by | Budget |
|---|---|---|---|---|---|
| `ResourceStore` | — | app | at start; no content file read | an install | repeat launch reads no content file (PERF:100) |
| `BookResources` (LRU: current and previous book) | (pins digest, book) | session | first need, `tc_resources::load::load_book` (`crates/tc-resources/src/load.rs:163`) | a pins change | 300 ms first; 60 MiB per book (PERF:87) |
| `Lexicons` | lexicon pin | app | first lookup | an install | 100 ms load; under 1 ms per lookup (PERF:88) |
| `HelpsCache` | (pins digest, book or story, helps-source setting) | session | with `BookResources` | pins, setting | part of the 300 ms |
| `CheckListCache` | (tool, unit, resource ref, versification digest, scope digest) | session | first `check/open` | key change; a decision updates only its row | 250 ms first, 20 ms cached (PERF:77) |
| `CheckSession` | `CheckSessionId` | session | `check/open` | replaced; a verse delta re-assesses only that verse's checks | 1 ms (PERF:79-80) |
| `AlignerSession` | (book, chapter, verse) | session | `align/open` | leaving the verse; a text change marks it stale | 30 ms (PERF:83, 85) *(RT-4)* |
| `SuggestModel` | (project, testament) | trainer thread | background training from one snapshot (DELTA:627-629) | a save appends its verse (`crates/tc-align/src/suggest/engine.rs:218`) | 50 ms (PERF:84) |
| `ProjectIndex` | — | session | once per open, background | each delta | removed by R1 *(RT-21)* |
| `DraftedCounts` | book | session | at open, background | each delta | 1 ms (PERF:76) |
| `SummaryCache` | folder | app, persisted | while open, from deltas; on a miss, from derived files | stamp change | Home costs O(projects) |
| `ReplyCache` | `RequestId` | session | per write | bounded at 1,024 | — |

### 4.5 Home and the project registry

- `ProjectRegistry` lists `<projects>/` through `ProjectStore::list` and orders cards by
  `lastUsed`. Each card comes from `peek` plus a summary file.
- A summary holds per-book drafted and total verses (tC4's `draftPercent`: a span counts every
  verse it covers; `""` and `___` are undrafted; `tC4:src/data/usfm/indexer.ts:216-228`),
  conflict counts by book and kind, the team's labels, OBS story percentages and the in-progress
  count.
- The summary's stamp is SHA-256 over the sorted names of the pack and pending files (content
  hashes, STORAGE §4.2–4.3) plus `materialized.json`'s size and mtime. Listing two directories
  costs what one card touches.
- While a project is open, its summary follows each delta and is written at close.
- When a stamp differs (a folder received, copied or written by another installation), the card
  shows "—" (DELTA:928-933) and one background `HomeSummary` job reads the derived USFM files and
  `conflicts.json`. No fold, no lock. One job at a time; opening any project cancels it.
- A conflict chip reads `ingredients/tc/conflicts.json` (STORAGE:891, 2492), never a fold.

### 4.6 Background work and cancellation

- `JobRegistry::start(kind, project, f)` runs `f` on the right executor: rayon or
  `spawn_blocking` for blocking work, tokio for network work. `f` gets
  `JobCtx { progress: ProgressSink, cancel: CancelToken }`.
- **Reporter.** One tokio task per tc-app job reads atomic counters. It sends nothing before
  300 ms, then at most one report every 100 ms while a count moves. Counts never fall within a
  phase. The last report comes before `Done`. A job that ends sooner sends only its outcome. *(RT-29)*
- **The open** forwards tc-store's reports with no timer of its own
  (`app_forwards_open_reports_unchanged`).
- **Cancel.** `jobs/cancel` sets the token. Jobs poll it between files of an install, between
  books of a plan, per chunk of a download (`crates/tc-door43/src/client.rs:651` already re-reads
  the internet switch per chunk), and in git2's transfer callbacks (STORAGE:2725, 2861). A
  cancelled job reports `Cancelled` and changes no project data.

| Job | Progress unit | Cancel leaves | Budget |
|---|---|---|---|
| `Open` | files, bytes, paths (tc-store) | not cancellable, as in tC4 | 3 s / 10 s; first report within 300 ms (PERF:89-90) |
| `Create`, `Join`, `Import` | steps, bytes | the staging folder removed | import 2 min (PERF:95) |
| `Sync` | objects, bytes | before integrate: nothing; after: the step finishes and says what arrived (Q8) | 3 s plus network (PERF:93) |
| `ExchangeExport` | bytes, then verify | no file on the stick | 15 s on USB 2 (PERF:94) |
| `Download`, `FixFetch` | per repository, bytes (after B9a streams) | the partial install removed | background |
| `UpgradePlan`, `GatewayPlan` | books | nothing; plans are read-only | 2 s count, 5 s plan (PERF:81-82) |
| `Export` | steps | no file | — |
| `HomeSummary` | files | the card keeps "—" | background |
| `Train` | verses | the old model stays | background |
| `Seed` (first launch) | files | resumes at the next launch | 60 s (PERF:99) |

- **Saves keep flowing** during every job (I9, STORAGE:2976-2984). tc-app holds no lock across a
  job.
- **After a restart** import, seed and install resume from their own temporary state; training,
  plans and summaries are computed again; nothing syncs at launch or while idle (DELTA:737).

### 4.7 Services behind the screens

| Service | Uses | Notes |
|---|---|---|
| `editor` | `FoldState::projected`, `key_state`, `token`; `chapter_usfm` (`storage:…/fold/project.rs:494`); the tc-usfm codec | builds a chapter from that chapter's keys (R1 or `ProjectIndex`); section values from `model::SectionBuffer`; empty words become `___` as in tC4 (`tC4:src/state.jsx:7183-7186`) |
| `story` | `tc_obs` `present_story`, `resolve_image`, the frame rules | text rules checked before the command; the store checks again |
| `helps` | `tc_helps::Helps`, `highlight_quote`, tc-helps' block renderer | titles are gateway renderings, as `glTitleFor` |
| `check` | `tc_check` after unit C: derive, `session::open` over the fold, `rail`, `status`, `selection`; `tc_resources::resolve::preflight_tool_book` | `decide` returns the new row and counts; a verse delta re-assesses only that verse's checks |
| `align` | `tc_align::Aligner`, `suggest::Model` | one edit at a time per aligner (a `seq` check) *(RT-14)*; the session keeps `shown` and `verse_shown` (STORAGE:1500-1505) |
| `conflicts` | `FoldState::conflicts`, `conflicts_of`, `conflict_counts`, `history`, the choices in `fold/resolve.rs` | review tokens from `mint_review`; proposals refused with `LookAgain` when heads moved |
| `resources` | `Door43Client`, tc-resources installs and updates, `tc_check::carry::plan` | installs on blocking threads, because they fsync inside async functions |
| `settings` | `SetSetting`, `SetMeta` | two keys are two batches until R18 |
| `account` | `InternetSwitch`, `mint_token` (`crates/tc-door43/src/client.rs:375`), `session_for_token` (`:414`), `list_orgs` (`:420`) | `Keychain` trait: `OsKeychain` (`keyring`, Q-3) and `MemKeychain`; "no keychain on this computer" kept |
| `door43`, `exchange`, `import` | `SyncPort`, `ExchangePort`, `ImportPort` | map reports to view types and refusals to §3.5 |
| `export` | tc-export | warns while conflicts are open (STORAGE:2696-2697); checkpoints first from D1 |
| `feedback` | `ReportLog` | port of `tC4:src/data/feedback.ts:7-31` |

### 4.8 Plans and proposals

Upgrades, gateway changes and conflict choices that build values follow one pattern. The server
computes the plan or proposal on a snapshot and keeps it with the heads it read. The dialog shows
the consequences. Confirming sends only the id. The actor checks the heads (STORAGE:1603-1606)
or refuses with `PlanStale` or `LookAgain`, and the UI shows the dialog again, as tC4 does
(`tC4:src/state.jsx:4800-4806`). Journal `Record` values never enter tc-api. *(RT-27)*

### 4.9 App data

All files are written by one `PrefsWriter` task with `tc_store::atomic_write`. It debounces as
tC4 does (place 500 ms, `lastEdit` 1 s; `tC4:src/state.jsx:3789-3799, 3821-3845`) and keeps
unknown keys. A failed write keeps the value for the session and adds a report. Before S1 these
files live in memory.

| File | Content |
|---|---|
| `prefs/app.json` (not in STORAGE §3.7, STORAGE:1093-1103) | `schemaVersion`, `appLocale`, `lastUsed`, `lastEdit`, `alignSuggestions` |
| `prefs/<projectId>.json` | place per book or story, `draftUnit`, OBS story and recency |
| `summaries/<folder hash>.json` | §4.5 |
| `reports.json` | the last 50 operation reports, for Feedback and the dev Inspector |
| `uploads/`, `downloads/` | temporary; emptied at start and after use |
| `app.lock` | single-instance lock of `tc4` |

`lastEdit` (the Resume card) is recorded on each acknowledged save and on each `Draft`, so it
follows typed text within one second, as tC4's follows typing (`tC4:src/state.jsx:3847-3858`). *(RT-20)*

---

## 5. tc-server

### 5.1 Start, address and one instance

- `tc4 serve --app-data <dir> --projects <dir> --ui <dir> [--door43 <url>]`
  `[--backend memory --scenario <dir>] [--test-hooks]`. The last three exist only with their
  cargo features.
- It takes `app.lock` in app data and exits with a message if another `tc4` holds it. *(RT-3)* The shell
  also holds Electronite's single-instance lock, as tC4 does (`tC4:scripts/desktop-main.cjs:108-111`).
- It binds `127.0.0.1:0` and prints one JSON line before anything else:
  `{"url":"http://127.0.0.1:PORT/","token":"<43 chars base64url>"}`. The token is never logged.
- On stdin `{"quit":true}` it checkpoints every session within 5 s, closes the actors, answers
  `{"closed":true}` and exits. *(RT-3, RT-4)*

### 5.2 Security

A tower layer checks, in order:

1. `Host` equals `127.0.0.1:PORT`, else 403 (DNS rebinding, ARCH:45-46).
2. `X-Tc-Token` matches in constant time, else 401. This applies to every route, static files
   included (ARCH:42-44). The shell opens the window at `url#t=<token>`.
3. On any method other than GET, `Origin` equals the server's own origin, else 403.
4. On `/api/`, `X-Tc-Api` equals 1, else 409.
5. No CORS header is ever sent.

How every request carries the token:

| Client | Mechanism |
|---|---|
| The UI's `fetch` calls | the UI reads the token from the URL fragment, keeps it in `sessionStorage`, and sends the header |
| Shell window: `index.html`, scripts, CSS, fonts, `<img>`, help `<iframe>`, `<a download>`, the hidden print window | the Electronite main script adds the header to every request to the server's origin with `session.webRequest.onBeforeSendHeaders` |
| Playwright | `extraHTTPHeaders` on the context |
| A developer's own browser | debug builds only: `/?dev-token=<token>` once sets an `HttpOnly; SameSite=Strict` cookie, which step 2 then accepts, and redirects to `/#t=<token>` so the UI's own calls send the header |

If unit G1 finds a request type that `onBeforeSendHeaders` misses in Electronite v37, `/media`,
`/download` and `/print` switch to capability URLs (an HMAC of kind, project and path under a
per-launch key). The change is confined to tc-server and one UI helper.

Headers on `index.html`: `Content-Security-Policy: default-src 'self'; script-src 'self'
'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src
'self'; frame-src 'self'; object-src 'none'; base-uri 'none'` (inline styles carry tC4's DOM),
`Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`.

Tests (`tc-server/tests/security.rs`): `request_without_token_is_401`, `foreign_host_is_403`,
`foreign_origin_post_is_403`, `no_cors_headers_ever`, `binds_loopback_only`,
`dev_cookie_only_in_debug_builds`, `help_never_redirects`.

### 5.3 Routes

| Method, path | Kind | Notes |
|---|---|---|
| `GET /` | static | `index.html`, `no-store`, with `boot.js` |
| `GET /ui/*` | static | hashed wasm, JS glue, CSS (tC4's `styles.css`, tokens, `ui.css`), fonts, catalogs; immutable |
| `GET /help/*` | static | vendored help pages as `text/html`, never a redirect (Q28, DELTA:928-944), with the page-swap shim |
| `GET /prepare.html` | static | first-launch window, strings in 10 languages (Q25) |
| `GET /api/v1/events` | stream | §5.4 |
| `POST /api/v1/{service}/{name}` | typed | §3.9 |
| `POST /api/v1/uploads` | upload | §5.5 |
| `GET /media/obs/{pack}/{path}` | media | OBS pictures for `<img>` |
| `GET /download/{id}/{name}` | media | a finished export, `Content-Disposition: attachment` |
| `GET /print/{job}` | media | print HTML for the shell's `printToPDF` and for `page.pdf()` |

`every_endpoint_is_routed_once` checks the router serves exactly `tc_api::ALL`.

### 5.4 The event channel

- `GET /api/v1/events?after=<seq>` answers `text/event-stream`: `id: <seq>`, `data: <json>`, and
  a comment line every 15 s.
- The server keeps a ring of 4,096 frames. Progress frames are coalesced: the newest per job
  replaces older ones. A reconnect with a sequence the ring no longer holds gets `Resync`, and
  the UI reads every live query again. Every query is idempotent.
- Each connection has a bounded queue. A reader too slow for it is disconnected and reconnects.
- The UI reads the stream with `fetch` and a `ReadableStream` reader, and parses the frames in
  `tc_ui::app::events`, a host-tested parser.
- **Ordering.** The server publishes a command's events before it sends the command's reply. An
  `Ack` and a `Changed` event for the same write may still arrive in either order at the UI; both
  carry a `ViewSeq`, and the UI keeps the newer. `events_reply_order_is_safe` *(RT-6)* is a property test
  over random orders.

### 5.5 Uploads, downloads, pictures and PDF

- **Uploads.** The DOM keeps tC4's `<input type=file>` and drop zones
  (`tC4:src/views/modals/Import.jsx:63-68`, `AddBook.jsx:57-62`, `GuidedFix.jsx:62-68`). The UI
  posts each `File` as the `fetch` body, with its name percent-encoded in `X-Tc-Upload-Name`. The
  server streams it to `uploads/<id>` and returns `UploadView { id, name, size }`. Caps: 512 MiB
  per file and 4 GiB per flow, STORAGE's exchange caps (STORAGE:3027). *(RT-28)* Parsing happens on the
  server.
- **Receive work from a file.** In the shell, `tc4Shell.chooseFile()` gives a path and the server
  reads the file in place. In a browser, an upload.
- **Downloads.** An `Export` job ends with `ExportReady { download, file_name }`. The UI clicks an
  `<a download href="/download/<id>/<name>">`, as tC4's `deliverFile` does
  (`tC4:src/data/export/kernel.ts:38-57`). Electronite shows the save dialog and reports saved or
  cancelled through `onDownloadDone`, which drives "Saved {file}" (`tC4:src/views/ExportMenu.jsx:26-34`).
  In a browser the toast says "Export ready: {file}". Files stay 10 minutes.
- **USB export** is not a download. `tc4Shell.chooseFolder()` gives the destination; the job
  writes, then verifies from the device; "You can remove the stick" shows only after the verify
  (STORAGE:3010-3017).
- **PDF.** `export/run { producer: Pdf }` makes print HTML with OBS pictures inlined as `data:`
  URLs (`tC4:src/data/export/pdf.ts:76-87`) and returns `/print/<job>`. The UI calls
  `tc4Shell.printPdf(url)`. The menu item exists only with the bridge, as in tC4
  (`tC4:src/data/export/pdf.ts:120`). Tests use `page.pdf()` on the same URL (ARCH:108-112).

### 5.6 The internet switch and Door43 sign-in

1. The server's `InternetSwitch` is the only state. It is off at every launch and never stored.
   `hello` and `Internet` events mirror it into the account menu's dot (`tC4:src/views/AccountMenu.jsx:67-68`).
2. Before a request whose endpoint has `NET`, the UI opens M15 with `net.ask.<kind>` if the switch
   is off. "Not now" sends nothing. "Turn on internet" calls `net/set {on:true}` and then the
   request. A failed `net/set` shows "tC4 could not use the internet". Two quick presses share one
   question (`tC4:src/state.jsx:4249-4282`).
3. The server refuses with `NeedsInternet` if the switch went off meanwhile; the UI asks once
   more. A job that loses the internet while it runs ends with its own refusal, for example
   `SyncRefusal::Offline`.
4. Turning the switch off stops the very next request: tc-door43 checks it before every request
   and every download chunk (`crates/tc-door43/src/client.rs:597, 651`). tC4 only blocked in the
   client (`tC4:src/state.jsx:4296-4299`).
5. Sign-in sends login and password once, in the body of `account/signIn`, over the local API.
   tc-app mints a Door43 token, keeps it in memory, and with "Stay signed in" in the keychain. The
   token never reaches the UI; `AccountView { user, kept }` does.
6. `account/state` reads only whether a token is kept. The first Door43 step resumes it. A 401
   forgets it and answers `NeedsSignIn`. `AuthFailed` during a sync forgets both copies
   (DELTA:735-736). A sign-in result that arrives after Cancel is dropped by the flow's generation
   (`tC4:src/state.jsx:5876-5881`).
7. Private repositories (Q3, PLAN:51-54): `door43/open` fetches without credentials first; on 401
   or 404 with no session the job ends with `NeedsSignIn(PrivateRepository)`, the UI shows M12 and
   runs the open again.
8. Links to a profile, a repository or help ask the question first, then `account/link` gives the
   URL and the shell opens it with `openExternal` (`window.open` in a browser).
9. The Door43 server is `--door43`: QA in dev builds, production otherwise
   (`tC4:src/data/dcsServer.ts:65-84`), and the fake's URL in every test.

### 5.7 The shell bridge (`window.tc4Shell`)

`printPdf(url)`, `chooseFolder()`, `chooseFile()`, `openExternal(url)`, `setCanClose(bool)`,
`onCloseRequested(fn)`, `onDownloadDone(fn)`, `feedbackSend(report)` (if Q-2 picks the shell),
`platform`. Without the bridge (dev, Playwright) the features degrade as tC4's do in a browser: no
PDF item, and Feedback says it was not sent (`tC4:src/state.jsx:4143-4167`). Playwright injects a
stub with `addInitScript` where a journey needs a folder or a file path. *(RT-33)*

### 5.8 Test hooks (feature `test-hooks`)

| Hook | Use |
|---|---|
| `test/quiet` | `{busy}`: true while a request, job, actor queue or pending materialize is live; the capture's settle waits for false |
| `test/hold`, `test/release` | hold a book read, a checkpoint, a materialize, a save before B3, or a sync step (R8); replaces tC4's `e2e/helpers/checkpointHold.ts:14` |
| `test/clock` | a fixed clock for dates in screenshots |
| `test/script` | queue scripted save or job outcomes (memory backend and port doubles) |
| `test/door43` | configure tc-door43-fake faults, including R13's dropped connection |
| `test/inspect` | fold facts journeys assert: heads, values, conflicts, commits, `.tc/` files |
| `test/writeTeammate` | teammate changes through tc-journal and the store's sealing API (AD36) |
| `test/counters` | operation counters |
| `test/reset` | reset prefs, summaries and reports to the scenario seed; the harness's `home()` |

---

## 6. The UI

### 6.1 Data flow

- `use_app()` returns `App { s, actions, locale }`, provided once at the root. Field and action
  names are tC4's in snake case (`s.projects`, `s.book_model`, `actions.go`,
  `actions.open_project_at`, `actions.record_decision`), so each tC4 view file becomes one Rust
  module read top to bottom.
- Behind `s` there are three sources, and each value has one owner:
  1. **Server state** in a `QueryCache`: projects, cards, the book model, helps, the check session,
     the aligner view, jobs, internet, account, prefs. These fields of `s` are read-only signals.
  2. **Drafts**: typed text not yet acknowledged, in the save machine and the browser buffer.
  3. **View state**, the "memory only" slices of tC4 (`tC4:src/state.jsx:408-588`): view and
     dialogs, rail and helps visibility and width, tabs, filter and sort, `toolPos`, page setup,
     the open editor card, selections that are not the aligner's.
- Fields that were tC4 plumbing are not ported: sequence counters, per-scheduler save mirrors and
  store refs are replaced by epochs, `ViewSeq` and the save machine.
- Every async completion updates its signal with a keyed `update` closure, never by setting a
  value captured earlier. This is tC4's reducer rule (`tC4:src/state.jsx:767-853`).
- Each `Action` does tC4's view-state work, drains saves where tC4 drained its schedulers, and
  calls one endpoint. Navigation is flush-and-go (§6.5).

### 6.2 Reading

- `api.query::<E>(req).await -> Reply<E>`. A screen holds `Load<T> { Loading, Ready(T),
  Refreshing(T), Failed(ApiError) }`. "Failed" is never drawn as "absent".
- A reply is dropped when its scope is no longer current, or when its `ViewSeq` is older than the
  cached one.
- A `Session::Changed` event whose scope a query `depends_on` moves it to `Refreshing` and fetches
  it again. The old data stays on screen; a book waiting behind a commit shows as pending, never
  as empty (DELTA:625-626). `Resync` refreshes every query.
- Shared reads are shared: Understand and Translate use one helps query, as tC4 shares
  `useLoadHelps` (`tC4:src/views/HelpsPanel.jsx:474-480`).
- `window.__tcIdle` is true when no fetch, save or job is in flight.

### 6.3 Subscribing

One task reads the stream and dispatches:

- `Changed { scope }` re-queries only the chapters, items or cards in scope. A field with unsaved
  text keeps its buffer and its token. If the new value's token differs, the field shows
  "Juan changed this verse while you were typing. Your text will be kept as a second version."
  (STORAGE:2978-2981), naming the author from `ChangeScope.records`. The later save becomes a
  conflict, never a revert. *(RT-6)*
- A `Changed` event with `scope.conflicts`, and `new_conflicts` in an `Ack`, update the markers
  and chips at once.
- `Checkpoint(Failed)` sets "Checkpoint not saved" with Retry (`tC4:src/App.jsx:63-67`).
- `Job` events feed the open overlay and the busy labels. The overlay (S14) appears only when the
  first `Open` progress arrives; tc-store sends none before 300 ms, which matches tC4's
  `SHOW_THRESHOLD_MS` (`tC4:src/views/OpenProgress.jsx:19`). *(RT-22)*
- `HomeCard` makes a mounted card read again.

### 6.4 Shown tokens in the UI

- A token is stored next to the value it stands for, inside `Shown<T>`. The UI never builds,
  parses or compares one beyond equality.
- A save sends the token of the value the editor started from, never one fetched later. *(RT-6)*
- An `Ack` replaces tokens per record.
- Tokens never enter the DOM. `no_token_in_dom` scans every journey checkpoint for base64url runs
  of 40 or more characters. *(RT-7)*
- `WriteCap` is a zero-sized value that exists only while the session mode is `Writable`.
  Editors take `Option<WriteCap>`. A read-only open or a `SecondWriter` switch removes it: the
  edit controls turn off and the drafts stay. *(RT-10)*

### 6.5 Saving

**The save machine.** `tc_ui::logic::save_machine` is a host-tested port of
`tC4:src/data/saveScheduler.ts`. Per field it holds `current`, `persisted`, the base tokens, and
a state:

```rust
pub enum FieldSave { Clean, Dirty { since: f64 },
                     Saving { sent: String, request: RequestId },
                     Failed(NotSaved), SavedWithWarning(String) }
```

- One save in flight per field. The next save of a field waits for the reply and uses its tokens.
- On a reply: if the text still equals `sent`, the field is clean and its buffer entry is removed;
  otherwise it stays dirty on the new tokens (tC4's version-aware rule, `saveScheduler.ts:214-219`).
- After a failure nothing saves automatically; only Retry continues (`saveScheduler.ts:314`).
- A lost reply (a dropped connection, or a UI reload with a save in flight) is retried with the
  same `RequestId` and text, which the server's reply cache answers. *(RT-2, RT-3)* After a server restart the
  field is read again: a value equal to `sent` means the save landed; anything else is resent
  with the base token (AD4).
- `drain()` resolves false while a failure stands (`saveScheduler.ts:139-146`).
- Tests port tC4's scheduler tests, plus a proptest: no `current` is dropped while
  `persisted != current`.

**Timing** (AD20):

| Field | Saves |
|---|---|
| verse, layout, section | on blur, or 2 s after the last input *(RT-1)* |
| user comment | on blur, or 2 s *(RT-18)* |
| OBS frame, title, reference | on blur, or 2 s |
| alignment | on leaving the verse, or 2 s after the last link change *(RT-4)* |
| decision (status, selections, nothing-to-select, bookmark, check comment) | at once, on each act *(RT-10)* |

**The indicator** (X2) shows the worst state of all fields: error, then saving, then dirty, then
saved (`tC4:src/App.jsx:57-62`). "Checkpoint not saved" shows only when no save failed. A disk
warning shows as a notice.

**The navigation gate.** Changing mode, book, story or project, exporting, printing and Keep this
version first drain every field. A standing failure keeps the person where they are (FR-32,
`tC4:src/state.jsx:4018-4024`). *(RT-26)*

**The editing lock.** `Editing` is one enum: `None | Verse(VerseKey) | Section(SectionKey) |
Frame(FrameKey)`. Anything but `None` disables the exits DELTA:909-927 lists, in Bible Translate
and, by ruling Q21, in OBS Translate. Leaving a project clears it.

**The close guard.** The UI calls `tc4Shell.setCanClose(all_clean)` on every change, or registers
`beforeunload` in a browser (`tC4:src/state.jsx:3684-3721`). *(RT-4)*

### 6.6 The draft buffer

- `localStorage["tc4.drafts.v1"]` maps `"<projectRef>|<fieldKey>"` to
  `{api, base: [tokens], text, request, at}`. It is written synchronously on every `input`.
  Every access is wrapped in a `Result`. *(RT-2, RT-15)*
- While a field is dirty, a `Draft` command goes to the server at most once a second; the actor
  writes `.tc/drafts.json` (STORAGE:1752-1759). *(RT-20)*
- A section draft holds the whole buffer and every displayed token (STORAGE:1486-1488, 1754-1756).
  Until R6, tc-app sends `text = {"tokens":[…],"buffer":"…"}` under the first verse's token.
- A user comment has no server draft, because `DraftField` has no comment variant
  (`storage:crates/tc-journal/src/command.rs:207-215`). Until R15 its text is safe across a reload
  but not across a crash of the whole app.
- **After a reload in the same launch:** `app/hello` returns the open sessions; the UI restores the
  view from a `sessionStorage` navigation snapshot; buffered entries return as dirty fields;
  the open editor card opens again. No question is asked: the screen and its tokens are the same. *(RT-6, RT-15)*
- **At the next launch:** the open offers drafts from `drafts.json`: "Unsaved text from your last
  session was found for Titus 1:4. Keep it / Discard." (STORAGE:1757-1759). Keep saves with the
  draft's token, so a verse changed since becomes a conflict. *(RT-9)* Where the browser buffer also holds
  an entry for that field (only with a stable origin, Q-1), the browser entry is never older and
  wins.

### 6.7 Flows and dialogs

```rust
pub enum Pending {                       // the step to run again after the question or the sign-in
    Sync(ProjectRef), Share(ShareStep), OpenDoor43(OpenDoor43Form), Sources(SourcesStep),
    Fix(Tool), ImportVersions(ReviewId), Feedback, Link(LinkKind, String),
}
pub enum InternetFlow { Idle, Asking { kind: NetKind, then: Pending }, TurningOn { then: Pending }, Failed }
pub enum SignInFlow { Idle, Open { flow: u64, then: Option<Pending>, error: Option<Refusal> }, Busy { flow: u64 } }
pub struct Layers {                       // z-order is tC4's mount order (tC4:src/App.jsx:138-154)
    pub dialog: Option<Dialog>, pub gateway: Option<GatewayPreview>, pub upgrade: Option<UpgradePreview>,
    pub internet: InternetFlow, pub opening: Option<JobId>, pub drafts_offer: Option<OfferedDrafts>,
}
```

Only one question is open at a time; a step that asks while one is open does nothing, as tC4's
`pendingAsk` rule says (`tC4:src/state.jsx:4258-4263`). `View` is `Home | Read | Draft | Check |
Publish` with tC4's string values; `MainView` is a port of `tC4:src/App.jsx:192-209`.

### 6.8 Panics and view errors

- No `unwrap`, `expect` or `panic` in tc-ui (workspace lints, `Cargo.toml:15-20`). Errors render
  tC4's boundary DOM: `data-testid="view-error"`, "This screen could not be shown. {message}",
  "Back to Home" (`tC4:src/App.jsx:163-190`).
- A panic anyway: the hook runs before the abort. It stores `{view, project, message, at}` in
  `sessionStorage["tc4.crash.v1"]`, sends `app/clientError` with `keepalive`, and schedules
  `location.reload()`.
- After the reload the app reattaches to its session (AD22) and shows the view-error callout in
  place, with the top bar.
- Three panics within 10 s stop the reloads. *(RT-15)* `boot.js` then draws the same callout as static DOM;
  "Back to Home" reloads to Home.

### 6.9 Language and right-to-left

- `t(key, vars)`, `t_or` port `tC4:src/i18n/index.js:76-95`: English fallback per key; an unknown
  key warns once and shows the key; an explicit fallback wins; `{name}` is replaced.
- `build.rs` generates key constants from `assets/i18n/en.json` and
  `assets/i18n-rebuild/en.json`, so a misspelled static key does not compile. The port of
  `test/i18n-keys.test.ts`, with floors, covers dynamic keys such as `net.ask.<kind>` and
  `syncResult.error.*` (DELTA:525-531).
- English is built in. The active catalog is fetched before the first render; the App language
  preview (M16) fetches one catalog on demand and re-renders without remounting. *(RT-23)*
- Rebuild-only keys live in `assets/i18n-rebuild/<id>.json`, merged after the vendored catalog
  without overriding it. The vendored files stay byte-identical (DELTA:532-536).
- `<html lang dir>` comes from `LOCALES` (`crates/tc-ui/src/i18n.rs:41-92`) and is set before the
  first paint from `hello.prefs.appLocale`. `lang` matters as well as `dir`:
  `html:lang(ar) ::placeholder` switches the face (`crates/tc-ui/assets/ds/tokens/typography.css:166`).
- Three direction axes, each carried by its view type: the interface from the app language;
  scripture from `ProjectFacts.dir`; each help card from its gateway package
  (`tC4:src/views/scriptStyle.js:24-38`). Hebrew source is RTL with `lang=hbo`
  (`tC4:src/views/Draft.jsx:119-120`).
- The marking-row arrows turn under an RTL interface (`tC4:src/views/ItemNav.jsx:11-13`). Window
  controls stay on the physical side, which needs tC4's `src/ui.css`, vendored again.

### 6.10 Keyboard

tC4 has no global shortcuts; the only global key is Alt+Shift+I for the dev Inspector
(`tC4:src/views/dev/Inspector.jsx:53-58`), dev builds only. Each screen ports its own contract:

| Contract | Where | Proof |
|---|---|---|
| Escape closes the innermost layer; modals trap focus | DS `Layer` (ported) | DS scenarios |
| Account menu: first row focused, arrows, Home, End, Tab closes, Escape returns focus | X6 | `account-menu` scenario |
| Enter and Space on Understand units, story units, the Resume card, picker cards | Home, Understand, Check picker, OBS Understand | screen scenarios |
| Check rail is one Tab stop with roving focus; ↑ ↓ Home End | S6 | `check-keyboard` |
| Target words: ← → in visual order, Shift+arrows, Space, Enter; drag-select | S6 | `check-keyboard`, J4 drag (DELTA:122, 889-905) |
| Section editor: Tab and Shift+Tab indent; Place mode Enter, Space, Escape | S3 | J2 |
| Align bank: Ctrl/Cmd/Shift + click, Enter, Space | S7 | J5 |
| Focus returns to the card that opened a session | S5 | `check-keyboard` |
| Marking row geometry within 1 px, en and ar | S6, S7 | `item-navigation` (DELTA:1118) |

### 6.11 Lists

Lists of 400 rows or fewer render whole, so the Titus rails keep tC4's exact DOM. Longer lists
are virtualized with fixed row heights: the Psalms tN rail has 8,212 rows (PERF:3-5, 97). Rows use
`keyed_list` (`crates/tc-ui/src/ds/mod.rs:232`), which keeps focus across moves. The threshold is
a constant with a test (Q-12). *(RT-24)*

### 6.12 Screen port map

| tC4 file | tc-ui module | Unit |
|---|---|---|
| `src/App.jsx` | `app/shell.rs` (TopBar, SaveIndicator, MainView, ViewBoundary) | A5 |
| `views/AccountMenu.jsx` (menu, help, language, about, internet) | `views/account_menu.rs`, `help_panel.rs` | A5, A8 |
| `views/Home.jsx`, `SyncResult.jsx` | `views/home.rs`, `sync_result.rs` | B2, B10 |
| `views/OpenProgress.jsx` | `views/open_progress.rs` | B3 |
| `views/Understand.jsx`, `SourceVerse.jsx`, `HelpsPanel.jsx`, `BookRail.jsx`, `railChrome.jsx`, `PanelIcons.jsx` | same names in snake case | B3 |
| `views/Draft.jsx`, `SectionEditor.jsx`, `draftChrome.jsx`, `conflictMarkers.jsx` | same names | B4 |
| `views/Check.jsx`, `ItemNav.jsx` | `views/check/{picker,session,rail,detail,marks,drawer}.rs`, `item_nav.rs` | B5a, B5b |
| `views/Align.jsx` | `views/align.rs` | B6 |
| `views/StoryUnderstand.jsx`, `StoryDraft.jsx`, `StoryRail.jsx` | `views/story_*.rs` | B7 |
| `views/CommunityChecking.jsx`, `ExportMenu.jsx`, `print/*` | `views/community_checking.rs`, `export_menu.rs`, `print/*` | B8, D2 |
| `views/ConflictReview.jsx` | `views/review/*.rs` (tabs for every kind) | B11, C7 |
| `views/modals/*.jsx` (18 files) | `views/modals/*.rs` | B2, B9a, B9b, C6, C8, F1 |
| `views/dev/Inspector.jsx` | `views/dev/inspector.rs` (dev only) | late |

Rebuild-only screens with no tC4 view: the review tabs beyond Text, History, Detached, Notices,
the received-work sheet, the draft offer, the read-only banner with the copy offer, the USB flows
and the first-launch window (Q-9).

---

## 7. Parity testing

### 7.1 One harness, two environments

`parity/tc4-baseline/capture.mjs` runs `await main()` when imported (`capture.mjs:1110`), so it
is split as `parity/ds-gallery` was:

| Module | Content |
|---|---|
| `parity/app/screens.mjs` | `SCREEN_LIST` and each screen's `run`, unchanged |
| `parity/app/driver.mjs` | `appDriver` over an environment |
| `parity/app/observe.mjs` | `settle`, `observe`, `domSnapshot` rooted at `#root` |
| `parity/app/env-tc4.mjs` | today's mock, `.state` and in-browser fake Door43 |
| `parity/app/env-rust.mjs` | launches `tc4 serve`; `extraHTTPHeaders` with the token |
| `parity/tc4-baseline/capture.mjs` | a short entry over `env-tc4` |

**Proof the split changed nothing.** The baseline test checks sizes and byte counts only
(`parity/tc4-baseline/baseline.test.mjs:45-59`). Unit A6 adds a sha256 per PNG to `manifest.json`
and a check for it, then recaptures tC4 and requires identical hashes.

The Rust environment's seams:

| Seam | Rust environment |
|---|---|
| `home()` | `test/reset` with the scenario's prefs and locale, `localStorage.clear()`, load `/#t=<token>`, wait for both cards and `<html lang>` |
| `card(repo)` | maps `BIBLE` and `OBS` paths to `project-<folder>` test ids |
| `trackRequests` | counts `/api/v1/` and `/media/`, never `/api/v1/events`; otherwise `settle` never sees 800 ms of quiet and silently waits out 30 s (`capture.mjs:136-155`) |
| `settle` | also waits for `test/quiet` to answer `{busy:false}`; a screen that hits the deadline fails |
| `routeFakeDoor43` | removed: `tc4 serve --door43 <url>` points at a tc-door43-fake process seeded with `facilitator-zq` and the organisations of `fake-door43.mjs:16-21`; the guard that refuses other origins stays |
| `pushToDoor43`, `teammatesChangeOneVerse`, `loseFromDoor43`, `dropNextClone` | phase 1: scenario files for the scripted doubles plus teammate heads in the fold; from E1: real packs written by tc-journal and tc-store into the fake's bare repositories, and R13's dropped connection |

### 7.2 The same state on both sides

- `parity/app/state.mjs` runs `build-state.mjs` against a tC4 root assembled from the repository's
  own copies: `test/fixtures` from `fixtures/tc4/test-fixtures`, `conformance/sample-burrito*`
  from `fixtures/tc4/sample-burrito*`. `build-state.mjs` reads only those and the vrs files
  (`parity/tc4-baseline/fixtures/build-state.mjs:41, 91, 214-215, 405`). The test
  `state_from_repo_equals_state_from_tc4` compares the two trees byte for byte. CI then needs no
  tC4 checkout.
- `MemoryBackend` reads the same `.state/<scenario>`. Both apps start from the same bytes,
  substitutes and absences included.
- Bundle seeding is off during captures (Q-11).

### 7.3 Four layers

1. **Pixels.** The baseline's capture rules: Playwright 1.56.1, Chromium 141 build 1194, the five
   flags, the same context and viewports, the pointer at (1000, 28), a viewport screenshot with
   animations disabled (`capture.mjs:37, 50, 987, 1016-1019`). *(RT-34)* `pixelmatch` with threshold 0,
   anti-aliasing counted (`parity/ds-gallery/diff.mjs:47`). Declared masks, each a rectangle in
   `parity/app/masks.json` with the ruling it cites: the build line in `about`, the build and
   platform lines in `feedback`, and the Q2 and Q3 text on the eight `rebuildDiffers` screens
   (`parity/tc4-baseline/README.md:380-402`).
2. **DOM.** `domSnapshot` (`parity/ds-gallery/protocol.mjs:174-220`) rooted at `#root`. tC4's
   snapshot per screen is recorded once and committed as `parity/tc4-baseline/dom/<screen>.json`.
   Normalizations, each listed and tested: project test ids through the scenario map; generated
   ids `tc-[fg]\d+` renumbered by first appearance; hashed asset names; picture `src` by file name;
   comment nodes skipped (already).
3. **Meaning.** `observe`'s `homeCards` and `syncToast` (`capture.mjs:914-960`) field by field.
   Declared difference: under Q2 the OBS card's action is `sync`, not `upload`.
4. **Behaviour.** `scenarios.mjs`-style sequences on full screens: DOM, focus, scroll lock and node
   identity after each step (`parity/ds-gallery/scenarios.mjs:137-151`).

### 7.4 The tC4 view-state gallery

Many states have no baseline. A Vite app mounts tC4's real views from the checkout with
`src/state.jsx` replaced by a stub through an alias, as `parity/ds-gallery/vite.config.mjs:15-19`
already aliases tC4's design system. The 34 view files under `src/views` and `src/App.jsx` import
from `state.jsx`; the stub exports what they use: `useApp`, `SCRIPT_FONTS`, `SUITE_VERSION`,
`door43` and `isOldTestament`. `actions` records each call, so tC4's action trace can be compared
with the Rust screen's requests and navigation.

A case has three parts: a tC4 `s` fragment, the matching tc-api replies, and steps. The replies are
recorded from `MemoryBackend` where the state exists there, and written by hand only where it
cannot exist yet. Cases cover: S4 Text tab; S12; S14 with its three captions; M9; M10; M18; the
License dialog; net failed; share progress and done; import steps after `kind`; Add a book pick and
USFM steps; Feedback sent and not sent; the sign-in error; the check empty states; the align
unavailable states; Home loading, empty and error; the Resume card; the import toast.

### 7.5 Report and gate

`parity/app/report.json` holds, per screen and gallery case: pixel count, diff image, DOM diff,
meaning diff, masks used, the backend phase (`memory`, `memory+scripted`, `store`), `pending_until`
(the unit that must recapture a scripted screen), and a status (`same`, `declared`, or `differs`
with a reason). `crates/tc-ui/tests/screen_record.rs` fails when:

- a baseline screen or gallery case has no row;
- a row is `differs` without an entry in `parity/app/residuals.json`;
- a row's PNG hash disagrees with the committed capture;
- a row is still `memory+scripted` after the unit named in `pending_until` has landed.

This follows `crates/tc-ui/tests/gallery_record.rs` and meets P7: "a human reviews the
differences, and the differences are listed" (PLAN:112-115).

### 7.6 The 62 baseline screens

"M" is `MemoryBackend` with real fixture data. "M+S" is a scripted job outcome. "Recapture" is
the unit that captures the screen again on its final backend.

| Screen | Scenario | First capture | Recapture | Notes |
|---|---|---|---|---|
| `home` | sample | B2, M | C1 | project test-id map |
| `add-project` | sample | B2, M | — | four option cards |
| `new-bible` | sample | B2, M | — | language suggestions from the server |
| `new-obs` | sample | B2, M | — | — |
| `import` | sample | B2, M | — | kind step |
| `add-book` | sample | B2, M | — | method step |
| `account-menu` | sample | A5 DOM; B2 pixels | — | — |
| `app-language` | sample | A5 DOM; B2 pixels | — | — |
| `help` | sample | A5 DOM; B2 pixels | — | vendored help pages |
| `about` | sample | A5 DOM; B2 pixels | — | mask: build line |
| `feedback` | sample | A5 DOM; B2 pixels | — | mask: attachment lines |
| `understand-tit1` | sample | B3, M | C1 | — |
| `en-understand-tit1` | en-pins | B3, M | C1 | — |
| `en-understand-tw-article` | en-pins | B3, M | — | article pane |
| `en-understand-questions` | en-pins | B3, M | — | incomplete: no en_tq in the state |
| `translate-tit1` | sample | B4, M | C2 | — |
| `en-translate-tit1` | en-pins | B4, M | C2 | — |
| `en-translate-section-editor` | en-pins | B4, M | C2 | editing lock |
| `en-translate-verse-editor` | en-pins | B4, M | C2 | the textarea holds STORAGE `words` (Q-20) |
| `check-picker` | sample | B5a, M | C4 | incomplete in the baseline |
| `en-check-picker` | en-pins | B5a, M | C4 | — |
| `guided-fix` | sample | B5a, M | C8 | — |
| `check-tn-session` | sample | B5b, M | C4 | resumes at item 2 |
| `en-check-tw-session` | en-pins | B5b, M | C4 | — |
| `en-check-tw-article` | en-pins | B5b, M | — | Academy drawer |
| `en-check-tn-session` | en-pins | B5b, M | C4 | — |
| `en-check-ta-drawer` | en-pins | B5b, M | — | — |
| `ar-check-tw-session` | en-pins, ar | B5b, M | — | RTL, arrows turned |
| `en-check-tw-session-620` | en-pins | B5b, M | — | marking row wraps |
| `ar-check-tw-session-620` | en-pins, ar | B5b, M | — | — |
| `en-check-tn-session-row` | en-pins | B5b, M | — | scroll steps need equal pane heights |
| `ar-check-tn-session-row` | en-pins, ar | B5b, M | — | — |
| `en-check-tn-session-620` | en-pins | B5b, M | — | — |
| `ar-check-tn-session-620` | en-pins, ar | B5b, M | — | — |
| `align-workspace` | sample | B6, M | C5 | Q15 changes nothing on 1:1 |
| `align-original` | sample | B6, M | — | occurrence numbers |
| `align-workspace-row` | sample | B6, M | — | — |
| `ar-align-workspace-row` | sample, ar | B6, M | — | — |
| `align-workspace-620` | sample | B6, M | — | — |
| `ar-align-workspace-620` | sample, ar | B6, M | — | — |
| `obs-understand` | sample | B7, M | C3 | incomplete in the baseline |
| `obs-translate` | sample | B7, M | C3 | incomplete in the baseline |
| `obs-check-picker` | sample | B7, M | — | incomplete in the baseline |
| `en-obs-understand` | en-pins | B7, M | — | incomplete in the baseline |
| `en-obs-translate` | en-pins | B7, M | — | incomplete in the baseline |
| `en-obs-check-tn-session` | en-pins | B7, M | — | — |
| `en-community-checking` | en-pins | B8, M | D2 | print model in tc-export |
| `project-settings` | sample | B9a, M | C6 | — |
| `source-texts` | sample | B9a, M | C8 | opens the project behind the dialog |
| `share-internet-consent` | sample | A8 DOM; B9b pixels | — | kind `share` |
| `share-signin` | sample | A8 DOM; B9b pixels | — | name notice; fake Door43 |
| `share-dialog` | sample | B9b, M + fake | E1 | "Recommended" needs R12 |
| `share-dialog-check` | sample | B9b, M + fake | E1 | — |
| `open-door43` | sample | B9b, M | — | Q3 text masked |
| `open-door43-internet-consent` | sample | A8 DOM; B9b pixels | — | kind `open` |
| `shared-home` | shared | B10, M | E1 | Q2 mask |
| `sync-internet-consent` | shared | B10, M | — | Q2 mask behind the scrim |
| `open-door43-refused` | sample | B10, M+S | E1 | Q3 text (Q-21); `pending_until: E1` |
| `sync-failed` | shared | B10, M+S | E1 | needs R13; `pending_until: E1` |
| `sync-ok` | shared | B10, M+S | E1 | missing-pins count by tC4's rule (`capture.mjs:110-128`) |
| `sync-ok-teammates` | shared | B10, M+S | E1 | real fold conflict behind the chip |
| `sync-ok-restored` | shared | B10, M+S | E1 | — |

57 screens use real fixture data and 5 use scripted outcomes. Milestone M1 is "62 of 62 screens
in the report", at the end of B10.

### 7.7 Journeys and specs

Each journey is a step list in the `SCREEN_LIST` style with named checkpoints
(`parity/app/journeys/*.mjs`). It runs on the Rust app and asserts the user-visible end state with
tC4's test ids and the on-disk end state in the rebuild's format, through `test/inspect` and a
`tc-parity-assert` helper that validates the Burrito (P1, P5). Where tC4's mock supports the steps
(drafting, checking, aligning, comments, sync with teammates, clone), the same list runs on tC4
and keeps a screenshot and DOM per checkpoint. Where it does not (create, import, catalog,
repository creation and push: `mock-server/server.mjs:406-409, 443-446`, `fake-door43.mjs:6-7`),
tC4's spec assertions are the reference.

| Journey or spec | Units | Backend | Rebuild assertions beyond tC4's |
|---|---|---|---|
| J1 create Bible (`e2e/j01-create-project.spec.ts:16`) | B2 (UI), C6 | store, R2 | valid Burrito; `project.json` |
| J2 draft verse (`e2e/j02-draft-verse.spec.ts:96-1143`) | B4 (UI on scripted saves), C2 | store | one pending batch per save; "Saved" only after B3 (held save) |
| J3 get resources, guided fix | B9a, C8 | store + fake with resources | receipts; pins with sha |
| J4 check book | B5b, C4 | store | decision records; drag-select |
| J5 align verse | B6, C5 | store | one `Alignment` change per save |
| J6 edit invalidation | C2, C4 | store | nothing written on decisions (STORAGE §0 item 4) |
| J7, J23 publish | B8, D2 | store + tc-export | Burrito export carries `DeviceProfile` (DELTA:806-812) |
| J8 resume and checkpoints | C1, D1 | store | one commit when leaving with changes, none without (STORAGE:3820-3821) |
| J9a, J9c, J9d import | F1 | store + I | P9 byte equality |
| J11, J24 share | B9b, E1 | store + X | Upload cases become sync cases on the OBS card (Q2) |
| J12 upgrade, J13 gateway change | C8 | store | one `ApplyUpgrade` batch; look-again when heads moved |
| J15 slow open | B3 (UI with a held open), C1 | store with `FaultFs` delay | first report 300–400 ms (STORAGE:1883-1891) |
| J16, J25 Understand | B3, B7, C3 | store | `Comment` records |
| J17 open from Door43 | B9b, E1 | store + X | private repository through sign-in (Q3) |
| J18 keep a version | B11 (read), C7 | store, second device's pending batch | `Resolve` with review tokens |
| J20 OBS create | C6 | store, R2 | — |
| J21 OBS draft | B7, C3 | store | Q21 editing lock |
| J22 OBS check | B7, C4 | store | — |
| J26 team sync | E1 | store + X, three app-data folders (DELTA:945-969) | plus resolving a check conflict (PLAN P2) |
| USB both directions (rebuild) | E2 | store + X | STORAGE §10 |
| sync-button (`e2e/sync-button.spec.ts:107-270`) | B10, E1 | store + X | no request at launch or after 5 s idle |
| app-language a–k, help a–i, feedback, internet-consent a–j | A5, A8, B2 | memory, then store | — |
| check-keyboard, item-navigation, ds-labels, check-project-switch, check-returns-draft | B4, B5a, B5b, B6, C4 | memory, then store | geometry within 1 px |
| settings-contract, language-suggestions, language-tags | B2, C6 | memory, then store | — |

### 7.8 Tests below the full app

- **Host tests** (`cargo test`): `tc_api::model`, the save machine, the SSE parser, `t()`, the
  `wire` conversions. tC4's unit tests are ported with their vectors: `test/syncButton.test.tsx`,
  `test/actorLabel.test.ts`, `test/homeProjectCard.test.tsx`, `test/sectionDraftExits.test.tsx`,
  `test/openFromDoor43Dialog.test.tsx`, `test/i18n-*.test.ts` (DELTA:1065-1092).
- **Server tests** with `tower::ServiceExt::oneshot` over `MemoryBackend`: routing coverage, the
  security layer, scope staleness, the reply cache, event order and replay, every typed error.
- **Single-screen scenarios**: a `harness` feature mounts one screen against a `ScriptedApi` that
  replays replies recorded from `MemoryBackend`, for fast focus and behaviour tests.

### 7.9 CI

- **Now**: the `rust` job adds wasm clippy for `tc-api` and for `tc-ui` with `--features app` and
  `--features harness`, beside today's two runs (`.github/workflows/ci.yml:11-25`).
  `screen_record.rs` gates the committed report in `cargo test`.
- **Later (G2)**: an `app-e2e` job builds the wasm with wasm-bindgen 0.2.129, builds `tc4` with
  `test-hooks`, installs Chromium build 1194 in a pinned image with this container's fonts, and
  runs the journeys, the DOM and meaning layers, and the perf specs on Psalms. Pixels gate only on
  the pinned image, because fallback glyphs come from the machine
  (`parity/ds-gallery/README.md:294-308`). The whole-Bible perf run is nightly.

---

## 8. Performance

### 8.1 Counters

- `FoldState::counters()` (`storage:crates/tc-journal/src/fold/state.rs:1819`).
- `tc_resources::content_files_opened()` is thread-local (`crates/tc-resources/src/store.rs:183-192`).
  A count test that loads on rayon threads sums the counter on every worker, or loads on the test
  thread.
- tc-store's counters (R14).
- `AppCounters`: `records_read`, `values_decoded`, `items_derived`, `rows_serialized`,
  `chapters_projected`, `book_resource_loads`, `summaries_computed`, `index_updates`.
- `CountingSnapshot` wraps every snapshot a query gets. It counts `record_keys`, full conflict-list
  walks, `render_book` and `book_usfm`, and fails a test whenever an interactive path calls
  `record_keys`.

Every count test runs the action on tc-corpus Psalms and on ten times Psalms and fails if a count
grows (PERF:20-24). From B1 to C1 they run on `MemoryBackend`; from C1 on, on `StoreBackend` too.

### 8.2 Budgets and count tests per screen

Count tests live in `crates/tc-app/tests/op_counts.rs` unless named otherwise.

| Screen, action | Budget (PERF line) | Count test | Earliest backend |
|---|---|---|---|
| Home list, 10 cards | proposed 3 s repeat launch (PERF:100); no cost per history (PERF:12-19) | `home_costs_projects_not_history`: fresh summaries mean 0 folds, 0 pack reads, at most 5 files per card; a stale card starts one summary job | memory (peek path), store *(RT-25)* |
| Repeat launch | 0 resource content reads (PERF:100) | `repeat_launch_reads_no_resource_content_file` (`tc4` test) | store |
| Open project | 3 s / 10 s; first report within 300 ms (PERF:89-90) | tc-store owns it; tc-app adds `app_forwards_open_reports_unchanged` and one `ProjectIndex` pass | store |
| Open a book (Understand, Translate) | 150 ms; first chapter 50 ms (PERF:73) | `open_book_touches_one_book`: records read equal that book's | memory |
| Move to another chapter | 16 ms (PERF:75) | `chapter_switch_sends_no_request` (harness) | memory |
| Save one verse until "Saved" | 25 ms with fsync (PERF:74) | `save_verse_touches_one_verse`: one command, `record_keys` 0, summary update one chapter | store *(RT-20)* |
| Counts and conflicts after a save | 1 ms (PERF:76) | `drafted_counts_update_from_delta`, `conflict_counts_update_from_delta` | store |
| Book resources | 300 ms first, then cached (PERF:87) | `helps_load_one_book`: `book_resource_loads` 1, then 0 | memory |
| Lexicon | 100 ms once; under 1 ms (PERF:88) | `lexicon_lookup_reads_nothing` | memory |
| Open the Check screen | 250 ms first, 20 ms cached (PERF:77) | `check_open_derives_one_unit`; `check_open_cached_derives_nothing` | memory *(RT-12)* |
| Rail filter, sort, group | 16 ms (PERF:78) | `rail_filter_sends_no_request` (harness); Playwright frame timing on the Psalms rail | memory *(RT-13)* |
| Record a decision | 25 ms; counts 1 ms (PERF:79) | `decision_serializes_one_row` | store *(RT-11, RT-12)* |
| Re-assess after a verse edit | 1 ms (PERF:80) | `verse_edit_reassesses_its_checks_only` | memory (scripted delta), store |
| Count and plan an upgrade or gateway change | 2 s; 5 s background, 300 ms per book (PERF:81-82) | tc-check owns the counts; tc-app `plan_cost_follows_affected_books` | memory |
| Open the aligner | 30 ms (PERF:83) | `align_open_touches_one_verse` | memory |
| Aligner edit | inside 30 ms with the round trip (PERF:85) | `align_edit_touches_one_verse`; Playwright `align_click_round_trip_p95` | memory |
| Reflow after a verse edit | 2 ms (PERF:86) | `reflow_touches_one_verse` (at aligner open, AD19) | memory |
| Suggestions | 50 ms; training in the background (PERF:84) | `suggest_uses_trained_model_without_training` | memory |
| Conflict review | interactive | `conflict_tab_costs_its_conflicts` | memory |
| OBS story | interactive | `story_open_touches_one_story` | memory |
| Typing, input to paint | 16 ms (PERF:98) | Playwright `typing_input_to_paint_p95` (Event Timing) | memory |
| Long lists | 16 ms per frame (PERF:97) | Playwright trace on the Psalms rail and chapter | memory |
| Memory | 350 MiB peak, 150 MiB steady (PERF:96) | G2 measures with one screen session and one side session (Q-15) | store |

### 8.3 Never blocking

- **Server**: `tokio_never_blocked`, a test-only watchdog that pings the runtime every 10 ms
  under journey load and fails on a ping more than 50 ms late. Installs run on blocking threads,
  because `install_dcs` fsyncs inside an async function.
- **UI**: no synchronous XHR; typing writes only the browser buffer; the Playwright perf specs
  fail on any long task over 50 ms during a measured action (Long Tasks API).
- **The aligner round trip** is measured first, in unit A2: a 20 KB JSON reply over loopback from
  a wasm `fetch`, p95 recorded in `docs/evidence/`. A miss raises Q-13 before B6 starts. *(RT-14)*

### 8.4 Benchmarks

`crates/tc-app/examples/appbench.rs` drives server actions through `tc-server` over localhost and
prints action, budget, median, p95 and pass or fail. It joins unit P's report and CI job, which
fails a budget only above three times its value (PERF:30-34). UI timings run on a `wasm-release`
build. Results go to `docs/evidence/app-<date>.md` with the machine and commit (PERF rule 5).

---

## 9. What needs the storage units, and what can be built first

### 9.1 What each storage unit gives the app

| Unit | Gives the app | App units it unblocks |
|---|---|---|
| C | tc-journal and the domain crates in one workspace; one owner per shared type; `Session::open` over the fold; validity read from the fold; the To do and alignment-span disagreements settled | B1–B11 (the read halves) |
| S1 | the actor, commands, durable acks, the open with progress, read-only modes, drafts, `atomic_write`, app-data layout | C1–C8 |
| S2 | materialize, checkpoint, outside edits, local git | D1, D2 |
| P | budgets at whole-Bible scale | G2 |
| X | sync, share, join, USB exchange, `ExchangeReport` | E1, E2 |
| I | importers, the tC4 seed | F1 |

Unit C settles two disagreements that block the Check and Align read halves. Its task says so
(`.audit/workflows/storage-chain-B.js:19`):

- **To do.** STORAGE stamps the verse md5 and clears `imported_invalid` whenever the status changes
  (STORAGE:1526-1528); tc-check's To do changes only the status (`crates/tc-check/src/status.rs:295-306`).
  Unit C is told to follow tC4: To do changes only the status.
- **Alignment span.** `staleness` compares `span_end` with the run end directly
  (`crates/tc-align/src/value.rs:174`); STORAGE computes the expected end from the anchor
  (STORAGE §7.7). Unit C makes tc-align anchor-aware or reads validity from the fold.

### 9.2 Requests to the storage team

None changes a storage spec unless it says so. Each has a fallback.

| R | Need | Owner | Fallback |
|---|---|---|---|
| R1 | Range reads: one chapter's verses, one (tool, book)'s decisions, one book's alignments and comments, each costing its result. The fold already keeps them per chapter in `ChapterEntry` (`storage:crates/tc-journal/src/fold/state.rs:242-257`), which is crate-private. | C or P | `ProjectIndex` in tc-app, built once per open in the background *(RT-5, RT-21)* |
| R2 | Create a project in a staging folder and rename it into place without replacing (DELTA:619-624) | S1 | create waits; parity projects stay on `MemoryBackend` until then |
| R3 | Add a book with imported text (tC4 `addUsfmBooks`) | S1 or I | `AddBook` with the imported USFM as `SeedSource`, then one `SaveSection` per chapter on the stub tokens; History shows a seed and an edit |
| R4 | The `ProjectHandle` surface of §4.2: snapshot with its sequence, mint and parse, an async submit, typed events, drafts, leave, checkpoint, counters, close | S1, S2 | — *(RT-5)* |
| R5 | A read never waits behind a long actor write; reads use the latest published snapshot | S1 | — *(RT-5)* |
| R6 | `Draft` for `DraftField::Section` carries every displayed token (STORAGE:1754-1756; the command has one, `command.rs:113-117`) | S1 | the JSON envelope of §6.6 |
| R7 | Commands for the §8.3 choices the set cannot express: bulk resolve in one batch, carried-file rename and keep both, a pins choice with its carry-over, the presence-race restore, accept all from the file, act on a held file, dismiss a notice, undo an outside edit, re-key a decision with `MovedTo`, copy a read-only project | S1, S2 | the review offers only what exists, and "Later" |
| R8 | Test-only holds before a book read, a checkpoint, a materialize, B3 | S1, S2 | delays through `FaultFs` |
| R9 | Measure save latency during a materialize, a checkpoint, a merge commit and a 150 MB `ApplyUpgrade` (25 ms budget, PERF:74) | P | the UI shows "Saving…" and never blocks |
| R10 | Sync, share and join entry points with progress, cancel, `ExchangeReport`, and `NotShared` (DELTA:645) | X | `ScriptedSync` |
| R11 | Import review as a dry run; import job with progress and cancel; tC3 version lookup | I | `ScriptedImport` |
| R12 | `tc-door43`: `count_repositories(owner, lang)` and a `repos/search` route in the fake | app track, B9b | — |
| R13 | `tc-door43-fake`: a dropped-connection fault and a runnable binary; today's faults are statuses, delays, redirects and archive changes (`crates/tc-door43-fake/src/lib.rs:171-203`) | X or app track | scripted outcome |
| R14 | `FoldState::counters` and tc-store counters reachable from tc-app tests | C, S1 | — *(RT-5)* |
| R15 | A `DraftField` for user comments (`command.rs:207-215` has none) | S1 | browser buffer only *(RT-3, RT-18)* |
| R16 | `StoreReply.seq` for read-your-writes; typed notices for `SecondWriter`, `SavedWithDiskError`, a blocked materialize | S1, S2 | — *(RT-5)* |
| R17 | A checkpoint on every mode switch with pending batches (amends STORAGE:1723) | S2, owner | as STORAGE says today if the owner declines *(RT-11)* |
| R18 | Two settings in one batch (direction and font) | S1 | two batches; the dialog says which failed |

A lockless read-only open for Home summaries is not requested: AD13 needs no fold.

### 9.3 What can be built when

| Stage | Real | Compared with tC4 |
|---|---|---|
| Now (phase 0) | server, transport, security, jobs, events, i18n, RTL, shell chrome, account menu, the internet question, help, about, feedback compose, Door43 sign-in against the fake, first-launch seeding, the parity harness and the tC4 gallery | DOM of about 12 dialog subtrees; behaviour of the account menu, M15 and M16; pixels wait for Home (B2) |
| After C (phase 1) | every screen's read side over a real fold of the fixtures; check lists, aligner sessions, helps, OBS, the print preview, the conflict review read side; save behaviour on scripted outcomes | all 62 baselines (57 real, 5 scripted); operation counts for every read |
| After S1 (phase 2) | every save, the real open, drafts, conflict choices, create (with R2), upgrades | J2, J4, J5, J6, J12, J13, J15, J16, J18 (local), J21, J22, J25; J1 and J20 with R2 |
| After S2 (phase 3) | checkpoints, outside edits, exports | J7, J23 (without the Burrito zip), J8 |
| After X (phase 4) | sync, share, join, USB | the 5 scripted screens recaptured; J11, J17, J24, J26, USB, sync-button |
| After I (phase 5) | import | J9a, J9c, J9d; the seed replaced |

---

## 10. Work breakdown

Each unit is one builder agent's job and one coherent commit series. Every unit ends with
`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, wasm clippy for `tc-api` and for `tc-ui` with `gallery`, `app` and
`harness` (AGENTS.md), its rows in `report.json`, and its operation-count tests.

### Phase 0: now, on main

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| A1 tc-api foundations | the crate; ids; `Scope`; `Endpoint`, `endpoints!`, `ALL`; tokens; `Shown`; `Ack`, `NotSaved`; `ApiError`, `Refusal` families, `SyncRefusal`; events, jobs, progress; `ChangeScope`, `depends_on`; `model` (section buffer, drag range, `take_spans`, rail mask and groups); rebuild key files (English, placeholders for nine languages) | — | serde proptest; golden JSON per endpoint (rewritten only with `UPDATE_GOLDEN=1`); the four refusal tests (§3.5); section buffer proptest | builds and lints for wasm32 |
| A2 tc-server and `tc4 serve` | axum on loopback; address line; `app.lock`; security layer; CSP; static routes; `hello`; event stream with ring, heartbeat, `Resync`; uploads with caps; downloads registry; `test-hooks` feature; stdin quit; the loopback spike | A1 | §5.2 security tests; replay and `Resync`; upload over cap is 413; `every_endpoint_is_routed_once`; `tokio_never_blocked` | the spike's p95 is in `docs/evidence/` *(RT-14)* |
| A3 tc-app foundations | `App`, `AppConfig`; `JobRegistry` with reporter and cancel; `EventBus`; `FlowGate`; `PrefsWriter` (memory); `ReportLog`; internet wiring; `Keychain` trait with `MemKeychain`; `app`, `prefs`, `net`, `jobs` handlers | A1 | `job_reports_nothing_before_300ms_then_every_100ms`; `counts_only_grow`; `cancel_stops_at_next_chunk`; flow-gate tests | `tc4 serve` answers `hello` with real state |
| A4 tc-ui app runtime | feature `app`; API client with scope drop; SSE-over-fetch reader; `QueryCache`; `Load<T>`; save machine; draft buffer; `t()` with generated keys and rebuild keys; `<html lang dir>`; panic hook and crash marker; `boot.js`; `build-app.sh`; web-sys features (`Storage`, `Headers`, `Request`, `RequestInit`, `ReadableStream`, `ReadableStreamDefaultReader`, `TextDecoder`, `AbortController`, `File`, `Blob`, `DragEvent`, `DataTransfer`, `HtmlAnchorElement`, `HtmlIFrameElement`, `PerformanceObserver`); vendor tC4's `src/ui.css` again | A1, A2 | host tests of the SSE parser, `t()`, save machine (port plus proptest), draft merge; `i18n-keys` port with floors | Playwright smoke: boot, `hello`, reload keeps place, a forced trap shows the static callout *(RT-15, RT-17)* |
| A5 Shell chrome and static surfaces | `App`, TopBar (X1), save indicator (X2), account menu (X6), M15, M16 with preview, M17 with License, M14 compose, S13 with vendored help pages (AD29) and the page-swap shim, S15 | A4 | scenarios: account-menu keyboard, M16 preview and restore, help a–i (no internet case yet) | dialog DOM parity for `account-menu`, `app-language`, `help`, `about`, `feedback` |
| A6 Parity harness v2 | the `capture.mjs` split; sha256 per PNG in the manifest; tC4 DOM snapshots for all 62 screens; `state.mjs` from repo fixtures; `env-rust.mjs`; compare tool for four layers; `masks.json`, `report.json`, `residuals.json`; `screen_record.rs` | — (Rust env needs A2, A4) | `state_from_repo_equals_state_from_tc4`; the gate fails on a planted difference | tC4 recapture has identical hashes *(RT-17)* |
| A7 tC4 view-state gallery | Vite app with the `state.jsx` stub; case format; action recorder; the §7.4 cases | A6 | every case renders with no console error; two runs are byte-identical | cases are rows in `report.json` |
| A8 Door43 account and internet consent | `account` service: sign-in with `mint_token` against tc-door43-fake, keep, resume, forget; `cancelSignIn`; "no keychain"; M12; the internet question flow (`InternetFlow`, `Pending`) | A3, A5 | sign-in cases of J11; internet-consent a–j that need no project | DOM parity for `share-signin`, `share-internet-consent`, `open-door43-internet-consent` |
| A9 First-launch seeding | progress and cancel hooks in tc-resources seeding; `Seed` job; `Startup` events; `/prepare.html` in 10 languages | A2, A3 | seeding count targets (PERF:133-138); cancel leaves a resumable state | the window shows "Preparing resources: n of total" and Close cancels |

### Phase 1: after unit C merges (MemoryBackend)

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| B1 tc-app core and MemoryBackend | the seam; `MemoryBackend` with scripted outcomes; `seed::tc4_state_seed` with §13.2's check; scenario loader with resources from `.state`; `SessionRegistry`, epochs, `ReplyCache`; `ProjectIndex`; `wire`; `AppCounters`, `CountingSnapshot`; tc-corpus loading; contract suite read cases | C, A1, A3 | seed reproduces both samples' USFM, `vrs.json` and sidecars; `ProjectIndex` equals a scan; contract read cases; epoch refusal tests | count-test harness runs on Psalms and ten times Psalms *(RT-16)* |
| B2 Home and project dialogs | `home/*`; `SummaryCache` (memory); cards, tiles, "—" rules; Resume card; toasts; M1, M2, M3 (server language search, versifications); M4 method step; M5 kind step | B1, A5, A6 | `homeProjectCard` port; `home_costs_projects_not_history`; language-suggestions (UI part) | 11 screens in the report (§7.6) |
| B3 Open and Understand | open job and S14 overlay; `editor/book`, `chapters`, `sources`; `helps/*`; tc-helps block renderer (port of `renderArticleBlocks`); highlights; book rail; helps panel with tabs, slots, article pane, resize; comment box (read) | B2 | J15 UI with a held open; `open_book_touches_one_book`; `helps_load_one_book` | 4 Understand screens |
| B4 Translate: read, editors, scripted saves | grid, sections, source and original chips, Section/Verse switch, conflict markers (read); verse and section editors with Type and Place; M18; `Editing` lock with the rail number button (DELTA:519-523); save machine against scripted outcomes; "changed while typing" | B3 | `sectionDraftExits` port; save states for every scripted outcome; `chapter_switch_sends_no_request` | 4 Translate screens *(RT-1, RT-17)* |
| B5a Check: picker and rail | preflight; picker progress; `CheckListCache`; `check/open` with compact rail; mask and groups; virtual list; M11 view | B3, C | `check_open_cached_derives_nothing`; the rail differential test against `tc_check::rail::groups`; rail perf spec | `check-picker`, `en-check-picker`, `guided-fix` |
| B5b Check: item detail | `check/item`; compare card; target words with drag-select; marks (local); Academy drawer; notices; M8 entry | B5a | check-keyboard and item-navigation (navigation parts); `drag_range` differential test | 12 session screens |
| B6 Align: read and session | `align/index`, `open`, `edit`, `keepReflow` over a server `Aligner`; bank, cards, lenses, occurrence numbers, multi-select, drag; marking row; training job and `Suggest` events | B5a | J5 steps that do not save; `align_open_touches_one_verse`; `align_click_round_trip_p95` within 30 ms | 6 Align screens |
| B7 OBS | story rail; OBS Understand, Translate, Check; `/media/obs` | B3, B5a | `story_open_touches_one_story` | 6 OBS screens |
| B8 Community Checking preview | `tc-export` crate with the print model (Bible, port of `tC4:src/views/print/PrintBook.jsx:44-71`, and OBS); page setup; export items listed | B4, B7 | print model tests on fixtures | `en-community-checking` |
| B9a Settings, source texts, downloads | M7 (read, and `viewFromHome` per AD14); M8 steps 1 and 2; download jobs; `tc-door43` streaming downloads with byte progress and per-chunk cancel (today `download` returns one buffer, `crates/tc-door43/src/client.rs:520-526`) | B2, B3 | settings-contract (read part); cancel leaves no partial install | `project-settings`, `source-texts` *(RT-17)* |
| B9b Share steps and Open from Door43 | share target and check steps against the fake; R12 (count and the fake's route); M6 form with `ScriptedOpenDoor43`; shared state on cards | A8, B2 | `openFromDoor43Dialog` port; internet-consent share and open cases | 6 screens (§7.6) |
| B10 Scripted sync and conflict chips | `ScriptedSync`; `SyncResultView`; X10 toast; X12 chips and tile counts from fold conflicts | B9b | `syncButton` toast ports; sync refusal closed-set test through the UI | 7 screens; milestone: 62 of 62 in the report |
| B11 Conflict review: read side | review tabs, grouping, labels, colours, clock warning; detail; History; Detached; Notices; "2 versions" markers in Translate, Align and Check; resolve on scripted outcomes | B4, B5b, B6 | `actorLabel` port; gallery case S4 Text tab against tC4's verse review | J18's UI steps on scripted outcomes |

### Phase 2: after S1

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| C1 StoreBackend | the adapter; contract suite on the store; app data on `atomic_write`; persisted summaries; read-only banners and the copy offer *(RT-10)*; `SecondWriter`; draft offers; parity projects on disk through S1's create and sealing API; the parity environment switched to the store | S1, B1 | every contract case; J15 on real `FaultFs` delays; drafts crash test (kill the server, relaunch, offer, Keep makes a conflict when the verse changed) | every phase-1 screen captured so far is recaptured on `store` with the same report; `MemoryBackend` is deleted when the last B unit has moved to the store |
| C2 Translate saves | `saveVerse`, `saveLayout`, `saveSection`, `draft`; Retry; `NotWritten`; `SavedWithDiskError`; Resume from saves and drafts | C1 | J2; J6 edit part; `save_verse_touches_one_verse`; "Saved" never before B3 (held save); `draft_survives_panic` | J2 green |
| C3 Comments and OBS saves | `addComment` with the reply cache and the restart rule; `saveFrame`, `saveRef`; OBS editing lock | C1 | J16, J21, J25 | green |
| C4 Check writes | `check/decide`; re-assessment after verse deltas; late draft (DELTA:906-907) | C1, B5b | J4, J6, J22; check-project-switch, check-returns-draft; `decision_serializes_one_row` | green |
| C5 Align writes and suggestions | `align/save`, `markValid`; training per testament from one snapshot; suggest, accept, reject | C1, B6 | J5; suggestion budget | green |
| C6 Create, add books, settings | M2, M3 create (R2); M4 blank books (`AddBook`, seeding from the pinned ULT); add from USFM (R3 or fallback); settings save, including from Home (AD14) | C1 | J1, J20; settings-contract; language-tags; `settings_from_home_mints_only_for_shown_values` | green |
| C7 Conflict choices | every §8.3 choice the commands allow; review tokens; `LookAgain`; History and Restore; Detached and Dismiss; received sheet (Q-8) | C1, B11 | J18 local; a check-conflict resolution; the look-again refusal through the UI | green *(RT-17)* |
| C8 Resources apply | upgrade offers; M9; gateway change M10; plans with `carry::plan` converted to planned changes; `applyPlan` with `PlanStale`; guided-fix fetch, re-pin, sideload | C1, B9a | J3, J12, J13; bundled-upgrade; `plan_cost_follows_affected_books` | green |

### Phase 3: after S2

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| D1 Checkpoints and outside edits | leave triggers; checkpoint per mode switch and on leaving (AD34); "Checkpoint not saved" with Retry; materialize-blocked message; outside-edit toast with Undo and grouped rows; held-file actions as R7 allows; window focus check; quit flow | S2, C2 | J8; no commit when nothing changed | green |
| D2 Exports | tc-export producers: aligned and plain USFM, OBS Markdown, print HTML; export jobs; `/download`, `/print`; PDF through the shell; conflict warning; checkpoint first | S2, B8 | J7, J23 except the Burrito zip; every Burrito output validated | green |

### Phase 4: after X

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| E1 Sync, share, open from Door43 | the real `SyncPort`; OS keychain (Q-3); sync button flow (DELTA:717-738); share create, push, done; open with sign-in on 401 or 404; R13 | X, B10, C7, D1 | J11, J17, J24, J26 with three app-data folders, sync-button | the 5 scripted screens and `shared-home` recaptured on real data *(RT-17)* |
| E2 USB exchange | Save work for a teammate (folder, progress, verify, "You can remove the stick"); Receive work from a file (identify, confirm, job, report); the J7 Burrito zip as the exchange file | X, C1, D2 | USB journey both ways, then a relay through a third person (STORAGE:3819) | green |

### Phase 5: after I

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| F1 Import | M5 all steps with uploads; tC3 version lookup; name clash; working state with progress and cancel; toast and badge | I, C1 | J9a, J9c, J9d; language-suggestions | `memory_seed_equals_tc_import_seed` passes and the local seed is deleted |

### Phase 6: shell and performance

| Unit | Scope | Depends on | Tests | Done when |
|---|---|---|---|---|
| G1 Electronite shell | main script: start `tc4`, read the address line, header injection with a probe of every request type, single-instance lock, the prep window, close guard and quit, `printToPDF`, file and folder dialogs, `openExternal`, keychain or feedback per Q-2 and Q-3 | A2, A9, C1, D2 | port of `scripts/desktop-prepare.test.cjs` (DELTA:1127-1131) | the probe shows the header on every request type, or capability URLs replace the ones it misses *(RT-14, RT-17)* |
| G2 Budgets and the P7 report | `appbench`; Playwright perf specs on the whole-Bible project; memory with a side session; the `app-e2e` CI job; the 62-screen report in `en` and `ar` | P, every unit | §8.2 green at the 3× tolerance | `docs/evidence/app-budgets-<date>.md`; a person reviewed the residuals |

**Order and parallelism.** In phase 0, A1 comes first; A2, A3 and A6 then run in parallel; A4
after A2; A5 after A3 and A4; A9 after A2 and A3; A7 after A6; A8 after A5. In phase 1, B1 comes
first and B2 after it; B3 and B9b can run in parallel after B2; B4, B5a, B7 and B9a after B3; B5b
and B6 after B5a; B8 after B4 and B7; B10 after B9b; B11 after B4, B5b and B6. C1 can start as soon
as S1 and B1 have landed; C2 to C8 run in parallel after C1, each after the B unit it extends. D, E
and F start when their storage unit lands.

---

## 11. Risks

1. **Unit C gates phase 1.** It is in progress. If it is slow, phase 0 still delivers the
   server, the shell, the harness and the gallery.
2. **The seed is a second §13.2 until unit I lands.** It is checked byte for byte, compared with
   unit I's seed, and then deleted. C1 recaptures every screen on disk.
3. **The aligner round trip** may miss 30 ms with large verses. A2 measures it before anything
   depends on it; the fallback needs an owner ruling (Q-13).
4. **Saves may wait behind long actor writes** (R9): a checkpoint, a merge commit or a 150 MB
   upgrade batch on the one writer. The UI never blocks, but the 25 ms budget can fail. *(RT-11, RT-20)*
5. **Large replies.** The Psalms rail and book model are about 0.5–1 MB of JSON, 10–20 ms to parse
   once per open on the UI thread. A slow laptop may drop a frame when a screen opens. *(RT-12)*
6. **DOM equality at screen scale.** Generated ids follow mount order; Leptos adds comment
   markers; virtualized lists differ above 400 rows. Normalizations and the threshold cover the
   fixture screens; rebuild-only DOM has no reference.
7. **Silent settle timeouts** would give screenshots that look valid. The event stream is excluded,
   `test/quiet` is added, and a screen that hits the deadline fails.
8. **Machine-dependent glyphs.** Both captures run on one image until CI has a pinned one.
9. **Header injection** may miss a request type in Electronite v37. G1 probes every type; the
   fallback is capability URLs.
10. **A panic** loses a navigation that was in flight when it happened. Saves are not lost: their
    text and `RequestId` are in the browser buffer, and the reply cache answers the retry. *(RT-2, RT-15)*
11. **Scripted saves do not change the fold.** A phase-1 test that saves and then reads again sees
    the old value. Such tests wait for C1.
12. **A save in flight when the server process dies**, whose reply was lost, is checked by
    content after the restart (AD4). If a teammate's change built on that save arrived in the same
    gap, the resend shows as a conflict the person did not need. This needs a server crash, a
    lost reply and a sync in one window; the conflict loses nothing. *(RT-3)*
13. **Home summaries come from derived files.** After a crash before a materialization, or with a
    held file, a card shows the files' text until the project opens and its summary is rebuilt
    from the fold.
14. **Two folders with one project id** share a token key and a prefs file. The scope check keeps
    tokens in their session; shared place prefs are acceptable.
15. **Side sessions.** A sync from Home beside an open whole Bible holds two folds (PERF:96, Q-15).
16. **Rebuild-only text** needs ten languages before release (Q-10).
17. **The view-state gallery** has hand-written fixtures where no recorded reply exists; a pixel
    difference cannot tell a fixture bug from a port bug. Recorded replies are preferred.

---

## 12. Questions only the owner can answer

| Q | Question | Recommendation |
|---|---|---|
| Q-1 | **Stable origin for the browser draft buffer.** Every launch has a new port, so `localStorage` covers only a reload; across launches only `drafts.json` (up to one second old) counts. STORAGE §2.7 expects tokens in the browser buffer to survive a restart (STORAGE:745-747). Should the shell serve the UI from a privileged custom scheme (Electron `protocol.handle`) proxied to the server? That gives one origin across launches and lets the shell add the token header itself. | Yes, in G1. The UI code is the same either way. *(RT-3)* |
| Q-2 | **Feedback send.** tC4 sends from Electron main to SendGrid (`tC4:scripts/desktop-feedback.cjs:13`). ARCH:13-14 gives every network request to the server; AGENTS.md allows network only in tc-door43 and tc-resources. Options: (a) the shell sends, gated by the server's switch; (b) a mail client in tc-door43 with the key as a build secret; (c) no sending, keep "Copy the report". | (a): it keeps the key out of the server and reuses tC4's code. |
| Q-3 | **Keychain.** The `keyring` crate in the server, or Electronite `safeStorage` through the shell (`tC4:scripts/desktop-main.cjs:51-96`)? | `keyring` behind the `Keychain` trait; keep "no keychain on this computer". |
| Q-4 | **Projects folder.** A default under Documents that OneDrive redirects would open every project read-only (STORAGE:1387-1396). | `%USERPROFILE%\translationCore4\projects` (or `~/translationCore4/projects`), checked against the cloud detection at first launch. |
| Q-5 | **Checkpoint on every mode switch.** tC4's J8 asserts it (`tC4:e2e/j08-resume.spec.ts:194`); STORAGE checkpoints only on leaving Translate or the project (STORAGE:1723). Each checkpoint runs on the actor. | Follow tC4; amend STORAGE (R17). *(RT-11)* |
| Q-6 | **Alignment reflow after a verse edit.** tC4 writes the reflow at once (`tC4:src/state.jsx:7194-7220`), so a lossless reflow keeps the verse "aligned". The rebuild writes nothing (STORAGE:1482-1485), so the Align rail shows "needs review" until the aligner opens the verse and saves the lossless reflow. Accept, or let tc-app write lossless reflows as a second batch after the verse save (an automated writer, allowed by I4)? | Accept for now; measure how often it shows in J5 and J6. |
| Q-7 | **Open progress captions.** STORAGE has five phases (STORAGE:1873-1876); tC4's overlay has three captions. Map reading and folding to the journal caption, finishing and checking to "Checking the project state", writing to "Preparing the project"; or add keys. | Map, with a rebuild key for "reading changes". *(RT-22)* |
| Q-8 | **After a sync:** STORAGE asks for a received-work sheet (STORAGE:2681-2684); tC4 shows the X10 toast. | The toast for Door43 sync; the sheet for Receive work from a file. |
| Q-9 | **Rebuild-only screens** with no tC4 design: review tabs beyond Text, History, Notices, the draft offer, the read-only copy offer, USB flows, the prep window. Design them here from tC4's design system, or in tC4 first so the catalogs stay shared? | Here, signed off by the owner, listed under P7. |
| Q-10 | **Translations** of rebuild-only keys into nine languages: who, and does a release wait? | English fallback per key until translated; release waits. |
| Q-11 | **Parity resources.** Reproduce the baseline's substitutes and absences (the default), or recapture tC4 from `bundle/en` so more screens are complete? | Reproduce now; recapture at the v4.0.0 tag. |
| Q-12 | **Virtualization threshold** of 400 rows: whole DOM on the fixtures, virtualized above. | Accept. |
| Q-13 | **Aligner fallback.** If a click's round trip misses 30 ms, may tc-ui link tc-align's pure `Aligner`, an exception to ARCH:88? | Decide after A2's measurement. *(RT-14)* |
| Q-14 | **Settings from Home** show from files and open the project (with progress) only on Save. tC4 also opens a store only for the save, without making the project current (`tC4:src/state.jsx:6741-6744`), but a whole Bible's open can take up to 10 s. | Accept. |
| Q-15 | **Memory.** A sync or USB export from Home opens a side session; with another whole Bible open that is two folds against a 350 MiB peak (PERF:96). Allow, or make the second open wait? | Allow; G2 measures. |
| Q-16 | **Branches.** Phase 0 lands on main. After unit C, should storage merge into main so app units land on main, or should app units land on a branch cut from storage? Starting B1–B4 on a merge branch before C finishes is possible but risks rework if C changes the pin types. | Merge storage into main after C; storage units merge into main as each lands. |
| Q-17 | **Storage requests** R2, R3, R6, R7, R15, R17 and R18: provide them, or keep the fallbacks? | Provide R2, R6, R7 and R15; R3 and R18 can keep their fallbacks. |
| Q-18 | **tc-export** has no owner in the storage chain. The app track builds it (B8, D2). | Confirm. |
| Q-19 | **Help pages** vendored from tc-website `cdfdf9b`: who updates them, and where is the licence recorded? | A `LICENSE` record beside them; updated with each tC4 reference move. |
| Q-20 | **Verse editor content.** The rebuild edits STORAGE `words`; tC4 edits the trimmed raw body. They differ only when a verse ends in a layout-only line (STORAGE §2.4). | Accept and list. |
| Q-21 | **The `open-door43-refused` sentence** under Q3 is not decided (`parity/tc4-baseline/README.md:391`). | A new key without "make sure the repository is public". |
| Q-22 | **Test hooks** compile only under cargo features that release builds never enable. | Confirm that no hook may ship. |

---

## 13. Sources

- **Repository documents**: `AGENTS.md`; `docs/ARCHITECTURE.md`; `docs/PERFORMANCE.md`;
  `docs/PLAN.md`; `docs/TC4-DELTA-2026-10-16.md`; `parity/tc4-baseline/README.md`;
  `parity/ds-gallery/README.md`; `.audit/workflows/storage-chain-B.js`.
- **Storage spec**: `git -C /home/user/tc-rst show storage:docs/STORAGE.md` at `225091b` (lines up to 3,445 identical at `6bb20d4`).
- **Code checked for this design** (main unless marked): `Cargo.toml`; `.github/workflows/ci.yml`;
  `crates/tc-core/src/{books,lib}.rs`; `crates/tc-check/src/{rail,status}.rs`;
  `crates/tc-align/src/{aligner,reflow,value}.rs`; `crates/tc-resources/src/store.rs`;
  `crates/tc-door43/src/{client,internet,repos,lib}.rs`; `crates/tc-door43-fake/src/lib.rs`;
  `crates/tc-ui/{Cargo.toml,vendor-tc4.sh}`; `crates/tc-ui/tests/`;
  `parity/tc4-baseline/{capture.mjs,baseline.test.mjs,fixtures/build-state.mjs}`;
  `parity/ds-gallery/vite.config.mjs`; `fixtures/tc4/`; storage:
  `crates/tc-journal/src/{command.rs,token.rs,fold/reads.rs,fold/state.rs,fold/project.rs,fold/sidecar.rs}`.
- **tC4** at `d310bf3`: `src/state.jsx` (`go` 4018-4040, `recordDecision` 5649-5689,
  `blurVerse` and `reflowAlignedVerses` 7194-7220), `src/data/share/door43Api.ts:210-231`,
  `src/data/saveScheduler.ts`, `src/views/OpenProgress.jsx:16-19`, `src/App.jsx:163-190`,
  `src/views/Home.jsx:108, 229`, `e2e/j08-resume.spec.ts:194`, and the `state.jsx` importers
  under `src/`.
- **Design run inputs**: the three designs (parity-first, core-first, slice-first); the two
  judgments and their graft lists; the five fact reports (screens, state, server, core, parity).

---

## Red team

Checked on 2026-10-17 against main `21fef4a`, storage `2dcfe6f` (its STORAGE.md is byte-identical
to `225091b`; the one new commit changes only the corpus loader) and tC4 `d310bf3`. Each finding
gives its severity, the evidence, what breaks, and what should change. The body text that a
finding concerns is marked *(RT-n)*.

- **High**: a translator's work can be lost or silently rewritten on a path this design
  prescribes, or the plan cannot proceed as written.
- **Medium**: a budget, a parity check or a unit plan fails unless the design changes.
- **Low**: a smaller gap.

Figures marked "estimate" are worked out from sizes, not measured.

| RT | Severity | Area | Finding |
|---|---|---|---|
| RT-1 | High | Parity, no loss | The section editor saves on blur and after 2 s; tC4 saves it only on Save. |
| RT-2 | High | No loss | A retry after a reload can mark newer text as saved, or run twice. |
| RT-3 | High | No loss, shell | A server crash cannot be recovered as AD4 says; a shell crash leaves a server that blocks the next launch. |
| RT-4 | High | No loss | Unsaved aligner links have no draft, no close guard and no save at quit. |
| RT-5 | High | Storage scope | R1, R4, R5, R14 and R16 are in no storage unit's task, and four of them have no fallback. |
| RT-6 | Medium | No loss, RTL | An open editor whose row is refreshed underneath has no rule. |
| RT-7 | Medium | Tokens | Review tokens and edit tokens are one type below tc-api. |
| RT-8 | Medium | No loss | Two opens of one folder at once make one of them read-only. |
| RT-9 | Medium | No loss | Drafts whose token no longer verifies are dropped as bugs. |
| RT-10 | Medium | No loss | Check-comment drafts and the read-only copy drop typed text. |
| RT-11 | Medium | Performance | A checkpoint at every mode switch holds the actor for up to 500 ms. |
| RT-12 | Medium | Performance | Coarse invalidation re-sends the whole rail; the cached Check open cannot meet 20 ms. |
| RT-13 | Medium | Performance | The rail grouping port costs rows × terms. |
| RT-14 | Medium | Performance | The aligner round trip is measured where it is cheapest; fast clicks can be dropped. |
| RT-15 | Medium | No loss | The panic path has five gaps. |
| RT-16 | Medium | Units | B1 is too big and gates all of phase 1. |
| RT-17 | Medium | Units | A4, A6, B4, C7, E1 and G1 are too big or hide unknowns. |
| RT-18 | Medium | Parity, storage | Every pause in the Understand comment box mints a permanent comment. |
| RT-19 | Medium | No loss | `StaleScope` on a write is undefined. |
| RT-20 | Medium | Performance | Typing causes two fsynced writes a second, one of them on the actor. |
| RT-21 | Medium | Performance | `ProjectIndex` costs the whole project at every open. |
| RT-22 | Low | Parity | The open overlay is event-driven; tC4's is timer-driven, with indeterminate stages. |
| RT-23 | Low | Parity | Catalogs are fetched on demand; tC4 bundles all ten. |
| RT-24 | Low | Parity | Virtualized rails above 400 rows need more than one fixed row height. |
| RT-25 | Low | Parity, tests | Phase-1 Home tests a path the product does not run; when the screen session closes is unclear. |
| RT-26 | Low | Parity | The navigation gate misses the gateway-change and upgrade previews. |
| RT-27 | Low | No loss | Double submits give false refusals and duplicate changes. |
| RT-28 | Low | Storage | The upload cap misreads STORAGE's per-entry cap. |
| RT-29 | Low | Performance | Background progress streams at 10 Hz to screens that do not show it. |
| RT-30 | Low | AGENTS.md | Several view types are bags of booleans. |
| RT-31 | Low | Storage | FlowGate leaves out USB export and receive. |
| RT-32 | Low | Catalogs | Placeholder translations pass the locale test. |
| RT-33 | Low | Shell | A tC4-style window-open handler sends the app's own links to the system browser. |
| RT-34 | Low | Parity | P7's pixels are taken in Chromium 141; the product ships Electronite's Chromium. |

### RT-1 (High) The section editor must save only on Save

- **Where:** §6.5 Timing, row "verse, layout, section"; AD20.
- **Evidence:** tC4's section card "keeps its draft locally — nothing is written until Save"
  (`tC4:src/views/SectionEditor.jsx:4-6`). Save and Cancel are buttons (`:203-226`), and Cancel
  calls `blurVerse`, which discards the card. tC4's state says "It holds its own text until
  Save, so opening it writes nothing" (`tC4:src/state.jsx:7219-7220`). A save that leaves the
  section with no words asks first (#600, `SectionEditor.jsx:8-9, 223`). STORAGE keeps the
  unsaved section buffer as a draft (STORAGE:1486-1487, 1754-1756), which fits Save-only.
- **What breaks:** In Place mode the translator drags verse pins. A click on a pin or on the
  "To place" bank blurs the textarea, and two seconds of thought also trigger a save. Each save
  is a passage batch that joins, splits or renumbers verses (STORAGE §5.2) that the translator
  has not confirmed. Cancel can no longer undo. The #600 question is skipped, so an emptied box
  writes `___` into every verse of the section. Half-made structures reach teammates at the
  next sync. J2's section steps and the `sectionDraftExits` port differ from tC4.
- **Change:** For the section editor, send `SaveSection` only on Save. Blur and the 2 s timer
  write the browser buffer and send the `Draft` only. Cancel deletes the buffer entry and the
  draft. Keep the #600 question. Add a count test: typing, placing pins and blurring in the
  section editor send no `editor/saveSection` until Save.

### RT-2 (High) A retry after a reload can mark newer text as saved, or run twice

- **Where:** AD4; §6.5 "A lost reply ... is retried with the same `RequestId` and text"; §6.6,
  the buffer entry `{api, base, text, request, at}`.
- **Evidence:** The buffer keeps the current text and the request id, but not the text that was
  sent. The save machine marks a field clean when "the text still equals `sent`" (§6.5). The
  reply cache answers by `RequestId` alone (AD4). `AddComment` mints a random `CommentId` each
  time it runs (STORAGE:1536).
- **What breaks:**
  1. The person types T1. The 2 s save sends T1 as request R1. They keep typing: T2. A panic
     reloads the page. The UI rebuilds the field from the buffer (text T2, request R1) and
     retries R1 with T2. The cache returns T1's `Ack`. The field is marked clean with T2 as its
     persisted text, so T2 is never sent. The navigation gate lets the person leave, and T2 is
     lost when the editor closes.
  2. A reload takes about a second. If R1 is still waiting on the actor (behind a checkpoint,
     RT-11), the retry finds no cache entry and runs again: a duplicate change, or for
     `AddComment` a duplicate comment.
- **Change:** The buffer entry gains `sent`. The reply cache stores a digest of each request body
  and an in-flight marker. A retry of an in-flight id waits for its result. A retry with a
  different body gets a typed `ApiError::RequestReused`, never the cached `Ack`. The UI marks a
  field clean only when `Ack.saved` equals the NFC form of its current text. Add a property test
  that reloads at every point of a type, save and reply interleaving and asserts that the last
  text reaches the store exactly once.

### RT-3 (High) After a server crash, AD4's restart path cannot happen; nothing supervises the server

- **Where:** AD4 ("After a server restart ..."), §3.10 (`ServerInstance`), §5.1, §5.2, AD21, Q-1,
  G1.
- **Evidence:** The server binds `127.0.0.1:0` (§5.1, ARCH:12), so a restarted server has a new
  port, a new origin and a new `Host`. §5.2 refuses the old page's requests (the Host and Origin
  checks, no CORS), and the old page cannot read the new origin's `localStorage`. G1 has no step
  for a child that exits. §5.1 quits only on `{"quit":true}` and says nothing about stdin
  closing. `app.lock` makes a second `tc4` exit (§5.1).
- **What breaks:** If `tc4` crashes, the window is dead and nobody restarts the server. What
  survives is the pending saves and `.tc/drafts.json`, up to a second old. Lost: the last
  second of typing, every unsent user comment (no `DraftField`, R15), unsaved aligner links
  (RT-4), and the browser buffer. If Electronite crashes and `tc4` lives on, the orphan keeps
  `app.lock` and `.tc/lock`, and the next launch is refused until the person ends it in Task
  Manager.
- **Change:** Decide Q-1 before A4, because it decides what the buffer is. Without a stable
  origin, the preload sends each buffer change to the shell (`ipcRenderer.send`), which writes
  it to a file in app data; at the next launch the shell hands the entries to the server as
  drafts. In G1, watch the child process. When it exits, read the old page's buffer with
  `webContents.executeJavaScript`, start `tc4` again, pass the entries as drafts, then load the
  new URL. `tc4` treats a closed stdin as a quit. `app.lock` records the pid and port so a new
  shell can ask an orphan to quit. Tests: kill `tc4` while typing, relaunch, the text is
  offered; kill the shell, `tc4` exits within 5 s and the next launch starts. Keep AD4's
  restart branch only for the stable-origin case.

### RT-4 (High) Unsaved aligner links have no draft, no close guard and no save at quit

- **Where:** AD18, AD22, §4.4 `AlignerSession`, §6.5 (alignment timing and the close guard),
  §5.1 quit.
- **Evidence:** The links live in the server's `Aligner` (AD18). `DraftField` has no alignment
  variant (`storage:crates/tc-journal/src/command.rs:207-215`). The close guard uses the UI
  fields' `all_clean` (§6.5). The quit checkpoints sessions (§5.1) and says nothing about live
  aligner sessions. tC4's close guard counts `alignSaveState` (`tC4:src/state.jsx:3718-3720`),
  and tC4 drains every scheduler on `pagehide` (`:3699-3704`).
- **What breaks:** The UI owns the 2 s timer and the save on leaving the verse. After a panic
  the UI may come back on another verse, and the server keeps unsaved links that nobody saves.
  Closing the window gives no warning for them. A quit or a crash drops them.
- **Change:** The server owns the alignment save: 2 s after the last link change, when an
  aligner session is replaced or closed, and before the quit checkpoint. `AlignerView.changed`
  feeds the close guard. Ask the storage team for `DraftField::Alignment`, or accept the 2 s
  window in writing. Test: link words, quit within 2 s, reopen, the links are there.

### RT-5 (High) R1, R4, R5, R14 and R16 are in no storage unit's task

- **Where:** §9.2 R1, R4, R5, R14 and R16; §9.1; phase 2.
- **Evidence:** The task texts of units C, S1 and P (`.audit/workflows/storage-chain-B.js:19-22,
  31`) ask for commands, the open, drafts, crash tests and budgets. They do not ask for a
  snapshot that readers take without waiting on the actor, an event subscription, an async
  submit, range reads, counters that another crate can read, or `StoreReply.seq`. §9.2 gives
  R4, R5, R14 and R16 no fallback.
- **What breaks:** C1 to C8 assume `ProjectHandle` (§4.2). If S1 lands with a synchronous actor
  API and no published snapshot, every phase-2 unit waits, or tc-app builds a second actor
  around the store.
- **Change:** Put R1, R4, R5, R14 and R16 into the task texts of S1 (and of C, for R1 and R14)
  now, with the contract cases of §4.2 as their tests. Otherwise plan C1 as an adapter that
  publishes a snapshot after each reply and forwards events, and size it as its own unit.

### RT-6 (Medium) An open editor whose row is refreshed underneath has no rule

- **Where:** §6.3 first bullet; §6.4; §5.4 Ordering; §6.2.
- **Evidence:** §6.3 covers a field "with unsaved text" only. §6.4 says a save sends "the token
  of the value the editor started from". §5.4 says the `Ack` and the `Changed` event of one
  write can arrive in either order. STORAGE stores the NFC form of the text (STORAGE:1445). tC4
  keeps the typed text in its editor and normalizes only in the store.
- **What breaks:**
  1. An editor is open and clean. An outside edit, a resolve or a `Resync` refreshes the row.
     If the textarea follows the query but the base token does not, the person edits the new
     text and the save conflicts with itself. If the textarea stays and the token follows the
     query, the save silently reverts the change.
  2. The person's own save emits `Changed` before its `Ack`. The field is `Saving`, the
     refreshed token differs, and §6.3 shows "Juan changed this verse while you were typing",
     about the person's own save.
  3. Writing `Ack.saved` into a focused field reorders Hebrew points and Arabic marks under NFC,
     moves the caret and breaks an IME composition.
- **Change:** An editor takes its value and token together when it opens. A refresh replaces
  both only when the editor has had no keystroke and no composition since it opened; otherwise
  it shows the notice. A `Changed` whose cause is `LocalSave { request }` for the field's own
  request shows nothing. Never write into a focused field, and compare texts under NFC. Tests:
  `refresh_under_open_clean_editor_swaps_value_and_token_together`,
  `own_save_event_before_ack_shows_no_notice`, and an Arabic IME case.

### RT-7 (Medium) Review tokens and edit tokens are one type below tc-api

- **Where:** AD3; §4.2 (`mint_review` returns `ShownToken`); §6.4 `no_token_in_dom`.
- **Evidence:** tc-journal has one `ShownToken { key, heads, mac }`
  (`storage:crates/tc-journal/src/token.rs`). For a record in conflict, an edit token lists only
  the displayed candidate (`storage:crates/tc-journal/src/fold/reads.rs:276-341`); a review
  token lists every head. The MAC does not say which kind it is.
- **What breaks:** A conversion bug in `tc-app::wire` that hands a review token to an editor's
  `Shown<T>` passes the MAC check. The edit then lists every head as a parent and quietly ends
  the conflict. tc-api's two types cannot catch a bug on the server side of the conversion.
  Separately, `no_token_in_dom` looks for base64url runs of 40 characters or more. A 40-hex
  commit sha (the Source texts dialog, J3) matches, so the test fails on correct screens or
  needs an exemption that could hide a token.
- **Change:** Separate the two MACs (a purpose byte in the payload, or a second derived key), so
  the actor refuses a review token in a save and an edit token in `Resolve`. That is a request
  to STORAGE §2.7. Give the wire form a fixed prefix, so the DOM scan matches exactly. Add a
  contract case through HTTP: editing a verse in conflict keeps the conflict.

### RT-8 (Medium) Two opens of one folder at once make one of them read-only

- **Where:** §4.3 first bullet; AD14; side sessions (§4.3, Q-15).
- **Evidence:** §4.3 makes an open wait only for a running leave-checkpoint. A second
  `ProjectStore::open` of the same folder finds `.tc/lock` held and opens read-only with "This
  project is open in another translationCore window" (STORAGE:1790).
- **What breaks:** The person clicks a card while a Home sync's side session is opening, or while
  the Settings-from-Home save opens the project. One of the two becomes read-only with a false
  message, or the sync refuses with `ReadOnly`.
- **Change:** `SessionRegistry` keeps one in-flight open per folder. Later callers wait for it
  and share the session. Test both orders.

### RT-9 (Medium) Drafts whose token no longer verifies are dropped as bugs

- **Where:** §3.4 `NotSaved::BadToken` ("a bug, reported"); §6.6 "At the next launch".
- **Evidence:** The token key comes from the installation secret and the project id
  (STORAGE:745-754). A new installation secret only means a new device (STORAGE:3607).
  `.tc/drafts.json` lives in the project folder and travels with a copied folder. STORAGE §5.14
  says nothing about a draft whose token fails its MAC.
- **What breaks:** After an app-data reset, a reinstall, or a folder copied to another computer,
  each offered draft fails the MAC. "Keep" is refused as a bug, and the text is gone. A browser
  buffer entry for a folder name that was deleted and made again meets the same fate.
- **Change:** Offer such drafts as text. "Keep as a separate version" writes a change with no
  parents, which shows as a conflict, so nothing is overwritten. Ask the storage team to state
  this in §5.14 and O12. Key browser buffer entries by folder and project id.

### RT-10 (Medium) Check-comment drafts and the read-only copy drop typed text

- **Where:** §6.5 Timing ("check comment ... at once, on each act"); §6.4 `WriteCap`; the copy
  offer (C1).
- **Evidence:** tC4 writes a check comment on Done, and a new item resets the editor to the
  stored text (`tC4:src/views/Check.jsx:815-825`), so tC4 discards a comment that is being typed
  when the checker moves on. `DraftField::CheckComment` exists
  (`storage:crates/tc-journal/src/command.rs:214`). The copy offered for a read-only project
  "keeps every pending save" (STORAGE:1389), not unsaved editor text, and the browser buffer is
  keyed by the old folder (§6.6).
- **What breaks:** Porting tC4 as it is loses the comment, and "at once, on each act" does not say
  which act. After a `SecondWriter` switch, the dirty fields cannot save, and the copy the
  person moves to does not have them.
- **Change:** A check comment saves on Done and on leaving the item, with a buffer entry and a
  `Draft` while its editor is open. The copy flow re-keys the buffer entries and the
  `drafts.json` entries to the copy and offers them there.

### RT-11 (Medium) A checkpoint at every mode switch holds the actor for up to 500 ms

- **Where:** AD34; R17; Q-5; risk 4.
- **Evidence:** tC4 does checkpoint at every mode switch (`tC4:src/state.jsx:4031-4038`), so AD34
  reads tC4 correctly. In STORAGE a checkpoint is an actor step (STORAGE:2720). It seals,
  materializes, checks the pack set, builds the whole tree and runs `tc_burrito::check`
  (STORAGE:1725-1735). Its budget is 500 ms for a whole Bible (PERF:92).
- **What breaks:** A checker who moves between Check and Align after each decision starts a
  checkpoint each time. The next decision waits behind it: up to twenty times the 25 ms save
  budget.
- **Change:** Before R17 is adopted, have unit P measure save p95 during a checkpoint (R9), and
  split the checkpoint so that only the seal and the final ref update are actor steps. If that
  cannot hold the budget, ask the owner whether mode-switch checkpoints may be coalesced (J8
  asserts one commit at the first switch).

### RT-12 (Medium) Coarse invalidation re-sends the whole rail

- **Where:** §3.6 `ChangeScope.checks`; §6.2 and §6.3; AD17; §8.2 "Open the Check screen".
- **Evidence:** `checks: Vec<(Tool, BookId)>` names a whole book, and `depends_on` is the only
  refresh rule. The Psalms tN rail has 8,212 rows (PERF:4), about 1.2 MB of JSON at about 150
  bytes a row (estimate). Risk 5 puts parsing alone at 10–20 ms. The budget is 20 ms from the
  cache (PERF:77).
- **What breaks:** Each decision and each verse save in Psalms refetches and parses the whole
  rail: a book-sized transfer per click and dropped frames. A cached open sends the rail again
  each time.
- **Change:** Patch rail rows from `ChangeScope.records`. Never refetch for a `LocalSave` whose
  `Ack` carried the row. Keep the rail in the UI across opens, keyed by the `CheckListCache` key
  and the `ViewSeq`, and ask only for rows changed since that `ViewSeq`. Count test: a decision
  sends one row.

### RT-13 (Medium) The rail grouping port costs rows × terms

- **Where:** §3.8, the rail model; AD17; PERF:78 (16 ms).
- **Evidence:** tC4 groups by word with one `filter` over all rows for each term
  (`tC4:src/views/Check.jsx:889-898`). tc-check does the same
  (`crates/tc-check/src/rail.rs:138-156`). Psalms has 7,683 TWL rows and 485 distinct articles
  (`fixtures/scale/psa/en_twl/twl_PSA.tsv`).
- **What breaks:** A literal port does about 3.7 million string comparisons for each regroup,
  plus the `contains` scan, in wasm on a five-year-old laptop. That is well past 16 ms
  (estimate).
- **Change:** Bucket rows by the `term` index the server already sends, in one pass. The rows
  already arrive in verse order. Keep the differential test, and add a count test on rows
  visited.

### RT-14 (Medium) The aligner round trip is measured where it is cheapest

- **Where:** AD18; AD11; Q-1; §8.3; §4.7 (the `seq` check); A2.
- **Evidence:** A2 measures a wasm `fetch` over loopback in this Linux container. In the product
  every request also passes through the Electron main process: its `onBeforeSendHeaders` handler
  (AD11) or `protocol.handle` (Q-1), on Windows with Defender. HTTP/1.1 allows six connections
  per origin, and the event stream holds one of them for good; OBS pictures and help pages
  compete for the rest. §4.7 allows one edit at a time per aligner.
- **What breaks:** The 30 ms budget can pass in A2 and fail in the product. A Ctrl+click made
  while an edit is in flight is refused or lost unless the UI queues it.
- **Change:** Add the header only on paths that the UI's own `fetch` calls do not cover (not
  `/api/`). Queue aligner clicks in order in the UI. Keep the view computation a pure function
  that can also run in wasm, so Q-13's fallback is a switch, not a rewrite. A2 measures through a
  proxy that adds the header, and a run in Electronite on the reference laptop is an open item
  before B6 starts.

### RT-15 (Medium) The panic path has five gaps

- **Where:** AD30; §6.8; §6.6 "After a reload in the same launch".
- **Evidence and change:**
  1. After the abort trap, closures that wasm registered still fire and trap again, and a
     `location.reload()` scheduled from wasm may never run. Schedule the reload from `boot.js`.
  2. "Three panics within 10 s" also counts those later traps, so one panic can go straight to
     the static callout. Count page loads that ended in a panic, in `sessionStorage`.
  3. §6.6 reopens the open editor after a reload; §6.8 shows the view-error callout instead.
     Restoring the state that panicked loops. Say which wins: the callout, with the buffered
     fields kept dirty.
  4. The static callout leaves buffered text unsent until the window closes, and across launches
     it is lost (AD21). `boot.js` posts each buffer entry as a `Draft` with a plain `fetch`.
  5. If the input handler panics before it writes the buffer, that keystroke is lost. Write the
     text from a `boot.js` capture listener into an entry that wasm creates when the field
     takes focus.
- **Also:** the workspace lints do not catch `get` or `with` on a disposed Leptos signal, which
  panics (`reactive_graph` 0.2.16, `src/traits.rs`), or slice indexing. Async completions should
  use the `try_` forms.

### RT-16 (Medium) B1 is too big and gates all of phase 1

- **Where:** B1; AD7; §4.2 `MemoryBackend`.
- **Evidence:** B1 holds the seam; the seed, which is an importer of tC4 state (pins migration,
  settings, decisions, alignments, OBS) checked byte for byte and field for field (STORAGE
  §13.2); sessions, epochs and the reply cache; `ProjectIndex`; all of `wire`; the counters;
  corpus loading; and the contract read cases. USFM already round-trips for both fixtures
  (`storage:crates/tc-journal/tests/projection.rs:46-63`), but `vrs.json` bytes, the fixtures'
  sidecars, OBS records and the pins migration are not yet proven. One mismatch refuses the
  scenario and blocks every B unit.
- **Change:** Split it. B1a is the seed and its check on the two `.state` scenarios, started now
  as a spike on `storage`. B1b is the seam, sessions, `wire` and counters.

### RT-17 (Medium) A4, A6, B4, C7, E1 and G1 are too big or hide unknowns

- **A4** holds the no-loss core (save machine, buffer, retries, panic hook) beside the transport
  and i18n. PLAN gives no-loss code property tests, crash tests and an adversarial review
  (PLAN:158-170). Split A4a (client, events, cache, i18n) from A4b (save machine, buffer, panic)
  and give A4b that rigor.
- **A6** holds the harness split, a sha per PNG, 62 DOM snapshots, state from the repository, the
  Rust environment, the four-layer compare and the gate. Split the tC4 side from the Rust side.
- **B4** holds the grid, the verse editor, the markers, the section editor with Type and Place
  and its pins, the editing lock and scripted saves. Split the section editor out.
- **C7** holds every §8.3 kind, History, Restore, Detached and the received sheet, including
  screens with no tC4 design (Q-9). Split it by kind, and start after the designs are signed
  off.
- **E1** holds the real sync, the keychain, the sync flow, share, open with sign-in, R13, and
  J11, J17, J24 and J26 with three app-data folders. Split sync from share and open.
- **G1** cannot run in this container (PLAN:151-152), yet it owns the header probe and Q-1, which
  A4 and B6 depend on. Schedule an Electronite spike on Windows before A4 and B6.
- **A9** changes tc-resources seeding and needs `/prepare.html` in ten languages (Q-10). **B8**
  creates the tc-export crate. **B9a** makes tc-door43 stream downloads. Each hides non-UI work
  inside a UI unit; name it in the unit's scope and tests.

### RT-18 (Medium) Every pause in the Understand comment box mints a permanent comment

- **Where:** §6.5 Timing ("user comment: on blur, or 2 s"); AD4's comment rule.
- **Evidence:** STORAGE comments are append-only: each `AddComment` is a new comment
  (STORAGE:1534-1541). tC4 stages each edit of the box in a scheduler that flushes on blur or
  after 2 s (`tC4:src/views/Understand.jsx:72-75, 138-166`). Its box shows the newest note of the
  unit and a count of the others (`:46-56, 174-178`), and it refuses to clear a saved note
  (`understand.cannotClear`).
- **What breaks:** A translator who pauses three times while writing one comment leaves four
  comments. They sync to every teammate and show wherever comments are listed. The design has
  no view rule for the box.
- **Change:** The box shows the newest comment in the unit and a count of the others, as tC4's
  does. Blur sends `AddComment`; the 2 s timer only writes the draft (R15). Keep the clear rule.
  Test it with a teammate's comment in the same unit.

### RT-19 (Medium) `StaleScope` on a write is undefined

- **Where:** §3.5 (`StaleScope`: "drop the reply"); §4.3; §6.2.
- **What breaks:** A write answered with `StaleScope` gets neither an `Ack` nor a failure, so the
  field stays `Saving` or is cleared. It happens with two tabs on one server in development
  and in Playwright (the second tab's open closes the first tab's session, §4.3), after a
  reopen, and after a server restart.
- **Change:** For a write, `StaleScope` becomes `NotSaved(StaleScope)`. The field keeps its text;
  the UI reattaches or reopens and resends with its base token. A browser allows one app tab
  (a `BroadcastChannel` check).

### RT-20 (Medium) Typing causes two fsynced writes a second, one of them on the actor

- **Where:** §6.6 (`Draft` once a second); §4.9 (`lastEdit` once a second).
- **Evidence:** The actor writes `.tc/drafts.json` with `atomic_write` (STORAGE:1752-1753).
  `PrefsWriter` writes `prefs/app.json` 1 s after `lastEdit` changes (§4.9). The 25 ms save budget
  includes the save's own fsync (PERF:74).
- **What breaks:** A blur save that arrives while the actor writes the drafts file waits for that
  fsync. On Windows with Defender, each new temporary file is also scanned.
- **Change:** Ask the storage team for a drafts writer beside the actor (drafts are not journal
  data). Count saves that waited behind a draft write. Record `lastEdit` on blur, or after a
  longer quiet period.

### RT-21 (Medium) `ProjectIndex` costs the whole project at every open

- **Where:** §4.3 "Queries never touch the actor"; §4.4; R1.
- **Evidence:** `ProjectIndex::build` lists every key (`record_keys`,
  `storage:crates/tc-journal/src/fold/reads.rs:82-91`) once per open. The fold already keeps an
  entry per chapter, but it is crate-private (`storage:crates/tc-journal/src/fold/state.rs:242-257`).
- **What breaks:** Until the index exists, `editor/book` waits, or misses 150 ms, right after an
  open. The index also holds a second copy of every key against the 150 MiB steady budget
  (PERF:96).
- **Change:** Build verse and alignment keys from the versification and the anchors, which needs
  no scan. Scan only for comments and detached records. Make R1 a required request (RT-5).

### RT-22 (Low) The open overlay is event-driven; tC4's is timer-driven

- tC4's view shows the overlay from its own 300 ms timer and draws indeterminate bars for the two
  short stages (`tC4:src/views/OpenProgress.jsx:16-40`). §6.3 shows the overlay on the first
  progress event that arrives over the event stream, and STORAGE's phases are all determinate.
  Port the view's timer, and map the phases to tC4's three stages (Q-7).

### RT-23 (Low) Catalogs are fetched on demand

- tC4 imports all ten catalogs (`tC4:src/i18n/index.js:12-21`), so the M16 preview changes
  language in one render. §6.9 fetches each catalog when it is needed, so a frame shows the old
  language, and a failed fetch shows English. The ten files hold about 0.94 MB. Fetch them all
  at boot.

### RT-24 (Low) Virtualized rails need more than one row height

- tC4 never virtualizes a list. Rail rows are one line (`tC4:src/views/Check.jsx:1012`), but group
  headers have another height. The roving tab stop and "focus returns to the card" need their
  row mounted, and the active row must be scrolled to by a computed offset. Use two fixed
  heights, keep the tab-stop row mounted, and test the keyboard contract on Psalms (§6.11).

### RT-25 (Low) Phase-1 Home and when the screen session closes

- `MemoryBackend` writes no derived files, so its `peek` must make conflict counts and summaries
  from its fold. `home_costs_projects_not_history` on memory then proves a path that the product
  does not run. Run it on the store only.
- §4.3 says opening another project closes the screen session. tC4 closes the project when the
  person goes Home, with the "leaving the project" checkpoint (`tC4:src/state.jsx:7286-7328`),
  and J8 asserts that commit. Say that the session closes at Home.

### RT-26 (Low) The navigation gate misses two previews

- tC4 drains every scheduler before the gateway-change preview
  (`tC4:src/state.jsx:4373-4377`). Add the gateway-change and upgrade previews to the gate in
  §6.5.

### RT-27 (Low) Double submits

- A second click on "Keep this version" gets `LookAgain` after the first click succeeded. A second
  "Keep" on a draft offer writes a duplicate change. A second create races the no-replace rename.
  Disable these actions while their request is in flight, and reuse one `RequestId` per dialog
  (§4.8).

### RT-28 (Low) The upload cap misreads STORAGE

- STORAGE's 512 MiB is per zip entry, and 4 GiB is the whole file (STORAGE:3027). §5.5 caps each
  uploaded file at 512 MiB, which refuses a large exchange file received in a browser. Cap
  exchange uploads at 4 GiB.

### RT-29 (Low) Background progress at 10 Hz

- §4.6 sends progress up to every 100 ms. `Train` and `HomeSummary` can stream for minutes to a
  UI that does not show them, and each frame wakes the UI thread while the person types. Send
  progress of jobs no screen shows at 1 Hz, or only to screens that asked for it.

### RT-30 (Low) Bags of booleans

- AGENTS.md asks for enums for states. `AlignerView { armed, stale, changed }`,
  `HomeCard { imported, syncs }`, `Ack.written` and the `ChangeScope` flags are booleans (§3.4,
  §3.6, §3.7). Use enums where a value is a state.

### RT-31 (Low) FlowGate leaves out USB export and receive

- AD31 does not list USB export or receive, so a receive and a Door43 sync of one project can
  run together. STORAGE does not say that this is safe. Add them per project, or have unit X
  prove it.

### RT-32 (Low) Placeholder translations pass the locale test

- A1 writes placeholders for nine languages. `every_refusal_has_a_key_in_every_locale` passes
  while the Arabic screens show English. Make the test fail on a placeholder in a release build
  (Q-10).

### RT-33 (Low) The window-open handler

- tC4 sends every `http` or `https` link that opens a new window to the system browser
  (`tC4:scripts/desktop-main.cjs:128-131`). The rebuild's own origin is `http://127.0.0.1`, so
  a ported handler sends the app's own links (help, downloads) to a browser that has no token,
  which shows a 401 page. Keep same-origin URLs inside the app (§5.7).

### RT-34 (Low) The Chromium that P7 compares

- The tC4 baseline and the Rust captures use Chromium 141 (§7.3). The product ships Electronite
  v37 (AD11), which is built on an older Chromium (Electron 37 uses Chromium 138). P7's pixels
  are approved in an engine that does not ship. Record this as a declared residual, and add one
  Electronite capture run on the release pipeline.
