#!/usr/bin/env node
// Records the gallery's states (states.mjs) from real tC4: each route runs from a fresh
// environment of the baseline's mock (parity/app/env-tc4.mjs), the screen is photographed as
// the baseline photographs it (the real capture, which calibration compares the gallery with),
// then tC4's whole state is read from its React tree, with the replies of the read-only actions
// the view draws.
//
//   TC4_ROOT=<tC4 clone with node_modules> node parity/view-gallery/record-states.mjs [--only a,b] [--port 19997]
//
// Writes states/<name>.json ({ name, s, returns }, large values in states/parts/<sha>.json,
// shared between states) and states.json. A state that is a baseline screen must photograph
// exactly as the committed screen, or this machine does not reproduce the baseline and nothing
// of it is written. A recording is not reproducible byte for byte (tC4 writes wall-clock times
// into journals and saves): the committed states are the reference, reviewed like a baseline
// capture. Uses the mock's .state folders: never run it beside parity/tc4-baseline/capture.mjs
// or parity/app/measure-masks.mjs.
import fs from 'node:fs';
import path from 'node:path';
import { appDriver } from '../app/driver.mjs';
import { buildClient, startTc4Env, tc4Identity } from '../app/env-tc4.mjs';
import { CHROMIUM_ARGS, CONTEXT_OPTIONS, captureScreen, machineFingerprint, trackRequests } from '../app/observe.mjs';
import { requireTc4 } from './build.mjs';
import { domDigestInput, encodeValue } from './lib.mjs';
import { HERE, PARTS_DIR, REPO, STATES_DIR, filesUnder, partsOf, prettyJson, sha256, splitParts } from './node-lib.mjs';
import { loadHarnessPlaywright } from './playwright.mjs';
import { STATES } from './states.mjs';

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};

/** Runs in tC4's page: its AppProvider value, found depth-first in the React tree, read as
 * JSON by `encode` (lib.mjs encodeValue, shipped as source), then each of `calls` called once
 * and its reply read. */
const READ_APP = async (encode, calls) => {
  const root = document.getElementById('root');
  const key = Object.keys(root).find((k) => k.startsWith('__reactContainer$'));
  if (!key) throw new Error('#root holds no React tree');
  const stack = [root[key].stateNode.current];
  let value = null;
  while (stack.length) {
    const fiber = stack.pop();
    const v = fiber.memoizedProps?.value;
    if (v && typeof v === 'object' && 's' in v && 'actions' in v) {
      value = v;
      break;
    }
    if (fiber.sibling) stack.push(fiber.sibling);
    if (fiber.child) stack.push(fiber.child);
  }
  if (!value) throw new Error('the React tree holds no provider value with s and actions');
  const state = encode(value.s, 's');
  const refused = [...state.refused];
  const returns = {};
  for (const name of calls) {
    const reply = encode(await value.actions[name](), `returns.${name}`);
    refused.push(...reply.refused);
    returns[name] = { resolves: reply.value };
  }
  return { s: state.value, returns, refused };
};

/** The committed baseline screen `name`: the sha256 of its PNG and the digest of its DOM. */
function baselineReal(name) {
  const manifest = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'tc4-baseline', 'manifest.json'), 'utf8'));
  const screen = manifest.screens.find((s) => s.name === name);
  if (!screen) throw new Error(`no baseline screen ${name}`);
  const dom = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'tc4-baseline', screen.dom), 'utf8'));
  return { png: screen.sha256, dom: sha256(domDigestInput(dom)) };
}

async function recordState(browser, state, { port, shots }) {
  const started = Date.now();
  const env = await startTc4Env({ scenario: state.scenario, port });
  const context = await browser.newContext(CONTEXT_OPTIONS);
  try {
    await env.prepare(context);
    const page = await context.newPage();
    const pageErrors = [];
    page.on('pageerror', (error) => pageErrors.push(error.message.split('\n')[0]));
    const inFlight = await trackRequests(page, env.tracks);
    const app = appDriver({ page, env });
    const file = path.join(shots, `${state.name}.png`);
    const { error, dom } = await captureScreen({
      page, app, inFlight, file,
      screen: { name: state.name, run: (driver) => state.route(driver, env) },
    });
    if (error) throw new Error(`the route failed: ${error}`);
    if (pageErrors.length) throw new Error(`page error: ${pageErrors[0]}`);
    if (env.refused().length) throw new Error(`a request to another origin: ${env.refused()[0]}`);
    fs.writeFileSync(path.join(shots, `${state.name}.dom.json`), prettyJson(dom));
    const real = { png: sha256(fs.readFileSync(file)), dom: sha256(domDigestInput(dom)) };
    if (state.baseline) {
      const want = baselineReal(state.baseline);
      if (want.png !== real.png || want.dom !== real.dom) {
        throw new Error(`this machine does not reproduce baseline screen ${state.baseline} (${want.png !== real.png ? 'pixels' : 'DOM'} differ; the shot is in ${path.relative(REPO, shots)}): capture the baseline again on this machine first`);
      }
    }
    const read = await page.evaluate(`(${READ_APP})(${String(encodeValue)}, ${JSON.stringify(state.calls)})`);
    if (read.refused.length) throw new Error(`tC4's state holds values JSON cannot carry: ${read.refused.slice(0, 8).join('; ')}`);
    return { ...read, real, ms: Date.now() - started };
  } finally {
    await context.close();
    await env.close();
  }
}

async function main() {
  requireTc4();
  const only = argValue('--only')?.split(',').filter(Boolean);
  const port = Number(argValue('--port') ?? 19997);
  const unknown = (only ?? []).filter((n) => !STATES.some((s) => s.name === n));
  if (unknown.length) throw new Error(`no such states: ${unknown.join(', ')}`);
  const selected = STATES.filter((s) => !only || only.includes(s.name));
  const tc4 = tc4Identity();
  buildClient(tc4.commit);
  const { chromium } = loadHarnessPlaywright();
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const machine = machineFingerprint(browser.version());
  const shots = path.join(HERE, '.cache', 'record');
  fs.mkdirSync(shots, { recursive: true });
  fs.mkdirSync(PARTS_DIR, { recursive: true });

  const indexFile = path.join(HERE, 'states.json');
  const previous = fs.existsSync(indexFile) ? JSON.parse(fs.readFileSync(indexFile, 'utf8')) : null;
  if (previous?.machine && JSON.stringify(previous.machine) !== JSON.stringify(machine) && only) {
    await browser.close();
    throw new Error(`states.json was recorded on another machine (${JSON.stringify(previous.machine)}): record every state again here`);
  }
  const entries = new Map((previous?.states ?? []).map((e) => [e.name, e]));
  const failed = [];
  try {
    for (const state of selected) {
      process.stdout.write(`  ${state.name} (${state.scenario}) ... `);
      let read;
      try {
        read = await recordState(browser, state, { port, shots });
      } catch (error) {
        failed.push(`${state.name}: ${error.message.split('\n')[0]}`);
        console.log(`FAILED: ${error.message.split('\n')[0]}`);
        continue;
      }
      const { s, parts } = splitParts(read.s);
      for (const [id, text] of Object.entries(parts)) {
        const file = path.join(PARTS_DIR, `${id}.json`);
        if (!fs.existsSync(file)) fs.writeFileSync(file, text);
      }
      const text = prettyJson({ name: state.name, s, returns: read.returns });
      fs.writeFileSync(path.join(STATES_DIR, `${state.name}.json`), text);
      entries.set(state.name, {
        name: state.name, scenario: state.scenario, locale: state.locale, baseline: state.baseline,
        portable: state.portable, calls: state.calls, notes: state.notes,
        file: `states/${state.name}.json`, sha256: sha256(text), parts: partsOf(s),
        real: read.real,
      });
      console.log(`ok (${(read.ms / 1000).toFixed(1)} s, ${(Buffer.byteLength(text) / 1024).toFixed(0)} KiB${Object.keys(parts).length ? ` + ${Object.keys(parts).length} parts` : ''})`);
    }
  } finally {
    await browser.close();
  }
  const index = {
    about: 'The recorded states of the tC4 view-state gallery (APP-DESIGN §7.4): the whole state of real tC4, read from its React tree after record-states.mjs drove it against the baseline\'s mock along each route of states.mjs. `real` is the sha256 of the PNG and of the DOM digest (lib.mjs domDigestInput) that real tC4 showed at that moment, which calibration.json compares the gallery\'s render with. A state that is a baseline screen (`baseline`) photographed exactly as the committed screen.',
    tc4: { commit: tc4.commit },
    machine,
    states: STATES.map((s) => entries.get(s.name)).filter(Boolean),
  };
  fs.writeFileSync(indexFile, prettyJson(index));
  if (!only && !failed.length) {
    const named = new Set(index.states.flatMap((e) => [`${e.name}.json`, ...e.parts.map((p) => `parts/${p}.json`)]));
    for (const rel of filesUnder(STATES_DIR)) if (!named.has(rel)) fs.rmSync(path.join(STATES_DIR, rel));
  }
  console.log(`${selected.length - failed.length} of ${selected.length} states recorded${failed.length ? `; failed:\n  ${failed.join('\n  ')}` : ''}`);
  if (failed.length) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.join(HERE, 'record-states.mjs')) {
  await main();
}
