# S2 mutation executor

Each recipe changes one line of `crates/tc-store/src` back to a known bug, in an isolated clone
under `target/mutants/source`, and runs the tests named for it. A recipe is detected only when
every one of its commands fails with the mutant and passes before and after it. The run stops
at the first recipe whose clean "before" run fails.

35 recipes are kept and 11 are retired (2026-10-22).

## How to run

From the repository root, on a clean committed tree, through the gate runner:

```bash
python3 -I tools/mutants/execute.py --owner "$PWD" --base "$(git rev-parse HEAD)" --cases all
```

- `--list` prints the selected recipes; `--check-anchors` checks in memory that every recipe
  still finds the line it mutates (no clone, no Cargo).
- `--cases primary|secondary|new|all`, or a comma-separated list of names.
- Evidence goes to `target/mutants/runs/<time>/`; `report.json` ends with `"complete": true`
  when every selected recipe was detected.
- About 25 minutes for `all` on the owner's PC. CHK-5 runs it nightly in CI.

`recipes.py` holds the two oracle-independence mutations (FALSE_ADOPTION and OMITTED_NOTICE);
`probes.rs` and `PLAN.md` are the 2026-10-20 probe tests and their plan, kept for the record
(the omitted-notice probe was removed with the model's `Damage` step).

## Retired by the 2026-10-21 rulings

Each mutates only code the owner's rulings of 2026-10-21/22 remove (`docs/STORAGE-DELTA-2026-10-21.md`
§3; S3 step 9 deletes the code). They are listed in `RETIRED` in `execute.py` and are not run.

- `e001b9c`: R9's Who::Ambiguous: the rulings make a write's `old` bytes the app's own (delta §3; §4.1 L106, L117).
- `L111`: R9's rule for `old` bytes (delta §4.1 L111).
- `L112`: the metadata.json twin of R9 (delta §4.1 L112).
- `OLD_CARRIED_BLOB`: carried backups, keep_prior_carried's backup half (carried-files ruling CF 2; delta §3).
- `CACHED_OLD_BLOB`: carried backups (CF 2; delta §3).
- `OLD_BACKUP_UNREADABLE`: carried backups (CF 2; delta §3).
- `DURABLE_BACKUP_BYPASS`: copy_blob_to_durable, the durable backup (CF 2; delta §3, §4.1 L135).
- `BACKUP_PARENT_SYNC`: sync_blob_backup (CF 2; delta §3, §4.1 L136).
- `INCOMPLETE_DEDUPE`: record_outside, the batch commit of automatic recording, and the writer's retry of it (delta §3, §16.7's dropped rows); the import carries L127's rule from S3 step 9; its mutant also survives its own test at cbe1bdb (2026-10-22).
- `HOLD_RELIST`: hold()'s metadata.json write matters only for a hold the storage check finds, which only R10's disk-damage hold made (delta §3); carried edits are recorded, not held, and the whole store suite passes under it (2026-10-22).
- `DURABILITY_DEDUPE`: the dedupe now lives only in the Writer map unrecorded_metadata of the metadata.json recording (delta §3; §4.1 L141-L143).

The oracle patch `record-attempt-bound-v1.patch` served only INCOMPLETE_DEDUPE and was not
brought in.

## Background: the 2026-10-20 preparation notes

The text below is the executor's original notes, kept as written; its counts are of that day.

> # S2 isolated mutation executor: prepared, not executed
>
> Initially prepared against product HEAD `a2b2107200a10482701c3be66d41d002fe6381d6`, then
> re-grounded against the owner's edited materialization code while its full precommit run
> was active and process adoption remained pending. No clone, Cargo invocation, mutant or
> new stage/gate tool has run. Python syntax and all 37 mutation anchors were checked in
> memory only. The owner must supply GO and the exact adopted SHA; the script refuses a
> different or dirty owner HEAD. Preparation is not a clean baseline or detection result.
>
> `execute.py` has eleven primary recipes: six pre-redesign mechanisms identified by their
> fix commit, then L108–L112. Secondary recipes are L118/L119/L121/L123/L125. L119 removes
> the entire blocked/missing-listing arm in the refactored `build_group_metadata`, including
> the L124 alternative, and checks the four relevant held/missing-book histories. L110 and
> L111 include the exhaustive ledger matrix. `--cases new` selects twenty-one newly fixed
> mechanisms; `--cases all` selects all 37. Comma-separated explicit cases also work.
>
> All sixteen original recipes (eleven primary and five secondary) additionally require
> the exact `model::random_histories_lose_no_work` test. That test runs 200 generated cases
> after its permanent named histories. Each recipe runs this separate Cargo command once
> before mutation, once with that mutant, and once after restoration. A recipe passes only
> when every required standalone/matrix/bounded command meets its own strict one-test
> criteria. Standalone detection alone cannot satisfy an original recipe. The result records
> each command's proof domain and the required bounded-model configuration.
>
> The twenty-one new cases do not claim the verse/metadata bounded model covers carried/audio,
> salvage or typed Git boundaries. Two carried cases additionally require the separate
> independent carried model described below; other new cases remain standalone named proof.
> The owner's strengthened L125 copy/notice oracle now runs from `histories()` before the
> named histories. The harness does not patch that product/model oracle.
>
> Each trial runs clean named regressions, writes one behavioral mutation only in the
> disposable clone, retains its patch and hashes, runs the named regressions, restores the
> same baseline bytes in a `finally` block, and reruns clean. The original product bytes are
> restored in an outer `finally` block, including for instrumented trials. A detection requires Cargo exit 101,
> the exact named libtest failure, its runtime panic/assertion, and a one-test failed summary.
> Preflight failures, compiler errors, omitted tests and interrupted commands are rejected.
> Reports and exact-command logs remain under `runs/<unique-run>/`; clone stage reports are
> saved only after the checkout is restored clean. A failed proof is recorded as incomplete.
>
> Launch the outer command from the owning worktree through its adopted `run-gates.mjs`
> with `--fresh`, after sourcing `~/.tc-env`. Hash the ignored harness and recipe notes into
> `TC_S2_MUTATION_PLAN_SHA256` before launch, so the outer receipt binds the script inputs.
> The outer runner must hold the original repository heavy lock for the entire harness run:
> the clone has its own Git common directory and thus its own inner gate lock.
>
> The harness will create one local `--no-hardlinks` clone at
> `.enjoy-lane/mutation-executor/source` and an independent target at
> `.enjoy-lane/mutation-executor/target`. It does not download, fetch, push, clean or reset.
> It never edits tracked builder source. It refuses an existing dirty or mismatched clone.
> The launcher must capture stdout/stderr from launch in a unique `.enjoy-logs/*.log`, retain
> its CLI handle, print/register the actual host PID immediately, and unregister after exit.
>
> Example arguments for the outer runner's command (substitute the adopted SHA):
>
> ```text
> python3 .enjoy-lane/mutation-executor/execute.py --owner /home/bot/tc-rst/.worktrees/s2-proof-executor --base ADOPTED_FULL_SHA --cases primary
> ```
>
> Keep permanent named histories enabled for subsequent bounded-model verification. This
> harness explicitly removes `TC_MODEL_NO_NAMED` from the clone test environment and makes
> no generator-only coverage claim.
>
> L118 now bypasses metadata byte classification when its stat looks clean, using the direct
> same-size/same-mtime regression. L125 disables both current unreadable-metadata barriers:
> the early group preflight and final metadata callback. The named held-after-rename history
> now directly snapshots external files and metadata notices around checkpoint/release;
> therefore it is eligible for execution again. The historical `outside` flag is unchanged.
> Its clean baseline and assertion detection are still unproven until GO execution.
>
> | New case | Mutation | Exact regression |
> |---|---|---|
> | SOURCE_ALIAS | snapshot `PendingInfo` before the interrupted-open cleanup loop instead of reading its current relocated source slot | `salvage_sources::salvaging_an_earlier_path_keeps_values_when_the_replaced_pending_is_removed` |
> | DURABILITY_DEDUPE | remember metadata digest before the preservation copy succeeds | `placement_regressions::metadata_copy_failure_is_retried_after_writes_resume` |
> | COPY_EXHAUSTION | overwrite occupied `-999` after all preservation names are tried | `placement_regressions::a_carried_restore_waits_until_outside_metadata_can_be_preserved` |
> | INCOMPLETE_CACHE | cache both unreadable prerenders as complete bytes | `model::an_unreadable_prerender_never_replaces_the_last_copy_of_a_save` |
> | SHORT_RENAME_RETRY | call checked-write callback only before the first rename, rather than after each sharing wait | library `atomic::tests::a_checked_write_rechecks_bytes_after_a_sharing_wait` |
> | DERIVED_SAME_STAT | trust equal size/mtime at the last derived-file look | `last_look::a_same_stat_edit_after_the_group_read_is_kept_at_the_last_look` |
> | CARRIED_SAME_STAT | trust equal size/mtime before hashing the carried replacement target again | `carried_last_look::a_same_stat_carried_edit_during_a_sharing_wait_is_recorded_before_replacement` |
> | INCOMPLETE_DEDUPE | let indexed incomplete/unreadable previous outside changes replace newly recorded ones | `model::an_incomplete_outside_batch_is_recorded_again_without_a_retry_loop` |
> | REMOVAL_LAST_LOOK | permit removal even when its checked callback finds changed bytes | `placement_regressions::a_carried_edit_after_removal_check_is_preserved_before_unlink` |
> | FINAL_METADATA_DENIAL | permit metadata replacement after the final read becomes denied | `placement_regressions::metadata_denied_after_group_preflight_is_preserved_and_retried` |
> | CARRIED_REFUSED_SETTLED | keep `settled: true` in the carried `Written::Refused` state update | `carried_last_look::a_settled_carried_edit_during_a_sharing_wait_is_relisted_before_metadata_changes` |
> | OLD_CARRIED_BLOB | return typed `AlreadyKept` before checking/recovering the old carried blob | `carried::a_replaced_carried_file_recovers_its_lost_old_blob_before_overwrite`, `carried::a_removed_carried_file_recovers_its_lost_blob_before_unlink`, and independent carried model |
> | CACHED_OLD_BLOB | use the original repository's cached `has` result instead of the independent probe (retain the probe for writes) | Unix-only `carried::a_cached_carried_blob_is_recovered_after_its_pack_disappears` |
> | COPY_FLUSH_ORDER | flush an existing identical copy's file before its directory | `placement_regressions::reusing_an_unflushed_metadata_copy_flushes_its_name_before_its_file` |
> | OLD_BACKUP_UNREADABLE | treat an existing old blob as kept without streaming/verifying its bytes | `carried::a_carried_replacement_waits_when_the_old_backup_blob_is_corrupt` and `carried::a_carried_removal_waits_when_its_backup_blob_is_corrupt` |
> | CARRIED_STAT_DENIAL | turn the carried preparation's temporary stat denial into a fatal I/O error | `carried::backup_model::a_temporarily_unreadable_carried_file_defers_and_retries_its_replacement` and independent carried model |
> | RETAG_OLD_BLOB | attach desired blob identity when listing unchanged old ingredient bytes | `carried::a_carried_replacement_waits_when_the_old_backup_blob_is_corrupt` (includes retry after repair) |
> | GIT_WHOLE_BLOB_KIND | let whole-object blob API accept a tree | library `git::tests::blob_apis_reject_non_blob_objects_before_delivering_any_bytes`, requiring the whole-object rejection assertion |
> | GIT_STREAM_BLOB_KIND | let streamed blob API accept a tree | same library test, requiring the separate streamed rejection assertion |
> | DURABLE_BACKUP_BYPASS | implement durable blob read through the non-durable streaming API | library `git::tests::a_durable_loose_blob_read_flushes_only_its_backup_and_parent_names` and `git::tests::a_durable_packed_blob_read_writes_a_pack_of_only_the_affected_blob` |
> | BACKUP_PARENT_SYNC | omit the objects directory from the backup-name flush chain | library `git::tests::a_backup_flush_covers_a_recreated_fanout_and_propagates_every_failure` |
>
> The latest preparation rechecked the counted-hash changes in `classify.rs`; all existing
> anchors still match. `CARRIED_REFUSED_SETTLED` changes only the refusal arm's `Event::Stat`
> to `settled: true`. Besides the exact named failure, its detection requires the direct
> oracle message `the refused carried replacement left a stale settled listing`. A failure
> of an unrelated precondition or a successful compilation cannot count as detection.
>
> All newest recipes are prepared only. `OLD_CARRIED_BLOB` requires both standalone named
> regressions and the exact independent
> `carried::backup_model::bounded_carried_histories_preserve_independent_old_bytes` test to
> fail assertions under the same mutation. `CARRIED_STAT_DENIAL` requires its standalone
> denial/retry test and that independent model. The carried model runs 32 generated cases
> of up to four rounds plus permanent replacement, removal and read-denial seeds. Its
> commands carry their own proof-domain label and require the same clean/mutant/restored
> sequence; the verse/metadata model claim remains separate.
>
> `CACHED_OLD_BLOB` refuses execution on non-Unix hosts, where the named test is omitted.
> Its current mutation retains durable backup-byte verification and uses cached `has` solely
> to decide whether recovery is needed, so a stale cache prevents recovery rather than
> silently proving old bytes. `OLD_BACKUP_UNREADABLE` preserves fresh-probe existence and
> recovery work but returns typed `AlreadyKept` before streaming/durability verification.
> The copy-order recipe changes only the existing-identical-copy branch's two flushes.
> The Git-kind mutations are separate: whole-object rejection must fail for one and streamed
> rejection for the other; compiler errors or the wrong assertion establish neither proof.
>
> The final durability recipes require runtime assertion detection. The bypass must fail
> both exact loose/packed tests, including their affected-backup budgets. The parent-sync
> mutant leaves the objects directory unflushed; the exact FaultFs test's first post-helper
> power-loss assertion requires the recreated fanout's old bytes to survive before checking
> the flush trace and propagation of each I/O failure. An omitted test, compiler error,
> timeout or interrupted command remains incomplete evidence. No optional cases were added
> beyond the two requested final durability recipes.
>
> The incomplete-dedupe spin requires a bounded oracle rather than treating timeout as
> detection. `oracles/record-attempt-bound-v1.patch` adds a counter only under
> `cfg(feature = "test-hooks")` at `Project::record_outside`. Each named test process permits
> 64 calls; call 65 panics with `TC_S2_DEDUPE_ATTEMPT_BOUND`. This identical oracle is present
> for clean, mutant and restored trials, and is retained separately from `mutant.patch` with
> its own patch/source hashes. The case report labels it supplemental instrumented proof,
> not product validation. The bound's prepared patch SHA-256 is
> `82eec44e9f95ddccf094c4e67734660cdac40bd0b683597fd92dcb9a3b4c91c9`.
> It is regenerated against the exact GO revision and its actual hash recorded at execution.
>
> An actor-thread panic may make the model's public operation fail with `ProjectClosed`.
> Detection still requires the exact named libtest failure, Cargo exit 101, runtime panic,
> one-test failure summary, and the unique attempt-bound marker in the retained log. An
> unmarked failure, timeout, compiler error, omitted test or killed process is incomplete
> evidence. No timeout-based assertion was added to product tests.
>
> Read-only preparation check (does not invoke Git, Cargo, clones, stages or gates):
>
> ```text
> python3 .enjoy-lane/mutation-executor/execute.py --owner . --base PREPARATION_ONLY --cases all --check-anchors
> ```
>
> ## 2026-10-21 corrections after the run at 7d3a5d3
>
> - INCOMPLETE_CACHE and REMOVAL_LAST_LOOK: their old named regressions pass under the mutant;
>   the proof fixer's new public tests (`9aa15f3`) detect them, so each case lists only the
>   new test (the executor requires every listed test to fail).
> - CARRIED_REFUSED_SETTLED: the test now asserts the marker before the focus (`9aa15f3`).
> - L111: detected by its named test after the new metadata.json history (`9aa15f3`).
> - SOURCE_ALIAS retired: an equivalent mutant at 7d3a5d3 (preserve_bytes always names salvage
>   copies `<base>~N`, so no salvage replaces a later pending key; analysis by the proof fixer,
>   confirmed by its reviewer). L128's guard has no reaching regression; logged in minors.
> - New: DISMISS_REPLACED, DISMISS_DENIED, HOLD_RELIST, SHOWN_METADATA, SEAL_UNREADABLE,
>   KEPT_HEAD_EDIT, FALSE_ADOPTION, OMITTED_NOTICE.
