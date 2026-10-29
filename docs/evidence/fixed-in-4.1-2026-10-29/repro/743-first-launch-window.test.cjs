'use strict';
// tC4 #743 on the real shell: a first launch (a bundle to install, paced so it is slow) of
// Electronite over shell/main.cjs and a real tc4. It records when the window shows, takes a
// picture of the window at once and while it prepares, samples the status line to see it move,
// then a second launch during the wait (one tc4, the window focused), and a close during the
// wait (no tc4 left).
//
//   cd <worktree> && t=$PWD/target && TC4_ELECTRON=<electronite 37.1.0>/electron TC4_BIN=$t/debug/tc4 \
//     TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_BUNDLE=<target/shell-bundle> \
//     xvfb-run -a tc-heavy node --test docs/evidence/fixed-in-4.1-2026-10-29/repro/743-first-launch-window.test.cjs
const assert = require('node:assert/strict');
const path = require('node:path');
const h = require('../../../../shell/e2e/harness.cjs');

const SHOTS = path.resolve(__dirname, '..', 'shots');

h.shellTest(
  '743_first_launch_window_shows_moves_and_closes_cleanly',
  async () => {
    const dir = h.appDataDir('743');
    const clicked = Date.now();
    const app = await h.launch(dir, { bundle: true, extra: ['--test-seed-pace', '1500'] });
    try {
      const page = await app.firstWindow();
      const firstSeen = Date.now() - clicked;
      await page.screenshot({ path: path.join(SHOTS, '743-window-at-once.png') });
      const log = () => h.logOf(dir);
      await h.until('window shown', () => log().find((l) => l.ev === 'window shown'));
      const shown = log().find((l) => l.ev === 'window shown');
      const tc4 = await h.until('tc4 started', () => log().find((l) => l.ev === 'tc4 started'));
      console.log(`# window shown ${shown.t - clicked} ms after the launch (playwright had it at ${firstSeen} ms); tc4 named its address at ${tc4.t - clicked} ms`);
      await page.getByText('Preparing translationCore4').waitFor();
      const lines = [];
      for (let i = 0; i < 8; i += 1) {
        const line = await page.locator('#progress').textContent({ timeout: 2000 }).catch(() => `(no status line: ${page.url().replace(/#.*/, '')})`);
        lines.push(line);
        await new Promise((r) => setTimeout(r, 500));
      }
      console.log(`# status line samples: ${JSON.stringify([...new Set(lines)])}`);
      console.log(`# page words: ${JSON.stringify(await page.locator('body').innerText())}`);
      await page.screenshot({ path: path.join(SHOTS, '743-window-preparing.png') });

      const before = h.tc4Pids(dir);
      const second = h.launchPlain(dir, { bundle: true });
      const ended = await second.exited;
      await h.until('focused', () => log().some((l) => l.ev === 'window focused'), 30000);
      console.log(`# second launch exit ${ended.code}; tc4 before ${before.length}, after ${h.tc4Pids(dir).length}`);
      assert.equal(ended.code, 0);
      assert.deepEqual(h.tc4Pids(dir), before);

      const [pid] = h.tc4Pids(dir);
      await app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows()[0].close());
      const quit = await app.exited;
      console.log(`# close while preparing: shell exit ${quit.code}; tc4 ${pid} alive: ${h.alive(pid)}; tc4 left: ${h.tc4Pids(dir).length}`);
      assert.equal(h.alive(pid), false);
      assert.ok(shown.t <= tc4.t, 'the window comes before tc4 answers');
      assert.ok(new Set(lines).size > 1, 'the status line moves');
    } finally {
      await h.finish(app);
    }
  },
  { bundle: true },
);
