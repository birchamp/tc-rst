'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const bridge = require('../bridge.cjs');
const lib = require('../lib.cjs');

const contract = require('../../docs/contracts/shell-bridge.json');
const preload = fs.readFileSync(path.join(__dirname, '..', 'preload.cjs'), 'utf8');
const ORIGIN = 'http://127.0.0.1:4100';

test('bridge_methods_equal_the_contract', () => {
  assert.equal(contract.version, 1);
  assert.deepEqual(Object.keys(bridge.BRIDGE).sort(), [...contract.methods].sort());
  // The members preload.cjs exposes, and the channel each uses, are the table's.
  const exposed = preload.slice(preload.indexOf("exposeInMainWorld('tc4Shell'"));
  const members = [...exposed.matchAll(/^ {2}([a-zA-Z]+):/gm)].map((m) => m[1]);
  assert.deepEqual(members.sort(), [...contract.methods].sort());
  for (const [name, entry] of Object.entries(bridge.BRIDGE)) {
    if (entry.kind === 'value') continue;
    const { channel } = entry;
    assert.ok(exposed.includes(`'${channel}`), `${name} uses ${channel} in preload.cjs`);
    if (entry.kind === 'invoke') assert.match(exposed, new RegExp(`${name}: invoke\\('${channel}'\\)`), name);
    if (entry.kind === 'send') assert.match(exposed, new RegExp(`${name}: send\\('${channel}'\\)`), name);
  }
});

// The shell's state and Electron's modules, faked; `calls` records what the handlers did.
function fakeCtx(over = {}) {
  const calls = [];
  const ctx = {
    calls,
    origin: () => ORIGIN,
    served: () => new Set(['http://127.0.0.1:4099', ORIGIN]),
    window: () => 'win',
    log: () => {},
    downloads: () => '/home/p/Downloads',
    windowOptions: (o) => lib.windowOptions({ platform: 'linux', ...o }),
    shell: { openExternal: (url) => calls.push(['browser', url]) },
    openInApp: (url) => calls.push(['app', url]),
    writeFile: async (file, bytes) => calls.push(['write', file, bytes.length]),
    dialog: {
      showSaveDialog: async (win, options) => {
        calls.push(['save', options]);
        return { canceled: false, filePath: '/home/p/Book.pdf' };
      },
      showOpenDialog: async (win, options) => {
        calls.push(['open', options]);
        return { canceled: false, filePaths: ['/media/usb'] };
      },
    },
    BrowserWindow: class {
      constructor(options) {
        calls.push(['window', options]);
        this.webContents = { getTitle: () => 'Titus: Community Checking', printToPDF: async () => Buffer.from('%PDF') };
      }
      async loadURL(url) {
        calls.push(['load', url]);
      }
      destroy() {
        calls.push(['destroy']);
      }
    },
    internetOn: async () => true,
    helpDesk: () => null,
    fetch: async () => ({ ok: true }),
    ...over,
  };
  return ctx;
}

const call = (name, ctx, ...args) => bridge.BRIDGE[name].handle(ctx, { sender: 'page' }, ...args);

test('open_external_refuses_the_app_origin', () => {
  const ctx = fakeCtx();
  assert.equal(call('openExternal', ctx, `${ORIGIN}/help/al-open.html`), 'app');
  assert.equal(call('openExternal', ctx, 'https://door43.org/u/x'), 'browser');
  assert.equal(call('openExternal', ctx, 'file:///etc/passwd'), 'refuse');
  assert.equal(call('openExternal', ctx, 42), 'refuse');
  assert.deepEqual(ctx.calls, [['app', `${ORIGIN}/help/al-open.html`], ['browser', 'https://door43.org/u/x']]);
  assert.ok(!ctx.calls.some(([where, url]) => where === 'browser' && url.startsWith(ORIGIN)), 'the app origin never reaches the system browser');
});

test('export_save_dialog_has_a_readable_title', async () => {
  const ctx = fakeCtx();
  const answer = await call('printPdf', ctx, '/print/job-7f3a');
  assert.deepEqual(answer, { ok: true, file: 'Book.pdf' });
  const [, options] = ctx.calls.find(([what]) => what === 'save');
  assert.equal(options.title, 'Save PDF');
  assert.equal(options.defaultPath, path.join('/home/p/Downloads', 'Titus Community Checking.pdf'));
  const [, windowOptions] = ctx.calls.find(([what]) => what === 'window');
  assert.equal(windowOptions.show, false);
  assert.equal(windowOptions.webPreferences.javascript, false);
  assert.deepEqual(ctx.calls.find(([what]) => what === 'load'), ['load', `${ORIGIN}/print/job-7f3a`]);
  assert.deepEqual(ctx.calls.find(([what]) => what === 'write'), ['write', '/home/p/Book.pdf', 4]);
  assert.equal(bridge.pdfName(''), 'translationCore4.pdf');
  assert.equal(bridge.pdfName('a/b:c'), 'a b c.pdf');
});

test('print_pdf_refuses_another_origin_and_a_cancelled_dialog_writes_nothing', async () => {
  const ctx = fakeCtx();
  assert.deepEqual(await call('printPdf', ctx, 'https://example.org/print/x'), { ok: false, reason: 'refused' });
  const cancelled = fakeCtx({ dialog: { showSaveDialog: async () => ({ canceled: true }) } });
  assert.deepEqual(await call('printPdf', cancelled, '/print/x'), { ok: false, reason: 'cancelled' });
  assert.ok(!cancelled.calls.some(([what]) => what === 'write'));
});

test('choose_folder_and_file_answer_a_path_or_null', async () => {
  const ctx = fakeCtx();
  assert.equal(await call('chooseFolder', ctx), '/media/usb');
  assert.equal(await call('chooseFile', ctx), '/media/usb');
  assert.deepEqual(ctx.calls.map(([, o]) => o.properties), [['openDirectory', 'createDirectory'], ['openFile']]);
  const none = fakeCtx({ dialog: { showOpenDialog: async () => ({ canceled: true, filePaths: [] }) } });
  assert.equal(await call('chooseFolder', none), null);
  assert.equal(await call('chooseFile', none), null);
});

test('feedback_send_without_a_key_answers_not_configured', async () => {
  assert.deepEqual(await call('feedbackSend', fakeCtx(), { message: 'hi' }), { ok: false, reason: 'not-configured' });
  // The internet switch first: off is offline, whatever the key.
  const off = fakeCtx({ internetOn: async () => false, helpDesk: () => ({ token: 'T', email: 'help@example.org' }) });
  assert.deepEqual(await call('feedbackSend', off, { message: 'hi' }), { ok: false, reason: 'offline' });
  const dir = fs.mkdtempSync(path.join(require('node:os').tmpdir(), 'tc4-desk-'));
  try {
    assert.equal(bridge.loadHelpDesk(dir), null);
    fs.writeFileSync(path.join(dir, bridge.HELP_DESK_KEY), JSON.stringify({ token: 'T', email: 'help@example.org' }));
    assert.deepEqual(bridge.loadHelpDesk(dir), { token: 'T', email: 'help@example.org' });
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('feedback_send_with_a_key_sends_from_the_main_process', async () => {
  let sent;
  const ctx = fakeCtx({
    helpDesk: () => ({ token: 'T', email: 'help@example.org' }),
    fetch: async (url, init) => {
      sent = { url, init };
      return { ok: true };
    },
  });
  const report = { category: 'Bug', message: 'It broke', name: 'Ana', email: 'ana@example.org', version: '4.0.0', attachment: 'log' };
  assert.deepEqual(await call('feedbackSend', ctx, report), { ok: true });
  assert.equal(sent.url, bridge.SENDGRID_URL);
  assert.equal(sent.init.headers.Authorization, 'Bearer T');
  const mail = JSON.parse(sent.init.body);
  assert.equal(mail.subject, 'tC: Bug');
  assert.deepEqual(mail.reply_to, { email: 'ana@example.org' });
  assert.equal(mail.content[0].value, 'It broke\n\nName: Ana\nEmail: ana@example.org\nVersion: 4.0.0\n');
  const refused = fakeCtx({ helpDesk: () => ({ token: 'T', email: 'e' }), fetch: async () => ({ ok: false }) });
  assert.deepEqual(await call('feedbackSend', refused, report), { ok: false, reason: 'refused' });
  const down = fakeCtx({ helpDesk: () => ({ token: 'T', email: 'e' }), fetch: async () => { throw new Error('ENOTFOUND'); } });
  assert.deepEqual(await call('feedbackSend', down, report), { ok: false, reason: 'offline' });
});

test('only_the_app_page_may_call_the_bridge', () => {
  const ctx = fakeCtx();
  assert.equal(bridge.trusted(ctx, { senderFrame: { url: `${ORIGIN}/` } }), true);
  // The page of the run before a tc4 restart, still on screen until Home reloads.
  assert.equal(bridge.trusted(ctx, { senderFrame: { url: 'http://127.0.0.1:4099/?probe=fields' } }), true);
  assert.equal(bridge.trusted(ctx, { senderFrame: { url: 'http://127.0.0.1:4098/' } }), false);
  assert.equal(bridge.trusted(ctx, { senderFrame: { url: 'https://door43.org/' } }), false);
  assert.equal(bridge.trusted(ctx, { senderFrame: null }), false);
  const handled = {};
  const ipc = { handle: (c, f) => (handled[c] = f), on: (c, f) => (handled[c] = f) };
  bridge.register(ipc, ctx);
  assert.equal(handled['shell:openExternal']({ senderFrame: { url: 'https://evil.example/' } }, 'https://door43.org/'), undefined);
  assert.deepEqual(ctx.calls, []);
});

test('window_preferences_isolate_the_renderer', () => {
  for (const platform of ['linux', 'win32', 'darwin']) {
    for (const options of [lib.windowOptions({ platform, preload: '/p.cjs' }), lib.windowOptions({ platform, print: true })]) {
      const web = options.webPreferences;
      assert.equal(web.contextIsolation, true, platform);
      assert.equal(web.sandbox, true, platform);
      assert.equal(web.nodeIntegration, false, platform);
      assert.equal(web.nodeIntegrationInSubFrames, false, platform);
      assert.equal(web.webviewTag, false, platform);
    }
  }
});

test('navigation_outside_the_app_origin_is_refused', () => {
  // main.cjs's will-navigate guard allows sameOrigin only; its window-open handler follows
  // linkTarget.
  assert.equal(lib.sameOrigin(`${ORIGIN}/help/x.html`, ORIGIN), true);
  for (const url of ['https://door43.org/', 'http://127.0.0.1:4101/', 'file:///etc/passwd', 'data:text/html,x', 'javascript:alert(1)']) {
    assert.equal(lib.sameOrigin(url, ORIGIN), false, url);
    assert.notEqual(lib.linkTarget(url, ORIGIN), 'app', url);
  }
  assert.equal(lib.sameOrigin(`${ORIGIN}/`, null), false, 'no origin before tc4 started: nothing navigates');
  const main = fs.readFileSync(path.join(__dirname, '..', 'main.cjs'), 'utf8');
  assert.match(main, /'will-navigate', \(event, url\) => \{\n\s+if \(!lib\.sameOrigin\(url, origin\(\)\)\) \{\n\s+event\.preventDefault\(\);/);
  assert.match(main, /setWindowOpenHandler\(\(\{ url \}\) => \{\n\s+const where = lib\.linkTarget\(url, origin\(\)\);/);
});

test('every_window_shows_the_translationcore_icon', () => {
  // #871: the main window and every window a link opens (main.cjs openInApp and the window-open
  // handler) carry the mark, not Electron's, as main.cjs calls windowOptions for each.
  for (const [platform, file] of [['win32', 'icon.ico'], ['linux', 'icon-1024.png']]) {
    for (const options of [lib.windowOptions({ platform, preload: '/p.cjs', show: false }), lib.windowOptions({ platform })]) {
      assert.equal(path.basename(options.icon), file, platform);
      assert.ok(fs.existsSync(options.icon), options.icon);
    }
  }
  // macOS takes the bundle's electron.icns (stage.sh).
  assert.equal(lib.windowOptions({ platform: 'darwin' }).icon, undefined);
});

test('windows_frames_and_the_menu_keys_follow_tc4', () => {
  for (const platform of ['linux', 'win32']) {
    assert.equal(lib.windowOptions({ platform }).titleBarStyle, 'hidden');
    assert.equal(lib.windowOptions({ platform }).titleBarOverlay.height, 56);
  }
  assert.deepEqual(lib.windowOptions({ platform: 'darwin' }).trafficLightPosition, { x: 20, y: 21 });
  const keys = (platform) => lib.menuTemplate(platform).flatMap((m) => m.submenu).map((i) => i.accelerator || i.role).filter(Boolean);
  assert.deepEqual(keys('win32').filter((k) => /zoom|Ctrl\+(0|R)/.test(k)), ['Ctrl+0', 'zoomIn', 'zoomOut', 'Ctrl+R']);
  assert.ok(keys('darwin').includes('Cmd+R'));
  const editable = { isEditable: true, editFlags: { canCut: true, canCopy: true, canPaste: false } };
  assert.deepEqual(lib.contextMenuTemplate(editable).map((i) => i.role), ['cut', 'copy', 'paste']);
  assert.deepEqual(lib.contextMenuTemplate({ isEditable: false, editFlags: {} }), []);
});
