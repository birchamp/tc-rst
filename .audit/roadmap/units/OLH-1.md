---
id: OLH-1
title: Highlight a note's words in the original-language pane
track: A
lane: app
milestone: M3
state: done
owner: C7a session (Claude Code 3058c665)
branch: track-OLH-1
worktree: .worktrees/track-OLH-1
depends: [INT-4]
needs_owner: []
proves: [P1]
no_loss: false
size: 2-3 sessions (basis: tC4 #840's scope; the gateway highlighter exists in tc-helps)
merged: 14e0f12b
updated: 2026-10-24
next: none
blocked_by: none
---

# OLH-1: Highlight a note's words in the original-language pane

## Goal

With a translation note current, the Greek or Hebrew pane marks the note's quote words, in the
original tab of Draft and Understand and in the original row of the check card. The gateway
highlight is unchanged. The owner ruled to build this now, ahead of tC4 (#840).

## Why it matters to a translator

A note's quote is written in the original language, so a checker looks for it there first.
Today that pane stays plain, in tC4 4.0 and in the rebuild.

## Before you start

- `ls .audit/roadmap/rulings/ | grep Q39` lists `2026-10-23-Q39-original-language-highlight.md`.
- `gh issue view 840 -R unfoldingWord/translationcore4` (read only): the rule and the
  acceptance criteria.
- `grep -n 'pub fn' crates/tc-helps/src/highlight.rs`: today's highlighter reads the gateway's
  `\zaln` milestones (`AlignedVerse`, `SourceWord`).
- The original row of the check card: `crates/tc-ui/src/views/check/detail.rs`.

## Funnel card

- **Invariants**:
  - INV-ORIGINAL-HIGHLIGHT-MATCHES-THE-QUOTE: with a note current, the original-language pane
    marks exactly the words of the note's quote at the note's occurrence, matched against the
    verse's `\w` words; a quote that does not match in full marks nothing; with no current note
    nothing is marked.
- **Wrong states**:
  - The pane marks a different occurrence of the same word.
  - A partial match marks some of the quote's words.
  - The check card's original row stays plain while Draft marks the words.
- **Code**:
  - Types: reuse tc-helps' `SourceWord` (content plus occurrence) for the quote's words.
  - One entry point: one pure function in tc-helps from (verse words, quote, occurrence) to the
    marked word indexes, called by every original-language pane.
  - Removed: nothing.
  - Left to checks, and why: which words a quote names is a matching rule over text.
- **Checks**:
  - `the_original_pane_marks_the_quote_at_its_occurrence` in `crates/tc-helps/tests/` (to create),
    with a Greek (TIT, UGNT) and a Hebrew (UHB) verse from `fixtures/`; kind: unit; runs in: CI.
  - `a_quote_the_verse_lacks_marks_nothing_in_the_original_pane` (to create); kind: unit.
  - A journey step in the Check and Understand journeys that the original row marks the words
    (to create); kind: journey; runs in: G5's app-e2e job.
  - Proof it can fail: mark the first occurrence instead of the note's; the first test fails.
- **Prose**: a declared difference in `docs/PLAN.md` until tC4 ships #840.
- **Human**: none; ruled (`rulings/2026-10-23-Q39-original-language-highlight.md`). New pixels
  in the Check and Understand captures are reviewed with the P7 report (G2).

## Steps

### Step 1: The matcher

- Do: claim the card. Write the two unit tests first, then the pure function in tc-helps,
  following tC4's gateway rule (`matchQuote`: the quote's words in order, the note's
  occurrence, a partial match marks nothing), over `\w` words compared as tc-helps'
  `normalize_word` compares them.
- Files: `crates/tc-helps/src/highlight.rs`, `crates/tc-helps/tests/`.
- Done when: both tests pass, and fail with the occurrence ignored.
- Record: a log line `step 1`.

### Step 2: The panes

- Do: call the function from the original tab of Draft and Understand and from the check
  card's original row; mark the words as the gateway panes mark theirs. Add the journey steps;
  recapture the affected screens; add the declared difference to `docs/PLAN.md` and set the
  #840 row of `docs/TC4-ISSUES.md` to covered with the test names.
- Files: `crates/tc-ui/src/views/**`, `parity/app/journeys/*.test.mjs`, captures, `docs/PLAN.md`,
  `docs/TC4-ISSUES.md`.
- Done when: the journeys pass with zero skips; `screen_record` passes with the new captures.
- Record: a log line `step 2`.

### Step 3: Verify and merge

- Do: one verify pass (`FUNNEL.md` §8), then merge as `TRACKING.md` says, titled
  `Merge OLH-1: a note's words are marked in the Greek and Hebrew text`, trailer `Unit: OLH-1`.
- Record: log lines `verify` and `merge`.

## Gates before merge

`scripts/gates.sh --journeys` and `scripts/gates.sh --check-receipt` on the exact commit main
moves to.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The mark would change what a translator sees beyond the original-language panes.

## References

- tC4 #840; `rulings/2026-10-23-Q39-original-language-highlight.md`;
  `rulings/2026-10-23-replace-tc4.md`.
- `crates/tc-helps/src/highlight.rs`; tC4 `src/data/sourceHighlight.ts` (`matchQuote`).

## Log

- 2026-10-23 | integration lane (Claude Code 3d07d5f8) | created | none | from the owner's Q39 answer of 2026-10-23 | rulings/2026-10-23-Q39-original-language-highlight.md
- 2026-10-24 | C7a session (Claude Code 3058c665) | claimed | none | on a branch cut from track-SWEEP (a7199102), as the orchestrator offered, while SWEEP waits for its merge turn | none
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 1 | none | `highlight_original` in tc-helps: the verse's `\w` words, the quote's `&` spans matched as `highlight_plain` matches them (one shared `quote_matches`), then the note's occurrence; `-1` or unrecorded marks every match; not tied, a partial match or a missing occurrence marks nothing. Tests on UGNT TIT 1:1 and 1:9 and UHB JON 1:3 and 1:5 (a maqaf compound): `the_original_pane_marks_the_quote_at_its_occurrence`, `a_quote_the_verse_lacks_marks_nothing_in_the_original_pane`. Red first (no function); with the first match taken instead of the note's occurrence both fail (highlight.rs:813, :859). tc-helps passes, clippy clean | cargo test -p tc-helps --test highlight
- 2026-10-24 | C7a session (Claude Code 3058c665) | step 2 | none | Draft's original tab: `App::focus` adds a focus for pane `orig` (no title) from `ChapterModel::original_focus`, remapped like the others in a mapped frame. The check card: `OriginalView::Ready.verse` is now `OriginalVerse { tokens, lit }` (golden updated) and the original row draws `orig-hl` marks through the gateway row's helper; both use `VerseModel::original_lit` over the `\\w` tokens. Understand has no original-language text (no tab, in tC4 or the rebuild), so it has nothing to mark; PLAN says so. Tests: tc-app `a_note_shows_its_panes_its_article_and_the_draft_words` (the Greek row marks TIT 1:1's seven quote words) and `a_chapters_helps_are_tc4s_cards_with_gateway_titles` (the orig focus); journeys `a_note_marks_its_quote_in_the_greek_tab`, `the_greek_row_marks_the_quote_of_the_check`; translate, check-detail, understand, check-keyboard and check-picker journeys 52 of 52, no skips. Native and wasm clippy clean; tc-ui and tc-api pass; screen_record passes on the committed files. Captures not retaken: this machine's font fingerprint differs from the captures' (minors), so pixels from here fail the gate. PLAN declares the difference; TC4-ISSUES #840 is covered | .enjoy-logs/olh-journeys.log
- 2026-10-24 | C7a session (Claude Code 3058c665) | verify | f6317810 | browser-and-parity lens (subagent review of 03a460a1..f6317810): no blocker, no major. `highlight_plain` is unchanged by the shared matcher; real quotes all match at their occurrence (en_tn Jonah 191/191 on UHB, Titus 202/202 on UGNT, Psalms 7,921/7,921); the orig focus changes no title or other pane; the original row's text is the same drawn from tokens; mapped frames remap the orig marks. Two minors logged: overlapping matches of a repeated-word quote (no fixture reaches it), and unit-test quotes built from verse words | cargo test -p tc-helps --test highlight
- 2026-10-24 17:36 EDT | integration lane (Claude Code 3d07d5f8) | merge | 14e0f12b | merged into main in batch 2a (UI-1 4a86b252, UI-2 312cef3f, SWEEP 21fbbec0, OLH-1 14e0f12b, T e1ffc583), gated by scripts/gates.sh --wait-ci batch-2a on the batch's last commit. | scripts/gates.sh --wait-ci batch-2a
