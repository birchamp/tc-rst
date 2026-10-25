// Generates fixtures/usfm/section-draft-vectors.json: what tC4's section drafting rules
// (`src/views/sectionDraft.js`, #141, #63, #54) answer for hand-written and seeded random
// section texts, so `tc_api::model::section` is held to them (APP-DESIGN §3.8, unit B4b).
//
// For each text and section keys: the words, separators, pins' word indices and blocks that
// `parseDraft` reads; the groups, keys, verse texts and formats the save would write; the Type
// text `serializeDraft` writes back; whether `canDrop` allows each pin at each word; and the
// markers `dropPin` gives for every drop it allows. Also `initialDraftText` for lists of
// verses, and `indentLine` for texts, carets and directions.
//
// The script imports tC4's own file, which has no dependency. Needs Node 22 or later. Run from
// the repository root with a tC4 checkout:
//   node crates/tc-api/tools/gen-section-vectors.mjs <tC4 checkout> > fixtures/usfm/section-draft-vectors.json
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const tc4 = process.argv[2];
if (!tc4) throw new Error('usage: gen-section-vectors.mjs <tC4 checkout>');
const {
  canDrop, dropPin, expandKeys, indentLine, initialDraftText, parseDraft, sectionGroups,
  sectionKeys, sectionVerses, serializeDraft, spanEnd,
} = await import(pathToFileURL(path.join(tc4, 'src/views/sectionDraft.js')).href);

// A seeded generator (mulberry32), so the file is the same on every run.
let state = 0x5ec7109;
const random = () => {
  state = (state + 0x6d2b79f5) | 0;
  let t = Math.imul(state ^ (state >>> 15), 1 | state);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (list) => list[Math.floor(random() * list.length)];
const between = (lo, hi) => lo + Math.floor(random() * (hi - lo + 1));

const KEY_SETS = [
  ['9', '10'], ['3', '4', '5'], ['9-10', '11'], ['1'], ['1', '2', '3', '4', '5', '6'],
  ['11', '12', '13'], ['4-5', '6', '7-9'], ['10', '11', '12', '13'],
];
const WORDS = ['a', 'bb', 'ccc', 'Exhorta', 'los', 'siervos', 'no', 'defraudando', '12', '7', 'talentos',
  '\\q2', '\\add', 'añadido', 'x-y', '9x', '9-', '-3', '10.', '(11)'];
const SPACES = [' ', ' ', ' ', '  ', ' ', '\t', ' \t '];

const randomWords = (n) => Array.from({ length: n }, () => pick(WORDS)).join(pick(SPACES));

/** A section text: marker lines of the section's pins and runs, stubs, plain continuation
 * lines, blank lines, tabs and escaped lines. */
const randomText = (keys) => {
  const pins = expandKeys(keys);
  const lines = [];
  const count = between(0, 7);
  for (let i = 0; i < count; i++) {
    const lead = pick(['', '', '', '\t', '\t\t', ' ']);
    const kind = random();
    if (kind < 0.45) {
      const pin = pick(pins);
      lines.push(`${lead}${pin} ${randomWords(between(0, 5))}`);
    } else if (kind < 0.55 && pins.length > 1) {
      const a = between(0, pins.length - 2);
      const b = between(a + 1, pins.length - 1);
      lines.push(`${lead}${pins[a]}-${pins[b]} ${randomWords(between(1, 4))}`);
    } else if (kind < 0.65) {
      lines.push(`${lead}${pick(pins)}`);
    } else if (kind < 0.75) {
      lines.push('');
    } else if (kind < 0.85) {
      lines.push(`${pick(['', ' ', '  '])}${pick(pins)} ${randomWords(between(1, 3))}`);
    } else {
      lines.push(`${lead}${randomWords(between(1, 5))}`);
    }
  }
  return lines.join(pick(['\n', '\n', '\n', '\r\n']));
};

const record = (text, keys) => {
  const pins = expandKeys(keys);
  const d = parseDraft(text, pins);
  const out = { keys, text, words: d.words, seps: d.seps, markers: d.markers, blocks: d.blocks };
  if (d.words.length === 0) return out;
  out.groups = sectionGroups(d.markers, pins, keys);
  out.newKeys = sectionKeys(d.markers, pins, keys);
  out.verses = sectionVerses(d.words, d.seps, d.markers, pins, keys, d.blocks);
  out.serialized = serializeDraft(d.words, d.seps, d.markers, pins, keys, d.blocks);
  // The words each pin may begin at (0 to one past the last word), and up to four of the drops
  // `canDrop` allows with the markers `dropPin` gives.
  out.canDrop = {};
  const allowed = [];
  for (const pin of pins) {
    out.canDrop[pin] = [];
    for (let index = 0; index <= d.words.length + 1; index++) {
      if (canDrop(d.markers, pins, pin, index)) {
        out.canDrop[pin].push(index);
        allowed.push([pin, index]);
      }
    }
  }
  out.drops = [];
  for (let i = 0; i < 4 && allowed.length; i++) {
    const [pin, index] = pick(allowed);
    out.drops.push({ pin, index, markers: dropPin(d.markers, pins, pin, index) });
  }
  out.spanEnds = [];
  for (let i = 0; i < 6; i++) {
    const index = between(0, d.words.length - 1);
    const held = pick([null, ...pins]);
    out.spanEnds.push([index, held, spanEnd(d.markers, index, held, d.words.length)]);
  }
  return out;
};

const parse = [];
// tC4's own test texts.
const hand = [
  ['9 Exhorta\n12 talentos', ['9', '10']], ['antes\n9 Exhorta\n10 no', ['9', '10']],
  ['10 aaa\n9 bbb', ['9', '10']], ['', ['9', '10']], ['  \n ', ['9', '10']],
  ['9 \n10 no defraudando', ['9', '10']], ['9-10 Exhorta a los siervos', ['9-10']],
  ['9 línea uno\nlínea dos\n10 no defraudando', ['9', '10']],
  ['3 a b\n4 c d\n5 e', ['3', '4', '5']], ['a b c d e', ['3', '4', '5']],
  ['9 y le dio\n 10 talentos\n10 no defraudando', ['9', '10']],
  ['9 y le dio\n  10 talentos\n10 no defraudando', ['9', '10']],
  ['9 y le dio\n12 talentos\n10 no defraudando', ['9', '10']],
  ['9-10 a b c d', ['9', '10']], ['3-4 a b\n5 c d', ['3', '4', '5']], ['9-12 a', ['9', '10']],
  ['9-10 \n11 c d', ['9-10', '11']], ['\t10 no defraudando', ['9', '10']],
  ['\t9-10 no defraudando', ['9-10']], ['9 a\n\n10 b', ['9', '10']], ['9 todo\nno defraudando', ['9', '10']],
  ['9 a\n10 b', ['9', '10']], ['9 a\n\t10 b', ['9', '10']], ['9 a\n\t\t10 b', ['9', '10']],
  ['9 a\n\n\t10 b', ['9', '10']], ['9 a b\n\t10 c d', ['9', '10']], ['9-10 a\n\t11 b', ['9-10', '11']],
  ['9 a\n\n\n   \n  b\n\n10 c', ['9', '10']], ['3 a\n4 b\n5 c\n\n\n\n6 d', ['3', '4', '5', '6']],
];
for (const [text, keys] of hand) parse.push(record(text, keys));
for (let i = 0; i < 700; i++) {
  const keys = pick(KEY_SETS);
  parse.push(record(randomText(keys), keys));
}

const initial = [];
const body = () => pick([
  'a', 'y le dio\n10 talentos', 'y le dio\n 10 talentos', 'y le dio\n12 talentos', 'no defraudando',
  'uno\ndos\ntres', '\\q2 que no hablen', 'a b  c', 'x\n9 y\n3-4 z', 'p\n  5', 'q\n5-6 r',
]);
for (let i = 0; i < 160; i++) {
  const keys = pick(KEY_SETS);
  const verses = keys.map((n) => {
    const drafted = random() < 0.7;
    return { n, drafted, body: drafted ? body() : '', format: pick([undefined, undefined, 'p', 'q1', 'q2']) };
  });
  initial.push({ keys, verses, text: initialDraftText(verses, expandKeys(keys)) });
}

const indent = [];
// As SectionEditor.jsx calls it: with the section's pins, a line inside a verse takes a third
// tab (#812); without them, every line takes two at most.
for (const pins of [undefined, ['9', '10']]) {
  for (const text of ['10 b', '\t10 b', '\t\t10 b', '9 a\n10 b', '9 a\n\t10 b', '9 a\n\t\t10 b', '\n9 a', '', '\t', 'é\n10 ñ', '😀\n\t10 b', '9 a\nb', '9 a\n\t\tb']) {
    for (let caret = 0; caret <= text.length + 1; caret++) {
      for (const delta of [1, -1, 0]) {
        const r = indentLine(text, caret, delta, pins);
        indent.push({ text, caret, delta, pins: pins ?? [], out: r.text, caretOut: r.caret });
      }
    }
  }
}

const expand = [['9-10', '11'], ['9', '10'], ['3-5'], ['5-3'], ['1-1'], ['x', '2-4']].map((keys) => ({ keys, pins: expandKeys(keys) }));

const lines = (name, list) => `"${name}": [\n${list.map((x) => JSON.stringify(x)).join(',\n')}\n]`;
process.stdout.write(`{\n"source": "tC4 src/views/sectionDraft.js",\n${lines('expand', expand)},\n${lines('parse', parse)},\n${lines('initial', initial)},\n${lines('indent', indent)}\n}\n`);
