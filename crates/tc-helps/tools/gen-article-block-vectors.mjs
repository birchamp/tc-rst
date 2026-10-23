// Generates fixtures/helps/article-block-vectors.json: the blocks tC4's `renderArticleBlocks`
// gives every Translation Notes row of the Titus fixtures (en_tn v91, es-419_tn v66), the
// Translation Academy and Translation Words articles of the fixtures, and a few lines written
// here for the marks tC4's comments name (#619); and what its `takeSpans` gives each block's
// spans at half the block's length.
//
// A note's input is its TSV `Note` cell with the escaped line breaks made real, as tC4's
// `ExpandableNote` passes it (`text.replace(/\\n/g, '\n')`). articles.ts imports only a type,
// so the script takes it from tC4 commit d310bf3 with `git show` and runs it as written (Node
// 22.18 or later runs TypeScript). Run from the repository root:
//   node crates/tc-helps/tools/gen-article-block-vectors.mjs <tC4 repository> \
//     > fixtures/helps/article-block-vectors.json
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const COMMIT = "d310bf3";
const FILE = "src/data/articles.ts";
const [tc4] = process.argv.slice(2);
if (!tc4) throw new Error("usage: gen-article-block-vectors.mjs <tC4 repository>");

const git = (...args) => execFileSync("git", ["-C", tc4, ...args], { encoding: "utf8" });
const commit = git("rev-parse", COMMIT).trim();
const blob = git("rev-parse", `${COMMIT}:${FILE}`).trim();
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "article-blocks-"));
const copy = path.join(dir, "articles.ts");
fs.writeFileSync(copy, git("show", `${COMMIT}:${FILE}`));
const { renderArticleBlocks, takeSpans } = await import(pathToFileURL(copy).href);
fs.rmSync(dir, { recursive: true });

const root = "fixtures/tc4/test-fixtures/resources";
const sha256 = (file) => crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const inputs = {};
const cases = [];

for (const file of ["en_tn@v91/TIT.tsv", "es-419_tn@v66/TIT.tsv"]) {
  const full = path.join(root, file);
  inputs[file] = sha256(full);
  const [header, ...lines] = fs.readFileSync(full, "utf8").split("\n");
  const note = header.split("\t").indexOf("Note");
  for (const line of lines) {
    const cell = line.split("\t")[note];
    if (cell) cases.push(cell.replace(/\\n/g, "\n"));
  }
}
for (const file of ["en_ta@v86/translate-figs-abstractnouns-01.md", "en_tw@v87/payload-kt-god.md"]) {
  const full = path.join(root, file);
  inputs[file] = sha256(full);
  cases.push(fs.readFileSync(full, "utf8"));
}
cases.push(
  "> A quoted line with **bold** and *italic*\n> > nested __Israel __ and figs_metaphor",
  "* an item with ***both*** marks\n- another with rc://*/ta/man/translate/figs-123 and \\* kept",
  "###### six\n####### seven is a paragraph\n#no space is a paragraph",
  "17:2__David__ and a blank ________ and _edge_ and in_word_mark",
  "[[rc://en/tw/dict/bible/kt/god]] and [a label](../01/02.md) and [](empty)",
  "   \n\t\n",
);

// A span as tC4 makes it: `bold` and `italic` only when set.
const shown = (spans) => spans.map((s) => ({ text: s.text, ...(s.bold ? { bold: true } : {}), ...(s.italic ? { italic: true } : {}) }));
const vectors = cases.map((input) => {
  const blocks = renderArticleBlocks(input).map((b) => ({
    kind: b.kind,
    level: b.level ?? null,
    quote: b.quote ?? 0,
    spans: shown(b.spans),
    // tC4's `text` is the spans joined; the test checks that and takeSpans at half of it.
    half: shown(takeSpans(b.spans, Math.floor(b.text.length / 2))),
  }));
  return { input, blocks };
});

process.stdout.write(
  `${JSON.stringify({ tc4: { commit, file: FILE, blob }, node: process.version, inputs, vectors })}\n`,
);
