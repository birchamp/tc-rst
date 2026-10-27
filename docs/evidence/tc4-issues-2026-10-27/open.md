# Open tC4 issues tested against the rebuild, 2026-10-27

- Rebuild: main 58e8e10b, branch `tc4-issues-2026-10-27`. Report only: no product code changed.
- Issues: unfoldingWord/translationCore4 #840, 839, 837, 836, 835, 833, 832, 831, 830, 829, 825,
  822, 821, 802, 764, read with `gh issue view` on 2026-10-27.
- Scope: 4.1.0 as ruled in `.audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md` (tC3 import
  and Update a book kept; tC4 4.0, outside USFM and outside Burrito imports cut), H14 for #837,
  Q39 for #840.
- Method: each reachable issue was driven in the real app (`tc4 serve` built with
  `test-hooks,fixture-backend`, the dev UI build, Chromium through `parity/app/journeys/kit.mjs`,
  fixtures from `fixtures/` and the `sample` / `en-pins` scenarios), or through a Rust test on the
  public API. The existing tests the 2026-10-23 sweep (`docs/TC4-ISSUES.md`) names were run too,
  and judged against each issue's own steps; several mock the server reply with `page.route` or
  use the sample project, so new repros drive the issue's real path.
- Files: `repro/` (the tests), `out/` (their output; the command is at the top or in the header of
  each repro), `shots/` (screenshots). Nothing here is in the gate: the journeys sit outside
  `parity/app/journeys/*.test.mjs`, and the Rust repros were copied into their crate for the run
  and removed again (their header names the target). A repro that fails is a "same bug".

## Verdicts

| Issue | What | Reachable in 4.1.0 | Verdict | Evidence |
|---|---|---|---|---|
| #840 | Mark the note's quote in the original-language pane | Yes (built ahead, ruling Q39) | **Fixed in rebuild** (Greek in the app; Hebrew at the matcher only) | `repro/840-orig-highlight.test.mjs`: no current card marks 0 words; occurrence 1 marks word 2 Θεοῦ, occurrence 2 marks word 10 Θεοῦ. tc-helps `highlight` tests pass (incl. JON 1:3 occurrence 2). `out/840-*.txt`, `shots/840-orig-highlight.png`. Not checked in the app: a Hebrew verse, since no fixture has notes, words or checks for Jonah (`shots/840-hebrew-jonah.png`). |
| #839 | Second tab on the rig has no single-instance guard | No | **Not reachable** | tC4 rig machinery. The rebuild's shell takes Electron's lock (`shell/main.cjs:39`), and `new_tc4_waits_for_the_old_one_then_names_the_holder` passes. The page gets its token only in the launch URL's fragment, so a window opened from an app link cannot call the API. A second tab does open in the test harness (see #802), but the store lives in the server, so it is no second copy of the project. |
| #837 | Open from Door43 under a new name: card shows the old name | No (ruling H14) | **Not reachable** | The open asks for no name, so no chosen name can be lost; two cards of one name each say where they live. `an_open_asks_no_name_and_keeps_the_teams_name`, `two_cards_of_one_name_each_say_where_they_live` pass (`out/existing-rust-tests.txt`). Open-from-Door43 journey not run. |
| #836 | English helps read right to left under an Arabic set | Yes | **Fixed in rebuild** for questions; small remainder | `repro/836-mixed-direction.test.mjs`: under an rtl set, the English question and answer compute ltr, the Arabic ones rtl. Remainder: note and word card **titles** still compute rtl (e.g. "vintro", "Paul"), so an English title ending in punctuation would show it at the wrong end. No Arabic helps in `fixtures/`, so the question text is injected; the notes and words are the server's real ones with the set direction forced to rtl. `shots/836-mixed-direction.png`. |
| #835 | Import page and server disagree on language codes | Yes (tC3 import review) | **Fixed in rebuild** | `repro/835-tc3-language-codes.test.mjs`: the page accepts x-scrqa, x-scratchqa, x-qiu, es-419, qaa and kau, and importing with x-scrqa, qaa or kau makes the project with that tag, no raw error. The hint's "kau" is a code the rebuild accepts. The existing tc-app test accepts either outcome and tries neither kau nor qaa. `shots/835-tc3-kau.png`. |
| #833.1 | Offline update check shows "Failed to fetch" | Yes | **Fixed in rebuild** | `an_offline_update_check_shows_one_plain_sentence` passes; it checks the typed refusal, not the words on screen. |
| #833.2 | Save dialog title is a blob URL | Yes | **Can't tell** for exports other than PDF | PDF: `export_save_dialog_has_a_readable_title` passes ("Save PDF"). Other exports save through `<a download>`; the shell's `will-download` sets no dialog title (`repro/833-2-download-dialog-title.test.cjs` reads `""`). Under xvfb no dialog window appeared, so the real title could not be read; Electron may then show the URL, which would be the same bug. Needs a look on a desktop. |
| #833.3 | Import refusal shows an internal code | Yes | **Fixed in rebuild** | `an_import_refusal_shows_a_sentence_not_a_code` passes. |
| #833.4 | No right-click menu in text fields | Yes | **Fixed in rebuild** (no Select all) | `a_text_field_right_click_offers_cut_copy_paste` passes under xvfb with Electronite 37.1.0: menu `[cut, copy, paste]`. tC4's issue also asks for Select all, which is missing. `out/833-shell-tests.txt`. |
| #833.5 | Get source texts names v89 while v91 is installed | Yes | **Same bug** | `repro/833-5-source-texts-versions.test.mjs` fails: the English card says "notes v89 · words v89 · academy v89" while en_tn, en_tw and en_ta v91 are installed and new projects pin v91. Cause: a fixed `("v89", "v89", "v89")` in `crates/tc-resources/src/gateways.rs:108`. The sweep's "cannot occur" is wrong. `shots/833-5-source-texts-versions.png`. |
| #832.1 | Home badge keeps LTR after Settings → Right to left | Yes (tC3 import) | **Fixed in rebuild** | `repro/832-tc3-rtl-badge.test.mjs` on `cfm_fbt_tit_book.zip`: the card reads LTR, then RTL after Save and a reload (`shots/832-tc3-rtl-badge.png`). Restart case: the Rust test passes on the sample project. |
| #832.2 | Imported project's Settings shows an empty language name | Yes (tC3 import) | **Same bug, worse** | Same repro fails: expected "Chin, Falam", got "cfm". `metadata.json` itself is written with `"name":{"en":"cfm"}`, so the name is lost, not only hidden. Cause: `create_details` (`crates/tc-app/src/import.rs:71`) sets `language_name: String::new()` and `direction: Ltr` and never reads the tC3 manifest. The existing test passes only because it uses the sample project. **Also:** a right-to-left tC3 project imports as left to right, for the same reason. |
| #831 | Resume card mixes one verse's number with another's text, or shows `___` | Yes | **Fixed in rebuild** | `repro/831-resume-card.test.mjs`, the issue's steps (save 1:1; type in undrafted 1:6 and Cancel; open Check › Translation Words; Home after each): the card reads `Last edited verse 1 — "Pablo, siervo de Dios … editado"` every time, never `___`. Opening a Check tool writes no Resume record. The existing Rust test covers neither step. `shots/831-resume-card.png`. Not checked: a Cancel after the 2 s autosave. |
| #830 | OT verse: placed words read backwards on a card | Yes | **Fixed in rebuild** | `repro/830-ot-card-order.test.mjs`, real path, Hebrew original: cards keep Hebrew order (x 1149, 1063, 977); in a Spanish project "Vino" x=1100 then "palabra" x=1153, ltr; in an Arabic project two words read right to left. The existing journey only mocks `origDir`. `shots/830-*.png`. |
| #829 | Poem indents 8 characters wide; Tab in the Verse editor leaves the field | Yes (REF1b shows poem lines as tabs, as rc.2 does) | **Same bug** (both parts) | `repro/829-poem-tabs.test.mjs` fails: computed `tab-size` is 8 in the Verse and Section editors. Tab in the Verse editor closes the card and the typed `XYZ` never reaches the verse; the Section editor indents. Nothing in tc-ui sets `tab-size`; `VerseEditor` (`crates/tc-ui/src/views/translate.rs`) has no Tab handler. The sweep's "not applicable" is out of date. `shots/829-*.png`. |
| #825 | A Translate save drops a link with no flag | Yes | **Same bug** | `repro/825-tie-drop.test.mjs` fails: TIT 1:1 `Dios y Dios y y Dios`, first Dios on Θεοῦ, saved in Translate as `y y Dios y`. After: links `[]`, `invalid: false`; the save reply carries no dropped links (tc-store returns `dropped_links`, tc-app never reads it); Translate says nothing; Align on the next open shows 0 placed, no `align-dropped`, no `align-stale`. The sweep's "Align's half fixed" holds only for the mocked reply. `shots/825-*.png`. |
| #822 | Import always uses the eng versification | Yes (tC3 import) | **Same bug** | `repro/822-tc3-org-versification.rs` (tc-app): a tC3 Malachi shaped like `org` (3 chapters, 3:19–24) imports with `vrs.json` Malachi `["14","17","18","6"]`, the `eng` shape (`StandardScheme::DEFAULT`, `crates/tc-app/src/import.rs:900`). No verse is lost: MAL.usfm keeps 3:19–24. Update a book uses the project's scheme. H13 is still with the owner. Not checked: what the editor and checking screens show for 3:19–24. |
| #821 | A journal with a book and no versification frame cannot checkpoint | No | **Not reachable** | `repro/821-open-tc4-repo.rs` (tc-sync): opening a tC4 team repository from Door43 is refused `Blocked(Tc4TeamRepository)` and places nothing (the existing test covered only sync). A rebuild repository with `ingredients/vrs.json` deleted opens, saves, writes `vrs.json` again and stays a valid Burrito. tC4 4.0 import is cut. |
| #802 | A book save overwrites newer text from another tab | Not as in tC4 (one window, see #839) | **Fixed in rebuild** | `repro/802-two-tabs.test.mjs` passes: two pages on one `tc4 serve`, both on TIT 1:5; B saves, then A (typed before B's save) saves. Result: a conflict with 2 versions ("2 versions · Review" on the verse, `shots/802-two-tabs-a.png`), never a revert. The second tab opened with no "already open" notice; nothing was lost. |
| #764 | Progress bars look empty at low progress | Yes | **Fixed in rebuild** | `repro/764-progress-fill.test.mjs` (en-pins, Translation Words, 3 of 182): picker card and sidebar fill 6 px (raw 4.3–4.5 px) and say "3 of 182 resolved" in `aria-label`. `aria-valuenow` is still rounded (1 of 5640 would read 0). Align and Community Checking bars have no minimum, as tC4. The 6 px minimum is in `parity/app/residuals.json` but not in PLAN's declared differences. `shots/764-*.png`. |

## Counts

Same bug: 5 (#825, #829, #832.2, #833.5, #822). Fixed in rebuild: 11 (#840, #836, #835, #833.1,
#833.3, #833.4, #832.1, #831, #830, #802, #764 — #833 and #832 counted per item). Not reachable: 3
(#839, #837, #821). Can't tell: 1 (#833.2, non-PDF exports).

## Where the 2026-10-23 sweep is wrong

- #833.5 "cannot occur": it does (`gateways.rs` fixed v89).
- #832.2 "covered": the test runs on the sample; a real tC3 import loses the language name.
- #825 "Align's half fixed": on the real path Align shows nothing; the test mocks the reply.
- #829 "not applicable": REF1b made poem lines tabs, so the bug is the rebuild's too.

## New, beyond the issues

- A right-to-left tC3 project imports as left to right (`create_details`, as #832.2).
- #836's remainder: note and word card titles take the set's direction, not their own.
