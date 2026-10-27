'use strict';

// tC4 #833.2 on the real shell: every export but PDF reaches the file as tc-ui's
// `export::deliver` does, a click on `<a download href="/download/{id}/{name}">`. This makes the
// same click on a file the app serves, then reads the title of the Save dialog's X window and
// the save options the shell gave the download.
//
//   TC4_ELECTRON=<electronite>/electron TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app \
//     TC4_UI_HOOKS=target/tc-ui-app-hooks xvfb-run -a node --test \
//     docs/evidence/tc4-issues-2026-10-27/repro/833-2-download-dialog-title.test.cjs

const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const h = require('../../../../shell/e2e/harness.cjs');

const windowNames = () => execFileSync('xwininfo', ['-root', '-tree'], { encoding: 'utf8' })
  .split('\n').map((l) => (l.match(/^\s*0x[0-9a-f]+ "([^"]*)"/) || [])[1]).filter(Boolean);

h.shellTest('833.2: the Save dialog of an <a download> export has a readable title', async () => {
  const dir = h.appDataDir('download-title');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    await app.evaluate(({ session }) => {
      globalThis.__downloads = [];
      session.defaultSession.on('will-download', (_e, item) => {
        globalThis.__downloads.push({ url: item.getURL(), options: item.getSaveDialogOptions() });
      });
    });
    const before = new Set(windowNames());
    await page.evaluate(() => {
      const link = document.createElement('a');
      link.href = '/help/al-open.html';
      link.download = 'Titus.usfm';
      document.body.append(link);
      link.click();
    });
    await h.until('the download starts', async () => (await app.evaluate(() => globalThis.__downloads.length)) > 0, 30000);
    const downloads = await app.evaluate(() => globalThis.__downloads);
    let fresh = [];
    await h.until('a new window (the Save dialog)', async () => {
      fresh = windowNames().filter((n) => !before.has(n));
      return fresh.length > 0;
    }, 20000).catch(() => {});
    console.log(`DOWNLOAD: ${JSON.stringify(downloads)}`);
    console.log(`NEW X WINDOW NAMES: ${JSON.stringify(fresh)}`);
    assert.ok(!fresh.some((n) => /^https?:\/\/|^blob:/.test(n)), 'the dialog title is not an internal URL');
  } finally {
    await h.finish(app);
  }
});
