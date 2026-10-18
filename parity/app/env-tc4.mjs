// The tC4 environment of the driver: the tC4 client, built from a tC4 checkout, against the
// Node mock of the Pankosmia API (parity/tc4-baseline/mock-server), on a state directory built
// from tC4's fixtures (parity/tc4-baseline/.state/<scenario>), with Door43 faked inside the
// browser context.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildState, DEFAULT_TC4 } from '../tc4-baseline/fixtures/build-state.mjs';
import { routeFakeDoor43 } from '../tc4-baseline/mock-server/fake-door43.mjs';
import { startMockServer } from '../tc4-baseline/mock-server/server.mjs';

const BASELINE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', 'tc4-baseline');
export const TC4 = path.resolve(process.env.TC4_ROOT ?? DEFAULT_TC4);
/** The built tC4 client, which the mock server serves at /clients/uw-tc4/. */
export const DIST = path.join(BASELINE, '.cache', 'tc4-dist');
/** The teammates of tC4's e2e/sync-button.spec.ts. */
const TEAMMATES = ['b678teammate0001', 'c678teammate0002'];

// ---- the tC4 client build ---------------------------------------------------

// GIT_OPTIONAL_LOCKS=0: `git status` must not refresh the index of a checkout that is
// shared and read-only.
const git = (...args) => execFileSync('git', ['-C', TC4, ...args], { encoding: 'utf8', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } }).trim();

export function tc4Identity() {
  const pkg = JSON.parse(fs.readFileSync(path.join(TC4, 'package.json'), 'utf8'));
  return {
    root: TC4,
    commit: git('rev-parse', 'HEAD'),
    commitDate: git('show', '-s', '--format=%cI', 'HEAD'),
    version: pkg.version,
    dirty: git('status', '--porcelain', '--untracked-files=no') !== '',
  };
}

/** Build the client unless this commit's build (with its help pages) is already there.
 * Returns whether the build carries the help pages. */
export function buildClient(commit, { rebuild = false } = {}) {
  const stampFile = path.join(DIST, '.tc4-build.json');
  const stamp = fs.existsSync(stampFile) ? JSON.parse(fs.readFileSync(stampFile, 'utf8')) : null;
  if (!rebuild && stamp?.commit === commit && stamp.helpPages) return true;
  if (!fs.existsSync(path.join(TC4, 'node_modules', 'vite'))) {
    console.log(`tC4 dependencies: npm ci in ${TC4}`);
    execFileSync('npm', ['ci', '--no-audit', '--no-fund'], { cwd: TC4, stdio: 'inherit' });
  }
  fs.mkdirSync(path.dirname(DIST), { recursive: true });
  const log = path.join(BASELINE, '.cache', 'vite-build.log');
  const out = () => ['ignore', fs.openSync(log, 'a'), fs.openSync(log, 'a')];
  fs.writeFileSync(log, '');
  // tC4's own prebuild step (scripts/fetch-help.mjs): the help pages of its pinned tc-website
  // commit into public/help/, which tC4 git-ignores. It needs GitHub; without it the build
  // goes on without the pages, as a build with no network would.
  let helpPages = true;
  try {
    execFileSync('node', ['scripts/fetch-help.mjs'], { cwd: TC4, stdio: out() });
  } catch {
    helpPages = false;
    console.log('tC4 help pages: not fetched (see .cache/vite-build.log); the help screen will show their absence');
  }
  console.log(`tC4 client: vite build at ${commit.slice(0, 7)} -> ${path.relative(BASELINE, DIST)}`);
  execFileSync('npx', ['vite', 'build', '--outDir', DIST, '--emptyOutDir'], { cwd: TC4, stdio: out() });
  fs.writeFileSync(stampFile, `${JSON.stringify({ commit, helpPages })}\n`);
  return helpPages;
}

/** tC4's own count for the toast's note on missing resource versions
 * (src/data/installed.ts `missingPinCount`), bundled with tC4's esbuild. */
export async function tc4MissingPinCount() {
  const out = path.join(BASELINE, '.cache', 'tc4-installed.mjs');
  execFileSync(path.join(TC4, 'node_modules', '.bin', 'esbuild'),
    [path.join(TC4, 'src', 'data', 'installed.ts'), '--bundle', '--format=esm', '--platform=node', `--outfile=${out}`, '--log-level=warning']);
  return (await import(pathToFileURL(out).href)).missingPinCount;
}

/** The pinned versions of the project at `repo` that the note counts: each pin that
 * `missingPinCount` counts on its own, over the install records in `state`. */
export function missingVersionsOf(missingPinCount, state, repo) {
  const json = (...parts) => JSON.parse(fs.readFileSync(path.join(state, ...parts), 'utf8'));
  const pins = json('repos', ...repo.split('/'), 'ingredients', 'checking', 'resources.json');
  const installed = json('client_settings', 'uw-tc4.json').installedResources;
  // the pins missingPinCount reads, in its order
  const all = [
    ...Object.values(pins.languageSets ?? {}).flatMap((set) => Object.values(set ?? {})),
    ...Object.values(pins.resources?.originalLanguage ?? {}),
    ...(pins.extraScripture ?? []),
  ];
  const missing = all.filter((pin) => missingPinCount({ extraScripture: [pin] }, installed) === 1);
  if (missing.length !== missingPinCount(pins, installed)) throw new Error(`the missing pins of ${repo} do not add up to tC4's count`);
  return missing.map((pin) => `${pin.repoPath} ${pin.version}`);
}

// ---- Door43 -----------------------------------------------------------------

/**
 * Two teammates change Titus 1:1 from the same version on Door43 `main`, which gives one
 * verse conflict. This is tC4's e2e/sync-button.spec.ts `teammatesChangeOneVerse`: the
 * segments are written by tC4's own reference journal writers, never by hand. Answers the
 * segment file of each teammate, in the order of TEAMMATES.
 */
async function teammatesChangeOneVerse(main) {
  const journal = (file) => import(pathToFileURL(path.join(TC4, 'journal', file)).href);
  const [{ makeClock }, { fold }, { actorDirFor, readUnion, writeActionSegment }] = await Promise.all(
    ['hlc.mjs', 'fold.mjs', 'files.mjs'].map(journal));
  const journalDir = path.join(main, 'ingredients', 'checking', 'journal');
  const union = readUnion(journalDir);
  const base = fold(union).headsTs['text|TIT|1:1'];
  if (!base) throw new Error('Door43 main holds no version of TIT 1:1: open the project before it is pushed');
  const last = union.map((e) => e.ts).sort().pop();
  let now = Date.now();
  return TEAMMATES.map((actor, i) => {
    const clock = makeClock(actor, () => now);
    clock.ratchet(last);
    now += 1000;
    const event = { v: 1, op: 'text.verse.set', actor, ts: clock.issue(), base, book: 'TIT', chapter: '1', verse: '1', text: `Pablo, versión del compañero ${i + 1}.\n` };
    return writeActionSegment(actorDirFor(journalDir, actor), [event]);
  });
}

/** Fold reads that differ only in a story number (Home reads every gateway story title) into
 * one line, so the manifest stays readable. */
function compactReads(reads) {
  const groups = new Map();
  for (const read of reads) {
    const key = read.replace(/content\/\d\d\.md/, 'content/NN.md');
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(read);
  }
  return [...groups].map(([key, list]) => {
    if (list.length < 3) return list;
    const numbers = list.map((read) => read.match(/content\/(\d\d)\.md/)[1]);
    return [`${key.replace('NN', `{${numbers[0]}..${numbers.at(-1)}}`)} (${list.length} reads)`];
  }).flat();
}

// ---- the environment --------------------------------------------------------

/**
 * Builds `.state/<scenario>` from the tC4 checkout's fixtures and starts the mock server on
 * `port` over it. Answers the AppEnv of driver.mjs, plus what the capture needs: the
 * routes of a browser context, the requests a screenshot waits for, and the reads that came
 * back absent.
 */
export async function startTc4Env({ scenario, port }) {
  const state = path.join(BASELINE, '.state', scenario);
  buildState({ tc4: TC4, out: state, scenario });
  const mock = await startMockServer({ state, client: DIST, port });
  const settingsFile = path.join(state, 'client_settings', 'uw-tc4.json');
  const pristineSettings = fs.readFileSync(settingsFile);
  const external = new Set();
  return {
    state,
    origin: mock.origin,
    /** Door43 is faked inside the context. Nothing else may leave the browser: any other
     * origin is refused and reported (`refused`). */
    async prepare(context) {
      await routeFakeDoor43(context, { door43: mock.door43 });
      await context.route((url) => url.origin !== mock.origin && url.origin !== 'https://git.door43.org', (route) => {
        external.add(route.request().url());
        return route.abort('blockedbyclient');
      });
    },
    tracks: (url) => new URL(url).pathname.startsWith('/api/'),
    catalog: (locale) => JSON.parse(fs.readFileSync(path.join(TC4, 'src', 'i18n', `${locale}.json`), 'utf8')),
    /** The per-client document goes back to its seeded form (install records only); another
     * `locale` is saved as the app language first, the way tC4 saves it (`appLocale`,
     * state.jsx readSavedLocale). */
    async load(page, { locale }) {
      fs.writeFileSync(settingsFile, locale === 'en' ? pristineSettings
        : `${JSON.stringify({ ...JSON.parse(pristineSettings), appLocale: locale }, null, 2)}\n`);
      await page.goto(`${mock.origin}/clients/uw-tc4/`);
    },
    projectId: (repo) => repo,
    /** tC4 seeds the journal of a conformance sample when it first opens it, so the project
     * must be opened before it is pushed, as tC4's e2e/sync-button.spec.ts opens it before it
     * shares it. Without the journal on Door43, the sync stops at sync.verify-failed. */
    pushToDoor43: (repo) => mock.door43.push(repo),
    dropNextClone: (repo) => mock.door43.dropNextClone(repo),
    teammatesChangeOneVerse: (repo) => teammatesChangeOneVerse(mock.door43.push(repo)),
    /** Git keeps no empty folder, so the folders the segment leaves empty go too. */
    loseFromDoor43: (repo, segment) => {
      const journal = path.join(mock.door43.mainOf(repo), 'ingredients', 'checking', 'journal');
      if (!segment.startsWith(`${journal}${path.sep}`)) throw new Error(`${segment} is not a journal file of Door43 main`);
      fs.rmSync(segment);
      for (let dir = path.dirname(segment); dir !== journal && fs.readdirSync(dir).length === 0; dir = path.dirname(dir)) fs.rmdirSync(dir);
    },
    /** A mark in the mock's request log, for `absentReads`. */
    requestMark: () => mock.requests.length,
    /** The reads since `mark` that came back absent (HTTP 400 and up), journal reads left out,
     * sorted: concurrent reads finish in a different order from run to run. */
    absentReads: (mark) => compactReads([...new Set(mock.requests.slice(mark)
      .filter((r) => r.status >= 400 && !/checking(\/|%2F)journal/.test(`${r.path}${r.query}`))
      .map((r) => `${r.method} ${decodeURIComponent(r.path.replace(/^\/api/, ''))}${decodeURIComponent(r.query)} -> ${r.status}`))].sort()),
    refused: () => [...external],
    close: () => mock.close(),
  };
}
