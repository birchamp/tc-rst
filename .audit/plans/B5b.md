# Plan for design unit B5b: Check, item detail

Worktree `.claude/worktrees/lane-b5b`, branch `lane-b5b`, cut from `claude/focused-ride-5uzj7p` at
`f5efad1` (B5a is on it). Row (docs/APP-DESIGN.md §10, phase 1): `check/item`; compare card;
target words with drag-select (`model::drag_range`); marks; the check comment saved on Done and on
leaving the item, on scripted outcomes (A-10); Academy drawer; notices; M8 entry. Tests:
check-keyboard and item-navigation (navigation parts); `drag_range` differential test;
`check_comment_saved_on_leaving_the_item`. Done when: the 12 session screens are in the report.
No Q-n or R-n waits (Q-25 is answered in the design: accept).

tC4 items for B5b (TC4-ISSUES, DELTA §3.12.1): #762/C40 compare card (tC4 9ad4d5c
`CompareCard.jsx`: 80ch lines, source texts as cards in a wrapping row, "Your translation" its own
card), #737 drag across draft words, #726 ItemNavButton (`check-prev`, `check-next`), #532 the new
draft on return, #42 the detail's keyboard stops, #619 article blocks (reuse B3's renderer), #614
Drawer label from the catalog, #29 the Arabic cases, and the follow-up that the selectable words
come from `tc_usfm::target_words` (tc-check `selection::target_words`).

## Steps (each one commit, built and tested)

1. **tc-api and `model::drag_range`.** `check/item { at, list, index }` with `CheckItemView` (title,
   note blocks, article link, original and gateway panes, `Shown<TargetWords>`,
   `Shown<DecisionView>`, notices) and `check/decide` (`Decide` with the item's check id, the two
   tokens and a `DecideEdit`), goldens; `model::drag_range` with its differential test against
   `tc_check::selection::from_indices`.
2. **tc-app.** `check/item` from the session's list entry and the fold (decision, target text,
   panes cut from the loaded book); `check/decide` through the storage seam's `SaveDecision`
   (scripted outcomes on the memory backend, the real store otherwise), rows in the `Ack` from the
   list entry. Tests through `App::request` on the parity scenarios.
3. **tc-ui pure.** `app::check` gets the selection and marking rules, item navigation and the
   comment's lifetime (open editor, Done, leaving the item); `FieldWriter` for check comments.
4. **tc-ui views.** `views/check/detail.rs` (header, quote, note, Learn more, notices, compare card,
   target words, marking row, bookmark and comment), the Academy drawer, `ItemNavButton`; the
   rail's `<main>` draws the detail.
5. **Parity and journeys.** `units.json` gains B5b; capture the 12 session screens; the
   check-keyboard and item-navigation journeys' detail parts; residuals; report.
6. **Docs.** B5b's notes in the row (D-n), TC4-ISSUES rows, declared differences.

## Rules kept

- Opening an item writes nothing. A mark is one `check/decide` per act; the comment is one more,
  sent on Done and on leaving the item while its editor is open, with its buffer entry and draft
  (A-10, declared difference Q-25: tC4 drops a comment being typed when the checker moves on).
- A decision shown is never rewritten: the request names the tokens of what was displayed.
