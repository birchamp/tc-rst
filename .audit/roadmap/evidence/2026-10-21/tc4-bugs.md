# tc4-bugs

## Summary
tC4 issue tracking (docs/TC4-ISSUES.md on resume-build-lane) has 271 rows: 103 covered, 60 not applicable, 80 unit todo, 15 rebuild bug, 9 reference change, 2 owner question, 2 done. The row statuses are out of date. All 17 of the 2026-10-19 sweep's bugs were fixed by the FIX-* units, and their tests are in the code, yet 15 rows still say "rebuild bug". Another 29 "unit todo" rows name only units that are already built. Seven of them have no test anywhere: #801 for A4b, #576 for C2, #179 for tc-journal, and #731, #613, #599 and #763 for A5.

Units not built yet carry 51 unit-todo rows plus reference changes. The table below lists them by unit, each with its test.

I sampled 16 covered or done tests. Thirteen are real and assert the fix. #612 is weak, #795's test was never written, #507 names a journey that was renamed, and the I-unit halves of #646 and #529 are missing.

Since the sweep, 18 tC4 PRs have merged and 16 issues are new or changed. The rebuild mentions none of them except #792. Reading the code turns up four tC4 bugs that the rebuild repeats or makes worse. #830: in a Hebrew verse, the words on an Align card read backwards. #832: Settings changes the project's direction, but Home, the open project and Align never read it. #836: English helps are laid out right to left under an Arabic language set. And for #825, the owner's Q30(a) rule says a link a Translate save cannot follow goes back to the bank and is shown in a list; tc-store throws that list away. #820's ambiguity is handled safely in code but has no test.

tC4 has tagged rc.2. It now has tab-indented poem lines (#810) and flags a verse for review when a link's word is unclear (D106). Some declared differences in PLAN now describe tC4 wrongly. Nine of tC4's 13 RISKS rows are still open and touch S2, D1, X, E1b, P, G2 or V.

# tC4 known bugs the rebuild must not repeat: evaluation, 2026-10-21

Snapshots under `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/snap/`. "RBL" means `resume-build-lane` (f9067bc). File paths are relative to RBL unless another snapshot is named. Nothing was built or run. Any judgement of a test's strength comes from reading its source.

## 1. Status counts

I parsed the area tables in §2–§6 of `docs/TC4-ISSUES.md` (RBL) with a script. Each row has 6 cells, and every row parsed.

| Area | bug | todo | ref | covered | owner Q | done | N/A | Rows |
|---|---|---|---|---|---|---|---|---|
| Align | 7 | 4 | 2 | 16 | 1 | 0 | 1 | 31 |
| USFM/Translate/helps | 1 | 12 | 1 | 26 | 0 | 0 | 8 | 48 |
| Check/resources | 3 | 3 | 2 | 29 | 1 | 2 | 23 | 63 |
| Sync/storage/import | 2 | 26 | 3 | 15 | 0 | 0 | 11 | 57 |
| Shell/startup/help | 2 | 35 | 1 | 17 | 0 | 0 | 17 | 72 |
| **All** | **15** | **80** | **9** | **103** | **2** | **2** | **60** | **271** |

§7 lists 6 more issues that were never triaged: #476 (A5), and #4, #43, #45, #3 and #32 (G1).

**What is out of date:**
- The "Counts" header (`TC4-ISSUES.md:31-38`) still gives the 2026-10-19 numbers: 269 rows, 17 bugs, 123 todo, 56 covered.
- All 15 "rebuild bug" rows are fixed. `.audit/STATUS.md:80-88` lists FIX-helps, FIX-misc, FIX-text and FIX-align as done, which covers #773, #661, #701, #552, #549, #782, #795, #796, #787, #789, #790, #793 and #794. #582 and #767 were fixed by 3ecdcae, and #710 by the windows-push-refusal merge. The tests named in §1 exist (for example `a_reordered_quote_lights_only_its_own_phrase`, `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari`, `a_project_picture_without_a_role_is_the_project_rung` and `the_bank_is_the_export_for_generated_verses`). Three names are missing:
  - `a_footnote_right_after_a_word_banks_both_words_and_weaves` (§2, the #796 row).
  - `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (tc-store, #794). The behavior exists: `tc-store/src/commands.rs:349` reads `reflow_edit(..).ok()?`. But no tc-store test pins it.
  - `after_a_source_upgrade_the_open_verse_shows_the_banner`, which was renamed to `…_notice` (`tc-align/tests/tc4_reconcile_open.rs:398`).
- **29 unit-todo rows name only units that are already built.** Most are A5 rows; the rest belong to A8, B2, B5b, C2, A4b and tc-journal. Many are in fact done; for example, `parity/app/journeys/help.test.mjs` runs cases a–i and `app-language.test.mjs` runs a–j. But no test exists for these:
  - #801 (A4b): `a_save_refused_for_good_lets_the_person_go_home_and_keeps_the_text`.
  - #576 (C2): `last_edited_names_the_span_then_its_first_verse`.
  - #179 (tc-journal): `loss_unknown_parent_verse_edit_keeps_standing_text`. It is also missing in both storage snapshots.
  - #731 (A5): two tests.
  - #613: `hindi_label_renders_in_the_bundled_face_offline`.
  - #599: `switcher_stays_centred_while_the_save_indicator_changes`.
  - #278: `about_commit_matches_the_release_record`.
  - #763 / Q31: `help_home_shows_no_rc1_build_text_offline`.
- **Owner questions that were answered but never assigned:**
  - Q33 / #711 (row 233 still says "owner question"). The test `a_project_with_a_tc4_aquifer_pin_opens_and_keeps_it` is missing.
  - Q34 / #720 (answer (b): show the Portuguese Bible as a reading pane labelled AI-made). `APP-DESIGN.md:1505-1512` says it "needs a design", and no unit owns it.

## 2. Issues assigned to units not built yet

The source is the RBL rows plus §8 "Notes by unit". Tests are named as the row names them. "(new)" marks a test proposed here from §5.

| Unit | Issues | Regression tests |
|---|---|---|
| C7a | #679, #78, #25 | J18 `keep_this_version_resolves_with_review_tokens_and_a_second_send_gets_the_first_reply` (already exists); journey `conflict-review`; Restore test `restore_writes_a_new_version_and_history_keeps_the_old` (new) |
| C7b | #25 (after Q-9), #791 (Alignments tab writable) | `the_tab_strip_shows_only_when_another_kind_holds_something` (already exists) |
| D1 | none; #536 is N/A | see RISKS #2 and #13 in §6 |
| D2 | #796 (export), #409, #553, #373, #420; T note | `export_reports_and_keeps_a_verse_that_needs_review`; `export_keeps_inline_zaln_bytes_even_with_a_matching_record`; `a_record_with_nothing_placed_exports_the_stored_bytes`; the J7 and J23 ports with tC4's `\add` case; `export_judges_validity_with_needs_review` (new) |
| E1a | #801 and #807, #761, #724, #681, #678, #24, #791 (D108) | `stale_scope_save_keeps_text_and_home_leaves_in_one_click`; `a_refused_align_save_behind_a_span_save_lets_home_leave` (new, #807); `failed_open_keeps_the_sync_toast_links`; `no_door43_request_at_launch_or_60_s_idle`; `two_different_alignments_of_one_verse_are_listed_once`; J26 through the UI; one test per guarantee (#24) |
| E1b | #766, #761, #724 (team-access line on the done step), #677, #372, #837 (new), RISKS #7 | `open_from_door43_equals_main`; `open_name_in_use_refused_before_request`; `open_private_repo_signs_in_and_retries`; J11 port; `opening_under_a_new_name_shows_that_name_on_its_card` (new) |
| E2 | #705 | `a_teammates_audio_file_arrives_byte_for_byte` |
| F1 | #577, #499, #502, #553, #766, #373, #835 (new), #833.3 (new), #822 (owner Q) | `typed_import_name_is_the_project_name`; J9 `a_two_file_drop_is_one_new_project`; J9 `aligned_titus_import_has_no_inline_zaln_and_46_records`; `import_accepts_x_codes_and_refuses_unusable_codes_before_import` (new); `an_import_refusal_shows_a_sentence_not_a_code` (new) |
| G1 | #743, #751, #740, #642, #606/#554, #278, #639/#214/#702, #284, #456, #44, #378, #632, #46, #29 (Graphite); never triaged: #4, #43, #45, #3, #32; #833.2 and #833.4 (new) | `window_is_painted_within_1_s_of_the_click`; `closing_the_window_while_preparing_leaves_no_tc4_process`; `linux_window_keeps_native_frame`; `windows_window_has_no_native_bar_and_keeps_zoom_and_reload`; `tc4_exe_imports_no_vcruntime`; Authenticode check; clean-machine smoke per installer; `about_commit_matches_the_release_record`; `a_text_field_right_click_offers_cut_copy_paste` (new); `export_save_dialog_has_a_readable_title` (new) |
| G2 | #751, #740, #29 (`ar` screens); RISKS #3 | `first_launch_under_60_s` (5 runs); J10 `rtl_project_drafts_checks_and_aligns` |
| S2 | #747, #161; RISKS #2 and #13 | `a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept` must add `TIT.usfm.bak`, a `~` file and a `.tmp` (the s2-proof-executor version at `crates/tc-store/tests/store/outside.rs` has none of them); `a_new_device_copies_the_profile_and_both_group_as_mine` |
| P | RISKS #3 (#160 is N/A) | budgets on Psalms and on ten times Psalms |
| T | the fold's `invalid` reads only the hash (`tc-journal/src/alignment.rs:47-74`); Q30(a) listing (new); #820 (new); #794 store test; #179 | `the_alignment_files_and_the_rail_agree_on_invalid`; `a_translate_save_that_drops_an_unclear_link_lists_it` (new); `replacing_one_copy_of_a_repeated_linked_word_keeps_no_unsure_link` (new); proptest `follow_matches_every_longest_common_subsequence` (new); `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment`; `loss_unknown_parent_verse_edit_keeps_standing_text` |
| X | #801, #761, #747, #705, #681, #680, #677, #676, #675, #161, #24, #821 path 1 (new); RISKS #5, #6, #8, #11 and #12. #710 is done. | `a_save_during_every_sync_step_is_kept_and_pushed_next`; `push_is_never_forced`; `non_fast_forward_retries_six_times_within_30_s_then_busy`; `crash_at_every_sync_step_converges_on_retry`; `door43_ingest_deterministic`; `a_damaged_remote_pack_is_salvaged_and_listed`; `username_written_once_after_first_successful_fetch`; `no_email_or_token_in_any_change`; `copied_username_is_replaced_by_the_first_signed_in_fetch`; library J26 `three_devices_converge_with_byte_identical_derived_files` |
| I | #794 (list books in the report), #553, #409, #499, #766, D104, #768, #646, #529, #549, #373, #813/#821 (new), #822 (owner Q), #832.2 (new) | `tc4_project_with_a_6_8_mib_book_add_segment_imports_whole`; `tc3_import_pins_the_nearest_twl_release`; `imported_obsolete_checking_language_is_kept_and_ignored`; `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme` (new); `an_imported_project_shows_its_metadata_language_name_in_settings` (new) |
| V | #46; RISKS #10 | `older_build_starts_on_newer_app_data`. The journal half is covered by `the_gate_refuses_newer_majors_and_reads_newer_minors` (`tc-journal/tests/versioning.rs:293`). |

## 3. Sample of covered and done rows (16 checked)

| Issue | Test (RBL file:line) | Verdict |
|---|---|---|
| #802 | `a_stale_token_makes_a_conflict_never_a_revert` (`tc-store/tests/store/contract.rs:71`) | Strong: asserts 2 heads and a conflict |
| #802 | `loss_rename_retry_never_overwrites_outside_save` (`tc-store/tests/store/loss.rs:273`) | Strong, but it polls with a 15 s deadline, so it depends on timing |
| #802 | `loss_f5_stale_buffer_never_reverts` (`tc-journal/tests/loss.rs:793`) | Strong |
| #767 | `adding_a_book_from_usfm_is_one_batch_whole_or_absent` (`tc-store/tests/store/commands.rs:1007`) | Strong: kills the process at every write point (at least 4) |
| #582 | `create_refuses_a_language_tag_the_burrito_cannot_hold` (`tc-store/tests/store/tc4.rs:406`) | Strong |
| #594 | `deciding_again_judges_the_text_and_the_list_in_front_of_the_person` (`tc-check/tests/status.rs:188`) | Strong as a pure-function test; no UI case |
| #580 | `a_change_back_to_the_resource_a_decision_was_made_against_revives_it` (`tc-check/tests/carry.rs:522`) | Strong |
| #754 | `la_fe_de_los_dropped_as_fe_la_los_reads_la_fe_los` (`tc-align/tests/tc4_card_order.rs:66`) | Strong |
| #734 | `a_repeated_word_returns_to_its_own_occurrence_slot` (`tc-align/tests/tc4_edit.rs:206`) | Strong |
| #722 | `ugnt_upgrade_marks_1_1_invalid_in_align` (`tc-app/tests/app/upgrades.rs:1781`) | Strong: Valid becomes Invalid and the alignment heads are kept |
| #559 | `every_call_is_refused_before_anything_is_sent_while_the_switch_is_off` (`tc-door43/tests/internet_switch.rs:14`) | Strong as far as I read: it lists every call; I did not read the assertion lines |
| #725 | `a_collision_at_the_rename_keeps_the_destination_byte_for_byte` (`tc-store/tests/store/tc4.rs:30`) | Strong |
| #274 | `a_prediction_over_two_cards_is_left_out_never_cut_to_the_first` (`tc-align/tests/tc4_suggest.rs:426`) | Strong |
| #643 | `a_decision_on_a_link_whose_article_left_is_kept_and_counted_as_to_check` (`tc-check/tests/carry.rs:267`) | Strong |
| #765 | `genesis_maqaf_quotes_title_in_the_ult` (`tc-helps/tests/highlight.rs:502`) | Strong |
| #612 | `vendored_assets` (`tc-ui/tests/vendored_assets.rs:87`) | **Weak**: it checks that vendored bytes match, not Hindi letter spacing in the screens the rebuild draws itself |

Other problems found in covered rows:
- #795's test is marked "(to add)" while the row says covered.
- #507's journey is now `j25_a_story_opens_with_its_rail_its_units_and_writable_comment_boxes` (`parity/app/journeys/story.test.mjs:26`).
- The unit I halves of covered rows #646 and #529 have no tests.
- #792 is marked N/A on the strength of a scratch test (`zz792_…`) that was never committed.

## 4. New in tC4 since the last sweep

tC4 `main` moved from 9ad4d5c to b86270f with 18 first-parent merges: #785, #804, #805, #806, #799, #803, #808, #800, #809, #811, #814, #815, #816, #818, #817, #810, #824 and #827. `release/4.0` gained 36 commits since 9ad4d5c and is now at 9f06f99. Tag `v4.0.0-rc.2` is a5ce030, and no `v4.0.0` tag exists yet. 9ad4d5c is not an ancestor of `release/4.0`; the release branch gets its fixes by cherry-pick. A grep of RBL's docs/, .audit/, crates/ and parity/ finds none of #803 to #837 (only #792).

| tC4 item | State | Rebuild relevance (evidence) | Unit | Regression test |
|---|---|---|---|---|
| #830: Hebrew verse, the words placed on a card read backwards | open | **Same bug.** The card grid takes `dir=orig_dir` (`tc-ui/src/views/align/session.rs:1136`). The flex body of placed words has no `dir` (`:350-397`). The bank does take the target direction (`:856`). No Old Testament align journey exists. | B6 (built) → fix unit | journey `an_ot_card_shows_placed_words_in_the_translation_direction` |
| #832.1: Home badge stays LTR after Settings switches to right to left | open | **Same bug, and worse.** Settings writes only the `textDirection` setting (`tc-app/src/settings.rs:237-245`). Home (`home.rs:220-226`), the open reply (`understand.rs:523`) and Align (`align/open.rs:80-93`) all read `metadata scriptDirection`. A change made in Settings shows nowhere. | B9a/C6 (built) → fix unit | `a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart` |
| #832.2: empty language name for an imported project | open | `settings_values` reads `languageName` only from settings.json (`settings.rs:46`) | B9a, I | `an_imported_project_shows_its_metadata_language_name_in_settings` |
| #836: English helps under an Arabic set read right to left | open | **Same risk.** Every card takes the pane's `ready.dir` (`helps_panel.rs:709,743`). BSOJ is a listed gateway (`tc-resources/src/gateways.rs:156`). | B3 (built) → fix unit | `an_english_question_under_an_arabic_set_reads_left_to_right` |
| #820 (fixed by #824): replacing one copy of a repeated linked word keeps a link it cannot be sure of | closed | Safe by code reading: `follow` requires every longest common subsequence to agree (`tc-align/src/reflow.rs:237-277`). No test exists (`tc4_reflow.rs:381-494` covers only delete, insert and move). | T | the vector "Dios Dios" → "Dios Pablo"; proptest against a brute-force search on 2–3 distinct words, up to 8+8 words |
| #825: a tie in the move rule drops a link without a flag | open (question) | **The rebuild drops such links silently.** tc-store uses only `reflowed.value` and discards `dropped` (`tc-store/src/commands.rs:349-362`). The aligner lists only the links its own carry-over drops at open (`tc-align/src/aligner.rs:180-183`). Q30(a)'s "listed" is never delivered for a Translate save. | T / C2 | `a_translate_save_that_drops_an_unclear_link_lists_it` |
| #801 and #807 (fixed by #803 and #808): a refused save after a sync blocks Home | closed | `gate()` blocks on any field that failed after one retry (`tc-ui/src/app/save/mod.rs:1211-1216`; `web/saves.rs:461`). Neither the code nor the design mentions #801. | A4b (orphaned), E1a | see §2 |
| #812 (fixed by #810): a mid-verse `\p`, `\ts\*`, `\b`, `\q3` or `\s` line cuts off the editable verse | closed | Covered by design, by inference: `split_verse` keeps every line up to the last non-layout line (`tc-usfm/src/codec.rs:130-141`). No corpus test. | C2 → T | corpus check: for every bundled ULT and UST verse, the body holds every `target_words` word; 1SA 2:2, 1SA 4:1, REV 7:12 and MAT 27:9 save whole |
| #810: poem lines as leading tabs in both editors | merged; in rc.2 (via #826) | A reference change. The rebuild shows `\q` markers (Q-20). | B4a/B4b | owner question at the reference move |
| #813 (fixed by #817), #821, #822: a burrito with no vrs.json; journal frame recovery; detecting the versification scheme | 2 open | Applies to tC4 project import and to opening from Door43 | I, F1, X; owner Q (#822) | `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme` |
| #831: Resume card pairs one verse's number with another verse's text | open | Not affected, by inference: the number and the text come from the same save (`tc-app/src/translate.rs:321-370`), and no Check-tool record exists | C2 | `resume_card_names_the_verse_of_its_text_and_never_shows_the_stub` |
| #833.1: "Failed to fetch" when the update check runs offline | open | Likely the same: `texts_error = Some(error.to_string())` (`tc-app/src/upgrades.rs:729-731`) | C8/B9a | `an_offline_update_check_shows_one_plain_sentence` |
| #833.2, #833.3, #833.4, #833.5 | open | Save dialog title (G1/D2); import error codes (F1); no right-click menu (G1; no context-menu code in RBL); v89 listed while v91 is installed (B9a, not checked) | G1, F1, B9a | see §2 |
| #835: the import's language-code rule disagrees with the server's | open | The rebuild accepts `x-` codes at create (`tc-store/tests/store/tc4.rs`, `create_keeps_regional_language_tags`). It vendors tC4's hint, "such as kau" (`tc-ui/assets/i18n/*.json`). | F1 | see §2 |
| #837: opening from Door43 under a new name; the card shows the old title | open | Home shows `identification.name` (`home.rs:207-213`) | E1b; owner Q | see §2 |
| #806 (D108, per-verse alignment rows), #815 (D106 review flag), #816 (#790 card repair), #800/#799/#809/#811/#814 | merged; in rc.2 | Reference changes. PLAN's declared differences at `docs/PLAN.md:91-100` now describe tC4 wrongly: "tC4 4.0 moves the link … unannounced" and "tC4 shows the stored cards as they are" | B10/E1a, PLAN | update the text; B10 `two_different_alignments_of_one_verse_are_listed_once` |
| #818: the word-key separator written as `\u0000` | merged | Not applicable: a source-only change with the same runtime string (commit 529c6b1b) | — | — |
| #827, #828, #834 (docs), #823 (moved to #821), #829 (tab-size) | — | Not applicable. #829 is moot because the rebuild shows markers, not tabs. | — | — |
| #367 (CSV export), #758, #628 (updated) | open | #367 has no row: if tC4 adds it, it falls to D2. #758 and #628 are already rows. | D2 | — |

## 5. tC4 RISKS.md rows that are still open and touch remaining units

Source: `ext/tc4-tip/docs/RISKS.md`, 13 rows; last changed in b49e8160.

| # | Risk | Rebuild state | Unit and test |
|---|---|---|---|
| 2 | Two writers on one book file | Outside-edit holds exist (`loss_rename_retry…`); S2 and D1 are not done | S2/D1: another program editing during materialize |
| 3 | Load-time cost on large books | budgets not proven | P, G2 |
| 5 | A main made by a share is not proven equal to a main made by team sync | not built | X: share, then sync, converges |
| 6 | Second device; push not fast-forward | not built | X: `push_is_never_forced`, `non_fast_forward_retries…` |
| 7 | Door43 name collision, tC3-era repositories included | not built | E1b: `open_name_in_use_refused_before_request` plus a share-name check |
| 8 | Door43 tools reading the rebuild's sidecars and journal | unverified | X/E1b: a Door43 Preview check at the first pilot |
| 10 | An older build reads a newer journal as damaged | journal gate covered (`versioning.rs:293`) | V: `older_build_starts_on_newer_app_data` |
| 11 | Power loss during a sync's apply | not built | X: `crash_at_every_sync_step_converges_on_retry` |
| 12 | A sync loses the translator's own work | not built | X: the G1 own-file guarantee |
| 13 | metadata.json torn mid-write | partly covered: `a_torn_missing_or_empty_metadata_json_never_locks_the_project` (`tc-store/tests/store/tc4.rs:136`) | S2: kill at every point of a metadata.json write |

Rows 1, 4 and 9 are tC4-only (the Pankosmia pin, process, and the platform's push panic). They do not apply.

## 6. Notes for the enforcement funnel

- **Code.**
  - Use one direction source for a project. Settings should write `metadata scriptDirection` or project it; today `textDirection` is a second truth (#832).
  - Placed words on an Align card should take the target's direction (#830).
  - Each helps card should get `dir="auto"` (#836).
  - The verse-save reply should carry `dropped` as a type, so a dropped link cannot be thrown away silently (Q30(a)).
  - The export should judge validity with `tc_align::needs_review`, not tc-journal's `is_valid`.
- **Checks.**
  - Add a CI script over `docs/TC4-ISSUES.md`: every name in backticks in a covered or done row must exist as a Rust `fn` or a journey title. A row may not say "rebuild bug" for an issue a done FIX unit lists, and a unit-todo row may not name only done units. Each failure names the row.
  - Add the `follow` proptest, the ULT/UST corpus check for whole verse bodies, an Old Testament align journey, and a test that a direction set in Settings reaches Home.
- **Prose.** Rewrite the declared differences at PLAN:91-100 for rc.2. Assign Q34 a unit.
- **Human.** Three owner questions: poem lines as tabs or as markers at the reference move (#810); how imports choose the versification scheme (#822); a card-only or metadata rename for an open under a new name (#837).

## Findings
- [T1] (medium/high) TC4-ISSUES.md row statuses are stale: all 15 'rebuild bug' rows were fixed by FIX-* units, the header counts are from 2026-10-19, and 29 'unit todo' rows name only built units.
  - evidence: RBL docs/TC4-ISSUES.md parsed: 271 rows = 103 covered, 60 N/A, 80 todo, 15 bug, 9 ref, 2 OQ, 2 done; header :31-38 says 269/17/123/56; RBL .audit/STATUS.md:80-88 FIX-helps, FIX-misc, FIX-text, FIX-align done; section-1 regression tests found in RBL crates, e.g. tc-helps/tests/highlight.rs a_reordered_quote_lights_only_its_own_phrase, tc-obs/tests/project_pictures.rs
  - rec: Add a deterministic CI check that cross-validates TC4-ISSUES rows against test names and STATUS units; reconcile the rows in one commit.
- [T2] (medium/high) Seven rows assigned to already-built units have no test anywhere: #801 (A4b), #576 (C2), #179 (tc-journal), #731, #613, #599 and #763/Q31 (A5).
  - evidence: grep of fn names in RBL crates and parity journeys: names missing; #801 not mentioned in RBL crates or APP-DESIGN; tc-ui/src/app/save/mod.rs:1211-1216 blocks on any failure after one retry; #179 test missing in RBL, s2-proof-executor and storage snapshots
  - rec: Create a fix unit for orphaned rows; write each named test (red first) or rule the row not applicable.
- [T3] (high/medium) tC4 #830 recurs in the rebuild: on a Hebrew (rtl) verse, the words placed on an Align card inherit rtl and read backwards for a left-to-right translation.
  - evidence: RBL crates/tc-ui/src/views/align/session.rs:1136 cards container dir=orig_dir; session.rs:350-397 placed-word flex body and buttons with no dir; session.rs:856 bank uses target_dir; parity/app/journeys/align.test.mjs has no OT case (grep PSA|JON|uhb = 0)
  - rec: Give the card body dir=target_dir; add the OT align journey an_ot_card_shows_placed_words_in_the_translation_direction.
- [T4] (high/medium) tC4 #832 is worse in the rebuild: Project settings writes only the textDirection setting, while Home, the open reply and Align read metadata scriptDirection, so a direction change shows nowhere; Settings also reads languageName only from settings.json.
  - evidence: RBL crates/tc-app/src/settings.rs:237-245 (write) and :46 (languageName); crates/tc-app/src/home.rs:220-226; crates/tc-app/src/understand.rs:523 dir: facts.dir; crates/tc-app/src/align/open.rs:80-93
  - rec: Have one direction source; test a_direction_changed_in_settings_shows_on_home_and_in_every_tool_after_restart; I/B9a test for the language-name fallback.
- [T5] (high/medium) Q30(a) promises that a link a Translate save cannot follow goes back to the bank 'and is listed'; tc-store discards the dropped list, so the translator is never told (the rebuild's form of tC4 #825/#820).
  - evidence: RBL crates/tc-store/src/commands.rs:349-362 uses reflowed.value only; crates/tc-align/src/aligner.rs:180-183 lists only drops from its own carry-over at open; docs/PLAN.md:91-95 says 'is listed'
  - rec: Carry dropped links in the save Ack/notice and show them in Align; test a_translate_save_that_drops_an_unclear_link_lists_it (unit T or C2 follow-up).
- [T6] (medium/medium) tC4 #820's ambiguous repeated-word replace is handled safely by the rebuild's all-LCS follow(), but no test pins it.
  - evidence: RBL crates/tc-align/src/reflow.rs:237-277 left_out/matched agreement; crates/tc-align/tests/tc4_reflow.rs:381-494 test only delete/insert/move; tC4 issue #820 vector 'Dios Dios' -> 'Dios Pablo'
  - rec: Add the #820 vector and a proptest against brute-force enumeration of all LCS for small inputs (2-3 distinct words, up to 8+8).
- [T7] (low/medium) tC4 #836 recurs: every helps card takes the pane's direction, so English questions in BSOJ ar_tq would read right to left.
  - evidence: RBL crates/tc-ui/src/views/helps_panel.rs:709 and :743 dir=ready.dir per card; crates/tc-resources/src/gateways.rs:156 lists ar/BSOJ
  - rec: Use dir=auto per card text and keep the pane direction; add an_english_question_under_an_arabic_set_reads_left_to_right.
- [T8] (medium/high) Since the 2026-10-19 sweep, tC4 merged 18 PRs on main and has 16 new or updated issues (#807-#837); the rebuild references none of them except #792.
  - evidence: git -C ext/tc4-tip log --first-parent 9ad4d5c..origin/main: 18 merges; gh issue list --search updated:>=2026-10-19 output; grep of RBL docs/.audit/crates/parity for #803-#837: none
  - rec: Run the next tc4-issue-sweep over 9ad4d5c..b86270f and release/4.0 9f06f99; add the section 4 rows.
- [T9] (medium/high) rc.2 (a5ce030) includes #810 (poem lines as tabs), #815/D106 (review flag), #816 (card repair) and #824; the declared differences at PLAN:91-100 now misdescribe tC4, and #810 conflicts with the rebuild's marker display (Q-20).
  - evidence: git log 9ad4d5c..origin/release/4.0 shows #826 pick-810, #824, #815, #816; tag v4.0.0-rc.2 -> a5ce030; no v4.0.0 tag; RBL docs/PLAN.md:91-100
  - rec: Update the declared differences now; ask the owner tabs vs markers before the reference moves.
- [T10] (high/high) Units not yet built carry 51 unit-todo rows plus reference changes; the largest are G1 (20 issues), X (18), I (12), F1 (9), E1b (9), E1a (8) and D2 (7).
  - evidence: script over RBL TC4-ISSUES.md unit column and section 8 'Notes by unit' (:431-556)
  - rec: Paste the section 2 table into each unit's plan as acceptance tests.
- [T11] (low/high) Of 16 sampled covered/done tests, 13 are strong; #612's vendored_assets is weak, #795's test was never written, #507 names a renamed journey, and the I halves of #646 and #529 are missing.
  - evidence: section 3 table with RBL file:line for each; crates/tc-ui/tests/vendored_assets.rs:87 checks bytes only
  - rec: Write the #795 test, give #612 a letter-spacing assertion, fix the #507 name, and mark the #646/#529 halves as I todos.
- [T12] (medium/medium) Versification at import (#813, #821, #822) is unplanned in the rebuild: a tC4 project with a book and no versification frame or vrs.json must import with a stated scheme, never silently eng.
  - evidence: tC4 issues #813 (closed via #817), #821 and #822 (open), bodies read via gh; no reference in RBL docs (grep)
  - rec: Unit I/F1 note and test a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme; owner question for detection.
- [T13] (medium/high) Owner answers Q33 (#711) and Q34 (#720) have no owning unit or test.
  - evidence: RBL TC4-ISSUES.md:233 still 'owner question'; test a_project_with_a_tc4_aquifer_pin_opens_and_keeps_it missing; RBL docs/APP-DESIGN.md:1505-1512 says Q34 needs a design
  - rec: Assign Q33's test to a C8 follow-up and Q34 to a named unit with a design step.
- [T14] (high/high) Nine tC4 RISKS rows are still open and touch remaining units (#2, #3, #5, #6, #7, #8, #11, #12, #13); #10 is covered at the journal level only.
  - evidence: ext/tc4-tip/docs/RISKS.md rows 1-13; RBL tc-journal/tests/versioning.rs:293; tc-store/tests/store/tc4.rs:136
  - rec: Make each risk an acceptance test in S2, D1, X, E1b, P, G2 or V as in section 5.
- [T15] (low/low) The offline update check likely shows a raw technical error, like tC4 #833.1, against the Q36 rule.
  - evidence: RBL crates/tc-app/src/upgrades.rs:729-731 Some(error.to_string()); crates/tc-ui/src/app/upgrades.rs:184-190 inserts it into upgrade.checkFailed
  - rec: Map Door43 errors to fixed sentences; test an_offline_update_check_shows_one_plain_sentence.

## Unverified
- No test or build was run (forbidden). Every judgement of a test's strength, and every finding of a rebuild bug (#830, #832, #836, the Q30(a) list, #833.1, #801 under A4b), comes from reading code. Prove each with the named test, red first, in a fix unit.
- #812 'covered by design' and #831 'not affected' are inferences from tc-usfm/src/codec.rs:130-141 and tc-app/src/translate.rs:321-370. Prove them with a corpus test over the bundled ULT/UST and a Resume journey.
- Only 16 of the 103 covered rows were sampled. A CI cross-check of TC4-ISSUES against test names would cover the rest.
- Not checked: whether each of the A5, A8, B2 and B5b unit-todo rows is behaviorally done. Only named-test existence was grepped. Read each row against the journeys in parity/app/journeys.
- For #559, the assertion lines of every_call_is_refused_before_anything_is_sent_while_the_switch_is_off were not read (only the first 40 lines).
- Not checked: whether a read-only or BadToken refusal is a 'held' field, which would not block the gate (the A4b #801 question). Read tc-ui/src/app/save/mod.rs sendable_by_gate and the C1b design.
- #833.5 (v89 listed while v91 is installed) was not checked in the rebuild.
- The gh issue and PR lists were read on 2026-10-21. Later tC4 changes need a re-run with the same queries.
- The storage snapshots were checked only for the named tests of #747, #161, #767, #386 and #179. P's partial work on the storage branch was not reviewed for RISKS #3.