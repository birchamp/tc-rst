# SWEEP step 3 (commits): tC4 9ad4d5c to main 24188928 and release/4.0 d25b7871

- Date: 2026-10-23. Read-only. Sweep clone `~/unfoldingword/tc4-sweep`; rebuild worktree
  `.worktrees/track-SWEEP` (cadf967d).
- Ranges: `git log --first-parent 9ad4d5c..origin/main` = 19 merges (#785 to #838);
  `git log 9ad4d5c..origin/release/4.0` = 40 commits.
- Statuses: the six of `docs/TC4-ISSUES.md` "Status key". "Row" = a `| #<n> ` row exists in
  `docs/TC4-ISSUES.md` today for the PR or its issue.
- Every test marked "exists" was found with `git grep` in `crates/` or `parity/`. "(to create)"
  = no such test today. Nothing below was reproduced by running code, except where it says so;
  "read" = read from code.

## 1. tC4 main: 19 first-parent merges

| # | Ref | What a translator sees in tC4 | Status | Owner card | Regression test | Action | Row |
|---|---|---|---|---|---|---|---|
| M1 | 371ddd9e · PR #785 · issue #784 | A `\v` right after `\q1` on its own line, or `\p\v N`, is a verse in Translate and Align; a section save keeps the `\q1`. | covered | none (B4a done) | exists: `the_check_text_is_what_tc4_reads` (tc-usfm/tests/derivations.rs), `a_verse_right_after_a_marker_is_listed_in_its_paragraph` and `verse_starting_on_its_own_poem_line_opens_with_its_text` (tc-app/tests/app/translate.rs), `every_aligned_ult_verse_opens_in_verse_order_with_nothing_to_save` (tc-align/tests/psalms.rs) | Row :150: drop "Declared difference until tC4 merges PR #785" (merged). | yes (#784 :150, :162) |
| M2 | c7b60d2b · PR #804 · issue #792 | A Retry after a join can no longer publish orphan alignment records (tC4's align queue drains before a span save). | not applicable | none | none (the rebuild has no book-wide align save queue; `git grep` for an align queue in tc-app finds none) | None; row :132 already says so. | yes (#792 :132) |
| M3 | 67ddf3f6 · PR #805 · issue #793 · D109 | Hebrew source words are NFC, so suggestions find words saved earlier. | covered | FIX-R (row reconcile) | exists: `memory_trained_on_nfc_records_suggests_for_raw_source_words` (tc-align/tests/tc4_suggest.rs) | Rows :60 and :131 still say "rebuild bug": set covered (FIX-R step that reconciles rows). | yes (#793 :60, :131) |
| M4 | b044f36e · PR #806 · issue #791 · D108 | The sync result names each verse where a teammate's newer alignment replaced yours: "{book} {ref}: {name}'s alignment replaced yours." Notice only. | reference change | E1a (REF1 for the PLAN wording) | exists: `a_verse_with_two_alignments_is_named_and_offers_review` (tc-ui/tests/app/sync.rs), `the_toast_counts_verse_conflicts_alignments_and_the_rest_apart` (tc-app/tests/app/sync.rs); `two_different_alignments_of_one_verse_are_listed_once` (to create, E1a; row :312 names it as if it existed) | The rebuild keeps both alignments as a conflict with Review (declared difference). E1a must not show "replaced yours". Rewrite PLAN :151-155 ("can disappear unannounced" is no longer true of tC4 main). | yes (#791 :133, :312) |
| M5 | 80839429 · PR #799 · issue #794 | A verse added from a file that already holds alignments does not open in Align ("This verse already holds an alignment"); the export keeps its markup. | covered | T (store test), D2 (export), REF1 (strings) | exists: `a_verse_that_holds_alignment_markup_does_not_open` (tc-align/tests/tc4_reconcile_open.rs, tc-app/tests/app/align.rs); `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (to create, T step 4; row :59 names it as if it existed); `export_keeps_inline_zaln_bytes_even_with_a_matching_record` (to create, D2) | The rebuild shows its own key `align.unavailable.alignedInText` (tc-ui/assets/i18n-rebuild/en.json:5-6); tC4 now ships `align.unavailable.inline-alignment.*` in 10 languages: REF1 adopts tC4's keys. PLAN :111-113 "tC4's open PR #799": merged. | yes (#794 :59, :129) |
| M6 | be58feb4 · PR #803 · issue #801 | After a sync replaced the store, a refused save shows one plain sentence and Home leaves (646-line `e2e/sync-refusal.spec.ts`). | unit todo | E1a step 4 (port), FIX-R step 2 (save machine) | `stale_scope_save_keeps_text_and_home_leaves_in_one_click` (to create, E1a); `parity/app/journeys/sync-refusal.test.mjs` (to create, port of J2 :215, J6 :409, J1b :514); `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text` (to create, FIX-R) | Read: `gate()` returns Blocked for any field that failed after its gate retry (`crates/tc-ui/src/app/save/mod.rs:1211-1214`), as E1a's card says; not reproduced, and sync is off in release builds, so no reachable "rebuild bug" scenario is named here. Add 095144ce and the spec to rows :282/:403. | yes (#801 :282, :403) |
| M7 | ef86c2d4 · PR #808 · issue #807 | A span save queued behind a refused alignment save carries the sync refusal, so Home leaves. | not applicable | E1a (row) | none for the mechanism; the outcome is #801's tests (M6) | The rebuild has no alignment-save queue for a span save to wait behind (read; no queue in tc-app). Add a row for #807 that points at M6's tests (E1a step 6 expects it). | no |
| M8 | b120a884 · PR #800 · issue #795 | No "brotherwill": the space after a footnote stays. Align stamps the export's own hash. New notice "tC4 cannot save an alignment for this verse … move it onto the same line." | covered | FIX-R (hash test), REF1 | exists: `a_footnote_between_words_keeps_its_space` (tc-usfm/tests/shown_text.rs), `imported_mat_5_22_opens_with_all_44_links` (tc-align/tests/bank_words.rs); `an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen` (to create, FIX-R; row :128 says "to add") | `align.unavailable.no-slot`: the rebuild has no such state; a verse whose text starts on the next line opens (`verse_starting_on_its_own_poem_line_opens_with_its_text`). Record a declared difference (tC4 refuses; the rebuild aligns). PLAN :73 "tC4's open PR #800": merged. | yes (#795 :55, :128) |
| M9 | 60dcd5ff · PR #809 · issue #796 slice 1 · D105.1 | The Align word bank holds the words the export checks (no "siervode" bank word). | covered | FIX-R (row names) | exists: `the_bank_is_the_export_for_every_vector`, `the_bank_is_the_export_for_every_verse_of_the_bundle_and_the_fixtures`, proptest `the_bank_is_the_export_for_generated_verses` (tc-align/tests/bank_words.rs) | Row :125 names `a_footnote_right_after_a_word_banks_both_words_and_weaves`, which does not exist: replace with the bank_words names (CHK-3 would flag it). | yes (#796 :57, :125) |
| M10 | f9ed80b9 · PR #811 · issue #796 slice 2 | Suggestions use the same verse words as the word bank. | covered | none | exists: `proposals_are_bank_words_each_once_on_the_card_of_the_source_and_never_a_placed_word`, `a_placed_word_the_text_no_longer_holds_is_not_a_training_link` (tc-align/tests/tc4_suggest.rs). Inference: the bank is `target_words` (row :57), so proposals use the export's words. | Add #811 to the #796 word-list row. | yes (via #796) |
| M11 | e58b0c1e · PR #814 · issue #796 slices 3-6 · D105 | One review rule for the rail, banner, Mark valid, tC3 import and export. The export leaves out a verse that needs review and names it: "Your alignment of these verses is not in the file. Check them in Align: {refs}". An edit never clears the review flag; only Mark valid does. | reference change | D2 (export), I (tC3 import), REF1 (flag rule) | exists: `a_record_holding_a_word_the_verse_lacks_is_invalid_on_the_rail`, `after_a_source_upgrade_the_open_verse_shows_the_notice` (tc-align/tests/tc4_reconcile_open.rs; row :58 calls it `..._shows_the_banner`); `export_reports_and_keeps_a_verse_that_needs_review`, `a_record_with_nothing_placed_exports_the_stored_bytes` (to create, D2); `a_tc3_alignment_whose_words_do_not_fit_is_marked_for_review` (to create, I) | D105.5: the rebuild clears the flag on the first edit (`an_edit_clears_the_re_review_flag_tc3_or_tc4_set`, tc-align/tests/tc4_edit.rs:597; `a_record_flagged_for_re_review_opens_as_it_is_and_the_first_edit_clears_the_flag`, tests/aligner.rs:341). Row :126 says keep that "until the owner rules otherwise": see §4. PLAN :102-105 ("tC4 shows the notice only for a changed text…") is now wrong. | yes (#796 :58, :126, :127) |
| M12 | b1911407 · PR #815 · issue #789 · D106 | After a Translate save a link follows its word; an insert or delete before a linked repeated word marks the verse for review. | covered | T step 5, FIX-R step 2 (show the dropped list), REF1 (difference) | exists: `deleting_an_earlier_copy_keeps_the_link_on_its_physical_word`, `inserting_an_earlier_copy_renumbers`, `delete_then_readd_never_reattaches_an_old_link`, `without_the_earlier_text_a_link_never_moves_to_a_retyped_copy_of_its_word` (tc-align/tests/tc4_reflow.rs), `a_link_follows_its_own_copy_of_a_repeated_word` (tc-store/tests/store/reflow.rs) | The rebuild follows by position too, but sends an unclear link to the bank and lists it, with no review mark (owner Q30 (a)); tC4 marks for review. Rewrite PLAN :91-95 ("tC4 moves the link … unannounced" is wrong now). Rows :62, :135 to covered (T step 5). | yes (#789 :62, :135) |
| M13 | 4b93ac14 · PR #816 · issue #790 · D107 | Align repairs source cards from the pinned verse (TIT 1:1 shows all 17 Greek words); the aligned export fits cards before weaving. | covered | D2 (export fit), REF1 | exists: `the_sample_opens_with_all_17_ugnt_words`, `a_link_to_a_source_word_the_pin_lacks_is_dropped_and_shown`, `opening_a_repaired_verse_writes_nothing`, `an_imported_nfc_hebrew_record_keeps_its_links`, `a_done_record_with_a_source_word_the_pin_lacks_is_invalid_on_the_rail_and_opens_stale` (tc-align/tests/tc4_reconcile_open.rs); `the_export_weaves_no_link_to_a_source_word_the_pin_lacks` (to create, D2) | Rewrite PLAN :96-99 ("tC4 shows the stored cards as they are"). Difference to note in REF1: D107.4 flags a `done` record that only gains empty cards; the rebuild's `needs_review` checks that cards stand on the pin, not that they cover it (read: `crates/tc-align/src/value.rs:108-119`). No reachable scenario confirmed. | yes (#790 :61, :134) |
| M14 | 51b94071 · PR #818 | Nothing (a source file writes `\u0000` instead of a literal NUL). | not applicable | none | none | None. | no |
| M15 | 27498dea · PR #817 · issue #813 · D110 (#821, #822) | A Scripture Burrito with no `vrs.json` imports with the English scheme, and its checkpoints are no longer refused. | unit todo | I (owner H13 for #822) | `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme` (to create, I step 1); `a_burrito_without_vrs_json_imports_with_a_named_scheme` (to create, I) | The rebuild guesses the scheme and says which (H13 recommendation) where tC4 uses English. Add rows #813/D110, #821, #822. | no |
| M16 | c4559662 · PR #810 · issues #786, #812 | Translate: a poetry verse is one body over all its lines; poem lines show as tabs; a mid-verse `\p`, `\b`, `\ts\*`, `\q3` or heading line no longer cuts the verse. | reference change | REF1 (owner H7, tabs); T step 4 (#812) | exists (#786): `verse_editor_shows_every_poem_line` (tc-ui and tc-app tests), `no_change_section_save_on_psalm_3_writes_nothing` (tc-app/tests/app/section.rs); `every_bundled_ult_and_ust_verse_body_holds_every_target_word` (to create, T step 4, #812) | Read: `split_verse` keeps everything before the trailing layout lines in `words` (`crates/tc-usfm/src/codec.rs:130-141`), so #812 does not happen here; the test proves it. Tabs follow H7 (recommendation: keep markers). Add rows #810 and #812. | yes (#786 :161); no (#810, #812) |
| M17 | b1d1f377 · PR #824 · issue #820 | "Dios Dios" with one Dios linked, saved as "Dios Pablo": the verse is marked for review. | unit todo | T step 4 (tests), FIX-R step 2 (show the list) | `replacing_one_copy_of_a_repeated_linked_word_keeps_no_unsure_link`, proptest `follow_matches_every_longest_common_subsequence` (both to create, T step 4) | Inference from `crates/tc-align/src/reflow.rs:8-16`: a word goes where every longest common subsequence puts it, else its link goes to the bank and is listed, so this vector should drop and list the link (no review mark, Q30 (a)). Not run. Add row #820. | no |
| M18 | b86270ff · PR #827 | Nothing (tC4 decision log: rc.2 cut at b1d1f37). | not applicable | none | none | None. | no |
| M19 | 24188928 · PR #838 · D111 | About shows "License: MIT"; the License dialog shows only `LICENSE`. | not applicable | none | none | The rebuild is GPL-2.0-or-later (`Cargo.toml:11`, `COPYING`, `about.licenseName` in `crates/tc-ui/assets/i18n/*.json:55`). Its vector generators pin the GPL `string-punctuation-tokenizer` 2.0.0 (dev tools only: `crates/tc-align/tools/gen-align-vectors.cjs:37`, `crates/tc-check/tools/gen-check-vectors.cjs:25`). A license choice for the owner, not a parity row; see §4. | no |

## 2. tC4 release/4.0: 40 commits

"Carries" = a cherry-pick of that main PR. Commits a5ce0307 (#819), 1cdf350c (#826), 90b8d6e1
(#780) and d25b7871 (#841) are merges of picks that are also listed one by one, so they count as
process (not applicable) to avoid counting a change twice.

| # | Ref | Carries | Status | Owner card | Regression test | Action | Row |
|---|---|---|---|---|---|---|---|
| R1 | d25b7871 · PR #841 | merge of R3 (main #838) | not applicable | none | none | None. | no |
| R2 | 3a11a43b · PR #834 | release-only (rc.2 notes: published, checksums) | not applicable | none | none | Card lists #834 as not applicable: confirmed (docs only). | no |
| R3 | cf9421f2 | main #838 (D111) | not applicable | none | none | As M19. | no |
| R4 | fca275be | release-only (inside #834) | not applicable | none | none | None. | no |
| R5 | 9f06f99c · PR #828 | release-only (rc.2 build evidence) | not applicable | none | none | Card lists #828 as not applicable: confirmed. | no |
| R6 | 4ad06ca3 | release-only (inside #828) | not applicable | none | none | None. | no |
| R7 | a5ce0307 · PR #819 (tag v4.0.0-rc.2) | merge of picks #783 to #816 (R9-R26 below) | not applicable | none | none | The rc.2 tag is here (H7). | no |
| R8 | 1cdf350c · PR #826 | merge of picks #818, #817, #810, #824 | not applicable | none | none | None. | no |
| R9 | 8b108a4c | main #824 (#820) | unit todo | T, FIX-R | as M17 (to create) | As M17. | no |
| R10 | f7e726b3 | main #810 (#786, #812) | reference change | REF1 (H7), T | as M16 | As M16. | partial |
| R11 | c6fb46e6 | main #817 (#813, D110) | unit todo | I (H13) | as M15 (to create) | As M15. | no |
| R12 | 3bc68e04 | main #818 | not applicable | none | none | None. | no |
| R13 | b5203aa8 | main #816 (#790, D107) | covered | D2, REF1 | as M13 | As M13. | yes |
| R14 | f8e0f563 | main #815 (#789, D106) | covered | T, FIX-R, REF1 | as M12 | As M12. | yes |
| R15 | 7baa1257 | main #814 (#796, D105) | reference change | D2, I, REF1 | as M11 | As M11. | yes |
| R16 | e67c94da | main #811 (#796) | covered | none | as M10 | As M10. | yes |
| R17 | 4b636583 | main #809 (#796) | covered | FIX-R | as M9 | As M9. | yes |
| R18 | 47fceb39 | main #800 (#795) | covered | FIX-R, REF1 | as M8 | As M8. | yes |
| R19 | 95f33191 | main #808 (#807) | not applicable | E1a (row) | as M7 | As M7. | no |
| R20 | f66a2fc2 | main #803 (#801) | unit todo | E1a, FIX-R | as M6 (to create) | As M6. | yes |
| R21 | 7ddd1962 | main #799 (#794) | covered | T, D2, REF1 | as M5 | As M5. | yes |
| R22 | 8640a1af | main #806 (#791, D108) | reference change | E1a | as M4 | As M4. | yes |
| R23 | 21eb9416 | main #805 (#793, D109) | covered | FIX-R | as M3 | As M3. | yes |
| R24 | e0de25b9 | main #804 (#792) | not applicable | none | none | As M2. | yes |
| R25 | 8131ceb7 | main #785 (#784) | covered | none | as M1 | As M1. | yes |
| R26 | bb16ccbc | main #797 (#788; DELTA-10-19 C50) | covered | none (B6 done) | exists: `only_the_shown_selection_is_placed_and_the_cards_look_armed_only_while_it_is_not_empty` (tc-align/tests/tc4_selection.rs) | None. | yes (#788 :136) |
| R27 | cb1ef532 | main #798 (#787; C47, C48) | covered | FIX-R (row) | exists: `a_stale_value_opens_with_the_draft_changed_notice` (tc-align/tests/tc4_reconcile_open.rs) | Row :137 still says rebuild bug; §1 row 4 says done: set covered. | yes (#787 :56, :137) |
| R28 | a40b4a12 | main #783 (#782; C45) | covered | FIX-R (row) | exists: `shown_text_reads_words_inside_nested_alignments`, `selah_inside_qs_shows` (tc-usfm/tests/shown_text.rs), `a_verse_kept_as_written_shows_yahweh` (tc-align/tests/bank_words.rs) | Rows :138, :174 still say reference change / rebuild bug: set covered. | yes (#782 :54, :138, :174) |
| R29 | 90b8d6e1 · PR #780 | merge of picks #770 to #779 and D104 (R30-R39) | not applicable | none | none | None. | no |
| R30 | aa53bc26 | main 5f0794f (D104, C38: 8 MiB journal pieces) | unit todo | I | `a_tc4_segment_of_8_mib_imports` (to create, I) | None new (I step lists C38). | yes (D104 row :331 per I.md) |
| R31 | 2f96549e | main #779 (#769; C43, fold groups by figure) | reference change | REF1 | `rail_groups_by_figure_in_first_note_order` (to create; row :217 names it, not found) | None new. | yes (#769 :217) |
| R32 | 78fde396 | main #775 (#767; C37) | covered | none (C6 done) | exists: `adding_a_book_from_usfm_is_one_batch_whole_or_absent` (tc-store/tests/store/commands.rs) | None. | yes (#767 :173, :313) |
| R33 | 655d8382 | main #778 (#763; C41, C42) | reference change | REF1 (A5 help pages; Q31) | help case b `help_home_shows_no_rc1_build_text_offline` (per row :268; not checked) | None new. | yes (#763 :268, :352) |
| R34 | e7e1c71c | main #777 (#762; C40) | covered | none (B5b done) | exists: `compare_card_columns_fit_the_width` (parity/app/journeys/check-detail.test.mjs) | None. | yes (#762 :218) |
| R35 | c6388cc0 | main #776 (#773; C39) | covered | FIX-R (row) | exists: `a_reordered_quote_lights_only_its_own_phrase` (tc-helps/tests/highlight.rs) | Rows :53, :175, :214 still say rebuild bug: set covered once FIX-R confirms it passes. | yes (#773) |
| R36 | eac74839 | main #774 (C52, contributor rules) | not applicable | none | none | None. | no |
| R37 | b21f65b8 | main #772 (#766; C36) | covered | none | exists: `open_writes_creation_pins_only_when_none_exist` (tc-app/tests/app/store_scenario.rs) | Rows :267, :302 say unit todo / reference change: set covered. | yes (#766 :267, :302) |
| R38 | 4659df20 | main #771 (#765; C34) | covered | none | exists: `genesis_maqaf_quotes_title_in_the_ult` (tc-helps/tests/highlight.rs) | Row :215 says "(to add)": drop it. | yes (#765 :215) |
| R39 | 3b47d9f6 | main #770 (#768; C35) | covered | none | exists: `gateway_label_survives_an_nfc_record` (crates/tc-app/src/align/view.rs, a unit test) | Row :139 says reference change: set covered. | yes (#768 :139) |
| R40 | 2545c2e2 · PR #760 | release-only: the D102 re-cut of release/4.0 from main 52e03ba (carries #759, C33) | not applicable | none | none | None. | no |

## 3. New interface strings (`git diff 9ad4d5c origin/main -- src/i18n/en.json`)

Six keys added and one changed, in each of the ten catalogs. release/4.0 has the same seven.

| Key | English | From | Rebuild |
|---|---|---|---|
| `cc.exportSkipped` (new) | "Your alignment of these verses is not in the file. Check them in Align: {refs}" | #814 (D105) | D2 uses it for the export notice (REF1 vendors it). |
| `align.unavailable.inline-alignment.title` (new) | "This verse already holds an alignment" | #799 (#794) | The rebuild has its own `align.unavailable.alignedInText.*` (i18n-rebuild, pending in 9 languages). REF1 switches to tC4's keys, which removes two strings from H10's translation list. |
| `align.unavailable.inline-alignment.body` (new) | "{book} was added from a file that already holds alignments. tC4 keeps this verse's alignment as it is. You cannot change it in tC4 yet." | #799 | As above. |
| `align.unavailable.no-slot.title` (new) | "tC4 cannot save an alignment for this verse" | #800 (#795) | No such state in the rebuild; not vendored for use (declared difference). |
| `align.unavailable.no-slot.body` (new) | "tC4 cannot find this verse in the saved text of {book}, so it cannot save an alignment for it. If the verse text starts on the line after its verse number, move it onto the same line." | #800 | As above. |
| `syncResult.alignReplaced` (new) | "{book} {ref}: {name}'s alignment replaced yours." | #806 (D108) | Not used: the rebuild keeps both alignments (E1a). |
| `about.licenseName` (changed) | "License: GNU GPL v2 or later" to "License: MIT" | #838 (D111) | The rebuild keeps its own GPL line until the owner decides (§4). |

## 4. Choices between tC4's behavior and a translator's need that no H question covers

H7 (#810 tabs), H13 (#822 scheme) and H14 (#837 name) cover their changes. Q30 (a) already rules
#789/#820 (no review mark), and the rebuild's two-alignment conflict (#791) is an earlier ruling.
Not covered by any H in `.audit/roadmap/ROADMAP.md`:

1. **Does an Align edit clear "needs review"?** tC4 main (D105 point 5, #814): no, only Mark
   valid clears it. The rebuild clears it at the first edit (tests at tc4_edit.rs:597 and
   aligner.rs:341), and `docs/TC4-ISSUES.md:126` keeps that "until the owner rules otherwise".
   For the translator: with tC4's rule, a verse flagged after a source upgrade or a teammate's
   change stays red until they say it is right; with the rebuild's rule, moving one word makes
   the red go away even if the rest was never looked at. Needs an H question (or a PLAN
   declared difference with the owner told).
2. **A `done` alignment whose cards miss pinned words** (D107 point 4, #816): tC4 marks it for
   review; the rebuild shows it Valid (value.rs:108-119). Small, and no reachable scenario
   confirmed; REF1 can decide it as a parity port unless the owner wants a say.
3. **The rebuild's license** (#838): tC4 is MIT now; the rebuild is GPL-2.0-or-later. Not a
   translator need, so not an H-style question, but only the owner can decide it.
4. **"tC4 cannot save an alignment for this verse"** (#800 no-slot): the rebuild aligns such
   verses. That serves the translator, so no question: record a declared difference.

## 5. Checks of the card's expected rows (step 4 item 2) against the code

| Expected | Finding |
|---|---|
| #807 | "Likely not applicable": agreed (M7). No align-save queue in tc-app. Outcome tested via #801 (E1a). |
| #813, D110, #821 | Unit I (M15). The import is not built; tests to create. |
| #822 | I, owner H13. |
| #812 | T: the corpus test is to create; `split_verse` already keeps mid-verse layout lines in `words` (codec.rs:130-141), so expect it to pass first time. |
| #820 | T: tests to create; design suggests drop-and-list (M17). |
| `e2e/sync-refusal.spec.ts` | E1a step 4 port list; exists at tC4 b86270f (646 lines). |
| D108 (#806) | E1a note; the rebuild deliberately differs (M4). |
| D105 (#809, #811, #814) | D2 for export wording; also the flag rule of §4 item 1, which the card's list does not name. |
| #830 | FIX-R. Consistent with code: Align draws the cards inside `dir=orig_dir` (`crates/tc-ui/src/views/align/session.rs:1136`) and `card_header` ignores `rtl_orig` (`:224`). Not run. |
| #832, #836 | FIX-R (not re-checked here). |
| #825 | FIX-R step 2 and T (the dropped-link list). |
| #831 | FIX-R step 5: `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub` does not exist; only `the_resume_card_names_the_place_and_the_text` (tc-ui/tests/app/home.rs:227). |
| #833 | .1 FIX-R (`an_offline_update_check_shows_one_plain_sentence`, to create), .2 and .4 G1, .3 F1, .5 FIX-R (`source_texts_lists_the_installed_version`, to create). |
| #835 | F1 (and I). |
| #837 | E1b, owner H14. |
| #810 | REF1, owner H7. |
| #818, #827, #828, #834 | Not applicable: confirmed (they are PRs: code hygiene, decision log, release notes). #823 and #829 are issues and were not checked here. |
| #815/#816 for PLAN.md | Confirmed stale: PLAN :91-95 (#789 "unannounced") and :96-99 (#790 "stored cards as they are"). Also stale: :73 (#800 "open PR"), :102-105 (#787/#796 one rule now in tC4), :111-113 (#799 "open PR"), :151-155 (#791 "disappear unannounced"; D108 now names the verse). |

Test names that rows in `docs/TC4-ISSUES.md` give but that do not exist: `two_different_alignments_of_one_verse_are_listed_once`,
`a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment`, `export_keeps_inline_zaln_bytes_even_with_a_matching_record`,
`export_reports_and_keeps_a_verse_that_needs_review`, `a_record_with_nothing_placed_exports_the_stored_bytes`,
`stale_scope_save_keeps_text_and_home_leaves_in_one_click`, `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text`,
`an_nfd_typed_verse_aligned_in_the_same_session_is_valid_after_reopen`, `a_footnote_right_after_a_word_banks_both_words_and_weaves`,
`after_a_source_upgrade_the_open_verse_shows_the_banner` (exists as `..._shows_the_notice`), `rail_groups_by_figure_in_first_note_order`.

## 6. Counts

Lines: 19 main merges (M1-M19) + 40 release commits (R1-R40) = 59.

| Status | Main (19) | Release (40) | All (59) |
|---|---|---|---|
| rebuild bug | 0 | 0 | 0 |
| unit todo | 3 (M6, M15, M17) | 4 (R9, R11, R20, R30) | 7 |
| reference change | 3 (M4, M11, M16) | 5 (R10, R15, R22, R31, R33) | 8 |
| covered | 8 (M1, M3, M5, M8, M9, M10, M12, M13) | 17 (R13, R14, R16, R17, R18, R21, R23, R25, R26, R27, R28, R32, R34, R35, R37, R38, R39) | 25 |
| owner question | 0 | 0 | 0 |
| not applicable | 5 (M2, M7, M14, M18, M19) | 14 (R1-R8, R12, R19, R24, R29, R36, R40) | 19 |
| Total | 19 | 40 | 59 |


