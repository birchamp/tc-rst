#!/usr/bin/env node
// Behaviour parity on full screens (APP-DESIGN 7.3 layer 4): the same keyboard and mouse steps,
// from the same baseline screen, on tC4 and on the Rust app, with the DOM (each element with
// its node number), the focus and the scroll lock recorded after every step. A port that draws
// a node again where React keeps it loses the node's transition and any focus on it, which no
// still image shows. The steps are those of parity/ds-gallery/scenarios.mjs.
//
//   node parity/app/behaviour.mjs --env tc4|rust [--only <sequence>[,<sequence>...]]
//        [--tc4 target/debug/tc4] [--ui target/tc-ui-app]
//
// Writes parity/tc4-baseline/behaviour/<sequence>.json (tC4's, with TC4_ROOT as for
// capture.mjs) or parity/app/behaviour/<sequence>.json (the Rust app's). The gate
// (crates/tc-ui/tests/screen_record.rs) compares the pairs.
import fs from 'node:fs';
import path from 'node:path';
import { appDriver } from './driver.mjs';
import { CHROMIUM_ARGS, CONTEXT_OPTIONS, domSnapshot, settle, tagNodes, trackRequests } from './observe.mjs';
import { SCREEN_LIST } from './screens.mjs';

/**
 * A sequence: { name, screen: <baseline screen it starts from>, steps }. A step is { click: sel }
 * (an element inside #root), { hover: sel }, { mouse: [x, y] } (a click on the page there),
 * { move: [x, y] }, { key: 'Enter' } or { type: 'jo' }; `wait` is how long to let the page move
 * before the snapshot (350 ms unless given: the panel motion takes 240). Unit A5 adds the first
 * ones (APP-DESIGN 10).
 */
export const SEQUENCES = [];

/** Runs in the page: what the DOM snapshot leaves out. */
function pageState() {
  const a = document.activeElement;
  const focus = !a || a === document.body ? 'body'
    : `${a.tagName.toLowerCase()}${a.getAttribute('role') ? `[${a.getAttribute('role')}]` : ''} "${(a.textContent || a.value || '').trim().slice(0, 30)}"`;
  return { focus, overflow: document.body.style.overflow || '(none)' };
}

async function perform(page, step) {
  const at = (sel) => page.locator(`#root ${sel}`).first();
  if (step.click) await at(step.click).click({ timeout: 2000 });
  else if (step.hover) await at(step.hover).hover({ timeout: 2000 });
  else if (step.mouse) await page.mouse.click(step.mouse[0], step.mouse[1]);
  else if (step.move) await page.mouse.move(step.move[0], step.move[1]);
  else if (step.key) await page.keyboard.press(step.key);
  else if (step.type) await page.keyboard.type(step.type);
  else throw new Error(`a step of no known kind: ${JSON.stringify(step)}`);
}

/**
 * Drives `screen` (an entry of SCREEN_LIST, or any { name, run }) to its state, then each step,
 * and answers the recording: { sequence, screen, steps: [{ label, dom, focus, overflow }] }.
 * Each snapshot carries the node numbers given at the start (`#n`) or `#new`. A step that
 * fails is an error: a recording of the wrong state proves nothing.
 */
export async function recordSequence({ page, app, sequence, screen, steps, inFlight, busy = null, settleOptions = {} }) {
  const snap = async (label) => ({
    label,
    dom: await page.evaluate(domSnapshot, { ids: true }),
    ...await page.evaluate(pageState),
  });
  await screen.run(app);
  await settle(page, inFlight, { ...settleOptions, busy });
  await page.evaluate(tagNodes);
  const recorded = [await snap('start')];
  for (const step of steps) {
    await perform(page, step);
    await page.waitForTimeout(step.wait ?? 350);
    await settle(page, inFlight, { ...settleOptions, busy });
    recorded.push(await snap(JSON.stringify(step)));
    await page.evaluate(tagNodes);
  }
  return { sequence, screen: screen.name, steps: recorded };
}

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};

async function main() {
  const which = argValue('--env');
  if (!['tc4', 'rust'].includes(which)) throw new Error('--env tc4 or --env rust');
  const only = argValue('--only')?.split(',').filter(Boolean);
  const selected = SEQUENCES.filter((s) => !only || only.includes(s.name));
  if (!selected.length) {
    console.log(only ? `no sequence is named ${only.join(', ')}` : 'no sequences yet: unit A5 adds the first ones');
    return;
  }
  const { REPO, startRustEnv } = await import('./env-rust.mjs');
  const { loadPlaywright } = await import('./capture-rust.mjs');
  const out = path.join(REPO, 'parity', which === 'tc4' ? 'tc4-baseline' : 'app', 'behaviour');
  fs.mkdirSync(out, { recursive: true });
  const { chromium } = loadPlaywright();
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  let failed = 0;
  try {
    for (const sequence of selected) {
      const screen = SCREEN_LIST.find((s) => s.name === sequence.screen);
      if (!screen) throw new Error(`${sequence.name} starts from ${sequence.screen}, which is no baseline screen`);
      let env;
      if (which === 'tc4') {
        const { buildClient, startTc4Env, tc4Identity } = await import('./env-tc4.mjs');
        buildClient(tc4Identity().commit);
        env = await startTc4Env({ scenario: screen.scenario, port: Number(argValue('--port') ?? 19998) });
      } else {
        env = await startRustEnv({ scenario: screen.scenario, tc4: argValue('--tc4') ?? path.join(REPO, 'target', 'debug', 'tc4'), ui: argValue('--ui') ?? path.join(REPO, 'target', 'tc-ui-app') });
      }
      try {
        const context = await browser.newContext(env.contextOptions ?? CONTEXT_OPTIONS);
        await env.prepare(context);
        const page = await context.newPage();
        const errors = [];
        page.on('pageerror', (e) => errors.push(e.message.split('\n')[0]));
        const inFlight = await trackRequests(page, env.tracks);
        const recording = await recordSequence({
          page, app: appDriver({ page, env }), sequence: sequence.name, screen, steps: sequence.steps, inFlight,
          busy: env.busy ? () => env.busy(page) : null,
        });
        if (errors.length || env.refused().length) {
          failed += 1;
          console.log(`${sequence.name}: FAILED: ${errors[0] ?? `refused ${env.refused()[0]}`}`);
        } else {
          fs.writeFileSync(path.join(out, `${sequence.name}.json`), `${JSON.stringify(recording, null, 2)}\n`);
          console.log(`${sequence.name}: ${recording.steps.length} snapshots`);
        }
        await context.close();
      } finally {
        await env.close();
      }
    }
  } finally {
    await browser.close();
  }
  if (failed) process.exitCode = 1;
}

if (process.argv[1] && path.resolve(process.argv[1]) === new URL(import.meta.url).pathname) await main();
