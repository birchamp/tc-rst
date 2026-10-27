// tC4 #831 repro on the real app, the issue's own steps: save Titus 1:1 in Translate › Verse,
// open an undrafted verse, type, Cancel, open Check › Translation Words, go Home, read the
// Resume card.
//
//   (env as in the journeys) node --test docs/evidence/tc4-issues-2026-10-27/repro/831-resume-card.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });
const indicator = (page) => page.getByTestId('save-indicator');
const resume = (page) => page.getByTestId('resume-card');
const home = async (page) => {
  await page.getByTestId('project-switch').click();
  await page.getByTestId(`project-${BIBLE}`).waitFor();
};

browserTest('831_resume_card_after_cancel_and_check_tool', async () => {
  await withApp(async ({ page }) => {
    await titus(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    // 1. Save Titus 1:1.
    await page.getByTitle('Edit this verse').first().click();
    await box(page, 1).waitFor();
    const one = `${await box(page, 1).inputValue()} editado`;
    await box(page, 1).fill(one);
    await page.getByRole('button', { name: 'Save verse' }).click();
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await home(page);
    const afterSave = await resume(page).textContent();
    console.log('Resume after saving 1:1:', JSON.stringify(afterSave));
    await titus(page).click();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    // 2. An undrafted verse (6), something typed, Cancel.
    await page.getByRole('button', { name: 'start this verse' }).first().click();
    await box(page, 6).waitFor();
    await box(page, 6).pressSequentially('algo escrito');
    await page.waitForTimeout(500);
    await page.getByRole('button', { name: 'Cancel', exact: true }).click();
    await box(page, 6).waitFor({ state: 'detached' });
    await page.waitForTimeout(2500);
    await home(page);
    const afterCancel = await resume(page).textContent();
    console.log('Resume after Cancel on 1:6:', JSON.stringify(afterCancel));
    // 3. Check › Translation Words.
    await titus(page).click();
    await page.getByRole('tab', { name: 'Check', exact: true }).click();
    const tw = page.getByTestId('open-translationWords');
    const all = page.getByRole('button', { name: /All checking tools/ });
    await tw.or(all).first().waitFor();
    if (!(await tw.count())) await all.click();
    await tw.click();
    await page.getByTestId('check-quote').waitFor();
    const item = await page.getByTestId('check-quote').textContent().catch(() => '');
    console.log('Check tool open, first quote:', JSON.stringify(item));
    // 4. Home.
    await home(page);
    const afterCheck = await resume(page).textContent();
    console.log('Resume after opening Translation Words:', JSON.stringify(afterCheck));
    await page.screenshot({ path: path.join(SHOTS, '831-resume-card.png') });
    for (const [when, text] of [['cancel', afterCancel], ['check', afterCheck]]) {
      assert.ok(!text.includes('___'), `${when}: no stub on the card: ${text}`);
    }
    // The card's verse and its text must belong together.
    const m = afterCheck.match(/verse (\d+)/i);
    if (m?.[1] === '1') assert.ok(afterCheck.includes('editado') || !afterCheck.includes('"'), afterCheck);
    if (m?.[1] === '6') assert.ok(!afterCheck.includes('editado'), afterCheck);
  }, { scenario: 'en-pins' });
});
