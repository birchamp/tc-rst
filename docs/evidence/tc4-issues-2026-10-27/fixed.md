# Recently fixed tC4 issues, tested against the rebuild

- Date: 2026-10-27. Rebuild: main `58e8e10b` (branch `tc4-fixed-issues-2026-10-27`).
- Asked by: the owner, through the audit session: "test the rebuild against tC4 issues that were
  fixed recently, so it doesn't repeat bugs tC4 already fixed." Report only: no product code changed.
- Scope: 4.1.0 as ruled in `.audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md`. Importing
  outside Scripture Burritos, outside USFM projects and tC4 4.0 projects is cut, so is Aquifer.
- Method: one new test per reachable issue, written fresh from the tC4 issue's own steps, through
  each crate's public API on `fixtures/` data. Earlier mapping in `docs/TC4-ISSUES.md` (2026-10-23)
  was used as a lead only. Each test asserts what tC4's fix requires. A test that fails on the
  rebuild is kept with `#[ignore]` and a line in `scripts/ignored-tests.txt`, so the gate stays green.
  Two real-app journeys on the sample project (Titus) add screenshots.
- Test output: `output/` (one file per area; `final-run.txt` is the last run of every new test).

## Summary

| Verdict | Issues |
|---|---|
| **Same bug** | **#791**, **#820** (app layer) |
| Fixed in rebuild | #765, #766, #767, #768, #769, #773, #782, #784, #786, #787, #788, #789, #790, #792, #793, #794, #795, #796, #801, #807, #812, #823 |
| Not reachable | #813 |

## The table

| tC4 issue | Reachable in 4.1.0? | Test (file) | Result | Verdict |
|---|---|---|---|---|
| #765 tN/tW list titles show Hebrew for maqaf quotes | Yes: Check rail and detail titles | `tc4_765_maqaf_quotes_show_english_titles_in_the_genesis_1_lists` (tc-check) | pass | Fixed. 0 Hebrew titles over every tN and tW check of Genesis 1 (en_tn, en_twl v91; ULT `GEN-1.usfm`); xxe6 "Then there was evening and there was morning", xfx7 "And it was so", tW 1:28 vzz2 "the earth"; non-maqaf titles unchanged. |
| #766 a project with no pins gets the creation pins | Yes, through Open from Door43 | `tc4_766_a_project_opened_from_door43_with_no_pins_gets_the_creation_pins` (tc-app, `_sync_store`) | pass | Fixed. A repo with no pins opens with the creation pins; a repo with pins keeps them. tC3 import and New project use the same open (code read, not run). |
| #767 an empty book after the size warning | Yes: Add a book from USFM | `tc4_767_a_book_over_4_mb_is_added_whole_never_empty` (tc-app) | pass | Fixed. The 5.1 MB Psalms file is added whole: 150 chapters, every verse has text, no `___`. |
| #768 Align card labels lost after a save (NFC Hebrew) | Yes | `tc4_768_genesis_1_1_saved_with_nfc_hebrew_reopens_with_every_card_and_link` (tc-align) | pass | Fixed. The NFC-saved Gen 1:1 record reopens with all 7 cards and every link. The label itself (`Gateway::label`) is private to tc-app; its in-crate test `gateway_label_survives_an_nfc_record` was not rerun. |
| #769 fold and unfold the Check sidebar groups | Yes | `tc4_769_by_figure_groups_fold_and_the_fold_survives_a_restart` (tc4); journey `tn_by_figure_headings_fold_and_count` (`parity/app/journeys/check-keyboard.test.mjs`); screens `screens/769-*.png` | pass, pass | Fixed. One group per tA module ("Abstract Nouns"), resolved/total counts, folds, the active group stays open, Fold all → Unfold all, fold survives a restart, By verse has no fold. In the real app: Next into a folded group unfolds it, arrow keys reach the heading. |
| #773 a reordered quote picks up other words | Yes | `tc4_773_reordered_quotes_title_and_light_only_their_own_phrase` (tc-check) | pass | Fixed. Gen 33:5 mix3, 24:48 gyfv, 9:11 r2u9 get tC4's fixed titles; the highlight has no word outside the phrase. |
| #782 words inside nested alignments dropped | Yes: Add a book from aligned USFM | `tc4_782_align_and_translate_show_the_heavens_in_genesis_1_1` (tc-app) | pass | Fixed. Gen 1:1 reads "…created the heavens and the earth."; every `\w` word of all 31 verses is in Translate; each Align rail row equals Translate's text. |
| #784 `\v` after `\q1` on the same line missing | Yes | `tc4_784_psalms_verses_after_q1_are_in_translate_and_align` (tc-app) | pass | Fixed. en_ult Psalms: Translate and the Align rail each list 2,461 verses, all with text. (Gen 3:15 is in no fixture; Psalms has the same `\q1 \v N` form.) |
| #786 a multi-line poem verse saves only its first line | Yes | `tc4_786_saving_a_poem_verse_replaces_every_line_of_it` (tc-app) | pass | Fixed. Ps 3:1 (Verse editor) and 3:2 with `\qs Selah` (Section editor) show every line and a save replaces the whole verse; bytes outside are unchanged. |
| #787 a changed verse opens with its old bank | Yes | `tc4_787_reopen_after_siervo_became_esclavo_banks_esclavo_not_siervo`, `tc4_787_a_never_saved_titus_1_1_opens_with_its_bank_in_verse_order` (tc-align) | pass, pass | Fixed. The bank has "esclavo", not "siervo"; opening writes nothing; the "draft changed" notice shows while a link stands; a never-saved bank is in verse order. |
| #788 a card click places a word a suggestion moved | Yes | `tc4_788_a_card_click_places_no_word_a_suggestion_moved` (tc-align) | pass | Fixed. The click places nothing, cards are not armed, drag and click carry the same words. |
| #789 a link moves to another copy of a repeated word | Yes | `tc4_789_translate_save_deleting_de_dios_after_siervo_keeps_dios_on_its_own_theou` (tc-store); `tc4_789_dios_never_moves_to_the_other_theou_card_when_the_text_changes_outside_align` (tc-align) | pass, pass | Fixed: no link moves. The remaining Dios stays on Θεοῦ² and the store lists the dropped Θεοῦ¹ link. That list does not reach the translator in Translate (see #820); here the dropped link's word was deleted, so less is lost. |
| #790 stored source cards never compared with the pin | Yes | `tc4_790_sample_titus_1_1_shows_all_17_ugnt_words_and_a_foreign_source_link_is_listed` (tc-align); screen `screens/790-titus-1-1-align.png` | pass, pass | Fixed. Titus 1:1 shows all 17 UGNT words in order, in the test and in the real app; a link to a word the pin lacks is dropped, listed and needs review. |
| #791 tell a translator a teammate's alignment replaced theirs | Yes: Door43 sync | `tc4_791_after_sync_the_translator_is_told_a_teammates_alignment_replaced_theirs` (tc-app, `_sync_store`), **ignored** | **fail**: `replaced: []`, `kept_newest: 0` on both computers | **Same bug.** Two computers align Titus 1:1 differently and sync through the Door43 fake. The fold holds both heads, but neither sync result says what was replaced, which ruling H5 requires ("the Received sheet says what was replaced"). Cause: `App::door43_sync` calls `synced_view(&snapshot, me, &[], …)` with no received change ids (`crates/tc-app/src/sync.rs:411`), so the replaced rows and the newest-wins count are always empty after a real Door43 sync. USB exchange and the test-hook sync pass the ids. |
| #792 a span made while an alignment save waits leaves orphans | The late save is reachable in the store; the UI's 2-second timer was not driven | `tc4_792_a_late_alignment_save_after_a_span_leaves_no_orphan_alignment` (tc-store) | pass | Fixed. Align 1:2, join 1:1-2, then the late alignment save: nothing is detached, before or after reopening. |
| #793 Hebrew source words in two Unicode forms miss suggestions | Yes | `tc4_793_memory_trained_on_nfc_jonah_suggests_for_fresh_raw_hbo_uhb_verses` (tc-align) | pass | Fixed. Raw hbo_uhb words get as many suggestions as NFC words. |
| #794 aligning over imported inline `\zaln` | Yes: Add a book from aligned USFM | `tc4_794_titus_1_1_holding_inline_zaln_opens_no_aligner_and_reflows_nothing` (tc-align); `tc4_794_export_keeps_the_19_inline_zaln_of_titus_1_1_with_a_matching_record` (tc-export) | pass, pass | Fixed. The verse does not open in Align, the reflow is refused, and the export keeps all 19 `\zaln-s` and names the verse. |
| #795 the client and the export hash a verse differently | Yes | `tc4_795_an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen` (tc-store); `tc4_795_a_psalm_verse_with_qs_selah_aligned_in_align_exports_woven` (tc-export) | pass, pass | Fixed. An NFD-typed aligned verse is valid after reopen and its stamp equals the stored verse's hash; Ps 3:2 with `\qs Selah\qs*` exports woven. |
| #796 the bank and the export use different words; one review rule | Yes | `tc4_796_footnote_after_siervo_banks_two_words_and_the_aligned_verse_weaves` (tc-align); `tc4_796_a_footnote_right_after_siervo_exports_aligned_and_keeps_the_footnote`, `tc4_796_rail_notice_and_export_give_one_answer_for_each_review_case`, `tc4_796_mark_valid_with_nothing_placed_exports_the_stored_bytes` (tc-export) | all pass | Fixed. "siervo\f…\f* de" banks two words and exports aligned with its footnote; the rail, the notice and the export agree in every case. One declared difference: a record that lost a placed word does not need review in the rebuild (its links still hold; the export weaves the verse with that word unaligned, nothing dropped). tC4 flags it. |
| #801 after a sync, a refused save shows the plain sentence and Home leaves | Yes, but the "old store" state does not exist: a sync joins the open session | `tc4_801_a_save_after_a_sync_is_kept_and_home_leaves` (tc-app, `_sync_store`) | pass | Fixed by design. A save in the open from before the sync is written, the teammate's verse arrives, and Home leaves on the first press. |
| #807 a span save waiting on a refused alignment save blocks Home | The window does not exist: no alignment-save queue for a span save to wait behind | `tc4_807_a_span_save_after_a_sync_does_not_block_home` (tc-app, `_sync_store`) | pass | Fixed. Align saves around a sync, then a 1:1-2 span save: written, and Home leaves. The wasm aligner gate hook in tc-ui was not driven. |
| #812 a mid-verse `\p`, `\ts\*`, `\q3`, `\pi`, `\pmo` line hides the rest | Yes | `tc4_812_verse_with_a_mid_verse_marker_shows_and_saves_whole`, `tc4_812_no_change_save_leaves_the_book_byte_for_byte` (tc-app) | pass, pass | Fixed. On the `*-812` extracts (1SA 2, MAT 27, REV 7, PSA 9) the editor holds every word; a save replaces the whole verse; a no-change save leaves the book byte-identical. |
| #813 a Burrito import with no `vrs.json` refuses every checkpoint | **No**: the Scripture Burrito import card is disabled ("later") in `crates/tc-ui/src/views/modals/import.rs`, per the fast-release ruling | — | — | Not reachable. Its reachable cousin is #823. |
| #820 an ambiguous repeated-word edit keeps a link without review | Yes | `tc4_820_dios_dios_first_linked_saved_as_dios_pablo_keeps_no_unsure_link` (tc-align); `tc4_820_translate_save_of_dios_dios_as_dios_pablo_keeps_no_unsure_link` (tc-store); `tc4_820_a_translate_save_that_sends_a_link_back_to_the_bank_tells_the_translator` (tc-app), **ignored** | pass, pass, **fail**: `nothing tells the translator the Dios link went back to the bank: dropped [], freshness Current` | **Same bug, in another form.** No wrong link survives: the reflow drops the unsure link and the store lists it in `StoreReply.dropped_links`. But tc-app never reads that list (`reply()` in `crates/tc-app/src/backend/store.rs` has no such field to fill, and `tc_api::write::Ack` has none), so after saving "Dios Dios" as "Dios Pablo" in Translate, the translator's link is gone with no message, and Align on reopen shows 0 placed, no notice. tC4 flags the verse for review; the rebuild loses the work silently. FIX-R's log (`.audit/roadmap/units/FIX-R.md`, 2026-10-23) left this Translate half waiting for S2; its test `a_link_sent_back_to_the_bank_is_listed` was never written. |
| #823 an open from Door43 with no frame or no `vrs.json` refuses checkpoints | Yes (a repo written by the rebuild) | `tc4_823_a_project_with_no_versification_frame_opened_from_door43_still_checkpoints`, `tc4_823_a_repository_without_vrs_json_opened_from_door43_still_checkpoints` (tc-sync) | pass, pass | Fixed. After the join: save, leave-Translate checkpoint, save, leave-project checkpoint all commit, and the next sync works. |

## Seen in passing, not tested

- Align left open across a Translate save: in a first draft of the #820 app test, without
  `align/close` before the Translate save, reopening Align still showed the old "Dios Dios" verse
  after Translate had stored "Dios Pablo". Whether the UI can leave the aligner open while the person
  edits in Translate was not checked.

## Files

- Tests: `crates/{tc-align,tc-store,tc-export,tc-check,tc-sync,tc-app,tc4}/tests/tc4_fixed_2026_10_27.rs`,
  `crates/{tc-store,tc-app}/tests/tc4_fixed_2026_10_27_sync_store.rs`.
- Journey screenshots: `screens.test.mjs` (`TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test docs/evidence/tc4-issues-2026-10-27/screens.test.mjs`),
  `screens/769-by-figure-open.png`, `screens/769-fold-all.png`, `screens/790-titus-1-1-align.png`.
- Ignored repros: two lines in `scripts/ignored-tests.txt` (#791, #820). Run them with
  `cargo nextest run -p tc-app --run-ignored only -E 'test(/^tc4_(791|820)_/)'`.

## Limits

- #765 is checked on Genesis 1 (the fixture), not all of Genesis.
- #765 and #773 rebuild the ULT pane's aligned verse the way tc-app's private `sources::aligned`
  does (about 20 lines copied), not through tc-app itself.
- #791's two computers sign in as the same Door43 user, so the test does not check who is named.
