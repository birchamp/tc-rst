# Alignment fixtures for `tc-align`

Tests read these files read-only. Do not edit them by hand.

| File | What it is | How it was made |
|---|---|---|
| `el-x-koine_ugnt-TIT.usfm` | unfoldingWord® Greek New Testament **v0.34** (the release tC4 pins), Titus: the original-language verses that en_ult Titus is aligned to. | Copied from `parity/tc4-baseline/fixtures/vendor/el-x-koine_ugnt/TIT.usfm` (md5 `65133339aca588706545122b56eedde7`), which `parity/tc4-baseline/fixtures/vendor/make-vendor.mjs` cut from `tcResources/el-x-koine/bibles/ugnt/v0.34_unfoldingWord/books.zip` in unfoldingWord/translationCore at `5729aa7b977ef85356139f73d957ee65a276460b` (translationCore 3's bundle, verse-object JSON written back to USFM by usfm-js 3.4.3). Every Greek word the en_ult v89 alignment of Titus points at is in it with the same occurrence count. |
| `bootstrap-vectors.json` | What tC4's `bootstrapVerse` makes for every verse of en_ult Titus against UGNT Titus, for UHB Jonah 1 against itself (a right-to-left target with word joiners), and for the sample burrito's Spanish span JON 2:9-10 against UHB 2:9 and 2:10. | `crates/tc-align/tools/gen-align-vectors.cjs` |
| `extract-vectors.json` | word-aligner's `unmerge` of every aligned verse of en_ult and en_ust Titus with its UGNT verse, and of the 136 partly aligned verses that tC4's export wrote in `fixtures/usfm/weave-vectors.json`. | same |
| `reflow-vectors.json` | tC4's `reflowAlignment` on every third en_ult Titus verse after seeded edits of its text (a word deleted, repeated or replaced, punctuation only, unchanged, no words, and a record marked invalid). | same |
| `heading-source.usfm` | Twenty-nine synthetic original-language verses (Hebrew letters as words). Chapters 1 and 2: a heading, title or note inside the verse: a mid-verse `\s5`, `\r`, `\qa`, `\sp`, `\d`, `\ms`, `\s`, `\s1` or `\cl` followed by `\w` words, a footnote holding a `\w`, an `\s5` with words between the last verse of chapter 1 and `\c 2`, and a `\cl` and an `\s` with words after `\c 2`. Chapters 3 and 4: sidebars (`\esb` … `\esbe`) inside a verse, before the next `\v` and before `\c 4`; `\w` words inside `\pro`, `\ndx`, `\fv`, `\xt`, `\ior`, `\iqt`, `\fdc`, `\xdc`, `\fm`, `\fa`, `\fqa`, `\rt`, `\sis`, `\xnt`, `\xop` and `\xot`; footnotes left open across a paragraph; a `\pro` inside a `\pro`; and footnotes inside `\add` and `\k-s`, before and after a chunk milestone. | Written for `tc-align` (GPL-2.0-or-later, this repository). |
| `heading-vectors.json` | What tC4's `bootstrapVerse` makes for each verse of `heading-source.usfm` and for the spans 1:1-2, 1:10-11 and 3:2-3: which original words get a card. | `crates/tc-align/tools/gen-align-vectors.cjs` (section `headings`) |
| `span-source.usfm` | 107 synthetic original-language verses, one for each span usfm-js 3.4.3 knows (each marker of its `USFM_PROPERTIES` with an end marker, `\w` aside) and for 11 it does not, each holding a `\w` word and then a footnote with a `\w` word. | `crates/tc-align/tools/gen-align-vectors.cjs` (section `spans`), from usfm-js's own marker table |
| `span-vectors.json` | What tC4's `bootstrapVerse` makes for each verse of `span-source.usfm`. | same (section `spans`) |
| `suggest-vectors.json` | The suggestion benches' training verses for en_ult and en_ust Titus (`versesOfBook`); every link and confidence `predictLinks` gives from memory-only models and from boosted models trained with `Math.random` seeded as tC4's tests seed it (mulberry32, seed 0); and the growth bench's protocol (`test/align-suggest-growth-bench.test.ts`) on both Titus corpora. | same |

The generator bundles tC4's own sources (`src/data/align/edit.ts`, `suggest.ts`,
`suggestEngine.ts`, `test/helpers/ult-corpus.ts`) with the esbuild in tC4's
`node_modules` and runs them, so the vectors are what tC4 itself computes. It refuses to
run against other versions of the libraries tC4 pins: usfm-js 3.4.3, word-aligner 1.0.3,
word-aligner-lib 1.0.1, string-punctuation-tokenizer 2.0.0, wordmap 0.6.2, wordmap-lexer
0.3.6 and uw-wordmapbooster 1.0.5. The files were generated on 2026-10-15 from the tC4
checkout that `fixtures/tc4/` came from (commit
`d494a9d6ba9f4cdcc09faff9038c93159106045b`, version 4.0.0-rc.1). To regenerate, from the
repository root (the suggestion section takes about ten minutes):

```sh
node crates/tc-align/tools/gen-align-vectors.cjs /path/to/translationcore4 fixtures/align
node crates/tc-align/tools/gen-align-vectors.cjs /path/to/translationcore4 fixtures/align bootstrap extract reflow headings spans
```

`heading-vectors.json`, `span-source.usfm` and `span-vectors.json` were generated on
2026-10-15 from the same tC4 checkout.

## Licenses

UGNT, en_ult, en_ust and UHB are © unfoldingWord, CC BY-SA 4.0 (see
`../tc4/LICENSE-CONTENT.md`); the vector files hold those texts and what tC4's code
computes from them. The generator is part of this repository (GPL-2.0-or-later); it runs
tC4's code (GPL-2.0-or-later) and the libraries above (usfm-js ISC, word-aligner ISC,
word-aligner-lib MIT, string-punctuation-tokenizer GPL-2.0, wordmap ISC, wordmap-lexer MIT,
uw-wordmapbooster ISC) without copying them. `crates/tc-align/src/suggest/` ports wordmap
and uw-wordmapbooster's algorithms.
