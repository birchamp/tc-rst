# Helps fixtures

Tests read these files read-only. Do not edit them.

## Constructed (tC4 does not carry these)

The tC4 fixtures (`../tc4/test-fixtures/resources/`) are real DCS exports, but they lack
three formats that the helps parsers also read: the legacy nine-column tN TSV, an RC
`manifest.yaml`, and a tA `config.yaml`. `git.door43.org` is blocked from the machine that
made these fixtures, so real copies could not be fetched. The files below are
**constructed**, and each row says how. Replace them with real copies when a machine with
Door43 access can fetch them, and record the source here.

| Path | How it was made | Based on |
|---|---|---|
| `tn-tsv9/en_tn_57-TIT.tsv` | `make_tsv9.py` (in this directory) lays out the first 12 rows of the real en_tn v86 Titus TSV in the legacy nine-column layout: `Book Chapter Verse ID SupportReference OrigQuote Occurrence GLQuote OccurrenceNote`. Newlines become `<br>`, two rows carry a GLQuote taken from the en_ult Titus 1:1 rendering, and every third SupportReference is cut to the bare slug, the older form. | `../tc4/test-fixtures/resources/en_tn@v86/TIT.tsv` (rev `c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5`). The layout follows the research note `door43-resources.md` §2.6, which is from memory and marked unverified. |
| `rc/en_tn/manifest.yaml` | Written by hand in the RC 0.2 shape. The values are en_tn's: identifier `tn`, subject `TSV Translation Notes`, version `86`, and two projects with the `tn_<BOOK>.tsv` paths. | uW-Tools-Collab `docs/2-unfoldingword-developer-guide.mdx` and `docs/5-migration/tc-to-rc/new-rc-creation.mdx` (manifest shape); research note `door43-resources.md` §2.1 and §2.5 (subject string, file names). |
| `ta/translate-config.yaml` | Written by hand: four article slugs, each with `dependencies` and `recommended` lists, including an empty list (`[]`) and a missing key. | Research note `door43-resources.md` §2.9 (from memory, marked unverified). |

## Generated from tC4

| Path | What it is | How it was made |
|---|---|---|
| `v91-tsv-rows.json` | For every TSV of the en_tn, en_twl and en_tq v91 burrito exports in `../../bundle/en/` (59 books each), the number of rows tC4's TSV reader returns (`rows`), the md5 of their identities in order (`md5`: one `checkId\|chapter\|verse\|occurrence` line per row, joined by a newline), and the lines it skips (`skipped`, counted from 1 at the header). tC4's `tsvRows` (`src/data/derive.ts`) skips only lines whose `trim()` is empty; in these files that is the empty line after the final newline. `inputs` names each export and its sha256. | `crates/tc-check/tools/gen-psalms-vectors.cjs`, which bundles tC4's own `src/data/derive.ts` with the esbuild in tC4's `node_modules` and runs `deriveTnItems` (with `keepPlain`), `deriveTwlItems` and `deriveTqItems` on each file. Generated on 2026-10-15 on Node 22.22.0 from the tC4 checkout `../tc4/` came from (commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`, version 4.0.0-rc.1); the generator reads the commit from the checkout's git and refuses a checkout with local changes in `src/`. The same run writes `../check/psalms-vectors.json`. To regenerate, run `node crates/tc-check/tools/gen-psalms-vectors.cjs /path/to/translationcore4` from the repository root. |

License: the TSV rows are unfoldingWord® Translation Notes, © unfoldingWord, CC BY-SA 4.0
(https://creativecommons.org/licenses/by-sa/4.0/). The re-laid-out copy is an adaptation
under the same license. The manifest and config files carry no translated content.
