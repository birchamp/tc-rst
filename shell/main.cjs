'use strict';

// The Electronite main script of translationCore4 (docs/contracts/SHELL.md). It keeps one
// running copy, starts and supervises `tc4`, gives the browser's own requests the token, mirrors
// typed text to disk, asks before closing over unsaved text, and answers `window.tc4Shell`.
//
//   electron main.cjs --tc4 <exe> --ui <dir> --app-data <dir> [--bundle <dir>] [-- <more tc4 arguments>]
//
// The lifecycle is lib.cjs's machine: `dispatch` alone calls `next()`, keeps the state and runs
// the actions. Everything else here is I/O.

const { app, BrowserWindow, Menu, dialog, ipcMain, session, shell } = require('electron');
const { spawn } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const lib = require('./lib.cjs');
const mirrorFile = require('./mirror.cjs');
const bridge = require('./bridge.cjs');

const ASK_PAGE_MS = 5000;
// tc4 closes within 5 s (SHELL.md item 3); the kill comes a second later, so it never cuts a
// close that keeps its promise.
const QUIT_MS = 6000;
const QUERY_MS = 5000;

let options;
try {
  options = lib.parseArgs(process.argv.slice(1));
} catch (error) {
  console.error(error.message);
  app.exit(2);
  return;
}

// The lock follows the profile folder, and the profile lives in the person's app-data folder,
// so one copy runs per person, and a test's copy never meets the person's own.
app.setPath('userData', path.join(path.resolve(options.appData), 'shell-profile'));
if (!app.requestSingleInstanceLock()) {
  app.exit(0);
  return;
}

if (process.platform === 'linux') {
  // tC4 #451: the print compositor keeps a printed PDF in /dev/shm, which is small in containers.
  app.commandLine.appendSwitch('disable-dev-shm-usage');
}

// ---- The shell's log: every line redacted, written without blocking ----

const secrets = [];
const logStream = fs.createWriteStream(path.join(path.resolve(options.appData), 'shell.log'), { flags: 'a' });
logStream.on('error', () => {});
function log(ev, data) {
  const line = JSON.stringify({ t: Date.now(), pid: process.pid, ev, ...(data === undefined ? {} : { data }) });
  logStream.write(`${lib.redact(line, secrets)}\n`);
}
fs.mkdirSync(path.resolve(options.appData), { recursive: true });

// ---- What the executor acts on ----

let machine = lib.initial();
let win = null;
let server = null; // { child, origin, token }
let spawned = null; // the tc4 process, from its spawn on, before and after its address
const served = new Set(); // every origin this shell's tc4 has had
let canClose = true;
let closeHandler = null; // the page's webContents once it called onCloseRequested
let closeAsk = 0;
let pageTimer = null;
let quitTimer = null;
let exiting = false;
let startError = 'tc4 could not start';
// Opened at Electron's ready; the bridge waits for it, so an early takeBuffered still answers.
let mirrorReady = Promise.resolve(null);
const helpDesk = bridge.loadHelpDesk(__dirname);
if (helpDesk) secrets.push(helpDesk.token);

const origin = () => (server ? server.origin : null);

function dispatch(event) {
  const from = machine.state;
  const step = lib.next(machine, { at: Date.now(), ...event });
  machine = step.machine;
  log('dispatch', { event: event.type, from, to: machine.state, actions: step.actions });
  for (const action of step.actions) {
    try {
      ACTIONS[action](event);
    } catch (error) {
      log('action failed', { action, message: error.message });
    }
  }
}

// tc4 answers an API call only with its own Origin, which the page's fetch sends by itself.
function headers(s) {
  return { 'X-Tc-Token': s.token, 'X-Tc-Api': '1', 'Content-Type': 'application/json', Origin: s.origin };
}

// A query of the running tc4 from the main process; null when it does not answer.
async function query(name) {
  const s = server;
  if (!s) return null;
  try {
    const response = await fetch(`${s.origin}/api/v1/${name}`, {
      method: 'POST',
      headers: headers(s),
      body: '{}',
      signal: AbortSignal.timeout(QUERY_MS),
    });
    const reply = await response.json();
    return reply && reply.type === 'ok' ? reply.value : null;
  } catch (_) {
    return null;
  }
}

// AD11: the token on the browser's own requests to tc4 (the page's documents, scripts, help,
// media, downloads and print pages); never on /api/, where the page sends it itself.
function installFilter(s) {
  session.defaultSession.webRequest.onBeforeSendHeaders({ urls: lib.filterPatterns(s.origin) }, (details, callback) => {
    const requestHeaders = details.requestHeaders;
    if (lib.wantsToken(details.url, s.origin)) requestHeaders['X-Tc-Token'] = s.token;
    callback({ requestHeaders });
  });
}

function spawnServer() {
  const child = spawn(path.resolve(options.tc4), lib.serveArgs(options), {
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
    // AD39: out of the job object libuv puts every child in on Windows, which would end tc4
    // the moment the shell ends, before it could finish a save. tc4 ends by itself when its
    // stdin closes.
    detached: true,
  });
  spawned = child;
  child.stdin.on('error', () => {});
  child.stderr.on('data', (chunk) => log('tc4 stderr', { text: String(chunk).slice(0, 2000) }));
  let addressed = false;
  let refusal = null;
  let reported = false;
  let held = false;
  const failed = (reason) => {
    if (reported) return;
    reported = true;
    startError = reason;
    dispatch({ type: 'startError', reason, held });
  };
  readline.createInterface({ input: child.stdout }).on('line', (line) => {
    if (addressed || refusal) return;
    const parsed = lib.parseAddress(line);
    if (parsed.error) {
      // tc4 exits after an error line; the exit reports it.
      held = parsed.error.type === 'appLockHeld';
      refusal = held ? lib.exitReason(3, null) : parsed.error.message;
      log('tc4 refused to start', parsed.error);
      return;
    }
    addressed = true;
    secrets.push(parsed.address.token);
    server = { child, ...parsed.address };
    served.add(server.origin);
    log('tc4 started', { pid: child.pid, port: parsed.address.port });
    installFilter(server);
    query('app/hello').then((hello) => {
      if (!server || server.child !== child) return;
      // Without an answer, the prepare page shows the state from tc4's event stream.
      const type = hello && hello.startup && hello.startup.type;
      dispatch({ type: 'address', startup: type === 'ready' ? 'ready' : 'preparing' });
    });
  });
  child.once('error', (error) => {
    log('tc4 did not start', { message: error.message });
    if (!addressed) failed(`tc4 could not be started: ${error.message}`);
  });
  child.once('exit', (code, signal) => {
    log('tc4 ended', { pid: child.pid, code, signal });
    if (server && server.child === child) {
      server = null;
      dispatch({ type: 'serverExit' });
    } else if (!addressed) {
      failed(refusal || lib.exitReason(code, signal));
    }
  });
  log('tc4 spawned', { pid: child.pid });
}

const pageUrl = (page) => `${server.origin}${page}#t=${server.token}`;

function load(url) {
  if (!win || win.isDestroyed()) return;
  win.loadURL(url).catch((error) => log('load failed', { message: error.message }));
}

function showWindow() {
  // Maximized before it shows, as tC4's window, in the same moment.
  win = new BrowserWindow(lib.windowOptions({ platform: process.platform, preload: path.join(__dirname, 'preload.cjs'), show: false }));
  win.maximize();
  win.show();
  log('window shown', { visible: win.isVisible() });
  win.on('close', (event) => {
    if (exiting) return;
    event.preventDefault();
    dispatch({ type: 'closeRequested' });
  });
  win.webContents.on('render-process-gone', (_e, details) => log('page ended', { reason: details.reason }));
  win.webContents.on('did-start-navigation', (details) => {
    if (details.isMainFrame && !details.isSameDocument) {
      closeHandler = null;
      canClose = true;
    }
  });
  win.webContents.on('did-navigate', (_e, url) => {
    if (lib.sameOrigin(url, origin()) && new URL(url).pathname === '/') dispatch({ type: 'homeShown' });
  });
  win.webContents.on('context-menu', (_e, params) => {
    const template = lib.contextMenuTemplate(params);
    if (template.length > 0) Menu.buildFromTemplate(template).popup({ window: win });
  });
  win.once('ready-to-show', () => log('window painted'));
}

function askPage() {
  clearTimeout(pageTimer);
  const id = ++closeAsk;
  if (!closeHandler || closeHandler.isDestroyed()) {
    // A page that never registered (still loading, or gone) holds nothing the mirror lacks.
    setImmediate(() => dispatch({ type: 'pageAnswered', clean: canClose }));
    return;
  }
  closeHandler.send('shell:closeRequested', id);
  pageTimer = setTimeout(() => {
    if (id !== closeAsk) return;
    // A late answer belongs to this round, which the person now answers.
    closeAsk++;
    dispatch({ type: 'pageTimeout' });
  }, ASK_PAGE_MS);
}

ipcMain.on('shell:closeAnswer', (event, id, clean) => {
  if (!bridge.trusted(ctx, event) || id !== closeAsk) return;
  clearTimeout(pageTimer);
  closeAsk++;
  log('page answered', { clean: clean === true });
  dispatch({ type: 'pageAnswered', clean: clean === true });
});

// tC4's own question (its template's close guard).
function askUser() {
  dialog
    .showMessageBox(win, {
      type: 'question',
      title: 'Unsaved changes',
      message: 'You have unsaved changes. Are you sure you want to close the application?',
      buttons: ['Yes', 'No'],
    })
    .then(({ response }) => dispatch({ type: 'userAnswered', close: response === 0 }));
}

function sendQuit() {
  const child = spawned;
  if (child && child.exitCode === null && child.signalCode === null) {
    try {
      // Before its address line tc4 reads no message yet; its stdin closing quits it too.
      if (server && server.child === child) child.stdin.write('{"quit":true}\n');
      else child.stdin.end();
    } catch (_) {
      // The pipe is gone: tc4 sees its stdin closed, which quits it too.
    }
  }
  clearTimeout(quitTimer);
  quitTimer = setTimeout(() => dispatch({ type: 'quitTimeout' }), QUIT_MS);
}

async function exitApp() {
  if (exiting) return;
  exiting = true;
  clearTimeout(quitTimer);
  const mirror = await mirrorReady;
  if (mirror) {
    const flushed = await Promise.race([
      mirror.close().then(() => true, () => false),
      new Promise((resolve) => setTimeout(() => resolve(false), 5000)),
    ]);
    log('mirror flushed', { flushed });
  }
  log('exit');
  logStream.end(() => app.exit(0));
}

const ACTIONS = {
  spawnServer,
  showWindow,
  loadPrepare: () => load(pageUrl('/prepare.html')),
  loadHome: () => load(pageUrl('/')),
  reloadHome: () => {
    for (const other of BrowserWindow.getAllWindows()) if (other !== win) other.destroy();
    load(pageUrl('/'));
  },
  focusWindow: () => {
    if (!win || win.isDestroyed()) return;
    if (win.isMinimized()) win.restore();
    win.show();
    win.focus();
    log('window focused');
  },
  askPage,
  askUser,
  sendQuit,
  killServer: () => {
    if (spawned) spawned.kill('SIGKILL');
  },
  exitApp,
  showStartError: () => load(lib.startErrorPage(startError)),
};

// ---- The bridge ----

const ctx = {
  BrowserWindow,
  dialog,
  shell,
  log,
  fetch: (...args) => fetch(...args),
  origin,
  served: () => served,
  window: () => win,
  windowOptions: (o) => lib.windowOptions({ platform: process.platform, ...o }),
  downloads: () => app.getPath('downloads'),
  writeFile: (file, bytes) => fs.promises.writeFile(file, bytes),
  openInApp: (url) => {
    const child = new BrowserWindow(lib.windowOptions({ platform: process.platform }));
    child.loadURL(url).catch((error) => log('load failed', { message: error.message }));
  },
  internetOn: async () => {
    const state = await query('net/state');
    return Boolean(state && state.on === true);
  },
  helpDesk: () => helpDesk,
  // In order: each append waits on the same promise.
  mirrorAppend: (line) => {
    mirrorReady.then((mirror) => mirror && mirror.append(line));
  },
  mirrorTake: async () => {
    const mirror = await mirrorReady;
    return mirror ? mirror.take() : [];
  },
  setCanClose: (clean) => {
    canClose = clean;
  },
  closeHandler: (contents) => {
    closeHandler = contents;
  },
};
bridge.register(ipcMain, ctx);

// INV-RENDERER-IS-ISOLATED: every page, the child windows included, stays on tc4's origin;
// a link elsewhere goes to the system browser.
app.on('web-contents-created', (_e, contents) => {
  contents.on('will-navigate', (event, url) => {
    if (!lib.sameOrigin(url, origin())) {
      event.preventDefault();
      log('navigation refused');
    }
  });
  contents.on('will-attach-webview', (event) => event.preventDefault());
  contents.setWindowOpenHandler(({ url }) => {
    const where = lib.linkTarget(url, origin());
    if (where === 'app') {
      return { action: 'allow', overrideBrowserWindowOptions: lib.windowOptions({ platform: process.platform }) };
    }
    if (where === 'browser') shell.openExternal(url);
    return { action: 'deny' };
  });
});

app.on('second-instance', () => dispatch({ type: 'secondInstance' }));
app.on('before-quit', (event) => {
  if (exiting) return;
  event.preventDefault();
  dispatch({ type: 'closeRequested' });
});
app.on('window-all-closed', () => {});

app.whenReady().then(() => {
  log('shell started', { versions: { electron: process.versions.electron, chrome: process.versions.chrome } });
  const template = lib.menuTemplate(process.platform).map((menu) => ({
    ...menu,
    submenu: menu.submenu.map(({ action, ...item }) => {
      if (!action) return item;
      const run = {
        resetZoom: (w) => w.webContents.setZoomLevel(0),
        reload: (w) => w.webContents.reload(),
        forceReload: (w) => w.webContents.reloadIgnoringCache(),
        toggleDevTools: (w) => w.webContents.toggleDevTools(),
      }[action];
      return { ...item, click: (_item, w) => w && run(w) };
    }),
  }));
  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
  session.defaultSession.on('will-download', (_event, item, contents) => {
    item.once('done', (_done, state) => {
      const saved = item.getSavePath();
      const filename = saved ? path.basename(saved) : item.getFilename();
      if (!contents.isDestroyed()) contents.send('shell:downloadDone', { state, filename });
    });
  });
  mirrorReady = mirrorFile
    .openMirror(path.resolve(options.appData), { onError: (error) => log('mirror write failed', { message: error.message }) })
    .catch((error) => {
      // The page's localStorage still holds the text; the log says why the shell could not.
      log('mirror could not open', { message: error.message });
      return null;
    });
  dispatch({ type: 'ready' });
});
