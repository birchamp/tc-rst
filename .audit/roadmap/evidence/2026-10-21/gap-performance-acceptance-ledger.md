# Gap: performance-acceptance-ledger

## Summary
No PERFORMANCE.md budget row is met end to end on the reference laptop under rule 5. The only laptop evidence covers the fold part of some rows. It comes from storage-fold-windows-g0 at 58b6874, an old storage commit. The G0 aligner round trip was measured on the laptop, but against a stand-in server. Every other figure comes from SHA-capable Linux machines, from estimates, or not at all.

There are 34 rows on the lane and on main, not 33. s2 has 33 because commit 00e06ae, which adds the "Window on screen" row, is not on s2.

Six rows are missed, at least in part, even on the SHA CPU:
- checkpoint: p95 586-761 ms, against 500 ms
- save during a checkpoint
- the default-heap store memory
- suggestions: NT p95 95.9 ms
- the app's first aligner open: 36.7-40 ms
- the 10 s open after a reboot: one run at 10,038 ms

Two more rows, materialize one book and close, are estimate-only. Five store rows wait for an owner remedy. Those remedies are written only in the s2 and storage copies of PERFORMANCE.md (commit 2c2d172 is not on the lane). No rebuild.tsv row and neither HANDOFF owner list tracks them.

PLAN.md's P1-P10 never mention budgets: grep finds none on the lane or s2. G2's "done when" is CI at 3x, which cannot judge the laptop.

CI checks the 3x tolerance only in the lane's alignment-budget job and in storage's perf job. The release-only timing asserts never run in CI, because CI tests a debug build.

Unit P is unfinished. Its perfreport and the Windows track's Task 8 exist only on storage, and no storage-scale-*.md evidence doc exists in any ref. A dry-run merge of storage into the lane conflicts in 17 files.

Windows track: tasks 1-7 are done or superseded by the VM. Task 8 is open. Of the 3 Windows commits missing from the lane, 8fa3c99 and 460fa44 are still needed; 4887336 was redone as eec1feb and a33aef4.

# Performance acceptance ledger (read-only, 2026-10-21)

Abbreviations: **L** = lane snapshot (resume-build-lane f9067bc), **S2** = s2-proof-executor snapshot, **ST** = `git show storage:`. Evidence files are in `docs/evidence/`:

| Code | File | Machine | SHA? | Commit |
|---|---|---|---|---|
| FW | storage-fold-windows-g0-2026-10-17.md (L, S2) | reference laptop i5-8350U, Defender on | no (FW:43) | storage 58b6874 (FW:22) |
| SM | storage-materialize-2026-10-18.md (S2, ST only) | i7-12700 | yes (SM:129) | d4649c7/e0ed96d/7f58505 (SM:138) |
| SO | storage-open-save-2026-10-17.md | VM | yes (SO:35-40) | — |
| TS | translate-saves-2026-10-20.md | i7 | yes | b661bfb |
| CO | check-open-psalms.md | Xeon VM | not recorded | 99b2926 |
| AO | align-open-psalms.md | Xeon VM | not recorded | 0471d8e |
| AS | align-session-2026-10-19.md | i7 | yes | lane-b6 |
| ASG | align-save-suggestions-2026-10-20.md | i7 | yes | — |
| EG | electronite-spike-2026-10-17.md | laptop, stand-in server | no | 939596a |
| LX | lexicon-2026-10-16.md | Xeon VM | no | 4667e09 |
| FL | first-launch-seeding-2026-10-18.md | i7 | yes | cc9781a |
| UN | ui-no-loss-2026-10-18.md | i7, headless probe page | yes | — |
| MC | memory-compare-windows-2026-10-19.md | laptop; tC4 and the spike shell only | no | — |

**rows.rs** = ST `crates/tc-corpus/examples/perfreport/rows.rs`. The ST perf job is ST ci.yml:45-54 and runs the rows flagged ci=true.

**Owners** = PERF L:177-187.

Other short forms:
- JC = tc-journal/tests/operation_counts.rs
- SC = tc-store/tests/store/opcount.rs
- tc-app tests are under tc-app/tests/op_counts/.

## (a) Ledger: 34 rows on L and main, 33 on S2

The 34th row, L:116 "Window on screen", came in with 00e06ae. `merge-base --is-ancestor 00e06ae s2-proof-executor` returns 1, so it is not on s2.

| L line | Action / budget | Owner | Op-count test (L; S2) | Release evidence | Verdict | CI 3x | Owner remedy pending |
|---|---|---|---|---|---|---|---|
| 83 | Open a book: 150 ms, ch.1 50 ms | not named (rows.rs:49 says tc-journal) | L understand.rs:36 `open_book_touches_one_book`, JC:403; S2 JC:403 | FW:103-104: USFM 41-47 ms, chapter 0.29-0.34 ms (fold only) | met on laptop (fold part only) | L no; ST rows.rs:49-50 | no |
| 84 | Save a verse: 25 ms with fsync; 2 ms fold | tc-store, tc-journal | L JC:228, SC:135, translate.rs:119; S2 JC:228, SC:136 | FW:79,83,89: fold p95 ≤0.36 ms. TS:41-46: API p95 17.9-18.3 ms, direct store 26.5-26.7 ms. No laptop flush figure (ST P.md:83) | 2 ms met on laptop; 25 ms met only on SHA CPU | ST rows.rs:52,56 | no |
| 85 | Move chapter: 16 ms | not named | L session.rs:75, journey translate.test.mjs:77; S2 none | FW:89,104: p95 ≤0.90 ms | met on laptop (fold part only) | ST rows.rs:51 | no |
| 86 | Counts and conflicts after save: 1 ms incremental, 100 ms from scratch | tc-journal | L none (the APP-DESIGN:3167 names are absent); S2 none | FW:82 incremental 0.000 ms; FW:86 from scratch p95 24.7 ms | met on laptop (fold part) | ST rows.rs:53-54 | no |
| 87 | Check screen: 250 ms, 20 ms cached | tc-check | L check.rs:62,82,151 and tc-check performance.rs:52; S2 performance.rs:52 | CO:25-31 p95 ≤100 ms. Derivation only: "Placing saved decisions and computing status are not part of this measurement" (CO:14-15). Cached case: none | partial (non-reference VM) | L no*; ST rows.rs:99,102 | no |
| 88 | Rail filter/sort/group: 16 ms | tc-check | L tc-api rail.rs:73; journey check-keyboard.test.mjs:196 asserts only with TC4_PERF=1 (:227); S2 none | none | unmeasured | ST rows.rs:103 (library) | no |
| 89 | Record a decision: 25 ms; counts 1 ms | tc-check, tc-store | L check.rs:272,309, JC:289; S2 JC:289 | SO:29 p95 3.68 ms; FW:80 fold p95 0.04 ms | met only on SHA CPU | ST rows.rs:57,104 | no |
| 90 | Re-assess a verse: 1 ms | tc-check | L check.rs:366; S2 none | none | unmeasured | ST rows.rs:105 | no |
| 91 | Count invalidations: 2 s | tc-check | L plans.rs:230; S2 none | none | unmeasured | ST rows.rs:106 | no |
| 92 | Carry-over plan: 5 s, 300 ms per book | tc-check | L plans.rs:230; S2 none | none | unmeasured | ST rows.rs:107 (108 not in CI) | no |
| 93 | Open the aligner: 30 ms | tc-align | L align.rs:63; S2 none | AO:31-37 library p95 39-75 µs. AS:54 app first open 36.7-40.0 ms, "above 30 ms" (AS:65) | **missed** (app, SHA CPU) | ST rows.rs:110 | no (not raised) |
| 94 | Suggestions: 50 ms | tc-align | L align.rs:313; S2 none | ASG:44-47: NT p95 95.9 ms, OT 26.6 ms. "remains unmet" (ASG:63) | **missed** | **yes** L ci.yml:47-53 (150 ms); ST rows.rs:113 | not raised; ASG:63 leaves it to P/G2 |
| 95 | Aligner edit: inside 30 ms | tc-align | L align.rs:84, journey align.test.mjs:231; S2 none | EG:181-205 painted p95 22.0-25.5 ms on laptop with stand-in server. AS:18-22 real app p95 ≤20.6 ms | met on laptop (stand-in only); real app met only on SHA CPU | ST rows.rs:111 | no |
| 96 | Reflow: 2 ms | tc-align | L none (`reflow_touches_one_verse` absent); S2 none | AO:31-37 p95 ≤32 µs | met on VM only; no count test | ST rows.rs:112 | no |
| 97 | Book resources: 300 ms; 60 MiB | tc-resources | L understand.rs:157; S2 none | none end to end | unmeasured | ST rows.rs:119-120 (memory row 121 not in CI) | no |
| 98 | Lexicon: 100 ms load; under 1 ms lookup | tc-resources | L and S2 lexicon_performance.rs:98,135 | LX:33-57 worst load 27.2 ms (no-SHA VM) | met on a no-SHA VM; laptop unmeasured | L no*; ST rows.rs:122-123 | no |
| 99 | Open with cache: 3 s; progress in 300 ms | tc-store | L SC:83,112, progress.rs:44; S2 SC:84,113, progress.rs:44 | FW:54 fold only, p95 1,987 ms. SO:24 full open p95 2.06 s; SM:27 p95 ≤1.05 s | fold met on laptop; full open met only on SHA CPU | ST rows.rs:44,47 | no |
| 100 | Open without cache / after restart: 10 s | tc-store | same as row 99 | FW:55 warm p95 6,543 ms; FW:171 after reboot 10,038 ms, one run | **missed** in its one run; owner says "not shown met" (rebuild.tsv:94) | ST rows.rs:45-46 | measurement owed: ≥5 runs by P after S2 (FW:175-176) |
| 101 | Save during background work: 25 ms | tc-store | L none; S2 writer.rs:38,65,188 and groups.rs:361 (behaviour tests) | SM:21 during checkpoint p95 19.7-26.8 ms, 29.8-44.9 ms in 3 of 13 rounds, 190-244 ms when stalled | **missed / at the edge** on SHA CPU | ST rows.rs:72-74 | **yes**, S2 PERF:128 |
| 102 | Materialize one book: 1 s | tc-store | L none; S2 groups.rs:109,162,454 | SM:22 p95 231-333 ms; laptop estimate 0.5-1.3 s | estimate-only | ST rows.rs:60 | **yes**, S2 PERF:129 |
| 103 | Materialize many: background, progress | tc-store | L SC:507; S2 groups.rs:361 | SM:23 2.8-4.7 s, first progress 315-325 ms | met only on SHA CPU | ST rows.rs:65-66 | no |
| 104 | Close: 3 s | tc-store | none (ST SC:922 only) | SM:24 p95 1.0-1.4 s; laptop estimate 2.7-4.2 s | estimate-only | ST rows.rs:69 | **yes**, S2 PERF:130 |
| 105 | Checkpoint: 500 ms | tc-store | L none; S2 checkpoint.rs:428 | SM:25 p95 586-761 ms dirty, 316-525 ms settled; laptop estimate 0.8-2.4 s | **missed** (measured, not only estimated) | ST rows.rs:67 | **yes**, S2 PERF:127 |
| 106 | Daily sync: 3 s + network | tc-sync | none | estimate only (S2 STORAGE:4639) | blocked by unit X | no (rows.rs:128 not built) | no |
| 107 | USB export: 15 s | tc-sync | none | estimate only (S2 STORAGE:4641) | blocked by unit X | no | no |
| 108 | tC4 import: 2 min | tc-import | none | none | blocked by unit I | no | no |
| 109 | App memory: 1 GiB / 1.25 GiB | tc4, tc-app | none | MC:82: spike shell 83.3 MiB (a floor) | blocked by G1, G2 | no (rows.rs:131) | no |
| 110 | Store memory: 400/450 MiB | tc-store | none | SM:26 one heap 302-305 / 321-326 MiB; glibc heap 403-405 / 457-461 MiB | met only on SHA CPU with one heap; **missed** with the default heap | no (rows.rs:87-90 not in CI) | **yes**, S2 PERF:131 |
| 111 | Memory growth: 300 B per change | tc-store | none | S2 STORAGE:4642 "≈ 280 B" (estimate) | estimate-only | no | no |
| 112 | Long lists: 16 ms per frame | tc-ui, tc-app | none (virtual list at tc-ui/src/app/check.rs:11) | none | unmeasured | no | no |
| 113 | Typing to paint: 16 ms | tc-ui, tc-app | `typing_input_to_paint_p95` absent | UN:24-41 probe page: work p95 1.3 ms, next frame 15.6 ms | met only on SHA CPU, probe page | no | no |
| 114 | First launch: under 60 s | tc4, tc-resources | L tc-app tests/app/seed.rs:186; S2 none | FL:44 seeding 14.8 s median, 16.2 s worst | blocked by G1 and a signed installer (seeding met only on SHA CPU) | ST rows.rs:124 | **yes**: "The first measurement of the rebuild on Windows sets a tighter target" (L:133-135) |
| 115 | Repeat launch: no resource I/O; 3 s Home "proposed" | tc-resources, tc4 | L seed.rs:237, tc4/tests/seed.rs:245; S2 none | FL:45 `App::start` 4.3 ms | count met; 3 s unmeasured | count test runs in L ci.yml:37; ST rows.rs:125 | **yes**: the 3 s is "proposed" |
| 116 | Window on screen: 1 s | not named | none | none for the rebuild (MC:192 is tC4's) | blocked by G1 | no | no |

\* The timing asserts are gated on `!cfg!(debug_assertions)`, for example tc-check performance.rs:79-105 and lexicon_performance.rs:195. The only CI test step is the debug `cargo test` (L ci.yml:37), so they never run in CI. The only `--release` test in L CI is ci.yml:53.

**Totals:**
- On the laptop: 0 rows met end to end. 5 rows (83-86, 99) are met only for their fold part, and 1 row (95) only through G0's stand-in server.
- Missed, at least in part: 7 rows (93, 94, 100, 101, 105, the 25 ms part of 84 in the direct-store control, 110 with the default heap).
- Estimate-only: 3 rows (102, 104, 111).
- Blocked by an unbuilt unit: 7 rows (106-109, 114, 116, and 115's 3 s).
- Unmeasured: the rest.

## (b) Windows track

| Task | Status | Evidence |
|---|---|---|
| Setup | done | windows-machine-2026-10-17.md |
| 1 Build and test the workspace | done; the rerun is superseded by the VM | windows-build:235-245; rebuild.tsv:117 "tests 2,574 passed, 0 failed … the six laptop failures are fixed"; rebuild.tsv:118 "the VM runs all Windows builds and tests" |
| 2 Door43 facts | done (step 6 not run) | door43-live:3-6 |
| 3 Fold benchmark | done, as FW (FW:5-6 says it stands for task 3) | FW. Its numbers are for old storage 58b6874 and old memory budgets (FW:157) |
| 4 G0 spike | done | EG; STATUS L:41 |
| 5 Memory comparison | done (Paratext not measured) | MC; rebuild.tsv:121,125 |
| 6 Hazards | done (merged as 125dd41, an ancestor of the lane) | — |
| 7 Hazard 6 and rerun | hazard 6 done (FIX-proxy, STATUS L:88, rebuild.tsv:124); rerun superseded | windows-build:218-233 still lists the rerun as open, which is stale |
| 8 perfreport on the laptop | **open**, only on ST (afa8bc6) | Needs P to be finished on storage. It cites `docs/evidence/storage-scale-2026-10-19.md`, which does not exist in any ref (`git log --all` on the path is empty). No windows-perfreport branch exists |

The track file differs per branch: L has tasks 1-7 plus the roles note (L:7-12), S2 has tasks 1-6, and ST has tasks 1-8 with no roles note.

**Windows-related commits missing from the lane.** All other origin/windows-* branches, review/windows-push-refusal, g0-windows and door43-facts are ancestors of the lane (`--is-ancestor` exit 0).

| Commit | Branches | Change | Verdict |
|---|---|---|---|
| 8fa3c99 | windows-fixes, door43-push-refusal | Rewrites STORAGE §7.4 so a restore race keeps its choices; records that Defender has no exclusions | **Needed.** L STORAGE.md:2667-2673 still has the old rule. L windows-machine:28 still says "No Defender exclusion was added" (the Defender fact is already in FW:6-8 and EG:5-7). Not on L, S2 or ST (grep 'restore race lists' finds 0) |
| 460fa44 | same | Owner ruling of 2026-10-18 plus test `an_incomplete_restore_race_can_be_settled_before_the_held_batch_arrives` | **Needed.** The test is absent on L, S2 and ST, and no rebuild.tsv row records the ruling. Inference: C7a and C7b (conflict choices) depend on it |
| 4887336 | windows-hazards | Refuse names Windows renames; fold case | **Already redone** as eec1feb plus a33aef4 inside 125dd41 |
| 213a500 | door43-push-refusal | Push refusal | Already redone as 02300d9 (merge 1234a01) |

**What each machine still owes:**
- **Laptop** (measures only, L:7-12):
  - Task 8, the cold run after a reboot and the full run.
  - A flush figure: laptop-ratios.json says "disk has no ratio until the laptop runs `perfreport flush`".
  - The real-app aligner click (AS:77-78).
  - The rebuild's own startup and memory, once G1 and an installer exist.
- **VM:** gates on each new main (rebuild.tsv:117). Inference: the lane's 89 commits are not gated, because main has not moved. CI has no Windows runner (Q-26, L APP-DESIGN:3579; ci.yml uses ubuntu only).

## (c) Rows that can only be measured later

| Waits for | Rows |
|---|---|
| S2 on the lane | 101-105, 110 |
| P on storage | 99-100 again after S2 (FW:175-176; P continues after S2 merges, HANDOFF L:93-97); every laptop-unmeasured row, through Task 8 |
| X | 106, 107 |
| I | 108 |
| G1 (then G2, which waits for P per STATUS L:75) | 109, 112, 113, 116 on Windows and Electronite |
| A signed installer | 114 (the "signed, installed Windows build", L:114) and 115's 3 s |

Merge risk: `git merge-tree --write-tree resume-build-lane storage` conflicts in 17 files, including ci.yml, tc-align suggest engine/wordmap/boost (C5's rewrite), tc-check rail/change, tc-resources load and tc-store. A merge of s2 conflicts in 13.

## (d) Owner decisions needed (exact quotes)

1. **Budgets in "done".**
   - PLAN L:234-235: "The rebuild is done when every statement below is true and each one is shown by a check in CI that can fail." P1-P10 (L:237-300) have no performance item; grep for budget or PERFORMANCE in PLAN.md is empty on L and S2.
   - AGENTS.md:63-64: "meet the budgets in release builds".
   - rebuild.tsv:122: "speed is judged against tC4 doing the same work on the reference laptop … and against docs/PERFORMANCE.md's budgets".
   - G2's "done when" is "§8.2 green at the 3× tolerance" (APP-DESIGN:3426), which never yields a laptop verdict.
   - Decide: a P11, or a laptop clause in each owning unit's "done when".
2. **Checkpoint, 500 ms.** S2 PERF:127: "write one checkpoint's git objects into one pack (about 3 flushes in place of about 24) and accept libgit2's search for deltas between the new versions of the large files, or relax the row". ST P.md:674: "**do not edit the 500 ms row**". It is tied to Q-5 (L APP-DESIGN:3558). It has been open since BUDGET-REVIEW:110: "It should be reviewed next."
3. **The other pending rows (S2 PERF:128-131):**
   - Save during a checkpoint: "fewer flushes per checkpoint (the same remedy)".
   - Materialize one book: "a measurement on the laptop with Defender on".
   - Close: "a measurement on the laptop;".
   - Store memory: "Windows' heap is not glibc's; a measurement on the laptop decides".
   - None of these is in a rebuild.tsv row or in the HANDOFF owner lists (L:103-108, S2:145-150).
4. **Other open rows:**
   - First launch (L:133-135): "The first measurement of the rebuild on Windows sets a tighter target".
   - ST Task 8.4: "If `seed.first` is over 40 s … The owner decides whether the installer writes its files on several threads."
   - Repeat launch (L:115): "proposed: a usable Home in 3 s".
   - Suggestions (94) and the aligner's first open (93) are missed but have never been raised to the owner.

**Stale documents:**
- L APP-DESIGN:3183 and Q-15 (:3568) still say "350 MiB peak, 150 MiB steady"; PERF L:109-110 have newer limits.
- The §8.2 "PERF:nn" references point about 10 lines off.
- L STORAGE §14 still says the open "is not measured until `tc-store` exists".
- The lane's PERFORMANCE.md lacks the S2 open-rows table (2c2d172).

## Findings
- [PL-1] (critical/high) PLAN.md's definition of done (P1-P10) contains no performance or budget item, and nothing makes a reference-laptop verdict part of any unit's done.
  - evidence: L docs/PLAN.md:232-300 (P1-P10); `grep -n -i -E 'budget|PERFORMANCE|reference laptop|perf'` on L and S2 PLAN.md returns nothing; L AGENTS.md:63-64 'meet the budgets in release builds'; L APP-DESIGN.md:3426 G2 done-when '§8.2 green at the 3× tolerance'; L rebuild.tsv:122 'speed is judged against tC4 doing the same work on the reference laptop ... and against docs/PERFORMANCE.md's budgets'
  - rec: Ask the owner to choose a P11 (each budget row has a release measurement on the reference laptop plus a CI 3x check), or add a laptop clause to each owning unit's done-when (P, G2, X, I, G1).
- [PL-2] (medium/high) The budget table has 34 rows on the lane and main and 33 on s2. The extra row, 'App | Window on screen' (L PERFORMANCE.md:116), came with 00e06ae, which is not on s2.
  - evidence: awk count of Budgets rows: lane 34, s2 33, main 34, storage 34; `git merge-base --is-ancestor 00e06ae s2-proof-executor` exit 1
  - rec: Use 34 rows in the plan. Carry 00e06ae when s2 merges.
- [PL-3] (critical/high) No budget row has an end-to-end, rule-5 release measurement on the reference laptop. Laptop evidence covers only the fold part of rows 83-86 and 99-100 (storage commit 58b6874), G0's aligner round trip with a stand-in server, and tC4 and other programs.
  - evidence: FW (storage-fold-windows-g0-2026-10-17.md):13-14 'Only O8 of section 6.1 is measured, so the 3 s cold open is not shown end to end'; FW:22 storage commit 58b6874; EG (electronite-spike-2026-10-17.md):10-12 'The server is a stand-in'; MC:328-336 'The rebuild's own page' not measured; `git log --all -- docs/evidence/storage-scale-windows* docs/evidence/app-budgets*` is empty
  - rec: Make Windows-track Task 8 (perfreport on the laptop) plus a G2 laptop run the acceptance evidence. Merge Task 8 to the main line.
- [PL-4] (high/high) The checkpoint 500 ms row is missed by measurement even on the SHA i7-12700 (Psalms dirty p95 586-761 ms), not only by the laptop estimate. Its remedy waits for the owner, but no rebuild.tsv row records the question and neither HANDOFF lists it.
  - evidence: S2 docs/evidence/storage-materialize-2026-10-18.md:25; S2 PERFORMANCE.md:127 (remedy text); ST .audit/plans/P.md:663-675 F4 'do not edit the 500 ms row'; grep of rebuild.tsv on L, S2 and ST finds no checkpoint escalation; L HANDOFF.md:103-108 and S2 HANDOFF.md:145-150 omit it; L BUDGET-REVIEW-2026-10-17.md:110 'It should be reviewed next.'
  - rec: Put the checkpoint question, with Q-5/AD34, to the owner now, using P.md 13.4's options. Add it to HANDOFF 'What needs the owner'.
- [PL-5] (high/high) Five store rows (checkpoint, save during a checkpoint, materialize one book, close, store memory) are marked open for an owner remedy only in the s2 and storage copies of PERFORMANCE.md (2c2d172). The lane's PERFORMANCE.md and STORAGE §14 lack S2's numbers.
  - evidence: S2 PERFORMANCE.md:117-136; `git merge-base --is-ancestor 2c2d172 resume-build-lane` exit 1; L STORAGE.md section 14 (from :4114) lacks S2 rows and says the open is 'not measured until `tc-store` exists'
  - rec: When S2 merges into main, carry PERFORMANCE 'Rows of the project store that are open' and the §14 figures. Track each row's owner decision.
- [PL-6] (high/high) Two app-level rows are missed on the lane even on a SHA CPU and were never raised to the owner: suggestions (NT p95 95.9 ms against 50 ms) and the aligner's first open in a session (36.7-40.0 ms against 30 ms).
  - evidence: L align-save-suggestions-2026-10-20.md:44-47,63 'The 50 ms reference target remains unmet'; L align-session-2026-10-19.md:54,63-65; no rebuild.tsv ruling or HANDOFF owner item mentions them
  - rec: Profile and fix under rule 6, or raise them to the owner with evidence. Assign them to P (library) and G2 (app).
- [PL-7] (high/high) CI checks the 3x tolerance only in the lane's alignment-budget job (one row) and in storage's perf job. The release-only timing asserts in tc-check, tc-align, tc-resources and tc-usfm never run in CI, because CI tests a debug build.
  - evidence: L ci.yml:47-53 (alignment-budget) and :37 (debug `cargo test --workspace`); grep '--release' finds only L ci.yml:53 and ST ci.yml:53; ST ci.yml:45-54 perf job; ST rows.rs:42-132 ci flags; L crates/tc-check/tests/performance.rs:79-105 asserts only `if !cfg!(debug_assertions)`; lexicon_performance.rs:195 the same
  - rec: Land storage's perf job (perfreport ci) on main, or add a release `cargo test` job for the timing tests. Add app rows through G2's appbench (absent today).
- [PL-8] (high/high) Unit P is unfinished on storage. perfreport and Task 8 exist only there, and the final storage-scale evidence doc was never written. A dry-run merge of storage into the lane conflicts in 17 files, including tc-align suggest code that C5 rewrote.
  - evidence: L STATUS.md:18 'P | scale against the reference laptop | to do'; `git branch -a --contains afa8bc6` lists only storage; `git ls-tree storage docs/evidence/storage-scale-2026-10-19` has only laptop-ratios.json and machine.txt; L HANDOFF.md:93-97 (merge s2 into storage, then continue P); `git merge-tree --write-tree --name-only resume-build-lane storage` exit 1, 17 conflicted paths
  - rec: Plan P's continuation after S2 with an explicit conflict-resolution step for tc-align suggest, tc-check rail, tc-resources load, tc-store and ci.yml. Fix Task 8's reference to the missing storage-scale-2026-10-19.md.
- [PL-9] (medium/high) Windows track: tasks 1-7 are done or superseded by the VM, and Task 8 is open. Of the commits missing from the lane, 8fa3c99 and 460fa44 (restore-race ruling, STORAGE §7.4, a test) are still needed and exist on no main-line branch. 4887336 was redone in 125dd41.
  - evidence: `git cherry resume-build-lane origin/windows-fixes`: + 8fa3c99, + 460fa44; `origin/windows-hazards`: + 4887336; L STORAGE.md:2667-2673 still has the old §7.4 rule; grep 'restore race lists' finds 0 on L, S2 and main; test name absent on L and S2; `git merge-base --is-ancestor 125dd41 resume-build-lane` exit 0 (includes eec1feb, a33aef4); rebuild.tsv:117-118 (VM supersedes laptop builds)
  - rec: Cherry-pick 8fa3c99 and 460fa44 (or redo them) before C7a/C7b. Mark Task 1/7 reruns as superseded in windows-build-2026-10-17.md:218-233.
- [PL-10] (medium/high) The laptop still owes Task 8: a cold run after a reboot (no-cache open at least 5 times, first Psalms resource load, seeding), the full run, and a flush figure. It also owes the real-app aligner click, and later the rebuild's own launch and memory once G1 and an installer exist. CI has no Windows runner.
  - evidence: ST windows-track.md:177-202 (Task 8); ST laptop-ratios.json method: 'disk has no ratio until the laptop runs `perfreport flush`'; L align-session-2026-10-19.md:77-78; rebuild.tsv:94 (at least 5 runs); L APP-DESIGN.md:3579 Q-26; L ci.yml runs-on ubuntu-latest only
  - rec: Write a single laptop work order (Task 8 plus a G2 run) gated on P and G1. Give G1 and G2 a Windows runner or the VM.
- [PL-11] (medium/high) The 10 s open after a restart without a cache measured 10,038 ms in a single laptop run. The owner ruled it 'not shown met' and asked unit P to measure it at least five times after S2.
  - evidence: L FW:170-176; L rebuild.tsv:94
  - rec: Make it a named Task 8 cold case with at least 5 runs after S2 and P land.
- [PL-12] (medium/high) APP-DESIGN §8.2 names count tests that do not exist on the lane, and its memory row and Q-15 still cite the superseded 350/150 MiB budgets.
  - evidence: grep on L crates/ and parity/ is empty for drafted_counts_update_from_delta, conflict_counts_update_from_delta, lexicon_lookup_reads_nothing, reflow_touches_one_verse, suggest_uses_trained_model_without_training, rail_filter_sends_no_request, typing_input_to_paint_p95; L APP-DESIGN.md:3167-3183, :3568; L PERFORMANCE.md:109-110
  - rec: Add the missing count tests, or rename §8.2 entries to the tests that exist (for example tc-api rail.rs:73). Update the memory rows to 1 GiB/1.25 GiB for the app and 400/450 MiB for the store.
- [PL-13] (low/high) The Owners table names no owner for 'Open a book', 'Move to another chapter' and 'Window on screen'.
  - evidence: L PERFORMANCE.md:177-187; ST rows.rs:49-51 assigns the first two to tc-journal; no owner for L:116
  - rec: Add owners: tc-journal plus tc-app for the editor rows, and tc4 (G1) for the window.

## Unverified
- Whether storage's `perf` CI job ever ran green on GitHub within its 90-minute limit. Check with `gh run list --branch storage --workflow ci.yml`, which I did not run.
- Whether the Xeon VM used for check-open-psalms.md (99b2926) and align-open-psalms.md (0471d8e) had SHA instructions. The files do not say. lexicon-2026-10-16.md:33-34 says a similar VM did not. Check with `git show 99b2926` or the container records of 2026-10-16.
- Whether the Windows VM gate has run on any of the lane's 89 commits. HANDOFF says it runs on each new main, and main has not moved. Check with `journalctl --user -u 'tc-wingate-*'` on the Linux PC.
- Whether the cfg(windows) test a_real_hold_is_waited_out_while_a_scanner_is_simulated ran in the VM gate. Inferred yes from rebuild.tsv:117 (2,574 passed at dc5fcd8), but its name was not checked in the logs.
- The laptop session's current state ('Part 3 complete'). I did not message it (read-only task).
- Whether a perfreport run's raw output exists outside the repository, for example under /var/tmp target folders. I only checked the git refs.
- Whether the owner gave rulings outside rebuild.tsv (chat) on the checkpoint, suggestions, or the restore race (460fa44's message cites an owner ruling of 2026-10-18 that no rebuild.tsv row records).
- The task brief's ST-3, ST-10 and ST-14 identifiers come from other evaluators and could not be checked here.
