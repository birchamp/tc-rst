---
id: LOSS-2
title: Fix two store losses the model test wrongly excused
track: A
lane: storage
milestone: M1
state: done
owner: storage lane (Claude Code LOSS-2)
branch: track-LOSS-2
worktree: /home/bot/tc-rst/.worktrees/track-LOSS-2
depends: []
needs_owner: []
proves: [P1]
no_loss: true
size: 1-2 d (basis: two named model cases, three store paths named by the review)
merged: 5e3da206
updated: 2026-10-27
next: none
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
- 2026-10-26 23:55 EDT | storage lane (Claude Code LOSS-2) | step 1 | none | case 1 (pack): real loss. Session 2 sealed zq2x (K3 blob in its PackInfo only), died before a commit; session 3 read the pack at open with `blob: None` and `seen: None`, a same-size flip went unseen, and the next open quarantined it with nothing to restore from. Case 2 (pending): no loss. The flip lands before the reading session's open; that open finds the damaged line and keeps the book file that still shows the save as `TIT.usfm.lost` with *Bring it back* (N4). The model failed it only under 2d912e4a's oracle, because FaultFs's flip replaced the file, so the open's handle still read the good bytes and nothing was kept, and the kill took them (the model's older `Fate::InMemory` rule) | TC_MODEL_TRACE runs of both named cases
- 2026-10-26 23:58 EDT | storage lane (Claude Code LOSS-2) | step 2 | none | store: the open stores a blob of each valid pack no blob or HEAD holds (fails without it); open, seal and late-found packs record size and mtime, and the same-size shortcut is gone; a pending file's bytes as the open read them are kept, and K1 takes a lost line from them when the handle reads damage (new case `a_superseded_save_read_at_open_and_changed_in_place` fails without it). A manifest-blob restore source was tried and dropped: no reachable history needs it (250 kill points). STORAGE §5.11 K1 updated | cargo test -p tc-store --test store -- a_flipped_ a_superseded
- 2026-10-26 23:59 EDT | storage lane (Claude Code LOSS-2) | step 3 | none | oracle: both widenings removed (flip back to `by_another_program`, the Destroyed branch gone); FaultFs gets `change_in_place` so a flip is in place for handles too. The older `Fate::InMemory` rule (outside damage plus a kill before the next seal) is untouched; asked the orchestrator whether it stands, since an inherited pending save can be 30 min old | message to the orchestrator, 2026-10-26
- 2026-10-27 00:20 EDT | storage lane (Claude Code LOSS-2) | verify | d846ba12 | loss lens (independent agent): no blocker; majors: salvaged pending files kept no verified bytes, and a fingerprint mismatch at the open's blob store left `seen` standing; minors logged | review of 25035dfe..d846ba12
- 2026-10-27 00:20 EDT | storage lane (Claude Code LOSS-2) | verify | d846ba12 | races lens (independent agent): no blocker; the seal's `seen` came from a stat after the read-back; minors logged | review of 25035dfe..d846ba12
- 2026-10-27 00:20 EDT | storage lane (Claude Code LOSS-2) | verify | d846ba12 | browser and parity lens (independent agent): nothing a translator sees changes (no new notice or wording); STORAGE §5.11 matches; one line rewrapped | review of 25035dfe..d846ba12
- 2026-10-27 00:40 EDT | storage lane (Claude Code LOSS-2) | fix | cb8a47a4 | both majors and the stat fixed; the orchestrator relayed the owner's ruling that the model's InMemory excuse may cover no save older than 2 minutes. So: an open that finds pending files checkpoints before it returns (STORAGE §6.1 O15), the actor checkpoints once a save waited 2 minutes (§5.13), and the fold reads pending values from the verified bytes (in-memory handle; replaces the kept-bytes field). Model: `Fate::InMemory` only for saves at most 2 minutes old; new cases `an_inherited_save_older_than_two_minutes`, `an_own_save_older_than_two_minutes` fail without each fix. Checkpoint tests whose premise was an unsealed pending file read at open were restaged before the open, and two that only covered it were removed | cargo test -p tc-store --test store -- model:: checkpoint::
- 2026-10-27 01:10 EDT | storage lane (Claude Code LOSS-2) | verify | cb8a47a4 | loss and races re-review (independent agent): majors fixed: the open waited for a whole checkpoint, and files no seal can take kept triggering seals; minors: clock set back, the wording of the 2-minute commit, pack and commit volume | review of d846ba12..cb8a47a4
- 2026-10-27 02:30 EDT | storage lane (Claude Code LOSS-2) | fix | 9fd96438 | the open seals what it inherited after O9, inside a write group O10 closes (no commit at open); `Journal::has_unsealed` counts only sealable files; tests that staged pending files before an open restaged in the session under test or checked on a copy (consolidation harness, crash, review, tc4, placement, salvage probes) | cargo nextest run -p tc-store
- 2026-10-27 03:00 EDT | storage lane (Claude Code LOSS-2) | note | none | found on main: a checkpoint while a book file another program holds is unreadable, after a kill past its rename, leaves metadata.json listing bytes not on disk (the folder is already invalid after the open). The orchestrator split it into card BURRITO-HOLD (checks lane, option a: valid commit, the folder heals, nothing leaves the app from it). LOSS-2 skips the open's seal while O9 could not read a derived file (STORAGE §6.1 O15) | message from the orchestrator, 2026-10-27
- 2026-10-27 03:20 EDT | storage lane (Claude Code LOSS-2) | note | none | packs stay bounded under the 2-minute seal: `tc_journal::consolidate::consolidation_groups` (crates/tc-journal/src/consolidate.rs:32), run by the seal's K6 (`Project::consolidate`, crates/tc-store/src/seal.rs), merges every earlier month into one pack per 64 MiB and the current month's packs under 1 MiB once there are more than 32. Commits are not merged, so the 2-minute seal commits nothing (`Actor::seal_now`): its packs wait as K3 blobs for the next ordinary checkpoint | crates/tc-journal/src/consolidate.rs
- 2026-10-27 03:40 EDT | storage lane (Claude Code LOSS-2) | verify | 9fd96438 | final review (independent agent): no loss; major fixed: a seal error at the open turned the session read-only (test `a_seal_that_fails_at_the_open_keeps_the_project_writable` fails without the fix); minors fixed or logged | review of cb8a47a4..9fd96438
- 2026-10-27 05:10 EDT | storage lane (Claude Code LOSS-2) | fix | 33ad3595 | the open's write group never closed (the flag was taken by the wrong condition), which kept the store's own I7 probes off for the session; p5 found it in the gate | scripts/gates.sh --touched
- 2026-10-27 05:30 EDT | storage lane (Claude Code LOSS-2) | gate | 33ad3595 | scripts/gates.sh --touched passed: 5 crates, 1577 tests, every step | scripts/gates.sh --touched
- 2026-10-27 06:40 EDT | storage lane (Claude Code LOSS-2) | fix | e6fdf3d7 | held from batch 31 by the integration lane: clippy type_complexity in fault.rs (a type alias now), and INV-NO-EXCUSED-LOSS and INV-PACK-RESTORE named in no test (now in the model's assert messages); main fb960700 merged (REL-1, DONE and ROADMAP depends taken as the union). Workspace clippy (Linux and Windows) clean; claims passes with LOSS-2 set done | cargo clippy --locked --workspace --all-targets (both targets); node --test .audit/tools/claims.test.mjs
- 2026-10-27 00:51 EDT | integration lane (Claude Code 3d07d5f8) | merge | 5e3da206 | merged into main in batch 31; the held oracle widening 7fbbed45 never reached main; gated by scripts/gates.sh --wait-ci batch-31 | scripts/gates.sh --wait-ci batch-31
