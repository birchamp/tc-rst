# Test fixtures — provenance

These files are pinned copies. Do not edit them. Tests read them read-only.

| File | Source | Version | Commit (revision in the export's metadata.json) | How fetched | Date |
|---|---|---|---|---|---|
| `en_ult/TIT.usfm` | unfoldingWord en_ult (aligned USFM) | v89 | `84c73ba00fc8a95a9033f9efb14bb905a2a52ee4` | DCS sb-zip export (`/sb/<tag>.zip`) | 2026-07-30 |
| `en_ust/TIT.usfm` | unfoldingWord en_ust (aligned USFM) | v89 | `37ec223166bbd73fb55abc7840be8310c0fee7f2` | DCS sb-zip export (`/sb/<tag>.zip`) | 2026-07-30 |
| `hbo_uhb/JON.usfm` | unfoldingWord hbo_uhb (Hebrew, right-to-left; `\w` tokens with lemma/strong/morph, U+2060 word joiners inside words, maqaf U+05BE between words) | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` (the tag's commit) | raw file `32-JON.usfm` at the tag; sha256 `55a36ef7ab573305e6281d5ae28223fe525e478f7e12e946b64099f6b646ca7b` | 2026-09-12 |

`test/align-tokenize.test.ts` (issue #255) reads `hbo_uhb/JON.usfm` for its right-to-left
verses. The two `en_*` copies came from the dev rig at
`dev-env/state/work/repos/_local_/_sideloaded_/en_ult|en_ust/ingredients/TIT.usfm`.
The commit hashes above match the `revision` field in each sideloaded burrito's
`metadata.json` (verified 2026-07-30). The copies make the tests independent of
rig state.

The plain-draft corpora are in `sample-burrito/`:

| File | Source | Contents | Date |
|---|---|---|---|
| `sample-burrito/TIT.usfm` | generated `sample-burrito/ingredients/TIT.usfm` | plain draft, `___` stubs | 2026-07-30 |
| `sample-burrito/JON.usfm` | generated `sample-burrito/ingredients/JON.usfm` | plain draft, `___` stubs, JON 2:9-10 span verse | 2026-07-30 |

`test/indexer.test.ts` and `test/splice.test.ts` read these two files.

These two files are a **dated snapshot**, not a live mirror. The generator makes the
live copy: run `npm run generate` in the `sample-burrito-validation` workspace, which
writes `../sample-burrito/`. That workspace is not part of this repository. Do not edit
the snapshot. To refresh it, run the generator and copy the two draft files again.
