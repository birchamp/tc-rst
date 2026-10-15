#!/usr/bin/env node
// Capture the design-system baseline: one PNG per case of gallery-spec.json.
//
//   node capture.mjs [--only <prefix>[,<prefix>...]] [--jobs 6] [--no-build]
//
// 1. Regenerates gallery-spec.json from cases/*.mjs and builds the gallery with Vite (dist/).
// 2. Serves dist/ on a local port and opens Chromium (Playwright's, from PLAYWRIGHT_BROWSERS_PATH).
// 3. For each case, follows the spec's `protocol`: a fresh page sized to the frame, fonts loaded,
//    a pointer click before mounting, the steps, motion off, then a screenshot of the frame
//    element into screens/<Component>/<state>.png.
// 4. Fails (exit 1) when a case throws, logs an error, or overflows its frame. A full run also
//    deletes screens that no longer have a case.
//
// --only takes case-id prefixes: `--only Button/,Select/open` captures those and leaves the
// other screens alone.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { build } from 'vite';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DIST = path.join(HERE, 'dist');
const SCREENS = path.join(HERE, 'screens');

/** Rendering flags that keep repeated runs pixel-identical (the same set as parity/tc4-baseline). */
const CHROMIUM_ARGS = ['--force-color-profile=srgb', '--disable-partial-raster', '--disable-gpu-rasterization', '--disable-lcd-text', '--font-render-hinting=none'];
const STILL = '*, *::before, *::after { transition: none !important; animation: none !important; caret-color: transparent !important; }';

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};
const only = argValue('--only')?.split(',').filter(Boolean);
const jobs = Number(argValue('--jobs') ?? 6);

const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json',
  '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.png': 'image/png', '.svg': 'image/svg+xml',
};

function serve(root) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, 'http://x');
    const file = path.join(root, decodeURIComponent(url.pathname === '/' ? '/index.html' : url.pathname));
    if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404).end();
      return;
    }
    res.writeHead(200, { 'content-type': MIME[path.extname(file)] ?? 'application/octet-stream' });
    fs.createReadStream(file).pipe(res);
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

const settle = (page, ms) => page.evaluate(async (wait) => {
  await new Promise((r) => setTimeout(r, wait));
  await document.fonts.ready;
  /* An image that has not decoded yet draws nothing and, with width auto, takes no room. */
  await Promise.all(Array.from(document.images, (img) => img.decode().catch(() => null)));
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
}, ms);

/** Keyboard focus: Tab until the target has the focus, as a person would reach it. */
async function focusByKeyboard(page, target) {
  for (let i = 0; i < 40; i++) {
    await page.keyboard.press('Tab');
    const there = await page.evaluate((sel) => {
      const frame = document.querySelector('[data-case]');
      const a = document.activeElement;
      return !!a && frame.contains(a) && a.matches(sel);
    }, target);
    if (there) return;
  }
  throw new Error(`Tab never reached ${target}`);
}

async function runStep(page, step) {
  const target = step.target && page.locator(`[data-case] ${step.target}`).first();
  switch (step.do) {
    case 'focus': return focusByKeyboard(page, step.target);
    case 'hover': return target.hover();
    case 'press': await target.hover(); return page.mouse.down();
    case 'click': return target.click();
    case 'key': return page.keyboard.press(step.key);
    case 'type': return page.keyboard.type(step.text);
    default: throw new Error(`unknown step ${step.do}`);
  }
}

async function capture(context, origin, c) {
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  try {
    await page.setViewportSize({ width: c.frame.width, height: c.frame.height });
    await page.goto(`${origin}/?case=${encodeURIComponent(c.id)}`);
    await page.waitForFunction(() => window.__galleryBooted === true);
    await page.addStyleTag({ content: STILL });
    await page.mouse.click(1, 1);
    await page.evaluate(() => window.__mount());
    await page.waitForFunction(() => window.__galleryMounted === true);
    await settle(page, 150);
    /* A settle after every step, not only the last: Select focuses its search field on
       timers after it opens, and a `type` step must not run before that. */
    for (const step of c.steps ?? []) {
      await runStep(page, step);
      await settle(page, 300);
    }
    const overflow = await page.evaluate(() => {
      const f = document.querySelector('[data-case]');
      return f.scrollWidth > f.clientWidth || f.scrollHeight > f.clientHeight
        ? `content ${f.scrollWidth}x${f.scrollHeight} > frame ${f.clientWidth}x${f.clientHeight}` : null;
    });
    if (overflow) errors.push(`overflows its frame: ${overflow}`);
    const file = path.join(SCREENS, c.component, `${c.state}.png`);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    await page.locator('[data-case]').screenshot({ path: file, animations: 'disabled', caret: 'hide', scale: 'css' });
  } catch (e) {
    errors.push(e.message.split('\n')[0]);
  } finally {
    await page.close();
  }
  return errors;
}

async function main() {
  if (!process.argv.includes('--no-build')) {
    execFileSync('node', [path.join(HERE, 'build-spec.mjs')], { stdio: 'inherit' });
    await build({ configFile: path.join(HERE, 'vite.config.mjs') });
  }
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const cases = spec.cases.filter((c) => !only || only.some((p) => c.id.startsWith(p)));
  const server = await serve(DIST);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const context = await browser.newContext({ deviceScaleFactor: 1, colorScheme: 'light', locale: 'en-US', timezoneId: 'UTC' });

  const failures = [];
  let next = 0;
  let done = 0;
  const started = Date.now();
  const worker = async () => {
    while (next < cases.length) {
      const c = cases[next++];
      const errors = await capture(context, origin, c);
      if (errors.length) failures.push({ id: c.id, errors });
      done += 1;
      if (done % 50 === 0) console.log(`${done}/${cases.length}`);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, cases.length) }, worker));
  await browser.close();
  server.close();

  if (!only) {
    const want = new Set(spec.cases.map((c) => path.join(SCREENS, c.component, `${c.state}.png`)));
    for (const dir of fs.readdirSync(SCREENS)) {
      for (const f of fs.readdirSync(path.join(SCREENS, dir))) {
        const file = path.join(SCREENS, dir, f);
        if (!want.has(file)) fs.rmSync(file);
      }
      if (!fs.readdirSync(path.join(SCREENS, dir)).length) fs.rmdirSync(path.join(SCREENS, dir));
    }
  }
  console.log(`${cases.length} cases in ${((Date.now() - started) / 1000).toFixed(1)} s, ${failures.length} failed`);
  for (const f of failures) console.log(`  ${f.id}: ${f.errors.join(' | ')}`);
  process.exitCode = failures.length ? 1 : 0;
}

main();
