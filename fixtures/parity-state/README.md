# The parity scenarios' projects

`parity/tc4-baseline/fixtures/build-state.mjs` builds three scenarios. Each one copies the two
sample projects (`../tc4/sample-burrito` as `sample_burrito`, `../tc4/sample-burrito-obs` as
`sample_burrito_obs`) into `repos/_local_/_local_/` and then changes a few files:

| Scenario | Files here | What `build-state.mjs` did |
|---|---|---|
| `sample` | none | Nothing: the projects are the samples byte for byte. |
| `en-pins` | `en-pins/sample_burrito/ingredients/checking/{resources.json, translationNotes/TIT.json, translationWords/TIT.json}`, `en-pins/sample_burrito_obs/ingredients/checking/resources.json` | `pinEnglishSuite` and `pinEnglishObsSuite`: both rungs pin the installed English suite, and the decision files record the English pins. |
| `shared` | `shared/sample_burrito/.remotes.json`, `shared/sample_burrito_obs/.remotes.json` | `shareOnDoor43`: the mock server's remote list of a shared project. |

A scenario's project is the sample with the files here laid over it. The files were copied
from the output of `node parity/app/state.mjs --scenario <name> --out <dir>` on 2026-10-18,
never written by hand. `crates/tc-app/tests/app/state_seed.rs`
(`scenario_projects_are_the_recorded_state_bytes`) checks every file of every assembled project,
both ways, against `parity/app/state.sha256.json`, the sha256 of each file as built from tC4 at
`d310bf3`, so these copies cannot drift from what `build-state.mjs` makes.
