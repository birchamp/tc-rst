# Helps fixtures that tC4 does not carry

Tests read these files read-only. Do not edit them.

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

License: the TSV rows are unfoldingWord® Translation Notes, © unfoldingWord, CC BY-SA 4.0
(https://creativecommons.org/licenses/by-sa/4.0/). The re-laid-out copy is an adaptation
under the same license. The manifest and config files carry no translated content.
