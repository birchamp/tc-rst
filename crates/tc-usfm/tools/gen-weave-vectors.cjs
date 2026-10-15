// Generates fixtures/usfm/weave-vectors.json: what tC4's aligned-USFM export writes for a
// verse and its alignment record. tC4 weaves each verse with word-aligner-lib's
// `AlignmentHelpers.addAlignmentsToTargetVerseUsingMerge` (src/data/export/weave.mjs);
// this script calls the same function on:
//
// - every verse of the en_ult and en_ust fixtures, with the record tC4 reads back from the
//   verse (`extractAlignmentsFromTargetVerse`, occurrences made integers as tC4 stores them);
// - the stored record of the sample burrito (TIT 1:1);
// - hand-written plain verses (poetry, notes, character markup, quotes, numbers, Hebrew,
//   CRLF), each with alignments drawn by a seeded random generator.
//
// The Rust tests weave the same plain verse with the same alignments and compare bytes.
//
// Run from the repository root, with tC4's pinned packages installed somewhere (tC4's own
// node_modules works):
//   node crates/tc-usfm/tools/gen-weave-vectors.cjs <path to node_modules> > fixtures/usfm/weave-vectors.json
const fs = require("fs");
const path = require("path");
const nm = process.argv[2];
if (!nm) throw new Error("usage: gen-weave-vectors.cjs <node_modules>");
const pinned = { "word-aligner-lib": "1.0.1", "word-aligner": "1.0.3", "usfm-js": "3.4.3", "string-punctuation-tokenizer": "2.0.0" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const { AlignmentHelpers, UsfmFileConversionHelpers } = require(path.join(nm, "word-aligner-lib/dist/index.cjs.js"));
const { VerseObjectUtils } = require(path.join(nm, "word-aligner/lib/index.js"));

// tC4's journal decomposition (journal/skeleton.mjs): a verse runs from after `\v N ` to the
// next `\v ` or `\c `.
const verses = (usfm) => {
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
    out.push({ ref: `${chapter}:${marks[i].key}`, content: next < 0 ? rest : rest.slice(0, next) });
  }
  return out;
};

// The key order of a stored §5.1 record. word-aligner writes a milestone's attributes in
// the key order of its top word, so the order matters to the woven bytes.
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
  AlignmentHelpers.addAlignmentsToTargetVerseUsingMerge(plain, { alignments, wordBank });

const cases = [];
const quiet = (f) => {
  const error = console.error;
  console.error = () => {};
  try {
    return f();
  } finally {
    console.error = error;
  }
};

for (const book of ["fixtures/tc4/test-fixtures/en_ult/TIT.usfm", "fixtures/tc4/test-fixtures/en_ust/TIT.usfm"]) {
  for (const { ref, content } of verses(fs.readFileSync(book, "utf8"))) {
    const plain = UsfmFileConversionHelpers.cleanAlignmentMarkersFromString(content);
    const record = AlignmentHelpers.extractAlignmentsFromTargetVerse(content, null);
    const alignments = record.alignment.map((a) => ({ topWords: a.topWords.map(topWord), bottomWords: a.bottomWords.map(bottomWord) }));
    const wordBank = record.wordBank.map(bottomWord);
    const woven = quiet(() => merge(plain, alignments, wordBank));
    // `woven` is left out when tC4 writes the verse back exactly as it was.
    const c = { source: `${book} ${ref}`, aligned: content, plain, alignments, wordBank };
    cases.push(woven === content ? c : { ...c, woven });
  }
}

{
  const book = "fixtures/tc4/sample-burrito/ingredients/TIT.usfm";
  const records = JSON.parse(fs.readFileSync("fixtures/tc4/sample-burrito/ingredients/checking/alignments/TIT.json", "utf8"));
  const byRef = Object.fromEntries(verses(fs.readFileSync(book, "utf8")).map((v) => [v.ref, v.content]));
  for (const [chapter, recs] of Object.entries(records.chapters)) {
    for (const [verse, record] of Object.entries(recs)) {
      const plain = byRef[`${chapter}:${verse}`];
      const alignments = record.alignments.map((a) => ({ topWords: a.topWords.map(topWord), bottomWords: a.bottomWords.map(bottomWord) }));
      const wordBank = record.wordBank.map(bottomWord);
      cases.push({ source: `${book} ${chapter}:${verse}`, plain, alignments, wordBank, woven: quiet(() => merge(plain, alignments, wordBank)) });
    }
  }
}

// mulberry32: a small seeded generator, so the file is the same on every run.
let seed = 0x7c4e2a91;
const random = () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (n) => Math.floor(random() * n);

const templates = [
  "Paul, a servant of God and an apostle of Jesus Christ, for the faith of God's chosen people.\n",
  "“Look,” he said, “the Lord—the Lord God—is here!” And they said, ‘Yes.’\n",
  "In the beginning God created the heavens and the earth.\n\\p\n",
  "Blessed is the man\n\\q2 who does not walk in the advice of the wicked,\n\\q1 or stand in the pathway with sinners,\n",
  "He said this \\f + \\fr 1:1 \\ft Some versions read “that”.\\f* to them, and they went away.\n",
  "The \\nd Lord\\nd* is my shepherd; I will lack nothing.\n",
  "They caught 153 fish, fed 2,000 people, and read 3:16 again.\n",
  "וַֽ⁠יְהִי֙ דְּבַר־יְהוָ֔ה אֶל־יוֹנָ֥ה בֶן־אֲמִתַּ֖י לֵאמֹֽר׃\n",
  "a a a b a, b; a. b b a\n",
  "Selah. Then the end of the song came, a song of the end.\n\n",
  "Word  with  double  spaces and\ttab, then more words.\n",
  "First line of the verse,\r\nsecond line of the verse.\r\n",
  "x-ray and don't and self-control, God's and gods' things.\n",
  "Then he went \\add up\\add* to the city, \\add and\\add* he stayed there.\n",
  "Grace to you, \\x - \\xo 1:2 \\xt Rom 1:7\\x* and peace from God our Father.\n",
  "(This is in brackets) and [this] and {this} too.\n",
  "Pay $5 or € 3 + tax, then go … away… now.\n",
];
for (const [t, plain] of templates.entries()) {
  const { wordMap } = VerseObjectUtils.getOrderedVerseObjectsFromString(plain);
  const words = wordMap.map(({ array, pos }) => array[pos]).map((w) => ({ word: w.text, occurrence: w.occurrence, occurrences: w.occurrences }));
  for (let variant = 0; variant < 8; variant++) {
    const alignments = [];
    const wordBank = [];
    let source = 0;
    for (let i = 0; i < words.length; ) {
      if (random() < 0.25) {
        wordBank.push(words[i]);
        i++;
        continue;
      }
      const len = 1 + pick(3);
      const bottomWords = words.slice(i, i + len);
      i += len;
      const topWords = [];
      for (let s = 0, n = 1 + pick(3); s < n; s++, source++) {
        const top = { strong: `G${String(source).padStart(4, "0")}0`, lemma: `λέξις${source}` };
        if (source % 2 === 0) top.morph = "Gr,N,,,,,NMS,";
        topWords.push(topWord({ ...top, word: `λ${source}`, occurrence: 1, occurrences: 1 }));
      }
      alignments.push({ topWords, bottomWords });
    }
    // Some original words have no rendering.
    if (random() < 0.5) alignments.splice(pick(alignments.length + 1), 0, { topWords: [topWord({ strong: "G02430", word: "ἄλλος", occurrence: 1, occurrences: 1 })], bottomWords: [] });
    // Some alignments take words that are not next to each other.
    if (alignments.length > 2 && random() < 0.5) {
      const [a, b] = [pick(alignments.length), pick(alignments.length)];
      if (a !== b && alignments[a].bottomWords.length && alignments[b].bottomWords.length) {
        alignments[a].bottomWords.push(...alignments[b].bottomWords);
        alignments.splice(b, 1);
      }
    }
    cases.push({ source: `template ${t} variant ${variant}`, plain, alignments, wordBank, woven: quiet(() => merge(plain, alignments, wordBank)) });
  }
}

// One case per line keeps the file reviewable in a diff.
process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-weave-vectors.cjs",\n "libraries": ${JSON.stringify(pinned)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n",
);
