#!/usr/bin/env node
// Measure the rectangle of each declared mask (parity/app/masks.json) on tC4's screens: drives
// each screen of the mask as parity/tc4-baseline/capture.mjs does, with the same browser,
// context, request tracking, settle and pointer, and writes where the mask's element lies
// into `rects`.
//
//   TC4_ROOT=<tC4 clone> node parity/app/measure-masks.mjs [--only <mask id>[,...]] [--grow <px>] [--port 19998]
//
// Needs the tC4 client that capture.mjs builds (parity/tc4-baseline/.cache/tc4-dist) and
// Playwright from parity/tc4-baseline/node_modules. Rectangles named after a gallery case are
// the gallery capture's (parity/view-gallery/capture.mjs) and are left as they are. Run it on the machine that captured the
// baseline (manifest.json `machine`). A target that is not found stops the run, naming the mask
// and the screen. `--grow` pads every rectangle on each side, clipped to the window.
import fs from 'node:fs';
import path from 'node:path';
import { loadPlaywright } from './capture-rust.mjs';
import { appDriver } from './driver.mjs';
import { REPO } from './env-rust.mjs';
import { buildClient, startTc4Env, tc4Identity } from './env-tc4.mjs';
import { CHROMIUM_ARGS, CONTEXT_OPTIONS, POINTER_REST, VIEWPORT, measureTargets, settle, trackRequests } from './observe.mjs';
import { BIBLE, OBS, SCREEN_LIST } from './screens.mjs';

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};

async function main() {
  const file = path.join(REPO, 'parity', 'app', 'masks.json');
  const doc = JSON.parse(fs.readFileSync(file, 'utf8'));
  const only = argValue('--only')?.split(',').filter(Boolean);
  const grow = Number(argValue('--grow') ?? 0);
  const port = Number(argValue('--port') ?? 19998);
  const masks = doc.masks.filter((m) => !only || only.includes(m.id));
  buildClient(tc4Identity().commit);
  const { chromium } = loadPlaywright();
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  try {
    const scenarioOf = (name) => SCREEN_LIST.find((s) => s.name === name)?.scenario;
    for (const scenario of [...new Set(masks.flatMap((m) => Object.keys(m.rects)).map(scenarioOf).filter(Boolean))]) {
      const env = await startTc4Env({ scenario, port });
      const context = await browser.newContext(CONTEXT_OPTIONS);
      await env.prepare(context);
      const page = await context.newPage();
      const inFlight = await trackRequests(page, env.tracks);
      const app = appDriver({ page, env });
      const resolve = (value) => value.replaceAll('<OBS>', env.projectId(OBS)).replaceAll('<BIBLE>', env.projectId(BIBLE));
      try {
        for (const screen of SCREEN_LIST.filter((s) => s.scenario === scenario)) {
          const mine = masks.filter((m) => m.rects[screen.name]);
          if (!mine.length) continue;
          const size = screen.viewport ?? VIEWPORT;
          await page.setViewportSize(size);
          await screen.run(app);
          await settle(page, inFlight);
          await page.mouse.move(POINTER_REST.x, POINTER_REST.y);
          await page.waitForTimeout(150);
          const targets = mine.map((m) => ({ id: m.id, target: Object.fromEntries(Object.entries(m.target).map(([k, v]) => [k, resolve(v)])) }));
          const rects = await measureTargets(page, targets);
          for (const m of mine) {
            const r = rects[m.id];
            if (!r) throw new Error(`mask ${m.id}: its target ${JSON.stringify(m.target)} is not on ${screen.name}`);
            const x = Math.max(0, r.x - grow);
            const y = Math.max(0, r.y - grow);
            m.rects[screen.name] = { x, y, width: Math.min(size.width, r.x + r.width + grow) - x, height: Math.min(size.height, r.y + r.height + grow) - y };
            console.log(`${m.id} on ${screen.name}: ${JSON.stringify(m.rects[screen.name])}`);
          }
        }
      } finally {
        await context.close();
        await env.close();
      }
    }
  } finally {
    await browser.close();
  }
  fs.writeFileSync(file, `${JSON.stringify(doc, null, 2)}\n`);
}

await main();
