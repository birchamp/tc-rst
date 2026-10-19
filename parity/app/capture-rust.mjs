#!/usr/bin/env node
// Capture the Rust app's screens with the driver, the screen list and the capture rules that
// captured tC4's (parity/tc4-baseline/capture.mjs), over `tc4 serve` (env-rust.mjs).
//
//   node parity/app/capture-rust.mjs --unit <A5|B2|...> (--only a,b,... | --all | --case c,d,...)
//        [--layers pixels,dom,meaning] [--backend memory|memory+scripted|store]
//        [--tc4 target/debug/tc4] [--ui target/tc-ui-app] [--out parity/app]
//
// Needs `tc4` built with `--features test-hooks` and the UI folder of crates/tc-ui/build-app.sh.
// `--unit` names the unit of docs/APP-DESIGN.md §10 that captures (the gate checks it has
// landed); `--layers` the layers the capture claims (`dom` alone is a DOM-first capture of a
// screen whose pixels come later, APP-DESIGN 7.6). `--case` captures cases of the tC4
// view-state gallery (parity/view-gallery/cases.mjs, APP-DESIGN 7.4), which have no meaning
// layer: each through its `rust` route, or, for a case with no patch and no replies on a state
// whose route uses only the driver, that route and the case's steps.
//
// Writes into `--out`: screens/<name>.png, dom/<name>.json and captures.json, whose entries are
// merged by name, screens in the order of the screen list and then cases in the order of
// cases.mjs, so a partial run updates its entries in place and keeps every other.
// Then: UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record, and review the report.
// Exit code 1 when a screen failed or a request to another origin was refused.
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { appDriver } from './driver.mjs';
import { REPO, startRustEnv } from './env-rust.mjs';
import { CHROMIUM_ARGS, captureScreen, machineFingerprint, measureTargets, trackRequests } from './observe.mjs';
import { BIBLE, OBS, SCREEN_LIST } from './screens.mjs';
import { repoUnits } from './units.mjs';
import { CASES } from '../view-gallery/cases.mjs';
import { stateNamed } from '../view-gallery/states.mjs';
import { runSteps } from '../view-gallery/steps.mjs';

const LAYERS = ['pixels', 'dom', 'meaning'];
const BACKENDS = ['memory', 'memory+scripted', 'store'];

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

/** Playwright from the baseline's own install: parity/app has no node_modules. */
export function loadPlaywright() {
  const dir = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
  try {
    return createRequire(import.meta.url)(dir);
  } catch (error) {
    throw new Error(`Playwright 1.56.1 is not at ${dir}: npm ci in parity/tc4-baseline, or set PLAYWRIGHT_MODULE (${error.code ?? error.message})`);
  }
}

/** The masks of `screen` with their targets on the Rust side: `rust` when the mask names one,
 * else tC4's `target`, with `<OBS>` and `<BIBLE>` resolved to the scenario's folders. */
export function rustMaskTargets(masks, screen, projectId) {
  const resolve = (value) => value.replaceAll('<OBS>', projectId(OBS)).replaceAll('<BIBLE>', projectId(BIBLE));
  return masks.filter((mask) => mask.rects[screen]).map((mask) => {
    const target = mask.rust ?? mask.target;
    return { id: mask.id, target: Object.fromEntries(Object.entries(target).map(([k, v]) => [k, resolve(v)])) };
  });
}

/**
 * The entries of captures.json after a run: every entry of `previous` and `captured`, a
 * captured one replacing the previous one of its kind and name, screens in the order of
 * `screenList` and then cases in the order of `cases`. A run of screens keeps the cases, and a
 * run of cases keeps the screens.
 */
export function mergeCaptures(previous, captured, screenList, cases) {
  const key = (e) => `${e.kind}:${e.name}`;
  const byKey = new Map([...previous, ...captured].map((e) => [key(e), e]));
  return [
    ...screenList.map((s) => byKey.get(`screen:${s.name}`)),
    ...cases.map((c) => byKey.get(`case:${c.name}`)),
  ].filter(Boolean);
}

/** How the Rust app reaches gallery case `c`: its `rust` route, or the route of its state and
 * its steps when the case is that state as tC4 drew it and the route uses only the driver. */
export function caseRoute(c, state) {
  if (c.rust) return c.rust;
  if (state.portable && !c.patch && !c.returns) {
    return async (app, env) => {
      await state.route(app, env);
      await runSteps(app.page, c.steps ?? []);
    };
  }
  return null;
}

async function captureScenario(browser, scenario, screens, { tc4, ui, out, unit, layers, backend, masks }) {
  const env = await startRustEnv({ scenario, tc4, ui });
  const entries = [];
  try {
    const context = await browser.newContext(env.contextOptions);
    await env.prepare(context);
    const page = await context.newPage();
    const inFlight = await trackRequests(page, env.tracks);
    const pageErrors = [];
    page.on('pageerror', (error) => pageErrors.push(error.message.split('\n')[0]));
    page.on('console', (message) => { if (message.type() === 'error') pageErrors.push(message.text().slice(0, 300)); });
    const app = appDriver({ page, env });
    for (const screen of screens) {
      pageErrors.length = 0;
      const refusedBefore = new Set(env.refused());
      process.stdout.write(`  ${screen.name} ... `);
      const file = path.join(out, 'screens', `${screen.name}.png`);
      // A gallery case's route takes the environment too (a state's route may need it).
      const shot = screen.kind === 'case' ? { ...screen, run: (driver) => screen.run(driver, env) } : screen;
      const { error, homeCards, syncToast, dom } = await captureScreen({
        page, app, screen: shot, inFlight, file, busy: () => env.busy(page), failureTestIds: env.failureTestIds,
      });
      const maskRects = await measureTargets(page, rustMaskTargets(masks, screen.name, env.projectId));
      const domFile = path.join(out, 'dom', `${screen.name}.json`);
      fs.writeFileSync(domFile, `${JSON.stringify({ screen: screen.name, ...dom }, null, 2)}\n`);
      const refused = env.refused().filter((url) => !refusedBefore.has(url));
      const failed = error || pageErrors.length || refused.length;
      console.log(failed ? `FAILED: ${error ?? pageErrors[0] ?? `refused ${refused[0]}`}` : 'ok');
      entries.push({
        name: screen.name,
        kind: screen.kind ?? 'screen',
        scenario,
        locale: screen.locale ?? 'en',
        by: unit,
        backend,
        layers,
        error,
        pageErrors: [...pageErrors],
        refused,
        homeCards,
        syncToast,
        maskRects,
        png: `screens/${screen.name}.png`,
        sha256: sha256(fs.readFileSync(file)),
        dom: `dom/${screen.name}.json`,
        domSha256: sha256(fs.readFileSync(domFile)),
      });
    }
    await context.close();
    return { entries, build: await env.build(), projects: env.projects };
  } finally {
    await env.close();
  }
}

async function main() {
  const unit = argValue('--unit');
  const units = repoUnits(REPO);
  if (!unit || !units.has(unit)) throw new Error(`--unit must name a unit of docs/APP-DESIGN.md §10 (${[...units].join(', ')})`);
  const only = argValue('--only')?.split(',').filter(Boolean);
  const caseNames = argValue('--case')?.split(',').filter(Boolean);
  if (!only && !caseNames && !process.argv.includes('--all')) throw new Error('say what to capture: --only <screens>, --all or --case <gallery cases>');
  if (caseNames && (only || process.argv.includes('--all'))) throw new Error('--case captures gallery cases alone: run the screens separately');
  const layers = (argValue('--layers') ?? (caseNames ? 'pixels,dom' : LAYERS.join(','))).split(',');
  if (!layers.length || layers.some((l) => !LAYERS.includes(l))) throw new Error(`--layers is a list of ${LAYERS.join(', ')}`);
  if (caseNames && layers.includes('meaning')) throw new Error('a gallery case has no meaning layer: --layers pixels,dom or dom');
  const backend = argValue('--backend') ?? 'memory';
  if (!BACKENDS.includes(backend)) throw new Error(`--backend is one of ${BACKENDS.join(', ')}`);
  const tc4 = argValue('--tc4') ?? path.join(REPO, 'target', 'debug', 'tc4');
  const ui = argValue('--ui') ?? path.join(REPO, 'target', 'tc-ui-app');
  const out = path.resolve(argValue('--out') ?? path.join(REPO, 'parity', 'app'));
  const unknown = (only ?? []).filter((name) => !SCREEN_LIST.some((s) => s.name === name));
  if (unknown.length) throw new Error(`no such screens: ${unknown.join(', ')}`);
  const unknownCases = (caseNames ?? []).filter((name) => !CASES.some((c) => c.name === name));
  if (unknownCases.length) throw new Error(`no such gallery cases: ${unknownCases.join(', ')}`);
  const selected = caseNames
    ? CASES.filter((c) => caseNames.includes(c.name)).map((c) => {
      const state = stateNamed(c.state);
      const route = caseRoute(c, state);
      if (!route) throw new Error(`gallery case ${c.name} has no Rust route: the unit that ports its group (${c.group}) adds \`rust\` to it in parity/view-gallery/cases.mjs`);
      return { name: c.name, kind: 'case', scenario: state.scenario, locale: state.locale, run: route };
    })
    : SCREEN_LIST.filter((s) => !only || only.includes(s.name));
  const masks = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'app', 'masks.json'), 'utf8')).masks;

  const capturesFile = path.join(out, 'captures.json');
  const previous = fs.existsSync(capturesFile) ? JSON.parse(fs.readFileSync(capturesFile, 'utf8')) : null;
  const { chromium } = loadPlaywright();
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const machine = machineFingerprint(browser.version());
  // One machine's pixels in one file: a partial run from another machine would leave screens
  // of two machines side by side, and the gate compares pixels only within one.
  if (previous?.machine && JSON.stringify(previous.machine) !== JSON.stringify(machine)) {
    const left = previous.screens.filter((s) => !selected.some((t) => t.name === s.name && (t.kind ?? 'screen') === s.kind)).map((s) => s.name);
    if (left.length) {
      await browser.close();
      throw new Error(`captures.json holds screens of another machine (${JSON.stringify(previous.machine)}); recapture all of them on this one: ${left.join(', ')}`);
    }
  }
  fs.mkdirSync(path.join(out, 'screens'), { recursive: true });
  fs.mkdirSync(path.join(out, 'dom'), { recursive: true });
  const captured = [];
  const projects = { ...(previous?.projects ?? {}) };
  let build = previous?.build ?? null;
  try {
    for (const scenario of [...new Set(selected.map((s) => s.scenario))]) {
      console.log(`scenario ${scenario}`);
      const run = await captureScenario(browser, scenario, selected.filter((s) => s.scenario === scenario), { tc4, ui, out, unit, layers, backend, masks });
      captured.push(...run.entries);
      projects[scenario] = run.projects;
      build = run.build;
    }
  } finally {
    await browser.close();
  }
  const captures = {
    about: 'The Rust app\'s captures: what capture-rust.mjs saw at each shot. The PNG and DOM files are beside it, in screens/ and dom/.',
    machine,
    build,
    projects: Object.fromEntries(Object.keys(projects).sort().map((k) => [k, projects[k]])),
    screens: mergeCaptures(previous?.screens ?? [], captured, SCREEN_LIST, CASES),
  };
  fs.writeFileSync(capturesFile, `${JSON.stringify(captures, null, 2)}\n`);
  const bad = captured.filter((e) => e.error || e.pageErrors.length || e.refused.length);
  console.log(`${captured.length} ${caseNames ? 'cases' : 'screens'} captured, ${bad.length} failed. Next: UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`);
  if (bad.length) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === new URL(import.meta.url).pathname) await main();
