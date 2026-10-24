# Print fixtures for `tc-export`

Tests read these files read-only. Do not edit them by hand: regenerate them with the
script the table names.

| File | What it is | How it was made |
|---|---|---|
| `print-vectors.json` | What tC4's own print code answers. `pageSetup`: `DEFAULT_PAGE_SETUP` and `PAGE_SPACING_FACTOR` (`src/data/export/pageSetup.ts`), `PRINT_LEADING`, `PAPER_MM` and `PAGE_MARGIN_MM` (`src/views/print/PrintBook.jsx`), and for each paper, column count and spacing, `printVariables` and the `@page` and `:root` rules of `printDocument` (`src/data/export/pdf.ts`). `books`: the Titus and Jonah samples (`../tc4/sample-burrito/ingredients/`), the three books of tC4's `test/export/pdf.test.ts`, five hand-written edge cases (no chapters, chapters out of order or repeated, a verse drafted with only a note, a chapter written `\c 01`) and 24 generated books (bridged verses, empty and `___` verses, notes, `\add` and `\w` markup, poetry, Arabic, Hebrew and Thai text, characters HTML escapes). For each, the input: the book model `bookModel` builds from the USFM (`byChapter` with each verse's `n`, `drafted` and `text`, and `chapterNums`); then `printedItems`, `partsOfItems`, the Community Checking callout's `undrafted` (`src/views/CommunityChecking.jsx`), whether `printDocument` refuses, and `printBookHtml` for each drop-cap and verse-number choice and each direction. `preview`: the preview's pages, from `paginate` and `partsOf` (`src/views/print/PrintPages.jsx`) with a `fits` that counts characters, and `PrintParts` rendered on each page with its `continues`. `stories`: the OBS sample (`../tc4/sample-burrito-obs/`), the six story sets of `pdf.test.ts`, a project with no story and 20 generated sets, read with `parseStory` (`journal/story.mjs`); the pictures each frame resolves to (a rule in the script, `pictureSources`); then `readPrintedStories` (`src/data/storyModel.ts`) and `printStoriesHtml` (`src/views/print/PrintStories.jsx`) for each layout, pictures choice and direction. `catalog`: the English strings of the keys the print shows. | `crates/tc-export/tools/gen-print-vectors.mjs`, which bundles those modules with the esbuild in tC4's `node_modules` (0.25.12) and tC4's React 18.3.1 and usfm-js 3.4.3 into a scratch file it removes. `atomsOf` and `carriesOn`, which `PrintPages.jsx` does not export, are taken from its source as written. The generated cases come from a seeded generator, so a second run writes the same file. |

`print-vectors.json` was generated on 2026-10-17 on Node 22.22.0 from tC4 commit `d310bf3`
(`git rev-parse HEAD` is recorded in the file as `tc4Commit`). The generator refuses a
checkout with local changes in `src/`, `journal/` or `conformance/`. To regenerate, run from
the repository root:

```sh
node crates/tc-export/tools/gen-print-vectors.mjs /path/to/translationcore4 > fixtures/export/print-vectors.json
```

## Licenses

The Titus, Jonah and OBS samples' Spanish text is CC BY-SA 4.0, Equipo Ejemplo (see
`../tc4/LICENSE-CONTENT.md`). The hand-written and generated cases were written for this
file and are in the public domain. The file holds what tC4's code (MIT)
computes, not the code.
