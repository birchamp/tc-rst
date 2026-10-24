'use strict';

// What the shell's end-to-end tests share: a real Electronite running shell/main.cjs over a real
// `tc4` (built with --features test-hooks,fixture-backend) and the real UI builds, each launch on
// its own app-data folder under target/.
//
//   TC4_ELECTRON=<electron> TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app \
//     TC4_UI_HOOKS=target/tc-ui-app-hooks [TC4_BUNDLE=target/shell-bundle] \
//     xvfb-run -a node --test 'shell/e2e/*.test.cjs'
//
// Playwright drives the window (PLAYWRIGHT_MODULE, or parity/tc4-baseline/node_modules, as
// kit.mjs resolves it). Without the builds a test is skipped and says why; CI's no-skips check
// fails such a run.

const test = require('node:test');
const { spawn, execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

const REPO = path.resolve(__dirname, '..', '..');
const MAIN = path.join(REPO, 'shell', 'main.cjs');
const { TC4_ELECTRON, TC4_BIN, TC4_UI, TC4_UI_HOOKS, TC4_BUNDLE } = process.env;

function resolveSetup() {
  if (!TC4_ELECTRON || !TC4_BIN || !TC4_UI) return { skip: 'set TC4_ELECTRON, TC4_BIN and TC4_UI' };
  if (process.platform === 'linux' && !process.env.DISPLAY) return { skip: 'needs a display (xvfb-run -a)' };
  try {
    const dir = process.env.PLAYWRIGHT_MODULE || path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
    return { electron: require(dir)._electron };
  } catch (error) {
    return { skip: `Playwright does not resolve (${error.code || error.message}): npm ci in parity/tc4-baseline` };
  }
}
const setup = resolveSetup();

function shellTest(name, body, { hooks = false, bundle = false, timeout = 180000 } = {}) {
  let skip = setup.skip || false;
  if (!skip && hooks && !TC4_UI_HOOKS) skip = 'set TC4_UI_HOOKS (build-app.sh --test-hooks --out target/tc-ui-app-hooks)';
  if (!skip && bundle && !TC4_BUNDLE) skip = 'set TC4_BUNDLE (cargo run -p tc-app --example bundle_store -- --from bundle/en --only en_obs-tn --out target/shell-bundle)';
  return test(name, { skip, timeout }, body);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Polls `read` until it gives something truthy. The bound only catches a hang, never slowness
// (ruling 2026-10-23: tests prove behavior, not speed).
async function until(what, read, ms = 60000) {
  const stop = Date.now() + ms;
  let last;
  for (;;) {
    last = await read();
    if (last) return last;
    if (Date.now() > stop) throw new Error(`hung: no ${what} after ${ms} ms (last: ${JSON.stringify(last)})`);
    await sleep(25);
  }
}

let runs = 0;
function appDataDir(name) {
  const dir = path.join(REPO, 'target', 'shell-e2e', `${process.pid}-${Date.now()}-${runs++}-${name}`);
  fs.mkdirSync(path.join(dir, 'projects'), { recursive: true });
  return dir;
}

// The shell's arguments for a run on `dir`. Tests add tc4's test flags after `--`, as G3's
// launcher never does (a release tc4 refuses them itself).
function shellArgs(dir, { hooks = false, bundle = false, extra = [] } = {}) {
  const args = [];
  // A root runner (a container) has no user namespace for Chromium's sandbox.
  if (process.platform === 'linux' && process.getuid && process.getuid() === 0) args.push('--no-sandbox');
  args.push(MAIN, '--tc4', path.resolve(TC4_BIN), '--ui', path.resolve(hooks ? TC4_UI_HOOKS : TC4_UI), '--app-data', path.join(dir, 'appdata'));
  if (bundle) args.push('--bundle', path.resolve(TC4_BUNDLE));
  args.push('--', '--projects', path.join(dir, 'projects'), '--door43', 'http://127.0.0.1:9', '--test-hooks', ...extra);
  return args;
}

// The shell's log lines (shell.log in the app-data folder).
function logOf(dir) {
  try {
    return fs
      .readFileSync(path.join(dir, 'appdata', 'shell.log'), 'utf8')
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch (_) {
    return [];
  }
}

const dispatched = (dir, to) => logOf(dir).filter((l) => l.ev === 'dispatch' && l.data.to === to);
const started = (dir) => logOf(dir).filter((l) => l.ev === 'tc4 started');

function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === 'EPERM';
  }
}

// The pids of every `tc4 serve` on this app-data folder, from the operating system's own list.
function tc4Pids(dir) {
  const marker = path.join(dir, 'appdata');
  if (process.platform === 'win32') {
    const out = execFileSync('powershell.exe', [
      '-NoProfile', '-Command',
      'Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like "*serve*" -and $_.CommandLine -notlike "*main.cjs*" } | ForEach-Object { "$($_.ProcessId)`t$($_.CommandLine)" }',
    ], { encoding: 'utf8' });
    return out.split(/\r?\n/).filter((l) => l.includes(marker) && / serve /.test(l) && !/electron|main\.cjs/i.test(l)).map((l) => Number(l.split('\t')[0]));
  }
  const out = execFileSync('ps', ['-eo', 'pid=,args='], { encoding: 'utf8' });
  return out
    .split('\n')
    .filter((l) => l.includes(marker) && / serve /.test(l) && !/electron|main\.cjs/.test(l))
    .map((l) => Number(l.trim().split(/\s+/)[0]));
}

// The electron and tc4 processes with their parents, for a failure's output.
function processTree() {
  if (process.platform === 'win32') {
    return execFileSync('powershell.exe', [
      '-NoProfile', '-Command',
      'Get-CimInstance Win32_Process | Where-Object { $_.Name -match "electron|tc4" } | ForEach-Object { "$($_.ProcessId) <- $($_.ParentProcessId) $($_.Name) $($_.CommandLine.Substring(0, [Math]::Min(160, $_.CommandLine.Length)))" }',
    ], { encoding: 'utf8' });
  }
  return execFileSync('ps', ['-eo', 'pid=,ppid=,args='], { encoding: 'utf8' })
    .split('\n')
    .filter((l) => /electron|tc4/.test(l))
    .map((l) => l.slice(0, 200))
    .join('\n');
}

function hardKill(pid) {
  if (process.platform === 'win32') execFileSync('taskkill', ['/F', '/PID', String(pid)], { stdio: 'ignore' });
  else process.kill(pid, 'SIGKILL');
}

// A shell launched by Playwright; `app.window` is its page.
async function launch(dir, opts = {}) {
  const app = await setup.electron.launch({
    executablePath: path.resolve(TC4_ELECTRON),
    args: shellArgs(dir, opts),
    cwd: REPO,
  });
  app.dir = dir;
  // Kept: Playwright's app.process() throws once the app has gone.
  app.proc = app.process();
  app.pid = app.proc.pid;
  app.exited = new Promise((resolve) => app.proc.once('exit', (code, signal) => resolve({ code, signal })));
  return app;
}

// A shell started as a person starts it: no debugger attached.
function launchPlain(dir, opts = {}) {
  const child = spawn(path.resolve(TC4_ELECTRON), shellArgs(dir, opts), { cwd: REPO, stdio: 'ignore' });
  child.exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  return child;
}

// Waits for Home: the shell Running and the page's account menu on screen.
async function home(app) {
  await until('the shell running', () => dispatched(app.dir, 'Running').length > 0);
  const page = await app.firstWindow();
  await page.getByTestId('account-menu').waitFor({ timeout: 60000 });
  return page;
}

// Ends a run: the window closes as a person closes it, and every tc4 of the run ends.
async function finish(app) {
  try {
    if (app.proc.exitCode === null && app.proc.signalCode === null) {
      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().forEach((w) => w.close())).catch(() => {});
      await Promise.race([app.exited, sleep(15000)]);
    }
  } finally {
    if (app.proc.exitCode === null && app.proc.signalCode === null) hardKill(app.pid);
    for (const pid of tc4Pids(app.dir)) hardKill(pid);
    // Playwright's connection would keep the test process alive.
    await app.close().catch(() => {});
  }
}

module.exports = {
  REPO, alive, appDataDir, processTree, dispatched, finish, hardKill, home, launch, launchPlain, logOf, setup, shellArgs, shellTest,
  sleep, started, tc4Pids, until,
};
