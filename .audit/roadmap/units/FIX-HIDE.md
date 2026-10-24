---
id: FIX-HIDE
title: A held leave never hides a verse whose save failed
track: A
lane: app
milestone: M1
state: gating
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
next: The integration lane: gate it on its own or in a batch and merge it.
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
  - INV-NEVER-HIDE-UNSAVED-TEXT: while the save machine holds a verse field whose save has not
    landed, Translate shows that field's text, never the stored text in its place. A server
    restart is outside it: the page goes Home at once and the restart offer brings the text
    back at the next open (AD4).
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
- 2026-10-24 | loss lens (Claude Code subagent of 3058c665) | verify | 23cd0c39 | no blocker. Unsaved verse text is on the page in every state (dirty, saving, failed, held), in the conflict branch, for an undrafted verse, after a reload and a chapter switch; no silent rewrite (Cancel is sections only; a refresh keeps the translator's text); clicking reopens with the field's text. Majors: (1) a server restart closes the card and forgets a kept field, so the grid would show the stored text; (2) a dirty section field restored after a reload stays hidden until the same section is drafted again. Minors: an emptied verse shows no pill; a teammate's newer text waits behind a failed save with no notice; a flash of the stored text after a save | cargo test -p tc-ui --features tc-api/test-hooks --test app save
- 2026-10-24 | races and protocol lens (Claude Code subagent of 3058c665) | verify | 23cd0c39 | no blocker; no RefCell panic (`machine` uses try_borrow_mut; `changes` bumps after the borrow); every machine path bumps `changes`. Major: every keystroke redrew every verse of the chapter (each verse tracked `changes` directly). Minors: a flash of the stored text between the Ack and the chapter's reread; the book read untracked inside the tracked closure | code reading
- 2026-10-24 | browser and parity lens (Claude Code subagent of 3058c665) | verify | 23cd0c39 | no blocker, no major. The journey would fail if the fix broke (the indicator is outside `main`; innerText does not include a textarea's value; the phrase is unique). tC4 also draws unsaved text in the grid after `blurVerse` (bookRaw), so this matches tC4: no declared difference. No capture affected. Minors: an emptied verse shows no pill; the journey's 1 s sleep backs a negative check | code reading, ~/unfoldingword/tc4-rc2 src/state.jsx:7612-7631
- 2026-10-24 | C7a session (Claude Code 3058c665) | fix | none | races major: one `Memo` per verse over `unsaved_verse`, so a keystroke redraws only the verse whose unsaved text changed; `book` is passed in, not read untracked; an emptied unsaved verse shows its Draft pill. Loss major 1 is outside the invariant, now worded to say so (the page goes Home on a restart and the AD4 offer restores the text). Loss major 2 (a section restored after a reload) is not a verse and predates this card: a minor naming the section card's restore, raised with the orchestrator. translate, section and shell journeys `# tests 44 # pass 44 # fail 0 # skipped 0`; wasm clippy clean | .enjoy-logs/hide-journeys2.log
