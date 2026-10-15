// The capture protocol (gallery-spec.json, `protocol`), shared by capture.mjs (tC4's React
// gallery) and capture-rust.mjs (the Leptos port), so both sides are drawn and photographed
// the same way.
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { chromium } from 'playwright';

/** Rendering flags that keep repeated runs pixel-identical (the same set as parity/tc4-baseline). */
export const CHROMIUM_ARGS = ['--force-color-profile=srgb', '--disable-partial-raster', '--disable-gpu-rasterization', '--disable-lcd-text', '--font-render-hinting=none'];
export const STILL = '*, *::before, *::after { transition: none !important; animation: none !important; caret-color: transparent !important; }';

const MIME = {
  '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json',
  '.woff2': 'font/woff2', '.ttf': 'font/ttf', '.png': 'image/png', '.svg': 'image/svg+xml',
  '.wasm': 'application/wasm',
};

/** A static server for `root` on a free local port. */
export function serve(root) {
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

/**
 * Draws case `c` from the gallery at `origin` and writes the frame to `file`. `inspect`, when
 * given, runs in the page after the steps and its result is returned as `info`. Returns the
 * errors the page reported.
 */
export async function captureCase(context, origin, c, file, inspect) {
  const page = await context.newPage();
  const errors = [];
  let info;
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
    if (inspect) info = await page.evaluate(inspect);
    if (file) {
      fs.mkdirSync(path.dirname(file), { recursive: true });
      await page.locator('[data-case]').screenshot({ path: file, animations: 'disabled', caret: 'hide', scale: 'css' });
    }
  } catch (e) {
    errors.push(e.message.split('\n')[0]);
  } finally {
    await page.close();
  }
  return { errors, info };
}

/** Opens Chromium as the baseline does. */
export async function openBrowser() {
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const context = await browser.newContext({ deviceScaleFactor: 1, colorScheme: 'light', locale: 'en-US', timezoneId: 'UTC' });
  return { browser, context };
}

/** Runs `job` over `cases`, `jobs` at a time, logging progress every 50. */
export async function pool(cases, jobs, job) {
  let next = 0;
  let done = 0;
  const worker = async () => {
    while (next < cases.length) {
      const c = cases[next++];
      await job(c);
      done += 1;
      if (done % 50 === 0) console.log(`${done}/${cases.length}`);
    }
  };
  await Promise.all(Array.from({ length: Math.min(jobs, cases.length) }, worker));
}

/** Deletes screens under `dir` that no case names any more. */
export function prune(dir, cases) {
  if (!fs.existsSync(dir)) return;
  const want = new Set(cases.map((c) => path.join(dir, c.component, `${c.state}.png`)));
  for (const sub of fs.readdirSync(dir)) {
    for (const f of fs.readdirSync(path.join(dir, sub))) {
      const file = path.join(dir, sub, f);
      if (!want.has(file)) fs.rmSync(file);
    }
    if (!fs.readdirSync(path.join(dir, sub)).length) fs.rmdirSync(path.join(dir, sub));
  }
}

/**
 * Runs in the page: the frame's subtree as lines, one per element or text run, with the sorted
 * attributes, every longhand of the inline style as the browser parsed it, input values and
 * checked states, and the focused element. With `ids`, each element also shows the id
 * `tagNodes` gave it (or `new`), so a node drawn again shows even when it looks the same.
 */
export function domSnapshot(ids = false) {
  const out = [];
  const walk = (n, depth) => {
    const pad = '  '.repeat(depth);
    if (n.nodeType === Node.TEXT_NODE) {
      if (n.data.length) out.push(`${pad}"${n.data}"`);
      return;
    }
    if (n.nodeType !== Node.ELEMENT_NODE) return;
    // An image's URL is the gallery's: Vite serves tC4's assets under hashed names.
    const value = (a) => (a.name === 'src' ? a.value.replace(/^.*\//, '').replace(/-[\w-]{8}(\.\w+)$/, '$1') : a.value);
    const attrs = Array.from(n.attributes)
      .filter((a) => a.name !== 'style')
      .map((a) => `${a.name}=${JSON.stringify(value(a))}`)
      .sort();
    // Longhands sorted by name. React writes a shorthand's longhand in place where a parsed
    // style attribute appends it, which reorders cssText without changing any value.
    const decls = n.style ? Array.from(n.style, (p) => `${p}: ${n.style.getPropertyValue(p)}`).sort() : [];
    const style = decls.length ? ` style=${JSON.stringify(decls.join('; '))}` : '';
    const props = [];
    if (n instanceof HTMLInputElement || n instanceof HTMLTextAreaElement) {
      props.push(`.value=${JSON.stringify(n.value)}`);
      if (n.type === 'checkbox' || n.type === 'radio') props.push(`.checked=${n.checked}`, `.indeterminate=${n.indeterminate}`);
    }
    if (document.activeElement === n) props.push('.focused');
    if (ids) props.push(`#${n.__sid ?? 'new'}`);
    out.push(`${pad}<${n.tagName.toLowerCase()} ${attrs.join(' ')}${style}${props.length ? ' ' + props.join(' ') : ''}>`);
    // Adjacent text nodes render as one run; compare their joined text. Comments (Leptos's
    // markers for dynamic children) take no part in layout.
    let text = '';
    const flush = () => {
      if (text) out.push(`${pad}  "${text}"`);
      text = '';
    };
    for (const c of n.childNodes) {
      if (c.nodeType === Node.TEXT_NODE) text += c.data;
      else if (c.nodeType === Node.COMMENT_NODE) continue;
      else {
        flush();
        walk(c, depth + 1);
      }
    }
    flush();
  };
  walk(document.querySelector('[data-case]'), 0);
  return out;
}

/** Runs in the page: numbers, in document order, the frame's elements that have no id yet. */
export function tagNodes() {
  let next = window.__nextSid ?? 0;
  for (const el of document.querySelector('[data-case]').querySelectorAll('*')) {
    if (el.__sid === undefined) el.__sid = next++;
  }
  window.__nextSid = next;
}

export const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};
