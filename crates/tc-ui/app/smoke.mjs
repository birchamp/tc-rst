// The smoke of unit A4a (docs/APP-DESIGN.md section 10): it starts `tc4 serve` on the folder
// that build-app.sh made, opens the page in Chromium as the shell does, and checks the
// runtime end to end: boot, `hello` once after the event stream opens, a language switch that
// sends nothing and remounts nothing, events that reach the page, a boot in the saved
// language, a boot that stops on a catalog that does not load, a reload that keeps the token,
// a server of another API version or build (one reload, then the boot error; none when the
// page cannot keep its marker, in the middle of a session too), a `Resync` or a restart while
// the page holds a running job and an aligner's save state, and a quit.
//
//   crates/tc-ui/build-app.sh
//   node crates/tc-ui/app/smoke.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app
//
// With `--measure N --out FILE` it measures instead (see measure() below). With
// `--proxy-selftest` it only checks the test proxy against `tc4`, with no browser (see
// proxySelftest() below). `--only app,stale,jobs` runs some groups of cases: the app's basics
// (A to H), a server of another build or API version (I, J), and jobs when the stream has a
// gap (K).
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
 * A forwarding proxy in front of `tc4`, so a test can do what the browser cannot do to its own
 * connections and what a real server would do only after an update:
 *
 * - `cutStreams()` cuts the open event streams, as a network failure would;
 * - `refuseEvents = { status, body }` answers the stream's open itself, as a server of another
 *   API version would (the server checks the version of every /api/ path, the stream's too);
 * - `rewrites.set('app/hello', (value) => value)` changes the value of an endpoint's reply
 *   (a server of another build, a job the server still runs);
 * - `inject(event, { instance })` writes a frame into every open stream (a `Resync`, a job,
 *   a frame of another server run), which the real server sends only after 4,096 frames or a
 *   restart. Its number is the newest the proxy has passed plus one unless `seq` says
 *   otherwise, so the page reads it as new; a real frame of that number would then be a repeat
 *   for the page, so the cases that inject make no real change afterwards.
 *
 * It rewrites `Host` and `Origin` to the server's, which the server's checks demand, and passes
 * everything else, the token header included, through.
 */
class Proxy {
  static async start(server) {
    const proxy = new Proxy(server);
    await new Promise((resolve) => proxy.http.listen(0, '127.0.0.1', resolve));
    proxy.url = `http://127.0.0.1:${proxy.http.address().port}/`;
    return proxy;
  }

  constructor(server) {
    this.server = server;
    this.streams = new Set();
    this.lastSeq = 0;
    this.refuseEvents = null;
    this.rewrites = new Map();
    this.http = http.createServer((req, res) => this.handle(req, res));
  }

  handle(req, res) {
    const { base } = this.server;
    const events = req.url.startsWith('/api/v1/events');
    if (events && this.refuseEvents) {
      req.resume();
      res.writeHead(this.refuseEvents.status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      res.end(this.refuseEvents.body);
      return;
    }
    const headers = { ...req.headers, host: base.host };
    if (headers.origin) headers.origin = base.origin;
    const endpoint = req.method === 'POST' && req.url.startsWith('/api/v1/') ? req.url.slice('/api/v1/'.length) : null;
    const rewrite = endpoint && this.rewrites.get(endpoint);
    const upstream = http.request(
      { host: base.hostname, port: base.port, method: req.method, path: req.url, headers },
      (up) => {
        if (rewrite) return this.rewritten(up, res, rewrite);
        res.writeHead(up.statusCode, up.headers);
        res.flushHeaders();
        if (events) {
          this.streams.add(res);
          res.on('close', () => this.streams.delete(res));
          let partial = ''; // a chunk may end inside a line
          up.on('data', (chunk) => {
            const lines = (partial + chunk.toString('utf8')).split('\n');
            partial = lines.pop();
            for (const line of lines) this.noteSeq(line);
            res.write(chunk);
          });
          up.on('end', () => res.end());
        } else {
          up.pipe(res);
        }
      },
    );
    upstream.on('error', () => res.destroy());
    res.on('close', () => upstream.destroy());
    req.pipe(upstream);
  }

  /** The reply of an endpoint with its `ok` value passed through `rewrite`. */
  rewritten(up, res, rewrite) {
    const parts = [];
    up.on('data', (part) => parts.push(part));
    up.on('end', () => {
      const reply = JSON.parse(Buffer.concat(parts).toString('utf8'));
      if (reply.type === 'ok') reply.value = rewrite(reply.value);
      const body = JSON.stringify(reply);
      const headers = { ...up.headers, 'content-length': Buffer.byteLength(body) };
      delete headers['transfer-encoding'];
      res.writeHead(up.statusCode, headers);
      res.end(body);
    });
  }

  /** The newest `id:` line seen, so that an injected frame is newer than any the page has read. */
  noteSeq(line) {
    const match = /^id: (\d+)$/.exec(line);
    if (match) this.lastSeq = Math.max(this.lastSeq, Number(match[1]));
  }

  /** Writes a frame into every open stream, as `tc-server` writes one. Returns how many got it. */
  inject(event, { instance, seq = this.lastSeq + 1 }) {
    const text = `id: ${seq}\ndata: ${JSON.stringify({ seq, instance, event })}\n\n`;
    for (const res of this.streams) res.write(text);
    this.lastSeq = Math.max(this.lastSeq, seq);
    return this.streams.size;
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
      body: request.postData(),
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

// --- a server that is not this page's build -----------------------------------------------------
/** Every document the page asked for: one at the first load, one more for each reload. */
const documents = (log) => log.requests.filter((r) => r.type === 'document');
const calls = (log, endpoint) => log.requests.filter((r) => r.path === `/api/v1/${endpoint}`);
const marker = (page) => page.evaluate(() => sessionStorage.getItem('tc4.reloadedFor'));
/** The reply of a server that speaks another API version, as `tc-server` writes it (409). */
const versionRefusal = (server) => ({ status: 409, body: JSON.stringify({ type: 'err', value: { type: 'version', value: { server } } }) });
/** The same server, as another build: only the commit that `hello` names differs. */
const otherBuild = (hello) => ({ ...hello, build: { ...hello.build, commit: 'ffffff0' } });

async function until(predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}

/**
 * The server refuses the page's API version (APP-DESIGN 3.10, D8). The first request of a page
 * is the event stream's, so it is the one that finds out: the page reloads once and keeps a
 * marker; the same refusal after the reload shows the boot error naming the version, and no
 * second reload follows. A server that is fixed meanwhile boots on Retry and the marker goes.
 * Then the same in the middle of a session, when the stream's reconnect meets the refusal.
 */
async function caseI(browser, server) {
  const proxy = await Proxy.start(server);
  try {
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    proxy.refuseEvents = versionRefusal(2);
    // 'commit': the page reloads itself while it loads, which would interrupt a wait for 'load'.
    await page.goto(`${proxy.url}#t=${server.token}`, { waitUntil: 'commit' });
    await until(() => documents(log).length >= 2, 15000, 'the one reload');
    await page.waitForSelector('[data-testid=boot-error]', { timeout: 15000 });
    await sleep(1500); // a second reload would have started by now
    check('I: a stream refused for the API version reloaded the page once', documents(log).length === 2, documents(log).length);
    const shown = await text(page, 'boot-error');
    check('I: the boot error names both builds, with the server\'s API version', shown.includes('(API 1)') && shown.includes('(API 2)'), shown);
    check('I: it was the stream that found out: hello was never asked', calls(log, 'app/hello').length === 0);
    check('I: the marker of the reload names that version', /\/2$/.test((await marker(page)) ?? ''), await marker(page));
    await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    check('I: __tcIdle becomes true', true);

    proxy.refuseEvents = null;
    await page.click('[data-testid=boot-error-retry]');
    await page.waitForSelector(READY, { timeout: 60000 });
    check('I: Retry boots once the server speaks this API version', true);
    check('I: and the marker is gone', (await marker(page)) === null, await marker(page));
    await context.close();

    // The middle of a session: the stream is cut, and what answers its reconnect is another API.
    const second = await openContext(browser, server);
    const live = await second.newPage();
    const liveLog = watch(live);
    await live.goto(`${proxy.url}#t=${server.token}`);
    await live.waitForSelector(READY, { timeout: 60000 });
    await live.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    check('I: a page of the same API version boots', documents(liveLog).length === 1);
    proxy.refuseEvents = versionRefusal(3);
    proxy.cutStreams();
    await until(() => documents(liveLog).length >= 2, 15000, 'the reload after the reconnect was refused');
    await live.waitForSelector('[data-testid=boot-error]', { timeout: 15000 });
    await sleep(1500);
    check('I: a reconnect refused for the API version reloads the page once', documents(liveLog).length === 2, documents(liveLog).length);
    check('I: and the marker names that version', /\/3$/.test((await marker(live)) ?? ''), await marker(live));
    await second.close();

    // The middle of a session, with storage that refuses the marker: no reload is safe, since
    // the page after it would reload again, so the boot error replaces the app at once, and the
    // stream is not retried behind it.
    const blocked = await openContext(browser, server);
    await blocked.addInitScript(() => {
      Object.defineProperty(window, 'sessionStorage', {
        configurable: true,
        get() {
          throw new DOMException('The operation is insecure.', 'SecurityError');
        },
      });
    });
    const stuck = await blocked.newPage();
    const stuckLog = watch(stuck);
    proxy.refuseEvents = null;
    await stuck.goto(`${proxy.url}#t=${server.token}`);
    await stuck.waitForSelector(READY, { timeout: 60000 });
    await stuck.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    const streamRequests = () => stuckLog.requests.filter((r) => r.path.startsWith('/api/v1/events'));
    check('I: with storage blocked, a page of the same API version boots', streamRequests().length === 1);
    proxy.refuseEvents = versionRefusal(4);
    proxy.cutStreams();
    await stuck.waitForSelector('[data-testid=boot-error]', { timeout: 15000 });
    check('I: with storage blocked, a reconnect refused for the API version shows the boot error at once', true);
    check('I: and the app is off the page', (await stuck.$(READY)) === null);
    const stuckText = await text(stuck, 'boot-error');
    check('I: and the error names both builds', stuckText.includes('(API 1)') && stuckText.includes('(API 4)'), stuckText);
    const attempts = streamRequests().length;
    await sleep(3500); // a stream that is retried would have asked again after 250 ms, 500 ms, 1 s and 2 s
    check('I: and the page made no reload', documents(stuckLog).length === 1, documents(stuckLog).length);
    check('I: and the stream is not retried behind the error', streamRequests().length === attempts, { attempts, now: streamRequests().length });
    check('I: and no page error', stuckLog.errors.length === 0, stuckLog.errors);
    await stuck.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    check('I: __tcIdle is true behind the error', true);
    await blocked.close();
  } finally {
    await proxy.stop();
  }
}

/**
 * The server is another build: `hello` names another commit. The page reloads once; the same
 * mismatch after the reload shows the boot error naming both builds. A page whose
 * `sessionStorage` refuses the marker (a browser with storage blocked) cannot tell its reload
 * from a first load, so it shows the boot error at once and makes no reload. Storage that
 * refuses must not stop a page that has nothing to reload for.
 */
async function caseJ(browser, server) {
  const proxy = await Proxy.start(server);
  try {
    proxy.rewrites.set('app/hello', otherBuild);
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    await page.goto(`${proxy.url}#t=${server.token}`, { waitUntil: 'commit' });
    await until(() => documents(log).length >= 2, 15000, 'the one reload');
    await page.waitForSelector('[data-testid=boot-error]', { timeout: 15000 });
    await sleep(1500);
    check('J: another build reloaded the page once', documents(log).length === 2, documents(log).length);
    check('J: hello was read once for each load', calls(log, 'app/hello').length === 2, calls(log, 'app/hello').length);
    const shown = await text(page, 'boot-error');
    check('J: the boot error names both builds', shown.includes('ffffff0') && shown.includes('API 1'), shown);
    check('J: the marker names the server\'s build', (await marker(page)) === 'ffffff0/1', await marker(page));
    proxy.rewrites.delete('app/hello');
    await page.click('[data-testid=boot-error-retry]');
    await page.waitForSelector(READY, { timeout: 60000 });
    check('J: Retry boots once the builds agree, and the marker goes', (await marker(page)) === null, await marker(page));
    await context.close();

    // sessionStorage refuses: reading `window.sessionStorage` throws, as it does with site data blocked.
    const blocked = await openContext(browser, server);
    await blocked.addInitScript(() => {
      Object.defineProperty(window, 'sessionStorage', {
        configurable: true,
        get() {
          throw new DOMException('The operation is insecure.', 'SecurityError');
        },
      });
    });
    const stuck = await blocked.newPage();
    const stuckLog = watch(stuck);
    proxy.rewrites.set('app/hello', otherBuild);
    // 'commit': a page that reloads for ever would never reach 'load', and the check below says so.
    await stuck.goto(`${proxy.url}#t=${server.token}`, { waitUntil: 'commit' });
    check('J: the page cannot reach sessionStorage', (await stuck.evaluate(() => { try { return sessionStorage.length >= 0; } catch (error) { return error.name; } })) === 'SecurityError');
    await stuck.waitForSelector('[data-testid=boot-error]', { timeout: 60000 });
    await sleep(2000); // a reload would have come, and with no marker it would come again
    check('J: with storage blocked, another build shows the boot error and makes no reload', documents(stuckLog).length === 1, documents(stuckLog).length);
    check('J: hello was read once', calls(stuckLog, 'app/hello').length === 1, calls(stuckLog, 'app/hello').length);
    check('J: the boot error names the builds', (await text(stuck, 'boot-error')).includes('ffffff0'));
    check('J: no page error with storage blocked', stuckLog.errors.length === 0, stuckLog.errors);
    await stuck.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    check('J: __tcIdle becomes true', true);

    proxy.rewrites.delete('app/hello');
    const fine = await blocked.newPage();
    const fineLog = watch(fine);
    await fine.goto(`${proxy.url}#t=${server.token}`);
    await fine.waitForSelector(READY, { timeout: 60000 });
    check('J: with storage blocked, a page of the same build still boots', true);
    check('J: with storage blocked, no page error', fineLog.errors.length === 0, fineLog.errors);
    await blocked.close();
  } finally {
    await proxy.stop();
  }
}

// --- a job the page holds when the stream has a gap --------------------------------------------
const frames = JSON.parse(fs.readFileSync(path.join(crateDir, 'app/smoke-events.json'), 'utf8'));
/** A job frame's event, for job `id` (the shapes are the ones tests/app/sse.rs reads). */
const jobEvent = (name, id, value = {}) => {
  const event = structuredClone(frames[name]);
  event.value.id = id;
  Object.assign(event.value.state.value ?? {}, value);
  return event;
};
const idle = (page) => page.evaluate(() => window.__tcIdle);

/**
 * `hello` carries no jobs, so a final frame lost in a gap would leave a job running in the page
 * for ever, with `window.__tcIdle` false (PERFORMANCE rule 4). After a `Resync` the page reads
 * each job it holds as active with `jobs/get`; a restart forgets them all. The server here knows
 * no job at all, as after a long gap: the proxy writes the frames and, for one step, a reply.
 */
async function caseK(browser, server) {
  const proxy = await Proxy.start(server);
  try {
    const { instance } = await server.call('app/hello');
    const context = await openContext(browser, server);
    const page = await context.newPage();
    const log = watch(page);
    await page.goto(`${proxy.url}#t=${server.token}`);
    await page.waitForSelector(READY, { timeout: 60000 });
    await page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });
    const inject = (event, options = {}) => proxy.inject(event, { instance, ...options });
    const settle = () => page.waitForFunction(() => window.__tcIdle === true, null, { timeout: 5000 });

    check('K: the proxy holds the page\'s stream', inject(frames.internetOn) === 1);
    await page.waitForSelector('[data-testid=runtime-internet][data-on=true]', { timeout: 5000 });
    check('K: an injected frame reaches the page', true);

    inject(jobEvent('jobRunning', 4242));
    await page.waitForFunction(() => window.__tcIdle === false, null, { timeout: 5000 });
    check('K: a running job is work in flight', (await idle(page)) === false);

    // The server still runs it: a Resync reads it again, and it stays.
    proxy.rewrites.set('jobs/get', () => jobEvent('jobRunning', 4242, { done: 5 }).value);
    inject(frames.resync, { seq: proxy.lastSeq });
    await until(() => calls(log, 'jobs/get').length >= 1, 5000, 'jobs/get after the Resync');
    await sleep(400);
    check('K: a Resync reads the job it holds with jobs/get', JSON.parse(calls(log, 'jobs/get')[0].body).job === 4242, calls(log, 'jobs/get')[0].body);
    check('K: a Resync reads hello again', calls(log, 'app/hello').length === 2, calls(log, 'app/hello').length);
    check('K: a job the server still runs stays in flight', (await idle(page)) === false);

    inject(jobEvent('jobDone', 4242));
    await settle();
    check('K: its end frame ends it', true);

    // The end frame is lost in the gap, and the server no longer knows the job.
    proxy.rewrites.delete('jobs/get');
    inject(jobEvent('jobRunning', 4243));
    await page.waitForFunction(() => window.__tcIdle === false, null, { timeout: 5000 });
    inject(frames.resync, { seq: proxy.lastSeq });
    await settle();
    check('K: a job whose end frame was lost is not left running', calls(log, 'jobs/get').length === 2 && JSON.parse(calls(log, 'jobs/get')[1].body).job === 4243, calls(log, 'jobs/get').map((r) => r.body));

    // A restart: the new run numbers its jobs and aligners from 1, so an old id may name
    // another job or another aligner, and `hello` carries neither.
    inject(jobEvent('jobRunning', 4244));
    await page.waitForFunction(() => window.__tcIdle === false, null, { timeout: 5000 });
    inject(frames.alignSaving);
    // The held counts sit on an empty <p>, which is never visible: wait for it in the DOM.
    await page.waitForSelector('[data-testid=runtime-held][data-align-saves="1"]', { state: 'attached', timeout: 5000 });
    check('K: an aligner\'s save state reaches the page', true);
    inject(frames.internetOn, { instance: 'cd'.repeat(16), seq: 1 });
    await settle();
    check('K: a frame of another server run forgets the jobs, and asks for none', calls(log, 'jobs/get').length === 2, calls(log, 'jobs/get').length);
    await page.waitForSelector('[data-testid=runtime-held][data-align-saves="0"][data-jobs="0"][data-checkpoints="0"][data-blocked="0"]', { state: 'attached', timeout: 5000 });
    check('K: and the aligner save states, which only events carry and the old run numbered', true);
    check('K: no page error', log.errors.length === 0, log.errors);
    await context.close();
  } finally {
    await proxy.stop();
  }
}

/**
 * The proxy, with no browser: what the cases above stand on. It passes `hello` through, changes
 * the commit it names, refuses the stream's open as `tc-server` refuses another API version
 * (the same status and body), and writes frames into an open stream as `tc-server` writes them.
 */
async function proxySelftest(server) {
  const proxy = await Proxy.start(server);
  const base = new URL(proxy.url);
  const request = (method, url, { headers = {}, body } = {}) =>
    new Promise((resolve, reject) => {
      const req = http.request(
        { host: base.hostname, port: base.port, method, path: url, headers: { 'x-tc-token': server.token, 'x-tc-api': '1', ...headers } },
        (res) => resolve(res),
      );
      req.on('error', reject);
      req.end(body);
    });
  const readAll = (res) =>
    new Promise((resolve) => {
      const parts = [];
      res.on('data', (part) => parts.push(part));
      res.on('end', () => resolve(Buffer.concat(parts).toString('utf8')));
    });
  const post = async (endpoint, body = {}) => {
    const text = JSON.stringify(body);
    const res = await request('POST', `/api/v1/${endpoint}`, { headers: { origin: proxy.url.slice(0, -1), 'content-type': 'application/json', 'content-length': Buffer.byteLength(text) }, body: text });
    return { status: res.statusCode, headers: res.headers, text: await readAll(res) };
  };
  try {
    const direct = await server.call('app/hello');
    const passed = await post('app/hello');
    check('proxy: hello passes through unchanged', passed.status === 200 && JSON.stringify(JSON.parse(passed.text).value) === JSON.stringify(direct), passed.text);

    proxy.rewrites.set('app/hello', otherBuild);
    const changed = await post('app/hello');
    const hello = JSON.parse(changed.text);
    check('proxy: a rewritten hello names the other commit and keeps the rest', hello.type === 'ok' && hello.value.build.commit === 'ffffff0' && hello.value.instance === direct.instance, changed.text);
    check('proxy: a rewritten reply has the right content-length', Number(changed.headers['content-length']) === Buffer.byteLength(changed.text), changed.headers['content-length']);
    proxy.rewrites.delete('app/hello');

    // The refusal the cases use is the one the server writes for another API version.
    const real = await request('GET', '/api/v1/events?after=0', { headers: { 'x-tc-api': '2' } });
    const realText = await readAll(real);
    proxy.refuseEvents = versionRefusal(1);
    const faked = await request('GET', '/api/v1/events?after=0');
    check('proxy: a refused stream is the server\'s own refusal', faked.statusCode === real.statusCode && faked.headers['content-type'] === real.headers['content-type'] && (await readAll(faked)) === realText, realText);
    proxy.refuseEvents = null;

    const stream = await request('GET', '/api/v1/events?after=0');
    const seen = [];
    stream.on('data', (part) => seen.push(part.toString('utf8')));
    await until(() => seen.join('').includes(': tc4'), 3000, 'the stream\'s first line');
    check('proxy: the stream is open through the proxy', proxy.streams.size === 1);
    const sent = proxy.inject(frames.resync, { instance: direct.instance });
    await until(() => seen.join('').includes('"resync"'), 3000, 'the injected frame');
    const wire = seen.join('').split('\n\n').find((part) => part.includes('"resync"'));
    const [idLine, dataLine] = wire.split('\n');
    const frame = JSON.parse(dataLine.slice('data: '.length));
    check('proxy: an injected frame is written as the server writes one', sent === 1 && idLine === `id: ${frame.seq}` && frame.event.type === 'resync' && frame.instance === direct.instance, wire);
    check('proxy: the next injected frame is numbered after it', proxy.inject(frames.internetOn, { instance: direct.instance }) === 1 && proxy.lastSeq === frame.seq + 1);
    stream.destroy();
  } finally {
    await proxy.stop();
  }
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

/** `--only app,stale,jobs` runs some of the groups of cases; the default is all of them. */
const only = args.only ? new Set(args.only.split(',')) : null;
const wants = (group) => !only || only.has(group);

async function smoke() {
  const server = await Server.start();
  const browser = await chromium.launch();
  const summary = { build: buildName, browser: browser.version(), playwright: JSON.parse(fs.readFileSync(path.join(playwrightDir, 'package.json'), 'utf8')).version };
  try {
    await proxySelftest(server);
    if (wants('app')) {
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
    }
    if (wants('stale')) {
      await caseI(browser, server);
      await caseJ(browser, server);
    }
    if (wants('jobs')) await caseK(browser, server);
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

async function selftest() {
  const server = await Server.start();
  try {
    await proxySelftest(server);
  } finally {
    await server.stop();
  }
}

try {
  await (args.measure ? measure() : args['proxy-selftest'] ? selftest() : smoke());
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
