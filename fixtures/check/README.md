# Checking fixtures for `tc-check`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
script the table names.

| File | What it is | How it was made |
|---|---|---|
| `check-vectors.json` | What the JavaScript libraries behind tC4's Check screens answer. `scope`: `doesReferenceContain(range, "c:v")` for 17 scope ranges in every form a burrito scope allows, against 6 chapters and 24 verse forms (numbers, spans, `intro`, `1a`, `3,5`, `2ff`, an en dash), as tC4's `refInScope` asks it. `selections`: for 126 real verses, the selection tC4's `selectionsFromTokens` records for pseudo-random taps, the taps `tokenIndicesFromSelections` restores, and `validateVerseSelections` against the verse and four edits of it; `validations` adds eight hand-written cases. `quoteWords`: tC4's `tnQuoteWords` for every en_tn v86 Titus quote. `sameQuote`: tC4's `sameOrigQuote` for the en_tn v86 and es-419_tn v66 Titus quotes of each shared check id, and five hand-written pairs. `terms`: 430 tW and tA slugs and hand-written strings in the order `String.prototype.localeCompare` gives them. | `crates/tc-check/tools/gen-check-vectors.cjs` with `bible-reference-range@1.1.1`, `word-aligner-lib@1.0.1`, `string-punctuation-tokenizer@2.0.0` and `usfm-js@3.4.3`, on Node 22.22.0 (whose ICU decides `localeCompare`). The functions of tC4's `src/data/selections.ts` and `src/data/derive.ts` that the vectors need are copied into the script verbatim. |
| `psalms-vectors.json` | What tC4's own derivation makes of the v91 helps the app ships (`../../bundle/en/`). `cases`: tC4's `deriveForProject` on the Psalms rows of en_tn v91 (`translationNotes`) and en_twl v91 (`translationWords`, gated by the articles of the en_tw v91 pin as tC4's Check screen gates them), from `eng` into `eng` for the whole book and six partial scopes, and into `org` for the whole book and chapter 3. Each case lists the checks in order as `[checkId, chapter, verse, occurrence]` (with `groupId` and `category` in the whole-book `eng` lists), the rows the conversion could not place with tC4's reason, and whether a conversion ran. `books`: for every book of en_tn and en_twl v91, the checks of a whole-book `eng` project: their number and the md5 of their identities in order (one `checkId\|chapter\|verse\|occurrence\|groupId` line each, joined by a newline), leaving out the checks on every occurrence (Occurrence -1), which `every` lists. `inputs` names each export and its sha256. | `crates/tc-check/tools/gen-psalms-vectors.cjs`, which bundles tC4's `src/data/derive.ts` (and the `mapReference.ts`, `bible-reference-range` 1.1.1 and `proskomma-core` 0.11.3 it calls) with the esbuild in tC4's `node_modules`, and reads the exports with tC4's `fflate`. The schemes are `../tc4/test-fixtures/vrs/eng.json` and `org.json`, the files tC4's own tests read. The same run writes `../helps/v91-tsv-rows.json`. |
| `reference-forms-vectors.json` | What tC4 makes of a `Reference` cell in a form no shipped table has. `tables`: a Psalms tN, TWL and tQ table written by the generator, one row per form (28: `1:1a`, `0:front`, `3:front ` with a trailing space, `3:Front`, `front:front`, `3:1,front`, `1:0`, `3:2-1`, `1:1:1`, `intro`, an empty cell, `3:`, `:1`, ` 3:1`, `3:01`, `3:1-2:4`, an en dash, an Arabic-Indic digit, eight forms tc-helps gives a place to, and `3:1` and `3:front` each followed by a CR, which tC4's `tsvRows` drops from the end of every cell), then one row whose every cell but the last ends in a CR. `rows`: what tC4's `deriveTnItems` (with `keepPlain`), `deriveTwlItems` and `deriveTqItems` return for them, as `[checkId, chapter, verse, occurrence]`, `null` for a part tC4 leaves `undefined`. `cases`: tC4's `deriveForProject` on the tN and TWL tables (no tW article gate), from `eng` into `eng` for the whole book and four partial scopes, and into `org` for the whole book and chapter 3, with the checks in order and the rows the conversion could not place. | `crates/tc-check/tools/gen-psalms-vectors.cjs`, in the same run as `psalms-vectors.json`, with the same bundle of tC4's `src/data/derive.ts` and the same schemes. |

A test (`crates/tc-check/tests/fixtures_readme.rs`) checks that every file in this
directory has a row in the table above.

## `check-vectors.json`

The library versions are the ones tC4 pins. `check-vectors.json` was generated on
2026-10-15 from the `node_modules` of the tC4 checkout this repository's `fixtures/tc4/`
came from (commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`); `gen-check-vectors.cjs`
refuses to run against other versions. To regenerate, run from the repository root with
any `node_modules` that holds those versions (tC4's own works):

```sh
node crates/tc-check/tools/gen-check-vectors.cjs <node_modules> > fixtures/check/check-vectors.json
```

`gen-check-vectors.cjs` reads its verse text from `../usfm/verse-text-vectors.json` and
`../usfm/tokenizer-vectors.json`, and its quotes and slugs from the resources in
`../tc4/test-fixtures/resources/`.

## `psalms-vectors.json` and `reference-forms-vectors.json`

`gen-psalms-vectors.cjs` wrote both files, and `../helps/v91-tsv-rows.json`, in one run on
2026-10-16 on Node 22.22.0 from the same tC4 checkout (commit
`d494a9d6ba9f4cdcc09faff9038c93159106045b`, version 4.0.0-rc.1). It refuses other
versions of `bible-reference-range`, `proskomma-core`, `usfm-js`, `fflate` and `esbuild`,
and a checkout with local changes in `src/`. To regenerate all three, run from the
repository root:

```sh
node crates/tc-check/tools/gen-psalms-vectors.cjs /path/to/translationcore4
```

## Licenses

The verses, quotes and slugs come from the tC4 fixtures in `../tc4/` and keep the licenses
stated in `../README.md`: unfoldingWord content is CC BY-SA 4.0, © unfoldingWord. The ids
and references in `psalms-vectors.json` come from the en_tn and en_twl v91 releases (CC BY-SA
4.0, © unfoldingWord), and the file holds what tC4's code (GPL-2.0-or-later) computes from
them, not the code. The tables in `reference-forms-vectors.json` were written for that file
and are in the public domain; the file holds what tC4's code computes from them. The
hand-written cases were written for this file and are in the public domain. The file holds
only the libraries' output, no library code. The libraries are `bible-reference-range`
(MIT), `word-aligner-lib` (MIT), `string-punctuation-tokenizer` (GPL-2.0) and `usfm-js`
(ISC); the generator script is part of this repository (GPL-2.0-or-later).
