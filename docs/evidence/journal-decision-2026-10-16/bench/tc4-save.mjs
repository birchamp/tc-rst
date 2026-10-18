#!/usr/bin/env node
// bench-fold.mjs — issue #80: measure the size and the speed cost of the journal.
//
// The benchmark folds a synthetic aligned New Testament journal through the REAL
// reference fold (journal/fold.mjs — the same module the production client imports,
// issue #62). It reports:
//   1. the full fold time for the aligned-NT journal;
//   2. the three known cost centres (issue #80):
//      C1 — one usfm-js parse per alignment head at projection (verseTextMd5);
//      C2 — a scan of all heads for each mapped key inside text.structure.apply;
//      C3 — a scan of all heads for each book at projection;
//   3. the open() full-scan baseline deferred from #62: one sealed action segment
//      per event, read + validated + folded, as a function of journal length.
//
// The corpus is built from REAL material, never from invented shapes:
//   - verse counts: fixtures/vrs/eng.json (the org versification), NT books only;
//   - verse text: the Spanish TIT verse texts of sample-burrito (cycled);
//   - alignment records: the complete §5.1 records of sample-burrito TIT (cycled),
//     with targetVerseMd5 recomputed against the cycled text so every alignment is
//     VALID at projection — the fold pays the same parse either way, and a clean
//     corpus proves the generator against the fold (no forks, no retained heads).
//
// Usage:  node bench-fold.mjs [--quick | --bible]
//   --quick: 1 repetition, 5 books, small open() curve — a smoke run, not a record.
//   --bible: the CHECKED WHOLE BIBLE — all 66 books, aligned, PLUS one
//            check.decision.set for every real translationNotes and
//            translationWords check (fixtures/check-density.json: per-book row
//            counts from en_tn/en_twl v89). One event = one save = one segment.
// Timings are medians over repetitions (3 by default). Record results in
// docs/evidence/ with the machine, the commit and the date (issue #80).

import fs from 'fs';
import os from 'os';
import path from 'path';
import { execSync } from 'child_process';
import { fileURLToPath } from 'url';
import { fold, verseTextMd5, slotKeysOf } from '/home/user/unfoldingword/translationcore4/journal/fold.mjs';
import { decompose, SLOT } from '/home/user/unfoldingword/translationcore4/journal/skeleton.mjs';
import { makeClock } from '/home/user/unfoldingword/translationcore4/journal/hlc.mjs';
import { writeActionSegment, readUnion, actorDirFor } from '/home/user/unfoldingword/translationcore4/journal/files.mjs';

const HERE = '/home/user/unfoldingword/translationcore4/conformance';
const QUICK = process.argv.includes('--quick');
const BIBLE = process.argv.includes('--bible');
const REPS = QUICK ? 1 : 3;
const ACTOR = 'bench-a';

// Canonical book order (§2 book ids).
const OT_ALL = [
  'GEN', 'EXO', 'LEV', 'NUM', 'DEU', 'JOS', 'JDG', 'RUT', '1SA', '2SA', '1KI', '2KI',
  '1CH', '2CH', 'EZR', 'NEH', 'EST', 'JOB', 'PSA', 'PRO', 'ECC', 'SNG', 'ISA', 'JER',
  'LAM', 'EZK', 'DAN', 'HOS', 'JOL', 'AMO', 'OBA', 'JON', 'MIC', 'NAM', 'HAB', 'ZEP',
  'HAG', 'ZEC', 'MAL',
];
const NT_ALL = [
  'MAT', 'MRK', 'LUK', 'JHN', 'ACT', 'ROM', '1CO', '2CO', 'GAL', 'EPH', 'PHP', 'COL',
  '1TH', '2TH', '1TI', '2TI', 'TIT', 'PHM', 'HEB', 'JAS', '1PE', '2PE', '1JN', '2JN',
  '3JN', 'JUD', 'REV',
];
const NT = BIBLE ? [...OT_ALL, ...NT_ALL] : QUICK ? NT_ALL.slice(0, 5) : NT_ALL;
const CORPUS_LABEL = BIBLE ? 'checked whole Bible' : 'aligned NT';
// Real per-book check counts (en_tn/en_twl v89) — the --bible decision densities.
const density = BIBLE
  ? JSON.parse(fs.readFileSync(path.join(HERE, 'fixtures/check-density.json'), 'utf8'))
  : null;

// ---------- real material ----------
const vrs = JSON.parse(fs.readFileSync(path.join(HERE, 'fixtures/vrs/eng.json'), 'utf8'));
const titUsfm = fs.readFileSync(path.join(HERE, 'sample-burrito/ingredients/TIT.usfm'), 'utf8');
const titAlign = JSON.parse(
  fs.readFileSync(path.join(HERE, 'sample-burrito/ingredients/checking/alignments/TIT.json'), 'utf8'),
);
const titVerses = decompose(titUsfm).verses;
// (text, §5.1 record) pairs — only verses that carry BOTH real text and a real record
const material = [];
for (const [ch, byVerse] of Object.entries(titAlign.chapters)) {
  for (const [v, record] of Object.entries(byVerse)) {
    const text = titVerses[`${ch}:${v}`];
    if (typeof text === 'string' && text.trim() !== '' && !text.includes('___')) {
      material.push({ text, record });
    }
  }
}
if (material.length === 0) throw new Error('no usable TIT material — corpus cannot be built');

// ---------- corpus: one aligned-NT journal ----------
// Per book: one book.add (skeleton from eng.vrs), one text.verse.set per verse (a
// drafting write on the book.add base), one align.verse.set per verse (rootless first
// write with the mandatory generation stamp) — the D50 model: each event is one save.
const clock = makeClock(ACTOR);
const books = []; // {book, bookAddTs, skeleton, slots, textTs: Map, texts: Map, events: []}
let materialIdx = 0;
for (const book of NT) {
  const chapters = vrs.maxVerses[book];
  if (!chapters) throw new Error(`eng.vrs has no maxVerses for ${book}`);
  let skeleton = `\\id ${book} bench corpus\n\\h ${book}\n\\mt ${book}\n`;
  const slots = [];
  chapters.forEach((maxV, ci) => {
    skeleton += `\\c ${ci + 1}\n\\p\n`;
    for (let v = 1; v <= Number(maxV); v++) {
      const key = `${ci + 1}:${v}`;
      slots.push(key);
      skeleton += `\\v ${v} ${SLOT}${key}${SLOT}`;
    }
  });
  const events = [];
  const bookAddTs = clock.issue();
  events.push({ v: 1, op: 'book.add', actor: ACTOR, ts: bookAddTs, base: null, book, scope: [], skeleton });
  const textTs = new Map();
  const texts = new Map();
  const alignEvents = [];
  for (const key of slots) {
    const { text, record } = material[materialIdx % material.length];
    materialIdx += 1;
    const [chapter, verse] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)];
    const ts = clock.issue();
    textTs.set(key, ts);
    texts.set(key, text);
    events.push({ v: 1, op: 'text.verse.set', actor: ACTOR, ts, base: bookAddTs, book, chapter, verse, text });
    alignEvents.push({
      v: 1, op: 'align.verse.set', actor: ACTOR, ts: clock.issue(), base: null,
      generation: bookAddTs, book, chapter, verse,
      alignments: record.alignments, wordBank: record.wordBank,
      sourceVersion: record.sourceVersion, targetVerseMd5: verseTextMd5(text),
    });
  }
  events.push(...alignEvents);
  // --bible: one check.decision.set per REAL check (en_tn/en_twl v89 row counts),
  // spread round-robin over the book's verses. checkId is unique per (tool, book),
  // so every decision is its own §5.2 register key — one save each, like the client.
  if (density) {
    for (const toolId of ['translationNotes', 'translationWords']) {
      const n = density[toolId][book] ?? 0;
      const prefix = toolId === 'translationNotes' ? 'tn' : 'tw';
      for (let i = 0; i < n; i++) {
        const key = slots[i % slots.length];
        const [chapter, verse] = [key.slice(0, key.indexOf(':')), key.slice(key.indexOf(':') + 1)];
        events.push({
          v: 1, op: 'check.decision.set', actor: ACTOR, ts: clock.issue(), base: null,
          generation: bookAddTs, toolId,
          decision: {
            contextId: { checkId: `${prefix}${i}`, occurrence: 1,
              reference: { bookId: book.toLowerCase(), chapter, verse } },
            selections: [{ text: 'palabra', occurrence: 1, occurrences: 1 }],
            nothingToSelect: false,
          },
        });
      }
    }
  }
  books.push({ book, bookAddTs, skeleton, slots, textTs, texts, events });
}

// Three event sets, used by DIFFERENT controls (PR #96 review, finding 2):
//   allAligned    — the whole corpus (the measured fold);
//   allMinusAlign — the corpus WITHOUT align.verse.set only (the C1 control:
//                   decisions stay in, so the difference attributes to alignment
//                   projection alone);
//   allNoAlign    — text + structure only (the C2/C3 base: a small union whose
//                   head-key count is stated with the result).
const CONTENT_OPS = new Set(['align.verse.set', 'check.decision.set']);
const eventsOf = (bs, { aligned }) =>
  bs.flatMap((b) => (aligned ? b.events : b.events.filter((e) => !CONTENT_OPS.has(e.op))));
const allAligned = eventsOf(books, { aligned: true });
const allMinusAlign = books.flatMap((b) => b.events.filter((e) => e.op !== 'align.verse.set'));
const allNoAlign = eventsOf(books, { aligned: false });
const verseCount = books.reduce((n, b) => n + b.slots.length, 0);
const decisionCount = allAligned.filter((e) => e.op === 'check.decision.set').length;

// ---------- a text.structure.apply with M mapped keys (cost centre C2) ----------
// CONSTANT-SHAPE variants (PR #96 review, finding 3): every variant RENAMES the
// first M slots of the book (verse v → v+500 in its chapter) and carries every
// other slot as an identity transition. M=0 is the pure identity apply. All
// variants therefore have the SAME slot count, the SAME transition count and the
// SAME texts — only the mapped-key set (renamed sources ≠ their destinations)
// differs, which is exactly the per-mapped-key scan C2 measures. On the
// text-only union no alignment or decision head exists, so the affected set is
// empty and dispositions stay [].
const buildRenameApply = (b, m) => {
  const renamed = new Map(); // old slot key -> new slot key
  for (let i = 0; i < m && i < b.slots.length; i++) {
    const k = b.slots[i];
    const c = k.slice(0, k.indexOf(':'));
    const v = k.slice(k.indexOf(':') + 1);
    renamed.set(k, `${c}:${Number(v) + 500}`);
  }
  let skeleton = b.skeleton;
  for (const [oldK, newK] of renamed) skeleton = skeleton.replace(`${SLOT}${oldK}${SLOT}`, `${SLOT}${newK}${SLOT}`);
  const transitions = {};
  for (const k of b.slots) {
    const dest = renamed.get(k) ?? k;
    transitions[dest] = { text: b.texts.get(k), sources: [{ key: k, ts: b.textTs.get(k) }] };
  }
  return {
    event: { v: 1, op: 'text.structure.apply', actor: ACTOR, ts: clock.issue(), base: b.bookAddTs,
      book: b.book, skeleton, transitions, dispositions: [] },
    mapped: renamed.size,
  };
};

// ---------- timing ----------
const ms = (t0) => Number(process.hrtime.bigint() - t0) / 1e6;
const bench = (fn) => {
  const runs = [];
  let out;
  for (let i = 0; i < REPS; i++) {
    const t0 = process.hrtime.bigint();
    out = fn();
    runs.push(ms(t0));
  }
  runs.sort((a, b) => a - b);
  return { median: runs[Math.floor(runs.length / 2)], runs, out };
};
const f1 = (x) => x.toFixed(1);

// ---------- header ----------
// The stamp must identify the CODE that ran, not just the checked-out commit
// (PR #96 review, finding 4): a dirty tree gets a visible marker, and a record
// made from a dirty run is not a valid evidence baseline.
const rev = (() => {
  try {
    const head = execSync('git rev-parse --short HEAD', { cwd: HERE }).toString().trim();
    const dirty = execSync('git status --porcelain', { cwd: HERE }).toString().trim();
    return dirty ? `${head}-DIRTY (not a valid evidence baseline)` : head;
  } catch { return 'unknown'; }
})();
console.log('bench-fold — issue #80');
console.log(`date: ${new Date().toISOString()}  commit: ${rev}  node: ${process.version}`);
console.log(`machine: ${os.cpus()[0].model} (${os.cpus().length} cores), ${Math.round(os.totalmem() / 2 ** 30)} GB, ${os.platform()} ${os.release()}`);
console.log(`mode: ${QUICK ? 'QUICK (smoke run — not a record)' : `full (${REPS} repetitions, median)`}${BIBLE ? ' — checked whole Bible' : ''}`);
console.log(`corpus: ${NT.length} books, ${verseCount} verses, ${decisionCount} check decisions, ${allAligned.length} events total, ${allNoAlign.length} events (text-only)`);
console.log('');

// ---------- sanity: the corpus folds CLEAN ----------
{
  const r = fold(allAligned);
  const dirty = ['forks', 'invalid', 'retained', 'pendingStructural', 'vrsRejected', 'supersedeRefused']
    .filter((k) => r[k].length > 0);
  if (dirty.length) throw new Error(`corpus does not fold clean: ${dirty.join(', ')} non-empty`);
  if (Object.keys(r.books).length !== NT.length) throw new Error('corpus lost a book at projection');
}

// ---------- h2h: write one sealed segment per event, as bench section 5 does ----------
const OUT = process.argv[process.argv.length - 1];
const journalDir = path.join(OUT, 'journal');
const actorDir = actorDirFor(journalDir, ACTOR);
const byTs = [...allAligned].sort((a, b) => (a.ts < b.ts ? -1 : 1));
const t0 = process.hrtime.bigint();
for (const e of byTs) writeActionSegment(actorDir, [e]);
const segDir = path.join(actorDir, 'segments');
let diskBytes = 0; let n = 0;
for (const f of fs.readdirSync(segDir)) { diskBytes += fs.statSync(path.join(segDir, f)).size; n++; }
console.log('wrote ' + n + ' segments, ' + diskBytes + ' bytes in ' + ms(t0).toFixed(0) + ' ms; ops ' + JSON.stringify(byTs.reduce((m, e) => { m[e.op] = (m[e.op] || 0) + 1; return m; }, {})));
