---
id: BURRITO-HOLD
title: A book file the app cannot read never leaves metadata.json listing bytes that are not there
track: A
lane: storage
milestone: M1
state: gating
owner: storage lane (Claude Code e10d898d)
branch: track-BURRITO-HOLD
worktree: .worktrees/track-BURRITO-HOLD
depends: []
needs_owner: []
proves: [P5]
no_loss: true
size: 0.5-1 d (basis: one listing rule in materialize.rs, one crash test)
merged: none
updated: 2026-10-27
next: Pushed; the integration lane takes it into a batch with LOSS-2 (this branch holds LOSS-2 at 5e3da206).
blocked_by: none
---

# BURRITO-HOLD: A book file the app cannot read never leaves metadata.json listing bytes that are not there

## Goal

A checkpoint that runs while a book file cannot be read (another program holds it, a cloud
placeholder), after a kill past that file's rename, writes `metadata.json` listing the bytes
before the write, while the bytes after it are on disk: the folder is not a valid Scripture
Burrito. After this card no checkpoint lists md5 and size that are not in the folder (STORAGE
§3.4, §5.10 M2, I7).

## Why it matters to a translator

The owner's ruling (`rulings/2026-10-22-acceptable-loss.md`): a corrupted project is never
acceptable. Paratext, Door43 and another tC4 trust `metadata.json`.

## Before you start

- Repro: `groups::a_group_keeps_the_intent_of_a_path_it_could_not_read` plus one checkpoint
  while `TIT.usfm` is unreadable (minor on track-LOSS-2, `.audit/minors.tsv`).
- Read STORAGE §3.4, §5.10 (M2, M6), §6.2 and §6.4 row M4.

## Funnel card

- **Ruling** (orchestrator, 2026-10-27, option B): the state is an interrupted write group until
  the file reads (STORAGE I7). The holder is another program, so only a read decides which bytes
  are on disk; no guess from the file's size (two versions can have the same size).
- **Invariants**:
  - INV-VALID-BURRITO (P5, STORAGE I7): no write lists md5 and size the folder did not already
    claim; every commit and exchange file is valid; the folder heals at the first open or
    checkpoint that reads the file.
- **Wrong states**:
  - A path in `Writing{old, new}` whose file cannot be read is listed as `old`: after a kill
    past `metadata.json` (§6.4 M5) the folder was valid, and the checkpoint made it invalid.
  - A commit or exchange file built from the folder's stale list.
- **Code**:
  - Types: none.
  - One entry point: `Writer::metadata_entries` in `crates/tc-store/src/materialize.rs`: an
    undecided path keeps the entry `metadata.json` has for it now (`listed_in_metadata`).
  - Removed: the "listed as the bytes before the write" fallback.
  - Left to checks, and why: commits and exchange files already come from the projection
    (`build_tree`, `tc_sync::exchange::export` takes HEAD after a checkpoint): no code, a test each.
- **Checks** (`crates/tc-store/tests/store/groups.rs` unless named):
  - `a_checkpoint_that_cannot_read_a_book_file_keeps_a_valid_folder_valid`: the fix.
  - `a_checkpoint_while_a_book_file_is_undecided_commits_a_valid_burrito`: proof (1).
  - `a_checkpoint_after_the_held_book_file_reads_again_heals_the_folder`,
    `an_open_after_the_held_book_file_reads_again_heals_the_folder`: proof (2).
  - `exchange::an_export_while_a_book_file_is_undecided_is_a_valid_burrito` in
    `crates/tc-sync/tests/sync/exchange.rs`: proof (3); the sync push sends the same commits.
  - Proof it can fail: each fails with its guard planted away (log).

## Steps

### Step 1: Tests and fix

- Done when: the five tests pass, each fails with its guard planted away, STORAGE §1 I7, §5.10
  M2 and §15 L153 say the rule.

### Step 2: Merge LOSS-2

- Done when: track-LOSS-2 is merged in and the tc-store tests pass.

## Gates before merge

`scripts/gates.sh --touched`, three verify lines (loss first), then the integration lane.

## Stop and ask when

- Proof (3) needs real work: tell the orchestrator in one line.

## References

- Orchestrator message, 2026-10-26; minor on track-LOSS-2 (`.audit/minors.tsv`).

## Log

- 2026-10-27 00:50 EDT | storage lane (Claude Code e10d898d) | created | none | from the orchestrator: a checkpoint while a book file is unreadable, after a kill past its rename, lists bytes not on disk; REL-1 waits for it | message from the orchestrator, 2026-10-26
- 2026-10-27 00:50 EDT | storage lane (Claude Code e10d898d) | claim | none | claimed on track-BURRITO-HOLD from origin/claude/focused-ride-5uzj7p (30ce3573) | python3 -I .audit/tools/board.py
- 2026-10-27 02:10 EDT | storage lane (Claude Code e10d898d) | ruling | none | orchestrator, option B: an interrupted write group until the file reads (I7, the holder is another program); no guess from the size; prove (1) commits valid, (2) the folder heals at the first open or checkpoint that reads the file, (3) nothing leaves the app built from the folder | message from the orchestrator, 2026-10-27
- 2026-10-27 02:40 EDT | storage lane (Claude Code e10d898d) | step 1 | none | Found: after a kill past TIT.usfm's rename (§6.4 M4) the folder is invalid before any checkpoint, as I7 allows; worse, after a kill past metadata.json (M5) the folder is valid and a checkpoint that cannot read TIT made it invalid by listing the bytes before the write. Fix: an undecided path keeps the entry metadata.json has for it now (`listed_in_metadata`); not listing it instead left the seal's new pack unlisted. (3) needed no code: the exchange file and the push are HEAD's commit after a checkpoint, whose tree comes from the projection. Plants, each reverted: the old `old`-placement fallback fails keeps_a_valid_folder_valid (TIT listed 584, disk 585); build_tree taking the folder's metadata.json fails (1); reads that never decide a `Writing` path fail the open heal, and with the listing also taking metadata.json's entry for a readable file the checkpoint heal fails too (two reads heal it, both planted); the export without its checkpoint fails (3). STORAGE I7, §5.10 M2 and §15 L153 updated. tc-store: 488 passed | cargo nextest run -p tc-store; cargo nextest run -p tc-sync --test sync an_export_while
- 2026-10-27 03:30 EDT | storage lane (Claude Code e10d898d) | verify | eee0a0fe | loss lens (independent agent): no blocker, no major; replanted the old fallback (fails), probed kills before and after metadata.json, a save or none, a reopen while held, metadata.json deleted by another program: the folder never got worse and healed once TIT read; siblings (held, carried, fixed files, build_tree, exchange, O10) cannot make a valid folder invalid; 2 minors logged | .audit/roadmap/evidence/2026-10-27/BURRITO-HOLD-lens-loss.md
- 2026-10-27 03:30 EDT | storage lane (Claude Code e10d898d) | verify | eee0a0fe | races lens (independent agent): no blocker, no major; an outside metadata.json write between the group's reads and its rename is copied or refused by M6's last look, so no written list carries replaced bytes; one LOCK writer; cost one read and parse per undecided path; 2 minors logged | .audit/roadmap/evidence/2026-10-27/BURRITO-HOLD-lens-races.md
- 2026-10-27 03:30 EDT | storage lane (Claude Code e10d898d) | verify | eee0a0fe | browser and parity lens (independent agent): no blocker, no major; nothing a translator sees changes (no notice or tc-app path reads the ingredient list), no tC4 difference to declare, no journey needed; its 2 minors (STORAGE did not say what happens with no entry; a card id in I7) fixed in the docs | .audit/roadmap/evidence/2026-10-27/BURRITO-HOLD-lens-parity.md
- 2026-10-27 05:10 EDT | storage lane (Claude Code e10d898d) | step 2 | ab827b0e | merged track-LOSS-2 at 5e3da206 (on main fb960700); conflicts only in the depends lists of DONE, REL-1 and ROADMAP's REL-1 row (both cards kept); groups:: 24 passed, the tc-sync export test passed | cargo nextest run -p tc-store --test store groups::
- 2026-10-27 05:40 EDT | storage lane (Claude Code e10d898d) | gate | none | scripts/gates.sh --touched on the merged tree: 5 crates, 0 journey files, 1587 tests passed (7 slow), exit 0; clippy, clippy-release-bins and clippy-release-app pass after two needless clones in the export test were dropped; Windows clippy left to track CI | scripts/gates.sh --touched
