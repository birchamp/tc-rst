# P-ledger: which CI job and which tests show each of P1 to P10

Read a row as: `docs/PLAN.md`'s promise, the GitHub CI job that runs its checks, and the tests
that show it today; `(pending: <ID>)` names each card that is not done and still owes a part.
`.audit/tools/claims.test.mjs` checks every row: each named test exists, each job is a job of a
workflow, and each pending card is not done.

| P | What it promises | CI job | Tests |
|---|---|---|---|
| P1 | Every live tC4 journey has an automated end-to-end test | `journeys`, `parity-baseline` | `parity/app/journeys/coverage.json` maps each journey and spec (`parity/app/journeys/coverage.test.mjs`); `j4_a_mark_on_dragged_words_a_comment_and_a_bookmark_are_one_decision_record_on_disk`, `j5_align_steps_that_do_not_save` (pending: D1, D2, F1, E1a, E1b, FIX-J, G4, REF1) |
| P2 | Team work: sync, share, open, exchange and conflicts end to end | — | (pending: X, E1a, E1b, E2, C7b) |
| P3 | No silent loss across random concurrent histories | `test` | `fold_converges_under_any_union`, `properties::conservation`, `concurrent_differences_are_visible`, `merge_units_sound`, `resolution_always_possible` (`crates/tc-journal/tests/properties.rs`) |
| P4 | Crash safety at every write point | `test` | `crates/tc-store/tests/store/crash.rs` |
| P5 | Every project state written by a test is a valid Scripture Burrito | `test` | `assert_valid_burrito` (`crates/tc-store/tests/store/common.rs`) (pending: CHK-2) |
| P6 | The helps screens work with Door43 and Aquifer resources | `test` | `crates/tc-helps/tests/aquifer.rs`, `crates/tc-resources/tests/aquifer_install.rs` (pending: AQ1) |
| P7 | The key screens look like tC4's | `test` | `the_committed_record_passes_the_gate` (`crates/tc-ui/tests/screen_record.rs`) (pending: CHK-6, G2) |
| P8 | Arabic and Hebrew render and edit right to left | `journeys` | `arabic_helps_panes_are_rtl_in_an_english_interface` (pending: G4) |
| P9 | tC4, tC3, USFM and Burrito projects carry over | — | (pending: I, F1) |
| P10 | Old projects keep working across frozen format versions | `test` | `every_kind_has_the_pinned_wire_form`, `the_encoding_is_pinned`, `tokens_have_the_pinned_byte_form_and_key` (pending: V) |
