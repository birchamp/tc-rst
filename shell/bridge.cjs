'use strict';

// The main process's side of `window.tc4Shell` (SHELL.md item 5): one entry per member, keyed
// as docs/contracts/shell-bridge.json. `kind` says how preload.cjs reaches it: `invoke` answers
// a promise, `send` is fire and forget, `listen` registers a page function the shell calls on
// `channel`, `value` is a plain value. Each handler takes `ctx` (the Electron modules and the
// shell's state, which tests replace with fakes), the IPC event and the page's arguments.

const fs = require('node:fs');
const path = require('node:path');
const lib = require('./lib.cjs');

const HELP_DESK_KEY = 'help-desk.key';
const SENDGRID_URL = 'https://api.sendgrid.com/v3/mail/send';
const FEEDBACK_TIMEOUT_MS = 30000;

// The help desk's { token, email }, which the release pipeline writes beside main.cjs (SHELL.md
// item 11, tC4's two secrets); null when this build has none.
function loadHelpDesk(dir, read = fs.readFileSync) {
  try {
    const { token, email } = JSON.parse(read(path.join(dir, HELP_DESK_KEY), 'utf8'));
    return typeof token === 'string' && token && typeof email === 'string' && email ? { token, email } : null;
  } catch (_) {
    return null;
  }
}

const text = (v) => (typeof v === 'string' ? v : '');

// The message, then the Name and Email lines when given, then the version, as tC4 sends it
// (tC4:scripts/desktop-feedback.cjs).
const feedbackBody = ({ message, name, email, version }) =>
  `${text(message)}\n\n${name ? `Name: ${name}\n` : ''}${email ? `Email: ${email}\n` : ''}Version: ${text(version)}\n`;

// The page's own origin decides the question; a URL it gives is resolved against it.
function appUrl(ctx, url) {
  const origin = ctx.origin();
  if (!origin || typeof url !== 'string') return null;
  try {
    const target = new URL(url, `${origin}/`).toString();
    return lib.sameOrigin(target, origin) ? target : null;
  } catch (_) {
    return null;
  }
}

// A file name for the save dialog from the print page's title (tC4 #833.2: a readable name, not
// the job's id).
function pdfName(title) {
  const base = text(title).replace(/[\\/:*?"<>|\u0000-\u001f]+/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 120);
  return `${base || 'translationCore4'}.pdf`;
}

async function printPdf(ctx, _event, url) {
  const target = appUrl(ctx, url);
  if (!target) return { ok: false, reason: 'refused' };
  const printer = new ctx.BrowserWindow(ctx.windowOptions({ show: false, print: true }));
  let pdf;
  let title;
  try {
    await printer.loadURL(target);
    title = printer.webContents.getTitle();
    pdf = await printer.webContents.printToPDF({ preferCSSPageSize: true });
  } catch (error) {
    ctx.log('print failed', error.message);
    return { ok: false, reason: 'failed' };
  } finally {
    printer.destroy();
  }
  const answer = await ctx.dialog.showSaveDialog(ctx.window(), {
    title: 'Save PDF',
    defaultPath: path.join(ctx.downloads(), pdfName(title)),
    filters: [{ name: 'PDF', extensions: ['pdf'] }],
  });
  if (answer.canceled || !answer.filePath) return { ok: false, reason: 'cancelled' };
  await ctx.writeFile(answer.filePath, pdf);
  return { ok: true, file: path.basename(answer.filePath) };
}

async function chooseFolder(ctx) {
  const answer = await ctx.dialog.showOpenDialog(ctx.window(), { properties: ['openDirectory', 'createDirectory'] });
  return answer.canceled || answer.filePaths.length === 0 ? null : answer.filePaths[0];
}

async function chooseFile(ctx) {
  const answer = await ctx.dialog.showOpenDialog(ctx.window(), { properties: ['openFile'] });
  return answer.canceled || answer.filePaths.length === 0 ? null : answer.filePaths[0];
}

// http and https of another site go to the system browser. The app's own origin is refused
// there, where it is a 401 page without the token (A-33); it opens in the app instead.
function openExternal(ctx, _event, url) {
  const where = typeof url === 'string' ? lib.linkTarget(url, ctx.origin()) : 'refuse';
  if (where === 'browser') ctx.shell.openExternal(url);
  else if (where === 'app') ctx.openInApp(url);
  return where;
}

// The page asks the internet question first (Q-2); the shell still reads the switch itself, so
// no report leaves while it is off.
async function feedbackSend(ctx, _event, report) {
  if (!(await ctx.internetOn())) return { ok: false, reason: 'offline' };
  const desk = ctx.helpDesk();
  if (!desk) return { ok: false, reason: 'not-configured' };
  const r = typeof report === 'object' && report !== null ? report : {};
  const mail = {
    personalizations: [{ to: [{ email: desk.email }] }],
    from: { email: desk.email },
    ...(r.email ? { reply_to: { email: text(r.email) } } : {}),
    subject: `tC: ${text(r.category)}`,
    content: [{ type: 'text/plain', value: feedbackBody(r) }],
    attachments: [
      { content: Buffer.from(text(r.attachment), 'utf8').toString('base64'), filename: 'report.txt', type: 'text/plain', disposition: 'attachment' },
    ],
  };
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), FEEDBACK_TIMEOUT_MS);
  try {
    const response = await ctx.fetch(SENDGRID_URL, {
      method: 'POST',
      headers: { Authorization: `Bearer ${desk.token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(mail),
      signal: abort.signal,
    });
    return response.ok ? { ok: true } : { ok: false, reason: 'refused' };
  } catch (_) {
    return { ok: false, reason: abort.signal.aborted ? 'timeout' : 'offline' };
  } finally {
    clearTimeout(timer);
  }
}

const BRIDGE = Object.freeze({
  bufferChanged: { kind: 'send', channel: 'shell:bufferChanged', handle: (ctx, _e, line) => ctx.mirrorAppend(line) },
  chooseFile: { kind: 'invoke', channel: 'shell:chooseFile', handle: chooseFile },
  chooseFolder: { kind: 'invoke', channel: 'shell:chooseFolder', handle: chooseFolder },
  feedbackSend: { kind: 'invoke', channel: 'shell:feedbackSend', handle: feedbackSend },
  // The page registers once; the shell sends `shell:closeRequested` with an id, and the page
  // answers on `shell:closeAnswer`.
  onCloseRequested: { kind: 'listen', channel: 'shell:closeRequested', handle: (ctx, event) => ctx.closeHandler(event.sender) },
  onDownloadDone: { kind: 'listen', channel: 'shell:downloadDone', handle: () => {} },
  openExternal: { kind: 'invoke', channel: 'shell:openExternal', handle: openExternal },
  platform: { kind: 'value' },
  printPdf: { kind: 'invoke', channel: 'shell:printPdf', handle: printPdf },
  setCanClose: { kind: 'send', channel: 'shell:setCanClose', handle: (ctx, _e, clean) => ctx.setCanClose(clean === true) },
  takeBuffered: { kind: 'invoke', channel: 'shell:takeBuffered', handle: (ctx) => ctx.mirrorTake() },
});

// Only the app's own page, in the shell's own window, may call the bridge.
function trusted(ctx, event) {
  const url = event.senderFrame ? event.senderFrame.url : '';
  return lib.sameOrigin(url, ctx.origin());
}

// Registers every entry with ipcMain.
function register(ipcMain, ctx) {
  for (const [name, entry] of Object.entries(BRIDGE)) {
    if (entry.kind === 'value') continue;
    const guarded = (event, ...args) => {
      if (!trusted(ctx, event)) {
        ctx.log('bridge call refused', name);
        return undefined;
      }
      return entry.handle(ctx, event, ...args);
    };
    if (entry.kind === 'invoke') ipcMain.handle(entry.channel, guarded);
    else if (entry.kind === 'send') ipcMain.on(entry.channel, guarded);
    else ipcMain.on(`${entry.channel}:register`, guarded);
  }
}

module.exports = { BRIDGE, HELP_DESK_KEY, SENDGRID_URL, feedbackBody, loadHelpDesk, pdfName, register, trusted };
