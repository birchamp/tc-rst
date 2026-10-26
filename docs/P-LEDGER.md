# P-ledger: which CI job and which tests show each of P1 to P10

Read a row as: `docs/PLAN.md`'s promise, the GitHub CI job that runs its checks, and the tests
that show it today; `(pending: <ID>)` names each card that is not done and still owes a part.
`.audit/tools/claims.test.mjs` checks every row: each named test exists, each job is a job of a
workflow, and each pending card is not done.

| P | What it promises | CI job | Tests |
|---|---|---|---|
| P1 | Every live tC4 journey has an automated end-to-end test | `journeys`, `parity-baseline` | `parity/app/journeys/coverage.json` maps each journey and spec (`parity/app/journeys/coverage.test.mjs`); `j4_a_mark_on_dragged_words_a_comment_and_a_bookmark_are_one_decision_record_on_disk`, `j5_align_steps_that_do_not_save`, `rtl_project_drafts_checks_and_aligns`, `j7_aligned_and_plain_usfm_download_and_leave_the_project_unchanged`, `j23_story_markdown_and_the_pdf_leave_the_project_unchanged`; F1's J9a and Update a book: journeys `import`, `update-book` (pending: D1, E1a, E1b) |
| P2 | Team work: sync, share, open, exchange and conflicts end to end | `test`, `journeys` | Newest wins outside verse text: `newest_wins_converges`, `the_loser_stays_in_history`, `only_text_conflicts` (`crates/tc-journal/tests/properties.rs`), `newest_version_wins_and_history_restores_the_older` (`parity/app/journeys/conflict-kinds.test.mjs`) (pending: E1a, E1b, E2) |
| P3 | No silent loss across random concurrent histories | `test` | `fold_converges_under_any_union`, `properties::conservation`, `concurrent_differences_are_visible`, `merge_units_sound`, `resolution_always_possible` (`crates/tc-journal/tests/properties.rs`) |
| P4 | Crash safety at every write point | `test` | `crates/tc-store/tests/store/crash.rs` |
| P5 | Every project state written by a test is a valid Scripture Burrito | `test` | `assert_valid_burrito` (`crates/tc-store/tests/store/common.rs`); `every_store_a_tc_app_test_builds_is_watched`, `p5_watch_fails_a_test_whose_store_reported_a_problem`, `p5_watch_sees_the_last_save_of_a_project_still_open`, `p5_probe_names_a_planted_bad_md5` (`crates/tc-app/tests/app/p5.rs`) |
| P6 | The helps screens work with Door43 and Aquifer resources | `test` | `crates/tc-helps/tests/aquifer.rs`, `crates/tc-resources/tests/aquifer_install.rs` (pending: AQ1) |
| P7 | The key screens look like tC4's | `test` | `the_committed_record_passes_the_gate` (`crates/tc-ui/tests/screen_record.rs`) (pending: CHK-6, G2) |
| P8 | Arabic and Hebrew render and edit right to left | `journeys`, `e2e-linux`, `e2e-windows`, `e2e-macos` | `arabic_helps_panes_are_rtl_in_an_english_interface`, `rtl_project_drafts_checks_and_aligns`, `graphite_shapes_awami_nastaliq_in_the_shell`, `plain_chromium_fails_the_graphite_probe`, `hebrew_original_aligns_right_to_left_on_jonah` |
| P9 | tC4, tC3, USFM and Burrito projects carry over | — | tC3: `a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project`, `every_tc3_manifest_case_imports_or_refuses_as_tc4_does`, `typed_import_name_is_the_project_name`, `a_cancelled_or_stopped_import_leaves_no_project`, journey `import`; Update a book from USFM: `update_a_book_report_counts_match_the_batch`, `update_a_book_refusal_names_the_line_and_writes_nothing`, journey `update-book`; tC4 4.0, USFM and Burrito wait (pending: DONE) |
| P10 | Old projects keep working across frozen format versions | `test` | `every_kind_has_the_pinned_wire_form`, `the_encoding_is_pinned`, `tokens_have_the_pinned_byte_form_and_key` (pending: V) |
