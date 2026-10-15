# Checking fixtures for `tc-check`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
script named below.

| File | What it is | How it was made |
|---|---|---|
| `check-vectors.json` | What the JavaScript libraries behind tC4's Check screens answer. `scope`: `doesReferenceContain(range, "c:v")` for 17 scope ranges in every form a burrito scope allows, against 6 chapters and 24 verse forms (numbers, spans, `intro`, `1a`, `3,5`, `2ff`, an en dash), as tC4's `refInScope` asks it. `selections`: for 126 real verses, the selection tC4's `selectionsFromTokens` records for pseudo-random taps, the taps `tokenIndicesFromSelections` restores, and `validateVerseSelections` against the verse and four edits of it; `validations` adds eight hand-written cases. `quoteWords`: tC4's `tnQuoteWords` for every en_tn v86 Titus quote. `sameQuote`: tC4's `sameOrigQuote` for the en_tn v86 and es-419_tn v66 Titus quotes of each shared check id, and five hand-written pairs. `terms`: 430 tW and tA slugs and hand-written strings in the order `String.prototype.localeCompare` gives them. | `crates/tc-check/tools/gen-check-vectors.cjs` with `bible-reference-range@1.1.1`, `word-aligner-lib@1.0.1`, `string-punctuation-tokenizer@2.0.0` and `usfm-js@3.4.3`, on Node 22.22.0 (whose ICU decides `localeCompare`). The functions of tC4's `src/data/selections.ts` and `src/data/derive.ts` that the vectors need are copied into the script verbatim. |

The library versions are the ones tC4 pins. The file was generated on 2026-10-15 from the
`node_modules` of the tC4 checkout this repository's `fixtures/tc4/` came from (commit
`d494a9d6ba9f4cdcc09faff9038c93159106045b`); the script refuses to run against other
versions. To regenerate, run from the repository root with any `node_modules` that holds
those versions (tC4's own works):

```sh
node crates/tc-check/tools/gen-check-vectors.cjs <node_modules> > fixtures/check/check-vectors.json
```

The script reads its verse text from `../usfm/verse-text-vectors.json` and
`../usfm/tokenizer-vectors.json`, and its quotes and slugs from the resources in
`../tc4/test-fixtures/resources/`.

## Licenses

The verses, quotes and slugs come from the tC4 fixtures in `../tc4/` and keep the licenses
stated in `../README.md`: unfoldingWord content is CC BY-SA 4.0, © unfoldingWord. The
hand-written cases were written for this file and are in the public domain. The file holds
only the libraries' output, no library code. The libraries are `bible-reference-range`
(MIT), `word-aligner-lib` (MIT), `string-punctuation-tokenizer` (GPL-2.0) and `usfm-js`
(ISC); the generator script is part of this repository (GPL-2.0-or-later).
