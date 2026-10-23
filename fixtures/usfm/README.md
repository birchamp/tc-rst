# USFM fixtures for `tc-usfm`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
scripts named below.

| File | What it is | How it was made |
|---|---|---|
| `tokenizer-vectors.json` | What tC4's word tokenizer returns for 195 real verses, 23 hand-picked edge cases and 500 seeded random strings: every token with its type (`word`, `number`, `punctuation`, `whitespace`, `unknown`). | `crates/tc-usfm/tools/gen-tokenizer-vectors.cjs` with `string-punctuation-tokenizer@2.0.0`, `xregexp@4.4.1` and `usfm-js@3.4.3`. |
| `verse-text-vectors.json` | The text tC4 hashes for the alignment record of each of 243 verses (`verseTextMd5` in `journal/fold.mjs`, without the md5): the plain drafts of the sample burrito and the import fixtures, the exotic corpus and UHB Jonah. | `crates/tc-usfm/tools/gen-verse-text-vectors.cjs` with `usfm-js@3.4.3`. |
| `weave-vectors.json` | What tC4's aligned-USFM export writes (word-aligner-lib's `addAlignmentsToTargetVerseUsingMerge`, as `src/data/export/weave.mjs` calls it) for 229 verses with their alignment records: the 92 verses of en_ult and en_ust Titus with the record tC4 reads from each, the sample burrito's stored record for TIT 1:1, and 136 generated alignments over 17 hand-written verses (poetry, notes, cross references, character markup, quotes, numbers, symbols, Hebrew, CRLF). `woven` is left out where it equals `aligned`. | `crates/tc-usfm/tools/gen-weave-vectors.cjs` with `word-aligner-lib@1.0.1`, `word-aligner@1.0.3`, `usfm-js@3.4.3` and `string-punctuation-tokenizer@2.0.0`. |
| `superscription-vectors.json` | What tC4 reads from the 102 verses of the ULT v91 and UST v91 Psalms that start with a superscription (`\v 1 \d \zaln-s …`): the verse text its panes show and its aligner splits, the words with their occurrences, and the original-language words above each word. Also 8 hand-written drafts with `\d`, `\sp`, `\qa`, `\lit` and `\s` inside a verse, with what tC4's export writes for each when every word is aligned, and an aligned word in each of 17 headings (`\d`, `\sp`, `\qa`, `\lit`, `\s`, `\ms`, `\r`, `\d1`…) with whether tC4's export writes it back or cannot merge it. Also LAM 1:1 of the ULT v91 and UST v91, which in tC4's cut ends with the bare acrostic letter of verse 2 (`\qa BETH`): its text, words, the words no alignment holds, and what tC4's export writes once the verse's record banks them. The generator also checks that tC4's export writes each published Psalm verse back as published, and that it cannot merge the record it reads out of the published LAM 1:1. | `crates/tc-usfm/tools/gen-superscription-vectors.mjs` with `usfm-js@3.4.3`, `string-punctuation-tokenizer@2.0.0` and `word-aligner-lib@1.0.1`, running tC4's own `src/data/sourceHighlight.ts` and `src/data/align/tokenize.ts` (Node 22.18 or later). |
| `check-text-vectors.json` | What tC4's Check screen and aligner read of each of 450 verses (`verseTextIndex` of `src/state.jsx`): whether tC4's indexer reads the verse as drafted (`indexBook`, `src/data/usfm/indexer.ts`) and, for the 99 drafted ones, the text it shows (`verseText` of `src/data/bookModel.js` over usfm-js's whole-book parse). The plain drafts of the tC4 fixtures, UHB Jonah, and hand-written drafts (held in the file's `drafts`) with poetry verses whose text starts on a line of its own, a stub followed by poetry, a superscription, a `\qs` Selah, list items, notes, headings, CRLF line ends and verse markers inside a line. None nests an alignment or holds a note between two words, so tC4's #782 and #795 change none of them. | `crates/tc-usfm/tools/gen-check-text-vectors.mjs` with `usfm-js@3.4.3`, running tC4's own `indexBook`, `tokenizeVerse` and `tokensText` and the `verseTextIndex`, `parseChapters` and `verseText` it reads out of `src/state.jsx` and `src/data/bookModel.js` (Node 22.18 or later), from tC4 5dcd147 (byte-identical from d310bf3 and 9ad4d5c). |
| `shown-text-vectors.json` | What tC4's Translate, Check and Align show of 8,143 verses (`verseText` of `src/data/bookModel.js`, the walk of `tokenizeVerse` in `src/data/sourceHighlight.ts`), for every verse usfm-js finds, `\q1 \v N` verses included: the text of every verse of tC4's aligned fixtures (en_ult and en_ust Titus, en_ult Genesis 1:1, Genesis 1 and Psalm 3:1-2 of tC4 9ad4d5c's `test/fixtures`) and of the exotic corpus; the text of 26 drafts (held in the file's `drafts`): tC4 PR #800's cases with a note, cross reference or figure between two words of the sample Titus 1:1, the same with `\va`, `\vp`, `\ca`, `\rq`, `\cat` and `\ts\*` (`hidden-*`, which tC4 joins and the rebuild does not, a declared difference), and hand-written notes before a line break, a poetry line, right-to-left words, a word outside the Basic Multilingual Plane and a no-break space, notes inside an aligned verse (`aligned-notes`, where a note inside a run right before `\zaln-e\*` still joins in tC4), and alignments nested three deep and inside `\add`; and the md5 of the text of every verse of the ULT v91 and UST v91 Genesis and Psalms (7,983). `tc4` is the reference: tC4 PR #800's head 5dcd147. `main` is tC4 main's text (9ad4d5c) on the 12 cases where it differs, all a note between two words; the bundle verses are the same in both. | `crates/tc-usfm/tools/gen-shown-text-vectors.mjs` with `usfm-js@3.4.3` and `fflate@0.8.2`, running each tree's own `tokenizeVerse`, `tokensText` and the `verseText` it reads out of `src/data/bookModel.js` (Node 22.18 or later), on `git archive` trees of tC4 `5dcd1472f9a3dec36bbc2f4e811c40d1387e7068` and `9ad4d5c506466819e5b10f255bbb4a36d394e498` with a linked `node_modules`, on 2026-10-19. |
| `export-vectors.json` | For each of the 133 books of the ULT v91, the UST v91 and the scale Psalms (`../scale/psa/en_ult/19-PSA.usfm`), the verses that tC4's own aligned-USFM export does not write back as published when its alignment records are the ones it reads out of the published file: `rewritten` (same text, other markup), `changed` (other verse text) and `unaligned` (the merge fails, so the verse is written without its alignment), with the counts of verses cut, aligned, written back, and written back but for one more space after a closing paragraph marker (`spaced`). | `crates/tc-usfm/tools/gen-export-vectors.mjs` with `usfm-js@3.4.3` and `word-aligner-lib@1.0.1`, running tC4's own `journal/skeleton.mjs`, `journal/fold.mjs`, `src/data/export/weave.mjs` and `src/data/sourceHighlight.ts` (Node 22.18 or later, about five minutes). |
| `section-draft-vectors.json` | What tC4's section drafting rules (`src/views/sectionDraft.js`: `parseDraft`, `sectionGroups`, `sectionKeys`, `sectionVerses`, `serializeDraft`, `canDrop`, `dropPin`, `spanEnd`, `initialDraftText`, `indentLine`, `expandKeys`) answer for 29 hand-written section texts (tC4's own test cases and the ones around them) and 700 seeded random ones over 8 key sets, with the markers of up to four allowed drops, 160 lists of verses and 249 indents. | `crates/tc-api/tools/gen-section-vectors.mjs`, which imports tC4's file as it is (no library; Node 22 or later). |

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
node crates/tc-usfm/tools/gen-check-text-vectors.mjs <tC4 checkout> > fixtures/usfm/check-text-vectors.json
node crates/tc-api/tools/gen-section-vectors.mjs <tC4 checkout> > fixtures/usfm/section-draft-vectors.json
node crates/tc-usfm/tools/gen-shown-text-vectors.mjs <tC4 5dcd147 tree> 5dcd1472f9a3dec36bbc2f4e811c40d1387e7068 \
  <tC4 9ad4d5c tree> 9ad4d5c506466819e5b10f255bbb4a36d394e498 > fixtures/usfm/shown-text-vectors.json
```

`gen-superscription-vectors.mjs`, `gen-export-vectors.mjs`, `gen-check-text-vectors.mjs` and
`gen-shown-text-vectors.mjs` take a tC4 tree (the checkout, or `git archive <commit> src |
tar -x` with a `node_modules` linked in), not only its `node_modules`, because they run tC4's
own code; the first two and the last read the ULT and UST from `bundle/en/`.
`shown-text-vectors.json` and `check-text-vectors.json` were made on 2026-10-19 from the
trees named in their rows; the `node_modules` held the versions above.

## Licenses

The real verses come from the tC4 fixtures in `../tc4/` (en_ult and en_ust Titus, hbo_uhb
Jonah, the sample drafts and the exotic corpus) and from the ULT and UST v91 Psalms in
`../../bundle/en/`, and keep the licenses stated in `../README.md`: unfoldingWord content
is CC BY-SA 4.0, © unfoldingWord. `shown-text-vectors.json` holds verse text of tC4's
fixtures (en_ult Genesis 1, Psalm 3:1-2 and Titus, en_ust Titus, CC BY-SA 4.0) and the md5s
of bundle verses. `export-vectors.json` holds only verse references and
counts. The edge cases,
random strings and hand-written verses were written for these files and are in the public
domain. The files hold only the libraries' output, no library code. The libraries are
`string-punctuation-tokenizer` (GPL-2.0), `xregexp` (MIT), `usfm-js` (ISC),
`word-aligner-lib` (MIT) and `word-aligner` (ISC); the generator scripts are part of this
repository (GPL-2.0-or-later).
