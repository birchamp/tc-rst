// The renumber fixture (issue #209, BURRITO-SPEC R-8.5.20–R-8.5.22): every verse of one
// chapter moves up one number, as ONE `text.structure.apply`. The dispositions are the
// approved ones: alignments and verse-targeted notes re-key and follow their text;
// decisions keep their key (`invalidate-retain`), because the project frame does not move.
import { decompose, slotKeysOf } from '../../journal/skeleton.mjs';
import { splitDecisionKey } from '../../journal/grammar.mjs';

export const shiftChapter = (foldOut, book, chapter, { actor, ts }) => {
  const ch = String(chapter);
  let inChapter = false;
  const shifted = foldOut.books[book].usfm.replace(/\\([cv]) (\d+)/g, (m, tag, n) => {
    if (tag === 'c') { inChapter = n === ch; return m; }
    return inChapter ? `\\v ${Number(n) + 1}` : m;
  });
  const { skeleton } = decompose(shifted);
  const inCh = (k) => k.split(':')[0] === ch;
  const up = (k) => `${ch}:${Number(k.split(':')[1]) + 1}`;
  const down = (k) => `${ch}:${Number(k.split(':')[1]) - 1}`;
  const verses = foldOut.books[book].verses;
  const transitions = {};
  for (const dest of slotKeysOf(skeleton)) {
    const src = inCh(dest) ? down(dest) : dest;
    transitions[dest] = { text: verses[src], sources: [{ key: src, ts: foldOut.headsTs[`text|${book}|${src}`] }] };
  }
  const dispositions = [];
  const live = foldOut.liveHeads;
  for (const key of Object.keys(live)) {
    if (key.startsWith(`align|${book}|`)) {
      const k = key.slice(`align|${book}|`.length);
      if (inCh(k)) for (const h of live[key]) dispositions.push({ surface: 'alignment', key: k, ts: h.ts, action: 're-key', to: up(k) });
    } else if (key.startsWith('dec|')) {
      const { bookId, chapter: c } = splitDecisionKey(key.slice(4));
      if (bookId === book.toLowerCase() && String(c) === ch)
        for (const h of live[key]) dispositions.push({ surface: 'decision', key: key.slice(4), ts: h.ts, action: 'invalidate-retain' });
    }
  }
  for (const n of foldOut.liveNotes) {
    const tg = n.target;
    if (tg.book === book && String(tg.chapter) === ch)
      dispositions.push({ surface: 'note', ts: n.ts, action: 're-key', to: up(`${tg.chapter}:${tg.verse}`) });
  }
  return { v: 1, op: 'text.structure.apply', actor, ts, base: foldOut.headsTs[`skel|${book}`], book, skeleton, transitions, dispositions };
};
