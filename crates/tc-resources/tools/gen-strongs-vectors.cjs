// Generates fixtures/lexicon/strongs-vectors.json: which lexicon entry tC4's rule gives
// for a word's `strong` attribute. The rule is `lexiconHelpers` in word-aligner-lib 1.0.1,
// the version tC4 pins (tC4 has no lexicon reader of its own; see
// crates/tc-resources/src/strongs.rs). This script runs that code, from tC4's
// node_modules, on:
//
// - `cases`: edge cases (prefixes, suffix letters, Greek variant digits, and the quirks of
//   JavaScript's `parseInt`: leading white space, signs, `0x`, digits it stops at);
// - `corpora`: every `strong` attribute of the bundled UHB 3.0.0 and UGNT 0.34 exports in
//   bundle/en/, looked up in the bundled en_uhl and en_ugl archives. Each corpus keeps
//   its counts, the parts whose entry the lexicon lacks, and the SHA-256 of one line per
//   distinct attribute, so a test can check all of them without a large file.
//
// Run from the repository root with a tC4 checkout:
//   node crates/tc-resources/tools/gen-strongs-vectors.cjs /path/to/translationcore4 fixtures/lexicon
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execFileSync } = require("child_process");

const [tc4, outDir] = process.argv.slice(2);
if (!tc4 || !outDir) throw new Error("usage: gen-strongs-vectors.cjs <tc4 checkout> <out dir>");
const nm = path.join(tc4, "node_modules");
const LIB = "word-aligner-lib";
const LIB_VERSION = "1.0.1";
const pinnedByTc4 = JSON.parse(fs.readFileSync(path.join(tc4, "package.json"), "utf8")).dependencies[LIB];
const installed = JSON.parse(fs.readFileSync(path.join(nm, LIB, "package.json"), "utf8")).version;
if (pinnedByTc4 !== LIB_VERSION || installed !== LIB_VERSION) {
  throw new Error(`expected ${LIB}@${LIB_VERSION}; tC4 pins ${pinnedByTc4}, node_modules has ${installed}`);
}
const { lexiconHelpers: h } = require(path.join(nm, LIB, "dist/index.cjs.js"));
const { unzipSync } = require(path.join(nm, "fflate"));
const tc4Commit = execFileSync("git", ["-C", tc4, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();

const bundle = path.resolve("bundle/en");
const UGL = "en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip";
const UHL = "en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip";
const unzip = (name) => unzipSync(fs.readFileSync(path.join(bundle, name)));

// tC4's entry id as the vectors store it: 0, an id an entry can have, or "out of range"
// (negative, or above 2^32 - 1), which no entry has.
const encode = (id) => (id === 0 ? 0 : Number.isInteger(id) && id >= 1 && id <= 4294967295 ? id : "out of range");
const parts = (strong) =>
  h.getStrongsParts(strong).map((part) => [part, h.lexiconIdFromStrongs(part), encode(h.lexiconEntryIdFromStrongs(part))]);

const CASES = [
  "", "G35880", "G3588", "G00010", "G1", "G10", "G", "H", "H1", "H01", "H0", "H00", "H1234",
  "H1254a", "H1254b", "H1254c", "c:H1254a", "b:c:H776", "c:d:H0776", "H123:H7225", "H9003",
  "G99237", "G40560", "g35880", "h1234", "Γ1230", "G 1230", " G1230", "G1230 ", "\tH12",
  "\nH12", "H 12", "H﻿12", "H​12", "H\u008512", "H　  12", "H᠎12",
  "+H12", "-H12", "H+12", "H-12", "H-0", "H+0", "H--1", "H0x1F", "G0x1F0", "H0X1g", "H0x",
  "Hx12", "H12.5", "H1e3", "H١٢", "Ĝ12", "_12", "9999999999", "H99999999999",
  "H4294967295", "H4294967296", "H9007199254740993", "H123456789012345678901234567890",
  "G0x1000000000", "::", ":H1", ":", "H1:", "a:b:c", "c:", "G35880:G35880", "H1:G10", "G10:H1",
  "b:", "d:H2:m:H3", "G\u{1F600}0", "H\u{1F600}", "\u{1F600}G10", "H12\u{1F600}", "中文",
  "1", "12", "_", "x", "é12", "Hé12", "GH10", "HG10", "G0", "G00", "G000", "H 1",
  "c:H0853", "k:H3605", "i:H0", "s:H1", "H5414a", "G26320", "G25320", "G55470",
];

const corpus = (archive) => {
  const counts = new Map();
  for (const [name, bytes] of Object.entries(unzip(archive))) {
    if (!name.endsWith(".usfm")) continue;
    for (const m of Buffer.from(bytes).toString("utf8").matchAll(/strong="([^"]*)"/g)) {
      counts.set(m[1], (counts.get(m[1]) || 0) + 1);
    }
  }
  return counts;
};
const numbers = (archive) => {
  const found = new Set();
  for (const name of Object.keys(unzip(archive))) {
    const m = name.match(/^[^/]+\/ingredients\/content\/(\d+)\.json$/);
    if (m) found.add(Number(m[1]));
  }
  return found;
};
const lexicons = { ugl: numbers(UGL), uhl: numbers(UHL) };
const getLexiconData = (id, entry) => (lexicons[id].has(entry) ? { [id]: { [entry]: { long: "" } } } : null);

const corpora = ["hbo_uhb_v3.0.0.sb.zip", "el-x-koine_ugnt_v0.34.sb.zip"].map((archive) => {
  const counts = corpus(archive);
  const distinct = [...counts.keys()].sort((a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)));
  let occurrences = 0;
  let partsWithNumber = 0;
  let partsFound = 0;
  const notFound = {};
  const lines = [];
  for (const strong of distinct) {
    const n = counts.get(strong);
    occurrences += n;
    const found = h.lookupStrongsNumbers(strong, getLexiconData);
    for (const [part, id, entry] of parts(strong)) {
      if (entry === 0) continue;
      partsWithNumber += n;
      if (found[id] && found[id][entry]) partsFound += n;
      else notFound[part] = (notFound[part] || 0) + n;
    }
    lines.push(`${strong}\t${parts(strong).map(([, id, entry]) => `${id}:${entry}`).join(" ")}\n`);
  }
  return {
    archive,
    distinct: distinct.length,
    occurrences,
    partsWithNumber,
    partsFound,
    notFound,
    sha256: crypto.createHash("sha256").update(lines.join("")).digest("hex"),
  };
});

const out = {
  about:
    "What tC4's Strong's rule (word-aligner-lib 1.0.1 lexiconHelpers) gives. Made by crates/tc-resources/tools/gen-strongs-vectors.cjs; see fixtures/lexicon/README.md.",
  tc4: { commit: tc4Commit, [LIB]: LIB_VERSION },
  lexicons: { ugl: UGL, uhl: UHL },
  cases: CASES.map((strong) => ({ strong, parts: parts(strong), valid: h.containsValidStrongsNumber(strong) })),
  corpora,
};
// One case per line, so a review reads each case whole.
const text = JSON.stringify({ ...out, cases: "CASES" }, null, 1).replace(
  '"CASES"',
  `[\n${out.cases.map((c) => `  ${JSON.stringify(c)}`).join(",\n")}\n ]`,
);
fs.writeFileSync(path.join(outDir, "strongs-vectors.json"), `${text}\n`);
console.log(JSON.stringify(corpora, null, 1));
