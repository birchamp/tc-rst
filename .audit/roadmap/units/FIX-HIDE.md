---
id: FIX-HIDE
title: A held leave never hides a verse whose save failed
track: A
lane: app
milestone: M1
state: building
owner: C7a session (Claude Code 3058c665)
branch: track-FIX-HIDE
worktree: .worktrees/track-FIX-HIDE
depends: []
needs_owner: []
proves: [P1]
no_loss: true
size: 0.5 d (basis: FIX-R's save-gate fix, one view and one journey)
merged: none
updated: 2026-10-24
next: Fix, then the three review lenses, then the integration lane.
blocked_by: none
---

# FIX-HIDE: A held leave never hides a verse whose save failed

## Goal

In Translate by verse, a verse whose save stands failed keeps showing the translator's unsaved
text on the page, with the failure and Retry, after any held leave or anything else that closes
its card.

## Why it matters to a translator

Today the leave is held, but the verse card closes and the grid shows the old saved text: the
words just typed are nowhere on the page until Retry. A translator who sees the old text may
type the verse again or think it was lost.

## Before you start

- Found by the checks lane's UI-3a browser review; it predates UI-3a. Cut from main 15dc42e9.

## Funnel card

- **Invariants**:
  - INV-NEVER-HIDE-UNSAVED-TEXT: while a verse field's save has not landed, the page shows that
    field's text, never the stored text in its place.
- **Wrong states**:
  - The card closes on the blur that the "Switch project" click makes (`close_verse`, tC4's
    `blurVerse`), the save gate holds the leave, and the grid draws the server's text.
- **Code**:
  - The grid's verse (`target_verse`) shows the save machine's text for a verse whose field is
    not clean; one accessor, `Saves::unsaved_text`.
- **Checks**:
  - Journey `a_held_leave_keeps_a_failed_verse_text_on_the_page` (translate.test.mjs); kind:
    journey; runs in: app-e2e. Red first on 15dc42e9: `the typed text is on the page`.
- **Human**: none.

## Steps

### Step 1: Journey red, then the fix

- Done when: the journey passes, the translate and save journeys pass, and tc-ui's tests pass.

## Log

- 2026-10-24 | C7a session (Claude Code 3058c665) | created | none | from the orchestrator's message: the checks lane's UI-3a browser review found it | none
- 2026-10-24 | C7a session (Claude Code 3058c665) | claim | none | on track-FIX-HIDE from main 15dc42e9; the journey fails there: `AssertionError: 'the typed text is on the page'` | .enjoy-logs/hide-red.log
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | `Saves::unsaved_text` (the machine's text while a field is dirty, saving, failed or held) and `Actions::unsaved_verse`, tracking the machine's runs; the grid's verse shows that text in place of the stored one once its card closed. The journey passes; translate, section and shell journeys `# tests 44 # pass 44 # fail 0 # skipped 0`; tc-ui passes (run with `--features tc-api/test-hooks`, as the workspace unifies it: alone, `-p tc-ui` cannot write a `FieldKind::Test` buffer entry and three save tests fail, on main too); wasm clippy clean for app, test-hooks, gallery and none | .enjoy-logs/hide-journeys.log
