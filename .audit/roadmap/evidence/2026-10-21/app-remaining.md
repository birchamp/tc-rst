# app-remaining

## Summary
The app layer has 38 of 48 units merged on resume-build-lane at f9067bc. Main has 29. B11 is merged (f07fd9e), but its records are unfinished: STATUS.md:56 still says "ready to start", RESUME-2026-10-20.md has no B11 checkpoint, two B11 minors are uncommitted, the lane worktree is still there, and no post-merge gate is recorded.

Ten units remain:
- **Ready now:** C7a.
- **Needs the owner:** C7b waits for the Q-9 design sign-off.
- **Waits for S2:** D1 and D2. S2 is on s2-proof-executor, built on an old base. A dry-run merge into the lane conflicts in 13 files.
- **Waits for X or I:** E1a, E1b and E2 wait for X, and F1 waits for I. Neither crate exists on any branch. Their UI and port seams can still be built now on the scripted fallbacks (R10, R11).
- **Shell:** G1 can mostly start now. It is too big for one unit; split it in three.
- **Last:** G2 comes at the end, but its CI half should be built first.

Three things block PLAN done no matter how fast units land:
- All 200 browser journeys skip in CI, so P1, P2 and P7 have no CI check that can fail.
- The release-only translation gate never runs in CI, and 155 of 156 rebuild keys are untranslated in nine languages.
- P6 (Aquifer helps in the app) has no owning unit.

Other risks:
- The Dropbox bug (R28) is still in all three branches.
- E1a's OS keychain needs the `keyring` crate, which is not in the offline cargo cache.
- tC4 has moved 98 commits since the last issue sweep. This touches D2, E1a and F1.

The critical path is set by storage: S2 → P → T → X → I → V. The app units on it are D1 → E1a → E1b, then F1, then G2. Q-9 gates C7b, and C7b gates P2's J26 check-conflict step.

# App layer: remaining units to done

Snapshot `snap/resume-build-lane` (= `resume-build-lane` @ f9067bc, "lane") unless marked `s2:` (`snap/s2-proof-executor`). Layers: **Code**, **Checks**, **Prose**, **Human**. Nothing was built or run; all sizes are inferences from git history.

## 0. State

| Fact | Evidence |
|---|---|
| 38/48 app units on lane; main 29 | `.audit/plans/HANDOFF-resume-lane.md:20`; `PROGRESS.md:13` |
| B11 merged at f07fd9e (2026-10-21 11:56); rl-b11 2fdd3f3 is an ancestor | `git merge-base --is-ancestor rl-b11 resume-build-lane` → true |
| B11's records are unfinished | `STATUS.md:56` "ready to start" (generated at 3eac396, before the merge); `RESUME-2026-10-20.md` has 0 "B11 checkpoint"; `rebuild.tsv` ends 2026-10-20T11:30; lane worktree `git diff --stat` shows `.audit/minors.tsv +2` (B11 minors); `git worktree list` still shows `target/lanes/b11` |
| B11's "done when" is met (J18's UI steps, scripted) | `.audit/plans/B11.md` "Check after the usage-limit stop" |
| No `tc-sync` or `tc-import` crate exists | `ls crates` in all 4 snapshots |
| 200 `browserTest`s in 29 files; all skip in CI | `parity/app/journeys/kit.mjs:22-23`; `.github/workflows/ci.yml:59-63` |
| Parity: 130 rows: 33 same, 79 differ, 18 unported, 0 failed; only 6 `ar` rows | `parity/app/report.json` totals |

**Size bases (merge on lane: commits, +lines in crates and parity, span):** B11 f07fd9e: 11, +6,904, 15 h. C4 1a5d5d1 + 3eac396: 9, +2,926. C8 112f839: 9, +12,672, 2.7 h. C6 acba162: 10, +6,728, 2.1 h. B8 cad4961: 5, +4,319. B9b 6b1bb72: 6, +4,867. B10 5f7bc9a: 4, +3,885. C1b 8c36fef: 13, +6,737, 5 h. C0 a52bc0c: 23, +6,372, 4.5 h. A6b 45d71dd: 7, +6,737. G0: 11 commits, 2026-10-17 11:23→19:10. Line counts include goldens and captures.

## 1. Units

### B11 close-out (do first, before C7a)
Commit the 2 minors. Regenerate STATUS (`.audit/tools/status.py`; its regex counts "Merge B11:"). Add the B11 checkpoint to RESUME. Run `git worktree remove target/lanes/b11` and keep the branch. Carry B11's minor "no test covers the Alignments/Checks tabs or the markers" into C7b step 1.

### C7a Conflict choices: text and passage (APP-DESIGN.md:3396)
- **Scope:** in the review, every Text-tab kind can be resolved (verse structure, passage, structure issue, book header, chapter front). Restore from History. Bulk keep. A look-again refusal through the UI. The double-submit rule for every new choice.
- **Today:** `entry_keeps` allows only `VerseText` (`crates/tc-ui/src/views/review.rs:298-301`); every other kind gets `later_bar` (:925-945). `Resolve.keep: ChangeRef` (`tc-api/src/conflicts.rs:382`) cannot express passage choices. Storage already does all of it: `Command::{Resolve, ResolveBulk, Restore}` (`tc-store/src/commands.rs:192-205`) and `Choice::{KeepPassage, KeepSeparate, KeepJoined}` (`tc-journal/src/fold/resolve.rs:87-110`). LookAgain already maps to `OTHER_KEPT` (`tc-ui/src/app/web/review.rs:19-26`).
- **Depends on:** C1a and B11, both done. R7 is met (`ResolveBulk`, `tc-journal/src/command.rs:111`).
- **Owner questions:** Q-9 covers History and any bulk button. It is answered by recommendation (APP-DESIGN.md:39): designed here, the owner signs off at P7.
- **Proves:** P1 and P2's J18 (local); P3's "a conflict the user can see". Tests (proposed names): `keep_passage_resolves_every_record_of_the_group`, `restore_writes_over_every_shown_head`, `bulk_keep_leaves_out_conflicts_with_two_candidates`, and J18 local in `conflict-review.test.mjs`.
- **Risks:** a passage choice that writes one record of a joined span with a stale token would overwrite a teammate's verse. Rule: one review token per record. Restore must not hide a newer head. LookAgain needs a reachable trigger (inference: none before D1's outside edits or E1a's sync).
- **Size:** 8-12 commits, 3-4 sessions. Basis: B11 and C4.

| # | Goal | Files | Done when | Layer |
|---|---|---|---|---|
| 1 | Replace `busy: bool` + `sent` + `outcome` with `enum KeepPhase {Idle, InFlight(SentKeep), Answered{..}}`; `SentKeep.change` → `choice` | `tc-ui/src/app/review.rs:21-46`, `app/web/review.rs`, `tests/app/review.rs` | B11's 13 host tests pass; wasm clippy `--features app` | Code |
| 2 | `enum KeepChoice {Candidate, Passage, Separate, Joined}`; `conflicts/restore`; `conflicts/resolveBulk` | `tc-api/src/conflicts.rs`, `endpoint.rs`, `tests/golden/endpoints/conflicts/*` | `cargo test -p tc-api`; tc-api wasm clippy | Code |
| 3 | One entry point, `App::choose(conflict, review, KeepChoice)` → `Command`; restore; bulk; reply cache | `tc-app/src/conflicts.rs`, `tests/app/conflicts.rs`, `tests/op_counts/conflicts.rs` | new cases pass on both backends; `conflict_tab_costs_its_conflicts` unchanged | Code |
| 4 | Passage footer; Restore in History; bulk button as a gallery case for Q-9 | `views/review.rs`, `assets/i18n-rebuild/*.json` | host tests; wasm clippy (none, gallery, app) | Code |
| 5 | Journeys: passage keep from a scripted sync, Restore, double press | `parity/app/journeys/conflict-review.test.mjs` | green 3 runs; screen gate | Checks |
| 6 | PLAN declared differences (B11's minor + C7a's); TC4-ISSUES #78, #25, #679; STATUS and RESUME | docs, .audit | — | Prose |

### C7b Conflict choices: the other kinds (:3397)
- **Scope:** choices for check, moved check, alignment, settings and pins, book presence, comment and carried file (rename). Detached → Dismiss. ReleaseBatch. Notice dismiss. The received sheet's view (Q-8, answered by recommendation: the sheet is for Receive work from a file).
- **Depends on / waits for:** C7a. **Q-9 sign-off: not given** (`HANDOFF.md`, "What needs the owner"). R7 is met: `MoveCheck`, `Dismiss`, `ReleaseBatch` (`command.rs:119-143`), `Choice::RenameCarried` (:284).
- **Proves:** P2's J26 check-conflict step (with E1a); P3 for every kind; P7 for the rebuild-only screens.
- **Exists:** read-only tabs, 23 `ConflictKind`s (`tc-api/src/conflicts.rs:115-141`).
- **Risks:** a pins or settings choice that moves resource pins silently; a carried-file rename that drops one file. These kinds arrive only by X's sync or USB (B11.md "Reachable"), so tests use folds built by tc-journal.
- **Size:** 12-16 commits, 5-7 sessions, plus the owner's wait. Basis: B11 and C8.

| # | Goal | Files | Done when | Layer |
|---|---|---|---|---|
| 1 | Read-side tests for the non-Text tabs and the markers, on tc-journal folds | `tc-app/tests/app/conflicts.rs`, `tc-ui/tests/app/review.rs` | a test fails if `tab_of` (`tc-app/src/conflicts.rs:37`) remaps a kind | Checks |
| 2 | One sign-off packet for **every** Q-9 screen: review tabs, History and Restore, bulk, draft offer, read-only banner, first-launch failure, boot error, USB flows | gallery captures in `docs/evidence/q9-<date>/` | `ruling` line in `rebuild.tsv` | Human |
| 3-6 | One kind group per session: (a) check and moved check; (b) alignment; (c) settings, pins, presence; (d) comment, carried file, Dismiss, ReleaseBatch, notices | `tc-api/conflicts.rs`, `tc-app/conflicts.rs`, `views/review.rs` | `one_resolution_per_kind_through_the_ui` slice per group | Code |
| 7 | DOM equal to the signed-off designs | `parity/app/*`, `tc-ui/tests/screen_record.rs` | screen gate | Checks |

### D1 Checkpoints and outside edits (:3404)
- **Scope:**
  - The real checkpoint commit and its outcome (Committed, Unchanged, Failed).
  - Retry, and the failure banner on Home (the gallery case `home-checkpoint-error`, unported).
  - Mode-switch checkpoints per Q-5.
  - The last outcome and blocked paths in `OpenSession`, so a Resync shows them again.
  - The materialize-blocked message.
  - On window focus, the outside-edit toast with Undo; held-file actions.
  - Quit order: aligner links, then checkpoints.
- **Today:** S1's checkpoint seals and materializes without a commit (`tc-store/src/actor.rs:220-237`). The event types already exist (`tc-api/src/event.rs:81-83`, `:234`). Missing endpoints: `session/{leave, focus, notices, dismissNotice, retryCheckpoint}` (none in the goldens).
- **The S2 branch provides:** `checkpoint() -> CheckpointReport`, with the work off the actor (R21) (`s2:crates/tc-store/src/handle.rs:916-931`); `focus`, `take_in_held` (:838-855); `UndoOutsideEdit` (`s2:commands.rs:206`).
- **Depends on:** S2 (unmerged), C2, C5. **Waits for:** R9 (P, unmet) to settle Q-5; until then, Q-5's fallback of at most one mode-switch checkpoint every 2 minutes. R17 follows Q-5.
- **Proves:** P1's J8; P4's leave and quit points; P5 (commits checked by tc_burrito in S2). Tests: J8 "one commit when leaving with changes, none without"; `a_save_makes_no_commit` (#536); `save_latency_while_a_checkpoint_runs`.
- **Risks:** quit order (RT-4). Undo must take back only this device's changes (`s2:commands.rs:43`). A lost `Checkpoint(Failed)` event would hide "Checkpoint not saved" (§5.4).
- **Size:** 12-18 commits, 5-6 sessions, plus 1-2 sessions of integration. Basis: C1b and C0.

| # | Goal | Files | Done when | Layer |
|---|---|---|---|---|
| 0 | Merge S2 into the lane. 13 conflicts (`git merge-tree`): tc-store `handle.rs`, `hooks.rs`, `open.rs`, `tests/store/{commands,drafts}.rs`, `Cargo.toml`/`.lock`, `ci.yml`, 5 .audit files. Keep the lane's 36 tc-store/tc-journal changes (+2,291/−357 since 7e62455: draft machine, creation pins, AddBookText, R26) | — | full gate | Human (who merges) + Checks |
| 1 | Map `CheckpointReport` to `CheckpointOutcome`; `enum CheckpointState` per session | `tc-app/src/backend/{mod.rs:1133, store.rs:1389, memory.rs:723}`, `session.rs:700-745` | store_contract cases; no commit when nothing changed | Code |
| 2 | Outcome and blocked paths in `OpenSession`; `retryCheckpoint` | `tc-api/src/{session,event}.rs` + goldens | Resync tests after a failed checkpoint and a blocked file | Code |
| 3 | Focus check, toast with Undo, held-file actions, notices | `tc-app/src/session.rs`, `tc-ui/src/app/effects.rs` | another program edits a book file → toast → Undo restores | Code |
| 4 | Pure `checkpoint_due(policy, last, now, reason)` | `tc-app/src/session.rs` | unit tests; latency row once P reports | Code + Human (Q-5) |
| 5 | J8 journey; `home-checkpoint-error` capture; quit journey | `parity/app/journeys/` | journeys and gate | Checks |

### D2 Exports (:3405)
- **Scope:**
  - Producers: aligned USFM, plain USFM, OBS Markdown, print HTML. The Burrito zip producer is shared with E2.
  - `export/{preflight, run}` jobs and a `GET /print/{job}` route (absent: `tc-server/src/lib.rs:338-349`).
  - Warn while conflicts are open; checkpoint first (D1).
  - tC4 rules #796, #409, #553.
- **Exists:** `tc-export` (750 lines: book, page, stories, text); `weave_writes_what_tc4_writes` (`tc-usfm/tests/tc4_weave.rs`); the downloads registry; the export menu, off until D2 (`tc-ui/src/app/print.rs:236-249`).
- **Depends on:** S2, B8. **Waits for:** none.
- **Proves:** P1's J7 (without the zip, which is E2's) and J23; P5. Named tests (TC4-ISSUES:127): `export_reports_and_keeps_a_verse_that_needs_review`, `export_keeps_inline_zaln_bytes_even_with_a_matching_record`, `a_record_with_nothing_placed_exports_the_stored_bytes`.
- **tC4 moved after the sweep:** e2bfd8c3 and 50cfec3e (#796: at most 20 skipped verses listed, with the total).
- **Risks:** an alignment dropped silently from an export; a write into the project (J7 says MUST NOT).
- **Size:** 8-12 commits, 4 sessions. Basis: B8 and C6.

| # | Goal | Done when | Layer |
|---|---|---|---|
| 1 | Pure producers in `tc-export/src/{usfm,markdown}.rs` returning `(bytes, ExportFacts)` | proptest round trip through tc-usfm; the #796 tests | Code |
| 2 | API, job and `/print` route | goldens; `every_endpoint_is_routed_once` | Code |
| 3 | `every_export_validates` (`tc_burrito::validate_dir`) | test | Checks |
| 4 | Menu on, toasts | host tests; wasm clippy | Code |
| 5 | J7 and J23 journeys with `page.pdf()`; project bytes unchanged | journeys | Checks |

### E1a Sync (:3411)
- **Scope:**
  - A real `SyncPort` over X.
  - An OS keychain (Q-3, answered by recommendation: the `keyring` crate).
  - The sync-button flow; R13's dropped connection.
  - J26 with three app-data folders.
  - Recapture `shared-home`, `sync-failed`, `sync-ok`, `sync-ok-teammates` and `sync-ok-restored`.
  - tC4 #801, #761, #724, #24, and #791 (D108 shipped in tC4 at 43164cbe).
- **Today:** production answers `Unavailable(Sync)` (`tc-app/src/sync.rs:147`). `ScriptedSync` is test-hooks only (`app.rs:195-197`). There is only `MemKeychain` (`account.rs:190`). `keyring` is not in `~/.cargo/registry/cache`, and cargo runs offline.
- **Depends on:** X, B10, C7a, D1. **Waits for:** R13 (unmet).
- **Proves:** P2's J26 (and J18 through its step 3); P1; P4's sync from the app.
- **Risks:** a teammate's journal file lost; a forced push; a save during a sync.
- **Size:** 12-16 commits, 5-6 sessions. Basis: B10 and C0.

| # | Goal | Done when | Layer |
|---|---|---|---|
| 1 | **Now:** a `SyncPort` trait as the one entry point; `ScriptedSync` (hooks) and `Unavailable` implement it | the existing sync tests are unchanged | Code |
| 2 | `OsKeychain` behind `Keychain` | owner approves fetching the crate, or the shell's `safeStorage` | Human, then Code |
| 3 | `Door43Sync` over tc-sync | tc-app tests against tc-door43-fake | Code |
| 4 | Dropped-connection fault in `tc-door43-fake` | `sync-failed` journey | Checks |
| 5 | J26 with 3 folders; `no_door43_request_at_launch_or_60_s_idle` | journeys | Checks |

### E1b Share and open from Door43 (:3412)
- **Scope:** share create, push and done (the team-access line, #724); open with sign-in on 401 or 404; Home's words for a join cut short; creation pins at first open (#766); J11, J17, J24; recapture 3 screens and port the 4 unported `share-*` cases.
- **Exists:** B9b's steps up to the check; `ScriptedOpenDoor43` (`share.rs:145`).
- **Depends on:** X, B9b, E1a. **Owner questions:** none (Q-21 answered).
- **Proves:** P1's J11, J17, J24; P2's J17.
- **Size:** 8-10 commits, 3-4 sessions. Basis: B9b.
- **Steps:**
  1. **Now:** a `door43/share` job on a `SharePort` with a scripted implementation; the progress and done UI; the 4 cases (Code).
  2. The real port (Code).
  3. J11 and J24 journeys (Checks).
  4. J17, including private-repo sign-in and the join cleanup (Checks).
  5. Recapture (Checks).

### E2 USB exchange (:3413)
- **Scope:** Save work for a teammate (folder, progress, verify, then "You can remove the stick"). Receive work from a file (identify, confirm, job, report; the Received sheet survives a Resync). The Burrito zip. Never beside a sync.
- **Exists:** `FlowRequest::Exchange{Export,Receive}` (`flows.rs:19-27`); `receive_and_sync_of_one_project_never_overlap` (`tests/app/flows.rs:25`); the exchange upload cap (`tc-server/src/uploads.rs:237`). No UI yet.
- **Depends on:** X, C1a, D2, and C7b's sheet.
- **Proves:** P2's USB journey both ways and the relay (STORAGE:3819); the zip half of J7; P5.
- **Risk:** "remove the stick" shown before the verify.
- **Size:** 8-12 commits, 4 sessions. Basis: C6.
- **Steps:**
  1. **Now:** an `ExchangePort` with a scripted implementation, and the UI (Code).
  2. The real port (Code).
  3. A journey both ways with edits on both sides, plus the relay (Checks).
  4. Validate every exchange file (Checks).

### F1 Import (:3419)
- **Scope:** every M5 step; the tC3 version lookup; name clash; progress and cancel; toast and badge; an import that was cut short is removed and Home says so; J9a, J9c, J9d. Replace the seed: once `memory_seed_equals_tc_import_seed` passes, delete `tc-app/src/backend/seed*`. tC4 #577, #499, #502, #553, #766, and #813/D110 (tC4 tip 0b4cb270, not yet in TC4-ISSUES).
- **Exists:** M5's first step only (`tc-ui/src/views/modals/import.rs:1-2`); 6 unported import cases.
- **Depends on:** I, C1a. **Waits for:** R11 and R29 (both unmet).
- **Proves:** P1's J9; P9 together with I.
- **Size:** 10-12 commits, 4-5 sessions. Basis: C6.
- **Steps:**
  1. **Now:** an `ImportPort` with `ScriptedImport` (R11's fallback); the M5 steps and 6 cases (Code).
  2. The real port (Code).
  3. J9 journeys (Checks).
  4. The seed equality test, then delete the seed (Checks).

### G1 Electronite shell (:3425). Split it.
- **Exists:** G0's spike: `spikes/electronite/shell/main.cjs` (535 lines), `lib.cjs` (202), `lib.test.cjs` (110), `run-spike.ps1`; the page side of the bridge (`tc-ui/src/app/web/bridge.rs`, `app/boot.js`, `app/no-loss.mjs`); `crates/tc4/tests/lifecycle.rs`; 7 unported `feedback-*` cases.
- **Depends on:** A2, A4b, A9, C1b, G0 (done); D2 for `printToPDF` only.
- **Owner questions:** Q-1 decided (the filter); Q-2 answered (the shell sends feedback); Q-3 per E1a.
- **Proves:** P8's Graphite probe on Windows and macOS; P4's shell crash (end `tc4` while typing, relaunch, the text is offered).
- **Design conflict:** the owner ruled a window within 1 s and no splash (`rebuild.tsv:94`), but APP-DESIGN.md:1727 opens no window on a repeat launch. TC4-ISSUES:344 says to fix §5.3.
- **Risks:**
  - The mirror loses keystrokes if its appends block the main thread.
  - R28: every project opens read-only on macOS or Linux with Dropbox installed (`tc-store/src/platform.rs:136-138`, the same in all 3 branches).
- **Size:** 20-30 commits. Basis: G0 (11 commits in 8 h) and A5 (21).
- **G1a (now):** start, supervise, header filter, mirror, single instance, close guard. Pure decisions go in `shell/lib.cjs` with `node --test`; it runs on Linux under Xvfb (Code + Checks).
- **G1b:** PDF, dialogs, `openExternal` same-origin rule, feedback send, the 7 cases (Code).
- **G1c:** installers, signing, static CRT, notices, build commit, window frames, zoom keys, and `cargo test --release -p tc-api --test refusals` in the release pipeline (Checks + Human: certificates, Windows runner).

### G2 Budgets and the P7 report (:3426)
- **Today:** no `appbench`. No `app-e2e` job: every journey skips in CI. `release_has_no_pending_translation` asserts only under `--release` (`tc-api/tests/refusals.rs:599-617`), and CI runs debug. Each of 9 `*.pending.json` files lists 155 of 156 keys.
- **Depends on:** P and every unit. Q-28 is answered by recommendation (Chromium 141).
- **Size:** 10-15 commits. Basis: A6b.
- **Steps:**
  1. **G2a now:** an `app-e2e` CI job, where a skip fails when `TC4_REQUIRE_BROWSER=1`, and a P1 matrix test so every live J-id maps to a journey file (Checks).
  2. `crates/tc-app/examples/appbench.rs` (Code).
  3. Playwright perf specs and memory on the whole Bible (Checks).
  4. `ar` captures for the 62 screens (Checks).
  5. A human reviews the residuals (Human).

## 2. Storage requests still unmet (APP-DESIGN.md §9.2)

| R | Need | Unit that needs it | State |
|---|---|---|---|
| R9 | Save latency during checkpoint, materialize, merge, upgrade | D1 (Q-5), G2 | in P's task text (storage-chain-B.js:40); not measured |
| R10 | Sync, share and join entry points with progress and cancel | E1a, E1b, E2 | in X's text; no tc-sync |
| R11 | Import dry run, job, tC3 lookup | F1 | in I's text; no tc-import |
| R13 | The fake's dropped connection | E1a | "no" (§9.2:3266) |
| R17 | Checkpoint at every mode switch | D1 | follows Q-5 and R9 |
| R21 | Checkpoint work off the actor | D1 | built only on unmerged S2 (`s2:handle.rs:916-919`) |
| R28 | Dropbox marker rule | G1 (release); every open | unfixed in 3 branches |
| R29 | A public `project.json` writer | F1 / I | absent in 3 branches |
| R30 | `ProjectHandle::is_idle` | G2 capture settle | absent in 3 branches |

## 3. Critical path to PLAN done
1. **Lane:** close out B11 → C7a → batch gate → fast-forward main (needs the owner: HANDOFF.md allows pushing main, but HANDOFF-resume-lane.md:7 forbids it).
2. **Owner:** Q-9 sign-off → C7b, which P2's J26 step needs.
3. **S2:** S2 proof → S2 merged into the lane (13 conflicts) → D1 and D2 in parallel.
4. **Storage chain:** P (R9) → T → X → E1a → E1b; E2 after D2.
5. **Import:** I → F1; V gives P10.
6. **Shell:** G1a and G1b now, G1c on the Windows runner.
7. **Last:** G2a now; G2 last, then the human P7 review.

The longest chain is storage-bound: S2 → P → T → X → I → F1 → G2.

## 4. Gaps outside these units
- **P6:** no unit installs Aquifer helps in the app (APP-DESIGN.md:1508-1512). Q34 needs a design.
- **J3** has no journey that names it.
- **tC4 has moved:** 98 commits since 9ad4d5c, with no newer sweep. Rerun `tc4-issue-sweep.js` before D2, E1a and F1.

## 5. Handoff candidate: G1 to another developer
G1 is a different stack (Node and Electron, Windows signing). Its contract is already written:
- the `tc4 serve` address line, stdin close and `app.lock` (§5.1);
- `window.tc4Shell` (§5.7);
- the AD11 header filter;
- the `BufferLine` mirror format.

Integration: G1 lives in `shell/`, with its own CI job. Rust changes only in `tc-ui/.../bridge.rs` and `crates/tc4`. A shared test is needed on each side of the bridge.

## Findings
- [F1] (medium/high) B11 is merged on resume-build-lane, but its post-merge records and gate are unfinished.
  - evidence: git merge-base --is-ancestor rl-b11 resume-build-lane → true; merge commit f07fd9e 2026-10-21 11:56; snap/resume-build-lane/.audit/STATUS.md:56 '| B11 | … | ready to start |' generated at 3eac396 (before the merge); RESUME-2026-10-20.md: 0 matches for 'B11 checkpoint'; rebuild.tsv last line 2026-10-20T11:30; git diff --stat in .worktrees/resume-build-lane: .audit/minors.tsv +2 (B11 minors); git worktree list still shows target/lanes/b11; HANDOFF-resume-lane.md:20-21 'Running: B11's post-merge records'
  - rec: Before C7a: commit the minors, regenerate STATUS with status.py, add the B11 checkpoint to RESUME, remove the lane worktree, and leave the batch gate to resume-lane.js.
- [F2] (high/high) C7a must widen the resolve API. The Text tab can keep only VerseText; passage kinds show 'Every version is kept', though storage already supports every choice.
  - evidence: snap/resume-build-lane/crates/tc-ui/src/views/review.rs:298-301 entry_keeps == VerseText && unreadable == 0; crates/tc-api/src/conflicts.rs:382 'pub keep: ChangeRef'; crates/tc-store/src/commands.rs:192-205 Resolve/ResolveBulk/Restore; crates/tc-journal/src/fold/resolve.rs:87-110 KeepPassage/KeepSeparate/KeepJoined
  - rec: Add a KeepChoice enum, restore and resolveBulk endpoints, one App::choose entry point, and a KeepPhase enum to replace busy/sent/outcome in tc-ui app/review.rs:21-46.
- [F3] (critical/high) No CI check can fail for P1, P2 or P7 journeys today: all 200 browser journeys skip in CI.
  - evidence: parity/app/journeys/kit.mjs:22-23 skip when TC4_BIN/TC4_UI are unset; snap/resume-build-lane/.github/workflows/ci.yml:59-63 'their browser tests skip here too'; grep count: 200 browserTest calls in 29 files
  - rec: Build G2's app-e2e CI job now. Make a skip a failure in that job, and add a P1 matrix test so every live J-id maps to a journey.
- [F4] (high/high) The release translation gate never runs in CI, and almost every rebuild key is untranslated.
  - evidence: crates/tc-api/tests/refusals.rs:599-617 asserts only when !debug_assertions; ci.yml runs only 'cargo test --workspace' (debug); its only release test is tc-align's; crates/tc-ui/assets/i18n-rebuild/*.pending.json: 155 keys each in 9 languages; en.json has 156
  - rec: Run 'cargo test --release -p tc-api --test refusals' in the release pipeline (G1c). Ask the owner who translates (Q-10 'who' is unanswered).
- [F5] (critical/high) D1 and D2 are blocked on merging S2 into the app line, and that merge is not trivial.
  - evidence: git merge-tree --write-tree resume-build-lane s2-proof-executor: 13 CONFLICT files, incl. tc-store handle.rs, hooks.rs, open.rs, tests/store/{commands,drafts}.rs, Cargo.toml/.lock, ci.yml; git diff --stat 7e62455..resume-build-lane -- crates/tc-store crates/tc-journal: 36 files, +2,291/−357; s2:crates/tc-store/src/handle.rs:916-931 checkpoint returns CheckpointReport (R21); :838-855 focus, take_in_held
  - rec: Plan an explicit S2 integration step with a named owner. Keep the lane's draft machine, creation pins, AddBookText and R26 changes. Adapt tc-app backend/{mod,store,memory}.rs to CheckpointReport.
- [F6] (high/high) E1a, E1b, E2 and F1 wait on storage units that have no code yet, but their seams and UI can be built now.
  - evidence: No crates/tc-sync or crates/tc-import in any of the 4 snapshots; tc-app/src/sync.rs:147 Unavailable(Sync) in production; ScriptedSync is test-hooks only (app.rs:195-197); APP-DESIGN §9.2 R10/R11 fallbacks ScriptedSync/ScriptedImport; 10 unported gallery cases (share 4, import 6) in report.json
  - rec: Start SyncPort, SharePort, ExchangePort and ImportPort traits with scripted implementations, plus their UI and gallery cases, now. Swap in the real ports when X and I land.
- [F7] (high/high) The Q-9 design sign-off is the human gate for C7b, and through C7b for P2's J26 check-conflict step.
  - evidence: APP-DESIGN.md:3397 C7b waits 'Q-9: the owner signs off the designs first'; APP-DESIGN.md:3411 J26 'its check-conflict step once C7b has landed'; PLAN.md:243-249 P2 adds the check-conflict resolution; HANDOFF.md 'What needs the owner: Q-9'
  - rec: Send the owner one packet covering every rebuild-only screen (review tabs, History and Restore, bulk keep, draft offer, read-only banner, first-launch failure, boot error, USB flows) and record a ruling line.
- [F8] (medium/medium) E1a's OS keychain (Q-3: the keyring crate) cannot be built offline.
  - evidence: ~/.cargo/registry/cache has no keyring crate (zip and security-framework are present); Cargo.toml [workspace.dependencies] has no keyring; tc-app/src/account.rs:190 implements only MemKeychain; RESUME-2026-10-20.md: Cargo is offline
  - rec: Ask the owner to approve fetching the crate, or switch Q-3 to the shell's safeStorage before E1a step 2.
- [F9] (high/high) PLAN P6 (Aquifer helps in the same screens) has no owning app unit.
  - evidence: APP-DESIGN.md:1508-1512 'nothing in the app installs an Aquifer asset'; PLAN.md:270-271 P6; library tests only: tc-helps/tests/aquifer.rs, tc-resources/tests/aquifer_proxy.rs
  - rec: Add a unit (an Aquifer install job plus helps screens on pinned offline fixtures) and get the owner's Q34 design.
- [F10] (high/high) R28 is unfixed: on Linux or macOS with Dropbox installed, every project opens read-only.
  - evidence: crates/tc-store/src/platform.rs:136-138 ancestor.join('.dropbox').exists(), identical in all 3 snapshots; APP-DESIGN.md:3541 risk 23
  - rec: Make it a release blocker for G1c's non-Windows installers and assign it to storage unit T.
- [F11] (medium/high) G1 is too large for one unit, and the design text contradicts the owner's ruling on the first-launch window.
  - evidence: APP-DESIGN.md:3425 G1 row (shell, supervision, mirror, single instance, PDF, dialogs, keychain/feedback) plus TC4-ISSUES rows for installers, signing, CRT, notices and frames; rebuild.tsv:94 ruling 'window shows within 1 s … no splash' vs APP-DESIGN.md:1727 'a repeat launch shows no window'; TC4-ISSUES:344 'fix §5.3'
  - rec: Split G1 into G1a (core, now), G1b (bridge features) and G1c (packaging and release pipeline). Fix §5.3 first.
- [F12] (medium/high) The 18 unported parity cases map exactly to remaining units, and the Arabic coverage P7 needs is mostly missing.
  - evidence: report.json: unported share-* 4, import-* 6, feedback-* 7, home-checkpoint-error 1; report.json locale counts: en 124 rows, ar 6 rows
  - rec: Make each unit's 'done when' include porting its cases. G2 adds ar captures for all 62 screens.
- [F13] (medium/high) tC4 has moved 98 commits since the last issue sweep, with changes relevant to D2, E1a and F1 that TC4-ISSUES does not yet track.
  - evidence: git rev-list --count 9ad4d5c..b86270f = 98 in ext/tc4-tip; e2bfd8c3 and 50cfec3e (#796 export), 43164cbe (#791 D108), 095144ce (#801), 0b4cb270 (#813), f2ccc715 (#807); TC4-ISSUES.md: 0 matches for #807, #812, #813, #820, #822
  - rec: Rerun tc4-issue-sweep.js against b86270f before D2, E1a and F1 start.
- [F14] (high/medium) Most live P1 journeys with no journey file are owned by remaining units; J3 is the exception.
  - evidence: grep of parity/app/journeys finds no J3, J7, J8, J9a, J9c, J9d, J17, J23, J24, J26; APP-DESIGN.md:3050-3068 maps J7/J23→D2, J8→D1, J9→F1, J11/J17/J24→E1b, J26→E1a; J3→B9a/C8 (done)
  - rec: Add a P1 matrix file and test. Assign J3 an explicit journey (guided-fix and source-texts cover only parts).
- [F15] (high/high) Nine storage requests the remaining app units need are still unmet.
  - evidence: APP-DESIGN.md §9.2 rows R9, R10, R11, R13, R17, R21, R28, R29, R30; R21 exists only on unmerged s2-proof-executor (handle.rs:916-919); no is_idle and no public project.json writer in 3 snapshots
  - rec: Track them as a checklist in STATUS. D1 uses Q-5's two-minute fallback until R9 is measured.

## Unverified
- I did not run any build, test or gate; no claim that a test passes is my own observation.
- Whether B11's merge commit f07fd9e got its own post-merge gate: the .enjoy-logs folder is not in the snapshot, and I read no log.
- Whether GitHub Actions actually runs ci.yml for main or the lane: I checked only the file.
- Size estimates are inferences from merge-commit counts, line counts (which include goldens and captures) and timestamps.
- Whether a reachable trigger for C7a's look-again test exists before D1 or E1a: an inference from B10's note that teammates' changes come only with a scripted sync.
- Whether crates.io is reachable to fetch keyring: I only listed the local cargo cache.
- S2's proof completeness and P's R9 numbers: these belong to the storage evaluators; I read only S2-PROOF-2026-10-21.md and the handle API.
- Whether guided-fix and source-texts journeys assert J3's full end state (pins with sha and repoPath): not read.