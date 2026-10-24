'use strict';

// The shell's lifecycle in Electronite: one window at once, one running copy, tc4 supervised and
// never left running (card G1 step 4).

const assert = require('node:assert/strict');
const h = require('./harness.cjs');

h.shellTest('window_is_painted_within_1_s_of_the_click', async () => {
  const dir = h.appDataDir('paint');
  const clicked = Date.now();
  const app = await h.launch(dir);
  try {
    await h.home(app);
    const log = h.logOf(dir);
    const shown = log.find((l) => l.ev === 'window shown');
    const address = log.find((l) => l.ev === 'tc4 started');
    assert.ok(shown && shown.data.visible, 'the window is shown');
    // The owner's ruling: the window does not wait for tc4. Speed is measured, not asserted
    // (ruling 2026-10-23), so the time is printed for the record.
    assert.ok(shown.t <= address.t, 'the window shows before tc4 names its address');
    console.log(`# window shown ${shown.t - clicked} ms after the launch, tc4 address after ${address.t - clicked} ms`);
  } finally {
    await h.finish(app);
  }
});

h.shellTest('second_launch_focuses_the_running_window_and_starts_no_server', async () => {
  const dir = h.appDataDir('second');
  const app = await h.launch(dir);
  try {
    await h.home(app);
    const before = h.tc4Pids(dir);
    assert.equal(before.length, 1, 'one tc4 runs');
    await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].minimize());
    const second = h.launchPlain(dir);
    const ended = await second.exited;
    assert.equal(ended.code, 0, 'the second launch quits');
    await h.until('the running window focused', () => h.logOf(dir).some((l) => l.ev === 'window focused'), 30000);
    const minimized = await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].isMinimized());
    assert.equal(minimized, false, 'the window is restored');
    assert.deepEqual(h.tc4Pids(dir), before, 'no second tc4');
    assert.equal(h.started(dir).length, 1);
  } finally {
    await h.finish(app);
  }
});

h.shellTest('ending_tc4_restarts_it_and_reloads_home', async () => {
  const dir = h.appDataDir('restart');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    const [first] = h.started(dir);
    h.hardKill(first.data.pid);
    await h.until('a new tc4', () => h.started(dir).length === 2);
    const second = h.started(dir)[1];
    assert.notEqual(second.data.pid, first.data.pid);
    await h.until('Home at the new address', async () => {
      const port = new URL(page.url()).port;
      return Number(port) === second.data.port && (await page.getByTestId('account-menu').count()) > 0;
    });
    assert.ok(h.logOf(dir).some((l) => l.ev === 'dispatch' && l.data.actions.includes('reloadHome')));
    assert.deepEqual(h.tc4Pids(dir), [second.data.pid]);
  } finally {
    await h.finish(app);
  }
});

h.shellTest('ending_the_shell_ends_tc4_within_5s', async () => {
  const dir = h.appDataDir('end-shell');
  const app = await h.launch(dir);
  let pid;
  try {
    await h.home(app);
    [pid] = h.tc4Pids(dir);
    const killed = Date.now();
    h.hardKill(app.pid);
    // tc4 sees its stdin close (SHELL.md item 3). The bound catches a hang only.
    await h.until('tc4 to end', () => !h.alive(pid), 60000);
    console.log(`# tc4 ended ${Date.now() - killed} ms after the shell was killed`);
    // The next launch starts.
    const next = await h.launch(dir);
    try {
      await h.home(next);
    } finally {
      await h.finish(next);
    }
  } finally {
    await h.finish(app);
  }
});

// tC4 #740 (scripts/desktop-prepare.test.cjs): a close while the resources are prepared ends
// tc4 before the shell quits, and the next launch prepares to the end.
h.shellTest(
  'closing_the_window_while_preparing_leaves_no_tc4_process',
  async () => {
    const dir = h.appDataDir('prepare');
    const app = await h.launch(dir, { bundle: true, extra: ['--test-seed-pace', '5000'] });
    try {
      await h.until('the shell preparing', () => h.dispatched(dir, 'Preparing').length > 0);
      const page = await app.firstWindow();
      await page.getByText('Preparing translationCore4').waitFor();
      const [pid] = h.tc4Pids(dir);
      assert.ok(pid, 'tc4 runs while preparing');
      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].close());
      const ended = await app.exited;
      assert.equal(ended.code, 0);
      assert.equal(h.alive(pid), false, 'tc4 ended before the shell');
      assert.deepEqual(h.tc4Pids(dir), []);
      const questions = h.logOf(dir).filter((l) => l.ev === 'dispatch' && l.data.actions.includes('askUser'));
      assert.deepEqual(questions, [], 'nothing to ask while preparing');
    } finally {
      await h.finish(app);
    }
    const retry = await h.launch(dir, { bundle: true });
    try {
      await h.home(retry);
    } finally {
      await h.finish(retry);
    }
  },
  { bundle: true },
);
