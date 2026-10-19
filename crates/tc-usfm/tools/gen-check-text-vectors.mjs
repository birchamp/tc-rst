// Generates fixtures/usfm/check-text-vectors.json: what tC4's Check screen and aligner read
// of each verse of a draft (`verseTextIndex` of src/state.jsx).
//
// For every verse tC4's indexer finds (`indexBook`, src/data/usfm/indexer.ts):
//
// - `drafted`: whether the verse is drafted, so that it has a text at all. The indexer's
//   body of the verse runs from after `\v N` to the first line that starts with a
//   paragraph-level marker, `\v` or `\c`; the verse is drafted when that body, trimmed, is
//   neither empty nor the stub `___`. A poetry verse whose text starts on a `\q1` line has
//   an empty body, so tC4 reads it as undrafted.
// - `text`, for a drafted verse: `verseText` of src/data/bookModel.js over usfm-js's
//   whole-book parse (`parseChapters`), joined and trimmed. It is what the Check screen
//   shows and selects in and what the aligner splits into words (`buildAlignmentSession`
//   of src/state.jsx). At d310bf3 it reads each verse object's `text`, or its children's
//   one level down; from 9ad4d5c on (#782) it is `tokensText(tokenizeVerse(vObj))` of
//   src/data/sourceHighlight.ts, which reads words at any depth, and PR #800 (5dcd147) adds
//   the space after a note between two words. No draft here nests an alignment or holds a
//   note between two words, so the file is the same from all three.
//
// The script runs tC4's own code: `indexBook`, `tokenizeVerse` and `tokensText` are imported
// from the checkout, and `verseTextIndex`, `parseChapters` and `verseText` are read out of
// src/state.jsx and src/data/bookModel.js and run as written (bookModel.js itself imports
// modules Node cannot load without tC4's bundler).
//
// Needs Node 22.18 or later, which runs tC4's TypeScript as it is. Run from the repository
// root with a tC4 tree (a checkout, or a `git archive` of a commit) whose node_modules hold
// usfm-js 3.4.3:
//   node crates/tc-usfm/tools/gen-check-text-vectors.mjs <tC4 tree> > fixtures/usfm/check-text-vectors.json
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const tc4 = process.argv[2];
if (!tc4) throw new Error("usage: gen-check-text-vectors.mjs <tC4 checkout>");
const nm = path.join(tc4, "node_modules");
const pinned = { "usfm-js": "3.4.3" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfm = require(path.join(nm, "usfm-js"));
const { indexBook } = await import(pathToFileURL(path.join(tc4, "src/data/usfm/indexer.ts")).href);
const { tokenizeVerse, tokensText } = await import(
  pathToFileURL(path.join(tc4, "src/data/sourceHighlight.ts")).href
);

/** The source of one top-level declaration of `file`, from `start` to the line that closes it. */
const declaration = (file, start, end) => {
  const src = fs.readFileSync(path.join(tc4, file), "utf8");
  const at = src.indexOf(start);
  if (at < 0) throw new Error(`${file}: no ${start}`);
  const stop = src.indexOf(end, at);
  if (stop < 0) throw new Error(`${file}: ${start} does not end with ${JSON.stringify(end)}`);
  return src.slice(at, stop + end.length).replace(/^export /, "");
};
const parseChapters = new Function(
  "usfm",
  `${declaration("src/data/bookModel.js", "export const parseChapters = ", "\n};")} return parseChapters;`,
)(usfm);
// Unused at d310bf3, where verseText reads the objects itself.
const verseText = new Function(
  "tokenizeVerse",
  "tokensText",
  `${declaration("src/data/bookModel.js", "export const verseText = ", ".trim();")} return verseText;`,
)(tokenizeVerse, tokensText);
const verseTextIndex = new Function(
  "parseChapters",
  "indexBook",
  "verseText",
  `${declaration("src/state.jsx", "function verseTextIndex(bookRaw) {", "\n}")} return verseTextIndex;`,
)(parseChapters, indexBook, verseText);

const quiet = (f) => {
  const { error, warn, log } = console;
  console.error = console.warn = console.log = () => {};
  try {
    return f();
  } finally {
    Object.assign(console, { error, warn, log });
  }
};

// Every plain draft among tC4's fixtures. en_ult and en_ust are left out: a project's book
// never holds alignment markup (I-1).
const files = [
  "fixtures/tc4/sample-burrito/ingredients/TIT.usfm",
  "fixtures/tc4/sample-burrito/ingredients/JON.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-partial-book.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/sample-JON-span.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/sample-TIT-draft.usfm",
  "fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm",
  "fixtures/tc4/conformance-fixtures/import/usfm/32-JON.usfm",
  "fixtures/tc4/conformance-fixtures/import/usfm/57-TIT.usfm",
  "fixtures/tc4/conformance-fixtures/import/usfm/no-id.sfm",
  "fixtures/tc4/conformance-fixtures/import/damaged/57-TIT-unclosed.usfm",
];
// Drafts written for the rules above: poetry verses whose text starts on a line of its own,
// a stub followed by poetry, a superscription, list items, a `\qs` Selah, CRLF line ends,
// headings and footnotes, and verse markers inside a line.
const drafts = {
  poetry:
    "\\id PSA\n\\c 1\n\\v 1\n\\q1 Blessed is the man\n\\q2 who walks not\n\\v 2 ___\n\\q2 in the counsel\n" +
    "\\v 3 \\q1 He is like a tree\n\\v 4 \n\\q1 The wicked are not so\n\\v 5 Therefore\n\\q2 the wicked will not stand\n" +
    "\\v 6\n\\b\n\\q1 For the LORD knows\n\\v 7\n\\v 8 After an empty verse.\n",
  superscription:
    "\\id PSA\n\\c 3\n\\d A psalm of David, when he fled.\n\\q1\n\\v 1 \\d To the chief musician.\n\\q1 LORD, how many\n" +
    "\\q2 are my foes! \\qs Selah\\qs*\n\\v 2 Many are saying\n\\q2 of my soul,\n\\qs Selah\\qs*\n",
  lists:
    "\\id NUM\n\\c 1\n\\p\n\\v 1 The list has these members:\n\\li1 the first member,\n\\li2 a nested member,\n" +
    "\\li1 the last member.\n\\m The list ends here.\n\\v 2\n\\li1 a list item first,\n\\li1 then another.\n",
  crlf: "\\id TIT\r\n\\c 1\r\n\\v 1\r\n\\q1 Paul a servant\r\n\\v 2 Of God\r\n\\q2 and an apostle\r\n\\v 3 ___\r\n\\p\r\n",
  notes:
    "\\id TIT\n\\c 1\n\\s1 A heading\n\\p\n\\v 1 Paul\\f + \\ft A note.\\f*, a \\add servant\\add* of God\n\\s1 Next heading\n" +
    "\\p\n\\v 2 In hope \\w of|strong=\"G1\"\\w* life\n\\v 3-4 A span\n\\q1 over two verses.\n",
  // The ULT's layout: a `\v` after `\q1 ` on its line, which tC4's indexer does not read.
  midline:
    "\\id PSA\n\\c 115\n\\q1\n\\v 12 Yahweh renew us;\n\\q1 he will bless.\n\\q1 \\v 13 He will bless those fearing Yahweh,\n" +
    "\\q1 the small with the great.\n\\q1 \\v 14 ___\n\\q1 \\v 15 May you be blessed \\v 16 by Yahweh.\n",
};

const cases = [];
const add = (book, raw) => {
  const index = quiet(() => verseTextIndex(raw));
  for (const e of indexBook(raw)) {
    const key = `${e.chapter}:${e.verseKey}`;
    const drafted = Object.hasOwn(index, key);
    cases.push({ book, chapter: e.chapter, verse: e.verseKey, drafted, text: drafted ? index[key] : null });
  }
};
const books = {};
for (const file of files) add(file, fs.readFileSync(file, "utf8"));
for (const [name, raw] of Object.entries(drafts)) {
  books[name] = raw;
  add(name, raw);
}
process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-check-text-vectors.mjs",\n "libraries": ${JSON.stringify(pinned)},\n` +
    ` "drafts": ${JSON.stringify(books)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n",
);
