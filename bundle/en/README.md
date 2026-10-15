# The bundled English suite

English is always installed as the fallback language set, so the app ships these
unfoldingWord releases and works offline from the first run. `tc-resources` holds the same
pins in `suite::bundled_pins()`, ported from tC4's `src/data/installedSuite.js`. The
benchmarks and the scale corpus (`docs/PERFORMANCE.md`) also read these files.

Each zip is a Door43 archive of the pinned commit, byte for byte. Its zip comment is that
commit, and it equals the tC4 pin. The project owner supplied the files on 2026-10-15.

| Pin (tC4 `installedSuite.js`) | Repository | Version | Commit | File | sha256 |
|---|---|---|---|---|---|
| `originalLanguage.ot` | `unfoldingWord/hbo_uhb` | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` | `hbo_uhb-v3.0.0.zip` | `c4abce7cad1532d9481b289fc3819d8ab0ca01ef41899e205117cd3dc8cac8b5` |
| `originalLanguage.nt` | `unfoldingWord/el-x-koine_ugnt` | v0.34 | `fc95b2b8aad08bb65ab54628ab685413a1139e97` | `el-x-koine_ugnt-v0.34.zip` | `07318be27d0027146a62363c20ffedd0db695d7680f4b32c8ae8bd3020a29d00` |
| `extraScripture` `ult` | `unfoldingWord/en_ult` | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | `en_ult-v91.zip` | `f5c08f7a50ef88afe7b4811acdee352b0302e9f0bf1269f217d97121d2113d44` |
| `extraScripture` `ust`, `simplifiedText` | `unfoldingWord/en_ust` | v91 | `85f274a74245cb418f85266e1a5b524bc3e91e9c` | `en_ust-v91.zip` | `0c7c3f9747f58a08a9901d1acfc26cd1356aebdb161671a3becfc403dca3d28c` |
| `translationNotes` | `unfoldingWord/en_tn` | v91 | `e586762e330f482a60c52aedd1c7b3a2f155df8a` | `en_tn-v91.zip` | `45fed63b7e11594be6c6de288832be97285d9d11995cc968a0960be574ad4855` |
| `translationWordsLinks` | `unfoldingWord/en_twl` | v91 | `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64` | `en_twl-v91.zip` | `6b4979dc684fde52298f9a60e40281690eec148024558b2a8a012257b315f437` |
| `translationWords` | `unfoldingWord/en_tw` | v91 | `ff5b3852c27c3a0d01b109e482eb26047dcd20e2` | `en_tw-v91.zip` | `78efd237fc9868ef44500305f3005b472111fde641af7cdff44cdcb10b1a2318` |
| `translationAcademy` | `unfoldingWord/en_ta` | v91 | `ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd` | `en_ta-v91.zip` | `cd19a889437c70fabb1db4c9b87afebed463d872549edb91e14f26ac30c82a62` |

## Still missing

| Pin | Repository | Version | Commit |
|---|---|---|---|
| `translationQuestions` | `unfoldingWord/en_tq` | v91 | `8be02772584ff5a5fea893a392b4f019e3efcc77` |
| `obs` | `unfoldingWord/en_obs` | v9 | `d39a1dc7a7557ac54e4a8fecc3462147fe7eec3b` |
| `obs-tn` | `unfoldingWord/en_obs-tn` | v13 | `e86138ea13f619f09f7a6dcaa60592716d407fe4` |
| `obs-twl` | `unfoldingWord/en_obs-twl` | v3 | `44ebc9fafe8101665f985007d566f5036a2be85b` |
| `obs-tq` | `unfoldingWord/en_obs-tq` | v10 | `01b92fe8793d62cff3a2221f5174c768cbad3dc1` |
| `lexicon.nt` | `uW/en_ugl` | none (commit only) | `d9d29e2d589258ce27f92b59f753a3af03ab7a72` |
| `lexicon.ot` | `uW/en_uhl` | none (commit only) | `72df5ac25acf9d51e826b20e3ad883a5a657ef4e` |
| OBS images | `uW/obs_images_360` | none (commit only) | `7146d5b504f6b63b9e11f7dc0b18c594d0ae179d` |

Licenses: the unfoldingWord resources are CC BY-SA 4.0, © unfoldingWord.
