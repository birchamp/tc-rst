---
id: POEM-TAB
title: Poem indents are two characters wide, and Tab indents in the Verse editor
track: A
lane: app
milestone: M5
state: verifying
owner: tc4-issues session (Claude Code dc817a96)
branch: track-POEM-TAB
worktree: .worktrees/track-POEM-TAB
depends: []
needs_owner: []
proves: []
no_loss: true
size: 0.5 d (basis: REF1b)
merged: none
updated: 2026-10-27
next: Step 3: the three review lenses (loss, races and protocol, browser and parity), then main merged in and scripts/gates.sh --touched.
blocked_by: none
---

# POEM-TAB: Poem indents are two characters wide, and Tab indents in the Verse editor

## Goal

In Translate's Verse and Section editors one poem level (one leading tab) is two characters wide (`tab-size: 2ch`), and Tab / Shift+Tab in the Verse editor indent and unindent the line as the Section editor does, instead of leaving the field.

## Why it matters to a translator

Today a level is 8 characters wide, so poem lines start a third of the way across the card; and Tab in the Verse editor closes the card, so what the translator types next never reaches the verse. Reproduced on main 58e8e10b: `docs/evidence/tc4-issues-2026-10-27/repro/829-poem-tabs.test.mjs`.

## Before you start

- Nothing in `crates/tc-ui` sets `tab-size`.
- The Section editor's Tab handler: `indentLine` use in `crates/tc-ui/src/views/section_editor.rs` (about line 442); the Verse editor is `VerseEditor` in `crates/tc-ui/src/views/translate.rs`.

## Funnel card

- **Invariants**: none new.
- **Code**: tc-ui: `tab-size: 2ch` on both textareas; the Verse editor reuses the Section editor's Tab handler. The saved bytes do not change.
- **Checks**: Journey: both editors compute `tab-size` 2ch; Tab in the Verse editor indents and keeps the focus, and the typed text is saved at the new level.
  - Proof it can fail: the test fails on main 003aa087.
- **Prose**: no new words unless the step says so.
- **Human**: none.

## Steps

### Step 1: A gate test that fails on main
### Step 2: The fix
### Step 3: Review lens(es) the card needs, then `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; the `verify` lines FUNNEL §8 asks for.

## Stop and ask when

- The fix would change what a translator sees beyond the issue (a new word, a new flow).

## References

- tC4 #829; `docs/evidence/tc4-issues-2026-10-27/open.md`.

## Log

- 2026-10-27 09:10 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: fix the 4.1.0 blockers the tC4 issue sweep found | docs/evidence/tc4-issues-2026-10-27/open.md
- 2026-10-27 09:10 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed in its worktree on track-POEM-TAB | .worktrees/track-POEM-TAB
- 2026-10-27 09:25 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | journeys fail on main 003aa087's code: tab_indents_a_poem_line_in_the_verse_card_and_escape_saves_it "expected: '2ch' actual: '8'" (and, with that line left out, "Tab keeps the focus" expected 'Verse 4' actual null); paragraph_and_poetry_lines_from_the_card_land_in_the_verse_before "expected: '2ch' actual: '8'"; tc-api the_verse_card_indents_its_inner_lines_and_never_its_first does not compile without indent_verse_line | TC4_BIN=… node --test --test-name-pattern='tab_indents_a_poem_line|paragraph_and_poetry_lines_from_the_card' parity/app/journeys/translate.test.mjs parity/app/journeys/section.test.mjs
- 2026-10-27 10:03 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | both cards' fields have tab-size 2ch; the verse card's Tab and Shift+Tab go through the section card's handler (section_editor::tab_indent) with tc-api indent_verse_line (inner lines up to three tabs, the first line none), Escape leaves the field and so saves and closes the card; declared difference in APP-DESIGN §6.10; parity residuals for the two editor captures name the extra longhand. translate.test.mjs + section.test.mjs + coverage.test.mjs 42 pass, 0 skipped; tc-api section 48 pass; tc-ui all pass | TC4_BIN=… node --test parity/app/journeys/translate.test.mjs parity/app/journeys/section.test.mjs; cargo test -p tc-api --test section; cargo test -p tc-ui
