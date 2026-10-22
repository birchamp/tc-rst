# Where the rebuild stands (2026-10-21)

This is the evidence behind `ROADMAP.md`. It comes from eleven read-only evaluations run on
2026-10-21. Each one was checked by a second agent that tried to refute it. The full reports are
in `evidence/2026-10-21/`. Every claim below names its source. Where a number is an inference,
it says so.

## 1. In one paragraph

The hard foundations are built and mostly sound. The app layer is 38 of 48 units built, but only
29 of them are on main: the other nine sit on an unpushed branch. Storage, which every remaining
feature needs, is 4 of 10 units done. S2 is close to finished but not verified at its latest
commit, and sync (X), import (I) and the format freeze (V) have not started. GitHub CI on main
has been red since 2026-10-19, and none of the browser journeys run in CI. So no item of PLAN's
definition of done is yet fully shown by a CI check that can fail; P1, P2 and P9 have nothing in
CI. The work has split across three branches with three diverged trackers. 182 commits exist on
no remote, only on this PC. The root disk is 97% full. The code is far larger than planned
(about 143,000 product lines against an estimate of 50,000 to 70,000), and most of the excess is
repeated patterns in the UI glue and new mechanisms in tc-store.

## 2. Branches and what is on them

| Branch | Tip | Ahead / behind main | Pushed | Holds |
|---|---|---|---|---|
| `claude/focused-ride-5uzj7p` (main) | af4eef3 | | yes | 29 app units, storage J1, J2, C, S1, all six tC4 sweep fixes |
| `resume-build-lane` | f9067bc | 89 / 0 | no (89 commits on no remote) | main plus B4b, B5b, B7, B8, B11, C2, C3, C4, C5 (38 app units). Owned by a running session |
| `s2-proof-executor` | 3b075d7 | 77 / 541 | no (69 commits on no remote) | S2: the placement ledger, local git, outside edits, carried files. Owned by a running session |
| `storage` | dd63328 | 64 / 351 | no (56 commits on no remote) | Unit P's partial work, built on the old S2 and never reviewed |

Sources: `git rev-list --left-right --count`; `git rev-list --count <branch> --not --remotes` (182 for the three together); `evidence/2026-10-21/process-integration.md` §0-§1.

- Main fast-forwards to `resume-build-lane` with no conflicts. That branch has not passed a
  full gate since its C4 and B11 merges (B8's merge had one), and no Windows VM gate has run
  since 2026-10-19 (process-integration F1, corrected by its verifier).
- Merging S2 conflicts in 12 files against main and 13 against `resume-build-lane`. The real
  work is in `tc-store/src/open.rs` (414 conflict lines): C1a's creation pins must be ported into
  S2's new open (storage-remaining ST-2).
- `storage` conflicts in 17 files with `resume-build-lane`, including a second, independent
  speed-up of the aligner's suggestions. Do not merge it; re-apply P on a fresh branch after S2
  (storage-remaining ST-3).
- Stray work outside these branches: `lane-a8` holds a sign-in cancel fix a translator would see
  (commit 58aa8a9), and `quick-timing` holds 54 uncommitted files nobody tracks
  (process-integration F4, F5).

## 3. Units

### App layer (`docs/APP-DESIGN.md` §10)

| State | Units |
|---|---|
| On main (29) | A1-A11, G0, B1a, B1b, B2, B3, B4a, B5a, B6, B9a, B9b, B10, C0, C1a, C1b, C6, C8 |
| Built on `resume-build-lane` only (9) | B4b, B5b, B7, B8, B11, C2, C3, C4, C5 |
| Ready now | C7a (the resume lane is starting it) |
| Waits for the owner | C7b (Q-9 design sign-off) |
| Waits for S2 | D1, D2 |
| Waits for X | E1a, E1b, E2 |
| Waits for I | F1 |
| Can mostly start now | G1 (the shell; listed as waiting for D2, but only `printPdf` needs D2) |
| Last | G2 (budgets and the P7 report) |

Sources: app-remaining §0, tc4-coverage §1, split-seams S2.

B11 merged at f07fd9e, but its records are unfinished: STATUS still says "ready to start", two
of its minors are uncommitted, and its merge gate has no recorded pass (app-remaining F1,
checks-inventory C3).

### Storage (`docs/STORAGE.md`)

| Unit | State | What is left |
|---|---|---|
| J1, J2, C, S1 | done | |
| S2 | built, not verified at 3b075d7 | Full validation (44 mutation recipes, 4 long model seeds), re-review of changed areas, merge |
| P | 23 unreviewed commits on the old base | Re-apply on post-S2 main: tooling first, then each speed-up alone, then measure |
| T | mostly landed through other units | Two derived-byte fixes before the format freeze, a pin-order rule, four tests |
| X | not started | New crate `tc-sync`; git fetch, push and list in `tc-door43`; one intake entry in `tc-store` |
| I | not started | New crate `tc-import`; the tC4 4.0, tC3, USFM and Burrito importers; the team move |
| V | not started | Frozen sample projects, a byte pin, a CI job; the owner freezes format 1.0 |

At 7d3a5d3, the last full S2 validation failed: 38 of 43 mutants detected, long seeds 002 to
004 failed. Fixes were merged after that; no validation has run since (storage-remaining ST-1).
The owner has already answered the questions X and I need (ST-7).

## 4. The definition of done (PLAN P1 to P10)

CI has checks for the parts already built: P3, P4 and P10 at the journal and store level, P5 where a
test asks for it, P7 over the committed captures. No item is fully shown, and P1, P2 and P9 have
nothing in CI.

| P | Today | Main gap |
|---|---|---|
| P1 Journeys | 29 journey files, about 200 browser tests, all local. 10 of 24 live journeys reach a browser and disk end state; 7 partial; 7 none | Every browser test skips in CI. No app-e2e job |
| P2 Team work | Scripted pieces only | No sync, share push, USB or check-conflict choice exists |
| P3 No silent loss | Strong journal and store properties | Nothing for sync or import; STORAGE §15 names 47 tests that do not exist |
| P4 Crash safety | Save, commands, checkpoint, Git on S2 | No sync or import write points; S2's nightly never ran |
| P5 Valid burritos | Validation is opt-in per test; S2 has a writer hook | No hook on main; journeys never validate |
| P6 Helps sources | DCS works; Aquifer in libraries only | No unit puts Aquifer in the app; the owner answered Q33 and Q34, and the screens need a design |
| P7 Same look | 62 baseline screens (6 in Arabic), a gate over committed captures | The gate cannot see stale captures; human review not recorded |
| P8 Scripts | RTL interface journeys, local | The Graphite probe is a spike with no owner unit; no RTL project journey |
| P9 Carry-over | 16 import fixtures read by no test | No importer |
| P10 Old projects | Wire format pins | No frozen sample projects; no old-build CI job |

Source: done-definition §1-§2, findings DOD-1 to DOD-15.

## 5. The checks

- **CI is red on main.** The last green run is a52bc0c (2026-10-19 12:59Z). Of 163 runs on main,
  20 passed, 79 failed and 64 were cancelled. The tip fails at "clippy for Windows" because
  `crates/tc-app/tests/op_counts/harness.rs:179` calls `std::os::unix::fs::symlink` without
  `#[cfg(unix)]`, so `cargo test` never ran. Of the 79 failures, 58 stopped at workspace clippy
  (56 of them on one lint, `chunks_exact_to_as_chunks`, that the floating `stable` toolchain
  brought), 11 at `cargo test` (among them real failures such as
  `a_1_gib_exchange_upload_streams_to_disk` and `tokio_never_blocked`), and 6 at the Windows
  clippy. Some runs died on a full runner disk (checks-inventory C1 and its verifier; checked
  again with `gh run view 39426828571`).
- **Local gates do not match CI.** No Linux gate list includes the Windows clippy step; the
  Windows VM gate would catch it, but it has not run since 2026-10-19 (8d039a8). Merge scripts
  test a few crates; one local runner runs `cargo fmt --all` (which rewrites instead of
  checking) and `UPDATE_REPORT=1` before the gate that reads that report (C2).
- **Merges land before their gates.** B11 and C4 have merge commits with no recorded passing
  gate (C3).
- **Rules held only by prose:** the dependency rules, the I/O rules, "tests never touch the
  network", shared dependency versions (one violation: `tc-door43-fake` declares `futures-util`
  inline) (C5).
- **Not deterministic:** wall-clock windows in the J15 tests, 45 sleep-then-assert sites, random
  proptest seeds (one property flaked 2 times in about 55 runs) (C8, C9).
- **Slow:** about 899 s of test binaries; tc-api's round-trip binary alone takes 240-291 s (C10).
- **Good:** op-count tests at Psalms and ten times Psalms; crash injection at every write point
  of save; the placement-ledger model test; clean panic discipline enforced by lint.

## 6. Code health

| Area | Verdict | Main points (source) |
|---|---|---|
| Domain and foundation crates | Good | 0 unwrap and 0 panic in library code; pure crates do no I/O; big parsers have round-trip properties. Duplicate readers for one import job, three shapes of chapter and verse numbers, six copies of the JavaScript whitespace rule (health-domain) |
| App stack | Disciplined at the bottom, repetitive in the UI glue | One endpoint table, one write entry point, one save machine for eight text kinds. But 4 job-follow loops, 14 places in 6 files that handle "needs internet" or "needs sign-in", about 8 refusal-to-words sites, hand-made busy and error state in many dialogs, 9 direct navigation-gate calls in product code. The unbuilt units would copy them again (health-app CH-2 to CH-8) |
| Storage core | Right structure, unfinished edges | The ledger is a real pure state machine. S2 more than doubled tc-store (about 11,400 source lines at the merge base to 26,046). Since the ledger landed, 9 of 10 fixes went to fact producers and executors, not the machine. `Writer` has 42 fields; journal files have no state machine; at least seven places make "keep a copy" files, five of which can replace an existing file (health-storage F1-F7) |
| Size | Far over the estimate | About 143,000 product lines against PLAN's 50,000-70,000; tc-ui alone about 2.15 times tC4's views, design system and state (health-app CH-1) |

The loss list in STORAGE §15 grew from 96 rows to 143 between 2026-10-15 and 2026-10-21. S2's
review rounds added 22, then 10, then 8. By one evaluator's reading (an inference), most of S2's
40 rows come from three mechanisms: outside-edit detection (26, 12 of them `metadata.json`),
carried-file history inside git (9), and journal files read by offset (5). 19 of the 40 involve a
journal file cut, deleted or damaged between steps, by another program or a power cut
(health-storage F11 and its verifier).

## 7. tC4 coverage and tC4's bugs

- Most of tC4 4.0's screens are built. Gaps: installers for Windows, macOS and Linux and their
  signing sit with G1 only through TC4-ISSUES rows, not in G1's scope; Aquifer in the app (P6;
  the owner's Q33 and Q34 answers need screens); the move to tC4's newer reference; a way to
  import a tC4 4.0 project from the UI; the "move the team" prompt (tc4-coverage §3 and its
  verifier).
- tC4 has moved 98 commits since the last issue sweep (9ad4d5c to b86270f) and tagged
  `v4.0.0-rc.2`; there is no `v4.0.0` tag yet. Several of its changes have no row in
  `docs/TC4-ISSUES.md`: #807, #812, #813, #820, #821, #822 and the new sync-refusal journey
  (tc4-coverage F6, tc4-bugs T8).
- **tC4 bugs the rebuild repeats, found by reading code (each needs a red-first test):**
  - #830: on a Hebrew verse, the words placed on an Align card read backwards.
  - #832: Project settings writes a direction that Home, the open project and Align never read.
  - #836: English helps under an Arabic language set are laid out right to left.
  - Q30(a)/#825: a link a Translate save cannot follow goes back to the bank, but the list the
    owner asked for is thrown away in `tc-store/src/commands.rs:349-362`.
  - #801: a save refused for good blocks the way to Home.
  - R28: with Dropbox installed on macOS or Linux, every project opens read-only.
  (tc4-bugs T3-T5, app-remaining F10.)
- `docs/TC4-ISSUES.md` is stale: all 15 "rebuild bug" rows are fixed, about 29 "unit todo" rows
  name only built units, and rows such as #801, #576, #179, #731, #613 and #599 have no test
  anywhere (tc4-bugs T1, T2).

## 8. Process and tracking

- Several different app-unit counts across the tracking files; both committed STATUS files are stale;
  three different restart recipes; contradictory push rules (process-integration F8, F11).
- The watcher is not running, so STATUS is not regenerated and the Windows VM gates do not run.
- `status.py` decides "done" from merge subjects plus a hard-coded list; committing its output
  caused four merges that carried nothing else (F10, corrected by its verifier).
- The root disk is 97% full: 815 GB of stale build folders in `/var/tmp/tc-rst-target`
  (F7). 28 old worktrees are merged and clean (F15).
- What slowed the work: usage limits, workflow state lost between sessions, no-loss review loops,
  half merges and duplicate agents, a full disk, and load-sensitive tests (F13). Units on
  2026-10-19 took a median of about 2.8 hours from first commit to merge; units that crossed an
  interruption took 9 to 18 hours; S2 has run about 76 hours (F12).

## 9. What the completeness critic added

A critic read all eleven verified reports and sent three gaps back for research
(`evidence/2026-10-21/gap-*.md`):

- **Release and the app's identity.** No branch has a release pipeline; `ci.yml` uses only Linux
  runners. Undecided: the app's name, version, application id and download page (the tC4 team
  needs them for `moved.json`), the signing credential for this repository, and whether the
  first launch looks for tC4 4.0 projects. A translator can meet one problem today: the help
  pages the rebuild ships say projects live in `~/pankosmia/tc4-projects`, while the rebuild
  keeps them in `~/translationCore4/projects` (owner questions H4, H16).
- **Performance is not part of PLAN's done.** `docs/PERFORMANCE.md` has 34 budget rows on main.
  None is met end to end on the reference laptop under its rule 5. Six rows miss even on the fast
  build machine (checkpoint p95 586-761 ms against 500 ms; a save during a checkpoint; the store's
  memory with the default heap; suggestions NT p95 95.9 ms; the first aligner open; the cold open
  after a reboot, one run at 10,038 ms). The remedies the owner was asked to choose for five store
  rows are written only on the S2 and storage branches (owner question H17; card P).
- **tC4 teammates who have not moved.** STORAGE §13.3's promise that no straggler's work is lost
  holds in the normal case: tC4 refuses to sync (`sync.verify-failed`) before it pushes. One
  state escapes it: if the Door43 repository has no `main` branch, tC4's Home flow uploads the
  straggler's branch with no check, says the upload worked, and the team never sees the work.
  Only an owner's action on Door43 reaches that state. No test covers a moved repository (card X).

The critic also reconciled conflicts between reports: the order of the storage chain (decided in
`ROADMAP.md`), P7's partial CI gate, the answered Q33 and Q34, and the size of the scope cuts in
H8 (another program changing a byte is inside REACH; only a disk change that keeps size and time
is outside it).

## 10. What follows from this

1. Converge first. Every week the branches stay apart, the merge gets harder and the trackers
   drift further. Back up the unpushed work.
2. Make CI the gate again, with one gate list shared by CI and every local runner.
3. Put the most effort at the top of the funnel: build the shared structures the remaining units
   need once (typed jobs, ports, one leave path, one network gate, one reader per import job, a
   simpler store), and add the mechanical checks that keep the rules true.
4. Then finish storage (S2, T, P, X, I, V) and the app units that wait on it.
5. Hand the shell and release work to a second developer: it has a written contract and touches
   almost no shared files (`TRACK-B.md`).
