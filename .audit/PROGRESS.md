# Progress toward done

The goal is `docs/PLAN.md`, "Definition of done": the rebuild is done when P1 to P10 are each
shown by a CI check that can fail. The units below build toward it. The orchestrating session
updates this file at every merge into `claude/focused-ride-5uzj7p`; `.audit/rebuild.tsv` holds the
detail of each decision and merge.

States: **done** (merged into main), **running**, **next** (its dependencies are met or nearly),
**waits** (for the units named).

## Storage (`docs/STORAGE.md`, run `storage-chain-B.js`, branch `storage`)

| Unit | What | State |
|---|---|---|
| J1, J2 | tc-journal: model, codecs, fold, conflicts | done |
| C | converge with main | done |
| S1 | tc-store: store I/O, open, saves | done |
| S2 | materialize off the actor, Git, held-file retries, outside edits | gated; extra verify loop running on `s2-verify` before main |
| P | scale against the reference laptop | about 23 commits built on `storage`; continues when the chain restarts |
| T | the storage side of the tC4 issue sweep | waits: P |
| X | Door43 sync and USB exchange | waits: P |
| I | tC4 and tC3 import | waits: X |
| V | the CI gate that keeps old projects working (PLAN P10) | waits: I |

## App layer (`docs/APP-DESIGN.md` §10)

Phase 0 runs in `app-phase0.js` (integration branch `phase0`); phases 1 to 6 run in
`app-layer.js` (integration branch `app`). Both mark each merged unit with `ready/<unit>`, and the
orchestrating session merges it into main with `merge-main.js`.

| Phase | Units done | Running | Waiting |
|---|---|---|---|
| 0 | A1, A2, A3, A4a, A4b, A6a, A6b, A7, A9, A10, A11, G0 (run on Windows) | A5 | A8 (A5) |
| 1 | B1a, B1b | | B2 (A5); B3 to B11 after B2 |
| 2 | | C0 | C1a, C1b, then C2 to C8 with their B units |
| 3 | | | D1 (S2, C2, C5), D2 (S2, B8) |
| 4 | | | E1a, E1b, E2 (X and their units) |
| 5 | | | F1 (I, C1a) |
| 6 | | | G1 (shell), G2 (budgets and the P7 report: P and every unit) |

## Windows test bed (`.audit/tasks/windows-track.md`, the laptop session "Part 3 complete")

| Task | State |
|---|---|
| 1 to 4, 6 (build, Door43 facts, G0, hazards) | done and merged |
| 5: memory of Word, a browser, tC4 and Paratext against our 1 GiB and 400 MiB limits | running (`windows-memory`) |
| 7: rerun at main; hazard 6's PAC and WPAD lookup | waits: task 5 (`windows-proxy`) |
| Door43's push refusal in tc-door43-fake and the client | running (`windows-push-refusal`) |

## tC4 issue sweep

Fix run `tc4fix` (app-layer.js with a unit list): FIX-text, FIX-align, FIX-helps, FIX-misc, running.

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
