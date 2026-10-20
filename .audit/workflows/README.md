# Workflow scripts

The orchestration scripts that build this repository, kept here so a cleanup of a scratch
folder cannot lose them. These scripts use the original Workflow tool, when available; current
agent delegation can follow the same stages when it is absent. A run id resumes cached work
only where that runner and its cache still exist. Durable stage reports and command receipts
live outside that session cache; see [`../PROCESS.md`](../PROCESS.md).

Adopt the state/gate tools and read-only review policy at the next completed-stage boundary.
Running sessions do not reload these scripts. Keep in-flight work running; do not start a
second copy to adopt an edited script. Give S2 a dedicated owner and preserve the complete
integration gates, safety properties and required platform checks.

| Script | What it does |
|---|---|
| `build-lane.js` | Builds remaining units from main, verifies once with independent lenses for no-loss units, fixes blockers and majors that harm translator data or the user, then serializes verified integration |
| `s2-placement-redesign.js` | S2's placement ledger, typed state machine and model-based crash/restart proof; priority on the delivery critical path |
| `tc4-delta-main.js` | Applies the main-side items of `docs/TC4-DELTA-2026-10-16.md` in four tracks |
| `journal-decision.js` | The journaling decision: tC4's method against `STORAGE.md`, benchmarked and red-teamed |
| `j2-finish.js` | Finishes unit J2 on `storage`: fold cache, hash once, SHA-256, O9 spec, measurements |
| `storage-chain-B.js` | Units C, S1, S2, P, X and I of `STORAGE.md` under the owner's option B |
| `domain-obs-lexicon.js` | The `tc-obs` crate and the lexicon repackaging, beside the storage chain |
| `app-layer-design.js` | Designs `tc-api`, `tc-app`, `tc-server` and the screen port; writes `docs/APP-DESIGN.md` |
| `app-phase0.js` | Phase 0 of the app layer: the Windows Electronite spike package (G0) and units A1 to A11, merged into `phase0` |

`../research/` holds the research notes the early units read (recovered on 2026-10-16 from the
agents' transcripts after a scratch folder was cleared).
