// Generates fixtures/usfm/tokenizer-vectors.json: what tC4's word tokenizer
// (string-punctuation-tokenizer 2.0.0 on xregexp 4.4.1) returns for real verse text and
// for hand-picked edge cases. The Rust tests compare `tc_usfm::words::classify` with it.
//
// Run from the repository root, with the pinned packages installed somewhere:
//   npm install --ignore-scripts --legacy-peer-deps string-punctuation-tokenizer@2.0.0 usfm-js@3.4.3
//   node crates/tc-usfm/tools/gen-tokenizer-vectors.cjs <path to node_modules> > fixtures/usfm/tokenizer-vectors.json
const fs = require("fs");
const path = require("path");
const nm = process.argv[2];
if (!nm) throw new Error("usage: gen-tokenizer-vectors.cjs <node_modules>");
const req = (m) => require(path.join(nm, m));
const pinned = { "string-punctuation-tokenizer": "2.0.0", xregexp: "4.4.1", "usfm-js": "3.4.3" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const spt = req("string-punctuation-tokenizer/lib/tokenizers.js");
const usfmjs = req("usfm-js");

// The same call tC4 makes in src/data/align/tokenize.ts, but through classifyTokens so
// the characters tC4 drops ("unknown" tokens) are visible too.
const classify = (text) =>
  spt
    .classifyTokens(text, { word: spt.word, whitespace: spt.whitespace, punctuation: spt.punctuation, number: spt.number })
    .map((t) => [t.type, t.token]);

// Every verse's text, gathered from usfm-js verse objects (footnote content excluded).
const textOf = (objects) =>
  (objects || [])
    .map((o) => (o.type === "footnote" ? "" : (o.text || "") + textOf(o.children)))
    .join("");
const books = [
  "fixtures/tc4/test-fixtures/en_ult/TIT.usfm",
  "fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm",
  "fixtures/tc4/test-fixtures/sample-burrito/TIT.usfm",
  "fixtures/tc4/test-fixtures/sample-burrito/JON.usfm",
  "fixtures/tc4/test-fixtures/usfm-corpus/exotic-poetry-footnotes.usfm",
];
const cases = [];
for (const book of books) {
  const json = usfmjs.toJSON(fs.readFileSync(book, "utf8"));
  for (const [c, verses] of Object.entries(json.chapters)) {
    for (const [v, verse] of Object.entries(verses)) {
      const text = textOf(verse.verseObjects);
      if (text.trim()) cases.push({ source: `${book} ${c}:${v}`, text });
    }
  }
}
const edges = [
  "$5 dollars, 10% off; a—b; x << y >> z <> w",
  "A <<quote>> and «guillemets» and “curly” and ‘single’ and don’t and don't",
  "Numbers: 3:16, 1,000.50 and ٣٤ and ⅓ and ²",
  "Mixed abc123def 45xyz and 6th",
  "Joiners: a‍b a⁠b a‌b a­b",
  "Astral: 𝐀𝐁 and 😀 and 𠀀x and a𝟙b",
  "Leading symbols: +, then ©. then ¶ then § then † end",
  "Spaces: nbsp thin　ideographic​zwsp﻿bom\tTab\r\nCRLF",
  "Hebrew: וַֽ⁠יְהִי֙ דְּבַר־יְהוָ֔ה אֶל־יוֹנָ֥ה׃ פ ס",
  "Arabic: بِسْمِ اللَّهِ، الرَّحْمَٰنِ؟ ٪ ؛",
  "Devanagari: नमस्ते। दुनिया॥ १२३",
  "Thai: ภาษาไทย ๑๒๓ ฯ",
  "CJK: 我爱你。你好！「引用」、",
  "Combining at start: ́abc ̈ x",
  "Brackets {are} [here] (and) there…",
  "Dash run: — – - ‐ ‑ ‒ ― ⸺",
  "",
  "   ",
  "...",
  "<<<",
  "a<<",
  "$,x",
  "$, x",
];
for (const text of edges) cases.push({ source: "edge", text });

// Random strings over characters at the edges of the tokenizer's classes, from a seeded
// generator (mulberry32) so the file is the same on every run.
const pool = [
  ..."adlo xyzABC019",
  ...".,;:!?'\"-()[]{}/_*&#@%",
  ..."$+=<>|~^`©¶§†€₹",
  " ", "\t", "\n", "\r", " ", " ", " ", " ", " ", "　", "﻿", "᠎",
  "​", "‌", "‍", "⁠", "­", "́", "̈", "ְ", "ּ",
  ..."אבגדה", "־", "׃", "׀", ..."αβΓωῦ", ..."بسم", "،", "٣", "٪",
  ..."नम", "।", "१", ..."我你", "。", "「", "²", "⅓", "Ⅻ",
  "‘", "’", "“", "”", "«", "»", "‐", "–", "—", "…", "⸺",
  "😀", "𝐀", "𠀀", "𝟙",
];
let seed = 0x2c1b3c6d;
const random = () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
for (let i = 0; i < 500; i++) {
  let text = "";
  for (let n = 1 + Math.floor(random() * 24); n > 0; n--) text += pool[Math.floor(random() * pool.length)];
  cases.push({ source: "random", text });
}

// One case per line keeps the file reviewable in a diff.
process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/gen-tokenizer-vectors.cjs",\n "libraries": ${JSON.stringify(pinned)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify({ ...c, tokens: classify(c.text) })).join(",\n") +
    "\n]}\n",
);
