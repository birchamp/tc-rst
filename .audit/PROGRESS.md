# Progress toward done

The goal is `docs/PLAN.md`, "Definition of done": the rebuild is done when P1 to P10 are each
shown by a CI check that can fail. The units below build toward it. The orchestrating session
updates this file at every merge into `claude/focused-ride-5uzj7p`; `.audit/rebuild.tsv` holds the
detail of each decision and merge.

States: **done** (committed in the reported ref), **running**, **next** (its dependencies are met or nearly),
**waits** (for the units named).

2026-10-20 continuation: main has 29/48 app units. The isolated `resume-build-lane` branch has
34/48 after fully gated B5b, B4b, B7, C5 and C2 work; no push or main fast-forward has occurred.
The app table below describes this integration branch. The final gate passes all required
formatting/workspace/release/wasm checks and 3,243 Rust tests (7 ignored); C2 passes 50 distinct browser journeys and three strict
Psalms/10× count rows. C5’s 13 alignment journeys remain verified. C2 production durable
API save p95 is 17.904/18.331 ms locally; the Windows reference measurement remains open. Section pin-loss and journal test-oracle
counterexamples found by the gate are fixed in separate commits. See
`.audit/plans/RESUME-2026-10-20.md`; preserve completed checks rather than restarting them.

## Storage (`docs/STORAGE.md`, run `storage-chain-B.js`, branch `storage`)

| Unit | What | State |
|---|---|---|
| J1, J2 | tc-journal: model, codecs, fold, conflicts | done |
| C | converge with main | done |
| S1 | tc-store: store I/O, open, saves | done |
| S2 | materialize off the actor, Git, held-file retries, outside edits | redesign as a placement ledger and a typed state machine with a model-based property test (`s2-placement-redesign.js`), then main |
| P | scale against the reference laptop | about 23 commits built on `storage`; continues when the chain restarts |
| T | the storage side of the tC4 issue sweep | waits: P |
| X | Door43 sync and USB exchange | waits: P |
| I | tC4 and tC3 import | waits: X |
| V | the CI gate that keeps old projects working (PLAN P10) | waits: I |

## App layer (`docs/APP-DESIGN.md` §10)

Every remaining unit runs in `build-lane.js`: built from main, one verification pass, fixes only for
blockers and majors (minors in `.audit/minors.tsv`), then merged straight into main with one gate run.
The Windows VM runs the full gates on each new main (`.audit/tools/winvm/`).

| Phase | Units done | Running | Waiting |
|---|---|---|---|
| 0 | all: A1 to A11 and G0 | | |
| 1 | B1a, B1b, B2, B3, B4a, B4b, B5a, B5b, B6, B7, B9a, B9b, B10 | | B8, B11 next |
| 2 | C0, C1a, C1b, C2, C5, C6, C8 | | C3, C4 next; C7 after B11 and its questions |
| 3 | | | D1 (S2, C2, C5), D2 (S2, B8) |
| 4 | | | E1a, E1b, E2 (X and their units) |
| 5 | | | F1 (I, C1a) |
| 6 | | | G1 (shell), G2 (budgets and the P7 report: P and every unit) |

## Windows test bed (`.audit/tasks/windows-track.md`, the laptop session "Part 3 complete")

| Task | State |
|---|---|
| 1 to 4, 6 (build, Door43 facts, G0, hazards) | done and merged |
| 5: memory and startup of Word, Edge and tC4 (Paratext needs a registration) | done: tC4 709 MiB; Home 2.7 s warm, 13.2 s cold |
| 7: rerun at main; hazard 6 | done; hazard 6 built in the VM and merged |
| Builds and tests | moved to the Windows VM on the Linux PC; the laptop only measures |
| Door43's push refusal in tc-door43-fake and the client | merged |

## tC4 issue sweep

Sweep fixes on main: all six (FIX-helps, FIX-misc, FIX-text, FIX-align, FIX-quit, FIX-proxy).

Every few days, and before a unit's verification when tC4 has moved, rerun
`tc4-issue-sweep.js` with tC4's newest `main` as `tip`. Its results live in
`docs/TC4-ISSUES.md` (status of every tC4 issue in the rebuild) and the newest
`docs/TC4-DELTA-*.md`. First sweep: 2026-10-19, tC4 `d310bf3` to `9ad4d5c`.

## What needs the owner

- Freezing the project format 1.0 (unit V cuts the capsules once it is frozen).
- Q-9: signing off the conflict-choice designs before C7b compares its screens.
- G2: a person reviews the P7 screenshot differences.
- Any OWNER QUESTION a builder reports, in plain words with a recommendation.

## Restarting after an interruption

Workflow caches do not survive a new session. Restart each run with the units already on main
(see `.audit/HANDOFF.md` for the paths and the storage chain's `skip` and `cont`):

```js
Workflow({ scriptPath: '.audit/workflows/app-phase0.js', args: { root, tc4, onMain: [...], cont: [...] } })
Workflow({ scriptPath: '.audit/workflows/app-layer.js', args: { root, tc4, onMain: [...units and storage units on main], cont: [...] } })
```
