// Generates fixtures/obs/obs-vectors.json: what tC4's own OBS code answers, so tc-obs can
// be checked against it rather than against a reading of it.
//
// - `parse`: `parseStory` (journal/story.mjs) on the fifty stories of the template, of
//   sample-burrito-obs and of the en_obs v9 export, and on generated and hand-written
//   story files, valid and not.
// - `writes`: `writeFrame`, `writeTitle`, `writeRef`, `seedStory` and `applyStoryState` on
//   those files.
// - `textRules`: `frameTextError`, `titleTextError` and `refTextError` on edge strings.
// - `images`: `obsImageFileName`, `obsImagePackFromPaths` and `resolveObsImage`
//   (src/data/obsImages.ts).
// - `frameSet`: `obsFrameSetMismatch` (src/data/obsFrameSet.ts).
// - `print`: `printedStories` (src/data/storyModel.ts).
// - `recency`: `recordRecentStory` (src/data/obsRecency.ts).
// - `toNumber`: JavaScript's `Number(string)`, which state.jsx applies to a Home tile's
//   story and to the remembered story before it opens one.
// - `localizedNames`: `withLocalizedNames` (scripts/fix-obs-template.mjs).
// - `draft`: the drafted story 1 of conformance/generate-obs.mjs, from its
//   conformance/fixtures/obs-draft.mjs.
//
// The TypeScript modules are compiled with the esbuild in tC4's node_modules and the zip is
// read with tC4's fflate. Run from the repository root with a tC4 checkout:
//   node crates/tc-obs/tools/gen-obs-vectors.mjs /path/to/translationcore4 > fixtures/obs/obs-vectors.json
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const [tc4] = process.argv.slice(2);
if (!tc4) throw new Error('usage: gen-obs-vectors.mjs <tc4 checkout>');
const repo = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../..');
const nm = path.join(tc4, 'node_modules');
const pinned = { esbuild: '0.25.12', fflate: '0.8.2' };
for (const [name, version] of Object.entries(pinned)) {
  const found = JSON.parse(fs.readFileSync(path.join(nm, name, 'package.json'), 'utf8')).version;
  if (found !== version) throw new Error(`expected ${name}@${version}, found ${found}`);
}
const git = (...args) => execFileSync('git', ['-C', tc4, ...args], { encoding: 'utf8' }).trim();
const tc4Commit = git('rev-parse', 'HEAD');
if (git('status', '--porcelain', '--untracked-files=no', '--', 'src', 'journal', 'scripts', 'conformance')) {
  throw new Error(`${tc4} has local changes; the vectors must come from a commit`);
}
const require = createRequire(path.join(nm, 'x.js'));
const esbuild = require('esbuild');
const fflate = require('fflate');

const story = await import(pathToFileURL(path.join(tc4, 'journal/story.mjs')).href);
const fixObs = await import(pathToFileURL(path.join(tc4, 'scripts/fix-obs-template.mjs')).href);
const { DRAFT } = await import(pathToFileURL(path.join(tc4, 'conformance/fixtures/obs-draft.mjs')).href);
const tsModule = async (rel) => {
  const source = fs.readFileSync(path.join(tc4, rel), 'utf8');
  const { code } = esbuild.transformSync(source, { loader: 'ts', format: 'esm' });
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
};
const images = await tsModule('src/data/obsImages.ts');
const frameSet = await tsModule('src/data/obsFrameSet.ts');
const storyModel = await tsModule('src/data/storyModel.ts');
const recency = await tsModule('src/data/obsRecency.ts');

const md5 = (text) => crypto.createHash('md5').update(text, 'utf8').digest('hex');
const sha256File = (file) => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const attempt = (fn) => {
  try {
    return { ok: fn() };
  } catch (error) {
    return { error: String(error?.message ?? error) };
  }
};

// ---------- the story files ----------
const pad = (n) => String(n).padStart(2, '0');
const templateDir = path.join(tc4, 'conformance/fixtures/text_stories/ingredients/content');
const sampleDir = path.join(tc4, 'conformance/sample-burrito-obs/ingredients/content');
const enObsZip = path.join(repo, 'bundle/en/en_obs_v9.sb.zip');
const enObs = fflate.unzipSync(fs.readFileSync(enObsZip));
const sets = {
  template: (n) => fs.readFileSync(path.join(templateDir, `${pad(n)}.md`), 'utf8'),
  sample: (n) => fs.readFileSync(path.join(sampleDir, `${pad(n)}.md`), 'utf8'),
  en_obs: (n) => fflate.strFromU8(enObs[`en_obs/ingredients/content/${pad(n)}.md`]),
};

// A parsed story, as compact as a test can compare it.
const digestStory = (s) => ({
  number: s.number,
  title: s.title,
  ref: s.ref,
  frames: s.frames.length,
  framesMd5: md5(s.frames.map((f) => `${f.image}\u0000${f.text}`).join('\u0001')),
});
const parseResult = (text, full) => {
  const r = attempt(() => story.parseStory(text));
  if (r.error !== undefined) return { error: r.error };
  return full ? { story: r.ok } : { digest: digestStory(r.ok) };
};
const textResult = (fn, full) => {
  const r = attempt(fn);
  if (r.error !== undefined) return { error: r.error };
  return full ? { text: r.ok } : { md5: md5(r.ok), length: r.ok.length };
};

// A deterministic pseudo-random source (mulberry32).
let seed = 0x0b5;
const random = () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const pick = (list) => list[Math.floor(random() * list.length)];
const int = (lo, hi) => lo + Math.floor(random() * (hi - lo + 1));

// Lines JavaScript's trim() empties, and three it does not.
const BLANKS = ['', '', '', '', ' ', '\t', '\u00a0', '\ufeff', '\u3000', '\u2028', '\u000b', '\u1680', '\u202f'];
const NOT_BLANK = ['\u0085', '\u200b', '\u180e'];
const blankLine = () => (random() < 0.03 ? pick(NOT_BLANK) : pick(BLANKS));
const WORDS = ['Al principio.', 'Dios creó todo.', 'línea', '«Que haya luz».', 'x', 'a b c', '# 3.', '# 3. Título', '_r_', '__', 'texto ', ' texto', '![x', 'x)', '_', 'α\u2028β'];
const IMAGES = [
  (n, f) => `![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-${pad(n)}-${pad(f)}.jpg)`,
  () => '![](x)',
  () => '![a](b c)',
  () => '![a]b](c)',
  () => '![a](b)c',
  () => '![a](b) ',
];
const TITLES = [
  (n) => `# ${n}.`,
  (n) => `# ${n}. La Creación`,
  (n) => `# ${n}. `,
  (n) => `# ${pad(n)}. Título`,
  (n) => `#  ${n}.`,
  (n) => `# ${n}.x`,
  () => '# 0.',
  () => '# 51. Nope',
  (n) => `# ${n}. a\u2028b`,
  (n) => `# ${n}.  dos espacios`,
  (n) => `# ${n}. \u00a0T`,
  () => 'no title',
  () => '',
];
const REFS = ['_A Bible story from: Genesis 1-2_', '_ _', '__', '_\u00a0_', '_ x _', '_x_', '_\ufeff_'];

// A story in tC4's grammar, with random spacing; most of them parse.
const wellFormed = () => {
  const n = int(1, 50);
  const lines = [random() < 0.85 ? `# ${n}.${random() < 0.6 ? ' Título' : ''}` : pick(TITLES)(n)];
  for (let i = int(0, 2); i > 0; i--) lines.push(blankLine());
  const frames = int(random() < 0.1 ? 0 : 1, 5);
  for (let f = 1; f <= frames; f++) {
    lines.push(random() < 0.9 ? IMAGES[0](n, f) : pick(IMAGES)(n, f));
    for (let i = int(0, 3); i > 0; i--) lines.push(blankLine());
    if (random() < 0.7) for (let i = int(1, 3); i > 0; i--) lines.push(random() < 0.85 ? pick(WORDS.slice(0, 6)) : pick(WORDS));
    for (let i = int(0, 3); i > 0; i--) lines.push(blankLine());
  }
  if (random() < 0.5) lines.push(random() < 0.8 ? REFS[0] : pick(REFS));
  for (let i = int(0, 2); i > 0; i--) lines.push(blankLine());
  let text = lines.join('\n');
  if (random() < 0.5) text += '\n';
  if (random() < 0.03) {
    const at = int(0, text.length);
    text = `${text.slice(0, at)}\r${text.slice(at)}`;
  }
  return text;
};
// Lines in any order.
const scrambled = () => {
  const n = int(1, 50);
  const kinds = [() => pick(TITLES)(n), () => pick(IMAGES)(n, int(1, 9)), () => pick(WORDS), () => pick([...BLANKS, ...NOT_BLANK]), () => pick(REFS)];
  const lines = [];
  for (let i = int(1, 9); i > 0; i--) lines.push(pick(kinds)());
  return lines.join('\n');
};

const HAND = {
  empty: '',
  'title only': '# 1.',
  'title and newline': '# 1. T\n',
  'one empty frame': '# 1.\n\n![a](b)',
  'text before image': '# 1.\n\nText\n\n![a](b)\n\nx\n',
  'two paragraphs': '# 1.\n\n![a](b)\n\nuno\n\ndos\n',
  'reference only': '# 1.\n\n_ref_\n',
  'blank reference': '# 2.\n\n![a](b)\n\nx\n\n_ \u00a0_\n',
  'reference not last': '# 2.\n\n![a](b)\n\n_r_\n\nx\n',
  'reference then blanks': '# 2.\n\n![a](b)\n\nx\n\n_r_\n\n \n\t\n',
  'reference after empty last frame': '# 2.\n\n![a](b)\n\n\n\n_r_\n',
  crlf: '# 1.\r\n\r\n![a](b)\r\n\r\nx\r\n',
  'lone cr': '# 1. T\r\n![a](b)\n',
  'story 0': '# 0. Cero\n',
  'story 51': '# 51.\n',
  'story 007': '# 007. Siete\n\n![a](b)\n\nx\n',
  'huge story number': '# 99999999999999999999.\n',
  'title u2028': '# 1. a\u2028b\n',
  'title u2029': '# 1. a\u2029b\n',
  'title nel': '# 1. a\u0085b\n',
  'title trailing space': '# 1. T \n\n![a](b)\n',
  'title no space': '# 1.T\n',
  'title-form line in a frame': '# 1.\n\n![a](b)\n\n# 2. Otra\n',
  'image with spaces': '# 1.\n\n![OBS Image](a b c)\n\nx\n',
  'image alt bracket': '# 1.\n\n![a]b](c)\n\nx\n',
  'image trailing space': '# 1.\n\n![a](b) \n\nx\n',
  'bom first': '\ufeff# 1.\n',
  'feff blank lines': '# 1.\n\ufeff\n![a](b)\n\ufeff\nx\n\ufeff\n',
  'nel blank lines': '# 1.\n\u0085\n![a](b)\n\nx\n\u0085\n',
  'u2028 blank': '# 1.\n\u2028\n![a](b)\n\nx\n',
  'zwsp is not blank': '# 1.\n\u200b\n![a](b)\n',
  'mongolian vowel separator': '# 1.\n\u180e\n![a](b)\n',
  'ref with nbsp text': '# 1.\n\n![a](b)\n\nx\n\n_\u00a0x\u00a0_\n',
  'ref underscore only': '# 1.\n\n![a](b)\n\nx\n\n_\n',
  'last line is image': '# 1.\n\n![a](b)\n\nx\n\n![c](d)',
  'many frames no blanks': '# 3. T\n![a](1)\nuno\n![a](2)\ndos\ntres\n![a](3)\n_r_',
};

const parseCases = [];
const fixtureParse = [];
for (const [set, read] of Object.entries(sets)) {
  for (let n = 1; n <= 50; n++) fixtureParse.push({ set, n, ...parseResult(read(n), false) });
}
for (const [name, text] of Object.entries(HAND)) parseCases.push({ name, text, ...parseResult(text, true) });
for (let i = 0; i < 600; i++) {
  const text = i % 3 === 2 ? scrambled() : wellFormed();
  parseCases.push({ name: `generated ${i}`, text, ...parseResult(text, true) });
}

// ---------- writes ----------
// Frame texts the writer accepts, then ones it refuses.
const GOOD_TEXTS = ['', 'Al principio.', 'uno\ndos', 'línea uno\nlínea dos', 'Así fue.\nDespués.\nAl fin.', 'x', 'x\u2028y', 'a\u00a0b', '«Que haya luz».'];
const TEXTS = [
  '', 'Al principio.', 'uno\ndos', 'a\n\nb', '\n', 'a\n', '\na', ' ', '\u00a0', 'x\u00a0', ' x', 'x ',
  '![x](y)', '![x](y) ', '_r_', '__', '_', '# 3.', '# 3. T', '# 3.x', 'a\rb', 'línea uno\nlínea dos',
  'x\u2028y', '\ufeff', 'a\n\u0085', 'a\n\u200b', 'a\n\ufeff', 'Así fue.\nDespués.\nAl fin.',
];
const STATES = [
  { frames: {}, ref: null },
  { frames: { 0: 'T', 1: 'uno' }, ref: 'R' },
  { frames: { 2: 'dos', 1: '' }, ref: null },
  { frames: { 1: 'a\nb' } },
  { frames: { 0: '' }, ref: 'Una referencia' },
  { frames: { 3: 'x' }, ref: 'R' },
  { frames: { 1: 'a\n\nb' }, ref: null },
  { frames: { 1: 'x' }, ref: '' },
];
const opsFor = (text, full, count) => {
  const parsed = attempt(() => story.parseStory(text));
  const frames = parsed.ok ? parsed.ok.frames.length : 2;
  const ops = [];
  const add = (op, args, fn) => ops.push({ op, args, ...textResult(fn, full) });
  add('seed', [], () => story.seedStory(text));
  for (let i = 0; i < count; i++) {
    const kind = int(0, 9);
    if (kind <= 5) {
      const frame = random() < 0.75 ? int(0, frames) : pick([frames + 1, -1, 1.5]);
      const t = random() < 0.7 ? pick(GOOD_TEXTS) : pick(TEXTS);
      add('frame', [frame, t], () => story.writeFrame(text, frame, t));
    } else if (kind <= 6) {
      const t = pick(['', 'Título', 'La Creación', 'Título', ' T', 'T ', 'a\nb', 'a\u2028b', '\u00a0']);
      add('title', [t], () => story.writeTitle(text, t));
    } else if (kind <= 8) {
      const t = pick(['R', 'Una historia bíblica de: Génesis 1-2', 'R', 'x_y', '_', '', ' R', 'R ', 'a\nb', '\u00a0']);
      add('ref', [t], () => story.writeRef(text, t));
    } else {
      const state = pick(STATES);
      add('state', [state], () => story.applyStoryState(text, state));
    }
  }
  return ops;
};
const writeCases = [];
parseCases.forEach((c, index) => {
  // `case` is the index of the story in `parse.cases`. Generated cases keep the md5 of
  // each result; the hand-written ones keep the text.
  writeCases.push({ case: index, ops: opsFor(c.text, !c.name.startsWith('generated'), c.error ? 2 : 12) });
});
const fixtureWrites = [];
for (const [set, read] of Object.entries(sets)) {
  for (const n of [1, 2, 7, 12, 50]) {
    fixtureWrites.push({ set, n, ops: opsFor(read(n), false, 25) });
  }
}

// ---------- text rules ----------
const RULE_INPUTS = [...new Set([...GOOD_TEXTS, ...TEXTS, ...BLANKS, ...NOT_BLANK, ...WORDS, ...REFS, 'T', 'a\u2029b', '\u2028', 'x\n\u2028', '\u0085', 'x\u0085', '\u180e', 'x\u180e', '\u3000x'])];
const textRules = RULE_INPUTS.map((input) => ({
  input,
  frame: story.frameTextError(input),
  title: story.titleTextError(input),
  ref: story.refTextError(input),
}));

// ---------- images ----------
const templateLines = [];
for (let n = 1; n <= 50; n++) for (const line of sets.template(n).split('\n')) if (line.startsWith('![')) templateLines.push(line);
const IMAGE_LINES = [
  ...templateLines.filter((_, i) => i % 37 === 0),
  '![x](obs-xx-99-99.jpg)',
  '![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg "title")',
  "![OBS Image](https://cdn.door43.org/obs/jpg/360px/obs-en-01-01.jpg 't')",
  '![a](x \'y")',
  '![a](b) ',
  '![a](b)\u00a0',
  '![a](b)\u0085',
  ' ![a](b)',
  '![a](b c)',
  '![a](%E0%A4%A)',
  '![a](/)',
  '![a](http://)',
  '![a](x%2Fy.jpg)',
  '![a](a/b/../c.jpg)',
  '![a](c.jpg?x=1#f)',
  '![a](c%20d.jpg)',
  '![a](//host/p.jpg)',
  '![a](file:///C:/x/y.png)',
  '![a](mailto:x@y)',
  '![a](data:image/png;base64,AAA)',
  '![a](\\\\server\\x.jpg)',
  '![a](https://h/%F0%9F%98%80.jpg)',
  '![a](https://h/é.jpg)',
  '![a](https://h/a%25b.jpg)',
  '![]()',
  '![a](<x>)',
  '![a](https://h/dir/)',
  '![a](https://h)',
  '![a](local://pic.jpg)',
  '![a](https://h/%C0%80.jpg)',
  '![a](https://h/%ED%A0%80.jpg)',
  '![a](https://h/x.jpg%)',
  '![a](https://h/a;b.jpg)',
  '![a](https://H/X.JPG)',
  '![a](javascript:alert(1))',
  '![a]b](c.jpg)',
  'not an image',
];
const fileNames = IMAGE_LINES.map((line) => ({ line, fileName: images.obsImageFileName(line) }));

const PACK_PATHS = [
  ['360px/obs-en-01-02.jpg', '360px/obs-en-01-01.jpg', 'README.md', 'obs-en-01-01.jpg', '720px/obs-en-01-01.JPG', 'x/a.jpeg', 'x/a.webp', 'x/a.png', 'x/a.gif', 'x/a.jpg.txt', 'b/\uffff.jpg', 'b/\u{1F600}.jpg', 'B/z.PNG', 'a/z.png'],
  [],
  ['only/one.jpg'],
];
const uriFor = (ipath) => `local://${ipath}`;
const packs = PACK_PATHS.map((paths) => ({ paths, pack: images.obsImagePackFromPaths(images.DEFAULT_OBS_IMAGES, paths, uriFor) }));

const line1 = templateLines[0];
const name1 = images.obsImageFileName(line1);
const EN_OBS_IMAGES = images.DEFAULT_OBS_IMAGES;
const OTHER = { repoPath: 'git.door43.org/es-419_gl/es-419_obs_images', sha: '1'.repeat(40), flavor: 'peripheral/x-obsimages' };
const pinsWith = (primary, fallback) => ({ schemaVersion: 2, languageSets: { primary: primary ? { 'obs-images': primary } : {}, fallback: fallback ? { 'obs-images': fallback } : {} }, resources: {} });
const ingredientUri = (ipath) => `https://rig.test/api/burrito/ingredient/bytes/_local_/_local_/project?ipath=${encodeURIComponent(ipath).replace(/%2F/g, '/')}`;
const RESOLVE = [
  { name: 'non-image line', line: 'not an image', pins: pinsWith(null, null), project: {}, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'nothing anywhere', line: line1, pins: pinsWith(null, null), project: {}, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'default', line: line1, pins: pinsWith(null, null), project: {}, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default' } } },
  { name: 'project wins', line: line1, pins: pinsWith(EN_OBS_IMAGES, EN_OBS_IMAGES), project: { [name1]: { uri: `local://${name1}`, role: 'x-obsimages' } }, packs: [{ pin: EN_OBS_IMAGES, images: { [name1]: 'local://override' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default' } } },
  { name: 'primary pin', line: line1, pins: pinsWith(EN_OBS_IMAGES, EN_OBS_IMAGES), project: {}, packs: [{ pin: EN_OBS_IMAGES, images: { [name1]: 'local://override' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default' } } },
  { name: 'fallback pin', line: line1, pins: pinsWith(OTHER, EN_OBS_IMAGES), project: {}, packs: [{ pin: EN_OBS_IMAGES, images: { [name1]: 'local://fallback' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'repo case ignored, sha exact', line: line1, pins: pinsWith({ ...EN_OBS_IMAGES, repoPath: 'GIT.door43.org/UW/OBS_images_360' }, null), project: {}, packs: [{ pin: EN_OBS_IMAGES, images: { [name1]: 'local://case' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'wrong sha falls through', line: line1, pins: pinsWith(OTHER, null), project: {}, packs: [{ pin: { ...OTHER, sha: '2'.repeat(40) }, images: { [name1]: 'local://wrong' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default.jpg' } } },
  { name: 'ambiguous pack', line: line1, pins: pinsWith(OTHER, null), project: {}, packs: [{ pin: OTHER, images: { [name1]: ['local://one.jpg', 'local://two.jpg'] } }], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default.jpg' } } },
  { name: 'corrupt pack', line: line1, pins: pinsWith(OTHER, null), project: {}, packs: [{ pin: OTHER, images: { [name1]: { uri: 'local://bad.jpg', decodable: false } } }], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default.jpg' } } },
  { name: 'first matching pack only', line: line1, pins: pinsWith(OTHER, null), project: {}, packs: [{ pin: OTHER, images: {} }, { pin: OTHER, images: { [name1]: 'local://second' } }], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'two project matches', line: line1, pins: pinsWith(null, null), project: { a: { uri: `local://a/${name1}`, role: 'x-obsimages' }, b: { uri: `local://b/${name1}`, role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default.jpg' } } },
  { name: 'project without role', line: line1, pins: pinsWith(null, null), project: { a: { uri: `local://a/${name1}` } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project undecodable', line: line1, pins: pinsWith(null, null), project: { a: { uri: `local://a/${name1}`, role: 'x-obsimages', decodable: false } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: 'local://default.jpg' } } },
  { name: 'project ipath', line: line1, pins: pinsWith(null, null), project: { [`ingredients/360px/${name1}`]: { uri: ingredientUri(`360px/${name1}`), role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project ipath percent', line: `![x](https://cdn.door43.org/obs/jpg/360px/${encodeURIComponent('obs-50%.jpg')})`, pins: pinsWith(null, null), project: { 'ingredients/360px/obs-50%.jpg': { uri: ingredientUri('360px/obs-50%.jpg'), role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project ipath plus', line: '![x](a%20b.jpg)', pins: pinsWith(null, null), project: { p: { uri: 'https://rig.test/bytes/r?ipath=360px/a+b.jpg', role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project empty ipath', line: '![x](q.jpg)', pins: pinsWith(null, null), project: { p: { uri: 'https://rig.test/bytes/q.jpg?ipath=', role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project host basename', line: '![x](q.jpg)', pins: pinsWith(null, null), project: { p: { uri: 'local://q.jpg', role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: {} } },
  { name: 'project bad uri', line: '![x](q.jpg)', pins: pinsWith(null, null), project: { p: { uri: 'http://', role: 'x-obsimages' } }, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: { 'q.jpg': 'local://d' } } },
  { name: 'default undecodable', line: line1, pins: pinsWith(null, null), project: {}, packs: [], fallbackPack: { pin: EN_OBS_IMAGES, images: { [name1]: { uri: 'local://bad', decodable: false } } } },
];
const resolve = RESOLVE.map((c) => ({ ...c, result: images.resolveObsImage(c.line, c.pins, c.project, c.packs, c.fallbackPack) }));

// ---------- frame sets, print, recency ----------
const mk = (number, frames, text = 'x', title = text, ref = null) => ({
  number, title, ref,
  frames: Array.from({ length: frames }, (_, i) => ({ image: `![x](obs-${number}-${i}.jpg)`, text: typeof text === 'function' ? text(i) : text })),
});
const FRAME_SETS = [
  [[mk(1, 2, 'old')], [mk(1, 2, 'new')]],
  [[mk(1, 2)], [mk(1, 3)]],
  [[mk(1, 2)], [mk(2, 2)]],
  [[mk(1, 2), mk(2, 1)], [mk(1, 2)]],
  [[mk(2, 1), mk(1, 2)], [mk(1, 2), mk(2, 1)]],
  [[], []],
  [[mk(3, 4), mk(1, 1)], [mk(1, 1), mk(3, 5)]],
];
const frameSets = FRAME_SETS.map(([current, incoming]) => ({ current, incoming, mismatch: frameSet.obsFrameSetMismatch(current, incoming) }));

const blank = (n, frames) => mk(n, frames, '', '');
const PRINT = [
  [],
  [blank(1, 3), blank(2, 2)],
  [mk(1, 3, (i) => (i === 1 ? 'b' : ''), ''), blank(2, 2)],
  [blank(1, 1), mk(2, 4, (i) => (i % 2 ? 'x' : ''), 'T'), blank(3, 2), blank(4, 1), mk(5, 2, '', '', 'R'), blank(6, 2)],
  [mk(1, 2, 'a'), mk(2, 2, 'b'), blank(3, 1), mk(4, 1, '', 'Title only')],
  [mk(7, 3, (i) => (i === 2 ? 'c' : ''), ''), blank(2, 1), mk(3, 1, 'z')],
];
const print = PRINT.map((stories) => ({
  stories,
  items: storyModel.printedStories(stories),
  drafted: stories.map(storyModel.storyDrafted),
}));

const RECENCY = [
  [[undefined, 3, 1000], [null, 7, 2000], [null, 3, 3000]],
  [[[{ story: 1, at: 100 }, { story: 2, at: 200 }, { story: 3, at: 300 }], 12, 400]],
  [[[{ story: 5, at: 500 }], 4, 400], [null, 0, 900], [null, -2, 900]],
  [[[], 9, 5], [null, 9, 5], [null, 8, 5], [null, 7, 5], [null, 6, 1]],
  [[[{ story: 2, at: 10 }, { story: 2, at: 20 }], 2, 15]],
  [[[{ story: 1, at: 1 }, { story: 2, at: 2 }, { story: 3, at: 3 }, { story: 4, at: 4 }], 5, 0]],
];
const recencyCases = RECENCY.map((steps) => {
  let list;
  const out = [];
  for (const [given, n, at] of steps) {
    const input = given === null ? list : given;
    list = recency.recordRecentStory(input, n, at);
    out.push({ input: input ?? null, story: n, at, output: list });
  }
  return out;
});

// ---------- Number(string) ----------
const NUMBER_INPUTS = ['GEN', '', ' ', '7', ' 7 ', '07', '7.0', '7.5', '1e1', '1E1', '0x10', '0X1f', '0b11', '0o7', '+7', '-7',
  'Infinity', '-Infinity', '+Infinity', 'infinity', '1_000', '\u0661', '\u00a07\u00a0', '\ufeff7', '7\u0085', '\u20287\u2029', '.5', '5.',
  '0.0', '-0', '1e', 'e1', '0x', '+0x10', '00x10', '1 2', '12abc', '0.1e2', '5e-1', '9007199254740993', '1e400', '\t\n\v\f\r 3', '\u180e3'];
const encodeNumber = (x) => (Number.isNaN(x) ? 'NaN' : x === Infinity ? 'Infinity' : x === -Infinity ? '-Infinity' : Object.is(x, -0) ? '-0' : x);
const toNumber = NUMBER_INPUTS.map((input) => ({ input, number: encodeNumber(Number(input)) }));

// ---------- the template metadata ----------
const templateMetadata = fs.readFileSync(path.join(tc4, 'conformance/fixtures/text_stories/metadata.json'), 'utf8');
const LOCALIZED = [
  { name: 'template', text: templateMetadata },
  { name: 'already carries it', text: '{\n  "localizedNames": {},\n  "ingredients": {}\n}\n' },
  { name: 'tab indent', text: '{\n\t"a": 1,\n\t"ingredients": {}\n}' },
  { name: 'no ingredients key', text: '{\n  "a": 1\n}\n' },
  { name: 'key not at a line start', text: '{"a": 1, "ingredients": {}}' },
  { name: 'nested first', text: '{\n  "x": {\n    "ingredients": 1\n  },\n  "ingredients": {}\n}\n' },
];
const localizedNames = LOCALIZED.map(({ name, text }) => {
  const r = attempt(() => fixObs.withLocalizedNames(text));
  return r.error !== undefined ? { name, error: r.error } : { name, md5: md5(r.ok), text: name === 'template' ? undefined : r.ok };
});

// ---------- the drafted sample story (generate-obs.mjs) ----------
let drafted = story.seedStory(sets.template(1));
drafted = story.writeFrame(drafted, 0, DRAFT.title);
for (const [frame, text] of Object.entries(DRAFT.frames)) drafted = story.writeFrame(drafted, Number(frame), text);
drafted = story.writeRef(drafted, DRAFT.ref);
if (drafted !== sets.sample(1)) throw new Error('generate-obs.mjs no longer yields sample-burrito-obs story 1');
const seedsMatch = Array.from({ length: 50 }, (_, i) => i + 1).every((n) => n === 1 || story.seedStory(sets.template(n)) === sets.sample(n));
if (!seedsMatch) throw new Error('the sample stories 2-50 are not the seed forms of the template');

const out = {
  generator: 'crates/tc-obs/tools/gen-obs-vectors.mjs',
  tc4Commit,
  node: process.version,
  inputs: { enObsZip: { file: 'bundle/en/en_obs_v9.sb.zip', sha256: sha256File(enObsZip) } },
  parse: { fixtures: fixtureParse, cases: parseCases },
  writes: { fixtures: fixtureWrites, cases: writeCases },
  textRules,
  images: { fileNames, packs, resolve, default: images.DEFAULT_OBS_IMAGES },
  frameSet: frameSets,
  print,
  recency: recencyCases,
  toNumber,
  localizedNames,
  draft: { draft: DRAFT, story1Md5: md5(drafted) },
};
process.stdout.write(`${JSON.stringify(out)}\n`);
