// tC4 #78 repro on the real app: save Titus 1:1 twice in Translate › Verse (no teammate, no
// conflict), then look for a way to see that verse's earlier versions, with who and when.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/78-verse-history.test.mjs
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });
const indicator = (page) => page.getByTestId('save-indicator');

browserTest('78_own_verse_edits_history', async () => {
  await withApp(async ({ page }) => {
    await titus(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    for (const word of ['primero', 'segundo']) {
      await page.getByTitle('Edit this verse').first().click();
      await box(page, 1).waitFor();
      await box(page, 1).fill(`${await box(page, 1).inputValue()} ${word}`);
      await page.getByRole('button', { name: 'Save verse' }).click();
      await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
      await box(page, 1).waitFor({ state: 'detached' }).catch(() => {});
    }
    await page.waitForTimeout(1000);
    const historyControls = await page.getByText(/history/i).count();
    const historyButtons = await page.getByRole('button', { name: /history/i }).count();
    console.log(`78: Translate after two own saves: elements naming History = ${historyControls}, History buttons = ${historyButtons}`);
    await page.screenshot({ path: path.join(SHOTS, '78-translate-after-two-saves.png') });
    // The verse's menu, if it has one.
    const more = page.getByRole('button', { name: /more|actions|menu/i }).first();
    if (await more.count()) {
      await more.click().catch(() => {});
      console.log(`78: after opening ${JSON.stringify(await more.getAttribute('aria-label'))}: History items = ${await page.getByText(/history/i).count()}`);
    }
  });
});
