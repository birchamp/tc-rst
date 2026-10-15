# Scale fixtures

The real data behind `docs/PERFORMANCE.md`. Psalms is the largest book, so every budget is
measured on it.

| Path | What it is | Source | sha256 |
|---|---|---|---|
| `psa/en_ult/19-PSA.usfm` | unfoldingWord Literal Text, Psalms, aligned to the Hebrew: 2,461 verses, 21,600 `\zaln-s` groups, 42,292 words | Exported by translationCore on 2023-10-09 (its `\id` line); supplied by the project owner on 2026-10-15 | `f3753ca75732d3e7a1738014f4e845a39041c2ce8df7b5797f728f63b7baabbe` |
| `psa/en_tn/tn_PSA.tsv` | unfoldingWord translationNotes for Psalms: 8,212 rows, 8,052 with a Hebrew quote | Supplied by the project owner on 2026-10-15; the row count equals the en_tn v89 count in `tc4/conformance-fixtures/check-density.json` | `60e1b7f650731aff90d6670ae93735a03758725c62ad3806ed84265fb101c7e3` |
| `psa/web/web_psa.usx` | World English Bible British Edition, Psalms, USX 3.0: 2,461 verses | `test/test_data/usx/web_psa.usx` in https://github.com/Proskomma/proskomma-core at commit `0204ad3` | `a80457f92ac663a2d43777ac94f82ddfb8d4ff8b5bb0acf5e9659f138f029514` |

Per-book check counts for the whole Bible are in
`../tc4/conformance-fixtures/check-density.json` (en_tn and en_twl v89, row counts).

Still missing: the Hebrew Bible (UHB) Psalms and the translationWords links (TWL) for
Psalms. Until they are added, benchmarks generate them and label them as generated.

Licenses: ULT and translationNotes are CC BY-SA 4.0, © unfoldingWord. The World English
Bible is in the public domain.
