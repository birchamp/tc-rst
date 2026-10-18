// Proves the first-launch window in Chromium (docs/APP-DESIGN.md §10, unit A9): the window shows
// "Preparing resources: n of total" while `tc4 serve --bundle` seeds, Close cancels the seed and
// leaves only whole installs, the next launch finishes it and hands off to Home, a repeat launch
// shows no count, and a damaged bundle shows tC4's failure words with the reason.
//
//   cargo build -p tc4 --features test-hooks -j 2
//   cargo run -p tc-app --example bundle_store -j 2 -- --from bundle/en \
//     --out target/a9/bundle-smoke --only en_tw,en_obs,en_obs-tn,en_obs-tq,en_obs-twl
//   node crates/tc-ui/app/prepare-smoke.mjs --tc4 target/debug/tc4 \
//     --bundle target/a9/bundle-smoke --out docs/evidence/first-launch-seeding-2026-10-18
//
// The page is served the way the shell serves it (AD11): a request filter adds the token to what
// the browser loads by itself, and leaves `/api/` out, where the page sends the token itself.
// Playwright comes from PLAYWRIGHT_MODULE (default: the parity harness's copy), its Chromium
// from PLAYWRIGHT_BROWSERS_PATH. Every wait has a timeout, and a `finally` quits or kills `tc4`.
import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, arg, i, all) => {
    if (arg.startsWith('--')) pairs.push([arg.slice(2), all[i + 1]]);
    return pairs;
  }, []),
);
for (const required of ['tc4', 'bundle', 'out']) {
  if (!args[required]) throw new Error(`--${required} is required`);
}
const tc4Path = path.resolve(args.tc4);
const bundle = path.resolve(args.bundle);
const outDir = path.resolve(args.out);
const work = path.join(root, 'target', 'a9');
const pace = Number(args.pace ?? 4);

const require = createRequire(import.meta.url);
const playwrightDir =
  process.env.PLAYWRIGHT_MODULE ?? '/home/user/tc-rst/parity/ds-gallery/node_modules/playwright';
const { chromium } = require(playwrightDir);
const playwrightVersion = JSON.parse(fs.readFileSync(path.join(playwrightDir, 'package.json'), 'utf8')).version;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const round = (x) => Math.round(x);

/** Polls `read` until it returns something truthy, or fails after `ms`. */
async function until(what, ms, read) {
  const stop = Date.now() + ms;
  for (;;) {
    const value = await read();
    if (value) return value;
    if (Date.now() > stop) throw new Error(`timed out after ${ms} ms: ${what}`);
    await sleep(20);
  }
}

function check(condition, message) {
  if (!condition) throw new Error(`FAILED: ${message}`);
}

// --- the folders ------------------------------------------------------------------------------
fs.mkdirSync(work, { recursive: true });
const ui = path.join(work, 'ui');
fs.rmSync(ui, { recursive: true, force: true });
fs.mkdirSync(ui, { recursive: true });
fs.copyFileSync(path.join(here, 'prepare.html'), path.join(ui, 'prepare.html'));
fs.writeFileSync(path.join(ui, 'index.html'), '<!doctype html><title>home</title><p id="home">home</p>');
fs.mkdirSync(outDir, { recursive: true });

let runs = 0;
/** A fresh app-data and projects folder for a run. */
function newRun() {
  const dir = path.join(work, `run-${++runs}`);
  fs.rmSync(dir, { recursive: true, force: true });
  const run = { dir, appData: path.join(dir, 'app-data'), projects: path.join(dir, 'projects') };
  // The projects folder is created first: a named folder that is missing stops the start.
  fs.mkdirSync(run.projects, { recursive: true });
  fs.mkdirSync(run.appData, { recursive: true });
  return run;
}

// --- tc4 --------------------------------------------------------------------------------------
class Tc4 {
  static async start(run, { bundleDir, seedPace } = {}) {
    const extra = [];
    if (bundleDir) extra.push('--bundle', bundleDir);
    if (seedPace !== undefined) extra.push('--test-hooks', '--test-seed-pace', String(seedPace));
    // stdin stays open: a closed stdin makes tc4 quit, as the shell's Close does.
    const child = spawn(
      tc4Path,
      ['serve', '--app-data', run.appData, '--projects', run.projects, '--ui', ui, ...extra],
      { stdio: ['pipe', 'pipe', 'pipe'] },
    );
    const tc4 = new Tc4(child);
    const first = await Promise.race([
      tc4.lines.next(),
      tc4.exited.then(() => ({ done: true })),
      sleep(30000).then(() => ({ timeout: true })),
    ]);
    if (first.done || first.timeout) throw new Error(`tc4 gave no address line: ${tc4.stderr}`);
    const { url, token } = JSON.parse(first.value);
    tc4.url = url;
    tc4.token = token;
    tc4.base = new URL(url);
    return tc4;
  }

  constructor(child) {
    this.child = child;
    this.stderr = '';
    child.stderr.on('data', (chunk) => { this.stderr += chunk; });
    this.lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
    this.exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
    this.done = false;
    this.exited.then(() => { this.done = true; });
  }

  /** What the shell does on Close: `{"quit":true}`, then `{"closed":true}` and the exit. */
  async quit() {
    const began = performance.now();
    this.child.stdin.write('{"quit":true}\n');
    const closed = await Promise.race([
      this.lines.next(),
      sleep(10000).then(() => ({ timeout: true })),
    ]);
    check(!closed.timeout && closed.value === '{"closed":true}', `tc4 answered ${JSON.stringify(closed)} to the quit`);
    const closedAfter = performance.now() - began;
    const exit = await Promise.race([this.exited, sleep(10000).then(() => ({ timeout: true }))]);
    check(!exit.timeout, 'tc4 did not exit after closed');
    return { closedMs: round(closedAfter), exitMs: round(performance.now() - began), exit };
  }

  kill() {
    if (!this.done) this.child.kill('SIGKILL');
  }

  /** A POST to `/api/v1/<path>` the way the UI sends it. */
  api(pathName, body = {}) {
    const text = JSON.stringify(body);
    return new Promise((resolve, reject) => {
      const request = http.request(
        {
          host: this.base.hostname,
          port: this.base.port,
          method: 'POST',
          path: `/api/v1/${pathName}`,
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
            if (response.statusCode !== 200) return reject(new Error(`${pathName}: HTTP ${response.statusCode}`));
            const reply = JSON.parse(Buffer.concat(parts).toString('utf8'));
            if (reply.type !== 'ok') return reject(new Error(`${pathName}: ${JSON.stringify(reply)}`));
            resolve(reply.value);
          });
        },
      );
      request.on('error', reject);
      request.end(text);
    });
  }

  async hello() {
    return this.api('app/hello');
  }

  /** A fresh write id (32 hex digits): the reply cache answers a reused one with the first reply. */
  static request() {
    return crypto.randomBytes(16).toString('hex');
  }

  prepareUrl() {
    return `${this.url}prepare.html#t=${this.token}`;
  }
}

// --- the disk ---------------------------------------------------------------------------------
const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

/** The install folders of a store: `dcs/<owner>/<repo>/<sha>`. */
function installDirs(store) {
  const found = [];
  const dcs = path.join(store, 'dcs');
  if (!fs.existsSync(dcs)) return found;
  for (const owner of fs.readdirSync(dcs)) {
    for (const repo of fs.readdirSync(path.join(dcs, owner))) {
      for (const sha of fs.readdirSync(path.join(dcs, owner, repo))) {
        found.push(path.join(dcs, owner, repo, sha));
      }
    }
  }
  return found.sort();
}

/** Recomputes every listed file of an install against its receipt. Returns the number of files. */
function verifyInstall(dir) {
  const receipt = JSON.parse(fs.readFileSync(path.join(dir, 'receipt.json'), 'utf8'));
  for (const file of receipt.files) {
    const bytes = fs.readFileSync(path.join(dir, 'content', file.path));
    check(bytes.length === file.size, `${dir}: ${file.path} has ${bytes.length} bytes, the receipt says ${file.size}`);
    check(sha256(bytes) === file.sha256, `${dir}: ${file.path} does not match its receipt`);
  }
  check(fs.existsSync(path.join(dir, 'checked.json')), `${dir} has no check record`);
  return receipt.files.length;
}

/** What identifies an install's bytes and times, to see that a later start left it alone. */
function installFingerprint(dir) {
  const receipt = fs.readFileSync(path.join(dir, 'receipt.json'));
  const files = JSON.parse(receipt.toString('utf8')).files.map((file) => {
    const full = path.join(dir, 'content', file.path);
    return [file.path, fs.statSync(full).mtimeMs];
  });
  return { receipt: sha256(receipt), files };
}

const bundledInstalls = installDirs(bundle);
const bundledFiles = bundledInstalls.reduce(
  (sum, dir) => sum + JSON.parse(fs.readFileSync(path.join(dir, 'receipt.json'), 'utf8')).files.length,
  0,
);

// --- the browser ------------------------------------------------------------------------------
const results = {
  when: new Date().toISOString(),
  machine: { cpu: os.cpus()[0]?.model, cpus: os.cpus().length, platform: `${os.platform()} ${os.release()}`, node: process.version },
  playwright: playwrightVersion,
  bundle: { installs: bundledInstalls.length, files: bundledFiles, seedPaceMs: pace },
  steps: {},
};

/** A page like the shell's window: 520 × 240, with the token added to what is not `/api/`. */
async function newWindow(browser, tc4) {
  const context = await browser.newContext({ viewport: { width: 520, height: 240 } });
  await context.route(
    (url) => !url.pathname.startsWith('/api/'),
    async (route) => {
      // `allHeaders()`, not `headers()`: the latter leaves out what the network stack adds.
      const headers = { ...(await route.request().allHeaders()), 'x-tc-token': tc4.token };
      await route.continue({ headers });
    },
  );
  const page = await context.newPage();
  // Every text the status line takes, kept across navigations by a binding.
  const texts = [];
  await page.exposeBinding('tc4Text', (_source, text) => { texts.push(text); });
  await page.addInitScript(() => {
    let last = null;
    const note = () => {
      const text = document.getElementById('progress')?.textContent;
      if (text !== undefined && text !== last) {
        last = text;
        window.tc4Text(text);
      }
    };
    new MutationObserver(note).observe(document, { childList: true, subtree: true, characterData: true });
    document.addEventListener('DOMContentLoaded', note);
  });
  return { context, page, texts };
}

const progressOf = async (page) => {
  const text = await page.locator('#progress').textContent();
  const match = /^Preparing resources: (\d+) of (\d+)$/.exec(text ?? '');
  return match ? { done: Number(match[1]), total: Number(match[2]), text } : null;
};

const live = [];
const track = (tc4) => { live.push(tc4); return tc4; };

let browser;
try {
  browser = await chromium.launch();
  results.chromium = browser.version();

  // --- 1 and 2 and 3: the first launch, the saved language, Close --------------------------------
  const firstRun = newRun();
  let tc4 = track(await Tc4.start(firstRun, { bundleDir: bundle, seedPace: pace }));
  const startup = (await tc4.hello()).startup;
  check(startup.type === 'preparing' && startup.value.total === bundledFiles,
    `the first hello says ${JSON.stringify(startup)}, not preparing with ${bundledFiles} files`);
  const total = startup.value.total;

  let { context, page } = await newWindow(browser, tc4);
  await page.goto(tc4.prepareUrl());
  check((await page.title()) === 'translationCore4', 'the title');
  check((await page.locator('h1').textContent()) === 'Preparing translationCore4', 'the heading');
  check((await page.locator('p').first().textContent()) === 'Getting resources ready for offline use.', 'the lead');
  check(await page.evaluate(() => document.documentElement.lang === 'en' && document.documentElement.dir === 'ltr'), 'lang en, dir ltr');
  const grown = await until('a count of 1 or more', 30000, async () => {
    const progress = await progressOf(page);
    return progress && progress.done >= 1 ? progress : null;
  });
  check(grown.total === total, `the window says ${grown.total} files, hello said ${total}`);
  await sleep(300);
  const later = await progressOf(page);
  check(later && later.total === total && later.done > grown.done, `the count grew: ${grown.text} then ${later?.text}`);
  await page.screenshot({ path: path.join(outDir, 'prepare-en.png') });
  results.steps['1-first-launch'] = { total, first: grown.text, later: later.text };

  await tc4.api('prefs/setLocale', { request: Tc4.request(), locale: 'ar' });
  await page.reload();
  const arabic = await until('the Arabic window', 30000, async () => {
    const lang = await page.evaluate(() => [document.documentElement.lang, document.documentElement.dir]);
    const progress = await progressOf(page);
    return lang[0] === 'ar' && lang[1] === 'rtl' && progress ? { lang, progress } : null;
  });
  check(arabic.progress.total === total, 'the total after the reload');
  check(await page.locator('h1').textContent() === 'Preparing translationCore4', 'the words are still English while pending (Q-10)');
  await sleep(300);
  const arabicLater = await progressOf(page);
  check(arabicLater && arabicLater.done > arabic.progress.done, `a reload does not cancel the seed: ${arabic.progress.text} then ${arabicLater?.text}`);
  await page.screenshot({ path: path.join(outDir, 'prepare-ar.png') });
  results.steps['2-saved-language'] = { lang: arabic.lang, first: arabic.progress.text, later: arabicLater.text };
  await tc4.api('prefs/setLocale', { request: Tc4.request(), locale: 'en' });

  await page.close();
  await context.close();
  const closing = await tc4.quit();
  check(closing.exit.code === 0, `tc4 exited with ${JSON.stringify(closing.exit)}`);
  check(closing.closedMs < 5000, `Close took ${closing.closedMs} ms`);
  const whole = installDirs(path.join(firstRun.appData, 'resources'));
  check(whole.length < bundledInstalls.length, `the seed was not cut: ${whole.length} of ${bundledInstalls.length} installs`);
  const wholeFiles = whole.map(verifyInstall).reduce((a, b) => a + b, 0);
  const before = whole.map((dir) => [path.relative(firstRun.appData, dir), installFingerprint(dir)]);
  results.steps['3-close'] = {
    closedMs: closing.closedMs,
    exitMs: closing.exitMs,
    exitCode: closing.exit.code,
    wholeInstalls: whole.length,
    ofInstalls: bundledInstalls.length,
    filesInWholeInstalls: wholeFiles,
    partialLeft: fs.existsSync(path.join(firstRun.appData, 'resources', '.partial')),
  };

  // --- 4: the relaunch finishes the seed and hands off to Home ------------------------------------
  tc4 = track(await Tc4.start(firstRun, { bundleDir: bundle }));
  const resumed = (await tc4.hello()).startup;
  const missing = bundledFiles - wholeFiles;
  check(resumed.type === 'ready' || (resumed.type === 'preparing' && resumed.value.total === missing),
    `the relaunch plans ${JSON.stringify(resumed)}, not ${missing} files`);
  ({ context, page } = await newWindow(browser, tc4));
  await page.goto(tc4.prepareUrl());
  await page.waitForSelector('#home', { timeout: 60000 });
  check(new URL(page.url()).pathname === '/', `the window ended at ${page.url()}`);
  const finished = installDirs(path.join(firstRun.appData, 'resources'));
  check(finished.length === bundledInstalls.length, `${finished.length} installs after the relaunch`);
  finished.forEach(verifyInstall);
  for (const [relative, print] of before) {
    const now = installFingerprint(path.join(firstRun.appData, relative));
    check(JSON.stringify(now) === JSON.stringify(print), `${relative} changed after the relaunch`);
  }
  check((await tc4.hello()).startup.type === 'ready', 'hello says ready after the hand-off');
  results.steps['4-relaunch'] = {
    planned: resumed.type === 'preparing' ? resumed.value.total : 0,
    missingFiles: missing,
    reachedHome: true,
    installs: finished.length,
    earlierCopiesUnchanged: before.length,
  };
  await context.close();
  check((await tc4.quit()).exit.code === 0, 'the relaunch quit');

  // --- 5: a repeat launch shows no count ------------------------------------------------------------
  tc4 = track(await Tc4.start(firstRun, { bundleDir: bundle }));
  check((await tc4.hello()).startup.type === 'ready', 'a repeat launch says ready at once');
  let repeat = await newWindow(browser, tc4);
  const began = performance.now();
  await repeat.page.goto(tc4.prepareUrl());
  await repeat.page.waitForSelector('#home', { timeout: 2000 });
  const handoffMs = round(performance.now() - began);
  check(new URL(repeat.page.url()).pathname === '/', 'the repeat window is on Home');
  check(!repeat.texts.some((text) => /Preparing resources/.test(text)), `a count was shown: ${JSON.stringify(repeat.texts)}`);
  results.steps['5-repeat-launch'] = { handoffMs, statusTexts: repeat.texts };
  await repeat.context.close();
  check((await tc4.quit()).exit.code === 0, 'the repeat launch quit');

  // --- 6: a damaged bundle fails the start with the reason ------------------------------------------
  const damaged = path.join(work, 'bundle-damaged');
  fs.rmSync(damaged, { recursive: true, force: true });
  fs.cpSync(bundle, damaged, { recursive: true });
  const victim = installDirs(damaged).find((dir) => dir.includes('en_obs-tq'));
  check(victim, 'the smoke bundle holds en_obs-tq');
  const receipt = JSON.parse(fs.readFileSync(path.join(victim, 'receipt.json'), 'utf8'));
  const victimFile = path.join(victim, 'content', receipt.files[0].path);
  const bytes = fs.readFileSync(victimFile);
  bytes[0] ^= 1;
  fs.writeFileSync(victimFile, bytes);

  const failedRun = newRun();
  tc4 = track(await Tc4.start(failedRun, { bundleDir: damaged }));
  ({ context, page } = await newWindow(browser, tc4));
  await page.goto(tc4.prepareUrl());
  const failed = await until('the failure window', 30000, async () => {
    const h1 = await page.locator('h1').textContent();
    return h1 === 'translationCore4 could not start'
      ? { h1, lead: await page.locator('p').first().textContent(), reason: await page.locator('#progress').textContent() }
      : null;
  });
  check(failed.lead === 'The bundled resources could not be prepared. Please quit and try again.', `the failure sentence: ${failed.lead}`);
  check(failed.reason.includes('en_obs-tq'), `the reason names the install: ${failed.reason}`);
  await sleep(1000);
  check(new URL(page.url()).pathname === '/prepare.html', 'the failure stays on its page');
  check((await page.locator('h1').textContent()) === 'translationCore4 could not start', 'the failure stays');
  await page.screenshot({ path: path.join(outDir, 'prepare-failed.png') });
  results.steps['6-failure'] = { ...failed };
  await context.close();
  check((await tc4.quit()).exit.code === 0, 'the failed start quit on request');

  fs.writeFileSync(path.join(outDir, 'prepare-smoke.json'), `${JSON.stringify(results, null, 2)}\n`);
  console.log(JSON.stringify(results, null, 2));
  console.log('prepare-smoke: all steps passed');
} finally {
  await browser?.close();
  for (const tc4 of live) tc4.kill();
}
