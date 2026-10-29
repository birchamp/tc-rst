// tC4 #701 on the real app: the sample project's Project settings > Get source texts lists every
// gateway package; read the Urdu (Devanagari) card's name and its dir attribute.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/701-urdu-devanagari-card.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, catalog, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const en = catalog('en');

browserTest('701_urdu_devanagari_card_reads_left_to_right', async () => {
  await withApp(async ({ page }) => {
    await page.getByTestId('project-sample_burrito').getByRole('button', { name: en['home.settings'], exact: true }).click();
    await page.getByTestId('settings-manage-sources').click();
    const modal = page.getByTestId('sources-modal');
    await modal.waitFor();
    const choose = page.getByText(en['sources.chooseGateway']);
    if (!(await choose.count())) await page.getByRole('button', { name: en['sources.changeLanguage'] }).click();
    await choose.waitFor();
    const cards = await modal.locator('span[dir]').evaluateAll((els) => els.map((e) => `${e.getAttribute('dir')} ${e.textContent}`));
    console.log('package names with dir:', JSON.stringify(cards));
    const urdu = modal.locator('span[dir]', { hasText: 'उर्दू' });
    await urdu.scrollIntoViewIfNeeded();
    await page.screenshot({ path: path.join(SHOTS, '701-urdu-devanagari-card.png') });
    assert.equal(await urdu.count(), 1, 'one Urdu card named उर्दू');
    assert.equal(await urdu.getAttribute('dir'), 'ltr');
    assert.equal(await modal.locator('span[dir]', { hasText: 'اردو' }).count(), 0, 'no Arabic-script Urdu name');
  });
});
