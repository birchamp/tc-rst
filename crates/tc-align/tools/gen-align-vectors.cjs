// Generates the tC4 test vectors for tc-align in fixtures/align/: what tC4's own alignment
// code returns for the real fixtures. It bundles tC4's sources (src/data/align/edit.ts,
// suggest.ts, suggestEngine.ts and the suggestion benches' corpus loader) with the esbuild
// in tC4's node_modules and runs them on:
//
// - bootstrap-vectors.json: `bootstrapVerse` for every verse of en_ult Titus against UGNT
//   Titus, every verse of UHB Jonah 1 against itself (a right-to-left target), and the
//   sample burrito's Spanish span JON 2:9-10 against UHB 2:9 and 2:10.
// - extract-vectors.json: word-aligner's `unmerge` of every aligned verse of en_ult and
//   en_ust Titus, with and without the UGNT verse, and of the partly aligned verses that
//   tC4's export wrote in fixtures/usfm/weave-vectors.json.
// - reflow-vectors.json: `reflowAlignment` of the en_ult records after seeded edits of
//   their text.
// - suggest-vectors.json: the suggestion benches' training verses for en_ult and en_ust
//   Titus; `predictLinks` from memory-only models and from boosted models trained with
//   `Math.random` seeded as tC4's tests seed it; and the growth bench's protocol on Titus.
// - heading-vectors.json: `bootstrapVerse` against heading-source.usfm, original-language
//   verses with headings, titles and notes inside them.
//
// Run from the repository root with a tC4 checkout at the commit fixtures/tc4 came from:
//   node crates/tc-align/tools/gen-align-vectors.cjs /path/to/translationcore4 fixtures/align
const fs = require("fs");
const os = require("os");
const path = require("path");

const [tc4, outDir, ...only] = process.argv.slice(2);
if (!tc4 || !outDir) throw new Error("usage: gen-align-vectors.cjs <tc4 checkout> <out dir> [bootstrap|extract|reflow|headings|suggest ...]");
const wanted = (section) => only.length === 0 || only.includes(section);
const nm = path.join(tc4, "node_modules");
const pinned = {
  "usfm-js": "3.4.3",
  "word-aligner": "1.0.3",
  "word-aligner-lib": "1.0.1",
  "string-punctuation-tokenizer": "2.0.0",
  wordmap: "0.6.2",
  "wordmap-lexer": "0.3.6",
  "uw-wordmapbooster": "1.0.5",
};
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const tc4Version = JSON.parse(fs.readFileSync(path.join(tc4, "package.json"), "utf8")).version;

// Bundle tC4's own modules, so the vectors come from the code tC4 runs.
const work = fs.mkdtempSync(path.join(os.tmpdir(), "tc-align-gen-"));
const entry = path.join(work, "entry.ts");
fs.writeFileSync(
  entry,
  [
    `export * from ${JSON.stringify(path.join(tc4, "src/data/align/edit"))};`,
    `export * from ${JSON.stringify(path.join(tc4, "src/data/align/suggest"))};`,
    `export * from ${JSON.stringify(path.join(tc4, "src/data/align/suggestEngine"))};`,
    `export { versesOfBook } from ${JSON.stringify(path.join(tc4, "test/helpers/ult-corpus"))};`,
    `export { md5Hex } from ${JSON.stringify(path.join(tc4, "src/data/httpStore"))};`,
    `export { usfmjs, wordaligner } from ${JSON.stringify(path.join(tc4, "src/data/vendor"))};`,
  ].join("\n"),
);
const bundle = path.join(work, "tc4-align.cjs");
require(path.join(nm, "esbuild")).buildSync({
  entryPoints: [entry],
  bundle: true,
  platform: "node",
  format: "cjs",
  outfile: bundle,
  nodePaths: [nm],
  logLevel: "error",
});
const tc = require(bundle);
const { usfmjs, wordaligner } = tc;

// mulberry32, the generator tC4's suggestion tests install as Math.random.
const mulberry32 = (seed) => {
  let a = seed;
  return () => {
    let t = (a += 0x6d2b79f5);
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};
const realRandom = Math.random;
const seeded = async (seed, f) => {
  Math.random = mulberry32(seed);
  try {
    return await f();
  } finally {
    Math.random = realRandom;
  }
};

const read = (rel) => fs.readFileSync(rel, "utf8");
const ULT = "fixtures/tc4/test-fixtures/en_ult/TIT.usfm";
const UST = "fixtures/tc4/test-fixtures/en_ust/TIT.usfm";
const UGNT = "fixtures/align/el-x-koine_ugnt-TIT.usfm";
const UHB = "fixtures/tc4/test-fixtures/hbo_uhb/JON.usfm";
const SAMPLE_JON = "fixtures/tc4/sample-burrito/ingredients/JON.usfm";
const SOURCE_NT = "dcs::unfoldingWord/el-x-koine_ugnt@v0.34";
const SOURCE_OT = "dcs::unfoldingWord/hbo_uhb@v3.0.0";

// tC4's journal slot (journal/skeleton.mjs): from after `\v N ` to the next `\v ` or `\c `.
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
    out.push({ ref: `${chapter}:${marks[i].key}`, content: next < 0 ? rest : rest.slice(0, next) });
  }
  return out;
};

// tC4's verseObjectsFor (state.jsx): a span is its member verses' objects in order.
const bookJson = new Map();
const verseObjectsFor = (file, chapter, verse) => {
  if (!bookJson.has(file)) bookJson.set(file, usfmjs.toJSON(read(file)));
  const verses = bookJson.get(file).chapters[String(chapter)] ?? {};
  const m = /^(\d+)-(\d+)$/.exec(String(verse));
  const members = m ? Array.from({ length: Number(m[2]) - Number(m[1]) + 1 }, (_, i) => String(Number(m[1]) + i)) : [String(verse)];
  return members.flatMap((v) => verses[v]?.verseObjects ?? []);
};

// A verse's plain text as align-tokenize.test.ts reads it: the text of its verse objects,
// descending into milestones, whitespace collapsed.
const flatten = (objects) =>
  objects.map((vo) => (vo.children ? flatten(vo.children) : vo.type === "text" || vo.text ? vo.text ?? "" : "")).join("");
const plainOf = (file, chapter, verse) => flatten(verseObjectsFor(file, chapter, verse)).replace(/\s+/g, " ").trim();

const intWords = (list) => list.map((w) => ({ ...w, occurrence: Number(w.occurrence), occurrences: Number(w.occurrences) }));
const record = (alignments, wordBank) => ({
  alignments: alignments.map((a) => ({ topWords: intWords(a.topWords), bottomWords: intWords(a.bottomWords) })),
  wordBank: intWords(wordBank),
});
const line = (cases) => `{"generator": "crates/tc-align/tools/gen-align-vectors.cjs", "tc4": ${JSON.stringify(tc4Version)}, "libraries": ${JSON.stringify(pinned)},\n "cases": [\n${cases.map((c) => "  " + JSON.stringify(c)).join(",\n")}\n]}\n`;
const write = (name, cases) => {
  fs.writeFileSync(path.join(outDir, name), line(cases));
  console.error(`${name}: ${cases.length} cases`);
};

const unmerge = (content, original) => {
  const objects = usfmjs.toJSON(`\\v 1 ${content}`, { chunk: true }).verses["1"].verseObjects;
  const out = wordaligner.unmerge(objects, original);
  return record(out.alignment, out.wordBank);
};

(async () => {
  // ---- bootstrap ----
  const bootstrap = [];
  if (wanted("bootstrap")) for (const { ref } of slots(read(ULT))) {
    const [c, v] = ref.split(":");
    const text = plainOf(ULT, c, v);
    bootstrap.push({ book: "TIT", ref, source: UGNT, text, record: tc.bootstrapVerse(text, verseObjectsFor(UGNT, c, v), SOURCE_NT) });
  }
  // UHB writes a line break after each verse number, so its verses are listed from the
  // parsed book rather than by tC4's slot pattern.
  if (wanted("bootstrap")) {
    verseObjectsFor(UHB, 1, 1);
    for (const v of Object.keys(bookJson.get(UHB).chapters["1"]).filter((k) => /^\d+$/.test(k))) {
      const [c, ref] = ["1", `1:${v}`];
      const text = plainOf(UHB, c, v);
      bootstrap.push({ book: "JON", ref, source: UHB, text, record: tc.bootstrapVerse(text, verseObjectsFor(UHB, c, v), SOURCE_OT) });
    }
    const text = plainOf(SAMPLE_JON, 2, "9-10");
    bootstrap.push({ book: "JON", ref: "2:9-10", source: UHB, text, record: tc.bootstrapVerse(text, verseObjectsFor(UHB, 2, "9-10"), SOURCE_OT) });
    write("bootstrap-vectors.json", bootstrap);
  }

  // ---- extract ----
  // The verses themselves are in the fixture books and in weave-vectors.json.
  const extract = [];
  if (wanted("extract")) {
    for (const file of [ULT, UST]) {
      for (const { ref, content } of slots(read(file))) {
        const [c, v] = ref.split(":");
        // Without the original, fixtures/usfm/weave-vectors.json holds what tC4 reads.
        extract.push({ file, ref, original: UGNT, withOriginal: unmerge(content, verseObjectsFor(UGNT, c, v)) });
      }
    }
    const weave = JSON.parse(read("fixtures/usfm/weave-vectors.json")).cases.filter((c) => c.source.startsWith("template"));
    for (const c of weave) extract.push({ file: "fixtures/usfm/weave-vectors.json", ref: c.source, alone: unmerge(c.woven, null) });
    write("extract-vectors.json", extract);
  }

  // ---- reflow ----
  const reflow = [];
  const rng = mulberry32(0x5eed);
  const pick = (n) => Math.floor(rng() * n);
  // Every third verse keeps the file small; each gets every kind of edit.
  if (wanted("reflow")) for (const { ref, content } of slots(read(ULT)).filter((_, n) => n % 3 === 0)) {
    const [c, v] = ref.split(":");
    const text = plainOf(ULT, c, v);
    const base = {
      ...unmerge(content, verseObjectsFor(UGNT, c, v)),
      invalid: false,
      targetVerseMd5: tc.md5Hex(text),
      sourceVersion: SOURCE_NT,
    };
    const stored = tc.settleDone(base);
    const words = text.split(" ");
    const edits = [];
    const k = pick(words.length);
    edits.push(["delete a word", [...words.slice(0, k), ...words.slice(k + 1)].join(" ")]);
    const j = pick(words.length);
    edits.push(["repeat a word", [...words.slice(0, j + 1), words[j], ...words.slice(j + 1)].join(" ")]);
    const i = pick(words.length);
    edits.push(["replace a word", [...words.slice(0, i), "otherwise", ...words.slice(i + 1)].join(" ")]);
    edits.push(["punctuation only", text.replace(/[,.;:]/g, "") + "!"]);
    edits.push(["unchanged", text]);
    edits.push(["no words", "___"]);
    if (ref === "1:1") edits.push(["marked invalid", text + " again", { ...stored, invalid: true }]);
    reflow.push({
      ref,
      text,
      record: stored,
      edits: edits.map(([edit, newText, from = stored]) => ({
        edit,
        ...(from === stored ? {} : { invalid: true }),
        text: newText,
        result: tc.reflowAlignment(from, newText),
      })),
    });
  }
  if (wanted("reflow")) write("reflow-vectors.json", reflow);

  // ---- headings ----
  // A heading, title or note inside a verse of the original: which of its words tC4's
  // bootstrap gives a card.
  if (wanted("headings")) {
    const HEADINGS = "fixtures/align/heading-source.usfm";
    const headings = [];
    verseObjectsFor(HEADINGS, 1, 1);
    for (const v of Object.keys(bookJson.get(HEADINGS).chapters["1"]).filter((k) => /^\d+$/.test(k))) {
      const text = "one two";
      headings.push({ ref: `1:${v}`, source: HEADINGS, text, record: tc.bootstrapVerse(text, verseObjectsFor(HEADINGS, 1, v), SOURCE_OT) });
    }
    write("heading-vectors.json", headings);
  }

  // ---- suggestions ----
  if (!wanted("suggest")) return;
  const suggest = [];
  const ult = tc.versesOfBook("TIT", read(ULT));
  const ust = tc.versesOfBook("TIT", read(UST));
  suggest.push({ kind: "training", file: ULT, verses: ult });
  suggest.push({ kind: "training", file: UST, verses: ust });
  const ask = (v, manual = []) => ({ source: v.source, target: v.target, manual });

  const grown = (verses) => verses.reduce((m, v) => tc.appendVerse(m, v), tc.emptyModel("nt"));
  for (const k of [1, 5, 12, 23]) {
    const model = grown(ult.slice(0, k));
    const probes = ult.slice(30).map((v) => ({ ref: v.ref, links: tc.predictLinks(model, ask(v)) }));
    suggest.push({ kind: "memory", file: ULT, train: [0, k], verses: model.verses, probes });
  }
  for (const [from, to, probeFrom, probeTo] of [[0, 12, 20, 30], [0, 40, 40, 46], [0, 5, 5, 10]]) {
    const trained = await seeded(0, () => tc.trainModel("nt", ult.slice(from, to)));
    const probes = [];
    for (const v of ult.slice(probeFrom, probeTo)) {
      probes.push({ ref: v.ref, manual: [], links: tc.predictLinks(trained, ask(v)) });
      const manual = v.links.slice(0, Math.floor(v.links.length / 2));
      probes.push({ ref: v.ref, manual, links: tc.predictLinks(trained, ask(v, manual)) });
    }
    suggest.push({ kind: "boosted", file: ULT, seed: 0, train: [from, to], verses: trained.verses, boosted: trained.boosted ?? 0, tooFew: !!trained.tooFew, probes });
  }

  // The growth bench's protocol (test/align-suggest-growth-bench.test.ts) on Titus: a
  // seeded held-out set, the rest fed in order, exact-position recall at each checkpoint.
  for (const [file, all] of [[ULT, ult], [UST, ust]]) {
    const draw = mulberry32(1);
    const drawn = [...all.keys()].map((i) => ({ i, r: draw() })).sort((a, b) => a.r - b.r);
    const heldRefs = new Set(drawn.slice(0, 10).map(({ i }) => all[i].ref));
    const heldOut = all.filter((v) => heldRefs.has(v.ref));
    const training = all.filter((v) => !heldRefs.has(v.ref));
    const key = (l) => `${[...l.source].sort((a, b) => a - b)}|${[...l.target].sort((a, b) => a - b)}`;
    const score = (model) => {
      let correct = 0;
      let gold = 0;
      for (const v of heldOut) {
        const got = new Set(tc.predictLinks(model, ask(v)).map(key));
        correct += v.links.filter((g) => got.has(key(g))).length;
        gold += v.links.length;
      }
      return { correct, gold };
    };
    const rows = [];
    const FREEZE_AT = 10;
    let plain = tc.emptyModel("nt");
    let frozen = null;
    let fed = 0;
    const feed = (upTo) => {
      for (; fed < upTo; fed++) {
        plain = tc.appendVerse(plain, training[fed]);
        if (frozen) frozen = tc.appendVerse(frozen, training[fed]);
      }
    };
    for (const checkpoint of [1, 5, 10, 25, training.length]) {
      feed(checkpoint);
      if (!frozen && checkpoint >= FREEZE_AT) frozen = await seeded(0, () => tc.trainModel("nt", training.slice(0, checkpoint)));
      const retrained = await seeded(0, () => tc.trainModel("nt", training.slice(0, checkpoint)));
      rows.push({ verses: checkpoint, plain: score(plain), frozen: frozen ? score(frozen) : null, retrained: score(retrained), boosted: retrained.boosted ?? 0 });
    }
    suggest.push({ kind: "growth", file, heldOut: heldOut.map((v) => v.ref), freezeAt: FREEZE_AT, seed: 0, rows });
  }
  write("suggest-vectors.json", suggest);
})()
  .catch((e) => {
    console.error(e);
    process.exitCode = 1;
  })
  .finally(() => fs.rmSync(work, { recursive: true, force: true }));
