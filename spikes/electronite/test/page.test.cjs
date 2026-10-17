'use strict';

// The spike's page in Chromium through Playwright, against the stand-in server: the probes
// pass when something adds the token to the requests the browser makes by itself, and they
// fail when nothing does, so a probe cannot pass for the wrong reason.
//
//   cargo build --release --manifest-path spikes/electronite/Cargo.toml
//   TC4_STANDIN=spikes/electronite/target/release/tc4-standin \
//   PLAYWRIGHT_MODULE=/path/to/node_modules/playwright node --test spikes/electronite/test/page.test.cjs

const test = require('node:test');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const readline = require('node:readline');

const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const STANDIN = process.env.TC4_STANDIN || path.join(__dirname, '..', 'target', 'release', 'tc4-standin');

const BROWSER_PROBES = ['document', 'script', 'stylesheet', 'fetch', 'image', 'iframe', 'font', 'css-url', 'event-stream'];
const SELF_LOADED = ['image', 'iframe', 'font', 'css-url'];
const SHELL_ONLY = ['download', 'print', 'child-window', 'api-hop'];

async function startServer() {
  const appData = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-spike-page-'));
  const child = spawn(STANDIN, ['serve', '--app-data', appData], { stdio: ['pipe', 'pipe', 'inherit'] });
  const line = await new Promise((resolve, reject) => {
    readline.createInterface({ input: child.stdout }).once('line', resolve);
    child.once('error', reject);
  });
  const { url, token } = JSON.parse(line);
  return {
    url,
    token,
    appData,
    stop() {
      child.stdin.end();
      fs.rmSync(appData, { recursive: true, force: true });
    },
  };
}

async function runPage(server, contextOptions, route, query, withFragment) {
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext(contextOptions);
    if (route) await context.route('**/*', route);
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(String(e)));
    await page.goto(`${server.url}?${query}${withFragment ? `#t=${server.token}` : ''}`);
    await page.waitForFunction(() => window.__spikeResults, null, { timeout: 180000 });
    const results = await page.evaluate(() => window.__spikeResults);
    return { results, errors };
  } finally {
    await browser.close();
  }
}

function byName(results) {
  return Object.fromEntries(results.probes.map((p) => [p.name, p]));
}

test('with the token on every request, every probe a browser can run passes', async () => {
  const server = await startServer();
  try {
    const { results, errors } = await runPage(
      server,
      { extraHTTPHeaders: { 'X-Tc-Token': server.token } },
      null,
      'case=run&clicks=20&warmup=2',
      true,
    );
    assert.deepEqual(errors, []);
    const probes = byName(results);
    for (const name of BROWSER_PROBES) assert.equal(probes[name].pass, true, `${name}: ${probes[name].detail}`);
    for (const name of SHELL_ONLY) assert.equal(probes[name].pass, null, `${name} needs the shell`);
    assert.equal(results.tokenFromFragment, true);
    assert.equal(results.roundTrip.length, 2);
    for (const rt of results.roundTrip) {
      assert.equal(rt.reply.n, 20);
      assert.equal(rt.raw.frame.length, 20);
      assert.ok(rt.replyBytes >= 20 * 1024, `the reply is ${rt.replyBytes} bytes`);
      assert.ok(rt.reply.median > 0 && rt.reply.median <= rt.reply.p95 && rt.reply.p95 <= rt.frame.p95 + 1000);
    }
    const loading = results.roundTrip[1];
    assert.ok(loading.load.pictures > 0, 'pictures loaded during the second run');
    assert.equal(loading.load.pictureErrors, 0);
    assert.equal(results.graphite.fontLoaded, true);
    // Plain Chromium has no Graphite: recorded for the report's comparison.
    console.log(`graphite in plain Chromium: ${JSON.stringify(results.graphite)}`);
  } finally {
    server.stop();
  }
});

test('when nothing adds the token to loads the browser makes by itself, those probes fail', async () => {
  const server = await startServer();
  try {
    // Only the page, its script and its style sheet get the token, as if the shell's filter
    // listed nothing else.
    const route = (r, request) => {
      const { pathname } = new URL(request.url());
      if (pathname === '/' || pathname === '/ui/probe.js' || pathname === '/ui/probe.css') {
        r.continue({ headers: { ...request.headers(), 'x-tc-token': server.token } });
      } else {
        r.continue();
      }
    };
    const { results } = await runPage(server, {}, route, 'case=run&clicks=3&warmup=0', true);
    const probes = byName(results);
    for (const name of SELF_LOADED) {
      assert.equal(probes[name].pass, false, `${name} must fail without the token: ${probes[name].detail}`);
      assert.match(probes[name].detail, /401 token missing/);
    }
    // The page's own requests carry the header it sets.
    assert.equal(probes.fetch.pass, true);
    assert.equal(probes['event-stream'].pass, true);
  } finally {
    server.stop();
  }
});

test('a proxy that adds the token passes with no token in the page (the protocol.handle case)', async () => {
  const server = await startServer();
  try {
    const route = (r, request) => r.continue({ headers: { ...request.headers(), 'x-tc-token': server.token } });
    const { results } = await runPage(server, {}, route, 'case=run&clicks=3&warmup=0', false);
    assert.equal(results.tokenFromFragment, false);
    const probes = byName(results);
    for (const name of BROWSER_PROBES) assert.equal(probes[name].pass, true, `${name}: ${probes[name].detail}`);
  } finally {
    server.stop();
  }
});

test('each keystroke is in localStorage and reported to the server at once', async () => {
  const server = await startServer();
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext({ extraHTTPHeaders: { 'X-Tc-Token': server.token } });
    const page = await context.newPage();
    await page.goto(`${server.url}?case=type#t=${server.token}`);
    await page.waitForFunction(() => document.getElementById('draft').value.length >= 10, null, { timeout: 10000 });
    const { n, typed } = await page.evaluate(() => {
      const all = JSON.parse(localStorage.getItem('tc4.drafts.v2'));
      const entry = Object.values(all)[0];
      return { n: entry.n, typed: document.getElementById('draft').value.length };
    });
    assert.ok(n >= typed - 1 && n <= typed, `localStorage holds ${n} of ${typed}`);
    await page.waitForTimeout(300);
    const lines = fs.readFileSync(path.join(server.appData, 'typed.log'), 'utf8').trim().split('\n');
    assert.ok(lines.length >= 10, `the server saw ${lines.length} keystrokes`);
  } finally {
    await browser.close();
    server.stop();
  }
});
