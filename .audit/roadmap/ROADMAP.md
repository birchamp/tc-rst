# Roadmap to done

Written 2026-10-21 from `ASSESSMENT.md`. The live state of each item is in its card in `units/`
and on the board (`python3 -I .audit/tools/board.py`). This file holds the order and the
reasons; it changes only when the plan changes.

## The goal

The rebuild is done when:

1. every statement P1 to P10 of `docs/PLAN.md` is true and is shown by a CI check that can fail;
2. the app installs and runs on the platforms the owner chooses (decision H3), built by a
   release pipeline;
3. every tC4 bug that the rebuild could repeat has a test that fails if it comes back;
4. every budget row of `docs/PERFORMANCE.md` is measured on the reference laptop in a release
   build and is met, or has a remedy the owner chose (decision H17). PLAN's P1 to P10 do not
   mention the budgets; AGENTS.md requires them (`evidence/2026-10-21/gap-performance-acceptance-ledger.md`).

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
3. **Build once, then reuse.** Where the remaining units would copy a pattern again, the shared
   structure is built first (M1). This is the Code layer of the funnel, and it gets the most
   effort.
4. **Every item fills in its funnel card before code** (`FUNNEL.md`).
5. **Every result lands in its card in the same commit** (`TRACKING.md`).
6. **Simplify before you extend.** Storage gets one simplification pass (S3) before sync adds
   new writers to it.

## Milestones

Sizes are inferences from comparable finished units (the basis is in each card). A "session" is
one agent session of a few hours.

### M0. Converge, and make CI trustworthy (about 2 to 4 days)

| ID | Item | Lane | Depends on | Size |
|---|---|---|---|---|
| INT-1 | Back up the unpushed branches off this PC | integration | owner H1 | under 1 h |
| INT-2 | Make CI green and trustworthy | checks | | 0.5 d |
| INT-3 | One gate list for CI and every local runner; merge only with a passing gate | checks | INT-2 | 0.5-1 d |
| INT-4 | Land `resume-build-lane` on main | integration | INT-2, INT-3, the resume lane at a stage boundary, owner H1 | 0.5 d plus gate time |
| S2 | Finish S2 and merge it | storage | INT-4 | 1-2 d |
| INT-5 | Converge the trackers onto this roadmap | integration | INT-4 | 0.5 d |
| INT-6 | Rescue stray work and free the disk | integration | INT-4, owner OK for deletions | 0.5 d |
| INT-7 | Prepare the Track B hand-off: freeze the shell contract | integration | INT-4 | 1 d |

**Exit:** main holds every built unit and S2; CI on main is green; the gate list is one script;
every unpushed branch has an off-PC copy; the board is the only tracker.

### M1. Build once, check always (about 1 to 2 weeks, lanes in parallel)

| ID | Item | Lane | Depends on | Size |
|---|---|---|---|---|
| CHK-1 | Architecture and I/O boundary checks | checks | INT-4 | 0.5-1 d |
| CHK-2 | Every written project is validated (P5) | checks | S2 | 1-2 d |
| CHK-3 | Claimed tests must exist (STORAGE §15, TC4-ISSUES, a P1 journey map, a P-item ledger) | checks | INT-4 | 1 d |
| CHK-4 | Deterministic and fast tests | checks | INT-4 | 2-3 d |
| CHK-5 | Nightly, Windows and no-network jobs; the mutation executor in the repo | checks | S2, INT-3 | 1-2 d |
| CHK-6 | Lint ratchet and fresh P7 captures | checks | INT-4 | 1 d |
| UI-1 | Typed jobs and one job follower | app | INT-4, C7a | 1 d |
| UI-2 | Ports for sync, share, exchange and import | app | UI-1 | 1-2 d |
| UI-3 | One leave path, one network-call helper, one refusal text, Submit, dialogs that own their forms, WriteCap enforced | app | UI-1 | 2 d |
| DOM-1 | One network gate: all network code behind `tc-door43` | domain | S2 | 1-2 d |
| DOM-2 | One reader per job and one set of identifiers | domain | S2 | 2-3 d |
| S3 | Simplify the store before sync | storage | S2, CHK-1 | core 1-1.5 weeks; all 2-3 weeks |
| FIX-R | Fix the tC4 bugs the rebuild repeats; reconcile TC4-ISSUES | app | INT-4 | 2-3 d |
| SWEEP | Sweep tC4 again (b86270f and release/4.0), then every week | integration | INT-4 | 0.5 d |

**Exit:** the checks in `FUNNEL.md` §"Checks the repository must have" all run in CI; the shared
structures exist and the remaining units' cards point at them; S3's core slices are merged.

### M2. Storage to done (the critical path)

| ID | Item | Depends on | Size |
|---|---|---|---|
| T | The storage side of the tC4 issue sweep (what is left of it) | S2 | 0.5-1 d |
| P | Scale: re-apply on post-S2 main; tooling first, speed-ups after S3's core | S2 | 2-3 d |
| X | Door43 sync and USB exchange (`tc-sync`). Steps 1-2 run beside S3; intake waits for S3's core | S2, DOM-1, T | 5-8 d |
| I | Import (`tc-import`). The pure core can start after DOM-2; the store write after S3's core; the team move after X | S2, DOM-2 | 3-5 d |
| V | Old projects keep working; the owner freezes format 1.0 | X, I, T, owner H9 | 1.5-3 d |

**Exit:** P2, P3, P4, P9 and P10 have their CI checks; the format is frozen.

### M3. App to done

| ID | Item | Depends on | Size |
|---|---|---|---|
| C7a | Conflict choices: text and passage | B11 (done) | 3-4 sessions |
| C7b | Conflict choices: every other kind | C7a, owner H5 (Q-9) | 5-7 sessions |
| D1 | Checkpoints and outside edits | S2, UI-3 | 5-6 sessions |
| D2 | Exports | S2, UI-1 | 4 sessions |
| E1a | Sync | X, C7a, D1, UI-2, UI-3 | 5-6 sessions |
| E1b | Share and open from Door43 | E1a | 3-4 sessions |
| E2 | USB exchange | X, D2, C7b, UI-2 | 4 sessions |
| F1 | Import | I, UI-2 | 4-5 sessions |
| AQ1 | Aquifer helps in the app (P6) | INT-4, owner H6 (Q34) | 3-5 sessions |
| REF1 | Move the tC4 reference and port its changes | SWEEP, owner H7 | 3-5 sessions |

**Exit:** every live tC4 journey (P1) and the team journeys (P2) run in CI; P6 is shown.

### M4. Shell and release (Track B, from now)

| ID | Item | Depends on | Size |
|---|---|---|---|
| G5 | Browser journeys in CI, with no skips allowed | INT-2 | 2 d |
| G1 | The Electronite shell | INT-7 | 20-30 commits |
| G3 | Release pipeline and installers | G1, owner H3, H4 | 1-2 weeks |
| G4 | The P8 Graphite probe and right-to-left project journeys | G1 | 2-3 d |

G5 is the most valuable check the project lacks. If Track B has not started it by the end of M0,
the checks lane takes it.

**Exit:** installers build and pass a smoke test on each chosen platform; P8 runs on each.

### M5. Done

| ID | Item | Depends on | Size |
|---|---|---|---|
| G2 | Budgets, the whole-Bible performance runs, Arabic captures of all 62 screens, the P7 review | P, every app unit, G5 | 10-15 commits |
| DONE | Release candidate: P1 to P10 green in CI, the owner's reviews recorded | everything | 1-2 d |

## The critical path

```
INT-2 ─ INT-3 ─ INT-4 ─ S2 ─┬─ S3 (core) ─┬─ X (intake onward) ─┬─ E1a ─ E1b ─┐
                            │             │                     ├─ E2 ───────┤
                            ├─ DOM-1 ─────┘ X steps 1-2         └─ I (team move)
                            ├─ T ─────────────────────────────────────── V ─┤
                            ├─ DOM-2 ─ I (core, store) ─ F1 ────────────────┤
                            ├─ D1, D2 ──────────────────────────────────────┤
                            └─ P ───────────────────────────────── G2 ─ DONE
Track B, in parallel:  G5 ─ G1 ─ G3 ─ G4 ───────────────────────────────────┘
```

The longest chain runs through storage: S2, S3's core, X, E1a and E1b, then G2. By inference
from the sizes above, about five to seven weeks of steady work. Usage limits and owner waits add
to it. Moving I's core to its own owner, or to Track B′ (`TRACK-B.md` §8), takes I off the chain.

## The two sessions running today

Two sessions were working when this plan was written. Nobody else touches their worktrees.

| Session | Branch and worktree | Doing | Its next boundary |
|---|---|---|---|
| "tC4 Rebuild" (Enjoy, `d8a954f9…`) | `resume-build-lane`, `.worktrees/resume-build-lane` | B11's records, then C7a, then C7b | after B11's records and one full gate |
| S2 owner (Enjoy, `19f61dcb…`) | `s2-proof-executor`, `.worktrees/s2-proof-executor` | S2's full validation at 3b075d7 | after validation and re-review |

At its next boundary each session: commits its loose state; reads this folder; claims its cards
(C7a for the first, S2 for the second) on its own branch; and continues by the card. INT-4 and
S2's merge step say how their work reaches main.

## The push rule until the owner answers H1

- Main may be pushed after its full gates pass. That is the standing authorization of
  2026-10-18. INT-4 still asks the owner first, because the resume lane's own instruction says
  "Do not push without the owner".
- Every other branch stays on this PC until H1 is answered.
- Never push a branch that holds a token, a password or a machine-local path to a secret.

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
| `.audit/*.tsv` merge with git's `union` driver | Append-only logs then never conflict between branches |
| The board (`board.py`) replaces STATUS.md and PROGRESS.md | Generated from cards and git; never committed as output; checks itself |

## What needs the owner

Each question is put to the owner in plain words with its effect on translators. The card that
needs the answer is blocked until a ruling file exists in `rulings/`.

| ID | Question | Recommendation | Needed by (a card, or one step of it) |
|---|---|---|---|
| H1 | May unpushed branches go to the private GitHub repository (as a backup, and so CI runs on them)? | Yes: push each integration branch under its own name; main only after gates | INT-1, INT-4 |
| H2 | Who is the second developer, and do they have a Mac and Windows? | Mac and Windows: Track B (shell and release). Rust only: Track B′ (import core) | Track B start |
| H3 | Which platforms ship? tC4 4.0 ships Windows, macOS (Apple silicon) and Linux | The same three as tC4 | G3 |
| H4 | Signing: use unfoldingWord's Windows signing (as tC4 does), and leave macOS unsigned for now as tC4 rc.2 does? The Azure signing credential is bound to tC4's repository; someone with access must add one for this repository | Yes to both; the owner names who adds the credential | G3 |
| H5 | Sign off the conflict-choice screens (Q-9), sent as one packet | Approve the packet C7b step 2 sends | C7b |
| H6 | Sign off the screens for Aquifer in the app. The owner already answered Q33 (Aquifer helps are reading only until tC4 settles its pin) and Q34 (the AI-linked Portuguese Bible as a labelled reading pane); the screens need a design | Approve the packet AQ1 step 1 sends | AQ1 (from step 2) |
| H7 | Move the look-and-behaviour reference to tC4 `v4.0.0-rc.2` now, or wait for `v4.0.0`? It brings poem lines shown as tabs (#810) | Move to rc.2 now, keep showing markers for poem lines, and move again at v4.0.0 | REF1 |
| H8 | ~~Storage scope cuts~~ Answered 2026-10-21 by two rulings: `rulings/2026-10-21-outside-edits.md` (a changed book file is kept and the app asks "This book has been changed externally. Would you like to import it into this project?"; other generated files are rewritten with a kept copy; folder pushes to Door43 from other tools are unsupported) and `rulings/2026-10-21-carried-files.md` (each version records the version it replaced and its Door43 user; no old bytes kept; conflicts keep both until a choice). (b), a disk change that keeps size and time, follows from rule 1: the app rewrites the file from its record | Done | S2, S3, X, I, F1, D1, C7b, E2, V (see `docs/STORAGE-DELTA-2026-10-21.md`) |
| H9 | Freeze project format 1.0 | Freeze when T, X and I have landed and V's sample projects pass | V |
| H10 | Who translates the 155 interface strings the rebuild added, in nine languages? | Name a translator or service; until then the release build refuses untranslated strings | G2, DONE |
| H11 | Fetch the `keyring` crate for "Stay signed in", or let the shell keep the token? | Fetch `keyring` | E1a |
| H12 | In Check, does a Psalm verse whose words sit on poetry lines count as drafted? Home already says yes; Check says no | Yes, everywhere | FIX-R |
| H13 | A project imported with no versification file: guess the scheme from its contents and say so, or use English as tC4 does? | Guess, and say which scheme was chosen | I |
| H14 | Opening a Door43 project under a new name: change only the card, or the project's own name too (#837)? | Only the card, as tC4 does today | E1b |
| H15 | The help-desk key for "Report a problem" (tC4 sends it through SendGrid) | Use the same service; the owner provides the key to the release pipeline | G1 |
| H16 | The app's identity, which a translator sees and the tC4 team needs (`docs/TC4-REQUEST-MOVED-NOTICE.md`): its name, version number, download page and application id; and where projects live. The help pages the rebuild ships say projects are in `~/pankosmia/tc4-projects`, while the rebuild keeps them in `~/translationCore4/projects`; and should the first launch offer to bring in tC4 4.0 projects it finds in the old folder? | Decide the name and page with the tC4 team; correct the help pages to the real folder; at first launch, list tC4 4.0 projects found in the old folder and offer Import for each | G3, F1 |
| H17 | Are the speed budgets part of done? Six budget rows already miss on the fast build machine, and none is measured on the reference laptop yet | Yes: every row measured on the reference laptop in a release build, met or with a remedy the owner chose | P, G2, DONE |
| H18 | Edits made with Door43's own tools (gatewayEdit, tcCreate, the web editor) on a team's project repository: merged into the project (today's design, STORAGE §11.2), or unsupported like a folder push from another tool (kept in Door43's history, a notice, not merged)? | Answered: `rulings/2026-10-22-H18.md: unsupported, with a notice` | X |
| H19 | Carried files before translationCore uses them (delta Q3): keep S2's working code now, without its git backups, so OBS front matter and other extra files travel and conflict visibly; or park it until the feature is built | Answered: `rulings/2026-10-22-H19.md: keep S2's carried code now, without its git backups` | S2, C7b |
| H20 | "Update a book" with a file whose starting point the app cannot know (delta Q5): every changed verse waits in one review row with "Accept all from the file"; or the file's text simply wins (a teammate's newer verse could be replaced, kept in History); or the person picks which checkpoint the file came from | Answered: `rulings/2026-10-22-H20.md: one review row with "Accept all", no conflicts for empty verses` | I, F1 |
| H21 | Alignment and check-decision files changed by another program (delta Q8): rewritten with a kept copy and a notice like other generated files; or asked like book files with Import | Answered: `rulings/2026-10-22-H21.md: rewritten with a kept copy` | S2, F1 |
| H22 | Smaller choices (delta Q2, Q4, Q6, Q7): "Bring it back" for lost work found only in a file the app wrote; the second file's name under Keep both (`jon_01-Juan.wav`, since paths cannot hold spaces); a changed file inside a USB exchange file; and seven small behaviours | Answered: `rulings/2026-10-22-H22.md: "Bring it back" asked once; `jon_01-Juan.wav`; exchange-file edits get a notice; the small behaviours` | S2, X, E2 |

### Owner actions (no question, only something the owner must do)

| ID | Action | Why | Needed by |
|---|---|---|---|
| A1 | Run once on the Linux PC: `sudo apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 nasm` | Lets this PC run CI's Windows clippy, so a local gate pass means a CI pass (INT-3's receipts) | INT-2 step 7, INT-3 step 5 |
| A2 | Optional: GitHub Pro, to make the CI check required before main changes | GitHub refuses required checks on a private repository on the free plan (HTTP 403). INT-3's merge receipt guards merges without it | none |
| A3 | Add the second developer as a collaborator on `birchamp/tc-rst`, and confirm GitHub Actions minutes for Windows and macOS runners | Track B pushes `ext/*` branches and runs installers on those runners | Track B start, G3 |

## When this plan changes

Change this file only when the order, a dependency or a principle changes, and log the reason
in the change's commit message. A card's own steps can change in the card. A new owner ruling
goes in `rulings/`. Rerun the evaluation (`evidence/`) when a milestone ends.
