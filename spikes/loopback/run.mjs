// Drives the loopback spike (docs/APP-DESIGN.md §8.3, AD18, A-14): starts `tc4 serve`, then
// times the aligner's click, a `test/echo` call from a wasm `fetch` in Chromium, twice:
//   plain  the page sets X-Tc-Token itself, as the UI does (AD11);
//   proxy  the page sets no token and a request interceptor adds it to every request, `/api/`
//          included: the cost if the shell's filter could not leave `/api/` out.
// It also times the same call from Node without a browser, as a baseline for the server and
// loopback alone. Throwaway: it is never shipped.
//
//   node run.mjs --tc4 <path> --ui <dir> [--clicks 500] [--warmup 50] [--size 20480]
//                [--passes plain,proxy,proxy,plain] --out <file.json>
//
// Playwright comes from PLAYWRIGHT_MODULE (default: the parity harness's copy, 1.56.1, whose
// Chromium build 1194 is installed in PLAYWRIGHT_BROWSERS_PATH).
import { execFileSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, arg, i, all) => {
    if (arg.startsWith('--')) pairs.push([arg.slice(2), all[i + 1]]);
    return pairs;
  }, []),
);
for (const required of ['tc4', 'ui', 'out']) {
  if (!args[required]) throw new Error(`--${required} is required`);
}
const clicks = Number(args.clicks ?? 500);
const warmup = Number(args.warmup ?? 50);
const size = Number(args.size ?? 20480);
const passes = (args.passes ?? 'plain,proxy,proxy,plain').split(',');
for (const mode of passes) {
  if (mode !== 'plain' && mode !== 'proxy') throw new Error(`unknown pass ${mode}`);
}

const require = createRequire(import.meta.url);
const playwrightDir =
  process.env.PLAYWRIGHT_MODULE ?? '/home/user/tc-rst/parity/ds-gallery/node_modules/playwright';
const { chromium } = require(playwrightDir);
const playwrightVersion = JSON.parse(fs.readFileSync(path.join(playwrightDir, 'package.json'), 'utf8')).version;

/** Nearest-rank percentile of an unsorted sample list. */
function percentile(samples, p) {
  const sorted = [...samples].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil(p * sorted.length) - 1)];
}
const round = (x) => Math.round(x * 1000) / 1000;
function summary(samples) {
  return {
    n: samples.length,
    median: round(percentile(samples, 0.5)),
    p95: round(percentile(samples, 0.95)),
    max: round(Math.max(...samples)),
  };
}

// --- tc4 ----------------------------------------------------------------------------------
const appData = fs.mkdtempSync(path.join(os.tmpdir(), 'loopback-appdata-'));
// stdin stays open: a closed stdin makes tc4 quit.
const tc4 = spawn(args.tc4, ['serve', '--app-data', appData, '--ui', path.resolve(args.ui), '--test-hooks'], {
  stdio: ['pipe', 'pipe', 'inherit'],
});
const lines = readline.createInterface({ input: tc4.stdout })[Symbol.asyncIterator]();
const exited = new Promise((resolve) => tc4.once('exit', (code, signal) => resolve({ code, signal })));
const first = await Promise.race([lines.next(), exited.then(() => ({ done: true }))]);
if (first.done) throw new Error('tc4 ended before printing its address');
const { url, token } = JSON.parse(first.value);
const base = new URL(url);

// --- the Node baseline ----------------------------------------------------------------------
async function nodeBaseline() {
  const agent = new http.Agent({ keepAlive: true, maxSockets: 1 });
  const body = JSON.stringify({ size });
  const once = () =>
    new Promise((resolve, reject) => {
      const request = http.request(
        {
          agent,
          host: base.hostname,
          port: base.port,
          method: 'POST',
          path: '/api/v1/test/echo',
          headers: {
            Host: base.host,
            Origin: base.origin,
            'X-Tc-Token': token,
            'X-Tc-Api': '1',
            'Content-Type': 'application/json',
            'Content-Length': Buffer.byteLength(body),
          },
        },
        (response) => {
          const parts = [];
          response.on('data', (part) => parts.push(part));
          response.on('end', () => {
            const bytes = Buffer.concat(parts);
            if (response.statusCode !== 200) return reject(new Error(`status ${response.statusCode}`));
            const reply = JSON.parse(bytes.toString('utf8'));
            if (reply.type !== 'ok' || reply.value.words.length === 0) return reject(new Error('reply'));
            resolve(bytes.length);
          });
        },
      );
      request.on('error', reject);
      request.end(body);
    });
  const samples = [];
  let bytes = 0;
  for (let i = 0; i < warmup + clicks; i++) {
    const t0 = performance.now();
    bytes = await once();
    const t1 = performance.now();
    if (i >= warmup) samples.push(t1 - t0);
  }
  agent.destroy();
  return { ...summary(samples), replyBytes: bytes };
}

// --- the browser passes ---------------------------------------------------------------------
async function browserPass(mode) {
  const loadBefore = os.loadavg();
  const browser = await chromium.launch();
  try {
    const context = await browser.newContext();
    let intercepted = 0;
    if (mode === 'plain') {
      // The page's files need the token; the measured requests must carry only the page's own.
      await context.setExtraHTTPHeaders({ 'x-tc-token': token });
    } else {
      await context.route('**/*', async (route) => {
        const request = route.request();
        // `allHeaders()`, not `headers()`: the latter leaves out what the network stack adds,
        // Origin included, and a POST without Origin is a 403.
        const headers = { ...(await request.allHeaders()), 'x-tc-token': token };
        if (request.url().includes('/api/v1/test/echo')) intercepted++;
        await route.continue({ headers });
      });
    }
    const page = await context.newPage();
    page.on('pageerror', (error) => console.error('page error:', error.message));
    await page.goto(`${url}#t=${token}`);
    await page.waitForFunction(() => window.__ready === true, null, { timeout: 30000 });
    if (mode === 'plain') await context.setExtraHTTPHeaders({});
    const samples = await page.evaluate((o) => window.__run(o), {
      clicks,
      warmup,
      size,
      self: mode === 'plain',
    });
    if (samples.length !== clicks) throw new Error(`${samples.length} samples, wanted ${clicks}`);
    if (mode === 'proxy' && intercepted !== warmup + clicks) {
      throw new Error(`the interceptor saw ${intercepted} echo calls, wanted ${warmup + clicks}`);
    }
    return {
      mode,
      samples,
      ...summary(samples),
      interceptedEchoCalls: mode === 'proxy' ? intercepted : null,
      loadBefore,
      loadAfter: os.loadavg(),
      browserVersion: browser.version(),
    };
  } finally {
    await browser.close();
  }
}

// --- the run --------------------------------------------------------------------------------
function cpuinfo() {
  const text = fs.readFileSync('/proc/cpuinfo', 'utf8');
  const model = /^model name\s*:\s*(.*)$/m.exec(text)?.[1] ?? 'unknown';
  const flags = (/^flags\s*:\s*(.*)$/m.exec(text)?.[1] ?? '').split(' ');
  return { model, sha_ni: flags.includes('sha_ni'), avx512f: flags.includes('avx512f') };
}
const memTotalKiB = Number(/MemTotal:\s+(\d+)/.exec(fs.readFileSync('/proc/meminfo', 'utf8'))?.[1] ?? 0);

const result = { settings: { clicks, warmup, size, passes } };
result.nodeBaseline = await nodeBaseline();
console.log('node baseline', JSON.stringify(result.nodeBaseline));
result.passes = [];
for (const [index, mode] of passes.entries()) {
  const pass = await browserPass(mode);
  result.passes.push({ index: index + 1, ...pass });
  console.log(`pass ${index + 1} ${mode}: median ${pass.median} ms, p95 ${pass.p95} ms, max ${pass.max} ms`);
}
result.pooled = {};
for (const mode of ['plain', 'proxy']) {
  const samples = result.passes.filter((pass) => pass.mode === mode).flatMap((pass) => pass.samples);
  if (samples.length > 0) result.pooled[mode] = summary(samples);
}
console.log('pooled', JSON.stringify(result.pooled));

// Quit tc4 the way the shell does, and check it says closed and ends.
tc4.stdin.write('{"quit":true}\n');
const closed = await Promise.race([lines.next(), new Promise((r) => setTimeout(() => r({ timeout: true }), 10000))]);
result.tc4Closed = closed.value === '{"closed":true}';
const end = await Promise.race([exited, new Promise((r) => setTimeout(() => r({ timeout: true }), 10000))]);
result.tc4Exit = end;

result.machine = {
  ...cpuinfo(),
  logicalCpus: os.cpus().length,
  memTotalGiB: round(memTotalKiB / 1024 / 1024),
  kernel: os.release(),
  loadavgAtEnd: os.loadavg(),
};
result.playwright = playwrightVersion;
result.commit = execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
result.date = new Date().toISOString();
fs.mkdirSync(path.dirname(args.out), { recursive: true });
fs.writeFileSync(args.out, JSON.stringify(result, null, 1) + '\n');
fs.rmSync(appData, { recursive: true, force: true });
console.log(`wrote ${args.out}`);
