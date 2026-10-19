# Test fixtures — provenance

These files are pinned copies. Do not edit them. Tests read them read-only.

| File | Source | Version | Commit (revision in the export's metadata.json) | How fetched | Date |
|---|---|---|---|---|---|
| `en_ult/TIT.usfm` | unfoldingWord en_ult (aligned USFM) | v89 | `84c73ba00fc8a95a9033f9efb14bb905a2a52ee4` | DCS sb-zip export (`/sb/<tag>.zip`) | 2026-07-30 |
| `en_ult/GEN-1.usfm` | unfoldingWord en_ult (aligned USFM), Genesis header and chapter 1 only (lines 1–819 of `GEN.usfm`, cut before `\c 2`) | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | rig cache `dev-env/resources-cache/en_ult-v91-unwrapped.zip` (DCS sb-zip export); full `GEN.usfm` sha256 `bcc81339556b4cb27de812590e89884243f3a69da70d3c8412bbc9291430e4e4` | 2026-10-16 |
| `en_ust/TIT.usfm` | unfoldingWord en_ust (aligned USFM) | v89 | `37ec223166bbd73fb55abc7840be8310c0fee7f2` | DCS sb-zip export (`/sb/<tag>.zip`) | 2026-07-30 |
| `hbo_uhb/JON.usfm` | unfoldingWord hbo_uhb (Hebrew, right-to-left; `\w` tokens with lemma/strong/morph, U+2060 word joiners inside words, maqaf U+05BE between words) | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` (the tag's commit) | raw file `32-JON.usfm` at the tag; sha256 `55a36ef7ab573305e6281d5ae28223fe525e478f7e12e946b64099f6b646ca7b` | 2026-09-12 |
| `hbo_uhb/GEN-1-1.usfm` | unfoldingWord hbo_uhb, Genesis from the file start up to `\v 2` (the header and Genesis 1:1; the Hebrew is not NFC) | v3.0.0 | `74022f0fed012a3ef169886f595dd98e7b200543` | lines of the sideloaded `ingredients/GEN.usfm` (sha256 `5b9391454b6b6f358838f2cbfd4f5b5cd90b447ad4320e087b698f2b9c167d71`), cut before `\v 2`; extract sha256 `db5b57f90710b5ac39eb8bed25b222d279ca1ebd04940008a3de7e36c52f02f5` | 2026-10-16 |
| `en_ult/GEN-1-1.usfm` | unfoldingWord en_ult (aligned USFM), Genesis from the file start up to `\v 2` | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | lines of the sideloaded `ingredients/GEN.usfm` (sha256 `bcc81339556b4cb27de812590e89884243f3a69da70d3c8412bbc9291430e4e4`), cut before `\v 2`; extract sha256 `44fe75ea16c38732866e3c9a4ced1783060aa177ed836b2275508c126aa4d405` | 2026-10-16 |
| `en_ult/GEN-773.usfm` | unfoldingWord en_ult (aligned USFM), Genesis header plus verses 9:11, 11:31, 24:48, 33:5 and 49:6 only (each `\c` line and the lines of those verses from `GEN.usfm`) | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | raw file `01-GEN.usfm` at the v91 tag (sha256 `bcc81339556b4cb27de812590e89884243f3a69da70d3c8412bbc9291430e4e4`, the same file as above); each verse parses to the same usfm-js verseObjects as in the full file, except that 49:6 lacks the trailing `\q1` marker of verse 7 (no text, no token); extract sha256 `a2a247ff5e2a5522d35d0abb386b4f2c4e835785861e97507097d1b63ecbc55c` | 2026-10-16 |
| `en_ult/PSA-3.usfm` | unfoldingWord en_ult (aligned USFM), Psalms header (the lines before `\c 1`) plus chapter 3 up to `\v 3` (the title and verses 1-2; verse 2 ends with an aligned "Selah" inside `\qs … \qs*`) | v91 | `35d215957f3203fd2e2fac5702ce14902d417f9d` | rig cache `dev-env/resources-cache/en_ult-v91-unwrapped.zip`, `ingredients/PSA.usfm` (sha256 `f3753ca75732d3e7a1738014f4e845a39041c2ce8df7b5797f728f63b7baabbe`), lines 1-11 and 349-398; extract sha256 `6068efd5b0a5b47ac342b69a1caaf3b1c4be795d2ba039c6b189b48d3806b05a` | 2026-10-17 |

`test/align-tokenize.test.ts` (issue #255) reads `hbo_uhb/JON.usfm` for its right-to-left
verses. `test/align-gateway-label.test.ts` (issue #768) reads the two `GEN-1-1.usfm`
extracts; their commit hashes match the `revision` field of the sideloaded burritos
(verified 2026-10-16). `test/sourceHighlight.test.ts` (issue #773) reads `en_ult/GEN-773.usfm`;
its source file has the same sha256 as the full `GEN.usfm` above. `test/bookModel-nested-alignment.test.ts` (issue #782) reads `en_ult/GEN-1-1.usfm`, `en_ult/GEN-1.usfm`, `en_ult/PSA-3.usfm` and `hbo_uhb/GEN-1-1.usfm`. The two `en_*` TIT copies came from the dev rig at
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
