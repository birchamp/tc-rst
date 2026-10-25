---
id: FIX-HIDE2
title: A section draft restored after a reload is shown, ready for Save
track: A
lane: app
milestone: M1
state: verifying
owner: C7a session (Claude Code 3058c665)
branch: track-FIX-HIDE2
worktree: .worktrees/track-FIX-HIDE2
depends: [FIX-HIDE]
needs_owner: []
proves: [P1]
no_loss: true
size: 0.5 d (basis: FIX-HIDE)
merged: none
updated: 2026-10-24
next: The three review lenses (loss first), then the fast checks and the integration lane.
blocked_by: none
---

# FIX-HIDE2: A section draft restored after a reload is shown, ready for Save

## Goal

A section draft that was never saved comes back after a reload (or a panic) where the translator
sees it: its section card opens over the same verses with the typed text, and Save stores it.

## Why it matters to a translator

Today the draft is kept in the browser's buffer but the card stays closed and the grid shows the
stored text, so the translator's typed section is nowhere on the page. Sections save only by
Save (A-1), so it also stays unsaved.

## Before you start

- FIX-HIDE's loss lens found it; the orchestrator asked for its own card. Cut from
  track-FIX-HIDE (fa874ebd).

## Funnel card

- **Invariants**:
  - INV-NEVER-HIDE-UNSAVED-TEXT (FIX-HIDE), for section fields: a section field whose save has
    not landed and is not on its way opens its card when Translate shows its chapter and no card
    is open.
- **Wrong states**:
  - A restored section field with its card closed; the grid draws the stored text.
- **Code**:
  - `SaveMachine::unsaved_keys` and `Saves::unsaved_keys`; `section_keys_of` (a section place's
    range back to the chapter's verse keys); `Actions::reopen_unsaved_section`; an effect per
    chapter in Translate. A save on its way does not open the card: it lands or fails.
- **Checks**:
  - Journey `a_section_draft_restored_after_a_reload_is_on_the_page` (section.test.mjs); unit
    `a_section_place_names_the_verses_its_card_covered`.
- **Human**: none.

## Steps

### Step 1: Journey red, then the fix

- Done when: the journey passes; the section, translate and shell journeys pass.

## Log

- 2026-10-24 | C7a session (Claude Code 3058c665) | created | none | from the orchestrator's message: FIX-HIDE's loss lens finding (a restored section draft stays hidden) is a never-hide break, not a minor | none
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | red on FIX-HIDE's build (fa874ebd): `hung: no the draft is on the page after 20000 ms`; green with the fix. A first version also opened the card for a section save on its way, which broke `a_section_card_opened_after_a_reload_during_its_save_saves_each_verse`; a save in flight is skipped now. section, translate and shell journeys `# tests 45 # pass 45 # fail 0 # skipped 0`; tc-ui passes with `--features tc-api/test-hooks`; wasm clippy clean | .enjoy-logs/hide2-journeys.log
