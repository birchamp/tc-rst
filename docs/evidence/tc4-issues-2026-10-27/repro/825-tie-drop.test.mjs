// tC4 #825 repro on the real app and real store, no page.route: Titus 1:1 reads
// "Dios y Dios y y Dios", only the first "Dios" is linked (to θεοῦ). The translator saves the
// verse in Translate › Verse as "y y Dios y". Is the link dropped, is the verse flagged, is the
// translator told (Translate after the save; Align on the next open)?
//
//   (env as in the journeys) node --test docs/evidence/tc4-issues-2026-10-27/repro/825-tie-drop.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../../../../parity/app/env-rust.mjs';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const indicator = (page) => page.getByTestId('save-indicator');
const placedOnCards = (page) => page.locator('[data-testid^="align-card-"] button[data-word]');
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();
const saved = (page) => until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');

async function openAlign(page) {
  await page.getByRole('tab', { name: 'Check', exact: true }).click();
  await page.getByTestId('open-align').waitFor();
  await page.getByTestId('open-align').click();
  await page.getByTestId('align-bank').waitFor();
}

async function alignmentOnDisk(env, page) {
  await page.getByTestId('project-switch').click();
  await page.getByTestId(`project-${BIBLE}`).waitFor();
  await env.call('settings/viewFromHome', { project: BIBLE });
  const file = path.join(env.projectsDir, BIBLE, 'ingredients', 'checking', 'alignments', 'TIT.json');
  return JSON.parse(fs.readFileSync(file, 'utf8')).chapters['1']['1'];
}

browserTest('825_tie_drops_a_link_after_a_translate_save', async () => {
  await withApp(async ({ page, env }) => {
    await titus(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    const at = (await env.call('app/hello', {})).sessions[0].at;
    const row = (await env.call('editor/chapters', { at, book: 'TIT' })).chapters[0].verses.find((v) => v.key === '1');
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words: 'Dios y Dios y y Dios' });
    await openAlign(page);
    // Only the first Dios linked: send every placed word back, then place Dios (1st) on the third card (Θεοῦ).
    while ((await placedOnCards(page).count()) > 0) {
      const n = await placedOnCards(page).count();
      await placedOnCards(page).first().click();
      await until('one sent back', async () => (await placedOnCards(page).count()) < n);
    }
    await chip(page, 'Dios').click();
    await until('Dios chosen', async () => (await chip(page, 'Dios').getAttribute('data-armed')) === '1');
    await page.locator('[data-testid^="align-card-"]').nth(2).click();
    await until('one placed', async () => (await placedOnCards(page).count()) === 1);
    await saved(page);
    const before = await alignmentOnDisk(env, page);
    const linked = before.alignments.filter((g) => g.bottomWords.length).map((g) => `${g.topWords.map((w) => w.word).join('+')}=${g.bottomWords.map((w) => `${w.word}#${w.occurrence}`).join('+')}`);
    console.log('before: links', JSON.stringify(linked), 'invalid', JSON.stringify(before.invalid ?? null));

    // Translate › Verse: save "y y Dios y" through the card.
    await titus(page).click();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    const replies = [];
    page.on('response', async (r) => { if (r.url().includes('/editor/saveVerse')) replies.push(await r.text().catch(() => '')); });
    await page.getByTitle('Edit this verse').first().click();
    const box = page.getByLabel('Verse 1', { exact: true });
    await box.waitFor();
    await box.fill('y y Dios y');
    await page.getByRole('button', { name: 'Save verse' }).click();
    await saved(page);
    await page.waitForTimeout(1000);
    console.log('saveVerse reply:', replies.join('\n'));
    const text = await page.locator('main').innerText().catch(() => page.locator('body').innerText());
    const told = /link|bank|vínculo|align/i.test(text);
    console.log('Translate screen mentions a link / the bank / alignment after the save:', told);
    await page.screenshot({ path: path.join(SHOTS, '825-translate-after-save.png') });

    // Align on the next open.
    await openAlign(page);
    await page.waitForTimeout(1000);
    const onCards = await placedOnCards(page).count();
    const dropped = await page.getByTestId('align-dropped').count();
    const stale = await page.getByTestId('align-stale').count();
    const index = await env.call('align/index', { at: (await env.call('app/hello', {})).sessions[0].at, book: 'TIT' });
    console.log('Align next open: placed on cards', onCards, '| align-dropped shown', dropped, '| align-stale shown', stale);
    console.log('align/index 1:1 row:', JSON.stringify(JSON.stringify(index).match(/.{0,120}"1:1".{0,200}/)?.[0] ?? index).slice(0, 600));
    await page.screenshot({ path: path.join(SHOTS, '825-align-next-open.png') });
    const after = await alignmentOnDisk(env, page);
    const linkedAfter = after.alignments.filter((g) => g.bottomWords.length).map((g) => `${g.topWords.map((w) => w.word).join('+')}=${g.bottomWords.map((w) => `${w.word}#${w.occurrence}`).join('+')}`);
    console.log('after: links', JSON.stringify(linkedAfter), 'invalid', JSON.stringify(after.invalid ?? null), 'keys', Object.keys(after).join(','));
    assert.ok(linkedAfter.length > 0 || dropped > 0 || told || after.invalid === true, 'a dropped link is kept, flagged, or named to the translator');
  });
});
