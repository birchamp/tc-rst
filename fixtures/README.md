# Test fixtures

Tests read these files read-only. Do not edit them.

## `tc4/`

Copied from translationCore 4 at commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`
(https://github.com/unfoldingWord/translationCore4), on 2026-10-14. Later copies name their
commit in their own README row: `tc4/test-fixtures/en_ult/GEN-1.usfm` and `GEN-773.usfm` come
from `9ad4d5c` (2026-10-19).

| Path here | Path in tC4 | Provenance |
|---|---|---|
| `tc4/test-fixtures/` | `test/fixtures/` | Read `tc4/test-fixtures/README.md`. Pinned unfoldingWord resources (en_ult, en_ust, hbo_uhb, helps) and draft corpora. |
| `tc4/conformance-fixtures/` | `conformance/fixtures/` | Import samples, team-sync journals, versification files, the OBS template. |
| `tc4/sample-burrito/` | `conformance/sample-burrito/` | The reference Bible project (Titus, Jonah). |
| `tc4/sample-burrito-obs/` | `conformance/sample-burrito-obs/` | The reference Open Bible Stories project. |
| `tc4/sb-schema/` | `conformance/sb-schema/` | The Scripture Burrito 1.0 JSON schema bundle, copied by tC4 from pankosmia `resource-core`. |

Licenses: the content licenses are in `tc4/LICENSE-CONTENT.md`. unfoldingWord resources
(ULT, UST, UHB, UGNT, tN, tW, tA, tQ, TWL, OBS) are CC BY-SA 4.0, © unfoldingWord. The
`.mjs` generator scripts are tC4 code under GPL-2.0-or-later, kept for provenance.

## `obs/`

What tC4's own OBS code answers, for `tc-obs`. Read `obs/README.md`.

## `scale/`

Real Psalms data for the performance budgets in `docs/PERFORMANCE.md` (the aligned ULT, the
UHB, the tN and TWL rows, and the WEB text). The unfoldingWord releases the corpus uses are
the bundled English suite in `../bundle/en/`. Read `scale/README.md` for provenance and
checksums.

## `lexicon/`

An excerpt of the bundled Hebrew lexicon as a `git archive` with its `fetched.lock`, and the
Strong's vectors computed by tC4's rule. Read `lexicon/README.md`.

## `export/`

What tC4's own print code answers, for `tc-export`'s print model. Read `export/README.md`.

## `parity-state/`

The files that make the parity scenarios' projects differ from the samples above, so cargo
tests read the exact bytes of `.state/<scenario>` without node. Read `parity-state/README.md`.
