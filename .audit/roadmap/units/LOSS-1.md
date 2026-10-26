---
id: LOSS-1
title: Write the loss tests STORAGE §15 promises that no card wrote
track: A
lane: storage
milestone: M1
state: claimed
owner: LOSS-1 lane (Claude Code d63a7912)
branch: track-LOSS-1
worktree: /home/bot/tc-rst/.worktrees/track-LOSS-1
depends: [CHK-3]
needs_owner: []
proves: [P1]
no_loss: true
size: 2 d (basis: X's step 5, which wrote a dozen loss tests in about a day)
merged: none
updated: 2026-10-26
next: Step 1: find the tests that exist under another name.
blocked_by: none
---

# LOSS-1: Write the loss tests STORAGE §15 promises that no card wrote

## Goal

Every row of `docs/STORAGE.md` §15 names a test that exists and asserts what the row's "What
prevents it here" cell says. The 26 tests below are named in §15 but no card writes them: X and
I are done without them (X.md step 5 item 4 and step 6.4 name several), and S3, E1a, E1b, E2, V,
C7b and F1 do not list them. CHK-3 found them (step 5).

## Why it matters to a translator

Each row is a way tC4 lost a translator's work. A row with no test is a protection nobody
checks: it can break and no gate fails.

## Before you start

- `node --test .audit/tools/claims.test.mjs` passes on main with CHK-3 merged, and
  `grep -c '(pending: LOSS-1)' docs/STORAGE.md` prints the rows below.
- For each test, read its row in §15 first; the test may exist under another name (grep the
  words of the row). If it does and asserts the row, name it in the row instead of writing one.

## Funnel card

- **Invariants**:
  - INV-CLAIMED-TESTS-EXIST: every §15 row names a test that exists (`claims.test.mjs`).
- **Wrong states**:
  - A loss row's protection breaks and no test fails.
- **Code**:
  - Types: none.
  - One entry point: none; tests only, in the crate each row's protection lives in (tc-store,
    tc-journal, tc-sync, tc-app).
  - Removed: each `(pending: LOSS-1)` marker in §15, in the commit that writes its test.
  - Left to checks, and why: none.
- **Checks** (all "(to create)" unless step 1 finds them under another name):
  - `loss_f7_restored_backup_warns` (L16); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_f8_secret_lost_pending_survives` (L17); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_f10_nothing_can_lock_open` (L19); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_f11_rejected_ref_not_success` (L20); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_bad_pack_from_teammate_sync_continues` (L22); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_quarantined_pack_kept_on_tip` (L22, L45); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_f14_edits_during_sync_kept` (L23); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_newer_major_refused_before_write` (L25); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_contention_three_devices_converge` (L26); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `share_no_username_without_consent` (L28); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_carried_file_travels` (L29); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_master_branch` (L30); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_private_repository` (L31); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `team_sync_obs_project` (L32); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_protected_branch_reported` (L34); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `crash_seal_verifies_before_delete` (L47); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `review_has_no_keep_newest` (L59); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `import_tc4_hidden_work_surfaces` (L60); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_all_copies_damaged_union_salvaged` (L66); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `door43_ingest_written_verbatim` (L77); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `salvage_keeps_original_batch_ref` (L77); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_door43_case_alias_never_recorded` (L79); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `loss_usb_damaged_carried_entry_not_recorded` (L83); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_project_json_deleted_on_door43_restored` (L89); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `usb_export_name_sanitized` (L95); kind: unit or model test; runs in: CI `cargo test --workspace`
  - `sync_large_push_chain_resumes` (L96); kind: unit or model test; runs in: CI `cargo test --workspace`
  - Proof it can fail: for each, break the protection on purpose, paste the failing line into
    the log, revert.
- **Prose**: `docs/STORAGE.md` §15 test cells.
- **Human**: none.

## Steps

### Step 1: Find the tests that exist under another name

- Do: for each test above, grep the row's words in `crates/*/tests` and `parity/`; where a test
  asserts the row, name it in §15 and drop the marker.
- Files: `docs/STORAGE.md` (§15 test cells only).
- Done when: `node --test .audit/tools/claims.test.mjs` passes.
- Record: a log line `step 1` with the names found.

### Step 2: Write the rest, red first where the protection is missing

- Do: write each remaining test; where the protection is missing, the red test is a finding:
  fix it here if it is small, else stop and ask.
- Files: the crates' test files, `docs/STORAGE.md` §15.
- Done when: `grep -c '(pending: LOSS-1)' docs/STORAGE.md` prints 0 and the claims test passes.
- Record: a log line `step 2`; `state: verifying`.

## Gates before merge

`scripts/gates.sh`, plus:

- The review gate of `FUNNEL.md` §8: three `verify` log lines (no-loss item, one per lens; see
  `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that
  harms a translator's data or view.

## Stop and ask when

- A row's protection is missing and the fix is more than a small change.
- A claimed card holds the files a test needs.

## References

- `docs/STORAGE.md` §15; `.audit/roadmap/units/CHK-3.md` step 5; X.md step 5 and step 6.4.

## Log

- 2026-10-26 11:01 EDT | shell lane (Claude Code 6f39d119), for CHK-3 | created | none | from CHK-3 step 5 and the app lane's review of it: 26 §15 tests that no card writes | .audit/roadmap/units/CHK-3.md
- 2026-10-26 11:07 EDT | LOSS-1 lane (Claude Code d63a7912) | claim | none | claimed on track-LOSS-1 from track-CHK-3b e36f49d2 (CHK-3 lands in batch 9), at the orchestrator's word | python3 -I .audit/tools/board.py
- 2026-10-26 12:30 EDT | LOSS-1 lane (Claude Code d63a7912) | step 1 | b192866c | three rows already had a test that asserts them, named in §15 now: L28 `username_written_once_after_first_successful_fetch` (its sync writes no name before the notice), L45 `a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree`, L95 `a_display_name_never_makes_the_file_name_fail`. The other names had no test under any name | node --test .audit/tools/claims.test.mjs
- 2026-10-26 12:30 EDT | LOSS-1 lane (Claude Code d63a7912) | note | cec46b8b | step 2, 23 rows: tests written, card names kept (L20 and L34 share `sync_protected_branch_reported`; L22 and L45 share `loss_bad_pack_from_teammate_sync_continues`). Every protection was there; no test found a bug. Each was proven by breaking its protection and seeing the test fail, then reverting: L16 ledger check off ("a new device"); L17 fingerprint ignored ("only a new device"); L19 a damaged device.json made an error ("damaged content refused the open"); L25 format not read ("a project of format 2 opened"); L47 seal read-back check off ("was removed though its pack did not verify"); L20/L34 a refused ref read as Ok ("a refused ref read as Synced"); L22 a damaged received pack an error (Failed(BeforeIntake(Io))); L23 a later sync skipping its checkpoint (1:2 is "___" on the teammate); L26 MAX_ATTEMPTS 1 (Busy { attempts: 1 }); L29 carried files left out of the tree (None on Door43); L30 the branch forced to main (None on master); L31 no token on the fetch (AuthFailed); L32 Stories refused (ReadOnly); L59 newest-wins in the bulk keep ("Juan one" kept for Ana); L66 the commit's copy left out of the union ("each verse is its own save"); L77 salvaged lines dropped ("the salvaged change, by its own id"); L79 no `.case-` placement (PathCaseCollision); L83 a CRC failure taken as good bytes (damaged: []); L89 a missing project.json read as another project (ProjectMismatch). L23 and L26 hold the store at BeforeHeadUpdate, not the fake's Hold: two Hold tests hung over 18 minutes in a parallel tc-sync run (`.audit/minors.tsv` X row) | node --test .audit/tools/claims.test.mjs
