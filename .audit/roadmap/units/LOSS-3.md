---
id: LOSS-3
title: A save a crash left unsealed is sealed at once when the open cannot read a book file
track: A
lane: storage
milestone: M1
state: building
owner: storage lane (Claude Code LOSS-3)
branch: claude/bold-volta-4r0w6t
worktree: none
depends: []
needs_owner: []
proves: [P1]
no_loss: true
size: 0.5 d (basis: two shrunk nightly histories, one actor change)
merged: none
updated: 2026-10-29
next: run the three review lenses (loss first), then scripts/gates.sh --touched and the long model.
blocked_by: none
---

# LOSS-3: A save a crash left unsealed is sealed at once when the open cannot read a book file

## Goal

`store-model-nightly` failed every night from 2026-10-24 (translationCore4 issue 847): a save the
app reported as kept was gone from History. After this card a save an earlier session left in a
pending file is sealed before the first command of the next session, even when the open could
not read a derived file.

## Why it matters to a translator

A save made just before a crash may wait, the next time the project opens, in a pending file
another program can damage. If a book file was held by another program at that open (a cloud
client, a scanner), the save waited up to 2 more minutes; a damaged pending file and a crash in
that window lost it, however old it was.

## Funnel card

- **Invariants**:
  - INV-NO-EXCUSED-LOSS: a save of unknown age never waits for the 2-minute seal (STORAGE §6.1
    O15, ruling 2026-10-22-acceptable-loss).
- **Wrong states**:
  - The open skips its seal (O9 could not read a derived file) and the actor's 2-minute wait
    starts at the open, so an inherited save waits 2 more minutes in a pending file.
- **Code**:
  - `crates/tc-store/src/actor.rs`: the actor starts with inherited pending files already due,
    and checks before its first command.
- **Checks**:
  - `an_inherited_name_unsealed_while_the_book_file_is_held_is_sealed_at_once` and
    `an_inherited_verse_unsealed_while_the_book_file_is_unreadable_is_sealed_at_once` in
    `crates/tc-store/tests/store/model.rs` (the issue's two shrunk histories, also named
    histories of every model run); each fails without the actor change.

## Log

- 2026-10-29 | storage lane (Claude Code LOSS-3) | step 1 | none | Both shrunk histories of issue 847 fail every run (not a race). Session 2 opens with `TIT.usfm` held, so O15 skips the seal; session 3 opens the same way, the pending file is cut in place, and another program replaces the last file that showed the save (`metadata.json` for the name, the book file deleted for the verse). The save is only in session 3's memory, the 2-minute seal has not fired (its wait began at the open), and a kill takes it. A store loss, not an oracle error: O15 says an inherited save's age is unknown | TC_MODEL_TRACE runs of both histories
- 2026-10-29 | storage lane (Claude Code LOSS-3) | step 2 | none | Actor: inherited pending files are due at spawn and `seal_when_due` runs before the first message. A seal that failed at the open is retried at once too (`a_seal_that_fails_at_the_open_keeps_the_project_writable` now asserts the actor's seal took the file). `a_group_keeps_the_intent_of_a_path_it_could_not_read` now stops the rig's probe as its sibling `held_after_a_kill_past_the_rename` does: the seal's group closes while the kill's metadata.json is still behind the rename, which I7 allows until a read decides; the test still asserts a valid folder at the end. STORAGE §6.1 O15 updated. tc-store: 423 integration tests pass; the lib's `native_link_fallback_keeps_the_installation_when_source_cleanup_is_denied` fails only as root | cargo test -p tc-store --no-fail-fast
