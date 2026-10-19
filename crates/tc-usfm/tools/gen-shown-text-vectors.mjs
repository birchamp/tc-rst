// Generates fixtures/usfm/shown-text-vectors.json: the text tC4's Translate, Check and Align
// show for a verse (`verseText` of src/data/bookModel.js, which reads the verse's usfm-js verse
// objects through `tokenizeVerse` of src/data/sourceHighlight.ts since tC4 9ad4d5c, #782), for
// every verse usfm-js finds in:
//
// - tC4's aligned test fixtures (en_ult and en_ust Titus, en_ult Genesis 1:1, Genesis 1 and
//   Psalm 3:1-2) and the exotic corpus, with the text itself;
// - drafts (held in the file's `drafts`) with notes and other markers between words (tC4 PR
//   #800's cases on the sample Titus 1:1, and hand-written ones), with the text itself;
// - Genesis and Psalms of the ULT v91 and UST v91 in bundle/en/, with the md5 of the text.
//
// Every usfm-js verse key is listed, not only the verses tC4's indexer finds (as
// gen-check-text-vectors.mjs does), because a verse whose `\v` stands after `\q1 ` on its line
// is still shown, and tC4's Psalm 3 case is such a verse.
//
// The first tree is the reference for `text` and `md5`: tC4 PR #800's head 5dcd147, which adds
// the space after a note between two words (#795) to 9ad4d5c's walk. With a second tree (tC4
// main, 9ad4d5c), a text case whose text there differs also records that text as `main`, and
// the script stops if any bundle verse differs between the two trees.
//
// The script runs tC4's own code: `tokenizeVerse` and `tokensText` are imported from each tree,
// and each tree's `verseText` is read out of its src/data/bookModel.js and run as written.
// Needs Node 22.18 or later, which runs tC4's TypeScript as it is. Run from the repository root
// with trees of tC4 (a `git archive` of each commit with `src`, and a `node_modules` that holds
// usfm-js 3.4.3 and fflate 0.8.2, tC4's pins):
//   node crates/tc-usfm/tools/gen-shown-text-vectors.mjs <tC4 tree> <tC4 commit> [<main tree> <main commit>] > fixtures/usfm/shown-text-vectors.json
import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const [tc4, commit, mainTree, mainCommit] = process.argv.slice(2);
if (!tc4 || !commit) throw new Error("usage: gen-shown-text-vectors.mjs <tC4 tree> <tC4 commit> [<main tree> <main commit>]");
if (mainTree && !mainCommit) throw new Error("a main tree needs its commit");
const nm = path.join(tc4, "node_modules");
const pinned = { "usfm-js": "3.4.3" };
for (const [name, version] of Object.entries({ ...pinned, fflate: "0.8.2" })) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfm = require(path.join(nm, "usfm-js"));
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));

// bookModel.js's own verseText, run as written with the functions it imports.
const verseTextOf = async (tree) => {
  const { tokenizeVerse, tokensText } = await import(pathToFileURL(path.join(tree, "src/data/sourceHighlight.ts")).href);
  const src = fs.readFileSync(path.join(tree, "src/data/bookModel.js"), "utf8");
  const at = src.indexOf("export const verseText = ");
  const stop = src.indexOf(".trim();", at);
  if (at < 0 || stop < 0) throw new Error(`${tree}: no verseText in src/data/bookModel.js`);
  const decl = src.slice(at, stop + ".trim();".length).replace(/^export /, "");
  return new Function("tokenizeVerse", "tokensText", `${decl} return verseText;`)(tokenizeVerse, tokensText);
};
const verseText = await verseTextOf(tc4);
const mainText = mainTree ? await verseTextOf(mainTree) : null;

const quiet = (f) => {
  const { error, warn, log } = console;
  console.error = console.warn = console.log = () => {};
  try {
    return f();
  } finally {
    Object.assign(console, { error, warn, log });
  }
};
const md5 = (text) => crypto.createHash("md5").update(text, "utf8").digest("hex");

const cases = [];
const add = (book, raw, digest) => {
  const chapters = quiet(() => usfm.toJSON(raw)).chapters || {};
  for (const [chapter, verses] of Object.entries(chapters)) {
    for (const [verse, object] of Object.entries(verses)) {
      if (verse === "front") continue;
      const text = verseText(object);
      if (digest) {
        cases.push({ book, chapter, verse, md5: md5(text) });
        if (mainText && mainText(object) !== text) throw new Error(`${book} ${chapter}:${verse}: tC4 main differs on a bundle verse`);
        continue;
      }
      const main = mainText ? mainText(object) : text;
      cases.push(main === text ? { book, chapter, verse, text } : { book, chapter, verse, text, main });
    }
  }
};

const files = [
  "fixtures/tc4/test-fixtures/usfm-corpus/en_ult-TIT-aligned.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/en_ust-TIT-aligned.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm",
  "fixtures/tc4/test-fixtures/en_ult/GEN-1-1.usfm",
  "fixtures/tc4/test-fixtures/en_ult/GEN-1.usfm",
  "fixtures/tc4/test-fixtures/en_ult/PSA-3.usfm",
];
for (const file of files) add(file, fs.readFileSync(file, "utf8"), false);

// tC4 PR #800's cases (test/sourceHighlight.test.ts, "a note between two words (#795)"),
// made from the sample Titus as that test makes them; and hand-written ones.
// Only Titus 1:1 of the sample, the verse every case changes, as a book of its own.
const sample = fs.readFileSync("fixtures/tc4/sample-burrito/ingredients/TIT.usfm", "utf8");
const line = sample.split("\n").find((l) => l.startsWith("\\v 1 Pablo"));
if (!line) throw new Error("the sample Titus has no 1:1 starting with Pablo");
const tit = `\\id TIT\n\\c 1\n\\p\n${line}\n`;
const titus = (find, replace) => {
  if (!tit.includes(find)) throw new Error(`the sample Titus has no ${find}`);
  return tit.replace(find, replace);
};
const drafts = {
  "note-f": titus("siervo de", "siervo\\f + \\fr 1:1 \\ft o esclavo\\f* de"),
  "note-x": titus("siervo de", "siervo\\x - \\xt Rom 1:1\\x* de"),
  "note-fe": titus("siervo de", "siervo\\fe + \\ft o esclavo\\fe* de"),
  "note-ef": titus("siervo de", "siervo\\ef + \\ft o esclavo\\ef* de"),
  "note-fig": titus("siervo de", 'siervo\\fig a|src="b"\\fig* de'),
  "note-after-space": titus("siervo de", "siervo \\f + \\ft o esclavo\\f* de"),
  "two-notes": titus("siervo de", "siervo\\f + \\ft a\\f*\\f + \\ft b\\f* de"),
  "note-space-note": titus("siervo de", "siervo\\f + \\ft a\\f* \\f + \\ft b\\f* de"),
  "note-before-comma": titus("siervo de", "siervo\\f + \\ft a\\f*, de"),
  "note-first": titus("\\v 1 Pablo", "\\v 1 \\f + \\ft a\\f* Pablo"),
  "note-last": titus("piedad,", "piedad,\\f + \\ft a\\f*"),
  "note-line": "\\id TIT x\n\\c 1\n\\p\n\\v 1 Pablo, siervo\\f + \\fr 1:1 \\ft o esclavo\\f* de Dios.\n",
  // Other markers the shown text leaves out, between two words: tC4 (even with PR #800) joins
  // the words; the rebuild keeps the space (a declared difference).
  "hidden-va": titus("siervo de", "siervo\\va 2\\va* de"),
  "hidden-vp": titus("siervo de", "siervo\\vp 1b\\vp* de"),
  "hidden-ca": titus("siervo de", "siervo\\ca 3\\ca* de"),
  "hidden-rq": titus("siervo de", "siervo\\rq Rom 1:1\\rq* de"),
  "hidden-cat": titus("siervo de", "siervo\\cat x\\cat* de"),
  "hidden-ts": titus("siervo de", "siervo\\ts\\* de"),
  // Hand-written: a note then a line break; a note before a poetry line; right-to-left words;
  // a word outside the Basic Multilingual Plane before a note; a no-break space after a note;
  // a note inside an aligned verse, between two runs and at a run's end; nesting three deep;
  // a nested alignment inside a character span other than \qs.
  "note-newline": "\\id TIT x\n\\c 1\n\\p\n\\v 1 brother\\f + \\ft a\\f*\nwill be\n",
  "note-poetry": "\\id PSA x\n\\c 1\n\\q1\n\\v 1 one\\f + \\ft a\\f*\n\\q2 two\n",
  "note-rtl": "\\id TIT x\n\\c 1\n\\p\n\\v 1 كلمة\\f + \\ft ملاحظة\\f* أخرى\n",
  "note-astral": "\\id TIT x\n\\c 1\n\\p\n\\v 1 a𐌰\\f + \\ft n\\f* b\n",
  "note-nbsp": "\\id TIT x\n\\c 1\n\\p\n\\v 1 a\\f + \\ft n\\f*\u00a0b\n",
  "aligned-notes":
    "\\id TIT x\n\\c 1\n\\p\n\\v 1 \\zaln-s |x-strong=\"G1\" x-lemma=\"a\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"α\"\\*\\w one|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\f + \\ft n\\f*\\zaln-e\\*\n" +
    "\\zaln-s |x-strong=\"G2\" x-lemma=\"b\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"β\"\\*\\w two|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*\\f + \\ft m\\f*\n" +
    "\\zaln-s |x-strong=\"G3\" x-lemma=\"c\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"γ\"\\*\\w three|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*.\n",
  "nested-three":
    "\\id GEN x\n\\c 1\n\\p\n\\v 1 \\zaln-s |x-content=\"α\"\\*\\zaln-s |x-content=\"β\"\\*\\zaln-s |x-content=\"γ\"\\*\\w deep|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*\\zaln-e\\*\\zaln-e\\*\n\\w words|x-occurrence=\"1\" x-occurrences=\"1\"\\w*.\n",
  "nested-in-add":
    "\\id GEN x\n\\c 1\n\\p\n\\v 1 \\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w* \\add \\zaln-s |x-content=\"α\"\\*\\w b|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*\\add* \\w c|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n",
};
for (const [name, raw] of Object.entries(drafts)) add(name, raw, false);

const bundle = [
  ["bundle/en/en_ult_v91.sb.zip", "en_ult/ingredients/GEN.usfm"],
  ["bundle/en/en_ult_v91.sb.zip", "en_ult/ingredients/PSA.usfm"],
  ["bundle/en/en_ust_v91.sb.zip", "en_ust/ingredients/GEN.usfm"],
  ["bundle/en/en_ust_v91.sb.zip", "en_ust/ingredients/PSA.usfm"],
];
for (const [archive, entry] of bundle) {
  const found = unzipSync(fs.readFileSync(archive), { filter: (f) => f.name === entry });
  add(`${archive} ${entry}`, strFromU8(found[entry]), true);
}

process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-shown-text-vectors.mjs",\n "tc4": ${JSON.stringify(commit)},\n` +
    (mainTree ? ` "main": ${JSON.stringify(mainCommit)},\n` : "") +
    ` "libraries": ${JSON.stringify(pinned)},\n "drafts": ${JSON.stringify(drafts)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n",
);
