# Plan for design unit B4a: Translate, the grid and the verse editor

Planned 2026-10-19 on branch `lane-b4a`, cut from `claude/focused-ride-5uzj7p` at `d250a89`.

The row (docs/APP-DESIGN.md §10, phase 1): grid, sections (display), source and original chips,
Section/Verse switch, conflict markers (read); the verse editor on scripted saves with the
refresh rule of AD37; "changed while typing"; the `Editing` lock with the rail number button.
Tests: save states for every scripted outcome; `chapter_switch_sends_no_request`; the §6.3
refresh tests, with the Arabic IME case in Playwright. Done when: `translate-tit1`,
`en-translate-tit1`, `en-translate-verse-editor`. Issues named B4a in TC4-ISSUES: #786 (the box
holds all of `words`), #784 (a `\v` after a marker is listed), #131 and #118 (another
numbering), #728 ("Draft section" pill), #602 (reuse the divider), #29 and #657 (right to
left, IME), C39 titles (residuals).

What exists and is reused: B3's book read, source panes, helps panel, book rail (its number
button already turns off while `Editing` is open), sections and paragraphs; A4b's save machine,
buffer and AD37 refresh tests; A5's mode switch and project chip, already off while `Editing`
is open; C0's `test/saveVerse`, a real verse save through the store.

## Steps (each one commit, built and tested)

1. **tc-api, tc-app: the chapters' target text.** `editor/chapters { at, book }`: every chapter
   of the book with its verses (key, drafted, display text, paragraph marks as the panes carry
   them, the verse's `words` as a `Shown` value with its token and conflict marker), the
   chapter's own `\ts\*` sections, and the project's open verse conflicts. One read per book
   (AD16): a chapter switch needs no target read. `SourcePane` gains its pinned version (the
   caption "· v91 · pinned") and `SourcesView` the original-language pane (the Greek or Hebrew
   chip). Goldens; tc-app tests on `sample` and `en-pins`, #786 and #784 cases;
   `open_book_touches_one_book` covers the new read.
2. **tc-ui: Translate.** Pure model `app::translate` (sections by the source's `\ts\*`, else the
   book's, else the whole chapter; one per verse in another numbering; span labels; target
   paragraphs; the verse key of a field), host tests. Views: `translate` (Draft.jsx),
   `draft_chrome` (cell, chip, pill, editing card), `conflict_markers`; the helps panel's
   Comments tab and its editing focus. The verse editor opens on the row's `Shown` pair through
   `Saves::open`, saves on blur or 2 s, Cancel puts back the text it opened with; the chapter
   read's refresh goes to `Saves::refresh` with the author from the `Changed` frame, and the
   card shows "changed while typing". The verse writer is `test/saveVerse` in test builds;
   without it a save is `Unavailable(Writes)`, "Not saved" with Retry, until C2.
3. **Parity and journeys.** The three screens captured; journeys: a verse typed and saved, a
   held save, Cancel, the lock, `chapter_switch_sends_no_request`, a teammate's change under
   the open editor (clean: swapped; typed: kept with the notice), the Arabic IME case.
4. **Docs.** APP-DESIGN B4a notes, TC4-ISSUES rows.

## Rules kept

- The verse box holds the verse's whole `words` (#786); nothing is taken from tC4's `drafted`
  flag. A save replaces `words` only.
- An editor's value and token stay one pair (AD37); a refresh never writes into a focused or
  typed field.
- Nothing a translator cannot reach is tested.
