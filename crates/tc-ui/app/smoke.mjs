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
// Playwright comes from PLAYWRIGHT_MODULE (default: the parity harness's copy, whose Chromium
// build is installed under PLAYWRIGHT_BROWSERS_PATH).
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
const playwrightDir = process.env.PLAYWRIGHT_MODULE ?? '/home/user/tc-rst/parity/ds-gallery/node_modules/playwright';
const { chromium } = require(playwrightDir);

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
  await smoke();
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
