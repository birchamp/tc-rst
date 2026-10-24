# The application layer of the tC4 rebuild

This is the design for `tc-api`, `tc-app`, `tc-server`, the server side of the `tc4` binary, and
the port of tC4's screens into `tc-ui`. It merges three competing designs (core-first,
parity-first, slice-first) after two judges scored them. Its claims were checked against the
code on 2026-10-17; §13 lists what was read. A red team attacked it the same day (the
"Red team" section at the end). This revision answers every finding: a change made in answer to
finding RT-n is marked *(A-n)*, and "How each point was answered", after the red team, says
what changed for each finding, which parts were done another way, and why.

## Owner's answers (2026-10-17)

- **Q-6, alignment after a verse edit:** adjust it right away and do not flag it for review
  (the owner clarified that "no" meant no review flag, and that leaving it unadjusted seemed
  dangerous). A verse save, and each changed verse of a section save, writes the reflowed
  alignment in the same batch as the text, so both land or neither does: links whose words are
  still in the verse are kept and the others return to the bank, as tC4 does (#213), but
  atomically, where tC4 writes the reflow afterwards. A lossless reflow leaves the verse
  aligned; a lossy one shows it partly aligned, as tC4 does. Requested from unit S1 as R25.
- **Q-24, the Understand comment box:** (c). R24, a record that revises the comment the box
  showed and keeps the earlier text in History, is requested from unit S1.
- **Q-4, projects folder:** `%USERPROFILE%\translationCore4\projects` (or
  `~/translationCore4/projects`), checked against the cloud detection at first launch.
- **Q-26, Windows runs:** the owner runs G0 on a Windows laptop on 2026-10-17 at 16:00 US
  Eastern (20:00 UTC). The release pipeline will have a Windows machine for G1 and G2.
- **Q-2, feedback:** the shell sends it, gated by the server's internet switch.
- **Phase 0 starts now.**
- **A rule for every unit:** where tC4's behavior works against what a feature is for, follow
  what the translator needs and record the difference with its reason (AGENTS.md, "Matching
  tC4").
- **tC4 alignment stamps on import (2026-10-18):** tC4 stamps an alignment with the hash of the
  verse as displayed but checks another hash, so poetry alignments look stale. The importer
  re-stamps an alignment only when its aligned words still match the verse word for word, and
  records each re-stamp in History; any other stays flagged. tC4 imports will be rare, so the
  import stays lean.
- **Single instance (2026-10-18):** translationCore runs as a single instance on a machine, so a
  second launch is blocked; people work on one project at the same time from separate machines.
  tC4 4.0 already works this way (D39, `tC4:docs/DECISIONS.md:458-474`).
- The other questions follow their recommendations in §12.
- **USB sticks and synced folders (2026-10-19).** Projects live in the one projects folder the app
  chooses (Q-4); a translator never picks where a project goes. Projects come in by creating,
  importing, opening from Door43 or receiving work, all into that folder, as in tC4. USB sticks pass
  work along (save work for a teammate, receive work). Only if the projects folder itself is on a
  FAT32 or exFAT drive or in a folder a cloud service syncs does a project open read-only; then
  "Copy to this computer" copies it, with every unsaved text, into the app's local projects folder
  (checked safe), with no folder to choose, and Home lists it. Nothing the person typed is lost.

## G0 results on the reference laptop (2026-10-17)

The owner's Windows session ran `.audit/tasks/g0-windows.md` at `939596a` on a Dell Latitude
7490 (i5-8350U, 4 cores, 8 GB, SATA SSD, Windows 10, Defender on, no SHA instructions).
Evidence: `docs/evidence/electronite-spike-2026-10-17.md` and
`docs/evidence/storage-fold-windows-g0-2026-10-17.md`.

- **AD11 holds.** Every request type the app uses passes under the header filter: document,
  script, style sheet, fetch, image, iframe, web font, CSS `url()`, the event stream, download,
  the print window and a same-origin child window. `/media`, `/download` and `/print` need no
  capability URLs.
- **Requests wait for the shell's main thread.** With that thread blocked 200 ms of every
  250 ms, an `/api/` fetch's p95 rose from 7-11 ms to 202-203 ms in every mode. G1 keeps all
  synchronous work off that thread (AD21's mirror included).
- **Q-13 does not arise.** The aligner click's painted p95 under the filter was 22.4-24.5 ms in
  every pass, alone and while pictures loaded, under the 30 ms budget. AD18 stands.
- **Q-1 is not decided by speed.** Painted p95: filter 22.4-24.5 ms, protocol 22.7-25.5 ms; the
  passes overlap. By §12's rule the custom scheme does not add less than the filter, so the
  filter (AD11) stays. The owner was told the choice is invisible to a translator and left it to
  the team: Q-1 is decided.
- **AD39 holds.** After the server was ended, the shell had a new server in 52-78 ms and Home
  again in 109 ms. Closing the window ended the server in 115 ms. Ending the shell's process
  ended the server through its closed stdin in 43-74 ms, started plainly or detached, and left
  no process behind, so G1 starts `tc4` plainly.
- **AD21 holds.** After a hard kill while typing, `localStorage` had lost the last 10.1 s of
  typing (Chromium first wrote it to disk 5.1 s after typing began); the shell's mirror kept
  every keystroke the server saw, with no torn line. This run's mirror still wrote on the main
  thread; `e2212de` moved it off, and the Linux rerun kept every keystroke too.
- **Graphite works on Windows.** Awami Nastaliq joined (302.9 px wide, as on Linux) in
  Electronite 37.1.0 (Chromium 138). This is a smoke test, not the P8 probe.
- **The driver's own event log can be torn by a hard kill**: the first hard-kill attempt failed
  to parse a half-written last line, and the retry ran. Every reader of a log the shell or
  `tc4` appends to (AD21's mirror, the event logs) skips a torn last line.
- **Storage on the laptop.** The fold opened in 1.86 s with its cache (budget 3 s) and in
  6.3 s without one (10 s); peak working set 270-273 MiB (350 MiB). The Psalms materialize
  (342-401 ms against 300 ms) and the working set right after the open (192-210 MiB against
  150 MiB steady) missed; the owner asked whether those two budgets matter for a person, and
  `docs/BUDGET-REVIEW-2026-10-17.md` answers that.

---

## 0. How to read this document

- **Paths.** Plain paths are in `/home/user/tc-rst`, branch `claude/focused-ride-5uzj7p` (HEAD
  `21fef4a`). `storage:path` is the same path on branch `storage` (HEAD `2dcfe6f`, worktree
  `.claude/worktrees/storage`). `tC4:path:n` is in `/home/user/unfoldingword/translationcore4`
  at `d310bf3`.
- **STORAGE:n** is line n of `storage:docs/STORAGE.md` (4,225 lines). The file is byte-identical
  at `225091b` and `2dcfe6f`. Unit C is editing it; up to line 3,445 it is identical to the copy
  at `6bb20d4`. The copy of STORAGE.md on main is older. Use the storage branch copy.
- **Storage moved during the revision.** Unit C's commit `658ed0b` settles its spec decisions in
  STORAGE.md and tc-journal, among them the two of §9.1, and shifts line numbers after line 204.
  None of its changes touches what this revision relies on: tokens (§2.7), comments (§5.5),
  drafts (§5.14), checkpoints (§5.13) and snapshots (§7.8). STORAGE:n stays the line at
  `2dcfe6f`.
- **ARCH, PERF, PLAN, DELTA** are `docs/ARCHITECTURE.md`, `docs/PERFORMANCE.md`, `docs/PLAN.md`
  and `docs/TC4-DELTA-2026-10-16.md`.
- **Screen ids.** S3–S16 are screens, M1–M18 dialogs, X1–X14 shared pieces, as in the screen map
  that the design run collected from tC4. The screen map is not in the repository; §7.4's table
  names the tC4 component of every id it uses *(A7)*. In this document S1 and S2 always mean
  storage units; the Home and Understand screens are called by name. The 62 baseline screens are
  listed in `parity/tc4-baseline/README.md:271-334`.
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
   window or the whole app loses at most the keystrokes still on their way to the shell *(A-3)*.
   Two limits remain: a power cut can lose the last second, and until R23 a crash can lose up to
   2 s of alignment links (Q-27).
6. **Parity** is checked in four layers (pixels, DOM, meaning, behaviour) against the committed
   tC4 record, plus a tC4 view-state gallery for states with no baseline. A cargo test gates the
   report, as `gallery_record.rs` gates the design system.
7. **Performance** is proved with operation counts on the Psalms corpus and on ten times Psalms,
   from the first data unit on, and with release budgets measured per screen.
8. **Before tc-store exists,** all 62 baseline screens can be built and compared: 57 on real
   fixture data, 5 on scripted job outcomes that are captured again when tc-sync lands. Every
   journey that writes waits for S1 or a later unit.
9. **The work** is 48 units in seven phases *(A-16, A-17)*. Phase 0 (14 units) can start now on
   main; one of them, the Electronite spike G0, must be run on Windows by the owner or a Windows
   runner. Phase 1 has 15 units: the seed spike B1a can start now on a branch cut from
   `storage`, and the other 14 start when unit C merges *(B1a ran after unit C merged, on a
   branch cut from `app`)*. Phase 2 starts with the store adapter
   C0, whose size depends on whether S1 provides R4, R5, R14 and R16 (§9.2) *(it did: C0 is the
   thin adapter)*.
10. **Open questions.** 29 points need the owner (§12). The most urgent is Q-23: ten requests
    this design depends on (R1, R4, R5, R14, R16 and R19 to R23) are in no storage unit's task
    text, and only the owner can add them *(A-5)*. The others with the most weight are feedback sending (Q-2), checkpoints on every
    mode switch (Q-5), alignment reflow after a verse edit (Q-6), the Understand comment box
    over append-only comments (Q-24) and the Windows run of G0 (Q-26).

---

## 2. Decisions

### 2.1 The decisions

| Id | Decision | Why | Rejected |
|---|---|---|---|
| AD1 | Core-first is the base; parity-first and slice-first supply grafts (§2.2 lists each). | Judge 1 scored no-loss highest and picked core-first; judge 2 scored parity and delivery highest and picked parity-first. Their graft lists converge. | Parity-first as the base: one opaque token type, no write capability, no retry dedupe. Slice-first: no retry dedupe, list round trips inside 16 ms budgets. |
| AD2 | `tc-api` holds wire types only and does not depend on `tc-journal`. `tc-app::wire` converts. | The UI must never reach storage types (ARCH:88), and the wire must not move when the journal changes. | Re-exporting journal types through tc-api. |
| AD3 | Two opaque token types, `ShownToken` and `ReviewToken`, with no constructor, no accessor and no conversion to or from text in the UI's build: there a token is made only by deserializing JSON and its text leaves only by serializing it. Code that re-encodes one through JSON on purpose could still relabel it, so the types stop slips and the MAC, which covers the kind (R19), stops misuse *(A1)*. The seam keeps them apart too: `mint` returns an `EditToken` and `mint_review` a `ReviewToken`, two tc-app types around tc-journal's one token, and `tc-app::wire` converts each to its own wire type only. The wire forms carry the prefixes `st1.` and `rt1.`; parsing refuses the wrong prefix. tc-journal puts the purpose under the MAC (R19, provided), so the actor also refuses a review token in a save and an edit token in a `Resolve`, and tc-app's parse refuses a relabelled one before any command is built (B1b). Tokens never enter the DOM; the DOM test looks for the two prefixes. *(A-7)* | STORAGE:756-765: a conflict's edit token lists only the shown candidate; `Resolve` needs every head (STORAGE:1610-1616). tc-journal has one `ShownToken` for both (`storage:crates/tc-journal/src/token.rs:54`) and its MAC does not say which kind it is, so a review token used in a save would supersede every head and end the conflict unseen. | One `ShownTokenText` for both; separate types in tc-api only, which stop at the wire. |
| AD4 | Every write carries a `RequestId`. The browser buffer entry keeps the id with the text that was sent (`sent`) beside the current text. A per-session `ReplyCache` (1,024 entries, memory only) keys each id with a digest of the request body: a retry of a finished request gets the first reply, a retry of a request still running waits for its reply, and a retry whose body differs gets `ApiError::RequestReused`, never the cached `Ack`. A field is clean only when `Ack.saved` equals the NFC form of its current text. A new `ServerInstance` means the cache is gone: the UI goes Home and resends nothing. Each buffered entry is offered at the next open of its project, beside the entries of `drafts.json`, unless the stored value already equals its text; a comment entry is dropped when the newest comment at its target has its text (tC4's rule, `tC4:src/data/journal/journalingStore.ts:3048-3058`). *(A-2, A-3)* | A retry rebuilt from the same token after a lost reply would otherwise make a person conflict with their own save (STORAGE:1422-1428, 1447). A retry matched by id alone could carry newer text and be answered with the older text's `Ack`, or run twice while the first attempt waits on the actor. `AddComment` has no token and mints a new id each time (STORAGE:1536). After a restart the old page cannot reach the server on its new port (§5.2), so an in-place resend cannot happen. | An echo-only request tag; matching retries by id alone; resending in place after a server restart. |
| AD5 | `Ack` arrives only after B3 and says exactly what was stored: `written`, the NFC text, the new tokens, new conflicts, `ViewSeq`, and the disk warning. | STORAGE:1415-1428. | An optimistic "Saved". |
| AD6 | One seam to storage. Before S1, `MemoryBackend` executes no command: it answers `Unavailable`, or a scripted outcome under the `test-hooks` feature. | No second copy of STORAGE's B1–B2 rules. Save behaviour in the UI is still testable before S1. | Core-first's temporary command builder, whose deletion depended on S1 publishing a pure `build_batch`. |
| AD7 | The fixture seed is STORAGE §13.2's state seed, with its checks, written as `tc_app::backend::seed::tc4_state_seed`. When unit I lands, `memory_seed_equals_tc_import_seed` runs and the copy is deleted. The check is §4.2's, and its mismatches on the parity scenarios are in `docs/evidence/state-seed-check-2026-10-18.md` (B1a). | §13.2 (STORAGE:3433-3442) requires byte-for-byte USFM and `vrs.json` and field-for-field sidecars. A checked seed cannot make a screen look right for the wrong reason. | A new `tc-import` crate now (unit I owns it); a seed without the check. |
| AD8 | The open's progress is tc-store's reporter, forwarded unchanged. tc-app's own reporter times only tc-app's jobs. | STORAGE:1870-1891 gives the clock to the store, and S1's `open_shows_determinate_progress_by_300_ms` tests it. | A tc-app clock for the open and an invented `Preparing` phase. |
| AD9 | Typed `POST /api/v1/<service>/<name>` with JSON. HTTP 200 carries `ReplyOf<E>`, the enum `Reply<E::Ok>`: `{"type": "ok", "value": …}` or `{"type": "err", "value": <ApiError>}`, the crate's one enum form, not serde's `{"Ok": …}` for a `Result` *(A1)*. Statuses are used only below the API: 401 token, 403 Host or Origin, 409 API version, 413 a body over its cap (8 MiB for an API body, the flow's cap for an upload), 503 while the server closes (§5.1). | One exhaustive Rust match in the UI. A proxy or auth failure cannot be mistaken for a domain refusal. | 422 for every error; a result type per area without one error enum. |
| AD10 | Events come on one SSE stream, `GET /api/v1/events`, read with `fetch` and the token header. Frames carry a sequence number; a ring of 4,096 frames serves reconnects; a gap gives `Resync`. | Works the same in the shell, in Playwright and in a developer's browser, whether or not header injection reaches it. | `EventSource` (needs injection or a cookie); WebSocket (no headers; opaque to Playwright); polling. |
| AD11 | ARCH:42-44 stays as written. The shell adds `X-Tc-Token` through `session.webRequest.onBeforeSendHeaders`, with a URL filter that lists only the server's paths outside `/api/`: `/`, `/ui/*`, `/help/*`, `/media/*`, `/download/*`, `/print/*` and `/prepare.html`. The UI's own `fetch` calls set the header themselves, so no API request or event-stream read runs the filter's JS listener. Every request the window makes, `/api/` included, still crosses Electron's request proxy on the main process's UI thread, with or without the filter, so a busy main thread delays every API call: the shell keeps synchronous work off that thread (AD21, G1), and G0 measures the delay. Playwright uses `extraHTTPHeaders`. Only debug builds accept a one-time `?dev-token=` cookie. Capability URLs are the fallback if Electronite misses a request type (G0 probes every type). *(A-14)* | No amendment to the fixed security decision; images, iframes, downloads and the print window carry the header, and the aligner's round trip does not pay for the filter's JS callback. It still waits for the main process's UI thread, as every request in an Electronite window does; Q-1 and Q-13 are decided from G0's measurements of that round trip. | A production launch cookie (core-first); public static files plus capability URLs (slice-first); a filter over every request. |
| AD12 | A project is named by `ProjectRef`, its folder under the projects root. `ProjectId` is data. Scopes, sessions, Home cards and summaries are keyed by folder. `prefs/<projectId>.json` stays as STORAGE §3.7 lists it. | Two imports of one tC4 project get one id (STORAGE:3374, T4), and a copied folder is expected (STORAGE:2015-2036, `FolderCopied`). Two folders must be two cards. | Keying by `ProjectId`. |
| AD13 | Home never folds a project. A card reads `metadata.json`, `.tc/sync.json`, `ingredients/tc/conflicts.json` and a summary file stamped by the names of the pack and pending files. A missing or stale summary is rebuilt in the background from the derived USFM files and `conflicts.json`, with no lock and no fold, one project at a time, cancelled when the person opens any project. | PERF rule 1. tC4 computes the drafted percent from USFM too (`tC4:src/state.jsx:2742-2752`). Reading derived files needs no `.tc/lock`, so a background job never holds the lock against a person's open (STORAGE §6.1 O1). | A background read-only fold (it holds the lock and a whole project's memory); a lockless "peek" open requested from S1 (not needed). |
| AD14 | Settings opened from Home show values from the derived files. Save opens the project with progress, checks that the fold's projected values equal what the dialog showed, mints tokens for those values, and saves. A difference shows "This project changed. Look again." The open goes through the registry's single open per folder (AD38), so a card click during the save shares the session instead of finding the lock held. *(A-8)* | The dialog does not fold a whole Bible to appear. The equality check keeps I4: the minted heads stand for exactly what was shown. | `session.open{purpose: Settings}`. |
| AD15 | A bulk resolution is one batch, confirmed with its count first, or it is not offered. Until the command exists (R7), the review offers only the choices the command set carries, and "Later". | STORAGE:1619-1626. | One `Resolve` per conflict: partial results, and tc-app would reimplement the device-set rule. |
| AD16 | Switching chapter sends no request. `editor/book` returns the book head with the focus chapter; `editor/chapters` returns the rest. Events re-query only the chapters they name. *(B4a: as built, `editor/chapters { at, book }` answers the translation of every chapter of the book in one read, so a chapter switch reads nothing of the translation or the book; a change in the book reads it again, a cost of one book (`translate_text_touches_one_book`). The source panes and the helps stay B3's reads per chapter (`editor/sources`, `helps/passage`), so a chapter seen for the first time reads those two, with tC4's place record; `chapter_switch_sends_no_request` holds the switch to exactly that.)* | PERF:73, 75. A 16 ms switch cannot include a round trip and a render. | A request per chapter. |
| AD17 | The check rail arrives once per check session as compact rows with flags and group keys. After that it is patched row by row: an `Ack` carries the rows its write changed, and a `Changed` event names the rows in `ChangeScope.records`. A cached open sends only the rows changed since the `ViewSeq` of the rail the UI still holds *(A-12)*. The UI masks, orders and groups with `tc_api::model::rail`, which buckets rows by the term index in one pass *(A-13)*. `tc-app` proves it equals `tc_check::rail::groups` for every filter and mode on Titus and Psalms. Category groups are ordered after the mask, by first appearance among shown rows. | `tc_check::rail::groups` filters, then sorts, then orders categories by first appearance among the kept rows (`crates/tc-check/src/rail.rs:113-172`); a group order computed before the filter is wrong under a filter. Filter changes send no request (PERF:78). The Psalms tN rail is about 1.2 MB of JSON (estimate), so a refetch per decision or per cached open misses the 20 ms budget (PERF:77). tC4's and tc-check's grouping filter every row once per term (`tC4:src/views/Check.jsx:889-898`, `rail.rs:138-156`): about 3.7 million comparisons on Psalms TWL's 7,683 rows and 485 terms. | Precomputed group starts; a copy of tc-check's rail in tc-ui; windowed rail requests; refetching the rail on every change. |
| AD18 | The aligner runs on the server. `tc_align::Aligner` holds the selection (`click_bank_word`, `selection`, `is_armed`, `crates/tc-align/src/aligner.rs:337-358`). Every click is a round trip; the UI queues clicks in order and sends the next when the previous reply arrives, so a fast click is never refused or lost *(A-14)*. `align/open` on a verse whose session is live returns that session. The server owns the alignment save: 2 s after the last link change, on leaving the verse, when the session is replaced or closed, and before the quit checkpoint; its state feeds the save indicator and the close guard *(A-4)*. The view is built by a pure function of the `Aligner` that depends only on tc-align and tc-api, so Q-13's fallback is a switch, not a rewrite *(A-14)*. A2 measures the loopback round trip, also through a header-adding proxy; G0 measures it in Electronite on Windows before B6 merges *(A-14)*. | One source of truth for the selection; unsaved links survive a reload; nobody but the server knows that links are unsaved. | Armed words in UI state; tc-align in wasm (needs an exception to ARCH:88, Q-13); a UI timer for the alignment save. |
| AD19 | A verse edit writes nothing on its alignment. The aligner opens the verse with the reflowed value, and a lossless reflow is saved on leaving the verse. This is how `tc-align` is built (`crates/tc-align/src/reflow.rs:9-17`, `aligner.rs:85-130`). | STORAGE §0 item 4 and §5.2 item 4 (STORAGE:1482-1485). | Writing the reflow at the verse save (tC4 #213); owner choice, Q-6. |
| AD20 | A per-field save state machine ports tC4's `SaveScheduler`, FR-32 included, with tC4's timing per field (§6.5). Verse, layout and story text save on blur or after 2 s idle. The section editor writes only on Save; blur and the timer write only its draft *(A-1)*. A user comment saves on blur; the timer writes only its draft *(A-18)*. A check comment saves on Done and on leaving the item *(A-10)*. Every decision change saves at once. Alignments are saved by the server (AD18). | tC4 flushes a decision at once (`tC4:src/state.jsx:5683-5685`); STORAGE's "1 s after the last change" (STORAGE:1522) is a ceiling this meets. tC4's section card writes nothing until Save (`tC4:src/views/SectionEditor.jsx:4-6`), and each section save is a passage batch that can join, split or renumber verses. STORAGE comments are append-only (STORAGE:1534-1541), so a save per pause would leave a comment per pause. | Decisions after 1 s; one blur-or-2 s rule for every text field. |
| AD21 | The browser draft buffer is `localStorage`, written synchronously on every input. In the shell, the preload also sends each change to the main process, which appends it as one line to `drafts-buffer.log` in app data at once and fsyncs the log at most once a second *(A-3)*. The appends and the fsync run off the main process's UI thread (asynchronous `fs` or a worker), because every request the window makes waits for that thread (AD11). An append never rewrites earlier entries, so a crash during a write can lose only the line being written. At each launch the shell reads the last line per field and server run *(A4b fix, §5.7)*, hands those entries to the page, and replaces the log atomically with the entries still unsaved. A `Draft` command follows at most once a second. Within a launch the buffer restores fields after a reload. After any crash, and across launches, the shell's mirror and `.tc/drafts.json` are offered at the next open of the project. Entries are keyed by folder and project id *(A-9)*. | An origin is scheme, host and port, and every launch has a new port (ARCH:12). Chromium may hold `localStorage` writes in memory before it commits them, so even a stable origin would not keep the last keystrokes through a crash; a file the main process has written survives a crash of any process in the operating system's cache. | Reusing the previous launch's port (fragile; the port may be taken); relying on a stable origin (Q-1) for no-loss. |
| AD22 | Open sessions live in the server. `app/hello` returns them, so a reload after a panic resumes without reopening the project. A live aligner session keeps its unsaved links, and the server saves them (AD18) *(A-4)*. The screen that panicked is not restored: the reload shows the view-error callout in its place, and its buffered fields stay dirty until they are saved or offered *(A-15)*. | A reopen costs 3–10 s (PERF:89-90) and would drop the live aligner session. Restoring the state that panicked can panic again. | Reopening on reload; restoring the editor that panicked. |
| AD23 | `use_app()` returns tC4's names: `s` for state and `actions` for actions. Server data in `s` is a read-only view of the query cache; only view state is writable; drafts live in the save machine. | A builder ports `Check.jsx` top to bottom, and each value has one owner. | One writable signal per tC4 field; a three-input model with no tC4 names. |
| AD24 | Logic both sides run lives in `tc_api::model`: the section buffer, the drag range, text truncation, rail masking and grouping, change-scope matching, the print parts of a preview page. Differential tests in `tc-app` compare each with its domain twin. | `tc-app` depends on both `tc-api` and the domain crates, so a separate test crate adds nothing. | A `tc-crosscheck` crate; ports in `tc-ui` with twins elsewhere. |
| AD25 | The UI asks the internet question before it sends any request whose endpoint declares `NET`. The server still refuses with `NeedsInternet` if the switch went off meanwhile, and the UI asks once more. | tC4's "Not now sends nothing" (ARCH:49-54); the server enforces the switch (`crates/tc-door43/src/internet.rs:19-45`). | Hand-wired kinds at call sites. |
| AD26 | Uploads are raw request bodies, one file each, streamed to app data. "Receive work from a file" takes a path from the shell in the desktop app. | No multipart parser; bytes never pass through wasm memory; a 4 GiB exchange file is not copied. | Multipart; uploads for USB files. |
| AD27 | Language suggestions run on the server (`home/languages`), a port of `tC4:src/data/languageSearch.ts`. | The 1 MB language list stays off the wasm heap, and the ranking has one copy. | Shipping `langnames.json` to the UI. |
| AD28 | The port of `renderArticleBlocks` (`tC4:src/data/articles.ts:142`) lives in `tc-helps`. | Pure logic belongs in a pure crate. | tc-app. |
| AD29 | Help pages are vendored from tc-website `cdfdf9b7…` with a per-file sha list and a licence record. *(A5, D3: the 180 files of `tc-help/` are in `crates/tc-ui/help/`, copied byte for byte by `crates/tc-ui/vendor-help.sh` (tC4's sparse fetch); `crates/tc-ui/help-pages.txt` lists each with its git blob id in tc-website, under the repository, the commit and the licence record, and `tests/help_pages.rs` checks every byte, that nothing is unlisted, and that no page loads anything from another origin. `build-app.sh` copies them to `<ui>/help/`, outside the hashed folder, where `tc4 serve` serves `/help/*`.)* | They are not in tC4's git (`parity/tc4-baseline/README.md:18-19, 41-46`); `vendor-tc4.sh` lists files with `git ls-tree` of a tC4 commit (`crates/tc-ui/vendor-tc4.sh:29-33`). | Extending `vendor-tc4.sh` with blob ids. |
| AD30 | A panic hook stores a crash marker and reports the error; `boot.js`, not wasm, reloads the page and counts the page loads that ended in a panic in `sessionStorage`. Three within 10 s stop the reloads; `boot.js` then draws tC4's view-error callout as static DOM and posts each buffered entry that has a `DraftField` as a `Draft` with a plain `fetch` (none for an entry with a save in flight, §6.8, A4b second review). A capture listener in `boot.js` writes each keystroke to the buffer before wasm handles it. *(A-15)* | wasm aborts on panic (`Cargo.toml:28`), after which closures it registered trap again and a reload it scheduled may never run; tC4 shows `data-testid="view-error"` (`tC4:src/App.jsx:163-190`). *(A4b: and those closures can also panic again on the state the abort left, so the hook keeps the first panic; a listener the WebAssembly registered can hold the reload, so `boot.js` owns `beforeunload` too; §6.8.)* | Reload forever; no recovery; counting traps. |
| AD31 | Sync, gateway change, share flow, sign-in and import each run one at a time across the app (`FlowGate`). USB export and receive run one at a time per project, never beside a sync of the same project. *(A-31)* *(A8: a second sign-in waits for the one in flight, as tC4's `signInFlight` makes it, instead of answering `Busy`: sign-ins and resumes take one lane in the account service, because each sign-in deletes and re-creates the token `translationCore` and two at once could delete each other's. `FlowRequest::SignIn` stays for the flows that must not run beside a sign-in.)* | DELTA:660-661; tC4's flags (`tC4:src/state.jsx:176-192`). STORAGE does not say that a receive and a sync of one project may run together. | — |
| AD32 | Operation-count tests start with the first data unit (B1b), on `MemoryBackend` folds of tc-corpus Psalms and ten times Psalms. | Count rows do not wait for S1 or P. | Counts only after P. |
| AD33 | The app track builds `tc-export`: the print model early (B8), the producers after S2 (D2). | No storage unit owns it; the Community Checking baseline needs the print model. | Waiting for an owner. |
| AD34 | Checkpoint on every mode switch, as tC4 does (`tC4:src/state.jsx:4031-4038`), until the owner and the storage team rule (Q-5). Before R17 is adopted, unit P measures save latency while a checkpoint runs (R9), and the storage team is asked to keep the long part of a checkpoint off the actor (R21). If the 25 ms save budget still fails, Q-5 offers one checkpoint per mode switch at most every two minutes. *(A-11)* | tC4's J8 asserts it (`tC4:e2e/j08-resume.spec.ts:194`); STORAGE lists only leaving Translate (STORAGE:1723). A checkpoint is an actor step with a 500 ms budget for a whole Bible (STORAGE:1725-1735, 2720; PERF:92), so a checker who switches after each decision would wait behind one each time. | Silently following either. |
| AD35 | Test hooks exist only under the cargo features `test-hooks` and `fixture-backend`. `release_has_no_test_hooks` checks the release binary. | No hook may ship. | Runtime flags. |
| AD36 | Teammate changes in parity scenarios are written only through tc-journal's encoders (phase 1) and the store's sealing API (from C1a), never by hand. | The scenario must exercise the real format. | Hand-made packs. |
| AD37 | An open editor holds its value and its token as one pair, taken when it opened, and taken again when an `Ack` leaves the field clean *(A4b)*. A refresh replaces the pair only while the editor has had no input and no composition since it last took it; otherwise the field keeps its text and token and shows the "changed while you were typing" notice. A `Changed` whose cause is the field's own request shows nothing. Nothing is ever written into a focused field, and texts are compared in NFC. *(A-6)* *(A4b fix: which of two pairs is newer is decided by their open first, then their `ViewSeq`, which counts per open (§6.3).)* | A value and a token that come apart make the next save conflict with itself, or revert a change unseen (STORAGE:1449-1453). The `Ack` and the `Changed` event of one write may arrive in either order (§5.4). The server knows the cause because each `StoreCommand` carries the request it answers, and the store adapter joins the store's `Committed` to that command (C0). NFC reorders Hebrew points and Arabic marks (STORAGE:1445), and writing into a focused field moves the caret and breaks an IME composition. | Following the query in an open editor; never refreshing an open editor. |
| AD38 | `SessionRegistry` runs one open per folder at a time; a second caller waits for it and shares the session. The screen session closes when the person goes Home, with tC4's "leaving the project" checkpoint. *(A-8, A-25)* | A second open finds `.tc/lock` held by this same `tc4` and is refused, with `tc4` itself as the holder (STORAGE §6.1 O1). tC4 closes the project at Home (`tC4:src/state.jsx:7286-7328`), and J8 asserts that commit. | Waiting only for a running leave-checkpoint; closing the session only when another project opens. |
| AD39 | The shell supervises `tc4`. When `tc4` exits without being asked, the shell starts it again and loads Home at the new address; the new page reads the shell's buffer mirror (AD21). `tc4` treats a closed stdin as a quit. The shell starts `tc4` with `detached: true` and `windowsHide: true`: on Windows, Node's plain `spawn` puts the child in libuv's job object, which ends it the moment the shell's process ends, before it can read its closed stdin and quit (G0's "end the shell" rows measure both). A `tc4` that finds `app.lock` held waits up to 10 s for the old process to finish closing, then names it (`app-lock.json` beside the lock records its pid, §5.1). *(A-3)* | Every launch has a new port, so the old page cannot reach a restarted server (§5.2). An orphaned `tc4` would hold `app.lock` and `.tc/lock` and refuse the next launch. When the shell dies, the pipe to `tc4`'s stdin closes. | A quit endpoint for orphans (it would need the old token); restarting in place. |

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
| 30 | Oversized units | Parity-first's B5 and B9; the red team's B1, A4, A6, B4, C7, E1 and G1; this revision's own check found C1. | Split into B5a/B5b and B9a/B9b; then B1a/B1b, A4a/A4b, A6a/A6b, B4a/B4b, C7a/C7b, E1a/E1b and C0/C1a/C1b; G1's unknowns move into the spike G0; the non-UI parts of B8 and B9a move to phase 0 as A10 and A11 *(A-5, A-16, A-17)*. |
| 31 | Start screens before unit C finishes | Judge 2's optional graft. | Not by default: unit C is already in progress. Kept as an option in Q-16. |
| 32 | Proof that the harness split changed nothing | Parity-first said the record pins hashes. | It pins sizes and byte counts (`parity/tc4-baseline/baseline.test.mjs:45-59`). A sha check is added (A6a). |

---

## 3. tc-api

### 3.1 Rules for the crate

- Builds for the host and for `wasm32-unknown-unknown`. CI adds
  `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`.
- Dependencies: `tc-core` (which already serializes `BookId`, `VerseSpan` and `Reference`,
  `crates/tc-core/src/books.rs:227-237`, `lib.rs:93-145`), `serde`, `thiserror`; `proptest` and
  `proptest-derive` only under the feature `proptest`.
- Features *(A1)*: `server` gives the token types their constructor and accessor (§3.4); tc-app
  enables it and tc-ui never does. The token types have no other conversion to or from text,
  so the UI's wasm32 build proves no UI code builds, reads or relabels a token except by
  re-encoding it through JSON, which the server's MAC refuses (R19, AD3).
  `test-hooks` adds the `test` service (AD35). `proptest` derives `Arbitrary` for every
  wire type, for the round trips here and for property tests in tc-app and tc-ui.
- No I/O and no `std::time`. Times are `WallMs(i64)`.
- Every identifier is a newtype whose `Deserialize` validates its format. Requests use
  `#[serde(deny_unknown_fields)]`, and so does every struct and enum a request holds, so an
  unknown key is refused at any depth (a test adds one to each object of each request).
  Struct fields are camelCase. An enum of unit variants is a
  camelCase string (`"rtl"`); every other enum is adjacently tagged,
  `{"type": "<camelCase variant>", "value": <data>}`, with no `value` for a unit variant. *(A1)*
  The reply body is such an enum too (`Reply`, AD9), and a test checks every golden file
  against this rule. A value that is one identifier written as text is a string whatever it
  holds, as `BookId` is (`"TIT"`); so is `CheckPlace` (`"PSA 60:front"`), which a rail repeats
  thousands of times.
  This design said "internally tagged" before unit A1 built the crate: serde's internal tagging
  cannot carry a variant that holds a string or a number (`NotSaved::Disk(String)`,
  `Who::Username(String)`), and it fails when such a value is written, not when the code
  compiles; and an enum inside an enum (`ApiError::Refused(Refusal::Sync(…))`) would put two
  `type` keys in one object. Adjacent tagging keeps the tuple variants of these sketches.
- Every enum on the wire is closed. A `proptest` round trip covers every type, every request
  and every reply.
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
pub struct StoryNo(u8);              // an OBS story, 1 to 50 (tc-helps `ObsRef`) *(A1)*
pub struct RefPart(String);          // a chapter or verse part as tC4 writes it: `1`, `9-10`, `front`, `intro`, `3,5`, `1a` *(A1)*
```

`ProjectRef` refuses only a name that could leave the projects root on some system the app runs
on: an empty name, one that holds `/`, `\`, NUL or `:`, or one made only of dots and spaces.
On Windows, joining a name with a drive prefix (`C:`, `c:..`) replaces the whole path and
`name:stream` names an NTFS stream, so no Windows folder holds a colon; and Windows drops the
trailing dots and spaces of a path's last segment, so `...` or `.. ` would name the root or its
parent. Every other name is kept, spaces, leading dots and every script included, so no project
folder is unreachable from the UI; the registry reports a folder whose name fails the check
instead of skipping it. Where the file system ignores letter case or a trailing dot, two names
can name one folder, so the registry opens only a folder it lists, by the name it listed. *(A1:
the first text kept colons, which let `c:..` leave the root on Windows.)* `LocaleId` is checked against
`tc_api::ids::LOCALE_IDS`, which a test compares with tc-ui's catalogs. *(A1)*

`RefPart` reads every part tc-check's `RefPart` holds, and a little more. tc-check's part is the
journal's identity grammar (`tc_journal::keys::RefPart`): NFC text that is not empty and holds neither `:` nor
`|`. This one takes any text that is not empty and holds no `:`, so a check the server sends
always reads, and the server refuses at its boundary a part that tc-check cannot hold. It has
no length limit, because a check whose place the UI cannot read drops out of the rail and shows
its decision as not saved. A request is bounded where the server reads its body (A2: 8 MiB, and 413
above it), not field by field. *(Merge of A1: tc-check's part took the journal's grammar after A1 was written.)*
*(A1: the first build typed a check's place as `tc_core::Reference`, which holds a chapter of at
least 1 and numeric verses, and the first fix of `prefs/recordPlace` capped a verse at 256 bytes.)*

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
    const NET: Option<NetKind>;            // the UI asks the internet question first (AD25)
    type Ok: Serialize + DeserializeOwned;
}
pub trait Write: Endpoint {                 // every Command and StartJob
    fn request(&self) -> RequestId;
    fn at(&self) -> Option<&Scope>;          // the open project; None for a write of no project (B1b)
}
pub enum Reply<T> { Ok(T), Err(ApiError) }  // {"type":"ok","value":…} | {"type":"err","value":…}; into_result() *(A1)*
pub type ReplyOf<E> = Reply<<E as Endpoint>::Ok>;

// endpoint_table!(callback) hands `callback!` one row per endpoint *(A1)*:
//     query "app/hello": app::Hello => app::HelloView;
//     command [SignIn] "account/signIn": account::SignIn => account::AccountView;
// tc-api implements `Endpoint` from it (and `Write`, from the request's `request` field, for every
// `command` and `job` row, so a write without a RequestId does not compile; `at()` comes from a
// `scoped!` or `unscoped!` line each write row needs, B1b) and builds
// `ALL: &[EndpointInfo { path, kind, net }]`. test_hook_table! holds the `test` rows and exists
// only with `test-hooks`; ALL includes them then. tc-server builds its routes from the same
// tables, and tc-app must implement `Handle<E>` for each row, so a request without a handler
// does not compile.
pub const API_VERSION: u32 = 1;           // header `X-Tc-Api`
```

*(B1b: `Write::at()` lets tc-app check a write's scope before it picks a reply cache (§4.3,
§4.4): the session's cache keeps project-scoped writes, so dispatch must know the scope first, and
a stale-scope refusal is never kept under its request id, since the UI resends the same id in the
new session. `a_write_names_its_scope_exactly_when_its_json_has_at` in tc-api's golden test checks
every write row. No wire shape changed.)*

tc-server routes `POST /api/v1/{path}` for exactly the rows of these tables (the `test` rows only
with `--test-hooks`) and hands the body to the backend, whose typed dispatch parses it (A3); a path
outside the tables is 404 before the backend sees it. *(A2)*

`NetKind::Upload` stays because tC4's catalogs have the key, but no endpoint uses it: ruling Q2
removes the Upload changes dialog (DELTA:1137, PLAN:49-50).

### 3.4 Shown values, writes and acknowledgements

```rust
// token.rs. Deserialize checks the prefix ("st1." for ShownToken, "rt1." for ReviewToken), then
// the base64url alphabet and length. No Display; Debug prints "ShownToken(..)". The constructor
// `from_body(&str)` and the accessor `body()` (tc-journal's token text) exist only with the
// `server` feature, which tc-ui never enables. Serialize and Deserialize are written by hand, so
// no `TryFrom<String>`, `Into<String>`, `FromStr`, `AsRef<str>` or `Display` exists for a UI to
// use; a test checks that none does. *(A-7, A1)*
pub struct ShownToken(String);   // STORAGE:745-767; JSON: "st1.<body>"
pub struct ReviewToken(String);  // every head of a record (STORAGE:1610-1616); JSON: "rt1.<body>"

// shown.rs
pub struct Shown<T> { pub value: T, pub token: ShownToken, pub marker: Option<ConflictMarker> }
pub struct ConflictMarker { pub conflict: ConflictRef, pub versions: u8 }  // "2 versions · Review"
pub struct PersonLabel { pub who: Who, pub computer: Option<Computer> }
pub enum Who { Mine, Username(String), DisplayName(String), Teammate(u16) }  // STORAGE:2686-2691
pub enum Computer { This, Other(u16) }  // when two computers' versions share a label (STORAGE §8.5)
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
    ReadOnly(ReadOnlyReason),             // NeedsNewerApp (O4), UnsafeDrive and CloudSynced
                                          // (O5), SecondWriter; C0 adds OpenOnAnotherComputer
                                          // (O1) and FolderNotWritable (a write the open
                                          // needed failed, STORAGE §6.1)
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
    ToolNotReady,                               // a check tool's pin is not ready: the UI shows the picker (B5a)
    RequestReused,                              // a RequestId came back with another body: a bug,
                                                // reported; the field stays dirty and saves anew *(A-2)*
    NotSaved(NotSaved),                         // writes only
    Refused(Refusal),                           // a domain refusal in tC4's words
    NeedsInternet(NetKind),                     // nothing was sent
    NeedsSignIn(SignInReason),                  // NotSignedIn | TokenRejected | PrivateRepository (Q3)
    Busy(Flow),                                 // a one-at-a-time flow runs (AD31)
    Unavailable(Feature),                       // the backend cannot do this yet (before S1, X, I; A5 built the feedback attachment, A8 the account rows and deleted `Feature::Account`)
    Internal { report: ReportId },              // a bug; "Ask for help" carries the report
}

/// Closed, grouped by family. Each family enum implements `Message`: `code()` (tC4's code where
/// one exists), `key()` (the catalog key), `vars()` (placeholder values) and `detail()` (the
/// diagnosis tC4's `failureText` puts before the sentence, when the sentence has no
/// placeholder for it). *(A1)*
pub enum Refusal {
    Open(OpenRefusal),          // NewerFormat, NotAProject, Unreadable, project lock held (names the holder)
    Create(CreateRefusal),
    Import(ImportRefusal),      // refusal.import.*, importer.*
    Export(ExportRefusal),      // refusal.export.* (5 keys)
    Sync(SyncFailure),          // Blocked(SyncBlock) | BeforeIntake(SyncRefusal) | AfterIntake(SyncRefusal) (Q8)
    Share(ShareRefusal),
    OpenDoor43(OpenDoor43Refusal),
    SignIn(SignInRefusal),      // signIn.error.*
    Resources(ResourceRefusal),
    Exchange(ExchangeRefusal),
    Feedback(FeedbackRefusal),
    Prefs(PrefsRefusal),        // a failed app-language save keeps the dialog (tC4:e2e/app-language.spec.ts:263)
}
// DELTA:740-800; tC4 SYNC_REFUSALS (tC4:src/views/SyncResult.jsx:14-51)
pub enum SyncBlock {            // found before intake (STORAGE §9.1 S0, §9.2 N1, N2) *(A1)*
    NotShared, ProjectMismatch, NeedsNewerApp { major: u16, min_reader_minor: u16 }, Tc4TeamRepository,
}
pub enum SyncRefusal {          // a step that can fail before or after intake
    Offline, NotSignedIn, AuthFailed, NoWriteAccess { server_message: String },
    RefRejected { server_message: String }, Busy { attempts: u8 }, RemoteMissing, Unreachable,
    ReadOnly, Io { reason: String },
}
```

The families hold the refusals their flows meet so far *(A1)*: Open (the three above), Sync,
Share and Sign-in (STORAGE §9.6, tC4's `refusalForSignIn`), Export (tC4's five codes), Import (tC4's
two `refusal.import.*` codes; F1 adds the importer's), Open from Door43 (`name-exists`,
`no-journal`, `removed`), Create (`wizard.nameInUse`, `wizard.error`), Resources (tC4's sources,
fix, upgrade and gateway messages), Exchange (STORAGE §10: damaged, newer format, verify failed,
not written, unreadable), Feedback (tC4's five `feedback.reason.*`, which the shell answers, Q-2)
and Prefs (`language.saveFailed`). A unit that builds a flow adds the refusals it meets, with
their keys.

A sync refusal maps to a message by DELTA §3.15.2's table. After intake *(A1, PLAN Q8)*: an
unreachable or missing remote shows a rebuild key that says the received work was kept and
yours was not sent, instead of `cloneFailed`'s "Nothing changed"; an `Io` or a `RefRejected` shows
"received and kept, but the sync stopped. Ask for help." *(B10, Q36)*; every other refusal keeps its key and adds a second
sentence, `SyncFailure::kept_note()`, "Your teammates' changes were received and kept." So no
failure after intake says "nothing changed". `NotShared`, `ProjectMismatch`, `NeedsNewerApp`
and `Tc4TeamRepository` are found before intake (STORAGE §9.1 S0, §9.2 N1, N2), so they are
`SyncBlock`s, which only `SyncFailure::Blocked` carries: three of their sentences say "Nothing
changed", and the type cannot pair them with "after intake". *(A1: the first sketch,
`{ refusal, after_intake: bool }`, could say "Nothing changed" and "received and kept" in one
banner.)*

The UI builds a sentence the way tC4's `failureText` does: diagnosis, then the recovery sentence
(`tC4:src/state.jsx:1781-1788`). "Ask for help" shows when the sentence contains `feedback.ask`
(`tC4:src/state.jsx:1792-1793`).

The sync toast and the share dialog are the exceptions, and they follow tC4: they show the code's
sentence alone ("The diagnosis never shows", DELTA §3.15.2; `tC4:src/views/SyncResult.jsx:47-51`,
`tC4:src/views/modals/ShareDialog.jsx:28-29`). So no `SyncFailure` and no `ShareRefusal` has a
`detail()`. Door43's words reach a banner only through a `{reason}` placeholder (`PushFailed`);
the `server_message` of `NoWriteAccess` and `CreateRejected` stays in the refusal
for the operation report (`ReportLog`) that "Ask for help" attaches. *(A1: the first build showed
both before the sentence, which tC4 never does.)* *(B10, Q36, tC4 #724 item 6: where tC4's toast
shows "The sync stopped: {reason}", the rebuild's shows one fixed sentence, `syncResult.error.stopped`
before intake and `keptStopped` after, so neither Door43's words nor an I/O reason, which can name a
file on the translator's computer, reaches the toast; `RefRejected` and `Io` keep them for the
operation report. Any other error of the sync job shows `syncResult.error.unexpected`. Test
`io_reason_renders_without_the_path`.)*

Tests in `tc-api`:

- `every_refusal_has_a_key_in_every_locale`: each variant's key resolves in all 10 vendored
  catalogs or in the rebuild key files, with matching placeholders. A rebuild key whose text is
  still English is listed in `assets/i18n-rebuild/<id>.pending.json`; it passes in debug builds
  and fails the release check `release_has_no_pending_translation`, so an Arabic screen cannot
  ship in English unnoticed (Q-10). *(A-32)*
- `no_two_sync_refusals_share_a_message` (DELTA:797-799): two refusals share a key only where
  DELTA §3.15.2's table joins them (`NotSignedIn` and `AuthFailed`; `Unreachable` and
  `RemoteMissing`; `RefRejected` and `Io` in `syncResult.error.stopped`, and after intake in
  `keptStopped` *(B10, Q36)*), and no two keys read the same *(A1)*.
- `no_tc4_code_without_a_variant`: every `refusal.*` code in the vendored catalogs has a variant
  or a line in a pinned "vendored and unused" list (DELTA:790-797). The list, with a reason per
  code, is `UNUSED_TC4_CODES` in `crates/tc-api/tests/refusals.rs`; a code both produced and
  listed fails too.
- `rebuild_keys_never_shadow_vendored_keys`.
- `release_has_no_pending_translation` passes in debug builds and fails under
  `cargo test --release`; the release pipeline (G1, G2) must run
  `cargo test --release -p tc-api --test refusals` before it builds an installer *(A1)*.
  The rebuild key files are in `crates/tc-ui/assets/i18n-rebuild/` (a README there says how to
  add a translation); tc-ui's `assets_hold_only_the_vendored_files` leaves that folder out.

### 3.6 Events, jobs and progress

```rust
// event.rs
pub struct EventFrame { pub seq: u64, pub instance: ServerInstance, pub event: Event }
pub enum Event {
    Resync,                                              // a gap: read every live query again
    Internet { on: bool },
    Account(AccountView),                                // { user, kept, notice, server, keychain } (A1; A8: the Door43 host and whether a keychain works)
    Job(JobView),                                        // { id, kind, project, state }; `jobs/get` answers the same view (A1)
    ProjectsChanged,                                     // create, import, join
    HomeCard { project: ProjectRef },                    // a summary was computed or changed
    Session { at: Scope, event: SessionEvent },
    Startup(StartupState),                               // Ready | Preparing(StartupProgress { done, total }) | Failed { reason } (A1)
}
pub enum SessionEvent {
    Opened, Closed,
    Mode(SessionMode),                                   // Writable | ReadOnly(reason)
    Changed { view: ViewSeq, scope: ChangeScope, cause: ChangeCause },
    Notice(NoticeView),                                  // STORAGE:680-735 anomalies and notices
    Checkpoint(CheckpointOutcome),                       // Committed | Unchanged | Failed { reason } (A1)
    MaterializeBlocked { paths: Vec<String> },           // "can't update TIT.usfm…" (STORAGE §5.10)
    Received(ReceivedSheet),                             // STORAGE:2681-2684
    Suggest { testament: Testament, status: SuggestStatus },
    AlignSave { aligner: AlignerId, state: AlignSave },  // the server's alignment save (AD18) *(A-4)*
}
pub enum ChangeCause { LocalSave { request: RequestId }, Intake { from: Vec<PersonLabel> },
                       OutsideEdit { path: String }, Upgrade, Resolve, Import, System }
pub enum ChangeScope { Parts(ChangeParts), Everything } // a list over its cap gives Everything *(A-30)*
pub struct ChangeParts {                                 // tc-app's form of the fold's Delta (STORAGE:628-629)
    pub chapters: Vec<ChapterRef>,                       // { book, chapter }
    pub books: Vec<BookId>, pub stories: Vec<StoryNo>,   // books: changed as a whole (presence, header,
                                                         // layout), never tc-journal's Delta.books (B1b)
    pub checks: Vec<CheckListRef>,                       // { tool, unit }; an open rail asks `check/rows { since }`
                                                         // for the rows that changed; never the whole rail *(A-12)*
    pub records: Vec<RecordChange>,                      // { record, by: Option<PersonLabel> }; capped at 256
    pub aspects: Vec<Aspect>,                            // Pins | Settings | Meta | Conflicts *(A-30)*
}
pub enum QueryKey { Chapter(ChapterRef), Book(BookId), Story(StoryNo), CheckList(CheckListRef),
                    Record(RecordRef), Aspect(Aspect) }  // a query names one or more (A1)
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

*(B1b: `tc_app::wire::scope::change_scope` builds `ChangeParts` from a fold `Delta`. tc-journal
fills `Delta.books` when a key of a book becomes active for the first time (a first decision on a
check, a first alignment of a verse, a new comment), not when the book changed as a whole, and
`QueryKey::CheckList(Book(b))` depends on `books`: copying it would refetch every rail of the book on
a first decision, against AD17 and C4's `decision_refetches_no_rail`. So `books` comes from
`BookPresence`, `BookHeader` and `BookLayout` keys only. A comment's place is in its value, not its
key, so the event pump reads each comment of a delta once to name its chapter, check list or story;
a comment it cannot read makes the scope `Everything`. A `Versification` key, a key whose
`RecordRef` would pass 4,096 bytes, and a story number tc-api cannot carry make it `Everything` too.
The mapping is one `match` over every `RecordKey` kind with no catch-all. A `Verse` or
`ChapterFront` key also names the book's tN and tW lists in `checks`: a check is assessed against
its verse's text, so a text edit can invalidate checks whose records the delta does not hold, and
an open rail that missed it would show an invalidated check as valid. The delta does not say which
checks moved, so both lists are named, which costs one `check/rows { since }` per open rail of the
book (AD17); C4, which re-assesses a verse's checks, may narrow it to the lists it re-assessed. An
`Alignment` key names none: no check reads alignments.)*

What unit A1 built *(A1)*: the frame, `Event`, and the session events `Opened`, `Closed`, `Mode`,
`Changed`, `Checkpoint`, `MaterializeBlocked` and `AlignSave`; `ChangeScope::bounded` turns a
change past the record cap into `Everything`. `Notice`, `Received` and `Suggest` carry view
types of later screens and come with the units that show them (C1b and D1, E2, B6), as do the
outcomes `Opened` (B3, which builds the open job over B1b's `SessionRegistry::open_screen`), `Synced` (B10) and `PlanReady` (C8) and the phases of share, exchange,
import and export; `JobOutcome::Finished` ends a job whose end is its whole result (a seed, a
download, a summary, a training). The change parts are named structs, not tuples, so their JSON
names its fields. A checkpoint failure carries its reason, not a `Refusal`: no refusal family
describes one, and the UI shows tC4's "Checkpoint not saved" with Retry and the reason as its
diagnosis (`tC4:src/App.jsx:63-67`). An event that carries a value whole must also have a read
that returns it, or a gap loses it (§5.4). `QueryKey::depends_on` errs toward a refresh: a chapter
follows its own edits and its book; a book any of its chapters; a check list its rows, its unit
and the pins (the unit of a book's list is the book; of a story's list, that story: a story's
session is scoped to the open story, as a book's is to the open book, tC4's `deriveObsItems`, and
`ChangeParts.stories` names a story whether it was added, removed or edited, so any change of the
story's own counts); a record, a story and an aspect only themselves. *(A1: the first build named a
check list by tool and book, which had no form for the OBS stories' lists. B7: it named the stories
as one list; a story's list is its own, `CheckUnit::Story`.)*

No screen shows `Train` or `HomeSummary` progress. Their `Job` frames go out once a second at
most; only their end (`Done`, `Failed`, `Cancelled`) goes out at once. The other jobs keep the
100 ms rate of §4.6, because a screen shows them. *(A-29)*

**`Startup`** *(A9)*. `hello.startup` is exact from the first request, because `App::start` plans
the first-launch seeding from receipts alone before it returns (§4.4): `Ready` when there is
nothing to copy or check, `Preparing { done: 0, total }` when there is, and `Failed { reason }`
when the plan cannot be made, a copy on disk cannot be trusted, or the bundle lacks an install its
manifest lists (§4.6). Without `--bundle` nothing is seeded and the state is `Ready`. `total` and `done` count **content files**, as the golden
`hello.json` already shows (`1200 of 3739`). Declared difference from tC4, which counts its 16
resources (`tC4:scripts/desktop-bootstrap.cjs:236`): one resource is up to 1,021 files, each
written with an fsync, so a count of resources would stand still for many seconds on the
reference laptop, and PERF rule 4 asks for a count that grows. It moves as each file is written;
reading and checking an install comes before its first write and moves nothing (§4.6). The words
are tC4's. The `Startup` frames follow the `Seed` job's own reports (none before 300 ms, then at
most one per 100 ms, §4.6). The state moves only forward: a `Preparing` count never falls,
`total` never changes within a run, and `Ready` or `Failed` is final for the run, so the job's
last `Running` frame, which the reporter sends after the job's own end, can never show
"preparing" again. The ring keeps only the newest `Startup(Preparing)` frame (§5.4). A repeat
launch plans nothing to do, starts no job, sends no `Startup` frame and adds no report.

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
// (tc_helps::highlight_quote, crates/tc-helps/src/highlight.rs:305).
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
                              pub progress: RailProgress, pub citation: Option<ResourceCitation>,  // *(B5a D5)*
                              pub dropped: u32, pub empty: Option<CheckEmpty>, pub start: u32 }
pub struct RailRow { pub index: u32, pub check_id: String, pub place: CheckPlace, pub title: String,   // *(A1, B5a D5)*
                     pub flags: RowFlags,                        // dot, decided, invalidated, invalid, bookmarked, commented
                     pub term: Option<u32>, pub category: Option<Category>,
                     pub marker: Option<ConflictMarker> }
// tc-check's CheckTarget and Collection on the wire. A check is not always at a numeric chapter:verse:
// tC4's resources put a Psalms superscription at `60:front`, a note on several verses at `5:1,3,8,12`
// (`5:2-3` for a span), a book's introduction at `front:intro` and a chapter's at `1:intro`, and an
// OBS note at a story and a frame (frame 0 is the title). `locatorOf` shows `c:v` or `story:frame`.
// JSON: the text tc-check prints for a target, `"PSA 60:front"` or `"OBS 3:4"`, and read back into the
// enum at the boundary. Text, not an object per row: the Psalms tN rail has about 8,000 rows. *(A1)*
pub enum CheckPlace { Verse { book: BookId, chapter: RefPart, verse: RefPart },
                      Frame { story: StoryNo, frame: u16 } }
pub enum CheckUnit { Book(BookId), Story(StoryNo) }                // what one check list covers; a story's decisions are tc-check's `Collection::Stories` *(B7)*
pub struct CheckListRef { pub tool: Tool, pub unit: CheckUnit }  // JSON: {"tool":…,"unit":{"type":"book","value":"TIT"}}
pub struct CheckItemView { pub index: u32, pub view: ViewSeq, pub check_id: String,   // *(B5b, as built:)*
                           pub place: CheckPlace, pub title: String,
                           pub note: Vec<ArticleBlock>, pub helps_dir: Option<Direction>,
                           pub article: CheckArticle,            // { link: ArticleLink, title: Option<String>, category }
                           pub original: OriginalView,           // Ready { testament, verse } | CrossFrame | Unavailable | Error
                           pub gateway: GatewayView,             // NoPane | CrossFrame | Pane { pane, text: GatewayText }
                           pub target: TargetWords,              // { words, selected, verse: Option<ShownToken> }
                           pub decision: Shown<DecisionView>,    // the token is the shown decision's stored key's
                           pub notices: Vec<CheckNotice> }       // Invalidated { count }

// align.rs: the session lives on the server (AD18); every edit returns the whole view *(B6: as built)*.
pub enum Opened { Ready(Box<AlignerView>), Blocked { why: Blocked, at: Option<OpenVerse> } }
pub struct AlignerView { pub aligner: AlignerId, pub seq: u64,   // an edit names the `seq` it was made on
                         pub place: AlignPlace, pub testament: Testament,
                         pub target_dir: Direction, pub orig_dir: Direction,
                         pub original: String, pub gateway: Option<GatewayLens>,  // the lens exists or not
                         pub bank: Vec<Word>, pub selection: BankSelection,       // None | Armed(Vec<u32>), by place in `bank`
                         pub cards: Vec<Card>,                    // top, placed, suggested, gateway label
                         pub placed: u32, pub total: u32, pub done: bool, pub mark_refused: bool,
                         pub suggest: SuggestView,                // switch, status, asked, standing, suggesting
                         pub freshness: Freshness,                // Current | Stale (the verse text changed)
                         pub dropped: Vec<DroppedLinkView>,
                         pub status: RailStatus,                  // the open verse's row on the rail
                         pub save: AlignSave }                    // Saved | Unsaved | Saving | Failed(NotSaved) *(A-4, A-30)*
// `align/edit` takes one `AlignEdit`: ClickBank { word, click }, PlaceSelection, PlaceWords, Unplace, Move,
// Merge, Split, MarkValid, Accept, AcceptAll, RejectAll. A chip is named by its word, never its place, so a
// click made before the reply to the one before it still means the chip the person saw.

// home.rs *(as B2 built it)*
pub struct HomeView { pub cards: Vec<HomeCard>, pub unusable: Vec<UnusableCard>, pub resume: Option<LastEditView> }
pub struct HomeCard { pub project: ProjectRef, pub name: String, pub tag: Known<String>, pub dir: Direction,
                      pub kind: ProjectKind,                      // Bible | Stories
                      pub door43: Door43Card,                     // Local | Shared { place: RepoPlace { owner, name, server } } *(A-30, A1, B2 D5)*
                      pub books: Known<Vec<BookTileView>>,        // { book, drafted: Known<u8>, conflicts: u32 }
                      pub stories: Option<StoriesView>,           // { drafted: Known<u8>, stories: Vec<StoryTileView>, recent }
                      pub conflicts: Known<u32> }                 // verse conflicts; Unknown or 0: no chip
pub struct UnusableCard { pub name: String, pub reason: UnusableReason }  // implements Message: home.unusable.*
pub struct LanguageSuggestion { pub name: String, pub own: Option<String>, pub code: Option<String>,
                                pub door43: Option<String>, pub dir: Direction }  // Q35: `code` is one a project can hold
```

*(B2: the card carries no `origin`: tC4's "Imported" badge marks the project an import of this
page made (`s.importedRepo`, view state F1 sets), not every imported project. The in-progress
count is not a field: the UI counts it from the tiles, and states it only when every tile is
known.)*

Every optional part is a `Slot`, `Known` or `Result`, so "could not be read" never looks like
"absent" (tC4's D30 rule).

Unit A1 built the state enums of this section *(A-30)*: `BankSelection`, `Freshness` and
`AlignSave` (`tc_api::align`), `ProjectOrigin` and `Door43Card` (`tc_api::home`), with the parts of
the Check rail that an `Ack` carries (`RailRow`, `RailRowPatch`, `RowFlags`, `Dot`, `Category`,
`Tool`, and the names of a place and a list, `CheckPlace`, `CheckUnit` and `CheckListRef`, in
`tc_api::check`). A test reads every check of `fixtures/check/psalms-vectors.json` (31,429 in
its 18 cases), the odd forms of `reference-forms-vectors.json`, every Titus tN note and every
OBS tN note back from an `Ack`. The screens' units build the views around them. `Door43Card::Shared`
has no `sync: SyncAction`: by the owner's ruling Q2 every shared project, Bible or OBS, offers
"Sync with Door43", and "Syncing…" comes from the running job, not from the card.

### 3.8 Shared pure models (`tc_api::model`)

| Model | Port of | Twin checked by a differential test in tc-app |
|---|---|---|
| `SectionBuffer`: parse, serialize, can drop, drop pin | `tC4:src/views/sectionDraft.js:147,244,263,282` | `tc-app` turns the buffer into `SectionValue`s with the same code; proptest `parse(serialize(b)) == b` |
| `drag_range(base, from, to)` | `tC4:src/views/Check.jsx:567-666` (#737) | `tc_check::selection::from_indices` on the saved indices |
| `ArticleBlock::take_spans` | `tC4:src/data/articles.ts:127-136` | tc-helps' block renderer |
| `rail::mask` and `rail::groups` | `tC4:src/views/checkFilters.js:15-28`, `Check.jsx:889-905`, with the same output but not the same algorithm: rows go into buckets by `term` index (or category) in one pass over the masked rows, which already arrive in verse order *(A-13)* | `tc_check::rail::groups` for every filter and mode on Titus and Psalms tN and tW; `rail_groups_visit_each_row_once` counts rows visited on Psalms TWL (7,683 rows, 485 terms) |
| `rail::patch` | — | applying `RailRowPatch`es equals a fresh rail, as a proptest over random decisions *(A-12)* |
| `print_parts`: a page's print parts under the page setup (drop cap or heading, verse numbers, a verse split by a page break) | `tC4:src/views/print/PrintBook.jsx:44-58`; the preview splits the pages in the UI (`PrintPages.jsx`) | `tc_export::book::print_parts` on every preview page of `fixtures/export/print-vectors.json` (B8) |
| `QueryKey::depends_on` | — | the server's own invalidation tests |

### 3.9 Endpoints by service

`[kind]` marks an endpoint that declares `NET`. "Job" marks a `StartJob`.

| Service | Endpoints |
|---|---|
| `app` | `hello` → `{api, build, instance, internet, account, sessions, prefs, startup}`; `quit`; `clientError` |
| `prefs` | `setLocale`, `setDraftUnit`, `setAlignSuggestions`, `recordPlace` |
| `net` | `state`, `set` |
| `account` | `state` (sends nothing to Door43), `signIn` [signIn], `cancelSignIn { signIn }` (the sign-in to cancel, by its request id, A8), `resume` [checkSignIn], `signOut`, `orgs { language }` [share] (the share dialog's first step: the user and each organization with `canCreate` and `recommended`, B9b), `profileLink` [profile], `repoLink { url }` [repoLink] (an address on `--door43` only, B9b), `helpLink` [helpLink] |
| `home` | `list`, `card`, `versifications`, `languages` *(B2; `nameFree` is not built, C6 D2)* |
| `session` | `open` (job), `drafts { buffered }` (the draft offer: `buffered` carries the browser and mirror entries for the folder, §6.6), `close`, `leave { mode \| book }`, `focus` (outside-edit check, STORAGE §11.1), `notices`, `dismissNotice`, `keepDraft`, `discardDraft`, `copy` (the copy offer of a read-only project, §6.4), `otherComputerClosed` (STORAGE §5.0), `retryCheckpoint` *(C1b: the offer is a command of its own, `drafts { at, request, buffered, mirror }`, which the UI sends once an open has its scope and again after a reload while the offer stands, so `open` carries no `buffered` and B3's open job is the open alone. It takes every unsaved text into `drafts.json` as an offered entry before it answers, and answers the questions and the buffer slots to `settle`; `mirror` says the lines include the shell's mirror (§6.6, the draft machine). An offer carries the tokens and text Keep sends back. `keepDraftAsVersion` is gone: the store decides from the token whether Keep writes over it or with no parents (R20), and the offer's `keep` tells the UI which words to use. `copy` and `otherComputerClosed` are the banner's two actions; `copy` goes into the local projects folder where the projects folder itself made the project read-only (Q-30, C6 D9), and is refused with `CreateRefusal::CopyUnsafe` only where that folder is unsafe too)* |
| `editor` | `book`, `chapters`, `sources`, `original`, `saveVerse`, `saveLayout`, `saveSection`, `draft`, `discardDraft`, `addComment` |
| `helps` | `passage`, `story`, `article`, `storyArticle` *(B7: `story` is the passage of a story, the story as the chapter and the frame as the verse; its articles are `storyArticle`, which needs no book)* |
| `story` | `list`, `open`, `saveFrame`, `saveRef` *(B7 builds `list` and `open`; the saves are C3's)* |
| `check` | `picker { unit }`, `open { since }`, `rows { since }`, `item`, `article`, `decide` *(B5a: no `close`, B5a D8; B5b: `article` reads a check's article from the resource its list derives from, and `decide` names its check by place and row id, B5b D3 and D4; C4: `item`, `article` and `decide` take a story's list as a book's, so B7's `storyItem` is gone, C4 D3)* |
| `align` | `index`, `open`, `edit`, `keepReflow`, `flush` (save now; the navigation gate drains with it), `suggest`, `close` *(B6: Mark valid, accept, accept all and reject all are edits of the one aligner, so each is one `AlignEdit`, not an endpoint)* |
| `conflicts` | `summary`, `list`, `detail`, `propose`, `resolve`, `history`, `restore`, `dismissDetached`, `releaseBatch` |
| `resources` | `sources`, `catalog` [sources], `download` [sources] (job) *(B9a: each names the open project, since every way into Get source texts opens it first)*, `checkUpdates` [sources], `planUpgrade` (job), `applyPlan`, `planGateway` (job), `fixState` *(B5a)*, `fixFetch` [fix] (job), `fixSideload`, `fixRepin` |
| `settings` | `viewFromHome`, `save` *(B9a: tC4 opens Project settings only from a Home card (#505), so `view`, the dialog inside an open project, has no way in and is not built; `save { request, project, shown, direction, font }` is C6's, D5)* |
| `create` | `bible`, `obs`, `addBlankBooks`, `addUsfmBooks` (commands), `reviewUsfm { file }` (query: what one uploaded USFM file is) *(C6 D1: commands, not jobs)* |
| `door43` | `share` [share] (job), `sync` [sync] (job), `open { repo, name }` [open] (job, `Join`, ending `Created { project }`) *(B9b: `shareTargets` is `account/orgs`, and `shareCheck` is gone: tC4's check step asks Door43 nothing, and its facts, the name and the books, are the card's)* *(B10: `sync { project }` asks the sign-in first, runs one at a time (AD31) and ends `Synced(SyncResultView)`)* |
| `exchange` | `exportFile` (job), `identify`, `receive` (job) |
| `import` | `review`, `versions` [importVersions], `run` (job) |
| `export` | `book { at, book }`, `stories { at }` (the Community Checking preview's reads, B8 D1), `preflight`, `run` (job) |
| `feedback` | `compose` (the shell sends the report, Q-2) |
| `jobs` | `get`, `cancel` |
| `test` (feature `test-hooks`) | `quiet`, `echo`, `hold`, `release`, `crash`, `fieldStore`, `fieldLog`, `fieldDraft` *(A4b)*, `keychain` *(A8)*, `clock`, `script`, `door43`, `inspect`, `writeTeammate`, `counters`, `reset`, `lastEdit` *(B2)* *(B10: `script` sets how the next `door43/sync` of a project ends, replacing what was set, and queues a `door43/open` failure; teammates' changes come with a scripted sync, so `writeTeammate` is not built)* |

**The endpoints of phase 0** *(A1)*, which unit A1 defined with golden JSON, are `app/hello`, `quit`,
`clientError`; all four of `prefs`; `net/state`, `set`; `jobs/get`, `cancel`; `account/state`,
`signIn`, `cancelSignIn`, `resume`, `signOut`, `profileLink`, `helpLink`; `feedback/compose`; and
the hooks `test/quiet`, `echo`, `crash`, `hold`, `release`, `fieldStore`, `fieldLog`, `reset`, and `fieldDraft`, which A4b added. Each later
unit adds the rows of its screens to `endpoint_table!` (B9b adds `account/orgs` and `repoLink`). `test/echo { size }` answers a list of Hebrew words whose
reply is at least `size` bytes long, by at most one word (A2: a reply of a chosen size, for the
loopback measurement of §8.3).

`prefs/recordPlace` carries one observation of the place in a book or a story, as tC4's
`rememberPlace` makes it (`tC4:src/state.jsx:3821-3845`): a mode, chapter or tool left out keeps
the stored one, and the verse (a text: `4-5` for a bridge, `intro` or `front` for a check note,
as tC4's `PlaceRecord` keeps `number | string`) or a story's frame is kept, cleared (a chapter
click) or set. A verse is a `RefPart` and a story a `StoryNo`. *(A1: the first build typed the
verse as a number and could not clear it.)*

Two rows changed when A1 built them. `account/link` became one endpoint per question, because an
endpoint declares one `NET` and the UI asks before it sends: `profileLink` [profile] and
`helpLink` [helpLink] now, `repoLink` [repoLink] with B9b. `feedback/send` is gone: the owner
answered Q-2 with (a), so the shell sends the report and the server only composes the attachment
(§5.7).

### 3.10 Versioning

- `API_VERSION = 1`, paths under `/api/v1/`, header `X-Tc-Api: 1`.
- The UI and the server ship as one build. `index.html` is `no-store`; every other UI file is in
  a folder named by a hash, `ui/<build>/`, where `<build>` is 16 hex digits of a SHA-256 over every
  file's path and bytes. *(A4a: a hashed folder, not hashed file names, because tC4's CSS imports
  `tokens/*.css` and its fonts by relative URL, which a renamed file would break.)* A
  `build.commit` in `hello` that differs from the wasm's compile-time commit forces one reload.
  Both read `tc_api::app::BUILD_COMMIT`, which is the environment variable `TC4_BUILD_COMMIT` at
  build time, or `dev` when it is not set; the release pipeline sets it for the server and the
  wasm build alike. *(A4a: so do a `hello.api` that differs from `API_VERSION` and any reply that
  is `ApiError::Version`, whether it answers a call or the open of the event stream. The server
  checks the version of every `/api/` path, and the stream's open is the page's first request,
  so it meets the refusal before `hello` does; so does a reconnect in the middle of a session.
  The page keeps a marker in `sessionStorage`, `tc4.reloadedFor`, naming the server's commit and
  API version it reloaded for. The same mismatch after that reload shows the boot error naming
  both builds, instead of reloading for ever; a different mismatch gets one reload of its own. A
  page whose `sessionStorage` refuses the marker does not reload at all: with no marker the next
  load would reload again, for ever, so it shows the boot error at once. One function,
  `tc_ui::app::boot::build_action`, decides all of this for `hello`, for a call and for the
  stream, and writes the marker before it says to reload. A session that meets the refusal in
  the middle, in a reconnect or a call, does the same: the reload happens, or the boot error
  replaces the app on the page at once, naming both builds. When several requests find out
  together the first decision stands (`boot::Fate`), so there is no second reload, no boot error
  while the page leaves and no reload of a page that has stopped, and the event stream stops
  reconnecting behind the page's new state.)*
- A new `ServerInstance` means the server restarted. The UI drops every scope, sends nothing
  again, keeps every buffered entry and goes Home; the entries are offered when their project
  next opens (AD4). In the shell this happens on a fresh page, because the shell reloads the
  window at the restarted server's address (AD39). *(A-3)* *(A4b fix: the page has one restart
  path, `Runtime::restarted`, and it runs once per run, whichever part of the page finds the
  run first: a frame of the new run, a `hello` that names it (after a `Resync`, `reseed` alone
  would have hidden it), or the `hello` a lost reply's resend asks first (§6.5). The resend used
  to reset only the save machine, so the page never showed the restart and a second report then
  dropped fields of the new run. Until A5 goes Home, a field whose editor is still on screen is
  kept as `Held(Hold::Restarted)` *(A5, D15: A5's `App` goes Home at once on a new
  `ServerInstance`, with no gate, since the new run has no reply cache and a held field cannot
  be saved there; it watches `ServerState.instance`, not `restarted`, which stays true after the
  first restart; editors that unmount close their fields, and the no-loss probe page stays a root
  of its own, outside the App, so A4b's restart cases keep their editors on screen)*: what is typed into it goes on into the old run's entry,
  beside the save that run never answered, which this run never restores, sends or drafts, and
  closing the editor forgets the field. Dropping it left typing in the DOM alone while the
  indicator said "saved".)*
- Persisted formats carry their own version: the browser buffer's item prefix `tc4.drafts.v2:`
  (one item per field, §6.6), the shell's `drafts-buffer.log` with the same entry format, one
  `BufferLine` per line, and `schemaVersion` in each app-data file. Unknown keys are kept on
  rewrite. *(A4b: the types are `tc_api::buffer`, shared by the UI, the shell's reader and the
  server; an entry accepts keys it does not know and a rewrite keeps them, while a request
  refuses unknown keys at any depth, so `session/open { buffered }` carries entries as raw
  lines.)*

---

## 4. tc-app

### 4.1 Module map

| Module | Owns |
|---|---|
| `lib.rs`, `config.rs`, `app.rs` | `App::start(AppConfig, AppDeps)`, `App::call` (bytes in, bytes out: the one entry tc-server routes to), `App::events`, `App::quit`, `App::sessions`, `App::read` (the request guard of queries), `App::counters`; `AppConfig { app_data, projects: Given(dir) \| Default, door43: ServerUrl, build, test_hooks }` (the UI folder is tc-server's); `AppDeps` holds what a test replaces: the clock, the keychain, the folder check, the app-data files, the project store (`StoreChoice::OnDisk`, tc-store over the projects folder, which `App::start` builds once the folder is chosen and checked; `StoreChoice::Given` for the memory backend in tests) and, with `test-hooks`, the process ender *(B1b: `AppDeps.sessions` became `AppDeps.store`)* *(C0: `NoProjects` is gone. The store is built inside `App::start`, after Q-4's folder is chosen, because that is the first moment the folder is known and tc-store makes a projects folder that is missing: a folder someone named is refused first, and `..AppDeps::production()` builds nothing)* |
| `dispatch.rs`, `handlers.rs` | `Handle<E>` for every row of `endpoint_table!`, the internet check (AD25), which a write hands to the reply cache so it applies only to an id not yet sent, crash points on every write (§5.8) |
| `replies.rs` | `ReplyCache` and `BodyDigest` (AD4); one per session for project-scoped writes, and one app-wide for writes of no project (B1b) |
| `folders.rs` | the cloud-folder check of STORAGE §5.0, a copy of tc-store's kept until R28 *(C1a, D6: tc-store's own check calls every folder under a home that has Dropbox's settings folder `~/.dropbox` synced, and reads the process's environment, so the first-launch rule could not be tested for another computer)* |
| `clock.rs` | the injected wall clock; timers use `tokio::time` |
| `hooks.rs` | the `test` service (feature `test-hooks`, §5.8) |
| `backend/{mod,memory,scenario,store,seed}.rs` | the seam (§4.2); `scenario.rs` reads a parity `.state` folder into the memory backend (B1b) |
| `session.rs` | `SessionRegistry` (one open per folder, AD38), `ProjectSession`, `SideLease`, `WriteLease`, epochs, the request guard, a `ReplyCache` per session (`replies.rs`), the event pump *(B1b: it replaced phase 0's `sessions.rs`)* |
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
    /// The projects, and each folder that holds a project's files (project.json, metadata.json,
    /// ingredients/ or .tc/) but cannot be opened as one, with its reason (C0: `Listing
    /// { projects, unusable }`, so no folder that holds someone's work is left off Home, AD12;
    /// a declared difference from tC4, §4.5). No lock, no fold.
    fn list(&self) -> Result<Listing, StoreError>;
    /// What a Home card shows besides its summary: metadata.json as a document, the Door43
    /// remote of .tc/sync.json, and the summary's `SummaryStamp` (§4.5; C1a, D8:
    /// `StoreBackend::summary_stamp`, one listing and one stat). No lock, no fold. The memory
    /// backend builds the same document from the fold's project details *(B2)*.
    fn peek(&self, folder: &ProjectRef) -> Result<ProjectPeek, StoreError>;
    /// What a summary is made of: each book's USFM, a story project's stories and
    /// resources.json, and tc/conflicts.json; the store reads its derived files, the memory
    /// backend renders them through `CountingSnapshot`. No lock. Blocking: the `HomeSummary`
    /// job calls it, and it stops at its cancel between files *(B2)*.
    fn summary_files(&self, folder: &ProjectRef, books: &[BookId], watch: &SummaryWatch) -> Result<SummaryFiles, StoreError>;
    /// O1–O13 (STORAGE:1787-1868). Blocking: called on a job thread. Progress goes to `progress`
    /// from tc-store's own reporter thread (AD8).
    fn open(&self, folder: &ProjectRef, progress: ProgressSink) -> Result<Arc<dyn ProjectHandle>, OpenFailure>;
    /// Staging folder, initial changes, no-replace rename (R2). `Unavailable` before S1.
    fn create(&self, spec: NewProject, progress: ProgressSink, cancel: CancelToken) -> Result<ProjectRef, CreateFailure>;
}

pub trait ProjectHandle: Send + Sync {
    fn facts(&self) -> ProjectFacts;                                // id, kind, device, mode
    fn snapshot(&self) -> Snapshot;                                 // the newest one: { fold, seq }, an Arc clone
    fn parse_edit(&self, token: &tc_api::ShownToken) -> Result<EditToken, TokenError>;    // the MAC, then the kind
    fn parse_review(&self, token: &tc_api::ReviewToken) -> Result<ReviewToken, TokenError>; // (R19, in tc-journal)
    fn submit(&self, cmd: StoreCommand) -> BoxFuture<Result<StoreReply, CommandError>>; // after B6; never blocks
    fn subscribe(&self) -> UnboundedReceiver<StoreEvent>;           // Committed { seq, delta, cause } | Mode
    fn offered_drafts(&self) -> Vec<OfferedDraft>;                  // O12 (STORAGE:1857); Verified | Unverifiable *(A-9)*; C0 (the list); C1b (keep, discard, the offer)
    fn leave(&self, what: Leave);                                   // materialize trigger (STORAGE:1639-1640); B3/D1
    fn checkpoint(&self, reason: CheckpointReason) -> BoxFuture<CheckpointOutcome>;
    fn counters(&self) -> StoreCounters;                            // C0
    fn close(self: Arc<Self>) -> BoxFuture<Result<(), StoreError>>; // materialize at close, release the lock
}
impl Snapshot {                                                     // tokens come from the snapshot a screen read
    pub fn mint(&self, key: &RecordKey, displayed: Option<ChangeId>) -> Result<EditToken, TokenError>; // *(A-7)*
    pub fn mint_review(&self, key: &RecordKey) -> Result<ReviewToken, TokenError>;                     // every head
}
impl StoreCommand {                                                 // built only from the token kind it needs
    pub fn save_verse(shown: EditToken, words: String) -> Self;     // each unit adds the constructors it sends
    pub fn resolve(conflict: ConflictId, shown: Vec<ReviewToken>, choice: Choice) -> Self;
    pub fn requested_by(self, request: RequestId) -> Self;          // C0: the origin its event names (AD37)
}
pub enum Origin { Request(RequestId), Server }                     // C0
pub enum StoreCause { Intake { devices }, Command { origin: Origin }, System }  // Command: C0
pub struct StoreReply { pub tokens: Vec<(RecordKey, EditToken)>, pub new_conflicts: Vec<ConflictId>,
                        pub stored: Stored, pub seq: u64, pub changes: Vec<ChangeId> }  // Stored::WrittenNotDurable(reason)
/// tc-journal has one `ShownToken` for both kinds (`storage:crates/tc-journal/src/token.rs:54`).
/// These two wrappers have no public constructor outside `backend`, so a review token cannot
/// reach an edit by a conversion slip. R19 puts the kind under the MAC as well. *(A-7)*
pub struct EditToken(tc_journal::ShownToken);
pub struct ReviewToken(tc_journal::ShownToken);
pub enum CommandError { Refused(RuleRefusal), NotWritten(String), LookAgain, PlanStale,   // Refused, LookAgain,
                        ReadOnly(ReadOnlyReason), BadToken(TokenError), Closed, Unavailable } // PlanStale: C0

/// Jobs owned by crates that do not exist yet. Each has a scripted double until its crate lands.
pub trait SyncPort: Send + Sync { /* sync, share, join */ }
pub trait ImportPort: Send + Sync { /* review, versions, run */ }
pub trait ExchangePort: Send + Sync { /* export, identify, receive */ }
```

*(B1b: built so in `crates/tc-app/src/backend/mod.rs`, with three changes to the first sketch.
`mint` and `mint_review` moved from the handle to `Snapshot`: the handle mints from the newest
snapshot, so a screen that read snapshot n while a teammate's change made n+1 would get a token
listing a head the person never saw, and the save would supersede the teammate's change unseen
(I4); tc-store's own `Snapshot::token` mints from the snapshot that was read. The parse functions
take the wire types, so the journal token never leaves the backend, and `submit` takes a
`StoreCommand` whose constructors take the right kind, which two `compile_fail` doctests prove. And
`Snapshot` publishes through a small trait, so C0 can put tc-store's own snapshot, whose key stays
inside tc-store, behind it. The names follow tc-store's `handle.rs` and `reply.rs` one to one. The
memory backend makes none of `Refused`, `LookAgain` and `PlanStale`; C0 and C2 add them with
`offered_drafts` (C1b), `leave` (B3, D1) and `counters` (C0), and `ProjectStore::peek` (B2) and
`create` (the unit that builds create, with R2).)*

`tc-app::wire` turns an `EditToken` into tc-api's `ShownToken` text with the prefix `st1.` and a
`ReviewToken` into `rt1.` text; there is no conversion across kinds. The token key is derived
from the installation secret and the project id (`storage:crates/tc-journal/src/token.rs:36`), so
two folders with one id share a key. The scope check (§4.3) keeps a token inside the session it
came from. A new installation secret (an app-data reset, a reinstall, a folder copied to another
computer) makes every older token fail its MAC; that is how a draft becomes `Unverifiable`
(§6.6), never a reason to drop its text. *(A-7, A-9)*

**`MemoryBackend`** (feature `fixture-backend`, never in release builds):

- **Projects.** It reads the parity state layout (`parity/tc4-baseline/fixtures/build-state.mjs:9-13`)
  and runs `seed::tc4_state_seed` on each project: `ProjectMeta`, `Versification`, books through
  the codec, pins from `resources.json` with the schema 1 → 2 migration, settings per top-level
  key, decisions and alignments per record, each OBS story as its `StoryBase`, and an OBS
  project's licence and front and back matter as `CarriedFile` records. It uses tc-journal's
  decoders: `decode_alignments` (`crates/tc-journal/src/fold/sidecar.rs:132`), `decode_decisions`
  (`:230`) and `records_from_usfm` (`:342`), each tC4 record first rewritten into the fields the
  decoder reads (`markedInvalid` from `invalid`; `importedInvalid`, `userStatus` and
  `verseTextMd5` for a decision). It seals the batches into packs with `tc_journal::pack::seal`
  under a fixed Local sealer, reads them back with `read_pack` and folds what they hold with
  `FoldState::fold`, so the fold takes real verified changes. Then it applies §13.2's check: the
  projection must reproduce every USFM file, `vrs.json`, story file and carried file byte for
  byte, every sidecar record field for field, and every free field of `metadata.json`, and every
  derived file of the folder must be one the fold derives. "Field for field" is tC4's reading of
  a record: an absent flag reads as false, a decision's absent `status` as tC4 derives it, an
  alignment's `invalid` with the I-3 hash rule, a timestamp as its instant, record order not at
  all. The differences the format makes by design and no tC4 screen shows are declared in the
  seed's report, never silent: `verseEdits` recomputed, a decision file's `summary` dropped, an
  empty `resources` or `languageSets` member added, a `schemaVersion: 1` `resources.json`
  migrated, a timestamp rewritten in tC4's own form, a decision with no `modifiedTimestamp`
  written with the time its change carries. A mismatch refuses that project: the scenario loads
  the projects that pass, lists each refused one with every mismatch (the scenario report,
  `tc4 serve`'s stderr, and a test that asserts the exact list), and shows nothing of it.
  *(B1b: the first text failed the whole scenario load. Every scenario holds
  `sample_burrito_obs`, which the check refuses with 52 mismatches the storage team owns (53 until C6), so no
  scenario would load at all; loading the refused project instead would make a screen look right
  for the wrong reason, against AD7. Until the storage team's fix lands, the Rust Home shows only
  the Bible card in every scenario, and B2's Home captures, whose `home()` waits for both cards
  (§7.1), wait for it.)* *(B2 D11: B2 did not wait. `home()` waits for the cards `home/list`
  names (§7.1), and every B2 row with tC4's OBS card is explained in `residuals.json`; the day
  this test fails, B2's rows are captured again and those residuals go.)* *(B7: the day came. The three findings are fixed in tc-journal (B7 D1), the seed check passes on the OBS project of every scenario, `every_scenario_loads_both_its_projects` asserts it, and B2's Home rows and C6's four Add a book cases were captured again with both cards.)*
  *(B1a: pending files are for Local devices only,
  and `encode_pending` refuses the seed's `Importer` device, so the seed seals packs as STORAGE
  §13.1 T8 does. tc-obs's `decode_story` records cannot give a story file back through the fold,
  which keeps the base's lines and splices frames differently, so a story's base is its file,
  with no `Frame` or `StoryRef` record, as tC4 keeps a project with no journal; the check reads
  every frame back through the fold and compares it with tC4's reading of the file. The OBS
  project failed the check until B7: `docs/evidence/state-seed-check-2026-10-18.md`.)*
- **Tokens.** Minted with a fixed test installation secret, so they are deterministic.
- **Commands.** `Unavailable`. Under `test-hooks`, `test/script` queues outcomes: a fresh token
  for the displayed value (the fold does not change), `NotWritten`, `WrittenNotDurable`, a delay,
  a switch to read-only, or a teammate's head added before the reply (a change encoded by
  tc-journal for another device and given to `FoldState::add`). Drafts and leaves are no-ops.
  *(B1b: the fresh token names exactly the heads the command's token named, signed again. The
  first build minted it from the newest snapshot, so after a scripted teammate's head on the
  record being saved, the "saved" reply's token listed the teammate's change as seen and the next
  save would have replaced it unseen (I4); the store's reply names only the new change, which
  continues only what the person saw (STORAGE:1449-1455).)*
- **Teammates.** `MemoryBackend::teammates_edit_verse` writes teammates' heads the same way (one
  change per text, each by its own Local device, all on the record's current heads), and
  `MemoryBackend::teammate_edits` folds them as an intake would, so the conflict chip and the
  sync toast read a real fold conflict. B10's scripted sync calls them
  when a screen's steps ask. *(B10: on the store, the scenario store's `teammates_edit_verse`
  seals each teammate's change into the closed project, reached through
  `ProjectStore::teammates_edit_verse` under `test-hooks`; the sync then opens the project through
  the registry, so the fold takes them in and writes `conflicts.json`.)* *(B1b: each call continues each teammate's sequence, which the
  backend keeps per project (`memory::teammates::Teammates`; a test that builds a fold by hand
  keeps its own), and a batch's id comes from its device and number. The first build gave every
  edit sequence number 1, so a teammate's second edit claimed the same device and number as the
  first, a `DuplicateDevice` anomaly whose notice tC4 never shows.)* *(B1b: the first text had the `shared` scenario load two teammates'
  heads on Titus 1:1. tC4 shows `shared-home`, `sync-internet-consent`, `sync-failed` and
  `sync-ok` without a conflict (`parity/tc4-baseline/README.md:407-410`); only after "Door43
  main: two teammates change Titus 1:1" (`sync-ok-teammates`) does the card show "1 conflict ·
  Review". Heads added at load would put a conflict chip on four screens tC4 shows without one.)*
- **Door43 remote.** The `origin` of a project's `.remotes.json` (the mock server's remote list,
  which the seed ignores) gives the project's Door43 owner and repository, for B2's "On Door43";
  one that cannot be read refuses the scenario, so a shared project never looks local (B1b).
- **Resources.** It installs every `repos/_local_/_sideloaded_/*` burrito of the scenario into the
  app's own resource store (`<app-data>/resources`, which the harness's run folder makes
  temporary), with the pins of the scenario's install records, before `App::start`; an install
  already there is kept, so a restart installs nothing. It never seeds `bundle/en/`, which would
  change what the screens show (`parity/tc4-baseline/README.md:224-246`). *(B1b: "a temporary
  store" is the app's own: B3's `BookResources` reads the app's `ResourceStore`, and a second
  store would need a second path.)*
- **Scale.** It also folds tc-corpus Psalms and ten times Psalms for the operation-count tests.
- **Home.** It writes no derived files, so its `peek` computes summaries and conflict counts from
  its fold. Home's screens are captured on it, but `home_costs_projects_not_history` runs only
  on the store, where `peek` reads the files the product reads. *(A-25)*

**`StoreBackend`** adapts tc-store's `ProjectActor` to the seam. It is its own unit, C0, because
its size depends on what S1 provides *(A-5)*. *(C0: S1 provides R4, R5, R14 and R16 and both
things phase 2 waits for, so C0 is the thin layer of the first bullet: no command thread, no own
`seq`, no frontier check. `crates/tc-store/src/handle.rs` has the handle surface (R4) and the
snapshot it publishes after every batch under an `RwLock`, which reads take without the actor
(R5, and the O(1) snapshot: an `Arc` clone); `StoreEvent::Committed { seq, delta, changes }`
carries each batch's `Delta`; `ProjectHandle::counters` is R14; `StoreReply.seq` and the typed
`StoreEvent::Notice` are R16. `crates/tc-app/src/backend/store.rs` adds what the store cannot
know: who asked for a command, since `Committed` names the batch's changes and is sent before the
reply resolves, so a relay thread records each reply's changes with the command's `Origin` and the
events thread waits for that record before it passes a `Committed` on (a batch with changes that
no submit asked for, such as S2's outside edits and E1a's intakes, must not wait for a record:
the events thread passes it on as `StoreCause::System` as soon as no submit is in flight or the
relay has settled a stored or unchanged reply the actor gave after it; an error reply (read-only,
look again, not written, refused) carries no `seq`, so it says nothing of where the batch stands
in the actor's order and does not end the wait, and behind such replies the batch waits until no
submit is in flight;
E1a maps an intake to `Intake` once tc-store's `Committed` names its devices); a close and a drop of
tc-store's handle, which wait for the actor, on a thread of their own; a listing that reads the
projects folder itself, because tc-store's `Store::list` keeps only folders with a `project.json`,
reports every folder that holds a project's files but cannot be opened, a link to one included,
instead of hiding it, and leaves out tc-store's staging folders; an open by the exact name the
listing holds; and a record in the report log of each staging folder tc-store's start removed
(STORAGE §6.4 N1), written from a thread of its own once that cleanup is over, so the start never
waits for it and "Ask for help" carries it. An import's and a join's are recorded as tC4's start
records the same moment (`tC4:src/data/journal/opsLog.ts:603-614, 677-691`): `import`, refused
with `import.write-failed`, and `open-door43`, failed with tC4's sentence, each with the folder,
`interrupted` and `rolledBack`, so the Feedback dialog says "Report: import, …" as tC4's does. tC4
makes no staging folder for a create or a copy, so theirs, and a folder whose name does not say its
work, are this app's `staging-removed`, with the folder and its kind *(after the third
verification: the first text recorded all four as `staging-removed`, unlike tC4)*.
E1b tells the person of a join that did not finish and F1 of an import. A create or a copy that
did not finish is recorded only: it held no typed work (a copy's original is untouched, and a
create's source files are where the person picked them), and the person sees that the project is
not there and makes it again. Every fold step S1 makes after an open is a command's or a
second writer's, and both send `Committed`, so `every_fold_step_reaches_the_seam` (consecutive
seqs ending at the newest snapshot) stands in for the frontier check. Measurements:
`docs/evidence/store-adapter-2026-10-19.md`.)*

- If S1 provides R4, R5, R14 and R16 (§9.2), C0 is a thin layer: each `ProjectHandle` method
  calls the matching store API.
- If it does not, C0 builds them around the actor without a second actor *(C0: not built, S1
  provides them)*: it runs commands on
  a dedicated thread and answers each `submit` through a oneshot channel; it publishes the fold
  clone the actor hands out after each command and each intake as the snapshot readers take
  (an `ArcSwap`), so reads never queue behind the actor; it turns each command's `Delta` into
  a `StoreEvent`; and it gives `seq` its own counter, raised with each published snapshot.
  Changes the actor makes on its own (an intake, an outside edit) reach the app as events only
  if S1 reports them; if it does not, C0 compares `FoldState::frontier()` after each actor step
  it can observe and sends `Resync` on a difference, which is correct but coarse.
- C0 cannot work around two things: an O(1) snapshot from the actor (STORAGE §7.8 requires one
  for sync, plans and export, so S1 should have it) and the `Delta` of each command. Without
  them phase 2 waits for a change to S1.
- Either way the contract suite of this section is its test.

C1a then moves the app onto the store: the parity scenario projects are written to disk with the
same seed *(C1a, D1: `backend/scenario_store.rs`, as an import is written: a `Store::staging`
folder, the fixed files, the very packs the seed sealed (same changes, same `Importer` device,
same project id, AD36), the carried files, `Store::place`, one open at the final path that
materializes every derived file, and a Scripture Burrito check. `Store::create` is not used: it
gives a random id and writes the records at the run's time as this computer's changes, so the
decisions' `modifiedTimestamp` and the screens would no longer be tC4's. `project.json`'s writer
is private to tc-store, so the scenario writer writes its four fields in tc-store's order (R29).
A scenario writes only into an empty projects folder. `test/reset` writes again each scenario
project opened or given teammates since it was written and removes every other entry of the
projects folder (D3); teammates are packs sealed by each teammate's own device, written while
no session holds the project (D12). `ProjectStore::store_hooks` gives `test/hold` tc-store's
hold points (D13).)* The memory backend and its feature are deleted once the last phase-1 unit
has moved to the store: until then `tc4 serve --scenario <dir> --backend memory` keeps it.

**Contract suite.** `tc-app/tests/store_contract.rs` defines `store_contract!(make_store)`. Each
case is named after the rule it protects.

| Case | Runs on | Rule |
|---|---|---|
| `chapter_view_equals_seed_usfm` | both | the projection matches the source bytes |
| `minted_token_parses`, `forged_token_is_refused` | both | a screen cannot forge a token (STORAGE:745-754) |
| `review_token_lists_every_head` | both | STORAGE:1610-1616 |
| `review_token_text_is_refused_as_an_edit_token` | both | AD3 *(A-7)* |
| `edit_of_a_verse_in_conflict_keeps_the_conflict` (through `App::call`; through HTTP from C2) | store | AD3, I4 *(A-7)* |
| `review_token_in_a_save_is_refused_by_the_actor` (a unit test in `backend/store.rs`) | store, after R19 | AD3 *(A-7)* |
| `snapshot_clone_costs_nothing` | both | STORAGE:574-577 |
| `reads_never_wait_behind_a_long_command` (a held checkpoint) | store | R5 *(A-5)* |
| `opens_of_one_folder_share_one_session`, in both orders | both | AD38 *(A-8)* |
| `draft_with_unverifiable_token_is_offered_as_text` | store | §6.6 *(A-9)* |
| `unchanged_save_writes_nothing` | store | STORAGE:1447 |
| `reply_token_lists_only_the_new_change` | store | STORAGE:1455 |
| `stale_token_after_intake_makes_a_conflict_never_a_revert` (the unseen head made by a save; E1a reruns it with an intake) | store | STORAGE:1449-1453, I4 |
| `retry_after_lost_reply_is_idempotent` | store (through tc-app) | AD4 |
| `retry_while_first_attempt_waits_runs_once`, `retry_with_another_body_is_request_reused` | store (through tc-app) | AD4 *(A-2)* |
| `read_your_writes` | store | `StoreReply.seq` (R16) |
| `section_save_checks_only_displayed_values` | store | STORAGE:1474-1481 |
| `resolve_with_moved_heads_is_look_again` | store | STORAGE:1615-1616 |
| `not_written_keeps_seq_and_retry_uses_the_same_token` | store | STORAGE:1422-1424 |
| `written_not_durable_is_saved_with_warning` | store | STORAGE:1424-1428 |
| `read_only_refuses_every_command_and_writes_nothing` | store | STORAGE:1893-1899 |
| `drafts_offered_after_reopen` | store | STORAGE:1750-1759 |
| `a_save_never_erases_a_newer_draft_of_its_field` | store | STORAGE §5.14, R26 *(C0, enabled by C5)* |

*(B1b: `crates/tc-app/tests/store_contract.rs` runs the "both" cases on the memory backend as
`store_contract!(memory, crate::memory_fixture)`, with one more token-kind case,
`edit_token_text_is_refused_as_a_review_token`, the mirror of `review_token_text_is_refused_as_an_edit_token`,
and `opens_of_one_folder_share_one_session` as two functions, `…_screen_first` and `…_side_first`.
A fixture gives the store, the sample Bible project, its USFM bytes, a record two teammates
changed, and a project of another id (a token minted under another project's key is one of the
forgeries). `tests/support/counting_store.rs` wraps any backend to count opens, checkpoints and
closes and to hold an open or a checkpoint, so C0's `store_contract!(store, crate::store_fixture)`
reuses it. The two `compile_fail` doctests on `StoreCommand` and wire's
`a_relabelled_review_token_is_bad_token` are the rest of the token-kind proof.)*

*(C0: `store_contract!(store, crate::store_fixture)` runs the "both" cases on tc-store, over a
world written through tc-store's own create on its in-memory file system
(`tests/support/store_world.rs`); `tests/store_contract/store_cases.rs` holds the store-only cases
and `tests/store_contract/through_app.rs` the cases through tc-app, which send production
`editor/saveVerse` (C2). Three cases run another way than the table first
said. `edit_of_a_verse_in_conflict_keeps_the_conflict` runs through `App::call`, not HTTP: no
endpoint opens a project before B3 or saves before C2, and `tc4 serve` on the store needs C1a's
on-disk scenario projects; `App::call` takes and gives the exact bytes tc-server forwards (A2's
tests prove tc-server forwards bodies unchanged), so the token's trip through JSON text, the point
of the case (A-7), is covered, and C2 reruns it through HTTP. S1's only intake is a second
writer's files, which turn the project read-only, so
`stale_token_after_intake_makes_a_conflict_never_a_revert` makes the unseen head the way an
intake would, by a save with a newer token, and E1a reruns it with a real intake.
`review_token_in_a_save_is_refused_by_the_actor` cannot be written through the seam, whose types
make the command unbuildable, so it is a unit test in `backend/store.rs` that hands tc-store the
raw command by the path every submit takes, which proves both the actor's refusal (R19) and the
wire's `NotSaved::BadToken` for it. Two cases marked C1b run on the store now
(`draft_with_unverifiable_token_is_offered_as_text`, `drafts_offered_after_reopen`), so C0 adds
`ProjectHandle::offered_drafts` and `StoreCommand::draft`. The adapter's own cases, a property test
and a crash test over every write point of a save are in `tests/store_adapter.rs`.)*

### 4.3 Sessions, epochs and the request guard

- `SessionRegistry` maps a `ProjectRef` to one `ProjectSession`. Each session counts its users:
  the screen, a sync, a USB export, a Settings-from-Home save. It closes when its last user
  leaves and its checkpoint is done. An open of the same folder while its leave-checkpoint runs
  waits for it, as tC4 waits for `leaveCheckpoints` (`tC4:src/state.jsx:199, 2177`).
- **One open per folder at a time** (AD38). The registry keeps the in-flight open of each folder
  as a shared future. A second caller (a card click while a Home sync or a Settings-from-Home
  save is opening the project, or the reverse) waits for it and becomes another user of the
  same session; it never calls `ProjectStore::open` a second time, which would find `.tc/lock`
  held by this same `tc4` and be refused, with `tc4` itself as the holder (STORAGE §6.1 O1).
  *(A-8)*
- **A project lock held by another process** refuses the open with a message that names the
  holder's process (a `tc4` left running after a crash, or another program). Inside one `tc4`
  the registry above is the only opener, so it never meets its own lock, and nothing opens
  read-only for this reason. The cases of another computer are unchanged: a lock that names
  another computer, a second writer, and a cloud-synced folder (owner's ruling of 2026-10-18;
  STORAGE §6.1 O1).
- One screen session at a time, as in tC4. Going Home closes the screen session with the
  "leaving the project" checkpoint, as tC4's `backToProjects` does
  (`tC4:src/state.jsx:7286-7328`) and J8 asserts; opening another project from Home therefore
  never has a screen session to close. A sync or USB export from Home holds a side session
  (Q-15 on memory). *(A-25)*
- Each open gets a new `Epoch`. Every project-scoped request is checked before any work: a
  closed session or another epoch gives `StaleScope` and does nothing. Check and align sessions
  get ids inside an epoch, and a replaced one answers `StaleSession`. This replaces tC4's sequence
  counters (`tC4:src/state.jsx:592-622, 3592-3631`).
- `ProjectSession` holds the handle, epoch, mode, presentation, `ReplyCache`, caches (§4.4),
  check sessions, aligner sessions, plans and proposals.
- *(B1b: built in `crates/tc-app/src/session.rs`. The users are the screen, side users
  (`SideLease`) and the writes in flight (`WriteLease`, taken in dispatch before the reply cache
  and moved into the write's work, so a session never closes under a running write even when its
  HTTP caller went away). Only the screen answers project-scoped requests: once the person has
  gone Home, a request with that scope is stale at once, even while the leave checkpoint runs or
  a side user keeps the session open, because the person is on Home and nothing they do there
  names the project; a side user acts through its lease, never through a scope. A screen open
  of another folder leaves the old one first, as going Home would; the check of the other folders
  and the taking of the screen happen under one lock, so two screen opens of two folders at once
  never both find the screen free, and a screen open waits for another folder's screen open to end
  before it leaves it. Screen opens that wait for one folder's open share its screen: one that is
  cancelled hands it to those still waiting, and only the last takes it away, so a cancelled open
  never closes the session another screen open is about to return. An open that meets a running
  leave checkpoint or close waits for both; if a side user kept the session open it joins it with
  its epoch, otherwise it opens anew with a new epoch. A failed open reaches every waiter, adds
  one `Open` report, uses no epoch and leaves nothing. A failed leave checkpoint is reported and
  sent as `Checkpoint(Failed)`, and the session closes all the same; the owed checkpoint is D1's.
  `test/reset` and `app/quit` close every session after its checkpoint (`close_all`); epochs are
  never reset.)*
- **Queries never touch the actor.** They run on the latest snapshot on a rayon pool. Nothing in
  tc-app calls `FoldState::record_keys` (`storage:crates/tc-journal/src/fold/reads.rs:82-91`),
  which lists every key in the project and sorts them, and there is no project-wide index.
  Until R1 exposes the fold's per-chapter entries (`ChapterEntry`,
  `storage:crates/tc-journal/src/fold/state.rs:242-257`), each read finds its keys at the cost
  of what it shows *(A-21)*:
  - a chapter's verses from `chapter_usfm` (`storage:…/fold/project.rs:505`), which the editor
    reads anyway, and `key_state` on each verse it holds;
  - alignments by probing `RecordKey::Alignment` at each verse of the chapter;
  - decisions by probing the check keys of the (tool, book) list that tc-check derives from the
    resource, plus `FoldState::detached()` for decisions whose check moved away;
  - a book's comments from its projected comments file (`render(&DerivedFile::Comments(book))`,
    `storage:…/fold/project.rs:613`), which costs that book.

  The differential test `probe_finds_every_key_a_scan_finds` compares each probe with a
  filtered `record_keys` on Titus, Psalms, the conflict scenarios and a project with verses
  beyond its versification, so a probe that misses a record fails a test instead of hiding
  work.

  *(B1b: R1 is provided, by tc-journal's `fold/ranges.rs` (`c4b8663`): `chapters_of`,
  `chapter_keys` (a chapter's front, verses, alignments, comments and decisions), `decisions_of`
  (tool, book), `alignments_of`, `comments_of` and `book_keys`, each costing what it returns, with
  tc-journal's own differential test (`tests/scope.rs`, `range_reads_find_what_a_scan_finds`). So
  the reads of this section are R1's, wrapped and counted in `CountingSnapshot` (`keys_probed`), and
  no probe above is built. A chapter's verses come from `chapter_keys`, not from its USFM, so a
  verse beyond the versification is found all the same. A book-wide read of alignments or comments
  uses `alignments_of` or `comments_of`, never a loop over `chapters_of`, which lists only chapters
  with a verse or a front and misses a record anchored in a chapter with neither (after a
  renumber). The test keeps its name; it runs in `crates/tc-app/tests/op_counts/probes.rs` on the
  sample Bible project, the same with two teammates on Titus 1:1, a project with verses beyond its
  versification and records in chapters with no verse, Psalms and ten times Psalms.)*

### 4.4 Caches

| Cache | Key | Owner | Filled | Invalidated by | Budget |
|---|---|---|---|---|---|
| `ResourceStore` | — | app | at start: `App::start` opens `<app_data>/resources` and plans first-launch seeding from receipts and check records alone *(A9)*; no content file read | an install | repeat launch reads no content file (PERF:100) |
| `BookResources` (LRU: current and previous book) | (pins digest, book) | session | first need, `tc_resources::load::load_book` (`crates/tc-resources/src/load.rs:163`) | a pins change | 300 ms first; 60 MiB per book (PERF:87) |
| `Lexicons` | lexicon pin | app | first lookup | an install | 100 ms load; under 1 ms per lookup (PERF:88) |
| `HelpsCache` | (pins digest, book or story, helps-source setting) | session | with `BookResources` | pins, setting | part of the 300 ms |
| `CheckListCache` | (tool, unit, resource ref, versification digest, scope digest) | session | first `check/open` | key change; a decision or a re-assessment updates only its rows and stamps each with the `ViewSeq` that changed it, so `rows { since }` costs the rows changed *(A-12)* | 250 ms first, 20 ms cached (PERF:77) |
| `CheckSession` | `CheckSessionId` | session | `check/open` | replaced; a verse delta re-assesses only that verse's checks | 1 ms (PERF:79-80) |
| `AlignerSession` | (book, chapter, verse) | session | `align/open` | leaving the verse, after the server has saved its links; a text change marks it stale. Its save timer (2 s after the last link change) runs here *(A-4)* | 30 ms (PERF:83, 85) |
| `SuggestModel` | (project, testament) | trainer thread | background training from one snapshot (DELTA:627-629) | a save appends its verse (`crates/tc-align/src/suggest/engine.rs:218`) | 50 ms (PERF:84) |
| `DraftedCounts` | book | session | at open, background | each delta | 1 ms (PERF:76) |
| `SummaryCache` | folder | app: in memory *(B2)*; persisted from C1a, whose kept summaries it reads on a miss | on a miss, by one `HomeSummary` job at a time from derived files *(B2: not from deltas while open; a save gives a new stamp, and the next Home reads it again)* | stamp change | Home costs O(projects) |
| `ReplyCache` | `RequestId`, with a SHA-256 of the request body | the session's cache for project-scoped writes; the app-wide cache for writes with no scope (B1b; until then one app-wide cache served every write, A3). Dispatch checks a write's scope (`Write::at`) before it picks the cache, so a stale-scope refusal is never kept under its id | when a write starts (in flight), then its reply | bounded at 1,024 finished replies; in-flight entries are never evicted *(A-2)*; a session's cache goes with the session; `test/reset` keeps no reply (§5.8) | — |

There is no `ProjectIndex` *(A-21)*: §4.3 says how each read finds its keys (R1's range reads,
B1b).

### 4.5 Home and the project registry

- `home/list` lists `<projects>/` through `ProjectStore::list` and orders the cards newest
  first by the later of the project's last use and `meta.dateCreated`, either missing as 0
  (`tC4:src/state.jsx:3925-3929`), ties by folder name. Each card comes from `peek` plus its
  summary; `home/card` answers one card, so a card reads again when its summary is ready
  (`Event::HomeCard`) and at each new Home visit, as tC4's card reads its progress at each mount
  *(B2 D1)*. A card's facts are those of tC4's platform summary of `metadata.json` (the name in
  `meta.defaultLocale`, else the first, else the folder's name; the first language's tag and
  direction; a Bible's books from `currentScope` in canonical order). A `metadata.json` that
  cannot be read still gives a card, named after its folder, its language and books "—" *(B2)*.
  The Resume card shows the last change's place while its project is listed and holds that book
  or that story (`resumeRecordHolds`).
- A folder that holds a project's files but cannot be opened as one (C0's `Listing.unusable`)
  is shown on Home with its name and the reason, and it opens nothing: a `project.json` that
  tc-store's open refuses or cannot read; a `project.json` that is missing while the folder
  holds `metadata.json`, `ingredients/` or `.tc/` (a project that lost the file, whose journal
  and drafts are still there, or a translationCore 4.0 project copied in, which Import brings
  in, F1); a name tc-api cannot carry; and a link to such a folder elsewhere, which tc-store
  never reads as project data (its checks of the lock, the drive and the cloud client would be
  about the link), so the reason asks the person to move the folder in. A project whose card
  cannot be read (`peek` fails other than `NotAProject`) is listed the same way, with the reason
  (`FolderUnreadable`); only a folder that went away since the listing is left off. A card
  whose saves cannot be looked at (another program holds `.tc/pending` or
  `.tc/materialized.json`) is shown with no stamp, so no summary is its and its counts are "—"
  *(B2 fix)*. A file, an empty folder
  and a folder with none of those files hold no one's work and are not listed; tc-store's
  staging folders are left out. Declared differences from tC4, which lists every local
  repository whose `metadata.json` has a project flavor and nothing else
  (`tC4:src/data/httpStore.ts:345-351`): a folder with broken or missing metadata, which tC4
  hides, is shown with its reason, so a folder that holds someone's work never disappears from
  Home without a word (C0, B2); and a translationCore 4.0 project, which tC4 opens, is shown
  as unusable until Import brings it in, because this app opens only projects in its own format
  (STORAGE §3.5, §13). Each reason is a typed `UnusableReason` with a rebuild sentence
  (`home.unusable.*`, pending in nine languages); the card names the folder, says "Cannot be
  opened" and the reason, and has no button. While such a card shows and no project is listed,
  the "No projects yet" box is not drawn: it would invite a new project in place of the
  person's own (tC4 D30's reasoning) *(B2 D9)*. When S2 restores a missing `project.json` from
  the HEAD tree at open
  (STORAGE §3.5), the listing must count such a folder a project, as the open does:
  `a_folder_is_listed_as_a_project_exactly_when_tc_store_opens_it` fails until it does.
- A summary (`tc_app::home::Summary`, serializable, so C1a keeps it as it is) holds per-book
  drafted and total verses (`tc_app::drafted`, one rule for the tiles and B3's rail, #572: a span
  counts every verse it covers; a verse is drafted when its stored words, trimmed as JavaScript
  trims, are neither empty nor `___`; a declared difference from tC4's `draftPercent`, which reads
  a verse's body only up to its first paragraph marker), the verse conflicts by book, and a story
  project's stories (`tc_obs::obs_progress`) with each story's title: the drafted one, else the
  title of the pinned gateway story when it is on this computer (#328), kept in the summary so a
  later launch reads no resource for it. The in-progress count is the UI's, from the tiles *(B2)*.
- The summary's stamp is SHA-256 over the sorted names in `.tc/pending/` (the saves not sealed
  yet; content hashes, STORAGE §4.3) plus `.tc/materialized.json`'s size and mtime: one listing
  and one stat per card, at any history *(C1a, D8: packs are not one folder, they are nested by
  sealer and month under `ingredients/tc/changes/`, so listing them grows with the history; and
  every seal, consolidation or intake rewrites `materialized.json`, which records the md5 of every
  pack it lists. `StoreBackend::summary_stamp`; the summary files are `PrefsWriter::summary` and
  `record_summary`, removed by `test/reset`)*. A save adds a pending file and a checkpoint
  rewrites the manifest, so a summary is never invalidated by hand. The memory backend's stamp is
  its fold's version *(B2 D8)*. A folder with no `materialized.json` (copied or received without
  `.tc/`, which Door43 and Git leave out) would share one stamp with every such copy whatever it
  holds, so its stamp also covers the name, size and mtime of `metadata.json`, `conflicts.json`
  and each book and story file the summary reads: one more listing (two for stories), only for a
  copy never opened here. A summary, of this launch or kept in app data, is used only when the
  project id it names is the listed folder's *(B2 fix, C1a's D8)*.
- Summaries are kept across launches *(C1a's D7; B2 built Home's half)*: a card with no summary of
  this launch reads the one app data keeps (`PrefsWriter::summary`) and uses it when its stamp is
  the card's; each new summary is kept with `record_summary`. A repeat launch with fresh summaries
  reads no derived file for its counts (`home_costs_projects_not_history`). A project open in this
  app gets a new stamp at each save, so its summary is computed again at the next Home; B2 does not
  follow deltas.
- When a card's summary is missing or stale (a folder received, copied, saved or written by
  another installation), the card shows "—" (DELTA:928-933) and one background `HomeSummary`
  job reads the derived USFM files, the stories and `conflicts.json`, under the stamp read before
  them. No fold, no lock. One job at a time, in the order the cards asked; opening any project
  cancels it (`App::cancel_home_summary`, B3). A failed or cancelled job keeps a failure under
  its stamp and tells no card to read: the card stays "—" until the next Home visit, which tries
  again, as tC4 caches nothing on a failed read *(B2)*.
- A conflict chip reads `ingredients/tc/conflicts.json` (STORAGE:891, 2492), never a fold, and
  counts verse conflicts (`verseText`, `verseStructure`, `passage`, `structureIssue`, tC4 #679);
  a story project has no chip, as in tC4.
- The language search of New Bible and New Open Bible Stories (AD27, #492) ranks on the server
  from the list tC4 ships, compiled into tc-app byte for byte (`tc_app::language_search`, a port
  of `languageSearch.ts`); the list is indexed at its first read, which the field asks for as it
  mounts. A Door43 code a Scripture Burrito cannot hold is offered as one it can that keeps every
  letter in private pieces of at most eight (`acm-x-lebarabic` → `acm-x-lebarabi-c`), with
  Door43's code beside it; when none can be made, none is offered (owner's answer to Q35, #582)
  *(B2)*.

### 4.6 Background work and cancellation

- `JobRegistry::start(kind, project, f)` runs `f` on the right executor: rayon or
  `spawn_blocking` for blocking work, tokio for network work. `f` gets
  `JobCtx { progress: ProgressSink, cancel: CancelToken }`.
- **Reporter.** One tokio task per tc-app job reads atomic counters. It sends nothing before
  300 ms, then at most one report every 100 ms while a count moves, or every second for the
  jobs no screen shows (`Train`, `HomeSummary`, §3.6) *(A-29)*. Counts never fall within a
  phase. The last report comes before `Done`. A job that ends sooner sends only its outcome.
- **The open** forwards tc-store's reports with no timer of its own
  (`app_forwards_open_reports_unchanged`).
- **Cancel.** `jobs/cancel` sets the token. Jobs poll it between files of an install, between
  books of a plan, per chunk of a download and every 100 ms while a download waits for Door43
  (A11: the job hands tc-door43 a `DownloadWatch` that answers from the token and keeps the byte
  count; `crates/tc-door43/src/download.rs`), and in git2's transfer callbacks (STORAGE:2725,
  2861). A cancelled job reports `Cancelled` and changes no project data. *(A11: the check while
  a download waits is added to the check per chunk, because a stalled transfer would otherwise
  hold a Cancel for up to the 60 s read timeout.)*

| Job | Progress unit | Cancel leaves | Budget |
|---|---|---|---|
| `Open` | files, bytes, paths (tc-store) | not cancellable, as in tC4 | 3 s / 10 s; first report within 300 ms (PERF:89-90) |
| `Create`, `Join`, `Import` | steps, bytes | the staging folder removed | import 2 min (PERF:95) |
| `Sync` | objects, bytes | before integrate: nothing; after: the step finishes and says what arrived (Q8) | 3 s plus network (PERF:93) |
| `ExchangeExport` | bytes, then verify | no file on the stick | 15 s on USB 2 (PERF:94) |
| `Download`, `FixFetch` | per repository, bytes (`DownloadWatch::received`, A11) | nothing: the part-downloaded file is removed and nothing is installed; a cancel that comes after the download is checked lets the atomic install finish | background |
| `UpgradePlan`, `GatewayPlan` | books | nothing; plans are read-only | 2 s count, 5 s plan (PERF:81-82) |
| `Export` | steps | no file | — |
| `HomeSummary` | files | the card keeps "—" | background |
| `Train` | verses | the old model stays | background |
| `Seed` (first launch) | content files *(A9)* | whole copies only: the copy being made stays in `resources/.partial/` until the next start removes it, and the next start copies only what is missing | 60 s (PERF:99) |

- **Close is a quit, and a quit stops the seed** *(A9)*. The shell's Close asks `tc4` to quit
  (§6.5, "The close guard"). `App::quit` cancels the `Seed` job and waits until it has stopped,
  before it flushes the prefs, all inside the server's 5 s close (§5.1), so `tc4` lets go of
  `app.lock` only when no seed writer runs (DELTA §3.15.10). The seed polls the token before each
  install and before each file it writes, never between an install's last file and its receipt,
  so a copy whose files are all written is finished. It reads and checks a whole install before it
  writes the first file of its copy, and does not poll in between, so a close waits for the
  install being read: 277 ms for the largest (`en_ust`, 94 MB) from the disk on the Linux PC
  (`docs/evidence/first-launch-seeding-2026-10-18.md`); on a reader that makes each open slow it
  grows with the install's file count (1,022 for `en_tw`), a poll per file read is the fix, and
  the Windows measurement (G1, G2) decides. A close that the 5 s budget cuts is safe, because an
  install is staged and renamed whole. `jobs/cancel` of the `Seed` job does the same without
  quitting; `hello.startup` then says `Failed` ("preparing the resources was stopped; it
  continues at the next start") until the next launch.
- **Seeding is planned from receipts** *(A9)*. `ResourceStore::seed_plan` reads each bundled
  install's receipt and the machine's copy's receipt and check record, never a content file, and
  says what to copy (an install the machine lacks) and what to check again (a copy not checked
  against its present receipt). So the plan runs inline in `App::start`, costs the receipts of the
  bundled installs and of their copies, and a repeat launch plans nothing.
- **A bundle says what it holds** *(A9)*. The folders of an expanded store cannot say that one
  is missing, so the build of the bundle (`crates/tc-app/examples/bundle_store`) writes
  `BUNDLE-MANIFEST.json` beside the installs, naming each by repository and commit, as tC4 walks
  the `bundled_resources` of its `BUILD-MANIFEST.json` and stops its start at a missing archive
  (`tC4:scripts/desktop-bootstrap.cjs:230-290`). The plan reads it with the receipts. A listed
  install that is neither in the bundle nor already on this computer ends the start `Failed` at
  once, naming what is missing, and nothing is copied from a damaged bundle; an install this
  computer already holds is no loss, as tC4 finds an installed resource before it looks for its
  archive. A folder with no manifest, an unreadable or empty one (a `--bundle` that names the
  wrong folder) ends it `Failed` as well, not `Ready` with nothing done. An install in the bundle
  that the manifest does not list is copied. Whatever adds an install to the bundle after the
  build, such as the packaging step that downloads the Open Bible Stories pictures, builds the
  manifest again with it (it runs `bundle_store` once, after the download).
  `ResourceStore::seed` does the plan with a `SeedWatch` that hears `done(files)` after each
  content file and is asked `cancelled()`.
- **Saves keep flowing** during every job (I9, STORAGE:2976-2984). tc-app holds no lock across a
  job.
- **After a restart** import resumes from its own temporary state, and the seed from the plan of
  what is missing (nothing is kept but whole copies, *A9*); a download starts
  again from its first byte when it is next asked for, because opening the resource store clears
  what an interrupted one left in `.partial/` *(A11)*; training, plans and summaries are computed
  again; nothing syncs at launch or while idle (DELTA:737).
- **Download memory.** A download streams to a file and is checked from it on the blocking pool,
  but the check returns every unpacked file in memory and tc-resources holds them until the store
  has written them: about 105 MiB peak for en_ust v91, of which the unpacked text is 89 MiB
  (`docs/evidence/streaming-downloads-2026-10-17.md`). Unpacking each checked entry into the
  store's staging folder would bound it by the largest file. Open after A11; it changes
  tc-door43's verify and tc-resources' `Staged`.
- **Aquifer downloads.** tc-resources' `AquiferClient` still reads an asset whole into memory,
  with no byte progress and no cancel (`crates/tc-resources/src/aquifer.rs`,
  `download_asset`); the English notes asset is 16.2 MB. A `Download` job for an Aquifer asset
  needs A11's streaming there first. Open after A11; B9a's scope names it. *(B9a: no Aquifer
  asset gets a `Download` job yet. Get source texts lists one Door43 organization's packages,
  as tC4's does, and nothing in the app installs an Aquifer asset, so the streaming waits for
  the unit that first offers one, such as the owner's answer to Q34 (TC4-DELTA §5, #720), which
  needs a design: where the Portuguese Bible is offered and how its label reads.)*

### 4.7 Services behind the screens

| Service | Uses | Notes |
|---|---|---|
| `editor` | `CountingSnapshot::projected`, `key_state`, `mint`; `chapter_usfm` (`storage:…/fold/project.rs:505`); the tc-usfm codec | builds a chapter from that chapter's keys (R1's `chapter_keys`, provided, B1b) *(A-21)*; section values from `model::SectionBuffer`; empty words become `___` as in tC4 (`tC4:src/state.jsx:7183-7186`) |
| `story` | `tc_obs` `present_story`, `resolve_image`, the frame rules | text rules checked before the command; the store checks again |
| `helps` | `tc_helps::Helps`, `highlight_quote`, tc-helps' block renderer | titles are gateway renderings, as `glTitleFor` |
| `check` | `tc_check` after unit C: derive, `session::open` over the fold, `rail`, `status`, `selection`; `tc_resources::resolve::preflight_tool_book` | `decide` returns the new row and counts; a verse delta re-assesses only that verse's checks |
| `align` | `tc_align::Aligner`, `suggest::Model` | one edit at a time per aligner (a `seq` check), with the UI queueing clicks so none is refused *(A-14)*; the session keeps `shown` and `verse_shown` (STORAGE:1500-1505); the session saves its own links (AD18) and reports `AlignSave` *(A-4)* |
| `conflicts` | `FoldState::conflicts`, `conflicts_of`, `conflict_counts`, `history`, the choices in `fold/resolve.rs` | review tokens from `mint_review`; proposals refused with `LookAgain` when heads moved |
| `resources` | `Door43Client`, tc-resources installs and updates, `tc_check::carry::plan` | installs on blocking threads, because they fsync inside async functions |
| `settings` | `SetSetting`, `SetMeta` | two keys are two batches until R18 |
| `account` | `InternetSwitch`, `mint_token` (`crates/tc-door43/src/client.rs:397`), `session_for_token` (`:436`), `list_orgs` (`:442`) | `Keychain` trait: `OsKeychain` (`keyring`, Q-3) and `MemKeychain`; "no keychain on this computer" kept. *(A8, tC4 #496: `AccountView.keychain` says whether a keychain works; without one M12 has no "Stay signed in on this computer" box and shows, in its place, tC4's `signIn.notKept` ("Stay signed in is not available on this computer, so your sign-in lasts for this app session."), which tC4 translates in all ten languages, rather than TC4-ISSUES' bare "Your sign-in lasts for one app session.", a new key that would ship English in nine. Phase 0 has no keychain (E1a adds `OsKeychain`): a keychain in memory forgets at quit and is not "Stay signed in on this computer", so the product hides the box until then; a `tc4` started with `--test-hooks` keeps sign-ins in a working test keychain that `test/keychain` sets.)* |
| `door43`, `exchange`, `import` | `SyncPort`, `ExchangePort`, `ImportPort` | map reports to view types and refusals to §3.5 |
| `export` | tc-export | warns while conflicts are open (STORAGE:2696-2697); checkpoints first from D1 |
| `feedback` | `ReportLog` | port of `tC4:src/data/feedback.ts:7-31` *(A5, D8: `tc_app::feedback`. The attachment is `translationCore {version} ({commit}) · {platform}`, then `no Report`, or `Report: {op}, {endedAt}` and the report as JSON indented by two spaces in tC4's `Report` key order: `op`, `ok`, `code` when a refusal has one, then the rebuild's `key` (a refusal's sentence) or `reason` (what failed without one), `facts`, `startedAt`, `endedAt` in `toISOString`'s form. `{platform}` is the server's `std::env::consts`, `Linux x86_64`, `Windows x86_64` or `macOS aarch64`, not `navigator.platform`, which says `MacIntel` on Apple silicon and `Win32` on 64-bit Windows (tC4 #731). The text is English whatever the app language, as tC4's; the server composes from its own log, which `test/reset` empties, so a parity capture shows `no Report` as tC4's does.)* |

### 4.8 Plans and proposals

Upgrades, gateway changes and conflict choices that build values follow one pattern. The server
computes the plan or proposal on a snapshot and keeps it with the heads it read. The dialog shows
the consequences. Confirming sends only the id. The actor checks the heads (STORAGE:1603-1606)
or refuses with `PlanStale` or `LookAgain`, and the UI shows the dialog again, as tC4 does
(`tC4:src/state.jsx:4800-4806`). Journal `Record` values never enter tc-api.

A confirming button ("Keep this version", "Apply", Keep on a draft offer, Create) is disabled
while its request is in flight, and one dialog sends one `RequestId` for its confirmation. A
second click therefore gets the first click's reply from the reply cache: never a false
`LookAgain`, a second change, or a race with the folder rename of a create. *(A-27)*

### 4.9 App data

All files are written by one `PrefsWriter` task with `tc_store::atomic_write`, except
`app-lock.json`, which tc-server writes before the app exists. It debounces
place as tC4 does (500 ms; `tC4:src/state.jsx:3818-3845`), and place is recorded when the person
moves (mode, book, chapter, the verse an editor opens on), not per keystroke. It keeps unknown
keys. A failed
write keeps the value for the session and adds a report. The files are on disk from C1a on
*(C1a, D4: `DiskFiles` over tc-store's `Vfs`, chosen by `FilesChoice::OnDisk` in `App::start`;
each write runs on the blocking pool, a write whose folder flush failed is kept and reported,
and the open removes what a crash left of this writer's own temporary files and nothing else;
the saved language survives a relaunch, tC4 #525)*.
The shell's `drafts-buffer.log` is not among them: the shell writes it (AD21).

| File | Content |
|---|---|
| `prefs/app.json` (not in STORAGE §3.7, STORAGE:1093-1103) | `schemaVersion`, `appLocale`, `lastUsed`, `lastEdit`, `alignSuggestions` |
| `prefs/<projectId>.json` | place per book or story, `draftUnit`, OBS story and recency |
| `summaries/<folder hash>.json` | §4.5 |
| `reports.json` | the last 50 operation reports, for Feedback and the dev Inspector, kept across launches *(C1a, D5: read before the launch's first report, so ids go on from the highest one the file named; written a second after the first change and at once at quit, leave and `test/reset`. Declared difference from tC4, which keeps its reports in memory only, so after a crash and a relaunch its "Ask for help" attaches nothing of what went wrong)* |
| `uploads/`, `downloads/` | temporary; emptied at start and after use |
| `resources/` | the machine's resource store (STORAGE §3.7), opened by `App::start`. A first launch with `--bundle <dir>` copies the bundled English suite into it as the `Seed` job (§4.6); `resources/.partial/`, where an interrupted copy or download stays, is emptied at each start. A leftover that cannot be removed (on Windows a scanner may hold one of its files) stops nothing: installs land by renaming a staging folder of their own name, so the start goes on and adds a `resources` report naming the folder, and the next start tries again. Only a `resources/` folder that cannot be made stops the start, as an app-data folder that cannot be made does (`AppData`, exit code 4) *(A9)* |
| `app.lock` | `tc4 serve`'s guard for the restart case: an old `tc4` still exiting after a crash (AD39). The holder writes its pid to `app-lock.json` beside it (Windows lets no other process read a locked file), so a new `tc4` that waits for it can name the holder's process. The single instance on a machine is the shell's lock (§5.1) *(A-3)* |
| `app-lock.json` | `{pid, at}` of the `tc4` that holds `app.lock`, written atomically after it takes the lock; read only to name the holder |
| `drafts-buffer.log` | written by the shell, not by `tc4`: the mirror of the browser buffer, one appended line per change, a clean field written as a removal line (AD21) *(A-3)* |

`lastEdit` (the Resume card) is recorded on each acknowledged save and when an editor loses
focus, and written 5 s after the last change, or at once when the person leaves the project.
tC4 writes it 1 s after typing stops (`tC4:src/state.jsx:3821-3858`); the Resume card reads it
only on Home, after the leave has written it, so the longer delay changes nothing a person
sees, and typing no longer costs an fsynced app-data write each second *(A-20)*.

**The projects folder** (Q-4, built in A3). Without `--projects`, the folder is
`%USERPROFILE%\translationCore4\projects` on Windows and `~/translationCore4/projects`
elsewhere. A folder that exists is used. At the first launch, when neither the default nor the
local folder exists, the default is checked with STORAGE §5.0's cloud detection; if a client
copies it, the app creates and uses `%LOCALAPPDATA%\translationCore4\projects` (macOS
`~/Library/Application Support/…`, Linux `$XDG_DATA_HOME` or `~/.local/share/…`), which no
client copies, so projects do not open read-only; if both are copied, the default is created
and each project's open says why. The choice is recorded in the report log. Later launches find
the folder that exists, so nothing needs to be stored before S1. When both folders exist, the
local one wins: nothing in the app creates it except that first launch, so it holds the
projects the person already works in, and a default folder made later by hand or by a sync
client must not replace it. A launch never lists fewer projects than the one before it. The
default folder's own projects are listed too, beside the local folder's; the report names that
folder (`localBesideDefault`, `alsoListed`), so "Ask for help" explains where they are. Where the default
folder is the chosen one, the app lists the local folder too when it exists, which only a copy of a
read-only project made (Q-30, C6 D9): Home shows its projects with the others, and a name never opens
two folders. A `--projects` folder is used as named; if it does not exist the
start stops with its path, because an empty folder made in its place would look as if the
projects were gone. Declared difference from tC4 (owner's answer Q-4): tC4 has no such
check (its profile is under the home folder, `tC4:scripts/desktop-bootstrap.cjs:29-32`), and
it was not built for a synced folder. Telling the person on Home where their projects are is
left to the owner. The check is a copy of
tc-store's, with one difference reported to the storage team: Dropbox's settings
folder `~/.dropbox`, which holds `info.json`, is not a sync marker (tc-store's copy calls
every folder under such a home synced). C1a keeps the copy until R28 (D6): tc-store's check also
reads the process's environment, so the first-launch rule could no longer be tested for a
Windows or OneDrive computer on Linux, and tc-store's open uses the same check for every
project (risk 23). Test builds also keep `test-hooks/fields.json`, the
fields of `test/fieldStore` (§5.8), written atomically before each `Ack`.

---

## 5. tc-server

### 5.1 Start, address and one instance

- translationCore is a single instance on a machine (owner's ruling of 2026-10-18; tC4 D39). The
  shell takes Electronite's single-instance lock, as tC4 does (`app.requestSingleInstanceLock()`,
  `tC4:scripts/desktop-main.cjs:108-111`). A second launch does not get it: it quits at once and
  starts no `tc4` server, and the running copy's window is brought to the front, restored if it
  was minimized, on `second-instance` (`tC4:scripts/desktop-main.cjs:134-140`). People work on
  one project at the same time from separate machines.
- `tc4 serve --app-data <dir> [--projects <dir>] --ui <dir> [--door43 <url>] [--bundle <dir>]`
  `[--backend memory --scenario <dir>] [--test-hooks] [--test-seed-pace <ms>]`. The last four
  exist only with their cargo features. `--scenario` writes the scenario's projects into the
  `--projects` folder through the store, so it needs `--projects`, and that folder must exist and
  be empty: a scenario never writes into a folder that holds anyone's work; `--backend memory`
  keeps them in the memory backend, which writes nothing *(C1a, D2)*. Without `--projects`, tc-app chooses the projects folder
  (§4.9, Q-4). A `--projects` folder that does not exist stops the start with its path: an empty
  folder made in its place would look as if the person's projects were gone. `--bundle <dir>`
  names the bundled resource store the installer ships, read only, with its `BUNDLE-MANIFEST.json`
  (§4.6); a first launch copies it into `<app-data>/resources` as the `Seed` job (§4.6), a folder
  that lacks an install its manifest lists ends the start `Failed`, and without the flag nothing
  is seeded
  (development, tests, the parity captures, Q-11). `--test-seed-pace <ms>` makes the seed wait
  that long before each file, so the process tests and Playwright can watch a seed and close it
  on any disk; a release build refuses it as an unknown flag (`release_has_no_test_hooks`)
  *(A9)*.
- It takes `app.lock` in app data, an operating-system lock it holds until the process ends (a
  crash included), and then writes its pid to `app-lock.json` beside it, because Windows lets no
  other process read a locked file. The lock is only its guard for the restart case: an old `tc4`
  may still be closing after a crash or after its shell died (AD39). If that `tc4` holds the
  lock, the new one waits up to 10 s for it, printing nothing and binding nothing, so the shell
  waits longer than that for the address line; then it prints
  `{"error":{"type":"appLockHeld","value":{"pid":<pid or null>}}}` as its only line, names the
  process on stderr, and exits with code 3. A start that fails otherwise prints
  `{"error":{"type":"start","value":{"reason":"…"}}}` and exits with code 4. The app starts
  before the address line is printed, so a start that fails (a `--projects` folder that is not
  there) prints that one line and never an address the shell would go to. *(A-3; A2: the pid
  is beside the lock, and the two error lines are what the shell reads; the join: the order of
  the app and the address line.)*
- It binds `127.0.0.1:0` and prints one JSON line before anything else:
  `{"url":"http://127.0.0.1:PORT/","token":"<43 chars base64url>"}`. The token is never logged.
- On stdin `{"quit":true}`, when stdin closes (the shell died; on Windows this needs the shell
  to start it detached, AD39), on Ctrl+C or SIGTERM, and when the UI calls `app/quit`, it quits:
  new requests get 503; requests being answered, those whose body is still arriving among them,
  get up to 1 s to finish, a body that outlasts that second gets 503 and is not run, and a call
  whose client went away still runs to its end; then it saves
  every aligner session's unsaved links (AD18), checkpoints every session and closes the actors.
  The whole close takes at most 5 s: what was written stays, and work still running then is cut
  (a cut checkpoint runs again at the next open). It then ends the event streams, answers
  `{"closed":true}` if stdin is still open, and exits. *(A-3, A-4; A2: the drain comes before the quit work, so a save accepted before the
  quit reaches an open backend and its reply reaches the UI. The first draft quit first and
  drained after, which let a save in flight meet a closing backend. FIX-quit: a request the
  server accepted before the quit gets its reply even when its body is read during the drain;
  the first rule answered it 503 whenever the quit began while its body was being read, though
  the backend was still open and the drain was waiting for it.)*

### 5.2 Security

A tower layer checks, in order:

1. `Host` equals `127.0.0.1:PORT`, else 403 (DNS rebinding, ARCH:45-46).
2. `X-Tc-Token` matches in constant time, else 401. This applies to every route, static files
   included (ARCH:42-44). The shell opens the window at `url#t=<token>`. A request with the header
   more than once, or with a comma-separated list, is accepted only when every value is the token:
   Playwright's `extraHTTPHeaders` and the UI's own header can both set it, and a wrong value
   anywhere is a 401. *(A2)*
3. On any method other than GET, `Origin` equals the server's own origin, else 403.
4. On `/api/`, `X-Tc-Api` equals 1, else 409.
5. No CORS header is ever sent.

How every request carries the token:

| Client | Mechanism |
|---|---|
| The UI's `fetch` calls | the UI reads the token from the URL fragment, keeps it in `sessionStorage`, and sends the header |
| Shell window: `index.html`, scripts, CSS, fonts, `<img>`, help `<iframe>`, `<a download>`, the hidden print window | the Electronite main script adds the header with `session.webRequest.onBeforeSendHeaders`, whose URL filter lists only the server's paths outside `/api/` (AD11). API calls and the event stream do not run the filter's JS listener, but, like every request of the window, they cross Electron's request proxy on the main process's UI thread, so that thread must stay free of synchronous work *(A-14)* |
| Playwright | `extraHTTPHeaders` on the context |
| A developer's own browser | debug builds only: `/?dev-token=<token>` once sets an `HttpOnly; SameSite=Strict` cookie, which step 2 then accepts, and redirects to `/#t=<token>` so the UI's own calls send the header |

Chromium writes `sessionStorage` to the profile's `Session Storage` folder, so the launch's token
reaches the disk there (G0 found it after its runs). It opens nothing once that launch has ended.

If the spike G0 or unit G1 finds a request type that `onBeforeSendHeaders` misses in Electronite v37, `/media`,
`/download` and `/print` switch to capability URLs (an HMAC of kind, project and path under a
per-launch key). The change is confined to tc-server and one UI helper.

Headers on `index.html`: `Content-Security-Policy: default-src 'self'; script-src 'self'
'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src
'self'; frame-src 'self'; object-src 'none'; base-uri 'none'` (inline styles carry tC4's DOM),
`Referrer-Policy: no-referrer`, `X-Content-Type-Options: nosniff`.

Tests (`tc-server/tests/security.rs`): `request_without_token_is_401`, `foreign_host_is_403`,
`foreign_origin_post_is_403`, `no_cors_headers_ever`, `binds_loopback_only`,
`dev_cookie_only_in_debug_builds`, `help_never_redirects`. Lifecycle tests
(`crates/tc4/tests/lifecycle.rs`, against the binary; tc-server's own `tests/lifecycle.rs`
drives the quit sequence in process): `closed_stdin_quits_within_5_s`,
`new_tc4_waits_for_the_old_one_then_names_the_holder` (the `app.lock` wait for the restart case)
*(A-3)*. The second launch is G1's test, in the shell:
`second_launch_focuses_the_running_window_and_starts_no_server`.

### 5.3 Routes

| Method, path | Kind | Notes |
|---|---|---|
| `GET /` | static | `index.html`, `no-store`, with `boot.js` |
| `GET /ui/<build>/*` | static | wasm, JS glue, `boot.js`, CSS (tC4's `styles.css`, tokens, `ui.css`), fonts, catalogs, in a folder named by a hash of its files (§3.10); immutable |
| `GET /help/*` | static | vendored help pages as `text/html`, never a redirect (Q28, DELTA:928-944); the page's shim `crates/tc-ui/app/help-shim.js` swaps them in the frame, as tC4's help panel does (`tC4:src/views/AccountMenu.jsx:182-274`), and the server serves them unchanged, so `help-pages.txt` checks every byte *(A5, D3, D4: DELTA-10-16 Q28 recommended a shim injected into the frame; tC4 runs the swap from the parent page, and pages served as vendored stay checkable)* |
| `GET /prepare.html` | static | first-launch window: tC4's `scripts/desktop-preparing.html` with its words in the saved app language (DELTA Q25; English until translated, Q-10). One file, `crates/tc-ui/app/prepare.html`, with its own CSP (the server sends none for this route); it reads `app/hello` and the event stream and goes to `/` at `Ready` *(A9)* |
| `GET /api/v1/events` | stream | §5.4 |
| `POST /api/v1/{service}/{name}` | typed | §3.9 |
| `POST /api/v1/uploads` | upload | §5.5 |
| `GET /media/obs/{pack}/{path}` | media | a picture of an installed picture pack for an `<img>`: `pack` is the pack's repository and commit in one segment (`git.door43.org~uW~obs_images_360~<sha>`, only the suite's pack), `path` the file below its `ingredients/`, and only a `.jpg`, `.jpeg`, `.png` or `.webp` that cannot leave the folder is served; the page may keep it for an hour, since the pack's id names its commit; anything else is a 404. A story project's own pictures are not served until a project that carries them can be had *(B7 D5)* |
| `GET /download/{id}/{name}` | media | a finished export, `Content-Disposition: attachment` |
| `GET /print/{job}` | media | print HTML for the shell's `printToPDF` and for `page.pdf()` |

`every_endpoint_is_routed_once` checks the router serves exactly `tc_api::ALL`. Each API call runs
on a task of its own, so a client that goes away (a reload, a closed window) never cuts a call
short, and the quit waits for calls still running (§5.1). *(A2)*

**Who opens the first-launch window** *(A9; a contract for G1 and A5, not built in A9)*. The
shell shows its window on every launch within 1 s of the click, painted in the app's background
colour, with no splash screen (owner ruling, 2026-10-18; `docs/PERFORMANCE.md` "Start-up"; tC4
#743). This section decides only which page the window loads: `/prepare.html#t=<token>` when
`hello.startup` is `Preparing` and still is 300 ms later, and `/#t=<token>` otherwise. The
window shows counts only, never resource names, as tC4's page does. It goes to `/#t=<token>` by
itself at `Ready`, and stays on its failure words at `Failed`, with the reason under them (§6.12).
A UI page that boots while `hello.startup` is not `Ready` goes to `/prepare.html` with its
fragment (A5's `App`, which owns the root). *(A5, D14: built. `boot::startup_action` decides,
host-tested; `boot()` replaces the page with `/prepare.html#t=<token>` right after the build
check, before the catalogs are installed or anything mounts, for `Preparing` and for `Failed`,
whose words the window shows; `prepare-smoke.mjs` proves it.)* The `Seed` job's failure is `JobState::Failed(
ApiError::Internal { report })`, the report being the `seed` report: no refusal family describes
a damaged resource copy, and "Ask for help" attaches that report. A9's own proof drives the page
in Chromium against the real `tc4`, with the request filter of AD11
(`crates/tc-ui/app/prepare-smoke.mjs`).

### 5.4 The event channel

- `GET /api/v1/events?after=<seq>` answers `text/event-stream`: `id: <seq>`, `data: <json>`, and
  a comment line every 15 s.
- The server keeps a ring of 4,096 frames. Progress frames are coalesced: the newest per job
  replaces older ones, and so does the newest startup progress frame (a `Ready` or `Failed`
  frame removes the last one and is itself kept) *(A9)*. A reconnect with a sequence the ring no
  longer holds gets `Resync`, and the UI reads every live query again. Every query is
  idempotent. The ring is tc-app's `EventBus` (§4.1); tc-server writes its frames as server-sent
  events, adds the heartbeat and bounds each connection's queue.
- Each connection has a bounded queue. A reader too slow for it is disconnected and reconnects.
- The UI reads the stream with `fetch` and a `ReadableStream` reader, and parses the frames in
  `tc_ui::app::events`, a host-tested parser.
- **The UI's reading rules** *(A4a)*. Its first connection asks `?after=0`, so the bus replays
  every frame since launch, or sends one `Resync` if the ring has wrapped, and applying the
  frames in order converges on the current state (none is suppressed as old). It reads
  `app/hello` only after the stream's response head has arrived, so the subscription is made
  before `hello`'s snapshot and no frame can fall between them. A `Resync` is numbered like the
  newest frame, which may be lower than the reader's `after`, so it is never skipped as a repeat
  and sets `after` to its own number; any other frame with `seq <= after` is a repeat and is
  skipped. A frame that does not parse is handled as a `Resync` (every live query is read
  again), never dropped, and `after` moves past its `id`. A frame of another `ServerInstance` is
  a restart (§3.10); the jobs the UI holds are forgotten, because the new run numbers its jobs
  from the start, and so are the aligner save states, the checkpoint outcomes and the blocked
  files (`effects::forget_run`), because the new run numbers its aligners the same way and its
  opens from epoch 1 again, so an entry under one of those keys would belong to another thing. The reader aborts a connection that sends no byte for 45 s (three heartbeats)
  and reconnects after 250 ms, doubling to 5 s, with the last `after`. After a `Resync` or a frame
  it could not read, the UI also reads each job it holds as active with `jobs/get`: `hello`
  carries no jobs, and the frame that ended one may have been in the gap. A reply replaces the
  held view unless a frame changed the job while the call was on its way (the frame is newer); a
  job the server no longer knows is forgotten. Without this a lost final frame would leave a
  progress indicator standing for ever, which PERFORMANCE rule 4 forbids.
- **What only events carry needs a read** *(A4a)*. The two repairs above work because a call
  returns the value: `hello` returns the internet switch, the account, the startup state, the
  open sessions and the saved language, and `jobs/get` a job. A value that an event carries whole
  and no call returns cannot be repaired after a gap: a lost `Checkpoint(Failed)` hides
  "Checkpoint not saved" from the translator, a lost `AlignSave` leaves the indicator on
  "Saving", and a lost `MaterializeBlocked` hides the file that was not updated. The rule for
  every unit that adds such an event is that the same unit makes the value readable, with a test
  that a `Resync` restores it, and says how in its row. Until then the UI keeps what it holds,
  and drops what belonged to an open that `hello` no longer lists. Phase 0's server emits none
  of these frames. D1 adds the last `CheckpointOutcome` and the blocked paths to `OpenSession`
  in `hello`, which the UI already reads after every gap. B6 reads the aligner view's `save`
  with each refresh of the view, which a `Resync` makes. `Notice` is readable through
  `session/notices`. `Received` and `Suggest` follow the same rule, and E2 and B6, which add
  them, say in their rows how each is repaired.
- **Ordering.** tc-store emits a command's `Committed` before it resolves the command's reply;
  the event reaches the bus through the session's event pump, so the UI may see the `Ack` or the
  `Changed` first and handles both *(C0: the first text said the server publishes the events
  before it sends the reply, which the pump's task does not promise)*. An
  `Ack` and a `Changed` event for the same write may arrive in either order at the UI; both
  carry a `ViewSeq`, and the UI keeps the newer. A `Changed` whose cause is
  `LocalSave { request }` for a request the UI has in flight changes no editor and shows no
  notice; the `Ack` settles the field (AD37). `events_reply_order_is_safe` is a property test
  over random orders, and it asserts that no notice names the person's own save *(A-6)*.

### 5.5 Uploads, downloads, pictures and PDF

- **Uploads.** The DOM keeps tC4's `<input type=file>` and drop zones
  (`tC4:src/views/modals/Import.jsx:63-68`, `AddBook.jsx:57-62`, `GuidedFix.jsx:62-68`). The UI
  posts each `File` as the `fetch` body, with its name percent-encoded in `X-Tc-Upload-Name`. The
  server streams it to `uploads/<id>` and returns `UploadView { id, name, size }`. It is
  `POST /api/v1/uploads` with the API's headers, the name in `X-Tc-Upload-Name` and the flow
  (`exchange`, `import`, `usfm` or `guidedFix`) in `X-Tc-Upload-Flow`. The server writes
  `uploads/<id>.part` and renames it to `uploads/<id>` after the last byte, and only then
  answers; a body over the flow's cap is 413, and a cut connection or a full disk leaves nothing
  (`UploadRefusal::NotWritten`). Nothing is fsynced: `uploads/` is emptied at every start, so a
  crash never reads a partial upload back. The UI checks a file's size against its flow's cap
  before it sends it (`UploadFlow::cap`), because a browser still sending when the server
  refuses may report a reset connection instead of the 413. The name is never a path. *(A2)*
  Caps follow
  the flow: an exchange file ("Receive work from a file" in a browser) may be 4 GiB, STORAGE's
  cap for the whole file, whose 512 MiB cap applies to each entry inside it (STORAGE:3027) and
  is checked when the file is read; every other upload (USFM, tC3 and tC4 projects, a guided-fix
  file) is capped at 512 MiB. *(A-28)* Parsing happens on the server.
- **Receive work from a file.** In the shell, `tc4Shell.chooseFile()` gives a path and the server
  reads the file in place. In a browser, an upload.
- **Downloads.** An `Export` job ends with `ExportReady { download, file_name }`. The UI clicks an
  `<a download href="/download/<id>/<name>">`, as tC4's `deliverFile` does
  (`tC4:src/data/export/kernel.ts:38-57`). Electronite shows the save dialog and reports saved or
  cancelled through `onDownloadDone`, which drives "Saved {file}" (`tC4:src/views/ExportMenu.jsx:26-34`).
  In a browser the toast says "Export ready: {file}". Files stay 10 minutes. A producer writes its
  file in `downloads/` and registers it (`Downloads::add`); the route answers only an id that is
  registered, with the name it was registered under, and streams the file. *(A2)*
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
   question (`tC4:src/state.jsx:4249-4282`). *(A8: a failed turn-on leaves the switch off, the dot
   says so, the step does not run and the next step asks again. tC4 turns its page flag on first
   and then reads the platform's gate, so a gate that fails leaves the flag on and the next step
   asks nothing and fails again (tC4 internet-consent f): a declared difference. tC4's f2, a gate
   that cannot be read, has no counterpart: the switch is the server's, with no separate gate.)*
3. The server refuses with `NeedsInternet` if the switch went off meanwhile; the UI asks once
   more. `NeedsInternet` says nothing was sent (§3.5), so the reply cache applies it only to a
   request id it has not seen: a retry of a write that ran, after the switch went off, gets
   that write's reply, never a refusal that says it did not happen. The refusal is never kept
   under the request's id, like a body that does not parse: the UI sends the same request
   again once the switch is on, and a kept refusal would answer that resend for ever. A job that loses the internet while
   it runs ends with its own refusal, for example `SyncRefusal::Offline`.
4. Turning the switch off stops the very next request: tc-door43 checks it before every request,
   before every download chunk is written and every 100 ms while a download waits
   (`crates/tc-door43/src/client.rs:576-609, 680`). tC4 only blocked in the client
   (`tC4:src/state.jsx:4296-4299`).
5. Sign-in sends login and password once, in the body of `account/signIn`, over the local API.
   tc-app mints a Door43 token, keeps it in memory, and with "Stay signed in" in the keychain. The
   token never reaches the UI; `AccountView { user, kept }` does. *(A8: the token is tC4's
   `translationCore` with its three write scopes, which replaces an earlier one of that name. A
   sign-in without "Stay signed in" forgets a token an earlier session kept (tC4 #366); a refusal
   keeps nothing and changes nothing. Keychain calls run on the blocking pool, never under a lock,
   because an OS keychain can prompt. Reports name the step, whether the token was kept and the
   HTTP status, never the login, the username, the email, the password, the token or Door43's
   message, which can name the account.)*
6. `account/state` reads only whether a token is kept. The first Door43 step resumes it. A 401
   forgets it and answers `NeedsSignIn`. `AuthFailed` during a sync forgets both copies
   (DELTA:735-736). A sign-in result that arrives after Cancel is dropped by the flow's generation
   (`tC4:src/state.jsx:5876-5881`). *(A8: the first Door43 step takes `App::door43_session`: the
   session, or the kept token resumed (the token read at start: the keychain is read once), or
   `NeedsSignIn(NotSignedIn | TokenRejected)`; a step that meets a 401 calls `App::door43_refused`,
   which forgets both copies of that token. Resume's outcomes are tC4's `ResumeOutcome`; a
   sign-out or sign-in that lands while a resume waits for Door43 wins. Cancel stops the sign-in
   wherever it is: `account/cancelSignIn { signIn }` names it, and a cancel that comes before its
   sign-in makes it send nothing, one that comes while it waits for its turn or for Door43 drops
   it there, and one that comes after it ended signs that session out and forgets the token it
   kept. In every timing nobody is signed in by that sign-in and no token it minted is kept. tC4
   lets a cancelled sign-in run to its end and then signs out, which also empties the keychain;
   here the next sign-in does not wait for Door43's answer to the cancelled one, and a token an
   earlier session kept survives a cancel: a declared difference, the person sees the same "not
   signed in", sooner.)*
7. Private repositories (Q3, PLAN:51-54): `door43/open` fetches without credentials first; on 401
   or 404 with no session the job ends with `NeedsSignIn(PrivateRepository)`, the UI shows M12 and
   runs the open again.
8. Links to a profile, a repository or help ask the question first, then `account/profileLink`,
   `repoLink` or `helpLink` gives the URL and the shell opens it with `openExternal`
   (`window.open` in a browser).
9. The Door43 server is `--door43`: QA in dev builds, production otherwise
   (`tC4:src/data/dcsServer.ts:65-84`), and the fake's URL in every test. *(A8, tC4 #120:
   `--door43` is the server of account and write calls, and M12 names its host (`signIn.server`);
   read calls (pins, catalog, downloads, pictures) never take it. A8 makes no read call; B9a, the
   first unit that downloads, proves the rest with `debug_build_downloads_resources_from_production`.)*

### 5.7 The shell bridge (`window.tc4Shell`)

`printPdf(url)`, `chooseFolder()`, `chooseFile()`, `openExternal(url)`, `setCanClose(bool)`,
`onCloseRequested(fn)`, `onDownloadDone(fn)`, `feedbackSend(report)` (the owner's answer to Q-2:
the UI asks the internet question with `net.ask.feedback` first, and the main script reads
`net/state` with the token before it sends, answering tC4's `offline` reason while the switch is
off), `bufferChanged(entry)` and `takeBuffered()` (the draft mirror, AD21), `platform`. Without the
bridge (dev, Playwright) the features degrade as tC4's do in a browser: no PDF item, Feedback
says it was not sent (`tC4:src/state.jsx:4143-4167`), and the buffer lives only in
`localStorage`. Playwright injects a stub with `addInitScript` where a journey needs a folder, a
file path or the mirror.

*(A4b: the page's side of the buffer and close calls, exactly. `bufferChanged(line: string)`
takes one `BufferLine` as JSON (§6.6) and returns nothing; the shell appends it. `takeBuffered():
Promise<string[]>` returns the shell's current lines, the last line per field and server run of
the log it read at launch and of every line appended since, with the entries a removal of the
same run ended left out, on every call, not once: a reload in the same launch must not lose them,
and a field removed meanwhile must not come back. A removal line names its run
(`{"type":"removed","value":{"key":…,"instance":…}}`) and drops only that run's entry: after a
restart behind a reused port one field can hold a text of each run, and saving the new run's text
must not drop the old run's, which the next launch, at a new origin, finds only in the mirror
(`tc_ui::app::buffer::current_lines` is the reduction the shell's tests and the page's play).
The page merges them with its own entries, newest `at` per field and run, its own on a tie.
`setCanClose(clean: boolean)` is called on every change of the close guard's value.
`onCloseRequested(fn)`: `fn` returns `Promise<boolean>`; the page runs the navigation gate and
answers whether everything is saved: the gate passed and nothing is left unsaved, so text the
gate cannot save (a held field, a section card's buffer) still gets the question; the shell
closes at once on `true`, and asks its "Unsaved changes, close anyway?" question on `false`, or
when `fn` has not answered within 5 s. This adds to tC4's best-effort drain on `pagehide`
(`tC4:src/state.jsx:3699-3704`), which the page also does, a drain that finishes before the
window goes, so the last sentence is saved rather than offered at the next launch: a declared
difference.)* *(A4b fix: the removal line had no run, so the shell's "last line per field" let a
saved field of the new run drop the old run's unsaved text; and the close answered the gate
alone, which passes over held and section text, so the window closed without asking.)*

The shell's window-open handler keeps tC4's rule for other sites, sending `http` and `https`
links to the system browser (`tC4:scripts/desktop-main.cjs:128-131`), but a URL on the server's
own origin (help, a download) stays in the app: the handler allows it as a child window of the
same session, where the header filter adds the token. The system browser has no token and would
show a 401 page.
*(A-33)*

### 5.8 Test hooks (feature `test-hooks`)

| Hook | Use |
|---|---|
| `test/quiet` | `{busy}`: true while a request, job, actor queue or pending materialize is live; the capture's settle waits for false *(C1a, D13: on the store also while a store thread waits at a held point; tc-store has no non-blocking idle, so its background work after an open stays unseen until R30)* |
| `test/hold`, `test/release` | hold a book read, a checkpoint, a materialize, a save before B3, a draft write, or a sync step (R8); replaces tC4's `e2e/helpers/checkpointHold.ts:14` *(C1a, D13: on the store each point also holds tc-store's matching point until released: `save` its durability point, `bookRead`, `checkpoint`, `materialize`, `draftWrite`; `test/reset` releases them)* |
| `test/crash` | end `tc4` at a named point (after a reply is sent, before B3, while a draft is written) for the reload and relaunch tests of §6.6 *(A-2, A-3)*. The points are in tc-app's write path and apply to every write but the arming hooks (§10 A3): before the handler (`beforeStore`), after it (`beforeReply`), and after the reply, which the server does when `App::call` answers `then: EndProcess`. An arm that can never fire (a query, an arming hook, or `duringDraftWrite` before any write reaches it) is a bad request; the unit that adds draft writes (S1, C1b) accepts it. |
| `test/fieldStore` | a write endpoint that goes through the real reply cache and records every text it stores, with holds and crash points, so A4b can prove the save machine's no-loss rules before any real save exists *(A-2)*. Its tokens carry the field and its version under a key kept with the fields, so they verify after a restart; a store over an older version is logged as `stale`, and an unchanged text answers with the token it came with, as the store does (STORAGE:1446-1447, STORAGE §5.1 step 4). |
| `test/fieldLog` | a test field's current value with its token, and every text `test/fieldStore` stored into it, in order, each with the token base it was saved over (`current` or `stale`): what A4b's tests read back *(A1)*; and the newest draft `test/fieldDraft` kept with how many came *(A4b)* |
| `test/fieldDraft` | a test field's draft, as `editor/draft` will keep a verse's in `.tc/drafts.json`: its token is checked as `fieldStore` checks one, the newest text per field is kept durably beside the stores, and the field's value and stores do not change. It waits at the `DraftWrite` hold before it takes the stores' lane, so a held draft never delays a store (R22). A4b's third panic case shows with it that the server holds the drafts `boot.js` posts (§6.8), before any real draft endpoint exists. `duringDraftWrite` stays refused: the unit that adds real draft writes accepts it *(A4b)* |
| `test/clock` | a fixed clock for dates in screenshots |
| `test/script` | queue scripted save or job outcomes (memory backend and port doubles) |
| `test/door43` | configure tc-door43-fake faults, including R13's dropped connection *(A8: not a hook: the fake runs as its own process with a control port on loopback that tests call directly; R13's dropped connection, when it comes, goes there)* |
| `test/keychain` | *(A8)* `{ mode?, holds?, relaunch }` → `{ mode, holds, keeps, reads, forgets }`: sets the test keychain's mode (`working`, `none`, `failingKeep`, `failingForget`, `unreadable`) and what it holds, then, with `relaunch`, reads the account as a launch finds it (sign-ins in flight cancelled, the session forgotten, the keychain read once). The token never comes back. Journeys use it for a sign-in kept at launch, a keychain that cannot forget, no keychain and the next launch, which tC4's journeys do with `fakeKeychain` and a reload |
| `test/inspect` | fold facts journeys assert: heads, values, conflicts, commits, `.tc/` files |
| `test/writeTeammate` | teammate changes through tc-journal and the store's sealing API (AD36) |
| `test/counters` | operation counters |
| `test/reset` | reset prefs, summaries and reports to the scenario seed (on the store the scenario's projects too: each one opened since it was written is written again, every other entry of the projects folder is removed, C1a D3), release every hold, disarm the crash, and turn the internet switch off as a launch finds it (tC4's switch is page state that a reload turns off, `tC4:e2e/helpers/door43Share.ts:21-25`), and forget the kept replies, so a request id may be used again in the next scenario (a write still running is kept, so its retry still waits for it); and sign out, the test keychain back to working and empty, so one captured screen's sign-in cannot reach the next *(A8)*; the harness's `home()`. The reset keeps no reply of its own either: running it again changes nothing, and a harness whose ids restart with each scenario sends it first with the id it used before, which a kept reply would answer without resetting anything |
| `test/lastEdit` | records a Resume record and the project's last use through the calls a save makes (C2), waits until they are written, and publishes `ProjectsChanged`, so the Resume card can be captured before any save exists *(B2 D12)* |

---

## 6. The UI

### 6.1 Data flow

- `use_app()` returns `App { s, actions, locale }`, provided once at the root. Field and action
  names are tC4's in snake case (`s.projects`, `s.book_model`, `actions.go`,
  `actions.open_project_at`, `actions.record_decision`), so each tC4 view file becomes one Rust
  module read top to bottom.
- Behind `s` there are three sources, and each value has one owner:
  1. **Server state**, from two places. Reads that screens hold, refcounted and shared by request,
     are in a `QueryCache`: projects, cards, the book model, helps, the check session, the aligner
     view. Values that arrive whole in `hello` and in events live in `ServerState` signals that
     exist for the life of the page. `hello` holds five of them: the internet switch, the
     account, the startup state, the open sessions and the saved app language. Events update
     them, and a `Resync` reads `hello` again. Jobs are in no read of `hello`: events carry
     them, and a `Resync` reads each job held as active with `jobs/get` (§5.4). The last
     checkpoint outcome, the files a materialize cannot write and the aligner save states are
     in neither: **only events carry them**, so no read restores one that a gap lost. A `Resync`
     keeps what the page holds of them, because a failure is never cleared by a gap, and drops
     what belonged to an open that `hello` no longer lists. A restart is not a gap: the new run
     numbers its opens and aligners again, so the page forgets them with its jobs (§5.4). The units that emit these events
     make them readable (§5.4): D1 for the checkpoint outcome and the blocked files, B6 for the
     aligner save state. These fields of `s` are read-only signals. *(A4a: the first draft put all
     of them in the cache. A cache entry with no holder is forgotten, and these values must exist
     before any screen holds them. It also said that `hello` seeds all of them; the review of the
     runtime found that `hello` holds none of the last three, so a `Resync` could not restore a
     "Checkpoint not saved", a "can't update TIT.usfm" or an aligner's "Saving" lost in a gap.)*
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

- `api.call(&req).await -> Result<E::Ok, CallError>`, with `CallError { Api(ApiError), Dropped,
  Transport(Transport) }` and `Transport { Unreachable(String), Status(u16), Unreadable(String) }`.
  A screen holds `Load<T> { Loading, Ready(T), Refreshing(T), Failed(CallError) }`. "Failed" is
  never drawn as "absent". *(A4a: the first draft had `-> ReplyOf<E>` and `Failed(ApiError)`.
  AD9 keeps HTTP statuses and network failures below the API, so a refused token, a proxy's
  page or a server that is gone are not `ApiError`s; and a reply dropped because its project
  closed must not look like a refusal.)*
- A *read's* reply is dropped when its scope is no longer current, when the server answered it
  with `ApiError::StaleScope`, or when its `ViewSeq` is older than the cached one. The query cache
  and `Runtime::read_once` (a read an action makes itself) apply the rule. **A write's reply is
  never dropped**: the save machine needs every `Ack` and every `NotSaved::StaleScope` (§6.5,
  A-19). *(A4a)*
- A `Session::Changed` event whose scope a query `depends_on` moves it to `Refreshing` and fetches
  it again. The old data stays on screen; a book waiting behind a commit shows as pending, never
  as empty (DELTA:625-626). `Resync` refreshes every query. The check rail is the exception: it
  is patched by row (AD17), and a `Changed` after a `LocalSave` whose `Ack` already carried the
  rows sends no request *(A-12)*.
- A browser tab cannot carry the per-launch token that the shell adds (§5.2), so outside the
  shell only the dev server and Playwright open the page, and tC4 removed its tab guard as dead
  code (D39). The one-tab check of RT-19 is removed by the owner's ruling of 2026-10-18 *(A-19)*.
- Shared reads are shared: Understand and Translate use one helps query, as tC4 shares
  `useLoadHelps` (`tC4:src/views/HelpsPanel.jsx:474-480`). *(A4a: within one server run. A restart
  numbers the opens again, so the same request, with `TIT` at epoch 1 in its body, names another
  open in the new run. The registry keys a read of a project by its run as well as its request
  (`ReadId`, which `hold` hands to the holder), so a screen of the new run gets a new read and
  a fetch while a screen of the old run still holds the dead one, and the old screen's release
  cannot take a holder from the new read. A read of the app has no run: a restart does not end
  it.)*
- A questions table (en_tq, en_obs-tq) is read as tC4's `deriveTqItems` reads it
  (`tC4:src/data/derive.ts:323-348`): one bad row (an `Occurrence` of `x`) costs only its own
  occurrence, not the table. Declared differences from tC4, each only for a damaged resource:
  a row whose `ID` is not a row id is left out, where tC4 shows its question, because a
  question here is keyed by a valid id; and an OBS row whose `Reference` has a third part
  (`1:2:3`) is left out, where tC4 shows it on frame 2 of story 1, because an OBS address here
  is a story and a frame only (`tc_helps::tsv::parse_questions_leniently`, DOM-2).
- `window.__tcIdle` is true when no fetch, save or job is in flight. *(A4b: a save counts while it
  is in flight and while its timer is armed, so a test that waits for idle sees a save that is
  due; a section card's buffer, which saves only on Save, does not keep the page busy.)*
  `window.__tcHeld` is `{jobs, checkpoints, blocked, alignSaves}`, how much of what only events
  carry the page holds, kept current for the smoke *(B2 D14: it moved off the stand-in Home)*.

### 6.3 Subscribing

One task reads the stream and dispatches:

- `Changed { scope }` re-queries only the chapters, items or cards in scope. Open editors follow
  AD37 *(A-6)*:
  - An editor holds the value and the token it opened with as one pair, and takes the stored
    value and the `Ack`'s token as its pair again when an `Ack` leaves it clean. If the editor has had
    no input and no composition since it last took its pair, a refresh replaces the pair together, unless
    the editor has focus: then the new pair waits and is applied on blur, so nothing is written
    into a focused field. If the person types before that blur, the waiting pair is dropped and
    the next rule applies. *(A4b: "since it opened" read literally would show "Juan changed this verse while you were
    typing" under a teammate's later change to a field that was typed into, saved and is clean,
    although nothing unsaved remains and no second version will be kept. A refresh that comes
    while the field's own save is in flight is judged once its reply has come: it may be that
    save's own refetch, which no `Changed` frame needs to name if a gap lost it.)* *(A4b fix:
    "newer" compares views within one open. `ViewSeq` counts per session (§3.2), so a pair, an
    `Ack` and a `LocalSave` carry `save::View { epoch, seq }`, ordered by the open first: a
    folder's epochs only grow in one server run, and a restart drops every field that could hold
    a view of the run before. Comparing the bare counts kept the first open's view 41 above the
    reopened project's view 2, so an editor reopened on a teammate's change showed the old text
    with the old token, and a field typed into got no notice. A clean field with no editor is
    forgotten once its open ends.)*
  - Once the person has typed, the field keeps its text and its base token. If the refreshed
    value differs from the base (compared in NFC), the field shows "Juan changed this verse
    while you were typing. Your text will be kept as a second version." (STORAGE:2978-2981),
    naming the author from `ChangeScope.records`. The later save becomes a conflict, never a
    revert.
  - A `Changed` caused by the field's own request (`LocalSave { request }`) shows nothing.
  - After an `Ack`, a focused field keeps the person's text as typed; the field is clean when
    `Ack.saved` equals its text in NFC, and the stored NFC form is shown only after blur.
  - Tests: `refresh_under_open_clean_editor_swaps_value_and_token_together`,
    `refresh_under_typed_editor_keeps_text_and_token`, `own_save_event_before_ack_shows_no_notice`,
    and a Playwright case that types Arabic through an IME composition while a refresh and an
    `Ack` arrive.
- A `Changed` event with `scope.conflicts`, and `new_conflicts` in an `Ack`, update the markers
  and chips at once.
- `Checkpoint(Failed)` sets "Checkpoint not saved" with Retry (`tC4:src/App.jsx:63-67`). The
  last outcome is also in `hello`'s `OpenSession` (D1, §5.4), so a `Resync` brings it back.
- `Job` events feed the open overlay and the busy labels. The overlay (S14) is a port of tC4's
  `OpenProgressView`: its own timer shows it 300 ms after the person's action
  (`SHOW_THRESHOLD_MS`, `tC4:src/views/OpenProgress.jsx:16-40`), whether or not a progress event
  has arrived. Its three stages are tC4's; STORAGE's five phases map onto them (Q-7), and a stage
  with no count yet draws tC4's indeterminate bar. *(A-22)*
- `HomeCard` makes a mounted card read again.

### 6.4 Shown tokens in the UI

- A token is stored next to the value it stands for, inside `Shown<T>`. The UI never builds,
  parses or compares one beyond equality.
- A save sends the token of the value-and-token pair the editor holds (AD37), never one fetched
  later. *(A-6)*
- An `Ack` replaces tokens per record.
- Tokens never enter the DOM. `no_token_in_dom` scans every journey checkpoint for the wire
  prefixes `st1.` and `rt1.`, so a commit sha or another long id cannot trip it and no exemption
  is needed. *(A-7)*
- `WriteCap` is a zero-sized value that exists only while the session mode is `Writable`.
  Editors take `Option<WriteCap>`. A read-only open or a `SecondWriter` switch removes it: the
  edit controls turn off and the drafts stay. When the person takes the offered copy of a
  read-only project, the copy flow moves the folder's buffer entries and `drafts.json` entries
  to the copy and offers them when it opens (§6.6), so text typed before the switch is not
  left behind. *(A-10)* *(C1b: the mirror's entries move too, and the page holds its own
  fields on in the copy, §6.6.)*

### 6.5 Saving

**The save machine.** `tc_ui::app::save` *(A4b: beside A4a's other pure runtime modules, which
are all under `tc_ui::app`; the first draft named it `tc_ui::logic::save_machine`)* is a host-tested port of
`tC4:src/data/saveScheduler.ts`. Per field it holds `current`, `persisted`, the base tokens, and
a state:

```rust
pub enum FieldSave { Clean, Dirty { since: f64 },
                     Saving { sent: SentSave },          // request, text and scope (§6.6)
                     Failed(SaveError), Held(Hold), SavedWithWarning(String) }
pub enum SaveError { NotSaved(NotSaved), Api(ApiError), Transport(Transport), BadAck(&'static str) }
pub enum Hold { NoOpen, ReadOnly(ReadOnlyReason), Restarted }
```

*(A4b: `Failed(SaveError)`, because a write can also end in `Unavailable` (every save before S1),
`Internal`, `BadRequest`, a refused token or an `Ack` the UI cannot read, and each shows "Not saved
… Retry" instead of vanishing. `Held` is a field whose text stays and cannot be saved now: its
project has no open after `StaleScope`, or is read-only (A-10); it does not block the navigation
gate and its entry stays for the copy or the next open. `Hold::Restarted` (A4b fix) is a field
whose editor was on screen when the server started again (§3.10): its typing is buffered in the
old run's entry and it is never saved by this page. The machine returns commands (buffer
writes, mirror lines, saves, resends, drafts, a value to show, and a read of the sessions after
`StaleScope`) that `web::saves` carries out and
tc-app's tests carry out against `test/fieldStore`.)*

- One save in flight per field. The next save of a field waits for the reply and uses its tokens.
- On a reply: if `Ack.saved` equals the NFC form of the field's current text, the field is clean
  and its buffer entry is removed; otherwise it stays dirty on the new tokens (tC4's
  version-aware rule, `saveScheduler.ts:214-219`). Comparing with the stored text, not with
  `sent`, is what makes a retry safe after a reload *(A-2)*.
- An `Ack` that leaves the field dirty sends the field's `Draft` at once, with the reply's tokens,
  outside the once-a-second limit (C2). R26 already preserves a newer durable draft before Ack;
  this refresh rebases the current text for recovery after Ack. The held-save crash test proves
  recovery and conflict creation before Ack, and the save-machine regression proves the
  immediate refresh even when the same text was drafted on the older tokens.
- Section Cancel waits for the field's draft calls, then durably discards only the exact
  current draft (`editor/discardDraft`, STORAGE §5.14). It retains the card, buffer and mirror
  until success; a failure keeps the text and offers Retry. Newer input and independent
  offered drafts are never removed by an older Cancel.
- After a failure nothing saves automatically; only Retry continues (`saveScheduler.ts:314`).
  *(A4b second review: per field, a declared difference. tC4's scheduler holds one failure for
  all its keys, every book's verses or every note, and after it nothing of that scheduler saves
  until Retry (`saveScheduler.ts:310-314`); the machine keeps a state per field, so the other
  fields go on saving while the failed one shows Retry, the indicator shows the error and the
  gate stops. Their text can be saved, and holding it back would only widen what a crash can
  lose; FR-32's aim, that the error stays visible and a failing store is not hammered, holds per
  field.)*
- A lost reply (a dropped connection, or a UI reload with a save in flight) is retried with the
  same `RequestId` and the same body, rebuilt from the buffer entry's `sent`, never from the
  current text. The reply cache answers it, waits for it if the first attempt is still running,
  or answers `RequestReused` if the bodies differ. If the person typed more in the meantime,
  the field stays dirty after the retry's `Ack` and saves the newer text next. *(A-2)* *(A4b: only
  a lost reply keeps its id: no answer at all, a 503, or a reload. The reply cache keeps every
  answer of a write, refusals included, so Retry, the navigation gate's retry, the resend after
  `StaleScope` and the save after `RequestReused` each mint a new id; the old one would get its
  refusal back for ever. A live page resends after 250 ms, doubling to 5 s, and only after
  `app/hello` names the server run the field sent to: a run that started again has no reply
  cache and would store the text a second time over its old token. A restarted `tc4` has a new
  token, so the old page's requests get 401 first; the `hello` check covers a page that can
  still reach the new run. A reload retries only an entry of the run that `hello` names (§6.6).
  The usual timer after a reload exists only for a field that saves on a pause; a restored
  comment or check comment waits for an editor's blur or the navigation gate.)* *(A4b second
  review: `Command::Resend` carries the run the request went to, and once the `hello` has come
  the machine decides (`SaveMachine::resend_check`): the request goes only if the `hello` names
  that run, the machine's run is still that run, and the field still waits for that very
  request; a `hello` of another run takes the runtime's one restart path. The first build
  compared the `hello` with the machine's run after the `await`, so a restart that the event
  stream's `Resync` found while the `hello` was on its way let the old request go to the new
  run, which stored the text a second time over its old token, as a conflict with the person's
  own save.)*
- `NotSaved::StaleScope` (the project was closed or reopened under the field) keeps the text:
  the UI reattaches to the current session and resends with the field's base token, so a change
  made meanwhile becomes a conflict, never a revert *(A-19)*. *(A4b fix: the reply and the
  session event of a reopen come in either order (§5.4). The field keeps the open that refused it
  and never sends to it again, since an epoch is never reused; while the page still holds that
  open, the field waits, Dirty and with no `sent` in its entry, asks for the sessions once
  (`Command::ReadSessions`, which reads `app/hello`), and sends once they name another open, or
  holds when they name none. The gate waits for it and `__tcIdle` counts it. The first build
  resent at once to the open the page still held, with a new id each time and no limit, until a
  frame or a `hello` arrived.)*
- `drain()` resolves false while a failure stands (`saveScheduler.ts:139-146`).
- A section card's restored buffer is never saved by anything but its Save: not by the timer,
  not by a reload, not by the navigation gate *(A4b)*.
- Async completions update signals with `try_update` and read them with `try_get`, so a reply
  that arrives after its screen was disposed does nothing instead of panicking (`reactive_graph`
  0.2.16 panics on a disposed signal); tc-ui's app modules also enable `clippy::indexing_slicing`
  *(A-15)*.
- Tests port tC4's scheduler tests, plus two proptests: no `current` is dropped while
  `persisted != current`; and `reload_anywhere_stores_the_last_text_once`, which reloads the UI
  (and, in a second run, ends `tc4` through `test/crash`) at every point of a random interleaving
  of typing, saves, replies and `Changed` events, and checks that the last text is stored
  exactly once and no earlier text is marked clean in its place *(A-2)*.

**Timing** (AD20). Each field keeps tC4's own rule:

| Field | Saves | Between saves |
|---|---|---|
| verse, layout | on blur, or 2 s after the last input | buffer and `Draft` |
| section editor | only on Save; Cancel discards the card, its buffer entry and its draft; a Save that leaves the section with no words asks first (#600, `tC4:src/views/SectionEditor.jsx:8-9, 223`) *(A-1)* | buffer and `Draft` on blur and after 2 s; placing pins writes nothing |
| user comment | on blur; a pause writes only the draft *(A-18)* | buffer and mirror; `Draft` once R15 exists |
| OBS frame, title, reference | on blur, or 2 s | buffer and `Draft` |
| alignment | by the server: 2 s after the last link change, on leaving the verse, when the session is replaced or closed, and before the quit checkpoint (AD18) *(A-4)* | the server's `Aligner`; R23 asks for a draft |
| check comment | on Done, and on leaving the item while its editor is open *(A-10)* | buffer and `Draft` (`DraftField::CheckComment`) |
| decision (status, selections, nothing-to-select, bookmark) | at once, on each act | — |

A count test in B4b: typing, placing pins and blurring in the section editor send no
`editor/saveSection` until Save *(A-1)*. tC4 drops a check comment that is being typed when the
checker moves to another item (`tC4:src/views/Check.jsx:815-825`); the rebuild saves it, a
declared difference listed in `residuals.json` (Q-25).

**The Understand comment box** *(A-18)*. tC4's box edits one note per target. It shows the note
at the unit's head verse, or the unit's only note, or none when several targets have notes, plus
a count of the others (`displayedUnitNote`, `tC4:src/views/Understand.jsx:40-56, 174-178`). It
refuses to clear a saved note (`understand.cannotClear`, `:138-145`). STORAGE comments are
append-only (STORAGE:1534-1541). The box keeps tC4's selection rule over comments, taking the
newest when one target has several. By the owner's answer to Q-24 (c), a blur after a change
revises the comment the box showed, through its displayed token (R24, `ReviseComment`), and
History keeps the earlier text; a box that showed no comment adds one; a pause writes only
the draft. Clearing a box that shows a saved comment is refused, as in tC4. *(C3: a new
comment's Add may commit before its Ack arrives. A recovered tokenless comment whose text
equals a head on its exact target settles; different text is offered as a new comment, never
rebased onto another comment, `.audit/plans/C3.md`.)* Test: a teammate's comment in the same
unit is never changed or hidden, three pauses and one blur add exactly one comment, and a
second blur revises that comment.

**The indicator** (X2) shows the worst state of all fields and of the open aligner's
`AlignSave`: error, then saving, then dirty, then saved (`tC4:src/App.jsx:57-62`). "Checkpoint
not saved" shows only when no save failed. A disk warning shows as a notice.

**The navigation gate.** Changing mode, book, story or project, going Home, exporting, printing,
Keep this version, and opening the gateway-change or upgrade preview first drain every field and
call `align/flush` for an open aligner. A standing failure keeps the person where they are
(FR-32, `tC4:src/state.jsx:4018-4024`, and before the gateway preview `:4373-4377`). *(A-4,
A-26)* *(A4b: exactly, `Saves::gate()` is tC4's `drainSchedulers` loop: it sends every field that
may save now, retries each failure that stood when it began once (a failure that first comes
during it is not retried, as tC4's `drain` does not), waits for every reply, runs the hooks
registered with `Saves::add_gate_hook` (B6 registers `align/flush`), and repeats while new work
appeared; it resolves false as soon as a failure stands after its retry. A section card's buffer
is never saved by the gate and does not block it: tC4's editing lock keeps the exits off while a
card is open, and a buffer restored after a panic stays a draft and is offered at the next open.
A held field does not block it either, since the person cannot save it. The gate has no timeout:
while a lost reply is resent the indicator says "Saving". A field that waits for its project's
new open after `StaleScope` keeps the gate waiting, as a save in flight does.)*

**The editing lock.** `Editing` is one enum: `None | Verse(VerseKey) | Section(SectionKey) |
Frame(FrameKey)`. Anything but `None` disables the exits DELTA:909-927 lists, in Bible Translate
and, by ruling Q21, in OBS Translate. Leaving a project clears it.

**The close guard.** The UI calls `tc4Shell.setCanClose(all_clean)` on every change, or registers
`beforeunload` in a browser (`tC4:src/state.jsx:3684-3721`). `all_clean` counts the open
aligner's `AlignSave`, as tC4's counts `alignSaveState` (`:3718-3720`). On close the shell asks
`tc4` to quit, which saves aligner links before its checkpoint (§5.1). *(A-4)* *(A4b: the browser's
`beforeunload` listener is `boot.js`'s, which the page keeps told with `tc4Boot.setUnsaved`; one
the WebAssembly registered held `boot.js`'s reload after a panic behind "Leave site?", and the
buffer brings the text back after that reload anyway. It never asks under the shell (tC4 #356), and
under the shell a close request runs the navigation gate before the window goes and answers true
only when everything is saved (§5.7). On `pagehide` the page sends every field that may save
with `keepalive` and retries each standing failure once with a new id, as tC4's `drain` does for
each of its schedulers then (`tC4:src/state.jsx:3699-3704`, `saveScheduler.ts:139-146`); each
entry names its save before the request goes, so the next load of the same run sends the same
request again if no answer came. After a WebAssembly panic the saves carry out nothing at all: the
aborted instance's closures (a timer, a late reply, `pagehide`) can still run, and their writes
would put the machine's older text over the keystrokes `boot.js` kept.)* *(A4b fix: the browser
saved nothing on leaving, and the drain's first run in Chromium found the panic case.)* *(A4b
second review: the drain skipped a standing failure, which tC4's `drain` retries, so a save that
had failed for a cause since gone was not stored when the person left.)*

### 6.6 The draft buffer

- Each field with typed text is one `localStorage` item, `"tc4.drafts.v2:" + <BufferKey as
  JSON>`, with `BufferKey { project: ProjectRef, id: ProjectId, field: FieldKey { kind, place } }`,
  and its value is one `BufferEntry { api, instance, base: [tokens], text, sent, at, draft }`
  (`tc_api::buffer`). `sent: { request, text, scope }` is the id and the whole body of the save
  in flight, if any *(A-2)*, and `instance` the server run the entry was written under. Keying by
  folder and project id means an entry for a folder that was deleted and created again never
  meets the new project's records *(A-9)*. Every access is wrapped in a `Result`; an item that
  does not parse, or whose key is not exactly the JSON serde writes, is skipped and never
  removed, and an entry is removed after a save only while its text is still the text that save
  stored, so a second tab's newer text stays. *(A4b: the first draft kept one map,
  `localStorage["tc4.drafts.v2"]`, keyed by `"<projectRef>|<projectId>|<fieldKey>"`. A folder name
  may hold `|` (§3.2), so two fields could share a key, and one map made every keystroke parse
  and rewrite every buffered field, a section buffer included, against the 16 ms typing budget.
  `sent` keeps the save's scope because the reply cache compares the whole body. An entry is
  restored, and its `sent` retried, only under the run it names: the operating system can give a
  restarted `tc4` the old port, which brings the old origin's storage back, and a resend there
  would store the text a second time over its old token. `draft: { path, body }` is how `boot.js`
  posts the entry as a `Draft` after three panics (§6.8): it adds a fresh `request` and the
  entry's `text` to `body`, so every draft endpoint's request has those two top-level keys.)*
- The entry is written synchronously on every `input`. A capture-phase `input` listener in
  `boot.js` writes the text into the entry that wasm created when the field took focus, before
  wasm handles the event, so a panic in the handler loses nothing *(A-15)*. *(A4b: when the field
  takes focus the WebAssembly hands `boot.js` the entry to write, `tc4Boot.track(element, itemKey,
  template)`, kept in a `WeakMap` rather than a DOM attribute that the DOM parity layer would see;
  `boot.js` updates the field's entry, or writes the template with the text when the field has
  none. No entry is stored for a field before its first keystroke, so a reload never restores a
  clean field as unsaved, and the first keystroke after an `Ack` cleaned the field and removed its
  entry is kept too. The template follows the field's tokens after every `Ack`. An entry of
  another server run is never written over: `boot.js` leaves it, and so does the WebAssembly,
  which then sends that field's typing to the shell's mirror only; `buffer::merge` keeps the
  newest entry per field and run, so the offer shows both texts. Without `boot.js` on
  the page the WebAssembly writes every keystroke's entry itself.)* *(A4b fix: the template is
  the field's entry as this page's machine holds it, its run, its tokens and its save in flight,
  and `boot.js` writes it with the new text over the stored entry, keeping the keys this build
  does not know. A keystroke in one tab therefore never leaves another tab's save or tokens under
  its own text; it did, and a reload of that tab sent the other tab's request again and stored
  its own text over the other tab's as a current write, unseen. For an item of another run, or
  one this build cannot read, `boot.js` writes nothing and still sends the mirror a line built
  from the template, so a panic in the input handler loses nothing there either. It says in
  `tc4Boot.wrote` whether the item took the keystroke, and the WebAssembly leaves the keystroke to
  it (`Source::Input`) only on that word; otherwise it writes the entry itself, which for such an
  item reaches the mirror only. The first build trusted `boot.js` without asking and learnt of
  another run's item only at the first save's write, so that field's typing reached neither the
  storage nor the mirror until a save, and a panic or a reload before it lost the text. When the
  storage refuses a write, the item is left as it is, whatever save it names: an older text of
  the field is better than none, and a save it names is one this run's reply cache answers, so a
  reload that sends it again gets the answer it already had (a refusal shows Retry again). If the
  save that followed landed, the older text a reload brings back is saved beside it as a
  conflict, which shows both texts rather than losing one. `buffer::write_entry` is what a
  `Write` does, in the glue and in the tests' pages alike.)* *(A4b second review: the first fix
  removed an item whose save differed from the write's, which also matched an item or a write
  that named no save, so a full storage lost the text when a save started or was refused.)*
  *(A4b second review, a known limit of a browser: one item holds one text per field, the last
  one written. Once another tab writes the field's item after this tab's keystroke (its save
  starting, or its own typing), this tab's unsaved text of that field is only in its memory and
  its requests. If this tab then crashes, with no `pagehide` drain, that text is lost, and its
  next load restores the other tab's entry and sends that tab's save again, which the reply cache
  answers. A reload runs `pagehide`, which drains this tab's text first (P12 of
  `app/no-loss.mjs`), and the shell has one window, so this needs two tabs of one browser on one
  origin, which only the dev server and Playwright open (§6.2). Entries marked with the page that
  wrote them, with a reload adopting only those of pages that are gone (the Web Locks API can
  tell), would close it if a browser ever hosts the app.)*
- In the shell, each change is also sent to the main process (`tc4Shell.bufferChanged`), which
  appends it as one line to `drafts-buffer.log` in app data at once and fsyncs the log at most
  once a second (AD21), off its UI thread, which every request of the window waits for (AD11). A field that becomes clean appends a removal line. A line is one `BufferLine`,
  `{"type":"changed","value":{"key":…,"entry":…}}` or `{"type":"removed","value":{"key":…,"instance":…}}`
  (a removal names its run, §5.7), as
  `crates/tc-api/tests/golden/types/buffer.json` shows; JSON escapes line feeds, so a line holds
  none *(A4b)*. A crash of the page,
  of `tc4` or of Electronite leaves the appended lines in the operating system's cache; a power
  cut can lose the last second, and a torn last line is skipped when the log is read. *(A-3)*
- While a field is dirty, a `Draft` command goes to the server at most once a second, and at once
  after an `Ack` that leaves it dirty (§6.5, R26); the store writes `.tc/drafts.json`
  (STORAGE:1752-1759). R22 asks that this write never delays a command
  *(A-20)*.
- A section draft holds the whole buffer and every displayed token (STORAGE:1486-1488, 1754-1756).
  Until R6, tc-app sends `text = {"tokens":[…],"buffer":"…"}` under the first verse's token.
- A user comment has no server draft, because `DraftField` has no comment variant
  (`storage:crates/tc-journal/src/command.rs:207-215`). Until R15 its text survives a reload
  through the browser buffer and a crash through the shell's mirror, but not a copy of the folder
  to another computer.
- **After a reload in the same launch:** `app/hello` returns the open sessions; the UI restores the
  view from a `sessionStorage` navigation snapshot *(A5, D16: `sessionStorage["tc4.nav.v1"]` holds
  `tc_ui::app::shell::NavSnapshot { instance, view, project }`, written at every change of the
  view, the project or the server run; `shell::restore` returns to the view and the project only
  when the snapshot's `instance` is `hello.instance` and `hello.sessions` lists its project as the
  `Screen` session, by scope and id, and to Home otherwise; screen units add their fields, a book,
  a chapter, a tool, with `#[serde(default)]`)*; buffered entries return as dirty fields. The
  open editor card opens again only if the reload did not follow a panic; after a panic the
  view-error callout takes the screen's place, and the fields stay dirty and save on the save
  machine's usual timer, which needs no editor; a failure there shows Retry, and the next open
  offers what is still unsaved (§6.8). No question is asked: the session and its tokens are the
  same. *(A-15)* *(A4b: only entries of the run `hello` names come back, and only those whose project
  is open, by folder and project id; the writers of their kinds must be registered first. Every
  other entry stays untouched for the offer at its project's next open.)*
- **At an open after a restart or a crash:** `session/open { buffered }` carries the folder's
  entries from `localStorage` and from the shell's mirror (`tc4Shell.takeBuffered`). The server
  merges them with the drafts of `drafts.json` (O12), keeps the newest per field, drops an entry
  whose text the stored value already equals (and a comment entry whose text the newest comment
  at its target has), and offers the rest: "Unsaved text from your last session was found for
  Titus 1:4. Keep it / Discard." (STORAGE:1757-1759). Keep saves with the entry's token, so a
  verse changed since becomes a conflict. *(A-3)* *(A4b: one exception, for C1b and the server's merge: an entry whose
  `sent` text the stored value equals in NFC is kept over the stored value's token. The save it
  was waiting for landed before the crash (a crash before the reply, or after it with the reply
  unread), and the newer text the person typed continues from it; kept over the entry's own
  older token, it would stand beside the person's own save as a conflict. tc-app's
  `crash_anywhere_stores_the_last_text_once` plays the offer with this rule, and fails without
  it. A change anyone else made since makes the stored value differ from `sent`, so it still
  becomes a conflict.)* *(C1b: superseded by the draft machine below: `session/drafts` is a
  command, every unsaved text is taken into `drafts.json` before the answer, and the rules
  are the machine's table.)*
- **A draft whose token no longer verifies** (a new installation secret after an app-data reset
  or a reinstall, or a folder copied to another computer) is offered as text beside the current
  value: "Unsaved text from your last session was found for Titus 1:4, but this computer cannot
  tell which version you were editing. Keep it as a separate version / Discard." Keep writes it
  as a change with no parents, which stands beside the current value as a conflict, as STORAGE
  does for outside edits it cannot place (STORAGE:3637). That needs R20; until then Keep is
  "Copy text", the offer stays until Discard, and nothing is dropped as a bug. *(A-9)* *(C1b: R20
  is on main, so Keep writes the text with no parents through `session/keepDraft` and no
  "Copy text" fallback is built; a buffered entry whose token fails its MAC is offered the same
  way (`Base::Unverified`). Over a seed's first text, which no person wrote, the kept text
  stands alone instead of as a conflict, by STORAGE's stub rules (§7.3); the seed's text stays
  in History.)*
- **The copy of a read-only project** gets the entries of the folder it was copied from, re-keyed
  to the copy, and offers them at its first open (§6.4) *(A-10)*. *(C1b: `session/copy` copies the
  folder with `Store::copy_project` (R7), which carries `drafts.json`, into "<name> - Copy" (then
  "- Copy (2)" and on) and answers the copy's folder and id, the same id as the original's. It
  is refused (`CreateRefusal::CopyUnsafe`) when the projects folder itself is on a drive that
  cannot keep a project safe or is copied by a cloud client: the copy would open read-only for
  the same reason (finding 4). By the owner's answer to Q-30 (2026-10-19) such a copy goes into the app's local projects folder with no folder to pick; C6 built that (D9); the refusal stands only where the local folder is unsafe too. The page moves every entry of the folder, the buffer's and the mirror's alike
  (`tc_ui::app::offer::copy`, finding 3), each keeping its run; the page holds its own run's
  fields on in the copy, whose tokens stand there too, and the copy's first offer takes the
  others in. A slot whose item the storage would not take stays with the original, and the
  mirror has its line under the copy.)*

#### The draft machine (C1b)

One field's unsaved text lives in four places: the page's buffer item (one per field, written
by one run), the shell's mirror (the last line per field and run), `.tc/drafts.json` and the
record. Three no-loss reviews of C1b's first build found nine ways the special cases between
them lost, hid or revived text. They are replaced by one typed machine, `tc_api::model::draft`:
a state per field, an event list, and one pure `next(state, event) -> (state, actions)` whose
`match` lists every pair. Executors carry out the actions: the browser glue and the shell
(`tc_ui::app::offer`: `buffered_lines`, `settle`, `copy`), tc-app for the open and the answers
(`tc_app::drafts`), tc-store for `drafts.json`. The page's own typing is A4b's save machine; the
machine's `Live` states say what it leaves in the places, and the property test plays it.

The state is `FieldDraft { live, offers, settle }`: `live` is the page's own field, `Clean`,
`Typed { base, current }`, `Sending { base, sent, current }` (the item names the save) or
`Landed { base, over, sent, current }` (the record holds `sent` as `over`; the page has not
read the reply); `offers` are an open's questions, one per distinct text, each with its homes
(an offered entry of `drafts.json`; for "Copy text", the buffer slots); `settle` are buffer
slots the page drops once the server holds their text.

| State | Event | Next state | Actions, in order |
|---|---|---|---|
| `Clean` | `Typed` | `Typed`, over the version shown | `Write` the item and the mirror line |
| `Typed`, `Sending`, `Landed` | `Typed` | the same, with the new text | `Write`, naming the save in flight (in `Landed` the page does not know yet) |
| `Typed` | `DraftTick` | `Typed` | `Draft`: `drafts.json`'s current entry |
| `Clean`, `Sending`, `Landed` | `DraftTick` | the same | none: no draft while a save is on its way (A4b's `pump_draft`) |
| `Typed` | `SaveSent` | `Sending` | `Write` naming the save, then `Send` |
| `Clean`, `Sending`, `Landed` | `SaveSent` | the same | none: one save in flight |
| `Sending` | `SaveLanded` | `Landed` | none: the store removes the field's current entry |
| `Clean`, `Typed`, `Landed` | `SaveLanded` | the same | none |
| `Landed`, text = `sent` | `SaveAcked` | `Clean` | `Remove`, while the item holds that text |
| `Landed`, text ≠ `sent` | `SaveAcked` | `Typed` over `over` | `Write`, `Draft` at once (R26) |
| `Clean`, `Typed`, `Sending` | `SaveAcked` | the same | none |
| `Sending` | `SaveRefused` | `Typed` | `Write` with no save |
| `Clean`, `Typed`, `Landed` | `SaveRefused` | the same | none |
| any | `Reload` | `live` kept; `offers`, `settle` forgotten | none: the page takes its run's entries back and asks again |
| any | `Relaunch` | `Clean`; the rest forgotten | none: every keystroke is in the buffer and the mirror |
| any | `Offer` | `live` kept; `offers` planned | `Absorb` (then its slots settle and its run's copies go), `Unstore` |
| any | `Keep(i)` with a *Keep* | offer *i* gone; its slots to settle | `Keep` (the save over its `over`), `Unstore` its entries |
| any | `Keep(i)`, "Copy text" | the same | none: the UI copies, the offer stays until Discard |
| any | `Discard(i)` | offer *i* gone; its slots to settle | `Unstore`, written before the answer |
| any | `Copy` | `live` kept; `offers`, `settle` cleared | `CopyFolder`, `Rekey` every slot of the folder |
| any | `Settle` | `settle` emptied | `Settle` each slot, then `Write` the page's own text into what it freed |

The open's plan, for one field's candidates (the buffer's and mirror's entries the page sends,
and `drafts.json`'s offered entries), newest first:

1. **Older copies.** A stored draft records the run whose page typed it. It goes when that run's
   own buffer entry of the field (one with a *Keep*) is at least as new: one page edits a field
   per run and its entry takes every keystroke. Both times are the page's clock: a draft carries
   its buffer entry's `at` (`Command::Draft`'s `at`, which `editor/draft` sends), so a keystroke
   while the draft's request is on its way is newer than the draft, not older than the time the
   store received it (C1b, after its fourth review). Behind the shell's mirror (`session/drafts`'s
   `mirror`), a run with no entry of the field ended with the field clean, so its stored drafts
   are older than its save and go (finding 5), but only when the record proves that save: a
   version after the draft's own, written on this computer at or after the draft's time
   (`Record::saved_here_since`). The mirror is synced at most once a second while `drafts.json`
   is synced at each write, so a power cut can take the lines of typing whose draft the file
   holds, and the aligner's links (R23) never have a line; either draft is offered (C1b, after
   its fourth review).
2. **Verdicts.** `Saved`: a head holds the text, or a version after the one it was typed over did
   (versions on another branch are not after it); the slot settles, the entry goes. `Undone`: a
   buffer entry back at its own version with no save of it landed; an offered entry is never
   judged undone again, since the save it was waiting for is no longer named. Else offered: when
   a version after its base held the text it sent (its save landed, whoever wrote it) and that is
   the record's only head, over that version, so the text continues from the save (finding 2);
   otherwise over its own version, so a change since becomes a conflict.
3. **One question per text, held by one entry.** An offer already held by an offered entry of no
   run settles its slots. Any other is taken in: `Absorb` writes an offered entry of no run before
   the answer, and only then do its slots settle and its run's stored drafts go; a stored draft of
   a run is judged by that run's buffer once, at the first open after it.

Rules the places keep:

- **The buffer item** is written only by its own run; settling removes it only while it holds the
  slot's text or an older one of that run (`BufferSlot::at`: a storage that refused the newer
  text left it). After settling, the page writes its own fields' entries again (`offer::settle`'s
  `own`), so a freed item takes its text at once (finding 8).
- **The mirror** keeps the last line per field and run; a removal that names a text drops the
  entry only while it holds that text (`BufferLine::Removed { text }`, finding 8).
- **`drafts.json`** holds *current* entries (the session's newest draft per field; a save of the
  field removes it, *Keep* never does) and *offered* entries (by number; *Keep* removes those of
  its text, *Discard* its own, written before it answers; a save never). An open turns the last
  session's entries offered, marked in the file before it returns; a draft of the opening run
  itself (a reopen or a copy in the same run, `OpenOptions::run`) stays that page's current entry
  (findings 1, 7, 9).
- **The page's own fields are never offered**: `buffered_lines` leaves out the entries its save
  machine holds, by field and run (finding 7). The glue must restore its run's entries of a
  project at every open of it, not only after a reload, so none of its run's entries waits
  unheld while its editors take input.
- **A read-only project** offers nothing and takes nothing in. A screen open never joins a
  read-only session the person left: after "That computer is closed" it waits for that session
  to close and opens afresh, so the one-time trust is used then (finding 6).

The property (`tc_ui` `tests/app/draft_machine.rs`) plays random interleavings of typing, drafts,
saves, landings, replies, refusals, reloads, crashes and relaunches (a page at a new origin, a
full storage and a power cut that takes the mirror's lines since its last sync among them), teammates' changes, offers, keeps, discards, settling, a second writer
and its copy, another computer's lock and an unsafe projects folder, over the real buffer and
mirror functions and models of `drafts.json` and the record, with an observation (a crash, a
relaunch and the open's offer) possible between any two steps. It holds that the last text of
every run's typing of a field that reached a place a crash does not take is stored, offered or
answered; that every offered text is such an unanswered text (a power cut that takes the mirror's
removal line of a discarded text brings it back to be answered again, and one that takes a
stretch's last line loses it even when an older line of the run holds the same text); that a copy is never read-only for
the folder it was made in; that "That computer is closed" ends that lock; and that an offer never
settles the text of a field the page holds. A known limit, which the property excuses: text
typed over an unanswered offer of its field and then followed by that offer's *Keep* looks, to
rule 1, saved by the *Keep* (a version written here after the draft), so a power cut that also
takes its mirror line loses it. The question is a dialog with a scrim (§6.7) and the editors
take input once the offer is settled, so no translator reaches it; if B3's glue lets an editor
take a keystroke before then, rule 1 must change. Each finding put back breaks it within the cases the
test tries. The page asks for the offer at every open, before its editors take input, so a run's
stored drafts are judged before that run's page saves the field. Without the shell's mirror, after
a power cut took its last lines, or after the storage refused a write, nothing tells a run's older
text from its last: an older one may be offered beside the last, never instead of it, which A4b
already accepts for a refused write.

### 6.7 Flows and dialogs

```rust
pub enum Pending {                       // the step to run again after the question or the sign-in
    Sync(ProjectRef), Share(ShareStep), OpenDoor43(OpenDoor43Form), Sources(SourcesStep),
    Fix(Tool), ImportVersions(ReviewId), Feedback, Link(LinkKind, String),
}
pub enum InternetFlow { Idle, Asking { kind: NetKind, then: Pending }, TurningOn { then: Pending }, Failed }
pub enum SignInFlow { Idle, Open(SignInStep) }   // A8: SignInStep { flow, form: SignInForm, then: Option<Pending>, error: Option<SignInProblem>, sending: Option<RequestId> }
pub struct Layers {                       // z-order is tC4's mount order (tC4:src/App.jsx:138-154)
    pub dialog: Option<Dialog>, pub gateway: Option<GatewayPreview>, pub upgrade: Option<UpgradePreview>,
    pub internet: InternetFlow, pub opening: Option<JobId>, pub drafts_offer: Option<OfferedDrafts>,
}
```

Only one question is open at a time; a step that asks while one is open does nothing, as tC4's
`pendingAsk` rule says (`tC4:src/state.jsx:4258-4263`). *(A5: `tc_ui::app::internet` builds
`InternetFlow` with `Pending::{Feedback, Link(LinkKind::Help, WebUrl)}`; `TurningOn` keeps its
kind and draws no dialog, because tC4 closes the question when it is answered and then turns the
switch on (`answerAsk`); A8 adds the steps that need a sign-in, B9b `LinkKind::Repo`, and each
flow unit its own `Pending`. The other layers, `gateway`, `upgrade`, `opening` and
`drafts_offer`, are added by C8, B3 and C1b.)* *(C1b: `drafts_offer` holds the reply of `session/drafts` and shows its first offer with `tc_ui::app::offer::offer_words`; the unit that draws the first project screen (B3) mounts it, since no screen opens a project before it.)* *(A8: `tc_ui::app::account` builds `SignInFlow`; Busy is the
step's `sending`, so the form stays editable while a sign-in is on its way, as tC4's does, and a
reply lands only in the opening it came from. `Pending` gains `SignIn { flow }` (M12's Sign in:
tC4 asks when the password is sent, not when the step opens), `CheckSignIn` and `Profile` (whose
address the server gives). A step that meets `NeedsSignIn` calls `Actions::needs_sign_in(then)`,
which opens M12 and runs `then` once signed in; A8 exercises it with the profile link.)* *(B9b:
`Pending::Share(Box<ShareProject>)` carries the card's folder, name, language tag and books;
`Pending::OpenDoor43` carries nothing and runs the form as it stands, which the question's scrim
keeps from changing; `LinkKind::Repo` is `account/repoLink`. The forms are `ShareForm` (tC4's
`sh`, its target and check steps; E1b adds the run's) and `OpenForm` (tC4's `od`), pure, in
`tc_ui::app::{share, open_door43}`; the repository and project-name rules are `tc_api::door43`'s,
which the server applies again.)* `View` is `Home | Read | Draft | Check |
Publish` with tC4's string values; `MainView` is a port of `tC4:src/App.jsx:192-209`.

### 6.8 Panics and view errors

- No `unwrap`, `expect` or `panic` in tc-ui (workspace lints, `Cargo.toml:15-20`). Errors render
  tC4's boundary DOM: `data-testid="view-error"`, "This screen could not be shown. {message}",
  "Back to Home" (`tC4:src/App.jsx:163-190`).
- A panic anyway: the hook runs before the abort. It stores `{view, project, message, at}` in
  `sessionStorage["tc4.crash.v1"]` and sends `app/clientError` with `keepalive`. It schedules
  nothing itself. The report log keeps the first 8 KiB of the message and marks it cut, so 50
  kept reports cannot hold hundreds of megabytes. *(A-15)* *(A4b: the marker also holds `page`, the
  id `boot.js` gave the page load, so the next load can tell a marker a previous load left from its
  own. The hook cuts the message to 8 KiB itself before the request, because a `keepalive` body is
  capped at 64 KiB. It reads its context with `try_borrow`, never `borrow`, since a panic while the
  context is borrowed would panic again before the marker exists. It keeps the first panic of the
  page only: the aborted instance can panic again in a closure it left registered, on the state
  the abort left behind ("RefCell already borrowed"), which would overwrite the cause.)*
- `boot.js` owns the recovery, because after the abort the wasm instance is dead: closures it
  registered trap again on their next event, and a reload scheduled from wasm may never run.
  `boot.js` catches the first trap (a `window` error handler that sees the crash marker),
  ignores every later trap of the same page, and reloads. It counts page loads that ended in a
  panic in `sessionStorage`, not traps. *(A-15)* *(A4b: a trap is a `window` `error` or an
  `unhandledrejection` whose error is a `WebAssembly.RuntimeError` (futures run in promise
  callbacks), or one `run_app` throws at boot; a trap with no marker of this page (an abort that
  skipped the hook) counts too. `boot.js` registers its listeners and then imports the
  WebAssembly's glue with a dynamic `import()`, so they exist even if the glue fails to load,
  which shows the static callout with no reload. When `sessionStorage` cannot keep the count it
  never reloads, since the reloads would never stop: it draws the static callout at once.)* *(A4b
  second review: that includes a browser that throws from the `sessionStorage` getter itself, as
  Chromium does when the site's data is blocked; `boot.js` reaches both storages through a getter
  that gives none instead of throwing. Before, the error handler threw there, so the page neither
  reloaded nor showed a callout and stayed on the aborted instance.)*
- After the reload the app reattaches to its session (AD22) and shows the view-error callout in
  place of the screen that panicked, with the top bar. It does not restore that screen's open
  editor, which could panic again. The buffered fields stay dirty, the indicator shows them,
  and the save machine saves them on its usual timer; a failure shows Retry, and the next open
  offers what is still unsaved. *(A-15)*
- Three page loads ending in a panic within 10 s stop the reloads. `boot.js` then draws the same
  callout as static DOM, posts every buffer entry of the current folder that has a
  `DraftField` as a `Draft` with a plain `fetch` (the shell's mirror holds the others, such as
  user comments until R15), and "Back to Home" reloads to Home. *(A-15)* *(A4b: it posts every entry
  of the server run the page last met (`sessionStorage["tc4.instance.v1"]`, which the page writes
  at boot and at a restart) that has a draft call, rather than those of the current folder: a
  panic at boot comes before any screen has named a folder, and each draft call names its own
  open, so the server refuses one whose project is not open and nothing changes. Entries of
  another run wait for the offer. Every text of the static callout goes in as a text node, never
  as HTML, because a panic message can hold a project name or typed text; its words come from
  `sessionStorage["tc4.viewErrorText.v1"]`, which the page keeps in the shown language, and are
  English when there are none. "Back to Home" removes the crash marker, the panic count and A5's
  navigation snapshot `tc4.nav.v1` before it reloads. The no-loss smoke compares the static
  callout's DOM with the in-app one's.)* *(A4b second review: an entry with a save in flight
  (`sent`) gets no draft, as the machine sends none while a save is in flight. Its tokens are
  those from before that save, so a draft over them would stand beside the person's own save as
  a conflict at the next open, where §6.6's exception for an entry whose `sent` text was stored
  cannot apply to a draft. The entry stays in the buffer and the mirror, with its save, and the
  next page or the offer saves it.)*
- Tests: a forced panic in an input handler keeps the keystroke (the `boot.js` listener); one
  panic followed by trapping closures reloads once and shows the callout; three panics show the
  static callout and the server holds the drafts.

### 6.9 Language and right-to-left

- `t(key, vars)` and `t_dyn(key, vars, fallback)` port `tC4:src/i18n/index.js:76-95`: a target
  entry that is missing, empty or not a string falls back to English per key; an unknown key
  warns once (`[i18n] missing key: <key>`) and shows the key; an explicit fallback wins and is
  returned as given; each `{name}` is replaced, the variables one after another in the order
  given, as tC4's loop does. *(A4a: one declared difference. tC4's `replaceAll` also reads
  `$&` and `$$` in a value as patterns, so a project named `Cost $$` would show `Cost $`; `t()`
  puts a value in as it is.)* `show_locale` ports `setLocale`: a language that is not
  installed, or the one already shown, changes nothing.
- `build.rs` generates key constants from `assets/i18n/en.json` and
  `assets/i18n-rebuild/en.json`, so a misspelled static key does not compile. The constants are
  nested modules, `keys::app::SAVED`, `keys::net::ask::SIGN_IN`,
  `keys::refusal::open::LOCK_HELD`: flat names collide in tC4's own catalog
  (`upgrade.text.gateway` and `upgrade.textGateway` would both be `UPGRADE_TEXT_GATEWAY`), and the
  build fails on a key that gives another key's path. *(A4a)* The port of
  `test/i18n-keys.test.ts`, with floors, covers dynamic keys such as `net.ask.<kind>` and
  `syncResult.error.*` (DELTA:525-531).
- English is built in: tC4's `en.json` and the rebuild's ten key files are compiled into the
  wasm, because `t()` needs English as the fallback of every key even when a fetch fails, and
  `build.rs` reads both English files anyway; the rebuild files hold a few hundred bytes each.
  The nine other tC4 catalogs (870,974 bytes of the 943,433) are fetched in parallel before
  the first render, where tC4 bundles all ten (`tC4:src/i18n/index.js:12-21`), so the App
  language preview (M16) changes language in one render without remounting, and a fetch
  failure shows the boot error instead of English in the middle of a preview. *(A-23; A4a:
  only the nine are fetched.)*
- Changing the language sends no request and remounts nothing: every text is a closure that
  reads the shown language, so a switch updates text nodes in place. The smoke counts the
  app's own requests (`fetch`, XHR, document, script, style sheet) and fails on any after boot;
  it lists font loads apart, because the browser itself loads the Arabic faces of tC4's fonts the
  first time Arabic is shown, as it does in tC4 (its app-language case j paints with the bundled
  Noto Sans Arabic). *(A4a)*
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
are virtualized: the Psalms tN rail has 8,212 rows (PERF:3-5, 97). tC4 never virtualizes a list,
so the rebuild must keep its keyboard contract by itself *(A-24)*:

- Two fixed heights, one for rows (one line, `tC4:src/views/Check.jsx:1012`) and one for group
  headers; a row's offset is computed from the counts of each above it.
- The roving tab-stop row and the row that focus returns to stay mounted even when scrolled out.
- Moving to the active row scrolls to its computed offset before focusing it.
- Rows use `keyed_list` (`crates/tc-ui/src/ds/mod.rs:232`), which keeps focus across moves.
- The `check-keyboard` scenario also runs on the Psalms rail. The threshold is a constant with a
  test (Q-12).

### 6.12 Screen port map

| tC4 file | tc-ui module | Unit |
|---|---|---|
| `src/App.jsx` | `app/shell.rs` (TopBar, SaveIndicator, MainView, ViewBoundary) | A5 |
| `views/AccountMenu.jsx` (menu, help, language, about, internet) | `views/account_menu.rs`, `help_panel.rs` | A5, A8 |
| `views/Home.jsx`, `SyncResult.jsx` | `views/home.rs`, `sync_result.rs` | B2, B10 |
| `views/OpenProgress.jsx` | `views/open_progress.rs` | B3 |
| `views/Understand.jsx`, `SourceVerse.jsx`, `HelpsPanel.jsx`, `BookRail.jsx`, `railChrome.jsx`, `PanelIcons.jsx` | same names in snake case | B3 |
| `views/Draft.jsx`, `draftChrome.jsx`, `conflictMarkers.jsx` | same names | B4a |
| `views/SectionEditor.jsx`, `sectionDraft.js` (in `tc_api::model`) | same names | B4b |
| `views/Check.jsx`, `ItemNav.jsx`, `CompareCard.jsx` | `views/check/{picker,rail,detail}.rs` (`rail.rs` holds the session; `detail.rs` the compare card, the target words, the marks, `ItemNavButton` and the Academy drawer), `app/check_item.rs` | B5a, B5b |
| `views/Align.jsx` | `views/align.rs` | B6 |
| `views/StoryUnderstand.jsx`, `StoryDraft.jsx`, `StoryRail.jsx` | `views/story_*.rs` | B7 |
| `views/CommunityChecking.jsx`, `ExportMenu.jsx`, `print/*` | `views/community_checking.rs`, `export_menu.rs`, `print/*`; the print model in `tc-export` | A10, B8, D2 |
| `views/ConflictReview.jsx` | `views/review/*.rs` (tabs for every kind) | B11, C7a, C7b |
| `views/modals/*.jsx` (18 files) | `views/modals/*.rs` | B2, B9a, B9b, C6, C8, F1 |
| `views/dev/Inspector.jsx` | `views/dev/inspector.rs` (dev only) | late |

Rebuild-only screens with no tC4 view: the review tabs beyond Text, History, Detached, Notices,
the received-work sheet, the draft offer, the read-only banner with the copy offer, the USB flows,
the first-launch window (Q-9), and the boot error *(A4a)*. The window is a port of tC4's
preparation page (`scripts/desktop-preparing.html`, `desktop-prepare.cjs`), with its markup, CSS,
title and English words unchanged and its words as rebuild keys (`prepare.*`) in the saved app
language (nine languages pending, Q-10). Only its failure state is new design: tC4 shows a native
error box and exits (`scripts/desktop-main.cjs:175-181`), while here the server seeds and the
window shows tC4's failure words with the reason under them, so the owner reviews it under Q-9
with `docs/evidence/first-launch-seeding-2026-10-18/prepare-failed.png` *(A9)*. tC4 bundles
everything and cannot fail to boot; the rebuild can: a catalog that does not load, a page opened
without its key, or a page and a server of two builds. The boot error is tC4's view-error callout
(`data-testid="boot-error"`, tone warn, `role="alert"`): `app.viewError`, the technical reason,
and a Retry button (`app.retry`) that reloads. It is shown in the saved language when its catalog
is installed, else in English.

---

## 7. Parity testing

### 7.1 One harness, two environments

`parity/tc4-baseline/capture.mjs` runs `await main()` when imported (`capture.mjs:1110`), so it
is split as `parity/ds-gallery` was:

| Module | Content |
|---|---|
| `parity/app/screens.mjs` | `SCREEN_LIST` and each screen's `run`, unchanged except that the share button and card are reached through the driver and every Door43 step is awaited |
| `parity/app/driver.mjs` | `appDriver` over an environment (`AppEnv`: `load`, `catalog`, `projectId` and the four Door43 steps) |
| `parity/app/observe.mjs` | the capture rules, `trackRequests`, `settle`, `captureScreen`, `observe`, `domSnapshot` rooted at `#root` |
| `parity/app/env-tc4.mjs` | today's mock, `.state` and in-browser fake Door43; tC4's catalogs and journal writers |
| `parity/app/env-rust.mjs` | launches `tc4 serve` with `--test-hooks`, `--scenario <state>` (the scenario's state built by `state.mjs` into the run folder, its projects written into the run's projects folder and opened through the store, C1a, or held by the memory backend with `--backend memory`, B1b; a `tc4` built with `--features test-hooks,fixture-backend`), an empty app-data and projects folder under `target/` and a dead `--door43`; `extraHTTPHeaders` with the token, and a context route that aborts every request to another origin before it is sent, so the token never leaves the machine *(A6b, D10)* |
| `parity/tc4-baseline/capture.mjs` | a short entry over `env-tc4` |
| `parity/app/capture-rust.mjs` | the Rust entry over `env-rust`: the same screens, driver and rules, into `parity/app/screens`, `dom` and `captures.json`, each capture naming the unit that made it and the layers it claims *(A6b)* |
| `parity/app/behaviour.mjs` | the recorder of layer 4: a sequence's steps from a baseline screen, on either environment, with node numbers *(A6b)* |

**Proof the split changed nothing.** The baseline test checked sizes and byte counts only
(`parity/tc4-baseline/baseline.test.mjs:45-59`). Unit A6a adds a sha256 per PNG to `manifest.json`
and a check for it, then recaptures tC4 and requires identical hashes. A control capture with the
old `capture.mjs` came first and reproduced every committed PNG, so the machine could tell.

The Rust environment's seams:

| Seam | Rust environment |
|---|---|
| `home()` | `test/reset` (prefs back to the seed, holds released, the internet off); `prefs/setLocale` when the screen's language is not English, because `test/reset` carries no locale and there is no scenario seed before B1b; `localStorage` and `sessionStorage` cleared; a load of `/#t=<token>` through `about:blank`, because a `goto` to the address the page already shows only moves to its fragment; then wait for both cards and `<html lang>` *(A6b, D9)* *(A5, D6: the environment says when Home is ready, `env.homeReady(page, card)`: tC4's two cards; on the Rust app the cards once `units.json` lists B2, and the top bar's account button before that, since the stand-in Home draws no project; the driver still waits for `<html lang>`)* *(B2 D13: on the Rust app, `+ Add a project` and every card `home/list` names, so a project the seed check refuses is not waited for)* |
| `card(repo)`, `shareButton(repo)`, `shareCard(repo)` | `projectId` maps `BIBLE` and `OBS` paths to the folder, for the `project-`, `share-` and `share-card-` test ids; screen runs name a project's test ids only through these three |
| `trackRequests` | counts `/api/v1/` and `/media/`, never `/api/v1/events`; otherwise `settle` never sees 800 ms of quiet and silently waits out 30 s (`capture.mjs:136-155`). Shared since A6a: requests are tracked per document through the DevTools protocol, because Playwright's request events can leave a request of a replaced page "in flight" for good, which made later tC4 screens wait out the deadline |
| `settle` | also waits for `test/quiet` to answer `{busy:false}` and for the page's `window.__tcIdle`, which also covers the catalogs under `/ui/` that the tracker does not count; once either says busy it is asked at every look, and the quiet counts again from the first idle answer *(A6b, D9)*; a screen that hits the deadline fails (in both environments since A6a; with requests tracked per document, no tC4 screen reaches it), and its error says when the server still answered busy |
| `routeFakeDoor43` | removed: `tc4 serve --door43 <url>` points at a tc-door43-fake process seeded with `facilitator-zq` and the organisations of `fake-door43.mjs:16-21`; the guard that refuses other origins stays *(A8: `startDoor43Fake` in `env-rust.mjs` starts the `tc-door43-fake` binary, which prints its address and a control port on loopback (requests, faults, holds, tokens); `capture-rust.mjs` starts one per scenario with `--door43-fake`, the journeys with `withApp({ door43 })`, and without it tc4 is pointed at a port nothing listens on)* |
| `rust` on a screen *(A8)* | a screen whose dialog the Rust app reaches another way until its unit builds the trigger has a `rust` route in `screens.mjs`, which `capture-rust.mjs` takes only for a capture that claims no pixels (`screenRoute`): `share-signin` opened from the account menu's "Sign in to Door43", the same M12 that Share opens, until B9b's card reached it the tC4 way, captured its pixels and deleted the route *(B9b: deleted; no screen has a `rust` route now, and the gallery case `signin-error` lost its own the same way)* |
| `pushToDoor43`, `teammatesChangeOneVerse`, `loseFromDoor43`, `dropNextClone` | phase 1: scenario files for the scripted doubles plus teammate heads in the fold; from E1a: real packs written by tc-journal and tc-store into the fake's bare repositories, and R13's dropped connection |

### 7.2 The same state on both sides

- `parity/app/state.mjs` runs `build-state.mjs` against a tC4 root assembled from the repository's
  own copies: `test/fixtures` from `fixtures/tc4/test-fixtures`, `conformance/sample-burrito*`
  from `fixtures/tc4/sample-burrito*`. `build-state.mjs` reads only those and the vrs files
  (`parity/tc4-baseline/fixtures/build-state.mjs:41, 91, 214-215, 405`). The test
  `state_from_repo_equals_state_from_tc4` compares the two trees byte for byte. CI then needs no
  tC4 checkout: there the test is skipped, and a build from the repository is compared with
  `parity/app/state.sha256.json`, the sha256 of every file as built from tC4 (A6a). The state's
  `provenance.json` names fixtures by their tC4 path and no longer records the tC4 root, so the
  two builds can be byte-identical.
- `MemoryBackend` reads the same `.state/<scenario>`, and from C1a on the store's scenario writer
  writes its projects from it (the seed's own packs, §4.2). Both apps start from the same bytes,
  substitutes and absences included.
- Bundle seeding is off during captures (Q-11).

### 7.3 Four layers

1. **Pixels.** The baseline's capture rules: Playwright 1.56.1, Chromium 141 build 1194, the five
   flags, the same context and viewports, the pointer at (1000, 28), a viewport screenshot with
   animations disabled (`capture.mjs:37, 50, 987, 1016-1019`). The product ships Electronite v37,
   whose Chromium is older (Electron 37 uses Chromium 138), so P7's comparison engine is not
   the shipped one. That is a declared residual in `residuals.json`, and G2 adds one capture run
   of the Rust app inside Electronite on the release pipeline, compared with the Chromium 141
   run of the same build (Q-28). *(A-34)* A pixel counts when any of its channel bytes
   differ, which on these opaque screenshots is exactly what `pixelmatch` counts with threshold
   0 and anti-aliasing counted (`parity/ds-gallery/diff.mjs:47`); the gate counts in Rust, and
   on all 62 pairs of the cloud machine's and the Linux PC's baselines it gave pixelmatch's
   counts *(A6b, D2)*. Declared masks, each a rectangle in
   `parity/app/masks.json` with the ruling it cites: the build line in `about`, the build and
   platform lines in `feedback`, the Door43 server line of the sign-in step (`signin-server`, which
   names git.door43.org in tC4's capture and the loopback fake in the rebuild's, A8), and the Q2
   and Q3 text on the nine `rebuildDiffers` screens
   (`parity/tc4-baseline/README.md`, "Where the rebuild differs on purpose"; the ninth,
   `open-door43-internet-consent`, shows the Q3 note behind its scrim and was added by A6b),
   and, from B9a, the Urdu (Devanagari) row of `source-texts` under tC4 #701. A ruling is an
   owner's answer (`Q3`) or a tC4 issue (`#701`) that PLAN lists in parentheses; the baseline's
   `rebuildDiffers` marks the screens of the owner's answers only, since the issues came after
   its capture *(FIX-misc)*.
   Fallback glyphs come from the machine's fonts: on the owner's Linux PC the same tC4 commit,
   Chromium build and capture gave other pixels than on the cloud machine on 57 of the 62
   screens and the same DOM on all 62. So both captures record their `machine` (platform,
   Chromium version, and the count and sha256 of `fc-list`'s sorted lines), the baseline is
   recaptured on the machine that captures the Rust app (A6b recaptured it on the Linux PC), and
   pixels are compared only between captures whose fingerprints are equal. A capture with no
   font list (Windows, macOS) is compared by DOM and meaning only. A fontconfig rule that
   changes a fallback without changing the list is not caught; `residuals.json` lists it.
   *(A6b, D3)*
2. **DOM.** `domSnapshot` (`parity/ds-gallery/protocol.mjs:174-220`) rooted at `#root`. tC4's
   snapshot per screen is recorded once and committed as `parity/tc4-baseline/dom/<screen>.json`,
   with its sha256 in `manifest.json`. Beside `#root` it records the attributes of `<html>`
   (`lang`, `dir`) and any element of `<body>` outside `#root`: none in tC4, whose layers render
   in place because it never sets `window.ReactDOM`
   (`tC4:src/ds/components/primitives/Layer.jsx:62-67`). A style is named by the first 12 hex
   digits of its sha256 and spelled out once per file, which cuts the 62 files from 12.8 MB to
   5.2 MB; two lines are equal exactly when their elements are. The snapshot records values as
   rendered; the comparison normalizes them by these rules, each listed and tested, and
   rewrites nothing else, so a test id, a text, a style or an attribute that differs is a
   difference: the Rust side's project folders become tC4's repo paths where they end an
   attribute value, through the scenario map (tC4's ids are the reference, so this rule applies
   to the Rust side only); same-origin asset paths (`src`, `href` and `poster` values that start
   with `/`, and `url(/…)` in a style) become their file name without a Vite hash, while other
   URLs compare as they are, so a wrong external link still shows *(A6b, D5)*; each mask's
   element and its subtree are taken out and compared as the mask's part; generated ids
   `tc-(f|g|dlg|cfm|rg)\d+` (the counters of tC4's `Field`, `FieldGroup`, `Modal`,
   `ConfirmDialog` and `Radio`, which tc-ui mints too) are renumbered per prefix by first
   appearance, in the rest and in each mask's part on its own, so a masked difference cannot
   shift the ids after it *(A6b, D4)*; comment nodes are skipped (already). A DOM-first capture
   (§7.6: a screen whose DOM unit comes before its pixels unit) compares only the subtree that
   `parity/app/scopes.json` names, the dialog or the account menu's panel, because the Home
   behind it is not the port's until its pixels unit; a capture that claims pixels compares
   the whole page *(A6b, D8)*.
3. **Meaning.** `observe`'s `homeCards` and `syncToast` (`capture.mjs:914-960`) field by field.
   Declared difference: under Q2 the OBS card's action is `sync`, not `upload`.
   `syncToast.missingVersions` is left out: tC4's capture lists the versions by tC4's own rule,
   which the Rust capture does not have; their count, `missingPins`, is compared *(A6b)*.
4. **Behaviour.** `scenarios.mjs`-style sequences on full screens: DOM, focus, scroll lock and node
   identity after each step (`parity/ds-gallery/scenarios.mjs:137-151`). Node numbers are
   renumbered by first appearance on each side, so they compare whatever else the page holds;
   the masks' elements take no part, and a DOM-first screen's sequence is compared within its
   scope *(A6b)*. *(A5, D7: a step may close that scope, as "Escape returns the focus" does: a
   scope that matches nothing is the marker `(scope absent)`, equal when it is gone on both
   sides and a difference on one side only, and every scoped step also compares `<html lang
   dir>`, which a language preview sets outside the scope; screen rows still fail on a scope
   they cannot find; three planted tests in `screen_record/planted.rs` prove the rule.)*

### 7.4 The tC4 view-state gallery

Many states have no baseline. `parity/view-gallery` is a Vite app that mounts tC4's real
`App.jsx` from a tC4 clone, built by the clone's own Vite, React plugin and node polyfills in
release mode with tC4's base and defines, as `npm run build` builds tC4, with `src/state.jsx`
replaced by a stub through a resolver plugin that fails the build if tC4's file is ever loaded
(the way `parity/ds-gallery/vite.config.mjs:15-19` aliases tC4's design system). The 32 view
files under `src/views` and `src/App.jsx` import from `state.jsx`; the stub exports what they use:
`useApp`, `SCRIPT_FONTS`, `SUITE_VERSION`, `door43` and `isOldTestament`, the second and the last
cut from `state.jsx`'s own source. `useApp` answers tC4's value `{ s, book, progress, sourceModel,
actions, BOOK_NAMES, locale }`, with `book` and `sourceModel` derived as tC4's provider derives
them. `actions` records each call, so tC4's action trace can be compared with the Rust screen's
requests and navigation; it answers the two selectors the views call while drawing
(`sourceGateways`, `installedCountFor`) with tC4's own code, and otherwise returns nothing unless
the case gives a reply. *(A7: the text said 34 view files; 32 import `state.jsx`.)*

A case has four parts: a recorded tC4 state, a patch, steps, and the matching tc-api replies. A
recorded state is the whole `s` of real tC4, read from its React tree after the driver took it,
against the baseline's mock, to a baseline screen or a few steps further, with the replies of the
read-only actions the view draws. The patch is tC4's reducer `set` patch, written only where the
mock cannot go, and built from recorded values wherever it can be. Every recorded state renders
in the gallery as tC4 rendered it: calibration draws each with no patch and compares it with real
tC4's own capture at that moment, pixels byte for byte and DOM after the gate's id renumbering
(`parity/view-gallery/calibration.json`). The replies are recorded from `MemoryBackend` where the
state exists there and written by hand only where it cannot exist yet, by the unit that ports the
case's group, in `parity/view-gallery/replies/<case>.json`: `MemoryBackend` is B1b's and a screen's
view types are its unit's (§10), so no reply exists when the gallery is built. *(A7: the text
said "a tC4 `s` fragment" and replies recorded from `MemoryBackend`; a whole recorded state needs
no hand-written fixture where the mock reaches the state, which is what risk 17 asks.)*

The groups, which the gate reads (keep the backticks and the four cells):

| Group | tC4 view and states | DOM | Pixels |
|---|---|---|---|
| `S4` | the verse conflict review that tC4's Text tab is (`ConflictReview.jsx`): unselected, a version selected, a version of mine | B11 | B11 |
| `S12` | OBS Community Checking (`CommunityChecking.jsx`, `StoryCommunityChecking`), with its layout choice | B8 | B8 |
| `S14` | the open overlay (`OpenProgress.jsx`) with its three captions | B3 | B3 |
| `M9` | the release upgrade confirmation (`UpgradeSet.jsx`): a set, harmless, blocked, failed, scripture text | C8 | C8 |
| `M9b` | the same confirmation titled "Re-pin" (`UpgradeSet.jsx`, `offer.kind` repin), over the Check picker that the guided fix's screen leaves *(C8: split from `M9`, whose rows are drawn over Get source texts; this one is drawn over B5a's Check picker, which C8 cannot show, so B5a was to capture it with the guided fix. Merge of C8, 2026-10-19: B5a landed with the three ways drawn and not wired, and C8 landed with their endpoints and no modal, so no screen reaches this confirmation yet. Wiring the three ways (download with its progress, a file install, the re-pin that opens this confirmation) and capturing this case moved to B5b, the next unit on the Check screens. B5b: wired and captured, tC4's pixels exactly; B5b's D9)* | B5b | B5b |
| `M10` | the gateway change (`GatewayChange.jsx`): the change, busy, blocked, failed to apply, failed preview | C8 | C8 |
| `M18` | "Remove all the text of section …?" (`SectionEditor.jsx:244-255`, tC4 #600) | B4b | B4b |
| `license` | the License dialog (`AccountMenu.jsx`, `LicenseDialog`) | A5 | B2 |
| `net-failed` | "tC4 could not use the internet" (`AccountMenu.jsx`, `InternetDialog`) | A8 | B2 |
| `share` | share progress and done (`ShareDialog.jsx`) | E1b | E1b |
| `import` | the import steps after `kind` (`Import.jsx`): files, the review of USFM and of tC3, working, failed | F1 | F1 |
| `add-book` | Add a book, the pick and USFM steps (`AddBook.jsx`) | C6 | C6 |
| `feedback` | Feedback sending, sent and not sent (`Feedback.jsx`) | G1 | G1 |
| `sign-in` | the sign-in error (`ShareSignIn.jsx`) | A8 | B9b |
| `check-empty` | the check empty states (`Check.jsx`: `CheckEmpty`, deriving, an error, a filter with no match, a picker card with no items) | B5a | B5a |
| `align-unavailable` | the align unavailable states (`Align.jsx`, `AlignBlocked`), loading and an error | B6 | B6 |
| `home` | Home empty and with an error of its listing (`Home.jsx`) | B2 | B2 |
| `home-held` | Home while its listing is read (`Home.jsx`): a photograph of a read in flight, which the capture of work in flight that B3 builds for S14 takes *(B2 D10)* | B3 | B3 |
| `home-open` | Home after an open was refused (`Home.jsx`, `bookError`): it needs the open *(B2 D10)* | B3 | B3 |
| `home-checkpoint` | Home after the checkpoint of the project just left failed (`Home.jsx`, `commitError`): it needs the leave checkpoint *(B2 D10)* | D1 | D1 |
| `resume` | the Resume card (`Home.jsx`, `ResumeCard`) | B2 | B2 |
| `import-toast` | the import toast (`Home.jsx`) | F1 | F1 |

A group whose DOM unit comes first is DOM-first, as in §7.6: its cases name their dialog in
`scope`, and the DOM layer compares only that subtree until the pixels unit captures the case
whole. `parity/app/cases.json` lists every case with its group, scenario, units, scope, the keys
its patch sets and the sha256 of its tC4 screenshot, DOM and action trace, and the machine that
captured them; `parity/view-gallery/README.md` says how a case is written, recorded and captured.
*(A7)*

### 7.5 Report and gate

`parity/app/report.json` holds, per screen and gallery case: pixel count, diff image, DOM diff,
meaning diff, behaviour, masks used, the backend phase (`memory`, `memory+scripted`, `store`),
`pending_until` (the unit that must recapture a scripted screen: its Recapture unit in §7.6),
and a status: `same`; `declared`, every difference inside a mask; `differs`, explained in
`residuals.json`; `unported`, no layer captured yet, with `missing` naming the unit of §7.6 that
captures each layer; or `failed`: a capture error, a page error, a view or boot error, a refused
request, the settle deadline, a screenshot of another size than tC4's, pixels from another
machine, or a mask or scope the comparison could not find *(A6b, D6, D7, D12)*. The report is
what `crates/tc-ui/tests/screen_record.rs` computes from the committed captures: every
`cargo test` recomputes the four layers in Rust from `parity/tc4-baseline` and `parity/app` and
fails when the committed file differs, so the report cannot be stale or edited by hand, a
planted difference fails `cargo test` itself, and CI's rust job runs the whole comparison with
no browser; Node only captures, and
`UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` rewrites the report and its diff
images *(A6b, D1)*. `parity/app/units.json` lists the units that have landed, because CI checks
out one commit and cannot read that from history.

A gallery case's row (§7.4) names its group's DOM and pixels units in `missing`; a case has no
meaning layer. A DOM-first case compares its `scope` until its pixels unit captures it whole, and
its pixels compare with the machine that captured the gallery (`cases.json` `machine`), not the
baseline's. The gate checks each case's tC4 PNG, DOM and action trace by sha256, refuses a file
under `parity/view-gallery/{screens,dom,actions}` that no case names, and fails a case whose
group §7.4 does not list, whose units are not its group's, whose scope does not match its
group (a DOM-first group needs one, any other has none), or that is named like a baseline screen
or another case. *(A7)*

`screen_record.rs` fails when:

- a baseline screen or gallery case has no row;
- a row is `differs` without an entry in `parity/app/residuals.json`, its entry does not name
  exactly the layers it differs in (pixels, DOM, meaning, behaviour), or an entry explains a row
  that does not differ *(A6b, D12)*;
- a row is `failed`, whatever `residuals.json` says;
- a row's PNG or DOM hash disagrees with the committed capture, or a file belongs to no capture;
- a layer is still missing after the unit that captures it has landed, or a capture names a unit
  that has not landed;
- a row is still `memory+scripted` after the unit named in `pending_until` has landed;
- a row is captured on the memory backend after C1a has landed (`MemoryAfterStore`): from then on
  the parity environment runs on the store *(C1a, D9)*;
- a mask cites a ruling docs/PLAN.md does not list (other than `build`), a screen where the
  rebuild differs on purpose has no mask citing its ruling, or the rebuild's masked element lies
  outside the mask's rectangle;
- a mask's element is missing on a side, unless the mask says why the rebuild has none yet
  (`rustAbsent`), and then once the rebuild draws it *(B10: the OBS card that the seed check
  refuses carries `q2-obs-card`)*.

This follows `crates/tc-ui/tests/gallery_record.rs` and meets P7: "a human reviews the
differences, and the differences are listed" (PLAN:112-115).

### 7.6 The 62 baseline screens

"M" is `MemoryBackend` with real fixture data. "M+S" is a scripted job outcome. "Recapture" is
the unit that captures the screen again on its final backend; for an M+S screen it is also its
`pending_until` (the notes repeat it for two of them). The pixels unit also captures what the DOM
shows. A row that names one unit for the DOM and another for the pixels is DOM-first: until the
pixels unit captures the screen whole, its DOM is compared within the scope of
`parity/app/scopes.json` (§7.3 layer 2) *(A6b, D7, D8)*. From C1a on an "M" screen is captured
on the store, so a screen first captured after C1a has landed needs no recapture by C1a; one
captured before it is taken again by whichever of its unit and C1a lands second *(C1a, D7, D9)*.

| Screen | Scenario | First capture | Recapture | Notes |
|---|---|---|---|---|
| `home` | sample | B2, M | C1a | project test-id map |
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
| `understand-tit1` | sample | B3, M | C1a | — |
| `en-understand-tit1` | en-pins | B3, M | C1a | — |
| `en-understand-tw-article` | en-pins | B3, M | — | article pane |
| `en-understand-questions` | en-pins | B3, M | — | incomplete: no en_tq in the state |
| `translate-tit1` | sample | B4a, M | C2 | — |
| `en-translate-tit1` | en-pins | B4a, M | C2 | — |
| `en-translate-section-editor` | en-pins | B4b, M | C2 | editing lock |
| `en-translate-verse-editor` | en-pins | B4a, M | C2 | the textarea holds STORAGE `words` (Q-20) |
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
| `share-internet-consent` | sample | B9b, M | — | kind `share` |
| `share-signin` | sample | A8 DOM; B9b pixels | — | name notice; fake Door43 |
| `share-dialog` | sample | B9b, M + fake | E1b | "Recommended" needs R12 |
| `share-dialog-check` | sample | B9b, M + fake | E1b | — |
| `open-door43` | sample | B9b, M | — | Q3 text masked |
| `open-door43-internet-consent` | sample | B9b, M | — | kind `open`; Q3 text masked behind the scrim |
| `shared-home` | shared | B10, M | E1a | Q2 mask |
| `sync-internet-consent` | shared | B10, M | — | Q2 mask behind the scrim |
| `open-door43-refused` | sample | B10, M+S | E1b | Q3 text (Q-21); `pending_until: E1b` |
| `sync-failed` | shared | B10, M+S | E1a | needs R13; `pending_until: E1a` |
| `sync-ok` | shared | B10, M+S | E1a | missing-pins count by tC4's rule (`capture.mjs:110-128`) |
| `sync-ok-teammates` | shared | B10, M+S | E1a | real fold conflict behind the chip |
| `sync-ok-restored` | shared | B10, M+S | E1a | — |

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
| J2 draft verse (`e2e/j02-draft-verse.spec.ts:96-1143`) | B4a, B4b (UI on scripted saves), C2 | store | one pending batch per save; "Saved" only after B3 (held save); the section steps write nothing before Save *(A-1)* |
| J3 get resources, guided fix | B9a, C8 | store + fake with resources | receipts; pins with sha |
| J4 check book | B5b, C4 | store | decision records; drag-select |
| J5 align verse | B6, C5 | store | one `Alignment` change per save |
| J6 edit invalidation | C2, C4 | store | nothing written on decisions (STORAGE §0 item 4) |
| J7, J23 publish | B8, D2 | store + tc-export | Burrito export carries `DeviceProfile` (DELTA:806-812) |
| J8 resume and checkpoints | C1a, D1 | store | one commit when leaving with changes, none without (STORAGE:3820-3821); the session closes at Home *(A-25)* |
| J9a, J9c, J9d import | F1 | store + I | P9 byte equality |
| J11, J24 share | B9b, E1b | store + X | Upload cases become sync cases on the OBS card (Q2) |
| J12 upgrade, J13 gateway change | C8 | store | one `ApplyUpgrade` batch; look-again when heads moved |
| J15 slow open | B3 (UI with a held open), C1a | store with `FaultFs` delay | first report 300–400 ms (STORAGE:1883-1891) |
| J16, J25 Understand | B3, B7, C3 | store | `Comment` records |
| J17 open from Door43 | B9b, E1b | store + X | private repository through sign-in (Q3) |
| J18 keep a version | B11 (read), C7a | store, second device's pending batch | `Resolve` with review tokens |
| J20 OBS create | C6 | store, R2 | — |
| J21 OBS draft | B7, C3 | store | Q21 editing lock |
| J22 OBS check | B7, C4 | store | — |
| J26 team sync | E1a, C7b | store + X, three app-data folders (DELTA:945-969) | plus resolving a check conflict (PLAN P2), once C7b has landed |
| USB both directions (rebuild) | E2 | store + X | STORAGE §10 |
| sync-button (`e2e/sync-button.spec.ts:107-270`) | B10, E1a | store + X | no request at launch or after 5 s idle |
| app-language a–k, help a–i, feedback, internet-consent a–j | A5, A8, B2 | memory, then store | — *(A5: A5 runs help a (Home half) and b to i, app-language a, b, b2, c, d, f and the no-project halves of g, h, j, and feedback c to i from Report a problem, in `parity/app/journeys/`; B2 the Home halves of app-language a, g, h, j; B3 and B4a app-language e, i, k and the in-project halves of help a and app-language h, j (B4a: e's Translate half and i's left-to-right project's half, in `translate.test.mjs`; C3 e's comment half, i's right-to-left project for C6's create, and k and the in-project halves of help a and app-language h and j are still open); B3 feedback a and b, which need a screen that opens a project and is refused (B1b, the other candidate, landed first with no screen that opens one); A8 internet-consent a–j)* |
| check-keyboard, item-navigation, ds-labels, check-project-switch, check-returns-draft | B4a, B5a, B5b, B6, C4 | memory, then store | geometry within 1 px |
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
- **G5 (journeys) and G2 (the rest)**: an `app-e2e` job builds the wasm with wasm-bindgen 0.2.129, builds `tc4` with
  `test-hooks`, installs Chromium build 1194 in a pinned image with the fonts of the machine
  that captured the baseline, and runs the journeys, the DOM and meaning layers, and the perf
  specs on Psalms. Pixels gate only where the machine fingerprint equals the baseline's
  (§7.3 layer 1), because fallback glyphs come from the machine
  (`parity/ds-gallery/README.md:294-308`): until the pinned image exists, on the machine that
  captured the baseline (the owner's Linux PC since A6b). The whole-Bible perf run is nightly.

---

## 8. Performance

### 8.1 Counters

- `FoldState::counters()` (`storage:crates/tc-journal/src/fold/state.rs:1830`), already public.
- `tc_resources::content_files_opened()` and `content_files_written()` are thread-local
  (`crates/tc-resources/src/store.rs`). A count test that loads on rayon threads sums the counter
  on every worker, or loads on the test thread. The seeding plan runs inline in `App::start`, on
  the caller's thread, and the `Seed` job's counts are taken on its own thread and reported in the
  `seed` report's facts (`contentFilesOpened`, `contentFilesWritten`) *(A9)*.
- tc-store's counters (R14), reachable through `ProjectHandle::counters` since C0 *(A-5; C0: S1
  provides them, so C0 counts nothing of its own)*.
- `AppCounters`: `records_read`, `values_decoded`, `items_derived`, `rows_serialized`,
  `chapters_projected`, `book_resource_loads`, `summaries_computed`, `keys_probed`,
  `saves_waiting_behind_a_draft_write` *(A-20, A-21)*, and `conflict_walks`, `books_rendered`,
  `book_usfm` and `files_rendered` for the counted walks below (B1b).
- `CountingSnapshot` wraps every snapshot a query gets. It counts the records it reads, the keys
  R1's range reads return (`keys_probed`), full conflict-list walks, `render_book` and `book_usfm`
  *(A-21)*, and each file `render` gives: a book's text as `book_usfm`, `conflicts.json` as a
  conflict walk, any other file as `files_rendered` (B1b). *(B1b: `crates/tc-app/clippy.toml`
  refuses `record_keys`, `frontier`, `device_stats`, `derived_files`, `batch_changes`,
  `conflicts`, `conflicts_of`, `unreadable_records`, `retry_unreadable`, `conflicts_rebuilt`,
  `metadata_source`, `render_book`, `book_usfm`, `render`, `scope_digest` and `scope_digests`
  anywhere in tc-app's code but `counters.rs`, whose counted wrappers carry an `#[expect]`, and
  B1a's seed check, which reads a whole fold once when a fixture loads. These are the public reads
  of `FoldState` that walk every key, change, book, batch or conflict, or project a whole book,
  for all they are asked or for some of it: `render` and the two scope digests walk every
  conflict for `conflicts.json`, and `render` projects a whole book for its USFM. Every other
  public method costs a number, one record, one chapter, one book or the stories, the books,
  devices and project details, the conflicts its caller names, or builds a fold.
  `tests/op_counts/lint.rs` reads every `pub fn` of tc-journal's `impl FoldState` blocks and
  fails when one is in neither its refused list (which must match `clippy.toml`) nor its list of
  what each other one costs, so a new method or a missed one waits for someone to say what it
  costs; the same file fails when any file of tc-app's library but those two and the seed's unit
  tests turns the lint off. A count fails only on a path a test runs; the lint fails every path.
  B1b proved it fires on each path through the re-export `tc_journal::fold::FoldState`; the first
  list missed the four conflict-list and every-book walks after `conflicts_of`, and the second
  missed `render` and the scope digests, which is why every method is now listed. Test crates
  allow it, because tests read whole folds to check results.)*

Every count test runs the action on tc-corpus Psalms and on ten times Psalms and fails if a count
grows (PERF:20-24). From B1b to C1a they run on `MemoryBackend`; from C1a on, on `StoreBackend` too
*(C1a, D10: the corpora written to tc-store's `FaultFs` as tc-store's own count tests write them;
the store's counters join each row's cost; an open on the store folds the project, which is the
store's budget (tc-store's `tests/store/opcount.rs`), so the open row holds the app's counts only
there)*.
*(B1b: the harness is `crates/tc-app/tests/op_counts/harness.rs`: `costs_what_it_touches(name,
before, action)` runs an action through a fresh `App` over each corpus, compares `AppCounts` and
the project's fold counters, and names every field that grew. Its rows are `open_lists_no_key`,
`snapshot_costs_nothing_at_any_size`, `a_chapter_read_costs_one_chapter` and
`a_token_round_trip_costs_one_record`; `the_harness_fails_an_action_that_costs_the_project` is the
planted case that shows it can fail. Each later unit adds its rows there.)*

### 8.2 Budgets and count tests per screen

Count tests live in `crates/tc-app/tests/op_counts.rs` unless named otherwise.

| Screen, action | Budget (PERF line) | Count test | Earliest backend |
|---|---|---|---|
| Home list, 10 cards | proposed 3 s repeat launch (PERF:100); no cost per history (PERF:12-19) | `home_costs_projects_not_history`: fresh summaries mean 0 folds, 0 pack reads, at most 5 files per card; a stale card starts one summary job | store only: the memory backend's `peek` folds, which the product never does *(A-25)* |
| First launch | 60 s with a determinate count (PERF:99, rule 4); a read of each bundled file once, a write of each once, no read back (PERF:133-138) | `first_launch_opens_each_bundled_file_once_and_writes_each_once` (`crates/tc-app/tests/app/seed.rs`, in process, because the counters are per thread); `quit_while_seeding_stops_the_seed_and_waits_for_it`; `c29_a_stop_asked_at_the_last_file_of_a_copy_leaves_that_copy_finished` (`crates/tc-resources/tests/seed.rs`: a copy whose files are all written is finished, whatever the watch says); `a_bundle_that_lost_an_install_fails_the_startup_at_once_and_copies_nothing` and `a_leftover_that_cannot_be_removed_stops_neither_the_start_nor_the_seed` (same file); Playwright `prepare-smoke.mjs`. Until the installer ships the archives themselves (DELTA §3.6 item 3), the count is per bundled file, not per archive; the measurement is `docs/evidence/first-launch-seeding-2026-10-18.md` *(A9)* | none (A9) |
| Repeat launch | 0 resource content reads (PERF:100) | `repeat_launch_reads_no_resource_content_file` (`crates/tc-app/tests/app/seed.rs`, in process, because the counters are per thread), with `a_repeat_launch_is_ready_at_once_and_changes_no_resource_file` (`crates/tc4/tests/seed.rs`, the real binary) *(A9)* | none (A9) |
| Open project | 3 s / 10 s; first report within 300 ms (PERF:89-90) | tc-store owns it; tc-app adds `app_forwards_open_reports_unchanged` and `open_lists_no_key` (`record_keys` 0, no index) *(A-21)*. *(B1b: `open_lists_no_key` runs on the memory backend now: an open and `hello` make no counted read and do no fold work, every `AppCounts` field and fold counter 0, on Psalms and ten times Psalms. tc-journal counts no work for `record_keys`, so no count can see a key listing: "`record_keys` 0", here and in every row that says it, is held by §8.1's lint, which refuses the call on every path, and by `only_the_counted_walks_and_the_seed_check_turn_the_lint_off`, which fails when any file of tc-app's library but the counted walks and the seed check turns the lint off)* | store (memory for `open_lists_no_key`) |
| Open a book (Understand, Translate) | 150 ms; first chapter 50 ms (PERF:73) | `open_book_touches_one_book`: records read equal that book's | memory |
| Move to another chapter | 16 ms (PERF:75) | `chapter_switch_sends_no_request` (journey `parity/app/journeys/translate.test.mjs`: no read of the translation or the book, only the new chapter's panes and helps and the place record, AD16) *(B4a)* | memory |
| Save one verse until "Saved" | 25 ms with fsync (PERF:74) | `save_verse_touches_one_verse`: one command, `record_keys` 0, summary update one chapter; `saves_waiting_behind_a_draft_write` is counted and must stay 0 once R22 lands *(A-20)* | store |
| Counts and conflicts after a save | 1 ms (PERF:76) | `drafted_counts_update_from_delta`, `conflict_counts_update_from_delta` | store |
| Book resources | 300 ms first, then cached (PERF:87) | `helps_load_one_book`: `book_resource_loads` 1, then 0 | memory |
| Lexicon | 100 ms once; under 1 ms (PERF:88) | `lexicon_lookup_reads_nothing` | memory |
| Open the Check screen | 250 ms first, 20 ms cached (PERF:77) | `check_open_derives_one_unit`; `check_open_cached_derives_nothing` and `check_open_cached_sends_changed_rows_only` (`rows_serialized` equals the rows changed since the UI's `ViewSeq`) *(A-12)* | memory |
| Rail filter, sort, group | 16 ms (PERF:78) | `rail_filter_sends_no_request` (harness); `rail_groups_visit_each_row_once` *(A-13)*; Playwright frame timing on the Psalms rail | memory |
| Record a decision | 25 ms; counts 1 ms (PERF:79) | `decision_serializes_one_row` and `decision_refetches_no_rail` (the `Ack` carries the row; the `Changed` that follows sends no request) *(A-12)*; save latency while a checkpoint runs is measured by unit P (R9) before mode-switch checkpoints are adopted *(A-11)* | store |
| Re-assess after a verse edit | 1 ms (PERF:80) | `verse_edit_reassesses_its_checks_only` | memory (scripted delta), store |
| Count and plan an upgrade or gateway change | 2 s; 5 s background, 300 ms per book (PERF:81-82) | tc-check owns the counts; tc-app `plan_cost_follows_affected_books` | memory |
| Open the aligner | 30 ms (PERF:83) | `align_open_touches_one_verse` | memory |
| Aligner edit | inside 30 ms with the round trip (PERF:85) | `align_edit_touches_one_verse`; Playwright `align_click_round_trip_p95` | memory |
| Reflow after a verse edit | 2 ms (PERF:86) | `reflow_touches_one_verse` (at aligner open, AD19) | memory |
| Suggestions | 50 ms; training in the background (PERF:84) | `suggest_uses_trained_model_without_training` | memory |
| Conflict review | interactive | `conflict_tab_costs_its_conflicts` (every tab, at most four reads per listed conflict and one walk of the conflict list, the same on Psalms and ten times Psalms); `a_verses_history_costs_its_versions` *(B11)* | memory, store |
| OBS story | interactive | `story_open_touches_one_story`: a story's open renders its one derived file and projects no book, walks no conflicts and probes no key, and its counts do not grow when every other story holds ten times the text and a comment per odd frame; `a_story_projects_resources_load_once_for_every_story`; `a_story_reads_its_own_decisions_and_comments` (tc-journal) *(B7)* | memory |
| Typing, input to paint | 16 ms (PERF:98) | Playwright `typing_input_to_paint_p95` (Event Timing) | memory |
| Long lists | 16 ms per frame (PERF:97) | Playwright trace on the Psalms rail and chapter | memory |
| Memory | 350 MiB peak, 150 MiB steady (PERF:96) | G2 measures with one screen session and one side session (Q-15) | store |

### 8.3 Never blocking

- **Server**: `tokio_never_blocked`, a test-only watchdog that pings the runtime every 10 ms
  under journey load and fails on a ping more than 50 ms late. Installs run on blocking threads,
  because `install_dcs` fsyncs inside an async function. A2 builds it (`tc_server::watchdog`) and
  runs it with the server on one worker thread, where any blocking call shows, under a paced
  synthetic load (API calls, event streams, an upload, static files and a download). It also
  times a plain thread's 10 ms sleep and allows that lateness on top of the 50 ms, so a loaded
  machine does not fail the test for what the operating system did. The load is paced because a
  saturated single worker polls its timers late and looks blocked with no blocking call in it.
  Two tests show the test can fail: the watchdog sees a worker blocked by `thread::sleep`, and the
  same load against a server with one blocking call is caught. *(A2)*
- **UI**: no synchronous XHR; typing writes only the browser buffer and sends one asynchronous
  message to the shell's mirror; the Playwright perf specs
  fail on any long task over 50 ms during a measured action (Long Tasks API).
- **The aligner round trip** is measured first, in unit A2: a 20 KB JSON reply over loopback from
  a wasm `fetch`, p95 recorded in `docs/evidence/`, and once more through a proxy that adds a
  header to each request: the cost if G0 finds that the shell's filter cannot leave `/api/` out.
  The proxy is Playwright's request interception, which pauses each request and lets a JS handler
  in another process add the header, as Electron's `onBeforeSendHeaders` listener does. A2's
  figures (`docs/evidence/loopback-2026-10-18.md`): plain p95 5.2 ms, through the proxy p95
  8.6 ms, a 20 KB reply, 1,000 clicks each, release `tc4`, headless Chromium 141 on the 4-vCPU
  build container (load about 3). *(A2)* Because the product's numbers come from Windows with Defender
  and Electronite's process model, the spike G0 measures the same round trip in Electronite on
  Windows, and the reference laptop's run is an owner action before B6 merges (Q-26). A miss in
  either raises Q-13. *(A-14)*

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
| C | tc-journal and the domain crates in one workspace; one owner per shared type; `Session::open` over the fold; validity read from the fold; the To do and alignment-span disagreements settled | B1b–B11 (the read halves) |
| S1 | the actor, commands, durable acks, the open with progress, read-only modes, drafts, `atomic_write`, app-data layout; and R4, R5, R14 and R16 (`crates/tc-store/src/handle.rs`, `reply.rs`, `counters.rs`) | C0, then C1–C8; C0's size depends on R4, R5, R14 and R16 *(A-5; C0: S1 provides all four, so C0 is the thin adapter)* |
| S2 | materialize, checkpoint, outside edits, local git | D1, D2 |
| P | budgets at whole-Bible scale; R9's save latency during a checkpoint, which decides AD34 *(A-11)* | G2; D1's mode-switch checkpoints |
| X | sync, share, join, USB exchange, `ExchangeReport` | E1a, E1b, E2 |
| I | importers, the tC4 seed | F1 |

Unit C settles two disagreements that block the Check and Align read halves. Its task says so
(`.audit/workflows/storage-chain-B.js:19`), and its commit `658ed0b` on `storage` has settled
both, as follows:

- **To do.** STORAGE stamps the verse md5 and clears `imported_invalid` whenever the status changes
  (STORAGE:1526-1528); tc-check's To do changes only the status (`crates/tc-check/src/status.rs:295-306`).
  Unit C was told to follow tC4, and STORAGE §5.4 now says so: To do changes only the status.
- **Alignment span.** `staleness` compares `span_end` with the run end directly
  (`crates/tc-align/src/value.rs:174`); STORAGE computes the expected end from the anchor
  (STORAGE §7.7). At `658ed0b`, `AlignmentValue::staleness` in `tc_journal::alignment` computes
  validity for both the fold and the Align screens.

### 9.2 Requests to the storage team

None changes a storage spec unless it says so. Each has a fallback, and the column "In a task
text" says whether a storage unit's task text in `.audit/workflows/storage-chain-B.js` asks for
it today *(A-5)*.

| R | Need | Owner | In a task text | Fallback |
|---|---|---|---|---|
| R1 | Range reads: one chapter's verses, one (tool, book)'s decisions, one book's alignments and comments, each costing its result. The fold already keeps them per chapter in `ChapterEntry` (`storage:crates/tc-journal/src/fold/state.rs:242-257`), which is crate-private. | C or P | provided: tc-journal `c4b8663`, `fold/ranges.rs` (B1b uses it) | the probes of §4.3, which cost what they show; no project-wide index *(A-21)* |
| R2 | Create a project in a staging folder and rename it into place without replacing (DELTA:619-624) | S1 | provided by S1: `crates/tc-store/src/store.rs` (`Store::create`, `staging`, `place`; `Vfs::rename_dir_noreplace`) *(C0)* | create waits; parity projects stay on `MemoryBackend` until then (no longer needed) |
| R3 | Add a book with imported text (tC4 `addUsfmBooks`) | S1 or I | built by C6: tc-journal `Command::AddBookText`, tc-store `add_book_text` (one batch, cause `Import`, §13.5's alignment split; C37) | none: the earlier fallback (`AddBook`, then one `SaveSection` per chapter) left a stub book when it stopped half way (#767) |
| R4 | The `ProjectHandle` surface of §4.2: snapshot with its sequence, mint and parse, an async submit, typed events, drafts, leave, checkpoint, counters, close | S1, S2 | provided by S1: `crates/tc-store/src/handle.rs` (`ProjectHandle`, `Snapshot`, `Pending`, `StoreEvent::Committed` with each batch's `Delta`) *(C0)* | C0 builds the surface around the actor (§4.2). C0 cannot build an O(1) snapshot or a command's `Delta`; without them phase 2 waits *(A-5)* (not needed: C0 is thin) |
| R5 | A read never waits behind a long actor write; reads use the latest published snapshot | S1 | provided by S1: `crates/tc-store/src/handle.rs` (`Shared::publish` after every batch; tc-store's `reads_never_wait_behind_a_held_checkpoint`, and C0's `reads_never_wait_behind_a_long_command` through the seam) *(C0)* | C0 publishes the snapshot after each command and intake, and reads take the published one *(A-5)* (not needed) |
| R6 | `Draft` for `DraftField::Section` carries every displayed token (STORAGE:1754-1756; the command has one, `command.rs:113-117`) | S1 | provided by S1: tc-journal `command.rs` (`Draft.shown: Vec<ShownToken>`, `DraftField::Section`) *(C0)* | the JSON envelope of §6.6 (no longer needed) |
| R7 | Commands for the §8.3 choices the set cannot express: bulk resolve in one batch, carried-file rename and keep both, a pins choice with its carry-over, the presence-race restore, accept all from the file, act on a held file, dismiss a notice, undo an outside edit, re-key a decision with `MovedTo`, copy a read-only project (with its drafts, §6.6) | S1, S2 | in part, by S1: tc-journal `command.rs` (`ResolveBulk`, `MoveCheck`, `Dismiss`, `ReleaseBatch`, `Choice::RenameCarried`), tc-store `Store::copy_project`, `ProjectHandle::{restore_held, dismiss_notice}` *(C0)* | the review offers only what exists, and "Later" |
| R8 | Test-only holds before a book read, a checkpoint, a materialize, a draft write, B3 | S1, S2 | provided by S1: `crates/tc-store/src/hooks.rs` (`Point`, feature `test-hooks`) *(C0)* | delays through `FaultFs` (no longer needed) |
| R9 | Measure save latency during a materialize, a checkpoint, a merge commit and a 150 MB `ApplyUpgrade` (25 ms budget, PERF:74) | P | no | the UI shows "Saving…" and never blocks; mode-switch checkpoints (AD34) are not adopted until this is measured *(A-11)* |
| R10 | Sync, share and join entry points with progress, cancel, `ExchangeReport`, and `NotShared` (DELTA:645) | X | yes (§9, §10) | `ScriptedSync` |
| R11 | Import review as a dry run; import job with progress and cancel; tC3 version lookup | I | partly | `ScriptedImport` |
| R12 | `tc-door43`: `count_repositories(owner, lang)` and a `repos/search` route in the fake | app track, B9b | built (B9b): `Door43Client::count_repositories(session, owner, language)` reads `X-Total-Count` of `repos/search?owner&lang&limit=1` and refuses an answer without it; the fake counts the repositories it holds by owner and language, private ones for members only | — |
| R13 | `tc-door43-fake`: a dropped-connection fault and a runnable binary; today's faults are statuses, delays, redirects, archive changes and, for archive and export downloads only, a held, broken-off or unsized body (A11) (`crates/tc-door43-fake/src/lib.rs:171-215`); git smart HTTP has none | X or app track | the runnable binary with a control port and a `Hold` fault for any request: A8; the dropped-connection fault: no (X or E1a) | scripted outcome |
| R14 | tc-store counters reachable from tc-app tests (files opened, fsyncs, actor queue waits). The fold's half exists: `FoldState::counters` is public (`storage:…/fold/state.rs:1830`) | S1 | provided by S1: `crates/tc-store/src/counters.rs` (`StoreCounters` through `ProjectHandle::counters`), reachable through the seam's `ProjectHandle::counters` *(C0)* | C0 counts commands, replies and waits itself; files opened are counted only if tc-store counts them *(A-5)* (not needed) |
| R15 | A `DraftField` for user comments (`command.rs:207-215` has none) | S1 | provided by S1: tc-journal `command.rs` (`DraftField::UserComment`) *(C0)* | the browser buffer and the shell's mirror *(A-3, A-18)* (no longer needed) |
| R16 | `StoreReply.seq` for read-your-writes; typed notices for `SecondWriter`, `SavedWithDiskError`, a blocked materialize | S1, S2 | provided by S1: `crates/tc-store/src/reply.rs` (`StoreReply.seq`), `handle.rs` (`StoreEvent::Notice` with tc-journal's `Notice`) *(C0)* | C0's own `seq`, raised with each published snapshot; notices mapped from tc-store's error types *(A-5)* (not needed) |
| R17 | A checkpoint on every mode switch with pending batches (amends STORAGE:1723) | S2, owner | no | as STORAGE says today if the owner declines; adopted only after R9 *(A-11)* |
| R18 | Two settings in one batch (direction and font) | S1 | no | two batches; the dialog says which failed *(C6 built the fallback: `SettingsRefusal::{DirectionNotSaved, FontNotSaved}`)* |
| R19 | The token's kind (edit or review) under its MAC, a purpose byte in the payload or a second derived key, so the actor refuses a review token in a save and an edit token in `Resolve` (STORAGE §2.7) | C, which is editing tc-journal now | provided: tc-journal `token.rs` (`TokenKind`, `"kind"` in the payload under the MAC); tc-app's `parse_edit` and `parse_review` check the MAC, then the kind (B1b) | the seam's two types and the wire prefixes (AD3) *(A-7)* |
| R20 | A draft whose token fails its MAC is offered as text, and Keep records it as a change with no parents, which stands as a conflict, as STORAGE does for outside edits it cannot place (STORAGE:3637). Stated in §5.14 and O12. | S1 | provided by S1: `crates/tc-store/src/drafts.rs` (`OfferedDraft.verified`), `commands.rs` (`KeepDraft` of an unverified draft writes no parents) *(C0)* | the offer shows the text with "Copy text" and stays until Discard (§6.6) *(A-9)* (no longer needed) |
| R21 | Keep a checkpoint's long work off the actor: only the seal and the final reference update are actor steps; materialize, the tree build and `tc_burrito::check` run on a snapshot (STORAGE:1725-1735) | S2, measured by P | no | Q-5's limit of one mode-switch checkpoint every two minutes *(A-11)* |
| R22 | A draft write never delays a command: drafts are not journal data, so they are written beside the actor, not as one of its steps (STORAGE:1752-1753) | S1 | provided by S1: `crates/tc-store/src/drafts.rs` (`DraftWriter`), `handle.rs` (a draft goes to the writer, not the actor); C0's `a_draft_never_waits_behind_a_long_command` *(C0)* | saves that waited behind a draft write are counted (§8.2) *(A-20)* (no longer needed) |
| R23 | `DraftField::Alignment`, so unsaved links survive a crash | S1 | provided by S1: tc-journal `command.rs` (`DraftField::Alignment`) *(C0)* | the server saves links 2 s after the last change; up to 2 s of links can be lost in a crash (Q-27) *(A-4)* (no longer needed) |
| R24 | A way to revise a comment: a record that replaces the comment the box showed, kept in History | S1, owner (Q-24) | provided by S1: tc-journal `command.rs` (`Command::ReviseComment`) *(C0)* | each blur after a change adds a comment (§6.5) *(A-18)* (no longer needed) |
| R26 | A save never erases a newer draft of its field: a successful save removes a current entry only when it predates command submission or equals stored field text (STORAGE §5.14). Ordering uses local monotonic instants, independent of page clocks; offered drafts keep their separate lifecycle | S1 | provided by C5: `DraftWriter::saved` preserves newer differing text before Ack; `a_save_never_erases_a_newer_draft_of_its_field` is enabled; `a_newer_alignment_draft_survives_a_crash_before_the_saves_ack` proves crash/reopen/Keep with both tokens and valid Burrito | No fallback needed. The editor may still refresh a dirty draft after Ack with the reply's tokens; recovery no longer depends on that round trip |
| R27 | tC4 #766: at the first open of a project whose pin groups are all empty, write the creation pins (one `Pins` change per group from `tc_resources::suite::project_pins()`, one batch) with a new `System` reason beside `PinCoverageBackfill`, before format 1.0; a project with any pin keeps its pins byte for byte (TC4-DELTA-2026-10-19 §3.3 item 4, §3.8 item 3). tc-app could only send `SetPins`, whose cause is `Edit`, so History would say the translator changed the resources *(C1a, D11)* | C1a | provided by C1a: tc-journal `SystemReason::CreationPins` (wire `creationPins`); tc-store `OpenOptions::creation_pins`, written by a writable open after O7 only when no pin group has a head and every journal file was read (`write_creation_pins`); tc-app passes `project_pins()` on every open (`StoreBackend::open`); `open_writes_creation_pins_only_when_none_exist` | — (STORAGE §5.6 and its wire table still name only `pinCoverageBackfill`: the storage branch adds `creationPins`) |
| R28 | tc-store's cloud check (`platform.rs`, `cloud_client`): `~/.dropbox` that holds `info.json` is Dropbox's settings folder, not a sync marker (today every project under such a home opens read-only, "copied by Dropbox", risk 23); and a way to give the check the environment as a value, so the app's first-launch rule can call it and be tested for another computer *(C1a, D6)* | S1 owner | no | `crates/tc-app/src/folders.rs` stays, with the right marker rule, for the first launch; projects on a Linux or macOS computer with Dropbox installed open read-only until R28 lands |
| R29 | A public writer of `ingredients/tc/project.json` (or an import entry over `Store::staging` and `Store::place`), for unit I and for C1a's scenario writer *(C1a, D1)* | I | no | the scenario writer writes the four fields C0's listing and tc-store's open read, in tc-store's order; `the_store_scenario_holds_exactly_the_seed` opens what it wrote |
| R30 | `ProjectHandle::is_idle()`: no queued command, no background job, no draft write pending, without blocking and without flushing drafts (`wait_idle` does both) *(C1a, D13)* | S1 owner | no | `test/quiet` counts the requests, jobs, sessions and held store points it sees; the open's storage check and a materialize after a save stay unseen (nothing a capture shows depends on them today) |
| R31 | `ApplyUpgrade` writes a plan's arrival at a record the plan did not read (a decision moving onto a check another decision holds, or two moving onto one) as a root change, a visible conflict, as STORAGE §5.8 says, instead of refusing the whole plan as stale *(C8)* | S1 owner | no | the fallback: such a decision is not moved and stays where it is, listed among the checks to check again, so nothing is replaced, hidden or lost; the dialog counts it as "to check again" |

**Who can add them** *(A-5)*. The task texts of units C, S1, S2 and P
(`.audit/workflows/storage-chain-B.js:19-22, 30-33`) ask for none of R1, R4's handle surface,
R5's guarantee, R14's tc-store counters, R16 or R19 to R23. This design cannot change those texts;
the owner can (Q-23). The recommendation is to add R1 and R19 to C's text, since C is editing
tc-journal now; R4, R5, R14, R16, R20, R22 and R23 to S1's, with the contract cases of §4.2 as
their tests; R21 to S2's; and R9 to P's. For each request left out, C0 is built at its larger
size or the fallback above applies, and §10 says which unit waits.

A lockless read-only open for Home summaries is not requested: AD13 needs no fold.

*(C0, 2026-10-19: S1's tc-store, merged into main and `app`, provides R2, R4, R5, R6, R8, R14,
R15, R16, R20, R22, R23 and R24, R7 in part, and R25, the reflow written in the verse's batch
(`crates/tc-store/src/commands.rs`, `save_verse`); the rows above say where. R3 is not: tc-store's
`AddBook` seeds tC4's blank book from a source, so imported text still needs R3's fallback.)*

### 9.3 What can be built when

| Stage | Real | Compared with tC4 |
|---|---|---|
| Now (phase 0) | server, transport, security, jobs, events, i18n, RTL, the save machine and draft buffer with their crash tests on scripted outcomes, shell chrome, account menu, the internet question, help, about, feedback compose, Door43 sign-in against the fake, first-launch seeding, the print model, streaming downloads, the parity harness and the tC4 gallery; the Electronite spike, run on Windows; the seed spike on a branch from `storage` | DOM of about 12 dialog subtrees; behaviour of the account menu, M15 and M16; pixels wait for Home (B2) |
| After C (phase 1) | every screen's read side over a real fold of the fixtures; check lists, aligner sessions, helps, OBS, the print preview, the conflict review read side; save behaviour on scripted outcomes | all 62 baselines (57 real, 5 scripted); operation counts for every read |
| After S1 (phase 2) | the store adapter, then every save, the real open, drafts and offers, conflict choices, create (with R2), upgrades | J2, J4, J5, J6, J12, J13, J15, J16, J18 (local), J21, J22, J25; J1 and J20 with R2 |
| After S2 (phase 3) | checkpoints, outside edits, exports | J7, J23 (without the Burrito zip), J8 |
| After X (phase 4) | sync, share, join, USB | the 5 scripted screens recaptured; J11, J17, J24, J26, USB, sync-button |
| After I (phase 5) | import | J9a, J9c, J9d; the seed replaced |

---

## 10. Work breakdown

Each unit is one builder agent's job and one coherent commit series. Every unit ends with
`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, wasm clippy for `tc-api` and for `tc-ui` with `gallery`, `app` and
`harness` (AGENTS.md), and its rows in `report.json` and its operation-count tests where it has
any.

This breakdown was checked again after the red team *(A-16, A-17)*:

- **One agent.** A unit holds one subsystem, or one screen group with its own endpoints. Work
  that is not UI (a domain crate change, a new crate, a streaming client) is named in the scope,
  or is its own unit (A10, A11). The view types and `wire` conversions of a screen are built by
  that screen's unit, not up front.
- **Verifiable here.** Every "done when" is a test, a report row or an evidence file this
  container can produce, except the Windows runs of G0, G1 and G2, which say so. Electronite's
  Linux build does run here, under Xvfb as root with `--no-sandbox`: G0 ran every case here on
  2026-10-17 (`docs/evidence/electronite-spike-linux-2026-10-17.md`). What needs Windows is the
  Windows numbers. *(G0: PLAN:151-152 said Electronite could not run here.)*
- **Ordered.** "Depends on" lists only units that come earlier in the order paragraph at the
  end, and storage units; owner answers and storage requests are under "Waits for".
- **Honest about storage.** "Waits for" names each storage request (§9.2) or owner answer a unit
  needs and what the unit does without it. Nothing in phases 0 and 1 writes a project. Phase 2
  starts with C0, whose size depends on S1.
- **No-loss rigor.** The units marked ◆ hold code whose mistakes lose a translator's work: A4b,
  B4b, C0, C1b, C2, C3, C5 and G1. They get PLAN's rigor (PLAN:158-170): property tests,
  crash-injection tests (`test/crash`, `FaultFs`), and an adversarial review by several models
  before they merge.

### Phase 0: now, on main (and G0 on Windows)

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| A1 tc-api foundations | the crate; ids; `Scope`; `Endpoint`, `endpoints!`, `ALL`; tokens with their prefixes *(A-7)*; `Shown`; `Ack` with `Stored` and rail patches; `NotSaved` with `StaleScope` *(A-19)*; `ApiError` with `RequestReused` *(A-2)*; `Refusal` families, `SyncRefusal`; events (`ChangeScope` as an enum, `Aspect`, `AlignSave`), jobs, progress; the state enums of §3.7 *(A-30)*; `depends_on`; the endpoints of phase 0; rebuild key files (English, with the nine other languages listed as pending) *(A-32)* | — | — | serde proptest; golden JSON per endpoint (rewritten only with `UPDATE_GOLDEN=1`); the four refusal tests and `release_has_no_pending_translation` (§3.5); a token text with the wrong prefix does not parse | builds and lints for wasm32; every phase-0 endpoint has golden JSON |
| A2 tc-server and `tc4 serve` | axum on loopback; address line; `app.lock`, with its holder's pid in `app-lock.json` beside it (§5.1), and the 10 s wait, only for the restart case (the single instance is G1's lock); tc-api's `ReadOnlyReason::OpenElsewhere` ("another translationCore window holds the project's lock", `crates/tc-api/src/write.rs` on branch p0-a1) is removed, and a held project lock is a refusal that names the holder's process (ruling of 2026-10-18); quit on `{"quit":true}` and on a closed stdin *(A-3)*; security layer; CSP; static routes; `hello` from a stand-in backend until the join with A3 (order paragraph); the event stream: the frames of tc-app's `EventBus` as server-sent events, with the heartbeat, ending a connection whose subscription lags (the ring, `Resync` and coalesced progress are tc-app's, §4.1); uploads with per-flow caps *(A-28)*; downloads registry; `test-hooks` feature with `test/crash`; the loopback spike, also through a header-adding proxy *(A-14)* *(A2: §4.1 gives the ring to tc-app, which A3 builds at the same time; a second ring in tc-server would be thrown away at the join.)* | A1 | — | §5.2 security tests; `closed_stdin_quits_within_5_s`, `new_tc4_waits_for_the_old_one_then_names_the_holder`; replay and `Resync` through HTTP (a scripted event source until the join); a 513 MiB import upload is 413 and a 1 GiB exchange upload streams to disk; `every_endpoint_is_routed_once`; `tokio_never_blocked` | the tests pass; the spike's two p95 figures are in `docs/evidence/` |
| A3 tc-app foundations | `App`, `AppConfig`; `JobRegistry` with reporter (100 ms, or 1 s for jobs no screen shows *(A-29)*) and cancel; `EventBus`; `FlowGate`, with USB export and receive per project *(A-31)*; `PrefsWriter` (memory) with the `lastEdit` rule *(A-20)*; `ReportLog`; internet wiring; `Keychain` trait with `MemKeychain`; `ReplyCache` with body digests and in-flight waits *(A-2)*; `test/fieldStore`, a test-only write endpoint that goes through the reply cache and records every text it stores, so A4b can prove its no-loss rules before any real save exists; `app`, `prefs`, `net`, `jobs` handlers; `App::call`, the transport-free entry the server routes to; the projects folder of Q-4 (§4.9) | A1 | — | `job_reports_nothing_before_300ms_then_every_100ms`; `unseen_jobs_report_once_a_second`; `counts_only_grow`; `cancel_stops_at_next_chunk`; flow-gate tests, including `receive_and_sync_of_one_project_never_overlap`; reply-cache tests for a finished request, one in flight, and another body; `hello_answers_real_state`; the `test/fieldStore` crash, hold and retry tests | `App::call("app/hello")` answers with real state (this launch's instance, the internet switch, the kept sign-in, the saved app language, the open sessions and the startup state), in the bytes `tc4 serve` sends; `tc4 serve` answers it over HTTP once A2 and A3 are joined (order paragraph) |
| A4a tc-ui runtime: transport, cache, language | feature `app`; API client with scope drop; SSE-over-fetch reader; `QueryCache`; `Load<T>`; no tab guard, because the ruling of 2026-10-18 removed the one-tab check *(A-19)*; `t()` with generated keys and rebuild keys; all ten catalogs at boot *(A-23)*; `<html lang dir>`; `build-app.sh`; web-sys features (`Storage`, `Headers`, `Request`, `RequestInit`, `ReadableStream`, `ReadableStreamDefaultReader`, `TextDecoder`, `AbortController`, `File`, `Blob`, `DragEvent`, `DataTransfer`, `HtmlAnchorElement`, `HtmlIFrameElement`, `PerformanceObserver`); vendor tC4's `src/ui.css` again *(A4a: built as the row says, with these refinements, each with its reason where the design text names it. `api.call` returns `Result<_, CallError>` and `Load::Failed` holds a `CallError`, because HTTP statuses and network failures are not `ApiError`s (§6.2, D1). Scope drop is for reads; a write's reply is never dropped, because A4b's save machine needs every `Ack` and `NotSaved::StaleScope` (§6.2, D2). Internet, account, startup, the open sessions, the saved language, jobs and the last checkpoint outcome are `ServerState` signals, not cache entries, because they arrive whole and must exist before any screen holds them (§6.1, D3). The event reader asks `?after=0`, reads `hello` only after the stream's head, never skips a `Resync`, treats a frame it cannot read as a `Resync`, reads each job it holds as active with `jobs/get` after one, and reconnects after 45 s of silence (§5.4, D4). English and the rebuild's key files are compiled in and only the nine other catalogs are fetched, because `t()` needs English even when a fetch fails (§6.9, D5). Key constants are nested modules, because flat names collide in tC4's catalog (§6.9, D6). The UI files are in a folder named by a hash, because tC4's CSS imports by relative URL (§3.10, D7). A different build or API version reloads once, whichever request finds out (`hello`, a call, or the open or reconnect of the event stream, which meets it first), and the same mismatch after that, or a page that cannot keep the marker of its reload, shows the boot error; `boot::build_action` is the one decision, host-tested, and the same decision is carried out in the middle of a session: the boot error replaces the app, the first decision of several requests stands, and the event stream reads no more (§3.10, D8). The boot error is a rebuild-only screen (§6.12, D9). The smoke's "no request after boot" leaves out font files, which the browser loads when Arabic first shows (§6.9, D10). Until A5 ports `App.jsx`, the page mounts a stand-in root: a sample line, the language picker, which calls the `show_locale` that the language dialog will, and the internet state (D11); A5 keeps it as Home's content until B2 ports `Home.jsx` (A5 D5). `hello` holds no checkpoint outcome, no blocked file and no aligner save state, so a `Resync` cannot restore what events alone carry: the page keeps what it holds, drops what belonged to an open `hello` no longer lists, and D1 and B6 make the rest readable; a restart forgets them, with the jobs, because the new run numbers its aligners and opens again (§5.4, §6.1, D12). A read of a project is keyed by its server run as well as its request, so the new run fetches a request that a dead read of the old run still holds (§6.2, D13). `build-app.sh` also puts A9's first-launch window, `app/prepare.html`, beside `index.html`, where `tc4 serve` serves `/prepare.html`, because A4a merged after A9 (A9's row). A host test of the event stream's parser and reader, of the cache's decisions, of what each frame does, and of `t()` and the key scan stands behind each; `docs/evidence/ui-runtime-2026-10-18.md` has the numbers and the smoke's last run.)* | A1, A2 | — | host tests of the SSE parser and `t()`; `i18n-keys` port with floors; Playwright smoke: boot, `hello`, a locale switch with no request after boot, a server of another API version or build (one reload, then the boot error; none with `sessionStorage` blocked), a `Resync` and a restart while a job is held | the smoke passes in this container |
| A4b ◆ tc-ui no-loss core: saves, buffer, panics | the join of A2 and A3 (order paragraph), if no earlier unit made it; the save machine (§6.5: `sent`, the NFC clean rule, retries with the first body *(A-2)*, `StaleScope` *(A-19)*); the refresh rule for open editors (AD37) *(A-6)*; the draft buffer v2 keyed by folder and project id *(A-9)*, with the bridge calls `bufferChanged` and `takeBuffered` *(A-3)*; `boot.js`: capture listener, reload, panic counting by page load, static callout, drafts posted with `fetch` *(A-15)*; panic hook and crash marker; the navigation gate and close guard *(A-26)*; `try_` forms and `clippy::indexing_slicing` in app modules *(A-15)* *(A4b: built as the row says, with these refinements, each written where the design text names it. The join was A3's (`crates/tc4/src/join.rs`). The save machine is `tc_ui::app::save` (D1, §6.5): pure, host-tested, returning commands that `web::saves` carries out in the browser and tc-app's tests carry out against `test/fieldStore`; `FieldSave` gains `Failed(SaveError)` and `Held(Hold)` (D2). The buffer is one `localStorage` item per field, keyed by the `BufferKey` as JSON, and `tc_api::buffer`'s entry and line are what the UI, `boot.js`, the shell and the server read (D3, D4, §3.10, §6.6). The bridge's buffer and close calls are exact (D5, §5.7). `test/fieldDraft` keeps a test field's draft, so the third panic case can show that the server holds the posted drafts (D6, §5.8). AD37's pair is taken again when an `Ack` cleans the field (D7, AD37, §6.3). Only a lost reply keeps its id, and a resend asks `app/hello` for the run first (D8, §6.5). The gate is tC4's `drainSchedulers` loop, never blocked by a section buffer or a held field (D9, §6.5). `boot.js` registers its listeners before a dynamic import of the glue, catches traps from `error`, `unhandledrejection` and a throwing `run_app`, counts page loads that ended in a panic, shows the static callout at once when `sessionStorage` cannot count, sets every text by `textContent`, and writes a field's entry from a template the WebAssembly hands it at focus (D10, §6.6, §6.8). `window.__tcIdle` counts saves (D11, §6.2). NFC is `unicode-normalization` on both sides, about 123 KB of the release wasm (D12). Found by the tests and changed in the design: the offer keeps an entry whose `sent` text the stored value equals over the stored value's token (§6.6, `crash_anywhere_stores_the_last_text_once`); the panic hook keeps the first panic of a page; `boot.js` owns the browser's `beforeunload`, which held its reload after a panic; and after three panics it posts the drafts of the run the page last met, because a panic at boot names no folder (§6.5, §6.8). Fixed after the adversarial review, each with a test that fails without it: `StaleScope` that comes before the session event waits for another open instead of going back at once, without limit, to the open that refused it (§6.5); views compare within one open, epoch first (§6.3, AD37); a field whose item holds another run's entry, or one this build cannot read, gets every keystroke into the mirror at once, and `Source::Input` rests on `boot.js`'s word (`tc4Boot.wrote`), not on an assumption (§6.6); the template `boot.js` writes is the page's own entry, so a second tab whose keystroke wrote the field last never takes over the first tab's save when it reloads (§6.6); a removal line names its run and the shell keeps the last line per field and run (§5.7); a restart has one idempotent path, and a field whose editor is on screen is kept as `Held(Hold::Restarted)` until A5 goes Home (§3.10); a close request answers whether everything is saved, held and section text included (§5.7); the page drains on `pagehide` as tC4 does, and after a panic the saves carry out nothing, which the drain's first Chromium run showed was needed (§6.5). Fixed after the second adversarial review, each with a test that fails without it: a storage that refuses a write leaves the stored item as it is, whatever save it names, where the first fix removed it when a save started or was refused (§6.6, `a_full_storage_keeps_the_text_*`); a reissue after `RequestReused` while the project has no open drops the answered request from the entry (§6.5; the property found it at 40,000 cases and now runs 2,048, with the shortest history as a test); a lost reply's resend carries the run it went to and goes only if the `hello` names that run and the field still waits for it, since the event stream can find a restart while the `hello` is on its way (§6.5, P16, with the test proxy now ending a response its server died in); `boot.js` reaches both storages through a getter that cannot throw (§6.8, P10b); after three panics `boot.js` posts no draft for an entry with a save in flight (§6.8, P3b); the drain on `pagehide` retries each standing failure once, as tC4's `drain` does (§6.5, P13b); and the `tc4Boot.wrote` handshake has a case of its own, in which `boot.js` writes nothing (P15). Declared differences from tC4: a close in the shell drains before the window goes, beside the best-effort drain on `pagehide` that the page keeps from tC4 (§5.7); a failure outside `NotSaved` shows Retry; one reload after a WebAssembly panic, then the static callout; a field restored after a reload saves with no editor (AD22); a failure stops only its own field's automatic saves, where tC4's stops every key of its scheduler until Retry (§6.5). Not built here, by the row: the screens (A5, B4a, B4b), the offer (B1b, C1b, through `Saves::buffered_for`), real saves and drafts (C2, C3, through `FieldWriter`), `align/flush` (B6, C5, through `Saves::add_gate_hook`), and the shell's side of the bridge (G1). Left open: a page that has read no frame cannot tell a restarted server behind a reused port from a reconnect by its event stream (A4a's reader asks `?after=0`, and the new run's ring is empty), so only the resend's `hello` check guards saves there; G1 should clear other origins' storage after it has read its mirror, since every launch is a new origin; and two tabs of one browser on one field, where a tab that crashes after the other tab's save wrote the field's item loses its own unsaved text of that field, a browser-only limit, since the shell has one window (§6.6). Tests: tc-api's `buffer.rs` and golden files; tc-app's draft tests and `tests/app/save_machine.rs` (8 deterministic cases and the two properties); tc-ui's `buffer`, `save` (the twelve ports of tC4's scheduler tests and the rebuild's rules), `save_refresh` (the AD37 cases), `save_props` (`no_current_is_dropped` at 2,048 cases, `events_reply_order_is_safe`) and `recovery`; `app/recovery.test.mjs` in node; `app/no-loss.mjs` in Chromium, 141 checks on dev and release (87 before the first adversarial review's fixes, 119 before the second's). Evidence: `docs/evidence/ui-no-loss-2026-10-18.md`: a keystroke's work 1.3 ms at p95 with 2 KB of text (release; 1.8 ms in the re-run after the review's fixes, which an interleaved A/B with the build before them puts down to the shared machine), one `localStorage` write and one mirror line per keystroke, no save and ten drafts in ten seconds of typing.)* | A1, A2, A3, A4a | — | port of tC4's scheduler tests; proptest: no `current` dropped; `reload_anywhere_stores_the_last_text_once` against `test/fieldStore`, with reloads and `test/crash` at every point; the AD37 refresh cases; Playwright: a panic in an input handler keeps the keystroke, one panic with trapping closures reloads once, three panics show the static callout and post the drafts | the tests pass and the adversarial review's findings are fixed |
| A5 Shell chrome and static surfaces | `App`, TopBar (X1), save indicator (X2), account menu (X6), M15, M16 with preview, M17 with License, M14 compose, S13 with vendored help pages (AD29) and the page-swap shim, S15 *(A5: built as the row says, with these refinements, each written where the design text names it. D1: S15 is the view-error boundary, by elimination: the screen map is not in the repository, `ui-inventory.md` lists after S13 the view-error fallback, the open overlay (S14, §7.4) and the Inspector (late, §6.12). D2: A5 builds the internet question itself (`tc_ui::app::internet`, M15 and its failure), because Help's links (S13) and Feedback's Send (M14) must ask before anything leaves; A8 keeps the sign-in steps (A8 row, §6.7); "Turn on internet" closes the question at once, as tC4's `answerAsk` does, while the switch turns on. The menu's switch turns away from the state last asked for, not the one the server has confirmed, and the `net/set` requests go one at a time, only the last choice made meanwhile going after the one on its way (`internet::SwitchRequests`; "Turn on internet" asks through the same queue and goes on when it settles): tC4's `setInternet` changes the switch before anything else, so a real double-click on the row turns it on and off again, here too, and two requests cannot reach the server in the wrong order. D3: the help pages are vendored with `vendor-help.sh` and `help-pages.txt` (AD29, Q-19). D4: the page-swap shim is tC4's code in JavaScript, `app/help-shim.js`, run by the page on the frame's documents; the server serves the pages unchanged (§5.3). D5: Home stays A4a's stand-in until B2 (A4a's note). D6: the driver's `home()` waits for what each app can draw (§7.1). D7: a behaviour step whose scope is gone on both sides compares equal, on one side differs, and every scoped step compares `<html lang dir>` (§7.3 layer 4). D8: `feedback/compose` composes the attachment on the server, in English, with the platform from Rust's constants (§3.5, §4.7). Send waits for that attachment, so nothing is sent that the dialog did not show (tC4 composes it on the spot, so its Send never waits): while the compose is on its way the box is empty and Send is off; a compose that fails says in the box, in the shown language, that the details could not be gathered and why (`feedback.attachmentFailed`, a rebuild key whose translations are pending), Send stays off, and closing the dialog and opening it again keeps what was typed and composes again, from "Ask for help" as from "Report a problem" (a declared difference, `residuals.json`). D9: the menu draws every row of tC4's three account states; the rows that need Door43 (sign in, check, profile, sign out) call `Actions` whose bodies A8 writes, and the last three cannot show before A8. D10: X2's "Checkpoint not saved" Retry (`retry-checkpoint`) calls `Actions::retry(Retry::Checkpoint)`, whose `session/retryCheckpoint` D1 adds; no phase-0 frame reaches it. D11: the Feedback report is never stored: no buffer item, no mirror line, no draft, as in tC4 (a name and an email are personal data, the message is not project work); a reload loses an unsent report, as in tC4; the journey `feedback_report_is_never_written_to_disk` checks it. D12: each Apply in M16 is a new request. D13: the design system's text props that a preview changes are signals (Modal's and Drawer's ✕ title, Field's label and hint, Select's and SearchField's texts, Layer's `label`, which names the account menu and the Help panel), and Modal takes live scrim attributes for `data-saving`; Action takes live attributes for the menu's `aria-checked`; Input writes a controlled textarea's text (its `defaultValue`) at each change of its value, as React's `updateWrapper` does, so the Feedback Message box's DOM follows what is typed; the DS gallery's DOM (751 of 751) and scenarios (53 of 53) were compared again with no change. D14: a page that boots before the resources are ready goes to `/prepare.html` (§5.3). D15: a new server run sends the App Home with no gate (§3.10). D16: the navigation snapshot `tc4.nav.v1` (§6.6). D17: tC4's `LICENSE` and `COPYING` are vendored and shown; the rebuild's own copyright line is Q-29. D18: what A5 leaves: the in-project halves of help a, of app-language g, h, j and of internet-consent i (About and License in a project), and app-language e, i, k go to B3 and B4a; the Home halves of app-language a, g, h, j (the Home heading, the cards) to B2; the M16 preview cost on the Check screen to B5b; tC4 #599's `switcher_stays_centred_while_the_save_indicator_changes` to B3; tC4 #613 (a Devanagari interface face, which tC4 4.0 lacks) waits for the owner; the Feedback mask's rectangle is tC4's 59-px attachment, so B2's pixel capture needs the `no Report` attachment that `test/reset` gives both sides. Found by the journeys and fixed: closures that read a signal which changes without the thing changing drew the Feedback dialog, the internet dot, X2 or the screen again (each reads a memo now), and a menu opened again while the last one faded out let its Layer give the focus back to the old panel's first row (the first-row hold waits one frame, as React runs the Layer's effect first). Found by the verification and fixed: About's focus hold, back from License, read its NodeRef for ten frames after About had closed, so a close inside them panicked and reloaded the page; every frame chain in the views now runs through `views::each_frame`, whose owner cleanup cancels the pending frame as tC4's effects do; and the account menu's name was read once, so it stayed in the old language after Apply (Layer's `label` is a signal now, D13). Found by the second verification and fixed: a real double-click on the internet row left the internet on, because both presses turned away from the state the server had confirmed (D2's queue now); the Feedback Message box's text stayed at its first value while the person typed (D13); the behaviour sequence `feedback-typing`, added for it, found that each keystroke drew the "not sent" callout again (it reads a memo now); CI's node step named the journey files one by one and left out About's (it takes the folder's files by a glob now, and `env-rust.test.mjs` fails while a journey file is not in the step); the model said the question stays on screen while the switch turns on, which D2 and the view contradict (corrected); and a failed compose was shown in English with no way back from "Ask for help" (D8). The mode switch is drawn again on a language change, and the project chip's title is read once, because AppHeader takes plain strings; B3, which first shows them, makes them follow a preview in place. Declared difference: a project name whose word starts outside the Basic Multilingual Plane gives its whole first character as an initial (`residuals.json`). Parity: `account-menu`, `app-language`, `help` and the `license` case the same as tC4, `about` and `feedback` declared under their masks; the sequences `account-menu-keyboard`, `app-language-preview` and `feedback-typing` the same after every step. Tests: tc-app's `feedback` (10), tc-ui's `shell`, `internet`, `feedback`, `language`, `indicator` (38), `help_pages` (3), three planted behaviour tests, tc-server's `a_help_page_with_a_query_is_the_page`; journeys in `parity/app/journeys/` (40 in Chromium, About's 5 among them); A4a's smoke gains the restart checks, A9's prepare smoke the redirect. Evidence: `docs/evidence/shell-chrome-2026-10-19.md`.)* | A3, A4a, A4b, A6b | — | scenarios: account-menu keyboard, M16 preview and restore, help a–i (no internet case yet) | dialog DOM parity for `account-menu`, `app-language`, `help`, `about`, `feedback` (layer 2, through A6b) |
| A6a Parity harness: tC4 side *(A-17)* | the `capture.mjs` split (`screens`, `driver`, `observe`, `env-tc4`); sha256 per PNG in `manifest.json` and its check; tC4 DOM snapshots for all 62 screens; `state.mjs` from the repository's fixtures | — (the tC4 checkout) | — | `state_from_repo_equals_state_from_tc4`; the baseline test fails on a changed PNG | a tC4 recapture after the split has identical hashes; the 62 DOM snapshots are committed |
| A6b Parity harness: Rust side and gate *(A-17)* | `env-rust.mjs` (`tc4 serve`, token header, `test/reset`, `test/quiet`, event stream left out of `trackRequests`); the four-layer compare tool; `masks.json`, `report.json`, `residuals.json`; `screen_record.rs` *(A6b: built. D1: the four-layer compare is Rust code inside the gate, `crates/tc-ui/tests/screen_record/`, which recomputes every layer from the committed captures on each `cargo test` and fails when the committed `report.json` differs (`UPDATE_REPORT=1` rewrites it), so a planted difference fails `cargo test` itself and the report cannot be stale; Node only captures (`capture-rust.mjs`, `behaviour.mjs`) (§7.1, §7.5). D2: a pixel counts when any channel byte differs, which is pixelmatch's count at threshold 0 on opaque screenshots and equalled it on all 62 cloud/Linux pairs (§7.3). D3: the committed baseline did not reproduce on the Linux PC (57 of 62 PNGs differ by fonts, no DOM differs), so A6b recaptured it there, and both captures record a machine fingerprint (platform, Chromium, the sha256 of the sorted `fc-list` lines); pixels compare only between equal fingerprints (§7.3, §7.9, risk 8). D4: generated ids are `tc-(f\|g\|dlg\|cfm\|rg)N`, tC4's five counters; `tc-[fg]N` missed `tc-dlg`, on every dialog (§7.3). D5: only same-origin asset paths are reduced to their file name, so a wrong external link still shows (§7.3). D6: two more statuses, `unported` (with the unit that captures each missing layer, from §7.6) and `failed` (nothing excuses it), and `parity/app/units.json` lists the landed units because CI checks out one commit; the gate fails a layer still missing after its unit landed (§7.5). D7: `pending_until` of an M+S screen is its Recapture unit; §7.6's notes gave it for two of the five (§7.5, §7.6). D8: a DOM-first capture compares only the subtree that `scopes.json` names, because the Home behind its dialog is not ported until its pixels unit (§7.3, §7.6). D9: the Rust `home()` is `test/reset`, `prefs/setLocale` for another language, cleared storage and a load through `about:blank`, and `settle` also asks `test/quiet` and `__tcIdle`, polling them once busy (§7.1). D10: the context aborts every request to another origin, so the token in `extraHTTPHeaders` never leaves the machine (§7.1). D11: `open-door43-internet-consent` shows the Q3 note behind its scrim, so it now cites Q3 in the baseline and the `q3-where` mask covers it: nine `rebuildDiffers` screens, eleven masked screens (§7.3, §7.6). D12 (verification): a residual names exactly the layers its row differs in, behaviour among them, so a later difference on another layer of an explained row still fails; and a Rust screenshot of another size than tC4's, a shot at the wrong viewport, is a failed capture that no residual excuses (§7.5). Masks are taken out before ids are renumbered, behaviour node numbers are renumbered per side, the masks' elements take no part in a sequence, `syncToast` is compared field by field (each with a planted test that fails when the rule is broken), and a diff image must be committed and equal what the gate draws. No baseline screen can be captured on the Rust side before B2's Home, so all 62 rows are `unported`; the planted tests prove the gate on the real baseline. Evidence: `docs/evidence/parity-harness-2026-10-18.md`.)* | A6a, A2, A4a | — | the gate fails on a planted pixel, DOM and meaning difference; a screen that reaches the settle deadline fails | the gate runs in `cargo test`, and the planted differences fail it |
| A7 tC4 view-state gallery | Vite app with the `state.jsx` stub; case format; action recorder; the §7.4 cases *(A7: built. D1: a case starts from a recorded state, the whole `s` of real tC4 read from its React tree after the driver took it against the baseline's mock to a baseline screen or a few steps further, with the replies of the read-only actions the view draws; a patch, tC4's reducer `set` patch, is written only where the mock cannot go, from recorded values and tC4's own functions (`parity/view-gallery/patch-kit.mjs`), never as free text: 23 states, 68 cases in the 18 groups of §7.4's table, 19 drawn from a recorded state with at most view-local steps and 49 patched, `cases.json` naming the keys each patch sets (§7.4, risk 17). D2: calibration draws every recorded state with no patch and compares it with real tC4's own capture at that moment: all 23 equal, pixels byte for byte and DOM after the gate's id renumbering, so the stub adds nothing; focus is the one thing a state drawn afresh lacks, so 11 states carry steps that put it back where the route left it (`refocus` by script with no keyboard ring, or `blur`), replayed by calibration and by the unpatched cases on them (§7.4). D3: the gallery is built by the tC4 clone's own Vite, React plugin, node polyfills and React through a `node_modules` link, in release mode with tC4's base and defines; its CSS is byte-identical to tC4's release build (§7.4). D4: no tc-api reply exists at A7 (`MemoryBackend` is B1b's and a screen's view types its unit's), so A7 defines the slot `parity/view-gallery/replies/<case>.json` and the porting unit fills it (§7.4). D5: a case row names its §7.4 group's DOM and pixels units, has no meaning layer, compares a DOM-first case within its `scope`, compares pixels with the gallery capture's own machine, and is checked by sha256 with stray files refused; new gate rules `CaseGroup`, `CaseScope`, `DuplicateRow` with eight planted tests (§7.5). D6: the stub answers the two actions a view calls while drawing (`sourceGateways`, `installedCountFor`) with tC4's own code; every other action is recorded per case (`actions/<case>.json`) and answered by the case's replies or not at all. D7: before mounting, the capture clicks a full-window layer at the pointer's resting point, so focus a view moves on mount is not drawn as keyboard focus, and the page flags the mount from an effect after React's first commit, so no step races a dialog's mount focus. D8: M18 is drawn only by the section editor, so its group names B4b, which builds it, and B4a's scope does not list it (B4a's and B4b's rows). D9: `capture-rust.mjs` keeps case entries and captures cases with `--case`; `measure-masks.mjs` leaves case rectangles to the gallery's capture. D10 (verification): an M9 case's confirmation of a set upgrade or re-pin is computed by tC4's own rules from the offer: the pins after it (`applyUpgrade`) and the consequences (`consequencesOfGatewayChange` over the sample's stored decision files), so a row appears exactly for a check whose resource the offer moves, and a carry-over row for each one not blocked; a test holds the drawn confirmations to that rule. Three things are taken, not drawn: each new release covers the books of the one it replaces, the machine holds the releases the checks resolve through unless the case blocks one, and the carry-over counts of each row, which need the new release's checks, are borrowed from the recorded gateway change for the same (tool, book), since the mock holds no newer release (risk 17). One value of tC4's state is not JSON, the sign-in step's identity Symbol (`state.jsx` `si.opened`), so it is carried as a tag and revived as a new Symbol, never dropped. Every case renders with no page error, console error, view error or request, two captures are byte-identical, and a fresh capture equals the committed record; `report.json` has 130 rows, all unported. Evidence: `docs/evidence/view-gallery-2026-10-18.md`.)* | A6a, A6b | — | every case renders with no console error; two runs are byte-identical | the cases are rows in `report.json` |
| A8 Door43 account and internet consent | `account` service: sign-in with `mint_token` against tc-door43-fake, keep, resume, forget; `cancelSignIn`; "no keychain"; M12; the sign-in steps of the internet question flow (A5 built the question: `InternetFlow`, `Pending::{Feedback, Link(Help, _)}`, M15 and its failure; A8 adds the `Pending` variants of the steps that need a sign-in, their re-run after it, and the `net-failed` capture) *(A8: built as the row says, with these refinements, each written where the design text names it. D1: `share-internet-consent` and `open-door43-internet-consent` move to B9b (§7.6): they are M15 asked with kind `share` and `open`, no phase-0 step asks with either, and a test-only opener in the product would break AD35; B9b builds both triggers and captures both screens whole. D2: `share-signin` is reached on the Rust side through the account menu's "Sign in to Door43", which opens the same M12; a screen's `rust` route is taken only by a DOM-only capture (§7.1), and B9b deletes it when it captures the pixels from its card. D3: the server line of M12 is masked with the `build` ruling (`signin-server`, §7.3): it names git.door43.org in tC4's capture and the loopback fake in the rebuild's. D4: the account service (§5.6 items 5 and 6, AD31): sign-ins and resumes take one lane, a second sign-in waits instead of `Busy`; Cancel names its sign-in and signs nobody in whatever its timing (a declared difference); resume uses the token read at start; `App::door43_session` and `App::door43_refused` are the seam B9b and later flows take; keychain calls run on the blocking pool. D5: "no keychain" hides "Stay signed in" and shows tC4's `signIn.notKept` (#496, §4.7); phase 0's product has no keychain until E1a, a `tc4` started with `--test-hooks` a working test keychain. D6: `test/keychain`, and `test/reset` signs out and empties the test keychain (§5.8). D7: `tc-door43-fake` runs as a binary with a control port and a `Hold` fault (R13's binary half, §9.2). D8: a failed turn-on leaves the switch off and the next step asks again (§5.6 item 2, a declared difference). D9: `--door43` is the server of account and write calls (#120, §5.6 item 9). D10: the sign-in step's state as built (§6.7): `SignInFlow::Open(SignInStep)`, `Pending::{SignIn { flow }, CheckSignIn, Profile}`, `Actions::needs_sign_in(then)`. Tests: tc-door43-fake 7 (the binary and its control port, the Hold fault, the parity account against the baseline's fake); tc-app 32 in `tests/app/account.rs` (ports of tC4's signin.test.ts and session.test.ts against the fake, the no-leak sweep, `no_keychain_hides_stay_signed_in`, the test hooks) and the updated internet, dispatch, hello, app-start and keychain tests; tc4 2 process tests (`a_development_build_signs_in_to_qa_door43`, the test keychain); tc-ui 12 host tests of the step's rules; the journeys `internet-consent.test.mjs` (a–h, 16) and `j11-sign-in.test.mjs` (J11 0, 1, 5, 6, 7, 7c, 7e, 7f), each against its own fake. DOM parity: `share-signin` declared (only the server line), `net-failed` same, `signin-error` declared. Evidence: `docs/evidence/door43-account-2026-10-19.md`.)* | A3, A5 | — | sign-in cases of J11; internet-consent a–j that need no project | DOM parity for `share-signin` (reached through the account menu's sign-in step, the same M12, until B9b's card reaches it the tC4 way) and the gallery cases `net-failed` and `signin-error` |
| A9 First-launch seeding | the join of A2 and A3 (order paragraph), if no earlier unit made it; **tc-resources**: progress and cancel hooks in bundle seeding, with that crate's own tests; the `Seed` job; `Startup` events; `/prepare.html` with English text and the other nine languages listed as pending (Q-10) *(A-17, A-32)* *(A9: built. D1: the plan is made at start, from receipts alone, inline in `App::start`, so `hello.startup` is exact from the first request (`Ready`, with no job, frame or report, when nothing is missing; `Preparing { 0, total }`; `Failed`), and without `--bundle` nothing is seeded. D2: tc-resources gets `seed_plan`, `seed(plan, &dyn SeedWatch)`, `Seeding::{Finished, Cancelled}` and `content_files_written()`; `seed_from` is the same two steps with a watch that never stops. D3: the count is content files, not tC4's 16 resources (declared difference, §3.6). D4: the `Startup` frames follow the `Seed` job's reports through `JobRegistry::watch` and only move forward, and the ring keeps the newest `Startup(Preparing)`. D5: Close is a quit, and `App::quit` cancels the seed and waits for it; a stop leaves whole copies, and the copy being made stays in `.partial/` until the next start. D6: a bundled install that fails its own check, a copy that cannot be trusted, a bundle that cannot be read and a bundle that lacks an install its manifest lists (D12) end the start `Failed`, and the window shows tC4's failure words with the reason; tC4 shows a native error box and exits, so this is a declared difference, and the screenshot goes to the owner under Q-9; tc-resources keeps its rule that a refused install does not stop `seed_from`. D7: `/prepare.html` is one file, `crates/tc-ui/app/prepare.html`, holding the `prepare.*` text of all ten languages (nine pending, Q-10) and one script that its own CSP allows by hash; `crates/tc-ui/tests/prepare_page.rs` keeps both equal to the rebuild files. D8: `tc4 serve --bundle <dir>`, and with `test-hooks` `--test-seed-pace <ms>`. D9: the installer still ships an expanded store, built from `bundle/en` by `crates/tc-app/examples/bundle_store`, which also writes the store's `BUNDLE-MANIFEST.json` (D12); seeding from the archives stays DELTA §3.6 item 3. D10: the shell (G1) shows its window on every launch within 1 s, and loads `/prepare.html` in it only when `hello.startup` is still `Preparing` 300 ms after launch (owner ruling, 2026-10-18; tC4 #743); A5's `App` sends a page that boots while it is not `Ready` to `/prepare.html` (§5.3). D11 *(added after the first verification)*: `App::start` opens the resource store with `ResourceStore::open_keeping_leftovers`, so a `.partial/` leftover that cannot be removed is a `resources` report and not a refusal to start (§4.9); `ResourceStore::open` still refuses it for other callers, except a leftover that a scanner still holds after the waits of STORAGE §3.7, which it keeps for the next open. D12 *(added after the first verification)*: a bundle says what it holds. The build of the bundle writes `BUNDLE-MANIFEST.json`, naming each install by repository and commit, and the plan compares it with the folders (§4.6): a listed install that is neither in the bundle nor on this computer, or a folder with no usable manifest, ends the start `Failed` at once and copies nothing. tC4 does the same with its `BUILD-MANIFEST.json` (`tC4:scripts/desktop-bootstrap.cjs:230-290`), so this is no declared difference; without it a lost install folder, or a `--bundle` that named the wrong folder, ended `Ready` and opened projects that could not find their helps. D13 *(added when A9 merged with windows-hazards)*: a seeded copy is an install like a download, through the same `install_until`: every file name is checked by `tc_door43::check_names` (STORAGE §3.7), a file is written with `create_new`, and the rename into place waits while a scanner holds a file, unless the same install has landed meanwhile. A bundled install with a file name Windows would rename or merge is therefore refused, left out of the store and named in `refused`, which ends the start `Failed` (D6). `seed_plan` runs the same check of paths and names on the receipt of each bundled install it will copy, and refuses it the same way before any of its files is read, so a device name in a receipt is never opened as the device and a `../` path never reads a file outside the bundle. The staging folder of an install that `create_new` refuses is removed with the same waits as the rename. An install that finds the same install landed first tries once to remove its staging folder and does not wait, since the install it lost to is in place; what a scanner holds then stays in `.partial/` until a later open removes it. `open_keeping_leftovers` removes `.partial/` with the same waits, and a leftover a scanner holds even then comes back as a `resources` report (D11). Evidence: `docs/evidence/first-launch-seeding-2026-10-18.md` (on the Linux PC: first launch 14.8 s median, repeat launch 4.1 ms, `App::quit` in a seed 4.6 ms; the Windows figure is G1's and G2's) and `crates/tc-ui/app/prepare-smoke.mjs`. A4a merged after A9 and added to `crates/tc-ui/build-app.sh` the copy of `app/prepare.html` to `OUT/prepare.html`, which A4a's smoke checks byte for byte through `tc4 serve`; A4a's key scan checks only that each key the Rust source names exists, never that each rebuild key is used, so the page's `prepare.*` keys need no entry there, and `prepare_page.rs` keeps them equal to the rebuild files.)* | A2, A3 | Q-10 for the translations | seeding count targets (PERF:133-138); cancel leaves a resumable state; tc-resources' tests stay green | Playwright: the window shows "Preparing resources: n of total" and Close cancels |
| A10 tc-export print model *(A-17)* | the **new crate `tc-export`**, pure, over tc-obs: ports of `printedItems` (`tC4:src/data/bookModel.js`), the print parts (`tC4:src/views/print/PrintBook.jsx:44-71`) and `PrintStories.jsx`, and the page setup; a typed model, no HTML yet. A book's input is its verse rows (label, drafted, shown text), the rows the editor's `VerseRow` is built from, not a second reading of the USFM: tC4 builds the editor and the PDF from one `bookModel` so that both state a verse the same way (`tC4:src/data/bookModel.js:1-4`), and the port of tC4's shown text is tc-usfm's `shown_text_v1`. tc-usfm joins with D2's USFM producers. Declared difference: the "Incomplete draft" notice shows whenever a verse prints as not yet drafted (`has_undrafted`); tC4 asks only for an undrafted verse (`tC4:src/views/CommunityChecking.jsx:182`), so a verse drafted with a footnote alone prints "[ verse not yet drafted ]" with no notice to explain it | — | — | vectors recorded by running tC4's functions in node on the Titus, Jonah and OBS samples, tC4's PDF test cases and generated books and story sets, compared as the markup tC4's print components render, the preview's split pages included; a proptest that every drafted verse appears once, and one for frames | the vectors are equal |
| A11 Streaming downloads in tc-door43 *(A-17)* | **tc-door43**: `download` streams to a file with byte progress and a cancel check per chunk, and every 100 ms while it waits, the commit lookup that comes before an archive included (it returned one buffer, `crates/tc-door43/src/client.rs:520-526` at `21fef4a`); tc-resources installs use it. Built: `DownloadTo`, `DownloadWatch`, `DownloadedZip`, `Door43Error::{Cancelled, Disk}`, `install_dcs_watched`, the fake's `HoldBody`, `BreakOff` and `NoContentLength` | — | — | against tc-door43-fake: progress counts, cancel leaves no partial install, the internet switch stops the next chunk | the tests pass; tc-resources' install tests stay green |
| G0 Electronite spike, run on Windows *(A-3, A-14, A-17)* | a throwaway Electronite v37 main script and page in `spikes/electronite/`, never shipped: start a stand-in for `tc4 serve` (`tc4-standin`: axum on loopback, the token, the §5.2 checks, the address line, a quit on a closed stdin), because A2 is built at the same time; the header filter of AD11 and a probe of every request type (fetch, image, iframe, download, print window, web font, CSS `url()`, the event stream, a same-origin child window); a `protocol.handle` prototype for Q-1; whether the page's `/api/` requests wait for a busy main thread; the aligner round trip with a 20 KB reply, up to the painted frame, alone and while OBS pictures and a help page load, in two passes of alternating order; supervision (end `tc4`: restart and reload; end the shell: `tc4` exits, with `tc4` started plainly and detached); what a hard kill leaves of `localStorage` and of the mirror file; the Chromium version; a PowerShell driver that writes the results; the task `.audit/tasks/g0-windows.md`, which also runs the storage benchmark on the same laptop | — (the stand-in; A2 measures `tc4 serve`'s own loopback cost) | Q-26: a person or a Windows runner runs it (PLAN:151-152) | the stand-in's tests; the page's probes in Chromium through Playwright, which fail when nothing adds the token; the whole driver run here with Electronite's Linux build; the probe table; the round trip p95 on the reference laptop or a named stand-in; the supervision cases | `docs/evidence/electronite-spike-<date>.md` holds the Windows results; Q-1 and, on a miss, Q-13 are put to the owner with them |

### Phase 1: after unit C merges (MemoryBackend)

B1a starts now, on a branch cut from `storage`, because it needs tc-journal *(A-16)*. *(B1a: it was
cut from the integration branch `app` after unit C had merged, like every other unit.)*

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| B1a Seed spike *(A-16)* | `crates/tc-app/src/backend/seed.rs` only: `tc4_state_seed` and §13.2's check on the `.state` scenarios (`sample`, `en-pins`, `shared`; `build-state.mjs` run on the tC4 checkout): `ProjectMeta`, `Versification`, books through the codec, pins with the schema 1 → 2 migration, settings, decisions, alignments, OBS story records *(B1a: built, with the check in `seed/check.rs` and the strict JSON reader and timestamps in `seed/strict.rs`, behind the feature `fixture-backend`. D1: `Tc4Folder::read` is the one reader; `tc4_state_seed(&Tc4Folder, ProjectId)` builds, seals, folds and checks, and returns the seed only when the check passes, else `SeedRefusal::Mismatches` with every mismatch. D2: every change by `Importer("tc4-seed")`, `cause: Import(Burrito { file_name: <the folder's name> })`, no parents, `seq` its index in its batch, `at` a decision's own `modifiedTimestamp`, else `meta.dateCreated`, else tC4's floor 2020-01-01. D3: one batch for the project, then one per book in canonical order or per story, records in key order, so the seed does not depend on the order of records in a file; batch ids from sha256("tc4-seed-batch\0" ‖ projectId ‖ label). D4: sealed with `pack::seal` under a fixed Local sealer, read back with `read_pack`, folded; never `encode_pending`, which refuses a non-Local device (§4.2). D5: a story's `StoryBase` is its file and nothing else, since `decode_story`'s records cannot give the files back through the fold (§4.2); an OBS project's licence and front and back matter are `CarriedFile` records by git blob id. D6: each tC4 decision gets `importedInvalid` from `invalidated`, `userStatus` from `status` (on an invalidated one only To do, or an Invalid that `userInvalid` marks as the person's own), and `verseTextMd5` from the text its target names in a fold of the project's texts, the text the projection compares. D7: the check reads as tC4 reads (§4.2); an absent `selections` is kept apart from `false`, since tC4's `isDecided` counts it as a selection. D8: a `schemaVersion: 1` `resources.json` migrates as BURRITO-SPEC §5.3 says, adding the bundled `en_twl` pin with its flavor when the file has no links pin. D9: JSON is read with a reader that refuses a repeated key, which serde_json would drop without a word. D10: `fixture_project_id` is the fixture backend's id rule, not an import rule. D11: the cargo tests assemble each scenario's projects from `fixtures/tc4` and the overlay `fixtures/parity-state` and check every file against `parity/app/state.sha256.json`, so they run on the exact bytes without node. D12: the mismatches the storage team owns are asserted exactly, so the test fails the day they are fixed. Outcome, the same in all three scenarios: `sample_burrito` passes (127 changes, three batches); `sample_burrito_obs` is refused with 53 mismatches, handed to the storage team in `docs/evidence/state-seed-check-2026-10-18.md`: frame 0 of every story reads with its number (STORAGE §7.7 against tC4's `TITLE_RE`), `checking/settings.json` is not among an OBS fold's derived files, and the OBS text pin's relationship makes `metadata.json` invalid for both language sets; two OBS edits outside the check (a title edit loses the number; an empty frame's edit takes another layout) are written up there too. B1b cannot load the OBS project of any scenario until the storage team fixes these. Declared differences from tC4 4.0: a `schemaVersion: 1` `resources.json` is migrated where tC4 refuses the project; `verseEdits` is recomputed; a decision with no `modifiedTimestamp` is written with `meta.dateCreated` (else 2020-01-01), where tC4 reads it as changed at each moment it reads it; the check compares tC4's reading of each record where tC4 compares whole documents; `metadata.json`'s free fields are seeded and checked where tC4 leaves them to its server. A seed takes 5-6 ms per project in a release build after the first, which compiles the Burrito schema once, about 20 ms.)* | `storage` (tc-journal); rebased when C merges | C, if a mismatch is a spec question | the seed check on every scenario: USFM and `vrs.json` byte for byte, sidecars field for field | the check passes on every scenario, or each mismatch is written up with the record, the bytes and the rule involved and handed to unit C *(B1a: unit C had merged, so to the storage team)* |
| B1b tc-app core and MemoryBackend | the seam with `EditToken` and `ReviewToken` *(A-7)*; `MemoryBackend` with scripted outcomes over B1a's seed; scenario loader with resources from `.state`; `SessionRegistry` with one open per folder and the close at Home *(A-8, A-25)*, epochs; the reply cache wired to sessions; `wire` for tokens, scopes and errors (each later unit adds its views); the key probes of §4.3 *(A-21)*; `AppCounters`, `CountingSnapshot`; tc-corpus loading; contract read cases *(B1b: built. R1 and R19 are provided by tc-journal, so the key reads are R1's range reads and the parse checks the kind under the MAC; no fallback is used. Tokens are minted from the `Snapshot` a screen read, not the handle's newest one, and a `StoreCommand` takes the right kind by type. A project the seed check refuses no longer fails its scenario: it is listed with every mismatch and left out, so the OBS sample is refused in every scenario until the storage team's fix, and the Rust Home shows only the Bible card until then. Teammates are not loaded with a scenario (B10 adds them). `Write::at()` lets dispatch check a write's scope before it picks the session's reply cache. The `Opened` outcome and the open job are B3's. A clippy lint keeps whole-fold walks out of tc-app's code. `tc4 serve --scenario` starts the app on a scenario in fixture builds. Measurements: `docs/evidence/memory-backend-2026-10-19.md`.)* | C, B1a, A1, A3 | — | contract read cases, including the token-kind cases; epoch refusal tests; `opens_of_one_folder_share_one_session` in both orders; `probe_finds_every_key_a_scan_finds`; `open_lists_no_key` | the count-test harness runs on Psalms and ten times Psalms |
| B2 Home and project dialogs | `home/*`; `SummaryCache` over the persisted summaries: it reads `PrefsWriter::summary`, compares the summary's stamp with `StoreBackend::summary_stamp` and keeps a new one with `PrefsWriter::record_summary` (Home's half of C1a's D7, built by whichever of B2 and C1a lands second); cards, tiles, "—" rules; Resume card; toasts; the unusable folders of the listing, with words for them (§4.5, a declared difference from tC4); M1, M2, M3 (server language search, versifications); M4 method step; M5 kind step *(B2: built as the row says, with these refinements, each written where the design text names it. D1: two reads, `home/list` (the whole Home, held by Home under `AppTopic::Projects`) and `home/card` (one card, held by each card under `AppTopic::HomeCard`), so a card reads again when its summary is ready and at each new Home visit, never at a redraw (tC4's `homeProjectCard` case 4, §4.5). D2: a card's frame comes from `ProjectStore::peek` at every read and its counts from a summary that one `HomeSummary` job at a time computes from `summary_files` with the same functions for both backends; the memory backend renders the files through `CountingSnapshot`, so the parity Home goes the product's way (§4.2, §4.5). D3: one drafted rule, `tc_app::drafted` (#572), on the stored verse parts; a poem verse whose words are on `\q` lines is drafted here and not in tC4 (declared, `residuals.json` `poem-verse-drafted`). D4: `home/nameFree` moved to C6, whose create decides the folder a name makes (R2). D5: `RepoPlace.server` names the QA server in a development build (#506). D6: the Share button asks the internet question first (`Pending::Share`, or `Pending::Sync` for a shared project, PLAN Q2), whose steps B9b and B10 write; this makes the `net-failed` case reachable, and B2's route to it (the Share question, with `net/set` answering that the switch stayed off) replaced A8's sign-in route when the two merged: tC4's case draws nothing behind the failure dialog, and B2 captures the case whole, pixels with the DOM. D7: C1a landed first, so B2 built Home's half of C1a's D7: `SummaryStamp` is C1a's (`StoreBackend::summary_stamp`), the cache reads a kept summary on a miss when its stamp is the card's and keeps each new one with `record_summary`, and `home_costs_projects_not_history` holds the Home row on the store (§4.5, §8.2). D8: the memory backend's stamp is its fold's version, raised by every change. D9: the unusable folders carry a typed reason with words (`home.unusable.*`, pending in nine languages), a rebuild-only card (Q-9), and no "No projects yet" over them (§4.5; `residuals.json` `unusable-folders`). D10: B2 captures `license`, `net-failed`, `home-empty`, `home-list-error` and `home-resume`, each through a Rust route in `parity/view-gallery/cases.mjs` with hand replies; `home-loading` (a read in flight), `home-open-error` (the open) and `home-checkpoint-error` (the leave checkpoint) moved to the groups `home-held` and `home-open` (B3) and `home-checkpoint` (D1) (§7.4). D11: the seed check still refuses the OBS sample, so the Rust Home lists one card; every B2 row with tC4's OBS card differs by it and is explained in `residuals.json`, and without it the DOM, computed styles included, is tC4's line for line (§4.2 B1b note; evidence). D12: `test/lastEdit` records the Resume record as a save will (§5.8). D13: `home()` waits for the cards `home/list` names (§7.1). D14: the stand-in's test ids moved: Home is drawn when `add-project` is, the language switch of the smoke and the environment's tests goes through the App language dialog's preview, the internet state is the account button's `data-internet`, and `window.__tcHeld` holds the counts (§6.2). Also: the language search (AD27, #492) runs on the server from the list tC4 ships, and a Door43 code a Burrito cannot hold is offered adjusted with one sentence (Q35; `residuals.json` `language-code-offer`); Home's `<svg>` icons are built with Leptos's SVG builders, because `view!` wrapped each path in a `<g>`; `BookTile`'s name and figure are signals, so a preview changes them in place (the DS gallery records stay green); the planted gate tests start from phase 0's units with no row explained, so a later unit's landing does not break them. Not in B2, each with its action named and empty: opening a project from a tile or the Resume card (B3), Settings (B9a), the share and sync after the question (B9b, B10), the conflict review (B11), Create and the later steps of Add a book (C6), Import after its kind step (F1), Open from Door43 (B9b); A5's note that B2 adds Tab to the account menu's keyboard sequence is left to the next unit that records that sequence on both sides. Tests: tc-app 27 Home tests (`tests/app/home.rs`), 5 drafted, 3 language search, 3 count rows (a second Home visit renders nothing and Home walks no history, on both backends; `home_costs_projects_not_history` on the store: with kept summaries ten cards open no project, read no pack or pending save, open at most five files each, cost the same beside Psalms and ten times Psalms, and a card whose kept summary is gone starts one job that the next launch reads); tc-api goldens for the four endpoints and the hook; tc-ui 18 host tests (the `homeProjectCard` and `homeObsCard` ports, the forms); journeys `home` (7), `language-suggestions` (3) and app-language's Home halves of a, g and h; the smoke (95 checks) and the environment's tests moved off the stand-in. Parity: 11 screens and 5 cases captured on the store (C1a), `home-empty` the same, every other row explained. Evidence: `docs/evidence/home-2026-10-19.md`.)* | B1b, A5, A6b | — | `homeProjectCard` port; language-suggestions (UI part); `home_costs_projects_not_history` on the store (§8.2), written by whichever of B2 and C1a lands second; `home_keeps_its_summaries_once_b2_and_c1a_have_landed` fails until it is (C1a, D7) *(A-25)* | 11 screens in the report (§7.6) |
| B3 Open and Understand | open job and the S14 overlay with its own 300 ms timer and tC4's three stages *(A-22)*; the bodies of B2's `Actions::open_project_at` and `resume`, `s.book_error` for a refused open, and `App::cancel_home_summary` at every open (§4.5); the capture of work in flight, for S14 and the gallery's `home-held` (`home-loading`), and the `home-open` case (`home-open-error`) *(B2 D10)*; `editor/book`, `chapters`, `sources`; `helps/*`; `model::take_spans` and the tc-helps block renderer (port of `renderArticleBlocks`); highlights; book rail; helps panel with tabs, slots, article pane, resize; comment box (read) with tC4's selection rule *(A-18)* *(B3 D1: Q-7 follows §12's recommendation: reading and folding the changes show as tC4's "Reading the journal", finishing and checking as its state stage, writing files as "Preparing"; the reading count names files, `open.changeFiles`, because the store counts change files, not tC4's saved actions. D2 (#118, #121): `editor/sources` and `helps/passage` answer a project chapter. In a project of another versification (New Bible offers six) the reply's `frame` is `Mapped`: each pane verse is the source verse the project's numbering names, under the project verse's key, and each help is placed in the project's numbering by the server (tC4's `mappedSourceReferences` and `deriveForProject`), so the focus, highlight and comment rules have no second path; a verse in another book or in no place is stated; a project whose scheme cannot be established shows `understand.frameUnavailable`. A chapter introduction stays at its chapter, where tC4 drops it. D3: the comment box is read-only until C3 saves it, a residual until C3. D4 (#628): articles and notes draw tables, numbered items and nested lists. D5: `home-open-error` is captured with a refusal the store gives, a project file it cannot read, because the rebuild opens a project whose file was written outside it (STORAGE §11.1).)* | B2 | Q-7 for the stage captions | J15 UI with a held open: the overlay shows at 300 ms with no event; `open_book_touches_one_book`; `helps_load_one_book`; `take_spans` differential test | 4 Understand screens |
| B4a Translate: grid and verse editor *(A-17)* | grid, sections (display), source and original chips, Section/Verse switch, conflict markers (read); the verse editor on scripted saves with the refresh rule of AD37 *(A-6)*; "changed while typing"; `Editing` lock with the rail number button (DELTA:519-523) *(A7: B4a does not build M18, "Remove all the text of section …?": only the section editor draws it (`tC4:src/views/SectionEditor.jsx:244-255`, tC4 #600), so B4b builds it as the #600 question and its gallery case names B4b.)* *(B4a: built as the row says, with these decisions. D1 (AD16): `editor/chapters { at, book }` answers every chapter of the open book in one read: each verse's key, display text, paragraph marks by the panes' rule (`lead`, `before`), record, drafted flag (#572's rule), and the verse's whole `words` as a `Shown` value with its token and conflict marker (#786, Q-20); each chapter's own `\ts\*` sections; and the project's open verse conflicts. A chapter switch reads nothing of the translation; the panes and helps stay B3's per chapter. D2: `SourcePane` names its pinned version (the caption "· v91 · pinned") and `editor/sources` carries the original-language pane (`original`, id `orig`, right to left for the Hebrew Bible) for the Greek or Hebrew chip. D3: the sections are the source pane's `\ts\*`, else the translation's own, else the whole chapter (tC4's `chapterSections`; a source pane that marks one section counts as unmarked); in another numbering one per verse. D4: the verse card is a save-machine field of kind `Verse` (6.5): it opens on the pair of the read its row came from (AD37), saves on blur, on Save or 2 s after the last keystroke, and Cancel puts back the text it opened with and saves it as the card closes, as tC4's `cancelVerse` does. A refresh of the chapter's read goes to `Saves::refresh` with the author the `Changed` frame names for the verse's record (`VerseRow::record`); the card shows `translate.changedWhileTyping` ("Juan changed this verse while you were typing. Your text will be kept as a second version.", STORAGE §9.9) or its unnamed form, rebuild keys pending translation. The screen follows only whether its reads are in, and each section's card only which verse is open, so a refreshed read never closes the card. D5: until C2's `editor/saveVerse`, a test build registers a verse writer over `test/saveVerse`, the real save through the store (C0), so the journeys run on real outcomes; any other build has no verse writer and a save is `Unavailable(Writes)`: "Not saved" with Retry, the text kept in the card and the buffer. No verse draft is sent before C2's `editor/draft`. D6: found by the first real save: a verse save's `Ack` carries two tokens, the verse's and its reflowed alignment's (R25, Q-6), and A4b's machine refused every `Ack` without exactly one, so every verse save ended in "Save failed". The field now takes the token of its saved value's record (`an_ack_with_the_reflowed_alignments_token_saves_the_field_on_its_own_token`). D7: the stub `___` of an undrafted verse shows as an empty box (tC4 shows it so), and an emptied box sends empty words; whether the store writes `___` for them is C2's (left open). D8: the editing lock. An off control takes no pointer event (`[data-i][disabled]` has `pointer-events: none` in tC4's states), so a press on an off chapter number or book tile lands on what is under it and, in tC4, takes the focus out of the card, which closes it. The screen stops a press that falls on an off button on its way down, so the card stays and keeps the focus; a press anywhere else still leaves the card and saves it, as in tC4. A declared difference. The handler B3 put on each number button could never run on a disabled button and is gone. D9 (#131): in another numbering Translate's source column shows, for each project verse, the source verse its numbering names, under that verse's place and number, with tC4's statements for a verse in another book, in no place, or an unknown numbering; tC4's Translate shows the verse with the same number. A declared difference. D10: the helps panel has Translate's Comments tab (#252), each verse's newest comment in focus, since comments are append-only; the panel follows the open card's verse (tC4's `editingFocus`). D11: "Draft section" opens B4b's section card and the conflict markers B11's review; until those land they show, and are off while a card is open. Parity: `translate-tit1`, `en-translate-tit1` and `en-translate-verse-editor` are tC4's pixel for pixel, and the verse editor's DOM line for line; the two Titus 1 screens differ only by the titles of note cards below the fold that FIX-helps took from tC4's later rendering (C39), residuals until the reference moves. Journeys (`parity/app/journeys/translate.test.mjs`, 10): the grid by section and by verse, `chapter_switch_sends_no_request`, a saved verse with the lock, a held save and Cancel, a build with no verse save, `refresh_under_open_clean_editor_swaps_value_and_token_together`, `refresh_under_typed_editor_keeps_text_and_token_through_an_arabic_ime` (its save becomes a second version with its marker), `cross_frame_translate_shows_the_mapped_source_verse`, and app-language e (Translate's half; the comment's is C3's) and i (the left-to-right project's half; a right-to-left project needs C6's create). Left open: the project's script font (Arabic, Nastaliq) is not in the open's reply, so the translation reads in the scripture face; C2 decides what an emptied box stores.)* | B3, A4b | — | save states for every scripted outcome; `chapter_switch_sends_no_request`; the §6.3 refresh tests, with the Arabic IME case in Playwright | `translate-tit1`, `en-translate-tit1`, `en-translate-verse-editor` |
| B4b ◆ Section editor *(A-1, A-17)* | `model::SectionBuffer` (port of `sectionDraft.js`); the section card with Type and Place, pins and the "To place" bank; Save only, Cancel, the #600 question (M18); section drafts (the envelope of §6.6 until R6); the editing lock for sections *(B4b: built as the row says, with these decisions. D1: the section buffer is `tc_api::model::section` (`SectionBuffer` with `parse`, `serialize`, `groups`, `verses`, `formats`, `can_drop`, `drop_pin`, `span_end`; `initial_text`, `indent_line`), held to tC4's own `sectionDraft.js` by vectors a generator records from it (`fixtures/usfm/section-draft-vectors.json`: 29 hand-written and 700 seeded random texts, 160 key lists, 249 indents) and by the ports of tC4's thirty section tests. D2: the card's one value is its Type text; Place reads it once and holds that reading for the session, as tC4 does, and each drop writes the session out as the text, so the browser buffer, the shell's mirror and a draft hold the placements in one string and the save reads them as the session's verses and formats; Place back to Type normalizes the text as tC4's `serializeDraft` does. (Fixed after the review: the card read its own text again after each drop, so a number moved twice left a line break inside the verse and a paragraph before it.) D3: `editor/chapters` gives each verse row `format` (the `\p`, `\q1` or `\q2` line before it, tC4's `gapMarkerOf` over the previous verse's layout) and `covered` (the tokens of the verses a span covers); the card's base is the token of every verse it displays (STORAGE §5.2), so a teammate's change under an open card becomes a second version at Save, never a revert; an `Ack` gives each of them its new token and a refresh taken whole gives them all (fixed after the review: both cut the base to the first verse, and the next save read the whole text as that verse). D4: the server reads the section's verse keys from the displayed values, not from the card, and `tc_app::section::section_values` turns the text into one `Text` per verse or span the text makes (the verse's own marker, anchor and layout, a typed format replacing the line before the next verse as tC4's `spliceVerseGap` does, the stub for no words, `Covered` for the rest of a span); a verse the text leaves as it was is not written, so a save that changes nothing writes nothing (#786). D5: until C2's `editor/saveSection`, a test build saves through `test/saveSection`, the same request through the store; any other build answers a Save with "Not saved" and Retry and keeps the card. D6: R6 is provided by S1, so the envelope of §6.6 is not built; between saves the card lives in the buffer and the mirror (A4b), and its `Draft` is sent by C2's `editor/draft`, as the verse card's is. D7: the card is read-only while its save runs, Tab and a drop in Place included (fixed after the review: both changed the text under a running save), and closes when the save has stored the text; a failure or a hold keeps it open with its text. D8: found by the M18 capture: a press on the section card's Save or Cancel takes the focus as any button's does (tC4's section card has no `onMouseDown` handler), where the verse card's keep it in the field (`draft_chrome::FooterPress`). D9: found by the `en-translate-section-editor` capture: the card types one step down, `--fs-verse-md` at the reading leading (tC4's `editType`). Declared differences: a format typed on the line of a verse with no words stays the line before that verse, and a verse with no words that the text gives no line keeps the line before it, where tC4's `groupFormats` gives that line to the next verse with words and so moved or removed the poetry and paragraph lines a seeded book holds before its undrafted verses (`a_verse_left_undrafted_keeps_the_line_before_it`); explicitly typed number-only lines and their formats also survive Place mode and pin moves (`a_place_session_keeps_explicit_formats_on_verses_without_words`), while omitted lines keep their stored formats; a typed span split by a later empty interior pin keeps every other pin through Place and back to Type: the covering span is written before its empty override, where tC4 drops the earlier placed pins (`an_empty_pin_inside_a_typed_span_preserves_the_other_stacked_pins`), preserving verse boundaries; when a section's first verse has no words, the text a Place session writes keeps the format of its first line, which tC4's `serializeDraft` drops; and while a section card holds text it has not saved, the save indicator reads "Unsaved changes"; tC4's mirrors only its verse scheduler and reads "Saved" (docs/PLAN.md, the residual of `section-empty-confirm`). Parity: `en-translate-section-editor` is tC4's pixel for pixel and line for line; the M18 case `section-empty-confirm` differs only by that indicator. Tests: tc-api `tests/section.rs` (47: the vectors, the thirty ports, four properties, a number moved twice in one Place session, the first line a session writes and the line before a verse with no words); tc-app `tests/app/section.rs` (20: the values with no store, the save on the store, Psalm 3 and Genesis 3:14-17 byte for byte, and the differential property `a_saved_section_opens_again_as_the_card_showed_it`: what tc-app writes, opened again, reads as the card's verses, bodies and lines) and the count row `section_save_touches_one_section` on Psalms and ten times Psalms; tc-ui host tests of the card's decisions and its base; 18 journeys on real saves (`parity/app/journeys/section.test.mjs`), J2's section steps among them: `section_editor_saves_only_on_save`, Cancel, #600, #598 (and a number moved twice), #604, #575, the formats, the exits (tC4's `sectionDraftExits`), a reload, a reload during a held save, Tab and a drop under a held save, a teammate's change, a build without a save and tC4's `ds-labels` d (#614). Left open: the server reads the section's verse keys from the displayed tokens alone; the request does not carry the card's keys, since a save restored after a reload has only its field's place, which names the first and last verse but not the spans, and a section can show fewer rows than its keys.)* | B4a | R6 (fallback: the envelope) | `sectionDraftExits` port; section buffer proptest and its tc-app differential test; `section_editor_saves_only_on_save` (typing, placing pins and blurring send no `editor/saveSection`); Cancel leaves no buffer entry and no draft; the #600 question | `en-translate-section-editor`; J2's section steps on scripted saves |
| B5a Check: picker and rail | preflight; picker progress; `CheckListCache` with per-row `ViewSeq`; `check/open { since }` and `check/rows { since }` *(A-12)*; `model::rail` (mask, one-pass groups *(A-13)*, patch); the virtual list with two heights and a mounted tab-stop row *(A-24)*; M11 view *(B5a: built as the row says, with these refinements. D1: the preflight is tc-check's `fix::preflight` over the pins as the journal's D30 ladder, the one the fold resolves decisions with, so the cards and the guided fix's offer (`fix::guided_fix`) read one answer; it is asked with tC4's `online = true` (D88), so a missing pin is always Fetch. D2: `check/picker` answers each tool's preflight with its progress, counted from the same session the tool opens (it fills the `CheckListCache`, so a tool opened from the picker is a cached open), and the Align card, counted with `tc_align::verse_status` over the drafted verses (#572's rule: `___` is not drafted) until B6's `align/index`. D3: the lists derive from the book's Door43 helps whatever the reading helps setting (Q33); B3's book cache keeps a Door43 reading beside an Aquifer one. D4: the `CheckListCache` is the session's, keyed by (list, resolved resource, project scheme, book scope); a snapshot that moved recomputes the states (`tc_check::session::open` over the fold's decisions, derive nothing) and stamps each changed row with that snapshot's view; re-assessing only a verse's checks is C4's. `check/open { since }` sends `Since` only for the same session and a view no older than the derivation, else `Full`; `check/rows { since }` answers `Reopen` when the list was derived again. D5: `RowFlags` gains `invalidated` (tC4's `data-invalid` and red dot) and `invalid` (the Invalid chip: tC4's `status === 'invalid'`); `RailRow` gains `check_id` (`data-check-id`); `terms` holds the group ids of both tools (each row's tooltip names its own); the session's progress is `RailProgress { decided, total }`. D6: titles are the literal pane's gateway words read at the place the resource gives the check (its own numbering), else the original quote, so a project in another numbering titles from the right verse. D7: M11 is `resources/fixState`, answered only for a card whose pin is missing; the view draws tC4's three ways and C8 runs them. D8: no `check/close`: closing a tool is the screen's, as tC4's `closeCheckTool`, and the cache goes with the project session. D9: tC4 #764, declared: the helps tools' bars fill at least 6 px once one check is resolved and name their count; #518 and #513: `session/open`'s kept place carries the Check tool and an open that lands in Check opens it at once; #769 (By figure, folds) waits for the reference move, and tc-check's reference sort is shared by both groupings. D10: the virtual rail keys its lines, keeps the selection out of the rows' drawing so the focused row is never drawn again, gives the focus back after a keyed move, and clamps its window when a filter shortens the list; check-keyboard runs on a rail of Psalms' size (8,212 rows) served in place of the sample's. D11: the `check-empty` cases are reached through `check/open`'s own reply edited in the route; `check-rail-empty` keeps a residual until B5b draws the detail beside the rail.)* | B3, C | — | `check_open_cached_derives_nothing`; `check_open_cached_sends_changed_rows_only`; the rail differential test against `tc_check::rail::groups`; `rail_groups_visit_each_row_once`; check-keyboard on the Psalms rail; rail perf spec | `check-picker`, `en-check-picker`, `guided-fix` |
| B5b Check: item detail | `check/item`; compare card; target words with drag-select (`model::drag_range`); marks; the check comment saved on Done and on leaving the item, on scripted outcomes *(A-10)*; Academy drawer; notices; M8 entry; the guided fix's three ways wired to C8's endpoints (download with its progress, a file install, the re-pin that opens M9b) and M9b captured *(merge of C8, 2026-10-19: B5a drew them unwired and C8 has no modal for them)* *(B5b: built as the row says, with these refinements, each written where the design text names it. D1: `check/item` answers from the session's list entry, which now keeps the fold's decisions of its tool and book, the one shown on each check and each check's state as of the snapshot, and from what one check touches: the verse run its words come from (the chapter's keys and one record, by the store's own no-text rule, never the book's text), the loaded book's panes and its article; reading writes nothing, and costs the verse on Psalms and on a corpus ten times larger (`check_item_costs_its_verse_not_the_book`). D2: the view is as §3.7 shows it. The draft words are `tc_check::selection::target_words`, the words the aligner and the export use (FIX-text's follow-up), and `selected` is where the stored selections match them; the gateway verse's tokens carry tC4's spaces, and the highlight is tc-helps'; the note is the TSV cell with its escaped line breaks read as breaks, as tC4's `ExpandableNote` reads them; the decision's token is the one of the stored key of the decision shown, so a decision shown on another check is edited where it is stored, never copied or left behind. `notices` holds tC4's `invalidated-notice`, counting the decisions whose words the draft no longer carries; tC4's `resolution-warning` (a decision file naming a resource in neither rung) is not built: a decision made against another resource shows red and Invalid and is counted by the rail, and the carry is C8's. D3: `check/article` reads from the resource the list derives from (Q33), where `helps/article` follows the helps setting; `ArticleView::Found` gains `path` (tC4's `ipath`, the drawer's last line), which a reply without it reads as none. D4: `check/decide { list, place, check_id, decision, verse, edit }` is one `SaveDecision` through the seam (`StoreCommand::save_decision`): scripted on the memory backend, the real store otherwise, with the check's row and the comment as stored in the `Ack`. The check is named by its place and its row's id, so a list derived again between the read and the mark cannot send it to another check (`LookAgain` when the check is gone). Mark valid turns the tapped indices into stored selections only while the verse still has the version the person saw (`LookAgain` otherwise, and nothing is recorded on other words). The page sends one mark at a time, each with the tokens the reply before it brought (the comment's writer tells the detail too), and a mark that fails shows tC4's `save-error` box in the rebuild's words (`check.markNotSaved`, `check.markLookAgain`, translations pending) and keeps what was chosen. D5: the check comment is a save-machine field of kind `CheckComment`: its place names the check (`tn|TIT 1:1|abcd`), so its writer rebuilds the request from the buffer entry; it saves on Done and on leaving the check while its editor is open (A-10, the declared difference Q-25 in `residuals.json`: tC4 drops a comment being typed when the checker moves on), and the typed text is read from the box, never from the machine's shown text; its server draft waits for C4. A mark and a comment are two writes of one decision, so they go one at a time, each over the newest token the page knows of its check (`DecisionWrites`: a read or an Ack brought it), and an Ack changes the check shown only if the Ack is that check's; otherwise the person's own mark and comment would be a conflict with themselves, or a late reply would put one check's token and comment on the next. D6: the compare card is tC4 9ad4d5c's (C40, #762); the 12 session screens are captured and each differs from the d310bf3 baseline in pixels and DOM for four named reasons (C40, #764, C39's titles, the Invalid chip), and every other element of the detail has tC4's DOM and styles. D7: tC4's session has no entry to the source texts (the Get source texts buttons are the picker's, B5a), so B5b adds none. D8: journeys `check-detail.test.mjs`: item navigation (both tools, both languages: the painted row, its geometry, Tab order, the ends of the list, wrapping at 620 px), the keyboard through the detail (drawer, words, marks, comment), the drag, the comment saved on leaving, a comment and a mark in either order with a slow disk, a late reply after Next, #532 and the compare card's columns. D9: the guided fix's three ways (the C8 merge's part of the row). The screen's state is `tc_ui::app::fix` (host tests `tests/app/fix.rs`): one way at a time, as tC4's `fix.busy` (a download and a file install never race to install one pin), each completion bound to the screen it started from (tC4's `fix.id`), and an install list that could not be read said when the screen opens (tC4's `resolutionError`). "Download it" is an internet task (`net.ask.fix`), shows `upgrade.installing` and `sources.progress` while `resources/fixFetch` runs, and on success reads the picker and the panes again and closes the screen (tC4 `finishFix`); a chosen or dropped file is uploaded (`guidedFix` flow) and sent to `fixSideload`, with `fix.sideload.reading` meanwhile; an installed version runs the navigation gate, then `fixRepin`, and the plan opens the confirmation titled Re-pin (M9b, C8's `UpgradeSet` and its `applyPlan`), which replaces the screen. A failure states `fix.failed` (an install) or `upgrade.failed` (a re-pin) with the server's reason, said once, and leaves every way at hand. The options are drawn once per screen and what a way does changes inside them, so the pressed control keeps the focus; `DropZone` gains tC4's `disabled` and a title that can change. `fixRepin` takes a plan scripted through `test/script`, as the endpoint's doc already said (test hooks only, AD35). M9b's `upgrade-repin` is captured: tC4's pixels exactly, and the DOM differs only in #764's count for screen readers on the picker's bar behind the scrim (`residuals.json`); the sample holds no other copy of es-419 TWL, so its route adds one installed version to the server's `fixState` reply, as tC4's case writes the release name. Journeys `guided-fix.test.mjs`: the download asks first and Not now sends nothing to Door43, and a download Door43 cannot give installs nothing and says why; a file of another commit is refused, and the pinned commit's file installs, closes the screen and moves the card on to what the tool needs next (es-419 tW); an installed version that the server cannot find says so, and with a plan opens the Re-pin confirmation, whose Keep moves nothing.)* | B5a | — | check-keyboard and item-navigation (navigation parts); `drag_range` differential test; `check_comment_saved_on_leaving_the_item` | 12 session screens |
| B6 Align: read and session | `align/index`, `open`, `edit`, `keepReflow`, `flush` over a server `Aligner`; the click queue *(A-14)*; the server's save timer on scripted outcomes, `AlignSave` in the indicator and the close guard *(A-4)*, read again from the view's `save` with each refresh, so a `Resync` cannot leave "Saving" standing (§5.4); the pure view builder *(A-14)*; bank, cards, lenses, occurrence numbers, multi-select, drag; marking row; training job and `Suggest` events, with the status read again after a `Resync` (§5.4) *(B6: built as the row says, with these refinements, each with its reason. D1: `align/open` answers `Opened::Ready` or `Opened::Blocked { why, at }`, where §3.7's view held `blocked`: a verse with nothing to align has no aligner, and `at` names the verse that was asked for or resumed, so the rail shows it open, as tC4's `alignVerse` does. D2: every edit is one `AlignEdit` of `align/edit` (Mark valid, accept, accept all and reject all too), one request per click with its `seq`, so §3.9's `markValid`, `accept`, `acceptAll` and `rejectAll` are not endpoints; a bank chip is named by its word, not its place, so a click queued behind another means the chip the person saw; A1's `BankSelection::Selected` is gone, since tc-align's selection is armed or nothing. D3: the aligner is a task (`tc_app::align::actor`) that owns the `Aligner`, its timer and its save, so an edit, the timer, a flush and the store's answer never run at once; the view is `align::view::build`, a pure function of the `Aligner` and the verse's facts. D4: the save is a typed state machine (`align::save::next`: Clean, Carried, Dirty, Saving, Failed) with a model-based test over edits, ticks, flushes, outcomes and crashes (a change is never forgotten, a failed save is tried again, a change made during a save is saved after it, "Saved" means the store holds the newest links, a restarted timer's tick starts nothing) and each bug put back to show the test catches it. A lossless carry-over (AD19) is saved by a flush or on leaving, never by the timer, and shows as Saved. A save that fails keeps the aligner open and unsaved: another verse is refused with `NotSaved`, Back and the navigation gate stay, and the leave and the quit save before their checkpoint; a save that still fails then is reported, and the draft that would keep it is C5's (R23). The save is a real `SaveAlignment` with the tokens read at open; the memory backend's scripted outcomes stand in for the store's in the tests. D5: the original-language book is cut once per session and book (`tc_align::SourceBook`), so opening a verse costs that verse (first open of a Psalms verse 38 ms in release, with the book's resources and the cut; the next 0.4 ms, `docs/evidence/align-session-2026-10-19.md`); the source label and pin come from the `Pins` record as the store stamps them. D6: `align/index` is the rail, and the picker's Align card counts from the same function (B5a's own count is gone), with the pin's sha in the rule, so a new UGNT marks a verse Invalid (#722); each row's text is cut at 240 characters (the row shows one line). D7: the gateway lens is a field of the view: its text, and each card's label compared in NFC on both sides (#768), only for a project numbered as its sources are and a pane that carries this very verse. D8: the engine is a `Train` job per testament from one snapshot (when the switch goes on and when a testament is opened with it on), announced as `Suggest` events and carried by every view, so a `Resync` restores it; `align/suggest` asks the model on the blocking pool. A saved verse does not join the model yet, and the retrain budget waits (C5). D9: the page queues clicks (`app::align::EditQueue`); a gap or a `Suggest` event reads the aligner again (`refresh_for`); the gate hook calls `align/flush`; each view and each flush answer set the aligner's save state, so a lost `AlignSave` frame cannot stand, and leaving the tool for another book closes the server's aligner. D10: declared differences: every card shows the pinned verse's words (#790); a card is a Tab stop while words are chosen and Enter places them, with the focus kept through the redraw (#568); a carry-over that loses nothing is saved on leaving (#787). D11: with C6 and C8 on main, three tC4 issues B6 shared became reachable and are proved: a verse aligned under one UGNT reads as needing review on the rail once C8's upgrade is confirmed, its links kept (#722, `ugnt_upgrade_marks_1_1_invalid_in_align`); a project C6 creates right to left, with a book added from an Arabic file, aligns with its bank right to left and its repeated words numbered in verse order (#29, `a_right_to_left_project_aligns_with_its_bank_right_to_left`); a span whose source words carry `x-ref`, as an aligned file added as a book brings it, opens with every link on its own card (#409, tc-align `an_imported_span_with_x_ref_opens_with_every_link`). Q-13 does not arise: G0's figure stands, and the page's own painted p95 is 19.3 to 20.6 ms in release (`align_click_round_trip_p95`, 60 clicks, three runs) and 23.0 to 24.3 ms for a placement on a rail of 2,461 rows (`align_placement_paints_within_30_ms_on_a_psalms_sized_rail`: the rows are keyed and each reads its own verse's state, so an edit that changes no verse's state draws no row). Left open: the rail draws every verse once (a Psalms rail has 2,461 rows, in the page 0.3 to 0.6 s after the first verse; B5a's virtual list would serve it if that is too slow); an edit in a read-only project is refused with `NotSaved(ReadOnly)` and shows nothing but the banner; a teammate's change to the open verse's text shows at its next open, not in the open view.)* | B5a | G0's round trip before B6 merges; Q-13 if it misses 30 ms. B6 can start before G0 reports, because the pure view builder makes Q-13's fallback a switch | J5 steps that do not save; `align_open_touches_one_verse`; `align_click_round_trip_p95` within 30 ms; `fast_clicks_are_queued_not_refused`; `unsaved_links_hold_the_close_guard`; `resync_restores_the_align_save_state` | 6 Align screens |
| B7 OBS | story rail; OBS Understand, Translate, Check; `/media/obs` *(B7: built as the row says, with these refinements. D1: the seed check no longer refuses the OBS sample: tc-journal gives frame 0 the title after its number, derives a story project's `settings.json`, and gives a pin that is no scripture no relationship (docs/evidence/state-seed-check-2026-10-18.md §4), so Home lists both sample cards and the Home-dependent parity rows were captured again. D2: `story/list` is Home's summary rows; `story/open` is one derived file of the story (`render(DerivedFile::Story)`) with its gateway story and pictures, so a story's read costs that story, not the project: its decisions and comments are read per story (`story_decisions`, `story_comments_of`), and `story_open_touches_one_story` holds on a project whose other stories hold ten times the text and comments. D3: `helps/story` is the passage view with the story as the chapter and the frame as the verse, so the helps panel is one component for both kinds of project; its Comments tab is Translate's; `helps/storyArticle` needs no book. D4: a check list is over a unit, `CheckUnit::Story(StoryNo)`, as tC4's `deriveObsItems` scopes a session to the open story; `check/picker { unit }` has no Align card for a story; a story's item is `check/storyItem` (the quote, the note, the gateway frame with the quote's words lit, the person's own text), not B5b's `check/item`, which is for a book. D5: `/media/obs/{pack}/{path}` serves the pictures of an installed pack, the pack named `owner-host~owner~name~sha`; a story project's own pictures (#549) are not served, because no screen reaches a project that carries pictures until X, F1 and E1b can bring one in. D6: declared, until C3 saves them: Translate's cells and the comment box of each unit take no typing (a box with nowhere to save would lose what was typed). D7: declared, until C4: a story's check card has no marking row, bookmark, comment, tap-to-select words or Learn more *(C4 D3: built)*. D8: the rail's percentage for the open story is Home's summary, as the rest are; a reload while in a story returns to story 1. D9: #507 is done (a story project opens at story 1); C40's compare card is the gateway card with B5b's remaining parts; #769's fold headings for the stories' groups wait for the reference move, as B5a's did. D10: parity: `obs-check-picker` is the same as tC4's; the other five rows differ only as the residuals say (the read-only comment boxes; the note on the picture pack, which tC4's mock lists as installed with no pictures; D7's parts); the q2-obs-card mask is wider because "Sync with Door43" is longer than tC4's "Upload changes"; journeys: `story.test.mjs`. D11: story checks' Previous/Next and item counter follow the resource's item order independently of the rail's grouping, matching tC4 (`story_word_checks_move_through_each_item_in_resource_order`); a help card follows its own memo, so moving the frame or clicking another card keeps an expanded note expanded, as tC4's keyed cards do (`an_expanded_note_stays_expanded_when_the_frame_moves_and_a_card_is_clicked`, with the Bible counterpart `a_card_click_keeps_an_expanded_note_expanded`).)* | B3, B5a | — | `story_open_touches_one_story`; `a_story_reads_its_own_decisions_and_comments`; `reading_a_story_writes_nothing`; journeys J22 and J25 (`story.test.mjs`) | 6 OBS screens |
| B8 Community Checking preview | the screen over A10's model; page setup; export items listed; the preview's pages: tC4 measures and splits them in the view (`tC4:src/views/print/PrintPages.jsx`), so the UI applies the page setup to each page's parts with `model::print_parts`, the twin of `tc_export::book::print_parts` (§3.8) *(B8: built as the row says, with these refinements. D1: the preview reads `export/book { at, book }`, tc-export's `printed_items` and `has_undrafted` over the verse records Translate's rows are made from, each verse's text `shown_text_v1`, so it prints the words inside nested alignments (tC4 #782), and `export/stories { at }`, tc-obs' printed stories with each drafted frame's picture under `/media/obs`; reading either writes nothing, and a book's print reads that book's verses once (`print_book_touches_one_book` on Psalms and ten times it). D2: `tc_api::model::print` is the twin of `PrintPages.jsx` (`atoms_of`, `parts_of`, `paginate`, `continues`) and of `PrintParts` (`print_parts`), held to `tc_export::book::print_parts` on every preview page of `fixtures/export/print-vectors.json` and its paging to tC4's pages. The UI measures each page in a hidden sheet with the markup the page then shows (`app::print::flow_markup`, tC4's static markup of `PrintParts` byte for byte on every preview page), splits a verse between `Intl.Segmenter`'s word breaks, and measures again when a web font arrives. D3: the page setup is the screen's view state, at tC4's default each time the screen opens, as in tC4. D4: the export menu lists the producers tC4's table gives the project (Export PDF only with the shell's `printPdf`, aligned and plain USFM for a Bible, Story Markdown for stories, the Burrito zip), each off until D2 builds the exports, so no click promises a file. D5: both previews follow the project's changes through the cache's `Book` and `Story` keys; tC4's book preview follows its state, but its story preview reads once each time it opens. D6: `en-community-checking` and `cc-obs` are tC4's pixels and DOM exactly; `cc-obs-wrapped` is tC4's pixels exactly and differs in the DOM only where React updated a chip's style one property at a time (its residual). D7: `FilterChip`'s style may change while the chip is on screen, so a page-setup chip keeps its node, and the focus, when chosen.)* | B4a, B7, A10 | — | print preview scenarios on Titus and the OBS sample; the `print_parts` differential test | `en-community-checking` |
| B9a Settings, source texts, downloads | M7 (read, and `viewFromHome` per AD14); M8 steps 1 and 2; download jobs over A11's streaming (`install_dcs_watched`, with a `DownloadWatch` over the job's counters and cancel token); A11's streaming in tc-resources' `AquiferClient` before an Aquifer asset gets a `Download` job (§4.6). tC4 shows only the repository being fetched (`sources.progress`) and has no Cancel; a byte count or a Cancel button is a declared difference for the owner (AGENTS.md, "Matching tC4"). `source-texts` draws the Urdu (Devanagari) row as उर्दू, left to right, where tC4 draws اردو: a mask in `parity/app/masks.json` covers that row and cites `#701`, which docs/PLAN.md lists among the differences on purpose (FIX-misc) *(B9a: built as the row says, with these refinements, each written where the design text names it. D1: `settings/viewFromHome` is one read of the project's derived files, `settings.json` and `resources.json`, through the new `ProjectStore::derived_file` (the store's file, or the memory backend's rendering), with no open and no fold: the language name, direction and font as tC4 reads them, the checking language from the pins' primary gateway named by its autonym (#579), never an obsolete `checkingLanguage` (tC4 #529), and the installed packages that cover the project's books (`gateways_covering_project`); one reply for the whole dialog, so tC4 #641's two reads cannot settle apart, and a reply lands only on its own project's form. A settings file another program holds is stated and keeps Save disabled. `settings/view` is not built (§3.9). D2: `resources/sources` gives the 18 packages with tC4's `seen` releases (now in tc-resources' registry) and this session's downloads per package (tC4 `installedSrc`), the project's checking language, the checkable packages and whether the project has language sets; `resources/catalog` ports tC4's `packageRows` over Door43's catalog search for the organization (`tc_app::resources::package_rows`, tC4's packageRows tests ported). D3: the `Download` job takes, for each repository, the project's pin when this computer lacks that version (tC4 `unsatisfiedProjectPinFor`), else what is installed, else the latest release (`pin_release`), and installs with `install_dcs_watched` on a `DownloadWatch` over the job's progress (`Phase::Install { repo }`, bytes) and cancel token; a cancel ends `Cancelled` with no install and no part-downloaded file; a download that installs something drops the session's book cache and the UI reads again what follows the pins (tC4's `installEpoch`). tC4's pin adoption after a download (`adoptDownloadedPins`) needs a pins write, which C8's re-pin brings. D4: resources are read from Door43's production server when the account server is QA, and from the account server otherwise (a test fake), so no flag is added (`resources_server`, #120). D5: Cancel while a download runs is the declared difference (`wizard.cancel`, a word tC4 translates); the footer shows tC4's `sources.progress` with no byte count. "Check for updates", "Use for checking" and a Checking language card's package are named and empty: C8's. D6: the Urdu row lies below the dialog's visible part, so a mask, whose rectangle must lie in the window, cannot cover it: `source-texts` is tC4's screenshot pixel for pixel and a DOM residual cites #701. Tests: tc-app 8 (`tests/app/sources.rs`: the settings-contract read part on the memory backend and on the store, a folder that is gone, a held file, the packages, a package's rows behind the internet switch, a download, a cancelled download) and 13 unit tests (the packageRows port, the settings values, #120's server); tc-api goldens for the four endpoints; tc-ui 6 host tests (tC4's settingsOpen cases that remain, the rows); journey `source-texts` (2). Parity: `project-settings` (the dialog's DOM line for line; the Home behind it lacks the OBS card, B2's residual) and `source-texts` captured on the store.)* | B2, B3, A11 | — | settings-contract (read part); cancel leaves no partial install | `project-settings`, `source-texts` |
| B9b Share steps and Open from Door43 | share target and check steps against the fake; R12 (count and the fake's route); M6 form with `ScriptedOpenDoor43`; shared state on cards *(A8: B9b also captures the DOM of `share-internet-consent` and `open-door43-internet-consent`, whose triggers it builds; it reaches `share-signin` from the card, captures its pixels with the `signin-server` mask, and deletes its `rust` route)* *(B9b: built as the row says, with these refinements, each written where the design text names it. D1: the share dialog's first step is one read, `account/orgs { language }` [share]: the session (`App::door43_session`, so a kept token resumes here), the organizations, each one's permission and its count of repositories in the project's language, and tC4's `recommendOrganization` (`tc_app::share::recommend`); a 401 forgets the token and answers `NeedsSignIn(TokenRejected)`, a Door43 that does not answer `share.offline`; `door43/shareTargets` is this endpoint and `door43/shareCheck` is gone (§3.9). Every call takes `--door43`, the account and write server (#120). D2: R12 built (§9.2): the parity account holds tC4's counts, 4 es-419 repositories in Equipo-Ejemplo and 12 in es-419_gl, checked against the baseline's fake, so Recommended falls where tC4's capture has it. D3: the dialog opens at once on its where-it-goes step when a sign-in is held, reading the organizations under tC4's spinner; without one the read answers `NeedsSignIn` and the sign-in step comes first, then the step again (§6.7). Change signs out and asks the sign-in again; Share on the check step is E1b's run. tC4 keeps the footer's buttons across Next and Back, so the pressed one keeps the focus under its new label; the rebuild gives the focus to the button in the same place. D4: the share dialog's progress and end, with the team-access line for every project (#724, Q2), are E1b's: §7.4 gives the `share` group to E1b, which meets them, so TC4-ISSUES' B9b item of #724 moved to E1b. D5: `door43/open { repo, name }` [open] is a `Join` job ending `Created { project }`; the name is refused when an entry of the projects folder has it, without letter case (`open-door43.name-exists`), before anything is sent: a project, an unusable folder, and the others Home does not list (a file, a folder with none of a project's files, a staging folder), which `ProjectStore::list` names in `Listing::others` and `home/list` in `others` for the dialog's own check, as tC4 checks `listLocalRepos()`; then `ScriptedOpenDoor43` answers: a test's scripted outcome (`App::script_open_door43`), else `Unavailable(Sync)` until X; the page waits for the job's end in the jobs it holds from the event stream. The source and name rules (tC4's `openSourceOf`, `openDefaultName`, `openRepoPathOf`, `repoPathInUse`) are `tc_api::door43`'s, applied by the dialog and again by the server; a link is read on `--door43`'s host and port. Q3's sign-in on 401 or 404 is E1b's, and the page already answers `NeedsSignIn` from the open with the sign-in step. D6: Q3's note is `openDoor43.whereAny`, each language's tC4 translation without its second sentence, in all ten languages; Q-21, answered by its recommendation, is `openDoor43.refused.notOpened`, without "make sure that the repository is public", pending outside English; tC4's `sync.obs-unsupported` sentence is not used, since story projects open and sync here (Q2). The `q3-where` mask finds the note by its first words on both sides. D7: the DS `Field` takes a `live_error`, so a field keeps its input and its focus while its error comes and goes, as React's re-render keeps it (the gallery's Field passes none, and its DOM is unchanged). D8: shared state on cards was B2's (`Door43Card` from `.tc/sync.json`, its tests in `tests/app/home.rs`); B9b adds nothing to it, and a share or an open that adds a project changes it when E1b and X write `.tc/sync.json`. D9: the gallery case `signin-error` takes its state's own route (Share, then the sign-in step) and lost its `rust` route, as `share-signin` did. Tests: tc-door43 2 (the count by owner and language, private repositories for members only; an answer without `X-Total-Count` is an error, not 0) and the internet-switch sweep; tc-door43-fake the parity account against the baseline's counts; tc-api 5 (M6's rules) and the goldens of the three endpoints; tc-app 13 in `tests/app/share.rs` (tC4's four `recommendOrganization` cases, the targets against the fake, the internet and sign-in before Door43 is asked, a refused token, an unreachable Door43, the repository link, the open's name check, question, `Unavailable` and scripted outcomes); tc-ui 14 host tests in `tests/app/share.rs` (the `openFromDoor43Dialog` port's rules and the share form's); the journey `share-open.test.mjs` (3: internet-consent a, b and d up to the check step, J11 2, and the open's question, Not now and refusal). Parity: the six screens captured on the store; each dialog's DOM is tC4's line for line apart from the masked server and Q3 lines, the dialogs' pixels are tC4's (Open from Door43's half a line lower by Q3's shorter note), and every row differs only by B2's OBS card (`residuals.json`); `signin-error`'s pixels likewise.)* | A8, B2 | — | `openFromDoor43Dialog` port; internet-consent share and open cases | 6 screens (§7.6) |
| B10 Scripted sync and conflict chips | `ScriptedSync`; `SyncResultView`; X10 toast; X12 chips and tile counts from fold conflicts *(B10: built as the row says, with these refinements, each written where the design text names it. D1: `door43/sync { project }` [sync] (job `Sync`) asks the sign-in first (`NeedsSignIn`; the UI shows the sign-in step and syncs after it), holds the `Sync` flow (AD31), waits for the project to settle, then ends as `test/script` set it, else `Unavailable(Sync)` until X (§3.9). `JobOutcome::Synced(SyncResultView)`: the changes received, who sent them (each sender's `PersonLabel`, the fold's own label, so the toast and B11's review name a teammate alike, #680), the conflicts that keep the newest version, the verse conflicts, the verses with two alignments (#791), the changes sent again, and the missing pins by tC4's rule (`tc_resources::resolve::missing_pin_count`); the counts come from the fold's counts by kind (`tc_app::sync::ConflictTally`), with no walk of the conflict list. D2: a scripted sync's teammates are sealed into the closed project and the project is opened through the registry, so the fold takes them in and writes `conflicts.json` (§4.2); the sync then tells Home the card changed, and the card's chip and the tiles' counts are the fold's conflict (X12, B2's summary). D3: Q36, one fixed sentence for an unexpected stop (§3.5). D4: the toast is `views::sync_result` over the pure `app::sync::SyncToast`. Review shows for a verse conflict, as in tC4, and for a verse with two alignments, under the note "1 verse has two alignments to review." (#791; a declared difference: tC4 4.0 keeps the newest alignment and counts it among the conflicts that kept the newest version; the per-verse rows of tC4's D108 wait for tC4 to ship them). Review and Download do not close the toast: the open they start (B11's review, B3's open and B9a's Source texts) closes it once open, so a failed open keeps its links (#724). D5: the `shared` scenario's project keeps its Door43 remote on the store (`.tc/sync.json`), which C1a's scenario store left out, so its card reads "Sync with Door43". D6: the parity environment's Door43 steps set the next sync through `test/script`. tC4 opens the Bible sample once before its push only because it writes the sample's journal at that first open (`parity/tc4-baseline/README.md`), so the driver's `openOnceBeforePush` runs where the environment says so (`journalAtFirstOpen`, tC4's) and the Rust app, whose projects hold their history from the start, skips it; `open-door43-refused`'s refusal is the environment's `door43Lacks`. Screens of scripted outcomes run on the store (`--backend memory+scripted` names the outcomes). D7: the seed check still refuses the OBS sample, so `q2-obs-card` has no element on the Rust page; its `rustAbsent` says why, and the gate takes tC4's element out alone and fails once the rebuild draws it (§7.5). Tests: tc-api goldens of `door43/sync` and `test/script`, and `io_reason_renders_without_the_path`; tc-app 5 in `tests/app/sync.rs` (the question and the sign-in before anything, `Unavailable`, `sync-ok` and `sync-ok-teammates` on the store with the chip and the Titus tile, `sync-ok-restored`, a failed sync changes nothing, the tally by kind) and `a_shared_scenarios_project_keeps_its_door43_remote_on_the_store`; tc-ui 9 host tests in `tests/app/sync.rs` (the `syncButton` result and `actorLabel` ports, the closed set of sync refusals); the gate's planted `rustAbsent` case; the journey `sync-button.test.mjs` (2: no Door43 request at launch or after 5 s idle, one press through the question and the sign-in to the toast; every refusal through the UI). Parity: the 7 screens on the store, each differing from tC4 only by the refused OBS card (`residuals.json`): the DOM with its computed styles line for line, the toast pixel for pixel, `syncToast` field by field. The milestone counts the screens of B3 to B9a too, which have not landed: 24 of 62 screens are captured.)* | B9b | — | `syncButton` toast ports; sync refusal closed-set test through the UI | 7 screens; milestone: 62 of 62 in the report |
| B11 Conflict review: read side | review tabs, grouping, labels, colours, clock warning; detail; History; Detached; Notices; "2 versions" markers in Translate, Align and Check; resolve on scripted outcomes with the double-submit rule *(A-27)* *(B11: built as the row says, with these refinements. D1: three endpoints (`tc_api::conflicts`). `conflicts/list { at, tab }` (query) answers every tab's count from the fold's counts by kind, and the tab's entries from one counted walk of the conflict list: each with its place, kind, group (the conflicts one set of batches made, STORAGE §8.1), its versions newest first (who made each by the fold's person label, so the review names a teammate as the sync toast does, #680; its date; the clock warning of a version dated more than a day after this computer's clock; the version Translate shows) and its review tokens. The Detached and Notices tabs list the records with no place and the store's notices. `conflicts/history { at, record }` (query) lists every version of one record with how it came to be (#78, read only; Restore is C7a's). `conflicts/resolve { at, request, conflict, review, keep }` (command) keeps one version through the store's `Resolve` with the entry's review tokens, on the store as on scripted outcomes, and the reply cache gives a second send of one request its first reply (A-27). D2: the Text tab is the port of tC4's `ConflictReview.jsx` (`views/review.rs`, its decisions in `app/review.rs`), drawn in Translate in place of its columns, as tC4 draws it. The other tabs (alignments; checks and comments; settings, sources and books), Detached and Notices are rebuild-only (Q-9) and read only until C7b; the tab strip shows only while a tab other than Text holds something, so with verse conflicts alone the review is tC4's one screen. A Text entry the tab cannot keep yet (a joined or split verse, a chapter opening, a version this computer cannot read) shows its versions with "Every version is kept" (`review.later`) in place of Keep. D3: the ways in. Home's chip and the sync toast's Review open the project in Translate at the book of its first verse conflict (tC4 `openProjectConflicts`), so Back and the ULT verse are that verse's; Translate's button and a verse's "2 versions · Review" marker open it at that verse. The aligner's verse and a check's decision carry their record's "2 versions · Review" marker (#791, rebuild-only) and open the review of their tab over the Check screen, whose Back returns there. The toast closes once the review is open, so a failed open keeps its links (#724). A mode tab or leaving the project closes the review, and leaving writes nothing. The review never opens over an open editing card. D4: Keep this version is off while it runs; a press after a lost reply sends the same request, which the server answers with its first reply, and once the server has answered (an `Ack` or a refusal) the next press is a new request (`a_press_after_an_answer_is_a_new_request`). D5: the leave checkpoint tells Home's card to read again (`Event::HomeCard`, `tc_app::session`): Home is already drawn when the checkpoint writes the conflicts file its counts come from, so a kept verse's conflict stayed on the card until the next Home visit, against J18's end state. D6: the parity environment's scripted teammates edit Door43's copy of the project (the project as the scenario wrote it, then as each scripted sync left it), as tC4's environment's teammates edit what they pulled; before, they edited the verse as this computer had it, so a verse saved here before the sync was never a version of its own (S4's `conflict-review-mine`; `a_verse_saved_here_before_the_sync_is_a_version_of_its_own`). D7: a verse version shows its whole content, words and layout trimmed, as tC4's `version.text.trim()` does, so versions that differ only in a footnote, a heading or a poetry line look different (`versions_that_differ_only_in_a_footnote_look_different`). D8: versions from two computers that carry one person's label (one person's two computers, a team account) are told apart by computer (STORAGE §8.5, `PersonLabel.computer`): this computer's says "this computer", another is numbered among the person's other computers, and another computer signed in as this computer's person is named by its username, as tC4 names it, so only this computer's version is "Mine" and light blue. tC4 shows such versions with one label; the computer is a declared difference, its words rebuild keys pending translation (`versions_of_two_computers_under_one_username_are_told_apart_by_computer`, `versions_from_two_computers_name_their_computer`). Tests: tc-api goldens of the three endpoints and the review types, and their round trips; tc-app 5 in `tests/app/conflicts.rs` (the Text tab's versions with teammate, date and order; the clock warning; Keep with review tokens, a second send's first reply, the grid and Home's card told to read again after the leave; History; a verse saved here before the sync stays a version of its own) and the count rows `conflict_tab_costs_its_conflicts` and `a_verses_history_costs_its_versions` on both backends (§8.2); tc-ui 13 host tests in `tests/app/review.rs` (the `conflictReview` decisions, the review's half of the `actorLabel` port, colours by owner, the tab strip, the footer's words, the double-submit rule) beside B10's `actorLabel` port; the journey `conflict-review.test.mjs` (3: J18's UI steps from the toast, which a refused open leaves in place with its links (#724), Home's chip, Translate's button and a verse's marker, one resolution however often Keep is pressed, History, Back writes nothing, a mode tab leaves the review, Previous and Next between the verses two syncs left in conflict, each shown with nothing chosen and the other shown once one is kept, and Translate and Home show the kept verse). Parity: S4's three cases are captured on the store with the scripted sync (`--backend memory+scripted`), each through its state's route: every pixel and DOM line, computed styles included, is tC4's but for the versions' dates and `data-ts` (each version's own time and id) and the History button (residuals). Left: the look-again refusal's words through the UI, Restore and bulk resolve are C7a's; the other kinds' choices and Detached's Dismiss are C7b's, after Q-9's sign-off.)* | B4a, B5b, B6 | — | `actorLabel` port; gallery case S4 Text tab against tC4's verse review; a second click gets the first reply | J18's UI steps on scripted outcomes |

### Phase 2: after S1

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| C0 ◆ Store adapter *(A-5)* | `StoreBackend` over S1's `ProjectActor` (§4.2): thin if S1 provides R4, R5, R14 and R16; otherwise the command thread, the published snapshot, events from each `Delta`, its own `seq`, the frontier check *(C0: built thin, because S1 provides R4, R5, R14, R16, the O(1) snapshot and each command's `Delta`; no fallback and no frontier check (every fold step S1 makes after an open sends `Committed`, which `every_fold_step_reaches_the_seam` proves). D1: a command carries its `Origin` and `StoreCause::Command { origin }` reaches the pump, so a person's own save is `LocalSave { request }` (AD37); B1b's plan gave it to C2, but only the adapter can join the store's event to its command. D2: the listing is `Listing { projects, unusable }` and an open needs the exact listed name; staging folders are left out. A folder is a project in the listing exactly when tc-store's open would not refuse it as not a project: the listing reads `project.json`'s four fields as tc-store does. The listing reads the projects folder itself, not tc-store's `Store::list`, which keeps only folders with a `project.json`: every other folder that holds a project's files (`project.json`, `metadata.json`, `ingredients/` or `.tc/`), and every link to a folder that does, is unusable with its reason, and a file or a folder with none of them is not listed *(after the second verification: a folder that had lost only its `project.json`, a translationCore 4.0 project copied in and a link to a project were at first left off without a word)*. Declared differences from tC4 (§4.5): Home shows an unusable folder with its reason (B2), so no one's work is hidden, and a translationCore 4.0 project is unusable until Import brings it in. D3: two unit read-only reasons in tc-api, `OpenOnAnotherComputer` and `FolderNotWritable` (§3.4). D4: `test/saveVerse` (§5.8) runs the contract cases that go through tc-app until C2. D5: three contract cases run another way than the table said (§4.2's note). D6: `AppDeps.store` is `StoreChoice` and `App::start` builds the store (§4.1). D7 *(added after the first verification)*: a batch with changes that no seam submit asked for (S2's outside edits, E1a's intakes) goes on as `StoreCause::System` as soon as it cannot be a submit's: when no submit is in flight, or when the relay settles a stored or unchanged reply the actor gave after it; an error reply carries no `seq` and does not end the wait (§4.2's note; *after the second verification*, which found that the first text, "at most until the actor's next reply", did not hold behind an error reply); and since C0 makes tc-app depend on tc-store, CI's two clippy steps without test hooks build tc-store too and are among the gates (AGENTS.md). D8 *(added after the second verification)*: since C0 builds tc-store at `App::start`, every launch runs tc-store's start cleanup (STORAGE §6.4 N1), which removes the staging folders that a create, copy, join or import cut short left; C0 records each in the report log, an import as `import` refused with `import.write-failed` and a join as `open-door43`, each with facts `folder`, `interrupted` and `rolledBack`, as tC4's start records them, and a create or a copy, which tC4 makes no staging folder for, as `staging-removed` with facts `folder` and `kind` *(after the third verification, which found all four recorded as `staging-removed`)*; E1b tells the person of a removed join and F1 of a removed import, and a removed create or copy is recorded only (§4.2's note). D9 *(added after the third verification)*: tc-store clears a field's draft entry on every successful save of the field, so a draft sent while that save was in flight, which holds newer text, is gone when it lands; C0 asks the storage team for R26 (§9.2), adds `a_save_never_erases_a_newer_draft_of_its_field`, ignored until R26, and gives the fallback to C2 (§6.5). Evidence: `docs/evidence/store-adapter-2026-10-19.md`)* | S1, B1b | R4, R5, R14, R16 (fallback: built here); an O(1) snapshot and each command's `Delta` (no fallback: phase 2 waits) | every contract case of §4.2 that runs on the store, including `reads_never_wait_behind_a_long_command` | the contract suite passes on the store through C0, and the adversarial review's findings are fixed |
| C1a Move the app onto the store *(A-17)* | app data on `atomic_write`; persisted summaries; parity projects on disk through S1's create and sealing API; the parity environment switched to the store *(C1a: built as the row says, with these refinements, each written where the design text names it. D1: the parity projects are written as an import is, through `Store::staging` and `Store::place` with the very packs the seed sealed, not through `Store::create`, whose random id and run-time `Seed` changes would make the screens and §13.2's check no longer tC4's (§4.2; R29 for `project.json`'s writer). D2: `tc4 serve --scenario` runs on the store and needs an empty `--projects` folder; `--backend memory` keeps the memory backend (§5.1). D3: `test/reset` writes again each scenario project opened since it was written and removes every other entry of the projects folder, on the blocking pool after every session closed (§5.8). D4: app data is on disk through `tc_store::atomic_write` (`DiskFiles`, `FilesChoice`), each write on the blocking pool; `app-lock.json` stays tc-server's (§4.9; tC4 #525: the saved language survives a relaunch). D5: `reports.json` keeps the last 50 reports across launches, a declared difference from tC4 (§4.9). D6: `folders.rs` is kept until R28, because tc-store's check calls every folder under a home with `~/.dropbox` synced and reads the environment (§4.1, §4.9, risk 23). D7: persisted summaries, the store's half: the stamp and the summary files; Home's half (`SummaryCache`, the cards, the `HomeSummary` job, `home_costs_projects_not_history`) and the recapture of B2's and B3's screens are done by whichever of B2/B3 and C1a lands second (§4.5, §7.6); neither B2 nor C1a had landed when this was written, so the order is not assumed: the screen gate fails a memory capture once C1a has landed (D9), and `home_keeps_its_summaries_once_b2_and_c1a_have_landed` (tc-app `app_data`) fails the first tree whose `parity/app/units.json` lists both B2 and C1a until tc-app's library calls `PrefsWriter::record_summary` and `home_costs_projects_not_history` exists. D8: the summary stamp lists `.tc/pending/` and stats `materialized.json`, never the packs, which are nested by sealer and month (§4.5). D9: the gate fails a capture on the memory backend once C1a has landed (`MemoryAfterStore`, §7.5). D10: the count rows run on the store too, the open row holding the app's counts there (§8.1). D11: tC4 #766 (creation pins at the first open) is built here, a declared difference from tC4 4.0 (TC4-DELTA-2026-10-19 §1, "Do not wait"): a new `System` reason, `CreationPins`, and the write in tc-store's open (R27, §9.2). D12: teammates on the store are packs sealed by each teammate's own device, written while no session holds the project (AD36, §4.2). D13: `test/hold` arms tc-store's hold points and `test/quiet` counts a store thread waiting at one; tc-store's background work stays unseen until R30 (§5.8). tC4 #746: the scenario writer asks for its staging folder only after tc-store's start cleanup is over, so the cleanup never removes it, and nothing else in tc-app writes a project folder. `MemoryBackend` stays: every B unit after B1b still builds on it. Done when: account-menu, app-language, help, about, feedback and the `license` case recaptured on the store, the report the same row for row but each row's `by`, `backend` and Rust DOM hash, which moved only with the UI build id in the logo's link (each DOM equals a memory-backend capture of the same build byte for byte); the three behaviour recordings differ only in that build id; the journeys pass on the store (54). Tests: tc-app `store_scenario` (17: the creation pins at the first open, the scenario written, the same seed, tC4's bytes on disk, an occupied folder refused, #746, a crash at each of the 153 write points, reset, teammates, holds, the summary stamp, J15 on `FaultFs` delays), `app_data` (12, with D7's gate), `reports` (13), the count rows on both backends; tc4 `scenario` (4) and its args cases; tc-ui's planted `a_memory_capture_fails_once_c1a_has_landed`. Evidence: `docs/evidence/app-on-store-2026-10-19.md`.)* | C0 | — | J15 on real `FaultFs` delays; `home_costs_projects_not_history` by whichever of B2 and C1a lands second (D7) *(A-25)* | every phase-1 screen captured so far is recaptured on `store` with the same report; `MemoryBackend` is deleted when the last B unit has moved |
| C1b ◆ Offers and read-only modes *(A-3, A-9, A-10)* | draft offers at open: `drafts.json`, buffered entries and the shell's mirror, merged and filtered (§6.6); unverifiable drafts as text; read-only banners; `SecondWriter`; the copy offer, which carries the folder's drafts to the copy *(C1b: built on the server and as the UI's pure side; R20 and R7's copy are on main, so no fallback is built. The first build's three no-loss reviews found 7 blockers and 11 majors, nine distinct findings, all in how a field's unsaved text moves between the buffer, the mirror, `drafts.json` and the record; instead of patches the lifecycle is one typed machine, `tc_api::model::draft` (§6.6, the draft machine, with its table), and the special cases it replaces are deleted. D1: the offer is `session/drafts { at, request, buffered, mirror }`, a command since it writes: every unsaved text is taken into `drafts.json` as an offered entry before the answer, so *Keep* and *Discard* answer one entry by its number, and the page drops its slots (`settle`) only once the server holds them. D2: tc-store's `drafts.json` keeps offered entries apart from the session's current drafts; a save or *Keep* never removes another text of the field, *Discard* is written before it answers, entries record the run that typed them, and a draft of the opening run stays its page's (`OpenOptions::run`; the app names its run at start). D3: whether a later save superseded a stored draft is judged with the shell's mirror (a run with no line of the field ended clean); a text is saved when a version after its own held it; a save landed when one held its sent text. D4: `BufferLine::Removed` names its text and `BufferSlot` its time, so settling drops neither newer text in the mirror nor anything but an older item of the run in the buffer; settling writes the page's own text again into what it freed. D5: the copy moves the mirror's entries too, keeps going when the storage refuses, and is refused (`CreateRefusal::CopyUnsafe`) where the projects folder itself made the project read-only (Q-30, answered 2026-10-19: into the local projects folder; C6 builds it, the refusal stands until then); a screen open never joins a read-only session the person left, so "That computer is closed" takes effect and its trust is used. D6: `tc_ui::app::offer` holds the lines the page sends (leaving out the fields its save machine holds), `settle`, `copy`, the offer's words and each reason's banner; the rebuild keys `drafts.offer.*` and `readOnly.*` (with `readOnly.copyUnsafe`) are English in every other locale until translated. No tC4 counterpart: tC4 4.0 has neither an offer nor read-only modes. Not built here: the dialog and the banner as views (B3, B4a, from `offer_words` and `banner`); the glue that restores the page's run's entries at every open of their project, not only after a reload (B3, §6.6); the banner names no host for another computer's lock. Tests: tc-api `tests/draft_next.rs` (every pair of state and event, 320, drops no text silently; the findings' rows); tc-ui `tests/app/draft_machine.rs` (the property, 2,048 cases here and 1,500,000 once in release while building; each of the nine findings put back is caught), `offer.rs` and `buffer.rs`; tc-store `drafts.rs` (one of two offered texts kept, Discard after typing, the run recorded, an offered draft outliving a save); tc-app `tests/app/drafts.rs`, 10 cases through `App::call` on tc-store over `FaultFs`, among them `relaunch_offers_buffered_and_mirrored_entries`, `draft_with_unverifiable_token_is_offered_as_text`, the crash tests through `test/crash`, `a_second_writer_makes_the_project_read_only_and_the_copy_carries_its_drafts`, `a_reopen_after_that_computer_is_closed_never_joins_the_read_only_session` and `an_older_draft_is_not_offered_over_the_sessions_own_save`; tc-api's golden files.)* | C1a | R20 (fallback: "Copy text"); R7's copy command (fallback: the banner without the copy offer) | `relaunch_offers_buffered_and_mirrored_entries`; `draft_with_unverifiable_token_is_offered_as_text`; crash tests through `test/crash` (end the server while typing, relaunch, the text is offered, Keep makes a conflict when the verse changed); the copy carries drafts (with R7) | the tests pass and the adversarial review's findings are fixed |
| C2 ◆ Translate saves | Production `editor/saveVerse`, `saveLayout`, `saveSection` (on Save only), `draft` with the buffer entry's original run and `at`, and exact durable `discardDraft` for Cancel. Ordinary UI builds register all three writers. A dirty Ack refreshes the newer draft immediately on the returned tokens; R26 already preserves it before Ack. Retry retains the request and text; `NotWritten` and `SavedWithDiskError` follow the store outcome. Confirmed saves and original draft times supply Resume. Implementation and evidence: `.audit/plans/C2.md`. | C1b, B4b | R6 and R22 provided; R26 fixed by C5 | J2; J6 edit part; strict Psalms/10× verse/layout/draft counts; held durability; disk refusal/Retry and saved-with-warning; real-store reload property; pre-Ack newer-draft crash/Keep conflict; panic buffer recovery; durable Cancel with failed/lost-request Retry | J2 green |
| C3 ◆ Comments and OBS saves | Production `editor/addComment` on blur only *(A-18)*: no token adds a comment, the displayed comment's token revises that comment (R24, History keeps the earlier text), a pause writes only the draft. Production `story/saveFrame` and `story/saveRef` for the title, frames and reference line; a text the story rules refuse is a typed `InvalidText` refusal that keeps the text for correction or Retry; the OBS editing lock and card stay until a clean Ack. Drafts, Ack/Retry, recovery and Resume use C2's save machine and reply cache; a recovered tokenless comment settles only on equal text at its exact target, else Keep adds it as a new comment. Implementation and evidence: `.audit/plans/C3.md`. | C1b, B3, B7 | R15 and R24 provided | J16, J21, J25, app-language e's comment half; three pauses and a blur add one comment; a teammate's comment in the unit is untouched; real-store reload and crash-before-Ack properties; strict comment and story counts | green |
| C4 Check writes | `check/decide` with rows in the `Ack` *(A-12; B5b built it through the seam, with the check's row and the comment in the `Ack`, so C4 keeps what is left: the comment's server draft)*; the check comment with its draft *(A-10)*; re-assessment after verse deltas; late draft (DELTA:906-907) *(C4: built as the row says, with these refinements. D1: the comment's server draft is C2's machinery: while the check comment is typed, its writer sends `editor/draft` with `DraftField::CheckComment` over the newest decision token the page knows, as its save does, so a draft kept by a crash continues from the person's own marks instead of becoming a conflict with them; the store's existing path offers it at the next open, Keep stores it on the decision, and a landed comment save clears it (`a_check_comment_typed_before_the_app_ended_is_offered_and_kept_beside_the_mark`, `a_saved_check_comment_leaves_no_draft_to_offer`, journey `a_check_comment_being_typed_is_drafted_on_the_server_until_it_is_saved`). A mark or a bookmark saves no comment, so it leaves the comment's draft (the store clears a check comment's draft only for a decision save that changes the comment), and after its Ack the page drafts the unsaved comment again over the mark's token (`a_check_comment_typed_before_a_bookmark_outlives_it_and_is_kept_beside_it`, `a_mark_under_an_unsaved_check_comment_drafts_it_again`, journeys `a_check_comment_typed_before_mark_invalid_stays_drafted_on_the_server` and its bookmark twin). Keep of a comment typed before the check had a decision is the save Done makes, `check/decide`'s comment over the draft's token, since a first decision takes the check's context and category from its list (`a_check_comment_typed_on_an_undecided_check_is_kept_as_its_first_decision`); when the check has a decision by then with no comment (the person's own mark, landed before the page drafted the comment again over it), the comment is saved over that decision's token, beside the mark with no conflict, since the draft's base had no comment either (`a_check_comment_typed_before_the_first_mark_is_kept_beside_it`), and only a decision that holds another comment stands beside it as a conflict. D2: re-assessment costs what a change reaches (AD17, PERF:79-80). The session's event pump logs each change's keys, and the chapters the fold projected again, before the UI hears of it. While every decision of a list sits on its own check (`tc_check::carry` places a decision by its key, quote and group id, never by its text or judgement), a changed decision re-assesses the checks it is shown on, a first decision its own check, and a verse, a chapter front or a re-projected chapter the decided checks of that chapter (of a frame, or every frame for a story's base text), each against that chapter's text alone (tc-journal's `decisions_at`, `check_texts_at`). A change that could move a decision to another check, a pin, a book's presence, a rebuilt fold or a gap in the log recomputes every row, as the open does. A verse key still names both of its book's lists in `ChangeParts.checks` (B1b): the shown check re-reads its verse after any text change, decided or not, which the late draft needs (D4). Tests: `decision_serializes_one_row`, `decision_refetches_no_rail` and `verse_edit_reassesses_its_checks_only` on Psalms and ten times Psalms, on memory and on the store; `marks_and_verse_edits_reassess_what_they_reach_and_the_rails_equal_a_fresh_open`, and the property `held_rails_equal_a_fresh_open_after_any_session` (marks, comments, bookmarks and verse edits). D3: a story's checks are written as a book's (B7 D7, J22): `check/item`, `check/article` and `check/decide` take a story's list; the item's source texts are a typed enum (a verse's original and gateway panes, or the gateway story's frame), so `check/storyItem` goes; the store judges a story decision against the frame's text as the card showed it (the frame's record, else the story's template text of it), so a frame edit that drops a tapped word flags the mark and keeps it. The story card is B5b's detail with B7's stacked card and 760 px column: Learn more, the words to tap with their hint, the marking row, the bookmark and the comment. D4: the late draft. The page holds no book load for Check: `check/item` reads the verse's current text from the session, and a `Changed` that names the open list (any text change of its book or story does) reads the shown check again, so a check shown before its verse has text shows the text when it arrives, with no navigation, as `data-drafted` 0 then 1 (journey `a_check_shown_before_its_verse_has_text_shows_the_text_when_it_arrives`), and on the return from Translate (#532, B5b's Bible journey and C4's story one). D5: found by the journeys and fixed: a mode tab pressed after a story's tabs show and before its story is read (the open's overlay shows 300 ms after the tile's press and waits for that read) unmounted the screen that takes the overlay down, so the overlay, its scrim and its Tab trap stayed over the whole window; a mode switch now ends the open's overlay, and the screen moved to shows its own loading state (journey `a_mode_tab_pressed_before_the_story_is_read_leaves_no_open_overlay`, which fails without the fix). D6: journeys `check-writes.test.mjs` on the store: J4 (a mark on dragged words, a comment and a bookmark are one record of `ingredients/checking/translationNotes/TIT.json`, the book's bytes unchanged, all three survive reopening, and clearing the comment writes `false`), J6 (a verse edited in Translate flags its check and keeps the decision with `userStatus` valid and the same token: the save wrote nothing on it), J22 (a story check marked in the browser is stored under `{story: 1, frame: 1}` with its word, the story file unchanged; a frame edit flags it and keeps it), check-project-switch (#434: stories, Bible, stories in one run, each rail on its own project's pins, and a tile that resumes into a tool), check-returns-draft's story half, the late draft and D5. The five check screens and the story session screen are captured again on C4's code.)* | C1b, B5b | Q-25 | J4, J6, J22; check-project-switch, check-returns-draft; `decision_serializes_one_row`; `decision_refetches_no_rail` | green |
| C5 ◆ Align writes and suggestions | the server's alignment save: 2 s, leaving the verse, replace or close, before the quit checkpoint *(A-4)*; `markValid`; training per testament from one snapshot; suggest, accept, reject. *(C5: alignment drafts carry both displayed tokens, and R26 preserves newer durable links before Ack. Background learning uses the exact confirmed save, preserves saves arriving during training and fences cancelled/superseded completions. The typed page queue retains failed requests and later clicks; Retry verifies the server run and resends the same body/id. Refresh leaves the workspace mounted, navigation drains edits, and an older Saved reply cannot certify newer clicks. A typed refusal retains the actions and states the error instead of silently rebasing them. This explicit failure state protects the translator's pending work. Five strict Psalms/10× count rows include saving while training is held; local saves no longer read every device profile. Release inference uses exact flat-tree batching and per-ngram metrics; the full NT+GEN+PSA probe and tC4 outputs agree. CI's unchanged 150 ms gate passes; the 50 ms reference target remains performance work for P/G2, not a claimed pass. Implementation and evidence: `.audit/plans/C5.md`.)* | C1b, B6 | R23 provided; R26 fixed here | J5; `links_saved_when_quitting_within_2_s`; timer/verse-change/Mark valid persistence; pre-Ack crash/Keep/Burrito; queue conservation and old-drop mutation; failed/lost-request Retry; old-Saved-reply fence; strict save counts; release suggestion budget | green, and the adversarial review's findings are fixed |
| C6 Create, add books, settings | M2, M3 create (R2), with Create disabled in flight and one `RequestId` *(A-27)*; `home/nameFree`, the folder a name makes, with R2 *(B2 D4)*; the language's Door43 code stored beside the project's when the dialog offered another (`NewProjectForm.door43_code`, Q35); M4 blank books (`AddBook`, seeding from the pinned ULT); add from USFM; settings save, including from Home through the single open (AD14, AD38); "Copy to this computer" from a project read-only because of its projects folder, into the local projects folder with no folder to pick (Q-30, in place of C1b's `CopyUnsafe` refusal) *(C6: built as the row says, with these refinements, each written where the design text names it. D1: the `create` service is five endpoints: `create/bible`, `create/obs` and `create/addBlankBooks` and `create/addUsfmBooks` are commands and `create/reviewUsfm` a query, not jobs: a create is one staged batch and a book is one batch, so nothing needs progress or a cancel. D2: `home/nameFree` is not built: tC4 checks the folder only when Create is pressed (`wizard.nameInUse`), so there is nothing to ask earlier; `tc_api::create::Details::folder` is the one pure function that makes the folder (tC4's slug of the name, else of the code) and refuses a missing name, code or folder name, run by the dialog before it sends and by the server again, and the server refuses a name any entry of the projects folder has (`NameInUse`, case ignored). D3: R3's fallback is replaced by `Command::AddBookText { book, scope, usfm, file_name }` (TC4-DELTA-2026-10-19 C37, #767): the presence, the book's records and the alignments `split_aligned` takes out of its aligned verses are one batch whose cause is `Import(Usfm)`, so a crash or a refusal leaves the whole book or none; a verse whose markup does not read apart keeps its text as written; a file with no chapter is refused before anything is written. `addBook.usfmTooLarge` is not built: no translator's file holds a verse that a change line cannot, and the rebuild never refuses a book for its size. D4: the files travel by the upload route (flow `usfm`, no size beyond 512 MiB; the API body cap of 8 MiB would refuse an aligned Psalms); the dialog uploads each file when it is picked, `create/reviewUsfm` says what that one file is (`Book`, or `NoId`, `NotText`, `Damaged` with where), and the book-level checks (the project holds it, an earlier file is the same book) are one pure rule, `tc_api::create::Claims`, which the dialog runs on its rows after every add and remove (tC4 #564: each file is classified once) and the server runs again on the request. tc-app reads an upload at `<app-data>/uploads/<id>` and deletes it once its row is answered. D5: R18's fallback is built: `settings/save` opens the project through the registry's single open (`open_side`, AD38), compares the fold's direction and font with the `shown` values the dialog sent (a difference is `SettingsRefusal::Changed`, nothing written), mints both tokens from that one snapshot, and writes only what changed, direction first; a refusal says which was written. `settings/view` is not built (tC4 opens Settings only from Home, #505). A project with neither `settings.json` nor `resources.json` (a Bible before its first book) is read from its history, not from derived files it does not have. D6: a create is one `Store::create` with the project's details, versification, the settings tC4 writes (`textDirection`, `textFont`, `languageName`), the bundled pins (tC4 #766: they are in the creation batch, not left to the first open) and, for stories, the fifty template stories; the language's name is the one typed, else the shipped Door43 list's name for the code, else the code; the Door43 code the dialog offered another for (Q35) is the setting `door43Language {code, name}`, since a Scripture Burrito has no field for it (`check_field` refuses `languagesExtra` and every `meta.*` name); `Store::create` checks each detail against the Burrito before a folder exists, so a code with a part over eight letters is refused with its reason (`CodeNotHeld`, #582). A story project derives `settings.json` as a Bible project does, as tC4's J20 expects and the storage team's note of 2026-10-18 (`docs/evidence/state-seed-check-2026-10-18.md` §4.2) asked; `derived_files()` lists it for both kinds. D7: a blank book is seeded from the project's pinned literal pane (`tc_resources::literal_pane_book`, tC4 `seedInitialUsfm`, #574), from the chapters and verses of its versification when this computer lacks the source, and not at all (the add is refused with the reason) when the source is installed and cannot be read, since a skeleton would stay in the book. D8: one `RequestId` for each attempt: a form keeps its attempt's id until the server answers or a field changes, so a press after a reply that never came gets the first attempt's reply from the cache, and Create is disabled while an attempt runs (A-27); one id for a whole dialog would be `RequestReused` at the first edit. D9: Q-30 is built: `StoreBackend::with_local_projects` names the local projects folder (`config::local_projects_root`), `copy` writes there when the projects folder is what made the project read-only and the local folder checks safe (made on the first copy, started with the app when it exists), Home lists its projects beside the others, and a name never opens two folders; where the local folder is no safer the copy is refused as before (`CopyUnsafe`, with the projects folder's reason). Declared differences: Add a book does not say that creating a held book again "will replace" it (`addBook.existsRest`), since it never does; a language with no name typed is named by the list, where tC4 had the platform's own table; the drop target is the whole USFM step. Gaps: a Bible with no book has no `metadata.json` (STORAGE §5.10), so its Home card shows its folder name until its first book; a new story project has no LICENSE or front and back matter, which are carried files (STORAGE §3.2) and need S2's blob store; the Copy button of the read-only banner waits for the banner, which no unit has drawn (C1b built the offer's pure side). Tests: tc-store `adding_a_book_from_usfm_is_one_batch_whole_or_absent`, `aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records`, `a_verse_whose_alignment_does_not_read_apart_keeps_its_text_as_written`, `a_psalm_adds_whole_with_every_verse_and_its_selah`, `a_book_from_a_file_is_refused_when_present_or_when_the_file_does_not_read`, `create_refuses_a_language_tag_the_burrito_cannot_hold`, `create_keeps_regional_language_tags`, and `AddBookText` in `crash_every_command_at_every_write_point`; tc-app `tests/app/create.rs` (16 cases through `App::request` on tc-store over `FaultFs`, among them `settings_from_home_mints_only_for_shown_values`, `card_click_during_settings_save_shares_the_session`, `copy_to_this_computer_goes_into_the_local_projects_folder` and the settings-contract cases); tc-ui host tests for the forms (`create_is_disabled_in_flight_and_a_press_after_a_lost_reply_sends_the_same_request`, `usfm_rows_are_judged_again_at_every_add_and_remove`, `one_chapter_book_says_1_chapter`, `removing_rows_from_25_aligned_books_removes_the_intended_rows`); the journeys `create` (J1, J20, language-tags, add books from files, settings-contract); parity: the four `add-book` cases captured, each dialog tC4's DOM line for line.)* | C1a, B2, B9a | R2 (no fallback: create waits); R3 and R18 (fallbacks in §9.2) | J1, J20; settings-contract; language-tags; `settings_from_home_mints_only_for_shown_values`; `card_click_during_settings_save_shares_the_session` | green |
| C7a Conflict choices: text and passage *(A-17)* | the choices tC4's Text tab has designs for: Keep this version with review tokens, `LookAgain`, History and Restore, with the double-submit rule *(A-27)* *(C7a: built as the row says, with these refinements. D1: `conflicts/resolve` carries a `KeepChoice` (`candidate`, `passage { picks }`, `separate { picks }`, `joined { picks }`) instead of one change id; `KeepChoice::Restore { from }` is History's *Restore this version*: the old version's record as the fold's `Values` choice over the entry's review tokens, so it is kept over every version of its record and refused when a head moved (a restore over the shown head only, through the store's `Command::Restore` with an edit token, would let the fold settle the conflict on the other side when the restored text equals the versions' base; the review has no such path); `Version.place` names each version's record, since a passage's versions are of several verses; `conflicts/resolveBulk { at, request, person, conflicts }` answers `BulkKept { ack, left }`, the person's computers being the devices the fold labels as that person (STORAGE §5.9 step 3); a store refusal of a choice is `NotSaved::Refused(SaveRefusal::Choice { reason })`. In tc-app, `choose` is the one way a choice becomes a store command, every `KeepChoice` to its journal `Choice` with no wildcard. D2: the footer per kind comes from one `choices(kind)` with no wildcard (`Choices::{Candidate, PassageSides, JoinedOrSeparate, Later}`): *Keep this version* (verse text, structure issue, book header, chapter front, the story kinds), *Keep <person>'s passage* per side (a passage), *Keep the joined verses* and *Keep the verses separate* (verse structure), Later for the kinds C7b takes; every History row but the shown one has *Restore this version*; with two or more conflicts listed, *Keep all of <person>'s* for each person with a version here, whose result says how many were left. Every press goes through one `KeepPhase` (`Idle`, `InFlight`, `ReplyLost`, `Answered`) and one `Actions::send`, so a double press sends one request and a lost reply resends the same one (A-27); a refused choice says why in words (`failure_text`, one arm per variant). The three new screens have no tC4 design and are not captured here: C7b step 2 draws them on its probe page for H5's packet.)* | C1a, B11 | R7 for bulk resolve (fallback: "Later") | J18 local; the look-again refusal through the UI; a second click gets the first reply | green |
| C7b Conflict choices: the other kinds *(A-17)* | check, alignment, settings and pins, book presence, comment and carried-file choices; Detached and Dismiss; the received sheet (Q-8) | C7a | Q-9: the owner signs off the designs first; R7 | a check-conflict resolution; one resolution per kind through the UI; DOM equal to the signed-off designs | green |
| C8 Resources apply | upgrade offers; M9; gateway change M10, with both previews behind the navigation gate *(A-26)*; plans with `carry::plan` converted to planned changes; `applyPlan` with `PlanStale`; guided-fix fetch, re-pin, sideload *(C8: built as the row says, with these refinements, each written where the design text names it. D1: the endpoints are `resources/checkUpdates` [sources] (a query: each language set's and each scripture text's newer releases, a text Door43 cannot answer for said in `textsError` beside the set offers, #258), `planUpgrade` [sources] (job `UpgradePlan`: finds the offer again against Door43's newest releases, so a screen that outlived its offer plans what is true, installs every repository of the release one after the other on a `DownloadWatch`, all of it or none, then plans), `planGateway` (job `GatewayPlan`, no internet question: it downloads nothing, a suite must be installed already), `applyPlan` (a command: `Applied`, or `Replanned` with the plan made again), `fixState`, `fixFetch` [fix] (job `FixFetch`), `fixSideload` (a command that takes the `UploadId` of a `guidedFix` upload; `tc_app::UploadedFiles` is the seam the server's registry fills, which `tc4` hands the app, and the file is the app's to delete once taken) and `fixRepin` (a command that answers a `PlanView`). The job outcome is `PlanReady(PlanView)`. A plan is kept on the server with the `UpgradePlan` it will write, by `PlanId`, for the open it was made for; the newest eight are kept. A `PlanView` says what the plan is for (`Set`, `Repin`, `Text`, `Gateway`, the languages by their autonyms, #579), the decisions at risk, a row per (tool, book) the change affects with the decisions it holds and, when its list could be derived, how many carry over and how many come back as work, the units it cannot be applied to and why (`NoResource`, or `VersificationUnknown`: `VersificationUnavailable` is not built, since the numbering schemes are bundled), and, for an original-language text, the verses whose alignments it marks for review. D2: the plan costs the books it affects (§8.2, `plan_cost_follows_affected_books`). `tc_check::change::unit_moves` says from the pins alone whether a (tool, book)'s list or its translation-words filter moves, so a book the change does not reach is never read; a moved book that holds no decision of the tool costs a range read, not a projection; a unit's decisions are counted by `tc_check::change::consequences` (which counts a Words decision whose article the new translation words lack, tC4 #661, where tC4 says nothing changes), and only an affected unit's list is derived from the new pins, through `PlanSources`, a trait over the resource store, and carried over by `tc_check::carry::plan`. A resource the plan cannot read fails the plan with its reason (tC4's D30 sweep: a count made over an unread resource would understate the change). The tools are listed Words before Notes, as tC4 lists them, and a blocked unit is listed too, with the decisions it holds and no carry-over (tC4's `gateway.affectedRow`). D3: what is written. The `Pins` groups the change moves, over the heads they had in the snapshot, cause `Edit` (the person's accept); coverage is recorded for the new pins only (tC4 D41), so the other set, the texts and the panes are the records they were. For each carried decision, one `Check` change over its heads, cause `CarryOver`, with the new resource and the new check's context over the decision's own (a field only the old context holds is kept); a decision that moves to another key is `MovedTo` at the old key and the whole decision at the new one. A decision no list places is not written: it computes as invalidated and is kept, and pinning its release again makes it valid. A decision that would move onto a check another decision holds or moves onto is not written either, and stays where it is, listed among the checks to check again: STORAGE §5.8 would write both as a conflict, but `ApplyUpgrade` takes one change per record and refuses an arrival over a record it did not read, so a request (R31) asks for it and this is the fallback. A decision whose record is in conflict is not written either, and none is moved onto such a record, since a change over the conflict's heads would end it and show one version; the dialog counts them as checks to check again too. Pins in conflict refuse the plan ("the project's resources have a conflict to resolve first"), and a download's pin adoption then ends `AdoptFailed` with that reason; heads that merge without disagreement are not a conflict, and a change over them keeps both. A decision shown on the swap of two keys is one change per key. D4: `applyPlan` submits one `ApplyUpgrade`. A plan the server no longer holds, or one for another open, is `NotSaved(PlanStale)`; a blocked plan is refused (`GatewayBlocked`, naming the books); `PlanStale` from the store makes the plan again once, from the project as it is, keeps it and answers `Replanned`, so the person reads the new figures and confirms again (tC4 #595: the preview is read once from one snapshot, never retried, and a verse that keeps changing touches no record the plan names); a save the disk refuses answers its reason and keeps the plan, so Confirm tries again. One dialog sends one `RequestId` for its confirmation (§4.8, A-27). D5: the guided fix. `fixState` is `tc_check::fix::preflight` and `guided_fix` over the project's pins and what is installed (an unreadable install list is said, never shown as "none installed"; a tool that can open has nothing to fix); `fixFetch` installs exactly the pinned commit, checked against it, and moves no pin; `fixSideload` installs a file only when it declares exactly the pinned commit (`install_dcs_zip`); `fixRepin` plans moving every slot of the rung that pins the missing commit to an installed commit of the same repository, which the person confirms like an upgrade. The modal that offers the three ways is B5a's (it opens from the Check picker's "Fix this…"): C8 gives it its endpoints, and the re-pin confirmation is the one M9 draws with the title "Re-pin". Because its case is drawn over B5a's Check picker, `upgrade-repin` is its own group `M9b` in §7.4, B5a's to capture. D6: the screens. `tc_ui::app::upgrades` holds what the dialogs hold and say (host tests); the web glue sends the requests and follows the jobs. Choosing a package in Project settings opens the project and asks the change over it (tC4 `chooseSettingsGateway`), and the change in Get source texts' second step asks it in place. One change of language at a time: a second choice while one is previewed or under way does nothing, and after Change neither button answers until it ends (tC4 #580). Both previews (and Check for updates' Upgrade) open only after the navigation gate has saved every field, so a plan is never read over work that is not safe (A-26); a standing failure keeps the person where they are with tC4's `gateway.unsaved` (for the gateway) and `upgrade.alignUnsaved` (for an upgrade, which tC4 gates only for a text). M10's headline and detail are English in every language, as tC4's `describeConsequences` writes them; a translated sentence needs the catalogs' words first. D7: a failed save holds the gateway preview (`GatewayState::apply_failed`, journey and tc-app test `a_failed_save_keeps_the_plan_for_the_retry`): the dialog stays on its plan, states the disk's reason under "The change was not applied.", and Change tries the same plan again. D8: the test hooks `test/script` gain `Updates` and `Plan`: the next `checkUpdates` answers a scripted view without asking Door43, and the next plan job ends with a scripted view, which the server keeps no plan under (confirming it is `PlanStale`, tC4's "the plan no longer matches the project"), because tC4's gallery draws offers and counts that its mock borrowed from another change. Never in a release build (AD35). D9: what a download brings (J3's "pins with sha"; the pins write B9a's D3 left for C8). After a `Download` that installed or found something, `tc_resources::gateway_change::merge_optional_pins` (tC4 `mergeOptionalPins`) adds to each of the project's sets that names the package the optional pins this computer now holds for it, the questions, the simplified Bible, the story members (all three or none) and the image pack, found apart from the required suite; a pin a set holds never moves. One `ApplyUpgrade`, with coverage recorded for the new pins, read again from the project as it is up to four times when heads moved, as tC4 retries; a write that fails ends the job `AdoptFailed` though the repositories are installed. A cancelled download changes no pin. Tests: tc-app 21 behaviour tests (`tests/app/upgrades.rs`: J12 in one batch with the other set, the texts and the heads of every other record untouched, a failed or untrustworthy download moves nothing, a stale confirm shows the dialog again with the new counts, a failed save keeps the plan, J13 with both languages by autonym and the English fallback kept, a preview that cannot read a resource, a verse that keeps changing, an unknown numbering blocks, a Words-only change that drops a decided article is counted and every decision is kept, scripture text offers and the alignments an original-language text marks, the guided fix's three ways, an app upgrade that leaves the pinned release and the pins as they were) and 6 unit tests of how a carry-over becomes changes (`plans.rs`), 4 of `merge_optional_pins` (tc-resources), `plan_cost_follows_affected_books` in `tests/op_counts/plans.rs` on Psalms and on the corpus ten times larger (a one-book plan costs the same on both; a plan of every book costs more and no more than two projections of a book's text per tool that holds decisions); tc-ui 13 host tests (`tests/app/upgrades.rs`); journeys `upgrades` (3: Keep, Change to English and Settings naming it afterwards, Check for updates behind the internet question). Parity: M10's `gateway-change`, `-busy`, `-blocked` and `-failed` are tC4's pixels and DOM exactly (the first from the real plan over the sample, whose counts are tC4's); `gateway-change-error` differs in the one error line, because the failure tC4 draws (another project open) cannot happen here; M9's five cases are tC4's pixels exactly and differ only in the Urdu row (#701), with the offers and counts scripted.)* | C1a, B9a | — | J3, J12, J13; bundled-upgrade; `plan_cost_follows_affected_books`; a failed save holds the gateway preview | green |

### Phase 3: after S2

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| D1 Checkpoints and outside edits | leave triggers; the checkpoint at Home *(A-25)*, and its failure's banner on Home (`s.commit_error`; the gallery's `home-checkpoint` case, B2 D10); mode-switch checkpoints by Q-5's ruling *(A-11)*; "Checkpoint not saved" with Retry; materialize-blocked message; the last checkpoint outcome and the blocked paths in `OpenSession`, so a `Resync` restores both (§5.4); outside-edit toast with Undo and grouped rows; held-file actions as R7 allows; window focus check; quit flow (aligner links, then checkpoints) | S2, C2, C5 | R9's numbers and Q-5 (fallback: STORAGE's rule today); R17, R21 | J8; no commit when nothing changed; `save_latency_while_a_checkpoint_runs` within 25 ms, or Q-5's limit applies; a `Resync` after a failed checkpoint and after a blocked file shows both again | green |
| D2 Exports | tc-export producers on A10's model: aligned and plain USFM, OBS Markdown, print HTML; export jobs; `/download`, `/print`; PDF through the shell; conflict warning; checkpoint first | S2, B8 | — | J7, J23 except the Burrito zip; every Burrito output validated | green |

### Phase 4: after X

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| E1a Sync *(A-17)* | the real `SyncPort` for sync; OS keychain (Q-3); the sync button flow (DELTA:717-738); R13's dropped connection | X, B10, C7a, D1 | R13 (fallback: scripted outcome); Q-3 | J26 with three app-data folders (its check-conflict step once C7b has landed); sync-button; `sync-failed` on a dropped connection | `shared-home`, `sync-failed`, `sync-ok`, `sync-ok-teammates` and `sync-ok-restored` recaptured on real data |
| E1b Share and open from Door43 *(A-17)* | share create, push, done (with the team-access line for every project, #724, Q2: B9b D4); open with sign-in on 401 or 404 (Q3); a join the start cleanup removed (C0's `open-door43` report with `interrupted` and `rolledBack`): Home says `refusal.open-door43.removed`'s words once, where tC4 says them when the person opens that project (`tC4:src/data/journal/opsLog.ts:317-318`), because this app's join never puts a half project at the project's name | X, B9b, E1a | — *(Q-21 is answered by its recommendation; B9b built its key, `openDoor43.refused.notOpened`)* | J11, J17, J24; a join cut short by an ended `tc4` is removed at the next start and Home says so | `share-dialog`, `share-dialog-check` and `open-door43-refused` recaptured on real data |
| E2 USB exchange | Save work for a teammate (folder, progress, verify, "You can remove the stick"); Receive work from a file (identify, confirm, job, report), never beside a sync of the same project *(A-31)*; the J7 Burrito zip as the exchange file; browser uploads up to 4 GiB *(A-28)*; the `Received` sheet readable again after a `Resync` (§5.4) | X, C1a, D2 | — | USB journey both ways, then a relay through a third person (STORAGE:3819) | green |

### Phase 5: after I

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| F1 Import | M5 all steps with uploads; tC3 version lookup; name clash; working state with progress and cancel; toast and badge; an import the start cleanup removed (C0's `import` report refused with `import.write-failed`): Home says the import did not finish and asks the person to import again | I, C1a | — | J9a, J9c, J9d; language-suggestions; an import cut short by an ended `tc4` is removed at the next start and Home says so | `memory_seed_equals_tc_import_seed` passes and the local seed is deleted |

### Phase 6: shell and performance

| Unit | Scope | Depends on | Waits for | Tests | Done when |
|---|---|---|---|---|---|
| G1 ◆ Electronite shell | main script: start `tc4` detached from libuv's job object (AD39), read the address line, the header filter of AD11 for paths outside `/api/` *(A-14)*; supervision: restart `tc4` and reload Home when it exits *(A-3)*; the buffer mirror (`bufferChanged`, `takeBuffered`, fsync at most once a second; appends and fsync off the main thread, which every request waits for, AD11) *(A-3)*; the single-instance lock (`app.requestSingleInstanceLock()`): a second launch quits at once and starts no `tc4`, and the running copy's window is restored if minimized and focused on `second-instance`, as `tC4:scripts/desktop-main.cjs:108-111, 134-140`; the prep window; close guard and quit; `printToPDF`; file and folder dialogs; `openExternal` and the same-origin rule *(A-33)*; keychain or feedback per Q-2 and Q-3; `protocol.handle` if Q-1 says so; `printPdf(url)` is part of the shell contract (`docs/contracts/SHELL.md`), so D2 is not needed to build the shell; only its end-to-end PDF test waits for D2's `/print` route | A2, A4b, A9, C1b, G0 | Q-1, Q-2, Q-3; a Windows runner (Q-26) | port of `scripts/desktop-prepare.test.cjs` (DELTA:1127-1131); end `tc4` while typing, relaunch, the text is offered; end the shell, `tc4` exits within 5 s and the next launch starts; `second_launch_focuses_the_running_window_and_starts_no_server`; a release smoke test that a second launch exits without starting a second server, as `tC4:scripts/package-desktop.zsh:936-946` does; a same-origin help link stays in the app | the tests pass on the release pipeline's Windows runner; the code is written and linted here |
| G2 Budgets and the P7 report | `appbench`; Playwright perf specs on the whole-Bible project; memory with a side session; the 62-screen report in `en` and `ar`; one capture run inside Electronite on the release pipeline *(A-34)* | P, every unit | Q-28 | §8.2 green at the 3× tolerance | `docs/evidence/app-budgets-<date>.md`; a person reviewed the residuals |
| G3 Release pipeline and installers | one script per platform, called by one workflow: build `tc4` and the UI with `TC4_BUILD_COMMIT`, the English bundle (`bundle_store`), the pinned Electronite zips checked by sha256, sign, install and a smoke test after install; writes `help-desk.key` (`docs/contracts/SHELL.md` items 7 to 9 and 11) | G1 | the owner's platforms and signing (ROADMAP H3, H4); Windows and macOS runners | the smoke test after install on each platform; the OBS pack check against `bundle/en/fetched.lock` | an installer per chosen platform installs and passes its smoke test |
| G4 P8 Graphite probe and right-to-left project journeys | the spike's `spikes/electronite/page/probe.js` run inside Electronite on three systems, with numeric limits; right-to-left project journeys | G1 | Windows and macOS runners | the probe on three systems; the right-to-left journeys | the probe passes inside Electronite on three systems |
| G5 Browser journeys in CI | *(built: `.github/workflows/app-e2e.yml`)* an `app-e2e` workflow that runs every `parity/app/journeys/*.test.mjs` and fails on any skip | nothing unbuilt | none | the `journeys` job; a skipped test fails the job | the job is green on main with zero skips |

**Order and parallelism.** In phase 0, A1 comes first, and A6a, A10 and A11 need nothing and can
start at once. After A1: A2 and A3 in parallel. Because they are built at the same time, neither
calls the other: A3's `App` answers every endpoint through `App::call`, `App::events` and `App::quit`,
with no HTTP, and A2's server routes to a `Backend` trait of the same shape, with a stand-in until the
two are joined. Whichever of A2 and A3 merges second joins them in `tc4`: a newtype implements
tc-server's `Backend` for tc-app's `App`, `tc4 serve` starts the `App` from its flags and routes the
API, the event stream and quit to it, ending the process after a reply whose `then` is `EndProcess`,
and `tc4_serve_answers_hello_with_real_state` proves it. If neither could, A9 or A4b, whichever starts
first, does it. The ring, `hello`'s state and the crash points before the store and the reply are
tc-app's (§4.1, §5.8); `tc4 serve` uses none of the server's copies of them. *(A3: the row's first
"done when", `tc4 serve` answers `hello`, needed A2. Done: A3 merged second, so its merge into
`phase0` joined them, in `crates/tc4/src/join.rs`. The server's stand-in, `Stub`, stays as the
backend of tc-server's own tests, which prove routing, security, streams and quit without an
application, and `tc4 serve` does not use it. The crash tests of the program stored a text with
`test/fieldStore` and read `test-hooks/fields.json` after the crash, because the stand-in's one
write, `app/clientError`, left a stderr line and the application's leaves a report in memory,
which a crash takes with it.)* A4a after A2; A4b after A3 and A4a;
A6b after A6a, A2 and A4a; A5 after A4b and A6b; A7 after A6b; A8 after A5; A9 after A2 and A3. G0 is written now,
with a stand-in server, and must report before B6 merges. B1a starts now on its storage branch
*(B1a: cut from `app` after unit C merged)*.

In phase 1, B1b comes first, after unit C and B1a, and B2 after it. B3 and B9b can run in
parallel after B2. B4a, B5a, B7 and B9a after B3. B4b after B4a. B5b and B6 after B5a, and B6
merges only after G0 has reported. B8 after B4a, B7 and A10. B10 after B9b. B11 after B4a, B5b and B6.

In phase 2, C0 starts as soon as S1 and B1b have landed, then C1a, then C1b. C6, C7a and C8 can
start after C1a; C2, C3, C4 and C5 after C1b; each also after the B unit it extends. C7b waits
for the owner's designs (Q-9). D, E and F start when their storage unit lands. G1 and G2 come
last.

---

## 11. Risks

1. **Unit C gates phase 1.** It is in progress. If it is slow, phase 0 still delivers the
   server, the shell, the harness and the gallery.
2. **The seed is a second §13.2 until unit I lands.** It is checked byte for byte, compared with
   unit I's seed, and then deleted. B1a runs the check now, so mismatches reach unit C while it
   is still changing tc-journal *(A-16)*. C1a recaptures every screen on disk. *(B1a: unit C had
   merged; the mismatches went to the storage team in
   `docs/evidence/state-seed-check-2026-10-18.md`.)*
3. **The aligner round trip** may miss 30 ms with large verses, or on Windows with Defender and
   Electronite's processes. A2 measures it in this container and G0 in Electronite on Windows
   before B6 merges; the fallback needs an owner ruling (Q-13), and the pure view builder
   makes it a switch *(A-14)*.
4. **Saves may wait behind long actor writes** (R9): a checkpoint, a merge commit, a 150 MB
   upgrade batch, or a draft write on the one writer. The UI never blocks, but the 25 ms budget
   can fail. R21 and R22 ask to move checkpoints' long work and draft writes off the actor;
   mode-switch checkpoints wait for R9's numbers *(A-11, A-20)*.
5. **Large replies.** The Psalms rail and book model are about 0.5–1 MB of JSON, 10–20 ms to parse
   on the UI thread. They are sent once per session view; later changes come by row and by
   chapter *(A-12)*. A slow laptop may still drop a frame when a screen first opens.
6. **DOM equality at screen scale.** Generated ids follow mount order; Leptos adds comment
   markers; virtualized lists differ above 400 rows. Normalizations and the threshold cover the
   fixture screens; rebuild-only DOM has no reference.
7. **Silent settle timeouts** would give screenshots that look valid. The event stream is excluded,
   `test/quiet` is added, and a screen that hits the deadline fails.
8. **Machine-dependent glyphs.** Both captures run on one image until CI has a pinned one. Each
   capture records its machine's fingerprint, and pixels are compared only between equal
   fingerprints (§7.3 layer 1), so a capture from another machine fails the gate instead of
   showing the machine's fonts as differences of the port.
9. **Header injection** may miss a request type in Electronite v37. G0 probes every type early
   and G1 again; the fallback is capability URLs.
10. **A panic** loses a navigation that was in flight when it happened. Typed text is not lost:
    `boot.js` writes each keystroke to the buffer before wasm handles it, a retry carries the
    body that was sent, and the reply cache matches it by id and digest *(A-2, A-15)*.
11. **Scripted saves do not change the fold.** A phase-1 test that saves and then reads again sees
    the old value. Such tests wait for C1a.
12. **A save in flight when the server process dies**, whose reply was lost, is not resent: its
    entry is offered at the next open unless the stored value already equals it (AD4). If the save
    landed and a teammate's change built on it arrived before the open, the offer shows text the
    person already saved. Keep makes a conflict the person did not need; Discard is right. This
    needs a server crash, a lost reply and a sync in one window, and nothing is lost *(A-3)*.
13. **Home summaries come from derived files.** After a crash before a materialization, or with a
    held file, a card shows the files' text until the project opens and its summary is rebuilt
    from the fold. Until C1a keeps them, summaries live in memory: each launch reads each
    project's derived files once, off the screen, in one background job at a time (B2;
    `docs/evidence/home-2026-10-19.md`).
14. **Two folders with one project id** share a token key and a prefs file. The scope check keeps
    tokens in their session; shared place prefs are acceptable.
15. **Side sessions.** A sync from Home beside an open whole Bible holds two folds (PERF:96, Q-15).
16. **Rebuild-only text** needs ten languages before release (Q-10).
17. **The view-state gallery** has hand-written fixtures where no recorded reply exists; a pixel
    difference cannot tell a fixture bug from a port bug. Recorded replies are preferred. On the
    tC4 side a case starts from a state recorded from tC4 itself, and calibration shows the
    gallery draws each recorded state as tC4 did; only the 49 patched cases of 68 hold values
    written for them, each built with tC4's own functions where one exists, and `cases.json`
    names the keys each patch sets *(A7)*. The carry-over counts of M9's set upgrades and
    re-pin are borrowed from the recorded gateway change, an operation that moves the same
    checks to other releases, because the mock holds no newer release to derive them from, so
    C8's replies for those cases carry the same borrowed counts, and a count that differs there
    is a fixture difference, not a port bug.
18. **Storage requests outside every task text.** R1, R4, R5, R14, R16 and R19 to R23 are in no
    storage unit's task text today. If they stay out, C0 is larger, R9's numbers may come late,
    and three fallbacks (R20, R21, R23) are weaker than the requests. Only the owner can add them
    (Q-23) *(A-5)*. *(C0, 2026-10-19: no longer outside every task: unit C provided R1 and R19,
    and S1 built R4, R5, R14, R16, R20, R22 and R23 (§9.2). R9 and R21 remain.)*
19. **Windows-only proofs.** G0, G1's tests and G2's Electronite capture need a Windows machine
    or runner (Q-26). Until G0 runs, the aligner budget, the header probe and the supervision
    design rest on reasoning, not measurement *(A-17)*.
20. **The shell's mirror** keeps a second copy of typed text in app data until it is saved or
    discarded. It never leaves the computer, and entries are removed when their field is clean;
    a file left by a crash is read at the next launch *(A-3)*.
21. **A save erases a newer draft of its field** (R26). Fixed by C5: the store preserves a newer
    differing draft before Ack, including an alignment with no browser mirror. The held-reply
    power-cut regression fails when the old unconditional removal is put back. A draft whose
    own write was refused remains best effort under STORAGE §5.14.
22. **A flaky no-loss property in tc-journal.** `mixed_minor_versions_converge` failed about one
    run in 35 on the third verification of C0, on assertions that a conflict was lost, or added,
    when some changes carry newer-minor fields; a lost conflict would hide a teammate's version.
    It is the storage team's (`docs/evidence/store-adapter-2026-10-19.md`); until it is fixed, a
    workspace test run that fails there is rerun once before the failure is read as a unit's
    *(C0)*.
23. **Every project opens read-only on a Linux or macOS computer with Dropbox installed** (R28).
    tc-store's cloud check takes any ancestor that holds `.dropbox` for a Dropbox folder, and
    Dropbox's settings folder `~/.dropbox` is such an ancestor of every folder under the home, so
    the open calls each project "copied by Dropbox" and no save is possible. tc-app's own copy of
    the check (`folders.rs`, kept by C1a) has the right rule for the first launch, but every open
    goes through tc-store's *(C1a, D6)*.

---

## 12. Questions only the owner can answer

| Q | Question | Recommendation |
|---|---|---|
| Q-1 | **Stable origin.** Every launch has a new port. The shell's mirror (AD21) now keeps typed text through any crash and across launches, so this no longer decides whether text survives *(A-3)*. It still decides whether the shell serves the UI from a privileged custom scheme (Electron `protocol.handle`) proxied to the server, which gives one origin across launches and lets the shell add the token header itself instead of filtering requests (AD11). G0 prototypes both. Under `protocol.handle`, Electronite v37 hands the handler a same-origin request without an `Origin` header, so the shell sets the server's own `Origin` on what it forwards (G0). Under both, every request crosses the main process's UI thread (AD11). | Decide after G0: the custom scheme if it adds less to the aligner's round trip than the filter, otherwise the filter. G0 times both in alternating passes; a difference inside the spread between passes does not decide. The UI code is the same either way *(A-14)*. **G0 (2026-10-17):** the passes overlap (filter 22.4-24.5 ms, protocol 22.7-25.5 ms painted p95), so the filter stays. **Decided (2026-10-17):** the filter (AD11). The owner was told that the choice is invisible to a translator and left it to the team. |
| Q-2 | **Feedback send.** tC4 sends from Electron main to SendGrid (`tC4:scripts/desktop-feedback.cjs:13`). ARCH:13-14 gives every network request to the server; AGENTS.md allows network only in tc-door43 and tc-resources. Options: (a) the shell sends, gated by the server's switch; (b) a mail client in tc-door43 with the key as a build secret; (c) no sending, keep "Copy the report". | (a): it keeps the key out of the server and reuses tC4's code. |
| Q-3 | **Keychain.** The `keyring` crate in the server, or Electronite `safeStorage` through the shell (`tC4:scripts/desktop-main.cjs:51-96`)? | `keyring` behind the `Keychain` trait; keep "no keychain on this computer". |
| Q-4 | **Projects folder.** A default under Documents that OneDrive redirects would open every project read-only (STORAGE:1387-1396). | `%USERPROFILE%\translationCore4\projects` (or `~/translationCore4/projects`), checked against the cloud detection at first launch. **Built in A3 (2026-10-18):** as recommended, with the local folder when a cloud client copies the default at the first launch, and kept when a default folder appears beside it later (§4.9). |
| Q-5 | **Checkpoint on every mode switch.** tC4's J8 asserts it (`tC4:e2e/j08-resume.spec.ts:194`); STORAGE checkpoints only on leaving Translate or the project (STORAGE:1723). Each checkpoint is an actor step of up to 500 ms on a whole Bible (STORAGE:2720, PERF:92), so a checker who switches after each decision would wait behind one each time. | Follow tC4 (R17) once unit P shows saves meet 25 ms while a checkpoint runs (R9, with R21 if needed). Otherwise checkpoint at a mode switch at most once every two minutes, which still gives J8 its commit at the first switch *(A-11)*. |
| Q-6 | **Alignment reflow after a verse edit.** tC4 writes the reflow at once (`tC4:src/state.jsx:7194-7220`), so a lossless reflow keeps the verse "aligned". The rebuild writes nothing (STORAGE:1482-1485), so the Align rail shows "needs review" until the aligner opens the verse and saves the lossless reflow. Accept, or let tc-app write lossless reflows as a second batch after the verse save (an automated writer, allowed by I4)? | Accept for now; measure how often it shows in J5 and J6. |
| Q-7 | **Open progress captions.** STORAGE has five phases (STORAGE:1873-1876); tC4's overlay has three captions and draws its two short stages as indeterminate bars (`tC4:src/views/OpenProgress.jsx:16-40`). Map reading and folding to the journal caption, finishing and checking to "Checking the project state", writing to "Preparing the project"; or add keys. | Map, with a rebuild key for "reading changes"; a stage shows tC4's indeterminate bar until it has a count *(A-22)*. |
| Q-8 | **After a sync:** STORAGE asks for a received-work sheet (STORAGE:2681-2684); tC4 shows the X10 toast. | The toast for Door43 sync; the sheet for Receive work from a file. |
| Q-9 | **Rebuild-only screens** with no tC4 design: review tabs beyond Text, History, Notices, the draft offer (including the offer of a draft whose token no longer verifies), the read-only copy offer, USB flows, the prep window, the unusable-folder card on Home (B2). Design them here from tC4's design system, or in tC4 first so the catalogs stay shared? | Here, signed off by the owner, listed under P7. C7b starts only after the sign-off *(A-17)*. |
| Q-10 | **Translations** of rebuild-only keys into nine languages: who, and does a release wait? | English fallback per key until translated; release waits, and `release_has_no_pending_translation` enforces it *(A-32)*. |
| Q-11 | **Parity resources.** Reproduce the baseline's substitutes and absences (the default), or recapture tC4 from `bundle/en` so more screens are complete? | Reproduce now; recapture at the v4.0.0 tag. |
| Q-12 | **Virtualization threshold** of 400 rows: whole DOM on the fixtures, virtualized above. | Accept. |
| Q-13 | **Aligner fallback.** If a click's round trip misses 30 ms, may tc-ui link tc-align's pure `Aligner`, an exception to ARCH:88? | Decide after G0's measurement in Electronite on Windows; A2's loopback number alone cannot show a miss the product would have. The pure view builder makes the fallback a switch *(A-14)*. **G0 (2026-10-17): does not arise;** the filter's painted p95 was 22.4-24.5 ms on the reference laptop. |
| Q-14 | **Settings from Home** show from files and open the project (with progress) only on Save. tC4 also opens a store only for the save, without making the project current (`tC4:src/state.jsx:6741-6744`), but a whole Bible's open can take up to 10 s. | Accept. |
| Q-15 | **Memory.** A sync or USB export from Home opens a side session; with another whole Bible open that is two folds against a 350 MiB peak (PERF:96). Allow, or make the second open wait? | Allow; G2 measures. |
| Q-16 | **Branches.** Phase 0 lands on main. After unit C, should storage merge into main so app units land on main, or should app units land on a branch cut from storage? Starting B1b–B4b on a merge branch before C finishes is possible but risks rework if C changes the pin types. | Merge storage into main after C; storage units merge into main as each lands. The seed spike B1a runs on a branch cut from `storage` now and is rebased when C merges *(A-16)*. |
| Q-17 | **Storage requests** R2, R3, R6, R7, R15, R17, R18, R20 and R24: provide them, or keep the fallbacks? | Provide R2, R6, R7, R15 and R20; R3 and R18 can keep their fallbacks; R17 follows Q-5 and R24 follows Q-24. |
| Q-18 | **tc-export** has no owner in the storage chain. The app track builds it (B8, D2). | Confirm. |
| Q-19 | **Help pages** vendored from tc-website `cdfdf9b`: who updates them, and where is the licence recorded? | A `LICENSE` record beside them; updated with each tC4 reference move. **Built in A5 (2026-10-19):** the record is the header of `crates/tc-ui/help-pages.txt`. tc-website holds no licence file at `cdfdf9b` (its root has none), so the record says that tC4 4.0 ships these pages inside its GPL-2.0-or-later build; the owner is asked to confirm that the rebuild may ship them under the same terms. `vendor-help.sh [REV]` updates them when the reference moves. The pages describe tC4 build b04c70f (v4.0.0-rc.1) and say so on every page; DELTA-10-19 Q31's question about that banner applies to these pages too. |
| Q-20 | **Verse editor content.** The rebuild edits STORAGE `words`; tC4 edits the trimmed raw body. *(B4a, tC4 #786: the first text said they differ only when a verse ends in a layout-only line (STORAGE §2.4), which is wrong. tC4's body stops at the verse's first paragraph marker, so its box holds only the first line of a poem verse, and a verse that opens on its own poem line (`\v 2 \q2 …`) opens empty and undrafted; a save then leaves the other lines behind. The rebuild's box holds all of `words`, every poem line with its markers, so nothing is left behind; the owner is told.)* | Accept and list. |
| Q-21 | **The `open-door43-refused` sentence** under Q3 is not decided (`parity/tc4-baseline/README.md:391`). | A new key without "make sure the repository is public". |
| Q-22 | **Test hooks** compile only under cargo features that release builds never enable. | Confirm that no hook may ship. |
| Q-23 | **Storage task texts.** R1, R4, R5, R14 (tc-store's counters), R16 and R19 to R23 are in no storage unit's task text (`.audit/workflows/storage-chain-B.js:19-22, 30-33`), and this design cannot change those texts. Add them? | Yes, now: R1 and R19 to unit C, which is editing tc-journal; R4, R5, R14, R16, R20, R22 and R23 to S1, with §4.2's contract cases as their tests; R21 to S2; R9 to P *(A-5)*. |
| Q-24 | **The Understand comment box over append-only comments.** tC4's box edits one note per target (`tC4:src/views/Understand.jsx:40-56`); STORAGE comments cannot be edited (STORAGE:1534-1541). Options: (a) tC4's box, where each blur after a change adds a comment and the earlier one stays (§6.5); (b) a box that always starts empty, with the unit's comments listed above it, a DOM difference from tC4; (c) R24, a record that revises the comment the box showed and keeps the old text in History. | (c) if the storage team agrees, else (a) *(A-18)*. |
| Q-25 | **A check comment being typed when the checker moves on.** tC4 drops it (`tC4:src/views/Check.jsx:815-825`); the rebuild saves it. Accept as a declared difference? | Accept: AGENTS.md forbids losing typed text *(A-10)*. |
| Q-26 | **Windows runs.** G0, G1's tests and G2's Electronite capture need Windows (PLAN:151-152). Who runs G0 on the reference laptop before B6 merges, and will the release pipeline have a Windows runner? | The owner runs G0 on the reference laptop; a Windows runner for G1 and G2 *(A-14, A-17)*. |
| Q-27 | **Unsaved aligner links in a crash.** Until R23 gives alignments a draft, a crash can lose up to 2 s of linking. Accept that in writing, or wait for R23? | Ask for R23 in S1; accept the 2 s window until it lands *(A-4)*. |
| Q-28 | **The engine P7 approves.** Both captures use Chromium 141; the product ships Electronite v37, on Chromium 138. Accept Chromium 141 as P7's engine, with a declared residual and one Electronite capture run per release? | Accept *(A-34)*. |
| Q-29 | **The rebuild's copyright line and LICENSE notice.** tC4's About shows the first line of tC4's `LICENSE` ("Copyright (C) 2017 - 2026  unfoldingWord …") and its License dialog shows `LICENSE` and `COPYING`. The rebuild's root has `COPYING` (identical to tC4's) and no `LICENSE`. Which copyright line and notice does the rebuild show? | tC4's, as A5 vendored them (`crates/tc-ui/assets/about/`), until the owner gives another. |
| Q-30 | **Where the copy of a project in an unsafe or synced projects folder goes.** When the projects folder itself is on a FAT32 or exFAT drive or copied by a cloud client, every project there opens read-only, and STORAGE's banner offers "Copy the project to this computer?". A copy into the same folder would be read-only for the same reason (C1b finding 4), so `session/copy` refuses it (`CreateRefusal::CopyUnsafe`) and the banner's Copy then explains why. The app lists one projects folder, so a copy elsewhere would be hidden from Home. Options: (a) ask for a folder and make it the projects folder from then on, moving the copy there; (b) keep refusing and tell the person to move the projects folder; (c) copy into the local projects folder of Q-4 and list it beside the other. | **Answered (owner, 2026-10-19, "Owner's answers"): (c).** The copy goes into the app's local projects folder (checked safe), with no folder to pick, and Home lists it; C6 built it (D9): `session/copy` writes into the local projects folder and refuses with `CopyUnsafe` only where that folder is unsafe too. |

---

## 13. Sources

- **Repository documents**: `AGENTS.md`; `docs/ARCHITECTURE.md`; `docs/PERFORMANCE.md`;
  `docs/PLAN.md`; `docs/TC4-DELTA-2026-10-16.md`; `parity/tc4-baseline/README.md`;
  `parity/ds-gallery/README.md`; `.audit/workflows/storage-chain-B.js`.
- **Storage spec**: `git -C /home/user/tc-rst show storage:docs/STORAGE.md` at `225091b`, byte-identical at `2dcfe6f` (lines up to 3,445 identical at `6bb20d4`).
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
- **Read again for the revision after the red team**: tC4 `src/views/SectionEditor.jsx:1-30,
  195-230`; `src/views/Understand.jsx:40-180`; `src/views/Check.jsx:805-906`;
  `src/views/OpenProgress.jsx:10-45`; `src/i18n/index.js:1-25` and the ten catalogs' sizes;
  `scripts/desktop-main.cjs:120-135`; `src/state.jsx:3690-3722, 3786-3860, 4018-4042, 4368-4380,
  7176-7225, 7280-7330`. Main: `crates/tc-check/src/rail.rs:110-175`;
  `fixtures/scale/psa/en_twl/twl_PSA.tsv` (7,683 rows, 485 distinct links); `docs/PLAN.md:105-170`;
  `docs/PERFORMANCE.md:72-100`; `docs/ARCHITECTURE.md:10-14`; `Cargo.toml:10-32`; `Cargo.lock`
  (`reactive_graph` 0.2.16). Storage `2dcfe6f`: `crates/tc-journal/src/command.rs:200-216`;
  `token.rs:54-77`; `fold/reads.rs:70-130, 350-392`; `fold/state.rs:236-262, 1818-1850`;
  `fold/project.rs:30-110, 496-625`; STORAGE §2.7, §5.0, §5.5, §5.14, §6.1 O12, §7.8 and table L.
  The task texts of `.audit/workflows/storage-chain-B.js:17-36`, searched for each request.

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
| RT-8 | Medium | No loss | Two opens of one folder at once make one of them fail. |
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

### RT-8 (Medium) Two opens of one folder at once make one of them fail

- **Where:** §4.3 first bullet; AD14; side sessions (§4.3, Q-15).
- **Evidence:** §4.3 makes an open wait only for a running leave-checkpoint. A second
  `ProjectStore::open` of the same folder finds `.tc/lock` held by this same `tc4` and is
  refused, with `tc4` itself as the holder (STORAGE §6.1 O1).
- **What breaks:** The person clicks a card while a Home sync's side session is opening, or while
  the Settings-from-Home save opens the project. One of the two is refused with a false holder,
  or the sync's open fails.
- **Change:** `SessionRegistry` keeps one in-flight open per folder. Later callers wait for it
  and share the session. Test both orders. A `.tc/lock` held by another process (a `tc4` left
  running after a crash, or another program) refuses the open with a message that names the
  holder, and nothing opens read-only for that reason (owner's ruling of 2026-10-18). The cases
  of another computer are unchanged.

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
  field stays `Saving` or is cleared. It happens after a reopen and after a server restart.
- **Change:** For a write, `StaleScope` becomes `NotSaved(StaleScope)`. The field keeps its text;
  the UI reattaches or reopens and resends with its base token. A browser tab cannot carry the
  per-launch token that the shell adds (§5.2), so outside the shell only the dev server and
  Playwright open the page, and tC4 removed its tab guard as dead code (D39). The one-tab check
  (a `BroadcastChannel` check) that this finding first asked for is removed by the owner's ruling
  of 2026-10-18.

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

### How each point was answered

Every finding was checked against the code before it was answered: tC4 at `d310bf3`, main at
`21fef4a`, storage at `2dcfe6f` (and `658ed0b`, which landed during the revision and changes
none of the sections used here, §0), and the task texts of `.audit/workflows/storage-chain-B.js`.
The main evidence of every finding held; two details did not, and the last column says so
(RT-5's counters, RT-18's account of tC4's comment box). All 34 are accepted; in nine of them one
part was done another way or not taken, and the last column says why, with the evidence. Each change is marked
*(A-n)* in the body, 283 marks in all. Only this file changed.

| RT | Answer | What changed | Not taken as proposed, and why |
|---|---|---|---|
| RT-1 | Accepted | AD20; §6.5 timing: the section editor saves only on Save, blur and the 2 s timer write only the buffer and the `Draft`, Cancel discards both, the #600 question stays; unit B4b with `section_editor_saves_only_on_save`; J2's section steps | — |
| RT-2 | Accepted | AD4; the buffer entry keeps `sent` (§6.6); the reply cache keys a body digest and waits for a request in flight (§4.4, unit A3); `RequestReused` (§3.5); a field is clean only when `Ack.saved` equals its NFC text (§3.4, §6.5); `reload_anywhere_stores_the_last_text_once` in A4b against the test-only `test/fieldStore`, and in C2 on real saves; two contract cases | — |
| RT-3 | Accepted, with a different mechanism | AD21: the shell appends every buffer change to `drafts-buffer.log` and fsyncs it once a second; AD39: the shell restarts `tc4` and reloads Home; `tc4` quits on a closed stdin; a new `tc4` waits 10 s for `app.lock` (§5.1); entries are offered at the next open (§6.6); AD4's in-place restart branch is removed; G1's crash tests; Q-1 rewritten | (1) "Decide Q-1 before A4": not needed. The mirror is kept under either answer, because Chromium may hold `localStorage` writes in memory before committing them, so even a stable origin would not keep the last keystrokes through a crash of the whole app. With the mirror, A4b's code is the same under both answers, and only G1 waits for Q-1. (2) Reading the old page with `webContents.executeJavaScript` when `tc4` exits: the mirror file already holds the entries and also survives a renderer crash, which `executeJavaScript` cannot read. (3) A pid-and-port record so a new shell can ask an orphan to quit: asking needs the old token. When the shell dies, the pipe to `tc4`'s stdin closes and `tc4` quits by itself; a new `tc4` waits 10 s for the lock and then names the pid. (4) "Keep AD4's restart branch only with a stable origin": removed in both cases. Going Home and offering the entries at the next open resends nothing automatically and has one code path. |
| RT-4 | Accepted | AD18 and AD22: the server owns the alignment save (2 s, leaving the verse, replace or close, before the quit checkpoint); `AlignSave` in §3.6 and §3.7; the indicator, navigation gate (`align/flush`) and close guard count it (§6.5); §5.1's quit saves links first; R23 and Q-27; B6, C5 with `links_saved_when_quitting_within_2_s` | — |
| RT-5 | Accepted, as an owner question | §9.2 gains an "In a task text" column for every request; Q-23 asks the owner to add R1, R4, R5, R14, R16 and R19 to R23 to the task texts; C0, a unit of its own, adapts S1's actor and builds R4, R5, R14 and R16 itself if S1 lacks them (§4.2); risk 18 | (1) "Put them into S1's task text now": this revision may change only this file, and the task texts belong to the owner's workflow, so it became Q-23. (2) R14 is half met already: `FoldState::counters` is public (`storage:crates/tc-journal/src/fold/state.rs:1830`); only tc-store's counters remain. (3) C0 cannot build two things around the actor: an O(1) snapshot and each command's `Delta`. Without them phase 2 waits, and §4.2 and C0 now say so. *(2026-10-19, C0: S1 built R4, R5, R14 and R16, the O(1) snapshot and each command's `Delta` in tc-store, so C0 is the thin adapter and phase 2 did not wait; §9.2 says where each is.)* |
| RT-6 | Accepted | AD37: an editor's value and token are one pair, replaced together only before any input or composition and never while focused; own-request `Changed` shows nothing; comparisons in NFC (§6.3, §6.4, §5.4); B4a's tests, including an Arabic IME case | — |
| RT-7 | Accepted, and taken further | AD3: the seam returns two types, `EditToken` and `ReviewToken` (§4.2), so the conversion slip the finding describes needs a type error; wire prefixes `st1.` and `rt1.` (§3.4); the DOM test looks for the prefixes (§6.4); R19 asks tc-journal for a kind under the MAC; three contract cases, one through HTTP | — |
| RT-8 | Accepted | AD38; §4.3: one open per folder, later callers share it; AD14; `opens_of_one_folder_share_one_session` in both orders; C6's card-click case; a `.tc/lock` held by another process refuses the open and names the holder, and nothing opens read-only for that reason (owner's ruling of 2026-10-18, §4.3) | — |
| RT-9 | Accepted | §6.6: a draft whose token fails its MAC is offered as text, "Keep it as a separate version" writes a change with no parents (R20), "Copy text" until then; `NotSaved::BadToken` no longer covers drafts (§3.4); `offered_drafts` returns `Unverifiable` (§4.2); buffer keys include the project id; C1b | — |
| RT-10 | Accepted | §6.5: a check comment saves on Done and on leaving the item, with a buffer entry and a `Draft` while open, declared as a difference from tC4 (Q-25); §6.4 and §6.6: the copy of a read-only project carries the folder's drafts; B5b, C1b, C4 | — |
| RT-11 | Accepted | AD34: mode-switch checkpoints are adopted only after unit P measures saves during a checkpoint (R9); R21 asks to keep the checkpoint's long work off the actor; Q-5 offers one mode-switch checkpoint every two minutes at most; D1; risk 4 | Splitting the checkpoint is a change to STORAGE's actor, so it is a request (R21), not a decision of this design. |
| RT-12 | Accepted | AD17: the rail arrives once per session view, then by row; `Ack.rows`, `check/rows { since }`, per-row `ViewSeq` in `CheckListCache` (§3.4, §3.6, §3.7, §3.9, §4.4); §6.2's exception; count tests `check_open_cached_sends_changed_rows_only` and `decision_refetches_no_rail`; B5a, C4 | — |
| RT-13 | Accepted | AD17 and §3.8: one-pass buckets by term index; `rail_groups_visit_each_row_once`; the differential test with `tc_check::rail::groups` stays | — |
| RT-14 | Accepted in part | AD11: the shell's header filter lists only paths outside `/api/`, so API calls skip the filter's JS listener (they still cross the main process's UI thread, as every request in Electronite does, so G1 keeps synchronous work off that thread); AD18: the UI queues aligner clicks, and the view builder is a pure function of tc-align and tc-api; G0 measures the round trip in Electronite on Windows before B6 merges; §8.3; Q-13 and Q-26 | (1) "Measure A2 through a header-adding proxy": with the filter, API calls do not pass a header hook, so that figure is now a second, pessimistic measurement, kept in case G0 finds the filter cannot leave `/api/` out. (2) HTTP/1.1's six connections: no change. The event stream holds one; the click queue sends one aligner request at a time, so the aligner needs one of the five others. G0 measures with OBS pictures and help pages loading. (3) The Electronite run "before B6 starts" becomes "before B6 merges", for the reason given under RT-17. |
| RT-15 | Accepted | AD30, AD22, §6.8: `boot.js` reloads and counts page loads that ended in a panic; the callout wins over restoring the editor, with fields kept dirty; the static callout posts drafts with `fetch`; a `boot.js` capture listener writes each keystroke first (§6.6); `try_` forms and `clippy::indexing_slicing` (§6.5); A4b | — |
| RT-16 | Accepted | B1a, the seed spike, starts now on a branch cut from `storage`; B1b holds the seam, the memory backend, sessions, probes and counters. The reply cache moved to A3, and each screen's `wire` conversions moved to that screen's unit; Q-16 | — |
| RT-17 | Accepted, with one ordering change | A4a and A4b; A6a and A6b; B4a and B4b; C7a and C7b, with C7b after Q-9's sign-off; E1a and E1b; G0, the Electronite spike; A9 names its tc-resources work; B8's crate became A10 and B9a's tc-door43 streaming became A11, both in phase 0. The re-check also split C1 into C1a and C1b and moved the shared models from A1 to the units that use them (§10) | "Run the spike before A4 and B6": G0 must report before B6 merges, not before A4 or B6 start. A4's code does not depend on the header probe, because the UI's own `fetch` sets the header (AD11), and the mirror (A-3) takes Q-1 off A4's path. B6 can start first because the pure view builder makes Q-13's fallback a switch, and gating its start would also hold B11, C5, C7a and E1a behind a Windows run. |
| RT-18 | Accepted in part | §6.5: a user comment saves on blur, and a pause writes only its draft (R15); the box's rule and the clear refusal are written down; R24 and Q-24 offer a way to revise a comment; C3's tests | The box rule as worded, "the newest note of the unit plus a count of the others", is not tC4's. `displayedUnitNote` shows the note at the unit's head verse, or the unit's only note, or none, and says "never a timestamp pick" (`tC4:src/views/Understand.jsx:40-56`). The design keeps tC4's rule and takes the newest only where one target has several comments, which tC4's per-target notes could not have. |
| RT-19 | Accepted | `NotSaved::StaleScope` for writes (§3.4, §3.5); the field keeps its text and resends with its base token (§6.5); no tab guard in a browser, because the owner's ruling of 2026-10-18 removed the one-tab check (§6.2); A1, A4a, A4b | — |
| RT-20 | Accepted | R22 asks that a draft write never delay a command; `saves_waiting_behind_a_draft_write` is counted (§8.1, §8.2); `lastEdit` is recorded on save and blur and written after 5 s or at leave (§4.9); risk 4 | — |
| RT-21 | Accepted, and taken further | §4.3: no `ProjectIndex`. Verses come from `chapter_usfm`, alignments and decisions from probes, detached decisions from `FoldState::detached()`, and a book's comments from its projected comments file; `probe_finds_every_key_a_scan_finds`; `open_lists_no_key`; §4.4, §4.7, §8.1; R1 stays as the cheaper path | The proposed source, "verse and alignment keys from the versification", would miss a verse the project has beyond its versification, and so hide work. `chapter_usfm` holds every verse the chapter projects. The proposed "scan only for comments and detached records" is still a whole-project scan; the comments file costs one book, and `detached()` already exists (`storage:crates/tc-journal/src/fold/reads.rs:215`). |
| RT-22 | Accepted | §6.3: a port of `OpenProgressView` with its own 300 ms timer; tC4's three stages with indeterminate bars until a stage has a count; Q-7; B3 | — |
| RT-23 | Accepted | §6.9: all ten catalogs (943,433 bytes in tC4) at boot; A4a | — *(A4a: English is compiled in and the other nine, 870,974 bytes, are fetched before the first render; a failed fetch shows the boot error.)* |
| RT-24 | Accepted | §6.11: two fixed heights, the tab-stop row kept mounted, computed scroll offsets, `check-keyboard` on Psalms; B5a | — |
| RT-25 | Accepted | `home_costs_projects_not_history` runs on the store only (§4.2, §8.2, C1a); the screen session closes at Home with tC4's leave checkpoint (AD38, §4.3, J8) | — |
| RT-26 | Accepted | §6.5: the navigation gate also covers going Home and the gateway-change and upgrade previews; C8's test | — |
| RT-27 | Accepted | §4.8: confirming buttons are disabled in flight and one dialog sends one `RequestId`; B11, C6, C7a | — |
| RT-28 | Accepted | §5.5: exchange uploads up to 4 GiB, the 512 MiB cap kept for other uploads and for each zip entry; A2's tests; E2 | — |
| RT-29 | Accepted | §3.6 and §4.6: `Train` and `HomeSummary` progress at most once a second; A3's test | Sending progress "only to screens that asked" was not taken: one rate per job kind needs no subscription state. |
| RT-30 | Accepted | `Stored`, `ChangeScope::{Parts, Everything}` with `Aspect`, `BankSelection`, `Freshness`, `AlignSave`, `ProjectOrigin`, `Door43Card` (§3.4, §3.6, §3.7) | `VerseRow`'s `drafted` and `para` stay booleans: they are facts about the text, not states. |
| RT-31 | Accepted | AD31: USB export and receive one at a time per project and never beside that project's sync; A3's `receive_and_sync_of_one_project_never_overlap`; E2 | — |
| RT-32 | Accepted | §3.5: untranslated rebuild keys are listed in `<id>.pending.json`, and `release_has_no_pending_translation` fails a release build; A1, A9, Q-10 | — |
| RT-33 | Accepted | §5.7: same-origin links open as a child window of the app's session, other `http` and `https` links in the system browser; G1's test | — |
| RT-34 | Accepted | §7.3: a declared residual and one Electronite capture run per release in G2; Q-28 | — |

**The work breakdown, checked again** *(A-16, A-17)*. The revised §10 has 48 units, against 37
before. Each was checked on four points:

- **One agent.** No unit now holds two subsystems. The splits are B1, A4, A6, B4, C7, E1 and C1.
  New crates and changes to domain crates are units of their own (A10, A11) or named in the scope
  (A9). Each screen's `wire` conversions and view types are built by that screen's unit.
- **Verifiable.** Every "done when" is a test, a report row or an evidence file this container
  can produce. G0 needs Windows, and G1 and G2 need a Windows runner for their final runs; the
  table says so, and Q-26 asks who runs them. A4b can prove its no-loss rules before any real save
  exists, through A3's test-only `test/fieldStore`.
- **Ordered.** Every "depends on" names a unit earlier in the order, or a storage unit; owner
  answers and storage requests sit under "waits for". A script over the tables found no unknown
  name and no cycle among the 48 units. A5 now waits for A6b, whose gate its "done when" uses;
  B6 may start before G0 but merges only after G0 reports.
- **Honest about storage.** A new "Waits for" column names each storage request a unit needs and
  what it does without it. Phases 0 and 1 write no project. C0 is sized by what S1 provides, and
  the two things it cannot build are named. Create waits for R2 with no fallback. C7b waits for
  the owner's designs.

The no-loss units (A4b, B4b, C0, C1b, C2, C3, C5) are marked and get PLAN's rigor: property
tests, crash injection and an adversarial review before they merge.
