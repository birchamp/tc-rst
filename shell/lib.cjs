'use strict';

// The shell's pure decisions (docs/contracts/SHELL.md), apart from Electronite so that node can
// test them: the lifecycle machine, the arguments, the address line, the token filter and the
// redaction of log lines.

const path = require('node:path');

// branding/ sits beside shell/ both in the repository and in the staged app (stage.sh).
const BRANDING = path.join(__dirname, '..', 'branding');

// ---- The lifecycle (INV-LIFECYCLE-HAS-ONE-STATE) ----

const State = Object.freeze({
  Starting: 'Starting',
  Preparing: 'Preparing',
  Running: 'Running',
  Restarting: 'Restarting',
  Closing: 'Closing',
  Quitting: 'Quitting',
  // Not in the card's list: tc4 could not start, or ended three times in a row within 10 s of
  // each start. The start-error page shows and nothing restarts; closing the window ends the app.
  Stopped: 'Stopped',
});

const EVENTS = Object.freeze([
  'ready', // Electron is ready
  'address', // tc4's first stdout line named its address; { startup: 'preparing'|'ready'|'failed', at }
  'homeShown', // the page is Home: the prepare page handed off once the resources were ready
  'startError', // tc4 could not start: an error line, a spawn error, or an exit before the line; { reason, at }
  'serverExit', // the running tc4 ended; { at }
  'closeRequested', // the person closed the window
  'pageAnswered', // the page's onCloseRequested answer; { clean }
  'pageTimeout', // the page gave no answer within 5 s
  'userAnswered', // "Unsaved changes, close anyway?"; { close }
  'secondInstance', // another launch of the shell
  'quitTimeout', // tc4 did not end within 5 s of the quit message
]);

const QUICK_EXIT_MS = 10000;
const QUICK_EXITS_TO_STOP = 3;

const stay = (machine) => ({ machine, actions: [] });
const go = (machine, state, actions, extra) => ({ machine: { ...machine, ...extra, state }, actions });
const focus = (machine) => ({ machine, actions: ['focusWindow'] });

// A tc4 that ends within 10 s of its start counts toward the stop; one that ran longer does not,
// and starts the count again.
function afterExit(machine, at) {
  const quick = machine.startedAt !== null && at - machine.startedAt < QUICK_EXIT_MS;
  const quickExits = quick ? machine.quickExits + 1 : 0;
  if (quickExits >= QUICK_EXITS_TO_STOP) {
    return go(machine, State.Stopped, ['showStartError'], { quickExits, startedAt: null });
  }
  return go(machine, State.Restarting, ['spawnServer'], { quickExits, startedAt: at });
}

function onAddress(machine, event, home) {
  if (event.startup === 'preparing') return go(machine, State.Preparing, ['loadPrepare'], { startedAt: event.at });
  return go(machine, State.Running, [home], { startedAt: event.at });
}

const TRANSITIONS = Object.freeze({
  [State.Starting]: {
    // The owner's ruling: the window shows at the click, before tc4 answers.
    ready: (m) => go(m, State.Starting, ['showWindow', 'spawnServer'], { startedAt: null }),
    address: (m, e) => onAddress(m, e, 'loadHome'),
    homeShown: stay,
    startError: (m) => go(m, State.Stopped, ['showStartError']),
    // tc4 ended after its address line, before it said whether it prepares.
    serverExit: (m, e) => afterExit(m, e.at),
    // tc4 is starting: its stdin closes, which quits it, and the 5 s count starts.
    closeRequested: (m) => go(m, State.Quitting, ['sendQuit']),
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: stay,
  },
  [State.Preparing]: {
    ready: stay,
    address: stay,
    homeShown: (m) => go(m, State.Running, []),
    startError: stay,
    serverExit: (m, e) => afterExit(m, e.at),
    // Nothing can be typed while the resources are prepared.
    closeRequested: (m) => go(m, State.Quitting, ['sendQuit']),
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: stay,
  },
  [State.Running]: {
    ready: stay,
    address: stay,
    homeShown: stay,
    startError: stay,
    serverExit: (m, e) => afterExit(m, e.at),
    closeRequested: (m) => go(m, State.Closing, ['askPage']),
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: stay,
  },
  [State.Restarting]: {
    ready: stay,
    address: (m, e) => onAddress(m, e, 'reloadHome'),
    homeShown: stay,
    // Another tc4 holds the app data folder: starting again cannot help.
    startError: (m, e) => (e.held ? go(m, State.Stopped, ['showStartError']) : afterExit(m, e.at)),
    serverExit: (m, e) => afterExit(m, e.at),
    // The old page cannot reach the new port, so it holds nothing the mirror does not.
    closeRequested: (m) => go(m, State.Quitting, ['sendQuit']),
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: stay,
  },
  [State.Closing]: {
    ready: stay,
    address: stay,
    homeShown: stay,
    startError: stay,
    // The person asked to close, and every typed line is in the mirror.
    serverExit: (m) => go(m, State.Quitting, ['exitApp']),
    closeRequested: stay,
    pageAnswered: (m, e) =>
      e.clean ? go(m, State.Quitting, ['sendQuit']) : go(m, State.Closing, ['askUser']),
    pageTimeout: (m) => go(m, State.Closing, ['askUser']),
    userAnswered: (m, e) => (e.close ? go(m, State.Quitting, ['sendQuit']) : go(m, State.Running, [])),
    secondInstance: focus,
    quitTimeout: stay,
  },
  [State.Quitting]: {
    ready: stay,
    // A close while tc4 was starting: it gets the quit message now.
    address: (m) => go(m, State.Quitting, ['sendQuit']),
    homeShown: stay,
    startError: (m) => go(m, State.Quitting, ['exitApp']),
    serverExit: (m) => go(m, State.Quitting, ['exitApp']),
    closeRequested: stay,
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: (m) => go(m, State.Quitting, ['killServer', 'exitApp']),
  },
  [State.Stopped]: {
    ready: stay,
    address: stay,
    homeShown: stay,
    startError: stay,
    serverExit: stay,
    closeRequested: (m) => go(m, State.Quitting, ['exitApp']),
    pageAnswered: stay,
    pageTimeout: stay,
    userAnswered: stay,
    secondInstance: focus,
    quitTimeout: stay,
  },
});

function initial() {
  return { state: State.Starting, quickExits: 0, startedAt: null };
}

// The one way the state changes. `machine` is { state, quickExits, startedAt }; the answer is
// { machine, actions }, where each action is a name the executor in main.cjs runs in order.
function next(machine, event) {
  const row = TRANSITIONS[machine.state];
  if (!row) throw new Error(`unknown state ${machine.state}`);
  const step = row[event.type];
  if (!step) throw new Error(`state ${machine.state} has no answer to event ${event.type}`);
  return step(machine, event);
}

// ---- Arguments ----

const USAGE = 'usage: electron main.cjs --tc4 <exe> --ui <dir> --app-data <dir> [--bundle <dir>] [-- <more tc4 arguments>]';
const OPTIONS = { '--tc4': 'tc4', '--ui': 'ui', '--app-data': 'appData', '--bundle': 'bundle' };

// The shell's own arguments. Electronite and Chromium put their own switches in argv too, so an
// unknown argument is left alone; everything after `--` goes to tc4.
function parseArgs(argv) {
  const options = { tc4: null, ui: null, appData: null, bundle: null, extra: [] };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--') {
      options.extra = argv.slice(i + 1);
      break;
    }
    const [name, inline] = arg.split(/=(.*)/s, 2);
    if (!(name in OPTIONS)) continue;
    const value = inline !== undefined ? inline : argv[++i];
    if (value === undefined || value === '') throw new Error(`${name} needs a value\n${USAGE}`);
    options[OPTIONS[name]] = value;
  }
  for (const name of ['--tc4', '--ui', '--app-data']) {
    if (!options[OPTIONS[name]]) throw new Error(`${name} is needed\n${USAGE}`);
  }
  return options;
}

function serveArgs(options) {
  const args = ['serve', '--app-data', options.appData, '--ui', options.ui];
  if (options.bundle) args.push('--bundle', options.bundle);
  return args.concat(options.extra);
}

// ---- The address line (SHELL.md item 2) ----

// { address: { origin, port, token } } or { error: { type, message } }.
function parseAddress(line) {
  let value;
  try {
    value = JSON.parse(line);
  } catch (_) {
    return { error: { type: 'garbled', message: 'tc4 did not print a JSON address line' } };
  }
  if (value && typeof value.error === 'object' && value.error !== null) {
    const type = typeof value.error.type === 'string' ? value.error.type : 'unknown';
    const detail = value.error.value;
    const message =
      detail && typeof detail.reason === 'string' ? detail.reason : typeof value.error.message === 'string' ? value.error.message : type;
    return { error: { type, message } };
  }
  const match = /^http:\/\/127\.0\.0\.1:(\d{1,5})\/$/.exec(value && value.url);
  if (!match) return { error: { type: 'garbled', message: 'the address line has no loopback url' } };
  if (typeof value.token !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(value.token)) {
    return { error: { type: 'garbled', message: 'the address line has no 43-character token' } };
  }
  const port = Number(match[1]);
  return { address: { origin: `http://127.0.0.1:${port}`, port, token: value.token } };
}

// Why tc4 ended before its address line, by its exit code (SHELL.md item 2).
function exitReason(code, signal) {
  if (code === 1) return 'tc4 stopped with a server error';
  if (code === 2) return 'tc4 was started with wrong arguments';
  if (code === 3) return 'another translationCore4 holds the app data folder';
  if (code === 4) return 'tc4 could not start';
  return `tc4 ended (${code === null ? signal : code}) before it named its address`;
}

// ---- The token filter (SHELL.md item 4, AD11) ----

// The server's paths outside /api/: the browser's own requests there get the token from the
// filter. The page sends it itself on /api/, so no API request runs the filter's listener on the
// main thread.
const FILTER_PREFIXES = ['/ui/', '/help/', '/media/', '/download/', '/print/'];
const FILTER_EXACT = ['/', '/prepare.html'];

// Electron matches a pattern against the path and the query, so an exact path also needs its
// form with a query (`/?probe=fields`).
function filterPatterns(origin) {
  return FILTER_EXACT.flatMap((p) => [`${origin}${p}`, `${origin}${p}?*`]).concat(FILTER_PREFIXES.map((p) => `${origin}${p}*`));
}

// Node's URL gives `null` as the origin of a scheme it does not know, so it is built by hand.
function originOf(url) {
  try {
    const parsed = new URL(url);
    return `${parsed.protocol}//${parsed.host}`;
  } catch (_) {
    return null;
  }
}

function sameOrigin(url, origin) {
  return origin !== null && origin !== undefined && originOf(url) === origin;
}

// Whether the filter adds the token to a request for `url`: the URL patterns decide in
// Electron, and the listener checks again, so a pattern that matches too much cannot leak it.
function wantsToken(url, origin) {
  if (!sameOrigin(url, origin)) return false;
  const path = new URL(url).pathname;
  return FILTER_EXACT.includes(path) || FILTER_PREFIXES.some((prefix) => path.startsWith(prefix));
}

// ---- Links and windows (INV-RENDERER-IS-ISOLATED) ----

function isWebUrl(url) {
  try {
    const { protocol } = new URL(url);
    return protocol === 'http:' || protocol === 'https:';
  } catch (_) {
    return false;
  }
}

// Where a link goes: the app's own origin stays in the app, where the filter adds the token
// (A-33: in the system browser it is a 401 page); another web page goes to the system browser;
// anything else nowhere.
function linkTarget(url, origin) {
  if (sameOrigin(url, origin)) return 'app';
  // This computer's own address is never a page for the system browser: an old run's port, or
  // another program's.
  if (isWebUrl(url) && !['127.0.0.1', 'localhost', '[::1]'].includes(new URL(url).hostname)) return 'browser';
  return 'refuse';
}

// The options of every window the shell makes. The renderer never sees Node: the page talks to
// the shell only through preload.cjs's bridge. Windows and Linux hide the bar and draw their
// controls over the header (#639, #642); macOS hides it and centres the traffic lights in the
// 56 px header (#214).
function windowOptions({ platform, preload, show = true, print = false }) {
  const webPreferences = {
    contextIsolation: true,
    sandbox: true,
    nodeIntegration: false,
    nodeIntegrationInSubFrames: false,
    webviewTag: false,
  };
  if (print) return { show: false, webPreferences: { ...webPreferences, javascript: false } };
  const frame =
    platform === 'darwin'
      ? { titleBarStyle: 'hidden', trafficLightPosition: { x: 20, y: 21 } }
      : { titleBarStyle: 'hidden', titleBarOverlay: { color: '#014263', symbolColor: '#FFFFFF', height: 56 } };
  // macOS takes the icon from the app bundle (stage.sh); Windows and Linux from the window. Every
  // window gets it, so a link opened in the app shows the translationCore mark too (#871).
  const icon = platform === 'darwin' ? {} : { icon: path.join(BRANDING, platform === 'win32' ? 'icon.ico' : 'icon-1024.png') };
  return {
    width: 1024,
    height: 768,
    minWidth: 900,
    minHeight: 600,
    show,
    // The app's own background, so the first paint before the page is not a white flash.
    backgroundColor: '#F7FAFC',
    title: 'translationCore4',
    ...frame,
    ...icon,
    webPreferences: { ...webPreferences, preload },
  };
}

// tC4's application menu (its template's InitializeMenu): Edit, the zoom keys Ctrl or Cmd with
// +, - and 0, and Reload, the way back from a blank window (#435). A window without a native bar
// never shows the menu row, but its keys still work.
function menuTemplate(platform) {
  const mac = platform === 'darwin';
  const template = [
    {
      label: 'Edit',
      submenu: [
        { role: 'undo' }, { role: 'redo' }, { type: 'separator' }, { role: 'cut' }, { role: 'copy' },
        { role: 'paste' }, { role: 'pasteAndMatchStyle' }, { role: 'selectAll' },
      ],
    },
    {
      label: 'View',
      submenu: [
        { label: 'Default Zoom', accelerator: mac ? 'Cmd+0' : 'Ctrl+0', action: 'resetZoom' },
        { role: 'zoomIn' },
        { role: 'zoomOut' },
      ],
    },
    {
      label: 'Window',
      submenu: [
        { label: 'Reload', accelerator: mac ? 'Cmd+R' : 'Ctrl+R', action: 'reload' },
        { label: 'Force Reload', accelerator: mac ? 'Shift+Cmd+R' : 'Ctrl+Shift+R', action: 'forceReload' },
        { label: 'Toggle Developer Tools', accelerator: mac ? 'Alt+Cmd+I' : 'Ctrl+Shift+I', action: 'toggleDevTools' },
      ],
    },
  ];
  if (mac) {
    template.unshift({
      label: 'translationCore4',
      submenu: [{ role: 'hide' }, { role: 'hideOthers' }, { role: 'unhide' }, { type: 'separator' }, { role: 'quit' }],
    });
  }
  return template;
}

// The right-click menu of a text field (tC4 #833.4).
function contextMenuTemplate(params) {
  if (!params.isEditable) return [];
  return [
    { role: 'cut', enabled: params.editFlags.canCut },
    { role: 'copy', enabled: params.editFlags.canCopy },
    { role: 'paste', enabled: params.editFlags.canPaste },
  ];
}

// The start-error page: what tc4 said, in the words of prepare.html's failure.
function startErrorPage(reason) {
  const escape = (t) => String(t).replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
  const html =
    '<!doctype html><html lang="en"><head><meta charset="utf-8"><title>translationCore4</title>' +
    "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">" +
    '<style>body{margin:36px;font:16px system-ui,sans-serif;color:#293847;background:#f5f7fa}h1{font-size:22px}p{line-height:1.5}</style>' +
    '</head><body><h1>translationCore4 could not start</h1>' +
    '<p>Please quit and try again.</p>' +
    `<p id="reason" style="font-size:14px;overflow-wrap:anywhere">${escape(reason)}</p></body></html>`;
  return `data:text/html;charset=utf-8,${encodeURIComponent(html)}`;
}

// ---- Log lines (INV-THE-TOKEN-NEVER-LEAVES-THE-APP) ----

// Text with every token taken out: the `#t=` fragment of a URL, and each secret given.
// Electron's own error messages quote the URL they failed to load, fragment and all.
function redact(text, secrets) {
  let out = String(text).replace(/#t=[A-Za-z0-9_-]+/g, '#t=<token>');
  for (const secret of secrets || []) {
    if (secret) out = out.split(secret).join('<token>');
  }
  return out;
}

module.exports = {
  EVENTS,
  QUICK_EXITS_TO_STOP,
  QUICK_EXIT_MS,
  State,
  TRANSITIONS,
  USAGE,
  contextMenuTemplate,
  exitReason,
  filterPatterns,
  initial,
  isWebUrl,
  linkTarget,
  menuTemplate,
  next,
  originOf,
  parseAddress,
  parseArgs,
  redact,
  sameOrigin,
  serveArgs,
  startErrorPage,
  wantsToken,
  windowOptions,
};
