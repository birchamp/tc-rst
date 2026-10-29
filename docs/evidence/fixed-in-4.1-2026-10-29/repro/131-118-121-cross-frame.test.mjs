// tC4 #131, #118 and #121 on the real app, no reply mocked: a New Bible created in another
// versification, a blank book added, then Understand and Translate read through the server's
// own `editor/sources` mapping.
//   - rsc Jonah: project 2:1 is the sources' (eng) Jonah 1:17 (#131 Translate, #118 Understand).
//   - lxx Ezra: project 11:1 is the sources' Nehemiah 1:1, another book (#121).
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/131-118-121-cross-frame.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { REPO } from '../../../../parity/app/env-rust.mjs';
import { browserTest, catalog, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const en = catalog('en');

// The scenario installs tC4's test fixtures, whose ULT holds Titus only, and no Titus verse moves
// in any scheme. So the bundled en_ult v91's Jonah, Ezra and Nehemiah (the books the installer
// ships) are put beside the installed ULT's Titus before the project is made.
function addBundledUltBooks(env) {
  const found = [];
  const walk = (d) => { for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name);
    if (e.isDirectory()) walk(p); else if (e.name === 'TIT.usfm' && /ult/i.test(p)) found.push(path.dirname(p));
  } };
  walk(env.appData);
  for (const dir of found) for (const book of ['JON', 'EZR', 'NEH']) {
    const bytes = execFileSync('unzip', ['-p', path.join(REPO, 'bundle', 'en', 'en_ult_v91.sb.zip'), `en_ult/ingredients/${book}.usfm`], { maxBuffer: 1 << 26 });
    fs.writeFileSync(path.join(dir, `${book}.usfm`), bytes);
  }
  console.log('ULT folders given JON, EZR, NEH:', JSON.stringify(found.map((d) => path.relative(env.appData, d))));
}

async function newBible(page, { name, scheme, book }) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-bible').click();
  await page.getByLabel('Language name').waitFor();
  await page.getByLabel('Bible name').fill(name);
  await page.getByLabel('Language name').fill('Español');
  await page.getByLabel('Code', { exact: true }).click();
  await page.getByLabel('Code', { exact: true }).fill('es-419');
  await page.getByRole('button', { name: 'Left to right' }).click();
  await page.getByRole('button', { name: /Versification/ }).click();
  await page.getByRole('combobox', { name: 'Versification' }).click();
  await page.getByRole('option', { name: scheme, exact: true }).click();
  await page.getByRole('button', { name: en['newBible.create'] }).click();
  await page.getByRole('button', { name: en['addBook.blankTitle'] }).click();
  await page.getByRole('combobox', { name: en['addBook.book'], exact: true }).click();
  await page.getByRole('option', { name: new RegExp(`^${book}`) }).first().click();
  await page.getByRole('button', { name: en['addBook.create'] }).click();
  await page.getByTestId('understand').waitFor();
}

async function chapter(page, book, n) {
  await page.getByRole('button', { name: String(n), exact: true }).click();
  await page.getByRole('heading', { name: `${book} ${n}`, exact: true }).waitFor();
}

browserTest('131_118_rsc_jonah_2_reads_the_sources_1_17', async () => {
  await withApp(async ({ page, env }) => {
    addBundledUltBooks(env);
    await newBible(page, { name: 'Jonás rsc', scheme: 'rsc', book: 'Jonah' });
    await chapter(page, 'Jonah', 2);
    const only = page.getByTestId('understand-verse-only');
    await only.waitFor();
    console.log('#118 Understand statement:', JSON.stringify(await only.textContent()));
    console.log('#118 Section/Verse tabs:', await page.getByRole('tab', { name: 'Verse', exact: true }).count());
    await page.screenshot({ path: path.join(SHOTS, '118-rsc-jonah-2-understand.png'), fullPage: true });
    console.log('#118 main text:', JSON.stringify((await page.locator('main').textContent()).replace(/\s+/g, ' ').slice(0, 600)));
    const units = page.locator('[data-testid^="understand-unit-"]');
    console.log('#118 Understand units in chapter 2:', await units.count());
    const first = units.first();
    const firstText = (await first.textContent()).replace(/\s+/g, ' ').slice(0, 220);
    console.log('#118 first unit:', await first.getAttribute('data-testid'), JSON.stringify(firstText));
    await page.screenshot({ path: path.join(SHOTS, '118-rsc-jonah-2-understand.png'), fullPage: true });
    assert.match(firstText, /great fish/, 'project 2:1 shows eng 1:17 (the fish), not eng 2:1 (the prayer)');

    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByTestId('draft-verse-only').waitFor();
    console.log('#131 Translate statement:', JSON.stringify(await page.getByTestId('draft-verse-only').textContent()));
    const src = await page.getByTestId('draft-source-text').first().textContent();
    console.log('#131 first source row:', JSON.stringify(src.slice(0, 200)));
    const labels = await page.locator('main span').filter({ hasText: /^Jonah \d/ }).allTextContents();
    console.log('#131 source labels:', JSON.stringify(labels.slice(0, 4)));
    await page.screenshot({ path: path.join(SHOTS, '131-rsc-jonah-2-translate.png'), fullPage: true });
    assert.match(src, /great fish/);
    assert.equal(labels[0], 'Jonah 1:17');
  });
}, { timeout: 170_000 });

browserTest('121_lxx_ezra_11_is_nehemiah_1', async () => {
  await withApp(async ({ page, env }) => {
    addBundledUltBooks(env);
    await newBible(page, { name: 'Esdras lxx', scheme: 'lxx', book: 'Ezra' });
    await chapter(page, 'Ezra', 11);
    await page.screenshot({ path: path.join(SHOTS, '121-lxx-ezra-11-understand.png'), fullPage: true });
    console.log('#121 main text:', JSON.stringify((await page.locator('main').textContent()).replace(/\s+/g, ' ').slice(0, 600)));
    const units = page.locator('[data-testid^="understand-unit-"]');
    await units.first().waitFor();
    const first = (await units.first().textContent()).replace(/\s+/g, ' ').slice(0, 260);
    console.log('#121 Understand Ezra 11 first unit:', await units.first().getAttribute('data-testid'), JSON.stringify(first));
    const helps = await page.getByText(/another book|different book/i).allTextContents();
    console.log('#121 statements naming another book:', JSON.stringify(helps.slice(0, 3)));
    await page.screenshot({ path: path.join(SHOTS, '121-lxx-ezra-11-understand.png'), fullPage: true });
    const shown = /words of Nehemiah|Hakaliah/.test(first);
    console.log('#121 Nehemiah 1:1 text shown:', shown);
  });
}, { timeout: 170_000 });
