// Generates fixtures/usfm/export-vectors.json: which verses of the ULT v91, the UST v91 and
// the scale Psalms tC4's own aligned-USFM export does not write back as published.
//
// For each book the script runs tC4's export on the book as tC4 holds it after importing
// the published file:
//
// - the book is cut into verse slots by tC4's `decompose` (journal/skeleton.mjs:21): a
//   verse's content runs from after `\v N ` to the next `\v ` or `\c `;
// - each slot with `\zaln` markup gets the §5.1 record word-aligner-lib 1.0.1 reads out of
//   it (`extractAlignmentsFromTargetVerse`), with the `targetVerseMd5` of its text
//   (`verseTextMd5`, journal/fold.mjs:24), and is stored without the markup
//   (`cleanAlignmentMarkersFromString`), as tC4 keeps no `\zaln` at rest;
// - `weaveBook` (src/data/export/weave.mjs:34) writes the book with those records, which
//   calls `addAlignmentsToTargetVerseUsingMerge` for each verse (weave.mjs:41) and keeps
//   the stored content where the merge fails.
//
// Then each slot of the exported book is compared with the published one:
//
// - `unaligned`: the merge failed, so the export writes the verse without its alignment;
// - `changed`: the export writes other verse text (markup and layout aside), such as
//   `s uch` for a `such` written as the two words `\w s\w*\w uch\w*`;
// - `rewritten`: the export writes the same text with other markup or layout;
// - `spaced`: the export writes the verse back but for one more space after the paragraph
//   marker that ends it (`\q1  ` for `\q1 `), which is otherwise the same;
// - `same`: the export writes the verse back as published.
//
// Run from the repository root with the tC4 checkout whose node_modules hold the pinned
// libraries (Node 22.18 or later, which runs tC4's TypeScript as it is):
//   node crates/tc-usfm/tools/gen-export-vectors.mjs <tC4 checkout> > fixtures/usfm/export-vectors.json
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const tc4 = process.argv[2];
if (!tc4) throw new Error("usage: gen-export-vectors.mjs <tC4 checkout>");
const nm = path.join(tc4, "node_modules");
const pinned = { "usfm-js": "3.4.3", "word-aligner-lib": "1.0.1" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));
const AlignmentHelpers = require(path.join(nm, "word-aligner-lib/dist/index.cjs.js")).AlignmentHelpers;
const { UsfmFileConversionHelpers } = require(path.join(nm, "word-aligner-lib/dist/index.cjs.js"));
const usfmjs = require(path.join(nm, "usfm-js"));
const tc4Module = (file) => import(pathToFileURL(path.join(tc4, file)).href);
const { tokenizeVerse, tokensText } = await tc4Module("src/data/sourceHighlight.ts");
const { decompose } = await tc4Module("journal/skeleton.mjs");
const { verseTextMd5 } = await tc4Module("journal/fold.mjs");
const { weaveBook } = await tc4Module("src/data/export/weave.mjs");

// The libraries log their failures; the result says them.
const quiet = (f) => {
  const { error, warn, log } = console;
  console.error = console.warn = console.log = () => {};
  try {
    return f();
  } finally {
    Object.assign(console, { error, warn, log });
  }
};
// A stored §5.1 record's words (gen-weave-vectors.cjs): word-aligner writes a milestone's
// attributes in the key order of its top word.
const topWord = (w) => {
  const o = {};
  for (const k of ["strong", "lemma", "morph"]) if (w[k] !== undefined) o[k] = w[k];
  o.occurrence = Number(w.occurrence);
  o.occurrences = Number(w.occurrences);
  o.word = w.word;
  return o;
};
const bottomWord = (w) => ({ word: w.word, occurrence: Number(w.occurrence), occurrences: Number(w.occurrences) });

const exportBook = (source, usfm) => {
  const { verses } = decompose(usfm);
  const keys = Object.keys(verses);
  // Where each slot stands, found as decompose finds it, to put the book back together
  // with every aligned slot stored without its markup: the book at rest in tC4.
  const re = /\\v[ \t]+(\d+(?:-\d+)?)[ \t]/g;
  const pieces = [];
  let at = 0;
  for (const key of keys) {
    re.lastIndex = at;
    const start = re.exec(usfm) && re.lastIndex;
    const next = usfm.slice(start).search(/\\[vc][ \t]/);
    const end = next < 0 ? usfm.length : start + next;
    if (usfm.slice(start, end) !== verses[key]) throw new Error(`${source} ${key}: not where decompose put it`);
    pieces.push(usfm.slice(at, start), verses[key]);
    at = end;
  }
  pieces.push(usfm.slice(at));
  const chapters = {};
  let aligned = 0;
  keys.forEach((key, n) => {
    const content = verses[key];
    if (!content.includes("\\zaln-s")) return;
    aligned++;
    const record = quiet(() => AlignmentHelpers.extractAlignmentsFromTargetVerse(content, null));
    const clean = UsfmFileConversionHelpers.cleanAlignmentMarkersFromString(content);
    const [chapter, verse] = key.split(":");
    chapters[chapter] ??= {};
    chapters[chapter][verse] = {
      alignments: record.alignment.map((a) => ({ topWords: a.topWords.map(topWord), bottomWords: a.bottomWords.map(bottomWord) })),
      wordBank: record.wordBank.map(bottomWord),
      invalid: false,
      targetVerseMd5: verseTextMd5(clean),
      sourceVersion: "published",
    };
    pieces[2 * n + 1] = clean;
  });
  const stored = pieces.join("");
  const exported = decompose(quiet(() => weaveBook(stored, { chapters }, AlignmentHelpers))).verses;
  // The verse text tC4's panes show (`verseText`, src/views/verseText.js:9), not the text
  // tC4 stores (`cleanAlignmentMarkersFromString` already writes `s uch` for `such`).
  const text = (usfm) => {
    const chunk = usfmjs.toJSON(`\\v 1 ${usfm}`, { chunk: true });
    return tokensText(tokenizeVerse(chunk.verses?.["1"])).replace(/\s+/g, " ").trim();
  };
  const result = { source, slots: keys.length, aligned, same: 0, spaced: 0, rewritten: [], changed: [], unaligned: [] };
  for (const key of keys) {
    const [published, written] = [verses[key], exported[key]];
    const record = chapters[key.split(":")[0]]?.[key.split(":")[1]];
    if (written === published) result.same++;
    else if (written === `${published} ` && /\\q\d? $/.test(published)) result.spaced++;
    else if (record && written === UsfmFileConversionHelpers.cleanAlignmentMarkersFromString(published)) result.unaligned.push(key);
    else if (text(written) !== text(published)) result.changed.push(key);
    else result.rewritten.push(key);
  }
  return result;
};

const books = [];
for (const archive of ["bundle/en/en_ult_v91.sb.zip", "bundle/en/en_ust_v91.sb.zip"]) {
  const files = unzipSync(fs.readFileSync(archive), { filter: (f) => f.name.endsWith(".usfm") });
  for (const entry of Object.keys(files).sort()) books.push(exportBook(`${archive} ${entry}`, strFromU8(files[entry])));
}
const scale = "fixtures/scale/psa/en_ult/19-PSA.usfm";
books.push(exportBook(scale, fs.readFileSync(scale, "utf8")));

process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-export-vectors.mjs",\n "libraries": ${JSON.stringify(pinned)},\n "books": [\n` +
    books.map((b) => "  " + JSON.stringify(b)).join(",\n") +
    "\n]}\n",
);
