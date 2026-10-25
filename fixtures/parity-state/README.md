# The parity scenarios' projects

`parity/tc4-baseline/fixtures/build-state.mjs` builds three scenarios. Each one copies the two
sample projects (`../tc4/sample-burrito` as `sample_burrito`, `../tc4/sample-burrito-obs` as
`sample_burrito_obs`) into `repos/_local_/_local_/` and then changes a few files:

| Scenario | Files here | What `build-state.mjs` did |
|---|---|---|
| `sample` | none | Nothing: the projects are the samples byte for byte. |
| `en-pins` | `en-pins/sample_burrito/ingredients/checking/{resources.json, translationNotes/TIT.json, translationWords/TIT.json}`, `en-pins/sample_burrito_obs/ingredients/checking/resources.json`, `en-pins/client_settings/uw-tc4.json`, `en-pins/repos/_local_/_sideloaded_/hbo_uhb/metadata.json` | `pinEnglishSuite` and `pinEnglishObsSuite`: both rungs pin the installed English suite, and the decision files record the English pins. The Hebrew Bible's Jonah (`hbo_uhb`, G4) is installed in this scenario only, so its install records and its burrito metadata differ from `common/`'s; Jonah's text is `../tc4/test-fixtures/hbo_uhb/JON.usfm`. Copied on 2026-10-24 from `--scenario en-pins`. |
| `shared` | `shared/sample_burrito/.remotes.json`, `shared/sample_burrito_obs/.remotes.json` | `shareOnDoor43`: the mock server's remote list of a shared project. |
| all three | `common/client_settings/uw-tc4.json`, `common/repos/_local_/_sideloaded_/{el-x-koine_ugnt,en_obs,en_ta,en_tw,en_ult,en_ust}/metadata.json` | The install records of the twelve sideloaded resources, and the six burrito metadata files whose md5s and sizes `build-state.mjs` writes for the vendored files it puts beside them. No other committed file has their bytes, and they are the same in every scenario. |

A scenario's project is the sample with the files here laid over it. The files were copied
from the output of `node parity/app/state.mjs --scenario <name> --out <dir>` on 2026-10-18
(`common/` on 2026-10-19, from `--scenario sample`, for unit B1b), never written by hand. Their
content's licence is that of `../tc4/LICENSE-CONTENT.md`.

`tc_app::backend::scenario::ScenarioState::from_repository` assembles a whole scenario state for
the memory backend without node: it finds each file `parity/app/state.sha256.json` records by its
sha256 among `../tc4`, this folder and `parity/tc4-baseline/fixtures/vendor`. Every other file of
a state (the projects, the vrs files and 173 sideloaded resource files) is byte-identical to one already there;
`provenance.json` is left out, since it names the build, not the state.
`crates/tc-app/tests/app/scenario.rs` (`every_scenario_is_assembled_from_committed_bytes`) checks
every assembled file against the record, both ways. `crates/tc-app/tests/app/state_seed.rs`
(`scenario_projects_are_the_recorded_state_bytes`) checks every file of every assembled project,
both ways, against `parity/app/state.sha256.json`, the sha256 of each file as built from tC4 at
`d310bf3`, so these copies cannot drift from what `build-state.mjs` makes.
