# Workflow scripts

The orchestration scripts that build this repository, kept here so a cleanup of a scratch
folder cannot lose them. Each runs with the Workflow tool; a run can resume from its run id.

| Script | What it does |
|---|---|
| `tc4-delta-main.js` | Applies the main-side items of `docs/TC4-DELTA-2026-10-16.md` in four tracks |
| `journal-decision.js` | The journaling decision: tC4's method against `STORAGE.md`, benchmarked and red-teamed |
| `j2-finish.js` | Finishes unit J2 on `storage`: fold cache, hash once, SHA-256, O9 spec, measurements |
| `storage-chain-B.js` | Units C, S1, S2, P, X and I of `STORAGE.md` under the owner's option B |
| `domain-obs-lexicon.js` | The `tc-obs` crate and the lexicon repackaging, beside the storage chain |
| `app-layer-design.js` | Designs `tc-api`, `tc-app`, `tc-server` and the screen port; writes `docs/APP-DESIGN.md` |
| `app-phase0.js` | Phase 0 of the app layer: the Windows Electronite spike package (G0) and units A1 to A11, merged into `phase0` |
| `merge-main.js` | Merges one branch or `ready/<unit>` ref into main in `.claude/worktrees/merge-main` with the full gates; branches no chain reviewed (the Windows session's) get an independent review and fixes first; the main session pushes |
| `app-layer.js` | Phases 1 to 6 of the app layer: each unit starts once its dependencies are on main or merged into the `app` branch; verified (three lenses for no-loss units), fixed, merged into `app`, and marked `ready/<unit>` for main |
| `tc4-issue-sweep.js` | Maps every tC4 commit after the reference and every open or recent tC4 issue onto the rebuild (covered with its test, same bug, a later unit, not applicable, owner question), verified by a skeptic per area; rerun with a newer `tip` to sweep again |
| `verify-loop.js` | Checks a unit on its own branch until an independent checker finds no blocker or major (check, fix, up to three rounds), then the gates; used when a chain's own rounds ran out |
| `merge-queue.js` | Merges several refs into main one after another through `merge-main.js`, pushing each as a fast-forward once its gates are green; stops at the first that is not |

`../research/` holds the research notes the early units read (recovered on 2026-10-16 from the
agents' transcripts after a scratch folder was cleared).
