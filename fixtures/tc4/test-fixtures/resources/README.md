# Vendored resource fixtures (derive-at-full-strength — OPEN-QUESTIONS #15)

Real one-book slices of published Door43 resources, cut from the DCS **sb-zip
export** of each pinned release (the pin path decided in OPEN-QUESTIONS #24).
Fetched 2026-08-03. Do not edit these files — refetch to update, and record the
new provenance here.

Each directory is `<repo>@<tag>/` and contains the slice plus the export's own
`metadata.json` **unmodified**. The metadata is the authoritative provenance:
`identification.primary.dcs[...].revision` is the tag's commit SHA, verified
against the project pins by the test suite. This README adds only what the
metadata cannot say: the fetch date, the pin slot each fixture serves, and the
fact that these are one-book slices of the full export.

| Directory | Source (sb-zip) | Tag commit SHA (from export metadata) | Pin slot served |
|---|---|---|---|
| `en_tn@v86/` | `git.door43.org/unfoldingWord/en_tn/sb/v86.zip` | `c354b8ae66a23c485bf6f38fd35bd8f7ef81e4e5` | fallback `translationNotes` |
| `en_twl@v86/` | `git.door43.org/unfoldingWord/en_twl/sb/v86.zip` | `570e76d0024c847689e48a20e2ac1a1d2c6eb6e3` | fallback `translationWordsLinks` |
| `en_twl@v91/` | `git.door43.org/unfoldingWord/en_twl/sb/v91.zip` (fetched 2026-10-12 through `dev-env/scripts/cache-resource.zsh`; `metadata.json` only, no slice — its `ingredients` list the 59 book TSVs the release contains; issue #644, D96) | `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64` | fallback `translationWordsLinks` (the shipped pin) |
| `es-419_tn@v66/` | `git.door43.org/Es-419_gl/es-419_tn/sb/v66.zip` | `22f3d0c61e2ab4701cb869547de9c3c43da07208` | primary `translationNotes` |
| `en_ta@v86/` | `git.door43.org/unfoldingWord/en_ta/sb/v86.zip` | `c7caddfb474efd713f36b35a3ffc927866c7b180` | fallback `translationAcademy` |
| `en_obs-tn@v13/` | `git.door43.org/unfoldingWord/en_obs-tn/sb/v13.zip` (fetched 2026-09-17 through `dev-env/scripts/cache-resource.zsh`) | `e86138ea13f619f09f7a6dcaa60592716d407fe4` | fallback `obs-tn` (§10.6) |
| `en_obs-twl@v3/` | `git.door43.org/unfoldingWord/en_obs-twl/sb/v3.zip` (fetched 2026-09-17, same path) | `44ebc9fafe8101665f985007d566f5036a2be85b` | fallback `obs-twl` (§10.6) |
| `en_obs-tq@v10/` | `git.door43.org/unfoldingWord/en_obs-tq/sb/v10.zip` (fetched 2026-09-18, same path; the whole `OBS.tsv`, 672 rows) | `01b92fe8793d62cff3a2221f5174c768cbad3dc1` | fallback `obs-tq` (§10.6, R-10.6.3) |
| `en_tw@v91/` | `git.door43.org/unfoldingWord/en_tw/sb/v91.zip` (copied 2026-10-12 from the rig cache `dev-env/resources-cache/en_tw-v91-unwrapped.zip`; the whole `2SA.tsv`, 3316 rows, and `TIT.tsv`, 182 rows, plus the export's `metadata.json`, 1021 ingredients) | `ff5b3852c27c3a0d01b109e482eb26047dcd20e2` | the shipped `translationWords` and `translationWordsLinks` pin (installedSuite.js); the §4.2 tw article gate (issue #643): `2SA.tsv` row `vwrw` links `kt/arcofthecovenant.md`, which the export does not carry |

Slice contents (paths flattened, `ingredients/` prefix dropped, `/` → `-`):

- `*_tn/twl`: `TIT.tsv` — the whole Titus TSV, byte-identical to the export.
- `en_ta`: `translate-toc.yaml` (the translate manual's toc — the tN groupId
  target space) + one full article (`translate-figs-abstractnouns-01.md` +
  `title.md`) as the article-shape fixture.

Ground truth (counted at vendor time; the test suite asserts these):

- `en_tn@v86/TIT.tsv`: 206 data rows; 49 have no SupportReference (plain notes,
  never checks) → **157 derivable items**, 28 distinct groupIds, all 28 present
  in the tA v86 translate toc. Category distribution through the tC3 map:
  grammar 71, figures 53, culture 26, other 6, discourse 1.
- `en_twl@v86/TIT.tsv`: **188 derivable items**; TWLink categories kt 111,
  other 71, names 6.
- `es-419_tn@v66/TIT.tsv`: 216 data rows → **112 derivable items**; 89 of the
  157 en checkIds also exist in es (cross-language re-attach exercises both the
  checkId-match and the fallback-key paths on real data).
- `en_obs-tn@v13/OBS.tsv`: the WHOLE export — one file covers the fifty
  stories; `Reference` is `story:frame`, frame 0 the story title. 2325 data
  rows; 58 are title notes (`N:0`) and none of those carries a SupportReference,
  so for OBS every row is a check (**2325 items**; issue #291). `Occurrence` is
  1 in every row.
- `en_obs-twl@v3/OBS.tsv`: the whole export, **2381 items**; `Occurrence` is 1
  in every row (D74 §8, R-10.5.1); no frame-0 rows; story 1 has 35.

Adding a book: create the same files for the new book in each directory (same
export, same tag) and extend the suite's per-book expectations — the layout is
per-book by design (multiple books are the declared next step).
