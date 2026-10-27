// tC4 #829 repro on the real app: a poem verse in Translate's Verse editor and Section editor.
// Reads the computed tab-size of both textareas, and presses Tab in each editor to see whether
// it indents the line or leaves the field (comment by eliaspinero).
//
//   (env as in the journeys) node --test docs/evidence/tc4-issues-2026-10-27/repro/829-poem-tabs.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { newRequestId } from '../../../../parity/app/env-rust.mjs';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });
const POEM = 'a Tito,\n\\q2 mi verdadero hijo en la fe común, una línea larga que se parte en dos o más renglones en la tarjeta\n\\q1 en la fe común';

async function openTranslate(page, env) {
  await titus(page).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
  await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
  const at = (await env.call('app/hello', {})).sessions[0].at;
  const row = (await env.call('editor/chapters', { at, book: 'TIT' })).chapters[0].verses.find((v) => v.key === '4');
  await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words: POEM });
  return at;
}

browserTest('829_verse_editor_tab_size_and_tab_key', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page, env);
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByTitle('Edit this verse').nth(3).click();
    await box(page, 4).waitFor();
    await until('poem shown', async () => (await box(page, 4).inputValue()).includes('\t\tmi'));
    const tabSize = await box(page, 4).evaluate((el) => getComputedStyle(el).tabSize);
    console.log('Verse editor value:', JSON.stringify(await box(page, 4).inputValue()));
    console.log('Verse editor computed tab-size:', tabSize);
    await page.screenshot({ path: path.join(SHOTS, '829-verse-editor.png') });
    // Tab at the start of the last line.
    await box(page, 4).focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Home');
    await page.keyboard.press('Tab');
    await page.keyboard.type('XYZ');
    const open = await box(page, 4).count();
    const value = open ? await box(page, 4).inputValue() : null;
    const focused = await page.evaluate(() => `${document.activeElement?.tagName} ${document.activeElement?.getAttribute('aria-label') ?? ''}`);
    console.log('After Tab+XYZ: editor open:', open, 'focused:', focused, 'value:', JSON.stringify(value));
    assert.ok(open && value.includes('\t\ten la fe') , 'Tab indents the line in the Verse editor');
  });
});

browserTest('829_section_editor_tab_size_and_tab_key', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page, env);
    await page.getByRole('button', { name: /Draft section 4/ }).first().click();
    const area = page.getByRole('textbox', { name: /^Section 4/ });
    await area.waitFor();
    const tabSize = await area.evaluate((el) => getComputedStyle(el).tabSize);
    console.log('Section editor value:', JSON.stringify(await area.inputValue()));
    console.log('Section editor computed tab-size:', tabSize);
    await page.screenshot({ path: path.join(SHOTS, '829-section-editor.png') });
    const before = await area.inputValue();
    const at = before.indexOf('\ten la fe');
    await area.evaluate((el, i) => { el.focus(); el.setSelectionRange(i + 1, i + 1); }, at);
    await page.keyboard.press('Tab');
    const after = await area.inputValue();
    const focused = await page.evaluate(() => document.activeElement?.getAttribute('aria-label'));
    console.log('Section editor after Tab: focused:', focused, 'value:', JSON.stringify(after));
    assert.equal(tabSize, '2ch', 'one indent level is two characters wide');
  });
});
