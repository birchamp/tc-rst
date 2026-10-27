---
id: HOME-SETTLE
title: Home never shows a project's progress from before its last edits
track: A
lane: app
milestone: M5
state: done
owner: tester findings lane (Claude Code 76125a0f)
branch: track-REL1-tester
worktree: .worktrees/track-REL1-tester
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: one guard in home.rs and one test, like viewFromHome's wait)
merged: a3a5bc83
updated: 2026-10-27
next: none
blocked_by: none
---

# HOME-SETTLE: Home never shows a project's progress from before its last edits

## Goal

A 4.1.0-preview tester saw a drafted Genesis at 0% on Home. Going Home answers before the leave
checkpoint writes the derived files, and Home read the old `GEN.usfm` and showed its count as
known, while the book rail (read from the journal) already said 1%. Now a project whose leave
runs shows its counts as unknown ("—", tC4 D30) in `home/list`. `home/card`, which the leave's
`Event::HomeCard` asks for, waits for the leave, as `settings/viewFromHome` already does.

## Why it matters to a translator

The tile said none of today's work was there. It came back a few seconds later, but a
translator who sees 0% after a morning's drafting fears the work is lost.

## Before you start

- `crates/tc-app/src/home.rs` `home_card`, `Handle<List>`, `Handle<Card>`.
- `crates/tc-app/src/session.rs` `settled`, and the leave's `Event::HomeCard` (about line 942).

## Funnel card

- **Invariants**:
  - INV-HOME-COUNTS-AFTER-LEAVE: Home shows a project's counts as known only from files written
    after its leave checkpoint ended.
- **Wrong states**: a known percentage read while the leave runs.
- **Code**: `SessionRegistry::leaving`; `home_card` keeps no summary of a leaving folder;
  `home/card` awaits `settled`. `home/list` does not wait, so Home shows at once (the design's
  `going_home_returns_before_the_leave_checkpoint_ends`).
- **Checks**: `home_shows_no_counts_of_a_project_while_its_leave_runs` in
  `crates/tc-app/tests/app/sessions.rs`; it fails on main 020e0ddf (the list shows the kept
  count during a held checkpoint) and passes with the fix.
- **Prose**: none. **Human**: none.
- Not bugs, checked against tC4 4.0 (`tc4-rc2` `indexer.ts` `draftPercent`): Genesis rounds to
  0% until 8 of its 1,533 verses are drafted, and a book counts only the verses its file holds,
  so the Titus import fixture (all 46 verses drafted) is 100%.

## Steps

### Step 1: The guard and the test
### Step 2: One review; `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; one `verify` line.

## Stop and ask when

- A leave that never ends would hold `home/card`: the leave always publishes and settles.

## References

- Tester report on 4.1.0-preview (relayed by the audit session, 2026-10-27).

## Log

- 2026-10-27 13:00 EDT | tester findings lane (Claude Code 76125a0f) | created | none | Reproduced: a card read during the leave showed a drafted Genesis at 0% while the rail said 1% | crates/tc-app/tests/app/sessions.rs home_shows_no_counts_of_a_project_while_its_leave_runs
- 2026-10-27 13:05 EDT | tester findings lane (Claude Code 76125a0f) | claim | none | Claimed with the fix on track-REL1-tester | none
- 2026-10-27 12:45 EDT | tester findings lane (Claude Code 76125a0f) | verify | 1943b005 | One review (general-purpose agent, FUNNEL §8): pass with minors. Hang: no, every leave and close sets done and a screen-open project returns at once; the leave's HomeCard ends with a fresh read; stale cache: no, the stamp is read before the files. Minors: a one-round-trip flash if the leave ends between peek and check (minors.tsv); the test did not check the counts come back | review report relayed in the session; crates/tc-app/tests/app/sessions.rs
- 2026-10-27 12:45 EDT | tester findings lane (Claude Code 76125a0f) | fix | none | The gate test now also checks the counts are known again after the leave | crates/tc-app/tests/app/sessions.rs home_shows_no_counts_of_a_project_while_its_leave_runs
- 2026-10-27 14:05 EDT | tester findings lane (Claude Code 76125a0f) | gate | eb1c7aa6 | passed scripts/gates.sh --touched with main 829f63f4 merged: 2019 Rust tests, 31 journeys, board, claims and coverage | scripts/gates.sh --touched
- 2026-10-27 14:06 EDT | integration lane (Claude Code 3d07d5f8) | merge | a3a5bc83 | merged into main in batch 47 with the other tester findings, every screen recaptured on the batch tree; gated by scripts/gates.sh --wait-ci batch-47 | scripts/gates.sh --wait-ci batch-47
