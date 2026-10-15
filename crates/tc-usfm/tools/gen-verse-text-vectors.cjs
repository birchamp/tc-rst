// Generates fixtures/usfm/verse-text-vectors.json: the text tC4 hashes for each verse
// (`verseTextMd5` in journal/fold.mjs: usfm-js 3.4.3's chunk parse of the verse slot, the
// top-level `text` of its verse objects joined and trimmed). tC4 stores the md5 of that
// text with every alignment record (`targetVerseMd5`, invariant I-3), so the Rust tests
// compare it with `tc_usfm::plain_text` of the same verse.
//
// Run from the repository root, with tC4's pinned packages installed somewhere (tC4's own
// node_modules works):
//   node crates/tc-usfm/tools/gen-verse-text-vectors.cjs <path to node_modules> > fixtures/usfm/verse-text-vectors.json
const fs = require("fs");
const path = require("path");
const nm = process.argv[2];
if (!nm) throw new Error("usage: gen-verse-text-vectors.cjs <node_modules>");
const pinned = { "usfm-js": "3.4.3" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const usfmjs = require(path.join(nm, "usfm-js"));

// journal/fold.mjs `verseTextMd5`, without the md5.
const verseText = (content) => {
  const parsed = usfmjs.toJSON(`\\v 1 ${content}`, { chunk: true });
  const parts = [];
  for (const k of Object.keys(parsed.verses || {})) for (const o of parsed.verses[k].verseObjects || []) parts.push(o.text || "");
  return parts.join("").trim();
};

// journal/skeleton.mjs `decompose`: a slot runs from after `\v N ` to the next `\v ` or `\c `.
const slots = (usfm) => {
  const re = /\\v[ \t]+(\d+(?:-\d+)?)[ \t]/g;
  const out = [];
  let chapter = "0";
  let m;
  const marks = [];
  while ((m = re.exec(usfm))) marks.push({ key: m[1], at: m.index, start: m.index + m[0].length });
  for (let i = 0; i < marks.length; i++) {
    const before = usfm.slice(i === 0 ? 0 : marks[i - 1].start, marks[i].at);
    const chapters = [...before.matchAll(/\\c[ \t]+(\d+)/g)];
    if (chapters.length) chapter = chapters[chapters.length - 1][1];
    const rest = usfm.slice(marks[i].start);
    const next = rest.search(/\\[vc][ \t]/);
    out.push({ chapter, verse: marks[i].key, slot: next < 0 ? rest : rest.slice(0, next) });
  }
  return out;
};

const books = [
  "fixtures/tc4/sample-burrito/ingredients/TIT.usfm",
  "fixtures/tc4/sample-burrito/ingredients/JON.usfm",
  "fixtures/tc4/conformance-fixtures/import/usfm/57-TIT.usfm",
  "fixtures/tc4/conformance-fixtures/import/usfm/no-id.sfm",
  "fixtures/tc4/conformance-fixtures/import/damaged/57-TIT-unclosed.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-partial-book.usfm",
  "fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm",
];
// en_ult and en_ust are left out: tC4 never hashes aligned text (I-1), and usfm-js drops
// the `\w` words inside `\zaln` milestones from the top-level text.
const cases = [];
for (const book of books) {
  for (const { chapter, verse, slot } of slots(fs.readFileSync(book, "utf8"))) {
    cases.push({ book, chapter, verse, text: verseText(slot) });
  }
}
process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-verse-text-vectors.cjs",\n "libraries": ${JSON.stringify(pinned)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n",
);
