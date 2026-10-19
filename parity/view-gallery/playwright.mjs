// Playwright for the gallery: the baseline's 1.56.1 (Chromium build 1194), never the tC4
// clone's. A bare `import 'playwright'` here would resolve through the node_modules link to
// tC4's own Playwright, another version with another Chromium and other pixels.
import fs from 'node:fs';
import path from 'node:path';
import { loadPlaywright } from '../app/capture-rust.mjs';
import { REPO } from './node-lib.mjs';

export const PLAYWRIGHT_VERSION = '1.56.1';

/** The harness's Playwright module, refused unless it is version 1.56.1. */
export function loadHarnessPlaywright() {
  const dir = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
  const playwright = loadPlaywright();
  const { version } = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8'));
  if (version !== PLAYWRIGHT_VERSION) throw new Error(`Playwright at ${dir} is ${version}; the baseline's capture rules need ${PLAYWRIGHT_VERSION} (npm ci in parity/tc4-baseline)`);
  return playwright;
}
