// Generates fixtures/helps/gateway-title-vectors.json: the gateway title tC4 gives every
// quoted Translation Notes and Translation Words row of Genesis, Psalms and Titus.
//
// The title is tC4's `gatewayQuote(tokenizeVerse(verse), quote, occurrence)`
// (src/data/sourceHighlight.ts at 9ad4d5c, which holds the #765 maqaf split and the #773
// shortest run): the ULT words that render the quote, in verse order, with `…` across a
// gap, or null when nothing renders it. A tN quote goes in as tC4's `tnQuoteWords`, a
// TWL quote as its string, as tC4's Check screen passes them (`gatewayTitleFor`).
//
// A row is every TSV line after the header whose Reference is `<chapter>:<verse>` in
// digits, whose quote cell (tN `Quote`, TWL `OrigWords`) is not empty, and whose verse
// usfm-js finds in the ULT. Inputs: the en_ult, en_tn and en_twl v91 exports in
// bundle/en (Genesis and Psalms), and for Titus the ULT v89 and en_tn v91 that the parity
// baseline shows (fixtures/tc4/test-fixtures).
//
// sourceHighlight.ts imports nothing, so the script takes it from the commit with
// `git show` and runs it as written (Node 22.18 or later runs TypeScript). Run from the
// repository root:
//   node crates/tc-helps/tools/gen-gateway-title-vectors.mjs <tC4 repository> <node_modules> \
//     > fixtures/helps/gateway-title-vectors.json
// <tC4 repository> needs commit 9ad4d5c in its history (its checkout may be at any commit);
// <node_modules> must hold usfm-js 3.4.3 and fflate 0.8.2, the versions tC4 pins.
import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const COMMIT = "9ad4d5c506466819e5b10f255bbb4a36d394e498";
const FILE = "src/data/sourceHighlight.ts";
const [tc4, nm] = process.argv.slice(2);
if (!tc4 || !nm) throw new Error("usage: gen-gateway-title-vectors.mjs <tC4 repository> <node_modules>");
const pinned = { "usfm-js": "3.4.3", fflate: "0.8.2" };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfm = require(path.join(nm, "usfm-js"));
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));

const git = (...args) => execFileSync("git", ["-C", tc4, ...args], { encoding: "utf8" });
const blob = git("rev-parse", `${COMMIT}:${FILE}`).trim();
const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gateway-titles-"));
const copy = path.join(dir, "sourceHighlight.ts");
fs.writeFileSync(copy, git("show", `${COMMIT}:${FILE}`));
const { gatewayQuote, tokenizeVerse } = await import(pathToFileURL(copy).href);
fs.rmSync(dir, { recursive: true });

/** tC4's `tnQuoteWords` (src/data/derive.ts at 9ad4d5c), copied verbatim. */
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

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const inputs = {};
const zips = {};
/** A text file: `<zip>:<entry>` reads one entry of a burrito export, anything else a file. */
const read = (source) => {
  const [file, entry] = source.split(":");
  if (!entry) {
    const bytes = fs.readFileSync(file);
    inputs[file] = sha256(bytes);
    return bytes.toString("utf8");
  }
  if (!zips[file]) {
    const bytes = fs.readFileSync(file);
    inputs[file] = sha256(bytes);
    zips[file] = unzipSync(new Uint8Array(bytes));
  }
  if (!zips[file][entry]) throw new Error(`${file} has no ${entry}`);
  return strFromU8(zips[file][entry]);
};

const ULT = "bundle/en/en_ult_v91.sb.zip";
const TN = "bundle/en/en_tn_v91.sb.zip";
const TWL = "bundle/en/en_twl_v91.sb.zip";
const specs = [
  { book: "GEN", helps: "tn", ult: `${ULT}:en_ult/ingredients/GEN.usfm`, tsv: `${TN}:en_tn/ingredients/GEN.tsv` },
  { book: "GEN", helps: "twl", ult: `${ULT}:en_ult/ingredients/GEN.usfm`, tsv: `${TWL}:en_twl/ingredients/GEN.tsv` },
  { book: "PSA", helps: "tn", ult: `${ULT}:en_ult/ingredients/PSA.usfm`, tsv: `${TN}:en_tn/ingredients/PSA.tsv` },
  { book: "PSA", helps: "twl", ult: `${ULT}:en_ult/ingredients/PSA.usfm`, tsv: `${TWL}:en_twl/ingredients/PSA.tsv` },
  {
    book: "TIT",
    helps: "tn",
    ult: "fixtures/tc4/test-fixtures/en_ult/TIT.usfm",
    tsv: "fixtures/tc4/test-fixtures/resources/en_tn@v91/TIT.tsv",
  },
];

const tables = specs.map((spec) => {
  const chapters = usfm.toJSON(read(spec.ult)).chapters;
  const lines = read(spec.tsv).split("\n");
  const rows = [];
  for (const line of lines.slice(1)) {
    if (line.trim() === "") continue;
    const cells = line.split("\t");
    const [ref, id] = cells;
    const [quote, occurrence] = spec.helps === "tn" ? [cells[4], cells[5]] : [cells[3], cells[4]];
    const m = /^(\d+):(\d+)$/.exec(ref);
    if (!m || !quote) continue;
    const [chapter, verse] = [Number(m[1]), Number(m[2])];
    const found = chapters[String(chapter)]?.[String(verse)];
    if (!found) continue;
    const words = spec.helps === "tn" ? tnQuoteWords(quote) : quote;
    rows.push([id, `${chapter}:${verse}`, gatewayQuote(tokenizeVerse(found), words, Number(occurrence))]);
  }
  return { ...spec, rows };
});

const out = {
  generator: "crates/tc-helps/tools/gen-gateway-title-vectors.mjs",
  tc4: { commit: COMMIT, file: FILE, blob },
  libraries: pinned,
  node: process.version,
  inputs,
  tables,
};
// One row per line keeps the file reviewable and its diffs small.
const json = JSON.stringify(out, null, 1).replace(/\[\n\s+("[^"]*"),\n\s+("[^"]*"),\n\s+("(?:[^"\\]|\\.)*"|null)\n\s+\]/g, "[$1, $2, $3]");
process.stdout.write(`${json}\n`);
