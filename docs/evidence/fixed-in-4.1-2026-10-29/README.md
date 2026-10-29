# Issues labeled `fixed-in-4.1` checked against the rebuild, 2026-10-29

- Rebuild: main 597a234e, branch `evidence/fixed-in-4.1`. Report only: no product code changed.
- Issues: every open unfoldingWord/translationCore4 issue labeled `fixed-in-4.1` (44), read with
  `gh issue view` on 2026-10-29. The label came from the 2026-10-23 sweep (`docs/TC4-ISSUES.md`)
  "covered" verdicts, which read named tests and never drove the issue's steps. Six were already
  proven on the real path on 2026-10-27 (`docs/evidence/tc4-issues-2026-10-27/open.md`) and were
  not retested here.
- Scope: 4.1.0 as ruled in `docs/rulings/2026-10-24-H3-H4-H7-fast-release.md`.
- Method: as on 2026-10-27. Each reachable issue was driven in the real app (`tc4 serve` built
  with `test-hooks,fixture-backend`, the dev UI build, Chromium through
  `parity/app/journeys/kit.mjs`, the fake Door43 server `tc-door43-fake`), or through a Rust test
  on the public API with `fixtures/` data or the bundled resources. No repro mocks the server reply
  the issue is about. The tests the sweep names were run too, and read for whether they fail if the
  fix is reverted ("revert check"; decided by reading, no revert was run).
- Files: `repro/` (the tests; the run command is at the top of each), `out/` (their output),
  `shots/` (screenshots). Nothing here is in the gate: browser repros sit outside
  `parity/app/journeys/*.test.mjs`, and Rust repros were copied to `crates/<crate>/tests/repro_<n>.rs`
  for the run and removed again (their header names the crate). Shell repros ran under xvfb with
  Electronite 37.1.0.
- Commit: the main commit that made the behavior, where one could be found.

## Verdicts

| Issue | What | Verdict | Evidence | Commit |
|---|---|---|---|---|
| #840 | Mark the note's quote in the original-language pane | **Fixed** | 2026-10-27: `repro/840-orig-highlight.test.mjs` | — |
| #835 | Import accepts the x- codes the server accepts | **Fixed** | 2026-10-27: `repro/835-tc3-language-codes.test.mjs` | — |
| #831 | Resume card mixes verses or shows `___` | **Fixed** | 2026-10-27: `repro/831-resume-card.test.mjs` | — |
| #830 | OT verse: placed words read backwards | **Fixed** | 2026-10-27: `repro/830-ot-card-order.test.mjs` | — |
| #802 | A book save overwrites newer text | **Fixed** | 2026-10-27: `repro/802-two-tabs.test.mjs` | — |
| #764 | Progress bars look empty at low progress | **Fixed** | 2026-10-27: `repro/764-progress-fill.test.mjs` | — |
| #758 | Clear every alignment of a verse | **Same bug** (not built) | `repro/758-clear-verse.test.mjs`: Align has no Clear control (`a Clear control: []`), Ctrl+K opens no dialog. Only the library `Aligner::clear` (`crates/tc-align/src/aligner.rs:312`) exists, with no caller. `clear_discards_standing_suggestions` tests only that. | 580f301b (library) |
| #568 | Align with only a keyboard | **Partly fixed** | Placing works: `repro/568-keyboard-align.test.mjs` (33 card Tab stops while a word is chosen, 6 → 7 placed), journey `align_by_keyboard_only` passes. Not: merging two cards is drag-only (`crates/tc-ui/src/app/align.rs:210`); a suggestion chip cannot be confirmed by keyboard (12 chips, 0 Tab stops; `crates/tc-ui/src/views/align/session.rs:478-488`). | cea968e0 |
| #420 | An aligned word keeps its character style | **Fixed** | `repro/420-style-after-align.test.mjs`: `\bd siervo` and `\it Dios` kept around their `\zaln`/`\w` after aligning and exporting aligned USFM. `repro/420-weave-styles.rs` (3 pass: `\add \bd \it \nd`, an unaligned word in a span, nested `\add … \+nd`). J7 #420 journey passes. | e44f1060, e7faaeb4 |
| #409 | x-ref for spans, occurrences per verse | **Partly fixed** | Import keeps `x-ref` (`an_import_keeps_x_ref`, `an_imported_span_with_x_ref_opens_with_every_link`). Not: a span aligned in the app writes no `x-ref` and counts occurrences across the span (`repro/409-span-weave.rs`: `x-occurrence="3" x-occurrences="3"`); `crates/tc-export/src/usfm.rs:11`. | 3180d499 |
| #274 | Phrase suggestion over several cards | **Same bug** (not built) | `crates/tc-align/src/suggest/mod.rs:350-377` leaves out a prediction over two cards; `Proposal` has one card. `a_prediction_over_two_cards_is_left_out_never_cut_to_the_first` proves the omission (`out/274-prediction-over-two-cards.txt`). | — |
| #664 | Hebrew quote highlight with a maqaf | **Fixed** | `repro/664-ruth-maqaf.rs` on the bundled en_tn v91 and en_ult v91 Ruth through `tc_app::sources::pane_book`: 91 maqaf rows, 84 Exact, 7 Approximate (English word order), 0 unlit; all 197 others Exact. `hebrew_quotes_split_maqaf_compounds_into_the_aligned_words`, `every_jonah_note_quote_finds_its_words_in_the_uhb_verse` fail if the split is reverted. | 24fc871a |
| #628 | Full Markdown in helps | **Partly fixed** | `repro/628-article-marks.rs` on the bundled en_ta v91: tables, numbered and nested lists render. Not: HTML shows as text (`<br>` in 17 articles, `<sup>` 11, `<u>` 2), images show as "!alt" (`figs-exclusive`), links are not clickable (`crates/tc-helps/src/blocks.rs:41`, 124 tA and 944 tW articles). The journey the sweep names fakes the `helps/article` reply. | 4ba84e5a |
| #179 | An edit with an unknown base displaces the verse | **Fixed** | `repro/179-unknown-base-edit.rs`: the verse keeps both heads, a VerseText conflict lists both (incomplete history), the `___` stub never shows. If the orphan edit is newer it shows while the conflict is open (newest-wins display, H5); both versions stay. `loss_unknown_parent_verse_edit_keeps_standing_text` passes only on its clock order. (`out/` exit 101 is that repro's deliberate "orphan newer" assertion.) | 13843456 |
| #131 | Translate source pane through mapped rows | **Fixed** | Rust, real `rsc` scheme: `a_pane_in_another_numbering_shows_the_source_verse_under_the_projects_key` (Jonah 2:1 shows source 1:17), `a_project_verse_shows_the_source_verse_its_numbering_names` (`out/131-118-121-numbering-tests.txt`). The browser journeys fake `editor/sources`; the real-app run lacked Jonah ULT in its store. | e9f53526 |
| #121 | Source text for cross-book mappings | **Same bug** (not built) | lxx Ezra 11 in the real app only states the other book (`shots/121-lxx-ezra-11-understand.png`; `crates/tc-ui/src/views/understand.rs:606`). `RowSource::OtherBook` carries no text. | — |
| #118 | Section view for cross-frame projects | **Same bug** (not built) | rsc Jonah 2 and lxx Ezra 11: "sections are not available", no switch (`out/131-118-121-cross-frame.txt`, `shots/118-rsc-jonah-2-understand.png`). `mapped_chapter` returns no sections (`crates/tc-app/src/understand.rs:1330`). | — |
| #549 | An OBS project's own story images | **Not reachable** | No 4.1.0 path brings in an OBS project with its own pictures (Burrito import cut). The rule is in tc-obs (`crates/tc-obs/src/images.rs:175-210`, 3 tests pass, `out/549-project-pictures.txt`), but `crates/tc-app/src/story.rs:195` hands the screen no project pictures. | c6bdb9d1 |
| #661 | Carry-over on a tW-only pin change | **Fixed** | Real store, fake Door43, en_tw v1 → v2: `a_tw_only_change_that_drops_a_decided_article_is_counted` (tc-app and tc-check) pass; the decision computes invalidated and is kept (`out/661-*.txt`). | 384d8465, 0adc54c8, fb04efb1 |
| #663 | Helps releases missing books or the topic | **Question answered** | `repro/663-listed-books-without-files.rs`: es-419_tn lists 4 books, holds TIT only; installed coverage `Books([Titus])`, Jonah resolves to the English fallback, or `NotCovered` without it. Coverage comes from the files (`crates/tc-resources/src/layout.rs`). A package without `tc-ready` is not listed (`crates/tc-app/src/resources.rs:157`). | — |
| #595 | Bounded checking-language preview retries | **Fixed** | No retry loop; a stale confirm replans once. `gateway_preview_appears_while_the_open_verse_keeps_changing` (40 saves during the plan, `Applied`), `a_stale_confirm_reshows_the_dialog_with_new_counts` (`out/661-595-tc-app-upgrades.txt`). | 1102e2c2 |
| #594 | Mark a draft-invalidated check Valid again | **Fixed** | `repro/594-mark-valid-after-invalidation.test.mjs`, the issue's steps: after Mark valid `invalid 0`, "2 of 182 resolved", on disk `"invalidated":false`. To do keeps the flag, as tC4 (`an_invalidated_check_put_back_to_do_stays_to_do`); the issue left that to the owner. | — |
| #518 | Return to the last Check tool | **Fixed** | `repro/518-resume-check-tool.test.mjs`: the tile resumes Translation Notes with no picker drawn; Translate clicked during the resume stays; a quit 100 ms after a move keeps the place. Part 1 is a page reload, not a relaunch. | 8b2ff5f1, 510fd828 |
| #761 | Sync OBS projects with teammates | **Partly fixed** | Story projects sync: `repro/761-obs-team-sync.test.mjs` (B's frame reaches A, "1 change from teammates"), `repro/761-obs-team-sync.rs` (merge, third computer byte-identical). Not: a same-frame conflict is recorded but never shown: the toast says "0 verse conflicts to review" (`crates/tc-app/src/sync.rs:39-45`), no Home chip, no review button (`crates/tc-ui/src/views/story_draft.rs:231-236`). | cec46b8b |
| #747 | Backup files out of syncs | **Fixed** | `repro/747-backup-files-sync.rs`: a team .gitignore without the rule and four .bak files: Door43 main after sync has no .bak, .gitignore restored, the backups kept locally under `.tc/external/`. | — |
| #710 | Record the Door43 403 push reply | **Fixed** | The reply is from a real push (`docs/evidence/door43-live-2026-10-17.md` step 6). `repro/710-declined-push.rs`: the fake's real receive-pack with Gitea's hook lines ends `NoWriteAccess`, `sync.no-write-access`, Door43 unchanged, save kept. 4 named tests pass. | 02300d97 |
| #705 | How do audio files sync? | **Question answered** | Carried files in the text Burrito, byte for byte; a collision keeps both (STORAGE.md:1009, H5). `repro/705-audio-two-recordings.rs`: three computers hold both recordings. | cec46b8b, 2ff9b1fb |
| #582 | Create in a language the server refuses | **Fixed** | `repro/582-door43-language-create.test.mjs`, the issue's steps: ug, bal, kas, ajp kept; `acm-x-lebarabic` → `acm-x-lebarabi-c` with a sentence and the Door43 code kept; none `und`. `a_suggestion_offers_a_code_a_burrito_can_hold`: 4 listed codes cannot be offered. | fef659a4 |
| #746 | Test that startup recovery runs before opens | **Fixed** | `repro/707-746-start-cleanup-import.rs`: an import waits while the start cleanup is held, keeps its staging, then finishes. `an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes`, `a_scenario_written_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes`. | 45237d52 |
| #707 | Can an import during start recovery be rolled back? | **Question answered** (no) | Same repro: no ops log, no rollback; an import is a staging folder renamed into place. A power cut after the rename keeps the project; a kill before it leaves only staging, which the next start removes and reports. | 45237d52 |
| #161 | Two actor ids under one name | **Partly fixed** | Devices group under one Door43 username (`crates/tc-journal/src/fold/people.rs:32-67`); a copied folder copies the profile. 3 tests pass (`out/161-device-grouping-tests.txt`). Not: the manual "attach a name to several ids" choice and its undo; a person with no username on a new computer shows as "Teammate n" (by reading). | 45237d52, 658ed0bc |
| #78 | See the history of my own changes | **Partly fixed** | History with Restore in the conflict review, check marks, alignments and comments; journey C7a passes. Not: a verse's text has no History outside a conflict (`repro/78-verse-history.test.mjs`: two saves, 0 History controls; `crates/tc-ui/src/views/translate.rs` never records it). | c61e2873 |
| #27 | Merge concurrent edits instead of refusing | **Fixed** | `repro/27-concurrent-saves.rs`: two writers from one state: status + bookmark merge with 0 conflicts; the same field: newest wins, older in History; verse text: a conflict with both versions (H5). `merge_bookmark_and_status`, `a_stale_token_makes_a_conflict_never_a_revert`. | 6c21b469 |
| #743 | A window from the click | **Fixed** (as ruled: no splash) | `repro/743-first-launch-window.test.cjs`: window at 338 ms, tc4 address at 456 ms, "Preparing resources: 0 of 3". `shell/e2e/lifecycle.test.cjs` 4/4. Owner ruling 2026-10-18 (APP-DESIGN §5.3): no splash, counts only. | 726b83f3 |
| #741 | Help wordmark renders cleanly | **Partly fixed** | Weight 600, antialiased (`repro/741-help-wordmark.test.mjs`). Not: 15.5px not 16px, letter spacing −0.009em not +.01em, Mulish not bundled; no Mac check (`crates/tc-ui/help/assets/style.css:23-27`). | — |
| #642 | No native title bar on Linux | **Same bug** (kept to match 4.0) | `shell/lib.cjs:302-319`; `linux_window_keeps_native_frame` asserts the menu bar (`out/642-linux-frame.txt`). No ruling or DIFFERENCES entry. | 8743dda1 |
| #278 | Show the build | **Partly fixed** | About shows "4.1.0-preview (dev)" (`repro/278-about-build.test.mjs`). Not: nothing in the console without a click. | a1747e55 |
| #496 | Hide "Stay signed in" without a keychain | **Partly fixed** | The box is hidden and the sign-in lasts the session (`repro/496-no-keychain.test.mjs`). Not: the "not available on this computer" line the issue's ruling removes still shows (`crates/tc-ui/src/app/account.rs:112`). | 55022e81 |
| #552 | Dropdown on a proven library | **Partly fixed** | Add-book dropdown keys and outside press work (`repro/548-552-book-dropdown.test.mjs`). Not: a press on the field's label with a search typed reopens the list and clears the search (`crates/tc-ui/src/ds/select.rs:290-316`). | 24efbbc3 |
| #548 | Dropdown follow-ups | **Fixed** | Same repro: Home/End move the caret in the search field, no duplicate ids, "jhn" → John, "1jn" → 1 John. `a_book_matches_by_name_or_code_prefix`. | 2bd7e4c0 |
| #731 | Report a problem attachment language | **Partly fixed** | `repro/731-attachment-in-arabic.test.mjs`: the attachment is English in Arabic, as designed (APP-DESIGN.md:1545), the platform line names OS and architecture. Not: the dialog does not say, in the app language, that the attachment is English. | 49d84f3a |
| #701 | Urdu (Devanagari) direction | **Question answered** | Left to right, named उर्दू (`crates/tc-resources/src/gateways.rs`). `repro/701-urdu-devanagari-card.test.mjs`: Get source texts card `ltr उर्दू`. | 34cc305c |
| #525 | Choose the app language on first launch | **Same bug** (not built) | `repro/525-first-launch-system-language.test.mjs` with `LANG=fr_FR.UTF-8` and a fr-FR browser: starts in English (`crates/tc-app/src/prefs.rs:675-681`). | — |
| #167 | Which UI languages, by when? | **Question answered** | Ten: en, es-419, fr, hi, pt-BR, id, uk, ru, vi, ar (`crates/tc-ui/src/i18n.rs:57-59`), decided in #522, #622, #649. `the_registry_names_the_ten_decided_locales_each_with_a_catalog`. | 3c0f72ad |

## Counts (the 38 checked here)

Fixed: 15 (#664, #179, #131, #661, #595, #594, #518, #747, #710, #582, #746, #27, #743, #548,
#420). Question answered: 5 (#663, #705, #707, #701,
#167). Partly fixed: 11 (#568, #409, #628, #761, #161, #78, #741, #278, #496, #552, #731). Same
bug: 6 (#758, #274, #121, #118, #642, #525). Not reachable: 1 (#549).

## Where the 2026-10-23 sweep is wrong

- Tests that prove a library or an omission, not the feature: #758 (`Aligner::clear` with no
  control), #274, #118, #121 (the statement counted as the fix), #525 (only the default `en`).
- Tests that fake the reply: #628 (`helps/article`), #131 (`editor/sources`; the Rust tests hold).
- `docs/TC4-DELTA-2026-10-19.md` says articles show line breaks properly (#628); they show `<br>`.

## New, beyond the issues

- A Bible left at the Add-book step with no book is not a valid Scripture Burrito
  (MissingMetadata), seen while writing the #548 repro.
