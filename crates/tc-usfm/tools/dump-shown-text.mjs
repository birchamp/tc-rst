// Writes, for the long parity run of crates/tc-usfm/tests/shown_text.rs
// (`the_shown_text_is_what_tc4_reads_on_a_dump`), the text tC4 shows for every verse of every
// book of the ULT v91 and UST v91 in bundle/en/: `verseText` of src/data/bookModel.js, run as
// gen-shown-text-vectors.mjs runs it, in the same file format (about 7 MB, not committed).
//
// Run from the repository root with a tC4 tree as gen-shown-text-vectors.mjs takes it:
//   node crates/tc-usfm/tools/dump-shown-text.mjs <tC4 tree> <tC4 commit> > <dump.json>
//   TC_USFM_SHOWN_TEXT_DUMP=<dump.json> cargo test --release -p tc-usfm --test shown_text -- --ignored
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const [tc4, commit] = process.argv.slice(2);
if (!tc4 || !commit) throw new Error("usage: dump-shown-text.mjs <tC4 tree> <tC4 commit>");
const nm = path.join(tc4, "node_modules");
const pinned = { "usfm-js": "3.4.3" };
for (const [name, version] of Object.entries({ ...pinned, fflate: "0.8.2" })) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const require = createRequire(import.meta.url);
const usfm = require(path.join(nm, "usfm-js"));
const { unzipSync, strFromU8 } = require(path.join(nm, "fflate"));

const { tokenizeVerse, tokensText } = await import(pathToFileURL(path.join(tc4, "src/data/sourceHighlight.ts")).href);
const src = fs.readFileSync(path.join(tc4, "src/data/bookModel.js"), "utf8");
const at = src.indexOf("export const verseText = ");
const stop = src.indexOf(".trim();", at);
if (at < 0 || stop < 0) throw new Error(`${tc4}: no verseText in src/data/bookModel.js`);
const decl = src.slice(at, stop + ".trim();".length).replace(/^export /, "");
const verseText = new Function("tokenizeVerse", "tokensText", `${decl} return verseText;`)(tokenizeVerse, tokensText);

const quiet = (f) => {
  const { error, warn, log } = console;
  console.error = console.warn = console.log = () => {};
  try {
    return f();
  } finally {
    Object.assign(console, { error, warn, log });
  }
};

const cases = [];
for (const archive of ["bundle/en/en_ult_v91.sb.zip", "bundle/en/en_ust_v91.sb.zip"]) {
  const entries = unzipSync(fs.readFileSync(archive), { filter: (f) => f.name.endsWith(".usfm") });
  for (const entry of Object.keys(entries).sort()) {
    const chapters = quiet(() => usfm.toJSON(strFromU8(entries[entry]))).chapters || {};
    for (const [chapter, verses] of Object.entries(chapters)) {
      for (const [verse, object] of Object.entries(verses)) {
        if (verse === "front") continue;
        cases.push({ book: `${archive} ${entry}`, chapter, verse, text: verseText(object) });
      }
    }
  }
}
process.stdout.write(
  `{"generator": "crates/tc-usfm/tools/dump-shown-text.mjs",\n "tc4": ${JSON.stringify(commit)},\n` +
    ` "libraries": ${JSON.stringify(pinned)},\n "drafts": {},\n "cases": [\n` +
    cases.map((c) => "  " + JSON.stringify(c)).join(",\n") +
    "\n]}\n",
);
