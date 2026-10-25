---
id: X
title: Door43 sync and USB exchange (tc-sync)
track: A
lane: storage
milestone: M2
state: building
owner: S2 session (Claude Code add49644)
branch: track-X
worktree: /home/bot/tc-rst/.worktrees/track-X
depends: [S2, DOM-1, T]
needs_owner: []
proves: [P2, P3, P4, P5]
no_loss: true
size: 4-6 d (basis: S1 16 commits and +19,944 lines in about 18 h; S2 about 76 h including proof; H18 unsupported removes the Door43 intake walk, docs/STORAGE-DELTA-2026-10-21.md §6 gives -1 to -2 d)
merged: none
updated: 2026-10-26
next: local scripts/gates.sh on track-X, then hand it to the integration lane for a batch after T.
blocked_by: none
later: [H5 replaced-list in SyncReport and ReceiveReport (by kind and person), once C7b's fold reports what newest-wins replaced]
---

# X: Door43 sync and USB exchange (tc-sync)

## Goal

A translator can sync with Door43, share, join on another computer, and exchange work by a USB
file, and no device loses, reverts or hides saved work. A new crate `crates/tc-sync` does this
through one typed state machine, one store entry for received work, and DOM-1's git calls. Every
sync and USB crash row of STORAGE §6.4 (Y1-Y6, U1-U2, I1-I2) has a test, and devices converge
byte for byte over git and over exchange files.

Since the owner's rulings of 2026-10-21 (`.audit/roadmap/rulings/2026-10-21-outside-edits.md`,
"OE"; `2026-10-21-carried-files.md`, "CF"): only translationCore's sync merges into a project
(OE 1, OE 6). A folder pushed to Door43 by another tool is unsupported: the sync keeps the
project's text and says so once (OE 7). Edits made with Door43's own tools are unsupported the
same way: the sync keeps the project's text and says once who changed what
(`rulings/2026-10-22-H18.md`, B). A hand-edited book inside an exchange file is not taken in and
nothing is kept; the app says once that the file on the stick still has it
(`rulings/2026-10-22-H22.md` part 3). Intake keeps no backups of old carried versions (CF 2);
carried files work from now on, without git backups (`rulings/2026-10-22-H19.md`, A).

## Why it matters to a translator

Sync is where a team's work meets. A forced push, a half-received batch or a reverted verse would
erase someone's work without anyone seeing it (tC4 RISKS #5, #6, #11, #12).

## Before you start

Run these from the repository root on your branch:

- `python3 -I .audit/tools/board.py` shows X `ready`;
  `for u in S2 DOM-1 T; do git merge-base --is-ancestor "$(sed -n 's/^merged: //p' .audit/roadmap/units/$u.md)" HEAD && echo "$u ok"; done`
  prints `S2 ok`, `DOM-1 ok`, `T ok`.
- `grep -n "pub enum SyncFailure\|pub enum SyncBlock\|pub enum SyncRefusal" crates/tc-api/src/refusal.rs`
  prints three lines (:375, :398, :420 at f9067bc).
- `grep -n "Intake," crates/tc-journal/src/pending.rs` prints line 32;
  `grep -rn "Purpose::Intake" crates/tc-store/src` prints nothing (no store code writes an intake).
- `grep -n "BreakOff" crates/tc-door43-fake/src/*.rs` prints lines in `lib.rs` (:228) and in
  `api.rs` `fn download` (:1156) only: git smart HTTP (`smart_http.rs`) has no cut connection.
- `grep -n "pub fn fetch\|pub fn push\|pub fn ls_remote" crates/tc-door43/src/git.rs` prints three
  lines (DOM-1). `test -d crates/tc-sync || echo absent` prints `absent`.
- Steps 4 to 8 need S3's core slices, packs included: `grep -n "core merged" .audit/roadmap/units/S3.md`
  prints S3's step 8 `note`, or S3's `state:` is `done`. Steps 1 to 3 run beside S3.
- `git -C ~/unfoldingword/tc4-sweep cat-file -t b86270f` prints `commit`; if not, run SWEEP step 1.
  Never fetch into `~/unfoldingword/translationcore4`.
- CHK-2: library tests install `Hooks::check_burrito` (`crates/tc-store/src/hooks.rs:108`, feature
  `test-hooks`) with `burrito_problems` (`crates/tc-store/tests/store/common.rs:229`); tc-app tests
  use `backend_over` and its `BurritoWatch`.

Read first: `docs/STORAGE.md` §6.4, §9, §10, §11.2, §11.3, §15, §16.1-16.3;
`docs/TC4-DELTA-2026-10-16.md` "Decisions (2026-10-16)" and Q6-Q13; `docs/TC4-REQUEST-MOVED-NOTICE.md`;
the two rulings above; `docs/STORAGE-DELTA-2026-10-21.md` §1, §2.8, §2.9, §2.12-§2.14, §3, §4,
§6 (row X), §7 (H18, Q3, Q6) and its "Review (2026-10-21)". The delta changes STORAGE.md when S2
merges; where they differ, the delta wins.

- `ls .audit/roadmap/rulings/2026-10-22-H18.md .audit/roadmap/rulings/2026-10-22-H19.md .audit/roadmap/rulings/2026-10-22-H22.md`
  lists three files: the rulings this card's steps 5 and 7 follow.

Answered, never ask again: Q1, Q4 keep both and show the conflict; Q2 OBS syncs like Bible; Q3
fetch anonymously, sign in on 401 or 404, retry; Q8 check `permissions.push` before the fetch; Q15.
By recommendation: Q7 username at the first successful sync, once; Q9 no card text, ok toast only
after confirmation; Q10 keep `tc.branch`; Q11 six retries, total wait at most 30 s. Q12 by
measurement: Door43 declines a push without write access inside an HTTP 200 reply, which
`tc_door43::refused_push` (`crates/tc-door43/src/push.rs:44`) maps to `NoWriteAccess`. The team
move is one way and writes `moved.json` once (`.audit/rebuild.tsv:61`). Ruled 2026-10-21: OE 6
(Door43 merges teammates' work that arrives by translationCore's sync), OE 7 (a folder pushed by
another tool: keep the project's text, say so once), CF 1-CF 3 (a carried version records its
hash, the version it replaced, its Door43 user and time; no old bytes kept; two versions that
replaced one are a conflict). Ruled 2026-10-22: H18 B (Door43's own tools: unsupported, a notice,
no intake walk), H19 A (carried files work now, without git backups), H22 part 3 (a changed book
inside an exchange file: not taken in, nothing kept, one notice).

## Funnel card

- **Invariants**:
  - INV-PUSH-IS-NEVER-FORCED: no push the app sends can remove a commit from Door43's branch.
  - INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT: each batch of a teammate's work (sync or file) is in
    the fold whole or not at all after any crash, and stays after any refusal after intake.
  - INV-OWN-WORK-NEVER-REVERTED: no sync or exchange removes, replaces or hides a change this
    device holds; two versions of one record are a listed conflict.
  - INV-SAVES-CONTINUE-DURING-SYNC: a save during any sync step succeeds without waiting and is in
    this sync's merge commit (if made before S7) or the next one.
  - INV-SYNC-CONVERGES: devices holding the same changes have the same frontier, conflicts and
    derived bytes, for any transport (memory, zip, git), order, duplication, crash or retry.
  - INV-ONLY-TRANSLATIONCORE-MERGES (OE 1, OE 7): a derived file changed in a commit pushed by
    another tool, or inside an exchange file, never enters the project on its own; the project
    keeps its text, the other version stays where it came from, and the app says so once. This
    covers Door43's own tools (`rulings/2026-10-22-H18.md`, B).
- **Wrong states**: a force push drops a teammate's commit (RISKS #6); a refusal after intake rolls
  received work back or says "Nothing changed" while HEAD moved (Q8); intake writes over another
  program's file (§5.10 M1); half a batch applied after a crash (Y2, I2); a damaged teammate pack
  dropped (§9.2 N3); a copied username grouped as "Mine" (#161); an email or token in a change; a
  sync rewrites `moved.json`; "Saved" before the export was read back from the device (§10.1 E5);
  an invalid exchange zip; a share-made `main` a sync does not converge with (RISKS #5); a commit
  pushed by another tool replaces the project's text, or is reported more than once (OE 7, H18); a
  hand-edited book file in an exchange file taken in, or kept as a copy (OE 1, H22 part 3); Door43
  holds no `main` branch after a move and a sync pushes a new `main` from the app's own tree
  (`evidence/2026-10-21/gap-tc4-straggler-sync-safety.md` state 3b); a carried version
  that is still a head, whose blob was lost, written over at intake (delta L151); intake writes
  `refs/tc/carried` or a backup pack (CF 2).
- **Code**:
  - Types (to create): `tc_sync::machine::{SyncState, SyncEvent, SyncAction}` and one pure,
    exhaustive `next(&SyncState, &SyncEvent) -> (SyncState, Vec<SyncAction>)` for S0-S10; the state
    records whether intake happened, so a failure is `SyncFailure::BeforeIntake` or `AfterIntake`
    by construction. `FastForwardRefspec` (`crates/tc-door43/src/git.rs`), built only from a branch
    name, so no refspec starts with `+`. `tc_store::Intake` with a private constructor, built only
    from packs that passed §9.2. `ExchangeFile`, a zip parsed with §10.2 R1's caps.
  - One entry point: `tc_sync::{sync, share, join, connect}` and `tc_sync::exchange::{export,
    receive}`, each an executor of `next()`. Project writes go through `ProjectHandle::take_in`
    (to create) or S2's checkpoint; network through `tc_door43::git`; other files through
    `tc_store`'s atomic helpers.
  - Removed: STORAGE §9.7's own `pub enum Refusal` (:3860 at 3b075d7) is never built; tc-api's
    `SyncFailure`, `SyncBlock`, `SyncRefusal` are the one set. `ScriptedSync`
    (`crates/tc-app/src/sync.rs:118`) stays until E1a swaps in the real port. Not built (delta §3):
    the §11.3 walk over exchange files; the carried backups at intake (`refs/tc/carried`,
    `sync_blob_backup`, `copy_blob_to_durable`; S3 step 9 deletes them from tc-store); the §11.2
    walk (`crates/tc-sync/src/walk.rs`) and §9.2 N4 (`rulings/2026-10-22-H18.md`, B).
  - Left to checks: Door43's behavior (non-fast-forward, cut connection, hook refusal) and
    convergence across devices are facts about other machines; no local type holds them.
- **Checks** (to create unless marked; CI job `rust` via `cargo test --workspace` unless marked):
  - All five: `sync_next_is_total_over_every_state_and_event` (unit, matrix); `random_syncs_lose_no_work`
    (model with crashes, restarts, Door43 races, saves at every step); `random_syncs_lose_no_work_long`
    (5,000 cases, `#[ignore]`, nightly job).
  - PUSH: `push_is_never_forced`, `non_fast_forward_retries_six_times_within_30_s_then_busy`.
    RECEIVED: `intake_never_writes_over_a_file_another_program_wrote`, `a_damaged_remote_pack_is_salvaged_and_listed`,
    the crash rows (crash kind). OWN-WORK: `crash_at_every_sync_step_converges_on_retry` (RISKS #11),
    `loss_g1_every_own_pack_and_pending_save_survives_sync` (RISKS #12), `no_email_or_token_in_any_change`,
    `a_sync_never_rewrites_or_deletes_moved_json`. SAVES: `a_save_during_every_sync_step_is_kept_and_pushed_next`.
    CONVERGES: `three_devices_converge_with_byte_identical_derived_files`,
    `exchange_idempotent_and_relayed`, `a_main_made_by_share_converges_with_one_made_by_sync`, and
    `fold_converges_under_any_union` (exists, property) over zip and git transports too;
    `a_sync_against_a_repository_with_no_main_branch_refuses_and_names_the_state` (step 5).
  - ONLY-TRANSLATIONCORE (to create, names from the delta or this card):
    `a_door43_edit_by_another_tool_is_reported_once` (delta L152) and
    `a_folder_pushed_by_another_tool_keeps_the_project_text_and_is_reported_once` (OE 7)
    (`rulings/2026-10-22-H18.md`, B); step 7's exchange-file test
    `a_hand_edited_file_in_an_exchange_file_is_not_taken_in_and_named_once` (H22 part 3).
  - CARRIED: `a_conflict_side_whose_blob_was_lost_moves_to_its_conflict_copy_before_the_path_is_replaced`
    (delta L151) driven through a sync (`rulings/2026-10-22-H19.md`, A: carried files work now).
  - Cost: `a_sync_costs_what_arrives_not_the_project` on Psalms and ten times Psalms (count).
  - Proof each can fail: each step names its mutation; quote the failure in the log. A `+` refspec
    fails `push_is_never_forced`; no compare-and-swap in `take_in` fails the outside-edit test; no
    30 s cap fails the retry test; merging a foreign commit's derived file fails
    `a_door43_edit_by_another_tool_is_reported_once`; pushing a new `main` when the remote has
    none fails `a_sync_against_a_repository_with_no_main_branch_refuses_and_names_the_state`.
- **Prose**: in the commit that changes the behavior: `docs/STORAGE.md` §9.7 (tc-api's refusal
  types); §3 and §13.3 (`moved.json`: format `tc-moved`, version 1, written once by the move,
  never rewritten by a sync; `docs/TC4-REQUEST-MOVED-NOTICE.md:66-71`); §11.2 and §9.2 N4 are not
  built (H18 B), §11.3 is the notice only (H22 part 3) (delta §2.12, §2.13). The delta itself is
  applied by S2's merge.
- **Human**: none open. H18 (`rulings/2026-10-22-H18.md`, B), H19 (`-H19.md`, A) and H22
  (`-H22.md`, part 3) are ruled; steps 5 and 7 cite them. The other sync questions are answered.

## Steps

### Step 1: Sweep tC4's sync changes since 9ad4d5c (X-0)

- Do:
  1. `grep -n "095144ce\|43164cbe\|sync-refusal.spec" docs/TC4-ISSUES.md`. If it prints rows naming
     X or E1a, SWEEP did this: log that and go to step 2.
  2. `git -C ~/unfoldingword/tc4-sweep log --oneline 9ad4d5c..b86270f -- src/data/share src/data/journal src/views/SyncResult.jsx e2e/sync-refusal.spec.ts`
     lists be58feb4, 095144ce (#801), 43164cbe (#791, D108). Repeat with `9ad4d5c..origin/release/4.0`.
  3. Row `| #801 | After a sync, show the plain refusal` exists: add 095144ce and tC4's
     `e2e/sync-refusal.spec.ts` (E1a ports it).
  4. Row `| #791 | Tell a translator about a teammate's alignment` waits for D108's rows; tC4
     shipped them (43164cbe). Say so, and add X: the §9.7 report names book, reference and teammate
     for each verse with two alignments; E1a shows the rows.
  5. tc4-bugs §2 gives X "#821 path 1" (read it: `gh issue view 821 -R unfoldingWord/translationCore4`).
     A tC4 team repository reaches the rebuild only through the team move (I step 8); a plain sync
     blocks it (step 6). Write that in the #813/#821/#822 row (I step 1 writes it; add it if missing).
- Files: `docs/TC4-ISSUES.md`.
- Done when: item 1's grep prints at least two rows naming X or E1a;
  `python3 -I .audit/tools/board.py --check` prints no error.
- Record: append a log line `step 1` (TRACKING.md format); update `state`, `updated`, `next`.

### Step 2: Build the sync state machine first (X-1)

- Do:
  1. Create `crates/tc-sync` (`publish = false`, edition 2024, `[lints] workspace = true`;
     `members = ["crates/*"]` picks it up). Dependencies: tc-core, tc-journal, tc-burrito, tc-api,
     tc-store, tc-door43, thiserror, zip. Dev: tc-store with `features = ["test-hooks"]`,
     tc-door43-fake, proptest, tokio. All through `[workspace.dependencies]` or path.
  2. In `src/machine.rs` write the machine for S0-S10 (STORAGE §9.1) in the shape of
     `crates/tc-store/src/ledger.rs` (`State` :92, `Event` :106, `next` :454), no wildcard arm.
     Offline at S0 → `BeforeIntake(Offline)`; Q8 pre-check fails → `BeforeIntake(NoWriteAccess)`;
     §9.2 N1 or N2 → `Blocked(..)`; any failure after intake → `AfterIntake(..)`; six
     non-fast-forwards → `Busy { attempts: 6 }` on the side of intake it happened.
  3. In `src/machine.rs` write `sync_next_is_total_over_every_state_and_event`: every state and
     event; every failure after integrate is `AfterIntake`; no action pushes with force.
  4. In `tests/sync/model.rs` (modules declared with `#[path]` in `tests/sync.rs`, as
     `crates/tc-store/tests/store.rs` does) write `random_syncs_lose_no_work` and `_long`: drive
     `next` against a simulated remote, teammates pushing between steps, a crash and restart at any
     step (keeping only persisted state), saves at any step; assert PUSH, RECEIVED and SAVES.
- Files: `crates/tc-sync/{Cargo.toml,src/lib.rs,src/machine.rs,tests/sync.rs,tests/sync/model.rs}` (to create).
- Done when: `cargo test -p tc-sync` passes; making `next` return `BeforeIntake` after integrate
  fails the matrix test with a message starting `INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT`. Restore.
- Record: log line `step 2` with the quoted failure; update `state`, `updated`, `next`.

### Step 3: Add the write-access pre-check, a fast-forward refspec and the cut connection (X-2)

- Do:
  1. Add `can_push: bool` to `RepoInfo` (`crates/tc-door43/src/repos.rs:21`) from Gitea's
     `permissions.push` (the fake sends it, `crates/tc-door43-fake/src/json.rs:147`). Before the
     fetch call `Door43Client::repo` (`crates/tc-door43/src/client.rs:256`) with the session;
     `can_push == false` → `BeforeIntake(NoWriteAccess { .. })` and no git request (Q8).
  2. Add `FastForwardRefspec` to `crates/tc-door43/src/git.rs`; `git::push` takes it, never a string.
  3. In `crates/tc-door43-fake/src/smart_http.rs` honor `Fault::BreakOff { after }` as `download()`
     does (`api.rs:1143-1160`): send `after` bytes of git's reply, then cut. For `git-receive-pack`
     git has applied the push, so a cut reply is "sent, not confirmed" (Y6). Update the `BreakOff`
     doc comment (`lib.rs:226-227`) and APP-DESIGN's R13 row (`docs/APP-DESIGN.md:3266`).
  4. Call `git::fetch` at S3 and `git::push` at S8; tc-sync calls no git2 network function itself.
  5. In `crates/tc-sync/tests/sync/door43.rs`: `no_write_access_is_refused_before_the_fetch_with_nothing_changed`
     (HEAD and files unchanged; no `git-upload-pack` in the fake's log), `a_fetch_cut_off_mid_pack_changes_nothing`
     (Y1), `a_push_whose_reply_is_cut_off_is_sent_not_confirmed_and_the_next_sync_confirms` (Y6),
     `a_push_declined_by_door43s_hook_is_no_write_access_and_keeps_received_work` (Q12).
- Files: `crates/tc-door43/src/{repos.rs,client.rs,git.rs}`, `crates/tc-door43-fake/src/{smart_http.rs,lib.rs}`,
  `crates/tc-sync/src/`, `crates/tc-sync/tests/sync/door43.rs` (to create), `docs/APP-DESIGN.md`.
- Done when: `cargo test -p tc-sync --test sync door43` and `cargo test -p tc-door43 -p tc-door43-fake`
  pass; without the pre-check the first test fails on the upload-pack request. Restore.
- Record: log line `step 3`; update `state`, `updated`, `next`.

### Step 4: Take received work into the store, whole or absent (X-3)

- Only with S3's core slices on your branch.
- Do:
  1. Add `ProjectHandle::take_in(Intake)` in `crates/tc-store/src/handle.rs` beside `submit` (:770
     at 3b075d7). `take_in_held` (:848) is gone after S3 step 9 (delta §3: *Import* and *Not now*
     replace it); never reuse it. One actor job does §9.3
     T1-T3 and §10.2 R7: each pack through §5.10 M1's compare-and-swap; `metadata.json` rewritten;
     the changes as one pending file with `Purpose::Intake` or `Purpose::Salvage`
     (`crates/tc-journal/src/pending.rs:32-34`); an incremental fold; for a USB file
     `.tc/intake.json` written first and removed last (I2). If S3 named this entry, use its name.
  2. Merge commit (§9.1 S7): `build_tree` (`crates/tc-store/src/checkpoint.rs:473`) takes R as a
     second parent and keeps R's foreign entries (§9.4 "No entry of R is dropped silently").
  3. Carried versions that arrive (delta §2.14, CF 2): keep no backup of an old version, write no
     `refs/tc/carried`. A version that is still a head (a conflict side that moves to its
     `.conflict-` copy) is written and flushed at its new place before the old place is written
     over; a missing blob is stored again from the file first. Carried files work from now on
     (`rulings/2026-10-22-H19.md`, A), so this item is built in full.
  3. In `crates/tc-store/tests/store/crash.rs`, with `kill_everywhere` (:85), `kill_everywhere_full`
     (:109) and kills around each git2 call (§16.2): `crash_intake_y2_some_packs_written_folds_whole_batches_only`,
     `crash_intake_y3_packs_written_batch_not_recomputes_the_same_changes`,
     `crash_intake_y4_merge_commit_not_made_commits_next`, `crash_usb_receive_i1_writes_nothing`,
     `crash_usb_receive_i2_takes_the_rest_on_retry`. In `tests/store/outside.rs`:
     `intake_never_writes_over_a_file_another_program_wrote`. Carried files (H19 A):
     `a_conflict_side_whose_blob_was_lost_moves_to_its_conflict_copy_before_the_path_is_replaced`
     (delta L151) with the teammate's version arriving through `take_in`; if S3 already wrote it
     in tc-store, add the intake case to it.
- Files: `crates/tc-store/src/{handle.rs,checkpoint.rs,actor.rs}`, `crates/tc-store/src/intake.rs`
  (to create), `crates/tc-store/tests/store/{crash.rs,outside.rs}`.
- Done when: `cargo test -p tc-store --test store crash::`, `... outside::` and `... model::` pass;
  removing the compare-and-swap from `take_in` fails the `outside.rs` test. Restore.
- Record: log line `step 4`; update `state`, `updated`, `next`.

### Step 5: Run a whole sync: retries, confirm, report, repack, commits made by other tools (X-4)

- Do:
  1. Write the S0-S10 executor in `crates/tc-sync/src/sync.rs`, clock injected (tests use tokio's
     paused time). Back off `min(2^attempt, 16) s × U(0.7, 1.3)`, total wait at most 30 s (Q11).
     Confirm with `git::ls_remote`, then write `.tc/sync.json` `confirmed` atomically (§9.5).
     Return the §9.7 `ExchangeReport`. Repack after a sync, never during (§9.8), with no
     `refs/tc/carried` step (delta §2.8).
  2. Commits not made by translationCore: unsupported, with a notice (`rulings/2026-10-22-H18.md`,
     answer B). Write no walk and no §9.2 N4. At intake, for each commit after the last app commit
     (one with the `Tc-Frontier` trailer) that changed a derived file, add one line to the report,
     remembered by commit id so it is said once: "Titus was changed on Door43 by jdoe (commit
     3fa2c1d) with another tool. translationCore kept its own text. That version is in Door43's
     history." (delta §2.12). The merge tree takes the fold's projection for every derived file.
     Nothing is kept as a copy and nothing is offered for *Import*; a person who wants the web
     version downloads the file and runs "Update a book". Tests:
     `a_door43_edit_by_another_tool_is_reported_once` (L152),
     `a_folder_pushed_by_another_tool_keeps_the_project_text_and_is_reported_once` (OE 7).
  2b. Door43 holds no `main` branch (`evidence/2026-10-21/gap-tc4-straggler-sync-safety.md`, state
     3b: an owner deleted it after a move). `git::ls_remote` in S1 sees no `refs/heads/main`: the
     machine goes to a refusal state, pushes nothing (never a new `main` from the app's own tree),
     and the report tells the person to ask the Door43 owner to restore the branch. Test
     `a_sync_against_a_repository_with_no_main_branch_refuses_and_names_the_state` against
     `tc-door43-fake` with the branch removed.
  3. Tests against `tc-door43-fake`: `push_is_never_forced`, `non_fast_forward_retries_six_times_within_30_s_then_busy`,
     `a_save_during_every_sync_step_is_kept_and_pushed_next`, `crash_at_every_sync_step_converges_on_retry`
     (Y1-Y6), `a_damaged_remote_pack_is_salvaged_and_listed`, `no_email_or_token_in_any_change`,
     library J26 `three_devices_converge_with_byte_identical_derived_files`
     (three app-data folders, one fake), and the five names `grep -n 'loss_g[18]_' docs/TC4-DELTA-2026-10-16.md`
     lists. Point `random_syncs_lose_no_work` and `_long` at the executor.
  4. Write each missing STORAGE §15 sync or Door43-edit test. Run the list on STORAGE.md after S2's
     merge applied the delta. Skip a name whose row the delta retires, or H18 B retires (L46, L90,
     and L91's web half; L14 takes L152's test). This lists them (23 at 3b075d7):

     ```sh
     sed -n '/^## 15\./,/^## 16\./p' docs/STORAGE.md \
       | grep -oE '`(sync_|loss_f4_|loss_f14_|loss_web_|loss_door43_|loss_bad_pack_|loss_damaged_teammate_|loss_all_copies_|loss_quarantined_|loss_foreign_|loss_bad_name_|door43_ingest_|team_sync_)[a-z0-9_]*`' \
       | tr -d '`' | sort -u | while read t; do git grep -qE "fn $t\b" -- crates || echo "$t"; done
     ```

     Step 6 writes `sync_master_branch`, `sync_private_repository`, `team_sync_obs_project`.
  5. Run `random_syncs_lose_no_work_long` nightly: in `scripts/gates.sh` if
     `scripts/gates.sh --list | grep nightly` prints lines, else as a step of the
     `store-model-nightly` job in `.github/workflows/ci.yml`.
- Files: `crates/tc-sync/src/sync.rs` (to create; no `walk.rs`), `crates/tc-sync/tests/sync/*.rs`,
  `.github/workflows/ci.yml` or `scripts/gates.sh`.
- Done when: `cargo test -p tc-sync` passes; item 4 prints only the three step-6 names and the
  names H18 B retired; items 2 and 2b pass their tests; each funnel-card mutation fails its
  test, quoted in the log.
- Record: log line `step 5`; update `state`, `updated`, `next`.

### Step 6: Share, join and connect (X-5)

- Do:
  1. Write `share`, `join`, `connect` (§9.6) in `crates/tc-sync/src/{share.rs,join.rs}`. Join
     builds in `Store::staging(StagingKind::Join)`, places with `Store::place`
     (`crates/tc-store/src/store.rs:247,259`), and writes files atomically, never by checkout.
  2. Write the Door43 username once, after this device's first successful fetch (Q7). A copied
     profile's username is replaced at the first signed-in fetch (#161).
  3. A repository with `ingredients/checking/journal/` and no `ingredients/tc/format.json` →
     `Blocked(SyncBlock::Tc4TeamRepository)` (the move is I step 8). No sync rewrites or deletes
     `ingredients/tc/moved.json`.
  4. Tests in `crates/tc-sync/tests/sync/share.rs`: `username_written_once_after_first_successful_fetch`
     (#680), `copied_username_is_replaced_by_the_first_signed_in_fetch`, `share_no_username_without_consent`,
     `sync_private_repository`, `team_sync_obs_project` (#761), `sync_master_branch`,
     `a_tc4_team_repository_is_blocked_and_nothing_changes`, `a_sync_never_rewrites_or_deletes_moved_json`,
     `a_main_made_by_share_converges_with_one_made_by_sync` (RISKS #5),
     `a_join_cut_off_leaves_only_a_staging_folder_the_next_start_removes` (§6.4 N1).
- Files: `crates/tc-sync/src/{share.rs,join.rs}`, `crates/tc-sync/tests/sync/share.rs` (to create).
- Done when: `cargo test -p tc-sync` passes; step 5 item 4 prints nothing but the names H18 B
  retired (none once §15 follows the ruling); a sync that writes
  `moved.json` again fails `a_sync_never_rewrites_or_deletes_moved_json`. Restore.
- Record: log line `step 6`; update `state`, `updated`, `next`.

### Step 7: Exchange by USB file, and converge over every transport (X-6)

- Do:
  1. Export (§10.1 E1-E5) in `crates/tc-sync/src/exchange.rs` with the workspace `zip` crate,
     verified from the device: Linux `rustix::fs::fadvise` (`Advice::DontNeed`); Windows
     `OpenOptionsExt::custom_flags` with `FILE_FLAG_NO_BUFFERING` and aligned reads; macOS
     `rustix::fs::fcntl_nocache`. No `unsafe`. "Saved" only after the verify.
  2. Receive (§10.2 R1-R8) through `take_in`. No §11.3 walk (delta §2.13, §3). Two ruled parts:
     - A derived file in the zip that differs from the projection of the zip's own packs
       (`rulings/2026-10-22-H22.md` part 3): nothing is kept and nothing is offered for *Import*;
       the app keeps its own text and says once, for example "Juan's file holds a changed Titus
       that was not made in translationCore. It was not taken in; the file on the stick still has
       it." The zip is never changed, so nothing is lost.
     - Carried entries (§10.2 R5): R5 as written, minus the backups (CF 2;
       `rulings/2026-10-22-H19.md`, A).
  3. Move the team simulator to a new `publish = false` crate `crates/tc-team-sim` (to create):
     `crates/tc-journal/tests/sim/mod.rs`, what it uses of `tests/support/` and `tests/common/`,
     `Transport` and `InMemory` (`tests/properties.rs:44-61` at f9067bc), and the body of
     `fold_converges_under_any_union` (:358) with `team_synced` and `Rng` as a function taking
     `&dyn Transport`. tc-journal's `properties.rs` and `scope.rs` (the users of `mod sim`) use it
     through a dev-dependency and keep running it over `InMemory`.
  4. In `crates/tc-sync/tests/sync/transports.rs` implement `Transport` for exchange zips and for a
     local bare git repository driven by the real sync code; run the property over both.
  5. Install `Hooks::check_burrito` in every tc-sync rig; check each zip's entries with
     `tc_burrito::validate_project` (`crates/tc-burrito/src/validate.rs:197`).
  6. Tests: `exchange_idempotent_and_relayed`, `usb_both_directions`, `usb_export_name_sanitized`,
     `usb_export_verify_reads_device`, `loss_usb_one_bad_pack_rest_taken`,
     `loss_usb_damaged_carried_entry_not_recorded` (delta L83's new wording: nothing taken in
     without the person), `a_teammates_audio_file_arrives_byte_for_byte` (#705; H19 A), the U1
     and U2 crash rows, and for item 2's first part (H22 part 3)
     `a_hand_edited_file_in_an_exchange_file_is_not_taken_in_and_named_once` (a book file and
     another derived file, both cases: the project's text unchanged, no `kept` entry, one notice).
- Files: `crates/tc-sync/src/exchange.rs`, `crates/tc-team-sim/` (to create), `crates/tc-journal/{Cargo.toml,tests/}`,
  `crates/tc-sync/tests/sync/{exchange.rs,transports.rs}` (to create).
- Done when: `cargo test -p tc-journal --test properties fold_converges_under_any_union` and
  `cargo test -p tc-sync --test sync transports` pass; taking a damaged pack whole fails
  `loss_usb_one_bad_pack_rest_taken`. Restore.
- Record: log line `step 7`; update `state`, `updated`, `next`.

### Step 8: Meet the budgets and write the evidence (X-7)

- Do:
  1. Write `a_sync_costs_what_arrives_not_the_project` in `crates/tc-sync/tests/sync/opcount.rs`
     in the pattern of `crates/tc-store/tests/store/opcount.rs`, on `psalms()` and `psalms_x10()`
     (`tests/store/corpus.rs:93,98`): one received pack costs the same on both.
  2. Measure release builds on `tc_corpus::Spec::whole_bible` (`crates/tc-corpus/src/lib.rs:139`):
     daily sync 3 s plus network; USB export 15 s on USB 2 with progress (`docs/PERFORMANCE.md`).
     Ask the laptop session (`.audit/tasks/windows-track.md`) for reference-machine numbers.
  3. Write `docs/evidence/sync-<date>.md` (to create): the numbers; the tC4 G1-G8 to test map
     (DELTA-16's G-id rule; #24); RISKS #8 left for a Door43 Preview check at the first pilot.
- Files: `crates/tc-sync/tests/sync/opcount.rs`, `docs/evidence/sync-<date>.md` (to create).
- Done when: `cargo test -p tc-sync --release --test sync opcount` passes; the evidence file has
  one line per budget, such as `daily sync: <n> s (budget 3 s): pass`.
- Record: log line `step 8`; set `state: verifying`, `updated`, `next`.

## Gates before merge

`scripts/gates.sh` once INT-3 creates it. Until then: `cargo fmt --all --check`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
`cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings` with no
features and with each of `--features gallery`, `--features app`, `--features test-hooks`;
`cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. Plus:

- `for s in 20261101001 20261101002 20261101003 20261101004; do PROPTEST_RNG_SEED=$s cargo test --locked --release -p tc-sync --test sync -- model::random_syncs_lose_no_work_long --ignored --exact || echo "seed $s failed"; done`
  prints no `failed` line; `cargo test -p tc-store --test store crash::` and `... model::` pass.
- `WINVM_DIR=x-sync .audit/tools/winvm/run.sh HEAD 'powershell -NoProfile -ExecutionPolicy Bypass -File .audit\tools\winvm\gates.ps1'`
  prints `exit 0` on every gate line and `tests: <n> passed, 0 failed` (E5 differs on Windows).
- The CHK-2 hook is on in every tc-sync rig; the op-count test passes on both corpora; every
  funnel-card test exists, runs in CI and has a recorded failing mutation.
- `grep -rn "refs/tc/carried\|sync_blob_backup\|copy_blob_to_durable\|take_in_held" crates/tc-sync`
  prints nothing (delta §3). `test -e crates/tc-sync/src/walk.rs || echo absent` prints `absent`
  (H18 B).
- The card's log names the ruling file each of steps 4, 5 and 7 followed (H18, H19, H22).
- Three verification lenses (FUNNEL §8 item 5); fix every blocker and every major that harms data
  or what the translator sees.

## Stop and ask when

- A step needs a sentence the translator sees that DELTA-16 or tc-api does not have: ask the owner
  with options, their effect and a recommendation.
- A step would edit files that S3 (`tc-store`), DOM-1 (`tc-door43`) or E1a (`tc-app`) has claimed
  on an active branch: ask that card's owner; do not edit.
- A gate fails twice in a row and you cannot tell why.
- A step would need a choice the H18, H19 or H22 ruling files do not make: ask the owner, as
  above; never pick a branch yourself.
- A Door43 repository whose owner deleted `main` after a move (step 5 item 2b) is reachable only
  by an owner's action on Door43. Beyond the refusal that item builds, the state is outside REACH
  (README "Rules that never bend"): do not test what tC4 does there; log it as a minor with this
  reason.
- Intake or receive would merge a file's changes into the project without a person's *Import*,
  keep a backup of an old carried version, or write `refs/tc/carried`: stop. The 2026-10-21
  rulings (OE 1, CF 2) forbid it.
- A sentence the translator sees about a commit made by another tool or a changed exchange file
  is not in the delta (§2.12, §2.13) or the ruling: ask the owner, as above.

## References

- Evidence (`.audit/roadmap/evidence/2026-10-21/`): `storage-remaining.md` §4, ST-4, ST-7, ST-8,
  ST-12; `health-domain.md` §7, F3, F14; `tc4-bugs.md` §2, §5; `done-definition.md` P2-P4, DOD-10
  to DOD-12; `verification.md` ST-8 (#801 and #791 already had rows);
  `gap-tc4-straggler-sync-safety.md` (state 3b, no `main` on Door43; step 5 item 2b).
- Rulings: `rulings/2026-10-22-H18.md` (B), `-H19.md` (A), `-H22.md` (part 3).
- `docs/STORAGE.md` §6.4, §9, §10, §11, §15, §16; `docs/APP-DESIGN.md` R10 (:3263), R13 (:3266);
  `docs/TC4-ISSUES.md` (f9067bc) rows 282-310 and 312.
- tC4 (b86270f): `src/data/share/syncOperation.ts`, `test/sync/syncOperation.test.ts`,
  `e2e/sync-refusal.spec.ts`, `docs/RISKS.md` rows 5, 6, 8, 11, 12.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/storage-remaining.md
- 2026-10-21 | roadmap | note | none | updated to the owner's rulings of 2026-10-21 and docs/STORAGE-DELTA-2026-10-21.md | .audit/roadmap/rulings/
- 2026-10-22 | roadmap review (completeness review 2026-10-22) | ruling | none | H18 B, H19 A and H22 part 3 applied: step 5 item 2 is the notice only, step 7 item 2 keeps nothing, carried files are built in full; size 4-6 d | rulings/2026-10-22-H18.md
- 2026-10-22 | roadmap review (completeness review 2026-10-22) | note | none | step 5 item 2b: a Door43 repository with no main refuses (gap-tc4-straggler state 3b); `later: S3 from step 4` so the board shows the step-4 wait | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-23 23:49 EDT | checks lane (Claude Code 2198ee19) | note | none | From CHK-3: your rows in docs/TC4-ISSUES.md and docs/P-LEDGER.md are (pending: X); when you write a test, replace the marker with its name in the same commit. .audit/tools/claims.test.mjs fails once this card is done while a marker remains | node --test .audit/tools/claims.test.mjs
- 2026-10-24 00:37 EDT | checks lane (Claude Code 2198ee19) | note | none | Correction to CHK-3's note above: X's only marker is in docs/P-LEDGER.md (P2); docs/TC4-ISSUES.md names X in unit cells, not in (pending: X) markers | grep -n 'pending: X' docs/P-LEDGER.md docs/TC4-ISSUES.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
- 2026-10-25 11:30 EDT | S2 session (Claude Code add49644) | claim | none | claimed on track-X, cut from track-T (e1ffc583) with main 0189cee4 merged in (DOM-1 is on main; T rides in batch 2), as the orchestrator asked under the fast-release ruling (rulings/2026-10-24-H3-H4-H7-fast-release.md): S3 parked, steps 4-8 build on the current store; one review for the card; first report when the sync half works against tc-door43-fake (E1a waits on it) | .audit/roadmap/units/X.md
- 2026-10-25 11:45 EDT | S2 session (Claude Code add49644) | step 1 | none | docs/TC4-ISSUES.md: the two #791 rows say tC4 shipped D108's per-verse rows in 43164cbe (PR #806), X's report names book, reference and teammate, E1a shows them; the #801 row names 095144ce (PR #803) and e2e/sync-refusal.spec.ts for E1a; a new #813/#821/#822 row says a tC4 team repository is blocked by a plain sync (X step 6) and reaches the rebuild only through the team move, which the fast-release ruling cuts from the first release. tc4-sweep at b86270f lists be58feb4, 095144ce, 43164cbe; release/4.0 adds only merges of those PRs | docs/TC4-ISSUES.md
- 2026-10-25 12:20 EDT | S2 session (Claude Code add49644) | step 2 | none | crates/tc-sync with src/machine.rs: SyncState (Idle, S0 Gate .. S10 Repack, Done), SyncEvent (the answer of each step), SyncAction (Push carries nothing: the executor builds the only refspec), one pure next(); each Attempt records whether work was integrated, so a failure is BeforeIntake or AfterIntake by construction; six raced pushes end Busy. Tests: sync_next_is_total_over_every_state_and_event (every state x event), six_raced_pushes_end_as_busy, random_syncs_lose_no_work (model: teammates push, saves and kills between any two steps; 512 cases) and _long (5,000, ignored). Mutations: fail() giving BeforeIntake after intake fails the matrix with `INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT: Fetch(Attempt { n: 1, received: Integrated }) + Start ended BeforeIntake(..)`; skipping intake on Fetched(New) fails the model with `INV-PUSH-IS-NEVER-FORCED: the push would drop [1]` | crates/tc-sync/src/machine.rs
- 2026-10-25 13:05 EDT | S2 session (Claude Code add49644) | note | none | step 3, Door43 half: RepoInfo.can_push from Gitea permissions.push (false anonymously); FastForwardRefspec (tc-door43 git.rs), built only from two branch names git accepts, and git::push takes it, never strings; the fake honors BreakOff on git smart HTTP (the request runs, its reply is cut: a push is applied, "sent, not confirmed"); APP-DESIGN R13 row updated. Tests: a_repository_says_whether_the_account_may_push, a_push_whose_reply_is_cut_off_is_applied_on_door43 (tc-door43 tests/git.rs); tc-door43 and tc-door43-fake suites pass. The tc-sync door43.rs tests come with the executor (step 5) | crates/tc-door43/src/git.rs
- 2026-10-25 15:15 EDT | S2 session (Claude Code add49644) | step 4 | none | the store takes a sync's packs in and makes the merge commit in one checkpoint job: ProjectHandle::sync_merge(remote, packs) (CheckpointReason::SyncMerge { remote }); Project::take_in runs after the checkpoint opens its write group and restores packs, before the seal, so the merge commit holds the received work and the saves since S1. A held pack is skipped; a damaged local copy is repaired from a valid received one (bytes kept in .tc/quarantine); a damaged received pack is salvaged; a path with other bytes is left and listed. The merge tree keeps R's foreign entries and every pack of R the project did not take (§9.2 N3, §9.4); a commit HEAD already descends from is not merged again (Git::descends_from). ProjectFacts.git_dir names .git for the network calls. One store call instead of take_in + checkpoint: a materialization closes a write group only as part of a checkpoint, so the intake rides in the checkpoint's group. Tests (crates/tc-store/tests/store/intake.rs): a_sync_merge_takes_a_teammates_packs_and_commits_with_both_parents, a_sync_merge_of_held_packs_takes_nothing_and_still_commits, intake_never_writes_over_a_file_another_program_wrote (a pack path written while open is a second writer: no merge, the file kept; the take_in compare-and-swap is defensive behind that guard, so removing it fails no test), a_damaged_local_pack_is_repaired_from_door43s_copy, a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree. Mutations: no R entries fails the last at intake.rs:240; no repair fails the repair test at :183. Not built here: the Y2-Y4 and I1-I2 crash rows, the carried-file conflict case (delta L151) | crates/tc-store/src/packs.rs
- 2026-10-25 15:30 EDT | S2 session (Claude Code add49644) | note | none | step 5, first half: tc_sync::sync (src/sync.rs) runs the machine against the store and Door43: the gate reads origin and tc.branch from the repository, checks the internet switch, the session and can_push before any git request (Q8); S1 checkpoint; S3 fetch into refs/remotes/origin/{b} on a blocking thread; S4 by graph_descendant_of; S5 reads R's tree (N1 newer format or tC4 team format, N2 project id, N3 packs HEAD lacks); S6/S7 one sync_merge call; S8 FastForwardRefspec push, RefMoved is a lost race; S9 ls_remote confirm and refs/remotes/origin/{b}; pauses through a Pause trait (Backoff: min(2^n,16) s x U(0.7,1.3), at most 30 s in all). Tests against tc-door43-fake (tests/sync/door43.rs, real folders, burrito probe on every written state): a_first_sync_pushes_the_project_and_confirms_it, two_computers_converge_through_door43 (byte-identical TIT.usfm), no_write_access_is_refused_before_the_fetch_with_nothing_changed, a_fetch_cut_off_mid_pack_changes_nothing (Y1), a_push_whose_reply_is_cut_off_is_sent_not_confirmed_and_the_next_sync_confirms (Y6), a_push_that_loses_a_race_fetches_again_and_keeps_both. Mutation: no can_push check fails the first refusal test at door43.rs:365 (the fetch ran and Door43's hook declined the push) | crates/tc-sync/src/sync.rs
- 2026-10-25 15:35 EDT | S2 session (Claude Code add49644) | note | none | step 5 item 2b: a repository whose branch was deleted (gap-tc4-straggler-sync-safety state 3b) stops the sync before anything is sent, as tc-api SyncBlock::BranchMissing ("Door43's copy of this project has no main branch, so the sync stopped. Nothing changed. Ask the project's owner on Door43 to restore the branch."; en.json, key added to the seven pending lists, golden refusals.json regenerated). The fetch step reads Door43's branch list first, because a fetch does not prune and the fake, like Gitea, reports a repository with no default branch as empty: a missing branch is a first push only when Door43 says empty and this project never fetched it. New machine answer Fetched(Remote::BranchMissing). Test a_sync_against_a_repository_with_no_main_branch_refuses_and_names_the_state; the mutant that treats every missing branch as a first push fails it at door43.rs:534 (it synced and pushed a new main). New translator-visible sentence: tell the owner | crates/tc-sync/src/sync.rs
- 2026-10-25 16:00 EDT | S2 session (Claude Code add49644) | note | none | .tc/sync.json (§3.6, §9.5): tc-store SyncFile { remoteUrl, branch, confirmed { commit, at } } through the actor (ProjectHandle::sync_file, set_sync_file, confirm_sync, the time by the store's clock), the shape history.rs already reads to rebuild origin after G4. tc_sync::connect (step 6's connect, src/connect.rs) sets origin, tc.branch and sync.json; the sync records the confirmed commit after S9. GitRemote::url is public. a_first_sync_pushes_the_project_and_confirms_it now checks sync.json's confirmed commit; the tests connect through tc_sync::connect | crates/tc-sync/src/connect.rs
- 2026-10-25 16:20 EDT | S2 session (Claude Code add49644) | note | none | step 6, join: tc_sync::join(store, client, session, repo, name) (src/join.rs) fetches the remote's default branch into a new history in Store::staging(Join) (tc.branch from it, Q10), applies the N1 gate (newer format, tC4 team repository: JoinError::Blocked), writes every regular file of the tree but foreign dot paths with the store's atomic writes (no checkout), sets the branch and the index to the fetched commit, writes .tc/sync.json, and places the folder with Store::place; the first open mints the device and folds. Store::vfs is public for this. Test a_joined_project_holds_the_teams_work_and_syncs_both_ways: Ana's text, byte-identical TIT.usfm, a device of its own, no restore notice at the first open, then work both ways. Not yet: the §6.4 N1 cut-off join test, share, the username write (Q7, #161) | crates/tc-sync/src/join.rs
- 2026-10-25 16:35 EDT | S2 session (Claude Code add49644) | note | none | step 6, share: tc_sync::share (src/share.rs) creates the repository (POST user or org repos, default branch main), connects the project and syncs; a taken name is ShareError::NameExists. Test share_makes_the_repository_and_pushes_the_project (pushed and confirmed; the same name again refused) | crates/tc-sync/src/share.rs
- 2026-10-25 16:55 EDT | S2 session (Claude Code add49644) | note | none | step 5 item 2 (H18 B, OE 7, delta L152): the intake walks the commits of Door43's branch this history lacks and lists each one without a Tc-Frontier trailer that changed a project file (ingredients/ outside tc/ and dot paths) in SyncReport.other_tools (commit, author, books, paths); nothing is merged from it, the merge tree takes the fold's projection, and once merged the commit is in this history, so it is said once with no stored list. A folder pushed by another tool is the same case. Test a_door43_edit_by_another_tool_is_reported_once: the project keeps its text and Door43's TIT.usfm after the sync is translationCore's; the mutant merge that takes Door43's derived files fails it at door43.rs:733 ("the merge puts translationCore's file back on Door43"); the first version of the test checked only the local file and missed that mutant | crates/tc-sync/src/sync.rs
- 2026-10-25 17:10 EDT | S2 session (Claude Code add49644) | note | none | §9.7 report: SyncReport.received_by (new changes of the received packs by person label: devices grouped by Door43 username, counted from the packs, so the cost is what arrived) and new_conflicts (listed after, not before). Test two_versions_of_one_verse_are_a_listed_conflict_after_sync (INV-OWN-WORK-NEVER-REVERTED: both texts are candidates of one new conflict) | crates/tc-sync/src/sync.rs
- 2026-10-25 17:50 EDT | S2 session (Claude Code add49644) | note | none | step 7, USB: tc_sync::exchange::export (E1-E5: checkpoint before export, HEAD's tree with metadata.json first and directory entries, root carried and foreign entries under .tc-root/, deflate 6, the tc-exchange 1 comment, temp file flushed and renamed to <abbreviation>-<date>-<person>.zip with -2.. on a clash and cleaned names, then read back: every CRC, the burrito's metadata against its entries, every pack) and receive (R1 caps and entry checks, R2 the open project only, R3 format gate, R4 packs through ProjectHandle::receive_packs = the store intake + a one-parent checkpoint "Received work from ...", damaged packs salvaged, R5 carried bytes into the object database, R6/H22 part 3 a derived file its own packs do not make is named in hand_edited and not taken; skipped when a pack is damaged). Report: intake, received_by, new_conflicts, damaged, hand_edited. Tests (tests/sync/exchange.rs): usb_both_directions (byte-identical TIT.usfm), exchange_idempotent_and_relayed, a_hand_edited_file_in_an_exchange_file_is_not_taken_in_and_named_once, a_file_of_another_project_is_refused, usb_export_is_a_burrito_with_metadata_first, loss_usb_one_bad_pack_rest_taken. Not built: E5's page-cache bypass (read back through the cache), R2's "create the project from this file", R7's .tc/intake.json, the team-sim transports (item 3-4), the U1/U2 crash rows | crates/tc-sync/src/exchange.rs
- 2026-10-25 18:05 EDT | S2 session (Claude Code add49644) | note | none | crash rows Y2-Y4 (and I1-I2, U1-U2 by the same store path, receive_packs being the same intake in a checkpoint): crash_intake_folds_whole_batches_only_and_commits_on_retry (tc-store tests/store/intake.rs) kills sync_merge at every write point under both power-loss models: this computer's 1:1 always stays (INV-OWN-WORK-NEVER-REVERTED), Juan's two verses are both there or both absent (INV-RECEIVED-WORK-IS-KEPT-WHOLE-OR-ABSENT), the reopened folder is a valid burrito with valid commits (reopen_checked), and a retry takes the work | crates/tc-store/tests/store/intake.rs
- 2026-10-25 18:15 EDT | S2 session (Claude Code add49644) | note | none | J26 three_devices_converge_with_byte_identical_derived_files (INV-SYNC-CONVERGES): Ana shares, Juan and Maria join, each saves a verse, five syncs; all three hold all three verses, and every file under ingredients/ and metadata.json is byte for byte the same on the three computers | crates/tc-sync/tests/sync/door43.rs
- 2026-10-25 19:00 EDT | S2 session (Claude Code add49644) | verify | 5df85645 | one review (all three lenses, read-only, fast-release ruling): fix_first, three majors: (1) the merge kept HEAD's foreign entries over Door43's, so a README or style guide edited or deleted on Door43 was reverted silently; (2) a store merge that failed after its pack-by-pack intake was reported BeforeIntake while the received work showed (PLAN Q8); (3) receive counted work it did not take (a skipped checkpoint, read-only or second writer). Minors: export from an older commit when the checkpoint was skipped; E5 page-cache bypass not built; the hand-edit check may name a good file when quarantined lines change a projection; join on case-insensitive disks and reserved names; connect takes the branch from its caller (master); a deleted branch is pushed again after a .git rebuild. Card tests not written: listed in the review and below | .audit/roadmap/units/X.md
- 2026-10-25 19:05 EDT | S2 session (Claude Code add49644) | fix | none | (1) the merge tree takes Door43's blob for every foreign entry and removes one Door43 deleted since the common ancestor unless this computer changed it (Git::merge_base); test a_file_the_app_does_not_own_follows_door43 fails on the old code ("the edit made on Door43 stays"). (2) the executor answers TakenIn when any received change is held after a failed merge, so the machine reports AfterIntake; test a_merge_that_fails_after_intake_says_the_work_was_kept fails without it (BeforeIntake). (3) receive refuses a skipped intake (ExchangeError::NotCommitted) and counts only changes held after it; test a_file_received_into_a_read_only_project_is_refused. Export refuses a checkpoint skipped for any reason but NoChanges. The sync-model-long nightly step and its ignored-tests line are added. Left for X: the remaining minors above; tests a_tc4_team_repository_is_blocked_and_nothing_changes, a_sync_never_rewrites_or_deletes_moved_json, a_save_during_every_sync_step_is_kept_and_pushed_next, crash_at_every_sync_step_converges_on_retry, push_is_never_forced, a_teammates_audio_file_arrives_byte_for_byte, loss_usb_damaged_carried_entry_not_recorded, usb_export_verify_reads_device, a_join_cut_off_leaves_only_a_staging_folder_the_next_start_removes; Q7 username; the team-sim transports; a_sync_costs_what_arrives_not_the_project; R2 receive into a new project; .tc/intake.json; then scripts/gates.sh | crates/tc-store/src/checkpoint.rs
- 2026-10-26 10:30 EDT | S2 session (Claude Code add49644) | note | none | trimmed list (orchestrator, after the fast-release ruling): Q7 username (tc_sync::record_username; .tc/sync.json username_device: written once after a sync and the name notice, a copied one replaced by the first signed-in sync on the new computer, then first wins; tests username_written_once_after_first_successful_fetch with share_no_username_without_consent folded in, copied_username_is_replaced_by_the_first_signed_in_fetch); R2 receive into a new project (exchange::receive_new; a_newcomer_creates_the_project_from_a_file); a_tc4_team_repository_is_blocked_and_nothing_changes, a_sync_never_rewrites_or_deletes_moved_json, push_is_never_forced; I1-I2 crash_usb_receive_takes_whole_batches_and_the_rest_on_retry (tc-store). U1-U2: export writes a temporary name and renames, so a kill leaves no file at the final name and the project unchanged; no kill test. Skipped as follow-ups (parked): team-sim transports, E5 page-cache bypass, the §15 list, the op-count test, a_save_during_every_sync_step_is_kept_and_pushed_next and crash_at_every_sync_step_converges_on_retry beyond the model, Y1 and Y6 tests | crates/tc-sync/src/username.rs
- 2026-10-25 08:30 EDT | S2 session (Claude Code add49644) | note | none | owner ruling (rulings/2026-10-24-H5.md, "recreate it"): a sync whose Door43 branch was deleted no longer stops; the fetch finds the branch gone and the push creates it again from this copy (a create, not a force push), and SyncReport.recreated is true once, for E1a's toast "The project was missing from Door43, so tC4 put your copy back. Ask your teammates to sync." SyncBlock::BranchMissing and its string are removed (tc-api, en.json, the pending lists, golden refusals.json). Test a_sync_against_a_repository_with_no_main_branch_recreates_it_and_says_so replaces the refusal test. | crates/tc-sync/src/sync.rs
- 2026-10-25 08:30 EDT | S2 session (Claude Code add49644) | note | none | nightly model run 40085200557 (main 33d2378d) replayed on track-X with batch 1: a held book file's notice (the manifest's kept entry) is lost with .tc/materialized.json, and the next open writes the projection over the file; its words are a version in History and its bytes a copy in .tc/external, so no work is lost, only the notice. Handed to D1 (checks lane), which owns kept entries: side-session-notes/nightly-40085200557.md. | .audit/roadmap/units/X.md
- 2026-10-25 08:40 EDT | S2 session (Claude Code add49644) | verify | 6f436230+wt | lens races and protocol (reviewer: Opus subagent, read-only): no blocker, no major; merge with three fixes taken: a USB receive that fails after some packs were folded says so (ExchangeError::PartlyReceived, PLAN Q8), `recreated` is set per round and only after the push, receive_new skips the zip's .git/ and .tc/. Minors logged in .audit/minors.tsv (block after intake loses its type, confirm compares HEAD not the pushed commit, sync.json written outside the actor, no single-flight guard, small cases). Code: FastForwardRefspec, a total machine, the BeforeIntake/AfterIntake split; Checks: model and race tests deterministic and in CI; Reach: all reachable but the two-syncs-at-once (app wiring); Siblings: Q8's pattern was missing in exchange, join's dot-path filter in receive_new; No loss: none found; Prose: a stale NotFastForward comment fixed | crates/tc-sync
- 2026-10-25 08:40 EDT | S2 session (Claude Code add49644) | verify | 6f436230+wt | lens parity (reviewer: Opus subagent, read-only, tC4 at d310bf3; no screen in this unit, so no Playwright): fix_first on one item, taken as a declared difference: the Door43 username reaches the team one sync late, because the name notice (consent, Q7) comes after the first confirmed sync and the name goes out with the next push; tC4 writes it inside the sync. Fixes taken: `recreated` only after a successful push (tC4 reports after the push), the gate's order is tC4's (signed in, online, then the repository), STORAGE §9.1 names the recreate ruling, §9.6 and §9.7 refreshed. Minor logged: a second share always creates. Same as tC4: offline, no write access, busy, never forced, nothing new, share's owner/branch/visibility. Declared: six retries (Q11), tc.branch (Q10), fetch-and-merge (C1), other tools kept (H18 B), carried files (H19 A), tC4 team repository blocked, private repositories (Q3), OBS (Q2), both verse texts kept (Q1, Q4). H5's replaced-list is a later field once C7b's fold says what newest-wins replaced | crates/tc-sync
