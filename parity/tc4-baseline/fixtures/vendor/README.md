# Vendored resource files

tC4 does not vendor these files, and git.door43.org cannot be reached from the machine that made
the baseline. `make-vendor.mjs` cuts them from two published copies. `SOURCES.json` names the
source of every file and its md5. Do not edit the files. To rebuild them, run
`npm run vendor` from `parity/tc4-baseline/`. The script checks every download against a
recorded sha256, so a rebuild gives the same bytes.

| Folder | Content | Source |
|---|---|---|
| `el-x-koine_ugnt/TIT.usfm` | unfoldingWord® Greek New Testament **v0.34** (the pinned release), Titus | `tcResources/el-x-koine/bibles/ugnt/v0.34_unfoldingWord/books.zip` in [unfoldingWord/translationCore](https://github.com/unfoldingWord/translationCore) at `5729aa7b977ef85356139f73d957ee65a276460b`. translationCore 3 keeps the book as verse-object JSON. tC4's own usfm-js 3.4.3 writes it back to USFM, and the USFM reads back to the same JSON. The book headers are written by the script, because the bundle has none. |
| `en_tw/payload/<cat>/<slug>.md` | en_tw **v88** articles: the 93 words that the en_twl v91 Titus list links | `tcResources/en/translationHelps/translationWords/v88_unfoldingWord/contents.zip`, same commit (`<cat>/articles/<slug>.md`). 89 of the 93 have the md5 that the v91 export metadata records; `kt/evil`, `kt/good`, `kt/save` and `other/courage` differ. |
| `en_ta/translate/<slug>/01.md`, `title.md` | en_ta **v88** articles: 31 of the 32 that the SupportReferences of the en_tn v91 and es-419_tn v66 Titus notes name | `tcResources/en/translationHelps/translationAcademy/v88_unfoldingWord/contents.zip`, same commit. translationCore 3 keeps one file per article (`# Title #` and then the body). The script splits it into the Door43 layout that tC4 reads. |
| `en_obs/01.md` | unfoldingWord® Open Bible Stories, English, story 1 (title, 16 frames, reference) | The example story in `dist/components/Examples/EditorExample.js` of the npm package `open-bible-stories@0.0.5`. The release of the text is not known. |

The script also checks the content against tC4's own fixtures:

- Each Greek word that the ULT v89 alignment of Titus points at must be in the UGNT verse, with
  the same occurrence count. The ULT must point at each word of the verse. All 46 verses pass.
- Each tW article that the Titus words link must be in the bundle.
- Each tA article that the Titus notes name must be in the bundle. The exception is
  `translate/figs-inclusive`, which es-419_tn v66 names but which en_ta v86 and v88 do not have.
  `SOURCES.json` lists it under `absentFromBundles`.

Licence: the content is © unfoldingWord and is licensed under
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). This is the licence of the
UGNT, tW, tA and OBS releases it comes from.
