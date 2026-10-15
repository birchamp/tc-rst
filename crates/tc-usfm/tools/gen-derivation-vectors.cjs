// Generates the golden outputs of the version-1 derivations (docs/STORAGE.md §7.7) from the
// JavaScript libraries tC4 pins:
//
//   fixtures/derivations/plain-text-v1.json  usfm-js 3.4.3: tC4's verseTextMd5 input
//                                            (journal/fold.mjs, without the md5) and
//                                            removeMarker, for every content
//   fixtures/derivations/selections-v1.json  word-aligner-lib 1.0.1 validateVerseSelections
//   fixtures/derivations/tokenize-v1.json    string-punctuation-tokenizer 2.0.0 tokenize
//
// The contents are every verse of tC4's USFM fixtures (en_ult and en_ust Titus, UHB Jonah,
// the drafts and the exotic corpus), hand-written edge cases, and seeded random verses
// built from USFM pieces.
//
// Run from the repository root, with tC4's pinned packages installed somewhere (tC4's own
// node_modules works):
//   node crates/tc-usfm/tools/gen-derivation-vectors.cjs <node_modules> fixtures/derivations
// With `--random N --seed S --out FILE [--pieces P]` it writes only N random plain-text cases
// of up to P pieces (24 by default) to FILE, for
// the long differential run (`TC_USFM_DERIVATION_VECTORS=FILE cargo test -p tc-usfm
// --test derivations -- --ignored`).
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const args = process.argv.slice(2);
const nm = args[0];
if (!nm) throw new Error("usage: gen-derivation-vectors.cjs <node_modules> <out dir> | --random N --seed S --out FILE");
const option = (name) => {
  const at = args.indexOf(name);
  return at >= 0 ? args[at + 1] : undefined;
};
const pinned = { "usfm-js": "3.4.3", "string-punctuation-tokenizer": "2.0.0", "word-aligner-lib": "1.0.1" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const usfmjs = require(path.join(nm, "usfm-js"));
const { tokenize } = require(path.join(nm, "string-punctuation-tokenizer"));
const { selectionsHelpers } = require(path.join(nm, "word-aligner-lib"));

// journal/fold.mjs `verseTextMd5`, without the md5 and without its refusal of contents with
// `\v` or `\c` markers (the journal never stores those, but plain_text_v1 is total).
const verseText = (content) => {
  const parsed = usfmjs.toJSON(`\\v 1 ${content}`, { chunk: true });
  const parts = [];
  for (const k of Object.keys(parsed.verses || {})) for (const o of parsed.verses[k].verseObjects || []) parts.push(o.text || "");
  return parts.join("").trim();
};
const attempt = (f) => {
  try {
    return f();
  } catch (e) {
    if (!(e instanceof TypeError)) throw e;
    return null;
  }
};

// A seeded generator (mulberry32), so the files are the same on every run.
const rng = (seed) => () => {
  seed |= 0;
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};

const PIECES = [
  // words and numbers
  "Paul", "servant", "God's", "Ἰησοῦ", "Χριστοῦ", "וַיְהִי", "דְּבַר", "神", "😀", "naïve", "é", "123", "1:2", "3,000", "word-joined",
  // punctuation and symbols
  ",", ".", ";", "?", "!", "“", "”", "'", "\"", "—", "(", ")", "*", "|", "/", "=", "$", "-",
  // white space and line breaks
  " ", " ", " ", "  ", "\t", " ", "　", "\n", "\n", "\r\n", "\r", " ",
  // paragraphs, poetry, headings
  "\\p", "\\p ", "\\q1 ", "\\q2", "\\m ", "\\b", "\\nb", "\\pi2 ", "\\li1 ", "\\li ", "\\s1 A heading", "\\s ", "\\d Title ",
  "\\ts\\*", "\\ts", "\\ts-s\\*", "\\ts-e\\*", "\\cl ", "\\mt1 ", "\\r (Gen 1)", "\\qa A", "\\sp Paul", "\\P ", "\\Q1 ",
  // character markers
  "\\add ", "\\add*", "\\nd ", "\\nd*", "\\+nd ", "\\+nd*", "\\wj ", "\\wj*", "\\qs ", "\\qs*", "\\k ", "\\k*", "\\bk ", "\\bk*",
  "\\sc ", "\\sc*", "\\it ", "\\it*", "\\+add ", "\\+add*", "\\tl ", "\\tl*", "\\sls ", "\\sls*",
  // words
  "\\w word\\w*", "\\w word|strong=\"G123\"\\w*", "\\w δοῦλος|x-occurrence=\"1\" x-occurrences=\"1\"\\w*", "\\+w nested\\+w*",
  "\\w word|x-content=\"x\" lemma='y'\\w*", "\\w a|type=\"text\"\\w*", "\\w\\w*", "\\w |x\\w*", "\\w Dios|\\w*", "\\w Jesús|x-morph=\"N\"",
  "\\w Pablo|x-occurrence=\"1\"\\w* ", "\\w a|text=\"b\"\\w*", "\\w a|nesting=\"0\"\\w*", "\\w a|tag=\"zaln\"\\w*",
  // milestones
  "\\zaln-s |x-strong=\"G3972\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*",
  "\\zaln-e\\*", "\\zaln-s |x-content=\"a\"", "\\zaln-e", "\\zaln-s\\*", "\\zaln-s | x-occurrence=\"1\"\n\\*",
  "\\k-s | x-tw=\"rc://*/tw/dict/bible/kt/god\"\\*", "\\k-e\\*", "\\k-s |x-tw=\"a\"", "\\k-e",
  "\\qt-s |who=\"Paul\"\\*", "\\qt-e\\*", "\\qt1-s\\*", "\\qt1-e |sid=\"x\"\\*", "\\qt-s\\*",
  // notes, figures, sidebars
  "\\f + \\fr 1:1 \\ft Some note.\\f*", "\\f + \\ft unclosed", "\\f*", "\\x - \\xo 1:1 \\xt Gen 1:1.\\x*", "\\fe + \\ft end\\fe*",
  "\\fig caption|src=\"a.jpg\"\\fig*", "\\esb", "\\esbe", "\\cat x\\cat*", "\\fqa alt\\fqa*", "\\ft ", "\\fk key ",
  // odd and broken markup
  "\\v1", "\\c2", "\\va 2\\va*", "\\vp 1a\\vp*", "\\ca 2\\ca*", "\\zz custom", "\\z1", "\\*", "\\+", "\\-", "\\", "-s", "-e",
  "\\p 5 text", "\\q1\\q2", "\\w*", "\\zaln-e\\*\\zaln-e\\*", "\\jmp link|link-href=\"x\"\\jmp*", "\\rb 漢字|gloss=\"kan:ji\"\\rb*",
  "\\v", "\\c", "\\v 2 ", "\\c 3\n", "\\ts\\* \\p", "\\nd*\\nd*", "\\add \\add \\add*", "\\+wj \\+wj*", "\\pb", "\\id x",
];

const maxPieces = Number(option("--pieces") || 24);
const randomContent = (next) => {
  const pieces = [];
  const n = 1 + Math.floor(next() * maxPieces);
  for (let i = 0; i < n; i++) pieces.push(PIECES[Math.floor(next() * PIECES.length)]);
  return pieces.join("");
};

if (option("--random")) {
  const count = Number(option("--random"));
  const next = rng(Number(option("--seed") || 1));
  const cases = [];
  for (let i = 0; i < count; i++) {
    const content = randomContent(next);
    cases.push({ content, text: attempt(() => verseText(content)), removed: attempt(() => usfmjs.removeMarker(content)) });
  }
  fs.writeFileSync(option("--out"), JSON.stringify({ libraries: pinned, cases }) + "\n");
  process.exit(0);
}

const outDir = args[1];
if (!outDir) throw new Error("usage: gen-derivation-vectors.cjs <node_modules> <out dir>");

// A verse's content as the journal stores it (`tc_usfm::codec`): from after `\v N` and one
// terminator (a space, a tab or a line break) to the next `\v` or `\c` marker.
const slots = (usfm) => {
  const re = /\\v[ \t]+(\d+(?:-\d+)?)[ \t\r\n]/g;
  const marks = [];
  let m;
  while ((m = re.exec(usfm))) marks.push({ at: m.index, start: m.index + m[0].length });
  return marks.map((mark) => {
    const rest = usfm.slice(mark.start);
    const next = rest.search(/\\[vc]([ \t\r\n]|$)/);
    return next < 0 ? rest : rest.slice(0, next);
  });
};
const walk = (dir) =>
  fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name);
    return e.isDirectory() ? walk(p) : /\.(usfm|sfm)$/.test(e.name) ? [p] : [];
  });

const contents = [];
const seen = new Set();
const add = (source, content) => {
  if (seen.has(content)) return;
  seen.add(content);
  contents.push({ source, content });
};
const books = walk("fixtures/tc4").sort();
const seenBooks = new Set();
for (const book of books) {
  const text = fs.readFileSync(book, "utf8");
  const hash = crypto.createHash("sha256").update(text).digest("hex");
  if (seenBooks.has(hash)) continue;
  seenBooks.add(hash);
  for (const slot of slots(text)) add(book, slot);
}
const HANDWRITTEN = [
  "", " ", "\n", "plain words only", "  leading and trailing  \n", "line one\n\\q2 line two\n\\q1\n",
  "text\r\n\\p\r\n", "a\rb", "\\w uno\\w* \\w dos\\w*,", "\\w uno\\w*\n\\w dos\\w*\n",
  "\\zaln-s |x-content=\"a\"\\*\\w one|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*, two",
  "\\zaln-s |x-content=\"a\"\\*\\zaln-s |x-content=\"b\"\\*\\w one\\w*\\zaln-e\\*\\zaln-e\\* rest",
  "\\zaln-s |x-content=\"a\"\\*\\zaln-s |x-content=\"b\"\\*\\zaln-e\\*\\zaln-e\\* rest",
  "text \\f + \\fr 1:1 \\ft note \\fqa alt\\fqa*\\f* more", "a \\x - \\xo 1:1 \\xt Gen 1:1\\x* b",
  "\\add implied\\add* words", "\\nd Lord\\nd* and \\+nd nested\\+nd*", "\\s1 A heading\n\\p next words",
  "words\n\\s1 Heading at the end", "\\li1 list item\n\\li2 another", "\\ts\\*\n\\p words", "\\v1 mangled verse",
  "\\c2 mangled chapter", "a\\", "a \\* b", "\\qt-s |who=\"Jesus\"\\*quoted\\qt-e\\* words",
  "\\k-s | x-tw=\"rc://*/tw/dict/bible/kt/god\"\\*\\w God\\w*\\k-e\\*", "a | b", "x-content=\"y\"",
  "\\w word|x-content=\"x\"\n more\\w*", "\\fig caption|src=\"a.jpg\" size=\"col\"\\fig* after",
  "\\esb \\p sidebar \\esbe after", "\\w a\\w*\\w b\\w*", "“\\w quoted\\w*”", "\\p 5 text", "\\q1\\q2 words",
  "\\constructor x", "\\toString 5 y", "\\w a|children=\"x\" type=\"t\"\\w*", "\\zaln-s |x-children=\"x\"\\*\\w a\\w*\\zaln-e\\*",
];
for (const content of HANDWRITTEN) add("handwritten", content);
const next = rng(20261015);
for (let i = 0; i < 1500; i++) add("random", randomContent(next));

const plainCases = contents.map(({ source, content }) => ({
  source,
  content,
  text: attempt(() => verseText(content)),
  removed: attempt(() => usfmjs.removeMarker(content)),
}));

// Selections: words of the shown text with their counts, some changed so they no longer fit.
const occurrencesOf = (text, word) => {
  if (!word.length) return 0;
  let n = 0;
  for (let at = text.indexOf(word); at !== -1; at = text.indexOf(word, at + word.length)) n++;
  return n;
};
const pick = rng(7);
const selectionCases = [];
for (const c of plainCases) {
  if (selectionCases.length >= 900) break;
  for (const text of [c.content, c.text]) {
    if (text === null) continue;
    const shown = attempt(() => usfmjs.removeMarker(text));
    const base = shown === null ? text : shown;
    const words = tokenize({ text: base });
    const selections = [];
    const take = Math.min(words.length, Math.floor(pick() * 4));
    for (let i = 0; i < take; i++) {
      const word = words[Math.floor(pick() * words.length)];
      let occurrences = occurrencesOf(base, word);
      if (pick() < 0.3) occurrences += pick() < 0.5 ? 1 : -1;
      selections.push({ text: word, occurrence: 1, occurrences });
    }
    if (pick() < 0.1) selections.push({ text: "", occurrence: 1, occurrences: 0 });
    if (pick() < 0.1) selections.push({ text: "nowhere", occurrence: 1, occurrences: 0 });
    let result;
    try {
      result = { changed: selectionsHelpers.validateVerseSelections(text, selections).selectionsChanged };
    } catch (e) {
      if (!(e instanceof TypeError)) throw e;
      result = { throws: true };
    }
    selectionCases.push({ text, selections, ...result });
  }
}

const tokenizeCases = [];
for (const c of plainCases) {
  if (c.text !== null && tokenizeCases.length < 1200) tokenizeCases.push({ text: c.text, words: tokenize({ text: c.text }) });
}

const write = (name, cases) => {
  const body =
    `{"generator": "crates/tc-usfm/tools/gen-derivation-vectors.cjs",\n "libraries": ${JSON.stringify(pinned)},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n";
  fs.writeFileSync(path.join(outDir, name), body);
};
fs.mkdirSync(outDir, { recursive: true });
write("plain-text-v1.json", plainCases);
write("selections-v1.json", selectionCases);
write("tokenize-v1.json", tokenizeCases);
