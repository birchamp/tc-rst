// Generates what tC4's own derivation makes of the v91 helps the app ships in bundle/en:
//
// - fixtures/check/psalms-vectors.json: `deriveForProject` (src/data/derive.ts) on the
//   Psalms rows of en_tn v91 and en_twl v91, the tW rows gated by the en_tw v91 articles as
//   tC4's Check screen gates them, for a whole-book `eng` project, for partial scopes, and
//   for an `org` project (a cross-frame conversion). Each case lists the derived checks in
//   order with their identity, and the rows the conversion could not place.
// - the same file's `books`: for every book of en_tn and en_twl v91, the checks of a
//   whole-book `eng` project: their number and the md5 of their identities in order, one
//   `checkId|chapter|verse|occurrence|groupId` line each, joined by "\n", leaving out the
//   checks on every occurrence (Occurrence -1), which `every` lists as
//   [checkId, chapter, verse] in order. tc-check reports those rows instead of deriving
//   them, because a check key's occurrence is a u32 (STORAGE.md §2.2).
// - fixtures/helps/v91-tsv-rows.json: for every TSV of en_tn, en_twl and en_tq v91, the
//   number of rows tC4's TSV reader returns and the lines it skips.
//
// It bundles tC4's sources with the esbuild in tC4's node_modules, as
// crates/tc-align/tools/gen-align-vectors.cjs does, so the vectors come from the code tC4
// runs. Run from the repository root with a tC4 checkout at the commit fixtures/tc4 came
// from:
//   node crates/tc-check/tools/gen-psalms-vectors.cjs /path/to/translationcore4
const crypto = require("crypto");
const fs = require("fs");
const os = require("os");
const path = require("path");

const [tc4] = process.argv.slice(2);
if (!tc4) throw new Error("usage: gen-psalms-vectors.cjs <tc4 checkout>");
const nm = path.join(tc4, "node_modules");
const pinned = {
  "bible-reference-range": "1.1.1",
  "proskomma-core": "0.11.3",
  "usfm-js": "3.4.3",
  fflate: "0.8.2",
  esbuild: "0.25.12",
};
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, "package.json"), "utf8")).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const tc4Package = JSON.parse(fs.readFileSync(path.join(tc4, "package.json"), "utf8"));

const work = fs.mkdtempSync(path.join(os.tmpdir(), "tc-check-psalms-"));
const entry = path.join(work, "entry.ts");
fs.writeFileSync(
  entry,
  `export { deriveForProject, deriveTnItems, deriveTwlItems, deriveTqItems, twArticlesOf } from ${JSON.stringify(path.join(tc4, "src/data/derive"))};`,
);
const bundle = path.join(work, "tc4-derive.cjs");
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
const { unzipSync } = require(path.join(nm, "fflate"));

const sha256 = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
const md5 = (text) => crypto.createHash("md5").update(text, "utf8").digest("hex");

// The exports the app ships, as bundle/en/README.md lists them.
const exportsUsed = {
  en_tn: "bundle/en/en_tn_v91.sb.zip",
  en_twl: "bundle/en/en_twl_v91.sb.zip",
  en_tq: "bundle/en/en_tq_v91.sb.zip",
  en_tw: "bundle/en/en_tw_v91.sb.zip",
};
const inputs = {};
const tsvFiles = {};
for (const [repo, zipPath] of Object.entries(exportsUsed)) {
  const bytes = fs.readFileSync(zipPath);
  inputs[repo] = { file: zipPath, sha256: sha256(bytes) };
  const wanted = repo === "en_tw" ? (name) => name === `${repo}/metadata.json` : (name) => /^[^/]+\/ingredients\/[0-9A-Z]{3}\.tsv$/.test(name);
  const files = unzipSync(new Uint8Array(bytes), { filter: (f) => wanted(f.name) });
  tsvFiles[repo] = Object.fromEntries(
    Object.entries(files)
      .map(([name, data]) => [name.split("/").pop().replace(/\.tsv$/, ""), Buffer.from(data).toString("utf8")])
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  );
}
const twArticles = tc.twArticlesOf(JSON.parse(tsvFiles.en_tw["metadata.json"]));

// ---- every TSV: the rows tC4's reader returns and the lines it skips -------------------
const reader = {
  en_tn: (tsv, book) => tc.deriveTnItems(tsv, book, { keepPlain: true }),
  en_twl: (tsv, book) => tc.deriveTwlItems(tsv, book),
  en_tq: (tsv, book) => tc.deriveTqItems(tsv, book),
};
const rows = {};
for (const repo of ["en_tn", "en_twl", "en_tq"]) {
  rows[repo] = {};
  for (const [book, tsv] of Object.entries(tsvFiles[repo])) {
    // tsvRows (derive.ts) drops every line whose trim() is empty; line numbers count
    // from 1 at the header, as tc-helps names them.
    const skipped = tsv
      .split("\n")
      .map((line, i) => [line, i + 1])
      .filter(([line]) => line.trim() === "")
      .map(([, n]) => n);
    rows[repo][book] = { rows: reader[repo](tsv, book.toLowerCase()).length, skipped };
  }
}

// ---- Psalms check lists ------------------------------------------------------------------
const schemes = Object.fromEntries(
  ["eng", "org"].map((n) => [n, JSON.parse(fs.readFileSync(`fixtures/tc4/test-fixtures/vrs/${n}.json`, "utf8"))]),
);
const TOOLS = { translationNotes: "en_tn", translationWords: "en_twl" };
const derive = (tool, book, to, scopeRanges) =>
  tc.deriveForProject({
    tsv: tsvFiles[TOOLS[tool]][book],
    tool,
    bookId: book.toLowerCase(),
    from: "eng",
    to,
    schemes,
    scopeRanges,
    twArticles: tool === "translationWords" ? twArticles : undefined,
  });
const identity = (c) => [c.checkId, c.reference.chapter, c.reference.verse, c.occurrence];

const main = async () => {
  const cases = [];
  const plan = [
    { to: "eng", scope: [], full: true },
    { to: "eng", scope: ["3"] },
    { to: "eng", scope: ["3:1-8"] },
    { to: "eng", scope: ["2:5-4:2"] },
    { to: "eng", scope: ["3-4"] },
    { to: "eng", scope: ["5:1", "5:3"] },
    { to: "eng", scope: ["119:1-8"] },
    { to: "org", scope: [] },
    { to: "org", scope: ["3"] },
  ];
  for (const tool of Object.keys(TOOLS)) {
    for (const { to, scope, full } of plan) {
      const out = await derive(tool, "PSA", to, scope);
      cases.push({
        tool,
        from: "eng",
        to,
        scope,
        mapped: out.mapped,
        // [checkId, chapter, verse, occurrence], with groupId and category in the
        // whole-book eng lists.
        items: out.items.map((it) =>
          full ? [...identity(it.contextId), it.contextId.groupId, it.category] : identity(it.contextId),
        ),
        unplaceable: out.unplaceable.map((u) => [...identity(u.item.contextId), u.reason]),
      });
    }
  }

  // Every book, whole-book eng: count and md5 of the identities in order.
  const books = {};
  for (const tool of Object.keys(TOOLS)) {
    books[tool] = {};
    for (const book of Object.keys(tsvFiles[TOOLS[tool]])) {
      const out = await derive(tool, book, "eng", []);
      const every = out.items.filter((it) => it.contextId.occurrence === -1);
      const lines = out.items
        .filter((it) => it.contextId.occurrence !== -1)
        .map((it) => {
          const c = it.contextId;
          return [c.checkId, c.reference.chapter, c.reference.verse, c.occurrence, c.groupId].join("|");
        });
      books[tool][book] = {
        checks: lines.length,
        md5: md5(lines.join("\n")),
        every: every.map((it) => identity(it.contextId).slice(0, 3)),
      };
    }
  }

  const tc4Source = { commit: "d494a9d6ba9f4cdcc09faff9038c93159106045b", version: tc4Package.version };
  const write = (file, body) => {
    const text = JSON.stringify(
      { generator: "crates/tc-check/tools/gen-psalms-vectors.cjs", tc4: tc4Source, libraries: pinned, inputs, ...body },
      null,
      1,
    );
    // One item per line keeps the file reviewable.
    fs.writeFileSync(file, text.replace(/\[\n\s+([^\[\]{}]*?)\n\s+\]/g, (_, inner) => `[${inner.replace(/,\n\s+/g, ",")}]`) + "\n");
  };
  write("fixtures/check/psalms-vectors.json", {
    note: "deriveForProject on the Psalms rows; twArticles from en_tw v91 metadata.json for translationWords; schemes from fixtures/tc4/test-fixtures/vrs",
    cases,
    books,
  });
  write("fixtures/helps/v91-tsv-rows.json", {
    note: "rows: the length of deriveTnItems(keepPlain) / deriveTwlItems / deriveTqItems; skipped: the lines (from 1 at the header) tsvRows drops",
    rows,
  });
};
main().catch((e) => {
  console.error(e);
  process.exit(1);
});
