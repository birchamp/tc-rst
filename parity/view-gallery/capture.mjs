#!/usr/bin/env node
// Captures the gallery: builds it with the tC4 clone's toolchain (build.mjs), calibrates every
// recorded state against real tC4's own capture of it, then draws every case of cases.mjs and
// photographs it with the baseline's capture rules (parity/app/observe.mjs).
//
//   TC4_ROOT=<tC4 clone with node_modules> node parity/view-gallery/capture.mjs
//        [--only <case>,...] [--no-build] [--out <dir>] [--check]
//
// Writes screens/<case>.png, dom/<case>.json, actions/<case>.json and calibration.json here,
// parity/app/cases.json (the gate's list of cases) and the rectangles of the cases' masks in
// parity/app/masks.json. `--out <dir>` writes all of it into <dir> instead (cases.json and
// masks.json too) and leaves the repository alone; `--check` captures into a temporary folder
// and compares every file with the committed one byte for byte (only the DOM files, traces and
// cases' units when the machine is not the one that captured them). A case fails, and nothing
// is written, on a page error, a console error, the view-error callout, a request, a step that
// fails, or an empty #root. A state that does not draw exactly as tC4 drew it fails the run.
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { CHROMIUM_ARGS, CONTEXT_OPTIONS, POINTER_REST, captureScreen, machineFingerprint, measureTargets, trackRequests } from '../app/observe.mjs';
import { repoGalleryGroups } from '../app/units.mjs';
import { DIST, buildGallery, requireTc4, tc4Build } from './build.mjs';
import { CASES, caseData, caseSteps } from './cases.mjs';
import { domDigestInput } from './lib.mjs';
import { HERE, REPO, filesUnder, prettyJson, readState, sha256 } from './node-lib.mjs';
import { loadHarnessPlaywright } from './playwright.mjs';
import { loadPatchKit } from './patch-kit.mjs';
import { stateNamed } from './states.mjs';
import { runSteps, still } from './steps.mjs';

const BASE = '/clients/uw-tc4/';
const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json',
  '.woff2': 'font/woff2', '.woff': 'font/woff', '.ttf': 'font/ttf', '.png': 'image/png', '.svg': 'image/svg+xml',
  '.jpg': 'image/jpeg', '.ico': 'image/x-icon', '.md': 'text/markdown',
};
/** The tC4 repo paths of the two projects, for mask targets. */
const PROJECTS = { '<BIBLE>': '_local_/_local_/sample_burrito', '<OBS>': '_local_/_local_/sample_burrito_obs' };

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};

/** Serves `dist` under tC4's base, and nothing else, on a free local port. */
function serve(dist) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://gallery');
    if (!url.pathname.startsWith(BASE)) return res.writeHead(404).end();
    const rel = decodeURIComponent(url.pathname.slice(BASE.length)) || 'index.html';
    const file = path.join(dist, rel);
    if (!file.startsWith(dist) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) return res.writeHead(404).end();
    res.writeHead(200, { 'content-type': MIME[path.extname(file)] ?? 'application/octet-stream' });
    return fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

/**
 * Draws `data` ({ s, returns, locale }) in a fresh context and photographs it into `file`
 * after `steps`. Answers the DOM, the action trace, the masks' rectangles and every failure.
 */
async function draw(browser, origin, { data, steps = [], file, masks = [] }) {
  const context = await browser.newContext(CONTEXT_OPTIONS);
  const failures = [];
  await context.route((url) => url.origin !== origin, (route) => {
    failures.push(`a request to ${route.request().url()}`);
    return route.abort('blockedbyclient');
  });
  try {
    const page = await context.newPage();
    page.on('pageerror', (error) => failures.push(`page error: ${error.message.split('\n')[0]}`));
    page.on('console', (message) => {
      if (message.type() === 'error') failures.push(`console error: ${message.text().slice(0, 300)}`);
    });
    const inFlight = await trackRequests(page, () => true);
    await page.goto(`${origin}${BASE}`);
    await page.waitForFunction(() => window.__galleryReady === true);
    // The full-window pointer layer takes the click, as the screen in tC4 was opened by one,
    // so focus a view moves on mount is not drawn as keyboard focus (README, "Focus").
    await page.mouse.click(POINTER_REST.x, POINTER_REST.y);
    const screen = {
      name: 'gallery',
      run: async () => {
        await page.evaluate((d) => window.__mount(d), data);
        await page.waitForFunction(() => window.__galleryMounted === true);
        await still(page, 300);
        await runSteps(page, steps, (i) => page.evaluate((phase) => { window.__galleryPhase = phase; }, `step ${i + 1}`));
      },
    };
    const { error, dom } = await captureScreen({ page, app: null, screen, inFlight, file, failureTestIds: ['view-error'] });
    if (error) failures.push(error);
    const empty = await page.evaluate(() => document.getElementById('root').childElementCount === 0);
    if (empty) failures.push('#root is empty: nothing was drawn');
    const calls = await page.evaluate(() => window.__galleryCalls);
    const rects = masks.length ? await measureTargets(page, masks) : {};
    return { dom, calls, rects, failures };
  } finally {
    await context.close();
  }
}

/** The bytes of a file as the gallery writes it. */
const write = (file, text) => {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
};

async function capture({ only, build, out }) {
  const tc4 = requireTc4();
  if (build) await buildGallery({ tc4 });
  if (!fs.existsSync(path.join(DIST, 'index.html'))) throw new Error('the gallery is not built: run without --no-build');
  const identity = tc4Build(tc4);
  const groups = repoGalleryGroups(REPO);
  const kit = await loadPatchKit(tc4);
  // Each state is read once: patches never change what they read (a patch is a new object).
  const cache = new Map();
  const recorded = (name) => {
    if (!cache.has(name)) cache.set(name, readState(name));
    return cache.get(name);
  };
  const index = JSON.parse(fs.readFileSync(path.join(HERE, 'states.json'), 'utf8'));
  const masksDoc = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'app', 'masks.json'), 'utf8'));
  const selected = CASES.filter((c) => !only || only.includes(c.name));
  const unknown = (only ?? []).filter((n) => !CASES.some((c) => c.name === n));
  if (unknown.length) throw new Error(`no such cases: ${unknown.join(', ')}`);

  const server = await serve(DIST);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const { chromium } = loadHarnessPlaywright();
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const machine = machineFingerprint(browser.version());
  const failed = [];
  const calibration = [];
  const results = [];
  const shots = path.join(HERE, '.cache', 'calibration');
  fs.mkdirSync(shots, { recursive: true });
  const timing = { states: [], cases: [] };
  try {
    // Calibration: each recorded state, unpatched, against real tC4's capture of it.
    for (const entry of index.states) {
      const started = Date.now();
      const state = stateNamed(entry.name);
      const { s, returns } = recorded(entry.name);
      process.stdout.write(`  state ${entry.name} ... `);
      const file = path.join(shots, `${entry.name}.png`);
      const drawn = await draw(browser, origin, { data: { s, returns, locale: entry.locale }, steps: state.focus ?? [], file });
      fs.writeFileSync(path.join(shots, `${entry.name}.dom.json`), prettyJson(drawn.dom));
      const png = sha256(fs.readFileSync(file));
      const dom = sha256(domDigestInput(drawn.dom));
      const equal = png === entry.real.png && dom === entry.real.dom;
      calibration.push({ name: entry.name, png, dom, equal });
      timing.states.push(Date.now() - started);
      if (drawn.failures.length) failed.push(`state ${entry.name}: ${drawn.failures[0]}`);
      else if (!equal) failed.push(`state ${entry.name}: the gallery does not draw it as tC4 did (${png !== entry.real.png ? 'pixels' : ''}${png !== entry.real.png && dom !== entry.real.dom ? ' and ' : ''}${dom !== entry.real.dom ? 'DOM' : ''} differ; compare .cache/calibration/${entry.name}.png with .cache/record/${entry.name}.png)`);
      console.log(drawn.failures.length ? `FAILED: ${drawn.failures[0]}` : equal ? 'calibrated' : 'DIFFERS from tC4');
    }
    // The cases.
    const scratch = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-gallery-'));
    for (const c of selected) {
      const started = Date.now();
      process.stdout.write(`  ${c.name} ... `);
      const data = caseData(c, recorded(c.state), (name) => recorded(name).s, kit);
      const masks = (c.masks ?? []).map((id) => {
        const mask = masksDoc.masks.find((m) => m.id === id);
        if (!mask) throw new Error(`case ${c.name} names the mask ${id}, which masks.json does not have`);
        return { id, target: Object.fromEntries(Object.entries(mask.target).map(([k, v]) => [k, Object.entries(PROJECTS).reduce((t, [a, b]) => t.replaceAll(a, b), v)])) };
      });
      const file = path.join(scratch, `${c.name}.png`);
      const drawn = await draw(browser, origin, { data, steps: caseSteps(c), file, masks });
      for (const m of masks) if (!drawn.rects[m.id]) drawn.failures.push(`the element of mask ${m.id} is not on the page`);
      timing.cases.push(Date.now() - started);
      if (drawn.failures.length) {
        failed.push(`${c.name}: ${drawn.failures.join('; ')}`);
        console.log(`FAILED: ${drawn.failures[0]}`);
        continue;
      }
      results.push({ c, patchKeys: data.patchKeys, png: fs.readFileSync(file), dom: drawn.dom, calls: drawn.calls, rects: drawn.rects });
      console.log('ok');
    }
    fs.rmSync(scratch, { recursive: true, force: true });
  } finally {
    await browser.close();
    server.close();
  }
  if (failed.length) {
    console.log(`\n${failed.length} failed; nothing written:\n  ${failed.join('\n  ')}`);
    return { ok: false, timing };
  }

  // Write.
  const root = out ?? HERE;
  const record = {
    calibration: prettyJson({
      about: 'Calibration of the tC4 view-state gallery (APP-DESIGN §7.4): each recorded state of states.json drawn by the gallery with no patch, the sha256 of its PNG and of its DOM digest (lib.mjs domDigestInput), and whether both equal what real tC4 showed when the state was recorded (`real` in states.json). Every state must be equal, which shows the stub adds nothing: a case differs from tC4 only by its patch and steps.',
      machine,
      states: calibration,
    }),
  };
  const previous = !out && fs.existsSync(path.join(REPO, 'parity', 'app', 'cases.json'))
    ? JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'app', 'cases.json'), 'utf8')) : null;
  const byName = new Map((only ? previous?.cases ?? [] : []).map((e) => [e.name, e]));
  for (const { c, patchKeys, png, dom, calls } of results) {
    const group = groups.find((g) => g.group === c.group);
    if (!group) throw new Error(`case ${c.name}: group ${c.group} is not in §7.4`);
    const files = {
      png: [`screens/${c.name}.png`, png],
      dom: [`dom/${c.name}.json`, prettyJson({ screen: c.name, ...dom })],
      actions: [`actions/${c.name}.json`, prettyJson({ case: c.name, calls })],
    };
    for (const [rel, bytes] of Object.values(files)) write(path.join(root, rel), bytes);
    const state = stateNamed(c.state);
    byName.set(c.name, {
      name: c.name,
      group: c.group,
      state: c.state,
      scenario: state.scenario,
      locale: state.locale,
      viewport: null,
      patch: patchKeys,
      png: files.png[0],
      sha256: sha256(files.png[1]),
      dom: files.dom[0],
      domSha256: sha256(files.dom[1]),
      actions: files.actions[0],
      actionsSha256: sha256(files.actions[1]),
      domUnit: group.dom,
      pixelsUnit: group.pixels,
      scope: c.scope ?? null,
    });
  }
  const cases = {
    about: 'The cases of the tC4 view-state gallery (APP-DESIGN §7.4), written by parity/view-gallery/capture.mjs: each case\'s tC4 screenshot, DOM and action trace under parity/view-gallery, the recorded state it starts from, the top-level keys of tC4\'s state its patch sets (none when tC4 drew the recorded state itself), its §7.4 group with the units that capture its Rust side, and the scope a DOM-first case compares. `machine` captured every case: a case\'s pixels compare only with a Rust capture of the same machine. Each case is a row of report.json.',
    tc4: { commit: identity.commit },
    machine,
    cases: CASES.map((c) => byName.get(c.name)).filter(Boolean),
  };
  record.cases = prettyJson(cases);
  // The masks' rectangles of the cases, measured after the steps.
  const masksOut = structuredClone(masksDoc);
  for (const mask of masksOut.masks) {
    for (const name of Object.keys(mask.rects)) {
      const c = CASES.find((x) => x.name === name);
      if (c && !(c.masks ?? []).includes(mask.id)) delete mask.rects[name];
    }
    for (const r of results) if ((r.c.masks ?? []).includes(mask.id)) mask.rects[r.c.name] = r.rects[mask.id];
  }
  record.masks = prettyJson(masksOut);
  write(path.join(root, 'calibration.json'), record.calibration);
  write(out ? path.join(out, 'cases.json') : path.join(REPO, 'parity', 'app', 'cases.json'), record.cases);
  write(out ? path.join(out, 'masks.json') : path.join(REPO, 'parity', 'app', 'masks.json'), record.masks);
  if (!only) {
    const named = new Set(cases.cases.flatMap((e) => [e.png, e.dom, e.actions]));
    for (const dir of ['screens', 'dom', 'actions']) {
      for (const rel of filesUnder(path.join(root, dir))) if (!named.has(`${dir}/${rel}`)) fs.rmSync(path.join(root, dir, rel));
    }
  }
  console.log(`${calibration.length} states calibrated, ${results.length} cases captured into ${path.relative(process.cwd(), root) || '.'}`);
  return { ok: true, timing, machine };
}

/** Compares a capture in `dir` with the committed record, byte for byte; answers the
 * committed files that differ. */
export function compareWithCommitted(dir, machine) {
  const committed = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'app', 'cases.json'), 'utf8'));
  const sameMachine = JSON.stringify(committed.machine) === JSON.stringify(machine);
  const pairs = [[path.join(dir, 'cases.json'), path.join(REPO, 'parity', 'app', 'cases.json')]];
  if (sameMachine) {
    pairs.push([path.join(dir, 'calibration.json'), path.join(HERE, 'calibration.json')]);
    pairs.push([path.join(dir, 'masks.json'), path.join(REPO, 'parity', 'app', 'masks.json')]);
  } else {
    console.log('this machine is not the one that captured the record: comparing the DOM files and the traces only');
  }
  for (const sub of sameMachine ? ['screens', 'dom', 'actions'] : ['dom', 'actions']) {
    const mine = filesUnder(path.join(dir, sub));
    const theirs = filesUnder(path.join(HERE, sub));
    for (const rel of new Set([...mine, ...theirs])) pairs.push([path.join(dir, sub, rel), path.join(HERE, sub, rel)]);
  }
  const differ = [];
  for (const [a, b] of pairs) {
    if (!sameMachine && a.endsWith('cases.json')) {
      const strip = (doc) => JSON.stringify(JSON.parse(doc).cases.map(({ sha256: _p, machine: _m, ...rest }) => rest));
      if (!fs.existsSync(a) || strip(fs.readFileSync(a, 'utf8')) !== strip(fs.readFileSync(b, 'utf8'))) differ.push(path.relative(REPO, b));
      continue;
    }
    if (!fs.existsSync(a) || !fs.existsSync(b) || !fs.readFileSync(a).equals(fs.readFileSync(b))) differ.push(path.relative(REPO, b));
  }
  return differ;
}

async function main() {
  const only = argValue('--only')?.split(',').filter(Boolean);
  const build = !process.argv.includes('--no-build');
  if (process.argv.includes('--check')) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-gallery-check-'));
    try {
      const run = await capture({ only: null, build, out: dir });
      if (!run.ok) process.exitCode = 1;
      else {
        const differ = compareWithCommitted(dir, run.machine);
        console.log(differ.length ? `${differ.length} files differ from the committed record:\n  ${differ.join('\n  ')}` : 'the capture equals the committed record');
        if (differ.length) process.exitCode = 1;
      }
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
    return;
  }
  const out = argValue('--out') ? path.resolve(argValue('--out')) : null;
  const run = await capture({ only, build, out });
  if (!run.ok) process.exitCode = 1;
  const median = (list) => (list.length ? [...list].sort((a, b) => a - b)[Math.floor(list.length / 2)] : 0);
  console.log(`median per state ${median(run.timing.states)} ms, per case ${median(run.timing.cases)} ms`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.join(HERE, 'capture.mjs')) await main();

