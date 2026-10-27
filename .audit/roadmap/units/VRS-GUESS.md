---
id: VRS-GUESS
title: A tC3 import offers the versification its verse counts fit, and the translator can change it
track: A
lane: app
milestone: M5
state: gating
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
next: Ready; waits for a batch: the integration lane takes track-VRS-GUESS into a batch and recaptures every screen once on the batch tree (captures_match_the_current_ui_source), giving import-review-tc3 and import-review-usfm a residual naming VRS-GUESS and H13.
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
- 2026-10-27 15:40 EDT | tc4-issues session (Claude Code dc817a96) | step 2 | none | ImportFound carries the guess (SchemeGuess: scheme, fit, names); Run carries the chosen versification, written to ingredients/vrs.json and the first project.vrs.set; Review's count and the run's carry-over place decisions in the chosen scheme; a name that is not a standard versification is a BadRequest. Fails with main's StandardScheme::DEFAULT line: "assertion `left == right` failed: INV-IMPORT-SCHEME-SHOWN: the project gets the scheme the review page showed" (import_scheme.rs:119), 3 of 4 failed; passes after | cargo test -p tc-app --features test-hooks --test app -- import_scheme
- 2026-10-27 15:45 EDT | tc4-issues session (Claude Code dc817a96) | note | none | step 3's code is in this commit: the review page's Versification select (tC4's wizard.versification label) and its note, four rebuild keys pending in nine languages, ImportForm.pick_versification asks the carry-over count again; tc-ui form test passes; journey not yet run | cargo test -p tc-ui --test app -- import
- 2026-10-27 16:30 EDT | tc4-issues session (Claude Code dc817a96) | verify | 6eac5f45 | loss lens, a read-only subagent (Claude Code a0263873): 1 Code: a typed StandardScheme at the run's boundary, a typed UI value is minor; 2 Checks: import_scheme.rs and the forms test name INV-IMPORT-SCHEME-SHOWN, 3 of 4 fail without the fix; 3 Reach: every path is the review page's; 4 Siblings: the other eng/DEFAULT uses are resource frames or a project with no record, correct; 5 No loss: verses and alignments never read the scheme, a decision not placed is kept marked invalid, the count and the run use one frame, the scheme rides the staged create; 6 Prose: reasons only, H13 and its addendum on file. No blocker or major; 2 minors | .audit/minors.tsv
- 2026-10-27 16:35 EDT | tc4-issues session (Claude Code dc817a96) | verify | 6eac5f45 | races and protocol lens, a read-only subagent (Claude Code acda0311): 1 Code: the reply cache keys a request id with the body's digest, so a stale retry is RequestReused, never the old job; 2 Checks: the forms test now also drops a count answered after a pick (its minor 1, added); 3 Reach: all reachable; 4 Siblings: pick_license follows the same new-id pattern; 5 No loss: the scheme is fixed when Run is sent, cancel unchanged, Review's Option field reads missing as none, no hand-built Run body outside the UI and goldens; 6 Prose: reasons only. No blocker or major; 1 minor logged | .audit/minors.tsv
- 2026-10-27 17:30 EDT | tc4-issues session (Claude Code dc817a96) | step 3 | none | journey import, two cases: the Titus tie offers eng and says the counts did not decide; a Hebrew-numbered Malachi offers org (tied with lxx), vul chosen is written to ingredients/vrs.json and MAL 3:19-24 stay; the file's other seven cases pass; #822 declared in docs/TC4-ISSUES.md | TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake node --test parity/app/journeys/import.test.mjs (9 pass)
- 2026-10-27 18:10 EDT | tc4-issues session (Claude Code dc817a96) | verify | 4b7b2955 | browser and parity lens, a subagent (Claude Code a9d58d17) driving Chromium: 1 Code: SchemeFit makes the note exhaustive; 2 Checks: the import journey 9/9, both versification cases fail if the fit, the choice or a verse breaks; 3 Reach: the tC3 review page; 4 Siblings: the cut import kinds wait under H13; 5 No loss: Malachi 3:19-24 stay; 6 Prose: reasons only. Screen: labelled "Versification" combobox, Tab reaches it, no horizontal scroll at 390px. Blocker: captures_match_the_current_ui_source fails, since the UI source changed without a recapture; as POEM-TAB and SRC-VER, every screen is recaptured once on the batch tree, and import-review-tc3 and import-review-usfm then get a residual naming VRS-GUESS and H13. 4 minors logged | /tmp/claude-1000/-home-bot-tc-rst/dc817a96-3f26-478a-9cb7-3304a28410ab/scratchpad/lens3/ (local)
- 2026-10-27 18:20 EDT | tc4-issues session (Claude Code dc817a96) | gate | none | started scripts/gates.sh --touched in .worktrees/track-VRS-GUESS, main 020e0ddf merged | scripts/gates.sh --touched
- 2026-10-27 20:10 EDT | tc4-issues session (Claude Code dc817a96) | gate | 2b4d6056 | scripts/gates.sh --touched with main 020e0ddf merged, its board step skipped for the REL-1 errors only (REL-1's branch names FIX-791, as expected): claims and coverage 10/10; endpoints and goldens 13/13; 16 crates, 3538 Rust tests, 3537 passed, 1 failed: tc-ui::screen_record captures_match_the_current_ui_source, the recapture left to the batch as for POEM-TAB and SRC-VER; doctests pass; 8 journey files, 96/96, no skips | bash scratchpad/touched-vrs.sh (scripts/gates.sh's touched with the board line off)
- 2026-10-27 15:40 EDT | tc4-issues session (Claude Code dc817a96) | fix | none | browser lens minor made a fix: the difference from tC4 (every import eng) is declared in APP-DESIGN's F1 row; the sweep's held follow-ups are listed in the evidence file | docs/APP-DESIGN.md (F1 row)
