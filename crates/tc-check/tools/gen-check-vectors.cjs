// Generates fixtures/check/check-vectors.json: what the JavaScript libraries behind tC4's
// Check screens answer, for the Rust tests of tc-check to reproduce.
//
// - scope: bible-reference-range 1.1.1 `doesReferenceContain(range, "c:v")`, as tC4's
//   `refInScope` asks it, for every scope range form against many reference forms.
// - selections: tC4's `selectionsFromTokens` and `tokenIndicesFromSelections`
//   (src/data/selections.ts, on string-punctuation-tokenizer 2.0.0) for real verse text,
//   and word-aligner-lib 1.0.1 `validateVerseSelections` for the same selections against
//   the verse and against edited verses.
// - quotes: tC4's `tnQuoteWords` for every en_tn v86 Titus quote, and `sameOrigQuote`
//   (string-punctuation-tokenizer) for the en_tn v86 and es-419_tn v66 quotes of the
//   check ids both carry, and for hand-written pairs.
// - terms: the order `String.prototype.localeCompare` gives the tW and tA slugs of the
//   fixtures and some hand-written strings (the Check rail's "By word" groups).
//
// Run from the repository root, with the pinned packages installed somewhere (tC4's own
// node_modules works):
//   node crates/tc-check/tools/gen-check-vectors.cjs <path to node_modules> > fixtures/check/check-vectors.json
const fs = require("fs");
const path = require("path");
const nm = process.argv[2];
if (!nm) throw new Error("usage: gen-check-vectors.cjs <node_modules>");
const req = (m) => require(path.join(nm, m));
const pinned = {
  "string-punctuation-tokenizer": "2.0.0",
  "word-aligner-lib": "1.0.1",
  "bible-reference-range": "1.1.1",
  "usfm-js": "3.4.3",
};
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const { tokenize, occurrencesInString } = req("string-punctuation-tokenizer");
const { selectionsHelpers } = req("word-aligner-lib");
const { doesReferenceContain } = req("bible-reference-range");

// ---- scope -----------------------------------------------------------------------
const ranges = ["1", "2", "3", "1-2", "2-3", "1:1", "1:5", "2:9", "2:10", "1:1-5", "2:10-12", "2:1-10", "1:5-2:3", "1:16-3:2", "2:5-2:9", "3:15", "10"];
const chapters = ["1", "2", "3", "10", "front", "0"];
const verses = ["1", "3", "5", "6", "9", "10", "12", "16", "17", "0", "9-10", "4-6", "11-12", "30-2", "intro", "1a", "3,5", "5,1", "1,10-11", "2ff", "2FF", "1–3", "x", "15-15"];
const scope = [];
for (const range of ranges) {
  for (const c of chapters) {
    for (const v of verses) {
      scope.push({ range, chapter: c, verse: v, inScope: doesReferenceContain(range, `${c}:${v}`) });
    }
  }
}

// ---- selections --------------------------------------------------------------------
// tC4 src/data/selections.ts, verbatim apart from the types.
const targetWords = (verseText) => tokenize({ text: verseText });
const selectionsFromTokens = (verseText, selectedIndices) => {
  const chosen = new Set(selectedIndices);
  const seen = new Map();
  const out = [];
  targetWords(verseText).forEach((word, i) => {
    const nth = (seen.get(word) ?? 0) + 1;
    seen.set(word, nth);
    if (chosen.has(i)) out.push({ text: word, occurrence: nth, occurrences: occurrencesInString(verseText, word) });
  });
  return out;
};
const tokenIndicesFromSelections = (verseText, selections) => {
  if (!selections || selections.length === 0) return [];
  const want = new Set(selections.map((s) => `${s.text} ${s.occurrence}`));
  const seen = new Map();
  const indices = [];
  targetWords(verseText).forEach((word, i) => {
    const nth = (seen.get(word) ?? 0) + 1;
    seen.set(word, nth);
    if (want.has(`${word} ${nth}`)) indices.push(i);
  });
  return indices;
};
const validate = (text, selections) => {
  const r = selectionsHelpers.validateVerseSelections(text, selections);
  return { changed: r.selectionsChanged, valid: r.validSelections };
};

// Real verse text: the drafts and Bibles of the tC4 fixtures, as fixtures/usfm holds them.
const verseVectors = JSON.parse(fs.readFileSync("fixtures/usfm/verse-text-vectors.json", "utf8"));
const tokenizerVectors = JSON.parse(fs.readFileSync("fixtures/usfm/tokenizer-vectors.json", "utf8"));
const texts = [
  ...new Set([
    ...verseVectors.cases.map((c) => c.text),
    ...tokenizerVectors.cases.filter((c) => /\.usfm \d+:/.test(c.source)).map((c) => c.text),
  ]),
].filter((t) => t.trim() !== "" && t !== "___");
// A small deterministic generator, so the vectors are the same on every run.
let seed = 20261015;
const rand = (n) => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed % n;
};
const selections = [];
for (const text of texts.slice(0, 160)) {
  const words = targetWords(text);
  if (words.length === 0) continue;
  const count = 1 + rand(3);
  const indices = Array.from({ length: count }, () => rand(words.length));
  const picked = selectionsFromTokens(text, indices);
  const first = picked[0].text;
  const edits = {
    same: text,
    removed: text.replace(first, "Zzz"),
    doubled: `${text} ${first}`,
    longer: `${text} ${first}s`,
    reworded: text.split(" ").reverse().join(" "),
  };
  selections.push({
    text,
    indices,
    selections: picked,
    restored: tokenIndicesFromSelections(text, picked),
    validations: Object.entries(edits).map(([edit, edited]) => ({ edit, text: edited, ...validate(edited, picked) })),
  });
}
const handwritten = [
  ["the theory", [{ text: "the", occurrence: 1, occurrences: 1 }]],
  ["the word", [{ text: "the", occurrence: 1, occurrences: 1 }]],
  ["  leading space and word", [{ text: "word", occurrence: 1, occurrences: 1 }]],
  ["line one\nline two", [{ text: "line", occurrence: 2, occurrences: 2 }]],
  ["", [{ text: "x", occurrence: 1, occurrences: 1 }]],
  ["aaa", [{ text: "aa", occurrence: 1, occurrences: 1 }]],
  ["God's God", [{ text: "God", occurrence: 2, occurrences: 2 }]],
  ["Dios y Dios", []],
];
const handValidations = handwritten.map(([text, sel]) => ({ text, selections: sel, ...validate(text, sel) }));

// ---- quotes ------------------------------------------------------------------------
const tnQuoteWords = (quote) => {
  const seen = {};
  return quote
    .split(/\s+/)
    .filter((w) => w !== "" && w !== "&")
    .map((word) => {
      seen[word] = (seen[word] ?? 0) + 1;
      return { word, occurrence: seen[word] };
    });
};
const sameOrigQuote = (a, b) => {
  const norm = (q) =>
    q
      .split("&")
      .map((seg) => tokenize({ text: seg.trim() }).join(" "))
      .join(" & ");
  return norm(a) === norm(b);
};
const tnRows = (file) =>
  fs
    .readFileSync(file, "utf8")
    .split("\n")
    .filter((l) => l.trim() !== "")
    .slice(1)
    .map((l) => l.split("\t").map((c) => c.replace(/\r$/, "")));
const enTn = tnRows("fixtures/tc4/test-fixtures/resources/en_tn@v86/TIT.tsv");
const esTn = tnRows("fixtures/tc4/test-fixtures/resources/es-419_tn@v66/TIT.tsv");
const quoteWords = [...new Set(enTn.map((r) => r[4]))].map((quote) => ({ quote, words: tnQuoteWords(quote) }));
const esById = new Map(esTn.map((r) => [r[1], r[4]]));
const pairs = enTn.filter((r) => esById.has(r[1])).map((r) => [r[4], esById.get(r[1])]);
pairs.push(
  ["Θεοῦ,", "Θεοῦ"],
  ["τοῦ Θεοῦ & τοῦ Σωτῆρος", "τοῦ Θεοῦ  &  τοῦ Σωτῆρος"],
  ["τοῦ Θεοῦ & τοῦ Σωτῆρος", "τοῦ Θεοῦ τοῦ Σωτῆρος"],
  ["κατὰ πίστιν", "κατὰ πίστιν ἐκλεκτῶν"],
  ["", ""],
);
const sameQuote = pairs.map(([a, b]) => ({ a, b, same: sameOrigQuote(a, b) }));

// ---- terms -------------------------------------------------------------------------
const slug = (link) => (link.split("/").pop() ?? "").replace(/\.md\r?$/, "");
const twlRows = (file) => tnRows(file).map((r) => slug(r[5]));
const terms = [
  ...twlRows("fixtures/tc4/test-fixtures/resources/en_twl@v86/TIT.tsv"),
  ...twlRows("fixtures/tc4/test-fixtures/resources/en_tw@v91/2SA.tsv"),
  ...enTn.map((r) => r[3].replace(/\/+$/, "").split("/").pop()).filter((s) => s),
  "Abel", "abel", "ABEL", "a", "A", "high-priest", "high_priest", "highpriest", "high priest",
  "10", "9", "a1", "a-1", "a.b", "a,b", "z", "Z", "god2", "god10",
];
const sorted = [...new Set(terms)].sort((a, z) => a.localeCompare(z));

process.stdout.write(
  JSON.stringify(
    {
      generator: "crates/tc-check/tools/gen-check-vectors.cjs",
      libraries: pinned,
      scope,
      selections,
      validations: handValidations,
      quoteWords,
      sameQuote,
      terms: sorted,
    },
    null,
    1,
  ) + "\n",
);
