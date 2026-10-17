// Generates fixtures/export/print-vectors.json: what tC4's own print code answers, so
// tc-export can be checked against it rather than against a reading of it.
//
// - `pageSetup`: DEFAULT_PAGE_SETUP, PAGE_SPACING_FACTOR, PRINT_LEADING, PAPER_MM and
//   PAGE_MARGIN_MM, and for every paper, column count and spacing: `printVariables` and the
//   `@page` and `:root` rules that `printDocument` (src/data/export/pdf.ts) writes.
// - `books`: for the Titus and Jonah samples, the cases of test/export/pdf.test.ts and
//   generated books, the book model that `bookModel` (src/data/bookModel.js) builds from the
//   USFM (the input), then `printedItems`, `partsOfItems` (src/views/print/PrintBook.jsx),
//   the Community Checking callout's `undrafted` (src/views/CommunityChecking.jsx), whether
//   `printDocument` refuses, and `printBookHtml` for each drop-cap and verse-number choice
//   and each direction. `preview` holds the preview's pages: `paginate` and `partsOf`
//   (src/views/print/PrintPages.jsx) with a `fits` that counts characters instead of
//   measuring, and `PrintParts` rendered on each page with its `continues`.
// - `stories`: for the OBS sample, the cases of test/export/pdf.test.ts and generated story
//   sets, the stories as `parseStory` (journal/story.mjs) reads them (the input), the
//   pictures each story's frames resolve to (an input; see `pictureSources`), then
//   `readPrintedStories` (src/data/storyModel.ts) and `printStoriesHtml`
//   (src/views/print/PrintStories.jsx) for each layout, pictures choice and direction.
// - `catalog`: the English strings of the i18n keys the print shows.
//
// The modules are bundled with the esbuild in tC4's node_modules (0.25.12), with tC4's React
// 18.3.1 and usfm-js 3.4.3, into a scratch file that is removed. Two private functions of
// PrintPages.jsx (`atomsOf`, `carriesOn`) are taken from its source as written. Run from the
// repository root with a tC4 checkout:
//   node crates/tc-export/tools/gen-print-vectors.mjs /path/to/translationcore4 > fixtures/export/print-vectors.json
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';

const [tc4] = process.argv.slice(2);
if (!tc4) throw new Error('usage: gen-print-vectors.mjs <tc4 checkout>');
const repo = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../..');
const nm = path.join(tc4, 'node_modules');
const pinned = { esbuild: '0.25.12', react: '18.3.1', 'react-dom': '18.3.1', 'usfm-js': '3.4.3' };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, 'package.json'), 'utf8')).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const git = (...args) => execFileSync('git', ['-C', tc4, ...args], { encoding: 'utf8' }).trim();
const tc4Commit = git('rev-parse', 'HEAD');
if (git('status', '--porcelain', '--untracked-files=no', '--', 'src', 'journal', 'conformance')) {
  throw new Error(`${tc4} has local changes; the vectors must come from a commit`);
}
const esbuild = createRequire(path.join(nm, 'x.js'))('esbuild');

// ---------- tC4's modules ----------
const source = (rel) => fs.readFileSync(path.join(tc4, rel), 'utf8');
/** The text of a top-level declaration in a tC4 file, from `start` through the first `end` after it. */
const declaration = (rel, start, end) => {
  const text = source(rel);
  const from = text.indexOf(start);
  if (from === -1 || text.indexOf(start, from + 1) !== -1) throw new Error(`${rel}: expected one ${start}`);
  const to = text.indexOf(end, from);
  if (to === -1) throw new Error(`${rel}: no end for ${start}`);
  return text.slice(from, to + end.length);
};
const entry = `
export { bookModel, printedItems } from ${JSON.stringify(path.join(tc4, 'src/data/bookModel.js'))};
export { partsOfItems, PrintParts, printBookHtml, printVariables, PAPER_MM, PAGE_MARGIN_MM, PRINT_LEADING } from ${JSON.stringify(path.join(tc4, 'src/views/print/PrintBook.jsx'))};
export { printStoriesHtml } from ${JSON.stringify(path.join(tc4, 'src/views/print/PrintStories.jsx'))};
export { partsOf, paginate } from ${JSON.stringify(path.join(tc4, 'src/views/print/PrintPages.jsx'))};
export { DEFAULT_PAGE_SETUP, PAGE_SPACING_FACTOR } from ${JSON.stringify(path.join(tc4, 'src/data/export/pageSetup.ts'))};
export { printDocument } from ${JSON.stringify(path.join(tc4, 'src/data/export/pdf.ts'))};
export { readPrintedStories } from ${JSON.stringify(path.join(tc4, 'src/data/storyModel.ts'))};
export { parseStory } from ${JSON.stringify(path.join(tc4, 'journal/story.mjs'))};
export { bookName } from ${JSON.stringify(path.join(tc4, 'src/data/bookNames.ts'))};
export { getLocale } from ${JSON.stringify(path.join(tc4, 'src/i18n/index.js'))};
import en from ${JSON.stringify(path.join(tc4, 'src/i18n/en.json'))};
export { en };
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
export { React, renderToStaticMarkup };
export const PRIVATE = (() => {
  ${declaration('src/views/print/PrintPages.jsx', 'const atomsOf = ', ';\n')}
  ${declaration('src/views/print/PrintPages.jsx', 'const carriesOn = ', ';\n')}
  return { atomsOf, carriesOn };
})();
`;
// pdf.ts reads print.css with Vite's `?raw`; PrintPages.jsx imports it for its side effect.
const rawCss = {
  name: 'raw',
  setup(build) {
    build.onResolve({ filter: /\?raw$/ }, (args) => ({ path: path.resolve(args.resolveDir, args.path.slice(0, -4)), namespace: 'raw' }));
    build.onLoad({ filter: /.*/, namespace: 'raw' }, (args) => ({ contents: fs.readFileSync(args.path, 'utf8'), loader: 'text' }));
  },
};
const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'gen-print-vectors-'));
let tc;
try {
  const { outputFiles } = await esbuild.build({
    stdin: { contents: entry, resolveDir: tc4, sourcefile: 'entry.jsx', loader: 'jsx' },
    bundle: true,
    format: 'cjs',
    platform: 'node',
    jsx: 'automatic',
    loader: { '.css': 'empty' },
    nodePaths: [nm],
    plugins: [rawCss],
    write: false,
    logLevel: 'silent',
  });
  const file = path.join(scratch, 'print.cjs');
  fs.writeFileSync(file, outputFiles[0].text);
  tc = createRequire(file)(file);
} finally {
  fs.rmSync(scratch, { recursive: true, force: true });
}
if (tc.getLocale() !== 'en') throw new Error('the catalog must be English');

const sha256 = (text) => crypto.createHash('sha256').update(text, 'utf8').digest('hex');
const setupWith = (patch) => ({ ...tc.DEFAULT_PAGE_SETUP, ...patch });

// ---------- a seeded generator ----------
let seed = 0x5eed2a10;
const rand = () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let x = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  x = (x + Math.imul(x ^ (x >>> 7), 61 | x)) ^ x;
  return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
};
const int = (lo, hi) => lo + Math.floor(rand() * (hi - lo + 1));
const pick = (items) => items[Math.floor(rand() * items.length)];

// ---------- page setup ----------
const PAGE_RULE = /@page \{ size: [^;]+; margin: [^;]+;/;
const ROOT_RULE = /:root \{ [^}]*\}/;
const PLAIN_BOOK = '\\id TIT\n\\c 1\n\\p\n\\v 1 Text.\n';
const pageSetup = {
  default: tc.DEFAULT_PAGE_SETUP,
  spacingFactor: tc.PAGE_SPACING_FACTOR,
  leading: tc.PRINT_LEADING,
  paperMm: tc.PAPER_MM,
  marginMm: tc.PAGE_MARGIN_MM,
  cases: [],
};
for (const paper of ['a4', 'letter']) {
  for (const columns of [1, 2]) {
    for (const spacing of ['single', 'double']) {
      const setup = setupWith({ paper, columns, spacing });
      const html = tc.printDocument(PLAIN_BOOK, 'TIT', setup, 'ltr');
      pageSetup.cases.push({
        setup,
        variables: tc.printVariables(setup),
        page: html.match(PAGE_RULE)[0],
        root: html.match(ROOT_RULE)[0],
      });
    }
  }
}

// ---------- books ----------
const BOOK_VARIANTS = [];
for (const dropCapChapters of [true, false]) {
  for (const verseNumbers of [true, false]) {
    for (const dir of ['ltr', 'rtl']) BOOK_VARIANTS.push({ dropCapChapters, verseNumbers, dir });
  }
}
/** A page holds this many characters; the first page less, for the title. */
const PAGE_CHARS = [260, 420];
const atomChars = (atom) => (atom.gap ? 40 : (atom.head ? 24 : 0) + 6 + (atom.v.rest ?? atom.v.text).length);
const fits = (page, atoms) => atoms.reduce((sum, atom) => sum + atomChars(atom), 0) <= PAGE_CHARS[Math.min(page, 1)];

const bookCase = (name, code, usfm) => {
  const { byChapter, chapterNums } = tc.bookModel(usfm);
  const items = tc.printedItems(byChapter, chapterNums);
  const parts = tc.partsOfItems(items);
  const title = tc.bookName(code);
  let refused = null;
  try {
    tc.printDocument(usfm, code, tc.DEFAULT_PAGE_SETUP, 'ltr');
  } catch (error) {
    refused = error.code ?? String(error.message);
  }
  const html = BOOK_VARIANTS.map(({ dir, ...patch }) => ({
    ...patch,
    dir,
    html: tc.printBookHtml({ title, items, pageSetup: setupWith(patch), dir }),
  }));
  const atoms = tc.PRIVATE.atomsOf(items);
  const pages = tc.paginate(atoms, fits);
  const preview = [];
  for (const patch of [{}, { dropCapChapters: false, verseNumbers: false }]) {
    const setup = setupWith(patch);
    preview.push({
      dropCapChapters: setup.dropCapChapters,
      verseNumbers: setup.verseNumbers,
      pages: pages.map((page, i) => {
        const pageParts = tc.partsOf(page);
        const continues = tc.PRIVATE.carriesOn(pages[i + 1]?.[0]);
        const element = tc.React.createElement(tc.PrintParts, { parts: pageParts, pageSetup: setup, dir: 'ltr', continues });
        // A verse of a page: its number, and the text it shows (`text`, or `rest` after a break).
        const piece = (v) => ({ n: v.n, drafted: v.drafted, text: v.text, ...(v.rest === undefined ? {} : { rest: v.rest }) });
        const shown = pageParts.map((part) => (part.gap ? part : { c: part.c, head: part.head, verses: part.verses.map(piece) }));
        return { parts: shown, continues, html: tc.renderToStaticMarkup(element) };
      }),
    });
  }
  return {
    name,
    code,
    title,
    usfmSha256: sha256(usfm),
    // The book model, the input: each chapter key as bookModel wrote it, and the chapter numbers.
    byChapter: Object.entries(byChapter).map(([key, verses]) => ({
      key,
      verses: verses.map((v) => ({ n: v.n, drafted: v.drafted, text: v.text })),
    })),
    chapterNums,
    items: items.map((item) => (item.gap ? { gap: item.gap } : { c: item.c, verses: item.verses.map((v) => v.n) })),
    parts: parts.map((part) => (part.gap ? { gap: part.gap } : { c: part.c, head: part.head, verses: part.verses.map((v) => v.n) })),
    undrafted: chapterNums.some((c) => (byChapter[String(c)] || []).some((v) => !v.drafted)),
    refused,
    html,
    preview,
  };
};

const fixtureText = (rel) => fs.readFileSync(path.join(repo, rel), 'utf8');
const PDF_TEST_USFM = `\\id TIT
\\c 1
\\p
\\v 1 Paul, a servant of God.
\\v 2 ___
\\c 2
\\p
\\v 1 Speak <sound> doctrine.
\\c 3
\\p
\\v 1 ___
\\v 2 ___
`;
const PDF_TEST_DRAFTED = new Set([2, 4, 8]);
const PDF_TEST_GAPS = `${['\\id TIT', ...Array.from({ length: 9 }, (_, i) => `\\c ${i + 1}\n\\p\n\\v 1 ${PDF_TEST_DRAFTED.has(i + 1) ? `Verse of chapter ${i + 1}.` : '___'}`)].join('\n')}\n`;

// Verse texts that test escaping, scripts, inline markup and notes.
const TEXTS = [
  'In the beginning & the end.',
  'He said, "Look!" <and> it\'s done.',
  'هذا نص عربي للآية.',
  'וַיְהִי בִּימֵי שְׁפֹט הַשֹּׁפְטִים',
  'A \\add word\\add* added and \\nd Lord\\nd* named.',
  'Text with a note\\f + \\fr 1:1 \\ft A note.\\f* inside.',
  'ภาษาไทยไม่มีช่องว่างระหว่างคำเลยครับ',
  'One long verse that runs on and on so that a page has to split it between its words somewhere in the middle of it all.',
  'Short.',
];
const BODIES = [
  ...TEXTS,
  '___',
  '',
  '\\f + \\ft only a note\\f*',
  'First line\n\\q2 second line of poetry',
  '\\w word|x-occurrence="1" x-occurrences="1"\\w* and more',
];
const generatedBook = () => {
  const lines = ['\\id GEN generated', '\\h Generated'];
  const chapters = int(1, 9);
  const draftedChapter = Array.from({ length: chapters }, () => rand() < 0.5);
  for (let c = 1; c <= chapters; c++) {
    lines.push(`\\c ${c}`, '\\p');
    if (rand() < 0.08) continue; // a chapter with no verse
    const verses = int(1, 7);
    for (let v = 1; v <= verses; v++) {
      const bridged = rand() < 0.1 && v < verses;
      const key = bridged ? `${v}-${v + 1}` : String(v);
      const body = draftedChapter[c - 1] && rand() < 0.75 ? pick(BODIES) : pick(['___', '___', '']);
      lines.push(body === '' ? `\\v ${key}` : `\\v ${key} ${body}`);
      if (bridged) v += 1;
      if (rand() < 0.1) lines.push('\\p');
    }
  }
  return `${lines.join('\n')}\n`;
};

const books = [
  bookCase('titus-sample', 'TIT', fixtureText('fixtures/tc4/sample-burrito/ingredients/TIT.usfm')),
  bookCase('jonah-sample', 'JON', fixtureText('fixtures/tc4/sample-burrito/ingredients/JON.usfm')),
  bookCase('pdf-test', 'TIT', PDF_TEST_USFM),
  bookCase('pdf-test-gaps', 'TIT', PDF_TEST_GAPS),
  bookCase('nothing-drafted', 'TIT', '\\id TIT\n\\c 1\n\\p\n\\v 1 ___\n\\v 2 ___\n'),
  bookCase('no-chapters', 'TIT', '\\id TIT\n\\h Tito\n'),
  bookCase('chapters-out-of-order', 'PHM', '\\id PHM\n\\c 2\n\\p\n\\v 1 Second.\n\\c 1\n\\p\n\\v 1 ___\n\\c 3\n\\p\n\\v 1 Third.\n'),
  bookCase('chapter-repeated', 'PHM', '\\id PHM\n\\c 1\n\\p\n\\v 1 One.\n\\c 2\n\\p\n\\v 1 ___\n\\c 1\n\\p\n\\v 2 One again.\n\\c 3\n\\p\n\\v 1 Three.\n'),
  bookCase('drafted-without-text', 'PHM', '\\id PHM\n\\c 1\n\\p\n\\v 1 \\f + \\ft only a note\\f*\n\\v 2 Two.\n\\c 2\n\\p\n\\v 1 \\f + \\ft only a note\\f*\n\\c 3\n\\p\n\\v 1 Three.\n'),
  bookCase('chapter-zero-padded', 'PHM', '\\id PHM\n\\c 01\n\\p\n\\v 1 One.\n\\c 2\n\\p\n\\v 1 Two.\n'),
];
for (let i = 0; i < 24; i++) books.push(bookCase(`generated-${i + 1}`, 'GEN', generatedBook()));

// ---------- stories ----------
const STORY_VARIANTS = [];
for (const obsLayout of ['above', 'wrapped']) {
  for (const pictures of [true, false]) {
    for (const dir of ['ltr', 'rtl']) STORY_VARIANTS.push({ obsLayout, pictures, dir });
  }
}
/** The picture each frame of a story resolves to, as Community Checking would give it.
 * Frames whose number is a multiple of 4 resolve none and multiples of 5 an empty source,
 * so the rule that keeps only real sources of drafted frames is tested; the `&` and `"`
 * test how the source is written into the print. */
const pictureSources = (number, story) =>
  Object.fromEntries(
    story.frames
      .map((_, i) => i + 1)
      .filter((n) => n % 4 !== 0)
      .map((n) => [String(n), n % 5 === 0 ? '' : `/media/obs/${number}-${n}.jpg?pin=a&size="360"`]),
  );
const storyCase = async (name, stories) => {
  const byNumber = new Map(stories.map((story) => [story.number, story]));
  const store = { listStories: async () => [...byNumber.keys()].reverse(), readStory: async (n) => ({ bytes: '', md5: '', story: byNumber.get(n) }) };
  const asked = [];
  const { items, pictures, undrafted } = await tc.readPrintedStories(store, async (number, story) => {
    asked.push(number);
    return pictureSources(number, story);
  });
  const html = STORY_VARIANTS.map(({ dir, ...patch }) => ({
    ...patch,
    dir,
    html: tc.printStoriesHtml({ items, pictures: patch.pictures ? pictures : {}, pageSetup: setupWith(patch), dir }),
  }));
  return {
    name,
    stories: [...byNumber.values()].sort((a, b) => a.number - b.number).map((s) => ({
      number: s.number,
      title: s.title,
      frames: s.frames.map((f) => f.text),
      ref: s.ref,
    })),
    items: items.map((item) => ('gap' in item ? { gap: item.gap } : { story: item.story.number, frames: item.frames })),
    pictures,
    picturesAskedFor: asked,
    undrafted,
    html,
  };
};

const sampleDir = path.join(repo, 'fixtures/tc4/sample-burrito-obs/ingredients/content');
const sampleStories = fs
  .readdirSync(sampleDir)
  .filter((f) => /^\d\d\.md$/.test(f))
  .sort()
  .map((f) => tc.parseStory(fs.readFileSync(path.join(sampleDir, f), 'utf8')));
const IMAGE = (n) => `![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-${n}.jpg)`;
const story = (number, frames, extra = {}) => ({ number, title: '', frames: frames.map((text, i) => ({ image: IMAGE(`${number}-${i + 1}`), text })), ref: null, ...extra });
const PDF_TEST_STORIES = [
  { number: 1, title: 'La Creación', frames: [{ image: IMAGE('01-01'), text: 'Así fue <como> Dios hizo todo.' }, { image: IMAGE('01-02'), text: '' }], ref: 'Una historia de Génesis 1-2' },
  { number: 2, title: '', frames: [{ image: IMAGE('02-01'), text: 'Adán y Eva.' }], ref: null },
  { number: 3, title: '', frames: [{ image: IMAGE('03-01'), text: '' }], ref: null },
];
const STORY_TEXTS = ['Así fue como Dios hizo todo.', 'He said, "Look!" <and> it\'s done & gone.', 'هذا نص عربي.', 'Two lines\nof one frame.', 'x'];
const generatedStories = () => {
  const out = [];
  const drafting = rand() < 0.85;
  for (let number = 1; number <= 50; number++) {
    if (rand() < 0.8) continue;
    const frames = Array.from({ length: int(0, 6) }, () => (drafting && rand() < 0.5 ? pick(STORY_TEXTS) : ''));
    const title = drafting && rand() < 0.3 ? pick(['La Creación', 'Título & <más>', 'العنوان']) : '';
    const ref = drafting && rand() < 0.3 ? pick(['Una historia de Génesis 1-2', '', 'مرجع']) : rand() < 0.5 ? null : '';
    out.push(story(number, frames, { title, ref }));
  }
  return out;
};
const stories = [
  await storyCase('obs-sample', sampleStories),
  await storyCase('pdf-test', PDF_TEST_STORIES),
  await storyCase('pdf-test-frame-runs', [story(1, ['', 'Uno.', '', '', 'Dos.', '', ''], { title: 'T' })]),
  await storyCase('pdf-test-all-drafted', [story(1, ['Uno.', 'Dos.'])]),
  await storyCase('pdf-test-no-frames', [story(1, [], { title: 'T' })]),
  await storyCase('pdf-test-story-runs', [
    story(1, ['']),
    story(2, ['', ''], { title: 'Solo el título' }),
    story(3, ['']), story(4, ['']), story(6, ['']),
    story(7, ['', ''], { ref: 'Una referencia' }),
    story(8, ['']),
    story(9, ['Texto.', '']),
    story(10, ['']), story(11, ['']),
  ]),
  await storyCase('nothing-drafted', [story(1, ['', '']), story(2, ['', ''])]),
  await storyCase('no-stories', []),
];
for (let i = 0; i < 20; i++) stories.push(await storyCase(`generated-${i + 1}`, generatedStories()));

// ---------- the catalog ----------
const KEYS = [
  'cc.chapterHeading', 'cc.chapterNotDrafted', 'cc.chaptersNotDrafted', 'cc.notYetDrafted',
  'cc.frameNotDrafted', 'cc.framesNotDrafted', 'cc.storyNotDrafted', 'cc.storiesNotDrafted',
  'cc.nothingDrafted', 'cc.nothingDraftedObs', 'storyDraft.storyNumber', 'storyDraft.imageAlt',
];
const catalog = Object.fromEntries(KEYS.map((key) => [key, tc.en[key]]));

process.stdout.write(`${JSON.stringify({
  tc4Commit,
  node: process.version,
  generator: 'crates/tc-export/tools/gen-print-vectors.mjs',
  catalog,
  pageSetup,
  books,
  stories,
})}\n`);
