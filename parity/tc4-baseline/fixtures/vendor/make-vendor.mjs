#!/usr/bin/env node
// Rebuild fixtures/vendor/: the resource files that tC4 does not vendor, cut from published copies.
//
//   node fixtures/vendor/make-vendor.mjs [--tc4 <translationcore4 root>]
//
// git.door43.org is out of reach here, so the Door43 releases that the sample projects pin cannot
// be fetched. Two published sources carry real copies, and this script cuts what the captured
// screens read out of them:
//
//   translationCore 3 (github.com/unfoldingWord/translationCore at TC3.commit) commits the
//   resources it bundles under tcResources/, in tC3's own layout:
//     el-x-koine_ugnt/TIT.usfm         UGNT v0.34 (the pinned release), Titus. tC3 keeps it as
//                                      verse-object JSON; usfm-js (tC4's own copy) writes it back
//                                      to USFM, and the result reads back to the same JSON.
//     en_tw/payload/<cat>/<slug>.md    the en_tw v88 article of every word the en_twl v91 Titus
//                                      list links (the project pins v91).
//     en_ta/translate/<slug>/01.md     the en_ta v88 article of every SupportReference in the
//     en_ta/translate/<slug>/title.md  en_tn v91 and es-419_tn v66 Titus notes (pinned: v91).
//                                      tC3 keeps one file per article ("# Title #" + body); it is
//                                      split back into the Door43 layout tC4 reads.
//   open-bible-stories@0.0.5 (npm) carries the whole of English OBS story 1 in its example:
//     en_obs/01.md                     (the project pins en_obs v9; the example's release is unknown)
//
// Checks before anything is written:
//   - every download matches the sha256 recorded below, and each tC3 bundle's manifest names the
//     expected version;
//   - the UGNT agrees with the ULT v89 alignment of Titus that tC4 vendors
//     (test/fixtures/en_ult/TIT.usfm), both ways and with occurrence counts: every Greek word
//     the ULT points at is in the verse, and the ULT points at every word of the verse;
//   - every tW and tA article the Titus helps link is in the bundle;
//   - OBS story 1 has its title, 16 frames and reference line.
//
// Downloads are kept in .cache/vendor/ and reused. They need github.com (raw) and
// registry.npmjs.org. SOURCES.json records where every written file came from.
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { unzipSync } from 'fflate';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BASELINE = path.resolve(HERE, '..', '..');
const CACHE = path.join(BASELINE, '.cache', 'vendor');
const arg = (name, fallback) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : fallback;
};
const TC4 = path.resolve(arg('--tc4', process.env.TC4_ROOT ?? '/home/user/unfoldingword/translationcore4'));

export const TC3 = { repo: 'unfoldingWord/translationCore', commit: '5729aa7b977ef85356139f73d957ee65a276460b' };
const BUNDLES = {
  ugnt: {
    dir: 'tcResources/el-x-koine/bibles/ugnt/v0.34_unfoldingWord',
    file: 'books.zip',
    version: '0.34',
    sha256: 'bffd8996e86794cc4b94175eee9b7765816b6fd459ac560913b23cfe5975dd3c',
  },
  tw: {
    dir: 'tcResources/en/translationHelps/translationWords/v88_unfoldingWord',
    file: 'contents.zip',
    version: '88',
    sha256: 'd17d35f1c73076a5c19c76bb259b9589f990a230d87b65743b973c19245f4b36',
  },
  ta: {
    dir: 'tcResources/en/translationHelps/translationAcademy/v88_unfoldingWord',
    file: 'contents.zip',
    version: '88',
    sha256: 'd651c7fb08ee10365f7a0041b64db99dcc8747a18fc7242e4fa4e53b43d216cd',
  },
};
const OBS_PACKAGE = {
  name: 'open-bible-stories',
  version: '0.0.5',
  integrity: 'sha512-WrBadu6Xuht6U5tLIkfxXdH9bdo9zUj3BWgqFSwA8Woeg71XiYTG7JzDmMyV9X55NDsrPyUBX64P9gtkVOCyVg==',
  file: 'dist/components/Examples/EditorExample.js',
};
/** The output folders this script owns (each is replaced on every run). */
const OUTPUTS = ['el-x-koine_ugnt', 'en_tw', 'en_ta', 'en_obs'];

const md5 = (bytes) => crypto.createHash('md5').update(bytes).digest('hex');
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
const decode = (bytes) => new TextDecoder().decode(bytes);
const rawUrl = (p) => `https://raw.githubusercontent.com/${TC3.repo}/${TC3.commit}/${p}`;

function download(url, file) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  execFileSync('curl', ['--fail', '--silent', '--show-error', '--location', '--output', file, url], { stdio: ['ignore', 'ignore', 'inherit'] });
}

/** One tC3 bundle: checks the manifest's version and the archive's sha256, returns its entries. */
function bundle(name) {
  const spec = BUNDLES[name];
  const file = path.join(CACHE, 'tc3', spec.dir, spec.file);
  if (!fs.existsSync(file) || sha256(fs.readFileSync(file)) !== spec.sha256) download(rawUrl(`${spec.dir}/${spec.file}`), file);
  const bytes = fs.readFileSync(file);
  if (sha256(bytes) !== spec.sha256) throw new Error(`${spec.dir}/${spec.file}: sha256 ${sha256(bytes)}, expected ${spec.sha256}`);
  const manifestFile = path.join(CACHE, 'tc3', spec.dir, 'manifest.json');
  if (!fs.existsSync(manifestFile)) download(rawUrl(`${spec.dir}/manifest.json`), manifestFile);
  const version = JSON.parse(fs.readFileSync(manifestFile, 'utf8')).dublin_core?.version;
  if (version !== spec.version) throw new Error(`${spec.dir}: manifest version ${version}, expected ${spec.version}`);
  // tC3's archives list every entry twice; both copies hold the same bytes, and the last one wins.
  return { entries: unzipSync(new Uint8Array(bytes)), source: (entry) => `github.com/${TC3.repo}@${TC3.commit.slice(0, 7)}:${spec.dir}/${spec.file}#${entry}` };
}

function obsPackage() {
  const dir = path.join(CACHE, 'npm', `${OBS_PACKAGE.name}-${OBS_PACKAGE.version}`);
  const tarball = path.join(dir, `${OBS_PACKAGE.name}-${OBS_PACKAGE.version}.tgz`);
  if (!fs.existsSync(tarball)) {
    fs.mkdirSync(dir, { recursive: true });
    execFileSync('npm', ['pack', `${OBS_PACKAGE.name}@${OBS_PACKAGE.version}`, '--silent', '--pack-destination', dir], { stdio: ['ignore', 'ignore', 'inherit'] });
  }
  const integrity = `sha512-${crypto.createHash('sha512').update(fs.readFileSync(tarball)).digest('base64')}`;
  if (integrity !== OBS_PACKAGE.integrity) throw new Error(`${OBS_PACKAGE.name}: tarball integrity ${integrity}, expected ${OBS_PACKAGE.integrity}`);
  execFileSync('tar', ['xzf', tarball, '-C', dir, `package/${OBS_PACKAGE.file}`]);
  return path.join(dir, 'package', OBS_PACKAGE.file);
}

/** Rows of a TSV as objects keyed by its header. */
function tsvRows(text) {
  const [header, ...lines] = text.split('\n').filter((line) => line.trim());
  const keys = header.split('\t');
  return lines.map((line) => Object.fromEntries(line.split('\t').map((value, i) => [keys[i], value])));
}

// ---- UGNT ------------------------------------------------------------------------

/** "chapter:verse" -> [word, occurrence, occurrences][] for every \w of a verse-object tree. */
function wordsOfChapters(chapters) {
  const out = {};
  for (const [chapter, verses] of Object.entries(chapters)) {
    for (const [verse, { verseObjects }] of Object.entries(verses)) {
      if (verse === 'front') continue;
      const words = [];
      const visit = (objects) => objects.forEach((o) => { if (o.type === 'word') words.push(o.text); if (o.children) visit(o.children); });
      visit(verseObjects);
      out[`${chapter}:${verse}`] = withOccurrences(words);
    }
  }
  return out;
}

function withOccurrences(words) {
  const total = new Map();
  for (const w of words) total.set(w, (total.get(w) ?? 0) + 1);
  const seen = new Map();
  return words.map((w) => {
    seen.set(w, (seen.get(w) ?? 0) + 1);
    return [w, seen.get(w), total.get(w)];
  });
}

/** "chapter:verse" -> the Greek words an aligned USFM book points at (x-content + occurrences). */
function alignedGreek(usfm) {
  const out = {};
  let chapter = null;
  let verse = null;
  for (const m of usfm.matchAll(/\\c (\d+)|\\v (\d+)|x-occurrence="(\d+)" x-occurrences="(\d+)" x-content="([^"]+)"/g)) {
    if (m[1]) chapter = m[1];
    else if (m[2]) verse = m[2];
    else (out[`${chapter}:${verse}`] ??= []).push([m[5], Number(m[3]), Number(m[4])]);
  }
  return out;
}

function buildUgnt() {
  const { entries, source } = bundle('ugnt');
  const chapters = {};
  for (const [name, bytes] of Object.entries(entries)) {
    const m = name.match(/^tit\/(\d+)\.json$/);
    if (m) chapters[m[1]] = JSON.parse(decode(bytes));
  }
  if (Object.keys(chapters).length !== 3) throw new Error(`UGNT bundle: Titus has ${Object.keys(chapters).length} chapters, expected 3`);

  const greek = wordsOfChapters(chapters);
  const aligned = alignedGreek(fs.readFileSync(path.join(TC4, 'test/fixtures/en_ult/TIT.usfm'), 'utf8'));
  const problems = [];
  for (const ref of new Set([...Object.keys(greek), ...Object.keys(aligned)])) {
    const have = new Set((greek[ref] ?? []).map((t) => t.join(' ')));
    const named = new Set((aligned[ref] ?? []).map((t) => t.join(' ')));
    for (const token of named) if (!have.has(token)) problems.push(`${ref}: the ULT aligns ${token}, the UGNT verse lacks it`);
    for (const token of have) if (!named.has(token)) problems.push(`${ref}: the UGNT verse has ${token}, the ULT does not align it`);
  }
  if (problems.length) throw new Error(`UGNT v0.34 disagrees with the ULT v89 alignment of Titus:\n  ${problems.join('\n  ')}`);

  // tC3 keeps no headers; these are the usual UGNT book headers.
  const headers = [
    { tag: 'id', content: 'TIT EL-x-koine_UGNT' },
    { tag: 'usfm', content: '3.0' },
    { tag: 'ide', content: 'UTF-8' },
    { tag: 'h', content: 'Titus' },
    { tag: 'toc1', content: 'Πρὸς Τίτον' },
    { tag: 'toc2', content: 'Πρὸς Τίτον' },
    { tag: 'toc3', content: 'Tit' },
    { tag: 'mt', content: 'Πρὸς Τίτον' },
  ];
  const usfmJs = createRequire(path.join(TC4, 'package.json'))('usfm-js');
  const usfm = usfmJs.toUSFM({ headers, chapters }, { forcedNewLines: true });
  const back = usfmJs.toJSON(usfm).chapters;
  for (const [chapter, verses] of Object.entries(chapters)) {
    for (const verse of Object.keys(verses)) {
      if (JSON.stringify(back[chapter]?.[verse]) !== JSON.stringify(verses[verse])) throw new Error(`UGNT ${chapter}:${verse} does not read back to the bundle's JSON`);
    }
  }
  const checked = Object.values(aligned).reduce((n, list) => n + list.length, 0);
  return {
    files: { 'el-x-koine_ugnt/TIT.usfm': { text: usfm, source: `${source('tit/{1,2,3}.json')} (usfm-js toUSFM)` } },
    note: `UGNT v0.34 Titus: ${Object.keys(greek).length} verses, all agreeing both ways with the ${checked} Greek words the ULT v89 alignment names`,
  };
}

// ---- tW and tA -------------------------------------------------------------------------

function buildTw() {
  const { entries, source } = bundle('tw');
  const twl = fs.readFileSync(path.join(TC4, 'test/fixtures/resources/en_tw@v91/TIT.tsv'), 'utf8');
  const links = new Set(tsvRows(twl).map((row) => row.TWLink).filter(Boolean));
  const files = {};
  const missing = [];
  for (const link of [...links].sort()) {
    // The v91 export links "./payload/<cat>/<slug>.md"; the rc form is the older spelling.
    const m = link.match(/^(?:\.\/payload\/|rc:\/\/\*\/tw\/dict\/bible\/)(kt|names|other)\/([\w-]+?)(?:\.md)?$/);
    if (!m) throw new Error(`en_twl v91 TIT.tsv: unexpected TWLink ${link}`);
    const entry = `${m[1]}/articles/${m[2]}.md`;
    if (!entries[entry]) { missing.push(entry); continue; }
    files[`en_tw/payload/${m[1]}/${m[2]}.md`] = { bytes: entries[entry], source: source(entry) };
  }
  if (missing.length) throw new Error(`en_tw v88 lacks articles the Titus links name: ${missing.join(', ')}`);
  return { files, note: `en_tw v88: ${Object.keys(files).length} articles, every word the en_twl v91 Titus list links` };
}

function buildTa() {
  const { entries, source } = bundle('ta');
  const slugs = new Set();
  for (const tn of ['test/fixtures/resources/en_tn@v91/TIT.tsv', 'test/fixtures/resources/es-419_tn@v66/TIT.tsv']) {
    for (const row of tsvRows(fs.readFileSync(path.join(TC4, tn), 'utf8'))) {
      const m = (row.SupportReference ?? '').match(/^rc:\/\/\*\/ta\/man\/translate\/([\w-]+)$/);
      if (m) slugs.add(m[1]);
    }
  }
  const files = {};
  const missing = [];
  for (const slug of [...slugs].sort()) {
    const entry = `translate/${slug}.md`;
    if (!entries[entry]) { missing.push(entry); continue; }
    const text = decode(entries[entry]);
    const newline = text.indexOf('\n');
    const title = text.slice(0, newline).replace(/^#+\s*/, '').replace(/\s*#+\s*$/, '');
    if (!title) throw new Error(`en_ta v88 ${entry}: no title line`);
    files[`en_ta/translate/${slug}/01.md`] = { text: text.slice(newline + 1), source: `${source(entry)} (after the title line)` };
    files[`en_ta/translate/${slug}/title.md`] = { text: `${title}\n`, source: `${source(entry)} (the title line)` };
  }
  // An article that the release does not carry is a fact about the release (es-419_tn v66 still
  // names translate/figs-inclusive, which en_ta v86 and v88 both lack), so it is recorded, not fatal.
  return {
    files,
    absent: missing,
    note: `en_ta v88: ${slugs.size - missing.length} translate articles, every SupportReference of the en_tn v91 and es-419_tn v66 Titus notes${missing.length ? ` except ${missing.join(', ')}, which v88 lacks` : ''}`,
  };
}

// ---- OBS story 1 -------------------------------------------------------------------------

function buildObs() {
  const file = obsPackage();
  const literal = fs.readFileSync(file, 'utf8').match(/var mdString = ("(?:[^"\\]|\\.)*");/);
  if (!literal) throw new Error(`${OBS_PACKAGE.name}: the example story is not where it was`);
  // The example writes the title line with a trailing space; a story file has none.
  const md = `${JSON.parse(literal[1]).split('\n').map((line) => line.replace(/\s+$/, '')).join('\n').trim()}\n`;
  const frames = md.match(/^!\[OBS Image\]\(https:\/\/cdn\.door43\.org\/obs\/jpg\/360px\/obs-en-01-\d\d\.jpg\)$/gm) ?? [];
  if (!md.startsWith('# 1. The Creation\n') || frames.length !== 16 || !md.trimEnd().endsWith('_A Bible story from: Genesis 1-2_')) {
    throw new Error(`${OBS_PACKAGE.name}: the example is not the whole of story 1`);
  }
  return {
    files: { 'en_obs/01.md': { text: md, source: `npm ${OBS_PACKAGE.name}@${OBS_PACKAGE.version}:${OBS_PACKAGE.file} (mdString)` } },
    note: 'en_obs: story 1 (title, 16 frames, reference)',
  };
}

const parts = [buildUgnt(), buildTw(), buildTa(), buildObs()];
for (const dir of OUTPUTS) fs.rmSync(path.join(HERE, dir), { recursive: true, force: true });
const sources = {};
const absent = parts.flatMap((part) => part.absent ?? []);
for (const part of parts) {
  for (const [rel, { text, bytes, source }] of Object.entries(part.files)) {
    const data = bytes ? Buffer.from(bytes) : Buffer.from(text, 'utf8');
    fs.mkdirSync(path.dirname(path.join(HERE, rel)), { recursive: true });
    fs.writeFileSync(path.join(HERE, rel), data);
    sources[rel] = { md5: md5(data), source };
  }
  console.log(part.note);
}
const sorted = Object.fromEntries(Object.entries(sources).sort(([a], [b]) => a.localeCompare(b)));
fs.writeFileSync(
  path.join(HERE, 'SOURCES.json'),
  `${JSON.stringify({ translationCore3: TC3, bundles: BUNDLES, npm: OBS_PACKAGE, absentFromBundles: absent, files: sorted }, null, 2)}\n`,
);
console.log(`wrote ${Object.keys(sorted).length} files and SOURCES.json under ${path.relative(BASELINE, HERE)}/`);
