# USFM fixtures for `tc-usfm`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
scripts named below.

| File | What it is | How it was made |
|---|---|---|
| `tokenizer-vectors.json` | What tC4's word tokenizer returns for 195 real verses, 23 hand-picked edge cases and 500 seeded random strings: every token with its type (`word`, `number`, `punctuation`, `whitespace`, `unknown`). | `crates/tc-usfm/tools/gen-tokenizer-vectors.cjs` with `string-punctuation-tokenizer@2.0.0`, `xregexp@4.4.1` and `usfm-js@3.4.3`. |
| `verse-text-vectors.json` | The text tC4 hashes for the alignment record of each of 243 verses (`verseTextMd5` in `journal/fold.mjs`, without the md5): the plain drafts of the sample burrito and the import fixtures, the exotic corpus and UHB Jonah. | `crates/tc-usfm/tools/gen-verse-text-vectors.cjs` with `usfm-js@3.4.3`. |
| `weave-vectors.json` | What tC4's aligned-USFM export writes (word-aligner-lib's `addAlignmentsToTargetVerseUsingMerge`, as `src/data/export/weave.mjs` calls it) for 229 verses with their alignment records: the 92 verses of en_ult and en_ust Titus with the record tC4 reads from each, the sample burrito's stored record for TIT 1:1, and 136 generated alignments over 17 hand-written verses (poetry, notes, cross references, character markup, quotes, numbers, symbols, Hebrew, CRLF). `woven` is left out where it equals `aligned`. | `crates/tc-usfm/tools/gen-weave-vectors.cjs` with `word-aligner-lib@1.0.1`, `word-aligner@1.0.3`, `usfm-js@3.4.3` and `string-punctuation-tokenizer@2.0.0`. |
| `superscription-vectors.json` | What tC4 reads from the 102 verses of the ULT v91 and UST v91 Psalms that start with a superscription (`\v 1 \d \zaln-s …`): the verse text its panes show and its aligner splits, the words with their occurrences, and the original-language words above each word. Also 8 hand-written drafts with `\d`, `\sp`, `\qa`, `\lit` and `\s` inside a verse, with what tC4's export writes for each when every word is aligned, and an aligned word in each of 17 headings (`\d`, `\sp`, `\qa`, `\lit`, `\s`, `\ms`, `\r`, `\d1`…) with whether tC4's export writes it back or cannot merge it. The generator also checks that tC4's export writes each published verse back as published. | `crates/tc-usfm/tools/gen-superscription-vectors.mjs` with `usfm-js@3.4.3`, `string-punctuation-tokenizer@2.0.0` and `word-aligner-lib@1.0.1`, running tC4's own `src/data/sourceHighlight.ts` and `src/data/align/tokenize.ts` (Node 22.18 or later). |
| `export-vectors.json` | For each of the 133 books of the ULT v91, the UST v91 and the scale Psalms (`../scale/psa/en_ult/19-PSA.usfm`), the verses that tC4's own aligned-USFM export does not write back as published when its alignment records are the ones it reads out of the published file: `rewritten` (same text, other markup), `changed` (other verse text) and `unaligned` (the merge fails, so the verse is written without its alignment), with the counts of verses cut, aligned, written back, and written back but for one more space after a closing paragraph marker (`spaced`). | `crates/tc-usfm/tools/gen-export-vectors.mjs` with `usfm-js@3.4.3` and `word-aligner-lib@1.0.1`, running tC4's own `journal/skeleton.mjs`, `journal/fold.mjs`, `src/data/export/weave.mjs` and `src/data/sourceHighlight.ts` (Node 22.18 or later, about five minutes). |

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
node crates/tc-usfm/tools/gen-superscription-vectors.mjs <tC4 checkout> > fixtures/usfm/superscription-vectors.json
node crates/tc-usfm/tools/gen-export-vectors.mjs <tC4 checkout> > fixtures/usfm/export-vectors.json
```

`gen-superscription-vectors.mjs` and `gen-export-vectors.mjs` take the tC4 checkout itself,
not only its `node_modules`, because they run tC4's own code, and they read the ULT and UST
from `bundle/en/`.

## Licenses

The real verses come from the tC4 fixtures in `../tc4/` (en_ult and en_ust Titus, hbo_uhb
Jonah, the sample drafts and the exotic corpus) and from the ULT and UST v91 Psalms in
`../../bundle/en/`, and keep the licenses stated in `../README.md`: unfoldingWord content
is CC BY-SA 4.0, © unfoldingWord. `export-vectors.json` holds only verse references and
counts. The edge cases,
random strings and hand-written verses were written for these files and are in the public
domain. The files hold only the libraries' output, no library code. The libraries are
`string-punctuation-tokenizer` (GPL-2.0), `xregexp` (MIT), `usfm-js` (ISC),
`word-aligner-lib` (MIT) and `word-aligner` (ISC); the generator scripts are part of this
repository (GPL-2.0-or-later).
