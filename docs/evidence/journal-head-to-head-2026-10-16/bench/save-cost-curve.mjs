// Per-save fold cost vs project history, using tC4's REAL fold module.
// A save in journalingStore.publishAndRegenerate nulls foldCache then awaits
// ensureFold() -> fold(this.events). So fold(N) IS the per-save cost at history N.
// Corpus construction replicates conformance/bench-fold.mjs (same fixtures, same
// event shapes); each point is a prefix at a BOOK boundary, so every corpus is valid.
import fs from 'fs';
import path from 'path';
const TC4 = '/Users/agenthermes/Development/translationCore4';
const CONF = path.join(TC4, 'conformance');
const { fold, verseTextMd5 } = await import(path.join(TC4, 'journal/fold.mjs'));
const { decompose, SLOT } = await import(path.join(TC4, 'journal/skeleton.mjs'));
const { makeClock } = await import(path.join(TC4, 'journal/hlc.mjs'));

const ACTOR = 'bench-a';
const OT = ['GEN','EXO','LEV','NUM','DEU','JOS','JDG','RUT','1SA','2SA','1KI','2KI','1CH','2CH','EZR','NEH','EST','JOB','PSA','PRO','ECC','SNG','ISA','JER','LAM','EZK','DAN','HOS','JOL','AMO','OBA','JON','MIC','NAM','HAB','ZEP','HAG','ZEC','MAL'];
const NT = ['MAT','MRK','LUK','JHN','ACT','ROM','1CO','2CO','GAL','EPH','PHP','COL','1TH','2TH','1TI','2TI','TIT','PHM','HEB','JAS','1PE','2PE','1JN','2JN','3JN','JUD','REV'];
const ALL = [...OT, ...NT];

const vrs = JSON.parse(fs.readFileSync(path.join(CONF,'fixtures/vrs/eng.json'),'utf8'));
const density = JSON.parse(fs.readFileSync(path.join(CONF,'fixtures/check-density.json'),'utf8'));
const titUsfm = fs.readFileSync(path.join(CONF,'sample-burrito/ingredients/TIT.usfm'),'utf8');
const titAlign = JSON.parse(fs.readFileSync(path.join(CONF,'sample-burrito/ingredients/checking/alignments/TIT.json'),'utf8'));
const titVerses = decompose(titUsfm).verses;
const material = [];
for (const [ch, byVerse] of Object.entries(titAlign.chapters))
  for (const [v, record] of Object.entries(byVerse)) {
    const text = titVerses[`${ch}:${v}`];
    if (typeof text === 'string' && text.trim() !== '' && !text.includes('___')) material.push({ text, record });
  }

const clock = makeClock(ACTOR);
const books = [];
let mi = 0;
for (const book of ALL) {
  const chapters = vrs.maxVerses[book];
  let skeleton = `\\id ${book} bench corpus\n\\h ${book}\n\\mt ${book}\n`;
  const slots = [];
  chapters.forEach((maxV, ci) => {
    skeleton += `\\c ${ci + 1}\n\\p\n`;
    for (let v = 1; v <= Number(maxV); v++) { const k = `${ci+1}:${v}`; slots.push(k); skeleton += `\\v ${v} ${SLOT}${k}${SLOT}`; }
  });
  const events = [];
  const bookAddTs = clock.issue();
  events.push({ v:1, op:'book.add', actor:ACTOR, ts:bookAddTs, base:null, book, scope:[], skeleton });
  const alignEvents = [];
  for (const key of slots) {
    const { text, record } = material[mi++ % material.length];
    const chapter = key.slice(0, key.indexOf(':')), verse = key.slice(key.indexOf(':')+1);
    const ts = clock.issue();
    events.push({ v:1, op:'text.verse.set', actor:ACTOR, ts, base:bookAddTs, book, chapter, verse, text });
    alignEvents.push({ v:1, op:'align.verse.set', actor:ACTOR, ts:clock.issue(), base:null, generation:bookAddTs,
      book, chapter, verse, alignments:record.alignments, wordBank:record.wordBank,
      sourceVersion:record.sourceVersion, targetVerseMd5:verseTextMd5(text) });
  }
  events.push(...alignEvents);
  for (const toolId of ['translationNotes','translationWords']) {
    const n = density[toolId][book] ?? 0;
    const prefix = toolId === 'translationNotes' ? 'tn' : 'tw';
    for (let i = 0; i < n; i++) {
      const key = slots[i % slots.length];
      const chapter = key.slice(0, key.indexOf(':')), verse = key.slice(key.indexOf(':')+1);
      events.push({ v:1, op:'check.decision.set', actor:ACTOR, ts:clock.issue(), base:null, generation:bookAddTs,
        toolId, decision:{ contextId:{ checkId:`${prefix}${i}`, occurrence:1,
          reference:{ bookId:book.toLowerCase(), chapter, verse } },
          selections:[{ text:'palabra', occurrence:1, occurrences:1 }], nothingToSelect:false } });
    }
  }
  books.push({ book, events });
}

const REPS = 3;
const ms = (t0) => Number(process.hrtime.bigint() - t0) / 1e6;
const median = (a) => { const s=[...a].sort((x,y)=>x-y); return s.length%2 ? s[(s.length-1)/2] : (s[s.length/2-1]+s[s.length/2])/2; };

console.log('events\tbooks\tfold_median_ms\tus_per_event\truns\tclean');
for (const nb of [1,2,4,8,16,24,32,40,48,56,62,66]) {
  const evs = books.slice(0, nb).flatMap((b) => b.events);
  const runs = [];
  let out;
  for (let i = 0; i < REPS; i++) { const t0 = process.hrtime.bigint(); out = fold(evs); runs.push(ms(t0)); }
  const m = median(runs);
  const clean = (out.forks?.length ?? 0) === 0 ? 'yes' : `forks=${out.forks.length}`;
  console.log(`${evs.length}\t${nb}\t${m.toFixed(1)}\t${(m*1000/evs.length).toFixed(2)}\t${runs.map(r=>r.toFixed(0)).join(',')}\t${clean}`);
}
