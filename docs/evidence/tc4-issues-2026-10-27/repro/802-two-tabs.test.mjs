// tC4 #802 and #839 against the rebuild: two pages (two JavaScript realms) on one `tc4 serve`,
// both on Titus 1:5. Page B saves the verse; page A, opened before B's save and typed in, saves
// after it. tC4 lost B's verse in 9 of 9 runs (#802). Outside the gate on purpose.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app PLAYWRIGHT_MODULE=... \
//     node --test docs/evidence/tc4-issues-2026-10-27/repro/802-two-tabs.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, loadHome, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const BIBLE = 'sample_burrito';
const SHOTS = path.join(path.dirname(new URL(import.meta.url).pathname), '..', 'shots');
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });
const indicator = (page) => page.getByTestId('save-indicator');

async function openVerse(page, n) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('mode-switch').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
  await page.getByRole('tab', { name: 'Verse', exact: true }).click();
  await page.getByTitle('Edit this verse').nth(n - 1).click();
  await box(page, n).waitFor();
}

async function verse(env, n) {
  const hello = await env.call('app/hello', {});
  const view = await env.call('editor/chapters', { at: hello.sessions[0].at, book: 'TIT' });
  return view.chapters[0].verses.find((v) => v.key === String(n));
}

browserTest('two_tabs_on_one_project_never_lose_a_verse', async () => {
  await withApp(async ({ page: a, env, context }) => {
    const b = await context.newPage();
    const bErrors = [];
    b.on('pageerror', (e) => bErrors.push(e.message));
    await loadHome(b, env);
    await openVerse(a, 5);
    try {
      await openVerse(b, 5);
    } catch (error) {
      await b.screenshot({ path: path.join(SHOTS, '802-second-tab.png') });
      console.log('SECOND TAB:', JSON.stringify((await b.locator('body').innerText()).slice(0, 1500)));
      throw error;
    }
    // A types first, so its card holds the version from before B's save.
    await box(a, 5).press('Control+End');
    await box(a, 5).pressSequentially(' (A)');
    await box(b, 5).fill('Texto del segundo tab (B)');
    await b.getByRole('button', { name: 'Save verse' }).click();
    await until('B saved', async () => (await verse(env, 5)).words.value === 'Texto del segundo tab (B)');
    await a.getByRole('button', { name: 'Save verse' }).click();
    await until('A saved', async () => (await indicator(a).getAttribute('data-state')) === 'saved');
    const row = await verse(env, 5);
    const usfm = fs.readFileSync(path.join(env.projectsDir, BIBLE, 'ingredients', 'TIT.usfm'), 'utf8');
    await a.screenshot({ path: path.join(SHOTS, '802-two-tabs-a.png') });
    await b.reload();
    await b.getByTestId('account-menu').waitFor();
    await b.screenshot({ path: path.join(SHOTS, '802-two-tabs-b.png') });
    console.log(JSON.stringify({ marker: row.words.marker, value: row.words.value, usfmHasB: usfm.includes('segundo tab (B)'), bErrors }));
    assert.equal(row.words.marker?.versions, 2, 'A\'s save is a second version beside B\'s, not a revert');
    assert.deepEqual(bErrors, []);
  });
});
