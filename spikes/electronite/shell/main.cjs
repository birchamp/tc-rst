'use strict';

// The Electronite spike's main script (G0, docs/APP-DESIGN.md §10). Throwaway: never shipped.
//
// It does what the product's shell will do, the way tC4's scripts/desktop-main.cjs does it:
// it takes Electronite's single-instance lock, starts the server (here the stand-in
// tc4-standin), reads its address line, opens the window, and supervises the server (AD39).
// It gives the page the token in one of three ways:
//   filter      AD11: session.webRequest.onBeforeSendHeaders adds X-Tc-Token to the requests
//               the browser makes by itself, on the paths outside /api/ only;
//   filter-all  the same filter over every path, the cost if /api/ cannot be left out;
//   protocol    Q-1: a privileged tc4:// scheme whose protocol.handle proxies every request to
//               the server with the header.
// It never writes the token anywhere.
//
//   electron main.cjs --spike-mode=filter --spike-case=run --spike-out=DIR --spike-server=EXE
//                     [--spike-clicks=200] [--spike-warmup=20] [--spike-spawn=job|detached]
//                     [--spike-port=N]   (inspect: the port of the launch that was killed)

const { app, BrowserWindow, ipcMain, net, protocol, session } = require('electron');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const lib = require('./lib.cjs');

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let options;
try {
  options = lib.parseArgs(process.argv);
} catch (error) {
  console.error(`spike: ${error.message}`);
  app.exit(2);
  return;
}

const out = path.resolve(options.out);
const tag = `${options.mode}-${options.case}${options.spawn === 'detached' ? '-detached' : ''}`;
fs.mkdirSync(out, { recursive: true });
const eventsFile = path.join(out, `events-${tag}.jsonl`);
const statusFile = path.join(out, `status-${tag}.json`);
const controlFile = path.join(out, `control-${tag}.txt`);
const resultsFile = path.join(out, `results-${tag}.json`);
const appData = path.join(out, `appdata-${tag}`);
const APP_ORIGIN = 'tc4://app';

const tokensSeen = [];

// Every event line goes through redact: no token reaches a file.
function record(ev, data) {
  const line = JSON.stringify({ t: Date.now(), ev, ...(data || {}) });
  fs.appendFileSync(eventsFile, `${lib.redact(line, tokensSeen)}\n`);
}

function writeJsonAtomically(file, value) {
  const temporary = `${file}.tmp-${process.pid}`;
  const fd = fs.openSync(temporary, 'w');
  try {
    fs.writeSync(fd, `${JSON.stringify(value, null, 2)}\n`);
    fs.fsyncSync(fd);
  } finally {
    fs.closeSync(fd);
  }
  fs.renameSync(temporary, file);
}

// One profile per mode, so a kill and the inspection after it share localStorage, and no run
// touches a real tC4 profile.
app.setPath('userData', path.join(out, `profile-${options.mode}`));
// A covered or background window would stop its animation frames and skew the round trip.
app.commandLine.appendSwitch('disable-backgrounding-occluded-windows');
app.commandLine.appendSwitch('disable-renderer-backgrounding');
if (process.platform === 'linux') {
  app.commandLine.appendSwitch('disable-dev-shm-usage');
  // The Linux runs are this container's, under Xvfb with no GPU, where the GPU process now and
  // then failed to start and the window never loaded. Windows and macOS keep the GPU.
  app.disableHardwareAcceleration();
}
if (options.mode === 'protocol') {
  protocol.registerSchemesAsPrivileged([
    {
      scheme: 'tc4',
      privileges: { standard: true, secure: true, supportFetchAPI: true, stream: true, corsEnabled: true, codeCache: true },
    },
  ]);
}

if (!app.requestSingleInstanceLock()) {
  record('second-instance-refused', {});
  app.exit(0);
  return;
}

let server = null;
let restarts = 0;
let quitting = false;
let win = null;
let mirror = null;
let patternForm = 'none';
let childWindowResult = null;
const seen = { total: 0, byPath: {} };

function count(url) {
  seen.total++;
  const kind = lib.pathClass(url);
  seen.byPath[kind] = (seen.byPath[kind] || 0) + 1;
}

function writeStatus() {
  writeJsonAtomically(statusFile, {
    shellPid: process.pid,
    serverPid: server ? server.pid : null,
    port: server ? server.port : null,
    mode: options.mode,
    case: options.case,
    spawn: options.spawn,
    restarts,
  });
}

function startServer() {
  return new Promise((resolve, reject) => {
    const args = ['serve', '--app-data', appData];
    const started = Date.now();
    // `detached` keeps the child out of the job object libuv puts every child in on Windows,
    // which ends the child the moment the shell's process ends.
    const child = spawn(options.server, args, {
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
      detached: options.spawn === 'detached',
    });
    child.stderr.pipe(fs.createWriteStream(path.join(out, `server-${tag}.err`), { flags: 'a' }));
    let addressed = false;
    readline.createInterface({ input: child.stdout }).on('line', (line) => {
      if (!addressed) {
        addressed = true;
        try {
          const address = lib.parseAddress(line);
          tokensSeen.push(address.token);
          resolve({ child, pid: child.pid, startedAt: started, addressAt: Date.now(), ...address });
        } catch (error) {
          reject(error);
        }
      } else if (line.includes('"closed"')) {
        record('server-closed', { pid: child.pid });
      }
    });
    child.once('error', (error) => {
      if (!addressed) reject(error);
    });
    child.once('exit', (code, signal) => {
      if (!addressed) reject(new Error(`the server exited (${code === null ? signal : code}) before its address line`));
      onServerExit(child, code, signal);
    });
  });
}

// AD39: when the server exits without being asked, start it again and load Home at its new
// address. The old page cannot reach the new port.
function onServerExit(child, code, signal) {
  record('server-exited', { pid: child.pid, code, signal, quitting });
  if (quitting || !server || server.child !== child) return;
  const exitedAt = Date.now();
  restarts++;
  startServer()
    .then((started) => {
      server = started;
      writeStatus();
      if (options.mode !== 'protocol') installFilter();
      record('server-started', { pid: started.pid, port: started.port, restart: true, sinceExitMs: Date.now() - exitedAt });
      if (win && !win.isDestroyed()) {
        win.loadURL(homeUrl()).catch((error) => record('load-failed', { message: error.message }));
      }
    })
    .catch((error) => record('restart-failed', { message: error.message }));
}

function homeUrl() {
  if (options.mode === 'protocol') return `${APP_ORIGIN}/`;
  return `${server.origin}/#t=${server.token}`;
}

function currentOrigin() {
  if (options.mode === 'protocol') return APP_ORIGIN;
  return server ? server.origin : null;
}

function installFilter() {
  const filter = session.defaultSession.webRequest;
  const origin = server.origin;
  try {
    filter.onBeforeSendHeaders({ urls: lib.filterPatterns(origin, options.mode) }, (details, callback) => {
      count(details.url);
      const headers = details.requestHeaders;
      headers['X-Tc-Token'] = server.token;
      callback({ requestHeaders: headers });
    });
    patternForm = 'patterns with the port';
  } catch (error) {
    // Every request to 127.0.0.1 then crosses this listener, /api/ included.
    filter.onBeforeSendHeaders({ urls: lib.portlessPatterns() }, (details, callback) => {
      count(details.url);
      const headers = details.requestHeaders;
      if (lib.wantsToken(details.url, server.origin, options.mode)) headers['X-Tc-Token'] = server.token;
      callback({ requestHeaders: headers });
    });
    patternForm = `patterns without the port (with the port: ${error.message})`;
  }
}

async function proxy(request, target) {
  const s = server;
  const headers = lib.proxyHeaders(request.headers.entries(), s.token, s.origin);
  const init = { method: request.method, headers, redirect: 'manual', bypassCustomProtocolHandlers: true };
  if (request.method !== 'GET' && request.method !== 'HEAD') init.body = Buffer.from(await request.arrayBuffer());
  return net.fetch(target, init);
}

function installProtocol() {
  protocol.handle('tc4', (request) => {
    count(request.url);
    if (!server) return new Response('no server', { status: 503 });
    return proxy(request, lib.proxyTarget(request.url, server.origin));
  });
}

// The inspection after a hard kill reads localStorage of the killed launch's origin. That
// origin's port belonged to the old server, so its requests are answered by the new one.
function interceptOldOrigin() {
  const old = `http://127.0.0.1:${options.port}`;
  protocol.handle('http', (request) => {
    if (lib.originOf(request.url) === old && server) {
      return proxy(request, lib.proxyTarget(request.url, server.origin));
    }
    return net.fetch(request, { bypassCustomProtocolHandlers: true });
  });
}

function openMirror() {
  const file = path.join(app.getPath('userData'), 'drafts-buffer.log');
  let text = '';
  try {
    text = fs.readFileSync(file, 'utf8');
  } catch (_) {
    text = '';
  }
  const launch = lib.parseMirror(text);
  // AD21: at each launch the log is replaced, atomically, with the entries still unsaved.
  const temporary = `${file}.tmp-${process.pid}`;
  const fd0 = fs.openSync(temporary, 'w');
  fs.writeSync(fd0, lib.compactMirror(launch.entries));
  fs.fsyncSync(fd0);
  fs.closeSync(fd0);
  fs.renameSync(temporary, file);
  const fd = fs.openSync(file, 'a');
  let dirty = false;
  // Each change is appended at once; the log is synced at most once a second (AD21).
  setInterval(() => {
    if (!dirty) return;
    dirty = false;
    try {
      fs.fsyncSync(fd);
    } catch (_) {
      dirty = true;
    }
  }, 1000);
  return {
    launch,
    append(entry) {
      if (!entry || typeof entry.key !== 'string') return;
      fs.writeSync(fd, `${JSON.stringify({ key: entry.key, text: entry.text, n: entry.n, at: entry.at })}\n`);
      dirty = true;
    },
  };
}

async function quit(why) {
  if (quitting) return;
  quitting = true;
  record('quit-start', { why });
  const s = server;
  if (s && s.child.exitCode === null && s.child.signalCode === null) {
    try {
      s.child.stdin.write('{"quit":true}\n');
    } catch (_) {
      // The pipe is gone; the server sees its stdin close instead.
    }
    const exited = await Promise.race([
      new Promise((resolve) => s.child.once('exit', () => resolve(true))),
      sleep(5000).then(() => false),
    ]);
    record('server-stopped', { exited });
    if (!exited) s.child.kill();
  }
  app.exit(0);
}

function shellFacts() {
  return {
    versions: {
      electron: process.versions.electron,
      chrome: process.versions.chrome,
      node: process.versions.node,
      v8: process.versions.v8,
    },
    platform: process.platform,
    arch: process.arch,
    spawn: options.spawn,
    patternForm,
    seen,
    restarts,
    mirrorAtLaunch: mirror ? { lines: mirror.launch.lines, torn: mirror.launch.torn, entries: mirror.launch.entries.length } : null,
  };
}

ipcMain.handle('spike:config', () => ({
  mode: options.mode,
  case: options.case,
  clicks: options.clicks,
  warmup: options.warmup,
}));
ipcMain.on('spike:event', (_event, name, data) => record(String(name), data));
ipcMain.handle('spike:report', (_event, results) => {
  writeJsonAtomically(resultsFile, { ...results, shell: shellFacts() });
  record('results-written', {});
  if (options.case === 'run' || options.case === 'inspect') setTimeout(() => quit('done'), 200);
  return true;
});
ipcMain.handle('spike:filterCounts', () => ({ mode: options.mode, patternForm, total: seen.total, byPath: { ...seen.byPath } }));
// The api-main-thread probe: block this thread `block` ms of every `every` ms, for at most
// `ms` ms or until the page asks to stop, as synchronous work in the shell would block it.
let busyTimer = null;
ipcMain.handle('spike:busy', (_event, plan) => {
  clearInterval(busyTimer);
  busyTimer = null;
  if (plan.stop) return { stopped: true };
  const block = Math.min(Number(plan.block) || 200, 1000);
  const until = Date.now() + Math.min(Number(plan.ms) || 10000, 60000);
  busyTimer = setInterval(() => {
    if (Date.now() >= until) {
      clearInterval(busyTimer);
      busyTimer = null;
      return;
    }
    const end = Date.now() + block;
    while (Date.now() < end) {
      // Busy on purpose: this is the blocked thread the probe measures.
    }
  }, Number(plan.every) || 250);
  return { until };
});
ipcMain.handle('spike:childWindow', async () => {
  for (let i = 0; i < 50 && !childWindowResult; i++) await sleep(100);
  return childWindowResult;
});
ipcMain.handle('spike:capture', async (_event, rect) => {
  try {
    const image = await win.webContents.capturePage({
      x: Math.max(0, rect.x),
      y: Math.max(0, rect.y),
      width: Math.max(1, rect.width),
      height: Math.max(1, rect.height),
    });
    const name = `graphite-${tag}.png`;
    fs.writeFileSync(path.join(out, name), image.toPNG());
    return name;
  } catch (error) {
    return `no screenshot: ${error.message}`;
  }
});
ipcMain.on('buffer:changed', (_event, entry) => mirror.append(entry));
ipcMain.handle('buffer:take', () =>
  mirror.launch.entries.map((entry) => ({ key: entry.key, n: entry.n, at: entry.at })),
);
ipcMain.handle('shell:printPdf', async (event, url) => {
  const base = currentOrigin();
  const target = new URL(url, `${base}/`).toString();
  const printer = new BrowserWindow({ show: false, webPreferences: { javascript: false, sandbox: true } });
  let status = 0;
  printer.webContents.on('did-navigate', (_e, _url, code) => {
    status = code;
  });
  try {
    await printer.loadURL(target);
    const pdf = await printer.webContents.printToPDF({ preferCSSPageSize: true });
    record('print-done', { status, bytes: pdf.length });
    return { status, bytes: pdf.length };
  } catch (error) {
    return { status, bytes: 0, error: error.message };
  } finally {
    printer.destroy();
  }
});

// The PowerShell driver asks for a normal close or a quit through a file, the same on every
// system.
setInterval(() => {
  let command;
  try {
    command = fs.readFileSync(controlFile, 'utf8').trim();
  } catch (_) {
    return;
  }
  fs.rmSync(controlFile, { force: true });
  record('control', { command });
  if (command === 'close' && win && !win.isDestroyed()) win.close();
  else if (command === 'quit') quit('control');
}, 200);

app.on('window-all-closed', () => {
  // The quit runs from the main window's `closed` handler, after the server is asked to stop.
});

app.whenReady().then(async () => {
  record('shell-started', { pid: process.pid, versions: shellFacts().versions, spawn: options.spawn });
  mirror = openMirror();
  try {
    server = await startServer();
  } catch (error) {
    record('start-failed', { message: error.message });
    app.exit(3);
    return;
  }
  writeStatus();
  record('server-started', { pid: server.pid, port: server.port, ms: server.addressAt - server.startedAt });
  if (options.mode === 'protocol') installProtocol();
  else installFilter();
  const inspectingOldOrigin = options.case === 'inspect' && options.mode !== 'protocol';
  if (inspectingOldOrigin) interceptOldOrigin();

  session.defaultSession.on('will-download', (_event, item, contents) => {
    const name = item.getFilename();
    const dir = path.join(out, 'downloads');
    fs.mkdirSync(dir, { recursive: true });
    item.setSavePath(path.join(dir, `${tag}-${name}`));
    item.once('done', (_done, state) => {
      let bytes = 0;
      try {
        bytes = fs.statSync(item.getSavePath()).size;
      } catch (_) {
        bytes = 0;
      }
      record('download-done', { state, name, bytes });
      if (!contents.isDestroyed()) contents.send('download:done', { state, filename: name, bytes });
    });
  });

  win = new BrowserWindow({
    width: 1280,
    height: 860,
    show: true,
    title: `tC4 spike ${tag}`,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      sandbox: true,
      backgroundThrottling: false,
    },
  });
  if (options.case === 'run') win.setAlwaysOnTop(true);
  // §5.7 (A-33): a URL on the app's own origin opens as a child window of the same session,
  // where the filter adds the token; other links would go to the system browser.
  win.webContents.setWindowOpenHandler(({ url }) => {
    if (lib.sameOrigin(url, currentOrigin())) {
      return { action: 'allow', overrideBrowserWindowOptions: { show: false } };
    }
    record('external-link-refused', {});
    return { action: 'deny' };
  });
  win.webContents.on('did-create-window', (child) => {
    child.webContents.on('did-navigate', (_e, _url, code) => {
      childWindowResult = { status: code };
      record('child-window', { status: code });
      setTimeout(() => {
        if (!child.isDestroyed()) child.destroy();
      }, 300);
    });
  });
  win.webContents.on('render-process-gone', (_e, details) => record('renderer-gone', { reason: details.reason }));
  win.webContents.on('did-finish-load', () => record('page-loaded', {}));
  win.on('close', () => record('window-closing', {}));
  win.on('closed', () => quit('window-closed'));
  try {
    await win.loadURL(inspectingOldOrigin ? `http://127.0.0.1:${options.port}/` : homeUrl());
  } catch (error) {
    record('load-failed', { message: error.message });
  }
});
