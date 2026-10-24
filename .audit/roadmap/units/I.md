---
id: I
title: Import tC3 projects and update a book from USFM (tc-import)
track: A
lane: storage
milestone: M2
state: verifying
owner: domain lane (Claude Code 9f599c21)
branch: track-I
worktree: .worktrees/track-I
depends: [S2, DOM-2]
needs_owner: []
later: []
proves: [P4, P5, P9]
no_loss: true
size: 4-7 d (basis: B1a's seed took about 2 h; §13.1 is the bulk; +1 to +2 d for Update a book, docs/STORAGE-DELTA-2026-10-21.md §6)
merged: none
updated: 2026-10-24
next: The integration lane merges it in a batch after DOM-2's; in the merge commit set state done and merged to this branch's tip.
blocked_by: none
---

# I: Import tC4 4.0, tC3, USFM and Burrito projects, and update a book (tc-import)

## Goal

A translator imports a tC4 4.0 project (with or without its journal), a tC3 project, raw USFM
files or a Scripture Burrito and gets a valid project whose text is byte for byte the source's,
with the work tC4 hid shown as conflicts. Split per `TRACK-B.md` §8: a pure crate
`crates/tc-import` with no `tc-store` dependency (identify, review, convert, verify); one store
write, `Store::create_imported`; and the move of a Door43 team from tC4 4.x (STORAGE §13.3), after X.

New on 2026-10-21 (owner ruling `rulings/2026-10-21-outside-edits.md`, OE 3 and OE 5): "Update a
book from USFM" (and "Update a story from Markdown") brings a file edited anywhere into an
existing project (delta §2.16, new STORAGE §13.6, U1-U6). Each changed verse becomes a change
marked as imported. A verse that also changed in the project since goes to the conflict review.
Nothing is removed and nothing newer is overwritten. I builds the job; F1 builds the menu item and
its screen; D1 builds the "changed externally" question; S3 step 9 builds the store's *Import* of a
kept copy, which calls I's job.

## Why it matters to a translator

Translators bring years of work when they switch. An import that changes a byte or keeps tC4's
hidden work hidden loses work at the moment they trust the new app most, and imported history
becomes permanent when V freezes the format.

Update a book is the supported way to bring work done in Paratext or another editor into the
project: edit, update, then sync. If it overwrote a teammate's newer verse, or dropped a verse the
file lacks, outside work would cost the team its own.

## Before you start

Run these from the repository root on your branch:

- `python3 -I .audit/tools/board.py` shows I **ready** (S2 and DOM-2 are `done`).
- `for u in S2 DOM-2; do git merge-base --is-ancestor "$(sed -n 's/^merged: //p' .audit/roadmap/units/$u.md)" HEAD && echo "$u ok"; done`
  prints `S2 ok` and `DOM-2 ok`.
- `test -d crates/tc-import && grep -rn "NotBuilt" crates/tc-import/src`: if the owner chose Track
  B′ (H2), INT-7 froze a skeleton crate; use it as it is. Otherwise step 2 creates the crate.
- `wc -l crates/tc-app/src/backend/seed.rs crates/tc-app/src/backend/seed/*.rs` prints `4231 total`;
  `sed -n 30,31p crates/tc-app/src/backend/seed.rs` names `memory_seed_equals_tc_import_seed`.
- `cargo test -p tc-app --test app state_seed` runs 32 tests and passes (31 plus the property test
  `random_texts_and_decisions_round_trip`); `grep -c "#\[test\]" crates/tc-app/src/backend/seed/tests.rs`
  prints `18` (the module's own unit tests).
- `python3 -I -c "import json;print(len(json.load(open('fixtures/tc4/conformance-fixtures/import/MANIFEST.json'))))"`
  prints `16`, and `git grep -n "import/MANIFEST" -- crates parity` prints nothing: no test reads it.
- `ScriptedImport` exists only in `docs/APP-DESIGN.md` (R11, :3264), not in code; F1 builds the
  app's import port.
- DOM-2's card log names the one reader per import job: aligned USFM (`split_aligned`,
  `crates/tc-journal/src/alignment.rs:235`) and the tC4 alignment sidecar (`decode_alignments`,
  `crates/tc-journal/src/fold/sidecar.rs:132`). Use those; write no other reader.
- Step 7 also needs S3's core: `grep -n "core merged" .audit/roadmap/units/S3.md` prints S3's
  `note` line (S3 step 8), or `sed -n 's/^state: //p' .audit/roadmap/units/S3.md` prints `done`.
  Step 8 needs X `done`.
- `ls .audit/roadmap/rulings/ | grep -- -H13` shows whether H13 is answered. Only one behavior
  waits for it: the scheme an import picks when the source has no versification file.
- Step 7a (Update a book) needs the delta in STORAGE: `grep -c '^### 13.6' docs/STORAGE.md`
  prints `1` (S2 applies the delta when it merges). It also needs the planner and decoders S2
  kept: `grep -n "fn plan_outside_edit" crates/tc-journal/src/outside.rs` prints one line (:676
  at 3b075d7). Its store half needs S3 step 9's part (d) (the `kept` list and *Import* in the
  store): S3's log has a `step 9` line naming (d), or S3 is `done`.
- `ls .audit/roadmap/rulings/ | grep -- '-H18\|-H20\|-H21\|-H22'` shows which questions are
  answered. Step 7b waits for H20; step 8 item (2) waits for H18. H21 and H22 change no step
  unless their ruling leaves the recommendation (see "Stop and ask when").

Read first: `docs/STORAGE.md` §13 (lines 4389-4598 at s2-proof-executor 3b075d7); `TRACK-B.md` §8;
`docs/TC4-DELTA-2026-10-16.md` Q13 (:1228), Q22 and Q23 (:1273-1279); `docs/TC4-DELTA-2026-10-19.md`
C35, C36, C38, C45 (:116-126); `docs/PLAN.md` P9 (:282-292); the owner's ruling
`.audit/rebuild.tsv:81` (re-stamp a tC4 alignment only when its words match the verse word for
word; keep I lean). For steps 7a, 7b and 8: the rulings `.audit/roadmap/rulings/2026-10-21-outside-edits.md`
and `2026-10-21-carried-files.md`; `docs/STORAGE-DELTA-2026-10-21.md` §1, §2.11, §2.12, §2.15,
§2.16 (U1-U6), §4.2-§4.3 (rows L41, L42, L140, L145, L148-L150), §5, §6 and §7 (H18, Q5, Q8).

## Funnel card

- **Invariants**:
  - INV-IMPORT-IS-WHOLE-OR-ABSENT: when an import ends or is killed at any point, the projects
    folder holds the whole new project or nothing of it (a staging folder the next start deletes
    is allowed); the source is never changed.
  - INV-SCRIPTURE-BYTES-UNCHANGED: every book of the source projects back byte for byte (USFM and
    `vrs.json`; tC3 through the usfm-js-compatible writer; aligned USFM as the woven projection),
    and every non-derived field of the source's `metadata.json` is kept; else the import refuses
    and creates nothing.
  - (retired: tC4 4.0 import cut, rulings/2026-10-24-H3-H4-H7-fast-release.md) INV-HIDDEN-WORK-SHOWS-AS-CONFLICT: every change of a tC4 journal that tC4's rules hid
    (experiments E1–E6, L2, L9–L11, L16) is a head in the imported fold: a listed conflict or in
    History, never dropped.
  - (retired: no job machine for the cut scope; a tC3 import is one create) INV-NO-PARTIAL-PROJECT-AFTER-CANCEL: once a cancel returns, no project folder and no staging
    folder of that import remain, whatever the progress point.
  - (retired: tC4 4.0 team import cut) INV-IMPORT-IS-DETERMINISTIC: two imports of one tC4 project give the same project id and change
    ids (STORAGE §13.1 T4, T6), so teammates' imports merge.
  - INV-UPDATE-NEVER-OVERWRITES-NEWER-WORK: Update a book gives each change the base's heads as
    parents, so a verse changed in the project since the base is a conflict, never replaced
    (delta U3, L145, L148); a change with no parents carries `unknown_base` and is never the
    provisional text (§2.15).
  - INV-UPDATE-NEVER-REMOVES: a verse or record the file lacks, or a value equal to an older
    version of the record, is a conflict, never a removal or a silent revert (I11, L41, L42, L140).
  - INV-UPDATE-KEEPS-ALIGNMENTS: a verse whose words are unchanged keeps its alignment; a verse
    whose words changed is reflowed in the same batch; a verse in conflict gets no alignment
    written (U4, L149).
- **Wrong states**: a partial project after a cancel or crash (§6.4 N1); an import into an
  existing project (tC4 D53) or over a folder of the same name; text changed by the import (a
  normalized byte, a line ending, NFC applied to text); tC4's silent losses kept silent (hidden
  structural branches, orphaned verse edits, newest-wins forks); a stopped tC4 sync apply imported
  half-applied (Q23); a torn `metadata.json` that fails the import or makes a wrong project; a pin
  without a sha (tC4 D82); a `.bak` file imported as content (it goes to `.tc/external/tc4-bak/`);
  an alignment re-stamped when its words do not match (`rebuild.tsv:81`); a source with no
  `vrs.json` silently given English (#813, #821, #822, H13); a 4–8 MiB tC4 segment refused (C38,
  D104); stored source words compared without NFC (C35); a nested alignment's words lost (C45).
  Update a book: a teammate's newer verse replaced; a verse the file lacks removed; an old file's
  text shown as the provisional text before the review; an alignment left on words the verse no
  longer has, or dropped for unchanged words; an unreadable file that writes anything, or a
  refusal without the line; a verse counted as imported while it waits in the review; Undo
  offered (delta Q7: History keeps every version); a second reader of USFM, Markdown or
  sidecars written for it.
- **Code**:
  - Types (in `tc-import`, to create, signatures per `TRACK-B.md` §8): `SourceTree`, `SourceKind`,
    `ImportReview`, `ImportPlan` (with `git: Fresh | AdoptFrom(path)` for §13.1 T1/T9), `Mismatch`,
    `trait TagResolver`; `ImportRefusal` with one variant per `import.damaged.*` code of the
    MANIFEST; `VerifiedPlan` with a private constructor that only `verify` calls, so an unchecked
    plan cannot be written; `job::{ImportState, ImportEvent, ImportAction, next}`, a pure,
    exhaustive machine: Identified → Reviewed → Converted → Verified, plus Refused and Cancelled.
  - One entry point: `tc_import::import(source, answers, progress, cancel) -> ImportJob` (to
    create), ending in `Result<VerifiedPlan, ImportRefusal>`. The one store write is
    `tc_store::Store::create_imported(VerifiedPlan, progress, cancel)` (to create, step 7) over
    `Store::staging(StagingKind::Import)` and `Store::place` (`crates/tc-store/src/store.rs:247,259`).
    `tc-store` depends on `tc-import`, never the reverse.
  - Update a book (step 7a, to create): pure `tc_import::update::plan_update(file, kind, base,
    project) -> Result<UpdatePlan, UpdateRefusal>` with `UpdateKind { Usfm, Markdown }` and
    `UpdateBase { Heads(..), None }`; the base is a value, so S3's *Import* of a kept copy,
    *Bring it back* (Q2) and any later entry pass their own base and reuse it unchanged. One store
    entry, `Store::update_book(file, file_name, book, progress, cancel) -> Result<UpdateReport,
    UpdateRefusal>`, with `UpdateReport { imported, in_conflict, not_readable }`. F1's
    `import/update-book` job calls it. Format (delta §2.15): `ImportSource::Markdown { file_name }`
    and the import cause's `unknown_base` mark, ranked 0 in `CauseClass::rank`, unless S3 step 9
    added them first.
  - Removed: F1 deletes tc-app's seed copy (`crates/tc-app/src/backend/seed.rs`, `seed/`) once
    `memory_seed_equals_tc_import_seed` passes (APP-DESIGN AD7 :189, F1 row :3419); the test-side
    copy of tC4's tC3 import rule, `imported()` in `crates/tc-align/tests/tc3_import.rs:55`, calls
    tc-import instead; tc-import adds no USFM, sidecar or versification reader of its own. Update
    a book uses the codec, the decoders and the planner `plan_outside_edit` S2 kept in
    `crates/tc-journal/src/outside.rs`, and `tc_align::reflow` (`crates/tc-align/src/reflow.rs:82`);
    it writes no reader or planner of its own.
  - Left to checks: byte equality with a source and tC4's hidden work are facts about each source;
    `verify` checks them at run time (the oracle), and per-fixture tests prove it refuses.
- **Checks** (as built for the first release; CI `rust` job via `cargo test --workspace`):
  - `every_tc3_manifest_case_imports_or_refuses_as_tc4_does` in `crates/tc-import/tests/tc3.rs`
    (the 5 tC3 entries of the MANIFEST), `a_tc3_project_reads_its_alignments_and_decisions`,
    `a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid` (#796),
    `a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project` (P9, P5) and
    `crash_import_leaves_no_project_or_a_whole_one` (a kill at every write point).
  - `tc3_usfm` in `crates/tc-usfm/tests/tc3_usfm.rs`: tC3's book text byte for byte against
    usfm-js 3.4.3 (`fixtures/usfm/tc3-usfm-vectors.json`).
  - Update a book (`crates/tc-store/tests/store/update.rs`):
    `update_a_book_conflicts_a_verse_changed_on_both_sides` (L145),
    `a_verse_the_file_lacks_is_never_removed` (I11, L41, L42),
    `update_a_book_from_an_unreadable_file_writes_nothing_and_names_the_line`,
    `an_aligned_usfm_file_updates_its_alignments` (U4), `a_no_base_import_never_becomes_the_provisional_text`,
    `updating_from_an_old_export_reverts_nothing` (L148) and the property
    `update_a_book_conserves` (no head lost, no verse removed, an empty verse filled).
  - Proof each can fail (quoted in the log): without the #796 word check, without the update's
    missing parents, and with a marked import ranked like any other change.
  - (retired: cut by rulings/2026-10-24-H3-H4-H7-fast-release.md, tC4 4.0, raw USFM and burrito
    imports, the job machine and the store's kept copies wait) the tC4 and seed checks
    `import_next_is_total_over_every_state_and_event`, `memory_seed_equals_tc_import_seed`,
    `import_tc4_hidden_work_surfaces`, `tc4_replay_<id>_shows_the_hidden_work`,
    `two_teammates_importing_one_tc4_project_get_identical_changes`,
    `tc4_project_with_a_6_8_mib_book_add_segment_imports_whole`,
    `a_cancel_at_every_progress_point_leaves_no_folder`; Update a story
    `update_a_story_from_markdown_imports_its_frames`; the kept-copy base's
    `update_a_book_keeps_alignments_of_unchanged_words_and_reflows_changed_ones` (with no
    base a changed verse is a conflict, so nothing reflows) and S3's
    `an_import_interrupted_after_its_batch_is_not_imported_twice`; the scale run
    `updating_a_book_costs_what_it_changed` (P is parked; the planner reads one book).
- **Prose**: `docs/STORAGE.md` §13, in the commit that changes the behavior: §13.1 T2 segments up to
  8 MiB (C38); §13.5 nested alignments at any depth (C45) and the rule for a burrito with no
  `vrs.json`; §13.3 `moved.json` (step 8). §13.6 is written by S2 from the delta; change it only
  where step 7a or 7b finds the delta's text wrong, and say so in the log.
- **Human**: H13 (`ROADMAP.md`): with no versification file, guess the scheme from the contents
  and say so, or use English as tC4 does? Recommendation: guess and say which. Only one test waits
  for it. Ruling file: `rulings/<date>-H13.md`. Ruled 2026-10-22: H20 A with the empty-verse
  rule (`rulings/2026-10-22-H20.md`; step 7b); H18 B, Door43 tool edits unsupported
  (`rulings/2026-10-22-H18.md`; step 8 item (2) builds the notice only); H21 A and H22 parts 1
  and 3 (`-H21.md`, `-H22.md`) reuse step 7a's job as recommended, so no extra work. H13 is not
  in `needs_owner` because only one test waits and the card gives English until then.

## Steps

### Step 1: Sweep tC4's import changes since 9ad4d5c (I-0)

- Do: (1) Run `grep -n "#813\|D110\|#821\|#822\|#835\|e79a2795" docs/TC4-ISSUES.md`; if SWEEP already
  added these rows, log that and go to step 2. (2) Use a tC4 clone holding b86270f: the reference
  clone `~/unfoldingword/translationcore4` is at d310bf3, lacks it, and is never fetched into;
  clone tC4 into `~/unfoldingword/tc4-tip` if that folder does not exist. (3) Run
  `git -C ~/unfoldingword/tc4-tip log --oneline 9ad4d5c..b86270f -- src/data/import`: it lists
  0b4cb270 (#813), e17fce08 and e79a2795 (#796: a tC3 alignment whose words do not fit the verse is
  flagged). Also read 71c2cb36 (D110: an import with no `vrs.json` gets English until #822) and
  issues #821, #822, #835 (`gh issue view <n> -R unfoldingWord/translationCore4`; without `gh`, the
  summaries in `.audit/roadmap/evidence/2026-10-21/tc4-bugs.md` §4). (4) Add one row per item to
  `docs/TC4-ISSUES.md` with its unit (I; F1 for #835) and test: #813/D110/#821/#822
  `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme`; #796
  `a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid`.
- Files: `docs/TC4-ISSUES.md`.
- Done when: the grep of (1) prints at least three rows that name I.
- Record: append a log line `step 1` (TRACKING.md format); update `state`, `updated` and `next` in
  the same commit.

### Step 2: Freeze the core contract and build the import machine (I-1, pure half)

- Do: (1) Create `crates/tc-import` (`publish = false`, edition 2024, `[lints] workspace = true`),
  or use INT-7's skeleton. Dependencies from `[workspace.dependencies]` only: tc-core, tc-usfm,
  tc-burrito, tc-journal, tc-versification, zip, thiserror; never tc-store or tc-api. (2) Write the
  types and functions of `TRACK-B.md` §8 with their final signatures (`identify`, `review`,
  `convert`, `verify`, `TagResolver`, `pub mod tc4` with `convert_segments`); each returns
  `Err(ImportRefusal::NotBuilt)` until a step fills it in. (3) In `src/job.rs` write
  `ImportState`, `ImportEvent`, `ImportAction` and the pure `next()` in the shape of
  `crates/tc-store/src/ledger.rs` (`State` :92, `Event` :106, `next` :454), with no wildcard arm;
  `import(...) -> ImportJob` runs `next()`, reports progress and checks `cancel` between actions.
  (4) Write `import_next_is_total_over_every_state_and_event`: a cancel in any state leads to
  `Cancelled`, and no state after `Refused` or `Cancelled` holds a `VerifiedPlan`.
- Files: `Cargo.toml` (workspace members); `crates/tc-import/{Cargo.toml,src/lib.rs,src/job.rs,tests/job.rs}` (to create).
- Done when: `cargo test -p tc-import` passes; making `next` keep the plan after a cancel fails the
  matrix test naming INV-NO-PARTIAL-PROJECT-AFTER-CANCEL; restore.
- Record: log line `step 2` with the failing output; update `state`, `updated` and `next`.

### Step 3: Import raw USFM and foreign burritos, and run the MANIFEST (I-2)

- Do: (1) Raw USFM (STORAGE §13.5): one `Import` batch per book through `tc_usfm::codec::decompose`
  (`crates/tc-usfm/src/codec.rs:478`). Take aligned verses apart with `split_aligned` at any
  nesting depth (C45; test vector: the GEN 1:1 "the heavens" verse of DELTA-19 C45). A project with
  no pins gets the creation pins in its import batch (C36, #766). (2) Foreign burrito: validate with
  `tc_burrito::validate_project` (`crates/tc-burrito/src/validate.rs:197`); text through the codec;
  tC4-shaped `x-` sidecars become records as in §13.2. For a source with no `vrs.json`, write one
  function `choose_versification` (to create): until H13's ruling exists it gives English as tC4's
  D110 does, and the report says which scheme it used; step 5 finishes it. (3) Write
  `every_manifest_case_imports_or_refuses_as_tc4_does` over every entry of
  `fixtures/tc4/conformance-fixtures/import/MANIFEST.json` (tC4's `ManifestEntry`,
  `test/helpers/import.ts:20-33`): pass `name`, `lang` and `license` as the review answers; an
  `accept` case gives its `language`, `books` and chapter `counts`, a valid project and unchanged
  text; a `refuse` case gives its `code`. Skip the first entry, `parser: "fake"` (tC4's stand-in
  parser for its import-shell tests, `test/importShell.test.ts:118`), and name it skipped in the
  table. The 5 `parser: "tc3"` entries are `pending until step 6` in the table and in the log. The
  zips are in git (not in plain snapshots). (4) Also write
  `a_usfm_import_with_no_pins_gets_the_creation_pins` (C36),
  `an_aligned_verse_with_a_nested_alignment_imports_every_word` (C45) and
  `imported_source_words_compare_with_the_resource_in_nfc` (C35).
- Files: `crates/tc-import/src/{usfm.rs,burrito.rs,versification.rs}`, `crates/tc-import/tests/manifest.rs` (to create).
- Done when: `cargo test -p tc-import --test manifest` passes for the 10 cases that are neither tC3
  nor `fake`; making `recompose` normalize CRLF fails it naming INV-SCRIPTURE-BYTES-UNCHANGED;
  restore.
- Record: log line `step 3`; update `state`, `updated` and `next`.

### Step 4: Move the state seed into tc-import (I-3)

- Do: (1) Move `tc4_state_seed` and its types (`crates/tc-app/src/backend/seed.rs:627`, with
  `seed/check.rs`, `records.rs`, `strict.rs`) to `crates/tc-import/src/tc4/seed.rs`. The seed uses
  two tc-app helpers, `crate::js::parse_utc_millis` and `crate::feedback::iso_utc_millis` (with
  `tc_api::ids::WallMs`), at `seed/strict.rs:139-145`: move what it needs into tc-import, adding no
  tc-api dependency. (2) Copy the 32 tests of `crates/tc-app/tests/app/state_seed.rs` to
  `crates/tc-import/tests/seed.rs`; move the 18 unit tests of `seed/tests.rs` with the code.
  (3) Keep tc-app's copy (F1 deletes it). Add `memory_seed_equals_tc_import_seed` to
  `crates/tc-app/tests/app/state_seed.rs`: on the scenarios `sample`, `en-pins` and `shared`, both
  seeds give identical changes and identical derived bytes.
- Files: `crates/tc-import/src/tc4/seed.rs`, `crates/tc-import/tests/seed.rs` (to create),
  `crates/tc-app/tests/app/state_seed.rs`, `crates/tc-app/Cargo.toml`.
- Done when: `cargo test -p tc-import --test seed` passes all 32; `cargo test -p tc-app --test app
  state_seed` passes with the new test; dropping tc-import's `schemaVersion` 1 migration fails the
  comparison test; restore.
- Record: log line `step 4`; update `state`, `updated` and `next`.

### Step 5: Convert tC4 4.0 projects with their journal (I-4)

- Do: (1) Write STORAGE §13.1 T2–T8 in `crates/tc-import/src/tc4/`: port tC4's segment validator
  with the 8 MiB limit (`SEGMENT_LIMIT`, tC4 `src/data/journal/seal.ts:31`; C38, D104); read the
  derived files as the oracle (T3); deterministic ids (T4, T6); the event table of §13.1; reconcile
  changes (T7). Expose `tc4::convert_segments` for step 8. (2) Re-stamp a tC4 poetry alignment only
  when its words match the verse word for word, recorded in History (`rebuild.tsv:81`); test
  `a_tc4_alignment_is_restamped_only_when_its_words_match_the_verse`. (3) Replays: the experiment
  scripts were never committed; `.audit/research/sync-design.md:747-771` describes E1–E6 and
  `docs/JOURNAL-DECISION-2026-10-16.md:239-254` describes L2, L9–L11 and L16. Build each as a tC4
  journal fixture under `fixtures/tc4/import-replays/<id>/` (to create), in the segment format of
  `fixtures/tc4/conformance-fixtures/team-sync/journals/`, and write
  `tc4_replay_<id>_shows_the_hidden_work` for each plus `import_tc4_hidden_work_surfaces`.
  (4) Build the four P9 fixtures under `fixtures/tc4/import-p9/` (to create) from tC4's test
  fixtures and rigs, never from real user data:
  - (a) a project synced with a team (name events, a local history that differs from Door43
    `main`, the folders tC4's sync leaves; the reserved ones are `ingredients/tc4-open/` and
    `ingredients/tc4-sync/`, tC4 `src/data/openCopyMarks.ts:18-23`):
    `a_synced_tc4_project_imports_and_skips_its_sync_leftovers`;
  - (b) a stopped sync apply: decide its mark by reading tC4 `src/data/journal/opsLog.ts` ("the
    sync apply", :201-209) and the header of `src/data/share/syncOperation.ts` (:1-32), write the
    rule in the fixture's README; `a_tc4_project_whose_sync_apply_stopped_is_refused_with_open_it_in_tc4_once`,
    with Q23's words "Open this project in translationCore 4 once, so it can finish its sync, then
    import it.";
  - (c) a torn `metadata.json`: `a_tc4_project_with_a_torn_metadata_json_imports_with_its_text_unchanged`
    (PLAN P9: it imports; the report says the file was damaged);
  - (d) platform stamps (`dateCreated`, `revision`, `timestamp`; DELTA-16 :135, C32):
    `a_tc4_project_with_platform_stamps_imports_with_every_field_kept`.
  (5) Also write `tc4_project_with_a_6_8_mib_book_add_segment_imports_whole`,
  `two_teammates_importing_one_tc4_project_get_identical_changes` and
  `imported_obsolete_checking_language_is_kept_and_ignored`. (6) H13: if the ruling exists, make
  `choose_versification` follow it and write
  `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme`. If not,
  write that test against the recommendation, mark it `#[ignore = "waits for H13"]`, log a `block`
  line naming H13, put H13 to the owner exactly as `ROADMAP.md` words it, and go on to step 6.
- Files: `crates/tc-import/src/tc4/` and `crates/tc-import/tests/tc4.rs` (to create);
  `fixtures/tc4/import-replays/`, `fixtures/tc4/import-p9/` (to create).
- Done when: `cargo test -p tc-import` passes; making the converter apply tC4's same-actor rule
  fails `tc4_replay_e1_*` naming INV-HIDDEN-WORK-SHOWS-AS-CONFLICT; restore.
- Record: log line `step 5`; update `state`, `updated` and `next`.

### Step 6: Import tC3 projects (I-5)

- Do: (1) Write STORAGE §13.4 in `crates/tc-import/src/tc3.rs` (to create), following tC4's
  `src/data/import/tc3.ts`: text through a usfm-js-compatible writer; check histories as chains of
  changes; tC3's invalid markers kept; pins through `TagResolver` (the app side makes the network
  call). Port e79a2795 and e17fce08: an alignment whose words do not fit the verse is marked
  invalid. (2) Point `imported()` in `crates/tc-align/tests/tc3_import.rs:55` at tc-import's
  converter. (3) Turn on the 5 tC3 MANIFEST cases (`tc3/cfm_fbt_tit_book.zip`, `tc3/multi/*.zip`,
  `damaged/truncated.zip`, `damaged/tc3-no-manifest`, `damaged/tc3-corrupt-entry.zip`). (4) Write
  `tc3_import_pins_the_nearest_twl_release` and
  `a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid`.
- Files: `crates/tc-import/src/tc3.rs`, `crates/tc-import/tests/{tc3.rs,manifest.rs}`,
  `crates/tc-align/tests/tc3_import.rs`, `crates/tc-align/Cargo.toml`.
- Done when: `cargo test -p tc-import --test manifest` passes all 15 real cases (only `fake`
  skipped) and `cargo test -p tc-align --test tc3_import` passes.
- Record: log line `step 6`; update `state`, `updated` and `next`.

### Step 7: Write the imported project into the store, whole or absent (I-1, store half)

- Only when S3's core slices are on your branch. The main team does this step even if Track B′
  built steps 2 to 6.
- Do: (1) Add `Store::create_imported(VerifiedPlan, progress, cancel) -> Result<PathBuf,
  CreateError>` to `crates/tc-store/src/store.rs` beside `create` (:312). In
  `Store::staging(StagingKind::Import)` it seals the plan's packs into a new Local device's sealer
  folder, materializes, runs CHK-2's validation, commits (a tC4 4.0 source: on the copied history,
  "Import from translationCore 4 (tC)" with the trailer `Tc-Import-Source`, §13.1 T1, T9; any other
  source: a fresh history), then places the folder with `Store::place`, which never replaces
  anything. A cancel deletes the staging folder before it returns. This is R29's import entry: mark
  R29 in `docs/APP-DESIGN.md` (:3281). (2) Copy `.bak` files to `.tc/external/tc4-bak/`; never import
  them. (3) Tests: `crash_import_leaves_no_project_or_a_whole_one` in
  `crates/tc-store/tests/store/crash.rs`, modelled on `crash_create_leaves_no_project_or_a_whole_one`
  (:549), showing the next start deletes the staging folder and names it in
  `Store::removed_at_start` (:235); `a_cancel_at_every_progress_point_leaves_no_folder`;
  `an_import_never_replaces_a_folder_with_the_same_name`; one end-to-end test per MANIFEST `accept`
  case through the store with CHK-2's hook on (`Hooks::check_burrito`,
  `crates/tc-store/src/hooks.rs:108`).
- Files: `crates/tc-store/{Cargo.toml,src/store.rs}`, `crates/tc-store/tests/store/{crash.rs,import.rs}`
  (`import.rs` to create), `docs/APP-DESIGN.md`.
- Done when: `cargo test -p tc-store --test store crash::crash_import` and `model::` pass;
  skipping the staging delete on cancel fails the cancel test naming
  INV-NO-PARTIAL-PROJECT-AFTER-CANCEL; restore.
- Record: log line `step 7`; update `state`, `updated` and `next`.

### Step 7a: Update a book from USFM or a story from Markdown, with a known base (§13.6)

- Two halves with one owner (this card), so the planner is written once. **Step 7a-1**, the pure
  half (part (1) below plus part (2)'s format items), needs only STORAGE §13.6 and S2's planner on
  your branch ("Before you start") and runs *before* S3 step 9 (d): S3's `import_kept` calls
  `tc_import::update::plan_update` and must have it. Log it as `step 7a-1`. **Step 7a-2**, the
  store half (parts (3) and (4)), needs S3 step 9 (d). Numbered 7a so other cards' references to
  steps 8 and 9 hold. D1 step 3 waits for both halves ("Before you start" of D1).
- Do: (1) Pure half in `crates/tc-import/src/update.rs` (to create), §13.6 U1, U3, U4, U5. U1:
  decode with the codec (§13.5) and the decoders in `crates/tc-journal/src/outside.rs`, with
  §13.5's refusals and the line; a story decodes into StoryBase, Frame and StoryRef; a file whose
  book the project lacks adds the book (`BookPresence::Present { how: Added }`, no parents). U3:
  compare each record with the base through `plan_outside_edit` (whitespace rule, CRLF restored,
  shown-digest prefilter); a different record gets parents = the base heads, so a verse the
  project changed since is a sibling, a conflict; a value equal to an older version of the record
  and a record the file lacks are conflicts (no parents, `unknown_base`); the "more than 50 keys"
  rule does not apply. U4: alignments in the same batch through `tc_align::reflow`; add tc-align
  to tc-import's dependencies (tc-align keeps only a dev-dependency on tc-import, from step 6).
  U5: one batch, cause `Import(Usfm { file_name })` or `Import(Markdown { file_name })`; changes
  that rewrite verse structure form a passage over their range. `UpdateBase::None` returns
  `UpdateRefusal::NotBuilt` until step 7b. (2) Format (delta §2.15), unless S3 step 9 did it:
  add `ImportSource::Markdown { file_name }` and the import cause's `unknown_base` mark, ranked 0
  in `CauseClass::rank` (`crates/tc-journal/src/fold/meta.rs:292-297`); every change with no
  parents carries it. Tell V's owner (V pins both). (3) Store half: `Store::update_book` in
  `crates/tc-store/src/store.rs` beside `create_imported`. U6: seal the batch, fold, materialize
  the book, report imported / in conflict / not readable. S3's *Import* of a kept copy calls
  `plan_update` (step 7a-1) with the entry's base and adds `reconciles` and `Imported{md5}`;
  there is no planner in `tc-store`. The person also chooses a book (F1's screen); when the file's `\id` names another
  book, refuse naming both and write nothing (inference: no ruling names this case; tell F1's
  owner). (4) Write the step 7a checks of the funnel card in `crates/tc-import/tests/update.rs`
  and `crates/tc-store/tests/store/update.rs` (to create). `update_a_book_conserves` runs random
  files against random bases: no head lost, a revert and a removal are conflicts (L41, L42,
  L140). If S3 has not yet turned `loss_obs_template_edit_recorded` into an Update a story test
  (delta §5.2), `update_a_story_from_markdown_imports_its_frames` replaces it here.
- Files: `crates/tc-import/{Cargo.toml,src/update.rs,tests/update.rs}`,
  `crates/tc-journal/src/{change.rs,fold/meta.rs}` (only if S3 did not), `crates/tc-store/src/store.rs`,
  `crates/tc-store/tests/store/update.rs`.
- Done when: `cargo test -p tc-import --test update` and `cargo test -p tc-store --test store
  update::` pass; `updating_a_book_costs_what_it_changed` meets its budget on both corpora in a
  release build; giving the changes the project's current heads as parents fails
  `update_a_book_conflicts_a_verse_changed_on_both_sides` naming
  INV-UPDATE-NEVER-OVERWRITES-NEWER-WORK; restore.
- Record: log line `step 7a-1` when the pure half merges (S3 step 9 (d) waits for it), and
  `step 7a` when the store half does, naming the job (`Store::update_book`) and its report type,
  for F1 step 8; update `state`, `updated` and `next`.

### Step 7b: Update a book with no known base (H20)

- Ruling: `rulings/2026-10-22-H20.md` chose A with an empty-verse rule. A file chosen from
  anywhere has no base (delta U2 b).
- Do, by the ruling: a verse equal to the project's text or to any version in that record's
  History imports nothing. A verse empty in the project and with text in the file imports
  directly, with no conflict. A verse with text in the project and empty in the file leaves the
  project's text, with no conflict, and the report lists it as "left as it was: empty in the
  file". "Empty" is the test Home's drafted count uses (no words; markers alone are empty). Any
  other difference is a change with no parents and `unknown_base`; all of them form the import's
  one grouped row (§8.1) with *Accept all from the file* (C7a reuses `OutsideEditLarge`'s
  choices), and the project's text stays shown until a choice is made. Write
  `an_empty_project_verse_takes_the_files_text_without_a_conflict` and
  `an_empty_file_verse_leaves_the_projects_text` for the two empty-verse rules. Then write
  `updating_from_an_old_export_reverts_nothing` (L148),
  `a_no_base_import_never_becomes_the_provisional_text`, and the no-base half of
  `update_a_book_conserves`.
- Files: `crates/tc-import/src/update.rs`, `crates/tc-import/tests/update.rs`,
  `crates/tc-store/tests/store/update.rs`.
- Done when: the tests pass; ranking an `unknown_base` import change above the others fails
  `a_no_base_import_never_becomes_the_provisional_text`; making an empty project verse a conflict
  fails `an_empty_project_verse_takes_the_files_text_without_a_conflict`; restore both.
- Record: log line `step 7b` naming the ruling file; update `state`, `updated` and `next`.

### Step 8: Move a Door43 team from tC4 4.x, and bridge stragglers (I-6)

- Only when X is `done`; X's owner does this step.
- Do: (1) The move (STORAGE §13.3): import the local tC4 copy (step 5), then sync with X's
  `tc_sync::sync`. The merge commit removes `ingredients/checking/journal/**`, adds
  `ingredients/tc/**` and writes `ingredients/tc/moved.json` once (format `tc-moved`, version 1,
  the application id as one constant, no person's name; `docs/TC4-REQUEST-MOVED-NOTICE.md:18-35`).
  Specify it in STORAGE §3 and §13.3. (2) Q13, a Door43 web edit made before the move
  (`rulings/2026-10-22-H18.md`, B: unsupported): keep the project's text and give X's one notice
  per commit naming who made it and that Door43's history keeps it (delta §2.12); nothing is
  compared, kept as a copy or offered for import. Write the test to match. (3) Bridge: tC4 segments that
  reach R after the move are converted with `tc4::convert_segments`; commits whose message ends in
  "(tC4)" are skipped; the report says that teammate still uses tC4. (4) Tests with
  `tc-door43-fake`: `team_move_tc4_no_phantom_door43_edits` (STORAGE §15),
  `the_move_commit_writes_moved_json_once`,
  `a_straggler_segment_pushed_after_the_move_is_converted_and_kept`,
  `a_door43_web_edit_made_before_the_move_is_kept` (Q13; under H18 B, kept means kept in Door43's
  history and named in one notice), and
  `a_moved_repository_holds_files_tc4_refuses_to_sync` (the tree holds a file under `ingredients/`
  that tC4's round-1 sync does not derive, so tC4 answers `sync.verify-failed`), and
  `a_moved_repository_with_no_main_branch_keeps_the_stragglers_branch`
  (`evidence/2026-10-21/gap-tc4-straggler-sync-safety.md` state 3b: a repository whose default
  branch is not `main`, where tC4 uploads a straggler's work unchecked; the move writes
  `moved.json` on the default branch too, or refuses naming the state, and the report names the
  teammate; X step 5 item 2b refuses a *sync* against such a repository, this test covers the
  *move*).
- Files: `crates/tc-sync/src/move_team.rs`, `crates/tc-sync/tests/sync/move_team.rs` (to create),
  `docs/STORAGE.md`.
- Done when: `cargo test -p tc-sync` passes with these tests; making the bridge drop segments fails
  the straggler test; restore.
- Record: log line `step 8`; update `state`, `updated` and `next`.

### Step 9: Import a whole Bible within the budget, and write the evidence (I-7)

- Do: (1) Write a generator of a whole-Bible tC4 4.0 project folder (journal segments in tC4's
  format plus derived USFM) in `crates/tc-corpus`, beside `Spec::whole_bible`
  (`crates/tc-corpus/src/lib.rs:139`) (to create). (2) Measure the import in a release build
  against 2 min, with progress, cancellable (`docs/PERFORMANCE.md:108`); ask the laptop session
  (`.audit/tasks/windows-track.md`) for the reference-machine number. (3) Write
  `docs/evidence/import-<date>.md`: the numbers, each P9 case with the test that proves it, and
  what each replay showed.
- Files: `crates/tc-corpus/src/`, `docs/evidence/import-<date>.md` (to create).
- Done when: the evidence gives a verdict for the 2-minute budget and
  `cargo test -p tc-import --release` passes.
- Record: log line `step 9`; set `state: verifying`, `updated` and `next` in the same commit.

## Gates before merge

The full gate list: `scripts/gates.sh` once INT-3 creates it. Until then: `cargo fmt --all --check`;
`cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`;
`cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings` with no
features, `--features gallery`, `--features app` and `--features test-hooks`;
`cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`. Plus:

- The P9 byte-for-byte tests, the determinism test and the MANIFEST test pass (TRACK-B §8's gate).
- `cargo test -p tc-store --test store crash::` and `model::` pass.
- `a_tc4_project_with_a_book_and_no_versification_frame_imports_and_reports_its_scheme` runs (not
  ignored) and passes: it needs H13's ruling.
- Step 7a's tests pass, and `updating_a_book_costs_what_it_changed` meets its budget in a release
  build on both corpora (`docs/PERFORMANCE.md`).
- Step 7b's tests run (not ignored) and pass: they need H20's ruling. Step 8's Q13 test matches
  H18's ruling.
- `WINVM_DIR=i-import .audit/tools/winvm/run.sh HEAD '.\.audit\tools\winvm\gates.ps1'` prints
  `exit 0` for every gate.
- No-loss item: three verification lenses review it (FUNNEL §8.5); if Track B′ built it, the main
  team reviews.

## Stop and ask when

- H13 is still unanswered when step 5 is done: send H13 once, as `ROADMAP.md` words it; ask nothing
  else about it.
- A tC4 fixture would need real user data: stop and build it from tC4's test fixtures instead.
- A step would edit files S3 (`tc-store`), X (`tc-sync`) or F1 (`tc-app`) has claimed: ask that
  card's owner; do not edit.
- A gate fails twice in a row and you cannot tell why.
- S3 step 9 (d) is being written and step 7a-1 has not merged: tell S3's owner the date 7a-1
  will merge; S3's `import_kept` calls `tc_import::update::plan_update` and writes no planner of
  its own.
- (H21 chose A and H22 chose the notice-only exchange-file rule, so the sidecar import and the
  exchange-file copy are not built; if a later ruling reopens either, stop and ask the roadmap to
  size it before you write it.)
- F1's screen and step 7a's report disagree on a word or a count (imported, to review, not
  readable): agree the words with F1's owner; F1 owns the screen.

## References

- Evidence: `.audit/roadmap/evidence/2026-10-21/storage-remaining.md` §5, ST-4, ST-7, ST-8, ST-12;
  `health-domain.md` §7 I row, F4, F14; `split-seams.md` §4, S4–S6; `tc4-bugs.md` §2 I row;
  `done-definition.md` P9, DOD-14; `tc4-coverage.md` F8, F9.
- `docs/STORAGE.md` §6.4 N1–N2, §13, §15, §16; `TRACK-B.md` §8; `docs/APP-DESIGN.md` AD7 (:189),
  R11 (:3264), R29 (:3281), F1 row (:3419).
- `docs/TC4-ISSUES.md` rows 129, 190, 267, 303, 314, 331 (#794, #499, #766, #549, D104, #373).
- tC4: `src/data/import/{tc3.ts,burrito.ts,usfm.ts,parsers.ts}`, `src/data/journal/{seal.ts,verify.ts}`.
  tC4-coverage F8 (a tC4 4.0 import kind in the UI) belongs to F1, not I.
- Rulings `.audit/roadmap/rulings/2026-10-21-outside-edits.md` (OE 3, OE 5) and
  `2026-10-21-carried-files.md`; `docs/STORAGE-DELTA-2026-10-21.md` §2.16 (new §13.6), §2.15,
  §4.3, §5, §6 (I row: +1 to +2 d), §7 (H18, Q5, Q8); `ROADMAP.md` H18, H20-H22; `units/F1.md`
  (menu item and screen), `units/S3.md` step 9 (store *Import*), `units/D1.md` (the question).

- `evidence/2026-10-21/gap-tc4-straggler-sync-safety.md` (the straggler states step 8 tests; state 3b is the no-`main` repository).
## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/storage-remaining.md
- 2026-10-21 | roadmap | note | none | updated to the owner's rulings of 2026-10-21 and docs/STORAGE-DELTA-2026-10-21.md | .audit/roadmap/rulings/
- 2026-10-23 23:49 EDT | checks lane (Claude Code 2198ee19) | note | none | From CHK-3: your rows in docs/TC4-ISSUES.md and docs/P-LEDGER.md are (pending: I); when you write a test, replace the marker with its name in the same commit. .audit/tools/claims.test.mjs fails once this card is done while a marker remains | node --test .audit/tools/claims.test.mjs
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
- 2026-10-24 14:27 EDT | domain lane (Claude Code 9f599c21) | claim | none | Claimed on the orchestrator's word in .worktrees/track-I, cut from track-DOM-2 5ee8859d (DOM-2 is with the integration lane for batch 1; S2 is done on main). Scope by rulings/2026-10-24-H3-H4-H7-fast-release.md: kept steps 1 (only #796, the tC3 row), 2 (crate and machine), 6 (tC3 import), 7 (create_imported, on the current store: S3 is parked), 7a (Update a book; its store half is built here as Store::update_book, since S3 step 9 is parked) and 7b (no base, H20 ruled A). Cut until after the first release: steps 3 (raw USFM and burrito import; the MANIFEST test covers only the tC3 cases), 4 and 5 (tC4 4.0 seed and conversion), 8 (team move) and 9 (whole-Bible tC4 import). Before you start: STORAGE §13.6 present; plan_outside_edit at crates/tc-journal/src/outside.rs:676; H18 and H20 ruled, H13 not (it only concerned tC4 4.0 and USFM imports, both cut); no crates/tc-import yet | python3 -I .audit/tools/board.py
- 2026-10-24 14:30 EDT | domain lane (Claude Code 9f599c21) | step 1 | none | Only the tC3 item is in scope: docs/TC4-ISSUES.md gets `#796 (tC3 import)` (unit I, test a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid, pending: I), read from ~/unfoldingword/tc4-sweep, which holds b86270f: `git log --oneline 9ad4d5c..b86270f -- src/data/import` lists 0b4cb270 (#813, burrito: cut), e17fce08 and e79a2795 (#796, tC3). #813, D110, #821, #822 and #835 concern the cut tC4 4.0, USFM and burrito imports and are left for later | git -C ~/unfoldingword/tc4-sweep log --oneline 9ad4d5c..b86270f -- src/data/import
- 2026-10-24 14:43 EDT | domain lane (Claude Code 9f599c21) | step 7a | none | Update a book from USFM, no base (7a with 7b, H20 A; the kept-copy base waits with S3). Planner `FoldState::plan_update_book(book, bytes, file_name) -> UpdatePlan` in tc-journal/src/outside.rs, beside plan_outside_edit, whose private helpers it needs (the card put the pure half in tc-import; it is the same planner, not a second one): decodes with the codec, takes aligned verses apart with split_aligned, compares through plan_outside_edit with Base::Unknown, then H20: a verse empty in the project and with words in the file is imported (parents = heads); words in the project and an empty verse in the file is left as it was (the reply's `kept`); any other difference is a change with no parents; a verse the file lacks is a conflict, never a removal; an alignment is written only for a verse imported directly or whose text is unchanged; a record other than a verse equal to a version in its History imports nothing; a file whose \id names another book is refused ("the file is Philemon, not Titus"). Store: Command::UpdateBook { book, bytes, file_name } plans it in the actor. Format (delta §2.15, before the freeze): ImportSource::Usfm gains `unknown_base` (`"unknownBase": true`, written only when set, so every journal so far reads and encodes unchanged; tC-journal canonical and properties pass, the generator now draws it), CauseClass::Import { unknown_base } ranked 0 like an unknown-base outside change, fold-cache cause code 11. Markdown story update not built (cut with the scope: the ruling keeps Update a book from USFM). Tests crates/tc-store/tests/store/update.rs: update_a_book_conflicts_a_verse_changed_on_both_sides, a_verse_the_file_lacks_is_a_conflict_never_a_removal, update_a_book_from_an_unreadable_file_writes_nothing_and_names_the_line, an_aligned_usfm_file_updates_its_alignments, all pass. Proof: giving a differing verse the current heads as parents fails the first, `assertion failed: conflicted(&handle, &verse(1, 1))` (update.rs:54). V's owner must know the new optional field | cargo test -p tc-store --test store update::
- 2026-10-24 15:00 EDT | domain lane (Claude Code 9f599c21) | step 2 | none | crates/tc-import created (tc-core, tc-usfm, tc-journal, serde_json, thiserror, unicode-normalization, zip; never tc-store). Kept lean for the cut scope: SourceFile and ImportRefusal (tC4's import.damaged.* codes through code()); no ImportJob state machine, no VerifiedPlan type: a tC3 import is read in one pass and written by one Store::create_imported, which is whole or absent through its staging folder, and a cancel before the write leaves nothing. TRACK-B §8's identify/review/convert signatures are not written (no B′). tc-arch's edge table has a tc-import row (tc-core, tc-usfm, tc-journal); not on the pure list, as it reads zips | cargo test -p tc-arch
- 2026-10-24 15:00 EDT | domain lane (Claude Code 9f599c21) | step 6 | none | tc_import::tc3::read(&[SourceFile]) -> Tc3Import (language, name, licenses, contributors, gateway, versions, books) and Tc3Import::records(&DecisionPins) -> Vec<Record>, as STORAGE §13.4 now says: text through tc_usfm::tc3_book_usfm (74a08561, byte for byte with usfm-js on all four fixture books and 47 edge cases), alignments through decode_alignment_record (Writer::Tc4) with #796, decisions through decode_decisions. tC4 imports the current state; so does this (STORAGE §13.4 updated: its chains of tC3 history were never in tC4). Found: en_kjv Luke holds one tN decision with occurrence -1 (every occurrence), which no check list of the rebuild holds; it is listed in Tc3Book::not_kept and not imported (the source zip keeps it), a declared difference to tell the owner. Tests crates/tc-import/tests/tc3.rs: every_tc3_manifest_case_imports_or_refuses_as_tc4_does (5 cases: 2 accept, 3 refuse with tC4's codes), a_tc3_project_reads_its_alignments_and_decisions (46 records, 5 decisions), a_tc3_alignment_whose_words_do_not_fit_the_verse_is_marked_invalid, a_tc3_project_imports_with_its_text_unchanged_into_a_valid_project. Proof: without the #796 word check the third fails, `left: [] right: ["1:1"]`. Not done: imported() in tc-align/tests/tc3_import.rs keeps its own model of tC4's records (it tests tc-align on them; pointing it at tc-import would compare values, not tC4's JSON) | cargo test -p tc-import
- 2026-10-24 15:00 EDT | domain lane (Claude Code 9f599c21) | step 7 | none | Store::create_imported(&NewProject, Cause): Store::create's path (staging folder, build, place without replacing, removed at next start if killed), with the given cause on every record instead of Seed, staging kind Import. S3 is parked, so it is built on the current store. The end-to-end test writes the Titus import and checks the book file is tC3's text byte for byte, a valid Scripture Burrito, and every change Import(Tc3). Not written: a crash or cancel test of its own (create's crash test covers the same path), the .bak copy (tC4 4.0 only) | cargo test -p tc-import --test tc3
- 2026-10-24 15:08 EDT | domain lane (Claude Code 9f599c21) | step 7b | none | No-loss checks added and two findings fixed. update_a_book_conserves (property, 24 cases of six verses: project empty or with words, file empty, missing or with words) found (1) a verse the file lacks and the project holds as an empty stub was removed: the removal wins over a seed stub (§7.3); and (2) one conflicting verse took the batch's filled verses into its passage conflict, so they did not show. Fixed: a verse the file lacks, or holds inside a span, is left as it was and listed (the reply's `kept`), never removed or restructured; and Update a book is two batches, `UpdatePart::Fill` (book, empty verses filled, their alignments) then `UpdatePart::Review` (the no-parent changes, planned again on the new fold). STORAGE §13.6 U3 and U5 say so. a_verse_the_file_lacks_is_a_conflict_never_a_removal became a_verse_the_file_lacks_is_never_removed. Added a_no_base_import_never_becomes_the_provisional_text (proof: ranking a marked import like any other fails it, `left: Some("Paul, a slave of God") right: Some("Paul, a servant of God")`), updating_from_an_old_export_reverts_nothing (L148) and crash_import_leaves_no_project_or_a_whole_one in tc-import (a kill at every write point of create_imported). The funnel card's Checks and Invariants now name what exists, the cut items marked retired with the ruling; docs/TC4-ISSUES.md #796 (tC3) covered, #553 covered by the Add-a-book and Update tests, #529 pending DONE (tC4 4.0); docs/P-LEDGER.md P9 names the tC3 tests. The claims check passes with this card marked done | cargo test -p tc-store --test store update::
- 2026-10-24 15:18 EDT | domain lane (Claude Code 9f599c21) | verify | 1c034eaf | One review, loss lens (fresh subagent; ran tc-import and update:: tests, probed scenarios in a scratch copy). 1 Code: missing a structural "fillable" test. 2 Checks: no test of an existing conflict, a span, a removed verse or a rerun. 3 Reach: all reachable (sync or a stale save, span projects, rerunning Update). 4 Siblings: plan_outside_edit treats conflicts as unknown-base; plan_update_book's fill path did not. 5 No loss: BLOCKER, a verse in conflict whose shown side is empty was filled with every head as parents, merging the conflict away (the other side only in History); MAJOR, covered and removed verses counted as empty, so a file's separate verse could restructure a span or restore a removed verse; minors: a rerun added the same conflict side again; a bridge in the file on an empty project made a passage conflict; tC3 decisions on a covered verse got the md5 of ""; no carry-over port for an unresolved tC3 version (F1). 6 Prose: one comment contradicted the code. Wire format, rank 0, rule 8, NFC, #796 and P9 checked fine | review report quoted in this line
- 2026-10-24 15:18 EDT | domain lane (Claude Code 9f599c21) | fix | none | Blocker and major: a verse is filled only when the project holds one version of it (one head), an empty Text of the same shape; a verse the file lacks or holds in another structure than the project (a span, covered, removed) is left as it was and listed. Minors: a verse whose file version is already in its History is skipped (a rerun or a replayed review batch adds nothing); a bridge in the file is a structure difference, left as it was; a tC3 decision on a covered verse takes the covering span's text. The carry-over port for an unresolved tC3 version is F1's (minors.tsv). Tests a_verse_in_conflict_is_never_filled (proof: dropping the one-head rule fails it, `a side of the conflict was merged away`), a_bridge_in_the_file_restructures_nothing, updating_twice_with_one_file_adds_nothing; update:: 10 pass, tc-import 5 pass | cargo test -p tc-store --test store update::
- 2026-10-24 15:19 EDT | domain lane (Claude Code 9f599c21) | note | none | later: ["S3 from step 7"] removed: S3 is parked by rulings/2026-10-24-H3-H4-H7-fast-release.md, and steps 7 and 7a are built on the current store (as the ruling says for X). State stays verifying until DOM-2 is done, then gating for the batch | python3 -I .audit/tools/board.py --check
- 2026-10-24 16:20 EDT | domain lane (Claude Code 9f599c21) | fix | none | CI on c1b37878 failed one test: the fold lint did not name plan_update_book. It plans an outside edit of a whole book, so it joins plan_outside_edit in the lint list and in tc-app's clippy.toml | cargo test -p tc-app --test op_counts op_counts::lint::; cargo clippy -p tc-app --lib -- -D warnings
- 2026-10-24 19:05 EDT | domain lane (Claude Code 9f599c21) | verify | 18947b1f | Races and protocol lens (fresh subagent, code read, no edits). No blocker or major. Plan and commit of each part run together on the actor thread (actor.rs:160), so no plan goes stale; a crash or retry after only Fill is safe (Fill is one batch, a rerun fills nothing, Review re-plans); a replayed Review or two tabs with one file add nothing; two tabs with different files make a visible conflict; create_imported builds in a staging folder and renames with no replace, so a double click gives NameExists. Minors: the History skip in plan_update_book hides a verse the file deliberately sets back to an older text, where STORAGE 13.6 U3 asks for a conflict (fixed next, heads only); F1 must send Review right after Fill (minors.tsv) | review report quoted in this line
- 2026-10-24 19:40 EDT | domain lane (Claude Code 9f599c21) | verify | 18947b1f | Browser and parity lens (fresh subagent; built tc4 with test-hooks and fixture-backend and the UI, drove Chromium with a throwaway journey, read tC4 at tc4-ref). Verdict merge, no blocker or major. Reach: neither tC3 import (Home, Add a project, Import, translationCore 3 project does nothing, app_ctx.rs:823) nor Update a book (nothing sends UpdateBook) is on screen yet; F1 wires both, as the ruling splits it. Same as tC4: no tC3 check history; tC3 text through the usfm-js writer; the #796 rule. Minors: three differences undeclared in APP-DESIGN (every-occurrence decisions, Update a book itself, tC3 decision times), declared next; refusal words and addBook.usfmExists for F1 (minors.tsv) | review report quoted in this line
- 2026-10-24 19:45 EDT | domain lane (Claude Code 9f599c21) | fix | none | Races-lens minor: plan_update_book skipped a verse whose file text was anywhere in its History, so a file setting an older text back on purpose vanished from the report; it now compares with the verse's heads only, as STORAGE 13.6 U3 says, and makes a conflict. updating_from_an_old_export_reverts_nothing asserts the conflict and fails on the old rule (update.rs:206). Parity-lens minors: I's tC4 differences declared in APP-DESIGN's F1 Import row | cargo test -p tc-store --test store update::; cargo test -p tc-journal; cargo clippy -p tc-journal -p tc-store --all-targets -- -D warnings
