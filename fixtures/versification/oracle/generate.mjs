// generate.mjs — expected versification outputs, produced by proskomma-core itself
// and by tC4's own mapReference.ts, for the Rust port in crates/tc-versification.
//
// Run (Node 22.18 or later, for TypeScript type stripping):
//
//   mkdir -p "$ORACLE_DIR" && cd "$ORACLE_DIR" && npm init -y && npm i proskomma-core@0.11.3
//   ORACLE_DIR=... TC4_DIR=<translationCore4 checkout> node fixtures/versification/oracle/generate.mjs
//
// It reads the scheme fixtures under fixtures/, and writes:
//   vrs/proskomma/*.vrs        the six schemes bundled in proskomma-core, as published
//   oracle/vrs2json.json       Proskomma's vrs2json() of every .vrs fixture
//   oracle/hops.json           mapVerse() through every forward and reverse table
//   oracle/compose.json        two-hop scheme-to-scheme mapping through org
//   oracle/tc4-map-reference.json  tC4's mapReference() outcomes
//
// Outputs list only results that differ from "unchanged", over a stated input domain,
// so a test can check every input of the domain and stay small on disk.

import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const FIXTURES = path.resolve(HERE, '..', '..');
const OUT = path.join(FIXTURES, 'versification');
const ORACLE_DIR = process.env.ORACLE_DIR;
const TC4_DIR = process.env.TC4_DIR;
if (!ORACLE_DIR || !TC4_DIR) throw new Error('set ORACLE_DIR and TC4_DIR');

const require = createRequire(path.join(ORACLE_DIR, 'package.json'));
const pk = require('proskomma-core');
const V = pk.utils.versification;
const PROSKOMMA_VERSION = JSON.parse(
  fs.readFileSync(path.join(ORACLE_DIR, 'node_modules/proskomma-core/package.json'), 'utf8'),
).version;
if (PROSKOMMA_VERSION !== '0.11.3') throw new Error(`expected proskomma-core 0.11.3, got ${PROSKOMMA_VERSION}`);

const writeJson = (rel, value) => {
  const file = path.join(OUT, rel);
  fs.writeFileSync(file, JSON.stringify(value, null, 1) + '\n');
  console.log(`${rel}: ${fs.statSync(file).size} bytes`);
};

// ---------------------------------------------------------------------------
// The six .vrs schemes proskomma-core ships, exactly as the package serves them.
// ---------------------------------------------------------------------------

const pkInstance = new pk.Proskomma();
const bundled = pkInstance.gqlQuerySync('{ versifications { id vrs } }');
if (bundled.errors) throw new Error(JSON.stringify(bundled.errors));
fs.mkdirSync(path.join(OUT, 'vrs/proskomma'), { recursive: true });
for (const { id, vrs } of bundled.data.versifications) {
  fs.writeFileSync(path.join(OUT, `vrs/proskomma/${id}.vrs`), vrs);
}

const vrsFiles = [];
for (const dir of ['vrs/proskomma', 'vrs/proskomma-tests']) {
  for (const f of fs.readdirSync(path.join(OUT, dir)).sort()) {
    if (f.endsWith('.vrs')) vrsFiles.push(`${dir}/${f}`);
  }
}

// ---------------------------------------------------------------------------
// vrs2json
// ---------------------------------------------------------------------------

const vrsJson = {};
for (const rel of vrsFiles) {
  vrsJson[rel] = V.vrs2json(fs.readFileSync(path.join(OUT, rel), 'utf8')).mappedVerses;
}
writeJson('oracle/vrs2json.json', {
  proskomma: PROSKOMMA_VERSION,
  note: 'vrs2json(text).mappedVerses for each file, key order as Proskomma produced it',
  files: vrsJson,
});

// ---------------------------------------------------------------------------
// Schemes and their compiled tables
// ---------------------------------------------------------------------------

const TC4_VRS = path.join(FIXTURES, 'tc4/test-fixtures/vrs');
const JSON_SCHEMES = ['eng', 'lxx', 'org', 'rsc', 'rso', 'vul'];
const docs = {};
for (const n of JSON_SCHEMES) {
  docs[n] = JSON.parse(fs.readFileSync(path.join(TC4_VRS, `${n}.json`), 'utf8'));
}
docs.syn = JSON.parse(fs.readFileSync(path.join(FIXTURES, 'tc4/test-fixtures/vrs-synthetic/syn.json'), 'utf8'));

// tC4 normalizes string values to arrays before calling Proskomma (versification.ts
// normalizeScheme); reverseVersification iterates characters otherwise.
const arrayify = (mv) =>
  Object.fromEntries(Object.entries(mv ?? {}).map(([k, v]) => [k, Array.isArray(v) ? v : [v]]));

const mappingSources = {};
for (const [n, doc] of Object.entries(docs)) mappingSources[`json:${n}`] = arrayify(doc.mappedVerses);
for (const rel of vrsFiles) mappingSources[`vrs:${rel.replace(/^vrs\//, '').replace(/\.vrs$/, '')}`] = vrsJson[rel];

const compile = (mv) => {
  const out = {};
  try {
    out.forward = V.succinctifyVerseMappings(mv);
  } catch (e) {
    out.forwardError = String(e.message);
  }
  try {
    out.reverse = V.succinctifyVerseMappings(V.reverseVersification({ mappedVerses: mv }).reverseMappedVerses);
  } catch (e) {
    out.reverseError = String(e.message);
  }
  return out;
};
const tables = Object.fromEntries(Object.entries(mappingSources).map(([k, mv]) => [k, compile(mv)]));

// The same lookup tC4 (applyOne) and Proskomma (do_cv) make: a chapter with no table
// maps to itself.
const apply = (succinct, book, c, v) => {
  const chapterTable = succinct?.[book]?.[String(c)];
  if (!chapterTable) return [book, [[c, v]]];
  return V.mapVerse(chapterTable, book, c, v);
};
const isIdentity = ([b, cvs], book, c, v) => b === book && cvs.length === 1 && cvs[0][0] === c && cvs[0][1] === v;
const hopText = ([b, cvs]) => `${b} ${cvs.map(([c, v]) => `${c}:${v}`).join(',')}`;

// ---------------------------------------------------------------------------
// Input domain for single-table lookups: every chapter any scheme or table names,
// verses 0 through one past the largest verse anything names there.
// ---------------------------------------------------------------------------

const domainMax = new Map(); // "BOOK c" -> max verse
const note = (book, c, v) => {
  const k = `${book} ${c}`;
  domainMax.set(k, Math.max(domainMax.get(k) ?? 0, v));
};
for (const doc of Object.values(docs)) {
  for (const [book, chapters] of Object.entries(doc.maxVerses)) {
    chapters.forEach((last, i) => note(book, i + 1, Number(last)));
  }
}
for (const mv of Object.values(mappingSources)) {
  for (const table of [mv, V.reverseVersification({ mappedVerses: mv }).reverseMappedVerses]) {
    for (const [book, chapters] of Object.entries(V.preSuccinctVerseMapping(table))) {
      for (const [c, records] of Object.entries(chapters)) {
        for (const [, [fs_, fe], targets] of records) {
          note(book, Number(c), Math.max(fs_, fe));
          for (const [tc, tv, te, tb] of targets) note(tb ?? book, tc, Math.max(tv, Number.isNaN(te) ? 0 : te));
        }
      }
    }
  }
}
const domain = [...domainMax.entries()]
  .map(([k, max]) => {
    const [book, c] = k.split(' ');
    return [book, Number(c), max + 1];
  })
  .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : a[1] - b[1]));

// Results are written compactly. A line is either
//   'B c:v=B c:v[,c:v...][;B c:v...]'  one input and its full result, or
//   'B c:v1..v2=B c:w'                 a run: every v in v1..=v2 maps to the single verse
//                                      B c:(w + v - v1). v1 can equal v2.
// Inputs whose result is themselves are left out.
const runEncoder = () => {
  const lines = [];
  let run = null;
  const flush = () => {
    if (!run) return;
    const range = `${run.v1}..${run.v2}`;
    lines.push(`${run.book} ${run.c}:${range}=${run.ob} ${run.oc}:${run.ow}`);
    run = null;
  };
  return {
    // single: [outBook, outChapter, outVerse] when the result is one verse, else null.
    push(book, c, v, single, text) {
      if (
        single &&
        run &&
        run.book === book &&
        run.c === c &&
        run.v2 + 1 === v &&
        run.ob === single[0] &&
        run.oc === single[1] &&
        run.ow + (v - run.v1) === single[2]
      ) {
        run.v2 = v;
        return;
      }
      flush();
      if (single) run = { book, c, v1: v, v2: v, ob: single[0], oc: single[1], ow: single[2] };
      else lines.push(`${book} ${c}:${v}=${text}`);
    },
    // An input that maps to itself ends any open run.
    skip() {
      flush();
    },
    done() {
      flush();
      return lines;
    },
  };
};

const singleOf = (hops) => (hops.length === 1 && hops[0][1].length === 1 ? [hops[0][0], ...hops[0][1][0]] : null);

const hopsOut = {};
for (const [name, compiled] of Object.entries(tables)) {
  hopsOut[name] = {};
  for (const dir of ['forward', 'reverse']) {
    if (compiled[`${dir}Error`]) {
      hopsOut[name][dir] = { error: compiled[`${dir}Error`] };
      continue;
    }
    const enc = runEncoder();
    for (const [book, c, max] of domain) {
      for (let v = 0; v <= max; v++) {
        const r = apply(compiled[dir], book, c, v);
        if (isIdentity(r, book, c, v)) enc.skip();
        else enc.push(book, c, v, singleOf([r]), hopText(r));
      }
    }
    hopsOut[name][dir] = { moved: enc.done() };
  }
}
writeJson('oracle/hops.json', {
  proskomma: PROSKOMMA_VERSION,
  note:
    'mapVerse() through each forward table (scheme -> org) and reverse table (org -> scheme), looked up ' +
    'the way tC4 applyOne and Proskomma do_cv do (no table for the chapter: unchanged). domain: ' +
    "'B c:last', every verse 0..=last of each listed chapter was mapped. moved: every result other " +
    "than the input itself, as 'B c:v=B c:v[,c:v...]' or the run form 'B c:v1..v2=B c:w'.",
  domain: domain.map(([b, c, max]) => `${b} ${c}:${max}`),
  tables: hopsOut,
});

// ---------------------------------------------------------------------------
// Two-hop mapping, as Proskomma's do_cv composes it: forward through the source
// table, then each pivot verse back through the target's reverse table.
// ---------------------------------------------------------------------------

// Verse counts of a .vrs file, read as the Rust parser reads them: a chapter that a
// `BOOK c:n c:n ...` line skips has no verses (vul.vrs lists `6EZ 1:63 12:78`).
const vrsMaxVerses = (text) => {
  const out = {};
  for (const line of text.split(/[\n\r]+/).map((l) => l.trim())) {
    if (!/^[A-Z0-9]{3}(\s+[0-9]+:[0-9]+)+$/.test(line)) continue;
    const [book, ...cvs] = line.split(/\s+/);
    const counts = [];
    for (const cv of cvs) {
      const [c, n] = cv.split(':').map(Number);
      while (counts.length < c - 1) counts.push('0');
      counts.push(String(n));
    }
    out[book] = counts;
  }
  return out;
};

// Every scheme a pair can name: tC4's JSON schemes by bare name, and Proskomma's own
// .vrs copies, which keep one-to-many entries, as `vrs:proskomma/<name>`.
const pairSchemes = {};
for (const n of Object.keys(docs)) {
  pairSchemes[n] = { maxVerses: docs[n].maxVerses, table: tables[`json:${n}`], mappedVerses: arrayify(docs[n].mappedVerses) };
}
for (const n of JSON_SCHEMES) {
  const name = `vrs:proskomma/${n}`;
  const text = fs.readFileSync(path.join(OUT, `vrs/proskomma/${n}.vrs`), 'utf8');
  pairSchemes[name] = { maxVerses: vrsMaxVerses(text), table: tables[name], mappedVerses: vrsJson[`vrs/proskomma/${n}.vrs`], vrs: text };
}

const PAIRS = [];
for (const from of [...JSON_SCHEMES, 'syn']) {
  for (const to of [...JSON_SCHEMES, 'syn']) {
    if (from === to) continue;
    if ((from === 'syn' || to === 'syn') && ![from, to].includes('eng') && ![from, to].includes('org')) continue;
    PAIRS.push([from, to]);
  }
}
for (const n of ['eng', 'lxx', 'rsc', 'rso', 'vul']) {
  PAIRS.push([`vrs:proskomma/${n}`, 'vrs:proskomma/org'], ['vrs:proskomma/org', `vrs:proskomma/${n}`]);
  if (n !== 'eng') PAIRS.push([`vrs:proskomma/${n}`, 'vrs:proskomma/eng'], ['vrs:proskomma/eng', `vrs:proskomma/${n}`]);
}

// Calls `visit(book, c, v, hops)` for every input of a pair's domain: every verse
// 0..=last+1 of every chapter in the source scheme's verse counts.
const composePair = (from, to, visit) => {
  const F = pairSchemes[from].table.forward;
  const R = pairSchemes[to].table.reverse;
  for (const [book, chapters] of Object.entries(pairSchemes[from].maxVerses)) {
    chapters.forEach((last, i) => {
      const c = i + 1;
      for (let v = 0; v <= Number(last) + 1; v++) {
        const [pb, pcvs] = apply(F, book, c, v);
        visit(book, c, v, pcvs.map(([oc, ov]) => apply(R, pb, oc, ov)));
      }
    });
  }
};

const composeOut = {};
const composeInputs = {};
for (const [from, to] of PAIRS) {
  const enc = runEncoder();
  let inputs = 0;
  composePair(from, to, (book, c, v, hops) => {
    inputs++;
    if (hops.length === 1 && isIdentity(hops[0], book, c, v)) enc.skip();
    else enc.push(book, c, v, singleOf(hops), hops.map(hopText).join(';'));
  });
  composeOut[`${from}>${to}`] = enc.done();
  composeInputs[`${from}>${to}`] = inputs;
}

// ---------------------------------------------------------------------------
// The same pairs through Proskomma's own query path. Each scheme gets a docSet whose
// documents hold every chapter of its verse counts, with text that names its verse
// (the text before verse 1 is verse 0), and setVerseMapping gives the docSet the
// scheme's mappings. mappedCv (do_cv) on a source document then reads the target
// docSet, so the returned text says where Proskomma landed. The composition above
// must agree with it on every input, once do_cv's reading of documents is accounted
// for:
//
// - with no document for the pivot book in the target docSet, do_cv skips the reverse
//   hop and then finds no document to read, so it returns nothing;
// - it reads every landing from the first landing's book;
// - it leaves out a verse the document lacks.
// ---------------------------------------------------------------------------

const docSetUsfm = (book, counts) => {
  let s = `\\id ${book}\n`;
  counts.forEach((n, i) => {
    s += `\\c ${i + 1}\n\\p\n${book}x${i + 1}x0\n`;
    for (let v = 1; v <= Number(n); v++) s += `\\v ${v} ${book}x${i + 1}x${v}\n`;
  });
  return s;
};
const vrsText = (mv) =>
  Object.entries(mv)
    .flatMap(([k, ts]) => ts.map((t) => `${k} = ${t}\n`))
    .join('');

const docSetPk = new pk.Proskomma();
const docSetIds = {};
for (const name of new Set(PAIRS.flat())) {
  const abbr = `s${Object.keys(docSetIds).length}`;
  docSetIds[name] = `xxx_${abbr}`;
  const scheme = pairSchemes[name];
  for (const [book, counts] of Object.entries(scheme.maxVerses)) {
    docSetPk.importDocument({ lang: 'xxx', abbr }, 'usfm', docSetUsfm(book, counts));
  }
  // setVerseMapping's jsonSource argument is a GraphQL string, which it never parses,
  // so the JSON schemes go in as .vrs text that vrs2json must read back unchanged.
  const vrs = scheme.vrs ?? vrsText(scheme.mappedVerses);
  if (JSON.stringify(V.vrs2json(vrs).mappedVerses) !== JSON.stringify(scheme.mappedVerses)) {
    throw new Error(`${name}: the .vrs text does not carry the scheme's mappings`);
  }
  const r = docSetPk.gqlQuerySync(
    `mutation { setVerseMapping(docSetId: "${docSetIds[name]}", vrsSource: ${JSON.stringify(vrs)}) }`,
  );
  if (r.errors || !r.data.setVerseMapping) throw new Error(`${name}: ${JSON.stringify(r)}`);
}

const holds = (maxVerses, book, c, v) => {
  const counts = maxVerses[book];
  return !!counts && c >= 1 && c <= counts.length && v >= 0 && v <= Number(counts[c - 1]);
};

let mappedCvChecked = 0;
for (const [from, to] of PAIRS) {
  const pair = `${from}>${to}`;
  const target = pairSchemes[to].maxVerses;
  const expected = new Map();
  composePair(from, to, (book, c, v, hops) => {
    const pivotBook = apply(pairSchemes[from].table.forward, book, c, v)[0];
    const readBook = hops[0][0];
    const read = pivotBook in target
      ? hops.flatMap(([, cvs]) => cvs).filter(([tc, tv]) => holds(target, readBook, tc, tv)).map(([tc, tv]) => `${readBook} ${tc}:${tv}`)
      : [];
    expected.set(`${book} ${c}:${v}`, read);
  });
  for (const [book, chapters] of Object.entries(pairSchemes[from].maxVerses)) {
    chapters.forEach((last, i) => {
      const c = i + 1;
      const fields = [];
      for (let v = 0; v <= Number(last) + 1; v++) {
        fields.push(`v${v}: mappedCv(chapter: "${c}", verses: ["${v}"], mappedDocSetId: "${docSetIds[to]}") { text }`);
      }
      const r = docSetPk.gqlQuerySync(
        `{ docSet(id: "${docSetIds[from]}") { document(bookCode: "${book}") { ${fields.join(' ')} } } }`,
      );
      if (r.errors) throw new Error(`${pair} ${book} ${c}: ${JSON.stringify(r.errors)}`);
      for (let v = 0; v <= Number(last) + 1; v++) {
        const got = r.data.docSet.document[`v${v}`].map(({ text }) => {
          const m = /^([A-Z0-9]{3})x(\d+)x(\d+)$/.exec(text.trim());
          if (!m) throw new Error(`${pair} ${book} ${c}:${v}: unexpected text ${JSON.stringify(text)}`);
          return `${m[1]} ${m[2]}:${m[3]}`;
        });
        const want = expected.get(`${book} ${c}:${v}`);
        if (JSON.stringify(got) !== JSON.stringify(want)) {
          throw new Error(`${pair} ${book} ${c}:${v}: mappedCv gave ${JSON.stringify(got)}, the composition ${JSON.stringify(want)}`);
        }
        mappedCvChecked++;
      }
    });
  }
  console.log(`mappedCv agrees on ${pair}`);
}

writeJson('oracle/compose.json', {
  proskomma: PROSKOMMA_VERSION,
  note:
    "Two-hop mapping from one scheme to another through org, as Proskomma's do_cv does it: forward " +
    "through the source scheme's table, then each pivot verse back through the target's reversed " +
    "table. Bare names are tC4's JSON schemes; vrs:proskomma/<name> are Proskomma's own .vrs copies. " +
    "Domain: every verse 0..=last+1 of every chapter in the source scheme's verse counts (`inputs` " +
    "gives the count per pair). Results other than the input itself, as 'B c:v=hop[;hop...]' (one " +
    "reverse hop per pivot verse) or the run form 'B c:v1..v2=B c:w'. Every input was also run " +
    `through Proskomma's mappedCv query on generated docSets (${mappedCvChecked} queries), which ` +
    'agreed once its reading of documents is accounted for (see generate.mjs).',
  inputs: composeInputs,
  pairs: composeOut,
});

// ---------------------------------------------------------------------------
// tC4's mapReference(), run from its own TypeScript source.
// ---------------------------------------------------------------------------

const tc4Commit = (() => {
  try {
    const head = fs.readFileSync(path.join(TC4_DIR, '.git/HEAD'), 'utf8').trim();
    if (!head.startsWith('ref: ')) return head;
    return fs.readFileSync(path.join(TC4_DIR, '.git', head.slice(5)), 'utf8').trim();
  } catch {
    return 'unknown';
  }
})();
const tc4Copy = path.join(ORACLE_DIR, 'tc4-src');
fs.mkdirSync(tc4Copy, { recursive: true });
fs.writeFileSync(path.join(tc4Copy, 'package.json'), '{"type":"module"}\n');
fs.copyFileSync(path.join(TC4_DIR, 'src/data/versification.ts'), path.join(tc4Copy, 'versification.ts'));
fs.writeFileSync(
  path.join(tc4Copy, 'mapReference.ts'),
  fs
    .readFileSync(path.join(TC4_DIR, 'src/data/mapReference.ts'), 'utf8')
    .replace("from './versification';", "from './versification.ts';"),
);
const { mapReference } = await import(pathToFileURL(path.join(tc4Copy, 'mapReference.ts')).href);

const outcomeText = (o) => {
  if (o.ok) return `${o.reference.book} ${o.reference.chapter}:${o.reference.verse}${o.mapped ? '' : ' unmapped'}`;
  const cands = (o.candidates ?? []).map((c) => `${c.book} ${c.chapter}:${c.verse}`).join(',');
  return `!${o.reason}${cands ? ` ${cands}` : ''}`;
};

const TC4_PAIRS = [];
for (const other of ['lxx', 'org', 'rsc', 'rso', 'vul', 'syn']) TC4_PAIRS.push(['eng', other], [other, 'eng']);
const WIDTHS = [1, 2, 3];
const tc4Out = {};
for (const [from, to] of TC4_PAIRS) {
  const out = { width1: [], width2: [], width3: [], chapter: [] };
  const books = Object.keys(docs[from].maxVerses).filter((b) => b in docs[to].maxVerses);
  for (const width of WIDTHS) {
    const enc = runEncoder();
    for (const book of books) {
      const chapters = docs[from].maxVerses[book];
      for (let i = 0; i < chapters.length; i++) {
        const c = i + 1;
        const last = Number(chapters[i]);
        for (let v = 1; v + width - 1 <= last; v++) {
          const verse = width === 1 ? v : `${v}-${v + width - 1}`;
          const o = await mapReference({ from, to, book, chapter: c, verse, schemes: docs });
          if (o.ok && o.mapped && o.reference.book === book && o.reference.chapter === c && `${o.reference.verse}` === `${verse}`) {
            enc.skip();
            continue;
          }
          let single = null;
          if (o.ok && o.mapped) {
            const m = /^(\d+)(?:-(\d+))?$/.exec(`${o.reference.verse}`);
            const lo = Number(m[1]);
            const hi = m[2] === undefined ? lo : Number(m[2]);
            if (hi - lo === width - 1) single = [o.reference.book, o.reference.chapter, lo];
          }
          enc.push(book, c, v, single, outcomeText(o));
        }
      }
    }
    out[`width${width}`] = enc.done();
  }
  for (const book of from === 'eng' ? books : []) {
    const chapters = docs[from].maxVerses[book];
    for (let i = 0; i < chapters.length; i++) {
      const last = Number(chapters[i]);
      if (last < 4) continue;
      const o = await mapReference({ from, to, book, chapter: i + 1, verse: `1-${last}`, schemes: docs });
      const same = o.ok && o.mapped && o.reference.book === book && o.reference.chapter === i + 1 && `${o.reference.verse}` === `1-${last}`;
      if (!same) out.chapter.push(`${book} ${i + 1}:1-${last}=${outcomeText(o)}`);
    }
  }
  tc4Out[`${from}>${to}`] = out;
}
writeJson('oracle/tc4-map-reference.json', {
  tc4Commit,
  proskomma: PROSKOMMA_VERSION,
  note:
    "tC4 src/data/mapReference.ts mapReference() over the tC4 scheme fixtures, for every book both schemes " +
    'list. widthN: every span of N verses inside each source chapter (width1 = single verses), keyed by ' +
    "its first verse: 'B c:v=B c:verse' (mapped), 'B c:v=!reason[ candidates]' (refused), or the run " +
    "form 'B c:v1..v2=B c:w', meaning the span starting at v maps to the span of the same width " +
    "starting at w + v - v1. chapter (from eng only): the whole-chapter span 1-last for chapters of 4 or more verses. " +
    "Outcomes that map to the same reference are left out.",
  pairs: tc4Out,
});

// ---------------------------------------------------------------------------
// tC4's mapReference() on the reference forms a resource row can carry, each part read
// as tC4's derive.ts reads it (refPart: plain digits become a number, anything else
// stays text). Covers verse 0, one-verse spans, reversed and oversized spans, and the
// forms tC4 passes through within one frame and refuses across frames.
// ---------------------------------------------------------------------------

const refPart = (s) => (/^\d+$/.test(s) ? Number(s) : s);
const FORM_PAIRS = [
  ['eng', 'eng'],
  ['eng', 'org'],
  ['eng', 'rsc'],
  ['eng', 'lxx'],
  ['rsc', 'eng'],
  ['lxx', 'eng'],
];
const FORM_INPUTS = [
  'PSA 51:0', 'PSA 51:00', 'PSA 51:0-2', 'PSA 51:1', 'PSA 51:1-2',
  'JON 1:5-5', 'JON 1:17', 'JON 1:17-17', 'JON 1:18', 'JON 1:18-18', 'JON 1:3-2',
  'JON 1:05', 'JON 1:017', 'JON 1:016-017', 'JON 1:1-1000', 'JON 1:1-1001',
  'JON 01:1', 'JON 0:1', 'JON 1:', 'JON 1: 3', 'JON 1:+3',
  'PSA 116:10-10', 'ACT 19:41-41', 'ACT 19:42', 'ACT 19:42-42',
  'PSA 3:front', 'PSA 5:1,3', 'JON 1:1a', 'JON front:intro', 'JON 1-2:1',
];
const formsOut = {};
for (const [from, to] of FORM_PAIRS) {
  const lines = [];
  for (const input of FORM_INPUTS) {
    const space = input.indexOf(' ');
    const [chapter, verse] = input.slice(space + 1).split(':').map(refPart);
    const o = await mapReference({ from, to, book: input.slice(0, space), chapter, verse, schemes: docs });
    lines.push(`${input}=${outcomeText(o)}`);
  }
  formsOut[`${from}>${to}`] = lines;
}
writeJson('oracle/tc4-ref-forms.json', {
  tc4Commit,
  proskomma: PROSKOMMA_VERSION,
  note:
    "tC4 src/data/mapReference.ts mapReference() on hand-picked resource-row references, chapter and verse " +
    "each read by derive.ts refPart (plain digits become a number). Every input is listed: 'INPUT=B c:v' " +
    "(mapped), 'INPUT=B c:v unmapped' (same frame, passed through), or 'INPUT=!reason[ candidates]' (refused).",
  pairs: formsOut,
});
