#!/usr/bin/env node
// Capture the translationCore 4 visual-parity baseline.
//
//   node capture.mjs [--only <name>[,<name>...]] [--port 19998] [--rebuild] [--build-only]
//
// 1. Builds the tC4 client into .cache/tc4-dist once per tC4 commit (--rebuild forces it), as
//    `npm run build` does: tC4's prebuild step fetches the pinned help pages from GitHub, then
//    vite builds the release client (base /clients/uw-tc4/). Without GitHub the help pages are
//    left out and the manifest says so.
// 2. For each scenario, rebuilds the mock server's state from tC4's fixtures
//    (fixtures/build-state.mjs) and starts the mock server (mock-server/server.mjs), which
//    serves both the Pankosmia API and the built client.
// 3. Drives Chromium at 1280x800 through each screen of parity/app/screens.mjs and writes
//    screens/<name>.png and dom/<name>.json, the DOM at the shot.
// 4. Writes manifest.json: every screenshot with its scenario, the app view it shows, the
//    steps that reach it, whether it is complete, the server reads that came back absent, and
//    the sha256 of its PNG and of its DOM file.
//
// Every screen starts from a reload of Home with the per-client place records cleared, so a
// screen never depends on where an earlier screen left the app. The screens, the driver and
// the way a screen is photographed are in parity/app, which the Rust app's capture shares;
// this file is the tC4 entry over parity/app/env-tc4.mjs.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { appDriver } from '../app/driver.mjs';
import { buildClient, missingVersionsOf, startTc4Env, tc4Identity, tc4MissingPinCount } from '../app/env-tc4.mjs';
import { CHROMIUM_ARGS, CONTEXT_OPTIONS, VIEWPORT, captureScreen, machineFingerprint, trackRequests } from '../app/observe.mjs';
import { BIBLE, SCREEN_LIST } from '../app/screens.mjs';
import { ABSENT, SCENARIOS } from './fixtures/build-state.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SCREENS = path.join(HERE, 'screens');
const DOM = path.join(HERE, 'dom');

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};
const only = argValue('--only')?.split(',');
const port = Number(argValue('--port') ?? process.env.TC4_BASELINE_PORT ?? 19998);
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

async function captureScenario(browser, scenario, screens, missingPinCount) {
  const env = await startTc4Env({ scenario, port });
  const context = await browser.newContext(CONTEXT_OPTIONS);
  await env.prepare(context);
  const page = await context.newPage();
  const inFlight = await trackRequests(page, env.tracks);
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  const app = appDriver({ page, env });
  const results = [];
  for (const screen of screens) {
    const mark = env.requestMark();
    pageErrors.length = 0;
    process.stdout.write(`  ${screen.name} ... `);
    const file = path.join(SCREENS, `${screen.name}.png`);
    const { error, homeCards, syncToast, dom } = await captureScreen({ page, app, screen, inFlight, file });
    // tC4 4.0 syncs Bible projects only, so every sync result is the Bible sample's.
    if (syncToast?.ok) syncToast.missingVersions = missingVersionsOf(missingPinCount, env.state, BIBLE);
    const domFile = path.join(DOM, `${screen.name}.json`);
    fs.writeFileSync(domFile, `${JSON.stringify({ screen: screen.name, ...dom }, null, 2)}\n`);
    console.log(error ? `FAILED: ${error}` : 'ok');
    results.push({
      name: screen.name,
      file: `screens/${screen.name}.png`,
      scenario,
      // tC4 has no router: the URL never changes, and the screen is the app state below.
      route: { url: '/clients/uw-tc4/', view: screen.view, modal: screen.modal },
      ...(screen.locale ? { locale: screen.locale } : {}),
      ...(screen.viewport ? { viewport: screen.viewport } : {}),
      ...(screen.markingRow ? { markingRow: screen.markingRow } : {}),
      ...(screen.rebuildDiffers ? { rebuildDiffers: screen.rebuildDiffers } : {}),
      ...(Object.keys(homeCards).length ? { homeCards } : {}),
      ...(syncToast ? { syncToast } : {}),
      steps: screen.steps,
      shows: screen.shows,
      status: error ? 'failed' : screen.incomplete ? 'incomplete' : 'complete',
      ...(screen.incomplete ? { incomplete: screen.incomplete } : {}),
      ...(error ? { error } : {}),
      ...(pageErrors.length ? { pageErrors: [...pageErrors] } : {}),
      absentReads: env.absentReads(mark),
      bytes: fs.statSync(file).size,
      sha256: sha256(fs.readFileSync(file)),
      dom: `dom/${screen.name}.json`,
      domSha256: sha256(fs.readFileSync(domFile)),
    });
  }
  await context.close();
  await env.close();
  const external = env.refused();
  if (external.length) console.log(`  refused external requests: ${external.join(', ')}`);
  return { results, external };
}

async function main() {
  const tc4 = tc4Identity();
  console.log(`tC4 ${tc4.version} at ${tc4.commit}${tc4.dirty ? ' (working tree has local changes)' : ''}`);
  const helpPages = buildClient(tc4.commit, { rebuild: process.argv.includes('--rebuild') });
  if (process.argv.includes('--build-only')) return;
  const missingPinCount = await tc4MissingPinCount();
  fs.mkdirSync(SCREENS, { recursive: true });
  fs.mkdirSync(DOM, { recursive: true });
  const selected = SCREEN_LIST.filter((s) => !only || only.includes(s.name));
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const screens = [];
  const refused = [];
  for (const scenario of Object.keys(SCENARIOS)) {
    const list = selected.filter((s) => s.scenario === scenario);
    if (!list.length) continue;
    console.log(`scenario ${scenario}: ${SCENARIOS[scenario]}`);
    const { results, external } = await captureScenario(browser, scenario, list, missingPinCount);
    screens.push(...results);
    refused.push(...external);
  }
  const browserVersion = browser.version();
  await browser.close();

  // A partial run (--only) updates its screens inside the existing manifest. A full run
  // deletes the DOM files of screens that are no longer in the list.
  const manifestFile = path.join(HERE, 'manifest.json');
  const previous = only && fs.existsSync(manifestFile) ? JSON.parse(fs.readFileSync(manifestFile, 'utf8')).screens : [];
  const byName = new Map(previous.map((s) => [s.name, s]));
  for (const s of screens) byName.set(s.name, s);
  const ordered = SCREEN_LIST.map((s) => byName.get(s.name)).filter(Boolean);
  if (!only) {
    const kept = new Set(ordered.map((s) => `${s.name}.json`));
    for (const f of fs.readdirSync(DOM)) if (!kept.has(f)) fs.rmSync(path.join(DOM, f));
  }
  const stateDir = Object.keys(SCENARIOS).map((name) => path.join(HERE, '.state', name)).find((dir) => fs.existsSync(dir));
  const provenance = JSON.parse(fs.readFileSync(path.join(stateDir, 'provenance.json'), 'utf8'));
  const manifest = {
    tc4: { commit: tc4.commit, commitDate: tc4.commitDate, version: tc4.version, dirtyWorkingTree: tc4.dirty },
    client: `the tC4 commit built as \`npm run build\` builds it (release build: base /clients/uw-tc4/, DEV false), ${helpPages ? 'with' : 'WITHOUT'} the help pages of its pinned tc-website commit, served by the mock server`,
    browser: `Chromium ${browserVersion} (Playwright ${JSON.parse(fs.readFileSync(path.join(HERE, 'node_modules', 'playwright', 'package.json'), 'utf8')).version}), headless`,
    // The fonts decide the fallback glyphs, so the Rust capture compares pixels only with a
    // machine of the same fingerprint (APP-DESIGN 7.3 layer 1).
    machine: machineFingerprint(browserVersion),
    viewport: { ...VIEWPORT, deviceScaleFactor: 1 },
    scenarios: SCENARIOS,
    resources: provenance.resources.map(({ dir, repoPath, version, sha, substitute }) => ({ dir, repoPath, version, sha, ...(substitute ? { substitute } : {}) })),
    absentResources: ABSENT,
    refusedExternalRequests: refused,
    totals: {
      screens: ordered.length,
      complete: ordered.filter((s) => s.status === 'complete').length,
      incomplete: ordered.filter((s) => s.status === 'incomplete').length,
      failed: ordered.filter((s) => s.status === 'failed').length,
      bytes: ordered.reduce((n, s) => n + s.bytes, 0),
    },
    screens: ordered,
  };
  fs.writeFileSync(manifestFile, `${JSON.stringify(manifest, null, 2)}\n`);
  const t = manifest.totals;
  console.log(`${t.screens} screens: ${t.complete} complete, ${t.incomplete} incomplete, ${t.failed} failed; ${(t.bytes / 1024 / 1024).toFixed(2)} MB`);
  if (t.failed) process.exitCode = 1;
}

await main();
