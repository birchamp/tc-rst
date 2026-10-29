// tC4 #420 repro on the real app: Titus 1:1 saved with "siervo" in \bd and the first "Dios" in
// \it; the verse is opened in Align, one more word is placed and saved; aligned USFM is exported.
// Each style must still wrap its word in the aligned file, and no word is lost.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/420-style-after-align.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../../../../parity/app/env-rust.mjs';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();
const placed = (page) => page.getByTestId('align-progress').textContent();
const downloads = (context, env) => context.addCookies([{ name: 'tc4_token', value: env.token, url: env.origin }]);

async function download(page, name) {
  await page.getByTestId('export-menu-trigger').click();
  const [file] = await Promise.all([page.waitForEvent('download'), page.getByRole('menuitem', { name, exact: true }).click()]);
  assert.equal(await file.failure(), null);
  return fs.readFileSync(await file.path(), 'utf8');
}

browserTest('420_character_styles_survive_align_and_aligned_export', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    const at = (await env.call('app/hello', {})).sessions[0].at;
    const view = await env.call('editor/chapters', { at, book: 'TIT' });
    const row = view.chapters[0].verses.find((v) => v.key === '1');
    const words = 'Pablo, \\bd siervo\\bd* de \\it Dios\\it* y apóstol de Jesucristo, según la fe de los escogidos de Dios y el conocimiento de la verdad que va con la piedad,';
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words });
    const book = path.join(env.projectsDir, BIBLE, 'ingredients', 'TIT.usfm');
    await until('the save on disk', () => fs.readFileSync(book, 'utf8').includes('\\bd siervo\\bd*'));

    // Align 1:1: place one more word, as a translator does, and let it save.
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    const banner = await page.getByText(/draft changed/i).count();
    const start = await placed(page);
    await chip(page, 'fe').click();
    await card(page, 'πίστιν').click();
    await until('placed', async () => (await placed(page)) !== start);
    console.log('Align 1:1: stale banner shown:', banner > 0, '| placed', start, '->', await placed(page));
    await page.screenshot({ path: path.join(SHOTS, '420-align-styled-verse.png'), fullPage: true });
    await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved', 30_000);
    console.log('book still holds the styles:', /\\bd siervo\\bd\*/.test(fs.readFileSync(book, 'utf8')) && /\\it Dios\\it\*/.test(fs.readFileSync(book, 'utf8')));

    await page.getByText('All checking tools').click();
    await page.getByTestId('open-community-checking').click();
    await page.getByTestId('cc-page').first().waitFor();
    const aligned = await download(page, 'USFM, aligned').catch(async () => {
      await page.keyboard.press('Escape');
      await page.getByTestId('export-menu-trigger').click();
      const items = (await page.getByRole('menuitem').allTextContents()).map((s) => s.trim());
      throw new Error(`export menu: ${JSON.stringify(items)}`);
    });
    const verse = aligned.split(/(?=\\v )/).find((p) => p.startsWith('\\v 1 ') && aligned.indexOf(p) < aligned.indexOf('\\v 2 '));
    console.log('aligned 1:1:\n' + verse);
    assert.equal(await page.getByTestId('export-skipped').count(), 0, 'the verse is woven');
    assert.match(verse, /\\zaln-s [^\n]*x-content="πίστιν"[^]*?\\w fe\|/, 'the new link is in the file');
    assert.match(verse, /\\bd [^]*?\\w siervo\|[^]*?\\bd\*/, '\\bd wraps siervo');
    assert.match(verse, /\\it [^]*?\\w Dios\|[^]*?\\it\*/, '\\it wraps Dios');
    for (const w of words.replace(/\\[a-z]+\*?/g, ' ').match(/[\p{L}]+/gu)) assert.ok(verse.includes(`\\w ${w}|`), `word ${w} kept`);
  }, { init: downloads });
});
