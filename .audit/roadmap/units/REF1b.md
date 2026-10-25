---
id: REF1b
title: Poem lines as tabs in the verse editor (tC4 #810)
track: A
lane: app
milestone: M3
state: building
owner: REF1 session (Claude Code 56b3f56d)
branch: track-REF1b
worktree: /home/bot/tc-rst/.worktrees/track-REF1
depends: [REF1]
needs_owner: []
proves: [P1, P7]
no_loss: true
size: 2-3 sessions (basis: tC4's change is about 1,300 lines with tests, PR #810 and its pick #826)
merged: none
updated: 2026-10-24
next: Merge main in, run the pre-hand-off checks (board.py --check, claims and coverage node tests, cargo check, wasm clippy), push once, send the tip to the integration lane.
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
- 2026-10-24 19:06 EDT | REF1 session (Claude Code 56b3f56d) | claim | none | Claimed on track-REF1b, cut from track-REF1 1ad7e1f8, on the orchestrator's word (REF1 is queued for batch 3; SWEEP is on main 15dc42e9). Note: REF1 is not done, so the merge waits for it. The branch lives in REF1's worktree (.worktrees/track-REF1) to spare the disk, as the orchestrator asked; REF1's branch there is unchanged | python3 -I -B .audit/tools/board.py
- 2026-10-24 19:16 EDT | REF1 session (Claude Code 56b3f56d) | note | none | Design (a read-only planning agent, checked here): one pure pair in tc-usfm poem.rs (editor_text, words_from_editor_text, shows_tabs), called only by tc-app: the Translate row's value, the verse save (opened = section::displayed, the version the card showed), the section save, the durable write text and the draft field text. The journal keeps the markers byte for byte, so the fold, rules and export do not change; the API's verse text becomes the editor text. tC4 #829 (2-character indent) is open in tC4 and not in rc.2: out of scope. Overlap with FIX-R step 4: crates/tc-app/src/translate.rs (near its words_drafted call) and docs/PLAN.md only | crates/tc-usfm/src/poem.rs
- 2026-10-24 19:16 EDT | REF1 session (Claude Code 56b3f56d) | step 1 | none | tc-usfm poem.rs with crates/tc-usfm/tests/poem.rs (9 tests: tC4 poemLines cases, the fixtures' round trip, a proptest that editing one line changes only that line) and resources.rs every_bundled_verse_saves_unchanged_through_the_editor_text (47,723 ULT/UST v91 verses round-trip; 0 keep markers). tC4's five #812 fixtures copied into fixtures/tc4/test-fixtures. Declared difference from tC4: a bare \q or \Q2 line shows as written (tC4 shows a tab and writes \q1/\q2 back), and a verse whose bytes tabs could not give back (a line led by a tab, mixed line ends) keeps its markers. Proof: shows_tabs forced true fails a_line_tabs_would_rewrite_stays_as_written; breaks not taken back fails four tests | cargo test -p tc-usfm --test poem
- 2026-10-24 19:41 EDT | REF1 session (Claude Code 56b3f56d) | note | none | Step 2 code: tc-app's Translate row sends the editor text (poem lines as tabs), SaveVerse turns it back by the version the token names (section::displayed), the Durable ack and the draft field text use the editor text, and section_values compares and converts by each verse's shown words. tc-api model/section.rs follows tC4 rc.2's sectionDraft.js: an inner line keeps its tabs (from blocks, three for \q3), escape_body puts its space after them, indent_line takes the pins (three tabs inside a verse), and only a span joined now loses its blank lines. fixtures/usfm/section-draft-vectors.json regenerated from tC4 d25b7871 with the generator calling sectionVerses with the blocks and indentLine with the pins, as SectionEditor.jsx does (729 texts, 594 indents). Declared difference: tC4's ownText keeps a verse's blank lines after its last words in its text; the rebuild keeps them in the verse's layout, which the section save carries (the test compares those texts without trailing line ends) | cargo test -p tc-api --test section
- 2026-10-24 19:41 EDT | REF1 session (Claude Code 56b3f56d) | note | none | Tests: tc-app tabs_saved_from_a_card_are_written_as_q_markers (Genesis 3:15-16: a verse card edit with a new third-level line and a section card level change reach TIT.usfm as \q2, \q1, \q3 lines, no tab); fails with words_from_editor_text forced to pass text through. verse_editor_shows_every_poem_line (tc-app, tc-ui) now expect tabs; the section property test reopens through the editor text as the app does. tc-app section:: and translate:: 28 pass, the wider touched set (drafts, check_item, align_store, conflicts) 78 pass before the reopen fix; tc-api all pass; tc-ui translate 14 pass; clippy wasm app and tc-api clean | cargo test -p tc-app --test app -- section:: translate::
- 2026-10-24 20:14 EDT | REF1 session (Claude Code 56b3f56d) | verify | b86cb60c | Lens loss, by an independent read-only reviewer (subagent of this session, did not write the code). Blocker: Keep on a restored verse draft (tc-app drafts.rs App::keep, then tc-store keep_draft) wrote the card's editor text (tabs, empty lines) into the book, losing every \q marker and break of the kept verse, unseen on reopen; proven with a failing scratch test (review_keep_of_a_poem_draft_writes_markers). Also found: tc-store drafts.rs field_text compared raw words with editor-text drafts, so a saved poem verse's draft would not clear. Minors: breaks go back by order, not position (as tC4's editorTextToVerse: deleting the first of two blank lines gives the \ts\* slot a \p); a pasted line with four or more tabs stays raw where tC4 writes \q3; a blank first line is kept where tC4 strips it (no text lost in either). Clean: unchanged saves byte-identical (CRLF, \q4, \qs, \qa, headings, ___, a trailing \b, odd spacing, a verse opening on its \q1); an untouched section card; SaveVerse converts by the version the card showed, a teammate's change included; the ack round trip; old marker-form drafts | this log
- 2026-10-24 20:14 EDT | REF1 session (Claude Code 56b3f56d) | fix | none | Blocker fixed: App::keep turns a Words draft back into words by the version its token names (its head, or the projection for several), as SaveVerse does; test keep_of_a_poem_draft_writes_its_markers (the reviewer's, fails before the fix). tc-store drafts.rs field_text compares a verse draft in editor form. Minors kept as tC4 has them (breaks by order) or logged: four-tab lines and a blank first line stay as typed. drafts:: 21 pass | cargo test -p tc-app --test app -- drafts::
- 2026-10-24 20:14 EDT | REF1 session (Claude Code 56b3f56d) | note | none | Journey poem_lines_show_as_tabs_and_save_as_their_markers in parity/app/journeys/translate.test.mjs (j02's poetry cases): a poem verse with a \ts\* break shows tabs and an empty line, a third-level line typed saves, and the card opens again as typed after Home and back. translate.test.mjs 13 pass, 0 skipped | TC4_BIN=… node --test parity/app/journeys/translate.test.mjs
- 2026-10-24 20:17 EDT | REF1 session (Claude Code 56b3f56d) | step 3 | none | The baseline's Translate screens show Titus, which has no poetry: the four Translate rows captured again (target/ref1b-app) have pixels identical to the committed ones; their DOM differs only in the helps panel's new div dir="auto", a change already on main and not REF1b's. So REF1b recaptures nothing, and leaves those captures to the unit that changed the helps panel. tc-store store.rs draft tests: 24 pass | cmp target/ref1b-app/screens/translate-tit1.png parity/app/screens/translate-tit1.png
- 2026-10-25 08:15 EDT | REF1 session (Claude Code 56b3f56d) | verify | afe48c3b | Lens races and protocol, by an independent read-only reviewer (subagent of this session, did not write the code). Major: a card over a verse with several heads (agreeing, merged, or settled by the stub rule) converted its text by the projection, a teammate's newer version after a sync, not by the version it showed (the store reads merged_at at the token's heads): a typed empty line took the teammate's \p instead of the card's \ts\*, and where that version could not show tabs, tabs reached the book; SaveVerse, the section save and Keep shared the rule. Minors: a typed text whose words show back otherwise (a literal \q1 x, tab then space, a tab before the first line, a trailing empty line with no break left) takes one extra save and shows as typed until reopened; such a draft can be offered again after a restart (Keep writes the same words); four tabs stay raw (logged). Clean: a resent save after a lost reply (the reply cache replays the first reply); one save in flight per field; a stale-token save stands beside the newer text with a clean field; two tabs; section and verse cards over one verse; drafts cleared by time and by editor-form text; CRLF and NFC | this log
- 2026-10-25 08:15 EDT | REF1 session (Claude Code 56b3f56d) | fix | none | Major fixed: section::shown_record reads the value a screen showed at its token's heads as the store does (one head's body, else Fold::merged_at; the projection stands in only for heads in conflict, where the store refuses the save), through CountingSnapshot::merged_at; section::displayed and Keep use it. Test a_card_over_two_heads_saves_by_the_version_it_showed (two agreeing heads, a teammate's \p meanwhile, the card's \ts\* kept in TIT.usfm); fails with the projection rule. section::, translate::, drafts:: 49 pass; tc-app clippy clean | cargo test -p tc-app --test app -- section::a_card_over_two_heads
- 2026-10-25 08:48 EDT | REF1 session (Claude Code 56b3f56d) | verify | afe48c3b | Lens the browser and parity, by an independent reviewer (subagent of this session, did not write the code), driving the app in Chromium and tC4 d25b7871's sectionDraft.js, verseEdit.ts and splice.ts under node. Major: a section save that clears the words after a verse's \ts\* lost the \ts\* (the last verse of a section) or turned it into \p (before the next verse), where tC4 rc.2 keeps it once (j02's #812 case); the earlier note that such blank lines live in the verse's layout was wrong: they are the verse's own words' breaks. Minors: the journey's grid comment had no check; tabs are eight wide in the verse card (tC4 sets no tab-size either; #829 is not merged). Matched tC4: the verse card's text, Tab moving focus and blur saving, a typed first-line tab stripped, a new empty line as \p, new trailing empty lines dropped, \q3 from three tabs, markers in TIT.usfm; the section card's text, Tab and Shift+Tab to three tabs inside a verse and two on a verse line, Place and back unchanged, its save writing \q3, \ts\* and \q1; the grid's text; right to left | this log
- 2026-10-25 08:48 EDT | REF1 session (Claude Code 56b3f56d) | fix | none | Major fixed: SectionBuffer::own_breaks (tC4 ownText's count: a verse's blank lines after its last words, less the line end before the next verse's line and the blank line that gives it its \p; none for a span joined now) and section_values gives them back to the verse before converting, so each takes back its break. own_breaks equals tC4's trailing line ends on all 729 vector texts (the_port_reads_every_text_as_tc4_does); test clearing_the_words_after_a_break_keeps_the_break (Titus 1:4 and 1:5, \ts\* kept once each), fails without the fix. The journey asserts the grid's text. section::, translate::, drafts:: 51 pass; tc-api section 47 pass; translate.test.mjs 13 pass | cargo test -p tc-app --test app -- section::clearing_the_words
