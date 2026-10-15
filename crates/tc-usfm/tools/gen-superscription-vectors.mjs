// Generates fixtures/usfm/superscription-vectors.json: what tC4 reads from a verse that holds
// a Psalm superscription (`\v 1 \d …`) or another heading usfm-js shows as verse text, and
// what its export writes for it.
//
// tC4 reads a verse through usfm-js 3.4.3's verse objects of the whole book. This script
// runs tC4's own code on them, imported from the checkout:
//
// - `text`: the verse's text, `verseText` of src/views/verseText.js:9, which is
//   `tokensText(tokenizeVerse(…))` of src/data/sourceHighlight.ts:34-66 with its whitespace
//   collapsed. It is what the source panes show and what the aligner splits into words
//   (`verseTextIndex` in src/state.jsx:329, `buildAlignmentSession` in src/state.jsx:1116).
// - `words`: those words with their occurrences, `wordTokens` of
//   src/data/align/tokenize.ts:59 (string-punctuation-tokenizer 2.0.0), the words the
//   aligner banks (`bootstrapVerse` in src/data/align/edit.ts:46).
// - `aligned`, for a published aligned verse: each word of `tokenizeVerse` with the
//   original-language words (`x-content`, `x-occurrence`) of the `\zaln` milestones above
//   it, in verse order, which is what tC4 highlights a note's quote with.
// - `alignments` and `woven`, for a hand-written draft: every word aligned to its own
//   made-up source word, and what tC4's export (word-aligner-lib 1.0.1's
//   `addAlignmentsToTargetVerseUsingMerge`, as src/data/export/weave.mjs:41 calls it)
//   writes for the draft and that record.
// - `headings`: an aligned word in each heading that may stand inside a verse, and
//   whether tC4's export writes the verse back (`same`) or cannot merge its alignment and
//   writes it without (`unaligned`).
//
// The verses are every verse of the ULT v91 and UST v91 Psalms whose text starts with `\d`
// (read from the burrito exports in bundle/en/), and hand-written drafts. For each
// published verse the script also checks that tC4's export writes the verse back as
// published from the record it reads out of it, but for one more space after a closing
// paragraph marker, and refuses to run otherwise.
//
// Needs Node 22.18 or later, which runs tC4's TypeScript as it is. Run from the
// repository root with the tC4 checkout whose node_modules hold the pinned libraries:
//   node crates/tc-usfm/tools/gen-superscription-vectors.mjs <tC4 checkout> > fixtures/usfm/superscription-vectors.json
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const tc4 = process.argv[2];
if (!tc4) throw new Error("usage: gen-superscription-vectors.mjs <tC4 checkout>");
const nm = path.join(tc4, "node_modules");
const pinned = { "usfm-js": "3.4.3", "string-punctuation-tokenizer": "2.0.0", "word-aligner-lib": "1.0.1" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfmjs = require(path.join(nm, "usfm-js"));
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));
const { AlignmentHelpers, UsfmFileConversionHelpers } = require(path.join(nm, "word-aligner-lib/dist/index.cjs.js"));
const tc4Module = (file) => import(pathToFileURL(path.join(tc4, file)).href);
const { tokenizeVerse, tokensText } = await tc4Module("src/data/sourceHighlight.ts");
const { wordTokens } = await tc4Module("src/data/align/tokenize.ts");

// src/views/verseText.js:9.
const verseText = (vObj) => tokensText(tokenizeVerse(vObj)).replace(/\s+/g, " ").trim();
const words = (text) => wordTokens(text).map((t) => [t.text, t.occurrence, t.occurrences]);
const quiet = (f) => {
  const { error, warn, log } = console;
  console.error = console.warn = console.log = () => {};
  try {
    return f();
  } finally {
    Object.assign(console, { error, warn, log });
  }
};

// tC4's journal decomposition (journal/skeleton.mjs): a verse runs from after `\v N ` to the
// next `\v ` or `\c `.
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
    out.push({ chapter, verse: marks[i].key, content: next < 0 ? rest : rest.slice(0, next) });
  }
  return out;
};

// The key order of a stored §5.1 record (as gen-weave-vectors.cjs writes it): word-aligner
// writes a milestone's attributes in the key order of its top word.
const topWord = (w) => {
  const o = {};
  for (const k of ["strong", "lemma", "morph"]) if (w[k] !== undefined) o[k] = w[k];
  o.occurrence = Number(w.occurrence);
  o.occurrences = Number(w.occurrences);
  o.word = w.word;
  return o;
};
const bottomWord = (w) => ({ word: w.word, occurrence: Number(w.occurrence), occurrences: Number(w.occurrences) });
const merge = (plain, alignments, wordBank) =>
  quiet(() => AlignmentHelpers.addAlignmentsToTargetVerseUsingMerge(plain, { alignments, wordBank }));

const cases = [];

for (const [archive, entry] of [
  ["bundle/en/en_ult_v91.sb.zip", "en_ult/ingredients/PSA.usfm"],
  ["bundle/en/en_ust_v91.sb.zip", "en_ust/ingredients/PSA.usfm"],
]) {
  const files = unzipSync(fs.readFileSync(archive), { filter: (f) => f.name === entry });
  const usfm = strFromU8(files[entry]);
  const book = usfmjs.toJSON(usfm);
  for (const { chapter, verse, content } of slots(usfm)) {
    if (!content.startsWith("\\d ")) continue;
    const vObj = book.chapters[chapter][verse];
    const text = verseText(vObj);
    const aligned = tokenizeVerse(vObj)
      .filter((t) => t.word)
      .map((t) => [t.text, t.orig.map((o) => [o.content, o.occurrence])]);
    // tC4's export writes the verse back from the record it reads out of it.
    const record = quiet(() => AlignmentHelpers.extractAlignmentsFromTargetVerse(content, null));
    const alignments = record.alignment.map((a) => ({ topWords: a.topWords.map(topWord), bottomWords: a.bottomWords.map(bottomWord) }));
    const woven = merge(UsfmFileConversionHelpers.cleanAlignmentMarkersFromString(content), alignments, record.wordBank.map(bottomWord));
    if (woven !== content && !(woven === `${content} ` && /\\q\d? $/.test(content))) {
      throw new Error(`${archive} ${chapter}:${verse}: tC4's export does not write the verse back`);
    }
    cases.push({ source: `${archive} ${entry}`, chapter, verse, text, words: words(text), aligned });
  }
}

// Hand-written drafts, each one verse at the end of a book so that the verse's text in tC4
// and its body in tc-usfm are the same bytes.
const drafts = [
  // A superscription whose words come again in the verse: `of` and `David` are counted
  // across the whole verse.
  "\\v 1 \\d A psalm of David, when he fled from Absalom his son.\n\\q1 Yahweh, how many are the enemies of David!\n\\q2 Many rise up against me.\n",
  // Case matters: `For` and `for`.
  "\\v 1 \\d For the music director. A psalm of David.\n\\q1 The heavens declare the glory of God;\n\\q2 the sky shows the work of his hands, for all to see.\n",
  // A footnote in the superscription.
  "\\v 1 \\d A psalm \\f + \\ft Or: song\\f* of David.\n\\q1 Hear me.\n",
  // A superscription in the middle of a verse.
  "\\v 7 He will be on the throne.\n\\d A song of ascents.\n\\q1 I lift up my eyes to the hills.\n",
  // The other headings usfm-js shows: a speaker, an acrostic letter, a liturgical note.
  "\\v 4 Take me with you; let us hurry.\n\\sp The young women\n\\q1 We will rejoice and be glad in you.\n",
  "\\v 8 I will keep your statutes.\n\\qa Beth\n\\q1 How can a young man keep his way pure?\n",
  "\\v 3 Give thanks to the Lord.\n\\lit Response\n\\p His love endures forever.\n",
  // A heading usfm-js does not show: its words are not the verse's.
  "\\v 5 They went out to the town.\n\\s The town by the river\n\\p And they came to the river.\n",
];
for (const draft of drafts) {
  const usfm = `\\id PSA\n\\c 1\n${draft}`;
  const [{ chapter, verse, content }] = slots(usfm);
  const text = verseText(usfmjs.toJSON(usfm).chapters[chapter][verse]);
  const listed = words(text);
  const alignments = listed.map(([word, occurrence, occurrences], i) => ({
    topWords: [{ strong: `H${1000 + i}`, lemma: `l${i}`, morph: "He,Ncmsa", occurrence: 1, occurrences: 1, word: `w${i}` }],
    bottomWords: [{ word, occurrence, occurrences }],
  }));
  const woven = merge(content, alignments, []);
  if (typeof woven !== "string") throw new Error(`tC4's export cannot weave the draft ${JSON.stringify(draft)}`);
  cases.push({ source: "hand-written", usfm, chapter, verse, text, words: listed, alignments, woven });
}

// An aligned word in each heading that may stand inside a verse, and whether tC4's export
// writes the verse back from the record it reads out of it (`same`) or cannot merge that
// record and writes the verse without its alignment (`unaligned`).
const milestone = (content, word) =>
  `\\zaln-s |x-strong="H0001" x-occurrence="1" x-occurrences="1" x-content="${content}"\\*\\w ${word}|x-occurrence="1" x-occurrences="1"\\w*\\zaln-e\\*`;
const headings = [];
for (const marker of ["d", "sp", "qa", "lit", "s", "s1", "s5", "d1", "ms", "ms1", "mr", "sr", "r", "rem", "sd", "cl", "mt1"]) {
  const content = `${milestone("א", "One")}.\n\\${marker} ${milestone("ב", "Two")}\n\\q1 ${milestone("ג", "Three")}.\n`;
  const record = quiet(() => AlignmentHelpers.extractAlignmentsFromTargetVerse(content, null));
  const alignments = record.alignment.map((a) => ({ topWords: a.topWords.map(topWord), bottomWords: a.bottomWords.map(bottomWord) }));
  const woven = merge(UsfmFileConversionHelpers.cleanAlignmentMarkersFromString(content), alignments, record.wordBank.map(bottomWord));
  const exported = woven === content ? "same" : woven === null ? "unaligned" : null;
  if (!exported) throw new Error(`tC4's export rewrites the aligned \\${marker}: ${JSON.stringify(woven)}`);
  headings.push({ marker, content, exported });
}

process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-superscription-vectors.mjs",\n "libraries": ${JSON.stringify(pinned)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n],\n \"headings\": [\n" +
    headings.map((h) => "  " + JSON.stringify(h)).join(",\n") +
    "\n]}\n",
);
