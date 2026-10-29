# tC4 issues tested against the rebuild

Swept: tC4 main 24188928bff9c3fc9c7a8f143fb80d645d66f1d1, release/4.0 d25b7871904228a342c0d80280136d8636833653, issues updated to 2026-10-23.

- Date: 2026-10-23. Companion to `TC4-DELTA-2026-10-23.md` (tC4 `main` 9ad4d5c to 24188928,
  `release/4.0` to d25b7871, issues updated since 2026-10-19) and `TC4-DELTA-2026-10-19.md`
  (d310bf3 to 9ad4d5c and the owner's decisions Q30 to Q36). The next sweep starts from the
  "Swept" line above.
- Scope: five areas (align; USFM, Translate and helps; check and resources; sync, storage
  and import; shell, startup, languages and help). Each area lists the issues named for it
  and every other tC4 issue in that area opened or updated since 2026-10-01 (sweep of 2026-10-19) or since 2026-10-19 (sweep of 2026-10-23). An issue that
  touches two areas appears in both, with each area's verdict.
- Method: each area was triaged, then checked by a verifier who reran the claims. Where the
  verifier overturned a verdict, this file uses the verifier's status, evidence and action.
  Claims were proven by running code in detached worktrees of `claude/focused-ride-5uzj7p` at
  6ba7514, which were removed afterwards. Nothing was written to either checkout.
- Latest tC4 state, checked on 2026-10-23: `main` is 24188928 (PR #838); `release/4.0` is
  d25b7871 (PR #841) and holds the tag v4.0.0-rc.2 (a5ce0307); there is no v4.0.0 tag. The
  newest issue is #840. Pull requests #785, #799 and #800 have merged. The 2026-10-23 rows
  were classified by reading code and checking each test name with `git grep`; the Align and
  USFM tests they name were run and pass (`tracker:roadmap/evidence/2026-10-23-sweep/`).
- Test names are the names the rebuild would use. A name in backticks with no note exists
  and passes today; "to add" or a unit name means it must be written.

## Status key

| Status | Meaning |
|---|---|
| rebuild bug | The rebuild's code or design is wrong today. Reproduced by running code, unless the row says it is a design defect. |
| unit todo | A note for a unit that is not built yet, so the builder gets it right the first time. |
| reference change | tC4 changed after d310bf3. Match it when the reference moves. |
| covered | The rebuild already does what the issue asks, and a test that can fail proves it. |
| owner question | Needs the owner's decision. See §5 of `TC4-DELTA-2026-10-19.md` or `TC4-DELTA-2026-10-23.md`. |
| not applicable | tC4 process, tC4-only machinery, or outside tC4 4.0's scope. |

## Counts

| Area | rebuild bug | unit todo | reference change | covered | owner question | not applicable | Rows |
|---|---|---|---|---|---|---|---|
| Align | 1 | 1 | 1 | 33 | 0 | 1 | 37 |
| USFM, Translate and helps | 0 | 3 | 1 | 41 | 0 | 9 | 54 |
| Check and resources | 0 | 2 | 2 | 37 | 1 | 22 | 64 |
| Sync, storage and import | 0 | 14 | 3 | 33 | 2 | 13 | 65 |
| Shell, startup, languages and help | 0 | 6 | 1 | 49 | 0 | 20 | 76 |
| All area rows | 1 | 26 | 8 | 193 | 3 | 65 | 296 |

Counted from the rows on 2026-10-26, after FIX-R (step 4 reconciled #569 and #505), the 2026-10-23 sweep, T, G1, I, REF1, G4, D2 and F1. A row counts once
per area table. All area rows: 296; 1 rebuild bug, 26 unit todo, 8 reference
change, 193 covered, 3 owner question and 65 not applicable. The 2026-10-23 sweep
added 24 area rows (271 before) and changed 31, and its commit range holds 59 items
(`TC4-DELTA-2026-10-23.md`; 25 covered, 7 unit todo, 8 reference change, 19 not applicable).
Earlier: the 2026-10-19 range added 22 items (`TC4-DELTA-2026-10-19.md` §2). §1 lists the
rebuild's bugs: rows 2 to 10, 14 and 15 are fixed, and FIX-R fixed rows 19 to 23 and Align's
half of row 18.

## 1. Bugs the rebuild has today

Each row was reproduced by running code, except #767, a design defect in a planned path.
The first four come from tC4's commit range; the rest come from the issue triages.

| # | Issue | Failing scenario | Crate | Regression test | Fix |
|---|---|---|---|---|---|
| 1 | #773 | GEN 33:5 note mix3 is titled "his eyes … the women … whom God has graciously given to your servant". gyfv, r2u9 and h9hm are also wrong; only zczu is right. 38 of the 2,224 Genesis Notes checks take this path. | tc-helps (`highlight_quote`, used by `CheckItem::title`) | `a_reordered_quote_lights_only_its_own_phrase` (tC4's five vectors on `GEN-773.usfm`, plus TIT 2:2 "in love") | Port `shortestRunInstances` into the multi-word fallback. Keep the single-word and strict paths. h9hm is a plain note, so it affects only the helps highlight. Fixed by 2026-10-23: the test passes. |
| 2 | #782 | The shown text of en_ult Genesis 1:1's aligned content is "In the beginning God created  and the earth."; the aligner has 8 words and no "heavens". PSA 34:4, kept as written, has no "Yahweh". 633 of 1,533 Genesis verses with markup lose words; 67 of the 265 en_ult verses the import keeps as written lose words. | tc-usfm (`shown_text_v1`), read by tc-align and tc-journal | `shown_text_reads_words_inside_nested_alignments`; `a_verse_kept_as_written_shows_yahweh`; `selah_inside_qs_shows`; plus a tC4-imported verse with nested `\zaln` | Walk `\zaln` and word nodes at any depth, as tC4 9ad4d5c does. Add tC4's vectors (`fixtures/usfm/shown-text-vectors.json`); the existing vectors do not change. Update STORAGE §7.7 ("Shown text and hashed text"). Land before format 1.0. |
| 3 | #795 | After the import split, MAT 5:22 shows "brotherwill", MAT 5:44 "enemiesand" and 1TH 3:2 "Godin" (12 Matthew and 3 1 Thessalonians verses). Opening MAT 5:22 in Align drops 2 of its 44 links, and "brotherwill" can never be aligned. | tc-usfm (`shown_text_v1`), tc-align | `a_footnote_between_words_keeps_its_space`; `imported_mat_5_22_opens_with_all_44_links` | Keep the character after `\f*` or `\x*` when the note sits between two non-space characters (tC4 PR #800), and after every other marker the shown text leaves out (`\va`, `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*`…), which PR #800 does not (declared difference, DIFFERENCES). Same change as #782. |
| 4 | #787 | A record saved for "Pablo, siervo de Dios", reopened against "Pablo, esclavo de Dios", gives `is_stale()` false. "The draft changed" never shows after a sync, a kept version or an outside edit changed the verse. | tc-align (`Aligner::open`, `is_stale`), tc-api (`Freshness`) | `a_stale_value_opens_with_the_draft_changed_notice` | Done by FIX-align: `Aligner::is_stale` is the flag set at open by `needs_review` (row 6), kept until the first edit, Mark valid or `keep_reflow`; tc-api's `Freshness::Stale` carries it. The save on leaving stays (AD19). |
| 5 | #796 (word list) | For "Pablo, siervo\f + \ft o esclavo\f* de Dios", the bank holds "siervode" but the export expects "siervo" and "de". After every word is placed and the verse is marked valid, `weave_verse` fails with `TargetWordNotFound("siervode")`, so the export drops the verse's alignment with no message. | tc-align (`TargetVerse`), tc-usfm | `the_bank_is_the_export_for_every_vector`, `the_bank_is_the_export_for_every_verse_of_the_bundle_and_the_fixtures` (every verse of the ULT and UST v91 and the fixtures) and the proptest `the_bank_is_the_export_for_generated_verses` (`tc-align/tests/bank_words.rs`): every banked word placed weaves, and the woven verse holds exactly the bank | Done by FIX-text: `TargetVerse` banks `tc_usfm::target_words`, the export's own list, which reads word markup through; the shown text decides no word. |
| 6 | #796 (review rule) | (1) After a source upgrade the rail shows Invalid, but the open verse shows no notice. (2) A record with the current hash that holds "siervo" for "Pablo, esclavo de Dios" shows Valid 4/4; it opens with a dropped link and the export fails. (3) Editing "Selah" to "Pause" inside `\qs` keeps the old alignment, and the rail stays Valid. | tc-align (`value.rs`, `aligner.rs`), tc-journal (fold), tc-store (reflow, R25) | `a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail`; `after_a_source_upgrade_the_open_verse_shows_the_notice`; `a_verse_edit_of_a_word_the_hash_leaves_out_reflows_its_alignment` (tc-store); `the_alignment_files_and_the_rail_agree_on_invalid` (tc-journal, T) | Done by FIX-align: `tc_align::needs_review` (the journal's staleness, a word the record holds that is not the verse's, or a card for a word the pinned verse lacks) decides the rail (`verse_status`), the notice and the reflow, and is D2's to call. `tc_align::reflow` carries a record whose words differ under a current hash. Done for storage by T step 1: the fold's `invalid` and the rail share `AlignmentValue::needs_review` (tc-journal); `tc_align::needs_review` adds only the pin check (`the_psalms_corpus_files_are_what_the_screens_compute` expects 0 disagreements, was 168 of 357). |
| 7 | #794 | The en_ult TIT 1:1 aligned body opens in the aligner with 29 words. Its hashed text is punctuation only, so a save vouches for nothing, and the next verse edit restamps it. | tc-align (`Aligner::open`), tc-store (reflow) | `a_verse_that_holds_alignment_markup_does_not_open` (tc-align); `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (tc-store, T) | Done by FIX-align: `TargetVerse::holds_alignment_markup`, `Unavailable::AlignedInText` (it wins over the others) and `NoReflow::AlignedInText`. D2 keeps its bytes. |
| 8 | #793 | A memory trained on JON 1:1-12 with NFC Hebrew source words (as tC4 4.0 stores them) gets 45 of 70 right for JON 1:13-17 asked with the raw words, against 51 asked in NFC: 12% fewer for every Hebrew project imported from tC4 4.0. | tc-align (suggest keys, `words.rs`) | `memory_trained_on_nfc_records_suggests_for_raw_source_words` (equal right counts) | Done by FIX-align: the engine puts every token's text and lemma in NFC where a sentence reaches it (`wordmap::tokens`); without it the test gets 31 right against 37. |
| 9 | #790 | The sample TIT 1:1 record opens with 10 cards holding 11 source words; UGNT TIT 1:1 has 17, so six Greek words can never be aligned. A card holding Σαῦλος, which is not in the pin, stays, with nothing dropped. STORAGE says this check happens; no code does it. | tc-align (`Aligner::open`) | `the_sample_opens_with_all_17_ugnt_words`; `a_link_to_a_source_word_the_pin_lacks_is_dropped_and_shown`; `opening_a_repaired_verse_writes_nothing`; `an_imported_nfc_hebrew_record_keeps_its_links` | Done by FIX-align: `Aligner::open` fits the cards to the pin, in source order, matching a word as written, else in NFC with the pin numbered in NFC. A word stays on two cards only while both hold links (the ULT's own form); an empty second card goes (324 in the ULT's Psalms). A value with `x-ref` is left as it is until #409. A card the pin cannot hold is part of `needs_review`, so the rail shows such a record Invalid and the verse opens stale (`a_done_record_with_a_source_word_the_pin_lacks_is_invalid_on_the_rail_and_opens_stale`). Declared difference (DIFFERENCES). |
| 10 | #789 | In TIT 1:1, link Θεοῦ¹ to Dios¹ and Θεοῦ² to Dios², then delete "de Dios" after "siervo". The surviving Dios, linked to Θεοῦ², moves to the Θεοῦ¹ card; Θεοῦ² is empty; nothing is flagged and the rail stays green. | tc-align (`reflow`), tc-store (verse save and section save) | `deleting_an_earlier_copy_keeps_the_link_on_its_physical_word`; `inserting_an_earlier_copy_renumbers`; `delete_then_readd_never_reattaches_an_old_link`; a tc-store section-save case | Done by FIX-align in tc-align, Q30 (a): `tc_align::reflow_edit(value, before, after)` follows each link by a longest-common-subsequence match of the two texts; a link the match does not place for sure returns to the bank and is reported; no review mark. Without the earlier text (`reflow`, or `Aligner::open` without `made_against`) a link to a changed verse keeps its occurrence only while the verse holds its word once, as when it was placed: a copy deleted and another typed elsewhere leaves the count, so a link to a repeated word returns to the bank, listed, and the open does not save it (`without_the_earlier_text_a_link_never_moves_to_a_retyped_copy_of_its_word`, `a_link_to_a_retyped_copy_of_its_word_opens_dropped_and_is_not_saved_on_leaving`). tC4's shown-text stamp counts as the same text. tc-store's verse and section saves call `reflow_edit` from the old text (`a_link_follows_its_own_copy_of_a_repeated_word`). The app's open, when it is wired, passes the verse version whose hash is the value's. |
| 11 | #661 | Change only the en_tw pin from v90 to v91 when a decided Words row (2SA vwrw, `kt/arcofthecovenant`) loses its article. `consequences` reports unaffected 1, affected 0, so the upgrade dialog says nothing changes. The decision itself is kept and listed off-list. | tc-check (`change::consequences`) | `a_tw_only_change_that_drops_a_decided_article_is_counted`, with a no-change control | Give `consequences` each set's tW article set before and after, and count a row whose item leaves the list. |
| 12 | #701 | Urdu (Devanagari) is right to left with the name اردو. Once B3 renders the registry, Devanagari helps read right to left under an Arabic-script name. | tc-resources (`gateways.rs:122-132`) | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | Left to right and उर्दू. Record the difference and tell the owner. B3 prefers the direction in the installed helps' manifest. |
| 13 | #552 | A Select made disabled while its list is open still accepts a row click and calls `on_change`. | tc-ui (`ds/forms/select.rs`) | ds-gallery case `Select/disabled-while-open` | Return early in `choose` when disabled, and close the list when it becomes disabled. Check the label-press edge in the same case. |
| 14 | #710 | `tc-door43-fake` refuses a push without write access with HTTP 403. Door43 answers 200, with "User permission denied for writing." and a declined ref. A sync built against today's fake would show Door43's real refusal as an unknown error. | tc-door43-fake (`api.rs:1485-1488`), tc-door43; X | `a_push_without_write_access_is_no_write_access_and_makes_no_branch` and two more, on `origin/windows-push-refusal` until its merge | Merge `windows-push-refusal` (02300d9) before X starts. Correct STORAGE §9.5 and DELTA-10-16 Q12. Done later on 2026-10-19: merged, with both corrections. |
| 15 | #549 | tc-obs finds a project's own story pictures only by the role `x-obsimages`, but the rebuild's `metadata.json` writes no role for carried files. An imported story project's own pictures would never show, only stock ones. | tc-obs (`images.rs:168-183`, `:300-309`) | `a_project_picture_without_a_role_is_the_project_rung` | Find project pictures by path under `ingredients/` (outside `checking/`, `tc/` and `content/`), matched by file name. Declared difference. |
| 16 | #582 | Creating a project with the code `acm-x-lebarabic` or `alk-alakatapue` succeeds, and `metadata.json` says the language is "und / Undetermined"; the name and script direction are dropped. | tc-store (`Store::create`, `apply_upgrade`) | `create_refuses_a_language_tag_the_burrito_cannot_hold` | Check every seed record with `tc_burrito::check_field`, and refuse before any folder exists. The dialog's offer is Q35. |
| 17 | #767 | Design defect; no code runs it yet. Add a book from USFM is planned as AddBook ("___" stubs), then one SaveSection per chapter. A crash or refusal between them leaves a stub book, and every retry is refused as "already in the project, or was removed". | tc-store and tc-journal (no one-batch command; APP-DESIGN §9.2 R3) | `adding_a_book_from_usfm_is_one_batch_whole_or_absent` (FaultFs at every write point) | A one-batch command, cause Import, with §13.5's alignment split. Remove R3's fallback from C6 (a change to Q-17). |

Since 2026-10-19: rows 2 to 10 are fixed (FIX-align, FIX-text) and their area rows now say
covered; row 14 is fixed by the merge of `windows-push-refusal`. Row 1's test exists; its area
rows change to covered once FIX-R confirms it passes.

Found by SWEEP (2026-10-23), from tC4 issues. Each was read from code, not run; FIX-R
reproduces and fixes them:

| # | Issue | Failing scenario | Crate | Regression test | Fix |
|---|---|---|---|---|---|
| 18 | #825 | In Translate, save TIT 1:1 "Dios Dios" (first copy aligned) as "Dios Pablo". The link goes back to the word bank and nothing tells the translator. | tc-store (`commands.rs` `fn reflow`, which drops `reflowed.dropped`) | `a_link_sent_back_to_the_bank_is_listed` (Align journey); Translate's reply test to create | FIX-R step 2: show the list of dropped links (Q30 (a)). Align's half fixed by FIX-R; Translate's half waits for tc-store's `reflow` to return the dropped links. |
| 19 | #830 | Align a Psalm verse into Spanish: the words placed on a card read right to left. | tc-ui (`views/align/session.rs`, the card grid takes `dir=orig_dir`) | `an_ot_card_shows_placed_words_in_the_translation_direction` (journey) | Fixed by FIX-R step 1: placed words carry the translation's direction. |
| 20 | #832 | Settings → Right to left → Save: Home and Align still read left to right; an imported project's language name shows empty. | tc-app (`settings.rs`, `home.rs`, `align/open.rs`) | `a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart`; `an_imported_project_shows_its_metadata_language_name_in_settings` | Fixed by FIX-R step 1: one direction reader; Settings shows the metadata's language name. |
| 21 | #833 (.1) | Internet on, no connection, Settings → Manage source texts → Check for updates: the raw error text shows. | tc-app (`upgrades.rs`) | `an_offline_update_check_shows_one_plain_sentence` | Fixed by FIX-R step 2: one plain sentence. |
| 22 | #836 | Understand with the BSOJ Arabic set: an English question (ar_tq Titus) reads right to left. | tc-ui (`views/helps_panel.rs`) | `an_english_question_under_an_arabic_set_reads_left_to_right` (journey) | Fixed by FIX-R step 1: the text's own language decides its direction. |
| 23 | #814 (D105.5) | Align: a verse marked "needs review" after a source upgrade; move one word: the mark goes away though the rest was never checked. The owner ruled that only Mark valid clears it (Q37). | tc-align (`is_stale` in `aligner.rs`); FIX-R | `an_edit_keeps_the_review_mark_until_mark_valid` (`crates/tc-align/tests/tc4_edit.rs`; replaces the old test that wanted an edit to clear the flag) | Fixed by FIX-R (Q37): an edit keeps the mark; only Mark valid clears it. |

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
  declared difference (`docs/DIFFERENCES.md`): the content of a paragraph marker but `\esb`, and of a
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
  difference (`docs/DIFFERENCES.md`); no ULT or UST v91 verse has one.
- **Words that join across a span or paragraph marker** (`the \add Lord\add*s house`,
  `Yahweh\q2 que`, adjacent ruby): tC4 banks "Lords", "Yahwehque" and "神愛", which the export
  splits. Fixed in the rebuild as a declared difference (`docs/DIFFERENCES.md`;
  `a_skipped_marker_right_before_a_letter_leaves_a_space`). The woven form of a ruby word
  (`\rb \w 神|…\w*|かみ\rb*`) still shows its gloss ("神|かみ"); the import's split reads it
  back to the plain form, whose bank is the export's.
- **Text after a character span that holds a note** (`\wj Blessed are the poor\f + \ft Or
  humble\f* in spirit\wj* and he went on`, valid USFM common in red-letter Gospels), and after
  a stray end marker (`a\nd* b`): usfm-js keeps it as the `content` of the end marker's own
  object, which tC4 hides, so the bank lacks words the export counts and the verse save drops
  their links. Fixed in the rebuild as a declared difference (`docs/DIFFERENCES.md`;
  `words_after_a_span_that_holds_a_note_show`). The rebuild's walk of a span's children had
  also shown the words of a character span nested in such a note; they stay hidden now.

## 2. Align

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #796 (word list) | The aligner and the export split a footnote verse into different words | covered | tc-align, tc-usfm; D2 | `the_bank_is_the_export_for_every_vector` | See §1 row 5, done by FIX-text. tC4 did the same in #809 and #811 (D105.1). |
| #796 (review rule) | One "needs review" rule for the rail, the open verse and the export | covered | tc-align, tc-journal, tc-store; T, D2 | `a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail`; `after_a_source_upgrade_the_open_verse_shows_the_notice`; `the_alignment_files_and_the_rail_agree_on_invalid` | See §1 row 6. tC4 adopted one rule in #814 (D105). Whether an edit clears the flag is the D105.5 row. T step 1 made the fold's `invalid` share the rule (`AlignmentValue::needs_review`). |
| D105.5 (#814) | Does an Align edit clear "needs review"? | covered | tc-align; FIX-R | `an_edit_keeps_the_review_mark_until_mark_valid` (replaces the old test that wanted an edit to clear the flag) | Ruled 2026-10-23 (Q37, `rulings/2026-10-23-Q37-needs-review.md`): follow tC4, only Mark valid clears it. Fixed by FIX-R: an edit keeps the mark (§1 row 23). |
| #796 (export) | Never drop a verse's alignment silently, and keep imported markup | covered | D2 | `export_reports_and_keeps_a_verse_that_needs_review`; `export_keeps_inline_zaln_bytes_even_with_a_matching_record`; `a_record_with_nothing_placed_exports_the_stored_bytes` | Weave only a verse that passes `needs_review` and whose words match. Otherwise write the stored bytes and name the verse in a notice. Check for inline `\zaln` before validity (tC4 PR #799). Done by D2: aligned USFM weaves a verse with `tc_align::usfm::weave_verse` only when `needs_review` is false; a verse that needs review, holds inline `\zaln` or whose weave is refused keeps its stored bytes and is named in the export's facts and on the menu (at most 20, then ", …"); a record placing nothing keeps the bytes unnamed (`export_judges_validity_with_needs_review`, `an_export_names_at_most_20_skipped_verses_with_the_total`). Declared difference (DIFFERENCES): e2bfd8c3, and #814 (7baa1257) that brings it to v4.0.0-rc.2, are newer than d310bf3. |
| D105 (#809, #811, #814) | The export leaves out a verse that needs review and names it | covered | D2; REF1 | `export_reports_and_keeps_a_verse_that_needs_review` | tC4's notice is `cc.exportSkipped`: "Your alignment of these verses is not in the file. Check them in Align: {refs}". D2 uses that string (REF1 vendors it); the rebuild writes the stored bytes for such a verse, as the #796 (export) row says. Done by D2: the `export-skipped` callout says rc.2's `cc.exportSkipped` words (rebuild key `export.skipped`) under the toast, at most 20 refs then ", …"; such a verse keeps its stored bytes. |
| #796 (tC3 import) | A tC3 alignment whose words no longer fit the verse imports as current | covered | I | `a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid` | tC4 e79a2795 and e17fce08 (after 9ad4d5c): a tC3 record is invalid when tC3 kept an invalid marker, or when it holds a link and its words are not the verse's words (text edited outside tC3); it is kept as tC3 held it, never rewritten to fit. The rebuild imports it with `marked_invalid` set. |
| #795 | One hash for the aligner and the alignment files | covered | tc-store, tc-align; FIX-R | `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen` (tc-app, FIX-R step 5) | The rebuild stamps `plain_text_v1` of the stored NFC verse everywhere. The display half is #796 (word list). tC4 #800 also refuses a verse whose text starts on the next line ("tC4 cannot save an alignment for this verse"); the rebuild aligns it, a declared difference (DIFFERENCES). |
| #794 | Do not align over a verse that holds imported alignment markup | covered | tc-align, tc-store; T, I, REF1 | `a_verse_that_holds_alignment_markup_does_not_open`; `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (tc-store, T) | See §1 row 7. I lists such tC4 books in the import report. tC4 merged #799; REF1 takes its `align.unavailable.inline-alignment.*` strings in place of the rebuild's `align.unavailable.alignedInText`. |
| #553 | Keep the alignments of an aligned USFM file on import or Add a book | covered | tc-store (Add a book, C6); I (Update a book) | `aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records`, `an_aligned_usfm_file_updates_its_alignments` | Both paths call `split_aligned`. A verse it refuses keeps its text, is counted, and falls under #794. Import of a raw USFM project waits (rulings/2026-10-24-H3-H4-H7-fast-release.md). Compare in NFC (#793) and per verse with x-ref (#409). |
| #793 | Suggestions: Hebrew source words in two Unicode forms | covered | tc-align | `memory_trained_on_nfc_records_suggests_for_raw_source_words` | See §1 row 8. |
| #792 | A span made while an alignment save waits leaves orphan records | not applicable | tc-store | — (the scratch test `zz792_late_alignment_save_after_join` was not kept) | No book-wide buffer. A late save after a join is kept and offered at the joined run ("Use as a starting point"). tC4's fix (#804, drain the align queue before a span save) caused #807 (§5). |
| #791 | Tell a translator when a teammate aligned the same verse differently | reference change | E1a, B11 | J26 step `two_actors_align_one_verse_and_the_row_and_review_show_it` (ruling H5: newest wins, the Received sheet says what was replaced; C7b) | tC4 shipped D108's per-verse rows in #806 (see the D108 row in §5). Storage keeps both as a conflict. B10 built the sync result's note "1 verse has two alignments to review." with Review (`a_verse_with_two_alignments_is_named_and_offers_review`, `the_toast_counts_verse_conflicts_alignments_and_the_rest_apart`); the per-verse rows of D108 (book, chapter, verse and teammate) are E1a's to port. Done by B11: the aligner's verse shows "2 versions · Review" and opens the review's Alignments tab, read only until C7b (`align/open` carries the record's marker). |
| #790 | Stored source cards are never compared with the pinned verse | covered | tc-align; D2 | `the_sample_opens_with_all_17_ugnt_words`; `the_export_weaves_no_link_to_a_source_word_the_pin_lacks` | See §1 row 9. tC4 repairs cards from the pin since #816 (D107). D107.4 also marks for review a done record that only gains empty cards; the rebuild shows it valid (REF1 decides it as a parity port). |
| #789 | A link moves to another copy of a repeated word | covered | tc-align, tc-store; T | `deleting_an_earlier_copy_keeps_the_link_on_its_physical_word`; `a_link_follows_its_own_copy_of_a_repeated_word`, `a_translate_save_that_drops_an_unclear_link_lists_it`, `a_section_save_that_drops_an_unclear_link_lists_it` (tc-store) | See §1 row 10 and Q30. tC4 #815 (D106) follows by position too but marks an unclear verse for review; the rebuild lists the link (Q30 (a), DIFFERENCES). A verse or section save lists the links it sent back to the bank in `StoreReply.dropped_links` (T step 5); FIX-R shows the list in Align. |
| #788 | A card click places a word that a suggestion moved | covered | tc-align; B6 | `only_the_shown_selection_is_placed_and_the_cards_look_armed_only_while_it_is_not_empty` | B6 draws the armed look and accepts drops from `is_armed` and `selection()` only. |
| #787 | A changed verse opens with its old bank and no warning | covered | tc-align, tc-api | `a_stale_value_opens_with_the_draft_changed_notice` | See §1 row 4. Saving a lossless carry on leaving stays (AD19), as a declared difference. |
| #782 | Words inside nested alignments are missing | covered | tc-usfm (B4a, B5b, B6, B8 done) | `shown_text_reads_words_inside_nested_alignments` | Done (§1 row 2); tC4 carried it to rc.2 (#783). B5b: the Check's tappable words are `words(shown_text_v1)` of the verse (`tc_check::selection::target_words`), the list the test reads ("heavens" in Genesis 1:1). B8: the Community Checking preview prints each verse's `shown_text_v1` (`export/book` reads Translate's verse records); `the_preview_prints_the_words_inside_nested_alignments` (tc-app) prints Genesis 1:1 whole from the en_ult file. |
| #768 | Card labels lost after a save (Hebrew NFC) | covered | B6 | `gateway_label_survives_an_nfc_record` | Compare in NFC on both sides, plus the occurrence. Reuse `tc_helps::highlight::normalize_word`. Done by B6: `Gateway::label` compares each card's source words with the ULT's `\zaln` x-content in NFC on both sides (`gateway_label_survives_an_nfc_record`: 7 labels for the resource's own Hebrew and for its NFC form). |
| #758 | Clear every alignment of a verse | covered | B6, tc-align | `clear_discards_standing_suggestions` | No Clear control while tC4 has none. `Aligner::clear` must drop standing suggestions (today one survives). Done by B6: the tool has no Clear control, and `Aligner::clear` drops the standing suggestions (`clear_discards_standing_suggestions`). |
| #754 | Keep each card's words in verse order | covered | tc-align, tc-store | `la_fe_de_los_dropped_as_fe_la_los_reads_la_fe_los` | None. |
| #753 | Preview a dragged word before or after a placed word | covered | B6 | `every_edit_leaves_each_card_in_verse_order_wherever_the_word_was_dropped` | None. The owner replaced it with #754; no preview. Done by B6: every edit returns each card's words in verse order, wherever the word was dropped (the drag step of J5). |
| #736 | Select several bank words and place them at once | covered | tc-align; B6 | `shift_adds_the_range_from_the_first_selected_word_in_bank_order_either_direction` | B6 passes Ctrl, Cmd and Shift to `click_bank_word`. Done by B6: Ctrl, Cmd and Shift pass to `click_bank_word` (`several_selected_words_are_placed_in_one_edit`, the journey's Ctrl and Shift steps); a drag of a chosen word carries the whole selection. |
| #735 | Number repeated words by occurrence | covered | tc-align; B6 | `a_repeated_word_shows_its_occurrence_and_is_named_word_n` | B6 shows the number as a superscript and uses the accessible name. Done by B6: a repeated word shows its number raised and its chip is named "word n" (`occurrence_name`); the DOM of the bank and the cards is tC4's. |
| #734 | An unaligned word returns to its place | covered | tc-align | `a_repeated_word_returns_to_its_own_occurrence_slot` | None. |
| #568 | A keyboard-only user cannot align words | covered | B6 | Playwright `align_by_keyboard_only` | Cards take a Tab stop; Enter places the shown selection. Declared difference; tell the owner. Joining cards waits for a tC4 design. Done by B6, journey `align_by_keyboard_only`: Enter or Space on a bank chip selects it (with Ctrl, Cmd or Shift for several), a card is a Tab stop while words are chosen (`tabindex` 0, else -1 so it can hold the focus) and Enter or Space places the chosen words; the focus stays through the redraw. A declared difference (residual of `align-workspace`). |
| #420 | An aligned word keeps its character style (`\add`) | covered | tc-usfm; D2 | `weave_writes_what_tc4_writes` | Add to DIFFERENCES' declared differences. J7 gets tC4's `\add` case. Done by D2: declared in DIFFERENCES; journey J7 `J7: an aligned word in an add span keeps the span in the aligned USFM (tC4 #420)` exports a saved `\add siervo\add*` woven with the span kept. |
| #409 | x-ref and per-verse occurrences in a verse span | covered | F1, B6, D2 | `an_imported_span_with_x_ref_opens_with_every_link`, `an_import_keeps_x_ref` | Keep imported x-ref. The #790 repair compares a span's source words per verse. D2 writes x-ref only once tC4 defines it. B6's half is done: a span whose source words carry `x-ref` (as an aligned file added as a book brings it, C6) opens with every link on its own card and nothing to save; the pin fit leaves it as it is rather than reading 1:3's first "Θεοῦ" as the span's first (tc-align `an_imported_span_with_x_ref_opens_with_every_link`, which fails with the skip taken out). D2's half is done: the export writes no `x-ref` (tC4 does not define it yet; `tc-export/src/usfm.rs`), and a verse kept as stored keeps whatever it holds; writing x-ref waits for tC4. F1's half is done: an aligned file's source word keeps `x-ref` in the stored alignment record (tc-app `an_import_keeps_x_ref`, through Update a book; a tC3 alignment goes through the same decoder of tC4's record). |
| #274 | A phrase suggestion over several cards | covered | tc-align | `a_prediction_over_two_cards_is_left_out_never_cut_to_the_first` | None. |
| #784 | A `\v` after a poetry marker on the same line | covered | tc-usfm; REF1 | `the_check_text_is_what_tc4_reads` | tC4 merged #785 (main and rc.2); REF1 rechecks the vectors. |
| #726 | Previous and Next beside the marking buttons (Align) | covered | B6 | DOM parity of `align-workspace-row` (en, ar, 620 px) | Row `[← Previous] [✓ Mark valid] [Next →]` as quiet pills; test ids `align-prev` and `align-next`. Done by B6: `align-prev` and `align-next` are quiet pills either side of Mark valid (`views::item_nav`, shared with B5b); their neighbours and the item counter come from the rail's own list. The DOM of `align-workspace-row`, `ar-align-workspace-row` and `align-workspace-620` is tC4's up to the first card the pinned verse adds. |
| #722 | The verse button is missing after the UGNT upgrade | covered | tc-align; B3, B6 | `staleness_names_each_reason_a_done_value_no_longer_vouches_for_the_verse` | None. |
| #720 | An aligned Portuguese Bible from Aquifer's AI alignments | unit todo | AQ1 | — (AQ1 names it after H6) | Q34 is answered (a labelled reading pane, ROADMAP H6). The screens wait for H6. |
| #516 | Suggestions grow with every saved verse | covered | tc-align; C5 | `a_confirmed_save_joins_the_memory_at_once_and_a_resave_keeps_the_count` | C5 calls `append_verse` on every confirmed save. |
| #400 | Suggestion accuracy and ramp-up | covered | tc-align | `the_growth_bench_on_en_ult_and_en_ust_titus_gives_tc4s_recall` (release, ignored; passed) | None. |
| #820 | Flag ambiguous repeated-word replacements for review | covered | T step 4; FIX-R step 2 | `replacing_one_copy_of_a_repeated_linked_word_keeps_no_unsure_link`; proptest `follow_matches_every_longest_common_subsequence` (tc-align, T step 4) | tC4 #824 marks the verse for review. The rebuild's `follow` keeps a link only where every longest common subsequence agrees, so "Dios Dios" saved as "Dios Pablo" drops and lists the link (Q30 (a)). T step 4 added the vector and the proptest. |
| #825 | Flag a link that a tie drops after a Translate save | rebuild bug | tc-store; FIX-R step 2, T | `a_link_sent_back_to_the_bank_is_listed` (Align journey, FIX-R); Translate's reply test to create | See §1 row 18. FIX-R fixed Align's half: the aligner lists the links sent back. Translate's half waits for tc-store's `reflow` to return them. |
| #830 | OT verse: placed words read backwards in a left-to-right translation | covered | tc-ui; FIX-R step 1 | `an_ot_card_shows_placed_words_in_the_translation_direction` (journey) | See §1 row 19; fixed by FIX-R step 1. |

## 3. USFM, Translate and helps

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #786 | Editing a multi-line poetry verse changes only its first line | covered | B4a, B4b; REF1 (H7) | `verse_editor_shows_every_poem_line` (tc-app and tc-ui); its case of a verse that starts on its own poem line (inside the tc-app test); `no_change_section_save_on_psalm_3_writes_nothing` and `no_change_section_save_on_genesis_3_14_to_17_writes_nothing` (B4b, tc-app) | B4a: `editor/chapters` carries each verse's whole `words`, every poem line with its markers, and the verse card edits them; the box's text and emptiness come from `words` alone (only the stub `___` shows empty). Q-20's wrong premise is rewritten (APP-DESIGN §12). B4b: a section card opens on each verse's whole `words`, and a save writes only the verses whose text changed, a changed verse its `words` only; every section of Psalm 3 and Genesis 3:14-17, as one card, by pairs and verse by verse, saves as no change, and a change to one verse keeps each line of the others byte for byte. C2: a save replaces `words` only. tC4 fixed it in #810 by showing poem lines as tabs; the rebuild shows `\q` markers (H7). |
| #784 | A `\v` after a marker is missing from Translate and Align | covered | tc-usfm; B6 (B4a done) | `every_aligned_ult_verse_opens_in_verse_order_with_nothing_to_save` (2,461 Psalms verses); `a_verse_right_after_a_marker_is_listed_in_its_paragraph` (tc-app, B4a) | B4a: a verse right after a marker (`\p\v 6`) is a row of its own that opens its paragraph; a save replaces `words` only, so the marker before `\v` (the previous verse's layout) stays. B6: 1,533 Genesis and 2,461 Psalms verses. |
| #802 | Refuse a book save over newer text on disk | covered | tc-store; C2 | `a_stale_token_makes_a_conflict_never_a_revert`; `loss_rename_retry_never_overwrites_outside_save` | None. No two-tab case: the one-instance ruling (2026-10-22) forbids two app instances writing at once. |
| #179 | An edit whose base is unknown displaces the verse's text | covered | tc-journal; T | `loss_unknown_parent_verse_edit_keeps_standing_text` | Two heads, the standing text shows, a conflict marked incomplete history (T step 4). |
| #364 | Renumber verses in the editor | not applicable | later B4b, C2 | — | tC4 4.1.0. STORAGE §5.2 already has the write. Port when tC4 ships it. |
| #365 | Import a book into an existing project | not applicable | later F1, C7a | — | tC4 4.1.0. Build on the §11.1 outside-edit path when wanted. |
| #353 | ULT and UST side by side in Translate | not applicable | B4a if adopted | — | Post-4.0, no layout decided. |
| #131 | Translate's source pane in projects with other verse numbers | covered | B4a | `cross_frame_translate_shows_the_mapped_source_verse` (tc-ui: a shift, a fan-out, another book, no place, an unknown numbering; and a journey) | Done by B4a: Translate reads `editor/sources`' mapped panes, one row per project verse labelled and numbered with the source verse it shows, with tC4's statements for a verse in another book or in no place. A declared difference (APP-DESIGN B4a D9); the owner is told. |
| #121 | Understand: source text for verses mapped into another book | covered | B3 | `cross_book_verse_shows_the_statement_and_keys_comments_to_the_project_verse` (tc-ui); `a_verse_in_another_book_names_that_book` (tc-app) | Done by B3: a project verse the sources have in another book is `understand.verseCrossBook` with its place (`DAG 13:1`), never this book's text; the comment box keeps the project verse; a chapter wholly in another book says `understand.helpsCrossBook`. |
| #118 | Understand: Section view in projects with other verse numbers | covered | B3, B4a | `cross_frame_project_shows_the_verse_only_statement_instead_of_the_switch` (B3 journey); `a_project_in_another_numbering_reads_each_verse_where_the_sources_have_it` (tc-app); `cross_frame_translate_shows_the_mapped_source_verse` (B4a journey) | Done by B3 and B4a: one unit per project verse, each the source verse the project's numbering names (`editor/sources` `frame`), `understand.crossFrameVerseOnly` in place of the switch on both screens, `understand.frameUnavailable` when the scheme is unknown. |
| #55 | Draft book introductions | not applicable | — | — | tC4 4.1.0. The introduction is kept byte for byte. |
| #27 | Merge concurrent edits automatically | covered | tc-journal | `merge_bookmark_and_status`; `loss_check_two_ways_newest_wins` | None. |
| #767 | Import creates an empty book after the size warning | covered | C6 | `adding_a_book_from_usfm_is_one_batch_whole_or_absent`; `a_psalm_adds_whole_with_every_verse_and_its_selah` | See §1 row 17. C6 built the one-batch command `AddBookText` (APP-DESIGN C6 D3). No refusal stays, so `addBook.usfmTooLarge` is not vendored: the rebuild never refuses a book for its size. |
| #782 | Words inside nested alignments are dropped | covered | tc-usfm | `shown_text_reads_words_inside_nested_alignments`; `selah_inside_qs_shows` | See §1 row 2 (fixed). |
| #773 | A reordered quote picks up words from elsewhere | covered | tc-helps | `a_reordered_quote_lights_only_its_own_phrase` | Fixed: the test passes (2026-10-23, `crates/tc-helps/tests/highlight.rs`); §1 row 1. |
| #664 | Test the Hebrew highlight with a maqaf | covered | tc-helps | `hebrew_quotes_split_maqaf_compounds_into_the_aligned_words` | Pin tC4's GEN 1:5, 1:7 and 1:28 cases. |
| #553 | Keep the alignments of an aligned USFM file | unit todo | DONE (after the first release) | `aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records`; `a_verse_whose_alignment_does_not_read_apart_keeps_its_text_as_written` | Split on Add a book as on import (C6: `AddBookText`), and list kept verses (C6 keeps a verse that does not read apart as written; the list of them is F1's). Unit I also splits tC4 4.0 verses that hold `\zaln`. Kept verses: Align unavailable, reflow skipped, export bytes kept (#794). Add a book (C6), Update a book (I, `an_aligned_usfm_file_updates_its_alignments`) and the export (D2) are done. Importing an aligned USFM project, and the list of its kept verses, wait for the USFM importer after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md). D2's half is done: a kept verse (inline `\zaln`) exports as stored in aligned USFM and is named (`export_keeps_inline_zaln_bytes_even_with_a_matching_record`); plain USFM takes the alignment markup out of it (declared difference, DIFFERENCES). |
| #420 | Keep a character style around an aligned word | covered | tc-usfm; D2 | `weave_writes_what_tc4_writes` | Add to DIFFERENCES' declared differences. Done by D2: declared in DIFFERENCES; journey J7 `J7: an aligned word in an add span keeps the span in the aligned USFM (tC4 #420)` exports a saved `\add siervo\add*` woven with the span kept. |
| #638 | Spike on a context-aware USFM editor | not applicable | — | — | tC4 research. |
| #600 | Save a section after deleting all of its text | covered | B4b | `emptied_section_save_asks_then_writes_stubs` (tc-app on the store, and a journey); gallery case `section-empty-confirm` (M18) | Done by B4b: Save section asks "Remove all the text of section 9–10?"; Keep editing, Escape and the scrim write nothing; the answer writes the stub for each verse, a span stays a span and the lines between the verses stay. |
| #598 | Move a placed verse number with click, then click | covered | B4b | J2 `break_a_span_with_two_clicks`; `a_held_number_selects_no_text_and_escape_puts_it_back` | Done by B4b: a pin picks up on click and lands on the word clicked next; a drag (press, move, release) and Enter or Space on the word still place it; Escape or a second click puts it back. |
| #604 | New wording for the Place instruction | covered | B4b | J2 `section_card_reads_the_idle_place_hint` (journey and tc-ui host test) | Done by B4b: the bank shows `draft.pickHint`, `draft.dropHint` or `draft.bankEmpty` by state, and the words do not move when a number is picked up. |
| #576 | Update the Resume banner after a span save | covered | C2; FIX-R | `last_edited_names_the_span_then_its_first_verse` (tc-app `section.rs`) | A join records "2-3"; a break records the first verse of the old span. |
| #575 | A paragraph break inside a new span | covered | B4b | `joined_span_collapses_blank_lines_to_one_space` (tc-api, tc-app and a journey) | Done by B4b, following D94: a blank line between verses joined into a span becomes one space; a single line break stays as typed. |
| #574 | The book's name in a new blank book's header | covered | tc-store | `a_seed_keeps_structure_and_drops_words` | J7 asserts the header. Optional: a seed test for a source with no `\h`. |
| #572 | A span counts as all of its verses in the drafted percentage | covered | B2, B3 | `joining_verses_keeps_the_drafted_percentage` | Done by B2 and B3: the tiles and the rail count with `drafted::count_verse`. The poem-verse rule is a declared difference the owner was told (DELTA-10-19 §5). |
| #603 | A tooltip on each book's percentage | covered | B3 | journey `a_tile_opens_understand_at_chapter_1_with_the_rails_tooltips` | Done by B3: `draft.bookPercentTitle` on each rail row. |
| #577 | "1 chapter" and "1 verse" | covered | C6, F1 | `one_chapter_book_says_1_chapter` | Choose the singular keys when the count is 1. Add a book: C6's test. F1's import review uses the singular keys too (book line and aligned verses, `crates/tc-ui/src/views/modals/import.rs`). |
| #507 | Open a new project in Understand | covered | B3, B7 | journeys `a_tile_opens_understand_at_chapter_1_with_the_rails_tooltips`, `j25_a_story_opens_with_its_rail_its_units_and_writable_comment_boxes` | Done by B3 and B7: an open with no remembered place lands in Understand at chapter 1, a story project at story 1; a remembered place still wins. |
| #499 | Use the Bible name typed on the import review page | covered | F1, I | `typed_import_name_is_the_project_name` | Leave review-edited fields out of the P9 comparison, or the import refuses. |
| #484 | Add a book from a USFM file | covered | C6 | `usfm_files_add_whole_books_and_refuse_the_rest_in_their_rows`; `a_book_from_a_file_is_refused_when_present_or_when_the_file_does_not_read`; journey `create` | One batch (#767), the alignment split (#553), `addBook.usfmExists`. |
| #564 | Add-a-book USFM step polish | covered | C6 | `removing_rows_from_25_aligned_books_removes_the_intended_rows`; `an_add_keeps_its_error_until_the_next_add_and_the_books_it_added_leave_the_list`; `usfm_rows_are_judged_again_at_every_add_and_remove` | Label "Add book" while disabled; keep an Add error until the next Add; classify each file once (the server reads a file once, `create/reviewUsfm`). |
| #502 | Two USFM files never reach the review page | unit todo | DONE (after the first release) | J9 `a_two_file_drop_is_one_new_project` (to create with the USFM importer) | Port the J9 case. Importing USFM projects waits for after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md); F1 imports tC3 projects only. |
| #619 | Quotes and bold in helps, not raw `>` and `__` | covered | B5b (B3 done) | `article_shows_no_raw_quote_or_bold_marks` (tc-helps); `a_checks_article_is_the_helps_blocks` (tc-app) | B3: `tc_helps::blocks`, tC4's renderer, held to tC4's own output on every Titus note (608 blocks). B5b: the Check drawer draws the same blocks, from `check/article`, which answers what `helps/article` answers for the same link. Done. |
| #621 | Superscript verse numbers in the simplified-text tab | covered | B3 | journey `simplified_tab_verse_numbers_are_superscript` | Done by B3, the source verse's `c:v` in another numbering. |
| #628 | Full Markdown in helps | covered | B3 | `ta_article_with_a_table_renders_a_table` (tc-helps); journey `an_article_draws_its_table_numbers_and_nesting` | Done by B3 for tables, numbered items and nested lists, a declared difference the owner was told (DELTA-10-19 §5). Links stay their label; a bare or `[[…]]` rc link shows its raw address, as tC4 4.0 does (owner ruling Q38; the #628 (rc links) row). Installed images are not drawn: tA names them by Door43 address, which no install keeps on this computer yet. |
| #602 | Resize the helps panel by dragging | covered | B3, B4a | `helps_panel_width_follows_the_divider_drag` | Done by B3 and B4a: the divider resizes the panel within 280 to 720 px; one show/hide button; Translate mounts the same panel. |
| #601 | Keep the article title and close button visible | covered | B3 | `article_close_button_visible_after_scroll` | Done by B3: the article's header stays in view. |
| #234 | Widen the helps pane | covered | B3 | `helps_panel_width_follows_the_divider_drag` | Done by B3 with #602's divider. |
| #728 | Make "Draft section" look like a button | covered | B4a | pixel and DOM parity of `en-translate-tit1` and `translate-tit1` (the two differ only by C39's note titles) | Done by B4a: `Button(Secondary, Sm)`, end-aligned, accent text while enabled. |
| #650 | An Arabic gateway package end to end | covered | B3 | journey `arabic_helps_panes_are_rtl_in_an_english_interface` | Done by B3: each helps slot carries its gateway's direction (`rung_dir`), and the cards, questions, simplified text and article read in it. |
| #29 | Right-to-left and complex scripts end to end | unit todo | G2 (B4a's case, B5b and B6's case done) | J10 `rtl_project_drafts_checks_and_aligns`; B4a `refresh_under_typed_editor_keeps_text_and_token_through_an_arabic_ime`; B6 `a_right_to_left_project_aligns_with_its_bank_right_to_left` | Each journey adds the Arabic case; G2 captures the `ar` screens. B4a: Arabic typed through an IME composition into the verse card while a teammate's change arrives keeps every keystroke and saves as a second version. B6: a project created right to left with Titus added from an Arabic file opens Titus 1:1 with its bank right to left and the Greek cards left to right, a repeated Arabic word numbered in verse order, and a placement saved to the store; the Arabic interface's Align screens are captured (`ar-align-workspace-row`, `ar-align-workspace-620`). |
| #614 | Four English literals in design-system props | covered | A5 (C6, B4b and B5b done) | port of tC4's ds-labels a–d; `ds_labels_d_a_verse_numbers_name_follows_the_app_language`; `academy_drawer_close_label_is_in_the_shown_language` | C6 passes catalog strings to Add a book; B4b passes `draft.moveVerse` to each `VerseMarker`; B5b passes the Check drawer eyebrow, title and close label from the catalog. |
| #29 | Right-to-left and complex scripts end to end | covered | B6, G4 (B4a's IME case and B5b done; G1's shell runs the probe) | Arabic IME Playwright case (B4a: `refresh_under_typed_editor_keeps_text_and_token_through_an_arabic_ime`); Graphite probe; `graphite_shapes_awami_nastaliq_in_the_shell` and `rtl_project_drafts_checks_and_aligns` | Use logical properties where tC4's physical ones break right to left, and record it. |
| #614 | Four English literals in design-system props | covered | B4b, C6, A5 (B5b done) | `ds_labels_d_a_verse_numbers_name_follows_the_app_language`; `academy_drawer_close_label_is_in_the_shown_language` | As in §3. |
| #702 | Text zoom sliders | not applicable | — | — | tC4 4.1.0. |
| #719 | Add source Bibles to a project | not applicable | later B9a | — | tC4 4.1.0. |
| #615 | The Understand journey after the Resume journey | not applicable | C3 (J16 port) | — | The fix is inside tC4's J16 at the reference; port J16 as written. |
| #628 (rc links) | Raw `rc://` links in helps (BSOJ Arabic notes for Titus) | covered | tc-helps; FIX-R | `a_bare_rc_link_shows_as_its_raw_text` | Ruled 2026-10-23 (Q38, `rulings/2026-10-23-Q38-raw-rc-links.md`): raw links are fine, as in tC4 4.0. The rebuild shows them raw; FIX-R's test keeps it so. |
| #810 | Poetry verse as one body; poem lines shown as tabs | reference change | REF1 (H7); T step 4 | `verse_editor_shows_every_poem_line`; `no_change_section_save_on_psalm_3_writes_nothing` | tC4 shows poem lines as tabs and no longer cuts a verse at a mid-verse `\p`, `\b`, `\ts\*`, `\q3` or heading. The rebuild shows `\q` markers until the owner answers H7 (recommendation: keep markers). |
| #812 | Editing a verse with a mid-verse `\p`, `\b` or `\ts\*` line changes only the part before it | covered | tc-usfm, tc-store; T | `every_bundled_ult_and_ust_verse_body_holds_every_target_word` (tc-usfm); `verses_with_a_layout_line_inside_save_whole` (tc-store) | Right by code read: `split_verse` treats only the trailing run of layout lines as layout. The corpus test pins it, with 1SA 2:2, 1SA 4:1, REV 7:12 and MAT 27:9 saved whole. |
| #829 | Poetry indents two characters wide per level | not applicable | REF1 (H7) | — | Only for poem lines shown as tabs (#810). If the owner chooses tabs in H7, REF1 takes it. |
| #831 | Home Resume card shows one verse's number with another's text, or `___` | covered | FIX-R step 5 | `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub` | Fixed by FIX-R step 5: the recorder names a verse by the span it now holds, and a stub is no snippet. The test failed before the fix. |
| #836 | English helps text read right to left under an Arabic gateway set | covered | tc-ui; FIX-R step 1 | `an_english_question_under_an_arabic_set_reads_left_to_right` (journey) | See §1 row 22; fixed by FIX-R step 1. |

## 4. Check and resources

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #773 | A reordered quote's title and highlight pick up other words | covered | tc-helps; tc-check | `a_reordered_quote_lights_only_its_own_phrase` | Fixed: the test passes (2026-10-23); see §1 row 1. |
| #765 | Hebrew quote shown instead of English when it has a maqaf | covered | tc-helps | `genesis_maqaf_quotes_title_in_the_ult` | No fix. The converter B3 or B5a builds from the ULT keeps nested `\zaln`; test 0 Hebrew titles over all of Genesis. |
| #664 | Test the Hebrew highlight with a maqaf | covered | tc-helps | `every_jonah_note_quote_finds_its_words_in_the_uhb_verse` | None beyond #765. |
| #769 | Group notes by figure, and fold groups | reference change | tc-check, tc-helps, B5a, B7, A3, A6a | `rail_groups_by_figure_in_first_note_order` (to create); `group_counts_ignore_the_filter`; `fold_survives_a_restart`; `next_into_a_folded_group_unfolds_it`; `fold_all_keeps_the_active_group_open` | See DELTA §3.7 and §3.12.1. Update APP-DESIGN §3.8 and §4.9. |
| #762 | Note at 80ch, source texts side by side | covered | B5b | journey `compare_card_columns_fit_the_width` (port of `compare-card.spec.ts`) | Done by B5b: tC4 9ad4d5c's compare card and `Check.jsx` widths (DELTA §3.12.1): the detail fills the window, lines stop at 80ch, the original and gateway texts are cards in a wrapping row, "Your translation" a card of its own. The 12 session screens differ from the d310bf3 baseline in pixels and DOM for this reason (residuals); A6a recaptures them when the reference moves. |
| #737 | A drag across draft words selects whole words | covered | B5b | `a_drag_selects_whole_words_and_mark_valid_saves_them` (tc-app proptest against from_indices; journey of the same name); `tc_api::model::selection` and WordNav tests | Done by B5b: `drag_range`, and the keyboard after a drag as tC4's `checkKeyboard` cases (Space toggles the word the drag started on, Shift+Right extends from it); Next or Previous before marking drops the selection (tC4's Q2). |
| #726 | Previous and Next beside the marking buttons | covered | B5b, B6, tc-ui | journeys `item_navigation_{translationNotes,translationWords}_{en,ar}_marking_row_boundaries_tab_and_wrapping` (port of `item-navigation.spec.ts`) | Done by B5b for Check: `ItemNavButton` on the DS Button, test ids `check-prev` and `check-next`, the painted row, 14 px end gaps, 30 px pills, arrows turned in right-to-left, Tab following the row, wrapping at 620 px. Done by B6: `align-prev` and `align-next` are quiet pills either side of Mark valid (`views::item_nav`, shared with B5b); their neighbours and the item counter come from the rail's own list. The DOM of `align-workspace-row`, `ar-align-workspace-row` and `align-workspace-620` is tC4's up to the first card the pinned verse adds. |
| #764 | Progress bars look empty at low progress | covered | B5a | `check_progress_bar_is_visible_after_one_resolved_item` (tc-ui) | Done by B5a: the helps tools' bars (picker cards, rail header) fill at least 6 px once one check is resolved and name their count (`aria-label`); the Align and Community Checking bars are tC4's. Declared difference (residuals `check-picker`, `en-check-picker`, `guided-fix`); tell the owner. |
| #757 | Clear every selected word with one button | not applicable | B5b | — | Open tC4 enhancement, outside 4.0. Keep the selection as one local value. |
| #661 | A tW-only pin change does not count rows that lose their article | covered | tc-check; C8, B5a | `a_tw_only_change_that_drops_a_decided_article_is_counted` | See §1 row 11. C8: done. The upgrade and gateway dialogs count a Words decision whose article the new translation words lack, as to check again, and write nothing: the decision is kept as it was (tc-app `a_tw_only_change_that_drops_a_decided_article_is_counted`, over a fake Door43 with two translation words releases). |
| #594 | Mark valid on an invalidated check is swallowed | covered | tc-check | `deciding_again_judges_the_text_and_the_list_in_front_of_the_person` | If tC4 rules that To do clears the flag, change `DecisionEdit::apply`. |
| #595 | The language-change preview retries without limit | covered | C8 | `gateway_preview_appears_while_the_open_verse_keeps_changing`; `a_stale_confirm_reshows_the_dialog_with_new_counts` | Compute once from a snapshot; on PlanStale recompute once. C8: the plan is made once from one snapshot; a confirm that finds a record's heads moved makes it once more, from the project as it is, and shows the dialog again with the new figures, writing nothing until the person confirms again. |
| #518 | Polish the return to the last Check tool | covered | B5a | `resume_shows_no_clickable_picker` (tc-ui); journey `resume_and_tab_click_open_the_tool_once` | Done by B5a: `session/open`'s kept place carries the Check tool, and an open that lands in Check opens it at once: no picker is drawn meanwhile, and a tab click while it opens keeps it. Part 3 (the late write) is covered by the prefs flush tests. |
| #628 | Full Markdown in helps | covered | B3 | `ta_article_with_a_table_renders_a_table` | Done by B3 (§3). Links stay plain text; rc links show their raw address (owner ruling Q38). |
| #663 | Door43 releases missing books | covered | tc-resources | `coverage_follows_what_the_files_hold` | None. |
| #139 | Check % on Home tiles | not applicable | B2 if adopted | — | tC4 4.1.0. |
| #259 | Check for newer releases automatically | not applicable | C8 if adopted | `bundled_upgrade_leaves_the_pinned_release_and_the_projects_pins_as_they_were` | Post-4.0, not adopted. Pins never move on their own: Check for updates is the person's press, and a release an app upgrade brings is only an offer. |
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
| #720 | Aquifer AI-aligned Portuguese Bible | unit todo | AQ1 | — (AQ1 names it after H6) | Q34 is answered (ROADMAP H6); as in §2. |
| #718 | Language code for Aquifer Chinese | not applicable | — | — | tC4 data question. |
| #654 | Research Aquifer resources | not applicable | — | — | Done; moved to #711. |
| #662 | Non-unfoldingWord Aquifer helps | not applicable | — | — | The rebuild reads only unfoldingWord's Aquifer repositories. |
| #701 | Is Urdu (Devanagari) right to left? | covered | tc-resources; B3 | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | See §1 row 12. |
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
| #579 | Name a gateway language the same way everywhere | covered | C8 (B9a done) | `gateway_change_dialog_names_both_languages_by_autonym` | Use the autonym, never the code, including both buttons. B9a: Project settings and Get source texts name a package by its autonym (`settings_from_home_show_the_sample_as_tc4_does`, journey `source-texts`). C8: the plan names both sides by their autonyms and the dialog's title and both buttons use them (journey `upgrades`, parity `gateway-change`). |
| #621 | Superscript verse numbers in the simplified tab | covered | B3 | `simplified_tab_verse_numbers_are_superscript` | Done by B3 (§3). |
| #619 | Quotes and bold in helps | covered | B5b (B3 done) | `article_shows_no_raw_quote_or_bold_marks` | As in §3, with #628's differences. Done by B5b for the Check drawer. |
| #601 | Sticky article header | covered | B3 | `article_close_button_visible_after_scroll` | Done by B3 (§3). |
| #602 | Resize the helps panel | covered | B3, B4a | `helps_panel_width_follows_the_divider_drag` | As in §3. |
| #606 | Third-party licenses in About, offline | unit todo | AQ1 (A5 and G1 done; Aquifer waits for after the first release, ruling 2026-10-24) | `about_lists_aquifer_attribution_offline` (pending: AQ1) | Also list each installed Aquifer resource's attribution (CC BY-SA). B9a: nothing in the app installs an Aquifer resource yet (APP-DESIGN §4.6), so the attribution goes with the unit that first installs one. |
| #532 | Show a verse's new draft on return to the check | covered | B5b (C4 for the writes) | journey `check_shows_new_draft_after_switching_back`; `check_shows_new_draft_after_switching_back` (tc-ui, the selection rule) | Done by B5b: a change that names the open list reads the active check again, so a verse drafted while the checker was in another mode shows on return, with its words as the new tappable ones and the stored selection for them. The journey changes the verse through `test/saveVerse` where Translate's save will. |
| #42 | Keyboard pass on the Check pages | covered | B5b (B5a done) | journeys `check_keyboard_picker_guided_fix_and_rail`, `check_keyboard_on_a_psalms_sized_rail`, `check_keyboard_detail_drawer_words_marks_and_comment`; `rail_keys_are_tc4s_roving_pattern` and the WordNav cases (tc-ui) | B5a: the picker's stops with the ring, the guided fix holding and returning the focus, the rail taking the focus once and being one Tab stop with tC4's keys, Tab out of the list, Back to the card, also on a virtual rail of Psalms' size. B5b: Tab from the rail into the detail, Learn more, the Academy drawer holding the focus and returning it, the target words as one stop with Shift+arrow selection, the marking row in order, the comment and Done returning the focus to its button. #769's heading stops wait for the reference move. |
| #513 | Resuming into tN stalls | covered | B5a | journey `resume_and_tab_click_open_the_tool_once` | Done by B5a: one `check/open` per resume, which settles. |
| #501 | Opening Check changes `resources.json` | covered | B5a; C8 | `open_check_preflight_offer_close_leaves_pins_unchanged` (tc-app) | Done by B5a: the picker, the guided fix's offer and a tool's rail submit no command and leave the pins as they were. C8 ports J12's "pins never move without the accept". C8: done. A plan, an offer and a guided fix move a pin only at the accept (`j12_an_upgrade_moves_one_sets_pins_and_carries_decisions_in_one_batch` reads the pins after the plan and before the confirm). |
| #722 | Verse button missing after the UGNT upgrade | covered | C8, B6 | J12 `ugnt_upgrade_counts_the_verses_whose_alignments_it_marks_and_keeps_every_alignment` (C8); `ugnt_upgrade_marks_1_1_invalid_in_align` (B6) | The pin moves only after the download. C8's half is done: the pin of an original-language text moves only after every repository of the release is installed and verified and the person has confirmed a plan that states how many verses' alignments it marks for review; the alignments themselves are untouched (same heads), and compute as invalid against the new pin. B6's half is done: `ugnt_upgrade_marks_1_1_invalid_in_align` confirms the upgrade through `App::request` and reads Titus 1:1, valid under v0.34, as needing review on the Align rail under v0.35, its links kept. |
| #766 | A project with no pins gets the creation pins | covered | I, F1, E1b, C1a | `open_writes_creation_pins_only_when_none_exist` | Built; tC4 carried #772 to rc.2. I, F1 and E1b keep it for their paths. |
| #763 | Help shows rc.1 pages | reference change | REF1 | help case b `help_home_shows_no_rc1_build_text_offline` | Vendor tc-website dbb7cd0 when the reference moves. |
| #641 | The checking language row never finishes loading | covered | B9a | `a_reply_for_another_projects_settings_changes_nothing`; `settings_from_home_show_the_sample_as_tc4_does` | One server read, `settings/viewFromHome`; a reply lands only on its own project's form. |
| #583 | Three journey tests fail on main | not applicable | — | — | tC4 test fixes. |
| #615 | Understand journey after Resume | not applicable | — | — | tC4 journey order. |
| #700 | Cache the bundle in packaging CI | not applicable | — | — | tC4 CI. |
| #655 | Plan the 4.1 ideas | not applicable | — | — | tC4 planning. |
| #652 | Basic checks | not applicable | — | — | tC4 4.1.0. |
| #653 | AI-assisted checking | not applicable | — | — | tC4 4.1.0. |
| #651 | BT Servant | not applicable | — | — | tC4 4.2.0. |
| #840 | Highlight the current note's words in the original-language text | covered | tc-helps, tc-app, tc-ui; OLH-1 | `the_original_pane_marks_the_quote_at_its_occurrence`, `a_quote_the_verse_lacks_marks_nothing_in_the_original_pane` (tc-helps); `a_note_shows_its_panes_its_article_and_the_draft_words`, `a_chapters_helps_are_tc4s_cards_with_gateway_titles` (tc-app); journeys `a_note_marks_its_quote_in_the_greek_tab`, `the_greek_row_marks_the_quote_of_the_check` | Ruled 2026-10-23 (Q39, `rulings/2026-10-23-Q39-original-language-highlight.md`): built ahead of tC4, a declared difference in DIFFERENCES. Draft's original tab and the check card's original row mark the quote; Understand shows no original text. |

## 5. Sync, storage and import

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #801 | After a sync, show the plain refusal and let the user go Home | covered | E1a, A4b, X; FIX-R | `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` (tc-ui); `stale_scope_save_keeps_text_and_home_leaves_in_one_click` (tc-ui, E1a) | The rebuild has no "old store": a sync joins the open session. Fixed by FIX-R: a refusal that can never succeed lets the person go Home and keeps the text. E1a still ports J1b's one-window route and tC4's 646-line `e2e/sync-refusal.spec.ts` (#803). X holds each sync step, OBS frames included. See §6 for refusals that can never succeed. |
| #761 | Sync story projects with teammates | covered | X, E1a, E1b | `team_sync_obs_project` (tc-sync: an Open Bible Stories project shares, joins and syncs both ways); `a_story_project_syncs_like_a_bible` (tc-app, both ports) | The owner ruled that stories sync (Q2). The planned frame-level tests (two devices' frames, one frame changed two ways, front matter as a carried file) were not written; they are a minor in `tracker:minors.tsv`. |
| #748 | Door43 journey uses committed journals | not applicable | E1b | — | tC4 rig. Seed the fake from a committed project. |
| #747 | Keep backup files out of syncs | covered | S2, X | `a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept` (with TIT.usfm.bak, notes.txt~ and draft.tmp, and the next checkpoint's tree) | It fails with the `.bak` rule deleted (T step 4). `scan.rs` keeps `.bak` out, not `checkpoint.rs`. X: a `.bak` from Door43 stays a foreign entry. |
| #746 | Test that startup recovery runs before projects open | covered | tc-store (S1); C1a | `an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` | C1a moves tc-app onto the store and creates, imports and joins only through `Store`. |
| #724 | Sync button follow-ups | unit todo | E1a, B9b, E1b, A1, B3, B11 | journey `E1a. Download on the sync toast opens the project's Source texts and then closes the toast` (sync-button; Download opens the project and its Source texts, and the toast closes only once they are open); journey `conflict-review` (B11: a refused open keeps the toast and its Review); journey `failed_open_keeps_the_sync_toast_links` (sync-button, E1d: a refused open keeps the toast, its links and counts, and the next Download opens Source texts); `io_reason_renders_without_the_path`; `a_failed_branch_lookup_is_a_failed_sync` (tc-app, E1a) | Item 6 is fixed by B10 under Q36: one fixed sentence, no reason (`io_reason_renders_without_the_path`). The toast's Review and Download leave it in place; the open each starts (B11's review, B3's open) closes it once open, so a failed open keeps its links; B11's review does (journey `conflict-review`, J18 from the toast: an open refused once leaves the toast with its Review, and the next Review opens the review and closes it). The team-access line shows for every project, OBS included. |
| #710 | Record Door43's 403 reply | covered | tc-door43-fake, tc-door43; X | `a_push_without_write_access_is_no_write_access_and_makes_no_branch` (`crates/tc-door43/tests/push.rs`) | See §1 row 14; fixed by its merge later on 2026-10-19. |
| #707 | An import during start-up recovery rolled back | covered | tc-store (S1); I | `an_import_started_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` | I builds every import through `Store::staging` and `Store::place`. |
| #705 | Audio between translators | covered | X, E2 | `a_teammates_audio_file_arrives_byte_for_byte` (tc-app, the real port: a recording put into A's folder by another program reaches B's folder byte for byte through an exchange file); `loss_carried_file_two_versions_both_kept` | Carried files; two versions both kept; a missing blob never refuses the commit. |
| #725 | A Door43 copy collision keeps the project | covered | tc-store (S1); X, E1b | `a_collision_at_the_rename_keeps_the_destination_byte_for_byte` | Join through staging and place; check the name case-insensitively before the internet question. |
| #723 | Discard an empty sync recovery copy | not applicable | X | X `an_empty_remote_tree_removes_nothing_local` | No scratch copy or apply marker in the rebuild. |
| #681 | The three-translator journey | covered | X, E1a | library J26 `three_devices_converge_with_byte_identical_derived_files`; journey `J26 [G1]. Three computers: B and C change verses, A takes their work in by name, and every computer ends with the same text` (team-sync, E1a) | Plus the check-conflict step after C7b. |
| #680 | Show each teammate's Door43 name | unit todo | X, A8, B11 | `username_written_once_after_first_successful_fetch`; `no_email_or_token_in_any_change` | One label function for toast and review: the server names each person by the fold's label (`tc_app::wire::scope::person_label`) and the UI writes it with `tc_ui::app::sync::person_label` (B10, `one_label_for_a_person`), which B11's review uses too (`a_version_is_labelled_as_the_sync_result_labels_its_person`). |
| #679 | Review conflicting verses one by one | covered | B11, C7a | J18 `keep_this_version_resolves_with_review_tokens_and_a_second_send_gets_the_first_reply`; `keep_passage_resolves_every_record_of_the_group`; `keep_my_passage_rewinds_the_join_and_keeps_my_verse`; `bulk_keep_leaves_out_conflicts_with_two_candidates`; `a_bulk_keep_costs_its_conflicts`; `a_look_again_refusal_says_another_version_was_kept`; `a_press_while_a_keep_is_in_flight_sends_nothing`; journey `conflict-review` (J18 and the three C7a tests) | B11 built the review, Keep this version through the store's Resolve with the double-submit rule, and Home's count after the keep; S4's three cases are tC4's but for the dates. C7a adds the passage and structure choices, the look-again and refusal words, Restore and the bulk keep, each with the double-submit rule. |
| #678 | The Sync with Door43 button and result | covered | E1a | `no_door43_request_at_launch_or_60_s_idle` (tc-app); journey `E1a. A real sync pushes the project to Door43; a cut connection fails and changes nothing` (sync-button) | B10 ported the toast and the button's question and sign-in on scripted outcomes, with a closed-set test of every refusal (`every_sync_refusal_shows_its_own_sentence_never_door43s_words_or_a_path`; journey `sync-button`, which also finds no Door43 request at launch or after 5 s idle). E1a runs the real sync. E1a: the sync is real through unit X, in the release build and in the journeys. |
| #677 | Open a project from Door43 on another computer | covered | X, E1b | `open_from_door43_equals_main`; `an_open_asks_no_name_and_keeps_the_teams_name`; `open_private_repo_signs_in_and_retries`; `a_join_cut_short_is_removed_and_home_says_so_once`; journey `open-door43` | Built (E1b): a public repository opens without a sign-in; a private one asks it once, then tries once more. Salvage a damaged pack (Q4); write creation pins when none (#766). |
| #676 | Sync shell with crash recovery | covered | X | `push_is_never_forced`; `a_push_that_loses_a_race_fetches_again_and_keeps_both`; `crash_intake_folds_whole_batches_only_and_commits_on_retry` (tc-store); `random_syncs_lose_no_work` (tc-sync model) | Six retries, not three (DELTA-10-16 Q11); the retry bound is MAX_ATTEMPTS in tc-sync's machine. |
| #675 | Sync core: combine and rebuild | covered | tc-journal; X | `fold_converges_under_any_union` (tc-journal); `a_damaged_remote_pack_is_salvaged_and_kept_in_the_tree`; `three_devices_converge_with_byte_identical_derived_files` | The fold half and the sync's intake are built and tested. |
| #674 | Sync rules and the three-translator fixture | covered | tc-journal; X | `fold_converges_under_any_union`; `conservation` | X ports tC4's team-sync fixture cases as one library scenario. |
| #673 | Spike on Door43 clone and push | not applicable | — | — | Pankosmia routes the rebuild does not use. |
| #766 | Pin the source texts when an import has no pins | covered | C8, F1, I, E1b, X | `open_writes_creation_pins_only_when_none_exist` (`crates/tc-app/tests/app/store_scenario.rs`) | Built; tC4 carried #772 to rc.2. I, F1 and E1b keep it for their paths. |
| #549 | Show a story project's own pictures | covered | tc-obs; B7, I | `a_project_picture_without_a_role_is_the_project_rung` | See §1 row 15. B7 serves the pictures of an installed pack at `/media/obs` and finds none of a project's own: no screen reaches a story project that carries pictures until X, F1 and E1b can bring one in, so the route for them is built with that unit. |
| #161 | Two device ids under one name after a computer change | covered | S2, X | `a_new_device_copies_the_profile_and_both_group_as_mine`; `copied_username_is_replaced_by_the_first_signed_in_fetch` (tc-sync) | The profile copy is tested (T step 4). A hand-copied folder copies the first person's name; X keeps that case. |
| #160 | Record four sync risks | not applicable | P | — | tC4 documentation. P owns the scale risk. |
| #78 | See the history of my changes | covered | B11, C7a | `verse_history_lists_every_version_with_person_and_time`; `restore_writes_over_every_shown_head`; `a_restore_costs_its_record`; journey `conflict-review` (Restore) | B11 built History, read only, in the review. C7a's *Restore this version* on every row but the shown one is a choice on the conflict (`KeepChoice::Restore`, the fold's `Values`): the old version kept over every version of its record, every other version kept in History. |
| #582 | Create a project in a language Door43 lists | covered | tc-store (S1); C6, B2 | `create_refuses_a_language_tag_the_burrito_cannot_hold` | See §1 row 16 and Q35. |
| #496 | Hide "Stay signed in" when it cannot work | covered | A8 | `no_keychain_hides_stay_signed_in` | Show "Your sign-in lasts for one app session." Declared difference; amend APP-DESIGN §4.7. |
| #27 | Merge concurrent edits | covered | tc-journal, tc-store | `merge_bookmark_and_status`; `a_stale_token_makes_a_conflict_never_a_revert` | None. |
| #24 | Team sync round 1 epic | covered | X, E1a | journey `team-sync` (its tests are titled J26 [G1] and J26 [G6], three computers on the fake Door43); tc-sync's guarantees: `team_sync_obs_project` | Put the tC4 G-id in each test or its comment. |
| #802 | Refuse a book save over newer text | covered | tc-store, A4b | `loss_f5_stale_buffer_never_reverts` | None. No two-tab case (one-instance ruling, 2026-10-22). |
| #791 | Tell a translator about a teammate's alignment | reference change | B10, E1a | `two_different_alignments_of_one_verse_are_listed_once` (ruling H5: newest wins, the Received sheet says what was replaced; C7b) | tC4 shipped D108 (#806); port its rows, worded as "a verse with two alignments to review". Until then the toast counts such verses in one note (B10; DIFFERENCES, a declared difference). |
| #767 | Import creates an empty book | covered | tc-store; C6 | `adding_a_book_from_usfm_is_one_batch_whole_or_absent` | See §1 row 17. The rebuild never refuses a book for its size. |
| 5fefba8 (D104) | tC4 journal pieces up to 8 MiB | reference change | I | `tc4_project_with_a_6_8_mib_book_add_segment_imports_whole` (to create, I) | Port the validator with 8 MiB. |
| #553 | Keep an aligned USFM file's alignments | covered | tc-store (Add a book, C6); I (Update a book) | `aligned_titus_added_as_a_book_has_no_inline_zaln_and_46_records`, `an_aligned_usfm_file_updates_its_alignments` | One function for both paths; coordinate with #794. |
| #484 | Add a book from a USFM file | covered | C6 | see the first #484 row | Built on the #767 command. |
| #499 | Use the typed Bible name | covered | F1, I | `typed_import_name_is_the_project_name` | Leave review-edited fields out of the P9 comparison. |
| #502 | Two USFM files never reach review | unit todo | DONE (after the first release) | J9 `a_two_file_drop_is_one_new_project` (to create with the USFM importer) | Each book in one Import batch. Importing USFM projects waits for after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md). |
| #492 | Guess a language code from the name | covered | B2, C6 | `language_suggestions_rank_as_tc4_offline` | Ship `langnames.json`; never let a suggestion become "Undetermined" (#582). |
| #386 | Keep regional language tags | covered | S1, C6 | `create_keeps_regional_language_tags` (tc-store and tc-app); journey `create` (`language-tags`) | es-419, pt-BR, x-ug and the adjusted `acm-x-lebarabi-c` are kept. |
| #529 | Remove unused checkingLanguage writes | covered | tc-journal; I | `imported_obsolete_checking_language_is_kept_and_ignored` (pending: DONE), as tC4 4.0 import is after the first release | Nothing writes it. |
| #530 | Upload details in a dialog | not applicable | B2 | — | No project keeps "Upload changes" (Q2). |
| #514 | Account menu and asking before internet use | covered | A5, A8 | `the_menu_has_tc4s_rows_in_order_for_signed_out`; `the_keys_move_wrap_and_tab_closes_the_menu` (journey `account-menu`); `internet-consent` case h | Three menu states; Escape returns focus. |
| #559 | One internet on/off state per app session | covered | tc-door43; A8 | `every_call_is_refused_before_anything_is_sent_while_the_switch_is_off` | A8 builds the dialog. |
| #536 | Flaky "nothing auto-committed" journey | not applicable | D1 | D1 `a_save_makes_no_commit` | tC4 journey race. |
| #501 | Opening Check changes `resources.json` | covered | tc-journal; C8 | `projection_ignores_installed_resources` | C8 ports J12's "pins never move without the accept". |
| #506 | QA server label on the card | covered | B2 | `debug_card_names_qa_server` | No top-bar label. |
| #120 | Development builds use QA for account and write calls | covered | A8, B9b (B9a done) | `a_development_build_downloads_resources_from_production` | Only account and write calls use the QA server. B9a: Get source texts reads and downloads from production when the account server is QA (`tc_app::resources::resources_server`). |
| #708 | An ops-log record stays open | covered | tc-store | same test as #707 | Duplicate of #707. |
| #372 | First share to Door43 (J11) | covered | E1b, A8, B9b | `share_pushes_the_checkpointed_main`; `share_name_taken_on_door43_is_refused_before_any_push`; journey `share` | Built (E1b): never force a push; a name in use is refused before any push. |
| #373 | Publish and import journeys | covered | D2, F1, I | J7, J9 and J23 ports: journeys `j07-export`, `import`, `update-book`; `a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project` | Validate every export and import against SB 1.0. D2's half: journeys J7 and J23 (`parity/app/journeys/j07-export.test.mjs`) download aligned and plain USFM and Story Markdown and print the PDF from `/print/{job}`, each leaving every project file but `.git/` unchanged; `every_export_validates` (tc-app) parses every USFM output and validates the project folder. The Burrito zip's validation is E2's; F1's half: journey `import` (J9a: a tC3 project imports as one valid project, or nothing) and `update-book`; I's `a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project` validates the imported project. J9c and J9d (USFM and Burrito import) wait for after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md). |
| #25 | Review competing drafts side by side | unit todo | B11, C7a, C7b | `the_tab_strip_shows_only_when_another_kind_holds_something`; `every_kind_has_its_choices` | B11 built the tabs for every kind, read only, shown only while one beyond Text holds something. C7a gives every kind of the Text tab its choices from one match on the kind; the other tabs answer Later until C7b, after Q-9 is signed off. |
| #46 | A rollback plan for a bad release | unit todo | V, G1 | `older_build_starts_on_newer_app_data` | The downgrade check also covers app data. |
| #23 | Journal store epic | not applicable | — | — | Superseded. |
| #26 | Staged rollout epic | not applicable | — | — | Superseded. |
| #157 | Scenario files for journal behavior | not applicable | — | — | Not planned. |
| #158 | A repository port with adapters | not applicable | — | — | Not planned. |
| #159 | `docs/SYSTEM.md` | not applicable | — | — | Not planned. |
| D108 (#806) | The sync result names each verse where a teammate's alignment replaced yours | reference change | E1a | `two_different_alignments_of_one_verse_are_listed_once` (to create, E1a) | tC4: "{book} {ref}: {name}'s alignment replaced yours." (`syncResult.alignReplaced`), and it keeps only the newest. The rebuild keeps both as a conflict; E1a ports the per-verse rows worded as a verse with two alignments to review, never "replaced yours" (DIFFERENCES). |
| #807 | After a sync, a span save waiting on a refused alignment save blocks Home | covered | E1a, E1d | journey `a_refused_align_save_behind_a_span_save_lets_home_leave` (align) | The rebuild has no alignment-save queue for a span save to wait behind, and the aligner has no text field. The trap was there in another form: the aligner's gate hook held Home for any refused save of the links. A refusal no retry can fix now lets the person leave in one click, as a field's does (#801, a declared difference in DIFFERENCES); closing keeps the links' draft, and the next open offers it (`a_crash_offers_the_aligners_links_with_both_displayed_tokens`, `the_aligners_unsaved_links_are_offered_behind_the_mirror`). |
| #813 | A burrito with no `vrs.json`: every checkpoint refused (D110) | unit todo | V | `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme` (to create; importing outside Burritos and tC4 4.0 projects is cut from the first release, rulings/2026-10-24-H3-H4-H7-fast-release.md, so it waits with V) | tC4 #817 (D110) imports with the English scheme. The rebuild guesses the scheme and names it in the import review (H13). |
| #821 | Recover a project with a book and no versification frame, also from Door43 | unit todo | V | as #813 (to create, V; importing tC4 4.0 projects is cut from the first release, rulings/2026-10-24-H3-H4-H7-fast-release.md); `a_tc4_team_repository_is_blocked_and_nothing_changes` (tc-sync, X) | The rebuild never makes this state. A tC4 4.0 project in it comes in through I's import, or as a teammate's Door43 repository (X). |
| #822 | Detect the versification scheme from the content | covered | tc-versification, tc-app, tc-ui; VRS-GUESS; H13 | `malachi_in_three_chapters_fits_the_hebrew_numbering`, `the_default_settles_a_tie_as_the_addendum_says`, `a_book_in_a_schemes_counts_fits_that_scheme`; `a_tc3_malachi_in_hebrew_numbering_imports_as_org`, `a_tc3_import_writes_the_versification_the_person_chose`, `a_tc3_import_the_verse_counts_do_not_decide_offers_eng_and_says_so`, `update_a_book_keeps_the_projects_versification`; journey `import` (tC3 versification, two cases) | Ruling H13 and its addendum: the tC3 import offers the versification the books' verse counts fit on its review page, says how (fits, did not decide, tied, nearest), and writes the one shown when Import is pressed; Update a book keeps the project's. Declared difference: tC4 gives every import `eng`. The cut import kinds take the same rule when they come back. |
| #835 | Import: accept x- codes the server accepts, refuse the others before Import | covered | F1, I | `import_accepts_x_codes_and_refuses_unusable_codes_before_import` | The vendored hint "such as kau or es-419" (`importer.review.langHint`, `langError`) suggests a code the server refuses; F1 must not show it as is. |
| #837 | Open from Door43: show the chosen name on the new card | reference change | E1c; H14 | `an_open_asks_no_name_and_keeps_the_teams_name`; `opening_a_project_you_have_makes_no_copy_and_offers_yours`; `two_cards_of_one_name_each_say_where_they_live`; `an_open_takes_the_first_free_folder_of_the_repository_name`; journey `open-door43` (J14, J17) | Ruling H14 (rulings/2026-10-26-H14.md): the open asks for no name, so there is no chosen name to lose. The project keeps its `identification.name`; its folder is chosen without asking; two cards of one name each say where they live; a project already here is not opened again, and the dialog offers to open it. Declared differences: tC4 asks for a new name and opens a second copy. |
| #367 | Export checking decisions and alignments as CSV | not applicable | D2 (when tC4 ships it) | — | tC4 left it out of 4.0 (D79). Port it with tC4's columns when it ships. |

## 6. Shell, startup, languages and help

| Issue | Title | Status | Unit or crate | Regression test | Action |
|---|---|---|---|---|---|
| #743 | A window from the click until the main window opens | covered | G1; APP-DESIGN §5.3, A9 D10 | `window_is_painted_within_1_s_of_the_click`; `closing_the_window_while_preparing_leaves_no_tc4_process` | Create the window at Electron `ready`; §5.3 picks the page, not whether a window shows. Fix the §5.3 and D10 wording now. |
| #741 | Help page wordmark | covered | A5 | `every_help_page_has_the_bytes_tc_website_has_at_the_pin` (help sha-list check) | Vendor the help pages byte for byte; vendor again when tc-website ships the fix. |
| #731 | Is the Report a problem attachment translated? | covered | A5; FIX-R | `the_attachment_is_english_in_every_app_language`; `platform_line_names_the_os_and_the_architecture` | Compose it in English, as tC4 does. An optional translated line explaining why is a declared difference. The platform line comes from Rust's OS and architecture constants. |
| #751 | Duplicate work in Windows install and start | unit todo | G1, G2 | G2 budget row `first_launch_under_60_s` (five runs) | Install from archives; measure on the reference laptop with Defender. |
| #740 | First launch in under a minute | unit todo | G1, G2 | keep `repeat_launch_reads_no_resource_content_file`; add the Windows measurement | The code half is covered. The Windows 60 s goal is not shown yet; merge with #751. |
| #730 | Correct 76 Arabic strings | covered | tc-ui | `every_vendored_file_has_the_bytes_tc4_has` | B5a uses `byFigure` after the next vendoring. |
| #729 | The help panel blinks white | covered | A5 | journey `help` cases g, h and i | Port `swapPages`; swap only HTML pages under `help/`. |
| #728 | "Draft section" looks like a button | covered | B4a | pixel and DOM parity of `translate-tit1` | As in §3. The RTL case waits for a right-to-left project, which C6's create makes. |
| #763 | The rc.2 help pages | reference change | REF1 | help case b; `help_home_shows_no_rc1_build_text_offline` (Q31) | Q31 for the banner and install sentences. Also triage #476 (§7). |
| #686 | Bundle Noto Sans Arabic | covered | tc-ui; A5 | port tC4 app-language case j, with a check that the font file answers 200 | None in tc-ui. |
| #657 | Mirror the interface for a right-to-left language | covered | A5, B3, C6 (B4a's half done) | tC4 app-language cases h and i; B4a `app_language_i_scripture_follows_the_project_under_an_arabic_interface` | Window-control room stays on the physical side. B4a: under Arabic the page reads right to left and a left-to-right project's source, translation and verse card read left to right; a right-to-left project under English needs C6's create. |
| #642 | Remove the Linux native title bar | covered | G1 | `linux_window_has_no_native_bar_and_keeps_zoom_and_reload` | As on Windows; owner ruling 2026-10-29 (`docs/DIFFERENCES.md`). |
| #613 | Bundle a Devanagari interface font | unit todo | G2 | `hindi_label_renders_in_the_bundled_face_offline` | No Devanagari face is vendored yet (neither G1 nor G4 bundles one; it waits with G2, whose P7 review covers how each script looks). Declared difference; note that it also changes Hindi's look on Windows and macOS; add the OFL text to the notices. |
| #606 | Third-party licenses in About | unit todo | REL-1 (G3 ships them) | `every_crate_in_cargo_lock_has_a_notice`, `every_bundled_resource_has_an_attribution` | G3 ships THIRD-PARTY-NOTICES.txt beside LICENSE in every installer (crates, NOTICE files, fonts' OFL texts, Electronite, each bundled resource's attribution); About does not list it yet. |
| #569 | Keyboard shortcuts for Understand, Translate and Check | not applicable | later, if tC4 adds them | — | Outside tC4 4.0, which has no such shortcuts; the rebuild matches it (FIX-R reconciled). Port tC4's if it adds them. |
| #525 | Choose the app language automatically | covered | tc-app | `hello_answers_real_state` | C1a adds a relaunch test that the saved language survives. |
| #523 | Collect translation corrections in Weblate | not applicable | tc-ui | — | They arrive with the next vendoring. |
| #278 | Show the build in the app | covered | G3 (A5 and G1 done: G1 passes the build to both builds) | `about_commit_matches_the_release_record` | The smoke test reads About's commit from the installed app on each platform. |
| #167 | Which interface languages | covered | tc-ui | `the_registry_names_the_ten_decided_locales_each_with_a_catalog` | None. |
| #37 | Unused catalog keys | not applicable | tc-ui | — | Optional: check that each rebuild key is used. |
| #29 | Right-to-left and complex scripts end to end | covered | B6, G4 (B4a's IME case and B5b done; G1's shell runs the probe) | Arabic IME Playwright case (B4a: `refresh_under_typed_editor_keeps_text_and_token_through_an_arabic_ime`); Graphite probe; `graphite_shapes_awami_nastaliq_in_the_shell` and `rtl_project_drafts_checks_and_aligns` | Use logical properties where tC4's physical ones break right to left, and record it. |
| #701 | Urdu (Devanagari) direction | covered | tc-resources | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | See §1 row 12. |
| #548 | Dropdown follow-ups | covered | tc-ui | `a_book_matches_by_name_or_code_prefix`; gallery `select-search` | None. |
| #552 | Move the dropdown onto a library | covered | tc-ui | ds-gallery case Select/disabled-while-open in `parity/ds-gallery/diff-report.json` | See §1 row 13. |
| #624 | Link help from translationcore.com | not applicable | — | — | Website only. |
| #611 | macOS manifest hashes before codesign | not applicable | G1 | — | If the release writes binary hashes, compute them after signing. |
| #700 | Cache the bundle in CI | not applicable | — | — | tC4 CI. |
| #546 | Release v4.0.0 | not applicable | — | — | Recapture and sweep again when v4.0.0 is tagged. |
| #42 | Keyboard pass on the Check pages | covered | B5b (B5a done) | port of `check-keyboard.spec.ts` | As in §4. |
| #568 | A keyboard-only user cannot align | covered | B6 | `align_by_keyboard_only` | As in §2. Done by B6, journey `align_by_keyboard_only`: Enter or Space on a bank chip selects it (with Ctrl, Cmd or Shift for several), a card is a Tab stop while words are chosen (`tabindex` 0, else -1 so it can hold the focus) and Enter or Space places the chosen words; the focus stays through the redraw. A declared difference (residual of `align-workspace`). |
| #599 | Keep the mode switch centred | covered | A5; FIX-R | journey switcher_stays_centred_while_the_save_indicator_changes in `parity/app/journeys/shell.test.mjs` | Within 1 px, in en and ar. |
| #505 | No Project settings in the top bar | not applicable | B2 | — | Outside tC4 4.0, which opens Project settings from the Home card only; the rebuild does the same (B2, journey `Settings from Home show the project as its files hold it, and Save waits for them`). FIX-R reconciled. |
| #506 | QA label on the card, not the top bar | covered | B2, A5 | `debug_card_names_qa_server` | As in §5. |
| #120 | Development builds use QA | covered | tc4 | `the_door43_server_defaults_by_build` | A8 and X test that reads still go to production under a QA config. |
| #514 | Account menu | covered | A5, A8 | `the_keys_move_wrap_and_tab_closes_the_menu` (journey `account-menu`) | Rows and order as tC4 `AccountMenu.jsx` at d310bf3. |
| #559 | One internet switch per session | covered | A8, A5 | journey `internet-consent` cases a to h | The server half is covered. |
| #519 | Help and guides, offline first | covered | A5 | journey `help` cases a, b, e and f | Same-origin frame; links to other sites go through the internet question. |
| #520 | About: version and license, offline | covered | A5 | journey `about` case i (About and License) | Ask the owner for the rebuild's copyright line and LICENSE notice. |
| #378 | Ask for help from a refusal | covered | A5, G1 | `feedback_send_without_a_key_answers_not_configured`; `feedback_send_with_a_key_sends_from_the_main_process`; journey `feedback` | The help-desk token stays in the shell's main process. |
| #521 | Report a problem in the account menu | covered | A5 | journey `feedback` cases c to i | Kept-report rules as tC4. |
| #522 | Choose the app language | covered | A5 | journey `app-language` cases a, b, c, d and f (e is B3's and B4a's) | Preview, Apply, Cancel; a typed verse survives the switch. |
| #622 | Add five interface languages | covered | tc-ui | `the_registry_names_the_ten_decided_locales_each_with_a_catalog` | None. |
| #649 | Add Arabic | covered | tc-ui | catalog and tracking tests | A5 ports app-language b2 and g. |
| #612 | Hindi labels keep letter spacing | covered | tc-ui | `vendored_assets`; journey `hindi_home_and_top_bar_keep_normal_letter_spacing` | B11 uses a tracking token where tC4 has a literal value. |
| #614 | Four English literals in design-system props | covered | B4b, C6, A5 (B5b done) | `ds_labels_d_a_verse_numbers_name_follows_the_app_language`; `academy_drawer_close_label_is_in_the_shown_language` | As in §3. |
| #632 | Help links stay in the panel on modified clicks | covered | A5, G1 | `same_origin_help_link_stays_in_app`; `open_external_refuses_the_app_origin` | G1's `openExternal` refuses same-origin addresses. |
| #634 | The Message box has no accessible name | covered | A5 | journey `feedback` case i (#634) | Give the TextArea an id. |
| #639 | Remove the Windows native title bar | covered | G1 | `windows_window_has_no_native_bar_and_keeps_zoom_and_reload` | Keep an application menu for reload and zoom keys. |
| #214 | Hide the macOS window frame | covered | G1 | `macos_window_hides_the_frame_and_centres_the_traffic_lights` | |
| #206 | The server does not stop on quit (macOS) | covered | tc4; G1 | `closed_stdin_quits_within_5_s` and the other lifecycle tests | Fix the SIGTERM race that makes `sigterm_quits_like_the_quit_message` flaky. |
| #284 | Ship or verify the Windows C runtime | covered | G3 | `tc4_exe_imports_no_vcruntime` | Built with `+crt-static`; the release job checks the shipped tc4.exe's imports. |
| #456 | Sign the Windows installer and app | unit todo | REL-1 (G3 built the signing) | `Get-AuthenticodeSignature` reports Valid for each file, after the first signed run | Signing is wired (Azure Artifact Signing through OIDC, environment windows-signing); it waits on owner action A4 (docs/RELEASE.md). |
| #44 | Installers for Windows, macOS and Linux | covered | G3 | `projects_survive`; `about_commit_matches_the_release_record` (smoke on a fresh runner of each platform) | Uninstall keeps projects and app data. |
| #554 | Third-party notices and the licence text | covered | G3 | `every_crate_in_cargo_lock_has_a_notice` | The MIT `LICENSE` and THIRD-PARTY-NOTICES.txt ship in every installer. |
| #640 | Start after an upgrade when a copy differs | covered | tc-resources | as §4 | None. |
| #528 | Each release in its own folder | covered | tc-resources | as §4 | None. |
| #707 | Import during start-up recovery | covered | tc-store | as §5 | None. |
| #746 | Startup recovery before opens | covered | tc-store | `the_start_cleanup_reports_a_join_it_removed_and_hands_out_no_staging_folder_before_it_ends` | None. |
| #708 | Ops-log record stays open | not applicable | — | — | tC4 machinery; duplicate of #707. |
| #801 | Let the user go Home after a refused save | covered | A4b; FIX-R | `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` (tc-ui) | Fixed by FIX-R step 2: a refusal that can never succeed lets Home proceed and keeps the text as a draft (tC4 drops it; declared difference). |
| #702 | Text zoom | covered | G1 | `windows_window_has_no_native_bar_and_keeps_zoom_and_reload`; `linux_window_has_no_native_bar_and_keeps_zoom_and_reload`; `macos_window_hides_the_frame_and_centres_the_traffic_lights` | Keep Ctrl or Cmd with +, - and 0 working. |
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
| #832 | Home badge keeps left to right after Settings says right to left; empty language name | covered | tc-app; FIX-R step 1 | `a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart`; `an_imported_project_shows_its_metadata_language_name_in_settings` | See §1 row 20; fixed by FIX-R step 1 (declared differences #832.1, #832.2). |
| #833 | rc.2 polish: offline update error, Save dialog title, import codes, right-click, source text version | covered | .1 FIX-R step 2; .2 G1, D2; .3 F1; .4 G1; .5 FIX-R step 5 | .1 `an_offline_update_check_shows_one_plain_sentence`; .2 `export_save_dialog_has_a_readable_title`; .3 `an_import_refusal_shows_a_sentence_not_a_code`; .4 `a_text_field_right_click_offers_cut_copy_paste`; .5 none needed | .1 is §1 row 21, fixed by FIX-R step 2. .5 cannot occur: Get source texts lists no version, only "n installed" (FIX-R step 5). .2 to .4 are covered by G1, D2 and F1. |
| #838 | About says "License: MIT" (D111) | covered | REF1 | `parity/app/journeys/about.test.mjs` (case i reads the vendored MIT licence file) | The rebuild is MIT since LIC-1; REF1 re-vendored About's line and dialog from tC4 release/4.0 d25b7871. |
| #839 | The single-instance guard is only in the packaged app | not applicable | G1 | `new_tc4_waits_for_the_old_one_then_names_the_holder` | tC4 rig machinery; the rebuild's guard is the app-data lock. |

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
- **B5b, C4 (Check):** *(done by B5b)* the Check screen's selectable target words come from
  `tc_usfm::target_words`, as the aligner and the export do (FIX-text): `check/item` cuts them with
  `tc_check::selection::target_words`, so an unknown marker's `|attributes` never show as words.
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
- **A4b (now FIX-R and E1a):** a refusal that can never succeed lets Home proceed and keeps the text as a draft;
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
- **B4a:** built: the verse box shows all of `words` (#786); a `\v` after a marker is a row
  (#784); the source pane follows the verse mapping (#131, #118); the "Draft section" pill
  (#728); the one divider (#602); the Arabic IME case (#29) and the interface half of #657.
  Left: a right-to-left project's Translate screen waits for C6's create (#657); what an emptied
  box stores is C2's (#786); the two Titus 1 screens' note titles are residuals until the
  reference moves (C39).
- **B4b:** emptied section save (#600); pin click-then-click (#598); state hints (#604);
  blank lines in a joined span (#575); VerseMarker label (#614); section-save tests (#786).
  *Done by B4b: all six; #786's save of `words` alone is C2's, and #614's other labels are
  B5b's and A5's.*
- **B5a:** visible progress bars (#764); resume (#518, #513); keyboard pass (#42); opening
  Check never moves pins (#501). *Done by B5a (#42's picker, dialog and rail parts); #769 waits
  for the reference move.*
- **B5b:** drag selection (#737); ItemNavButton (#726); new draft on return (#532); keyboard
  pass (#42); article panel (#619); Drawer label (#614); right-to-left case (#29).
- **B6:** NFC card labels (#768); no Clear, and Clear drops suggestions (#758); keyboard
  placement (#568); x-ref per verse (#409); marking row (#726); invalid after a UGNT upgrade
  (#722); the inline-markup guard (#553); right-to-left case (#29). *Done by B6: #768, #758,
  #568, #726, #735, #736, #753, #794 (`a_verse_that_holds_alignment_markup_does_not_open` through
  `App::request`), #787 and #790 in the open aligner; the rail and the Align card use #796's rule
  with the pin's sha (#722). #722's Align half (`ugnt_upgrade_marks_1_1_invalid_in_align`, after
  C8's upgrade) and #29's right-to-left project (`a_right_to_left_project_aligns_with_its_bank_right_to_left`,
  through C6's create), and #409's span with x-ref in the open aligner
  (`an_imported_span_with_x_ref_opens_with_every_link`); its writing waits for D2 and I.*
- **B7:** a story project opens at story 1 (#507). *Done by B7. #549 waits for a project that carries pictures; #769's fold headings for the stories' groups wait for the reference move. Story-check Previous/Next follow the resource's item order, matching tC4; moving frame focus or clicking a help card keeps an expanded note expanded, as tC4 does.*
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
  the review is open, and a failed open keeps it (#724). *Done by B11; C7a and C7b keep the
  choices beyond Keep this version.*
- **C1a:** creation pins at first open (#766).
- **C2:** a save replaces `words` only (#786); Resume banner after a span save (#576).
- **C4:** re-read the draft on return (#532). *Done by C4: the story half (journey `check_returns_draft_a_frame_drafted_in_translate_shows_on_the_return`) and the late draft with no navigation (`a_check_shown_before_its_verse_has_text_shows_the_text_when_it_arrives`); B5b did the Bible half.*
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
  *Done by D2: #796's export rule, kept verses (#553), no x-ref written (#409), the `\add`
  span kept (#420) and journeys J7 and J23 (#373, `j07-export.test.mjs`). Left: x-ref once
  tC4 defines it (#409), the Burrito zip (E2).*
- **E1a:** J1b one-window route (#801); story cards sync (#761); J26 through the UI (#681);
  the button flow (#678); a failed branch lookup is a failed sync (#724); one test per
  guarantee (#24); rows for verses with two alignments (#791).
- **E1b:** creation pins at first open (#766); stories (#761); J17 (#677); the done step
  with the team-access line for every project, OBS included (#724); J11 (#372).
- **E2:** audio through an exchange file (#705).
- **F1:** singular keys (#577); typed name (#499); two-file drop (#502); split aligned files
  (#553); creation pins (#766); J9 (#373).
  *Done by F1: singular keys (#577), typed name (#499), x- codes (#835), x-ref kept (#409), the tC3 import journey and Update a book (#373). Left for after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md): the two-file USFM drop (#502) and aligned USFM project import with its kept-verse list (#553).*
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

Added by SWEEP (2026-10-23):

- **FIX-R:** #825, #830, #832, #833 (.1, .5), #836, #831, #801 (save machine), #795's hash test.
- **T:** #812 corpus test; #820 vector and proptest; #794 store test; #789 section save.
- **D2:** D105 export notice (`cc.exportSkipped`); #790 export fit; #796 (export). *Done by D2; #790's fit is declared: the export names such a verse and weaves none of it (DIFFERENCES).*
- **E1a:** D108 rows (#806, #791); `e2e/sync-refusal.spec.ts` port (#801); #807 recorded.
- **E1c:** #837 (H14). **F1:** #835, #833.3. **G1:** #833.2, #833.4. **I:** #813, #821, #822 (H13).
- **REF1:** #810 and #829 (H7); #794's and D105's strings; #784 vectors; D107.4. **OLH-1:** #840 (built now, Q39).
  **AQ1:** #720.
- **docs:** rewrite APP-DESIGN Q-20 (#786); fix §5.3 and A9 D10 (#743): done by INT-7.
