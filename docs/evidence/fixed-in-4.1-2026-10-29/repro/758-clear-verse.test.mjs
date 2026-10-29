// tC4 #758 repro on the real app: place a word in Align, then look for a way to clear every
// alignment of the verse (a "Clear" control, or tC3's Ctrl+K) and a confirmation dialog.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/758-clear-verse.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();
const placed = (page) => page.getByTestId('align-progress').textContent();

browserTest('758_no_clear_alignments_control_in_align', async () => {
  await withApp(async ({ page }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    const start = await placed(page);
    await chip(page, 'fe').click();
    await card(page, 'πίστιν').click();
    await until('placed', async () => (await placed(page)) !== start);
    const after = await placed(page);
    console.log('placed before:', start, '| after placing fe:', after);
    // Turn suggestions on too, so the suggestions row shows every button it has.
    await page.getByTestId('align-suggest-switch').check();
    const buttons = await page.locator('main button, [data-testid="align-suggestions"] button').evaluateAll(
      (bs) => bs.map((b) => (b.textContent || '').trim() || b.getAttribute('aria-label') || b.getAttribute('title')).filter(Boolean));
    console.log('buttons on the Align screen:', JSON.stringify([...new Set(buttons)]));
    const clear = buttons.filter((b) => /clear|start over|remove all/i.test(b));
    console.log('a Clear control:', JSON.stringify(clear));
    await page.screenshot({ path: path.join(SHOTS, '758-align-no-clear.png'), fullPage: true });
    // tC3's shortcut.
    await page.locator('body').click({ position: { x: 2, y: 2 } });
    await page.keyboard.press('Control+k');
    await page.waitForTimeout(500);
    const dialogs = await page.locator('[role="dialog"], [role="alertdialog"]').count();
    console.log('after Ctrl+K: placed', await placed(page), '| dialogs open:', dialogs);
    assert.deepEqual(clear, [], 'no Clear control (the issue asks for one)');
    assert.equal(await placed(page), after, 'Ctrl+K clears nothing');
  });
});
