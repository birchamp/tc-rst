# tC4 issues tested against the rebuild

- Date: 2026-10-19. Companion to `TC4-DELTA-2026-10-19.md` (the commit range d310bf3 to
  9ad4d5c and the owner's decisions Q30 to Q36).
- Scope: five areas (align; USFM, Translate and helps; check and resources; sync, storage
  and import; shell, startup, languages and help). Each area lists the issues named for it
  and every other tC4 issue in that area opened or updated since 2026-10-01. An issue that
  touches two areas appears in both, with each area's verdict.
- Method: each area was triaged, then checked by a verifier who reran the claims. Where the
  verifier overturned a verdict, this file uses the verifier's status, evidence and action.
  Claims were proven by running code in detached worktrees of `claude/focused-ride-5uzj7p` at
  6ba7514, which were removed afterwards. Nothing was written to either checkout.
- Latest tC4 state, checked again on 2026-10-19: `main` is 9ad4d5c; the newest issue is
  #802; no newer issue or tag exists. Open pull requests #785, #799 and #800 may land soon.
- Test names are the names the rebuild would use. A name in backticks with no note exists
  and passes today; "to add" or a unit name means it must be written.

## Status key

| Status | Meaning |
|---|---|
| rebuild bug | The rebuild's code or design is wrong today. Reproduced by running code, unless the row says it is a design defect. |
| unit todo | A note for a unit that is not built yet, so the builder gets it right the first time. |
| reference change | tC4 changed after d310bf3. Match it when the reference moves. |
| covered | The rebuild already does what the issue asks, and a test that can fail proves it. |
| owner question | Needs the owner's decision. See `TC4-DELTA-2026-10-19.md` §5. |
| not applicable | tC4 process, tC4-only machinery, or outside tC4 4.0's scope. |

## Counts

| Area | rebuild bug | unit todo | reference change | covered | owner question | not applicable | Rows |
|---|---|---|---|---|---|---|---|
| Align | 7 | 7 | 2 | 13 | 1 | 1 | 31 |
| USFM, Translate and helps | 1 | 30 | 1 | 6 | 0 | 8 | 46 |
| Check and resources | 3 | 19 | 3 | 13 | 1 | 24 | 63 |
| Sync, storage and import | 4 | 29 | 3 | 10 | 0 | 11 | 57 |
| Shell, startup, languages and help | 2 | 38 | 1 | 14 | 0 | 17 | 72 |
| All area rows | 17 | 123 | 10 | 56 | 2 | 61 | 269 |

The commit range adds 22 items (`TC4-DELTA-2026-10-19.md` §2): 4 rebuild bugs, 4 unit
todos, 5 reference changes, 3 covered, 3 owner questions and 3 not applicable. Counting each
defect once across all sources, the rebuild has 17 bugs today (§1). The counts are the
triage's; the merge of `windows-push-refusal` later on 2026-10-19 fixed one of them (#710, §1
row 14).

## 1. Bugs the rebuild has today

Each row was reproduced by running code, except #767, a design defect in a planned path.
The first four come from tC4's commit range; the rest come from the issue triages.

| # | Issue | Failing scenario | Crate | Regression test | Fix |
|---|---|---|---|---|---|
| 1 | #773 | GEN 33:5 note mix3 is titled "his eyes … the women … whom God has graciously given to your servant". gyfv, r2u9 and h9hm are also wrong; only zczu is right. 38 of the 2,224 Genesis Notes checks take this path. | tc-helps (`highlight_quote`, used by `CheckItem::title`) | `a_reordered_quote_lights_only_its_own_phrase` (tC4's five vectors on `GEN-773.usfm`, plus TIT 2:2 "in love") | Port `shortestRunInstances` into the multi-word fallback. Keep the single-word and strict paths. h9hm is a plain note, so it affects only the helps highlight. |
| 2 | #782 | The shown text of en_ult Genesis 1:1's aligned content is "In the beginning God created  and the earth."; the aligner has 8 words and no "heavens". PSA 34:4, kept as written, has no "Yahweh". 633 of 1,533 Genesis verses with markup lose words; 67 of the 265 en_ult verses the import keeps as written lose words. | tc-usfm (`shown_text_v1`), read by tc-align and tc-journal | `shown_text_reads_words_inside_nested_alignments`; `a_verse_kept_as_written_shows_yahweh`; `selah_inside_qs_shows`; plus a tC4-imported verse with nested `\zaln` | Walk `\zaln` and word nodes at any depth, as tC4 9ad4d5c does. Add tC4's vectors (`fixtures/usfm/shown-text-vectors.json`); the existing vectors do not change. Update STORAGE §7.7 ("Shown text and hashed text"). Land before format 1.0. |
| 3 | #795 | After the import split, MAT 5:22 shows "brotherwill", MAT 5:44 "enemiesand" and 1TH 3:2 "Godin" (12 Matthew and 3 1 Thessalonians verses). Opening MAT 5:22 in Align drops 2 of its 44 links, and "brotherwill" can never be aligned. | tc-usfm (`shown_text_v1`), tc-align | `a_footnote_between_words_keeps_its_space`; `imported_mat_5_22_opens_with_all_44_links` | Keep the character after `\f*` or `\x*` when the note sits between two non-space characters (tC4 PR #800), and after every other marker the shown text leaves out (`\va`, `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*`…), which PR #800 does not (declared difference, PLAN). Same change as #782. |
| 4 | #787 | A record saved for "Pablo, siervo de Dios", reopened against "Pablo, esclavo de Dios", gives `is_stale()` false. "The draft changed" never shows after a sync, a kept version or an outside edit changed the verse. | tc-align (`Aligner::open`, `is_stale`), tc-api (`Freshness`) | `a_stale_value_opens_with_the_draft_changed_notice` | Done by FIX-align: `Aligner::is_stale` is the flag set at open by `needs_review` (row 6), kept until the first edit, Mark valid or `keep_reflow`; tc-api's `Freshness::Stale` carries it. The save on leaving stays (AD19). |
| 5 | #796 (word list) | For "Pablo, siervo\f + \ft o esclavo\f* de Dios", the bank holds "siervode" but the export expects "siervo" and "de". After every word is placed and the verse is marked valid, `weave_verse` fails with `TargetWordNotFound("siervode")`, so the export drops the verse's alignment with no message. | tc-align (`TargetVerse`), tc-usfm | `the_bank_is_the_export_for_every_vector`, `the_bank_is_the_export_for_every_verse_of_the_bundle_and_the_fixtures` (every verse of the ULT and UST v91 and the fixtures) and the proptest `the_bank_is_the_export_for_generated_verses` (`tc-align/tests/bank_words.rs`): every banked word placed weaves, and the woven verse holds exactly the bank | Done by FIX-text: `TargetVerse` banks `tc_usfm::target_words`, the export's own list, which reads word markup through; the shown text decides no word. |
| 6 | #796 (review rule) | (1) After a source upgrade the rail shows Invalid, but the open verse shows no notice. (2) A record with the current hash that holds "siervo" for "Pablo, esclavo de Dios" shows Valid 4/4; it opens with a dropped link and the export fails. (3) Editing "Selah" to "Pause" inside `\qs` keeps the old alignment, and the rail stays Valid. | tc-align (`value.rs`, `aligner.rs`), tc-store (reflow, R25) | `a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail`; `after_a_source_upgrade_the_open_verse_shows_the_banner`; `a_verse_edit_of_a_word_the_hash_leaves_out_reflows_its_alignment` (tc-store) | Done by FIX-align: `tc_align::needs_review` (the journal's staleness, a word the record holds that is not the verse's, or a card for a word the pinned verse lacks) decides the rail (`verse_status`), the notice and the reflow, and is D2's to call. `tc_align::reflow` carries a record whose words differ under a current hash. Left for storage: the fold's `invalid` reads the hash only, so the alignment files call such a record valid (`screens_and_fold` counts them: 168 of the Psalms corpus's 357, 0 in real data). |
| 7 | #794 | The en_ult TIT 1:1 aligned body opens in the aligner with 29 words. Its hashed text is punctuation only, so a save vouches for nothing, and the next verse edit restamps it. | tc-align (`Aligner::open`), tc-store (reflow) | `a_verse_that_holds_alignment_markup_does_not_open` (tc-align); `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (tc-store) | Done by FIX-align: `TargetVerse::holds_alignment_markup`, `Unavailable::AlignedInText` (it wins over the others) and `NoReflow::AlignedInText`. D2 keeps its bytes. |
| 8 | #793 | A memory trained on JON 1:1-12 with NFC Hebrew source words (as tC4 4.0 stores them) gets 45 of 70 right for JON 1:13-17 asked with the raw words, against 51 asked in NFC: 12% fewer for every Hebrew project imported from tC4 4.0. | tc-align (suggest keys, `words.rs`) | `memory_trained_on_nfc_records_suggests_for_raw_source_words` (equal right counts) | Done by FIX-align: the engine puts every token's text and lemma in NFC where a sentence reaches it (`wordmap::tokens`); without it the test gets 31 right against 37. |
| 9 | #790 | The sample TIT 1:1 record opens with 10 cards holding 11 source words; UGNT TIT 1:1 has 17, so six Greek words can never be aligned. A card holding Σαῦλος, which is not in the pin, stays, with nothing dropped. STORAGE says this check happens; no code does it. | tc-align (`Aligner::open`) | `the_sample_opens_with_all_17_ugnt_words`; `a_link_to_a_source_word_the_pin_lacks_is_dropped_and_shown`; `opening_a_repaired_verse_writes_nothing`; `an_imported_nfc_hebrew_record_keeps_its_links` | Done by FIX-align: `Aligner::open` fits the cards to the pin, in source order, matching a word as written, else in NFC with the pin numbered in NFC. A word stays on two cards only while both hold links (the ULT's own form); an empty second card goes (324 in the ULT's Psalms). A value with `x-ref` is left as it is until #409. A card the pin cannot hold is part of `needs_review`, so the rail shows such a record Invalid and the verse opens stale (`a_done_record_with_a_source_word_the_pin_lacks_is_invalid_on_the_rail_and_opens_stale`). Declared difference (PLAN). |
| 10 | #789 | In TIT 1:1, link Θεοῦ¹ to Dios¹ and Θεοῦ² to Dios², then delete "de Dios" after "siervo". The surviving Dios, linked to Θεοῦ², moves to the Θεοῦ¹ card; Θεοῦ² is empty; nothing is flagged and the rail stays green. | tc-align (`reflow`), tc-store (verse save and section save) | `deleting_an_earlier_copy_keeps_the_link_on_its_physical_word`; `inserting_an_earlier_copy_renumbers`; `delete_then_readd_never_reattaches_an_old_link`; a tc-store section-save case | Done by FIX-align in tc-align, Q30 (a): `tc_align::reflow_edit(value, before, after)` follows each link by a longest-common-subsequence match of the two texts; a link the match does not place for sure returns to the bank and is reported; no review mark. Without the earlier text (`reflow`, or `Aligner::open` without `made_against`) a link to a changed verse keeps its occurrence only while the verse holds its word once, as when it was placed: a copy deleted and another typed elsewhere leaves the count, so a link to a repeated word returns to the bank, listed, and the open does not save it (`without_the_earlier_text_a_link_never_moves_to_a_retyped_copy_of_its_word`, `a_link_to_a_retyped_copy_of_its_word_opens_dropped_and_is_not_saved_on_leaving`). tC4's shown-text stamp counts as the same text. tc-store's verse and section saves call `reflow_edit` from the old text (`a_link_follows_its_own_copy_of_a_repeated_word`). The app's open, when it is wired, passes the verse version whose hash is the value's. |
| 11 | #661 | Change only the en_tw pin from v90 to v91 when a decided Words row (2SA vwrw, `kt/arcofthecovenant`) loses its article. `consequences` reports unaffected 1, affected 0, so the upgrade dialog says nothing changes. The decision itself is kept and listed off-list. | tc-check (`change::consequences`) | `a_tw_only_change_that_drops_a_decided_article_is_counted`, with a no-change control | Give `consequences` each set's tW article set before and after, and count a row whose item leaves the list. |
| 12 | #701 | Urdu (Devanagari) is right to left with the name اردو. Once B3 renders the registry, Devanagari helps read right to left under an Arabic-script name. | tc-resources (`gateways.rs:122-132`) | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | Left to right and उर्दू. Record the difference and tell the owner. B3 prefers the direction in the installed helps' manifest. |
| 13 | #552 | A Select made disabled while its list is open still accepts a row click and calls `on_change`. | tc-ui (`ds/forms/select.rs`) | ds-gallery case `Select/disabled-while-open` | Return early in `choose` when disabled, and close the list when it becomes disabled. Check the label-press edge in the same case. |
| 14 | #710 | `tc-door43-fake` refuses a push without write access with HTTP 403. Door43 answers 200, with "User permission denied for writing." and a declined ref. A sync built against today's fake would show Door43's real refusal as an unknown error. | tc-door43-fake (`api.rs:1485-1488`), tc-door43; X | `a_push_without_write_access_is_no_write_access_and_makes_no_branch` and two more, on `origin/windows-push-refusal` until its merge | Merge `windows-push-refusal` (02300d9) before X starts. Correct STORAGE §9.5 and DELTA-10-16 Q12. Done later on 2026-10-19: merged, with both corrections. |
| 15 | #549 | tc-obs finds a project's own story pictures only by the role `x-obsimages`, but the rebuild's `metadata.json` writes no role for carried files. An imported story project's own pictures would never show, only stock ones. | tc-obs (`images.rs:168-183`, `:300-309`) | `a_project_picture_without_a_role_is_the_project_rung` | Find project pictures by path under `ingredients/` (outside `checking/`, `tc/` and `content/`), matched by file name. Declared difference. |
| 16 | #582 | Creating a project with the code `acm-x-lebarabic` or `alk-alakatapue` succeeds, and `metadata.json` says the language is "und / Undetermined"; the name and script direction are dropped. | tc-store (`Store::create`, `apply_upgrade`) | `create_refuses_a_language_tag_the_burrito_cannot_hold` | Check every seed record with `tc_burrito::check_field`, and refuse before any folder exists. The dialog's offer is Q35. |
| 17 | #767 | Design defect; no code runs it yet. Add a book from USFM is planned as AddBook ("___" stubs), then one SaveSection per chapter. A crash or refusal between them leaves a stub book, and every retry is refused as "already in the project, or was removed". | tc-store and tc-journal (no one-batch command; APP-DESIGN §9.2 R3) | `adding_a_book_from_usfm_is_one_batch_whole_or_absent` (FaultFs at every write point) | A one-batch command, cause Import, with §13.5's alignment split. Remove R3's fallback from C6 (a change to Q-17). |

Other findings from the verifiers that are not bugs in shipped behavior:

- #206: `sigterm_quits_like_the_quit_message` is flaky. tc4 prints its address line before it
  installs its SIGTERM handler, so an early SIGTERM ends it by the default action. No work is
  at risk. Create the signal stream in `Server::bind`.
- #161: a project folder copied by hand to another computer copies the first person's
  Door43 name, so a teammate's work would show as "Mine". Nothing is lost. Unit X treats a
  copied name as provisional.
- #179, #386 and #747: the behavior is right, but no test proves it; the rows say which test
  to add. The S2 `.bak` test passes even with the `.bak` rule deleted.
- #525: until C1a, a chosen interface language is lost at relaunch, because the production
  config keeps app data in memory.

Found by FIX-text (2026-10-19) (`docs/evidence/shown-text-2026-10-19.md`); each says whether
the rebuild fixed it or does as tC4 does:

- **Hidden verse text.** tC4's `verseText` hides the items of a `\li` list and the lines of a
  marker usfm-js 3.4.3 does not know and keeps as `content` (`\qm1`), while the export's
  `target_words` counts their words, so Check and the Align bank cannot reach them. In UST
  v91 EXO 15:21 the split plain text hid "Sing to Yahweh … enemies.", so 11 of the 28 words
  the import placed dropped at open and at the next verse save. Fixed in the rebuild as a
  declared difference (`docs/PLAN.md`): the content of a paragraph marker but `\esb`, and of a
  marker usfm-js does not know that USFM reads as a character marker (a typo such as `\pp`, a
  custom `\zx`, with or without its end marker), is shown
  (`list_items_and_unknown_paragraph_lines_show`, `imported_ust_exo_15_21_opens_with_all_its_links`).
- **Words that still join** in text that still holds alignment markup, where usfm-js misreads
  it (UST 2SA 23:8 "HeonceHe" and "killed800"). tC4 (PR #800 included) does the same. UST 1KI
  10:12, whose markup splits "such" into the aligned words "s" and "uch", shows "such". Neither
  is in Genesis or Psalms; the import cannot split them, so it keeps them as written, and #794
  keeps them out of the aligner. ULT 2SA 23:8 ("spearagainst" in tC4) and a note inside an
  alignment run no longer join: a space is put back where a left-out object stands between two
  letters with no space kept after it.
- **Words after a nested character span** (`\wj Worship the \+nd Lord\+nd* your God\wj*`):
  tC4's `verseText` shows only "Worship the ", so Check and the Align bank miss words the export
  counts, and a bank without them cannot be woven (#796). Fixed in the rebuild as a declared
  difference (`docs/PLAN.md`); no ULT or UST v91 verse has one.
- **Words that join across a span or paragraph marker** (`the \add Lord\add*s house`,
  `Yahweh\q2 que`, adjacent ruby): tC4 banks "Lords", "Yahwehque" and "神愛", which the export
  splits. Fixed in the rebuild as a declared difference (`docs/PLAN.md`;
  `a_skipped_marker_right_before_a_letter_leaves_a_space`). The woven form of a ruby word
  (`\rb \w 神|…\w*|かみ\rb*`) still shows its gloss ("神|かみ"); the import's split reads it
  back to the plain form, whose bank is the export's.
- **Text after a character span that holds a note** (`\wj Blessed are the poor\f + \ft Or
  humble\f* in spirit\wj* and he went on`, valid USFM common in red-letter Gospels), and after
  a stray end marker (`a\nd* b`): usfm-js keeps it as the `content` of the end marker's own
  object, which tC4 hides, so the bank lacks words the export counts and the verse save drops
  their links. Fixed in the rebuild as a declared difference (`docs/PLAN.md`;
  `words_after_a_span_that_holds_a_note_show`). The rebuild's walk of a span's children had
  also shown the words of a character span nested in such a note; they stay hidden now.

## 2. Align

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #796 (word list) | The aligner and the export split a footnote verse into different words | rebuild bug | tc-align, tc-usfm; D2 | `a_footnote_right_after_a_word_banks_both_words_and_weaves` | See §1 row 5. Port tC4 PR #800's separator rule into the shown text. |
| #796 (review rule) | One "needs review" rule for the rail, the open verse and the export | rebuild bug | tc-align, tc-store; B6, D2 | `a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail` | See §1 row 6. Keep tC4 4.0's rule that an edit clears the re-review flag until the owner rules otherwise. |
| #796 (export) | Never drop a verse's alignment silently, and keep imported markup | unit todo | D2 | `export_reports_and_keeps_a_verse_that_needs_review`; `export_keeps_inline_zaln_bytes_even_with_a_matching_record`; `a_record_with_nothing_placed_exports_the_stored_bytes` | Weave only a verse that passes `needs_review` and whose words match. Otherwise write the stored bytes and name the verse in a notice. Check for inline `\zaln` before validity (tC4 PR #799). |
| #795 | One hash for the aligner and the alignment files | covered | tc-store, tc-align | `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen` (to add, from the scratch test that passed) | The rebuild stamps `plain_text_v1` of the stored NFC verse everywhere. The display half is #796 (word list). |
| #794 | Do not align over a verse that holds imported alignment markup | rebuild bug | tc-align, tc-store; B6, I | `a_verse_that_holds_alignment_markup_does_not_open` | See §1 row 7. I lists such tC4 books in the import report. |
| #553 | Keep the alignments of an aligned USFM file on import or Add a book | unit todo | I (C6 done) | J9 `aligned_titus_import_has_no_inline_zaln_and_46_records` | Both paths call `split_aligned`. A verse it refuses keeps its text, is counted, and falls under #794. Compare in NFC (#793) and per verse with x-ref (#409). |
| #793 | Suggestions: Hebrew source words in two Unicode forms | rebuild bug | tc-align; B6 | `memory_trained_on_nfc_records_suggests_for_raw_source_words` | See §1 row 8. |
| #792 | A span made while an alignment save waits leaves orphan records | not applicable | tc-store; B6 | scratch `zz792_late_alignment_save_after_join` (passed) | No book-wide buffer. A late save after a join is kept and offered at the joined run ("Use as a starting point"). |
| #791 | Tell a translator when a teammate aligned the same verse differently | unit todo | E1a, B11 | J26 step `two_actors_align_one_verse_and_the_row_and_review_show_it` | Storage keeps both as a conflict. B10 built the sync result's note "1 verse has two alignments to review." with Review (`a_verse_with_two_alignments_is_named_and_offers_review`, `the_toast_counts_verse_conflicts_alignments_and_the_rest_apart`); the per-verse rows of D108 (book, chapter, verse and teammate) wait for tC4 to ship them; B11 shows the "2 versions" marker in Align. |
| #790 | Stored source cards are never compared with the pinned verse | rebuild bug | tc-align; B6 | `the_sample_opens_with_all_17_ugnt_words` | See §1 row 9. |
| #789 | A link moves to another copy of a repeated word | rebuild bug | tc-align, tc-store | `deleting_an_earlier_copy_keeps_the_link_on_its_physical_word` | See §1 row 10 and Q30. |
| #788 | A card click places a word that a suggestion moved | covered | tc-align; B6 | `only_the_shown_selection_is_placed_and_the_cards_look_armed_only_while_it_is_not_empty` | B6 draws the armed look and accepts drops from `is_armed` and `selection()` only. |
| #787 | A changed verse opens with its old bank and no warning | rebuild bug | tc-align, tc-api; B6 | `a_stale_value_opens_with_the_draft_changed_notice` | See §1 row 4. Saving a lossless carry on leaving stays (AD19), as a declared difference. |
| #782 | Words inside nested alignments are missing | reference change | tc-usfm; B4a, B5b, B6, B8 | `shown_text_reads_words_inside_nested_alignments` | Port 9ad4d5c's verse text. The USFM area and the commit range rate it a rebuild bug (§1 row 2). |
| #768 | Card labels lost after a save (Hebrew NFC) | reference change | B6 | `gateway_label_survives_an_nfc_record` | Compare in NFC on both sides, plus the occurrence. Reuse `tc_helps::highlight::normalize_word`. |
| #758 | Clear every alignment of a verse | unit todo | B6, tc-align | `clear_discards_standing_suggestions` | No Clear control while tC4 has none. `Aligner::clear` must drop standing suggestions (today one survives). |
| #754 | Keep each card's words in verse order | covered | tc-align, tc-store | `la_fe_de_los_dropped_as_fe_la_los_reads_la_fe_los` | None. |
| #753 | Preview a dragged word before or after a placed word | covered | B6 | `every_edit_leaves_each_card_in_verse_order_wherever_the_word_was_dropped` | None. The owner replaced it with #754; no preview. |
| #736 | Select several bank words and place them at once | covered | tc-align; B6 | `shift_adds_the_range_from_the_first_selected_word_in_bank_order_either_direction` | B6 passes Ctrl, Cmd and Shift to `click_bank_word`. |
| #735 | Number repeated words by occurrence | covered | tc-align; B6 | `a_repeated_word_shows_its_occurrence_and_is_named_word_n` | B6 shows the number as a superscript and uses the accessible name. |
| #734 | An unaligned word returns to its place | covered | tc-align | `a_repeated_word_returns_to_its_own_occurrence_slot` | None. |
| #568 | A keyboard-only user cannot align words | unit todo | B6 | Playwright `align_by_keyboard_only` | Cards take a Tab stop; Enter places the shown selection. Declared difference; tell the owner. Joining cards waits for a tC4 design. |
| #420 | An aligned word keeps its character style (`\add`) | covered | tc-usfm; D2 | `weave_writes_what_tc4_writes` | Add to PLAN's declared differences. J7 gets tC4's `\add` case. |
| #409 | x-ref and per-verse occurrences in a verse span | unit todo | B6, D2, I | `an_imported_span_with_x_ref_opens_with_every_link` | Keep imported x-ref. The #790 repair compares a span's source words per verse. D2 writes x-ref only once tC4 defines it. |
| #274 | A phrase suggestion over several cards | covered | tc-align | `a_prediction_over_two_cards_is_left_out_never_cut_to_the_first` | None. |
| #784 | A `\v` after a poetry marker on the same line | covered | tc-usfm | `the_check_text_is_what_tc4_reads` | Declared difference until tC4 merges PR #785. |
| #726 | Previous and Next beside the marking buttons (Align) | unit todo | B6 | DOM parity of `align-workspace-row` (en, ar, 620 px) | Row `[← Previous] [✓ Mark valid] [Next →]` as quiet pills; test ids `align-prev` and `align-next`. |
| #722 | The verse button is missing after the UGNT upgrade | covered | tc-align; B3, B6 | `staleness_names_each_reason_a_done_value_no_longer_vouches_for_the_verse` | None. |
| #720 | An aligned Portuguese Bible from Aquifer's AI alignments | owner question | B3, B9a | — | Q34. |
| #516 | Suggestions grow with every saved verse | covered | tc-align; C5 | `a_confirmed_save_joins_the_memory_at_once_and_a_resave_keeps_the_count` | C5 calls `append_verse` on every confirmed save. |
| #400 | Suggestion accuracy and ramp-up | covered | tc-align | `the_growth_bench_on_en_ult_and_en_ust_titus_gives_tc4s_recall` (release, ignored; passed) | None. |

## 3. USFM, Translate and helps

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #786 | Editing a multi-line poetry verse changes only its first line | unit todo | B4a, C2; APP-DESIGN Q-20 | `verse_editor_shows_every_poem_line`; `verse_starting_on_its_own_poem_line_opens_with_its_text`; B4b no-change section save on Psalm 3 and GEN 3:14-17 | The owner already ruled (Q-20): the box edits all of `words`, so no old line is left. B4a never takes the box's text or emptiness from tC4's `drafted` flag; a save replaces `words` only. Rewrite Q-20's wrong premise and tell the owner. |
| #784 | A `\v` after a marker is missing from Translate and Align | covered | tc-usfm; B4a, B6 | `every_aligned_ult_verse_opens_in_verse_order_with_nothing_to_save` (2,461 Psalms verses) | B4a: GEN 3:15 is listed, and an edit keeps the `\q1 ` before `\v 15`. B6: 1,533 Genesis and 2,461 Psalms verses. Optional codec test for `\p\v 6` (ZEC 2:6). |
| #802 | Refuse a book save over newer text on disk | covered | tc-store; C2 | `a_stale_token_makes_a_conflict_never_a_revert`; `loss_rename_retry_never_overwrites_outside_save` | Optional C2 two-tab Playwright case. |
| #179 | An edit whose base is unknown displaces the verse's text | unit todo | tc-journal | `loss_unknown_parent_verse_edit_keeps_standing_text` | The behavior is right (two heads, the standing text shows, a conflict marked incomplete history), but no test pins it. Commit the test. |
| #364 | Renumber verses in the editor | not applicable | later B4b, C2 | — | tC4 4.1.0. STORAGE §5.2 already has the write. Port when tC4 ships it. |
| #365 | Import a book into an existing project | not applicable | later F1, C7a | — | tC4 4.1.0. Build on the §11.1 outside-edit path when wanted. |
| #353 | ULT and UST side by side in Translate | not applicable | B4a if adopted | — | Post-4.0, no layout decided. |
| #131 | Translate's source pane in projects with other verse numbers | unit todo | B4a | `cross_frame_translate_shows_the_mapped_source_verse` (2:2 maps to 1:1, fan-out, unmapped, cross-book) | Map through `map_reference`, as Understand does. Show tC4's statements when a verse cannot be placed. Declared difference; tell the owner. |
| #121 | Understand: source text for verses mapped into another book | covered | B3 | `cross_book_verse_shows_the_statement_and_keys_comments_to_the_project_verse` (tc-ui); `a_verse_in_another_book_names_that_book` (tc-app) | Done by B3: a project verse the sources have in another book is `understand.verseCrossBook` with its place (`DAG 13:1`), never this book's text; the comment box keeps the project verse; a chapter wholly in another book says `understand.helpsCrossBook`. |
| #118 | Understand: Section view in projects with other verse numbers | unit todo | B4a (B3 done) | `cross_frame_project_shows_the_verse_only_statement_instead_of_the_switch` (B3 journey); `a_project_in_another_numbering_reads_each_verse_where_the_sources_have_it` (tc-app) | B3: one unit per project verse, each the source verse the project's numbering names (`editor/sources` `frame`), `understand.crossFrameVerseOnly` in place of the switch, `understand.frameUnavailable` when the scheme is unknown. B4a: the same on Translate. |
| #55 | Draft book introductions | not applicable | — | — | tC4 4.1.0. The introduction is kept byte for byte. |
| #27 | Merge concurrent edits automatically | covered | tc-journal | `merge_bookmark_and_status`; `loss_check_two_ways_visible` | None. |
| #767 | Import creates an empty book after the size warning | covered | C6 | `adding_a_book_from_usfm_is_one_batch_whole_or_absent`; `a_psalm_adds_whole_with_every_verse_and_its_selah` | See §1 row 17. C6 built the one-batch command `AddBookText` (APP-DESIGN C6 D3). No refusal stays, so `addBook.usfmTooLarge` is not vendored: the rebuild never refuses a book for its size. |
| #782 | Words inside nested alignments are dropped | rebuild bug | tc-usfm | `shown_text_reads_words_inside_nested_alignments` | See §1 row 2. |
| #773 | A reordered quote picks up words from elsewhere | reference change | tc-helps | `a_reordered_quote_lights_only_its_own_phrase` | The check area shows it is a rebuild bug today (§1 row 1). |
| #664 | Test the Hebrew highlight with a maqaf | covered | tc-helps | `hebrew_quotes_split_maqaf_compounds_into_the_aligned_words` | Pin tC4's GEN 1:5, 1:7 and 1:28 cases. |
| #553 | Keep the alignments of an aligned USFM file | unit todo | F1, I; B6, D2 (C6 done) | `aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records`; `a_verse_whose_alignment_does_not_read_apart_keeps_its_text_as_written` | Split on Add a book as on import (C6: `AddBookText`), and list kept verses (C6 keeps a verse that does not read apart as written; the list of them is F1's). Unit I also splits tC4 4.0 verses that hold `\zaln`. Kept verses: Align unavailable, reflow skipped, export bytes kept (#794). |
| #420 | Keep a character style around an aligned word | covered | tc-usfm; D2 | `weave_writes_what_tc4_writes` | Add to PLAN's declared differences. |
| #638 | Spike on a context-aware USFM editor | not applicable | — | — | tC4 research. |
| #600 | Save a section after deleting all of its text | unit todo | B4b | `emptied_section_save_asks_then_writes_stubs` | Port the confirmation; one change per emptied verse; Keep editing writes nothing; a span stays a span. |
| #598 | Move a placed verse number with click, then click | unit todo | B4b | J2 `break_a_span_with_two_clicks` | A pin picks up on click; drag and Enter still work; Type returns the pin. |
| #604 | New wording for the Place instruction | unit todo | B4b | J2 `section_card_reads_the_idle_place_hint` | Show `draft.pickHint`, `draft.bankEmpty` and `draft.dropHint` by state. |
| #576 | Update the Resume banner after a span save | unit todo | C2 | J8 `last_edited_names_the_span_then_its_first_verse` | A join records "2-3"; a break records the first verse of the old span. |
| #575 | A paragraph break inside a new span | unit todo | B4b | `joined_span_collapses_blank_lines_to_one_space` | Follow D94. |
| #574 | The book's name in a new blank book's header | covered | tc-store | `a_seed_keeps_structure_and_drops_words` | J7 asserts the header. Optional: a seed test for a source with no `\h`. |
| #572 | A span counts as all of its verses in the drafted percentage | covered | B2, B3 | `joining_verses_keeps_the_drafted_percentage` | Done by B2 and B3: the tiles and the rail count with `drafted::count_verse`. The poem-verse rule is a declared difference the owner was told (DELTA-10-19 §5). |
| #603 | A tooltip on each book's percentage | covered | B3 | journey `a_tile_opens_understand_at_chapter_1_with_the_rails_tooltips` | Done by B3: `draft.bookPercentTitle` on each rail row. |
| #577 | "1 chapter" and "1 verse" | unit todo | F1 (C6 done: `one_chapter_book_says_1_chapter`) | `one_chapter_book_says_1_chapter` | Choose the singular keys when the count is 1. |
| #507 | Open a new project in Understand | unit todo | B7 (B3 done) | journey `a_tile_opens_understand_at_chapter_1_with_the_rails_tooltips` | B3: an open with no remembered place lands in Understand at chapter 1; a remembered place still wins. B7: a story project at story 1. |
| #499 | Use the Bible name typed on the import review page | unit todo | F1, I | `typed_import_name_is_the_project_name` | Leave review-edited fields out of the P9 comparison, or the import refuses. |
| #484 | Add a book from a USFM file | covered | C6 | `usfm_files_add_whole_books_and_refuse_the_rest_in_their_rows`; `a_book_from_a_file_is_refused_when_present_or_when_the_file_does_not_read`; journey `create` | One batch (#767), the alignment split (#553), `addBook.usfmExists`. |
| #564 | Add-a-book USFM step polish | covered | C6 | `removing_rows_from_25_aligned_books_removes_the_intended_rows`; `an_add_keeps_its_error_until_the_next_add_and_the_books_it_added_leave_the_list`; `usfm_rows_are_judged_again_at_every_add_and_remove` | Label "Add book" while disabled; keep an Add error until the next Add; classify each file once (the server reads a file once, `create/reviewUsfm`). |
| #502 | Two USFM files never reach the review page | unit todo | F1 | J9 `a_two_file_drop_is_one_new_project` | Port the J9 case. |
| #619 | Quotes and bold in helps, not raw `>` and `__` | unit todo | B5b (B3 done) | `article_shows_no_raw_quote_or_bold_marks` | B3: `tc_helps::blocks`, tC4's renderer, held to tC4's own output on every Titus note (608 blocks). B5b: the Check article panel draws the same blocks. |
| #621 | Superscript verse numbers in the simplified-text tab | covered | B3 | journey `simplified_tab_verse_numbers_are_superscript` | Done by B3, the source verse's `c:v` in another numbering. |
| #628 | Full Markdown in helps | covered | B3 | `ta_article_with_a_table_renders_a_table` (tc-helps); journey `an_article_draws_its_table_numbers_and_nesting` | Done by B3 for tables, numbered items and nested lists, a declared difference the owner was told (DELTA-10-19 §5). Links stay their label until tC4 decides. Installed images are not drawn: tA names them by Door43 address, which no install keeps on this computer yet. |
| #602 | Resize the helps panel by dragging | unit todo | B4a (B3 done) | `helps_panel_width_follows_the_divider_drag` | B3: the divider resizes the panel within 280 to 720 px; one show/hide button. B4a reuses both. |
| #601 | Keep the article title and close button visible | covered | B3 | `article_close_button_visible_after_scroll` | Done by B3: the article's header stays in view. |
| #234 | Widen the helps pane | covered | B3 | `helps_panel_width_follows_the_divider_drag` | Done by B3 with #602's divider. |
| #728 | Make "Draft section" look like a button | unit todo | B4a | DOM parity of `en-translate-tit1` | `Button(Secondary, Sm)`, end-aligned, accent text while enabled. |
| #650 | An Arabic gateway package end to end | covered | B3 | journey `arabic_helps_panes_are_rtl_in_an_english_interface` | Done by B3: each helps slot carries its gateway's direction (`rung_dir`), and the cards, questions, simplified text and article read in it. |
| #29 | Right-to-left and complex scripts end to end | unit todo | B4a, B5b, B6, G2 | J10 `rtl_project_drafts_checks_and_aligns` | Each journey adds the Arabic case; G2 captures the `ar` screens. |
| #614 | Four English literals in design-system props | unit todo | B4b, B5b, A5 (C6 done: Add a book passes the catalog strings to its Select and its remove button) | port of tC4 `ds-labels` a–d | Pass the catalog strings; consider making them required props. |
| #702 | Text zoom sliders | not applicable | — | — | tC4 4.1.0. |
| #719 | Add source Bibles to a project | not applicable | later B9a | — | tC4 4.1.0. |
| #615 | The Understand journey after the Resume journey | not applicable | C3 (J16 port) | — | The fix is inside tC4's J16 at the reference; port J16 as written. |

## 4. Check and resources

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #773 | A reordered quote's title and highlight pick up other words | rebuild bug | tc-helps; tc-check | `a_reordered_quote_lights_only_its_own_phrase` | See §1 row 1. |
| #765 | Hebrew quote shown instead of English when it has a maqaf | covered | tc-helps | `genesis_maqaf_quotes_title_in_the_ult` (to add) | No fix. The converter B3 or B5a builds from the ULT keeps nested `\zaln`; test 0 Hebrew titles over all of Genesis. |
| #664 | Test the Hebrew highlight with a maqaf | covered | tc-helps | `every_jonah_note_quote_finds_its_words_in_the_uhb_verse` | None beyond #765. |
| #769 | Group notes by figure, and fold groups | reference change | tc-check, tc-helps, B5a, B7, A3, A6a | `rail_groups_by_figure_in_first_note_order`; `group_counts_ignore_the_filter`; `fold_survives_a_restart`; `next_into_a_folded_group_unfolds_it`; `fold_all_keeps_the_active_group_open` | See DELTA §3.7 and §3.12.1. Update APP-DESIGN §3.8 and §4.9. |
| #762 | Note at 80ch, source texts side by side | reference change | B5b | `compare_card_columns_fit_the_width` (port of `compare-card.spec.ts`) | See DELTA §3.12.1. |
| #737 | A drag across draft words selects whole words | unit todo | B5b | `a_drag_selects_whole_words_and_mark_valid_saves_them` (port of the `checkKeyboard` and J4 drag cases) | Build `drag_range`. Next or Previous before marking drops the selection (tC4's Q2). |
| #726 | Previous and Next beside the marking buttons | unit todo | B5b, B6, tc-ui | port of `item-navigation.spec.ts` | Build ItemNavButton on the gallery-proven Button styling; test ids `check-prev` and `check-next`. |
| #764 | Progress bars look empty at low progress | unit todo | B5a | `check_progress_bar_is_visible_after_one_resolved_item` | On the check bars only: a visible fill once one check is resolved, and the count for screen readers. Declared difference; tell the owner. |
| #757 | Clear every selected word with one button | not applicable | B5b | — | Open tC4 enhancement, outside 4.0. Keep the selection as one local value. |
| #661 | A tW-only pin change does not count rows that lose their article | rebuild bug | tc-check; C8, B5a | `a_tw_only_change_that_drops_a_decided_article_is_counted` | See §1 row 11. |
| #594 | Mark valid on an invalidated check is swallowed | covered | tc-check | `deciding_again_judges_the_text_and_the_list_in_front_of_the_person` | If tC4 rules that To do clears the flag, change `DecisionEdit::apply`. |
| #595 | The language-change preview retries without limit | unit todo | C8 | `gateway_preview_appears_while_the_open_verse_keeps_changing`; `a_stale_confirm_reshows_the_dialog_with_new_counts` | Compute once from a snapshot; on PlanStale recompute once. |
| #518 | Polish the return to the last Check tool | unit todo | B5a, B3 | `resume_shows_no_clickable_picker`; `a_tool_chosen_during_resume_stays_open` | Part 3 (the late write) is covered by the prefs flush tests. |
| #628 | Full Markdown in helps | covered | B3 | `ta_article_with_a_table_renders_a_table` | Done by B3 (§3). Links stay plain text until tC4 decides. |
| #663 | Door43 releases missing books | covered | tc-resources | `coverage_follows_what_the_files_hold` | None. |
| #139 | Check % on Home tiles | not applicable | B2 if adopted | — | tC4 4.1.0. |
| #259 | Check for newer releases automatically | not applicable | C8 if adopted | — | Post-4.0. Pins never move on their own. |
| #703 | Lexicon popover | not applicable | B5b or B6 if adopted | — | tC4 4.1.0. |
| #702 | Text zoom sliders | not applicable | G1 if adopted | — | tC4 4.1.0. |
| #711 | Aquifer helps built on the translator's machine | owner question | tc-helps, tc-resources, tc-check | `a_project_with_a_tc4_aquifer_pin_opens_and_keeps_it` | Q33. |
| #712 | Record the Aquifer local-build pin | not applicable | tc-resources | `a_tc4_aquifer_local_build_pin_round_trips_unchanged` (optional) | Part of Q33. An unreadable pin is already kept as written. |
| #713 | Convert Aquifer helps to tC4's format | not applicable | — | — | Revisit with Q33. |
| #714 | Build and install Aquifer helps on the machine | not applicable | — | — | Revisit with Q33. |
| #716 | Pilot: Titus in Portuguese with Aquifer | not applicable | — | — | Revisit with Q33. |
| #717 | Move five languages to Aquifer helps | not applicable | — | — | Revisit with Q33. A project on a removed registry entry still opens on its pins. |
| #715 | Academy and Bible from another organization | not applicable | — | — | tC4 4.1.0. |
| #719 | Add source Bibles to a project | not applicable | — | — | tC4 4.1.0. |
| #720 | Aquifer AI-aligned Portuguese Bible | not applicable | — | — | tC4 4.1.0. The align area keeps it as Q34. |
| #718 | Language code for Aquifer Chinese | not applicable | — | — | tC4 data question. |
| #654 | Research Aquifer resources | not applicable | — | — | Done; moved to #711. |
| #662 | Non-unfoldingWord Aquifer helps | not applicable | — | — | The rebuild reads only unfoldingWord's Aquifer repositories. |
| #701 | Is Urdu (Devanagari) right to left? | rebuild bug | tc-resources; B3 | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | See §1 row 12. |
| #704 | BSOJ's Arabic Bible as the source text | covered | tc-resources | `a_literal_bible_with_another_name_fills_the_glt_pane` | None. |
| #650 | An Arabic gateway package end to end | covered | B3 | journey `arabic_helps_panes_are_rtl_in_an_english_interface` | Done by B3 (§3). |
| #648 | tW checking uses released TWL | covered | tc-resources, tc-check | see #643, #644 and #646 | The follow-up gap is #661. |
| #643 | Hide a tW item with no article in the gateway language | covered | tc-check | `a_decision_on_a_link_whose_article_left_is_kept_and_counted_as_to_check` | None. |
| #644 | Pin English TWL to en_twl v91 | covered | tc-resources | `the_v91_export_lists_59_book_tsvs` | None. |
| #646 | Pin gateway TWL to its own release | covered | tc-resources, tc-check; I | `an_old_one_repository_word_links_pin_is_refused` | I: a tC3 import pins the nearest TWL release (`tc3_import_pins_the_nearest_twl_release`). |
| #640 | Start after an upgrade when a bundled copy differs | covered | tc-resources | `c29_a_changed_receipt_makes_that_copy_be_checked_again_against_its_own_receipt` | None. |
| #528 | Each release in its own folder | covered | tc-resources | `c29_an_upgraded_bundle_copies_only_its_new_install` | None. |
| #504 | Bundle the latest English releases | covered | tc-resources | `fetched_lock.rs` (8 tests) | None. |
| #580 | Invalid stays Invalid after a language change and back | covered | tc-check | `a_change_back_to_the_resource_a_decision_was_made_against_revives_it` | A decision invalidated only by the change becomes valid again; tC4 brings it back as To do (STORAGE §7.7 difference). |
| #579 | Name a gateway language the same way everywhere | unit todo | C8 (B9a done) | `gateway_change_dialog_names_both_languages_by_autonym` | Use the autonym, never the code, including both buttons. B9a: Project settings and Get source texts name a package by its autonym (`settings_from_home_show_the_sample_as_tc4_does`, journey `source-texts`). |
| #621 | Superscript verse numbers in the simplified tab | covered | B3 | `simplified_tab_verse_numbers_are_superscript` | Done by B3 (§3). |
| #619 | Quotes and bold in helps | unit todo | B5b (B3 done) | `article_shows_no_raw_quote_or_bold_marks` | As in §3, with #628's differences. |
| #601 | Sticky article header | covered | B3 | `article_close_button_visible_after_scroll` | Done by B3 (§3). |
| #602 | Resize the helps panel | unit todo | B4a (B3 done) | `helps_panel_width_follows_the_divider_drag` | As in §3. |
| #606 | Third-party licenses in About, offline | unit todo | A5, G1 | `about_lists_aquifer_attribution_offline` | Also list each installed Aquifer resource's attribution (CC BY-SA). B9a: nothing in the app installs an Aquifer resource yet (APP-DESIGN §4.6), so the attribution goes with the unit that first installs one. |
| #532 | Show a verse's new draft on return to the check | unit todo | B5b, C4 | `check_shows_new_draft_after_switching_back` | Read the verse again on return or when the book text arrives. |
| #42 | Keyboard pass on the Check pages | unit todo | B5a, B5b | port of `check-keyboard.spec.ts` and `checkKeyboard.test.tsx` | Include the focus-rule table and #769's heading stops. |
| #513 | Resuming into tN stalls | unit todo | B5a | `resume_and_tab_click_open_the_tool_once` | One open session that settles. |
| #501 | Opening Check changes `resources.json` | unit todo | B5a, C8 | `open_check_preflight_offer_close_leaves_pins_unchanged` | Opening Check may write only a coverage backfill, on pins that record no books. |
| #722 | Verse button missing after the UGNT upgrade | unit todo | C8, B6 | J12 `ugnt_upgrade_marks_1_1_invalid_in_align` | The pin moves only after the download. |
| #766 | A project with no pins gets the creation pins | unit todo | I, F1, E1b, C1a | `open_writes_creation_pins_only_when_none_exist` | Build now, as a declared difference, until the reference includes #766. |
| #763 | Help shows rc.1 pages | reference change | A5 | help case b `help_home_shows_no_rc1_build_text_offline` | Vendor tc-website dbb7cd0 when the reference moves. |
| #641 | The checking language row never finishes loading | covered | B9a | `a_reply_for_another_projects_settings_changes_nothing`; `settings_from_home_show_the_sample_as_tc4_does` | One server read, `settings/viewFromHome`; a reply lands only on its own project's form. |
| #583 | Three journey tests fail on main | not applicable | — | — | tC4 test fixes. |
| #615 | Understand journey after Resume | not applicable | — | — | tC4 journey order. |
| #700 | Cache the bundle in packaging CI | not applicable | — | — | tC4 CI. |
| #655 | Plan the 4.1 ideas | not applicable | — | — | tC4 planning. |
| #652 | Basic checks | not applicable | — | — | tC4 4.1.0. |
| #653 | AI-assisted checking | not applicable | — | — | tC4 4.1.0. |
| #651 | BT Servant | not applicable | — | — | tC4 4.2.0. |

## 5. Sync, storage and import

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #801 | After a sync, show the plain refusal and let the user go Home | unit todo | E1a, A4b, X | `stale_scope_save_keeps_text_and_home_leaves_in_one_click`; X `a_save_during_every_sync_step_is_kept_and_pushed_next` | The rebuild has no "old store": a sync joins the open session. E1a ports J1b's one-window route. X holds each sync step, OBS frames included. See §6 for refusals that can never succeed. |
| #761 | Sync story projects with teammates | unit todo | X, E1a, E1b | `obs_frames_from_two_devices_merge`; `obs_same_frame_two_ways_is_a_frame_conflict`; `obs_front_matter_travels_as_carried_file` | The owner ruled that stories sync (Q2). Make the front-matter test fail if front and back matter are stored any other way than as carried files. |
| #748 | Door43 journey uses committed journals | not applicable | E1b | — | tC4 rig. Seed the fake from a committed project. |
| #747 | Keep backup files out of syncs | unit todo | S2, X | `a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept` (add `TIT.usfm.bak`, a `~` file and a `.tmp`), plus a checkpoint tree assertion | Today's test passes even with the `.bak` rule deleted. `scan.rs` keeps `.bak` out, not `checkpoint.rs`. X: a `.bak` from Door43 stays a foreign entry. |
| #746 | Test that startup recovery runs before projects open | covered | tc-store (S1); C1a | `an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` | C1a moves tc-app onto the store and creates, imports and joins only through `Store`. |
| #724 | Sync button follow-ups | unit todo | E1a, B9b, E1b, A1, B3, B11 | `failed_open_keeps_the_sync_toast_links` (B3, B11); `io_reason_renders_without_the_path` | Item 6 is fixed by B10 under Q36: one fixed sentence, no reason (`io_reason_renders_without_the_path`). The toast's Review and Download leave it in place; the open each starts (B11's review, B3's open) closes it once open, so a failed open keeps its links. The team-access line shows for every project, OBS included. |
| #710 | Record Door43's 403 reply | rebuild bug | tc-door43-fake, tc-door43; X | `a_push_without_write_access_is_no_write_access_and_makes_no_branch` | See §1 row 14; fixed by its merge later on 2026-10-19. |
| #707 | An import during start-up recovery rolled back | covered | tc-store (S1); I | `an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` | I builds every import through `Store::staging` and `Store::place`. |
| #705 | Audio between translators | unit todo | X, E2 | `a_teammates_audio_file_arrives_byte_for_byte`; `loss_carried_file_two_versions_both_kept` | Carried files; two versions both kept; a missing blob never refuses the commit. |
| #725 | A Door43 copy collision keeps the project | covered | tc-store (S1); X, E1b | `a_collision_at_the_rename_keeps_the_destination_byte_for_byte` | Join through staging and place; check the name case-insensitively before the internet question. |
| #723 | Discard an empty sync recovery copy | not applicable | X | X `an_empty_remote_tree_removes_nothing_local` | No scratch copy or apply marker in the rebuild. |
| #681 | The three-translator journey | unit todo | X, E1a | library J26 `three_devices_converge_with_byte_identical_derived_files` | Plus the check-conflict step after C7b. |
| #680 | Show each teammate's Door43 name | unit todo | X, A8, B11 | `username_written_once_after_first_successful_fetch`; `no_email_or_token_in_any_change` | One label function for toast and review: the server names each person by the fold's label (`tc_app::wire::scope::person_label`) and the UI writes it with `tc_ui::app::sync::person_label` (B10, `one_label_for_a_person`), which B11's review uses too. |
| #679 | Review conflicting verses one by one | unit todo | B11, C7a | J18 `keep_this_version_resolves_with_review_tokens` | The Resolve command exists and is tested. |
| #678 | The Sync with Door43 button and result | unit todo | E1a | `no_door43_request_at_launch_or_60_s_idle` | B10 ported the toast and the button's question and sign-in on scripted outcomes, with a closed-set test of every refusal (`every_sync_refusal_shows_its_own_sentence_never_door43s_words_or_a_path`; journey `sync-button`, which also finds no Door43 request at launch or after 5 s idle). E1a runs the real sync. |
| #677 | Open a project from Door43 on another computer | unit todo | X, E1b | `open_from_door43_equals_main`; `open_name_in_use_refused_before_request`; `open_private_repo_signs_in_and_retries` | Salvage a damaged pack (Q4); write creation pins when none (#766). |
| #676 | Sync shell with crash recovery | unit todo | X | `push_is_never_forced`; `non_fast_forward_retries_six_times_within_30_s_then_busy`; `crash_at_every_sync_step_converges_on_retry` | Six retries, not three (DELTA-10-16 Q11). |
| #675 | Sync core: combine and rebuild | unit todo | X | `door43_ingest_deterministic`; `a_damaged_remote_pack_is_salvaged_and_listed` | The fold half is built and tested. |
| #674 | Sync rules and the three-translator fixture | covered | tc-journal; X | `fold_converges_under_any_union`; `conservation` | X ports tC4's team-sync fixture cases as one library scenario. |
| #673 | Spike on Door43 clone and push | not applicable | — | — | Pankosmia routes the rebuild does not use. |
| #766 | Pin the source texts when an import has no pins | reference change | C8, F1, I, E1b, X | `open_writes_creation_pins_only_when_none_exist` | The check verifier says build it now (§4). |
| #549 | Show a story project's own pictures | rebuild bug | tc-obs; B7, I | `a_project_picture_without_a_role_is_the_project_rung` | See §1 row 15. |
| #161 | Two device ids under one name after a computer change | unit todo | S2, X | `a_new_device_copies_the_profile_and_both_group_as_mine`; `copied_username_is_replaced_by_the_first_signed_in_fetch` | The profile copy is untested. A hand-copied folder copies the first person's name; tell the owner. |
| #160 | Record four sync risks | not applicable | P | — | tC4 documentation. P owns the scale risk. |
| #78 | See the history of my changes | unit todo | B11, C7a | `verse_history_lists_every_version_with_person_and_time` | Restore sends `Command::Restore` with the shown token. |
| #582 | Create a project in a language Door43 lists | covered | tc-store (S1); C6, B2 | `create_refuses_a_language_tag_the_burrito_cannot_hold` | See §1 row 16 and Q35. |
| #496 | Hide "Stay signed in" when it cannot work | unit todo | A8 | `no_keychain_hides_stay_signed_in` | Show "Your sign-in lasts for one app session." Declared difference; amend APP-DESIGN §4.7. |
| #27 | Merge concurrent edits | covered | tc-journal, tc-store | `merge_bookmark_and_status`; `a_stale_token_makes_a_conflict_never_a_revert` | None. |
| #24 | Team sync round 1 epic | unit todo | X, E1a | one named test per tC4 guarantee in `docs/evidence/sync-<date>.md` | Put the tC4 G-id in each test or its comment. |
| #802 | Refuse a book save over newer text | covered | tc-store, A4b | `loss_f5_stale_buffer_never_reverts` | A4b adds a two-tab Playwright case. |
| #791 | Tell a translator about a teammate's alignment | reference change | B10, E1a | `two_different_alignments_of_one_verse_are_listed_once` | Port D108's rows when tC4 ships them, worded as "a verse with two alignments to review". Until then the toast counts such verses in one note (B10; PLAN, a declared difference). |
| #767 | Import creates an empty book | covered | tc-store; C6 | `adding_a_book_from_usfm_is_one_batch_whole_or_absent` | See §1 row 17. The rebuild never refuses a book for its size. |
| 5fefba8 (D104) | tC4 journal pieces up to 8 MiB | reference change | I | `tc4_project_with_a_6_8_mib_book_add_segment_imports_whole` | Port the validator with 8 MiB. |
| #553 | Keep an aligned USFM file's alignments | unit todo | I (C6 done) | J9 `aligned_titus_import_has_no_inline_zaln_and_46_records` | One function for both paths; coordinate with #794. |
| #484 | Add a book from a USFM file | covered | C6 | see the first #484 row | Built on the #767 command. |
| #499 | Use the typed Bible name | unit todo | F1, I | `typed_import_name_is_the_project_name` | Leave review-edited fields out of the P9 comparison. |
| #502 | Two USFM files never reach review | unit todo | F1 | J9 `a_two_file_drop_is_one_new_project` | Each book in one Import batch. |
| #492 | Guess a language code from the name | covered | B2, C6 | `language_suggestions_rank_as_tc4_offline` | Ship `langnames.json`; never let a suggestion become "Undetermined" (#582). |
| #386 | Keep regional language tags | covered | S1, C6 | `create_keeps_regional_language_tags` (tc-store and tc-app); journey `create` (`language-tags`) | es-419, pt-BR, x-ug and the adjusted `acm-x-lebarabi-c` are kept. |
| #529 | Remove unused checkingLanguage writes | covered | tc-journal; I | I `imported_obsolete_checking_language_is_kept_and_ignored` | Nothing writes it. |
| #530 | Upload details in a dialog | not applicable | B2 | — | No project keeps "Upload changes" (Q2). |
| #514 | Account menu and asking before internet use | unit todo | A5, A8 | port of tC4's `internet-consent` menu cases | Three menu states; Escape returns focus. |
| #559 | One internet on/off state per app session | covered | tc-door43; A8 | `every_call_is_refused_before_anything_is_sent_while_the_switch_is_off` | A8 builds the dialog. |
| #536 | Flaky "nothing auto-committed" journey | not applicable | D1 | D1 `a_save_makes_no_commit` | tC4 journey race. |
| #501 | Opening Check changes `resources.json` | covered | tc-journal; C8 | `projection_ignores_installed_resources` | C8 ports J12's "pins never move without the accept". |
| #506 | QA server label on the card | unit todo | B2 | `debug_card_names_qa_server` | No top-bar label. |
| #120 | Development builds use QA for account and write calls | unit todo | A8, B9b (B9a done) | `a_development_build_downloads_resources_from_production` | Only account and write calls use the QA server. B9a: Get source texts reads and downloads from production when the account server is QA (`tc_app::resources::resources_server`). |
| #708 | An ops-log record stays open | covered | tc-store | same test as #707 | Duplicate of #707. |
| #372 | First share to Door43 (J11) | unit todo | E1b, A8, B9b | J11 port | Never force a push. |
| #373 | Publish and import journeys | unit todo | D2, F1, I | J7, J9 and J23 ports | Validate every export and import against SB 1.0. |
| #25 | Review competing drafts side by side | unit todo | B11, C7a, C7b | — | C7b starts after Q-9 is signed off. |
| #46 | A rollback plan for a bad release | unit todo | V, G1 | `older_build_starts_on_newer_app_data` | The downgrade check also covers app data. |
| #23 | Journal store epic | not applicable | — | — | Superseded. |
| #26 | Staged rollout epic | not applicable | — | — | Superseded. |
| #157 | Scenario files for journal behavior | not applicable | — | — | Not planned. |
| #158 | A repository port with adapters | not applicable | — | — | Not planned. |
| #159 | `docs/SYSTEM.md` | not applicable | — | — | Not planned. |

## 6. Shell, startup, languages and help

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #743 | A window from the click until the main window opens | unit todo | G1; APP-DESIGN §5.3, A9 D10 | `window_is_painted_within_1_s_of_the_click`; `closing_the_window_while_preparing_leaves_no_tc4_process` | Create the window at Electron `ready`; §5.3 picks the page, not whether a window shows. Fix the §5.3 and D10 wording now. |
| #741 | Help page wordmark | unit todo | A5 | help sha-list check | Vendor the help pages byte for byte; vendor again when tc-website ships the fix. |
| #731 | Is the Report a problem attachment translated? | unit todo | A5 | `feedback_attachment_is_english_and_explained_in_the_app_language`; `platform_line_names_the_architecture` | Compose it in English, as tC4 does. An optional translated line explaining why is a declared difference. The platform line comes from Rust's OS and architecture constants. |
| #751 | Duplicate work in Windows install and start | unit todo | G1, G2 | G2 budget row `first_launch_under_60_s` (five runs) | Install from archives; measure on the reference laptop with Defender. |
| #740 | First launch in under a minute | unit todo | G1, G2 | keep `repeat_launch_reads_no_resource_content_file`; add the Windows measurement | The code half is covered. The Windows 60 s goal is not shown yet; merge with #751. |
| #730 | Correct 76 Arabic strings | covered | tc-ui | `every_vendored_file_has_the_bytes_tc4_has` | B5a uses `byFigure` after the next vendoring. |
| #729 | The help panel blinks white | unit todo | A5 | tC4 help cases g, h and i | Port `swapPages`; swap only HTML pages under `help/`. |
| #728 | "Draft section" looks like a button | unit todo | B4a | DOM parity of `translate-tit1` | As in §3, with an RTL case. |
| #763 | The rc.2 help pages | reference change | A5, A6a | help case b | Q31 for the banner and install sentences. Also triage #476 (§7). |
| #686 | Bundle Noto Sans Arabic | covered | tc-ui; A5 | port tC4 app-language case j, with a check that the font file answers 200 | None in tc-ui. |
| #657 | Mirror the interface for a right-to-left language | unit todo | A5, B3, B4a | tC4 app-language cases h and i | Window-control room stays on the physical side. |
| #642 | Remove the Linux native title bar | unit todo | G1 | `linux_window_keeps_native_frame` | Match 4.0. |
| #613 | Bundle a Devanagari interface font | unit todo | A5 | `hindi_label_renders_in_the_bundled_face_offline` | Declared difference; note that it also changes Hindi's look on Windows and macOS; add the OFL text to the notices. |
| #606 | Third-party licenses in About | unit todo | G1 (A5 if tC4 ships it) | release check: every crate in `Cargo.lock` has a notice | Generate notices from `Cargo.lock`, fonts, vendored tC4 assets, Electronite and resources. Include the crates' own `NOTICE` files: `crates/tc-align/NOTICE`, and `crates/tc-door43/NOTICE` (proxy-watch's MIT notice, Windows builds). |
| #569 | Keyboard shortcuts for Understand, Translate and Check | unit todo | A5 | the Switcher key port | Match 4.0 (no shortcuts). Port tC4's if it adds them. |
| #525 | Choose the app language automatically | covered | tc-app | `hello_answers_real_state` | C1a adds a relaunch test that the saved language survives. |
| #523 | Collect translation corrections in Weblate | not applicable | tc-ui | — | They arrive with the next vendoring. |
| #278 | Show the build in the app | unit todo | A5, G1 | `about_commit_matches_the_release_record` | G1 sets `TC4_BUILD_COMMIT` for server and wasm. |
| #167 | Which interface languages | covered | tc-ui | `the_registry_names_the_ten_decided_locales_each_with_a_catalog` | None. |
| #37 | Unused catalog keys | not applicable | tc-ui | — | Optional: check that each rebuild key is used. |
| #29 | Right-to-left and complex scripts end to end | unit todo | B4a, B5b, B6, G1 | Arabic IME Playwright case; Graphite probe | Use logical properties where tC4's physical ones break right to left, and record it. |
| #701 | Urdu (Devanagari) direction | rebuild bug | tc-resources | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | See §1 row 12. |
| #548 | Dropdown follow-ups | covered | tc-ui | `a_book_matches_by_name_or_code_prefix`; gallery `select-search` | None. |
| #552 | Move the dropdown onto a library | rebuild bug | tc-ui | ds-gallery `Select/disabled-while-open` | See §1 row 13. |
| #624 | Link help from translationcore.com | not applicable | — | — | Website only. |
| #611 | macOS manifest hashes before codesign | not applicable | G1 | — | If the release writes binary hashes, compute them after signing. |
| #700 | Cache the bundle in CI | not applicable | — | — | tC4 CI. |
| #546 | Release v4.0.0 | not applicable | — | — | Recapture and sweep again when v4.0.0 is tagged. |
| #42 | Keyboard pass on the Check pages | unit todo | B5a, B5b | port of `check-keyboard.spec.ts` | As in §4. |
| #568 | A keyboard-only user cannot align | unit todo | B6 | `align_by_keyboard_only` | As in §2. |
| #599 | Keep the mode switch centred | unit todo | A5 | `switcher_stays_centred_while_the_save_indicator_changes` | Within 1 px, in en and ar. |
| #505 | No Project settings in the top bar | unit todo | A5 | DOM parity of the in-project screens | Settings open from the Home card (B2). |
| #506 | QA label on the card, not the top bar | unit todo | B2, A5 | `debug_card_names_qa_server` | As in §5. |
| #120 | Development builds use QA | covered | tc4 | `the_door43_server_defaults_by_build` | A8 and X test that reads still go to production under a QA config. |
| #514 | Account menu | unit todo | A5, A8 | account-menu keyboard scenario | Rows and order as tC4 `AccountMenu.jsx` at d310bf3. |
| #559 | One internet switch per session | unit todo | A8, A5 | tC4 `internet-consent` cases a–j | The server half is covered. |
| #519 | Help and guides, offline first | unit todo | A5 | tC4 help cases a, b, e and f | Same-origin frame; links to other sites go through the internet question. |
| #520 | About: version and license, offline | unit todo | A5 | the About cases of `internet-consent` | Ask the owner for the rebuild's copyright line and LICENSE notice. |
| #378 | Ask for help from a refusal | unit todo | A5, G1 | `feedback_draft_is_never_written_to_disk` | The help-desk token stays in the shell's main process. |
| #521 | Report a problem in the account menu | unit todo | A5 | the Report a problem case of `internet-consent` | Kept-report rules as tC4. |
| #522 | Choose the app language | unit todo | A5 | tC4 app-language cases a–f | Preview, Apply, Cancel; a typed verse survives the switch. |
| #622 | Add five interface languages | covered | tc-ui | `the_registry_names_the_ten_decided_locales_each_with_a_catalog` | None. |
| #649 | Add Arabic | covered | tc-ui | catalog and tracking tests | A5 ports app-language b2 and g. |
| #612 | Hindi labels keep letter spacing | covered | tc-ui | `vendored_assets` | B11 uses a tracking token where tC4 has a literal value. |
| #614 | Four English literals in design-system props | unit todo | screen units | tC4 `ds-labels` a–d | As in §3. |
| #632 | Help links stay in the panel on modified clicks | unit todo | A5, G1 | tC4 help case f | G1's `openExternal` refuses same-origin addresses. |
| #634 | The Message box has no accessible name | unit todo | A5 | tC4 `feedback.spec` case 3 | Give the TextArea an id. |
| #639 | Remove the Windows native title bar | unit todo | G1 | `windows_window_has_no_native_bar_and_keeps_zoom_and_reload` | Keep an application menu for reload and zoom keys. |
| #214 | Hide the macOS window frame | unit todo | G1 | macOS smoke: traffic lights centred in the header | |
| #206 | The server does not stop on quit (macOS) | covered | tc4; G1 | `closed_stdin_quits_within_5_s` and the other lifecycle tests | Fix the SIGTERM race that makes `sigterm_quits_like_the_quit_message` flaky. |
| #284 | Ship or verify the Windows C runtime | unit todo | G1 | `tc4_exe_imports_no_vcruntime` | Build with `+crt-static`, or ship the DLLs next to the program. |
| #456 | Sign the Windows installer and app | unit todo | G1 | `Get-AuthenticodeSignature` reports Valid for each file | Sign before hashing. |
| #44 | Installers for Windows, macOS and Linux | unit todo | G1 | clean-machine smoke per installer | Uninstall keeps projects and app data. |
| #554 | Third-party notices and the GPL text | unit todo | G1 | as #606 | Ship COPYING beside the notices. |
| #640 | Start after an upgrade when a copy differs | covered | tc-resources | as §4 | None. |
| #528 | Each release in its own folder | covered | tc-resources | as §4 | None. |
| #707 | Import during start-up recovery | covered | tc-store | as §5 | None. |
| #746 | Startup recovery before opens | covered | tc-store | `the_start_cleanup_reports_a_join_it_removed_and_hands_out_no_staging_folder_before_it_ends` | None. |
| #708 | Ops-log record stays open | not applicable | — | — | tC4 machinery; duplicate of #707. |
| #801 | Let the user go Home after a refused save | unit todo | A4b | `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` | Before A4b merges: a refusal that can never succeed lets Home proceed and keeps the text as a draft (tC4 drops it; declared difference). |
| #702 | Text zoom | unit todo | G1 | the zoom part of the #639 test | Keep Ctrl or Cmd with +, - and 0 working. |
| #46 | Rollback plan | unit todo | V, G1 | `older_build_starts_on_newer_app_data` | As in §5. |
| #658 | Arabic epic | not applicable | — | — | Children triaged. |
| #543 | Account menu epic | not applicable | — | — | Child #476 is not triaged (§7). |
| #59 | Packaged app epic | not applicable | — | — | Children #3, #4, #32, #43 and #45 are not triaged (§7). |
| #7 | Desktop client list | not applicable | — | — | Pankosmia. |
| #71 | Core-client set | not applicable | — | — | Pankosmia. |
| #461 | Pankosmia server pin | not applicable | — | — | Pankosmia. |
| #335 | Packaging smoke while another server runs | not applicable | — | — | tc4 binds a free loopback port. |
| #336 | One product.json in the zip | not applicable | — | — | Pankosmia. |
| #371 | Release v4.0.0-rc.1 | not applicable | — | — | Process. |
| #58 | Pilot program | not applicable | — | — | Process. |

## 7. Not triaged yet

The shell verifier found these children of epics #543 and #59 that no area covered. All are
closed in tC4, before d310bf3. Each needs a row in the next sweep.

| Issue | Title | Likely unit |
|---|---|---|
| #476 | Publish the v4.0.0 help pages on translationcore.com and pin them in tC4 | A5 (with #763) |
| #4 | Allow only one running copy of tC4 per machine | G1 (single instance, ruling of 2026-10-18) |
| #43 | Prove a full run with the network off | G1 (offline smoke) |
| #45 | Add a post-install smoke test | G1 |
| #3 | Ship the app's fonts locally | tc-ui, P8 |
| #32 | Confirm the desktop wrapper is Electronite and prove Graphite rendering | G1 (Graphite probe), P8 |

## 8. Notes by unit

The unit-todo rows above, grouped by the unit that must act. Reference changes and covered
rows also carry notes for builders; see their rows.

Follow-ups the fix units left (2026-10-19):

- **T (storage):** the fold's `invalid` flag for an alignment checks only the text hash, so the
  alignment files can call a record valid that the rail calls Invalid: use
  `tc_align::needs_review` for both. tc-store's two reflow call sites should call
  `tc_align::reflow_edit(value, before, after)`, which follows each link by position (Q30 a).
- **B5a, C4 (Check):** the Check screen still picks its selectable target words from the shown
  text; take them from `tc_usfm::target_words`, as the aligner and the export do (FIX-text), so an
  unknown marker's `|attributes` never show as words.
- **C6 (done on the server; the button waits for the read-only banner, which no unit has
  drawn):** the owner's "Copy to this computer" answer (APP-DESIGN, Owner's answers, 2026-10-19)
  is built: for a project in a projects folder on a FAT32 or exFAT drive or a synced folder,
  `session/copy` copies it with its unsaved texts into the app's local projects folder (checked
  safe; no folder picker), and Home lists it (APP-DESIGN C6 D9). `CreateRefusal::CopyUnsafe`
  stays only where the local folder is unsafe too. The banner's button calls `session/copy` with
  the open project's scope, then `tc_ui::app::offer::copy` and the open of the copy.
- **D2 (export):** the export and the aligner share `tc_usfm::target_words` and
  `tc_align::needs_review`; keep it that way.

- **A1 (tc-api):** never pass an absolute path into a sync error's `{reason}` (#724, Q36).
- **A4b:** a refusal that can never succeed lets Home proceed and keeps the text as a draft;
  a `StaleScope` save keeps its text and leaves Home in one click (#801).
- **A5:** vendor help pages byte for byte (#741); English report attachment and platform
  line (#731); help page swap (#729); mirrored chrome (#657); Devanagari font (#613); no mode
  shortcuts (#569); About shows the build (#278); switch stays centred (#599); no Project
  settings in the top bar (#505); account menu (#514); internet dialog with A8 (#559); help
  in a same-origin frame (#519); About and License (#520); report draft never on disk
  (#378); Report a problem row (#521); app-language preview (#522); design-system labels
  (#614); help links stay in the panel (#632); Message box id (#634); Aquifer attribution in
  About (#606).
- **A8:** hide "Stay signed in" without a keychain (#496); account menu (#514); the internet
  question (#559); the sign-in name notice (#680); account and write calls only on the QA
  server in debug builds (#120); J11 sign-in (#372).
- **B2:** one drafted-percentage function (#572); language search offline (#492); card names
  the QA server (#506).
- **B3:** cross-book statement (#121); verse-only statement (#118); book percentage and
  tooltip (#572, #603); new project lands in Understand (#507); helps rendering (#619, #621,
  #628); divider and sticky header (#602, #234, #601); helps direction from the gateway
  (#650, #657); resume shows no clickable picker (#518, with B5a); the sync toast's Download
  opens the project (B9a its Source texts) and closes the toast once open, and a failed open
  keeps it (#724).
- **B4a:** the verse box shows all of `words` (#786); the source pane follows the verse
  mapping (#131); verse-only statement (#118); "Draft section" pill (#728); reuse the divider
  (#602); right-to-left and IME cases (#29, #657).
- **B4b:** emptied section save (#600); pin click-then-click (#598); state hints (#604);
  blank lines in a joined span (#575); VerseMarker label (#614); section-save tests (#786).
- **B5a:** visible progress bars (#764); resume (#518, #513); keyboard pass (#42); opening
  Check never moves pins (#501).
- **B5b:** drag selection (#737); ItemNavButton (#726); new draft on return (#532); keyboard
  pass (#42); article panel (#619); Drawer label (#614); right-to-left case (#29).
- **B6:** NFC card labels (#768); no Clear, and Clear drops suggestions (#758); keyboard
  placement (#568); x-ref per verse (#409); marking row (#726); invalid after a UGNT upgrade
  (#722); the inline-markup guard (#553); right-to-left case (#29).
- **B7:** a story project opens at story 1 (#507).
- **B9a:** autonyms (#579); downloads always from production (#120); one settings read
  (#641). The Aquifer attribution (#606) and Q34's Portuguese Bible (#720) wait for a unit that
  installs an Aquifer resource (APP-DESIGN §4.6).
- **B9b:** QA only for account and write calls (#120); J11 steps up to the check step (#372).
  The team-access line for every project (#724) is on the share's done step, which is E1b's
  (APP-DESIGN B9b D4).
- **B10:** built: the toast (#678), one label function (#680), Q36's fixed sentence (#724 item
  6), and verses with two alignments counted on the toast (#791). Left: D108's per-verse rows
  when tC4 ships them (#791), and the closing of the toast once the open its Review or Download
  starts is open (B3, B11; #724).
- **B11:** "2 versions" in Align (#791); labels (#680, `tc_ui::app::sync::person_label`); the
  Text tab (#679); History (#78); review tabs (#25); the sync toast's Review closes the toast once
  the review is open, and a failed open keeps it (#724).
- **C1a:** creation pins at first open (#766).
- **C2:** a save replaces `words` only (#786); Resume banner after a span save (#576).
- **C4:** re-read the draft on return (#532).
- **C5:** compare stored source words with resources in NFC (#768).
- **C6 (done):** one-batch add from USFM, refuse only what a record cannot hold (#767); split
  aligned files (#553); singular keys (#577); the USFM method step (#484); dialog polish
  (#564); language suggestions (#492); regional tags test (#386); Select and SearchField
  labels (#614). Left for others: the list of verses kept as written (#553, F1), and the OBS
  front and back matter and licence, which are carried files and need S2's blob store.
- **C7a:** Keep this version (#679); Restore (#78); review choices (#25). **C7b:** after
  Q-9 (#25).
- **C8:** preview computed once (#595); autonyms in the dialog (#579); pins never move
  without the accept (#501); J12 UGNT case (#722).
- **D2:** export rules for verses that need review or hold markup (#796); x-ref (#409); kept
  verses export as stored (#553); J7 and J23 (#373).
- **E1a:** J1b one-window route (#801); story cards sync (#761); J26 through the UI (#681);
  the button flow (#678); a failed branch lookup is a failed sync (#724); one test per
  guarantee (#24); rows for verses with two alignments (#791).
- **E1b:** creation pins at first open (#766); stories (#761); J17 (#677); the done step
  with the team-access line for every project, OBS included (#724); J11 (#372).
- **E2:** audio through an exchange file (#705).
- **F1:** singular keys (#577); typed name (#499); two-file drop (#502); split aligned files
  (#553); creation pins (#766); J9 (#373).
- **G1:** window within 1 s (#743); Windows first-launch measurement (#751, #740); Linux
  frame (#642); notices (#606, #554); build commit (#278); Windows and macOS frames (#639,
  #214); zoom keys (#702); C runtime (#284); signing (#456); installers (#44); help-desk
  token in the main process (#378); `openExternal` refuses same-origin (#632); release notes
  for going back (#46).
- **G2:** first-launch budget row (#751, #740); `ar` screens (#29).
- **I:** creation pins in the import batch (#766); 8 MiB tC4 pieces (D104); NFC comparisons
  (#768); split aligned files, tC4 inline `\zaln` included (#553); keep x-ref (#409);
  review-edited fields out of P9 (#499); J9 (#373).
- **S1 and S2 (storage team):** one-batch add-a-book command (#767); `.bak` test (#747);
  profile copy test (#161); regional tags test (#386).
- **V:** the downgrade check covers app data (#46).
- **X:** saves during every sync step (#801); story sync (#761); `.bak` from Door43 stays
  foreign (#747); carried audio (#705); library J26 (#681); username written once (#680);
  J17 (#677); sync guarantees (#676); intake (#675); a copied username is provisional
  (#161); one test per guarantee (#24).
- **tc-journal:** commit the unknown-parent test (#179).
- **docs:** rewrite APP-DESIGN Q-20 (#786); fix §5.3 and A9 D10 (#743).
