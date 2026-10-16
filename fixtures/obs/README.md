# Open Bible Stories fixtures for `tc-obs`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
script the table names.

| File | What it is | How it was made |
|---|---|---|
| `obs-vectors.json` | What tC4's own OBS code answers. `parse`: `parseStory` (`journal/story.mjs`) on the fifty stories of the template (`../tc4/conformance-fixtures/text_stories`), of `../tc4/sample-burrito-obs` and of the en_obs v9 export (`../../bundle/en/en_obs_v9.sb.zip`), as a digest of each, and on 36 hand-written and 600 generated story files (random spacing, blank lines JavaScript's `trim()` empties and three it does not, title, image and reference forms valid and not, carriage returns), as the full result or the error message. `writes`: `seedStory`, `writeFrame`, `writeTitle`, `writeRef` and `applyStoryState` on those files and on five stories of each set, as the text, its md5 and UTF-16 length, or the error message. `textRules`: `frameTextError`, `titleTextError` and `refTextError` on edge strings. `images`: `obsImageFileName` on image lines (URL forms, percent escapes, quoted titles), `obsImagePackFromPaths` on path lists, and `resolveObsImage` on the picture ladder's cases, empty uris among them (`src/data/obsImages.ts`). `frameSet`: `obsFrameSetMismatch`. `print`: `printedStories` and `storyDrafted` (`src/data/storyModel.ts`). `recency`: `recordRecentStory` (`src/data/obsRecency.ts`). `toNumber`: JavaScript's `Number(string)`, which tC4's `src/state.jsx` applies before it opens a story. `localizedNames`: `withLocalizedNames` (`scripts/fix-obs-template.mjs`). `questions`: `deriveObsItems(tsv, 'translationQuestions', story)` (`src/data/derive.ts`) on small OBS Translation Questions tables, with no story and with stories 1 and 2. `draft`: story 1 of `conformance/generate-obs.mjs`, drafted from `conformance/fixtures/obs-draft.mjs`; the generator also checks that stories 2 to 50 of the sample are the template's seed forms. | `crates/tc-obs/tools/gen-obs-vectors.mjs`, which imports tC4's `journal/story.mjs`, `scripts/fix-obs-template.mjs` and `conformance/fixtures/obs-draft.mjs`, compiles the TypeScript modules with the esbuild in tC4's `node_modules` (0.25.12), bundling `derive.ts` with its imports into a scratch file it removes, and reads the zip with tC4's `fflate` (0.8.2). |

`obs-vectors.json` was generated on 2026-10-16 on Node 22.22.0 from tC4 commit
`d310bf3` (`git rev-parse HEAD` is recorded in the file as `tc4Commit`). The generator
refuses a checkout with local changes in `src/`, `journal/`, `scripts/` or `conformance/`.
To regenerate, run from the repository root:

```sh
node crates/tc-obs/tools/gen-obs-vectors.mjs /path/to/translationcore4 > fixtures/obs/obs-vectors.json
```

## Licenses

The stories come from the template (unfoldingWord® Open Bible Stories, © unfoldingWord,
CC BY-SA 4.0), the sample project (its Spanish text CC BY-SA 4.0, Equipo Ejemplo; see
`../tc4/LICENSE-CONTENT.md`) and the en_obs v9 release (CC BY-SA 4.0, © unfoldingWord).
The hand-written and generated cases were written for this file and are in the public
domain. The file holds what tC4's code (GPL-2.0-or-later) computes, not the code.
