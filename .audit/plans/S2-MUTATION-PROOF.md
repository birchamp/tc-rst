# S2 mutation execution queue

Recovered by read-only audit at `85fa0810387c76a0cb1317f2df0bbe2d986fbc92`.
This is a queue, not proof that any mutant has been detected. Original F1–F6 labels could
not be recovered; the six earlier mechanisms are keyed by their actual fix commits below.

Use one isolated executor and separate source/target directories. For each mutation, retain
the exact base revision, diff, command, timing, exit status and log. Run the clean named test,
require the mutated test to fail an assertion, restore, and require the clean test to pass.
A timeout, compiler error or omitted named history is not detection. Run the bounded model
with its permanent named histories enabled. Share all evidence with independent reviewers.

The model command is `cargo test -p tc-store --test store model::<name> -- --exact --nocapture`.

| Case | Mutation | Named model regression |
|---|---|---|
| L108 | `shows_held` accepts indexed but unapplied batch members | `a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file` |
| L109 | `read_base` keeps digests for heads `shows_held` refuses | `a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file` |
| L110 | disable ledger R6/R15 unreadable-projection guards | `a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent` and exhaustive matrix |
| L111 | let `Writing` plus another historical md5 write when holds=true | `a_revert_to_an_earlier_write_of_two_stopped_ones_is_kept` and matrix |
| L112 | skip metadata `read_event` classification while Writing | `a_revert_of_metadata_json_after_a_kill_past_its_rename_is_kept` |
| L118 | bypass clean-stat metadata's decision before overwrite | `a_settled_metadata_json_holding_a_lost_name_is_recorded_before_it_is_written_over` |
| L119 | remove the whole blocked/missing-listing error arm, including L124's alternative | all three held/missing-book writable histories |
| L121 | stop `secure_dir_chain` at a directory this session created | `a_pack_in_a_folder_a_killed_seal_made_survives_a_power_cut` |
| L123 | retain scope when represented md5 changes in `set_state` | `a_metadata_json_found_after_a_kill_past_its_rename_is_never_taken_for_converged` |
| L125 | permit unreadable unsettled metadata to start another write | existing named test needs a direct no-extra-copy/notice assertion |

The L125 model history begins with an outside revert. That sets `Run.outside`, disabling
`kept_nothing_twice`, so its current `assert_kept` checks no false metadata notice/copy.
Strengthen that assertion before treating a surviving L125 mutant as a product regression.
Historical generator-only runs used `TC_MODEL_NO_NAMED`; they cannot establish coverage of
the permanent deterministic cases.

| Earlier fix/mechanism | Direct store regression and mutation |
|---|---|
| `e293bb8`, pending removed during seal | `checkpoint::a_pending_file_removed_outside_during_the_seal_ends_the_seal_as_removed`; remove not-found acceptance in atomic removal |
| `ab681e1` / L104, damaged pending memory salvage | `checkpoint::a_pending_file_cut_inside_a_line_is_sealed_from_memory` plus changed-line variant; disable `lines_in_memory` salvage |
| `1c38c6c` / L105, file holds journal-lost save | checkpoint book/deleted-pending/metadata regressions; make `holds` always true |
| `540da13` / L107, unreadable unsettled path | `groups::a_group_keeps_the_intent_of_a_path_it_could_not_read`; turn unreadable read into WriteFailed |
| `e001b9c` / L106, restart revert | `groups::a_revert_made_while_the_app_is_closed_after_a_crash_past_the_rename_is_recorded` and old-time model cases; trust Writing.old as ours |
| `67fcea1`, redundant journal file held | `held::a_pending_file_the_seal_could_not_remove_is_no_second_writer` plus open variant; drop `journal.left` exclusion from unknown-file listing |
