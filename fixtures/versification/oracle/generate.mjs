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

const PAIR_SCHEMES = [...JSON_SCHEMES, 'syn'];
const composeOut = {};
for (const from of PAIR_SCHEMES) {
  for (const to of PAIR_SCHEMES) {
    if (from === to) continue;
    if ((from === 'syn' || to === 'syn') && ![from, to].includes('eng') && ![from, to].includes('org')) continue;
    const F = tables[`json:${from}`].forward;
    const R = tables[`json:${to}`].reverse;
    const enc = runEncoder();
    for (const [book, chapters] of Object.entries(docs[from].maxVerses)) {
      chapters.forEach((last, i) => {
        const c = i + 1;
        for (let v = 0; v <= Number(last) + 1; v++) {
          const [pb, pcvs] = apply(F, book, c, v);
          const hops = pcvs.map(([oc, ov]) => apply(R, pb, oc, ov));
          if (hops.length === 1 && isIdentity(hops[0], book, c, v)) enc.skip();
          else enc.push(book, c, v, singleOf(hops), hops.map(hopText).join(';'));
        }
      });
    }
    composeOut[`${from}>${to}`] = enc.done();
  }
}
writeJson('oracle/compose.json', {
  proskomma: PROSKOMMA_VERSION,
  note:
    "Two-hop mapping from one tC4 scheme to another through org, as Proskomma's do_cv does it. " +
    "Domain: every verse 0..=last+1 of every chapter in the source scheme's maxVerses. Results other " +
    "than the input itself, as 'B c:v=hop[;hop...]' (one reverse hop per pivot verse) or the run " +
    "form 'B c:v1..v2=B c:w'.",
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
