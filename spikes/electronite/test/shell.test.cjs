'use strict';

// The spike's shell in Electronite itself, against the stand-in: what AD11's filter spares and
// what it does not, and what a hard kill leaves of the shell's mirror (AD21).
//
//   cargo build --release --manifest-path spikes/electronite/Cargo.toml
//   TC4_ELECTRON=/path/to/electronite/electron TC4_STANDIN=.../release/tc4-standin \
//     xvfb-run -a node --test spikes/electronite/test/shell.test.cjs
//
// Skipped without TC4_ELECTRON, and on Linux without a display.
const test = require('node:test');
const assert = require('node:assert/strict');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const ELECTRON = process.env.TC4_ELECTRON;
const STANDIN = process.env.TC4_STANDIN || path.join(__dirname, '..', 'target', 'release', 'tc4-standin');
const MAIN = path.join(__dirname, '..', 'shell', 'main.cjs');
const SKIP = !ELECTRON
  ? 'needs TC4_ELECTRON (Electronite) and TC4_STANDIN'
  : process.platform === 'linux' && !process.env.DISPLAY
    ? 'needs a display (xvfb-run -a)'
    : false;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// A deadline in a race: unref'd, so it does not hold the test process open once the race is won.
const deadline = (ms) => new Promise((resolve) => setTimeout(resolve, ms).unref());

function startShell(out, mode, kase, extra = {}) {
  const argv = [];
  if (process.platform === 'linux' && process.getuid && process.getuid() === 0) argv.push('--no-sandbox');
  argv.push(MAIN, `--spike-mode=${mode}`, `--spike-case=${kase}`, `--spike-out=${out}`, `--spike-server=${STANDIN}`);
  for (const [key, value] of Object.entries(extra)) argv.push(`--spike-${key}=${value}`);
  const child = spawn(ELECTRON, argv, { stdio: 'ignore' });
  child.exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  return child;
}

function readJson(file) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (_) {
    return null;
  }
}

function events(out, tag) {
  try {
    return fs
      .readFileSync(path.join(out, `events-${tag}.jsonl`), 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch (_) {
    return [];
  }
}

async function until(what, ms, probe) {
  const stop = Date.now() + ms;
  while (Date.now() < stop) {
    const value = probe();
    if (value) return value;
    await sleep(100);
  }
  throw new Error(`${what}: not within ${ms} ms`);
}

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (_) {
    return false;
  }
}

// The most keystrokes the profile's localStorage files hold on disk, as run-spike.ps1 reads them.
function committedKeystrokes(profile) {
  const folder = path.join(profile, 'Local Storage', 'leveldb');
  let best = 0;
  let names = [];
  try {
    names = fs.readdirSync(folder);
  } catch (_) {
    return 0;
  }
  for (const name of names) {
    if (!/\.(log|ldb)$/.test(name)) continue;
    let text = '';
    try {
      text = fs.readFileSync(path.join(folder, name), 'latin1');
    } catch (_) {
      continue;
    }
    for (const m of text.matchAll(/"n":(\d+)/g)) best = Math.max(best, Number(m[1]));
  }
  return best;
}

function lastTyped(out, tag) {
  try {
    const lines = fs.readFileSync(path.join(out, `appdata-${tag}`, 'typed.log'), 'utf8').split('\n').filter(Boolean);
    return lines.length ? JSON.parse(lines[lines.length - 1]).n : 0;
  } catch (_) {
    return 0;
  }
}

test("AD11's filter spares only its JS listener: the page's /api/ requests wait for a busy main thread", { skip: SKIP, timeout: 240000 }, async () => {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-spike-shell-'));
  const shell = startShell(out, 'filter', 'run', { clicks: 5, warmup: 1 });
  try {
    const exit = await Promise.race([shell.exited, deadline(200000).then(() => null)]);
    assert.ok(exit, 'the run case did not finish in 200 s');
    const results = readJson(path.join(out, 'results-filter-run.json'));
    assert.ok(results && results.probes, `no results; events: ${JSON.stringify(events(out, 'filter-run'))}`);
    const probes = Object.fromEntries(results.probes.map((p) => [p.name, p]));
    assert.equal(probes['api-listener'].pass, true, probes['api-listener'].detail);
    const hop = probes['api-main-thread'];
    assert.equal(hop.finding, 'waits for the main process', hop.detail);
    assert.ok(hop.busy.p95 - hop.idle.p95 > 50, hop.detail);
  } finally {
    if (alive(shell.pid)) shell.kill('SIGKILL');
  }
});

for (const end of ['kill', 'quit']) {
  test(`after ${end === 'kill' ? 'a hard kill' : 'a normal quit'} while typing, the mirror holds what the server saw`, { skip: SKIP, timeout: 180000 }, async () => {
    const out = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-spike-shell-'));
    const tag = 'filter-type';
    const shell = startShell(out, 'filter', 'type');
    let serverPid = null;
    try {
      const typing = await until('typing', 60000, () => events(out, tag).find((e) => e.ev === 'typing-started'));
      const status = await until('status', 10000, () => readJson(path.join(out, `status-${tag}.json`)));
      serverPid = status.serverPid;
      await until('the first localStorage write', 60000, () => committedKeystrokes(path.join(out, 'profile-filter')) > 0);
      await sleep(3000);
      if (end === 'kill') {
        process.kill(status.shellPid, 'SIGKILL');
      } else {
        fs.writeFileSync(path.join(out, `control-${tag}.txt`), 'quit');
      }
      await until('the server exits', 10000, () => !alive(serverPid));
      await Promise.race([shell.exited, deadline(15000)]);
      const made = Math.floor((Date.now() - typing.t0) / typing.every);
      const serverSaw = lastTyped(out, tag);
      assert.ok(serverSaw > 60, `the server saw ${serverSaw} of about ${made} keystrokes`);

      const inspect = startShell(out, 'filter', 'inspect', { port: status.port });
      await Promise.race([inspect.exited, deadline(60000)]);
      const seen = readJson(path.join(out, 'results-filter-inspect.json'));
      assert.ok(seen, 'the inspection wrote no results');
      assert.ok(seen.localStorage.n > 0, `localStorage kept ${seen.localStorage.n}`);
      // The appends run off the main thread (AD21); a kill can lose only a line still on its way
      // to the file, never one written before it.
      assert.ok(seen.mirror.n >= serverSaw - (end === 'kill' ? 2 : 0), `the mirror kept ${seen.mirror.n}, the server saw ${serverSaw}`);
      assert.equal(seen.shell.mirrorAtLaunch.torn, 0);
    } finally {
      if (alive(shell.pid)) shell.kill('SIGKILL');
      if (serverPid && alive(serverPid)) process.kill(serverPid, 'SIGKILL');
    }
  });
}
