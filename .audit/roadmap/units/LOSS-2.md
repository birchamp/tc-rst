---
id: LOSS-2
title: Fix two store losses the model test wrongly excused
track: A
lane: storage
milestone: M1
state: claimed
owner: storage lane (Claude Code LOSS-2)
branch: track-LOSS-2
worktree: /home/bot/tc-rst/.worktrees/track-LOSS-2
depends: []
needs_owner: []
proves: [P1]
no_loss: true
size: 1-2 d (basis: two named model cases, three store paths named by the review)
merged: none
updated: 2026-10-26
next: Step 1, run both named cases on this branch and on batch/27's case.
blocked_by: none
---

# LOSS-2: Fix two store losses the model test wrongly excused

## Goal

`model::random_histories_lose_no_work` lost a save twice on 2026-10-26, and both times the
oracle was widened to excuse it (2d912e4a on main, 7fbbed45 in batch 27). After this card both
saves survive, or come back as *Bring it back*, and the oracle excuses only what STORAGE §4.4
and `rulings/2026-10-22-acceptable-loss.md` accept.

## Why it matters to a translator

A pack or pending file damaged in place by another program must not take a save with it when
the app still has a good copy (a stored blob, or the book file that shows the text).

## Before you start

- `git log --oneline -1 2d912e4a` is on main; `git show 7fbbed45` is on `batch/27`.
- Read STORAGE §4.4, §9.4 (K3 blobs), §15 L65/L66, and the acceptable-loss ruling.

## Funnel card

- **Invariants**:
  - INV-PACK-RESTORE: an expected pack that fails P2-P4 is restored from any valid blob at its
    path, including the blob stored when it was first read or written (§4.4, §9.4).
  - INV-NO-EXCUSED-LOSS: the model's oracle marks a save destroyed only when §4.4 or the
    ruling accepts it.
- **Wrong states**:
  - A same-size byte flip in a pack goes unseen and is committed.
  - A pack read at open has no stored blob, so a later flip cannot be restored.
  - A flipped pending file loses a save the book file still shows.
- **Code**:
  - Types: none expected.
  - One entry point: the expected-pack check in `tc-store` (packs.rs, open.rs, checkpoint.rs).
  - Removed: the oracle branches that excused the two cases.
  - Left to checks, and why: none.
- **Checks**:
  - `a_flipped_pack_after_a_cut_pending_and_a_reverted_book` in `crates/tc-store/tests/store/model.rs`; kind: named model history; runs in: CI.
  - `a_flipped_pending_file_under_a_session_that_saves_again` in the same file; same.
  - Proof it can fail: each fails with the store fix reverted and the narrowed oracle kept.

## Steps

### Step 1: Run both cases and find what is lost and why

- Done when: the card's log names the lost save and the code path for each case.

### Step 2: Fix the store

- Store a blob for a pack when it is first read; restore from the manifest's or K3's blob;
  recheck the same-size shortcut.
- Done when: both named cases pass with the narrowed oracle.

### Step 3: Narrow the oracle

- Done when: the oracle's destroyed branch cites the rule it follows.

## Gates before merge

`scripts/gates.sh --touched`, three verify lines (loss first), then the integration lane.

## Stop and ask when

- A case truly is accepted by a rule: tell the orchestrator in one line with rule and line.

## References

- Orchestrator message, 2026-10-26; commits 2d912e4a, 7fbbed45.

## Log

- 2026-10-26 23:30 EDT | storage lane (Claude Code LOSS-2) | created | none | from the orchestrator: two model losses excused by widening the oracle; an independent review disagrees | message from the orchestrator, 2026-10-26
- 2026-10-26 23:30 EDT | storage lane (Claude Code LOSS-2) | claim | none | claimed on track-LOSS-2 from origin/claude/focused-ride-5uzj7p | python3 -I .audit/tools/board.py
