# The bundled English suite

English is always installed as the fallback language set, so the app ships these
unfoldingWord releases and works offline from the first run. `tc-resources` holds the same
pins in `suite::bundled_pins()`, ported from tC4's `src/data/installedSuite.js`. The
benchmarks and the scale corpus (`docs/PERFORMANCE.md`) also read these files.

## Shipped: the DCS Scripture Burrito export of each pinned tag

As in tC4, a pin with a version tag ships as the DCS burrito export of that tag. An export is
installed only when its `metadata.json` names the pinned commit as its DCS revision and every
ingredient matches its md5 and size. `SHA256SUMS` lists each file. The project owner
downloaded the exports on 2026-10-15 (the timestamp in each `metadata.json`), and every one
passed those checks against the pins below.

| Pin (tC4 `installedSuite.js`) | Repository | Version | Commit | File | Ingredients | sha256 |
|---|---|---|---|---|---:|---|
| `originalLanguage.ot` | `unfoldingWord/hbo_uhb` | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` | `hbo_uhb_v3.0.0.sb.zip` | 40 | `7cc625318984213db671eab867ca115d1281e25c0a17b711ae155865d559af45` |
| `originalLanguage.nt` | `unfoldingWord/el-x-koine_ugnt` | v0.34 | `fc95b2b8aad08bb65ab54628ab685413a1139e97` | `el-x-koine_ugnt_v0.34.sb.zip` | 28 | `748b83459d8ffbb59fbc4cd6d956f37e0f4beab633031e37e21389c55859a549` |
| `extraScripture` `ult` | `unfoldingWord/en_ult` | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | `en_ult_v91.sb.zip` | 61 | `6a1989e89cd3f152f5cf6a1ad5866abda74ea90d0e8478cd3a5f742979e52e9f` |
| `extraScripture` `ust`, `simplifiedText` | `unfoldingWord/en_ust` | v91 | `85f274a74245cb418f85266e1a5b524bc3e91e9c` | `en_ust_v91.sb.zip` | 60 | `71b9c711428a0cebc15fb1dae35dfdb02715c9a38d71d489efe99df6d91445c8` |
| `translationNotes` | `unfoldingWord/en_tn` | v91 | `e586762e330f482a60c52aedd1c7b3a2f155df8a` | `en_tn_v91.sb.zip` | 60 | `53943fdfa3d87f59b2810a61693f5df7703b89bc3a79879bd2fbb8c39075d92c` |
| `translationWordsLinks` | `unfoldingWord/en_twl` | v91 | `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64` | `en_twl_v91.sb.zip` | 1014 | `2fbd8e56ab64d412383af8ba3ff347f044e28305753a72d0a292133e4499ff5c` |
| `translationWords` | `unfoldingWord/en_tw` | v91 | `ff5b3852c27c3a0d01b109e482eb26047dcd20e2` | `en_tw_v91.sb.zip` | 1021 | `7e5fa483c5c9783b7f85d7107c8bec8f6ae9bd2cb7736287d5bfce409e0d6577` |
| `translationAcademy` | `unfoldingWord/en_ta` | v91 | `ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd` | `en_ta_v91.sb.zip` | 735 | `d22c6cbcb6466ebd048f3c04c1f8fbde191baa212b665ca71ccedec0802cd032` |
| `translationQuestions` | `unfoldingWord/en_tq` | v91 | `8be02772584ff5a5fea893a392b4f019e3efcc77` | `en_tq_v91.sb.zip` | 60 | `010096509ce29cf7658b0d68f1e94ac6e8168b2341b7d4509eac390241ad60eb` |
| `obs` | `unfoldingWord/en_obs` | v9 | `d39a1dc7a7557ac54e4a8fecc3462147fe7eec3b` | `en_obs_v9.sb.zip` | 53 | `e447e239c1bfc0f7a915bbf69120f0ffcb290dd4ec44ac42d927e1ef328540dc` |
| `obs-tn` | `unfoldingWord/en_obs-tn` | v13 | `e86138ea13f619f09f7a6dcaa60592716d407fe4` | `en_obs-tn_v13.sb.zip` | 2 | `ccdb79cf2c4b9fd1c95460d182f82f3116ca29d5ae02b192b33dcc74dfd79f4c` |
| `obs-twl` | `unfoldingWord/en_obs-twl` | v3 | `44ebc9fafe8101665f985007d566f5036a2be85b` | `en_obs-twl_v3.sb.zip` | 2 | `fba8f254438f41ce775500343b3260f043749743c984460089abb84dc1fb375c` |
| `obs-tq` | `unfoldingWord/en_obs-tq` | v10 | `01b92fe8793d62cff3a2221f5174c768cbad3dc1` | `en_obs-tq_v10.sb.zip` | 2 | `c5a96d0b4c877954d55042f7b4b21ccad45edb174d32bfccc5f9584e4e99b6e5` |

Each export also holds a few files its metadata does not list (`README.md`, `LICENSE.md`,
`.gitignore`, `.github/…`, `.gitea/…`). Nothing verified them, so the installer leaves them
out (`tc_door43::VerifiedSbExport::unlisted`).

The TWL export differs from the repository it comes from: DCS rewrites each `TWLink` from
`rc://*/tw/dict/bible/<category>/<slug>` to `./payload/<category>/<slug>.md`, and the export
carries the linked tW articles under `ingredients/payload/`. `tc-helps` reads both forms.
The other TSV and USFM files are the same bytes in the export and in the repository.

## Plain archives kept for the scale corpus

`hbo_uhb-v3.0.0.zip`, `el-x-koine_ugnt-v0.34.zip`, `en_ult-v91.zip`, `en_ust-v91.zip`,
`en_tn-v91.zip`, `en_twl-v91.zip`, `en_tw-v91.zip` and `en_ta-v91.zip` are Door43 git
archives of the same commits: each zip comment is the commit. They are not shipped. The
scale corpus generator (`tc-corpus`) reads them until it reads the exports instead, and then
they are deleted.

## Still missing

The two lexicons have no tag, so they ship as the git archive of their commit, as in tC4.

| Pin | Repository | Commit |
|---|---|---|
| `lexicon.nt` | `uW/en_ugl` | `d9d29e2d589258ce27f92b59f753a3af03ab7a72` |
| `lexicon.ot` | `uW/en_uhl` | `72df5ac25acf9d51e826b20e3ad883a5a657ef4e` |

## Fetched when the app is packaged

The OBS image pack is too large to keep in git, so the packaging step downloads it. The
person who installs the app never downloads it: the installer carries it, and the first run
seeds it into the resource store like every file above.

| Pin | Repository | Commit |
|---|---|---|
| OBS images (`EN_OBS_IMAGES` in tC4) | `uW/obs_images_360` | `7146d5b504f6b63b9e11f7dc0b18c594d0ae179d` |

- The packaging step downloads the archive of exactly that commit. The release pipeline
  runs it, because this build environment cannot reach Door43.
- The first download records the archive's sha256 and size in `fetched.lock` in this
  directory, and that file is committed. Every later download must match it, or the build
  fails. A changed pin means a new commit and a new lock entry, made on purpose.
- A build whose installer lacks a pinned file fails. The app never falls back to
  downloading a bundled file.

Licenses: the unfoldingWord resources are CC BY-SA 4.0, © unfoldingWord.
