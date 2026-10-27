---
id: VRS-GUESS
title: A tC3 import offers the versification its verse counts fit, and the translator can change it
track: A
lane: app
milestone: M5
state: building
owner: tc4-issues session (Claude Code dc817a96)
branch: track-VRS-GUESS
worktree: .worktrees/track-VRS-GUESS
depends: []
needs_owner: []
proves: []
no_loss: true
size: 1-2 d (basis: F1's tC3 review page; a pure fit function plus one review-page field)
merged: none
updated: 2026-10-27
next: Step 2: carry the guess and the chosen scheme through tc-api and tc-app (crates/tc-app/src/import.rs found, run, carry_over).
blocked_by: none
---

# VRS-GUESS: A tC3 import offers the versification its verse counts fit

## Goal

Owner ruling H13 (`rulings/2026-10-27-H13.md`): the tC3 import guesses the scheme from the verse
counts of the books it holds across the six schemes, shows the guess as the default on the review
page, and the translator can change it. A tie gives `eng` with a note that the content did not
decide. Update a book keeps the project's scheme with no choice.

## Why it matters to a translator

Today every tC3 import gets `eng` (tC4 #822, reproduced on main 58e8e10b:
`docs/evidence/tc4-issues-2026-10-27/repro/822-tc3-org-versification.rs` on branch
`tc4-issues-2026-10-27`). A Malachi with 3 chapters gets the 4-chapter `eng` frame, so the wrong
source verse sits beside the draft, for the life of the project.

## Before you start

- `crates/tc-app/src/import.rs` uses `StandardScheme::DEFAULT` (about lines 685 and 900).
- Schemes and their `maxVerses`: `crates/tc-versification` (`StandardScheme`, `standard.rs`).
- The tC3 review page: `graft ask "tC3 import review page" --source`; its journey is
  `parity/app/journeys/import.test.mjs`.

## Funnel card

- **Invariants**:
  - INV-IMPORT-SCHEME-SHOWN: the scheme a tC3 import writes is the one its review page showed
    when Import was pressed.
- **Wrong states**:
  - An import that writes `eng` for content that fits only another scheme, with nothing shown.
  - A tie that picks a scheme other than `eng`, or says nothing.
- **Code**:
  - A pure fit function in tc-versification: books with their chapter and verse counts in, the
    best-fitting schemes out (a tie is a set of more than one).
  - tc-api/tc-app: the review page's details carry the guess and the tie note; the import request
    carries the chosen scheme; the import writes it.
  - tc-ui: a scheme field on the tC3 review page, defaulted to the guess, with the tie note.
  - Update a book: unchanged; a test proves it keeps the project's scheme.
- **Checks**:
  - Fit function unit tests: Psalm titles (fits `org`/`lxx`), Joel, Malachi, one tie (a short New
    Testament book), one book that fits no scheme. Property test: a book built from scheme S's
    counts always has S among the best fits.
  - tc-app test: the tC3 Malachi shaped like `org` imports with `org` by default; a chosen scheme
    is written; a tie imports `eng` and the details carry the note. Fails on main.
  - Journey: the review page shows the guess, a change is written, the tie note shows.
  - Proof it can fail: the tc-app test fails on main 003aa087.
- **Prose**: two new strings (the field's label, the tie note), added to every catalog as the
  repository does for rebuild keys.
- **Human**: none (H13 answered).

## Steps

### Step 1: The fit function and its tests
### Step 2: Carry the guess and the choice through tc-api and tc-app; the tc-app test
### Step 3: The review page field and the journey
### Step 4: Three lenses, loss first; `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; three `verify` lines.

## Stop and ask when

- A fit needs a rule the ruling does not give (for example a book that fits no scheme at all).

## References

- tC4 #822; ruling H13; `units/I.md` (H13 notes).

## Log

- 2026-10-27 10:10 EDT | tc4-issues session (Claude Code dc817a96) | created | none | from the orchestrator: the owner wants #822 in the preview, rule as the issue proposes (H13) | rulings/2026-10-27-H13.md
- 2026-10-27 10:10 EDT | tc4-issues session (Claude Code dc817a96) | ruling | none | H13 answered for the tC3 import | rulings/2026-10-27-H13.md
- 2026-10-27 11:00 EDT | tc4-issues session (Claude Code dc817a96) | claim | none | claimed in .worktrees/track-VRS-GUESS on track-VRS-GUESS | python3 -I .audit/tools/board.py
- 2026-10-27 12:10 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | best_fits and distance in tc-versification: a scheme's distance is the verses between each held chapter's last verse and the scheme's, the nearest win, more than one is a tie | cargo test -p tc-versification --lib fit
- 2026-10-27 12:15 EDT | tc4-issues session (Claude Code dc817a96) | block | none | H13 says a tie gives eng, but an org-shaped Malachi or Joel ties org with lxx (same counts in 22 books), so the literal rule keeps #822's eng for exactly the books it is meant to fix; the ruling gives no rule for a tie that leaves eng out | fit::tests::malachi_in_three_chapters_fits_the_hebrew_numbering
- 2026-10-27 13:00 EDT | tc4-issues session (Claude Code dc817a96) | unblock | none | the H13 addendum settles a tie without eng (org, lxx, vul, rso, rsc) and content no scheme holds (the nearest, said so) | rulings/2026-10-27-H13.md
- 2026-10-27 13:20 EDT | tc4-issues session (Claude Code dc817a96) | step 1 | none | guess and SchemeGuess (Fits, Undecided, Tied, Nearest) settle the default as the addendum says | cargo test -p tc-versification --lib fit
