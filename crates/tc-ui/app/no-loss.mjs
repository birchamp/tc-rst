// The browser tests of unit A4b (docs/APP-DESIGN.md section 10): the save machine, the draft
// buffer, boot.js's panic recovery, the navigation gate and the close guard, in Chromium, against
// `tc4 serve --test-hooks` and the probe page of the `test-hooks` build (`/?probe=fields`), whose
// two test fields save through `test/fieldStore`.
//
//   . ~/.tc-env && cargo build -p tc4 --features test-hooks -j 6
//   CARGO_JOBS=6 crates/tc-ui/build-app.sh --out target/tc-ui-app
//   CARGO_JOBS=6 crates/tc-ui/build-app.sh --test-hooks --out target/tc-ui-app-hooks
//   node crates/tc-ui/app/no-loss.mjs --tc4 target/debug/tc4 --ui target/tc-ui-app-hooks \
//     --ui-plain target/tc-ui-app
//
// `--only P1,P3` runs some cases. With `--measure N --out FILE` it measures instead (typing, the
// writes of a keystroke, panic to callout; see measure() below).
//
// Playwright comes from PLAYWRIGHT_MODULE, as for smoke.mjs. The page is driven as the shell
// drives it: the token in the address fragment, and in a header outside /api/ only. A stub of the
// shell bridge, where a case needs one, comes from an init script whose calls reach node through
// exposed bindings, so its log outlives the page.
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
const crateDir = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const require = createRequire(import.meta.url);
const playwrightDir = process.env.PLAYWRIGHT_MODULE ?? path.join(crateDir, '../../parity/ds-gallery/node_modules/playwright');
const { chromium } = require(playwrightDir);

const PROBE_PROJECT = 'test-fields';
const PROBE_ID = '00000000-0000-0000-0000-000000000000';
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// --- tc4 --------------------------------------------------------------------------------------
class Server {
  /** `tc4 serve --test-hooks`, on a fresh app-data folder or on the one a server before it used. */
  static async start(run = null) {
    if (!run) {
      run = path.join(path.dirname(uiDir), 'tc-ui-no-loss', `${process.pid}-${Date.now()}-${crypto.randomBytes(3).toString('hex')}`);
      fs.mkdirSync(path.join(run, 'projects'), { recursive: true });
      fs.mkdirSync(path.join(run, 'appdata'), { recursive: true });
    }
    const child = spawn(
      path.resolve(args.tc4),
      ['serve', '--app-data', path.join(run, 'appdata'), '--projects', path.join(run, 'projects'), '--ui', uiDir, '--test-hooks'],
      { stdio: ['pipe', 'pipe', 'inherit'] },
    );
    const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
    const first = await Promise.race([lines.next(), exited.then(() => ({ done: true }))]);
    if (first.done) throw new Error('tc4 ended before printing its address');
    const { url, token } = JSON.parse(first.value);
    return new Server(child, exited, run, url, token);
  }

  constructor(child, exited, run, url, token) {
    Object.assign(this, { child, exited, run, url, token });
    this.base = new URL(url);
  }

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

  command(endpoint, body) {
    return this.call(endpoint, { request: crypto.randomBytes(16).toString('hex'), ...body });
  }

  log(field = 'probe-a') {
    return this.call('test/fieldLog', { field });
  }

  async stop({ keep = false } = {}) {
    if (this.child.exitCode === null && this.child.signalCode === null) {
      this.child.kill('SIGKILL');
      await this.exited;
    }
    if (!keep) fs.rmSync(this.run, { recursive: true, force: true });
  }
}

/**
 * A proxy in front of tc4 that stands for a restarted tc4 which got the old port back: it passes
 * everything to the current server, rewriting Host and Origin, and the token header to the current
 * server's, which a reused port alone would not do. A page behind it can therefore reach a new run
 * as if nothing had changed, and only its `hello` check can tell (D8).
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
    this.http = http.createServer((req, res) => this.handle(req, res));
  }

  handle(req, res) {
    const { base, token } = this.server;
    const headers = { ...req.headers, host: base.host };
    if (headers.origin) headers.origin = base.origin;
    if (headers['x-tc-token']) headers['x-tc-token'] = token;
    const upstream = http.request({ host: base.hostname, port: base.port, method: req.method, path: req.url, headers }, (up) => {
      res.writeHead(up.statusCode, up.headers);
      res.flushHeaders();
      up.pipe(res);
    });
    upstream.on('error', () => res.destroy());
    res.on('close', () => upstream.destroy());
    req.pipe(upstream);
  }

  async stop() {
    this.http.closeAllConnections();
    await new Promise((resolve) => this.http.close(resolve));
  }
}

// --- the page ---------------------------------------------------------------------------------
/** A context that adds the token outside /api/, as the shell's header filter does (AD11). */
async function openContext(browser, server, { bridge = null, initScripts = [] } = {}) {
  const context = await browser.newContext();
  await context.route('**/*', async (route) => {
    const request = route.request();
    if (new URL(request.url()).pathname.startsWith('/api/')) return route.continue();
    return route.continue({ headers: { ...(await request.allHeaders()), 'x-tc-token': server.token } });
  });
  if (bridge) await bridge.install(context);
  for (const script of initScripts) await context.addInitScript(script);
  return context;
}

/**
 * A stub of the shell bridge (APP-DESIGN 5.7): `bufferChanged` appends to a log in node,
 * `takeBuffered` returns the last line per field with removed fields left out (a model of G1's
 * reduction of `drafts-buffer.log`), `setCanClose` records each value, and `onCloseRequested`
 * keeps the page's answer function as `window.__tc4Close`.
 */
class Bridge {
  constructor() {
    this.lines = [];
    this.canClose = [];
  }

  current() {
    const newest = new Map();
    for (const line of this.lines) {
      let parsed;
      try {
        parsed = JSON.parse(line);
      } catch {
        continue;
      }
      const key = JSON.stringify(parsed.value.key);
      if (parsed.type === 'removed') newest.delete(key);
      else newest.set(key, line);
    }
    return [...newest.values()];
  }

  async install(context) {
    await context.exposeBinding('__tc4Append', (_, line) => {
      this.lines.push(line);
    });
    await context.exposeBinding('__tc4Take', () => this.current());
    await context.exposeBinding('__tc4CanClose', (_, value) => {
      this.canClose.push(value);
    });
    await context.addInitScript(() => {
      window.tc4Shell = {
        bufferChanged: (line) => {
          window.__tc4Append(line);
        },
        takeBuffered: () => window.__tc4Take(),
        setCanClose: (value) => {
          window.__tc4CanClose(value);
        },
        onCloseRequested: (fn) => {
          window.__tc4Close = fn;
        },
      };
    });
  }
}

/** Records the page's navigations, requests, console and errors. */
function watch(page) {
  const log = { navigations: 0, requests: [], console: [], errors: [], dialogs: [] };
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) log.navigations += 1;
  });
  page.on('request', (request) => {
    const url = new URL(request.url());
    log.requests.push({ path: url.pathname, method: request.method(), at: Date.now() });
  });
  page.on('console', (message) => log.console.push({ type: message.type(), text: message.text() }));
  page.on('pageerror', (error) => log.errors.push(error.message));
  page.on('dialog', async (dialog) => {
    log.dialogs.push(dialog.type());
    // "Leave site?": leave, as a person who reloads on purpose does; the buffer keeps the text.
    await (dialog.type() === 'beforeunload' ? dialog.accept() : dialog.dismiss()).catch(() => {});
  });
  return log;
}

async function openProbe(page, url, token) {
  await page.goto(`${url}?probe=fields#t=${token}`);
  await page.waitForSelector('[data-testid=probe-field-a][data-ready=true]', { timeout: 60000 });
}

const field = (name) => `[data-testid=probe-field-${name}]`;
const indicatorState = (page) => page.getAttribute('[data-testid=probe-indicator]', 'data-state');

async function until(predicate, timeout, what) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    try {
      if (await predicate()) return;
    } catch {
      // The page or the server is between two states; ask again.
    }
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}

const texts = (log) => log.stores.map((s) => s.text);

/** Waits for `test/fieldLog`'s stores to be `want`. */
async function untilStores(server, want, what, field = 'probe-a', timeout = 10000) {
  await until(async () => JSON.stringify(texts(await server.log(field))) === JSON.stringify(want), timeout, `${what}: stores ${JSON.stringify(want)}`);
}

/** Types into a field as a person does: a click, then each key. */
async function typeInto(page, name, text) {
  await page.click(field(name));
  await page.keyboard.press('End');
  await page.keyboard.type(text);
}

/** Leaves the field: a click on the page outside it. */
async function blur(page) {
  await page.evaluate(() => document.activeElement?.blur());
}

// --- the cases --------------------------------------------------------------------------------
const results = [];
function check(name, ok, detail) {
  results.push({ name, ok: Boolean(ok), ...(detail === undefined ? {} : { detail }) });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${ok || detail === undefined ? '' : `: ${JSON.stringify(detail)}`}`);
  if (!ok) throw Object.assign(new Error(`check failed: ${name}`), { recorded: true });
}

/** After every case: no token in the page, and no console error but the panics a case causes. */
async function clean(name, page, log, allowed = []) {
  const html = await page.content().catch(() => '');
  check(`${name}: no token in the DOM`, !html.includes('st1.') && !html.includes('rt1.'));
  const unexpected = log.console
    .filter((m) => m.type === 'error')
    .filter((m) => !allowed.some((pattern) => pattern.test(m.text)));
  check(`${name}: no unexpected console error`, unexpected.length === 0, unexpected.map((m) => m.text.slice(0, 200)));
}

/** The view-error callout, with its message replaced, for the comparison of P2 and P3. */
async function calloutHtml(page) {
  return page.evaluate(() => {
    const callout = document.querySelector('[data-testid=view-error]');
    if (!callout) return null;
    const copy = callout.parentElement.cloneNode(true);
    const inner = copy.querySelector('[data-testid=view-error] > div');
    // The message is the text node between the title and the button.
    for (const node of [...inner.childNodes]) {
      if (node.nodeType === 3 && node.textContent.trim() !== '') node.textContent = ' MESSAGE ';
    }
    return copy.outerHTML;
  });
}

const PANIC_PATTERNS = [/panicked at/, /unreachable/, /RuntimeError/, /tc4 test: a panic/];

async function withPage(browser, server, options, body) {
  const context = await openContext(browser, server, options);
  const page = await context.newPage();
  const log = watch(page);
  try {
    return await body(page, log, context);
  } finally {
    await context.close();
  }
}

async function caseP0() {
  if (!args['ui-plain']) {
    check('P0: --ui-plain was given', false);
    return;
  }
  const plain = path.resolve(args['ui-plain']);
  const build = fs.readdirSync(path.join(plain, 'ui'))[0];
  const wasm = fs.readFileSync(path.join(plain, 'ui', build, 'tc_ui_bg.wasm'));
  for (const word of ['test/fieldStore', 'test/fieldDraft', 'tc4.test.panic', 'probe=fields']) {
    check(`P0: the plain UI holds no "${word}"`, !wasm.includes(Buffer.from(word)));
  }
  const hooks = fs.readFileSync(path.join(uiDir, 'ui', fs.readdirSync(path.join(uiDir, 'ui'))[0], 'tc_ui_bg.wasm'));
  check('P0: the probe build holds them (the search is not blind)', hooks.includes(Buffer.from('tc4.test.panicOnInput')));
}

/** P1 and P1b: a panic in an input handler; with `dropListener`, boot.js's listener is gone. */
async function panicInInput(browser, dropListener) {
  const name = dropListener ? 'P1b' : 'P1';
  const server = await Server.start();
  try {
    const initScripts = dropListener
      ? [
          () => {
            const add = EventTarget.prototype.addEventListener;
            EventTarget.prototype.addEventListener = function (type, fn, options) {
              if (this === window && type === 'input' && (options === true || options?.capture)) return;
              return add.call(this, type, fn, options);
            };
          },
        ]
      : [];
    await withPage(browser, server, { initScripts }, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await typeInto(page, 'a', 'draft');
      await blur(page);
      await untilStores(server, ['draft'], `${name}: the first text`);
      await page.evaluate(() => sessionStorage.setItem('tc4.test.panicOnInput', '1'));
      await typeInto(page, 'a', 'X');
      await page.waitForSelector('[data-testid=view-error]', { timeout: 20000 });
      const fieldsGone = (await page.$(field('a'))) === null;
      const message = await page.textContent('[data-testid=view-error]');
      if (dropListener) {
        await sleep(4000);
        const stored = texts(await server.log());
        check('P1b: without the capture listener the keystroke is lost', JSON.stringify(stored) === '["draft"]', stored);
      } else {
        check('P1: the callout replaces the fields after the reload', fieldsGone);
        check('P1: the callout names the panic', message.includes('a panic in an input handler'), message);
        await untilStores(server, ['draft', 'draftX'], 'P1: the panicking keystroke is saved', 'probe-a', 6000);
        await sleep(2500);
        const stored = texts(await server.log());
        check('P1: a panic in an input handler keeps the keystroke, stored once', JSON.stringify(stored) === '["draft","draftX"]', stored);
      }
      await clean(name, page, log, PANIC_PATTERNS);
    });
  } finally {
    await server.stop();
  }
}

let p2Callout = null;

async function caseP2(browser) {
  const server = await Server.start();
  try {
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await typeInto(page, 'a', 'kept');
      await page.evaluate(() => sessionStorage.setItem('tc4.test.panicOnInput', '1'));
      await page.keyboard.type('!');
      // The aborted instance's ticker traps every 20 ms until the page leaves; keep typing too.
      await page.keyboard.type('more', { delay: 10 }).catch(() => {});
      await page.waitForSelector('[data-testid=view-error]', { timeout: 20000 });
      await sleep(2000);
      check('P2: one panic with trapping closures reloads once', log.navigations === 2, log.navigations);
      const panics = JSON.parse(await page.evaluate(() => sessionStorage.getItem('tc4.panics.v1')));
      check('P2: one page load that ended in a panic is counted', Array.isArray(panics) && panics.length === 1, panics);
      const alive = await page.evaluate(() => typeof window.__tc4Probe?.state('probe-a'));
      check('P2: the callout is the running page\'s, not the static one', alive === 'string', alive);
      p2Callout = await calloutHtml(page);
      // The keys typed after the panic, before the page left, reach boot.js's listener too.
      await until(async () => (await server.log()).stores.length === 1, 6000, 'P2: the text is saved after the reload');
      const stored = texts(await server.log());
      check('P2: the text typed up to the panic and after it is saved once', stored.length === 1 && stored[0].startsWith('kept!'), stored);
      const message = await page.textContent('[data-testid=view-error]');
      check('P2: the callout names the first panic, not a later trap', message.includes('a panic in an input handler'), message);
      await page.click('[data-testid=view-error-home]');
      await page.waitForSelector(`${field('a')}[data-ready=true]`, { timeout: 10000 });
      check('P2: Back to Home shows the screen again, with no reload', log.navigations === 2, log.navigations);
      await clean('P2', page, log, PANIC_PATTERNS);
    });
  } finally {
    await server.stop();
  }
}

async function caseP3(browser) {
  const server = await Server.start();
  try {
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await typeInto(page, 'a', 'draft text');
      await page.evaluate(() => {
        sessionStorage.setItem('tc4.test.panicAtBoot', '2');
        sessionStorage.setItem('tc4.test.panicOnInput', '1');
      });
      await page.keyboard.type('!');
      await until(() => page.$('[data-testid=view-error]'), 20000, 'the static callout');
      await sleep(3000);
      check('P3: three panics, three page loads, then no more', log.navigations === 3, log.navigations);
      const alive = await page.evaluate(() => typeof window.__tc4Probe?.state('probe-a'));
      check('P3: the static callout is drawn without the WebAssembly', alive !== 'string', alive);
      const html = await calloutHtml(page);
      check('P3: the static callout has the in-app callout\'s DOM', p2Callout !== null && html === p2Callout, { static: html, app: p2Callout });
      await until(async () => (await server.log()).drafts >= 1, 5000, 'the posted draft');
      const logged = await server.log();
      check('P3: the server holds the draft', logged.draft?.text === 'draft text!' && logged.drafts === 1, logged.draft);
      check('P3: and stored nothing', logged.stores.length === 0, texts(logged));
      await page.click('[data-testid=view-error-home]');
      await page.waitForSelector(`${field('a')}[data-ready=true]`, { timeout: 20000 });
      check('P3: Back to Home reloads once and the page boots', log.navigations === 4, log.navigations);
      await untilStores(server, ['draft text!'], 'P3: then the text is saved', 'probe-a', 6000);
      await clean('P3', page, log, PANIC_PATTERNS);
    });
  } finally {
    await server.stop();
  }
}

async function caseP4(browser) {
  const server = await Server.start();
  try {
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await server.command('test/hold', { at: 'save' });
      await typeInto(page, 'a', 'T1');
      await blur(page);
      await until(async () => (await indicatorState(page)) === 'saving', 5000, 'saving');
      await page.reload();
      await page.waitForSelector(`${field('a')}[data-ready=true]`, { timeout: 20000 });
      await server.command('test/release', { at: 'save' });
      await untilStores(server, ['T1'], 'P4');
      await until(async () => (await indicatorState(page)) === 'saved', 5000, 'saved');
      await sleep(1000);
      const logged = await server.log();
      check('P4: a reload with a save in flight stores once, over the current version', JSON.stringify(logged.stores.map((s) => [s.text, s.base])) === '[["T1","current"]]', logged.stores);
      check('P4: the field shows the text', (await page.inputValue(field('a'))) === 'T1');
      await clean('P4', page, log);
    });
  } finally {
    await server.stop();
  }
}

async function caseP5(browser) {
  const server = await Server.start();
  try {
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await server.command('test/hold', { at: 'save' });
      await typeInto(page, 'a', 'T1');
      await blur(page);
      await until(async () => (await indicatorState(page)) === 'saving', 5000, 'saving');
      await typeInto(page, 'a', ' T2');
      await page.reload();
      await page.waitForSelector(`${field('a')}[data-ready=true]`, { timeout: 20000 });
      check('P5: after the reload the field shows the newer text', (await page.inputValue(field('a'))) === 'T1 T2');
      await server.command('test/release', { at: 'save' });
      await untilStores(server, ['T1'], 'P5: the first save');
      const early = await indicatorState(page);
      check('P5: the indicator is not "saved" before the newer text is stored', early !== 'saved', early);
      await untilStores(server, ['T1', 'T1 T2'], 'P5: the newer text');
      const logged = await server.log();
      check('P5: both stored in order, over the current version', JSON.stringify(logged.stores.map((s) => s.base)) === '["current","current"]', logged.stores);
      await until(async () => (await indicatorState(page)) === 'saved', 5000, 'saved');
      await clean('P5', page, log);
    });
  } finally {
    await server.stop();
  }
}

async function crashThenRestart(browser, at) {
  const name = `P6 (${at})`;
  const first = await Server.start();
  let second = null;
  const bridge = new Bridge();
  try {
    await withPage(browser, first, { bridge }, async (page, log) => {
      await openProbe(page, first.url, first.token);
      await first.command('test/crash', { at, path: 'test/fieldStore' });
      await typeInto(page, 'a', 'Typed before the crash');
      await blur(page);
      const ended = await first.exited;
      check(`${name}: tc4 ended at the crash point`, ended.code === 86, ended);
      await sleep(500);
      await clean(name, page, log, [/Failed to load resource/, /ERR_CONNECTION_REFUSED/, /net::/]);
    });
    second = await Server.start(first.run);
    await withPage(browser, second, { bridge }, async (page, log) => {
      await openProbe(page, second.url, second.token);
      const buffered = JSON.parse(await page.evaluate(([project, id]) => window.__tc4Probe.buffered(project, id), [PROBE_PROJECT, PROBE_ID]));
      const entry = buffered.find(([key]) => key.field.place === 'probe-a')?.[1];
      check(`${name}: the next page at a new origin is offered the entry from the shell's mirror`, entry?.text === 'Typed before the crash', buffered);
      check(`${name}: the entry names its save and its run`, entry?.sent?.text === 'Typed before the crash' && entry.instance !== (await second.call('app/hello')).instance, entry);
      const stored = texts(await second.log());
      const want = at === 'beforeReply' ? ['Typed before the crash'] : [];
      check(`${name}: the store holds the text ${want.length ? 'once' : 'not at all'}`, JSON.stringify(stored) === JSON.stringify(want), stored);
      await sleep(3000);
      check(`${name}: and nothing is resent`, JSON.stringify(texts(await second.log())) === JSON.stringify(want));
      const stores = log.requests.filter((r) => r.path === '/api/v1/test/fieldStore');
      check(`${name}: the new page sent no save`, stores.length === 0, stores.length);
      await clean(name, page, log);
    });
  } finally {
    await first.stop({ keep: true });
    if (second) await second.stop();
    else fs.rmSync(first.run, { recursive: true, force: true });
  }
}

async function caseP7(browser) {
  const server = await Server.start();
  const file = path.join(server.run, 'appdata', 'test-hooks', 'fields.json');
  try {
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      const original = fs.readFileSync(file);
      // A non-empty folder in the file's place: the rename over it fails, for any user.
      fs.rmSync(file);
      fs.mkdirSync(file);
      fs.writeFileSync(path.join(file, 'keep'), 'x');
      await typeInto(page, 'a', 'T1');
      await blur(page);
      await until(async () => (await indicatorState(page)) === 'error', 5000, 'error');
      check('P7: a failed save shows Retry', (await page.$('[data-testid=probe-retry]')) !== null);
      await page.click('[data-testid=probe-leave]');
      await sleep(1500);
      check('P7: a failed save holds the navigation gate', (await page.$('[data-testid=probe-left]')) === null);
      check('P7: the gate retried once and nothing more', (await indicatorState(page)) === 'error');
      fs.rmSync(file, { recursive: true });
      fs.writeFileSync(file, original);
      await page.click('[data-testid=probe-retry]');
      await until(async () => (await indicatorState(page)) === 'saved', 5000, 'saved after Retry');
      await typeInto(page, 'a', ' T2');
      await page.click('[data-testid=probe-leave]');
      await page.waitForSelector('[data-testid=probe-left]', { timeout: 10000 });
      const stored = texts(await server.log());
      check('P7: the gate let the person go only after the newer text was stored', JSON.stringify(stored) === '["T1","T1 T2"]', stored);
      await clean('P7', page, log);
    });
  } finally {
    await server.stop();
  }
}

async function caseP8(browser) {
  const server = await Server.start();
  try {
    const bridge = new Bridge();
    await withPage(browser, server, { bridge }, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await sleep(300);
      check('P8: setCanClose(true) at boot', bridge.canClose[0] === true, bridge.canClose);
      await typeInto(page, 'a', 'unsaved');
      await until(() => bridge.canClose.at(-1) === false, 3000, 'setCanClose(false)');
      check('P8: setCanClose(false) after typing', true);
      const answer = await page.evaluate(() => window.__tc4Close());
      check('P8: a close request saves first and answers true', answer === true, answer);
      check('P8: the text was stored before the answer', JSON.stringify(texts(await server.log())) === '["unsaved"]');
      check('P8: setCanClose(true) after the save', bridge.canClose.at(-1) === true, bridge.canClose);
      const file = path.join(server.run, 'appdata', 'test-hooks', 'fields.json');
      const original = fs.readFileSync(file);
      fs.rmSync(file);
      fs.mkdirSync(file);
      fs.writeFileSync(path.join(file, 'keep'), 'x');
      await typeInto(page, 'a', ' and failed');
      const refused = await page.evaluate(() => window.__tc4Close());
      check('P8: a close request while a save fails answers false', refused === false, refused);
      check('P8: setCanClose(false) while failed', bridge.canClose.at(-1) === false, bridge.canClose);
      fs.rmSync(file, { recursive: true });
      fs.writeFileSync(file, original);
      const beforeunload = log.dialogs.filter((d) => d === 'beforeunload');
      check('P8: no beforeunload under the shell (tC4 #356)', beforeunload.length === 0, log.dialogs);
      await clean('P8', page, log);
    });
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await typeInto(page, 'b', 'unsaved in a browser');
      await page.close({ runBeforeUnload: true });
      await until(() => log.dialogs.includes('beforeunload'), 3000, 'the beforeunload dialog');
      check('P8: without a bridge, closing with unsaved text asks', true);
    });
    await withPage(browser, server, {}, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await typeInto(page, 'b', 'x');
      await blur(page);
      await until(async () => (await indicatorState(page)) === 'saved', 5000, 'saved');
      await page.close({ runBeforeUnload: true });
      await sleep(500);
      check('P8: and closing with everything saved does not', !log.dialogs.includes('beforeunload'), log.dialogs);
    });
  } finally {
    await server.stop();
  }
}

async function caseP9(browser) {
  const first = await Server.start();
  const proxy = await Proxy.start(first);
  let second = null;
  try {
    await withPage(browser, first, {}, async (page, log) => {
      await openProbe(page, proxy.url, first.token);
      await first.command('test/crash', { at: 'beforeReply', path: 'test/fieldStore' });
      await typeInto(page, 'a', 'Stored once');
      await blur(page);
      await first.exited;
      // The page's save is a lost reply now, and it is sent again after its backoff.
      await sleep(600);
      const crashedAt = Date.now();
      second = await Server.start(first.run);
      proxy.server = second;
      // The lost reply's resend first asks app/hello, which names another run: the field is
      // dropped and nothing is sent (D8). A page that has read no frame cannot tell the new run
      // from a reconnect by its event stream alone; the check before a resend does not need it.
      await until(async () => (await page.evaluate(() => window.__tc4Probe.state('probe-a'))) === 'none', 20000, 'the field is dropped');
      await sleep(3000);
      const stored = texts(await second.log());
      check('P9: a restart behind the same origin resends nothing', JSON.stringify(stored) === '["Stored once"]', stored);
      const resends = log.requests.filter((r) => r.path === '/api/v1/test/fieldStore' && r.at > crashedAt);
      const hellos = log.requests.filter((r) => r.path === '/api/v1/app/hello' && r.at > crashedAt);
      check('P9: no save reached the new run', resends.length === 0, resends.length);
      check('P9: the page asked app/hello before any resend', hellos.length >= 1, hellos.length);
      const kept = await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('tc4.drafts.v2:')).length);
      check('P9: the entry stays in the storage for the offer', kept === 1, kept);
      await page.reload();
      await page.waitForSelector(`${field('a')}[data-ready=true]`, { timeout: 20000 });
      await sleep(3000);
      check('P9: a reload restores nothing of the dead run', JSON.stringify(texts(await second.log())) === '["Stored once"]');
      check('P9: and the field shows the stored text', (await page.inputValue(field('a'))) === 'Stored once');
      await clean('P9', page, log, [/Failed to load resource/, /net::/, /hello could not be read/, /event stream/]);
    });
  } finally {
    await proxy.stop();
    await first.stop({ keep: true });
    if (second) await second.stop();
    else fs.rmSync(first.run, { recursive: true, force: true });
  }
}

async function caseP10(browser) {
  const server = await Server.start();
  try {
    const initScripts = [
      () => {
        const set = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
          // The test triggers still get in; everything else of the page is refused.
          if (this === window.sessionStorage && !String(key).startsWith('tc4.test.')) throw new DOMException('blocked', 'SecurityError');
          return set.call(this, key, value);
        };
      },
    ];
    await withPage(browser, server, { initScripts }, async (page, log) => {
      await openProbe(page, server.url, server.token);
      await page.evaluate(() => sessionStorage.setItem('tc4.test.panicOnInput', '1'));
      await typeInto(page, 'a', 'x');
      await page.waitForSelector('[data-testid=view-error]', { timeout: 20000 });
      await sleep(2000);
      check('P10: with sessionStorage blocked a panic stops at once, with no reload', log.navigations === 1, log.navigations);
      await clean('P10', page, log, [...PANIC_PATTERNS, /sessionStorage/]);
    });
  } finally {
    await server.stop();
  }
}

// --- measurements -----------------------------------------------------------------------------
function percentile(sorted, p) {
  if (sorted.length === 0) return null;
  return sorted[Math.min(sorted.length - 1, Math.ceil((p / 100) * sorted.length) - 1)];
}

function stats(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return { n: sorted.length, median: percentile(sorted, 50), p95: percentile(sorted, 95), max: sorted.at(-1) ?? null };
}

/** Input to frame per keystroke: from the keydown's timeStamp to the next animation frame. */
async function typingRun(page, start, keys) {
  await page.evaluate((text) => {
    const area = document.querySelector('[data-testid=probe-field-a]');
    area.value = text;
    area.dispatchEvent(new Event('input', { bubbles: true }));
  }, start);
  await page.click(field('a'));
  await page.keyboard.press('End');
  await page.evaluate(() => {
    window.__frames = [];
    window.tc4Boot.measure = [];
    window.__onKey = (event) => {
      const at = event.timeStamp;
      requestAnimationFrame(() => window.__frames.push(performance.now() - at));
    };
    window.addEventListener('keydown', window.__onKey, true);
  });
  for (let i = 0; i < keys; i += 1) {
    await page.keyboard.type(String.fromCharCode(97 + (i % 26)));
    await sleep(30);
  }
  await sleep(100);
  return page.evaluate(() => {
    window.removeEventListener('keydown', window.__onKey, true);
    const out = { frames: window.__frames, listener: window.tc4Boot.measure };
    window.tc4Boot.measure = null;
    return out;
  });
}

async function measure() {
  const runs = Number(args.measure) || 20;
  const browser = await chromium.launch();
  const out = { build: path.basename(uiDir), browser: browser.version(), node: process.version, load: os.loadavg(), typing: {}, writes: {}, panic: {} };
  try {
    for (const [label, size] of [['2KB', 2048], ['20KB', 20480]]) {
      const frames = [];
      const listener = [];
      const server = await Server.start();
      try {
        await withPage(browser, server, {}, async (page) => {
          await openProbe(page, server.url, server.token);
          for (let run = 0; run < runs; run += 1) {
            const result = await typingRun(page, 'x'.repeat(size), 200);
            frames.push(...result.frames);
            listener.push(...result.listener);
            await blur(page);
          }
        });
      } finally {
        await server.stop();
      }
      out.typing[label] = { inputToFrame: stats(frames), captureListener: stats(listener) };
    }
    // Without boot.js's capture listener, to isolate its cost.
    {
      const server = await Server.start();
      const frames = [];
      try {
        const initScripts = [
          () => {
            const add = EventTarget.prototype.addEventListener;
            EventTarget.prototype.addEventListener = function (type, fn, options) {
              if (this === window && type === 'input' && (options === true || options?.capture)) return;
              return add.call(this, type, fn, options);
            };
          },
        ];
        await withPage(browser, server, { initScripts }, async (page) => {
          await openProbe(page, server.url, server.token);
          for (let run = 0; run < runs; run += 1) {
            frames.push(...(await typingRun(page, 'x'.repeat(2048), 200)).frames);
            await blur(page);
          }
        });
      } finally {
        await server.stop();
      }
      out.typing.withoutListener2KB = { inputToFrame: stats(frames) };
    }
    // The writes of a keystroke, and 10 s of typing.
    {
      const server = await Server.start();
      const bridge = new Bridge();
      try {
        await withPage(browser, server, { bridge, initScripts: [
          () => {
            window.__sets = 0;
            const set = Storage.prototype.setItem;
            Storage.prototype.setItem = function (key, value) {
              if (this === window.localStorage) window.__sets += 1;
              return set.call(this, key, value);
            };
          },
        ] }, async (page, log) => {
          await openProbe(page, server.url, server.token);
          await page.click(field('a'));
          await page.keyboard.type('a');
          await sleep(50);
          const setsBefore = await page.evaluate(() => window.__sets);
          const linesBefore = bridge.lines.length;
          await page.keyboard.type('b');
          await sleep(50);
          out.writes.localStorageSetsPerKeystroke = (await page.evaluate(() => window.__sets)) - setsBefore;
          out.writes.mirrorLinesPerKeystroke = bridge.lines.length - linesBefore;
          const requestsBefore = log.requests.length;
          for (let i = 0; i < 100; i += 1) {
            await page.keyboard.type(String.fromCharCode(97 + (i % 26)));
            await sleep(100);
          }
          const during = log.requests.slice(requestsBefore);
          out.writes.savesIn10sOfTyping = during.filter((r) => r.path === '/api/v1/test/fieldStore').length;
          out.writes.draftsIn10sOfTyping = during.filter((r) => r.path === '/api/v1/test/fieldDraft').length;
        });
      } finally {
        await server.stop();
      }
    }
    // From the panic to the in-app callout after the reload.
    {
      const times = [];
      const server = await Server.start();
      try {
        for (let run = 0; run < runs; run += 1) {
          await withPage(browser, server, {}, async (page) => {
            await openProbe(page, server.url, server.token);
            await page.click(field('b'));
            await page.evaluate(() => sessionStorage.setItem('tc4.test.panicOnInput', '1'));
            const started = Date.now();
            await page.keyboard.type('x');
            await page.waitForSelector('[data-testid=view-error]', { timeout: 20000 });
            times.push(Date.now() - started);
            await page.evaluate(() => sessionStorage.clear());
          });
        }
      } finally {
        await server.stop();
      }
      out.panic.toCalloutMs = stats(times);
    }
    const wasm = fs.statSync(path.join(uiDir, 'ui', fs.readdirSync(path.join(uiDir, 'ui'))[0], 'tc_ui_bg.wasm')).size;
    out.wasmBytes = wasm;
    if (args['ui-plain']) {
      const plain = path.resolve(args['ui-plain']);
      out.plainWasmBytes = fs.statSync(path.join(plain, 'ui', fs.readdirSync(path.join(plain, 'ui'))[0], 'tc_ui_bg.wasm')).size;
    }
    out.loadAfter = os.loadavg();
    if (args.out) fs.writeFileSync(args.out, `${JSON.stringify(out, null, 2)}\n`);
    console.log(JSON.stringify(out, null, 2));
  } finally {
    await browser.close();
  }
}

// --- main -------------------------------------------------------------------------------------
const only = args.only ? new Set(args.only.split(',')) : null;
const wants = (name) => !only || only.has(name);

async function noLoss() {
  const browser = await chromium.launch();
  const summary = { browser: browser.version() };
  const cases = [
    ['P0', () => caseP0()],
    ['P1', () => panicInInput(browser, false)],
    ['P1b', () => panicInInput(browser, true)],
    ['P2', () => caseP2(browser)],
    ['P3', () => caseP3(browser)],
    ['P4', () => caseP4(browser)],
    ['P5', () => caseP5(browser)],
    ['P6', async () => {
      await crashThenRestart(browser, 'beforeReply');
      await crashThenRestart(browser, 'beforeStore');
    }],
    ['P7', () => caseP7(browser)],
    ['P8', () => caseP8(browser)],
    ['P9', () => caseP9(browser)],
    ['P10', () => caseP10(browser)],
  ];
  try {
    for (const [name, run] of cases) {
      if (!wants(name)) continue;
      // P3 compares its callout with P2's.
      if (name === 'P3' && p2Callout === null) await caseP2(browser);
      try {
        await run();
      } catch (error) {
        if (!error?.recorded) {
          results.push({ name: `${name} threw: ${String(error?.message ?? error).split('\n')[0]}`, ok: false });
          console.log(`FAIL ${name} threw: ${error?.stack ?? error}`);
        }
      }
    }
  } finally {
    await browser.close();
    summary.checks = results.length;
    summary.failed = results.filter((r) => !r.ok).map((r) => r.name);
    console.log(JSON.stringify(summary, null, 2));
    if (summary.failed.length) process.exitCode = 1;
  }
}

try {
  await (args.measure ? measure() : noLoss());
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
