// Task 5: turn the bundled ULT Psalms USFM into plain text (one paragraph per verse, with its
// number; a heading per psalm; no USFM markers or alignment data) as HTML for Word and Edge,
// plus a JSON of every verse for checking against the USFM.
// usage: node usfm-to-psalms.mjs <PSA.usfm> <out-dir>
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const [src, outDir] = process.argv.slice(2);
let s = readFileSync(src, 'utf8').replace(/\r\n?/g, '\n');

// Marker inventory, before anything is removed.
const inventory = {};
for (const m of s.matchAll(/\\\+?([a-z]+[0-9]*(?:-[se])?)\*?/g)) inventory[m[1]] = (inventory[m[1]] ?? 0) + 1;

// Alignment data and word attributes.
s = s.replace(/\\zaln-s\s[^]*?\\\*/g, '');
s = s.replace(/\\zaln-e\\\*/g, '');
s = s.replace(/\\\+?w\s+([^|\\]*?)(?:\|[^\\]*)?\\\+?w\*/g, '$1');
// Footnotes and cross references are notes, not verse text.
s = s.replace(/\\f\s[^]*?\\f\*/g, '');
s = s.replace(/\\x\s[^]*?\\x\*/g, '');
// Milestones such as \ts\* and \k-s ...\* / \k-e\*.
s = s.replace(/\\[a-z]+[0-9]*-s\s[^]*?\\\*/g, '');
s = s.replace(/\\[a-z]+[0-9]*(?:-e)?\\\*/g, '');
// Closing character markers such as \qs* (Selah).
s = s.replace(/\\\+?[a-z]+[0-9]*\*/g, '');

// Header lines end at the first chapter.
const body = s.slice(s.indexOf('\\c '));

const chapters = [];
const verses = [];
let ch = null;
let cur = null; // the verse being read; it joins ch.items when the next item starts
let open = null; // a heading being read, whose text may run on over poetry markers
const leftovers = {};

const flush = () => {
  if (open) { open.text = clean(open.text); if (open.text) ch.items.push(open); open = null; }
  if (cur) { cur.text = clean(cur.text); verses.push(cur); ch.items.push({ marker: 'v', verse: cur }); cur = null; }
};
function clean(t) {
  return t.replace(/\s+/g, ' ').replace(/\s+([,.;:!?’”)\]])/g, '$1').replace(/([“‘(\[])\s+/g, '$1').trim();
}

// Split into marker-led pieces. Every heading keeps its place among the verses: a \qa before
// its stanza, and a \d before the verse it was written in (USFM puts most superscriptions
// after "\v 1", as in "\v 1 \d A psalm of Asaph \q1 Surely God ...").
for (const piece of body.split(/(?=\\[a-z]+[0-9]*\s)/)) {
  const m = piece.match(/^\\([a-z]+[0-9]*)\s+([^]*)$/);
  if (!m) { if (piece.trim()) leftovers['(no marker)'] = (leftovers['(no marker)'] ?? 0) + 1; continue; }
  const [, marker, rest] = m;
  if (marker === 'c') {
    flush();
    ch = { n: parseInt(rest, 10), ms: [], items: [] };
    chapters.push(ch);
  } else if (marker === 'v') {
    flush();
    const vm = rest.match(/^(\d+(?:-\d+)?)\s*([^]*)$/);
    cur = { c: ch.n, v: vm[1], text: vm[2] };
  } else if (marker.startsWith('ms')) {
    flush();
    ch.ms.push(clean(rest));
  } else if (['s', 's1', 's2', 'd', 'qa', 'sp', 'cl'].includes(marker)) {
    if (open) flush();
    if (cur && marker === 'd') {
      // A superscription inside verse 1: it is shown before that verse.
      ch.items.push({ marker, text: clean(rest) });
      if (!clean(rest)) ch.items.pop();
      continue;
    }
    flush();
    open = { marker, text: rest };
  } else {
    // Paragraph and poetry markers (\q1 \q2 \p \m \b \qs ...) carry on the verse.
    if (!['q', 'q1', 'q2', 'q3', 'p', 'm', 'b', 'qs', 'pi', 'pi1', 'nb', 'qr', 'qc', 'li', 'li1'].includes(marker)) {
      leftovers[marker] = (leftovers[marker] ?? 0) + 1;
    }
    if (cur) cur.text += ' ' + rest;
    else if (open) open.text += ' ' + rest;
    else if (clean(rest)) leftovers['(text outside a verse)'] = (leftovers['(text outside a verse)'] ?? 0) + 1;
  }
}
flush();

const residue = verses.filter(v => /[\\|]|x-occurrence|x-strong/.test(v.text));
const esc = t => t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
let html = '<!doctype html>\n<html lang="en"><head><meta charset="utf-8"><title>Psalms (ULT v91)</title></head><body>\n';
let txt = '';
for (const c of chapters) {
  for (const t of c.ms) { html += `<h1>${esc(t)}</h1>\n`; txt += `${t}\n`; }
  html += `<h2>Psalm ${c.n}</h2>\n`; txt += `Psalm ${c.n}\n`;
  for (const it of c.items) {
    if (it.verse) { html += `<p>${it.verse.v} ${esc(it.verse.text)}</p>\n`; txt += `${it.verse.v} ${it.verse.text}\n`; }
    else { html += `<p><i>${esc(it.text)}</i></p>\n`; txt += `${it.text}\n`; }
  }
}
html += '</body></html>\n';
writeFileSync(join(outDir, 'psalms.html'), html);
writeFileSync(join(outDir, 'psalms.txt'), txt);
writeFileSync(join(outDir, 'verses.json'), JSON.stringify(verses, null, 0));
console.log(JSON.stringify({
  chapters: chapters.length, verses: verses.length, words: verses.reduce((a, v) => a + v.text.split(' ').length, 0),
  txtBytes: Buffer.byteLength(txt), htmlBytes: Buffer.byteLength(html), residue: residue.length,
  emptyVerses: verses.filter(v => !v.text).map(v => `${v.c}:${v.v}`),
  headings: Object.fromEntries(['d', 'qa', 's', 's1', 'sp', 'cl'].map(k => [k, chapters.flatMap(c => c.items).filter(i => i.marker === k).length])),
  bookDivisions: chapters.flatMap(c => c.ms),
  lines: txt.split('\n').length - 1,
  leftovers, inventory,
}, null, 1));
