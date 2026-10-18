// The smoke of unit A4a (docs/APP-DESIGN.md section 10): it starts `tc4 serve` on the folder
// that build-app.sh made, opens the page in Chromium as the shell does, and checks the
// runtime end to end: boot, `hello` once after the event stream opens, a language switch that
// sends nothing and remounts nothing, events that reach the page, a boot in the saved
// language, a boot that stops on a catalog that does not load, a reload that keeps the token,
// and a quit.
//
//   crates/tc-ui/build-app.sh
//   node crates/tc-ui/app/smoke.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app
//
// With `--measure N --out FILE` it measures instead (see measure() below).
//
// Playwright comes from PLAYWRIGHT_MODULE (default: parity/ds-gallery/node_modules/playwright
// in this repository), and its Chromium build (1194) from PLAYWRIGHT_BROWSERS_PATH or
// Playwright's own cache folder.
//
// How the page is driven, because the checks mean nothing otherwise. The shell opens the window
// at `<url>#t=<token>` and adds `X-Tc-Token` to the requests the browser makes by itself (the
// page, scripts, styles, fonts, images) but not to `/api/` (APP-DESIGN AD11, 5.2). A route here
// does the same, and only outside `/api/`. Playwright's `extraHTTPHeaders` would add the token
// to `/api/` too and hide a client that forgets it.
import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, arg, i, all) => {
    if (arg.startsWith('--')) pairs.push([arg.slice(2), all[i + 1] && !all[i + 1].startsWith('--') ? all[i + 1] : 'true']);
    return pairs;
  }, []),
);
for (const required of ['tc4', 'ui']) {
  if (!args[required]) throw new Error(`--${required} is required`);
}
const uiDir = path.resolve(args.ui);
const buildName = fs.readdirSync(path.join(uiDir, 'ui'))[0];
const crateDir = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

const require = createRequire(import.meta.url);
const playwrightDir = process.env.PLAYWRIGHT_MODULE ?? path.join(crateDir, '../../parity/ds-gallery/node_modules/playwright');
let chromium;
try {
  ({ chromium } = require(playwrightDir));
} catch (error) {
  throw new Error(`Playwright 1.56.1 is not at ${playwrightDir}; set PLAYWRIGHT_MODULE to its folder (${error.code ?? error.message})`);
}

const LOCALES = ['en', 'es-419', 'fr', 'hi', 'pt-BR', 'id', 'uk', 'ru', 'vi', 'ar'];
const catalog = (id) => JSON.parse(fs.readFileSync(path.join(crateDir, 'assets/i18n', `${id}.json`), 'utf8'));

// --- tc4 --------------------------------------------------------------------------------------
class Server {
  /** Starts `tc4 serve` on a fresh app-data folder and an existing, empty projects folder. */
  static async start() {
    // Under target/, beside the built UI; an absent --projects makes tc4 use the person's own
    // folder, and a missing one stops the start.
    const run = path.join(path.dirname(uiDir), 'tc-ui-smoke', `${process.pid}-${Date.now()}`);
    fs.mkdirSync(path.join(run, 'projects'), { recursive: true });
    fs.mkdirSync(path.join(run, 'appdata'), { recursive: true });
    // stdin stays open: a closed stdin makes tc4 quit.
    const child = spawn(
      path.resolve(args.tc4),
      ['serve', '--app-data', path.join(run, 'appdata'), '--projects', path.join(run, 'projects'), '--ui', uiDir],
      { stdio: ['pipe', 'pipe', 'inherit'] },
    );
    const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
    const first = await Promise.race([lines.next(), exited.then(() => ({ done: true }))]);
    if (first.done) throw new Error('tc4 ended before printing its address');
    const { url, token } = JSON.parse(first.value);
    return new Server(child, lines, exited, run, url, token);
  }

  constructor(child, lines, exited, run, url, token) {
    Object.assign(this, { child, lines, exited, run, url, token });
    this.base = new URL(url);
  }

  /** A request to the API as the page makes it: Host, Origin, the token and the API header. */
  call(endpoint, body = {}) {
    const text = JSON.stringify(body);
    return new Promise((resolve, reject) => {
      const request = http.request(
        {
          host: this.base.hostname,
          port: this.base.port,
          method: 'POST',
          path: `/api/v1/${endpoint}`,
          headers: {
            Host: this.base.host,
            Origin: this.base.origin,
            'X-Tc-Token': this.token,
            'X-Tc-Api': '1',
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(text),
          },
        },
        (response) => {
          const parts = [];
          response.on('data', (part) => parts.push(part));
          response.on('end', () => {
            if (response.statusCode !== 200) return reject(new Error(`${endpoint}: status ${response.statusCode}`));
            const reply = JSON.parse(Buffer.concat(parts).toString('utf8'));
            if (reply.type !== 'ok') return reject(new Error(`${endpoint}: ${JSON.stringify(reply)}`));
            resolve(reply.value);
          });
        },
      );
      request.on('error', reject);
      request.end(text);
    });
  }

  /** A command: it carries a fresh request id. */
  command(endpoint, body) {
    return this.call(endpoint, { request: crypto.randomBytes(16).toString('hex'), ...body });
  }

  /** Quits as the shell does, and says how tc4 ended. */
  async quit() {
    this.child.stdin.write('{"quit":true}\n');
    const timeout = new Promise((resolve) => setTimeout(() => resolve({ timeout: true }), 10000));
    const answer = await Promise.race([this.lines.next(), timeout]);
    const ended = await Promise.race([this.exited, timeout]);
    return { closedLine: answer.value ?? null, exit: ended };
  }

  async stop() {
    if (this.child.exitCode === null && this.child.signalCode === null) {
      this.child.kill('SIGKILL');
      await this.exited;
    }
    fs.rmSync(this.run, { recursive: true, force: true });
  }
}

/**
 * A forwarding proxy in front of `tc4`, so a test can cut the event stream's connection, which
 * the browser offers no way to do. It rewrites `Host` and `Origin` to the server's, which the
 * server's checks demand, and passes everything else, the token header included, through.
 */
class Proxy {
  static async start(server) {
    const streams = new Set();
    const http_ = http.createServer((req, res) => {
      const headers = { ...req.headers, host: server.base.host };
      if (headers.origin) headers.origin = server.base.origin;
      const upstream = http.request(
        { host: server.base.hostname, port: server.base.port, method: req.method, path: req.url, headers },
        (up) => {
          res.writeHead(up.statusCode, up.headers);
          res.flushHeaders();
          if (req.url.startsWith('/api/v1/events')) {
            streams.add(res);
            res.on('close', () => streams.delete(res));
          }
          up.pipe(res);
        },
      );
      upstream.on('error', () => res.destroy());
      res.on('close', () => upstream.destroy());
      req.pipe(upstream);
    });
    await new Promise((resolve) => http_.listen(0, '127.0.0.1', resolve));
    return new Proxy(http_, streams);
  }

  constructor(http_, streams) {
    this.http = http_;
    this.streams = streams;
    this.url = `http://127.0.0.1:${http_.address().port}/`;
  }

  /** Cuts every event stream that is open, as a network failure would. */
  cutStreams() {
    const count = this.streams.size;
    for (const res of this.streams) res.socket?.destroy();
    this.streams.clear();
    return count;
  }

  async stop() {
    this.http.closeAllConnections();
    await new Promise((resolve) => this.http.close(resolve));
  }
}

// --- the page, driven as the shell drives it --------------------------------------------------
/** A browser context whose route adds the token outside /api/, with a log of what the page does. */
async function openContext(browser, server, { abort = [] } = {}) {
  const context = await browser.newContext();
  await context.route('**/*', async (route) => {
    const request = route.request();
    const { pathname } = new URL(request.url());
    if (abort.some((pattern) => pathname.endsWith(pattern))) return route.abort();
    if (pathname.startsWith('/api/')) return route.continue();
    // `allHeaders()`, not `headers()`: the latter leaves out what the network stack adds.
    return route.continue({ headers: { ...(await request.allHeaders()), 'x-tc-token': server.token } });
  });
  return context;
}

/** Records every request, response, console message and page error of a page. */
function watch(page) {
  const log = { requests: [], responses: [], console: [], errors: [] };
  page.on('request', (request) => {
    log.requests.push({
      url: request.url(),
      path: new URL(request.url()).pathname + new URL(request.url()).search,
      method: request.method(),
      type: request.resourceType(),
      at: performance.now(),
      headers: request.allHeaders().catch(() => null),
    });
  });
  page.on('response', (response) => {
    log.responses.push({ url: response.url(), status: response.status(), at: performance.now() });
  });
  page.on('console', (message) => log.console.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', (error) => log.errors.push(error.message));
  return log;
}

const API = (r) => r.path.startsWith('/api/');
/** What the page itself asks the network for (a font is the browser's own business, D10). */
const APP_REQUEST_TYPES = new Set(['fetch', 'xhr', 'document', 'script', 'stylesheet']);

// --- the cases --------------------------------------------------------------------------------
const results = [];
function check(name, ok, detail) {
  results.push({ name, ok: Boolean(ok), ...(detail === undefined ? {} : { detail }) });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok || detail === undefined ? '' : `: ${JSON.stringify(detail)}`}`);
  if (!ok) throw new Error(`check failed: ${name}`);
}

const READY = '[data-testid=runtime-ready]';
const text = (page, testid) => page.textContent(`[data-testid=${testid}]`);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function caseA(page, log, server) {
  await page.goto(`${server.url}#t=${server.token}`);
  await page.waitForSelector(READY, { timeout: 60000 });
  const html = await page.evaluate(() => ({
    lang: document.documentElement.lang,
    dir: document.documentElement.dir,
    outer: document.documentElement.outerHTML,
  }));
  check('A: <html lang dir> is en, ltr', html.lang === 'en' && html.dir === 'ltr', html);

  const catalogs = log.requests.filter((r) => /\/i18n\/[^/]+\.json$/.test(r.path));
  const wanted = LOCALES.filter((id) => id !== 'en').map((id) => `/ui/${buildName}/i18n/${id}.json`);
  check('A: exactly the nine catalogs are fetched', JSON.stringify(catalogs.map((r) => r.path).sort()) === JSON.stringify(wanted.sort()), catalogs.map((r) => r.path));
  const statuses = catalogs.map((r) => log.responses.find((s) => s.url === r.url)?.status);
  check('A: every catalog answered 200', statuses.every((s) => s === 200), statuses);

  const hellos = log.requests.filter((r) => r.path === '/api/v1/app/hello');
  check('A: exactly one POST app/hello', hellos.length === 1 && hellos[0].method === 'POST', hellos.length);
  const events = log.requests.filter((r) => r.path.startsWith('/api/v1/events'));
  check('A: one GET events?after=0', events.length === 1 && events[0].path === '/api/v1/events?after=0', events.map((r) => r.path));
  const head = log.responses.find((s) => s.url === events[0]?.url);
  check('A: hello is sent only after the stream opened (its head arrived)', head && hellos[0] && head.at <= hellos[0].at, { head: head?.at, hello: hellos[0]?.at });

  const apiRequests = log.requests.filter(API);
  const headers = await Promise.all(apiRequests.map((r) => r.headers));
  check(
    'A: every /api/ request carries the token and the API version itself',
    headers.length >= 2 && headers.every((h) => h && h['x-tc-token'] === server.token && h['x-tc-api'] === '1'),
    headers.map((h) => h && { token: h['x-tc-token'] === server.token, api: h['x-tc-api'] }),
  );

  check('A: the sample line is the English text', (await text(page, 'runtime-sample')) === catalog('en')['account.language']);
  check('A: no page error', log.errors.length === 0, log.errors);
  const bad = log.console.filter((m) => m.type === 'error' || /\[i18n\] missing key/.test(m.text));
  check('A: no console error and no missing key', bad.length === 0, bad);
  await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
  check('A: __tcIdle becomes true', true);
  const kept = await page.evaluate(() => sessionStorage.getItem('tc4.token'));
  check('A: the token is kept in sessionStorage', kept === server.token);
  check('A: the token is not in the page', !html.outer.includes(server.token));
  const marks = await page.evaluate(() => performance.getEntriesByType('mark').map((m) => [m.name, m.startTime]));
  const order = ['tc4:boot', 'tc4:hello', 'tc4:catalogs', 'tc4:mounted'].map((name) => marks.find(([n]) => n === name)?.[1]);
  check('A: the boot marks are in order', order.every((t, i) => t !== undefined && (i === 0 || t >= order[i - 1])), order);
  return { build: buildName, marks: Object.fromEntries(marks) };
}

async function caseB(page, log) {
  const handle = await page.$('[data-testid=runtime-sample]');
  const before = log.requests.length;
  await page.selectOption('[data-testid=runtime-locale]', 'ar');
  await page.waitForFunction(() => document.documentElement.lang === 'ar', null, { timeout: 5000 });
  const ar = await page.evaluate(() => ({ lang: document.documentElement.lang, dir: document.documentElement.dir }));
  check('B: <html lang dir> is ar, rtl', ar.lang === 'ar' && ar.dir === 'rtl', ar);
  await page.waitForFunction((want) => document.querySelector('[data-testid=runtime-sample]')?.textContent === want, catalog('ar')['account.language'], { timeout: 5000 });
  check('B: the sample is Arabic', true);
  check('B: the sample node was not remounted', await handle.evaluate((node) => node.isConnected));
  await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
  await sleep(400); // a request the switch caused would have been made by now
  const since = log.requests.slice(before);
  const app = since.filter((r) => APP_REQUEST_TYPES.has(r.type));
  check('B: the switch to ar sent no request', app.length === 0, app.map((r) => `${r.type} ${r.path}`));
  const fonts = since.filter((r) => r.type === 'font').map((r) => r.path);

  const mid = log.requests.length;
  await page.selectOption('[data-testid=runtime-locale]', 'en');
  await page.waitForFunction(() => document.documentElement.lang === 'en' && document.documentElement.dir === 'ltr', null, { timeout: 5000 });
  await page.waitForFunction((want) => document.querySelector('[data-testid=runtime-sample]')?.textContent === want, catalog('en')['account.language'], { timeout: 5000 });
  check('B: back to en: English and ltr', true);
  check('B: the sample node was still not remounted', await handle.evaluate((node) => node.isConnected));
  await sleep(300);
  const back = log.requests.slice(mid).filter((r) => APP_REQUEST_TYPES.has(r.type));
  check('B: the switch back sent no request', back.length === 0, back.map((r) => `${r.type} ${r.path}`));
  return { fontRequestsOnFirstArabic: fonts };
}

async function caseC(page, log, server) {
  const before = log.requests.length;
  await server.command('net/set', { on: true });
  await page.waitForSelector('[data-testid=runtime-internet][data-on=true]', { timeout: 2000 });
  check('C: net/set on reaches the page', true);
  await server.command('net/set', { on: false });
  await page.waitForSelector('[data-testid=runtime-internet][data-on=false]', { timeout: 2000 });
  check('C: net/set off reaches the page', true);
  await sleep(200);
  const asked = log.requests.slice(before).filter((r) => APP_REQUEST_TYPES.has(r.type));
  check('C: the page made no request for it', asked.length === 0, asked.map((r) => r.path));
}

/**
 * A change made while the page's stream is down reaches it when it reconnects. The page runs
 * behind a proxy that cuts its event stream; while it is down Node switches the internet on;
 * the page must ask for the frames after the last it read (`?after=N`, N above 0) and show the
 * change. Cut again, it reconnects again.
 */
async function caseH(browser, server) {
  const proxy = await Proxy.start(server);
  try {
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    await page.goto(`${proxy.url}#t=${server.token}`);
    await page.waitForSelector(READY, { timeout: 60000 });
    await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    // Frames the stream carries before the cut, so that `after` is above 0 when it reconnects.
    await server.command('net/set', { on: true });
    await page.waitForSelector('[data-testid=runtime-internet][data-on=true]', { timeout: 2000 });
    const events = () => log.requests.filter((r) => r.path.startsWith('/api/v1/events'));
    check('H: the page holds the stream through the proxy', events().length === 1 && proxy.streams.size === 1, events().length);

    const before = events().length;
    check('H: the proxy cuts the open stream', proxy.cutStreams() === 1);
    // While the stream is down the switch changes; the page cannot hear it.
    await server.command('net/set', { on: false });
    await page.waitForSelector('[data-testid=runtime-internet][data-on=false]', { timeout: 15000 });
    check('H: a change made while the stream was down reaches the page', true);
    const again = events().slice(before);
    const afters = again.map((r) => Number(new URL(r.url).searchParams.get('after')));
    check('H: the page reconnected, asking after the frames it had read', again.length >= 1 && afters[0] > 0, afters);

    // And again, to see that a second cut is survived too.
    await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    await sleep(300);
    const cut = proxy.cutStreams();
    await server.command('net/set', { on: true });
    await page.waitForSelector('[data-testid=runtime-internet][data-on=true]', { timeout: 15000 });
    check('H: a second cut is survived the same way', cut === 1);
    await server.command('net/set', { on: false });
    await context.close();
  } finally {
    await proxy.stop();
  }
}

async function caseF(page, log, server) {
  const before = log.requests.length;
  await page.goto(server.url); // no fragment: only the token kept in sessionStorage
  await page.waitForSelector(READY, { timeout: 60000 });
  const headers = await Promise.all(log.requests.slice(before).filter(API).map((r) => r.headers));
  check('F: a load without the fragment boots with the kept token', headers.length >= 2 && headers.every((h) => h && h['x-tc-token'] === server.token));
}

async function caseD(browser, server) {
  await server.command('prefs/setLocale', { locale: 'ar' });
  try {
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    // The order of the DOM's own changes: `lang` and `dir` of <html> before the first child
    // of #root. A MutationObserver batches its callbacks, so each record is stamped as one
    // observer sees them in order, and compared by position.
    await page.addInitScript(() => {
      window.__order = [];
      const observer = new MutationObserver((records) => {
        for (const record of records) {
          if (record.type === 'attributes') window.__order.push(record.attributeName);
          else if (record.target.id === 'root' && record.addedNodes.length > 0) window.__order.push('root-child');
        }
      });
      observer.observe(document, { subtree: true, childList: true, attributes: true, attributeFilter: ['lang', 'dir'] });
    });
    await page.goto(`${server.url}#t=${server.token}`);
    await page.waitForSelector(READY, { timeout: 60000 });
    const state = await page.evaluate(() => ({ order: window.__order, lang: document.documentElement.lang, dir: document.documentElement.dir }));
    const first = state.order.indexOf('root-child');
    check('D: lang and dir are ar and rtl', state.lang === 'ar' && state.dir === 'rtl', state);
    check('D: lang and dir were set before the root mounted', first > 0 && state.order.slice(0, first).includes('lang') && state.order.slice(0, first).includes('dir'), state.order);
    check('D: the sample is Arabic', (await text(page, 'runtime-sample')) === catalog('ar')['account.language']);
    check('D: no page error', log.errors.length === 0, log.errors);
    await context.close();
  } finally {
    await server.command('prefs/setLocale', { locale: 'en' });
  }
}

async function caseE(browser, server) {
  const context = await openContext(browser, server, { abort: ['/i18n/fr.json'] });
  const page = await context.newPage();
  const log = watch(page);
  await page.goto(`${server.url}#t=${server.token}`);
  await page.waitForSelector('[data-testid=boot-error]', { timeout: 60000 });
  const shown = await text(page, 'boot-error');
  check('E: the boot error shows the view-error words', shown.includes(catalog('en')['app.viewError']), shown);
  check('E: it names the file that failed', shown.includes('fr.json'), shown);
  check('E: it has a Retry button', (await page.textContent('[data-testid=boot-error-retry]')) === catalog('en')['app.retry']);
  check('E: the app never shows', (await page.$(READY)) === null);
  await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
  check('E: __tcIdle becomes true', true);
  check('E: no page error', log.errors.length === 0, log.errors);
  await context.close();
}

// --- measurements (docs/PERFORMANCE.md rule 5: median and p95 of at least 20 runs) ------------
/** Nearest-rank percentile of an unsorted sample list. */
function percentile(samples, p) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}
const round = (x) => Math.round(x * 100) / 100;
const summary = (samples) => ({
  n: samples.length,
  median: round(percentile(samples, 0.5)),
  p95: round(percentile(samples, 0.95)),
  min: round(Math.min(...samples)),
  max: round(Math.max(...samples)),
});

function folderBytes(dir) {
  let total = 0;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    total += entry.isDirectory() ? folderBytes(full) : fs.statSync(full).size;
  }
  return total;
}

/** One load in a fresh context: the boot marks and the bytes the page fetched. */
async function measureBoot(browser, server) {
  const context = await openContext(browser, server);
  const page = await context.newPage();
  await page.goto(`${server.url}#t=${server.token}`);
  await page.waitForSelector(READY, { timeout: 60000 });
  await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
  const got = await page.evaluate(() => {
    const marks = Object.fromEntries(performance.getEntriesByType('mark').map((m) => [m.name, m.startTime]));
    const navigation = performance.getEntriesByType('navigation')[0];
    const resources = performance.getEntriesByType('resource');
    const bytes = (list) => list.reduce((sum, r) => sum + r.encodedBodySize, 0);
    return {
      marks,
      domContentLoaded: navigation.domContentLoadedEventEnd,
      bytesFetched: bytes(resources.filter((r) => !/\.woff2?$/.test(new URL(r.name).pathname))),
      bytesOfFonts: bytes(resources.filter((r) => /\.woff2?$/.test(new URL(r.name).pathname))),
      requests: resources.length,
    };
  });
  await context.close();
  return got;
}

/**
 * `count` switches of the language, alternating ar and en, in one page. Each is timed in the
 * page twice from the dispatch of the select's `change`: to the moment the sample line's text
 * changes (a MutationObserver sees the change as it happens), which is what the update costs,
 * and to the first animation frame after that, which is what a person waits for and which
 * rounds up to the display's frame. Long tasks (over 50 ms, the Long Tasks API) are listed.
 * Two switches first, that are not timed, load the Arabic face of the fonts; the first one's
 * time is reported apart.
 */
async function measureSwitches(page, count) {
  return page.evaluate(async (count) => {
    const select = document.querySelector('[data-testid=runtime-locale]');
    const sample = document.querySelector('[data-testid=runtime-sample]');
    const longTasks = [];
    const watcher = new PerformanceObserver((list) => list.getEntries().forEach((e) => longTasks.push(Math.round(e.duration))));
    watcher.observe({ entryTypes: ['longtask'] });
    const switchTo = (locale) =>
      new Promise((resolve) => {
        let changed = 0;
        const observer = new MutationObserver(() => {
          observer.disconnect();
          changed = performance.now() - started;
          requestAnimationFrame(() => resolve({ dom: changed, frame: performance.now() - started }));
        });
        observer.observe(sample, { childList: true, characterData: true, subtree: true });
        const started = performance.now();
        select.value = locale;
        select.dispatchEvent(new Event('change', { bubbles: true }));
      });
    const cold = await switchTo('ar');
    await switchTo('en');
    longTasks.length = 0; // the cold pair is not part of the measurement
    const times = [];
    for (let i = 0; i < count; i++) times.push(await switchTo(i % 2 === 0 ? 'ar' : 'en'));
    await new Promise((resolve) => setTimeout(resolve, 100)); // long tasks are reported late
    watcher.disconnect();
    return { cold, times, longTasks };
  }, count);
}

async function measure() {
  const runs = Number(args.measure);
  const server = await Server.start();
  const browser = await chromium.launch();
  try {
    const loadBefore = os.loadavg();
    const boots = [];
    for (let i = 0; i < runs; i++) boots.push(await measureBoot(browser, server));
    const context = await openContext(browser, server);
    const page = await context.newPage();
    await page.goto(`${server.url}#t=${server.token}`);
    await page.waitForSelector(READY, { timeout: 60000 });
    await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    const switches = await measureSwitches(page, runs);
    await context.close();
    const cpu = fs.readFileSync('/proc/cpuinfo', 'utf8');
    const flags = (/^flags\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? '').split(' ');
    const buildDir = path.join(uiDir, 'ui', buildName);
    const mark = (name) => summary(boots.map((b) => b.marks[name]));
    const result = {
      build: buildName,
      runs,
      machine: {
        cpu: /^model name\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? 'unknown',
        vcpus: os.cpus().length,
        sha_ni: flags.includes('sha_ni'),
        avx512f: flags.includes('avx512f'),
        loadBefore,
        loadAfter: os.loadavg(),
      },
      browser: browser.version(),
      playwright: JSON.parse(fs.readFileSync(path.join(playwrightDir, 'package.json'), 'utf8')).version,
      bytes: {
        wasm: fs.statSync(path.join(buildDir, 'tc_ui_bg.wasm')).size,
        glue: fs.statSync(path.join(buildDir, 'tc_ui.js')).size,
        folder: folderBytes(buildDir),
        fetchedAtBoot: boots[0].bytesFetched,
        fontsAtBoot: boots[0].bytesOfFonts,
        requestsAtBoot: boots[0].requests,
      },
      bootMs: {
        wasmReady: mark('tc4:boot'),
        hello: mark('tc4:hello'),
        catalogsFetched: mark('tc4:catalogs-fetched'),
        catalogs: mark('tc4:catalogs'),
        catalogParse: summary(boots.map((b) => b.marks['tc4:catalogs'] - b.marks['tc4:catalogs-fetched'])),
        mounted: mark('tc4:mounted'),
        domContentLoaded: summary(boots.map((b) => b.domContentLoaded)),
      },
      switchMs: {
        toTextChange: summary(switches.times.map((t) => t.dom)),
        toNextFrame: summary(switches.times.map((t) => t.frame)),
        coldFirstArabic: { toTextChange: round(switches.cold.dom), toNextFrame: round(switches.cold.frame) },
      },
      longTasksMs: switches.longTasks,
    };
    if (args.out) fs.writeFileSync(args.out, `${JSON.stringify(result, null, 2)}\n`);
    console.log(JSON.stringify(result, null, 2));
  } finally {
    await browser.close();
    await server.stop();
  }
}

async function smoke() {
  const server = await Server.start();
  const browser = await chromium.launch();
  const summary = { build: buildName, browser: browser.version(), playwright: JSON.parse(fs.readFileSync(path.join(playwrightDir, 'package.json'), 'utf8')).version };
  try {
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    Object.assign(summary, await caseA(page, log, server));
    Object.assign(summary, await caseB(page, log));
    await caseC(page, log, server);
    await caseF(page, log, server);
    await context.close();
    await caseH(browser, server);
    await caseD(browser, server);
    await caseE(browser, server);
    const quit = await server.quit();
    check('G: quit answers closed', quit.closedLine === '{"closed":true}', quit);
    check('G: tc4 exits 0', quit.exit.code === 0, quit);
  } finally {
    await browser.close();
    await server.stop();
    summary.checks = results.length;
    summary.failed = results.filter((r) => !r.ok).map((r) => r.name);
    console.log(JSON.stringify(summary, null, 2));
  }
}

try {
  await (args.measure ? measure() : smoke());
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
