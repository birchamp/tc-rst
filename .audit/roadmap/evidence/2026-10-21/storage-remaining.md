# storage-remaining

## Summary
Storage is 4 of 10 units done (J1, J2, C, S1 on main). Nothing else in storage is merged. S2 (the placement-ledger state machine) is on s2-proof-executor @3b075d7 but is not verified at that commit. Its last full validation ran at 7d3a5d3, where 3 of 4 long-model seeds and 5 of 43 mutation recipes failed. Fixes were merged after that, and no validation has run since. Merging S2 conflicts in 12 paths against main and 13 against resume-build-lane. The real work is in open.rs: C1a's creation pins must be ported into S2's new writer-based open. Unit P's 23 commits on `storage` were built on the old S2 and were never reviewed. They conflict with S2 (7 paths) and with resume-build-lane (17 paths). Re-apply them on a fresh branch after S2 lands; do not merge `storage`. Most of T's task already landed on main through app and fix units. What remains is two derived-byte fixes that must land before the format freeze, a pin-order rule, and four missing tests. tc-sync (X) and tc-import (I) do not exist yet. tc-door43 has no git fetch, push or list. tc-store has no intake entry point. The owner has answered the questions for X and I. The owner still needs to decide the format 1.0 freeze (V), the 500 ms checkpoint row (P), and the mode-switch checkpoint (R17/Q-5). The storage critical path is S2 → X → I's team move → V → freeze. The cleanest piece to hand to another developer is I's file importers (new crate, oracle fixtures, narrow store API), built in parallel with X. Build the journal-files state machine after S2 merges. Its first two slices come before X, and its packs slice comes before X's intake.

# Storage units to done: assessment (2026-10-21)

Snapshots: `snap/<branch>`. Commands were read-only git commands (`merge-tree --write-tree`, `log`, `diff --stat`) plus greps.

## 0. Where storage stands

| Unit | State | Evidence |
|---|---|---|
| J1, J2, C, S1 | done, on main | resume-build-lane `.audit/STATUS.md:13-16`; S1 merged at 2d0cb2b (`rebuild.tsv:78`) |
| S2 | built and redesigned, **not verified at its head**, not merged | s2-proof-executor @3b075d7 (77 commits ahead of main, 541 behind) |
| P | 23 commits on `storage`, never reviewed or gated | `git log --first-parent 19e7fd2..storage`: 0929d5a…dd63328, 2026-10-19 00:35–05:39 -0400 |
| T, X, I, V | to do | STATUS.md:19-22 |
| JF (journal-files machine) | proposed | s2 `.audit/plans/JOURNAL-MACHINE-ASSESSMENT.md` |

**Crates missing from ARCHITECTURE.md's table** (`ARCHITECTURE.md:63-84`). On every branch: `tc-sync` (unit X creates it) and `tc-import` (unit I creates it). Every other crate in the table exists on main.

**App units blocked by storage** (resume-build-lane `build-lane.js` deps): D1 [S2,c2,c5], D2 [S2,b8], G1 [..d2], E1a [X,b10,c7a,d1], E1b [X,b9b,e1a], E2 [X,c1a,d2], F1 [I,c1a], G2 [P + all]. APP-DESIGN §9.1 (`APP-DESIGN.md:3223-3233`).

## 1. S2: finish and merge

1. **Scope.** Materialize off the actor, checkpoint and local git, outside edits, and carried files. All of it is decided by the placement-ledger state machine (`crates/tc-store/src/ledger.rs`: `State` :92, `Event` :106, `next()` :454) and proven by `tests/store/model.rs` (4,485 lines).
2. **Deliverables (exist).** New tc-store modules: writer, ledger, classify, checkpoint, git, repack, carried, outside, scan, watch, focus, pace, temps, packs, recovery. New test files: model, groups, carried, outside, checkpoint, placement_regressions, salvage_sources and others (`git diff --stat main...s2-proof-executor`, 264 files, +77,955 lines). CI job `store-model-nightly` (s2 `ci.yml:52`).
3. **Depends on.** Nothing open. Owner: the say-so to push main (see risk 4).
4. **Proves.** P3/P4/P5 for the store: `random_histories_lose_no_work` (200 histories per push; `_long` 5,000 nightly), `every_pair_of_the_table_keeps_every_byte`, the §16.2 crash rows M1–M6 and G1–G4.
5. **What remains** (s2 `HANDOFF.md:26-34`):
   - The last full run, `s2-validate-all.sh` at 7d3a5d3, failed. Gates passed (05:01–05:11Z). The mutation run exited 1: 38 of 43 recipes were detected, and L111, SOURCE_ALIAS, INCOMPLETE_CACHE, REMOVAL_LAST_LOOK and CARRIED_REFUSED_SETTLED were not. Long seeds: 001 passed; 002, 003 and 004 failed (worktree `.enjoy-logs/s2-validate-all.log`; `S2-PROOF-2026-10-21.md:3-11`).
   - The fixes were merged after that: a8e50e0, 9d8f11e, 2619a72 and 3b075d7 (1,768 tests in `s2-merge-proof-tests.log`, EXIT=0 at 16:23Z).
   - No validation log exists after 3b075d7.
6. **Owner questions.** None open for S2. The rulings are `rebuild.tsv` 2026-10-19T15:38Z and 15:41Z (the state machine) and 19:39Z (reachable scenarios only).
7. **Risks.**
   - The merge could drop C1a's creation pins: main's `write_creation_pins` sits inside the old open, at `open.rs` hunk 777-786 of the merge tree.
   - The merge could drop C5's R26 draft guarantees (drafts.rs test hunk: 223 lines against main, 485 against resume-build-lane).
   - The mutation recipes' anchors could drift on the merged tree (inference).
   - The HANDOFF contradicts itself on pushing main: line 33 asks for the owner's say-so, lines 74-75 say standing authorization.
8. **Size: 1–2 days.** One validation pass takes about 1 h 45 m (log timestamps 05:01→06:45Z). The last failure→witness→fix→merge cycle took about 6 h (`S2-PROOF-2026-10-21.md:19-55`).
9. **Merge conflicts.**
   - Main and s2-proof-executor, 12 paths: `.audit/{HANDOFF.md,minors.tsv,tools/README.md,workflows/build-lane.js,workflows/merge-main.js}`, `.github/workflows/ci.yml`, `Cargo.lock`, `Cargo.toml`, `crates/tc-store/src/{handle.rs,open.rs}`, `crates/tc-store/tests/store/{commands.rs,drafts.rs}`.
   - resume-build-lane and s2-proof-executor: the same 12 plus `crates/tc-store/src/hooks.rs`.
   - Hunks: open.rs 3 hunks/414 lines; commands.rs 210 lines; build-lane.js 8/240; Cargo.toml 2/40. Cargo.toml differs on git2 features: main has `vendored-libgit2`; S2 adds `https` and `notify`.

| Step | Goal | Files | Done when | Layer |
|---|---|---|---|---|
| S2-a | Validate 3b075d7 | `.enjoy-logs/s2-validate-all.sh` | GATES_EXIT=0, MUTATION_EXIT=0 (all 44 recipes detected), LONG_EXIT=0 for seeds 20261021001–004 | Checks |
| S2-b | Each failure becomes a named witness and a fix | `tests/store/model.rs`, product file | BEFORE/AFTER logs; new recipe in `mutation-executor/supplemental/recipes.py`; S2-a passes again | Code |
| S2-c | Re-review the changed areas only; record the stage | `.audit/minors.tsv`, `.audit/tools/lane-state.mjs` | No blocker, and no data or user major | Checks |
| S2-d | Merge into the main that resume-build-lane fast-forwards, in a separate worktree | the 13 paths. Port `write_creation_pins` before S2's O9 `writer.classify`. Union ci.yml (main's `rust` job plus `store-model-nightly` and the process-tools step). Take S2's git2 features. | All AGENTS gates, including `cargo clippy -p tc-app --lib`, `-p tc-server -p tc4 --lib --bins`, Windows-target clippy, wasm32 ×4; `open_writes_creation_pins_only_when_none_exist`, `a_save_never_erases_a_newer_draft_of_its_field`, `adding_a_book_from_usfm_is_one_batch_whole_or_absent`, `model::` pass; mutation executor rerun on the merged SHA; VM `gates.ps1`; storebench | Checks |
| S2-e | Record it | STATUS (status.py), HANDOFF, `rebuild.tsv` merge line | Push main after the owner's say-so | Prose/Human |

## 2. P: scale

1. **Scope** (`storage-chain-B.js:22,33`). Every budget that tc-journal, tc-store, tc-check, tc-align and tc-resources own, run end to end on the whole Bible. Also: R9 save latency during background work, three simulated years, memory M2/M3, a `perf` CI job (3× tolerance), and the tW/TWL ruling on the real exports.
2. **Deliverables.**
   - `crates/tc-corpus/examples/perfreport/` (exists on storage; 13 files, 6,799 lines).
   - `docs/evidence/storage-scale-<date>.md` (missing; only `machine.txt` and `laptop-ratios.json` exist).
   - STORAGE §14, PERFORMANCE "rows open", and the checkpoint review for the owner (`P.md` §13.2-13.4).
3. **Depends on.** S2 merged. The laptop session for the Windows rows. Owner: the 500 ms checkpoint row and R17/Q-5.
4. **Proves.** PLAN's PERFORMANCE rules, which feed G2. Done when `perfreport ci DIR --tolerance 3` passes in CI and the evidence doc gives a verdict for every owned row.
5. **What exists.** The perfreport frame and CI job (08a79c7), laptop ratios (a9bc72a), op counts (a694344, c5a82c1), off-actor batch and consolidation (b01c963), years (3b74198, fff9ac5), fold cache base+delta (eb108fb), domain speedups (fbef0dc, d28489a, bad2671, 0123eab, eacae8c, dd63328), the Windows script (afa8bc6).
   - The `storage` and `s2-proof-executor` merge conflicts in 7 paths, including `tc-store/src/seal.rs` (3 hunks/128 lines).
   - The `storage` and resume-build-lane merge conflicts in 17 paths, including tc-align `suggest/*`, tc-check `change.rs`/`rail.rs`, tc-resources `load.rs`, and tc-store `actor`/`open`/`store`.
6. **Owner.** The checkpoint row (S2 measured 460–591 ms median against 500 ms; s2 `STORAGE.md:4634`). The cold open measured five times on the laptop (ruling `rebuild.tsv:94`).
7. **Risks.** Porting b01c963/eb108fb onto the redesigned seal and open could break no-loss. They must pass `model::` and the crash rows.
8. **Size: 2–3 days.** The original build was 23 commits in about 5 h. The store underneath changed by +16,598/−1,873 lines (`git diff --shortstat 19e7fd2 s2-proof-executor -- crates`), and three rounds plus a laptop run are required.

| Step | Goal | Done when | Layer |
|---|---|---|---|
| P-1 | Branch `p-scale` from post-S2 main; cherry-pick the perfreport/tc-corpus/spikes commits | `cargo build --release -p tc-corpus --example perfreport`; `perfreport ci DIR --tolerance 3` | Checks |
| P-2 | Re-port the op-count tests and add the `perf` CI job | `tc-journal/tests/operation_counts.rs` and `tc-store/tests/store/opcount.rs` pass on both corpora | Checks |
| P-3 | Re-port the product speedups one at a time (eb108fb, b01c963, then the domain ones against main's tc-align/check/resources) | each has before/after numbers; `model::` and `crash::` pass | Code |
| P-4 | Three rounds of `run`, `r9`, `memory`, `growth`, `years`; write the evidence doc, §14 and the checkpoint review | evidence doc committed | Checks→Prose |
| P-5 | Laptop run (`spikes/perfreport-windows`) | laptop table in evidence | Human |

## 3. T: storage side of the tC4 issue sweep

Already done on main:
- #582 refusal: 3ecdcae, `create_refuses_a_language_tag_the_burrito_cannot_hold`. The Q35 suggestion is in tc-ui (`a_door43_code_a_burrito_cannot_hold_is_offered_adjusted`) and the `door43Language` setting is in tc-app (`create.rs:119-123`).
- #767: 3ecdcae.
- #789: both reflow call sites (`commands.rs:280,527` → `reflow_edit` at :349).
- #386: test exists.
- §9.5's 403 correction: main `STORAGE.md:3404-3420`.

Remaining:

| Step | Goal | Files | Done when | Layer |
|---|---|---|---|---|
| T-1 | One review rule: the fold's alignment `invalid` reads what `tc_align::needs_review` reads (words of the verse, pin). Move the rule into `tc_journal::alignment`, because tc-align depends on tc-journal (`tc-align/Cargo.toml:14`), and re-export it from tc-align | `tc-journal/src/alignment.rs`, `tc-align/src/value.rs:108` | `screens_and_fold` counts 0 for `words_not_the_verse` (today 168 of 357 corpus alignments; TC4-ISSUES row 6) | Code |
| T-2 | Splice an empty OBS frame the way tC4's `writeFrame` does (seed-check §5.2), then pin the relationships row order and first-row-per-repository (§4.3 remainder) | `tc-journal/src/fold/project.rs:2142`, `fold/metadata.rs` | `obs_frame_edits_project_other_bytes_than_tc4_writes_today` flipped to equality | Code |
| T-3 | Add the missing tests | `a_file_that_cannot_be_an_ingredient_is_moved_out_and_kept` strengthened (#747: `.bak`, `~` and `.tmp` files, plus a tree assertion), `a_new_device_copies_the_profile_and_both_group_as_mine` (#161), `loss_unknown_parent_verse_edit_keeps_standing_text` (#179), `a_verse_edit_of_an_inline_aligned_verse_writes_no_alignment` (#794) | each fails with its rule removed | Checks |
| T-4 | Close the rows | `docs/TC4-ISSUES.md`, `.audit/rebuild.tsv` | rows say covered | Prose |

- **Risk.** T-1 and T-2 change derived bytes. Land them before V's freeze (STORAGE §12.2 rule 3).
- **Size: 0.5–1 day.** Basis: 3ecdcae did two T items in one commit (+395 lines).

## 4. X: tc-sync (Door43 sync and USB exchange)

1. **Scope.**
   - STORAGE §9 (sync S0–S10, intake N1–N6, integrate T1–T3, push/confirm, share J11, join J17, report, editing during a sync).
   - §10 (USB export E1–E5 with a verify from the device, import R1–R8).
   - §11.2 (Door43 web edits) and §11.3 (edits inside an exchange file).
   - APP R10 and R13. `moved.json` once. Straggler refusal.
2. **Deliverables.**
   - New crate `crates/tc-sync`.
   - tc-door43: `fetch`, `push` and `list` on a `git2::Repository` behind `InternetSwitch` (today only `push.rs:44 refused_push` exists), plus the `permissions.push` pre-check (Q8).
   - tc-door43-fake: a dropped-connection fault for smart HTTP (`Fault` at `lib.rs:189` has `BreakOff` for downloads only).
   - tc-store: one intake entry point. `pending::Purpose::Intake` exists only in the codec (`tc-journal/src/pending.rs:31`).
   - The app replaces `ScriptedSync` (`tc-app/src/sync.rs:118`).
3. **Depends on.** S2. JF's packs slice before X-3 (inference: intake writes packs through the same code). The owner has answered everything: DELTA-10-16 Decisions (`TC4-DELTA-2026-10-16.md:1142-1160`), one-way move and `moved.json` (`rebuild.tsv:61`), Q12 by measurement.
4. **Proves.**
   - P2: library-level J17, J18 and J26, and USB both ways.
   - P3: `fold_converges_under_any_union` over the exchange-zip and git transports (`Transport`, `tc-journal/tests/properties.rs:47`), `door43_ingest_deterministic`, `exchange_idempotent_and_relayed`.
   - P4: Y1–Y6, U1–U2, I1–I2.
   - P5: every exchange zip.
5. **New tC4 input** (b86270f): #801 sync refusal (095144ce) and #791/D108 (43164cbe).
6. **Risks.** A forced push. Intake writing over someone's file (M1 compare-and-swap). Losing received work after a refused push. A copied username taken as "Mine" (#161).
7. **Size: 5–8 days** (inference). It is S1/S2-sized: S1 was 16 commits and +19,944 lines in about 18 h. S2 was 40 commits in about 16.5 h, then about 2.5 days of proof that is still open. X also falls under the owner's state-machine rule.

| Step | Goal | Done when | Layer |
|---|---|---|---|
| X-1 | `SyncState`/`SyncEvent`/pure `next()` for S0–S10, refusals typed onto tc-api `SyncFailure`/`SyncRefusal` | exhaustive matrix test | Code |
| X-2 | tc-door43 git calls and permissions check; fake dropped-connection | tests against the fake (`git http-backend`) | Code |
| X-3 | `ProjectHandle::take_in(Intake)` (§9.3 T1–T3, §10.2 R7) and a two-parent merge commit through S2's `build_tree` | crash rows Y1–Y6 and I1–I2 | Code |
| X-4 | Sync, retries (30 s cap, Q11), confirm, report, repack | `door43_ingest_deterministic`; three-device library J26; push never forced | Checks |
| X-5 | Share, join, connect; username at first sync (Q7); `moved.json` | named tests per §9.6 | Code |
| X-6 | USB export and import | `exchange_idempotent_and_relayed`; transports added to `fold_converges_under_any_union` | Checks |
| X-7 | Budgets: sync 3 s, export 15 s | `docs/evidence/sync-<date>.md`; op counts | Checks |

## 5. I: tc-import

1. **Scope.** STORAGE §13:
   - tC4 4.0 with a journal (T1–T10, oracle and reconcile, hidden work shown as conflicts);
   - the seed (§13.2; B1a's code lives in `tc-app/src/backend/seed`);
   - the team move and bridge (§13.3);
   - tC3 (§13.4);
   - USFM and foreign burritos (§13.5).
   It also covers R11 (dry run, job, tC3 version lookup) and R29 (a project.json writer).
2. **Deliverables.** New crate `crates/tc-import`. It replaces the app's `ScriptedImport`.
3. **Depends on.** S2, and X for §13.3. Fixtures: `fixtures/tc4/conformance-fixtures/import/{usfm,burrito,damaged,tc3}` and `team-sync/journals/translator-{a,b,c}`. The owner has answered: Q23, Q13, Q22 by recommendation; the alignment-stamp ruling (`rebuild.tsv:81`). New tC4 input not in the rebuild's docs (grep finds no #813 or D110): 0b4cb270 and 71c2cb36 (#813/D110), e79a2795 (#796 tC3 alignment flag).
4. **Proves.** P9 (each fixture's text byte-identical; refusal of debris and torn metadata), P4 (N1–N2), P5. Tests: one per fixture plus damaged-input cases.
5. **Risks.** A partial project left after a cancel. Writing into an existing project. Hiding tC4's silent losses.
6. **Size: 3–5 days** (inference). B1a's seed took about 2 h (commits 21:06–22:43 on 10-18). §13.1 is the bulk.

| Step | Goal | Done when | Layer |
|---|---|---|---|
| I-1 | One entry `import(source) -> ImportJob`, a typed `ImportState`, staging→place, cancel leaves nothing | crash and cancel tests | Code |
| I-2 | Raw USFM and burrito (creation pins C36, split C45) | per-fixture tests | Checks |
| I-3 | Move the seed into tc-import | the 31 `state_seed` tests pass from tc-import | Code |
| I-4 | tC4 4.0 converter and oracle (8 MiB, C38; NFC, C35) | experiments E1–E6, L2, L9–L11, L16 come back as conflicts | Checks |
| I-5 | tC3 | `tc3/*.zip` tests | Checks |
| I-6 | Team move and bridge (after X) | fake-Door43 straggler tests | Checks |
| I-7 | Whole-Bible import within 2 min | `docs/evidence/import-<date>.md` | Checks |

## 6. V: old projects keep working

1. **Scope** (`storage-chain-B.js:31`). Capsules under `fixtures/format/<v>/` (none exist), a manifest and a sha-256 lock, a read test, mixed versions, a byte pin, and a CI job.
2. **Builds on.** `the_encoding_is_pinned` (`tc-journal/tests/index.rs:867`), `tests/versioning.rs` (20 tests), `mixed_minor_versions_converge` (`properties.rs:1179`).
3. **Depends on.** X and I (the capsules need sync, USB and import), and T-1/T-2. Owner: freezing format 1.0 is open (HANDOFF "What needs the owner").
4. **Proves.** P10.
5. **Risk.** Someone edits a capsule to make a test pass. The lock test is the guard.
6. **Size: 1.5–3 days** (inference; nothing comparable has been built).

| Step | Goal | Done when | Layer |
|---|---|---|---|
| V-1 | Generator that refuses to overwrite; first set `1.0-draft` | lock file | Code |
| V-2 | Read, edit and exchange every capsule | test passes | Checks |
| V-3 | Byte pin of every kind and header | the pin fails on change | Checks |
| V-4 | Simulated older reader, then tag `format-v1.0` | CI job | Checks |
| V-5 | Write the release procedure; owner freezes | `rebuild.tsv` line | Human |

## 7. JF: journal-files state machine (proposed)

1. **Scope.** One typed machine for pending files, packs, temps, quarantine, salvage and not-saved copies. Today their state lives in `Journal` (`tc-store/src/project.rs:65-77`: five sets) plus `PendingInfo` (:53).
2. **Defects to test.** H1 and the others the assessment infers (`JOURNAL-MACHINE-ASSESSMENT.md:47-61`, none reproduced). I spot-checked `seal.rs:151-181` and `packs.rs:192-201`; they match the citations.
3. **Steps.**
   - JF-1: extend the model to reach the missing states [Checks].
   - JF-2: public BEFORE tests for H1, (e) and the others [Checks].
   - JF-3: `journal_file::next()` with per-kind matrices, running in shadow [Code].
   - JF-4 to JF-7: switch over copies, then pending retirement, packs and temps, each with mutation recipes [Code].
4. **Size.** 1–1.5 weeks for slices 1–2, 2–3 weeks for all four. This is the assessment's critics' estimate, and it fits S2's ledger history: started at 9d0df2d on 10-19 and still not verified on 10-21.

## 8. Critical path and split

- **Storage chain:** S2-finish (1–2 d) → JF-1..JF-4 and X-1/X-2 in parallel → X-3..X-7 → I-6 → T-1/T-2 already in → V → owner freezes.
- **App:** S2 → D1/D2 → G1, E2. X → E1a → E1b. I → F1. P → G2.
- **Handoff to a second developer:** I-1..I-5 and I-7 (tc-import). It is a new crate, it is checked against tC4's own outputs as oracles, and it touches only `Store::staging/place`, the seal of a new folder, and tc-journal types. It integrates by a merge into main after S2, with I-6 done by the X owner. Keep X and JF with the storage owner: both change tc-store internals.


## Findings
- [ST-1] (critical/high) S2 is not verified at its head 3b075d7. The last full validation ran at 7d3a5d3 and failed: mutation run exited 1 with 38 of 43 recipes detected, and long seeds 002, 003 and 004 failed. Fixes were merged afterwards, and no validation has run since.
  - evidence: worktree s2-proof-executor .enjoy-logs/s2-validate-all.log: VALIDATE_START 05:01:46Z at 7d3a5d3, MUTATION_EXIT=1, LONG_EXIT=1 for seeds 20261021002/3/4; snap/s2-proof-executor/.audit/plans/S2-PROOF-2026-10-21.md:3-11; snap/s2-proof-executor/.audit/HANDOFF.md:26-34 ('Next, in order: (1) full validation at the merged SHA')
  - rec: Run .enjoy-logs/s2-validate-all.sh at 3b075d7 before any merge. Turn every failure into a named witness and a fix, and repeat until all 44 recipes are detected and all 4 seeds pass.
- [ST-2] (high/high) Merging S2 conflicts in 12 paths against main and 13 against resume-build-lane (hooks.rs is the extra one). The semantic work is in open.rs: 3 hunks, 414 lines, where C1a's write_creation_pins sits in the old open path that S2 replaced.
  - evidence: git merge-tree --write-tree --name-only claude/focused-ride-5uzj7p s2-proof-executor → 12 CONFLICT lines; git merge-tree --write-tree --name-only resume-build-lane s2-proof-executor → 13 CONFLICT lines; merged tree b06d3d8 open.rs lines 669-673, 777-804, 1587-1617
  - rec: Merge into the main that resume-build-lane fast-forwards, in a separate worktree. Port write_creation_pins before S2's O9 classify. Gate on open_writes_creation_pins_only_when_none_exist, the R26 drafts tests, model:: and a mutation rerun on the merged SHA.
- [ST-3] (high/high) Unit P's 23 commits on storage (0929d5a..dd63328) were built on the pre-redesign S2 (19e7fd2) and never reviewed or gated. Its evidence doc was never written. Merging storage conflicts with s2-proof-executor in 7 paths and with resume-build-lane in 17.
  - evidence: git log --first-parent 19e7fd2..storage (2026-10-19 00:24–05:39 -0400); git ls-tree storage docs/evidence: only storage-scale-2026-10-19/{laptop-ratios.json,machine.txt}; merge-tree s2-proof-executor storage: seal.rs 3 hunks/128 lines; merge-tree resume-build-lane storage: 17 paths incl. tc-align suggest, tc-check change/rail, tc-resources load
  - rec: Do not merge the storage branch. Re-apply P on a fresh branch from post-S2 main: tooling first, then each product speedup alone, under model:: and the crash tests.
- [ST-4] (high/high) tc-sync and tc-import exist on no branch. tc-door43 has no git fetch, push or list, only refused_push. The fake has no dropped-connection fault for smart HTTP. tc-store has no public intake entry.
  - evidence: ls crates on all four snapshots; snap/resume-build-lane/crates/tc-door43/src/push.rs:44 is the only pub fn; tc-door43-fake/src/lib.rs:189-236 Fault variants; APP-DESIGN.md R13 row; grep commit_received in s2 tc-store/src: none; tc-journal/src/pending.rs:31 Purpose::Intake
  - rec: X creates tc-sync and I creates tc-import. Begin X with the typed sync state machine and a single tc-store take_in entry point.
- [ST-5] (medium/high) Most of T's task already landed on main through C6, FIX-align and windows-push-refusal. Remaining: the fold's invalid flag vs needs_review, the OBS empty-frame splice, the pin relationships row order, and four missing tests (#747, #161, #179, #794).
  - evidence: 3ecdcae (#582, #767); main tests tc4.rs:406, commands.rs:1007; resume-build-lane tc-store/src/commands.rs:280,527,349 reflow_edit; main docs/STORAGE.md:3404-3420 (200 + hook decline); resume-build-lane docs/TC4-ISSUES.md row 6 (168 of 357), :285, :304, :164; tc-app/src/backend/seed/tests.rs:719-760
  - rec: Shrink T to T-1..T-4. Put the rule in tc_journal::alignment, because tc-align depends on tc-journal.
- [ST-6] (high/high) T-1 and T-2 change derived bytes, so they must land before format 1.0 is frozen. After the freeze they would need a minReaderMinor bump, and older apps would turn read-only.
  - evidence: snap/s2-proof-executor/docs/STORAGE.md §12.2 rule 3 (derived bytes raise minReaderMinor); TC4-DELTA-2026-10-19 §3.3 item 1 'land it before format 1.0'
  - rec: Gate V-1 (capsule set) on T-1 and T-2 being merged.
- [ST-7] (medium/high) X and I are not blocked on the owner. The sync and import questions were answered on 2026-10-16 and 2026-10-18.
  - evidence: TC4-DELTA-2026-10-16.md:1142-1160 Decisions (Q2, Q3, Q8, Q15, Q1, Q4; every other question follows its recommendation); rebuild.tsv:61 (moved.json), :81 (alignment stamps); TC4-DELTA-2026-10-16.md:1231-1238 Q12 answered by measurement
  - rec: Start X and I without new owner questions. Report only translator-visible differences.
- [ST-8] (medium/high) tC4 changes after the last sweep (9ad4d5c..b86270f) touch import and sync and are not in the rebuild's docs: #813/D110 (a burrito with no vrs.json), the #796 tC3 alignment flag, and #801 sync refusal.
  - evidence: tc4-tip git log: 0b4cb270, 71c2cb36, e79a2795, e17fce08, 095144ce, 43164cbe; grep '#813|D110|#822' in resume-build-lane docs and .audit: no match
  - rec: Add X-0 and I-0 mini-sweeps (Prose) before those units build.
- [ST-9] (high/high) V needs X, I and T-1/T-2 to land first, and needs the owner's format freeze to cut its final capsule set. The pin and versioning tests it builds on already exist.
  - evidence: rebuild.tsv:96 (V after I); tc-journal/tests/index.rs:867 the_encoding_is_pinned; tests/versioning.rs; properties.rs:1179; no fixtures/format directory on any snapshot
  - rec: Cut 1.0-draft first. Ask the owner for the freeze once V-2 passes.
- [ST-10] (medium/high) The CI on main and resume-build-lane has no storage gates: no store-model-nightly (it is only on s2-proof-executor) and no perf job (only on storage). No P10 job exists anywhere.
  - evidence: resume-build-lane .github/workflows/ci.yml jobs: rust, alignment-budget, parity-baseline; s2 ci.yml:52 store-model-nightly; storage ci.yml:45 perf
  - rec: Union ci.yml in the S2 merge, add perf in P-2, and add the capsule job in V-4.
- [ST-11] (high/medium) S2's state machine covers derived files only. The journal's own files have no machine: their state is spread over Journal's five sets plus PendingInfo, and five defects are inferred but not reproduced.
  - evidence: s2 tc-store/src/project.rs:53-77; JOURNAL-MACHINE-ASSESSMENT.md:14-18, 47-61; seal.rs:151-181 and packs.rs:192-201 match the cited lines
  - rec: Build JF after S2 merges, starting with the model and BEFORE tests. Land its packs slice before X's intake (inference).
- [ST-12] (high/medium) The storage critical path is S2 → X → I team move → V → freeze. S2 also gates D1 and D2, and through them G1, E1a and E2. Moving I's file importers to a second developer, in parallel with X, shortens it.
  - evidence: resume-build-lane build-lane.js deps: d1 [S2..], d2 [S2..], e1a [X,..,d1], e2 [X,..,d2], f1 [I,..], g1 [..d2]; STORAGE §13 vs §13.3 (only the team move needs sync)
  - rec: Hand I-1..I-5 and I-7 to a second developer once S2 is on main. The X owner does I-6.
- [ST-13] (low/high) S2's HANDOFF contradicts itself on pushing main: one line requires the owner's say-so, another says standing authorization.
  - evidence: snap/s2-proof-executor/.audit/HANDOFF.md:33; snap/s2-proof-executor/.audit/HANDOFF.md:74-75
  - rec: Ask the owner once and fix both lines.
- [ST-14] (medium/high) The storage rows in STORAGE §14 predate S2's redesign. The checkpoint row missed: 460–591 ms median against the 500 ms budget.
  - evidence: snap/s2-proof-executor/docs/STORAGE.md:4633-4644 ('measured by unit S2', 2026-10-18)
  - rec: P re-measures on post-S2 main and puts the checkpoint review to the owner. P does not change the budget.

## Unverified
- Whether S2 at 3b075d7 passes validation: cargo was forbidden to me. Run .enjoy-logs/s2-validate-all.sh in the S2 worktree.
- How much semantic work porting C1a's creation pins and C5's draft tests into S2's open takes. I saw only the conflict hunks. Do the merge in a scratch worktree and run the named tests.
- Whether the mutation-recipe anchors survive the merge. Run execute.py --cases all on the merged SHA.
- Whether perfreport's r9, memory and years modes ever produced numbers. No evidence doc exists. Run perfreport on P-1's branch.
- Whether C7a and C7b, now running on resume-build-lane, will add tc-store conflicts. Rerun merge-tree when the lane stops.
- The journal-machine defects H1, (e) and the others: the assessment infers them and none is reproduced. JF-2 settles them.
- Every size estimate is an inference from git timestamps and diff sizes, not a measurement.
- Whether JF's packs slice must precede X's intake is an inference. Check whether X-3's pack writes go through packs.rs/seal.rs paths that JF replaces.