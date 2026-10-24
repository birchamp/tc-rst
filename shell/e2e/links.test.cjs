'use strict';

// A help link of the app's own origin stays in the app, where the filter adds the token; in the
// system browser it would be a 401 page (A-33, tC4 #632). Another site goes to the system browser.

const assert = require('node:assert/strict');
const h = require('./harness.cjs');

h.shellTest('same_origin_help_link_stays_in_app', async () => {
  const dir = h.appDataDir('links');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    await app.evaluate(({ shell, session }) => {
      globalThis.__browser = [];
      shell.openExternal = async (url) => {
        globalThis.__browser.push(url);
      };
      globalThis.__status = [];
      session.defaultSession.webRequest.onResponseStarted((d) => globalThis.__status.push({ url: d.url, status: d.statusCode }));
    });
    const origin = new URL(page.url()).origin;
    const answers = await page.evaluate(async (origin) => [
      await window.tc4Shell.openExternal(`${origin}/help/al-open.html`),
      await window.tc4Shell.openExternal('https://door43.org/u/unfoldingWord'),
      await window.tc4Shell.openExternal('file:///etc/passwd'),
    ], origin);
    assert.deepEqual(answers, ['app', 'browser', 'refuse']);
    // A link the page opens itself follows the same rule.
    await page.evaluate(() => {
      window.open('/help/al-modes.html', '_blank');
      window.open('https://door43.org/', '_blank');
    });
    const shown = await h.until('both help pages in app windows', async () => {
      const urls = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().map((w) => w.webContents.getURL()));
      return urls.some((u) => u.endsWith('/help/al-open.html')) && urls.some((u) => u.endsWith('/help/al-modes.html')) && urls;
    });
    assert.ok(!shown.some((u) => u.includes('door43.org')), 'no app window for another site');
    const browser = await app.evaluate(() => globalThis.__browser);
    assert.deepEqual(browser, ['https://door43.org/u/unfoldingWord', 'https://door43.org/']);
    assert.ok(!browser.some((u) => u.startsWith(origin)), 'the app origin never reaches the system browser');
    await h.until('the help pages answered', async () => {
      const status = await app.evaluate(() => globalThis.__status);
      const help = status.filter((s) => /\/help\/al-(open|modes)\.html$/.test(s.url));
      return help.length >= 2 && help.every((s) => s.status === 200);
    });
    // The help windows cannot navigate away from the app either.
    const refused = await app.evaluate(async ({ BrowserWindow }) => {
      const child = BrowserWindow.getAllWindows().find((w) => w.webContents.getURL().endsWith('/help/al-open.html'));
      await child.webContents.executeJavaScript("location.href = 'https://door43.org/'; 1");
      await new Promise((resolve) => setTimeout(resolve, 1000));
      return child.webContents.getURL();
    });
    assert.ok(refused.endsWith('/help/al-open.html'), refused);
  } finally {
    await h.finish(app);
  }
});
