// tC4 #830 repro on the real app, no page.route: an Old Testament (Hebrew) verse in Align, two
// translation words placed on one card, their on-screen x positions compared. Left-to-right
// (Spanish sample, Jonah 1:1) and right-to-left (a new Arabic project with Jonah from USFM).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=target/debug/tc-door43-fake PLAYWRIGHT_MODULE=... \
//     node --test docs/evidence/tc4-issues-2026-10-27/repro/830-ot-card-order.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../../../../parity/app/env-rust.mjs';
import { browserTest, catalog, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const en = catalog('en');
const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const SCENARIO = { scenario: 'en-pins' };
const allTools = (page) => page.getByRole('button', { name: /All checking tools/ });
const checkTab = (page) => page.getByRole('tab', { name: en['nav.check'], exact: true });
const bank = (page) => page.locator('[data-testid="align-bank"] > button');
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const cards = (page) => page.locator('[data-testid^="align-card-"]');
const armed = (page) => page.locator('[data-testid="align-bank"] > button[data-armed="1"]');

async function openAlign(page) {
  await checkTab(page).click();
  await page.getByTestId('open-align').or(allTools(page)).first().waitFor();
  if (!(await page.getByTestId('open-align').count())) await allTools(page).click();
  await page.getByTestId('open-align').click();
  await page.getByTestId('align-bank').waitFor();
  await page.locator('[data-testid^="align-card-"] span[lang="hbo"]').first().waitFor();
}

/** Places `a` then `b` (Ctrl) on the first card; returns the placed words in DOM order with x. */
async function placeTwo(page, a, b) {
  await chip(page, a).click();
  await until('one chosen', async () => (await armed(page).count()) === 1);
  await chip(page, b).click({ modifiers: ['Control'] });
  await until('two chosen', async () => (await armed(page).count()) === 2);
  await cards(page).first().click();
  const words = cards(page).first().locator('button[data-word]');
  await until('two placed on card 1', async () => (await words.count()) === 2);
  return words.evaluateAll((els) => els.map((e) => ({
    word: e.getAttribute('data-word'),
    x: Math.round(e.getBoundingClientRect().x),
    dir: getComputedStyle(e).direction,
    parentDir: getComputedStyle(e.parentElement).direction,
  })));
}

async function cardXs(page) {
  return cards(page).evaluateAll((els) => els.slice(0, 3).map((e) => Math.round(e.getBoundingClientRect().x)));
}

browserTest('830_ltr_translation_on_hebrew_verse', async () => {
  await withApp(async ({ page, env }) => {
    const project = 'sample_burrito';
    await page.getByTestId(`project-${project}`).getByTestId('book-tile-JON').click();
    await checkTab(page).waitFor();
    const at = (await env.call('app/hello', {})).sessions[0].at;
    const stub = (await env.call('editor/chapters', { at, book: 'JON' })).chapters[0].verses.find((v) => v.key === '1');
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: stub.words.token, words: 'Vino palabra de Jehová a Jonás' });
    await openAlign(page);
    const xs = await cardXs(page);
    const placed = await placeTwo(page, 'Vino', 'palabra');
    await page.screenshot({ path: path.join(SHOTS, '830-ltr-hebrew.png') });
    console.log('LTR cards x (first three):', JSON.stringify(xs));
    console.log('LTR placed on card 1:', JSON.stringify(placed));
    assert.ok(xs[0] > xs[1], `cards keep Hebrew order, right to left: ${xs}`);
    const [first, second] = placed;
    assert.equal(first.word, 'Vino');
    assert.ok(first.x < second.x, `LTR: "Vino" left of "palabra": ${JSON.stringify(placed)}`);
  }, SCENARIO);
});

const JONAH = Buffer.from([
  '\\id JON', '\\usfm 3.0', '\\h يونان', '\\mt يونان', '\\c 1', '\\p',
  '\\v 1 وصار قول الرب إلى يونان بن أمتاي قائلا',
  '\\v 2 قم اذهب إلى نينوى المدينة العظيمة',
  '\\v 3 فقام يونان ليهرب إلى ترشيش',
  '',
].join('\n'));

browserTest('830_rtl_translation_on_hebrew_verse', async () => {
  await withApp(async ({ page, env }) => {
    const listed = () => fs.readdirSync(env.projectsDir).filter((n) => !n.startsWith('.')).sort();
    const before = listed();
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-bible').click();
    await page.getByLabel(en['newBible.langName']).waitFor();
    await page.getByLabel(en['newBible.name']).fill('Arabic Jonah');
    await page.getByLabel(en['newBible.langName']).fill('العربية');
    await page.getByLabel(en['newBible.code'], { exact: true }).click();
    await page.getByLabel(en['newBible.code'], { exact: true }).fill('ar');
    await page.getByRole('button', { name: en['wizard.rtl'] }).click();
    await page.getByRole('button', { name: en['newBible.create'] }).click();
    await page.getByTestId('ab-usfm-option').click();
    await page.getByTestId('ab-usfm-input').setInputFiles([{ name: 'JON.usfm', mimeType: 'text/plain', buffer: JONAH }]);
    const row = page.getByTestId('ab-usfm-file');
    await until('valid', async () => (await row.count()) === 1 && (await row.getAttribute('data-status')) === 'valid');
    await page.getByTestId('ab-usfm-add').click();
    await page.getByTestId('ab-usfm').waitFor({ state: 'detached' });
    const [folder] = listed().filter((n) => !before.includes(n));
    await page.getByTestId(`project-${folder}`).getByTestId('book-tile-JON').click();
    await openAlign(page);
    const xs = await cardXs(page);
    const placed = await placeTwo(page, 'وصار', 'قول');
    await page.screenshot({ path: path.join(SHOTS, '830-rtl-hebrew.png') });
    console.log('RTL cards x (first three):', JSON.stringify(xs));
    console.log('RTL placed on card 1:', JSON.stringify(placed));
    const [first, second] = placed;
    assert.equal(first.word, 'وصار');
    assert.ok(first.x > second.x, `RTL: the first word right of the second: ${JSON.stringify(placed)}`);
  }, SCENARIO);
});
