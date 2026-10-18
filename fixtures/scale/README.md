# Scale fixtures

The real data behind `docs/PERFORMANCE.md`. Psalms is the largest book, so every budget is
measured on it. The project owner supplied the unfoldingWord files on 2026-10-15.

## Releases

The unfoldingWord releases the corpus uses (UHB 3.0.0, UGNT 0.34; ULT, UST, tN, TWL, tW and
tA v91) ship with the app, so they live in `bundle/en/` at the repository root, byte for byte, with their
pins and checksums. The v91 ULT, tN and TWL releases do not contain NUM, 1CH, 2CH, ECC, ISA,
JER or EZK. For those seven books the corpus generator uses the v89 row counts in
`../tc4/conformance-fixtures/check-density.json` and labels the rows as generated.

## `psa/`: Psalms

| Path | What it is | Source | sha256 |
|---|---|---|---|
| `en_ult/19-PSA.usfm` | unfoldingWord Literal Text, Psalms, aligned to the Hebrew: 2,461 verses, 21,600 `\zaln-s` groups, 42,292 words | Exported by translationCore on 2023-10-09 (its `\id` line). Its alignments were made against an older UHB than 3.0.0, so some Hebrew words differ, as in a real project after a resource upgrade. It differs from the Psalms of the ULT v91 (`en_ult/ingredients/PSA.usfm` in `bundle/en/en_ult_v91.sb.zip`); use it as a project made against an older release | `f3753ca75732d3e7a1738014f4e845a39041c2ce8df7b5797f728f63b7baabbe` |
| `hbo_uhb/19-PSA.usfm` | UHB 3.0.0 Psalms: 2,461 verses, 19,587 words | `hbo_uhb/ingredients/PSA.usfm` in `bundle/en/hbo_uhb_v3.0.0.sb.zip`, unchanged | `65ee8cad3b1f5a51f3b1def641720aceb2dfa24e167cca3bf7a69ba611c378de` |
| `en_tn/tn_PSA.tsv` | translationNotes v91, Psalms: 8,212 rows, 8,052 with a Hebrew quote | `en_tn/ingredients/PSA.tsv` in `bundle/en/en_tn_v91.sb.zip`, unchanged | `60e1b7f650731aff90d6670ae93735a03758725c62ad3806ed84265fb101c7e3` |
| `en_twl/twl_PSA.tsv` | translationWords Links v91, Psalms: 7,683 rows | `en_twl/twl_PSA.tsv` of the Door43 repository at the v91 commit `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64`, unchanged; `en_twl/ingredients/PSA.tsv` in `bundle/en/en_twl_v91.sb.zip` holds the same rows with each `TWLink` written `./payload/<category>/<slug>.md` (`bundle/en/README.md`) | `cb254eeaa6d9f442eccd642aa6cda862ec0f238ee330b1a5576269870fc8ccd4` |
| `web/web_psa.usx` | World English Bible British Edition, Psalms, USX 3.0: 2,461 verses | `test/test_data/usx/web_psa.usx` in https://github.com/Proskomma/proskomma-core at commit `0204ad3` | `a80457f92ac663a2d43777ac94f82ddfb8d4ff8b5bb0acf5e9659f138f029514` |

Licenses: the unfoldingWord resources are CC BY-SA 4.0,
© unfoldingWord. The World English Bible is in the public domain.
