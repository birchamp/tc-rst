# USFM fixtures for `tc-usfm`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
scripts named below.

| File | What it is | How it was made |
|---|---|---|
| `tokenizer-vectors.json` | What tC4's word tokenizer returns for 195 real verses, 23 hand-picked edge cases and 500 seeded random strings: every token with its type (`word`, `number`, `punctuation`, `whitespace`, `unknown`). | `crates/tc-usfm/tools/gen-tokenizer-vectors.cjs` with `string-punctuation-tokenizer@2.0.0`, `xregexp@4.4.1` and `usfm-js@3.4.3`. |
| `verse-text-vectors.json` | The text tC4 hashes for the alignment record of each of 243 verses (`verseTextMd5` in `journal/fold.mjs`, without the md5): the plain drafts of the sample burrito and the import fixtures, the exotic corpus and UHB Jonah. | `crates/tc-usfm/tools/gen-verse-text-vectors.cjs` with `usfm-js@3.4.3`. |
| `weave-vectors.json` | What tC4's aligned-USFM export writes (word-aligner-lib's `addAlignmentsToTargetVerseUsingMerge`, as `src/data/export/weave.mjs` calls it) for 229 verses with their alignment records: the 92 verses of en_ult and en_ust Titus with the record tC4 reads from each, the sample burrito's stored record for TIT 1:1, and 136 generated alignments over 17 hand-written verses (poetry, notes, cross references, character markup, quotes, numbers, symbols, Hebrew, CRLF). `woven` is left out where it equals `aligned`. | `crates/tc-usfm/tools/gen-weave-vectors.cjs` with `word-aligner-lib@1.0.1`, `word-aligner@1.0.3`, `usfm-js@3.4.3` and `string-punctuation-tokenizer@2.0.0`. |

The library versions are the ones tC4 pins. The files were generated on 2026-10-15 from
the `node_modules` of the tC4 checkout this repository's `fixtures/tc4/` came from
(commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`); each script refuses to run against
other versions. The tokenizer's character tables in `crates/tc-usfm/src/words/tables.rs`
come from the same `xregexp@4.4.1` through `crates/tc-usfm/tools/gen-unicode-tables.cjs`.

To regenerate, run from the repository root with any `node_modules` that holds those
versions (tC4's own works):

```sh
node crates/tc-usfm/tools/gen-tokenizer-vectors.cjs <node_modules> > fixtures/usfm/tokenizer-vectors.json
node crates/tc-usfm/tools/gen-verse-text-vectors.cjs <node_modules> > fixtures/usfm/verse-text-vectors.json
node crates/tc-usfm/tools/gen-weave-vectors.cjs <node_modules> > fixtures/usfm/weave-vectors.json
node crates/tc-usfm/tools/gen-unicode-tables.cjs <node_modules> > crates/tc-usfm/src/words/tables.rs
```

## Licenses

The real verses come from the tC4 fixtures in `../tc4/` (en_ult and en_ust Titus, hbo_uhb
Jonah, the sample drafts and the exotic corpus) and keep the licenses stated in
`../README.md`: unfoldingWord content is CC BY-SA 4.0, © unfoldingWord. The edge cases,
random strings and hand-written verses were written for these files and are in the public
domain. The files hold only the libraries' output, no library code. The libraries are
`string-punctuation-tokenizer` (GPL-2.0), `xregexp` (MIT), `usfm-js` (ISC),
`word-aligner-lib` (MIT) and `word-aligner` (ISC); the generator scripts are part of this
repository (GPL-2.0-or-later).
