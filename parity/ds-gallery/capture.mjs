#!/usr/bin/env node
// Capture the design-system baseline: one PNG per case of gallery-spec.json.
//
//   node capture.mjs [--only <prefix>[,<prefix>...]] [--jobs 6] [--no-build]
//
// 1. Regenerates gallery-spec.json from cases/*.mjs and builds the gallery with Vite (dist/).
// 2. Serves dist/ on a local port and opens Chromium (Playwright's, from PLAYWRIGHT_BROWSERS_PATH).
// 3. For each case, follows the spec's `protocol` (protocol.mjs): a fresh page sized to the
//    frame, fonts loaded, a pointer click before mounting, the steps, motion off, then a
//    screenshot of the frame element into screens/<Component>/<state>.png.
// 4. Fails (exit 1) when a case throws, logs an error, or overflows its frame. A full run also
//    deletes screens that no longer have a case.
//
// --only takes case-id prefixes: `--only Button/,Select/open` captures those and leaves the
// other screens alone.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { build } from 'vite';
import { argValue, captureCase, openBrowser, pool, prune, serve } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(HERE, 'dist');
const SCREENS = path.join(HERE, 'screens');

const only = argValue('--only')?.split(',').filter(Boolean);
const jobs = Number(argValue('--jobs') ?? 6);

async function main() {
  if (!process.argv.includes('--no-build')) {
    execFileSync('node', [path.join(HERE, 'build-spec.mjs')], { stdio: 'inherit' });
    await build({ configFile: path.join(HERE, 'vite.config.mjs') });
  }
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const cases = spec.cases.filter((c) => !only || only.some((p) => c.id.startsWith(p)));
  const server = await serve(DIST);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const { browser, context } = await openBrowser();

  const failures = [];
  const started = Date.now();
  await pool(cases, jobs, async (c) => {
    const { errors } = await captureCase(context, origin, c, path.join(SCREENS, c.component, `${c.state}.png`));
    if (errors.length) failures.push({ id: c.id, errors });
  });
  await browser.close();
  server.close();

  if (!only) prune(SCREENS, spec.cases);
  console.log(`${cases.length} cases in ${((Date.now() - started) / 1000).toFixed(1)} s, ${failures.length} failed`);
  for (const f of failures) console.log(`  ${f.id}: ${f.errors.join(' | ')}`);
  process.exitCode = failures.length ? 1 : 0;
}

main();
