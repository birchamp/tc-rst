---
id: BURRITO-HOLD
title: A book file the app cannot read never leaves metadata.json listing bytes that are not there
track: A
lane: storage
milestone: M1
state: claimed
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
next: Fix metadata_entries' rule for a write under way whose file cannot be read; test, plant, merge LOSS-2.
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

- **Invariants**:
  - INV-VALID-BURRITO (P5, STORAGE I7): `metadata.json` never lists md5 and size that are not in
    the folder.
- **Wrong states**:
  - A path in `Writing{old, new}` whose file cannot be read is listed as `old` after the rename
    put `new` there.
- **Code**:
  - Types: none expected.
  - One entry point: `Writer::metadata_entries` in `crates/tc-store/src/materialize.rs`.
  - Removed: the "listed as the bytes before the write" fallback.
  - Left to checks, and why: none.
- **Checks**:
  - `groups::a_checkpoint_that_cannot_read_a_book_file_lists_only_bytes_on_disk` in
    `crates/tc-store/tests/store/groups.rs`; kind: crash test; runs in: CI.
  - Proof it can fail: fails with the old fallback planted back.

## Steps

### Step 1: Test and fix

- Done when: the named test passes, fails with the old rule, and STORAGE says the rule.

### Step 2: Merge LOSS-2

- Done when: track-LOSS-2 is merged in and the tc-store tests pass.

## Gates before merge

`scripts/gates.sh --touched`, three verify lines (loss first), then the integration lane.

## Stop and ask when

- No listing can be proved true while the file is unreadable: ask the orchestrator.

## References

- Orchestrator message, 2026-10-26; minor on track-LOSS-2 (`.audit/minors.tsv`).

## Log

- 2026-10-27 00:50 EDT | storage lane (Claude Code e10d898d) | created | none | from the orchestrator: a checkpoint while a book file is unreadable, after a kill past its rename, lists bytes not on disk; REL-1 waits for it | message from the orchestrator, 2026-10-26
- 2026-10-27 00:50 EDT | storage lane (Claude Code e10d898d) | claim | none | claimed on track-BURRITO-HOLD from origin/claude/focused-ride-5uzj7p (30ce3573) | python3 -I .audit/tools/board.py
