// Generates fixtures/usfm/tc3-usfm-vectors.json: the USFM tC4's tC3 importer makes of a
// book's chapter files (`bookUsfm` in tC4's src/data/import/tc3.ts), which is usfm-js
// 3.4.3's `toUSFM({ headers, chapters }, { forcedNewLines: true })` over one text object
// per verse.
//
// - `books`: every book of the tC3 fixture zips. The Rust test reads the same zips, so
//   only the output's MD5 and length are stored, and the whole output for Titus.
// - `cases`: hand-written books (paragraph and poetry markers inside a verse, notes,
//   words, milestones, CRLF, spans, `front`, odd headers, odd verse keys).
//
// Each case's input is the headers as headers.json holds them and the chapters in the
// order of their files, each with its verses as [key, text] in the order of the file;
// `bookUsfm` below builds tC4's object from that exactly as tC4 does.
//
// Run from the repository root (needs `unzip`):
//   node crates/tc-usfm/tools/gen-tc3-usfm-vectors.cjs <path to node_modules> > fixtures/usfm/tc3-usfm-vectors.json
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { execFileSync } = require("child_process");
const nm = process.argv[2];
if (!nm) throw new Error("usage: gen-tc3-usfm-vectors.cjs <node_modules>");
const found = JSON.parse(fs.readFileSync(path.join(nm, "usfm-js", "package.json"), "utf8")).version;
if (found !== "3.4.3") throw new Error(`expected usfm-js@3.4.3, found ${found}`);
const usfmjs = require(path.join(nm, "usfm-js"));
// toUSFM logs every verse it rewrites.
console.log = () => {};

// tC4's bookUsfm, from the inputs as described above.
const bookUsfm = (headersIn, chaptersIn) => {
  const chapters = {};
  for (const [chapter, verses] of chaptersIn) {
    chapters[chapter] = {};
    for (const [verse, text] of verses) chapters[chapter][verse] = { verseObjects: [{ text: `${text}\n`, type: "text" }] };
  }
  const headers = headersIn.map((h) => ({ ...h }));
  const usfmHeader = headers.find((h) => h.tag === "usfm");
  if (usfmHeader) usfmHeader.content = "3.0";
  else headers.splice(headers.length < 2 ? headers.length : 1, 0, { tag: "usfm", content: "3.0" });
  return usfmjs.toUSFM({ headers, chapters }, { forcedNewLines: true });
};

const root = "fixtures/tc4/conformance-fixtures/import/tc3";
const zips = [`${root}/cfm_fbt_tit_book.zip`, ...fs.readdirSync(`${root}/multi`).sort().map((f) => `${root}/multi/${f}`)];
const books = [];
for (const zip of zips) {
  const names = execFileSync("unzip", ["-Z1", zip], { encoding: "utf8" }).split("\n");
  const read = (name) => JSON.parse(execFileSync("unzip", ["-p", zip, name], { encoding: "utf8", maxBuffer: 1 << 28 }));
  const book = names.map((n) => /^([a-z0-9]+)\/headers\.json$/.exec(n)?.[1]).find(Boolean);
  const chapters = names
    .filter((n) => new RegExp(`^${book}/\\d+\\.json$`).test(n))
    .map((n) => [String(parseInt(n.slice(book.length + 1), 10)), Object.entries(read(n))]);
  const usfm = bookUsfm(read(`${book}/headers.json`), chapters);
  const entry = { zip, book, md5: crypto.createHash("md5").update(usfm, "utf8").digest("hex"), length: Buffer.byteLength(usfm) };
  if (book === "tit") entry.usfm = usfm;
  books.push(entry);
}

const H = [{ tag: "id", content: "TIT EN_ULT" }, { tag: "usfm", content: "2.0" }, { tag: "h", content: "Titus" }];
const one = (verses) => [["1", verses]];
const handwritten = {
  plain: [H, one([["1", "Paul, a servant"], ["2", "in hope"]])],
  paragraph_end: [H, one([["1", "first\n\\p"], ["2", "second"], ["3", "third"]])],
  paragraph_middle: [H, one([["1", "first \\p second"], ["2", "next"]])],
  paragraph_spaces: [H, one([["1", "first  \n\\p   \n"], ["2", " lead"], ["3", "x\\p "]])],
  poetry: [H, one([["1", "\\q1 line one\n\\q2 line two"], ["2", "\\q1 more"], ["3", "after"]])],
  poetry_before_paragraph: [H, one([["1", "\\q1 a"], ["2", "\\q2 b"], ["3", "\\m c"], ["4", "\\q1 d"], ["5", "e"]])],
  margin_blank: [H, one([["1", "a\n\\b\n\\m b"], ["2", "c\\b"], ["3", "\\b"]])],
  heading: [H, one([["1", "a\n\\s Heading\n\\p b"], ["2", "\\s Heading only"], ["3", "\\s1 x \\p y"]])],
  chapter_in_verse: [H, [["1", [["1", "a\n\\c 2\n\\p b"], ["2", "c"]]], ["2", [["1", "d"]]]]],
  chapter_text_after: [H, one([["1", "a \\c 7 words after \\p p"], ["2", "b \\c x"], ["3", "\\c"]])],
  n_marker: [H, one([["1", "a \\nb b"], ["2", "\\nd Lord\\nd* and \\p"], ["3", "\\no x"]])],
  footnote: [H, one([["1", "a\\f + \\ft note\\f* b"], ["2", "a\\f + \\fr 1:2 \\ft note \\p in note\\f* b"], ["3", "x"]])],
  words: [H, one([["1", "\\w Paul|strong=\"G3972\" x-morph=\"Gr,N\"\\w*, \\w a|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\n\\p \\w servant\\w*"], ["2", "z"]])],
  words_odd: [H, one([["1", "\\w a|lemma=\"x\" strongs=\"G1\" x-tw=\"t\" x-foo=\"b\" 1=\"z\"\\w*\\p \\w b|plain\\w* \\w c|\\w*"], ["2", "y"]])],
  milestones: [
    H,
    one([
      ["1", "\\zaln-s |x-strong=\"G3972\" x-lemma=\"Παῦλος\" x-morph=\"Gr,N,,,,,NMS,\" x-occurrence=\"1\" x-occurrences=\"1\" x-content=\"Παῦλος\"\\*\\w Paul|x-occurrence=\"1\" x-occurrences=\"1\"\\w*\\zaln-e\\*,\n\\p \\zaln-s |x-content=\"δοῦλος\"\\*\\w servant\\w*\\zaln-e\\*\\zaln-s |x-content=\"a\"\\*\\w b\\w*\\zaln-e\\*"],
      ["2", "\\zaln-s |x-content=\"θεοῦ\"\\*\\zaln-s |x-content=\"x\"\\*\\w of\\w* \\w God\\w*\\zaln-e\\*\\zaln-e\\* \\p end"],
      ["3", "after"],
    ]),
  ],
  milestone_kinds: [H, one([["1", "\\ts\\* a \\qt-s |sid=\"q1\"\\*quote\\qt-e\\* \\p b \\k-s |x-k=\"1\"\\*k\\k-e\\* \\ts-s |a\\*"], ["2", "\\qs Selah\\qs* \\b"]])],
  character: [H, one([["1", "\\add added\\add* \\bd bold \\+it it\\+it*\\bd* \\wj words\\wj*\n\\p"], ["2", "\\xt x\\xt* \\x - \\xo 1 \\xt y\\x* \\m"]])],
  crlf: [H, one([["1", "a\r\n\\p b\r\n"], ["2", "c\r\n"], ["3", "d\r\n\\q1 e\r\n"], ["4", "f"]])],
  trailing_spaces: [H, one([["1", "a  "], ["2", "b \\p  "], ["3", " c "], ["4", "\\p"], ["5", "\\p "]])],
  empty: [H, one([["1", ""], ["2", ""], ["3", "\\p"], ["4", ""]])],
  spans: [H, one([["1", "a"], ["3-4", "span"], ["2", "b"], ["5", "c\\p"], ["3", "three"], ["10", "ten"], ["4-5", "x"]])],
  front: [H, [["1", [["front", "\\s Intro \\p"], ["1", "a"], ["2", "b"]]], ["2", [["front", "plain front"], ["1", "c"]]]]],
  front_paragraph_first: [H, one([["1", "a"], ["front", "x\\p y"]])],
  verse_markers_inside: [H, one([["1", "a \\v 2 b \\p c"], ["2", "d"], ["3", "\\v 9-10 e\\p"]])],
  odd_keys: [H, one([["b", "bee"], ["1", "one"], ["a", "ay"], ["01", "zero one"], ["2", "two"], ["", "empty"], [" 3", "three"]])],
  odd_keys_paragraph: [H, one([["x", "\\p ex"], ["1", "one"], ["y", "why"]])],
  duplicate_keys: [H, one([["2", "first two"], ["1", "one"], ["2", "second two"]])],
  chapter_order: [H, [["10", [["1", "ten"]]], ["2", [["1", "two"]]], ["1", [["1", "one"]]], ["2", [["1", "two again"]]], ["0", [["1", "zero"]]]]],
  big_chapter_number: [H, [["99999999999", [["1", "big"]]], ["3", [["1", "three"]]], ["4294967295", [["1", "max"]]], ["4294967294", [["1", "index"]]]]],
  headers_odd: [
    [
      { tag: "id", content: "TIT" },
      { tag: "toc1", content: "-dash" },
      { tag: "toc2", content: "*star" },
      { tag: "toc3", content: "\\*esc" },
      { tag: "mt", content: "" },
      { tag: "rem" },
      { type: "text", text: "loose text" },
      { tag: "ide", content: "UTF-8", type: "x" },
      { tag: "c", number: "1", content: "n" },
      { tag: "bd", content: "bold" },
      { tag: "ts", content: "ts" },
      { tag: "mt2", text: "text only" },
      { tag: "zx", content: "a", attrib: " |x=\"1\"" },
      { tag: "f", content: "+ note", endTag: "f*", nextChar: " " },
    ],
    one([["1", "a"]]),
  ],
  headers_none: [[], one([["1", "a"]])],
  headers_one: [[{ tag: "id", content: "TIT" }], one([["1", "a\\p"], ["2", "b"]])],
  headers_usfm_first: [[{ tag: "usfm", content: "3.1" }, { tag: "id", content: "TIT" }], one([["1", "a"]])],
  headers_no_usfm: [[{ tag: "id", content: "TIT" }, { tag: "h", content: "Titus" }, { tag: "mt", content: "Titus" }], one([["1", "a"]])],
  newline_start: [H, one([["1", "\nlead"], ["2", " \n"], ["3", "\\p\na"], ["4", "b"]])],
  quote_before_verse: [H, one([["1", "a\n\\q1 b"], ["2", "c\\p"], ["3", "\\q1 d"], ["4", "e"]])],
  unknown_markers: [H, one([["1", "\\pp a \\zx b\\zx* \\li1 c"], ["2", "\\pi d \\mi e"], ["3", "\\ms x \\cl y \\cp z \\ca q\\ca*"]])],
  table_and_lists: [H, one([["1", "\\tr \\th1 a \\tc1 b \\p"], ["2", "\\lh x \\li1 y \\lf z \\p"]])],
  paragraph_then_spaces: [H, one([["1", "a \\p   b"], ["2", "\\p \\p x"], ["3", "\\m \\q1  y"], ["4", "\\p \\w w\\w* z"], ["5", "\\p\n  \n\\p"]])],
  paragraph_inside_milestone: [H, one([["1", "\\zaln-s |x-content=\"a\"\\*\\w a\\w*\n\\p \\w b\\w*\\zaln-e\\*"], ["2", "\\qt-s |who=\"x\"\\*\\p q\\qt-e\\* \\ts\\*\\p"], ["3", "\\f + \\fr 1 \\ft x \\fqa y\\fqa*\\f*\\p"]])],
  chapter_markers: [H, one([["1", "a \\c 2 \\p after"], ["2", "\\c 3\n\\v 4 four\\p"], ["3", "x \\c 2\n𝄞y\\p"], ["4", "\\c 05 \\c 5 \\c 0 z\\b"]])],
  recursion_endless: [H, one([["1", "a"], ["2", "b \\c x"]])],
  recursion_endless_bare: [H, one([["1", "\\c"]])],
  sort_not_numbers: [H, one(["q", "3", "b", "", "12", "x9", "7", "a", "-1", "1-2", "z", "2", "0", "c", "10", "front2", "5", "y", "4", "m", "1", "e"].map((k, i) => [k, `v${i}`]))],
  sort_not_numbers_paragraph: [H, one([["c", "\\p one"], ["2", "two"], ["a", "x"], ["1", "y\\p"], ["b", "z"]])],
  header_tag_escape: [[{ tag: "id", content: "TIT" }, { tag: "zq\\*", content: "x", attrib: " a=\"1\"" }, { tag: "f", endTag: "", content: "note" }, { tag: "w", content: "word", nextChar: "\n" }], one([["1", "a"]])],
  backslash_alone: [H, one([["1", "a \\ b \\p"], ["2", "\\"], ["3", "\\*\\p"]])],
};
// `usfm` is null where toUSFM throws (it recurses without end on some verses).
const attempt = (headers, chapters) => {
  try {
    return bookUsfm(headers, chapters);
  } catch (e) {
    if (!(e instanceof RangeError || e instanceof TypeError)) throw e;
    return null;
  }
};
const cases = Object.entries(handwritten).map(([name, [headers, chapters]]) => ({ name, headers, chapters, usfm: attempt(headers, chapters) }));

process.stdout.write(JSON.stringify({ books, cases }, null, 1) + "\n");
