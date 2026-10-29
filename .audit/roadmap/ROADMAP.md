# Roadmap to done

Written 2026-10-21 from `ASSESSMENT.md`. The milestones were brought up to date on 2026-10-29,
after batch 48, for the fast-release ruling and the 4.1.0-preview. The live state of each item is
in its card in `units/` and on the board (`python3 -I .audit/tools/board.py`). This file holds the
order and the reasons; it changes only when the plan changes.

## The goal

The rebuild is done when:

1. every statement P1 to P10 of `docs/PLAN.md` is true and is shown by a CI check that can fail;
2. the app installs and runs on the platforms the owner chooses (decision H3), built by a
   release pipeline;
3. every tC4 bug that the rebuild could repeat has a test that fails if it comes back;
4. every budget row of `docs/PERFORMANCE.md` is measured on the reference laptop in a release
   build and is met, or has a remedy the owner chose (decision H17). PLAN's P1 to P10 do not
   mention the budgets; AGENTS.md requires them (`evidence/2026-10-21/gap-performance-acceptance-ledger.md`).

Accessibility beyond tC4's own (screen readers, contrast) is out of scope for 1.0; keyboard use
matches tC4 and is held by `parity/app/journeys/check-keyboard.test.mjs` and the journeys G5
runs. If the owner wants more, it is a new H row, not a card.

## How the work is organised

- **Work items.** Every piece of remaining work is one card in `units/<ID>.md`. Old unit ids
  (C7a, D1, X, ...) keep their names. New items use a prefix: `INT` (integration), `CHK`
  (checks), `UI` (shared app structures), `DOM` (domain crates), `FIX`, `SWEEP`, `AQ` (Aquifer),
  `REF` (reference move), and `G3` to `G5` (shell and release).
- **Tracks.** Track A is the main line on the owner's Linux PC. Track B is a package for a
  second developer (`TRACK-B.md`).
- **Lanes inside Track A.** Each lane has one owner at a time, so two writers never touch the
  same files:
  - `integration`: merges, trackers, disk (the orchestrator);
  - `storage`: tc-store, tc-journal, tc-corpus, tc-sync, tc-import;
  - `app`: tc-api, tc-app, tc-server, tc-ui;
  - `checks`: CI, test support, lints, the architecture tests;
  - `domain`: the other crates.
- **Capacity.** This PC runs at most two heavy gate runs at once (disk and CPU). The storage lane
  has first claim on the heavy-test slot, because it is the critical path.

## Principles for the rest of the work

1. **Converge before you build.** No new item starts on a branch more than one stage away from
   main. Each finished item is merged into main and CI must pass before the next one in its lane
   starts.
2. **CI is the gate.** One gate list (INT-3) runs in CI and in every local runner. A red main
   stops every merge until it is green.
   One merge into main at a time: the orchestrator hands out turns, and main's CI finishes green
   before the next (owner rule, 2026-10-23; `TRACKING.md` "Checklist: merge an item").
3. **Build once, then reuse.** Where the remaining units would copy a pattern again, the shared
   structure is built first (M1). This is the Code layer of the funnel, and it gets the most
   effort.
4. **Every item fills in its funnel card before code** (`FUNNEL.md`).
5. **Every result lands in its card in the same commit** (`TRACKING.md`).
6. **Simplify before you extend.** Storage gets one simplification pass (S3) before sync adds
   new writers to it.
7. **tC4 4.0 is the baseline, not the ceiling.** The rebuild matches tC4 4.0's look and
   behaviour, but where tC4 has an open issue or a planned feature, the rebuild may build it now
   instead of waiting to copy tC4; REF1 and SWEEP still track tC4's fixes
   (`rulings/2026-10-23-replace-tc4.md`: the rebuild will most likely replace tC4).

## Milestones

Sizes are inferences from comparable finished units (the basis is in each card). A "session" is
one agent session of a few hours; for adding up, count a session as half a day (the median
finished unit took 2.8 h, `ASSESSMENT.md`). The State column is the state on 2026-10-29 (main
bb61001, after batch 48); the board is the live state and wins where they differ.

### Where the milestones stand (2026-10-29)

74 of 87 cards are done. The owner's fast-release ruling (`rulings/2026-10-24-H3-H4-H7-fast-release.md`)
split the remaining work in two: the first release, **4.1.0-preview**, for named testers only
(H9), and the rest of the road to DONE after it.

| Milestone | Done | Open | Parked until after 4.1.0-preview |
|---|---|---|---|
| M0 | 9 of 9 | none | none |
| M1 | 29 of 33 | none | CHK-5, DOM-3, S3; UI-3 is split into UI-3a and UI-3b, both done |
| M2 | 4 of 6 | none | P; V (next after REL-1, H9) |
| M3 | 16 of 17 | none | AQ1 (with H6) |
| M4 | 4 of 5 | USB-VERIFY (ready) | none |
| M5 | 12 of 17 | REL-1 (building), PERF-LAPTOP (ready), LINK-MARK (after 4.1.0), DONE | G2 |

**What 4.1.0-preview still waits for.** Every card REL-1 depends on is done on main; batch 47
merged the last four (the tester findings). What is left is the owner's:

1. **Translations (H10).** The owner's translations landed in batch 48. Five keys added since are
   still English in each of the nine languages (`crates/tc-ui/assets/i18n-rebuild/*.pending.json`),
   so release.yml's `translations` job still refuses a tag.
2. **Signing (H4, A4).** Yakob signs, as for tC4; the `windows-signing` environment and its
   secrets are not created yet.
3. **REL-1 step 4.** The owner installs the preview and says yes. That review also stands in for
   P7 in 4.1.0 (`rulings/2026-10-27-p7-review.md`).

Then the tag `v4.1.0-preview` goes to named testers.

**After 4.1.0-preview.** V comes first (H9: the `1.0-draft` capsule set, so testers' projects
open in 1.0), then the parked cards, the three new cards (LINK-MARK, USB-VERIFY, PERF-LAPTOP;
each budget row that PERF-LAPTOP finds missed becomes a card, H17), G2, and DONE. V's card still
depends on S3, so S3 runs first unless the card drops that dependency; the integration lane
settles which when it claims V.

### M0. Converge, and make CI trustworthy (done)

| ID | Item | Lane | Depends on | Size | State |
|---|---|---|---|---|---|
| INT-1 | Back up the unpushed branches off this PC | integration | owner H1 | under 1 h | done |
| INT-2 | Make CI green and trustworthy | checks | | 0.5 d | done |
| INT-3 | One gate list for CI and every local runner; merge only with a passing gate | checks | INT-2 | 0.5-1 d | done |
| INT-4 | Land `resume-build-lane` on main | integration | INT-2, INT-3; its step 4 asks the owner before the push (`rulings/<date>-int-4-main.md`) | 0.5 d plus gate time | done |
| S2 | Finish S2 and merge it. Steps 1-4 run on 3b075d7 now; the merge (step 5 on) waits for the resume lane to land | none | 1-2 d | done |
| INT-5 | Converge the trackers onto this roadmap | integration | INT-4 | 0.5 d | done |
| INT-6 | Rescue stray work and free the disk | integration | INT-4, owner OK for deletions | 0.5 d | done |
| INT-7 | Prepare the Track B hand-off: freeze the shell contract | integration | INT-4 | 1 d | done |
| INT-8 | CI runs are the gate receipt | checks | INT-3; the owner asked for it on 2026-10-23 | 0.5 d | done |

**Exit (met):** main holds every built unit and S2; CI on main is green; the gate list is one
script; every unpushed branch has an off-PC copy; the board is the only tracker.

### M1. Build once, check always (done for 4.1.0; three cards parked)

| ID | Item | Lane | Depends on | Size | State |
|---|---|---|---|---|---|
| CHK-1 | Architecture and I/O boundary checks | checks | INT-4, CHK-1a | 0.5-1 d | done |
| CHK-1a | Architecture, I/O and network checks in tc-arch, and ARCHITECTURE.md matching the code | checks | INT-4 | 0.5 d | done |
| CHK-2 | Every written project is validated (P5) | checks | S2 | 1-2 d | done |
| CHK-3 | Claimed tests must exist (STORAGE §15, TC4-ISSUES, a P1 journey map, a P-item ledger) | checks | INT-4, CHK-3a | 1 d | done |
| CHK-3a | Claimed tests must exist: TC4-ISSUES, the done cards, the P-ledger and the tC4 journey map | checks | INT-4 | 0.5 d | done |
| CHK-4 | Deterministic and fast tests | checks | INT-4 | 2-3 d | done |
| CHK-5 | Nightly, Windows and no-network jobs; the mutation executor runs nightly | checks | S2, INT-3 | 1-2 d | parked until after 4.1.0 |
| CHK-6 | Lint ratchet and fresh P7 captures | checks | INT-4, CHK-6a | 1 d | done |
| CHK-6a | Release builds without the test server, every ignored test named, PLAN's reference checked | checks | INT-4 | 0.5 d | done |
| CHK-4b | tc-app's tests wait for events, never for a short clock | checks | CHK-4 | 0.5 d | done |
| CHK-8 | A hung test is named and stopped within minutes, in CI and in scripts/gates.sh | checks | none | 0.5 d | done |
| UI-1 | Typed jobs and one job follower | app | INT-4, C7a | 1 d | done |
| UI-2 | Ports for sync, share, exchange and import | app | UI-1 | 1-2 d | done |
| UI-3 | One leave path, one network-call helper, one refusal text, Submit, dialogs that own their forms, WriteCap enforced (split into UI-3a and UI-3b) | app | UI-1 | 2 d | parked: split, both halves done |
| UI-3a | One leave path, one network-call helper, one refusal text, Submit | app | UI-1 | 1 d | done |
| UI-3b | Dialogs that own their forms, WriteCap enforced | app | UI-3a | 1 d | done |
| DOM-1 | One network gate: all network code behind `tc-door43` | domain | S2 | 1-2 d | done |
| DOM-2 | One reader per job and one set of identifiers | domain | S2 | 2-3 d | done |
| DOM-3 | Capability tokens for project writes and the network | domain | S3, DOM-1 | 2-3 d | parked until after 4.1.0 |
| S3 | Simplify the store before sync | storage | S2, CHK-1 | core about 1 week; all 1-2 weeks | parked until after 4.1.0 |
| LOSS-1 | Write the loss tests STORAGE §15 promises that no card wrote | storage | CHK-3 | 2 d | done |
| LOSS-2 | Fix two store losses the model test wrongly excused | storage | none | 1-2 d | done |
| BURRITO-HOLD | A book file the app cannot read never leaves metadata.json listing bytes that are not there | storage | none | 0.5-1 d | done |
| FIX-R | Fix the tC4 bugs the rebuild repeats; reconcile TC4-ISSUES | app | INT-4 | 2-3 d | done |
| FIX-J | Finish the partial P1 journeys J3, J12 and J13 | app | INT-4 | 1-1.5 d | done |
| FIX-HIDE | A held leave never hides a verse whose save failed | app | none | 0.5 d | done |
| FIX-HIDE2 | A section draft restored after a reload is shown, ready for Save | app | FIX-HIDE | 0.5 d | done |
| FIX-RESET-RECENCY | Each captured screen starts with Home's cards in the seed's order | app | none | 0.25 d | done |
| SWEEP | Sweep tC4 again (b86270f and release/4.0), then every week | integration | INT-4 | 0.5 d | done; the weekly sweep waits until after 4.1.0 |
| LIC-1 | Move the rebuild to the MIT licence, as tC4 did | integration | SWEEP | 0.5 d | done |
| CHK-7 | CI runs the full tests only on gates and main; fast checks before a gate; nightly failures become cards | integration | | 0.5 d | done |
| CHK-7b | One command runs what a branch's change reaches before it is handed over | integration | CHK-7 | 0.25 d | done |
| CHK-7c | The scale tests run at twice Psalms on pushes and gates, ten times at night | integration | CHK-7b | 0.25 d | done |

**Exit:** the checks in `FUNNEL.md` §"Checks the repository must have" all run in CI; the shared
structures exist and the remaining units' cards point at them. Met for 4.1.0, except the
nightly jobs (CHK-5). S3's core slices and DOM-3 moved after the release by the fast-release
ruling.

### M2. Storage to done (4 of 6 done; P and V after 4.1.0)

| ID | Item | Depends on | Size | State |
|---|---|---|---|---|
| T | The storage side of the tC4 issue sweep (what is left of it) | S2 | 0.5-1 d | done |
| P | Scale: re-apply on post-S2 main; tooling first, speed-ups after S3's core | S2 | 2-3 d | parked until after 4.1.0 |
| X | Door43 sync and USB exchange (`tc-sync`). Steps 1-3 run beside S3; intake waits for S3's core | S2, DOM-1, T | 4-6 d | done (built on the current store, per the fast-release ruling) |
| PUSH-1 | A large push goes to Door43 as a chain of commits, so a poor link can finish it | LOSS-1, E1a | 2-3 d | done |
| I | Import (`tc-import`): tC3 projects and Update a book only, by the fast-release ruling | S2, DOM-2 | 4-7 d | done |
| V | Old projects keep working; the owner freezes format 1.0, with this preview's projects as the `1.0-draft` capsule set (H9) | X, I, T, S3, owner H9 | 1.5-3 d | parked until REL-1 is done; then next |

**Exit:** P2, P3, P4, P9 and P10 have their CI checks; the format is frozen. For 4.1.0-preview
the checks run for the kept scope; the freeze waits for V (H9).

### M3. App to done (16 of 17 done; AQ1 after 4.1.0)

| ID | Item | Depends on | Size | State |
|---|---|---|---|---|
| C7a | Conflict choices: text and passage | B11 (done) | 3-4 sessions | done |
| C7b | Newest wins outside verse text (H5), and the conflict screens that stay | C7a, S2, owner H5 (ruled) | 4.5-6.5 sessions | done |
| HIST-1 | History and Restore on the screens, and the Received sheet's replaced rows | C7b, UI-3b | 2-3 sessions | done |
| D1 | Checkpoints and outside edits | S2, UI-3a | 5-6 sessions plus 1-2 of integration | done |
| D2 | Exports | S2, UI-1 | 4 sessions | done |
| E1a | Sync | X, C7a, D1, UI-2, UI-3a | 5-6 sessions | done |
| E1b | Share and open from Door43 | E1a | 3-4 sessions | done |
| E1c | One name for a project: open from Door43 asks none (H14, #837) | E1b | 0.5 d | done |
| E1d | E1a's two unwritten tests | E1a | 0.5 d | done |
| ALIGN-LEAVE | Home asks before it leaves an aligner click behind | E1d | 0.5 d | done |
| E2 | USB exchange | X, D2, C7b, UI-2, UI-3a | 4-4.5 sessions | done |
| F1 | Import: a tC3 project and Update a book, by the fast-release ruling | I, UI-2, UI-3a | 5-7 sessions | done |
| AQ1 | Aquifer helps in the app (P6) | INT-4, UI-1, owner H6 (Q34) | 3-5 sessions | parked until after 4.1.0 |
| REF1 | Move the tC4 reference and port its changes | SWEEP, owner H7 | 3-5 sessions | done |
| OLH-1 | Highlight a note's words in the original-language pane (#840, built ahead of tC4) | INT-4 | 2-3 sessions | done |
| REF1b | Poem lines as tabs in the verse editor (tC4 #810) | REF1 | 2-3 sessions | done |
| REF1c | Notes by figure, with groups that fold (tC4 #769) | REF1 | 2-3 sessions | done |

**Exit:** every live tC4 journey (P1) and the team journeys (P2) run in CI; P6 is shown. P1 and
P2 are green for 4.1.0; P6 waits for AQ1.

### M4. Shell and release (4 of 5 done)

| ID | Item | Depends on | Size | State |
|---|---|---|---|---|
| G5 | Browser journeys in CI, with no skips allowed | INT-2 | 2 d | done |
| G1 | The Electronite shell | INT-7 | 20-30 commits | done |
| G3 | Release pipeline and installers | G1 | 1-2 weeks | done |
| G4 | The P8 Graphite probe and right-to-left project journeys | G1 | 2-3 d | done |
| USB-VERIFY | The USB export's check reads the stick, not the page cache | E2 | 1 d | ready |

The shell lane built G1, G4 and G5 on Track A, and the integration lane built G3; Track B was
not needed for them. G3's pipeline builds and smoke-tests the 4.1.0-preview installers on
Windows, Linux and macOS (REL-1 step 3).

**Exit:** installers build and pass a smoke test on each chosen platform; P8 runs on each. Met,
unsigned; the signed Windows build waits for H4's signing set-up (A4).

### M5. Release and done

**4.1.0-preview.** REL-1 depends on every card in the cut scope plus the fixes found on the way
(the loss tests, the tC4 issue sweep's five blockers and the four tester findings). All of those
are done; REL-1 waits only for the owner (above).

| ID | Item | Depends on | Size | State |
|---|---|---|---|---|
| FIX-KEPT-EVENT | Lost work kept just after a project opens reaches the pages without a reload | none | 0.25 d | done |
| FIX-791 | A Door43 sync says which alignment, check or setting a teammate's newer version replaced | none | 0.25 d | done |
| SAVE-WAIT | The save-machine tests fail with a message instead of hanging | none | 0.25 d | done |
| LINK-DROP | A Translate save that sends an alignment link back to the word bank says so (tC4 #825, #820) | none | 1 d | done |
| TC3-LANG | A tC3 import keeps the project's language name and direction | none | 0.5 d | done |
| SRC-VER | Get source texts names the versions that are installed | none | 0.5 d | done |
| POEM-TAB | Poem indents are two characters wide, and Tab indents in the Verse editor | none | 0.5 d | done |
| VRS-GUESS | A tC3 import offers the versification its verse counts fit, and the translator can change it (H13) | none | 1-2 d | done |
| HOME-SETTLE | Home never shows a project's progress from before its last edits | none | 0.5 d | done |
| FEEDBACK-PATHS | A Feedback report never carries the home folder or the account name | none | 0.5 d | done |
| PKG-HERE | Download package says when the package was already on this computer | none | 0.5 d | done |
| TOAST-ORDER | The sync toast names its teammates in the same order on every run | none | 0.1 d | done |
| REL-1 | First release candidate (4.1.0-preview): the cut scope green, the owner's reviews recorded | C7a, C7b, CHK-1, CHK-2, CHK-3, CHK-6, D1, D2, DOM-1, DOM-2, E1a, E1b, E2, F1, FIX-J, FIX-R, G1, G3, G4, G5, I, INT-6, REF1, S2, SWEEP, T, UI-1, UI-2, UI-3, X, REF1b, REF1c, HIST-1, LOSS-1, E1c, E1d, ALIGN-LEAVE, FIX-KEPT-EVENT, LOSS-2, BURRITO-HOLD, SAVE-WAIT, LINK-DROP, TC3-LANG, SRC-VER, POEM-TAB, VRS-GUESS, FIX-791, HOME-SETTLE, FEEDBACK-PATHS, PKG-HERE, TOAST-ORDER | 1 d | building: steps 1-3 done; waits for the owner's H10 keys, A4 and step 4 |

**After 4.1.0-preview, to DONE.**

| ID | Item | Depends on | Size | State |
|---|---|---|---|---|
| LINK-MARK | A link a save sends back to the word bank stays marked for review across a crash | LINK-DROP | 1-2 d | ready; after 4.1.0 |
| PERF-LAPTOP | Measure every speed and memory budget on the reference laptop (H17) | none | 1 d | ready, when the reference laptop is online |
| G2 | Budgets, the whole-Bible performance runs, Arabic captures of all 62 screens, the P7 review | P, G5, C7b, D1, D2, E1a, E1b, E2, F1, AQ1, REF1 | 10-15 commits | parked until after 4.1.0 |
| DONE | Release candidate: P1 to P10 green in CI, the owner's reviews recorded | every other card | 1-2 d | waits |

## The critical path

To 4.1.0-preview, no build work is left; only the owner's three items stand between main and the
tag:

```
main (bb61001, every REL-1 card done) ─┬─ H10: five keys × nine languages ─┐
                                       ├─ H4/A4: Windows signing set-up ───┼─ REL-1 step 4: the owner's review ─ v4.1.0-preview
                                       └───────────────────────────────────┘
```

After the preview:

```
S3 (core, then all) ─┬─ V (1.0-draft, H9) ─────────────────────┐
                     └─ DOM-3 ─────────────────────────────────┤
P ─────────────────────── G2 ──────────────────────────────────┤
AQ1 (H6) ──────────────── G2                                   ├─ DONE
CHK-5, LINK-MARK, USB-VERIFY, PERF-LAPTOP (misses become cards)┘
```

The longest chain after the preview is S3 then V, about two to three weeks by the sizes above;
P and G2 (about a week and a half) and AQ1 run beside it on other lanes. If V's card drops its
S3 dependency, V can follow REL-1 directly (1.5 to 3 days) and S3 moves off the path to DONE's
format freeze.

## The two sessions that were running on 2026-10-21

Two sessions were working when this plan was written. On 2026-10-22 12:53 the owner confirmed
both have ended. Their branches stand at `resume-build-lane` 43496296 (f9067bc plus a WIP
commit of B11's two loose minors rows, made at takeover) and `s2-proof-executor` 3b075d7 (clean).
C7a and S2 are released to `todo`; INT-4 step 1 takes over `resume-build-lane`, and S2 step 0
takes over `s2-proof-executor` on a `track-S2` worktree. The original worktrees stay untouched
as evidence until INT-6 removes them. The table below is the 2026-10-21 picture.

| Session | Branch and worktree | Doing | Its next boundary |
|---|---|---|---|
| "tC4 Rebuild" (Enjoy, `d8a954f9…`) | `resume-build-lane`, `.worktrees/resume-build-lane` | B11's records, then C7a, then C7b | after B11's records and one full gate |
| S2 owner (Enjoy, `19f61dcb…`) | `s2-proof-executor`, `.worktrees/s2-proof-executor` | S2's full validation at 3b075d7 | after validation and re-review |

INT-4 and S2's merge step say how their work reaches main.

## The push rule

The rule is written once, in `README.md` "Rules that never bend". In short: main may be pushed
after its full gates pass (INT-4 asks the owner first); every other branch on the owner's PC
stays there until H1 is answered; Track B's `ext/*` branches are pushed whenever the developer
commits; never push a token, a password or a machine-local path to a secret.

## Decisions I made (technical; recorded here, no owner question needed)

| Decision | Why |
|---|---|
| Git network calls live in `tc-door43`, behind the internet switch and its proxy finder; `tc-store` keeps `git2` for local history but without the `https` feature (added on the storage line in 61e2f7f) | AGENTS.md allows network only in `tc-door43` and `tc-resources`; STORAGE §9 already has `tc-door43` use the repository `tc-store` opened; a second network path would skip the switch and the PAC/WPAD finder (health-domain F3) |
| Do not merge the `storage` branch; re-apply P on post-S2 main | It is built on the old S2, unreviewed, and conflicts in 17 files (storage-remaining ST-3) |
| Merge S2 as it is, then simplify (S3) | S2's mutation recipes are anchored to its current lines; changing it before the merge throws its proof away (health-storage §10) |
| G1 no longer waits for D2 | Only `printPdf` needs D2, and the contract already names it (split-seams S2) |
| I no longer waits for X, except the team move (§13.3) | The importers need tc-journal, tc-usfm, tc-burrito and a store write, not tc-sync (split-seams S5) |
| T and X no longer wait for P | P measures budgets and re-applies speed-ups; the old chain ran T and X after P only because `storage-chain-B.js` ran its units in order. P's speed-ups follow S3's core so they are not built twice |
| Pin the toolchain to 1.99.1 in `rust-toolchain.toml` | A floating `stable` broke CI with new lints (checks-inventory C1) |
| The owner's rulings of 2026-10-21 change the storage design; the changes are written in `docs/STORAGE-DELTA-2026-10-21.md` and applied to `docs/STORAGE.md` when S2 merges | STORAGE.md is being edited on the S2 branch; a separate delta avoids a conflict and tells the S2 session what to stop proving |
| `.audit/*.tsv` merge with git's `union` driver (planned in INT-5 step 1; not in `.gitattributes` yet) | Append-only logs then never conflict between branches. Until INT-5 step 1 lands, resolve a conflict in one by keeping every row of both sides and the header once |
| No SQLite store before format 1.0 | `docs/STORAGE-DELTA-2026-10-21.md` §8 leaves it open. The file store is what S2 proved; a change now reworks S3, X, I and V and delays the freeze by weeks. It is out of scope unless the owner asks, and would be a new card after DONE (2026-10-22 review, risks R5 and cards F17) |
| The board (`board.py`) replaces STATUS.md and PROGRESS.md | Generated from cards and git; never committed as output; checks itself |

## What needs the owner

Each question is put to the owner in plain words with its effect on translators. The card that
needs the answer is blocked until a ruling file exists in `rulings/`: a whole card through
`needs_owner`, one step through the card's `later` field (`TRACKING.md`), which the board prints.
An answered row keeps only `Answered: rulings/<file>` here; the ruling file holds the words.

| ID | Question | Recommendation | Needed by (a card, or one step of it) |
|---|---|---|---|
| H1 | May unpushed branches go to the private GitHub repository (as a backup, and so CI runs on them)? | Answered yes: `rulings/2026-10-22-H1.md` | INT-1, INT-4 |
| H2 | Who is the second developer, and do they work in JavaScript or in Rust? Track B (shell and release) is built and tested on GitHub's Windows and macOS runners; a Mac helps only for G4 step 1 and debugging | Answered: Elias (`eliaspinero`), working through agents, in either language; he takes Track B (Package B); the hand-off waits for the owner's go: `rulings/2026-10-22-H2.md` | Track B start |
| H3 | Which platforms ship? tC4 4.0 ships Windows, macOS (Apple silicon) and Linux | Answered: `rulings/2026-10-24-H3-H4-H7-fast-release.md` | G3 |
| H4 | Signing: use unfoldingWord's Windows signing (as tC4 does), and leave macOS unsigned for now as tC4 rc.2 does? The Azure signing credential is bound to tC4's repository; someone with access must add one for this repository | Answered: `rulings/2026-10-24-H3-H4-H7-fast-release.md` (Yakob signs). The set-up, A4, is still open and REL-1 waits for it | G3 |
| H5 | Sign off the conflict-choice screens (Q-9), sent as one packet | Answered: `rulings/2026-10-24-H5.md` | C7b |
| H6 | Sign off the screens for Aquifer in the app. The owner already answered Q33 (Aquifer helps are reading only until tC4 settles its pin) and Q34 (the AI-linked Portuguese Bible as a labelled reading pane); the screens need a design | Parked with AQ1 until after 4.1.0-preview (`rulings/2026-10-24-H3-H4-H7-fast-release.md`) | AQ1 (from step 2) |
| H7 | Move the look-and-behaviour reference to tC4 `v4.0.0-rc.2` now, or wait for `v4.0.0`? It brings poem lines shown as tabs (#810) | Answered: `rulings/2026-10-24-H3-H4-H7-fast-release.md` | REF1 |
| H8 | ~~Storage scope cuts~~ | Answered: `rulings/2026-10-21-outside-edits.md` and `rulings/2026-10-21-carried-files.md` (applied through `docs/STORAGE-DELTA-2026-10-21.md`) | S2, S3, X, I, F1, D1, C7b, E2, V |
| H9 | Freeze project format 1.0 | Answered: `rulings/2026-10-27-H9.md` | V (step 8) |
| H10 | Who translates the 155 interface strings the rebuild added, in nine languages? | Answered: `rulings/2026-10-22-H10.md`. The translations landed in batch 48 (bb61001); five keys added since are pending in each of the nine languages, and REL-1 waits for them | G2 (step 4, before the Arabic captures), G3 (the `publish` job and its "Receive the translations" step), REL-1, DONE |
| H11 | Fetch the `keyring` crate for "Stay signed in", or let the shell keep the token? | Answered: `rulings/2026-10-25-H11.md` | E1a |
| H12 | In Check, does a Psalm verse whose words sit on poetry lines count as drafted? Home already says yes; Check says no | Answered: `rulings/2026-10-23-H12.md` ("poetry - yes, everywhere") | FIX-R |
| H13 | A project imported with no versification file: guess the scheme from its contents and say so, or use English as tC4 does? | Answered: `rulings/2026-10-27-H13.md` | I, VRS-GUESS |
| H14 | Opening a Door43 project under a new name: change only the card, or the project's own name too (#837)? | Answered: `rulings/2026-10-26-H14.md` | E1b |
| H15 | The help-desk key for "Report a problem" (tC4 sends it through SendGrid) | Use the same service; the owner provides the key to the release pipeline | G1 |
| H16 | The app's identity, which a translator sees and the tC4 team needs (`docs/TC4-REQUEST-MOVED-NOTICE.md`): its name, version number and version scheme, download page and application id; and where projects live. Facts from the owner (2026-10-22): tC4 went out as a soft beta, translationCore 4.0.0-rc2, build 2418892; the Rust build will be the first public release; the owner is weighing 4.1 against 5.0.0. The help pages the rebuild ships say projects are in `~/pankosmia/tc4-projects`, while the rebuild keeps them in `~/translationCore4/projects`; and should the first launch offer to bring in tC4 4.0 projects it finds in the old folder? | Partly answered, `rulings/2026-10-22-H16.md`: version 4.1.0 and semver (part 1); projects folder, help pages and the first-launch Import offer as recommended (part 2); the app is "translationCore", installed as "translationCore 4", download page https://translationcore.com/tc4 (part 3); the application id is open with `translationcore-4` recommended (part 4). If the tC4 team has not shipped the moved notice by F1's merge, stragglers keep seeing `sync.verify-failed`; F1's "Move the team" screen then tells the person to tell teammates by hand, and `docs/RELEASE.md` says so | F1 (its first-launch step), REF1 (its help-pages step), G3 |
| H17 | Are the speed budgets part of done? Six budget rows already miss on the fast build machine, and none is measured on the reference laptop yet | Answered: `rulings/2026-10-27-H17.md` | P, G2, DONE |
| H18 | Edits made with Door43's own tools (gatewayEdit, tcCreate, the web editor) on a team's project repository: merged into the project (today's design, STORAGE §11.2), or unsupported like a folder push from another tool (kept in Door43's history, a notice, not merged)? | Answered: `rulings/2026-10-22-H18.md` | X, I, S2, V |
| H19 | Carried files before translationCore uses them (delta Q3): keep S2's working code now, without its git backups, so OBS front matter and other extra files travel and conflict visibly; or park it until the feature is built | Answered: `rulings/2026-10-22-H19.md` | S2, S3, C7b, E2, X, V |
| H20 | "Update a book" with a file whose starting point the app cannot know (delta Q5): every changed verse waits in one review row with "Accept all from the file"; or the file's text simply wins (a teammate's newer verse could be replaced, kept in History); or the person picks which checkpoint the file came from | Answered: `rulings/2026-10-22-H20.md` | I (step 7b), F1 (step 8), V, S3 step 9 (d) |
| H21 | Alignment and check-decision files changed by another program (delta Q8): rewritten with a kept copy and a notice like other generated files; or asked like book files with Import | Answered: `rulings/2026-10-22-H21.md` | S2, S3 step 9 (d), D1 step 4, F1 |
| H22 | Smaller choices (delta Q2, Q4, Q6, Q7): "Bring it back" for lost work found only in a file the app wrote; the second file's name under Keep both; a changed file inside a USB exchange file; and seven small behaviours | Answered: `rulings/2026-10-22-H22.md` | C7b, E2, X, D1, S3 step 9 (d), V |

### Owner actions (no question, only something the owner must do)

| ID | Action | Why | Needed by |
|---|---|---|---|
| A1 | ~~Install MinGW and NASM on the Linux PC~~ Done: both are installed (`x86_64-w64-mingw32-gcc` and `nasm` on PATH, confirmed 2026-10-23) | This PC can run CI's Windows clippy locally | done |
| A2 | Optional: GitHub Pro, to make the CI check required before main changes | GitHub refuses required checks on a private repository on the free plan (HTTP 403). INT-3's merge receipt guards merges without it | none |
| A3 | Add `eliaspinero` (Elias, H2) as a collaborator on `birchamp/tc-rst` when the owner says the hand-off starts (not yet, 2026-10-22), and confirm GitHub Actions minutes for Windows and macOS runners | Add the second developer as a collaborator on `birchamp/tc-rst`, and confirm GitHub Actions minutes for Windows and macOS runners | Track B pushes `ext/*` branches and runs installers on those runners | Track B start, G3 |
| A4 | Create the GitHub environment `windows-signing` (deployment branch: the default branch only) and the repository secrets for the Windows signing identity (H4) and the help-desk key (H15); the secret names go into `docs/RELEASE.md` once H4 and H15 are answered. Creating environments and secrets needs repository admin, which a collaborator lacks (inference about GitHub, not cited) | G3's signed run after merge and G1's help-desk key read need them; nobody was assigned to create them | G1 step 7, G3 step 7, REL-1 step 3 |
| A5 | ~~Relicense the `string-punctuation-tokenizer` npm package to MIT~~ Not needed: version 2.2.0 is already MIT, and tC4 D111 (#838) proved its output equals 2.0.0's on 248,773 verse lines; `tc-usfm` cites 2.2.0 (LIC-1) | The rebuild's `tc-usfm` words module ports it | done |

## When this plan changes

Change this file only when the order, a dependency or a principle changes, and log the reason
in the change's commit message. A card's own steps can change in the card. A new owner ruling
goes in `rulings/`. Rerun the evaluation (`evidence/`) when a milestone ends: the integration lane
owns that rerun, and the board warns (`M<n> ended: rerun the evaluation`) until a newer
`evidence/<date>/` folder exists.
