# USFM identity corpus — provenance

This directory is the corpus for `test/usfm-identity-corpus.test.ts` (issue #17). The
test proves the editor's byte promise (D8): an edit to one verse changes no other byte
of the file. The test reads every `.usfm` file here. To extend the corpus, add a file
and add its source to the table below.

These files are pinned copies or hand-made fixtures. Do not edit them. Tests read them
read-only.

| File | Source | Exotic coverage | Date |
|---|---|---|---|
| `en_ult-TIT-aligned.usfm` | copy of `test/fixtures/en_ult/TIT.usfm` (unfoldingWord en_ult v89, commit `84c73ba00fc8a95a9033f9efb14bb905a2a52ee4` — see `test/fixtures/README.md`) | `\zaln-s`/`\zaln-e` milestones, `\w` attributes, `\ts\*` | 2026-08-14 |
| `en_ust-TIT-aligned.usfm` | copy of `test/fixtures/en_ust/TIT.usfm` (unfoldingWord en_ust v89, commit `37ec223166bbd73fb55abc7840be8310c0fee7f2` — see `test/fixtures/README.md`) | `\zaln-s`/`\zaln-e` milestones, `\w` attributes, `\ts\*` | 2026-08-14 |
| `sample-JON-span.usfm` | copy of `conformance/sample-burrito/ingredients/JON.usfm` (generated sample project) | span verse `\v 9-10`, `___` stubs | 2026-08-14 |
| `sample-TIT-draft.usfm` | copy of `conformance/sample-burrito/ingredients/TIT.usfm` (generated sample project) | plain draft, `___` stubs | 2026-08-14 |
| `exotic-poetry-footnotes.usfm` | hand-made for this corpus (no repository file has these markers) | `\q1`/`\q2` poetry, `\b`, `\d`, `\s`, footnotes `\f`…`\f*` with `\fr`/`\ft`/`\fq`/`\fqa`/`\fk` and nested `\+w`, lists `\lh`/`\li`/`\lf`, span verse `\v 4-5`, `\zaln` + footnote in one verse | 2026-08-14 |
| `exotic-partial-book.usfm` | hand-made for this corpus (partial-book class, D26) | only chapter 3 present, span verse `\v 3-4`, poetry, no final line terminator | 2026-08-14 |

The two aligned copies are the alignment-stripping regression fixtures that
`test/splice.test.ts` and `test/indexer.test.ts` also use (provenance:
`test/fixtures/README.md`). The copies here are frozen with the corpus on purpose:
the corpus test must not change when another test replaces its own fixture.
