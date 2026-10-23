# process-integration

## Summary
Getting everything onto main is doable, but the tracking cannot be trusted as it stands. Main (af4eef3) fast-forwards to resume-build-lane (f9067bc) with no conflicts. That makes 38 of 48 app units, by status.py's own rule run read-only on each ref. S2 conflicts with resume-build-lane in 13 files. Five are tracking files that both branches created. Five are real tc-store conflicts: open.rs (414 conflict lines), tests/store/drafts.rs (485), commands.rs (210), handle.rs and hooks.rs. Cargo.toml also differs on whether git2 gets TLS (https). The storage branch (unit P, partial) conflicts in 17 files. That includes duplicate speed-ups of the aligner's suggestions: C5's on the app side and P's on storage. Recommended order: (1) one full gate plus the Windows VM gate on f9067bc, then fast-forward main. (2) Merge S2 into main. (3) Merge main into storage and finish P there, then merge storage. Outside those branches: lane-a8 holds a sign-in cancel fix that the translator would see and that is on no integration branch. quick-timing holds 54 uncommitted files that no tracking file mentions. Nothing on the three newer branches is pushed (89, 77 and 246 commits), so GitHub CI has never run on them. The root disk is 97% full: 815G of it is old build folders in /var/tmp/tc-rst-target, moved there by the watcher. Tracking has split across branches and gone stale: four different unit counts. Both committed STATUS files are wrong (main says 24, the true count is 29; the resume lane says 37, the true count is 38). S2's HANDOFF points to files its branch lacks. Recommended fix: one hand-edited file per unit, one new file per event, minors split per unit, rulings one per file, merge-commit trailers, and a generator that reads all branches and whose STATUS output is never committed.

# Process, tracking and integration state (read-only, 2026-10-21 12:55 EDT)

## 0. Snapshot
| Branch | Tip | Last commit | Pushed? |
|---|---|---|---|
| main `claude/focused-ride-5uzj7p` | af4eef3 | 10-20 09:00 | yes (origin = af4eef3, 0/0) |
| `resume-build-lane` (RL) | f9067bc | 10-21 12:16 | no (89 ahead) |
| `s2-proof-executor` (S2) | 3b075d7 | 10-21 12:24 | no (77 ahead) |
| `storage` | dd63328 | 10-19 05:39 | no: origin/storage = 0d7d4f7, local 246 ahead |

- **Single-copy risk (inference):** RL, S2 and storage exist only on this PC. `ci.yml` runs `on: push`, so none of these commits has ever been through GitHub CI.
- **Sessions look idle:** `ps` at 12:55 showed no cargo, rustc, node or watch-runs process. The newest logs are from 12:21 (RL `.enjoy-logs/c4-check-tcapp.log`) and 12:24 (S2 `s2-merge-proof-tests.log`). The claude-desktop processes had been up only 29 minutes. Whether the two sessions are still "running" is unverified.

## 1. Integration debt

### 1.1 Pairs (from `git rev-list --left-right --count` and `git merge-tree --write-tree --name-only`)
| A | B | only in A | only in B | base | conflicted files |
|---|---|---|---|---|---|
| main | RL | 0 | 89 | af4eef3 | 0 (fast-forward) |
| main | S2 | 541 | 77 | 7e62455 | 12 |
| main | storage | 351 | 64 | c1bfa78 | 12 |
| RL | S2 | 630 | 77 | 7e62455 | 13 |
| RL | storage | 440 | 64 | c1bfa78 | 17 |
| S2 | storage | 37 | 214 | 19e7fd2 | 7 |

Conflict hunks are counted from the merge-tree result trees (`git cat-file -p <tree>:<path>`, counting `<<<<<<<` markers):
- **RL × S2** (tree cfe7568):
  - `.audit/` files that both branches added (add/add): HANDOFF.md (6 hunks), minors.tsv (1 hunk, 94 lines), tools/README.md (2), workflows/build-lane.js (8 hunks, 240 lines), workflows/merge-main.js (1).
  - Workspace files: `.github/workflows/ci.yml`, Cargo.toml (2 hunks), Cargo.lock (5).
  - tc-store: src/handle.rs (1), src/hooks.rs (1), src/open.rs (3 hunks, 414 lines), tests/store/commands.rs (210 lines), tests/store/drafts.rs (485 lines).
  - Main × S2 is the same list without hooks.rs.
  - **Decision inside Cargo.toml:** RL keeps git2 "without TLS… a crate that reaches Door43 adds https". S2 sets `features = ["https", "vendored-libgit2"]` and adds `notify = "8.2.0"`. AGENTS.md allows network access only in tc-door43 and tc-resources.
- **RL × storage** (tree 9a560b2):
  - Workspace files: ci.yml, Cargo.lock, Cargo.toml, tc-align/Cargo.toml.
  - **Duplicate speed-ups of the aligner's suggestions:** `tc-align/src/suggest/{boost (225 lines), engine (62), wordmap (142)}.rs`. RL's change is d45be90 (C5, "flat-tree batching"). Storage's is fbef0dc ("suggestions on the worker pool"). Two independent fixes of the same speed problem.
  - tc-check/src/{change (104), rail (96)}.rs, tc-resources/src/load.rs (43).
  - tc-store/src/{actor, handle, hooks, open (132), store}.rs, tests/store/{commands (210), opcount (504)}.rs.
- **S2 × storage** (tree c0483eb): .audit/HANDOFF.md (288 lines), merge-main.js, ci.yml, tc-store/src/seal.rs (3 hunks), store.rs, tests/store.rs, docs/STORAGE.md.

### 1.2 Recommended merge order and the gates for each step
- **M0. Freeze.** Each owner stops at a stage boundary and commits its loose state:
  - The RL worktree has 2 uncommitted B11 rows in `.audit/minors.tsv` (live file differs from f9067bc).
  - Fold `.audit/b5b-minors-pending.tsv` (5 rows) into the minors list.
- **M1. Fast-forward main to RL f9067bc.** No conflicts, but a full gate is still owed:
  - The merges after C3 had partial gates only. B8: 1,520 tests in 3 crates. C4: 1,958 tests in 5 crates. B11: "2125 passed" plus some tc-app binaries (RESUME-2026-10-20.md B8/C4 checkpoints; B11.md:80-82).
  - **Gates on f9067bc:**
    - `cargo fmt --all --check`
    - `cargo clippy --workspace --all-targets -- -D warnings`
    - `cargo test --workspace`
    - tc-ui wasm clippy with no features, `gallery`, `app` and `test-hooks`
    - tc-api wasm clippy
    - `cargo clippy -p tc-app --lib -- -D warnings`
    - `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
    - the windows-gnu workspace clippy (ci.yml:31-36)
    - the `node --test` parity and journeys line (ci.yml:72)
    - `alignment-budget` (ci.yml:53)
    - Windows VM `gates.ps1`. The newest `refs/winvm/*` is from 2026-10-19, so no VM gate has run on main's 10-20 merges, RL or S2.
  - **Owner decision first:** pushing main.
- **M2. Merge S2 into main** in a separate worktree, after S2's own remaining steps (S2 HANDOFF.md:29-33: validation at the merged SHA, 44 mutation recipes, 4 long seeds, re-review).
  - Who resolves: the S2 owner resolves tc-store. RL's author reviews `drafts.rs` and `commands.rs` (C1b-C5 draft logic).
  - Gates: all of M1, plus:
    - the long model test (`cargo test -p tc-store --test store model::random_histories_lose_no_work_long -- --ignored --exact`)
    - crash-injection and Burrito tests
    - op-count tests at Psalms and 10× scale
    - storebench release
    - the save journeys (translate, section, check-writes, story-writes, align)
    - the VM gate
  - Why S2 before storage: storage is cut from S2's base 19e7fd2 and overlaps S2 in only 7 files. P's speed work should be redone on S2's final store, not the other way round.
- **M3. Do not merge `storage` into main as it is.** P is a partial unit. Merge main (after M2) into `storage` on the P lane instead:
  - Resolve S2 × storage (7 files) and RL × storage (17 files).
  - Keep one suggestion implementation, proved by the bitwise oracle, the 19 tC4 fixtures and `suggestion_budget`.
  - Finish P, then merge storage into main with the M1 gates, storage's new `perf` CI job and the reference-laptop rows.
- **M4. Branches outside the integration line:**
  - `lane-a8` has 4 commits after A8's merge (32eaebd). Among them is 58aa8a9, "A cancel once the sign-in's token calls started signs out, as tC4's does". Only 2 of its 17 added `account.rs` lines exist on RL. It conflicts with RL in 6 parity files. Cherry-pick 58aa8a9 and 599e516 after M1 and capture the parity files again, or drop them by ruling.
  - `lane-c1a`: 1 commit that only changes a plan document, merges cleanly. Merge it or drop it.
  - `rebuild-process-improvements` (2fe47e2) is S2's 1ec5bad except 2 README lines and 4 lines of run-gates.mjs. Drop it after M2.
  - origin/door43-push-refusal: 3 commits not in RL by patch-id. Check them against the merged review/windows-push-refusal.

## 2. Worktrees and disk
`git worktree list` shows 46 entries (main checkout plus 45).
- **Owned by the running sessions; leave alone:**
  - `.worktrees/resume-build-lane`
  - `.worktrees/resume-build-lane/target/lanes/b11` (rl-b11, already in RL)
  - `.worktrees/s2-proof-executor`, `s2-fix-git`, `s2-fix-meta`, `s2-fix-proof` (all three are ancestors of S2)
  - `s2-review-exec` (detached f5e1613), `s2-commit-check` (detached 1ec5bad), `s2-proof-handoff` (= af4eef3)
  - the directory `.worktrees/s2-review-exec-evidence`, which is not a worktree; S2's HANDOFF cites it
- **Unique work; keep:**
  - `.claude/worktrees/storage` (P)
  - `lane-a8` (4 commits not in RL)
  - `lane-c1a` (1 commit not in RL)
  - `.worktrees/rebuild-process-improvements` (1 commit not in RL)
  - `s2-verify` (85fa081, an ancestor of S2, but RL HANDOFF.md:94-96 still names it as S2's home)
- **Uncommitted work; inspect before any removal** (checked with `git --no-optional-locks status`):
  - `quick-timing`: 54 modified files plus 1 untracked, +982/-600, dated 10-19 18:52. No tracking file mentions it.
  - `lane-b7`: 3 files.
  - `lane-merge` (detached at a896279, the old B5b merge): 2 files.
- **Safe to remove later** (branch in main or RL, clean worktree), 28 entries: app-b2, app-c1a, app-flife, app-ftext, app-int, lane-b10, lane-b3, lane-b4a, lane-b4b\*, lane-b5a, lane-b5b\*, lane-b6, lane-b9a, lane-b9b, lane-c1b, lane-c5, lane-c6, lane-c8, lane-falign, lane-flife, lane-fproxy, lane-winfix, merge-main, p0-a8, phase0, review-windows-push-refusal, tc4fix-int, status. (\* only after M1.)
- **Disk:**
  - Root drive: 905G, 97% used, 27G free. `/var/tmp/tc-rst-target` holds 815G. The biggest folders: lane-merge 92G, lane-c1b 69G, lane-b7 63G, lane-b5a 52G, lane-c8 49G, merge-main 47G. `lane-fproxy-gnu` and `merge-main-windows-gnu` have no worktree.
  - /home: 916G, 76% used, 213G free. RL's target is 153G. Merged worktrees' own targets add up to about 255G (app-b2 61G, app-ftext 51G, p0-a8 44G, app-c1a 32G, phase0 31G, tc4fix-int 18G, review-windows-push-refusal 14G, app-flife 3.7G).
  - Running sessions use no /var/tmp target.
  - **Cause:** `watch-runs.py` `relocate_idle` sends targets to `/var/tmp` when /home falls below 100G free. It only ever checks `statvfs('/home')`, never root, and it only globs `.claude/worktrees/*`.
  - The rl-b11 worktree lives inside a `target/` folder (RL HANDOFF-resume-lane.md:9), so a `cargo clean` there would delete it.
  - /tmp (31G tmpfs) hit its quota during this audit, and the tool shell failed for a few minutes.

## 3. Tracking state across branches
| File | main | RL | S2 | storage |
|---|---|---|---|---|
| STATUS.md | "24 of 48", generated at fa46807 (10-19 19:13) | "37 of 48", generated at 3eac396; B11 shown as "ready to start" | absent | absent |
| Real count (status.py's rule, my read-only copy) | 29 | 38 | n/a | n/a |
| PROGRESS.md | 75 lines | 84 lines; says "34/48" | absent | 71 lines (older) |
| HANDOFF.md | 109 lines | 117 lines | 159 lines, with an S2 section on top | 126 lines (the 10-18 move to Linux) |
| minors.tsv | 60 rows | 70 rows (+2 uncommitted) | 21 rows, S2 only (add/add) | absent |
| rebuild.tsv | 135 lines, same as RL | 135; last line 10-20T11:30Z | 77 | 103 |

- **Six different counts or "where it stands" claims** for app units:
  - STATUS: 24 on main, 37 on RL
  - PROGRESS: "29/48" on main, "34/48" on RL
  - RESUME-2026-10-20.md: 35, 36 and 37
  - plans/HANDOFF-resume-lane.md:20: 38
- **Three different restart recipes:**
  - PROGRESS.md:81-84: `app-phase0.js` / `app-layer.js`
  - STATUS: `build-lane.js`
  - plans/HANDOFF-resume-lane.md: `resume-lane.js` plus its args JSON
- **Pointers that are stale or contradict each other:**
  - S2 HANDOFF.md:44-49 says to read STATUS.md and PROGRESS.md, which do not exist on that branch.
  - S2's tools/README lists status.py, watch-runs.py and winvm, none of which are on that branch.
  - RL HANDOFF.md:94 says S2 lives on s2-verify.
  - The same S2 HANDOFF says "pushing main needs the owner's say-so" (line 33) and "push main (standing authorization)" (line 75). RL plans/HANDOFF-resume-lane.md:7 says "Do not push without the owner".
- **The watcher is not running.** There is no `~/.cache/tc-watch` and no matching process. Main's last "Regenerate the status list" commit is cbdcdac (10-19 19:16). B5a, B4a, C6, C8 and B6 merged after it.
- **status.py is fragile** (`.audit/tools/status.py`):
  - It decides "done" from the subject regex `(?:Merge|Build) <ID>[:\s]` (lines 50-54) plus a hardcoded `BASE_DONE` list (lines 16-18). 8 app units are in `BASE_DONE` only.
  - S2's merges are named "Merge the … fix round", so S2 will not count as done unless its merge subject is "Merge S2:".
  - It writes a committed file. That caused 6 merges whose only purpose was to carry the regenerated status list (f5efad1, fa46807, a3299c3, d250a89, 0266753, c6433e3).
  - 23 of RL's 74 non-merge commits since main touch only `.audit/`.
- **S2 keeps its stage state outside git.** `lane-state.mjs` and `run-gates.mjs` keep reports in ignored `.enjoy-lane/` and `.enjoy-logs/` folders inside the worktree. Their identity includes a digest of every environment variable (lane-state.mjs:56, run-gates.mjs `environment`). They are local restart hints that are lost with the worktree. RL has not adopted them, so the "shared heavy-run lock" (PROCESS.md:60-63) does not cover RL's `.enjoy-logs/resume-full-gates.sh`.
- **Can a new agent find the truth quickly? No.** It must reconcile about 10 sources across 4 branches plus local ignored folders. What breaks on merge: 5 add/add `.audit` conflicts (RL × S2), STATUS that is stale on both sides, and two orchestration toolsets (RL's build-lane.js v1 plus resume-lane.js against S2's build-lane.js v2 plus lane-state/run-gates).

## 4. Velocity and failure modes
First commit unique to the merged branch, to the merge commit (wall clock; the builder's start is earlier than the first commit; my script's output):

| Period | Units (hours) |
|---|---|
| Phase 0 (10-18) | A4a 5.7, A6b 1.7, B1a 1.6, A4b+A7 9.7, B1b 6.8 |
| Build lane (10-19) | A5 5.7, C0 5.0, FIX-misc 3.1, A8 2.0, C1a 2.8, FIX-proxy 0.6, FIX-text 5.7, B2 4.1, FIX-align 1.0, B9b 1.4, B3 2.7, C1b 5.4, B10 1.5, B9a 1.1, B5a 2.9, B4a 4.1, C6 2.3, C8 3.3. Median about 2.8 h |
| Across the 10-20 usage limit | B6 10.9, B5b 12.5, B4b 11.8, B7 13.5 |
| Resume lane | B8 9.4, C4 18.2 (+0.3 for a follow-up fix), B11 15.6 |
| Not measurable (one squashed `Build` commit each) | C5 d45be90, C2 b661bfb, C3 3067ce7 |

**S2:**
- First S2 commit 33f7809 (10-18 12:21Z). As of 10-21 16:24Z: about 76 h, 40 + 34 first-parent commits, not merged.
- The placement-ledger redesign has run about 48 h (8692f61, 10-19 16:03Z onwards).
- tc-store diff since base: S2 +34,500/-1,772 lines across 64 files; RL +1,484/-211.

**What slowed the work:**
1. **Usage limits:**
   - rebuild.tsv lines 19, 30, 37, 49 and 134 (C8 merged by hand after the weekly limit).
   - plans/HANDOFF-resume-lane.md:15: "every few hours".
   - C4.md:43-45 and B11.md:69 record a stop on 10-21.
   - Container restarts: lines 50 and 53.
2. **State lost between sessions:**
   - Workflow caches do not survive a new session (HANDOFF).
   - One session had no JS Workflow runner at all (RESUME-2026-10-20.md:7).
   - An agent wiped the scratch folder (line 62).
3. **No-loss review loops:**
   - S2's verify regression (line 101), the redesign rulings (lines 115-116), 9 serious review findings, and 3 of 4 long-model seeds failing (S2-PROOF-2026-10-21.md).
   - C1b's 7 blockers and 11 majors forced a restart (line 126).
4. **Half merges and duplicates:**
   - a896279 / lane-merge.
   - A message to a running agent started a duplicate copy (line 98).
5. **Disk:** low-disk events at lines 27, 49, 68, 74 and 132; root is now at 97%.
6. **Load:** op-count tests fail under heavy load (B8 and C4 checkpoints), and the heavy-run lock is not shared between sessions.
7. **Tracking churn:** see §3.
8. **Owner pause:** 10-20 20:20 to about 01:00.

## 5. Recommended tracking design
**Layout under `.audit/`:**
- `units/<ID>.md`
  - The only hand-edited per-unit state. Front matter: `id, owner, branch, worktree, state (todo|building|verifying|fixing|gating|merging|blocked|paused), next, blockers[], owner_question, updated`.
  - Only the branch that claims the unit edits it, so files never collide.
  - There is no "done" value; the generator derives it.
- `events/YYYY-MM-DD/HHMMSSZ-<ID>-<kind>.json`
  - Append-only and never edited. Fields: `ts, unit, kind (start|build|verify|fix|gate|merge|interrupt|resume|ruling|ops|handoff), branch, sha(40), result (pass|fail|partial|na), summary ≤200 chars, evidence[], next`.
  - A new file per event, so branches never conflict.
- `minors/<ID>.tsv`: columns `id, severity, harm, finding, evidence, status, sha`, one file per unit. Split the current `minors.tsv`, S2's 21 rows and `b5b-minors-pending.tsv` into these.
- `rulings/YYYY-MM-DD-<slug>.md`: one owner ruling per file. Migrate rebuild.tsv's 13 `ruling` lines.
- `HANDOFF.md`: machine, VM, laptop and tools only, at most about 80 lines, with no unit state.
- Frozen with a "history" header: `rebuild.tsv`, `PROGRESS.md`, `plans/RESUME-*`.
- `STATUS.md` becomes generated and gitignored, never committed.

**Truth from git, not from hand-written notes:**
- Merge commits use the subject `Merge <ID>: …` plus a `Unit: <ID>` trailer. Squashed builds use `Build <ID>:` plus the same trailer.
- **Generator** (status.py v2, `--all`):
  - Units come from APP-DESIGN §10 and the storage list.
  - For each branch (main, `rl-*`, `lane-*`, `s2-*`, `storage`), the units whose trailer is reachable (`git log --format=%(trailers:key=Unit,valueonly)`).
  - The latest event per unit across branches, read with `git ls-tree` and `git show`, without merging.
  - Claims; `git worktree list --porcelain` with uncommitted counts; unpushed counts; free space on `/` and `/home`.
  - Output sections: Done on main / Done elsewhere / In flight / Ready / Waiting / **Contradictions**. Examples: a claim says "building" but the unit is merged; a gate event's SHA is not the branch tip; a worktree has uncommitted files and no claim.
- **Check:** a CI job `node --test .audit/tools/*.test.mjs` plus `status.py --check`. It validates the event and claim schemas and fails when a merged unit has no trailer or merge event. It names the broken rule.

**Who writes what:**
- Agents return JSON and never touch `.audit/`.
- The orchestrator or integrator writes claims and events at each stage boundary through `node .audit/tools/event.mjs add --unit B11 --kind gate --result pass --summary … --evidence …`. The tool fills in ts, branch and sha and validates the fields.
- The integrator writes the merge event and closes the claim in the merge commit.
- Owner rulings go to `rulings/`.

**Checklist for a less capable model:**
1. Run `status.py --all`; never edit STATUS.md.
2. Claim the unit in `units/<ID>.md` and commit.
3. After each stage, run `event.mjs add` and commit it with the stage's work.
4. Findings go only to `minors/<ID>.tsv`.
5. When interrupted: write an `interrupt` event, save the work-in-progress to `refs/wip/<ID>/<ts>`, and update `next`.
6. When merging: `Merge <ID>:` with the trailer, a `merge` event with the gate evidence, and close the claim.
7. Never edit another unit's files or any old event.

## 6. A track another developer can take
The natural split is the **storage track**: finish S2 (merge M2), then P (M3), T, X, I and V. It mostly touches tc-store, tc-journal, tc-corpus and tc-door43 sync. The **app track** (C7a, C7b now; D1/D2 after S2; E1a, E1b and E2 after X; F1 after I; G1 after D2; G2 after P) consumes it.

**Contract between the tracks:**
- tc-store's public API plus the R-n requests in APP-DESIGN.
- Every storage merge lands on main with the M2 gates and a `Unit:` trailer. The app lane merges main within one stage of each storage merge.
- One `units/<ID>.md` claim per unit, so the two people never edit the same file.

## Findings
- [F1] (high/high) main fast-forwards to resume-build-lane with no conflicts, but f9067bc has never passed one full gate after the B8, C4 and B11 merges, nor the Windows VM gate.
  - evidence: git rev-list --left-right --count main...resume-build-lane = 0 89; merge-tree exit 0; RESUME-2026-10-20.md B8 checkpoint: 1,520 tests in 3 crates; C4: 1,958 tests in 5 crates; B11.md:80-82: partial runs; the newest refs/winvm/* is from 2026-10-19 (git for-each-ref refs/winvm)
  - rec: Run the full AGENTS/CI gate set and gates.ps1 on f9067bc, then fast-forward main once the owner rules on pushing.
- [F2] (critical/high) Merging S2 into resume-build-lane conflicts in 13 files: 5 .audit files that both branches created, ci.yml, Cargo.toml, Cargo.lock, and tc-store handle.rs, hooks.rs, open.rs (414 conflict lines), tests/store/commands.rs (210) and drafts.rs (485). Cargo.toml also holds a decision on giving git2 TLS (https).
  - evidence: git merge-tree --write-tree --name-only resume-build-lane s2-proof-executor (tree cfe7568); conflict-marker counts from git cat-file on tree cfe7568; Cargo.toml conflict: RL 'without TLS' vs S2 features=["https","vendored-libgit2"], notify 8.2.0
  - rec: Merge S2 second, in a separate worktree. The S2 owner resolves tc-store and RL's author reviews drafts/commands. Gates: full set, long model seeds, crash and Burrito tests, op-count tests, storebench, save journeys and the VM gate. Decide git2 https against the AGENTS.md network rule.
- [F3] (high/high) storage (unit P, partial) conflicts with resume-build-lane in 17 files, including two separate speed-ups of the same aligner suggestion code (C5's d45be90 and storage's fbef0dc).
  - evidence: merge-tree resume-build-lane storage (tree 9a560b2): tc-align/src/suggest/boost.rs 225, engine.rs 62, wordmap.rs 142 conflict lines; git log of crates/tc-align/src/suggest on each side
  - rec: After S2 lands, merge main into storage on the P lane. Keep one suggestion engine, proven by the bitwise oracle and suggestion_budget. Merge storage into main only when P is finished.
- [F4] (high/high) lane-a8 holds 4 commits after A8's merge, including a translator-visible fix (58aa8a9: cancelling during the token calls signs out, as tC4's cancel does). The fix is on no integration branch.
  - evidence: git cherry -v resume-build-lane lane-a8: 4 '+' lines; only 2 of 17 lines that 58aa8a9 added to account.rs are present in resume-build-lane:crates/tc-app/src/account.rs; merge-tree RL vs lane-a8: 6 conflicting parity files
  - rec: After M1, cherry-pick 58aa8a9 and 599e516 and capture the parity DOM files again, or drop them by an explicit ruling.
- [F5] (medium/high) Old worktrees hold uncommitted work that no tracking file mentions: quick-timing (54 files, +982/-600), lane-b7 (3 files), lane-merge (2 files). The RL worktree also has 2 uncommitted B11 rows in its minors list.
  - evidence: git --no-optional-locks -C .claude/worktrees/quick-timing status / diff --stat; grep of the tracking files for quick-timing: no hits; diff of the live RL .audit/minors.tsv against f9067bc: 2 extra B11 rows
  - rec: Save each to a refs/wip/<name> ref and judge it before removing any worktree. The RL owner commits its minors.
- [F6] (high/high) Every commit on the three newer branches exists only on this PC, and GitHub CI never ran on them.
  - evidence: origin/claude/focused-ride-5uzj7p == af4eef3; origin/storage = 0d7d4f7, local storage 246 ahead; RL and S2 not on origin; ci.yml triggers on push (lines 4-6)
  - rec: Owner decision: allow pushing lane branches (or a backup remote) so CI runs before merge and there is an off-machine copy.
- [F7] (high/high) The root disk is 97% full mostly because of 815G of stale build folders in /var/tmp/tc-rst-target. The watcher moves targets there while watching free space on /home only.
  - evidence: df: / 905G, 833G used, 27G free; du -sh /var/tmp/tc-rst-target = 815G; watch-runs.py relocate_idle uses statvfs('/home') and globs only .claude/worktrees/*; no /var/tmp target belongs to the running sessions (.worktrees/* all have their own target folders)
  - rec: Delete the /var/tmp targets of merged or idle worktrees (build outputs are regenerable). Make the watcher check / and stop relocating to it. Do not place lane worktrees inside target/.
- [F8] (high/high) The tracking truth is scattered and stale: six different app-unit counts, STATUS.md wrong on both main (24 against 29 true) and RL (37 against 38), the watcher not running, and three different restart recipes.
  - evidence: main STATUS.md:7 '24 of 48' generated at fa46807; my read-only status.py logic gives 29 for main and 38 for RL; PROGRESS.md:11-12 '34/48'; RESUME-2026-10-20.md:87,127,146,166; plans/HANDOFF-resume-lane.md:20 '38 of 48'; no ~/.cache/tc-watch; last 'Regenerate the status list' commit cbdcdac 10-19
  - rec: Generate status from git across all branches, never commit the output, and freeze PROGRESS, RESUME and rebuild.tsv as history.
- [F9] (medium/high) The tracking and orchestration files will conflict on every merge, and S2's HANDOFF points to files its branch does not have.
  - evidence: add/add conflicts: HANDOFF.md, minors.tsv, tools/README.md, workflows/build-lane.js (240 lines), merge-main.js; S2 HANDOFF.md:44-49 refers to STATUS.md and PROGRESS.md, which S2 lacks; RL HANDOFF.md:94 names s2-verify as S2's worktree
  - rec: Adopt the per-unit, per-event layout before M2, and resolve the .audit conflicts by migrating both sides into it.
- [F10] (medium/high) status.py decides 'done' from a merge-subject regex plus a hardcoded list, so S2 and P will not show as done unless their merge subjects follow the pattern. Committing its output caused 6 merges that carried nothing but the status list.
  - evidence: status.py:16-18 BASE_DONE; status.py:50-54 regex; S2 merge subjects 'Merge the … fix round'; merges f5efad1, fa46807, a3299c3, d250a89, 0266753, c6433e3
  - rec: Add a 'Unit: <ID>' trailer to merges, have the generator read trailers, and gitignore STATUS.md.
- [F11] (medium/high) The handoff files give contradictory push rules.
  - evidence: RL HANDOFF.md:29 'push main (standing authorization)'; plans/HANDOFF-resume-lane.md:7 'Do not push without the owner'; S2 HANDOFF.md:33 against HANDOFF.md:75 (the same file says both)
  - rec: Ask the owner for one rule and record it as a single file in rulings/.
- [F12] (medium/medium) Units merged on 10-19 took a median of about 2.8 h from first commit to merge. Units that crossed interruptions took 9-18 h. S2 has been running about 76 h without merging (about 48 h of that on the redesign).
  - evidence: merge-commit timing script: B2 4.1, B3 2.7, B9b 1.4, C1b 5.4, B10 1.5, B9a 1.1, B5a 2.9, B4a 4.1, C6 2.3, C8 3.3, C1a 2.8, A8 2.0; B6 10.9, B5b 12.5, B4b 11.8, B7 13.5, B8 9.4, C4 18.2, B11 15.6; S2 first commit 33f7809 at 10-18T12:21Z; tip 3b075d7 at 10-21 12:24 EDT
  - rec: Treat S2 as the critical path with a named owner, and record stage durations as events so active time can be told apart from waiting.
- [F13] (medium/high) The main things that slowed the work were usage limits and container restarts, workflow state lost between sessions, no-loss review loops, half merges and duplicate agents, disk exhaustion and load-sensitive op-count tests.
  - evidence: rebuild.tsv lines 19, 30, 37, 49, 50, 53, 62, 68, 74, 98, 101, 126, 132, 134; plans/HANDOFF-resume-lane.md:15; RESUME-2026-10-20.md:7 (no Workflow runner); C4.md:43-45, B11.md:69
  - rec: Use durable, committed per-stage events, a shared heavy-run lock used by every session, and op-count tests that count operations rather than time.
- [F14] (medium/high) The gate lock and receipts from S2's process tools do not cover the resume lane, and the receipts are git-ignored local files whose identity includes a digest of every environment variable.
  - evidence: PROCESS.md:60-63; run-gates.mjs snapshot(): environment digest over all env vars; lane-state.mjs:56; state kept in .enjoy-lane/ (tools/README.md:38-41); RL uses .enjoy-logs/resume-full-gates.sh (RESUME-2026-10-20.md)
  - rec: Make run-gates.mjs the only gate entry point on every branch after M2. Keep receipts as local caches and record gate results as committed events.
- [F15] (low/high) 28 old worktrees are fully merged and clean, and can be removed after M1. Their targets on /home free about 255G. Eight worktrees plus the s2-review-exec-evidence directory belong to the running sessions and must be left alone.
  - evidence: merge-base --is-ancestor results for every worktree HEAD; du of their own target folders: app-b2 61G, app-ftext 51G, p0-a8 44G, app-c1a 32G, phase0 31G, tc4fix-int 18G, review-windows-push-refusal 14G, app-flife 3.7G
  - rec: List them in the plan; remove only after M1, with the owner's go-ahead and after saving work-in-progress refs.

## Unverified
- Whether the resume-build-lane and S2 sessions are still running: ps at 12:55 EDT showed no cargo, rustc, node or watcher process and the newest logs were from 12:21-12:24. Ask the owner, or check the Claude desktop session list.
- Whether GitHub CI ran on main af4eef3: not checked, because I avoided network calls. Check with gh run list --branch claude/focused-ride-5uzj7p.
- Whether the conflict resolutions in tc-store open.rs and drafts.rs are semantically compatible: only the conflict sizes were measured. A trial merge in a throwaway worktree plus the M2 gates would show it.
- Whether quick-timing's 54 uncommitted files, and lane-b7's and lane-merge's changes, are valuable or superseded: their purpose is not in any tracking file. Diff them against resume-build-lane.
- Whether origin/door43-push-refusal's 3 commits are covered by the merged review/windows-push-refusal: only patch-ids were compared. Diff the two.
- Whether the Windows VM gate ever ran on main's 10-20 merges: inferred from refs/winvm dates and the missing watcher state. The VM's C:\src checkouts or logs on the VM would confirm.
- The time each unit was actively built: git only gives first-commit-to-merge wall time, and C2, C3 and C5 are single squashed commits. The Workflow journals or .enjoy-lane stage files would give real durations.
- Whether P's partial work on storage still meets its budgets after S2: not measured. Run perfreport after M3.