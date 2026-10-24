---
id: REF1b
title: Poem lines as tabs in the verse editor (tC4 #810)
track: A
lane: app
milestone: M3
state: todo
owner: none
branch: none
worktree: none
depends: [REF1]
needs_owner: []
proves: [P1, P7]
no_loss: true
size: 2-3 sessions (basis: tC4's change is about 1,300 lines with tests, PR #810 and its pick #826)
merged: none
updated: 2026-10-24
next: Claim this card after REF1 merges, then do step 1.
blocked_by: none
---

# REF1b: Poem lines as tabs in the verse editor (tC4 #810)

## Goal

The verse editor and the section editor show a verse as one body over all its lines, as tC4
rc.2 does: a poem line with leading tabs (one for `\q1` or `\q`, two for `\q2`, three for
`\q3`), a break (`\p`, `\m`, `\b`, `\pi1`, `\ts\*`) as a blank line, and a heading
with its marker. A mid-verse `\p`, `\b`, `\ts\*`, `\q3` or heading no longer cuts the verse.
Saving turns the editor text back into the same markers, so nothing a translator wrote is
lost or moved.

## Why it matters to a translator

Psalms, prophets and songs are poetry. Today the editor shows `\q` markers, or cuts a verse at a
mid-verse break, so part of the verse is hard to see or edit. With tabs, the verse reads as
poetry and every line stays with its verse. The owner chose to match rc.2 for the first release
(H7, `rulings/2026-10-24-H3-H4-H7-fast-release.md`).

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| REF1 '` shows REF1 `done`.
- `git -C ~/unfoldingword/tc4-sweep show --stat f7e726b3` lists tC4's change: `src/data/usfm/poemLines.ts`,
  `indexer.ts`, `splice.ts`, `verseEdit.ts`, `src/views/sectionDraft.js`, and the tests
  `test/poemLines.test.ts`, `test/indexer.test.ts`, `test/sectionDraft.test.ts`,
  `e2e/j02-draft-verse.spec.ts` with fixtures `test/fixtures/en_ult/{1SA-2,MAT-27,PSA-9,REV-7}-812.usfm`
  and `en_ust/GEN-29-812.usfm`. #829 is the follow-up SWEEP names with it.
- `docs/TC4-DELTA-2026-10-23.md` C68 and `docs/TC4-ISSUES.md` rows #810 and #812 (on main once
  SWEEP lands) say what the rebuild does today.
- Read `docs/STORAGE.md`: the verse save is a translator's text (no-loss).

## Funnel card

- **Invariants**:
  - INV-EVERY-POEM-LINE-STAYS: opening a verse and saving it unchanged writes the same bytes;
    every line of the verse body, poem lines, breaks and headings, is in the editor text.
  - INV-NO-CUT-AT-A-MID-VERSE-BREAK: a `\p`, `\m`, `\b`, `\pi1`, `\ts\*`, `\q3` or heading
    inside a verse belongs to that verse's body, not to the next.
- **Wrong states**:
  - A verse's later poem lines are cut off into the next verse or lost on save.
  - A blank line typed by the translator vanishes or turns into another break than it was.
- **Code**:
  - Types: one module (tc-usfm) with `body_to_editor_text` and `editor_text_to_verse`, the only
    place that turns a body into editor text and back, by the indexer's line kinds, as tC4's
    `poemLines.ts`.
  - One entry point: the verse save and the section save both go through it.
  - Removed: the rebuild's own marker display for poem lines and the cut at a mid-verse break.
  - Left to checks: the round trip over real corpora (a property test).
- **Checks**:
  - `verse_editor_shows_every_poem_line` and `no_change_section_save_on_psalm_3_writes_nothing`
    (the TC4-ISSUES #810 row's tests); ports of `test/poemLines.test.ts`, `test/indexer.test.ts`
    (#812 cases) and `test/sectionDraft.test.ts` on tC4's five fixtures; a proptest that
    `editor_text_to_verse(body_to_editor_text(b)) == b` for bodies from the corpus.
  - Journey: the poetry cases of `e2e/j02-draft-verse.spec.ts` in `parity/app/journeys/translate.test.mjs`.
  - Proof it can fail: revert the module to the marker display; the tests fail.
- **Prose**: `docs/PLAN.md` "differs on purpose": remove the #810 bullet (markers) that REF1
  step 5 would have added.
- **Human**: none (H7 answered).

## Steps

### Step 1: Port the line model and its tests

- Do: port `poemLines.ts` and the indexer's body rules (#812) into tc-usfm with the tests above,
  red first.
- Done when: the ported tests pass and fail with the module reverted.
- Record: a log line `step 1`.

### Step 2: The verse editor and the section editor use it

- Do: the Translate verse card and section editor show and save through the module; port the
  j02 poetry journeys.
- Done when: the journeys pass and `scripts/gates.sh --step journeys` shows 0 skipped.
- Record: a log line `step 2`.

### Step 3: Recapture the Translate screens

- Do: recapture the baseline screens that show poetry (REF1 step 3's harness), and update PLAN.
- Record: a log line `step 3`; `state: verifying`.

## Gates before merge

`scripts/gates.sh` (journeys included), plus the three verification lenses of a no-loss item
(`GLOSSARY.md` "Verification lenses"), each a `verify` log line, and a `fix` line for every
blocker and every major that harms a translator's data or view.

## Stop and ask when

- A verse would save different bytes than it opened with, and tC4 does the same: ask before
  matching it.

## References

- tC4 PR #810 (f7e726b3) and its pick #826 (1cdf350c); issues #786, #812, #829
- `docs/TC4-DELTA-2026-10-23.md` C68

## Log

- 2026-10-24 15:15 EDT | REF1 session (Claude Code 56b3f56d) | created | none | split from REF1 on the orchestrator's word: the owner chose to match rc.2 for the first release (H7), so poem lines show as tabs; REF1 lands first | rulings/2026-10-24-H3-H4-H7-fast-release.md
