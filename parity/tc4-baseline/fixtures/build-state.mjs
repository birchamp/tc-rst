#!/usr/bin/env node
// Assemble the mock Pankosmia server's working state from tC4's own fixtures.
//
//   node fixtures/build-state.mjs [--scenario sample|en-pins] [--tc4 <translationcore4 root>] [--out <state dir>]
//
// The default state dir is .state/<scenario>.
//
// The state dir is rebuilt from scratch on every run (idempotent), so every capture
// starts from the same bytes. Layout, as pankosmia-web keeps it on disk:
//   repos/_local_/_local_/<project>/          the two conformance sample projects
//   repos/_local_/_sideloaded_/<resource>/    helps and source texts, one burrito each
//   client_settings/uw-tc4.json               the install records (D57) tC4 reads
//   provenance.json                           what each resource is made of
//
// Every sideloaded file comes from a tC4 fixture or from fixtures/vendor/, which
// fixtures/vendor/make-vendor.mjs cuts from published copies (translationCore 3's bundled
// resources on GitHub, one npm package) because tC4 vendors no copy (see fixtures/vendor/
// SOURCES.json). A tC4 fixture whose md5 the burrito's own metadata.json records is checked
// against it, so a fixture that drifted fails here. A vendored file of another release than the
// metadata describes gets its md5 and size written into the metadata, and provenance.json
// records both md5s. Resources whose content is not the pinned release, or not the export file
// itself, are marked `substitute` in provenance.json and listed in the README.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BASELINE = path.resolve(HERE, '..');

const arg = (name, fallback) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? path.resolve(process.argv[at + 1]) : fallback;
};
export const DEFAULT_TC4 = process.env.TC4_ROOT ?? '/home/user/unfoldingword/translationcore4';
/** The capture keeps one state dir per scenario: .state/<scenario>. */
export const stateDirFor = (scenario) => path.join(BASELINE, '.state', scenario);

const RES = 'test/fixtures/resources';
/** Files cut from published copies by fixtures/vendor/make-vendor.mjs (see fixtures/vendor/SOURCES.json). */
const VENDOR = path.join(HERE, 'vendor');
/** A file under fixtures/vendor/ (a `files` value; plain strings are paths under the tC4 root). */
const vendored = (rel) => ({ vendor: rel });

// The pins of conformance/sample-burrito and src/data/installedSuite.js at tC4 d494a9d. They
// are the same at d310bf3.
const SHA = {
  en_ult_v91: '35d215957f3203fd2e2fac5702ce14902d417f9d',
  en_ust_v91: '85f274a74245cb418f85266e1a5b524bc3e91e9c',
  en_ta_v91: 'ce9a1bb9431317ca888e8c1f9620caa7f5fe45fd',
  ugnt_v0_34: 'fc95b2b8aad08bb65ab54628ab685413a1139e97',
  en_obs_v9: 'd39a1dc7a7557ac54e4a8fecc3462147fe7eec3b',
};

/**
 * One sideloaded burrito.
 * - dir: the folder under _local_/_sideloaded_/ (bare name, as dev-env/scripts/seed.zsh uses)
 * - record: the install record (repoPath + version); its sha is the metadata revision
 * - metadata: the fixture metadata.json the burrito starts from
 * - files: ingredient path (relative to the burrito) -> fixture file under the tC4 root, or
 *   vendored(<path under fixtures/vendor/>)
 * - vendorTrees: ingredient folder -> folder under fixtures/vendor/ whose files all go there
 *   (a path that `files` also names keeps the `files` source)
 * - rename: the metadata belongs to another resource of the same export family, so its
 *   identity, language, flavor and scope are rewritten and it lists only this burrito's files
 * - substitute: set when the content is not (only) the pinned release's export bytes; `why` says
 *   what it is, and `revision`, when set, overrides the metadata revision so the pin resolves
 */
export const RESOURCES = [
  {
    dir: 'en_ult',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_ult', version: 'v91' },
    metadata: `${RES}/en_ult@v89/metadata.json`,
    files: { 'ingredients/TIT.usfm': `${RES}/en_ult@v89/TIT.usfm` },
    substitute: {
      revision: SHA.en_ult_v91,
      why: 'tC4 vendors en_ult v89 (84c73ba), not the pinned v91; v89 Titus is served under the v91 identity',
    },
  },
  {
    dir: 'en_ust',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_ust', version: 'v91' },
    metadata: `${RES}/en_ult@v89/metadata.json`,
    rename: {
      dcsKey: 'unfoldingWord/en_ust',
      name: 'unfoldingWord® Simplified Text',
      abbreviation: 'UST',
    },
    files: { 'ingredients/TIT.usfm': 'test/fixtures/en_ust/TIT.usfm' },
    substitute: {
      revision: SHA.en_ust_v91,
      why: 'tC4 vendors en_ust v89 Titus (37ec223) with no metadata.json; the metadata is the en_ult v89 export metadata renamed to en_ust, under the v91 identity',
    },
  },
  {
    dir: 'el-x-koine_ugnt',
    record: { repoPath: 'git.door43.org/unfoldingWord/el-x-koine_ugnt', version: 'v0.34' },
    metadata: `${RES}/en_ult@v89/metadata.json`,
    rename: {
      dcsKey: 'unfoldingWord/el-x-koine_ugnt',
      name: 'unfoldingWord® Greek New Testament',
      abbreviation: 'UGNT',
      language: { tag: 'el-x-koine', name: { en: 'Koine Greek' }, scriptDirection: 'ltr' },
    },
    files: { 'ingredients/TIT.usfm': vendored('el-x-koine_ugnt/TIT.usfm') },
    substitute: {
      revision: SHA.ugnt_v0_34,
      why: 'tC4 vendors no Greek New Testament. The text is the pinned release, v0.34, from translationCore 3\'s bundle (verse-object JSON written back to USFM by usfm-js); every word agrees both ways with the ULT v89 alignment of Titus. The file is rebuilt, not the export\'s bytes, and the metadata is the en_ult v89 export metadata renamed',
    },
  },
  {
    dir: 'en_tn',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_tn', version: 'v91' },
    metadata: `${RES}/en_tn@v91/metadata.json`,
    files: { 'ingredients/TIT.tsv': `${RES}/en_tn@v91/TIT.tsv` },
  },
  {
    // en_twl v91's TIT.tsv is byte-identical (md5 d3eaff3f) to the TIT.tsv that the en_tw v91
    // export carries, so the real v91 bytes are available.
    dir: 'en_twl',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_twl', version: 'v91' },
    metadata: `${RES}/en_twl@v91/metadata.json`,
    files: { 'ingredients/TIT.tsv': `${RES}/en_tw@v91/TIT.tsv` },
  },
  {
    // kt/god.md of v87 has the md5 the v91 metadata records, so it is the v91 article. The other
    // articles of the Titus words are v88 (fixtures/vendor/en_tw); provenance.json says which of
    // them match the v91 md5.
    dir: 'en_tw',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_tw', version: 'v91' },
    metadata: `${RES}/en_tw@v91/metadata.json`,
    files: {
      'ingredients/TIT.tsv': `${RES}/en_tw@v91/TIT.tsv`,
      'ingredients/payload/kt/god.md': `${RES}/en_tw@v87/payload-kt-god.md`,
    },
    vendorTrees: { 'ingredients/payload': 'en_tw/payload' },
    substitute: {
      why: 'TIT.tsv and kt/god.md are the v91 bytes; the other 92 articles that the Titus words link are en_tw v88 from translationCore 3\'s bundle; all but kt/evil, kt/good, kt/save and other/courage have the md5 the v91 metadata records (provenance.json)',
    },
  },
  {
    dir: 'en_ta',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_ta', version: 'v91' },
    metadata: `${RES}/en_ta@v86/metadata.json`,
    files: {
      'ingredients/translate/toc.yaml': `${RES}/en_ta@v86/translate-toc.yaml`,
      'ingredients/translate/figs-abstractnouns/01.md': `${RES}/en_ta@v86/translate-figs-abstractnouns-01.md`,
      'ingredients/translate/figs-abstractnouns/title.md': `${RES}/en_ta@v86/translate-figs-abstractnouns-title.md`,
    },
    vendorTrees: { 'ingredients/translate': 'en_ta/translate' },
    substitute: {
      revision: SHA.en_ta_v91,
      why: 'tC4 vendors en_ta v86 (c7caddf: the translate toc and one article), not the pinned v91; the other 30 translate articles that the Titus notes name are v88 from translationCore 3\'s bundle; all served under the v91 identity with the v86 export metadata',
    },
  },
  {
    dir: 'es-419_tn',
    record: { repoPath: 'git.door43.org/es-419_gl/es-419_tn', version: 'v66' },
    metadata: `${RES}/es-419_tn@v66/metadata.json`,
    files: { 'ingredients/TIT.tsv': `${RES}/es-419_tn@v66/TIT.tsv` },
  },
  {
    dir: 'en_obs',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_obs', version: 'v9' },
    metadata: `${RES}/en_obs-tn@v13/metadata.json`,
    rename: {
      dcsKey: 'unfoldingWord/en_obs',
      name: 'unfoldingWord® Open Bible Stories',
      abbreviation: 'OBS',
      flavorType: { name: 'gloss', flavor: { name: 'textStories' } },
    },
    files: { 'ingredients/content/01.md': vendored('en_obs/01.md') },
    substitute: {
      revision: SHA.en_obs_v9,
      why: 'tC4 vendors no English OBS text; story 1 only, from the example story of open-bible-stories@0.0.5 (release unknown), served under the v9 identity; the metadata is the en_obs-tn v13 export metadata renamed',
    },
  },
  {
    dir: 'en_obs-tn',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_obs-tn', version: 'v13' },
    metadata: `${RES}/en_obs-tn@v13/metadata.json`,
    files: { 'ingredients/OBS.tsv': `${RES}/en_obs-tn@v13/OBS.tsv` },
  },
  {
    dir: 'en_obs-twl',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_obs-twl', version: 'v3' },
    metadata: `${RES}/en_obs-twl@v3/metadata.json`,
    files: { 'ingredients/OBS.tsv': `${RES}/en_obs-twl@v3/OBS.tsv` },
  },
  {
    dir: 'en_obs-tq',
    record: { repoPath: 'git.door43.org/unfoldingWord/en_obs-tq', version: 'v10' },
    metadata: `${RES}/en_obs-tq@v10/metadata.json`,
    files: { 'ingredients/OBS.tsv': `${RES}/en_obs-tq@v10/OBS.tsv` },
  },
];

/** Pinned by the sample projects, but no copy was found (README "Incomplete screens"). */
export const ABSENT = [
  'git.door43.org/unfoldingWord/en_tq v91 (the Questions tab of a Bible project)',
  'git.door43.org/es-419_gl/es-419_twl v18, es-419_tw v37, es-419_ta v4 (primary set of sample_burrito)',
  'git.door43.org/es-419_gl/es-419_obs v2, es-419_obs-tn v2, es-419_obs-twl v2 (primary set of sample_burrito_obs)',
  'git.door43.org/unfoldingWord/en_obs v9: stories 2-50 (only story 1 is served)',
  'git.door43.org/uW/obs_images_360 (the OBS pictures)',
  'git.door43.org/unfoldingWord/en_ugl v0.5 and hbo_uhb v3.0.0 / en_uhl (no captured screen reads them)',
  'every resource is a Titus slice: Jonah, the Bible sample\'s second book, has no helps, and only the tW and tA articles that the Titus helps link are served',
  'en_ta translate/figs-inclusive, which es-419_tn v66 names; en_ta v86 and v88 do not carry it either',
];

/** The two local projects: conformance sample folder -> repo folder name (as seed.zsh names them). */
export const PROJECTS = [
  { from: 'conformance/sample-burrito', dir: 'sample_burrito' },
  { from: 'conformance/sample-burrito-obs', dir: 'sample_burrito_obs' },
];

const md5 = (bytes) => crypto.createHash('md5').update(bytes).digest('hex');

/** Every file under `dir`, as `/`-joined paths relative to it. */
function listFiles(dir, rel = '') {
  if (!fs.existsSync(dir)) throw new Error(`${dir} is missing: run \`node fixtures/vendor/make-vendor.mjs\``);
  return fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap((entry) =>
    entry.isDirectory() ? listFiles(path.join(dir, entry.name), `${rel}${entry.name}/`) : [`${rel}${entry.name}`]);
}

function copyTree(from, to) {
  fs.mkdirSync(to, { recursive: true });
  for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
    const src = path.join(from, entry.name);
    const dst = path.join(to, entry.name);
    if (entry.isDirectory()) copyTree(src, dst);
    else fs.copyFileSync(src, dst);
  }
}

function buildResource(tc4, reposRoot, spec) {
  const meta = JSON.parse(fs.readFileSync(path.join(tc4, spec.metadata), 'utf8'));
  const dcs = meta.identification.primary.dcs;
  const [fixtureKey] = Object.keys(dcs);
  const fixtureRevision = dcs[fixtureKey].revision;
  if (spec.rename) {
    meta.identification.primary.dcs = { [spec.rename.dcsKey]: { ...dcs[fixtureKey] } };
    meta.identification.name = { en: spec.rename.name };
    meta.identification.description = { en: spec.rename.name };
    meta.identification.abbreviation = { en: spec.rename.abbreviation };
    // Another language or flavor: the source export's book names do not describe this burrito.
    if (spec.rename.language || spec.rename.flavorType) delete meta.localizedNames;
    if (spec.rename.language) meta.languages = [spec.rename.language];
    if (spec.rename.flavorType) meta.type = { flavorType: spec.rename.flavorType };
  }
  const key = Object.keys(meta.identification.primary.dcs)[0];
  if (spec.substitute?.revision) meta.identification.primary.dcs[key].revision = spec.substitute.revision;
  const out = path.join(reposRoot, '_local_', '_sideloaded_', spec.dir);
  fs.mkdirSync(out, { recursive: true });
  const sources = { ...spec.files };
  for (const [prefix, tree] of Object.entries(spec.vendorTrees ?? {})) {
    for (const rel of listFiles(path.join(VENDOR, tree))) sources[`${prefix}/${rel}`] ??= vendored(`${tree}/${rel}`);
  }
  const files = [];
  for (const [ipath, source] of Object.entries(sources).sort(([a], [b]) => a.localeCompare(b))) {
    const fromVendor = typeof source === 'object';
    const fixture = fromVendor ? `fixtures/vendor/${source.vendor}` : source;
    const bytes = fs.readFileSync(fromVendor ? path.join(VENDOR, source.vendor) : path.join(tc4, source));
    const recorded = meta.ingredients[ipath]?.checksum?.md5 ?? null;
    if (spec.rename || (fromVendor && recorded !== md5(bytes))) {
      // The metadata describes the bytes this burrito actually carries.
      const mimeType = { '.usfm': 'text/plain', '.md': 'text/markdown' }[path.extname(ipath)];
      meta.ingredients[ipath] = { mimeType, ...(meta.ingredients[ipath] ?? {}), checksum: { md5: md5(bytes) }, size: bytes.length };
    } else if (recorded !== md5(bytes)) {
      throw new Error(`${spec.dir}: ${fixture} has md5 ${md5(bytes)}, the metadata records ${recorded} for ${ipath}`);
    }
    fs.mkdirSync(path.dirname(path.join(out, ipath)), { recursive: true });
    fs.writeFileSync(path.join(out, ipath), bytes);
    files.push({ ipath, fixture, md5: md5(bytes), metadataMd5: recorded, matchesMetadata: recorded === md5(bytes) });
  }
  if (spec.rename) {
    // Only the files this burrito actually carries.
    for (const ipath of Object.keys(meta.ingredients)) if (!sources[ipath]) delete meta.ingredients[ipath];
    if (meta.type.flavorType.name === 'scripture') {
      meta.type.flavorType.currentScope = Object.fromEntries(Object.keys(sources)
        .map((ipath) => ipath.match(/^ingredients\/([A-Z0-9]{3})\.usfm$/)?.[1]).filter(Boolean).map((book) => [book, []]));
    }
  }
  fs.writeFileSync(path.join(out, 'metadata.json'), `${JSON.stringify(meta, null, 2)}\n`);
  const sha = meta.identification.primary.dcs[key].revision;
  const ft = meta.type.flavorType;
  return {
    localPath: `_local_/_sideloaded_/${spec.dir}`,
    pin: { ...spec.record, sha, flavor: `${ft.name}/${ft.flavor?.name ?? ''}` },
    provenance: {
      dir: spec.dir,
      ...spec.record,
      sha,
      metadataFrom: spec.metadata,
      fixtureRevision,
      files,
      substitute: spec.substitute ? spec.substitute.why : null,
    },
  };
}

/**
 * Scenario `en-pins`: tC4's own J4 journey setup (e2e/helpers/rig.ts writeProjectPins +
 * e2e/j04-check-book.spec.ts restateRecordAsEnglish), applied to sample_burrito before it
 * is ever opened. The sample pins an es-419 primary set whose TWL/tW/tA are not vendored,
 * so its Translation Words tool reads "Needs downloading"; the journeys pin the installed
 * English suite as both rungs instead, and restate each decision file's checked-against
 * record as the English pin (a drifted record refuses decision writes, D59 §3).
 */
function pinEnglishSuite(projectDir, installed) {
  const pinOf = (dir) => {
    const { repoPath, version, sha, flavor } = installed[`_local_/_sideloaded_/${dir}`];
    return { repoPath, version, sha, flavor };
  };
  const en = { tn: pinOf('en_tn'), twl: pinOf('en_twl'), tw: pinOf('en_tw'), ta: pinOf('en_ta') };
  const file = path.join(projectDir, 'ingredients', 'checking', 'resources.json');
  const existing = JSON.parse(fs.readFileSync(file, 'utf8'));
  const gatewayLanguage = { languageId: 'en', owner: 'unfoldingWord' };
  const languageSets = {};
  for (const rung of ['primary', 'fallback']) {
    const set = {
      gatewayLanguage,
      translationNotes: en.tn,
      translationWordsLinks: en.twl,
      translationWords: en.tw,
      translationAcademy: en.ta,
    };
    // Same-gateway optional slots carry over (writeProjectPins, #123/#124).
    const old = existing.languageSets?.[rung];
    if (old && JSON.stringify(old.gatewayLanguage) === JSON.stringify(gatewayLanguage)) {
      for (const [slot, entry] of Object.entries(old)) if (!(slot in set)) set[slot] = entry;
    }
    languageSets[rung] = set;
  }
  const next = { schemaVersion: 2, languageSets };
  for (const [key, value] of Object.entries(existing)) if (key !== 'schemaVersion' && key !== 'languageSets') next[key] = value;
  fs.writeFileSync(file, `${JSON.stringify(next, null, 2)}\n`);
  for (const [tool, pin] of [['translationNotes', en.tn], ['translationWords', en.twl]]) {
    const decisions = path.join(projectDir, 'ingredients', 'checking', tool, 'TIT.json');
    if (!fs.existsSync(decisions)) continue;
    const doc = JSON.parse(fs.readFileSync(decisions, 'utf8'));
    doc.resource = { repoPath: pin.repoPath, version: pin.version, sha: pin.sha, languageSet: 'primary' };
    fs.writeFileSync(decisions, `${JSON.stringify(doc, null, 2)}\n`);
  }
}

/**
 * Scenario `en-pins`, OBS half: the OBS sample's primary set becomes its own English
 * fallback set (the complete English OBS suite tC4 ships, src/data/installedSuite.js),
 * which is how tC4 pins a project it creates (primary === fallback until a gateway
 * language is chosen). The sample has no decision files to restate.
 */
function pinEnglishObsSuite(projectDir) {
  const file = path.join(projectDir, 'ingredients', 'checking', 'resources.json');
  const doc = JSON.parse(fs.readFileSync(file, 'utf8'));
  doc.languageSets.primary = JSON.parse(JSON.stringify(doc.languageSets.fallback));
  fs.writeFileSync(file, `${JSON.stringify(doc, null, 2)}\n`);
}

export const SCENARIOS = {
  sample: 'the conformance samples exactly as tC4 ships them (es-419 primary set, English fallback)',
  'en-pins': 'both samples pinned to the installed English suite on both rungs (sample_burrito: tC4 J4 journey setup, decision records restated as English; sample_burrito_obs: primary set = its English fallback set)',
};

export function buildState({ tc4 = DEFAULT_TC4, scenario = 'sample', out = stateDirFor(scenario) } = {}) {
  if (!SCENARIOS[scenario]) throw new Error(`unknown scenario ${scenario}`);
  fs.rmSync(out, { recursive: true, force: true });
  const repos = path.join(out, 'repos');
  for (const project of PROJECTS) {
    copyTree(path.join(tc4, project.from), path.join(repos, '_local_', '_local_', project.dir));
  }
  const installed = {};
  const provenance = [];
  for (const spec of RESOURCES) {
    const built = buildResource(tc4, repos, spec);
    installed[built.localPath] = built.pin;
    provenance.push(built.provenance);
  }
  if (scenario === 'en-pins') {
    pinEnglishSuite(path.join(repos, '_local_', '_local_', 'sample_burrito'), installed);
    pinEnglishObsSuite(path.join(repos, '_local_', '_local_', 'sample_burrito_obs'));
  }
  fs.mkdirSync(path.join(out, 'client_settings'), { recursive: true });
  fs.writeFileSync(
    path.join(out, 'client_settings', 'uw-tc4.json'),
    `${JSON.stringify({ installedResources: installed }, null, 2)}\n`,
  );
  fs.mkdirSync(path.join(out, 'vrs'), { recursive: true });
  for (const name of fs.readdirSync(path.join(tc4, 'test/fixtures/vrs'))) {
    if (name.endsWith('.json')) fs.copyFileSync(path.join(tc4, 'test/fixtures/vrs', name), path.join(out, 'vrs', name));
  }
  fs.writeFileSync(
    path.join(out, 'provenance.json'),
    `${JSON.stringify({ tc4, scenario, projects: PROJECTS, resources: provenance, absent: ABSENT }, null, 2)}\n`,
  );
  return { out, installed, provenance };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const at = process.argv.indexOf('--scenario');
  const scenario = at > 0 ? process.argv[at + 1] : 'sample';
  const { out, provenance } = buildState({ tc4: arg('--tc4', DEFAULT_TC4), out: arg('--out', stateDirFor(scenario)), scenario });
  for (const p of provenance) console.log(`sideloaded ${p.dir} ${p.version} ${p.sha.slice(0, 7)}${p.substitute ? '  (substitute)' : ''}`);
  console.log(`state: ${out}`);
}
