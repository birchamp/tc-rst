'use strict';

// Typed text survives the shell and tc4 stopping, and a close over unsaved text asks first
// (card G1 step 5, INV-UNSAVED-TEXT-SURVIVES-A-CRASH).

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const h = require('./harness.cjs');

// The no-loss probe page of the test build, as kit.mjs's loadProbe opens it.
async function probe(app) {
  const page = await h.home(app);
  await page.evaluate(() => {
    location.href = `/?probe=fields#t=${sessionStorage.getItem('tc4.token')}`;
  });
  await page.locator('[data-testid="probe-field-a"][data-ready="true"]').waitFor({ timeout: 60000 });
  return page;
}

// A test command of tc4 (test-hooks), sent from the page with its own token.
const command = (page, name, body) =>
  page.evaluate(
    async ({ name, body }) => {
      const headers = { 'X-Tc-Token': sessionStorage.getItem('tc4.token'), 'X-Tc-Api': '1', 'Content-Type': 'application/json' };
      const response = await fetch(`/api/v1/${name}`, { method: 'POST', headers, body: JSON.stringify(body) });
      return response.status;
    },
    { name, body: { request: crypto.randomBytes(16).toString('hex'), ...body } },
  );

const mirrored = (page) => page.evaluate(() => window.tc4Shell.takeBuffered());

h.shellTest(
  'hard_kill_while_typing_offers_the_text_at_relaunch',
  async () => {
    const dir = h.appDataDir('hard-kill');
    const text = 'Paul, a servant of God';
    const app = await h.launch(dir, { hooks: true });
    try {
      const page = await probe(app);
      // The save waits, so the text is typed but not saved when everything stops.
      assert.equal(await command(page, 'test/hold', { at: 'save' }), 200);
      await page.getByTestId('probe-field-a').click();
      await page.keyboard.type(text, { delay: 15 });
      await h.until('the typed text in the mirror', async () => (await mirrored(page)).some((l) => l.includes(text)));
      // Appended to the log's file before the kill: the kill takes the process, not the disk.
      const log = path.join(dir, 'appdata', 'drafts-buffer.log');
      await h.until('the typed text in drafts-buffer.log', () => fs.existsSync(log) && fs.readFileSync(log, 'utf8').includes(text));
      for (const pid of h.tc4Pids(dir)) h.hardKill(pid);
      const shell = app.shellPid;
      h.hardKill(shell);
      await h.until('the shell to end', () => !h.alive(shell), 30000);
    } finally {
      await h.finish(app);
    }
    const again = await h.launch(dir, { hooks: true });
    try {
      const page = await again.firstWindow();
      await h.until('the shell running', () => h.dispatched(dir, 'Running').length > 0);
      const lines = await mirrored(page);
      assert.ok(lines.some((l) => JSON.parse(l).value.entry.text === text), `takeBuffered offers the text: ${lines}`);
    } finally {
      await h.finish(again);
    }
  },
  { hooks: true },
);

h.shellTest(
  'close_with_unsaved_text_asks',
  async () => {
    const dir = h.appDataDir('close-asks');
    const app = await h.launch(dir, { hooks: true });
    try {
      await app.evaluate(({ dialog }) => {
        globalThis.__asked = [];
        dialog.showMessageBox = async (_win, options) => {
          globalThis.__asked.push(options.message);
          return { response: globalThis.__asked.length === 1 ? 1 : 0 };
        };
      });
      const page = await probe(app);
      await page.getByTestId('probe-field-a').click();
      await page.keyboard.type('saved', { delay: 15 });
      // The next save fails, as on a full disk: the probe's store file is a folder now.
      const store = path.join(dir, 'appdata', 'test-hooks', 'fields.json');
      await h.until('the first save', () => fs.existsSync(store) && fs.readFileSync(store, 'utf8').includes('saved'));
      fs.rmSync(store);
      fs.mkdirSync(store);
      fs.writeFileSync(path.join(store, 'keep'), 'x');
      await page.keyboard.type(' unsaved words', { delay: 15 });
      const close = () => app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].close());
      await close();
      await h.until('the question', async () => (await app.evaluate(() => globalThis.__asked.length)) === 1, 30000);
      const asked = await app.evaluate(() => globalThis.__asked);
      assert.deepEqual(asked, ['You have unsaved changes. Are you sure you want to close the application?']);
      // "No": the window stays, with its text.
      await h.until('the shell running again', () => h.dispatched(dir, 'Running').some((l) => l.data.from === 'Closing'));
      assert.equal(await page.getByTestId('probe-field-a').inputValue(), 'saved unsaved words');
      const answers = h.logOf(dir).filter((l) => l.ev === 'page answered');
      assert.deepEqual(answers.map((l) => l.data.clean), [false], 'the page was asked first, and said unsaved');
      // "Yes": the shell quits, and the text waits in the mirror.
      await close();
      const ended = await app.exited;
      assert.equal(ended.code, 0);
      assert.deepEqual(h.tc4Pids(dir), []);
      const log = fs.readFileSync(path.join(dir, 'appdata', 'drafts-buffer.log'), 'utf8');
      assert.ok(log.includes('saved unsaved words'));
    } finally {
      await h.finish(app);
    }
  },
  { hooks: true },
);
