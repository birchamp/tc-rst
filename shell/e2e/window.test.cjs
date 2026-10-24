'use strict';

// The window as tC4 frames it on each system, its menu keys, the text menu, and the feedback
// send without a help-desk key (card G1 step 7). A system's frame test exists only on that
// system: CI runs the file on all three, and a skipped test would fail its zero-skip check.

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const h = require('./harness.cjs');

const frame = (app, page) =>
  Promise.all([
    page.evaluate(() => ({
      titleBar: document.documentElement.dataset.titleBar || null,
      overlay: Boolean(navigator.windowControlsOverlay && navigator.windowControlsOverlay.visible),
    })),
    app.evaluate(({ BrowserWindow }) => {
      const win = BrowserWindow.getAllWindows()[0];
      return {
        menuBar: win.isMenuBarVisible(),
        buttons: process.platform === 'darwin' ? win.getWindowButtonPosition() : null,
      };
    }),
  ]).then(([page, win]) => ({ ...page, ...win }));

// The application menu's View and Window items, and what running them does.
async function menuKeys(app) {
  return app.evaluate(async ({ Menu, BrowserWindow }) => {
    const win = BrowserWindow.getAllWindows()[0];
    const items = Menu.getApplicationMenu().items.flatMap((menu) => menu.submenu.items);
    const find = (key) => items.find((i) => i.accelerator === key || i.role === key);
    const keys = items.map((i) => i.accelerator || i.role).filter(Boolean);
    const zoom = [];
    find('zoomin').click(undefined, win, win.webContents);
    zoom.push(win.webContents.getZoomLevel());
    find(process.platform === 'darwin' ? 'Cmd+0' : 'Ctrl+0').click(undefined, win, win.webContents);
    zoom.push(win.webContents.getZoomLevel());
    find('zoomout').click(undefined, win, win.webContents);
    zoom.push(win.webContents.getZoomLevel());
    win.webContents.setZoomLevel(0);
    const reloaded = new Promise((resolve) => win.webContents.once('did-finish-load', () => resolve(true)));
    find(process.platform === 'darwin' ? 'Cmd+R' : 'Ctrl+R').click(undefined, win, win.webContents);
    return { keys, zoom, reloaded: await Promise.race([reloaded, new Promise((r) => setTimeout(() => r(false), 30000))]) };
  });
}

function assertKeys(got) {
  const mod = process.platform === 'darwin' ? 'Cmd' : 'Ctrl';
  for (const key of [`${mod}+0`, 'zoomin', 'zoomout', `${mod}+R`]) assert.ok(got.keys.includes(key), `${key} in ${got.keys}`);
  assert.ok(got.zoom[0] > 0, 'zoom in');
  assert.equal(got.zoom[1], 0, 'default zoom');
  assert.ok(got.zoom[2] < 0, 'zoom out');
  assert.equal(got.reloaded, true, 'reload');
}

if (process.platform === 'linux') {
  h.shellTest('linux_window_keeps_native_frame', async () => {
    const dir = h.appDataDir('frame');
    const app = await h.launch(dir);
    try {
      const page = await h.home(app);
      const got = await frame(app, page);
      assert.deepEqual({ titleBar: got.titleBar, overlay: got.overlay, menuBar: got.menuBar }, { titleBar: null, overlay: false, menuBar: true });
      assertKeys(await menuKeys(app));
    } finally {
      await h.finish(app);
    }
  });
}

if (process.platform === 'win32') {
  h.shellTest('windows_window_has_no_native_bar_and_keeps_zoom_and_reload', async () => {
    const dir = h.appDataDir('frame');
    const app = await h.launch(dir);
    try {
      const page = await h.home(app);
      const got = await frame(app, page);
      assert.equal(got.titleBar, 'overlay');
      assert.equal(got.overlay, true, 'the window controls draw over the header');
      assertKeys(await menuKeys(app));
    } finally {
      await h.finish(app);
    }
  });
}

if (process.platform === 'darwin') {
  h.shellTest('macos_window_hides_the_frame_and_centres_the_traffic_lights', async () => {
    const dir = h.appDataDir('frame');
    const app = await h.launch(dir);
    try {
      const page = await h.home(app);
      const got = await frame(app, page);
      assert.equal(got.titleBar, 'hidden');
      assert.deepEqual(got.buttons, { x: 20, y: 21 });
      assertKeys(await menuKeys(app));
    } finally {
      await h.finish(app);
    }
  });
}

h.shellTest(
  'a_text_field_right_click_offers_cut_copy_paste',
  async () => {
    const dir = h.appDataDir('text-menu');
    const app = await h.launch(dir, { hooks: true });
    try {
      const page = await h.home(app);
      await app.evaluate(({ Menu }) => {
        globalThis.__menus = [];
        const build = Menu.buildFromTemplate.bind(Menu);
        Menu.buildFromTemplate = (template) => {
          const menu = build(template);
          if (template.some((i) => i.role === 'cut')) {
            globalThis.__menus.push(template.map((i) => i.role));
            menu.popup = () => {};
          }
          return menu;
        };
      });
      await page.evaluate(() => {
        location.href = `/?probe=fields#t=${sessionStorage.getItem('tc4.token')}`;
      });
      const field = page.locator('[data-testid="probe-field-a"][data-ready="true"]');
      await field.waitFor({ timeout: 60000 });
      await page.locator('body').click({ button: 'right', position: { x: 2, y: 2 } });
      await field.click({ button: 'right' });
      await h.until('the text menu', async () => (await app.evaluate(() => globalThis.__menus.length)) > 0, 30000);
      assert.deepEqual(await app.evaluate(() => globalThis.__menus), [['cut', 'copy', 'paste']], 'only the text field has the menu');
    } finally {
      await h.finish(app);
    }
  },
  { hooks: true },
);

h.shellTest('feedback_send_without_a_key_answers_not_configured', async () => {
  const dir = h.appDataDir('feedback');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    const report = { category: 'Bug', message: 'x', name: '', email: '', version: '4.0.0', attachment: '' };
    // The internet switch is off at every launch: the shell sends nothing.
    assert.deepEqual(await page.evaluate((r) => window.tc4Shell.feedbackSend(r), report), { ok: false, reason: 'offline' });
    const on = await page.evaluate(async (request) => {
      const headers = { 'X-Tc-Token': sessionStorage.getItem('tc4.token'), 'X-Tc-Api': '1', 'Content-Type': 'application/json' };
      return (await fetch('/api/v1/net/set', { method: 'POST', headers, body: JSON.stringify({ request, on: true }) })).status;
    }, crypto.randomBytes(16).toString('hex'));
    assert.equal(on, 200);
    assert.deepEqual(await page.evaluate((r) => window.tc4Shell.feedbackSend(r), report), { ok: false, reason: 'not-configured' });
    assert.equal(await page.evaluate(() => window.tc4Shell.platform), process.platform);
  } finally {
    await h.finish(app);
  }
});
