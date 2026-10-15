# Scale fixtures

The real data behind `docs/PERFORMANCE.md`. Psalms is the largest book, so every budget is
measured on it. The project owner supplied the unfoldingWord files on 2026-10-15.

## `resources/`: unfoldingWord releases, byte for byte

| File | Release | Contents | sha256 |
|---|---|---|---|
| `hbo_uhb-v3.0.0.zip` | Hebrew Bible (UHB) 3.0.0, issued 2026-04-12 | 39 books of USFM with lemma, Strong's and morphology | `c4abce7cad1532d9481b289fc3819d8ab0ca01ef41899e205117cd3dc8cac8b5` |
| `en_ult-v91.zip` | unfoldingWord Literal Text v91, issued 2026-09-26 (aligned to UHB 3.0.0 and UGNT 0.34) | 59 books, 23,900 verses, 369,635 `\zaln-s` groups | `f5c08f7a50ef88afe7b4811acdee352b0302e9f0bf1269f217d97121d2113d44` |
| `en_tn-v91.zip` | translationNotes v91, issued 2026-09-26 | 59 books, 87,573 rows | `45fed63b7e11594be6c6de288832be97285d9d11995cc968a0960be574ad4855` |
| `en_twl-v91.zip` | translationWords Links v91, issued 2026-09-26 (relates to UHB 3.0.0 and UGNT 0.34) | 59 books, 85,764 rows | `6b4979dc684fde52298f9a60e40281690eec148024558b2a8a012257b315f437` |
| `en_tw-v91.zip` | translationWords v91, issued 2026-09-26 | 953 articles | `78efd237fc9868ef44500305f3005b472111fde641af7cdff44cdcb10b1a2318` |
| `en_ta-v91.zip` | translationAcademy v91, issued 2026-09-26 | the translate, checking, process and intro manuals | `cd19a889437c70fabb1db4c9b87afebed463d872549edb91e14f26ac30c82a62` |

The v91 ULT, tN and TWL releases do not contain NUM, 1CH, 2CH, ECC, ISA, JER or EZK. For those
seven books the corpus generator uses the v89 row counts in
`../tc4/conformance-fixtures/check-density.json` and labels the rows as generated.

## `psa/`: Psalms

| Path | What it is | Source | sha256 |
|---|---|---|---|
| `en_ult/19-PSA.usfm` | unfoldingWord Literal Text, Psalms, aligned to the Hebrew: 2,461 verses, 21,600 `\zaln-s` groups, 42,292 words | Exported by translationCore on 2023-10-09 (its `\id` line). Its alignments were made against an older UHB than 3.0.0, so some Hebrew words differ, as in a real project after a resource upgrade. It differs from the Psalms in `resources/en_ult-v91.zip`; use it as a project made against an older release | `f3753ca75732d3e7a1738014f4e845a39041c2ce8df7b5797f728f63b7baabbe` |
| `hbo_uhb/19-PSA.usfm` | UHB 3.0.0 Psalms: 2,461 verses, 19,587 words | `hbo_uhb/19-PSA.usfm` in `resources/hbo_uhb-v3.0.0.zip`, unchanged | `65ee8cad3b1f5a51f3b1def641720aceb2dfa24e167cca3bf7a69ba611c378de` |
| `en_tn/tn_PSA.tsv` | translationNotes v91, Psalms: 8,212 rows, 8,052 with a Hebrew quote | `en_tn/tn_PSA.tsv` in `resources/en_tn-v91.zip`, unchanged | `60e1b7f650731aff90d6670ae93735a03758725c62ad3806ed84265fb101c7e3` |
| `en_twl/twl_PSA.tsv` | translationWords Links v91, Psalms: 7,683 rows | `en_twl/twl_PSA.tsv` in `resources/en_twl-v91.zip`, unchanged | `cb254eeaa6d9f442eccd642aa6cda862ec0f238ee330b1a5576269870fc8ccd4` |
| `web/web_psa.usx` | World English Bible British Edition, Psalms, USX 3.0: 2,461 verses | `test/test_data/usx/web_psa.usx` in https://github.com/Proskomma/proskomma-core at commit `0204ad3` | `a80457f92ac663a2d43777ac94f82ddfb8d4ff8b5bb0acf5e9659f138f029514` |

Still missing: the Greek New Testament (UGNT) and the UST. The Greek words of the New Testament are in the ULT alignments. Until
they are added, benchmarks generate what they need from the data above and label it as
generated.

Licenses: the unfoldingWord resources (UHB, ULT, tN, tW, TWL, tA) are CC BY-SA 4.0,
© unfoldingWord. The World English Bible is in the public domain.
