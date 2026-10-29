#!/usr/bin/env python3
"""Isolated S2 mutation proof; launch only after the owner supplies GO and adopted SHA.

Launch from the owner through its run-gates runner, keeping that repository's heavy
lock while this clone's runner produces exact-command logs for each trial.
"""

import argparse
import datetime as dt
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid


def regression(name):
    return ["cargo", "test", "-p", "tc-store", "--offline", "--test", "store", name,
            "--", "--exact", "--nocapture"]


def matrix():
    return ["cargo", "test", "-p", "tc-store", "--offline", "--lib",
            "ledger::tests::every_pair_of_the_table_keeps_every_byte", "--", "--exact", "--nocapture"]


def library_regression(name):
    return ["cargo", "test", "-p", "tc-store", "--offline", "--lib", name,
            "--", "--exact", "--nocapture"]


BOUNDED_MODEL_TEST = "model::random_histories_lose_no_work"


def bounded_model():
    return regression(BOUNDED_MODEL_TEST)


CASES = {
    "e293bb8": {"path": "atomic.rs", "tests": [
        "checkpoint::a_pending_file_removed_outside_during_the_seal_ends_the_seal_as_removed"]},
    "ab681e1": {"path": "seal.rs", "tests": [
        "checkpoint::a_pending_file_cut_inside_a_line_is_sealed_from_memory",
        "checkpoint::a_pending_file_changed_inside_a_line_is_sealed_from_memory_and_kept_in_quarantine"]},
    "1c38c6c": {"path": "classify.rs", "tests": [
        "checkpoint::a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open",
        "checkpoint::a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded",
        "checkpoint::metadata_json_holding_a_project_detail_whose_pending_file_was_deleted_is_recorded"]},
    "540da13": {"path": "materialize.rs", "tests": [
        "groups::a_group_keeps_the_intent_of_a_path_it_could_not_read"]},
    "67fcea1": {"path": "seal.rs", "tests": [
        "held::a_pending_file_the_seal_could_not_remove_is_no_second_writer",
        "held::a_pending_file_the_open_could_not_remove_is_no_second_writer"]},
    "L108": {"path": "classify.rs", "tests": [
        "model::a_batch_cut_at_a_line_while_closed_is_kept_from_the_book_file"]},
    "L109": {"path": "outside.rs", "tests": [
        "model::a_salvaged_change_whose_batch_lost_a_member_is_kept_from_the_book_file"]},
    "L110": {"path": "ledger.rs", "tests": [
        "model::a_head_whose_value_can_no_longer_be_read_is_never_written_as_absent"], "matrix": True},
    "L118": {"path": "materialize.rs", "tests": [
        "placement_regressions::a_same_size_same_mtime_edit_of_settled_metadata_is_kept_before_replacement"]},
    "L119": {"path": "materialize.rs", "tests": [
        "model::a_held_book_file_with_its_manifest_lost_leaves_the_project_writable",
        "model::a_deleted_book_file_whose_path_is_held_leaves_the_project_writable",
        "model::a_held_edited_book_file_with_its_manifest_lost_leaves_the_project_writable",
        "model::a_deleted_book_file_waiting_for_an_unreadable_value_fails_no_checkpoint"]},
    "L121": {"path": "counters.rs", "tests": [
        "model::a_pack_in_a_folder_a_killed_seal_made_survives_a_power_cut"]},
    "L123": {"path": "classify.rs", "tests": [
        "model::a_metadata_json_found_after_a_kill_past_its_rename_is_never_taken_for_converged"]},
    "L125": {"path": "materialize.rs", "tests": [
        "model::a_held_metadata_json_after_a_kill_past_its_rename_is_not_taken_for_an_edit"]},
    "COPY_EXHAUSTION": {"path": "materialize.rs", "tests": [
        "placement_regressions::a_carried_restore_waits_until_outside_metadata_can_be_preserved"]},
    "INCOMPLETE_CACHE": {"path": "classify.rs", "tests": [
        "placement_regressions::a_rendering_that_could_not_read_a_save_never_replaces_its_last_readable_copy"]},
    "SHORT_RENAME_RETRY": {"path": "atomic.rs", "tests": [], "lib_tests": [
        "atomic::tests::a_checked_write_rechecks_bytes_after_a_sharing_wait"]},
    "DERIVED_SAME_STAT": {"path": "materialize.rs", "tests": [
        "last_look::a_same_stat_edit_after_the_group_read_is_kept_at_the_last_look"]},
    "CARRIED_SAME_STAT": {"path": "carried.rs", "tests": [
        "carried_last_look::a_same_stat_carried_edit_during_a_sharing_wait_is_recorded_before_replacement"]},
    "REMOVAL_LAST_LOOK": {"path": "materialize.rs", "tests": [
        "placement_regressions::a_book_file_saved_during_the_sharing_wait_of_its_removal_is_kept"]},
    "FINAL_METADATA_DENIAL": {"path": "materialize.rs", "tests": [
        "placement_regressions::metadata_denied_after_group_preflight_is_preserved_and_retried"]},
    "CARRIED_REFUSED_SETTLED": {"path": "carried.rs", "tests": [
        "carried_last_look::a_settled_carried_edit_during_a_sharing_wait_is_relisted_before_metadata_changes"],
        "failure_marker": "the refused carried replacement left a stale settled listing"},
    "COPY_FLUSH_ORDER": {"path": "materialize.rs", "tests": [
        "placement_regressions::reusing_an_unflushed_metadata_copy_flushes_its_name_before_its_file"]},
    "CARRIED_STAT_DENIAL": {"path": "carried.rs", "tests": [
        "carried::a_carried_file_that_cannot_be_read_waits_for_its_replacement"]},
    "RETAG_OLD_BLOB": {"path": "materialize.rs", "tests": [
        "placement_regressions::a_carried_restore_waits_until_outside_metadata_can_be_preserved"]},
    "GIT_WHOLE_BLOB_KIND": {"path": "git.rs", "tests": [], "lib_tests": [
        "git::tests::blob_apis_reject_non_blob_objects_before_delivering_any_bytes"],
        "failure_marker": "whole-object reads must reject a tree"},
    "GIT_STREAM_BLOB_KIND": {"path": "git.rs", "tests": [], "lib_tests": [
        "git::tests::blob_apis_reject_non_blob_objects_before_delivering_any_bytes"],
        "failure_marker": "streamed reads must reject a tree"},
    "DISMISS_REPLACED": {"path": "writer.rs", "tests": [
        "drafts::dismissing_an_older_copy_never_deletes_its_unseen_replacement"]},
    "DISMISS_DENIED": {"path": "writer.rs", "tests": [
        "drafts::a_copy_stat_denial_cannot_turn_dismissal_into_success"]},
    "SHOWN_METADATA": {"path": "outside.rs", "tests": [
        "model::a_saved_name_whose_last_copy_is_metadata_while_the_book_is_missing_is_kept"],
        "bounded_model": True},
    # The two oracle-independence mutants of 2026-10-20: the repaired generic model must
    # now detect them (they passed it before the repair: oracle-before-logs).
    "SEAL_UNREADABLE": {"path": "seal.rs", "tests": [
        "checkpoint::a_save_whose_pending_file_the_seal_cannot_read_is_sealed_from_memory",
        "model::a_pending_file_cut_twice_around_a_revert_and_a_held_open_is_kept"],
        "bounded_model": True},
    "KEPT_HEAD_EDIT": {"path": "outside.rs", "tests": [
        "outside::an_edit_back_to_a_kept_head_after_a_lost_one_is_recorded"]},
    "FALSE_ADOPTION": {"path": "classify.rs", "tests": [
        "model::oracle_independence_false_adoption"], "bounded_model": True},
    # Found by the long model on 2026-10-22 (S2 step 2): every damaged copy is salvaged.
    "SALVAGE_ALL_COPIES": {"path": "packs.rs", "tests": [
        "model::a_save_only_a_damaged_committed_pack_holds_is_salvaged"]},
    # Found by the bounded model on 2026-10-22 (S2 step 2): an adopted book file must be listed.
    "ADOPTED_METADATA": {"path": "classify.rs", "tests": [
        "model::a_book_revert_after_a_kill_past_its_rename_and_a_lost_pending_file_leaves_a_valid_burrito"]},
    # Without a held book file (R10 is retired) the model cannot show a missing notice; the
    # N4 copies with their notices can.
    "OMITTED_NOTICE": {"path": "writer.rs", "tests": [
        "checkpoint::a_book_file_holding_a_save_its_pending_file_lost_in_place_is_recorded_at_the_next_open",
        "checkpoint::a_book_file_holding_a_save_whose_pending_file_was_deleted_before_a_crash_is_recorded",
        "checkpoint::a_book_file_whose_heads_were_put_back_is_recorded_not_written_over"]},
}
# Retired by the owner's rulings of 2026-10-21/22: each mutates only code they remove
# (S3 step 9 deletes it). Not run; listed for the record.
RETIRED = {
    "e001b9c": "R9's Who::Ambiguous: the rulings make a write's `old` bytes the app's own (delta §3; §4.1 L106, L117)",
    "L111": "R9's rule for `old` bytes (delta §4.1 L111)",
    "L112": "the metadata.json twin of R9 (delta §4.1 L112)",
    "OLD_CARRIED_BLOB": "carried backups, keep_prior_carried's backup half (carried-files ruling CF 2; delta §3)",
    "CACHED_OLD_BLOB": "carried backups (CF 2; delta §3)",
    "OLD_BACKUP_UNREADABLE": "carried backups (CF 2; delta §3)",
    "DURABLE_BACKUP_BYPASS": "copy_blob_to_durable, the durable backup (CF 2; delta §3, §4.1 L135)",
    "BACKUP_PARENT_SYNC": "sync_blob_backup (CF 2; delta §3, §4.1 L136)",
    "INCOMPLETE_DEDUPE": "record_outside, the batch commit of automatic recording, and the writer's retry of it (delta §3, §16.7's dropped rows); the import carries L127's rule from S3 step 9; its mutant also survives its own test at cbe1bdb (2026-10-22)",
    "HOLD_RELIST": "hold()'s metadata.json write matters only for a hold the storage check finds, which only R10's disk-damage hold made (delta §3); carried edits are recorded, not held, and the whole store suite passes under it (2026-10-22)",
    "DURABILITY_DEDUPE": "the dedupe now lives only in the Writer map unrecorded_metadata of the metadata.json recording (delta §3; §4.1 L141-L143)",
}
PRIMARY = ['e293bb8', 'ab681e1', '1c38c6c', '540da13', '67fcea1', 'L108', 'L109', 'L110']
SECONDARY = ["L118", "L119", "L121", "L123", "L125"]
NEW = ['COPY_EXHAUSTION', 'INCOMPLETE_CACHE', 'SHORT_RENAME_RETRY', 'DERIVED_SAME_STAT', 'CARRIED_SAME_STAT', 'REMOVAL_LAST_LOOK', 'FINAL_METADATA_DENIAL', 'CARRIED_REFUSED_SETTLED', 'COPY_FLUSH_ORDER', 'CARRIED_STAT_DENIAL', 'RETAG_OLD_BLOB', 'GIT_WHOLE_BLOB_KIND', 'GIT_STREAM_BLOB_KIND', 'DISMISS_REPLACED', 'DISMISS_DENIED', 'SHOWN_METADATA', 'SEAL_UNREADABLE', 'KEPT_HEAD_EDIT', 'FALSE_ADOPTION', 'SALVAGE_ALL_COPIES', 'ADOPTED_METADATA', 'OMITTED_NOTICE']
for original_case in PRIMARY + SECONDARY:
    CASES[original_case]["bounded_model"] = True


def commands_for(spec):
    commands = [regression(test) for test in spec["tests"]]
    commands.extend(library_regression(test) for test in spec.get("lib_tests", []))
    if spec.get("matrix"):
        commands.append(matrix())
    if spec.get("bounded_model"):
        commands.append(bounded_model())
    return commands


def proof_domain(command):
    if command == bounded_model():
        return "bounded-model-200-with-named-histories"
    if command == matrix():
        return "exhaustive-ledger-matrix"
    return "standalone-named-regression"


def once(source, before, after, expected=1):
    count = source.count(before)
    if count != expected:
        raise ValueError(f"mutation anchor count {count}, expected {expected}: {before!r}")
    return source.replace(before, after)


def function_body(source, signature, body):
    start = source.index("{", source.index(signature))
    depth = 1
    end = start + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[:start + 1] + "\n" + body + "\n    " + source[end - 1:]


def in_function(source, signature, transform):
    start = source.index("{", source.index(signature))
    depth = 1
    end = start + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[:start + 1] + transform(source[start + 1:end - 1]) + source[end - 1:]


def oracle_patch(name, source):
    if name != "record-attempt-bound-v1":
        raise ValueError(name)
    return in_function(source, "pub(crate) fn record_outside(", lambda body: once(
        body, "        use crate::writer::Recorded;",
        "        use crate::writer::Recorded;\n"
        "        #[cfg(feature = \"test-hooks\")]\n"
        "        {\n"
        "            static ATTEMPTS: std::sync::atomic::AtomicUsize =\n"
        "                std::sync::atomic::AtomicUsize::new(0);\n"
        "            let attempts = ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;\n"
        "            assert!(attempts <= 64, \"TC_S2_DEDUPE_ATTEMPT_BOUND: {attempts} record_outside calls\");\n"
        "        }"))


def mutate(case, source):
    if case == "e293bb8":
        return once(source, "vfs.is_held(io) || is_not_found(io)",
                    "vfs.is_held(io) || (false && is_not_found(io))")
    if case == "ab681e1":
        return once(source, "let raw = self.fold.raw(id)?;",
                    "let raw = self.fold.raw(id).filter(|_| false)?;")
    if case == "1c38c6c":
        return function_body(source, "pub(crate) fn holds(", "        let _ = (rel, md5);\n        Some(true)")
    if case == "540da13":
        return once(source, "self.note(&path, Event::Read(ledger::Disk::Unreadable));",
                    "self.note(&path, Event::WriteFailed);", expected=2)
    if case == "e001b9c":
        return once(source,
                    "State::Writing { old: Some(old), .. } if old.md5 == md5 => Who::Ambiguous,",
                    "State::Writing { old: Some(old), .. } if old.md5 == md5 => Who::Ours(old.clone()),")
    if case == "67fcea1":
        return once(source, "            .filter(|f| !self.journal.left.contains(&f.rel))\n", "")
    if case == "L108":
        return once(source, "self.fold.change(id).is_some_and(|view| view.folded)",
                    "self.fold.change(id).is_some()")
    if case == "L109":
        return once(source, "let lost = heads.len() != named;",
                    "let lost = heads.len() > named;")
    if case == "L110":
        source = once(source, "_ if !facts.heads_readable => (", "_ if false && !facts.heads_readable => (")
        return once(source, "Projection::Bytes { .. } if !facts.heads_readable => (",
                    "Projection::Bytes { .. } if false && !facts.heads_readable => (")
    if case == "L111":
        before = "        State::Writing { .. } => Who::Other {\n"
        after = ("        State::Writing { .. } if facts.holds == Some(true) =>\n"
                 "            Who::Ours(Placement::renamed(md5.to_owned(), 0)),\n" + before)
        return once(source, before, after)
    if case == "L112":
        return once(source, "        for action in self.read_event(METADATA, disk_md5, &facts)? {",
                    "        let actions = if matches!(self.state_of(METADATA), State::Writing { .. }) {\n"
                    "            Vec::new()\n"
                    "        } else {\n"
                    "            self.read_event(METADATA, disk_md5, &facts)?\n"
                    "        };\n"
                    "        for action in actions {")
    if case == "L118":
        return in_function(source, "fn look_at_metadata(", lambda body: once(
            body, "        let d = self.hash(&disk);",
            "        let d = self.hash(&disk);\n"
            "        if self.clean_by_stat(METADATA) {\n"
            "            return Ok(Some((meta, Some(d))));\n"
            "        }"))
    if case == "L119":
        def remove_wait_arm(body):
            start = body.index("            Err(_)\n                if !self.reasons.is_empty()")
            end = body.index("            Err(e) => Err(e),", start)
            return body[:start] + body[end:]
        return in_function(source, "fn build_group_metadata(", remove_wait_arm)
    if case == "L121":
        return once(source, "while at != root && !secured.contains(at) {",
                    "while at != root && !secured.contains(at) && !made.iter().any(|m| m == at) {")
    if case == "L123":
        return once(source, "if md5(&entry.state) != md5(&state) {",
                    "if false && md5(&entry.state) != md5(&state) {")
    if case == "L125":
        def permit_preflight(body):
            start = body.index("            Disk::Unreadable => {")
            end = body.index("\n        };\n        let wanted", start)
            return body[:start] + "            Disk::Unreadable => MetadataRead::Missing," + body[end:]
        source = in_function(source, "fn run_group(", permit_preflight)
        return in_function(source, "fn last_look_at_metadata(", lambda body: once(
            body,
            "            Disk::Unreadable => {\n"
            "                self.note(METADATA, Event::Read(ledger::Disk::Unreadable));\n"
            "                Ok(false)\n"
            "            }",
            "            Disk::Unreadable => Ok(true),"))
    if case == "SOURCE_ALIAS":
        before = ("        let pending: Vec<String> = self.journal.pending.keys().cloned().collect();\n"
                  "        for rel in pending {\n"
                  "            // Salvaging an earlier path can replace this pending file and its source slot.\n"
                  "            // Relocate the current slot before removal, including aliases from that salvage.\n"
                  "            let Some(info) = self.journal.pending.get(&rel).cloned() else {\n"
                  "                continue;\n"
                  "            };")
        after = ("        let pending: Vec<(String, PendingInfo)> = self.journal.pending.iter()\n"
                 "            .map(|(rel, info)| (rel.clone(), info.clone())).collect();\n"
                 "        for (rel, info) in pending {")
        return once(source, before, after)
    if case == "DURABILITY_DEDUPE":
        return once(source,
                    "        let copy = self.copy_external(METADATA, disk)?;\n"
                    "        self.copied_outside.insert(md5);",
                    "        self.copied_outside.insert(md5);\n"
                    "        let copy = self.copy_external(METADATA, disk)?;")
    if case == "COPY_EXHAUSTION":
        return once(source,
                    "        Err(StoreError::Other(format!(\n"
                    "            \"could not preserve {rel}: all external copy names are occupied; earlier copies were kept\"\n"
                    "        )))",
                    "        let name = format!(\"{base}-999\");\n"
                    "        atomic_write(&*self.vfs, &dir, &name, bytes, SyncDir::Now)\n"
                    "            .map_err(WriteError::into_inner)?;\n"
                    "        Ok(format!(\"{EXTERNAL}/{name}\"))")
    if case == "INCOMPLETE_CACHE":
        return once(source, "readable.then_some(new)", "Some(new)", expected=2)
    if case == "SHORT_RENAME_RETRY":
        return once(source,
                    "                let mut waits = retry.0.iter();\n"
                    "                loop {\n"
                    "                    if !before_rename()? {\n"
                    "                        return Ok(false);\n"
                    "                    }\n"
                    "                    match vfs.rename(&temp, &target) {",
                    "                if !before_rename()? {\n"
                    "                    return Ok(false);\n"
                    "                }\n"
                    "                let mut waits = retry.0.iter();\n"
                    "                loop {\n"
                    "                    match vfs.rename(&temp, &target) {")
    if case == "DERIVED_SAME_STAT":
        def trust_stat(body):
            body = once(body, "        let abs = self.folder.path(rel);",
                        "        let abs = self.folder.path(rel);\n"
                        "        let seen_stat = self.vfs.metadata(&abs).ok().map(|m| (m.len, m.mtime_ns));")
            return once(body, "            self.shared.hooks.wait(crate::hooks::Point::BeforeRename);",
                        "            self.shared.hooks.wait(crate::hooks::Point::BeforeRename);\n"
                        "            if self.vfs.metadata(&abs).ok().map(|m| (m.len, m.mtime_ns)) == seen_stat {\n"
                        "                return Ok(true);\n"
                        "            }")
        return in_function(source, "fn write_once(", trust_stat)
    if case == "CARRIED_SAME_STAT":
        def trust_stat(body):
            body = once(body, "        let streamed: Cell<Streamed> = Cell::new(Streamed::Nothing);",
                        "        let streamed: Cell<Streamed> = Cell::new(Streamed::Nothing);\n"
                        "        let seen_stat = self.vfs.metadata(&self.folder.path(&p.path))\n"
                        "            .ok().map(|m| (m.len, m.mtime_ns));")
            return once(body, "                Ok(match self.hash_file(&p.path) {",
                        "                if self.vfs.metadata(&self.folder.path(&p.path))\n"
                        "                    .ok().map(|m| (m.len, m.mtime_ns)) == seen_stat {\n"
                        "                    return Ok(true);\n"
                        "                }\n"
                        "                Ok(match self.hash_file(&p.path) {")
        return in_function(source, "fn execute_carried_one(", trust_stat)
    if case == "INCOMPLETE_DEDUPE":
        return once(source, "if !view.folded || self.fold.raw(id).is_none() {",
                    "if false && (!view.folded || self.fold.raw(id).is_none()) {")
    if case == "REMOVAL_LAST_LOOK":
        return in_function(source, "fn run_group(", lambda body: once(
            body, "let mut permitted =", "let mut permitted = true ||"))
    if case == "FINAL_METADATA_DENIAL":
        return in_function(source, "fn last_look_at_metadata(", lambda body: once(
            body, "                Ok(false)", "                Ok(true)"))
    if case == "CARRIED_REFUSED_SETTLED":
        return in_function(source, "fn execute_carried_one(", lambda body: once(
            body,
            "                        crate::ledger::Event::Stat {\n"
            "                            stat,\n"
            "                            settled: false,\n"
            "                        },",
            "                        crate::ledger::Event::Stat {\n"
            "                            stat,\n"
            "                            settled: true,\n"
            "                        },"))
    if case == "OLD_CARRIED_BLOB":
        return function_body(source, "pub(crate) fn keep_prior_carried(",
                             "        let _ = (rel, expected);\n        Ok(PriorBytes::AlreadyKept)")
    if case == "CACHED_OLD_BLOB":
        return in_function(source, "pub(crate) fn keep_prior_carried(", lambda body: once(
            body, "let recovered = !probe.has(oid);",
            "let recovered = !git.has(oid);"))
    if case == "COPY_FLUSH_ORDER":
        return in_function(source, "pub(crate) fn copy_external(", lambda body: once(
            body, ".sync_dir(&dir)\n                        .and_then(|()| self.vfs.sync_file(&path))",
            ".sync_file(&path)\n                        .and_then(|()| self.vfs.sync_dir(&dir))"))
    if case == "OLD_BACKUP_UNREADABLE":
        def skip_readable_backup(body):
            start = body.index("            let mut digest = Md5Stream::new();")
            return (body[:start] + "            let _ = (probe, expected_size);\n"
                    "            Ok(PriorBytes::AlreadyKept)\n        ")
        return in_function(source, "let kept = (|| -> Result<PriorBytes, StoreError>",
                           skip_readable_backup)
    if case == "CARRIED_STAT_DENIAL":
        return in_function(source, "fn prepare_one(", lambda body: once(
            body,
            "            Err(_) => {\n"
            "                self.note(\n"
            "                    &p.path,\n"
            "                    crate::ledger::Event::Read(crate::ledger::Disk::Unreadable),\n"
            "                );\n"
            "                self.reasons\n"
            "                    .insert(p.path.clone(), tc_journal::fold::Blocked::FileUnreadable);\n"
            "                report.blocked.push(p.path.clone());\n"
            "                return Ok(None);\n"
            "            }",
            "            Err(e) => return Err(StoreError::io(\"stat\", &path, e)),"))
    if case == "RETAG_OLD_BLOB":
        return in_function(source, "fn listing_of_file(", lambda body: once(
            body, "self.take_listed(rel, md5, size, entry.oid.clone());",
            "self.take_listed(rel, md5, size, oid);"))
    if case == "GIT_WHOLE_BLOB_KIND":
        return in_function(source, "fn whole_blob<'o>(", lambda body: once(
            body, "Ok(object) if object.kind() == ObjectType::Blob =>",
            "Ok(object) if true || object.kind() == ObjectType::Blob =>"))
    if case == "GIT_STREAM_BLOB_KIND":
        return in_function(source, "pub fn copy_blob_to(", lambda body: once(
            body, "Ok((mut reader, _, ObjectType::Blob)) => {",
            "Ok((mut reader, _, kind)) if true || kind == ObjectType::Blob => {"))
    if case == "DURABLE_BACKUP_BYPASS":
        return function_body(source, "pub fn copy_blob_to_durable(",
                             "        self.copy_blob_to(oid, out)")
    if case == "BACKUP_PARENT_SYNC":
        return in_function(source, "fn sync_blob_backup(", lambda body: once(
            body, "    for dir in [leaf, objects, git_dir, project] {",
            "    let _ = objects;\n    for dir in [leaf, git_dir, project] {"))
    if case == "DISMISS_REPLACED":
        return once(source, "CopyIdentity::Verified(expected) if actual == expected => Ok(true),",
                    "CopyIdentity::Verified(_) if actual == actual => Ok(true),")
    if case == "DISMISS_DENIED":
        return once(source, "Err(e) => Err(StoreError::io(\"read the copy\", &path, e)),",
                    "Err(_) => {\n                        missing = true;\n                        Ok(false)\n                    }")
    if case == "HOLD_RELIST":
        return once(source, "        self.dirty.metadata = true;\n        self.shared.remove_notices_of(rel);",
                    "        self.shared.remove_notices_of(rel);")
    if case == "SHOWN_METADATA":
        return once(source, "let expected = self.shown_metadata().map_err(|e| e.to_string())?;",
                    "let expected = self\n            .build_metadata(&std::collections::BTreeMap::new(), &[])\n            .map_err(|e| e.to_string())?;")
    if case == "KEPT_HEAD_EDIT":
        return once(source, "            let edited = lost\n", "            let edited = false && lost\n")
    if case == "SALVAGE_ALL_COPIES":
        return once(source, "                self.salvage_all_copies(&rel, found.as_deref())?;\n",
                    "                let _ = &found;\n")
    if case == "ADOPTED_METADATA":
        return once(source,
                    "                    // `metadata.json` lists each ingredient's checksum (I7).\n"
                    "                    self.dirty.metadata = true;\n", "")
    if case == "SEAL_UNREADABLE":
        return once(source, "                    self.seal_unreadable(run, rel, &reason);",
                    "                    self.leave_unsealed(run, &rel, &reason);")
    if case in ("FALSE_ADOPTION", "OMITTED_NOTICE"):
        import importlib.util
        spec = importlib.util.spec_from_file_location(
            "recipes", Path(__file__).with_name("recipes.py"))
        recipes = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(recipes)
        return recipes.mutate("FA" if case == "FALSE_ADOPTION" else "ON", source)
    raise ValueError(case)


def now():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def write_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w") as out:
        json.dump(value, out, indent=2)
        out.write("\n")
        out.flush()
        os.fsync(out.fileno())
    temporary.replace(path)


def git(source, *args):
    return subprocess.check_output(["git", "-C", str(source), *args], text=True).strip()


def invoke(command, source, destination, environment):
    started = now()
    clock = time.monotonic()
    with destination.open("w") as log:
        process = subprocess.run(command, cwd=source, env=environment, text=True,
                                 stdout=log, stderr=subprocess.STDOUT)
    result = {"command": command, "cwd": str(source), "startedAt": started,
              "finishedAt": now(), "durationMs": round((time.monotonic() - clock) * 1000),
              "exitCode": process.returncode, "log": str(destination),
              "logSha256": hashlib.sha256(destination.read_bytes()).hexdigest()}
    return result


def gate(command, source, output, environment):
    result = invoke(["node", "scripts/tools/run-gates.mjs", "--fresh", "--jobs", "6", "--", *command],
                    source, output, environment)
    text = output.read_text()
    events = []
    for line in text.splitlines():
        try:
            event = json.loads(line)
        except (ValueError, TypeError):
            continue
        if isinstance(event, dict) and event.get("type") in ("start", "finish"):
            events.append(event)
    result["gateEvents"] = events
    result["testCommand"] = command
    return result, text


def outcome(result, log, expect_failure, failure_marker=None):
    finishes = [event for event in result["gateEvents"] if event["type"] == "finish"]
    if len(finishes) != 1:
        return False, "gate did not finish exactly one command"
    finish = finishes[0]
    if finish.get("signal") or finish.get("interrupted") or not finish.get("unchanged"):
        return False, "interrupted command or inputs changed"
    if expect_failure:
        named = result["testCommand"][-4]
        # libtest and a runtime assertion/panic must both identify a failure. Compiler errors,
        # preflight denials, zero matching tests, process kills and timeouts are not detection.
        failed_test = re.search(r"test " + re.escape(named) + r"\s+\.\.\. FAILED", log)
        assertion = "panicked at" in log
        summary = re.search(r"test result: FAILED\. 0 passed; 1 failed;", log)
        detected = (finish.get("exitCode") == 101 and failed_test and assertion and summary
                    and not re.search(r"error(?:\[E\d+\])?: could not compile", log)
                    and (failure_marker is None or failure_marker in log))
        return bool(detected), "runtime assertion detected mutant" if detected else "mutant survived or evidence is invalid"
    passed = (result["exitCode"] == 0 and finish.get("exitCode") == 0
              and re.search(r"test result: ok\. 1 passed; 0 failed;", log))
    return bool(passed), "clean named regression passed" if passed else "clean named regression failed or was omitted"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--owner", type=Path, required=True)
    parser.add_argument("--base", required=True)
    parser.add_argument("--cases", default="primary")
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--check-anchors", action="store_true",
                        help="read source and validate transforms in memory; no clone, Cargo, Git or gate calls")
    args = parser.parse_args()
    selected = (PRIMARY if args.cases == "primary" else SECONDARY if args.cases == "secondary"
                else NEW if args.cases == "new" else list(CASES) if args.cases == "all"
                else args.cases.split(","))
    if any(case not in CASES for case in selected):
        parser.error("unknown mutation case")
    if args.list:
        print(json.dumps({case: CASES[case] for case in selected}, indent=2))
        return 0
    if args.check_anchors:
        checks = []
        for case in selected:
            spec = CASES[case]
            original = (args.owner / "crates/tc-store/src" / spec["path"]).read_text()
            baseline = oracle_patch(spec["oracle"], original) if spec.get("oracle") else original
            mutated = mutate(case, baseline)
            if mutated == baseline:
                raise RuntimeError(case + ": identical mutation")
            checks.append({"case": case, "anchors": "match", "executionProof": False})
        print(json.dumps(checks, indent=2))
        return 0
    if os.name != "posix" and any(CASES[case].get("platform") == "unix" for case in selected):
        parser.error("selected Unix-only regression cannot establish proof on this host")
    owner = args.owner.resolve()
    if git(owner, "rev-parse", "HEAD") != args.base:
        raise RuntimeError("owner HEAD differs from exact GO revision")
    if git(owner, "status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("owner checkout is dirty at executor start")
    base = owner / "target/mutants"
    source = base / "source"
    target = base / "target"
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + "-" + uuid.uuid4().hex[:8]
    evidence = base / "runs" / run_id
    evidence.mkdir(parents=True, exist_ok=False)
    environment = dict(os.environ)
    environment["CARGO_TARGET_DIR"] = str(target)
    environment["CARGO_NET_OFFLINE"] = "true"
    # The bounded model must judge each mutant by the same histories (FUNNEL §5: fixed seeds
    # in checks, random ones in the nightly job), or a fresh random find stops the run.
    environment.setdefault("PROPTEST_RNG_SEED", "20261022001")
    environment.pop("TC_MODEL_NO_NAMED", None)
    print(json.dumps({"event": "executor-start", "pid": os.getpid(), "base": args.base,
                      "source": str(source), "target": str(target), "evidence": str(evidence)}), flush=True)
    if not source.exists():
        clone = invoke(["git", "clone", "--local", "--no-hardlinks", "--no-checkout", str(owner), str(source)],
                       owner, evidence / "clone.log", environment)
        if clone["exitCode"]:
            raise RuntimeError("local clone failed: " + clone["log"])
        checkout = invoke(["git", "checkout", "--detach", args.base], source,
                          evidence / "checkout.log", environment)
        if checkout["exitCode"]:
            raise RuntimeError("clone checkout failed: " + checkout["log"])
    if git(source, "rev-parse", "HEAD") != args.base or git(source, "status", "--porcelain=v1", "--untracked-files=all"):
        raise RuntimeError("isolated source is not clean at exact GO revision; preserve it for inspection")
    if source.resolve() == owner or target.resolve() == (owner / "target").resolve():
        raise RuntimeError("source or target isolation failed")
    if not (source / "scripts/tools/run-gates.mjs").is_file():
        raise RuntimeError("GO revision lacks adopted gate tools")
    report = {"sha": args.base, "summary": "S2 mutation proof in progress; incomplete evidence is not a pass.",
              "startedAt": now(), "executorPid": os.getpid(), "cases": [], "evidence": str(evidence)}
    write_json(evidence / "report.json", report)
    for case in selected:
        spec = CASES[case]
        stage = "mutation-" + case.lower()
        folder = evidence / case
        folder.mkdir()
        invoke(["node", "scripts/tools/lane-state.mjs", "read", str(source), "S2", stage],
               source, folder / "stage-read.log", environment)
        beginning = invoke(["node", "scripts/tools/lane-state.mjs", "begin", str(source), "S2", stage],
                           source, folder / "stage-begin.log", environment)
        if beginning["exitCode"]:
            raise RuntimeError("could not begin mutation stage")
        commands = commands_for(spec)
        case_report = {"case": case, "base": args.base, "startedAt": now(), "commands": [], "detected": False}
        if spec.get("bounded_model"):
            case_report["requiredBoundedModel"] = {"test": BOUNDED_MODEL_TEST, "cases": 200,
                                                   "namedHistoriesEnabled": True,
                                                   "cleanBeforeMutantAndRestoredRequired": True}
        path = source / "crates/tc-store/src" / spec["path"]
        original = path.read_bytes()
        baseline = original
        relative = path.relative_to(source).as_posix()
        def diff(before, after):
            return "".join(difflib.unified_diff(before.decode().splitlines(keepends=True),
                                               after.decode().splitlines(keepends=True),
                                               fromfile="a/" + relative, tofile="b/" + relative))
        if spec.get("oracle"):
            baseline = oracle_patch(spec["oracle"], original.decode()).encode()
            oracle = diff(original, baseline)
            (folder / "oracle.patch").write_text(oracle)
            case_report["oracle"] = {"name": spec["oracle"],
                                     "patchSha256": hashlib.sha256(oracle.encode()).hexdigest(),
                                     "sourceSha256": hashlib.sha256(baseline).hexdigest(),
                                     "sameForCleanMutantAndRestored": True,
                                     "productValidation": False,
                                     "requiredFailureMarker": spec["failure_marker"]}
        try:
            path.write_bytes(baseline)
            clean_before = True
            for i, command in enumerate(commands):
                result, output = gate(command, source, folder / f"clean-before-{i}.log", environment)
                result["valid"], result["reason"] = outcome(result, output, False)
                result["phase"] = "clean-before"
                result["proofDomain"] = proof_domain(command)
                case_report["commands"].append(result)
                clean_before &= result["valid"]
            if not clean_before:
                case_report["blocked"] = "clean regression baseline failed"
            else:
                try:
                    mutated = mutate(case, baseline.decode()).encode()
                    if mutated == baseline:
                        raise RuntimeError("mutant is identical to baseline")
                    path.write_bytes(mutated)
                    patch = diff(baseline, mutated)
                    (folder / "mutant.patch").write_text(patch)
                    case_report["mutantSha256"] = hashlib.sha256(mutated).hexdigest()
                    case_report["patchSha256"] = hashlib.sha256(patch.encode()).hexdigest()
                    detections = []
                    for i, command in enumerate(commands):
                        result, output = gate(command, source, folder / f"mutant-{i}.log", environment)
                        result["valid"], result["reason"] = outcome(
                            result, output, True, spec.get("failure_marker"))
                        result["phase"] = "mutant"
                        result["proofDomain"] = proof_domain(command)
                        case_report["commands"].append(result)
                        detections.append(result["valid"])
                    case_report["detected"] = bool(detections) and all(detections)
                finally:
                    path.write_bytes(baseline)
                clean_after = True
                for i, command in enumerate(commands):
                    result, output = gate(command, source, folder / f"clean-after-{i}.log", environment)
                    result["valid"], result["reason"] = outcome(result, output, False)
                    result["phase"] = "clean-after"
                    result["proofDomain"] = proof_domain(command)
                    case_report["commands"].append(result)
                    clean_after &= result["valid"]
                case_report["restoredCleanPass"] = clean_after
                case_report["detected"] &= clean_after
        finally:
            path.write_bytes(original)
        if path.read_bytes() != original or git(source, "status", "--porcelain=v1", "--untracked-files=all"):
            raise RuntimeError("isolated source did not restore cleanly; preserve it for inspection")
        case_report["finishedAt"] = now()
        document = {"sha": args.base, "summary": f"{case}: " + ("detected and restored clean" if case_report["detected"] else "proof incomplete or mutant survived"),
                    **case_report}
        write_json(folder / "report.json", document)
        saved = invoke(["node", "scripts/tools/lane-state.mjs", "save", str(source), "S2", stage, str(folder / "report.json")],
                       source, folder / "stage-save.log", environment)
        case_report["stageSave"] = saved
        report["cases"].append(case_report)
        write_json(evidence / "report.json", report)
        print(json.dumps({"event": "case-finished", "case": case, "detected": case_report["detected"],
                          "stageSaved": saved["exitCode"] == 0, "evidence": str(folder)}), flush=True)
        if not clean_before:
            break
    report["finishedAt"] = now()
    report["complete"] = len(report["cases"]) == len(selected) and all(
        case["detected"] and case["stageSave"]["exitCode"] == 0 for case in report["cases"])
    report["summary"] = ("All selected mutants detected by runtime assertions; source restored and clean regressions pass."
                         if report["complete"] else "Mutation proof incomplete; inspect failed or surviving cases before integration.")
    write_json(evidence / "report.json", report)
    print(json.dumps({"event": "executor-finished", "complete": report["complete"], "report": str(evidence / "report.json")}), flush=True)
    return 0 if report["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
