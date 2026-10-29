// tC4 #278 on the real app: with no project open, open About from Home's account menu and read
// the build line; compare it with the server's own build (app/hello); then look for the build
// on the page without clicking (a window global or a data attribute), as a QA report would quote
// it from the console.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/278-about-build.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, openMenu, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');

browserTest('278_about_names_the_running_build', async () => {
  await withApp(async ({ page, env }) => {
    // Before any click: is the build readable from the console?
    const consoleView = await page.evaluate(() => ({
      htmlData: { ...document.documentElement.dataset },
      globals: Object.keys(window).filter((k) => /build|version|tc4/i.test(k)),
    }));
    console.log('before any click, page data and globals:', JSON.stringify(consoleView));
    const { build } = await env.call('app/hello', {});
    console.log('server app/hello build:', JSON.stringify(build));
    await openMenu(page);
    await page.getByTestId('account-about').click();
    const line = await page.getByTestId('about-version').textContent();
    console.log('About line:', JSON.stringify(line));
    await page.getByRole('dialog', { name: 'About translationCore' }).screenshot({ path: path.join(SHOTS, '278-about.png') });
    assert.equal(line, `${build.version} (${build.commit})`);
  });
});
