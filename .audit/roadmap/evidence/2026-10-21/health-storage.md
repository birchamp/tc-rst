# health-storage

## Summary
The storage core is well defended at the level of single decisions. A typed placement ledger (ledger.rs:454) decides who wrote a file. Panics are banned by lint. The fold crate does no I/O. Around that core, though, S2 has more than doubled tc-store, from 11,869 to 26,046 source lines. Its bugs now live in the executors, the code that gathers facts, and per-path state kept outside the ledger.

- **Executors are long.** run_group is 506 lines and open_inner 473.
- **The Writer struct is large.** It has 42 fields, 12 of them maps.
- **Preservation copies have six separate implementations.** Some can overwrite an existing copy.
- **Some writes skip the crash model.** These are the .git rebuild in history.rs and all libgit2 writes.

The loss list grew from 96 rows to 143 (2026-10-15 to 10-21). Each S2 review round added fewer rows (22, then 10, then 8), but the count has not reached zero. S2 has not been verified at 3b075d7.

I classified the 40 S2 rows myself (inferred):
- 26 come from outside-edit detection and the placement ledger, 12 of them from metadata.json.
- 9 come from carried-file history kept inside git.
- 5 come from journal files alone.
- 19 of the 40 need another program to cut or delete a journal file first.

The design is more complex than reachable scenarios need in four places:
- treating every journal-file loss as a value-precise recovery;
- recording outside edits of metadata.json automatically;
- carried-file backup engineered inside libgit2;
- test adversaries outside REACH (same-stat damage, byte flips, a lost .tc/, 999 copy names).

Recommendation: merge S2 unchanged. Then run a simplification unit (S3) before X:
- add the checks first;
- one preservation helper;
- the journal-file state machine;
- pure fact producers and step-split executors;
- per-path session state inside the ledger.

Three scope cuts need the owner's ruling: same-stat damage, metadata.json auto-recording, and carried-file scope.

# Storage core code health (S2 branch `s2-proof-executor` @ 3b075d7)

All numbers below come from the plain-file snapshots under `scratchpad/snap/`. Two snapshots are used:

- **s2**: `s2-proof-executor`.
- **rbl**: `resume-build-lane`, which carries main's older S1 version of tc-store.

Some numbers come from read-only git commands in `/home/bot/tc-rst`. The rest come from my scripts in `scratchpad/work/storage-health/` (`measure.py`, `callsites.py`, `structs.py`). Those scripts are heuristics: they treat everything after `#[cfg(test)] mod` as test code and find function ends by matching braces.

## 1. Size

| Crate | s2 src lines (files) | s2 lines outside test modules | rbl src lines (files) |
|---|---|---|---|
| tc-store | 26,046 (46) | 22,901 | 11,869 (30) |
| tc-journal | 25,271 (41) | 24,498 | 23,603 (39) |

- **tc-store tests:** about 25,173 lines on s2 against 7,906 on rbl.
- **`#[test]` and `proptest!` markers:** 435 on s2 against 150 on rbl.
- **Change since the merge base 7e62455** (`git diff --stat` over tc-store and tc-journal src):
  - s2: +18,112 / −1,705 lines.
  - rbl: +758 / −196 lines.

**Largest tc-store files on s2:**

| File | Lines |
|---|---|
| materialize.rs | 2,211 (rbl: 1,051) |
| git.rs | 2,025 |
| commands.rs | 1,667 |
| open.rs | 1,638 |
| fault.rs | 1,552 |
| handle.rs | 1,033 |
| source.rs | 995 |
| checkpoint.rs | 914 |
| outside.rs | 905 |
| atomic.rs | 869 (rbl: 308) |
| ledger.rs | 859 |

fault.rs is the test filesystem. It is declared `pub mod fault` in lib.rs with no `cfg`, so it compiles into release builds.

**Largest tc-journal files on s2:** fold/state.rs 2,333, fold/project.rs 2,289, canon.rs 2,098, fold/resolve.rs 1,623.

**tc-store lines by mechanism on s2** (whole files, in-file tests included):

| Mechanism | Files | Lines |
|---|---|---|
| Placement and outside edits | ledger, classify, materialize, writer, manifest, outside, focus, scan, watch | 6,474 |
| | plus tc-journal outside.rs | 920 |
| Local git | git, checkpoint, repack, history | 3,856 |
| Journal files | seal, temps, packs, recovery, source, load | 2,740, plus part of open.rs |
| Carried files | carried.rs | 780, plus part of git.rs |
| Test infrastructure inside src | fault, hooks, pace, counters | 2,452 |

## 2. Longest functions (outside test modules)

**tc-store on s2:** 853 functions. 17 are 100 lines or longer, and 3 are 200 or longer.

| Function | Location | Lines |
|---|---|---|
| `run_group` | materialize.rs:698 | 506 (steps M0–M8, about 90 `if`/`match`/`=>` lines) |
| `open_inner` | open.rs:269 | 473 (steps O1–O14) |
| `build_tree` | checkpoint.rs:473 | 316 |
| `metadata_entries` | materialize.rs:1910 | 186 |
| `save_section` | commands.rs:393 | 150 |
| `restore_with` | packs.rs:41 | 133 |
| `finish_interrupted` | open.rs:1084 | 124 |
| `execute_carried_one` | carried.rs:645 | 122 |
| `other_event` | ledger.rs:479 | 120 |

**tc-store on rbl:** 463 functions, 10 of them 100 lines or longer. The longest are `open_inner` (330) and `materialize` (295).

**tc-journal on s2:** 1,120 functions, 13 of them 100 lines or longer, none 200 or longer. The longest is `plan_outside_edit` (outside.rs:676, 191 lines).

## 3. State spread over flags and sets

- **`Writer`** (writer.rs:157-262) has 42 fields:
  - 6 bools: `network`, `manifest_changed`, `complete`, `opening`, `wrote_group`, `collecting`.
  - 2 sets: `unattempted`, `carried_kept`.
  - 12 maps. Nine of them hold per-path session state: `busy`, `unrecorded_metadata`, `written_seq`, `reasons`, `pending_record`, `shown_by_path`, `bases`, `pending_carried`, `waiting`.

  So a path's state is split between the ledger's `State` (kept in `manifest`) and these maps.
- **`Journal`** (project.rs:65-76) keeps packs and pending files in maps, plus three sets: `unreadable`, `missing`, `left`.
  - `PackInfo` adds `valid` and `damaged` bools (project.rs:35,48), and `PendingInfo` adds a `status` (project.rs:60).
  - On rbl, `Journal` has only `unreadable` and `missing`. S2 added `left`, plus `blob`, `seen` and `damaged` on `PackInfo`.
- **`Dirty`** (project.rs:105) has 5 bools and 2 sets. That is acceptable for a set of marks.
- **tc-journal:** `FoldState` has 55 fields, laid out for the memory budget. `BatchEntry` keeps `many`, `complete` and `held` as bools (fold/state.rs:233).

## 4. Typed state machines and ad hoc decisions

**Typed:**

- **`ledger::next`** (ledger.rs:454) is pure. It maps 4 states (`Unknown`, `Placed`, `Writing`, `Held`) and 11 events, plus `Facts`, to rows R1–R19 and 8 actions.
  - It has one executor entry, `Writer::apply` (classify.rs:87).
  - Hit counters (ledger.rs:274-330) let the model check its coverage.
- **`SealPhase`** (seal.rs:58, 7 phases) and **`Found`** (seal.rs:105, 6 kinds) are typed enums. The decisions made for each `Found` kind are written inline in seal_step and seal_read.
- **`recovery::next`** (recovery.rs:31) is trivial: 5 targets map to 3 actions.
- **`Mode`** (handle.rs:210) is typed.

**Ad hoc:**

- **The journal-file lifecycle.** About 17 call sites in seal, open, temps, packs and writer delete or overwrite journal files (JOURNAL-MACHINE-ASSESSMENT.md). `temps::settle_temps` (temps.rs:177-249) decides its four outcomes in an if-chain.
- **Writer's per-path maps** (section 3).
- **metadata.json's two-phase recording**, held in `pending_record`, `unrecorded_metadata`, `shown_by_path` and `bases`.
- **Backup of superseded carried versions** (carried.rs, git.rs).
- **Six preservation-copy implementations** (section 5).

**A decision has leaked into the executor.** `set_state` clears the scope digest whenever the md5 changes (classify.rs:56-71, the fix for L123). That decision sits outside `next()`.

**Where bugs land now that the ledger exists.** After the ledger landed (0821958), 10 product-fix commits changed storage src:
- 9 of them changed materialize.rs or classify.rs (4810cf5, f5aa408, 5cb8bb8, 34098bb, 8e3eb1a, 85fa081, f5e1613, 2d4219d, 3d9dfaa);
- only 2 changed ledger.rs (f5aa408, 5cb8bb8).

So the bugs live in the code that produces facts (`holds`, `shows_held`, `read_base`, `current_heads`) and in the executors. They are not in the pure machine.

## 5. File-mutation call sites (s2 tc-store src, outside test modules, excluding atomic.rs and the Vfs wrappers)

**Through the atomic.rs helpers.** atomic.rs has 14 public entry points. Their callers:

| Helper | Call sites |
|---|---|
| `atomic_write*` | 22 |
| `atomic_create` | 2 |
| `atomic_remove*` | 11 |
| `*_held` retries | 11 |
| `atomic_create_dirs` | 30 |

**Six separate "keep a copy" implementations, with different guarantees:**

| Location | Method | Can it replace an existing file? |
|---|---|---|
| open.rs:236 `restore_project_json` | `atomic_write` | yes |
| materialize.rs:154 `copy_external` | no-replace `atomic_create` | no |
| recovery.rs:41 `preserve_bytes` | no-replace `atomic_create` | no |
| temps.rs:265 `keep_as_copy` | probe, then `rename_held` | yes (the last name) |
| scan.rs:167 `move_out` | probe, then `rename_held` | yes |
| packs.rs:189 `write_back` | `atomic_write` into `.tc/quarantine` | yes |

**Writes that bypass `Vfs`, so FaultFs cannot inject crashes into them:**
- history.rs:119 and 129: the G4 rebuild uses `std::fs::create_dir_all` and `std::fs::rename` to move `.git` aside.
- repack.rs:597 and 612, git.rs:1079: `std::fs::remove_file` inside `.git`.
- Every libgit2 write. This is by design (git.rs:4-7); `core.fsyncObjectFiles` is on (git.rs:167).

**Justified in-place write.** The lock file's content is set in place (open.rs:339, actor.rs:510); vfs.rs:76-80 explains why.

**Nothing enforces the helpers.** `Vfs::rename`, `remove_file` and `create_new` are public trait methods (vfs.rs:92-103), and no lint restricts them. The workspace already uses `disallowed-methods` (rbl clippy.toml:7-13), so the mechanism is there.

## 6. Panics

- tc-store has 1 `panic!` in library code (handle.rs:85). It carries a proving comment and `#[allow(clippy::panic)]`.
- tc-journal has none. The four `.expect(` matches in json.rs:606-678 are the parser's own `expect(byte)` method.
- Enforcement is in place: Cargo.toml:18-23 sets `unwrap_used`, `expect_used` and `panic` to `warn`, and CI runs clippy with `-D warnings` (ci.yml:26).

## 7. History

**Rows in STORAGE §15**, counted with `git show <c>:docs/STORAGE.md | grep -c '^| L[0-9]'` along the s2 history:

| Commit | Time (UTC) | Rows | Doc lines |
|---|---|---|---|
| 59c550b | 10-15 07:58 | 96 | 3,507 |
| 2daead2 | 10-16 11:08 | 102 | |
| 65aba0d | 10-18 12:39 | 103 | 4,854 |
| 85fa081 | 10-19 22:31 | 125 | |
| f5e1613 | 10-21 00:04 | 135 | |
| 3b075d7 | 10-21 16:24 | 143 | 5,517 |

The 40 rows that S2's reviews found (L104–L143) came in about 2.5 days. Successive review rounds added 22, then 10, then 8 rows: the rate is falling but has not reached zero.

**The fix loop.**

- **Commits:** 73 non-merge commits on s2 since 36b1296. By UTC day: 18 on 10-18, 46 on 10-19, 3 on 10-20 and 6 on 10-21.
- **Storage src churn:** +12,365 / −1,415 on 10-18, +4,345 / −1,811 on 10-19, and +3,513 / −593 on 10-21.
- **f5e1613 alone** touched 20 storage src files, +2,982 / −469.
- **Validation at 7d3a5d3:** 38 of 43 mutants were detected. Long-model seed 001 passed; seeds 002 to 004 failed. Of the five failing histories, 2 were product bugs and 3 were oracle bugs (S2-PROOF-2026-10-21.md).
- **At 3b075d7:** full validation is still pending. That means 44 recipes, 4 long seeds and a re-review (HANDOFF.md, S2 section).

## 8. Which mechanisms generate the loss cases

I classified all 143 rows myself (inferred):

| Mechanism | All rows | S2 rows (L104–L143) |
|---|---|---|
| Merge and fold semantics | 43 | 0 |
| Outside-edit detection and the placement ledger | 35 (14 of them metadata.json) | 26 (12 metadata.json) |
| Journal files: pending, packs, seal, quarantine, salvage | 18 | 5 |
| Carried files | 17 | 9 |
| Door43 sync | 16 | 0 |
| USB exchange | 4 | 0 |
| Atomic writes and durability | 4 | 0 |
| Other | 3 | 0 |
| Local git | 2 | 0 |
| Import | 1 | 0 |

**19 of the 40 S2 rows need a journal file to be cut, flipped, deleted or made unreadable before the loss can happen:** L104, L105, L108–L110, L113–L116, L118, L120, L121, L123, L124, L126–L128, L139 and L140.

The root cause is that the fold reads values from journal files by offset (source.rs:1-13). It does so to meet the 400 MiB memory budget (PERFORMANCE.md:110), which is met only with a single heap (PERFORMANCE.md:131). The result is that "a head whose value cannot be read" and "the book file holds the last copy of a save" become states the code must handle, through ledger rows R4, R6 and R15 and through `holds`.

**Steps or rows outside REACH** (build-lane.js:22):
- the model's `Damage` step, a byte change that keeps the file's size and time (model.rs:151);
- the model's `Flip` step, one changed byte in a journal file (model.rs:171);
- the model's `LoseTc` step (model.rs:177);
- the on-disk half of L83;
- the 999-name exhaustion half of L130;
- L134 (a git tree object passing the blob API);
- L137 (a damaged git object).

**Comparison with tC4.** tC4 reconciles out-of-band USFM edits only at open, and refuses the open on any other divergence (`UnexplainedDivergenceError` at tc4-tip journalingStore.ts:276; `reconcileDivergenceFacts` at :2152-2170).

## 9. Judgement

The core is justified by reachable scenarios: immutable changes, a total fold, visible conflicts, atomic writes, and the placement ledger for book files. Crashes, other programs editing a book file, and teammates' work arriving all need them.

Four parts go beyond what is reachable:

1. Every journal-file loss is handled as a value-precise recovery problem (19 of 40 rows).
2. metadata.json is a source of automatically recorded edits (12 of 40).
3. The history of carried files is made durable inside libgit2's object store, which sits outside the crash model (9 of 40, and about 11 mutation recipes, judging by their names in execute.py).
4. The tests include adversaries that REACH excludes.

The structure (a pure machine with fact producers and executors around it) is the right one. Its edges are not finished: the Writer's maps, journal files with no state machine, and six copy makers.

## 10. Simplification candidates

All of these should start after S2 merges. Changing the code before then invalidates the line-anchored mutation recipes (JOURNAL-MACHINE-ASSESSMENT.md). Every row count and cost below is an inference.

| # | Change | Removes | Touches | Rows / bug classes (inferred) | Risk | Cost after S2 | Owner? |
|---|---|---|---|---|---|---|---|
| C1 | Add checks: `disallowed-methods` for `std::fs` mutators in the storage crates (`#[allow]` with a reason at history, repack and git); make the raw `Vfs` mutators reachable only from atomic.rs (split the trait); move the mutation executor into the repo and run it nightly; a check that every §15 test name exists | Bypasses that today can land unnoticed; an untracked proof | AGENTS atomic writes, I6, §16.7 | Guards every class | Low | 1–2 days | No |
| C2 | One no-replace `preserve` helper replaces 5 of the 6 copy makers | Replace-capable copies (open.rs:252, packs.rs:198, temps.rs:274-282, scan.rs:171-178) | I6, AGENTS no-loss | L130 class; the assessment's temps and packs defects | Low | 1–2 days | No |
| C3 | Journal-file machine `journal_file::next`, sliced as in JOURNAL-MACHINE-ASSESSMENT | `Journal`'s three sets, `PackInfo`'s bools, about 17 ad hoc deletions | §4.4, §5.11 K4, §6.1 O2/O7, I10; owner robustness ruling of 2026-10-19 | L128 and the journal bugs of 2026-10-20; inferred H1 and (e) | Medium | 1–1.5 weeks for two slices, 2–3 weeks for all (the assessment's estimate) | No |
| C4 | Split `run_group`, `open_inner` and `build_tree` into one function per step; make `holds`, `shows_held`, `read_base` and `current_heads` pure, with their own property tests | Functions of 300–500 lines; facts tested only through the full system | none | Makes L108, L109, L113–L115, L126 and L140 testable in isolation | Low | 2–4 days, plus re-anchoring the recipes | No |
| C5 | Move Writer's per-path session maps into ledger states (e.g. `Recording{md5, copy, base}`), keeping only caches outside | 9 maps; the scope side effect at classify.rs:56-71 | §6.2 table, §11.1 D7/X4 | L107, L123, L125, L141–L143 | Medium–high | About 1 week, plus matrix and model | No |
| C6 | Keep values of pending files read at open in memory, not by offset | Unreadable-value cases within a session | §2.6, PERFORMANCE memory row | L110, L113, L126, L139 (in-session parts); simplifies the K1 "from memory" path | Medium: memory, since glibc's default heap already misses by 3–5 MiB | 3–5 days plus storebench | No; unit P measures |
| C7 | Drop the adversaries outside REACH: same-stat damage (R10, `HeldCause::Damage`, `Step::Damage`), `Flip` of local journal files, `LoseTc`, name-exhaustion tests, L134 | One table row and one hold cause; model steps | L83, L130, L134, L137, §6.2 R10 | 3–5 rows | A real bit flip would be recorded as an outside edit and synced to the team | Small | **Yes** |
| C8 | Outside edit of metadata.json: keep a copy, show a notice and offer "Take in", with no automatic recording | 4 Writer maps and the two-phase recording | I8 ("recorded as changes first"), §11.1 X2, L92 | About 8 rows' machinery (L112, L114, L118, L123, L125, L141–L143) | Low; another tool's edit needs one click | 3–5 days plus model | **Yes** |
| C9 | Keep superseded carried versions as content-addressed files under `.tc` (via `atomic_create`, inside FaultFs); git only for transport | One-object backup packs, fanout flushes, blob-kind checks | §9.8, §11.4, L73, L84 | L134–L136, L138, parts of L132 and L137 | Medium: disk use for audio | About 1 week, a format change before V | Inform the owner |
| C10 | Rejected: a per-installation mirror of pending files | — | — | — | It doubles each save's flushes; the save p95 is already 23.9–26.8 ms during a checkpoint against a 25 ms budget (STORAGE.md:4637) | — | — |
| C11 | Option: take carried files out of 1.0 | carried.rs and much of git.rs | PLAN goal 1 | 17 of 143 rows | Loses a declared addition | Large | **Yes**; recommend keeping, simplified via C9 |

**Order:**

1. Finish S2 unchanged: validate, then merge. `git merge-tree` against rbl shows 13 conflicted files: 5 under `.audit`, ci.yml, Cargo.lock, Cargo.toml, `tc-store/src/{handle,hooks,open}.rs` and `tests/store/{commands,drafts}.rs`.
2. Ask the owner about C7, C8 and C11, and tell them about C9.
3. Run unit **S3** before X, in this order: C1, C2, C3, C4, C5, C6, then whichever of C7, C8 and C9 the owner approves.
   - Each slice gets BEFORE/AFTER tests and passes the model's long seeds and the tracked mutation recipes.
   - C2 to C6 touch only private modules (carried, checkpoint, classify, outside, packs, temps and writer are plain `mod` in lib.rs).
   - The app layer reaches only `tc_store::{layout, vfs, atomic_write, atomic_create_dirs, atomic_remove, platform, atomic::temp_target, appdata}` (grep on rbl tc-app, tc-server, tc4). App units are therefore not disturbed.

## 11. An enforcement gap in the record

STORAGE §15 says each row is a named regression test.

- 49 of the 219 test names it cites do not exist as `fn` anywhere in the s2 crates.
- For 33 rows, none of the cited names exist (32 in L1–L103, 1 in L104–L143).
- Many of these belong to units X and I, which are not built yet.
- Some tests were renamed. For example, L54 cites `loss_salvage_damaged_pack_lines`, and `a_truncated_pack_salvages_every_whole_line` exists instead.

The S2 mutation proof (`execute.py` and its 44 recipes) lives in the git-ignored `/.enjoy-lane/`, so neither another developer nor CI can reproduce it.

## Findings
- [F1] (high/high) S2 has more than doubled tc-store, from 11,869 source lines (rbl) to 26,046 (s2), with 22,901 of them outside test modules. Since the merge base, storage src changed by +18,112 / −1,705 on s2 against +758 / −196 on main.
  - evidence: find crates/tc-store/src -name '*.rs' | xargs wc -l on both snapshots: s2 26046 (46 files), rbl 11869 (30 files); git diff --stat 7e62455 s2-proof-executor -- crates/tc-store/src crates/tc-journal/src: 58 files, +18112 -1705; git diff --stat 7e62455 resume-build-lane: 19 files, +758 -196; measure.py: tc-store has 22,901 lines outside test modules
  - rec: Add no new storage mechanisms before X. Run a simplification unit S3 after S2 merges.
- [F2] (medium/medium) The executors are very long. run_group (materialize.rs:698) is 506 lines, open_inner (open.rs:269) 473, and build_tree (checkpoint.rs:473) 316. tc-store has 17 functions of 100 lines or more, against 10 on main.
  - evidence: measure.py brace-matched lengths in s2 tc-store/src; run_group holds steps M0–M8, about 90 if/match/=> lines (awk on lines 698–1203); rbl: open_inner 330 lines, materialize 295 lines
  - rec: Split into one function per M or O step with a typed hand-off (C4). Do it after S2 merges, then re-anchor the mutation recipes.
- [F3] (high/high) ledger::next is a real pure state machine with one executor entry. Since it landed, though, bugs have kept landing in the fact producers and executors instead: of 10 product-fix commits after 0821958, 9 changed materialize.rs or classify.rs and only 2 changed ledger.rs.
  - evidence: ledger.rs:92-106 (State, Event), :247 (Row R1–R19), :454 (next); classify.rs:87 Writer::apply is the only caller of ledger::next; git show --numstat for 4810cf5, f5aa408, 5cb8bb8, 34098bb, 8e3eb1a, 85fa081, f5e1613, 2d4219d, a8e50e0, 3d9dfaa; classify.rs:56-71: set_state clears the scope digest on an md5 change (the L123 fix), outside next()
  - rec: Make holds, shows_held, read_base and current_heads pure, each with its own property test (C4). Move the scope rule into next().
- [F4] (high/high) A path's state is spread between the ledger State and the Writer's maps. Writer has 42 fields: 6 bools, 2 sets and 12 maps, 9 of which hold per-path session state.
  - evidence: writer.rs:157-262 (field count by awk: 42 pub fields, 12 maps); maps: busy, unrecorded_metadata, written_seq, reasons, pending_record, shown_by_path, bases, pending_carried, waiting; fix rows tied to these maps: L107, L125, L141–L143 (STORAGE §15.5)
  - rec: C5: model 'being recorded' and 'blocked' as ledger states, and keep only caches outside the ledger.
- [F5] (high/high) Journal files have no state machine. Their state lives in three sets plus PackInfo.valid and .damaged plus PendingInfo.status. On main, Journal had only two sets.
  - evidence: s2 project.rs:27-76; rbl project.rs:55-60 (packs, pending, unreadable, missing); JOURNAL-MACHINE-ASSESSMENT.md: about 17 deletion and overwrite sites in seal, open, temps, packs and writer
  - rec: C3: build journal_file::next in slices after S2 merges, as the assessment proposes.
- [F6] (medium/high) Six separate implementations make preservation copies, with different guarantees, and four of them can replace an existing file.
  - evidence: open.rs:236-254 restore_project_json: atomic_write into .tc/external; materialize.rs:154-190 copy_external: no-replace; recovery.rs:41 preserve_bytes: no-replace; temps.rs:265-283 keep_as_copy: probe then rename_held; scan.rs:167-199 move_out: probe then rename_held; packs.rs:189-202 write_back: atomic_write into .tc/quarantine
  - rec: C2: one no-replace preserve helper, plus a check that nothing else writes into .tc/external or .tc/quarantine.
- [F7] (medium/high) Project writes do not all go through one helper, and nothing enforces it. atomic.rs has 14 public helpers. The G4 .git rebuild and .git cleanups call std::fs directly, outside the Vfs and its crash model. Vfs mutators are public trait methods.
  - evidence: callsites.py: atomic_write* 22, atomic_remove* 11, *_held 11, atomic_create_dirs 30; history.rs:119,129 std::fs::create_dir_all and rename; repack.rs:597,612 and git.rs:1079 std::fs::remove_file; vfs.rs:92-103; rbl clippy.toml:7-13 already uses disallowed-methods
  - rec: C1: disallowed-methods for std::fs mutators with reasoned allows; restrict the raw Vfs writes to atomic.rs by splitting the trait.
- [F8] (low/high) Panic discipline holds and is enforced by lint: one proven panic! in tc-store library code and none in tc-journal.
  - evidence: handle.rs:85 panic! with a proving comment and #[allow(clippy::panic)]; json.rs:606-678 matches are the parser's own expect(byte) method; Cargo.toml:18-23 lint levels; ci.yml:26 -D warnings
  - rec: None.
- [F9] (high/high) The loss list grew from 96 to 143 rows between 2026-10-15 and 2026-10-21. S2's reviews added 40 rows (L104–L143) in about 2.5 days, in rounds of 22, 10 and 8, so the rate has not reached zero.
  - evidence: git show <c>:docs/STORAGE.md | grep -c '^| L[0-9]': 59c550b 96, 2daead2 102, 65aba0d 103, 85fa081 125, f5e1613 135, 3b075d7 143; STORAGE.md lines: 3,507 → 4,854 (65aba0d) → 5,517
  - rec: Give S2 an exit criterion: all recipes detected and 4 long seeds pass, then merge. Count any new row only when its precondition is in REACH.
- [F10] (high/high) S2 has been in fix loops since 2026-10-19 and is not yet verified at 3b075d7. At 7d3a5d3, 38 of 43 mutants were detected, long seeds 002–004 failed, and 3 of the 5 failing histories were oracle bugs.
  - evidence: TZ=UTC git log 36b1296..s2-proof-executor --no-merges: 18/46/3/6 commits on 10-18/19/20/21; f5e1613: 20 storage src files, +2,982 / −469; S2-PROOF-2026-10-21.md; HANDOFF.md S2 section ('Next, in order: (1) full validation')
  - rec: Make no design changes before the S2 merge. Track the oracle and the mutation executor in the repo.
- [F11] (high/medium) By my classification (inferred), the S2 rows come from outside-edit detection and the placement ledger (26, 12 of them metadata.json), carried files (9) and journal files alone (5). 19 of the 40 need another program to cut or delete a journal file first.
  - evidence: STORAGE.md §15.5 L104–L143, read row by row; whole list: merge 43, outside 35, journal 18, carried 17, sync 16, USB 4, atomic 4, other 3, local git 2, import 1
  - rec: Aim the simplifications at these three mechanisms (C3, C5, C6, C8, C9).
- [F12] (high/medium) The fold reads values from journal files by offset to meet the memory budget. That turns 'a value cannot be read' and 'the book file is the last copy of a save' into states the code must handle.
  - evidence: source.rs:1-13; FoldState.kept holds bytes only for command-added changes (fold/state.rs:405-408); PERFORMANCE.md:110 (400 MiB) and :131 (glibc's default heap misses by 3–5 MiB); ledger.rs R4/R6/R15; classify.rs:110 holds
  - rec: C6: keep the values of pending files read at open in memory; unit P measures the memory cost.
- [F13] (medium/medium) The model and STORAGE include adversaries outside the owner's REACH rule: same-stat disk damage, byte flips in local journal files, a lost .tc/, 999 copy names, and git tree objects passing the blob API.
  - evidence: model.rs:151-152 Damage, :171-176 Flip, :177-178 LoseTc; ledger.rs:81-87 HeldCause::Damage, R10; STORAGE L83, L130, L134, L137; build-lane.js:22 REACH text
  - rec: C7: owner decision to drop or demote them, after S2 merges.
- [F14] (medium/medium) Automatic recording of outside edits to metadata.json drives 12 S2 rows and 4 Writer maps. tC4 reconciles only USFM out-of-band edits at open and refuses the open on other divergence.
  - evidence: L112, L114, L118, L119, L122–L125, L130, L141–L143; writer.rs:174-237; tc4-tip journalingStore.ts:276-294 and :2152-2170
  - rec: C8: owner decision on 'keep a copy, show a notice, offer Take in' for metadata.json; amend I8.
- [F15] (medium/high) The S2 proof cannot be reproduced from the repo, and §15's claim that each row is a named test is not enforced. The mutation executor is git-ignored, and 49 of the 219 cited test names do not exist.
  - evidence: .gitignore: /.enjoy-lane/ (holds mutation-executor/execute.py); my script over §15 Test cells vs fn names in s2 crates: 49 missing, 33 rows with no existing name; example: L54 cites loss_salvage_damaged_pack_lines; a_truncated_pack_salvages_every_whole_line exists
  - rec: C1: a tracked xtask for the mutants run nightly, and a check that every §15 test name resolves or is marked as pending in unit X or I.

## Unverified
- Cost estimates (days and weeks) are inferences; C3's come from JOURNAL-MACHINE-ASSESSMENT.md, not from measurements.
- The mechanism classification of all 143 L-rows, and the counts of rows each candidate would eliminate, are my own reading of STORAGE §15. A second reader should re-tag the rows.
- Function lengths and test-region boundaries come from a heuristic brace-matching script (scratchpad/work/storage-health/measure.py), not from a Rust parser.
- I did not check whether clippy's disallowed-methods can target trait methods such as tc_store::vfs::Vfs::rename. Splitting the trait is the fallback.
- I did not measure C6's memory cost. Unit P's storebench memory case on the reference laptop would settle it.
- I spot-checked, but did not verify in full, whether the 49 missing §15 test names exist under other names. A name-resolution script plus manual mapping would settle it.
- I did not confirm that tC4 refuses the open for a metadata.json divergence specifically; I only saw the general UnexplainedDivergenceError and reconcileUsfm path. Check classifyDivergence's projected paths in tc4-tip src/data/journal/verify.ts.
- I did not count the mutation recipes exactly; 44 is HANDOFF's figure. A grep of execute.py's recipe table would give the count.
- No cargo build, test or clippy was run (forbidden here), so no claim about the current pass or fail state of S2 is mine.
- The merge-conflict count (13 files) is against resume-build-lane, not main; main fast-forwards to it.