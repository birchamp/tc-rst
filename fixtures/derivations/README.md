# Derivation fixtures for `tc-usfm`

Golden outputs of the version-1 derivations of `docs/STORAGE.md` §7.7, produced once by the
JavaScript libraries tC4 pins. Tests read these files read-only. Do not edit them by hand:
regenerate them with the command below.

| File | What it is |
|---|---|
| `plain-text-v1.json` | For each verse content: `text`, the text tC4 hashes into `targetVerseMd5` (`verseTextMd5` in tC4's `journal/fold.mjs`, without the md5: usfm-js's chunk parse of `"\v 1 " + content`, the top-level `text` of its verse objects joined and trimmed), and `removed`, usfm-js's `removeMarker` of the content. `null` where the library throws. |
| `selections-v1.json` | Texts with selections and whether word-aligner-lib's `validateVerseSelections` reports them changed; `throws` where it throws (tC4 then treats the decision as not stale). |
| `tokenize-v1.json` | The words string-punctuation-tokenizer's default `tokenize` returns for each plain text. |

The contents are every verse of the USFM files in `../tc4/` (one copy of each file), 40
hand-written edge cases and 1,484 seeded random verses built from USFM pieces: words in
several scripts, white space and line breaks, paragraph and poetry markers, headings,
character markers, `\w` words with attributes, `\zaln`, `\k` and `\qt` milestones, notes,
figures and broken markup.

Libraries: `usfm-js@3.4.3`, `string-punctuation-tokenizer@2.0.0` and `word-aligner-lib@1.0.1`,
from the `node_modules` of the tC4 checkout this repository's `fixtures/tc4/` came from
(commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`); the script refuses other versions. Run
from the repository root:

```sh
node crates/tc-usfm/tools/gen-derivation-vectors.cjs <node_modules> fixtures/derivations
```

The same script writes a larger random corpus for a long differential run, which is not
committed:

```sh
node crates/tc-usfm/tools/gen-derivation-vectors.cjs <node_modules> --random 100000 --seed 2 --pieces 80 --out /tmp/vectors.json
TC_USFM_DERIVATION_VECTORS=/tmp/vectors.json cargo test -p tc-usfm -- --ignored
```

## Licenses

The real verses come from the tC4 fixtures in `../tc4/` and keep the licenses stated there:
unfoldingWord content is CC BY-SA 4.0, © unfoldingWord. The edge cases and random verses
were written for these files and are in the public domain. The files hold only the
libraries' output, no library code. The libraries are `usfm-js` (ISC),
`string-punctuation-tokenizer` (GPL-2.0) and `word-aligner-lib` (MIT); the generator script
is part of this repository (MIT).
