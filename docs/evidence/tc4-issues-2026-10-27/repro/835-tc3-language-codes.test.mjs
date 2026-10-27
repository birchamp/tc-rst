// tC4 #835 on the real app's tC3 import review page (USFM import is cut from 4.1.0): which
// language codes the page lets through, what its hint says, and whether x- codes, qaa and kau
// then import and are stored, or end in a raw error as in tC4.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app PLAYWRIGHT_MODULE=... \
//     node --test docs/evidence/tc4-issues-2026-10-27/repro/835-tc3-language-codes.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../../../../parity/app/env-rust.mjs';
import { browserTest, sleep, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const TIT = path.join(REPO, 'fixtures', 'tc4', 'conformance-fixtures', 'import', 'tc3', 'cfm_fbt_tit_book.zip');
const SHOTS = path.join(REPO, 'docs', 'evidence', 'tc4-issues-2026-10-27', 'shots');

async function toReview(page, name) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-import').click();
  await page.getByTestId('import-kind-tc3').click();
  await page.getByTestId('import-file-input').setInputFiles([TIT]);
  const next = page.getByTestId('import-to-review');
  await until('the file is sent', () => next.isEnabled());
  await next.click();
  await page.getByTestId('import-review').waitFor();
  await page.getByTestId('import-name').fill(name);
  await page.getByTestId('import-use-installed').click();
}

browserTest('835: the tC3 review page and import with x- codes, es-419, qaa and kau', async () => {
  await withApp(async ({ page, env }) => {
    await toReview(page, 'Tita probe');
    const lang = page.getByTestId('import-lang');
    const field = lang.locator('xpath=ancestor::*[.//label][1]');
    const hint = async () => (await field.innerText()).replace(/\s+/g, ' ');
    const seen = { initial: { code: await lang.inputValue(), field: await hint() } };
    for (const code of ['x-scrqa', 'x-scratchqa', 'x-qiu', 'es-419', 'qaa', 'kau', '']) {
      await lang.fill(code);
      await sleep(1500);
      seen[code || '(empty)'] = {
        field: await hint(),
        importEnabled: await page.getByTestId('import-run').isEnabled(),
        details: await page.getByTestId('import-check-details').getAttribute('data-status'),
      };
      if (code === 'kau') await page.getByTestId('import-review').screenshot({ path: path.join(SHOTS, '835-tc3-kau.png') });
    }
    for (const [k, v] of Object.entries(seen)) console.log(`CODE ${k}: ${JSON.stringify(v)}`);

    const results = {};
    for (const code of ['x-scrqa', 'qaa', 'kau']) {
      await page.goto('about:blank');
      await page.goto(`${env.origin}/#t=${env.token}`);
      await page.getByTestId('account-menu').waitFor();
      const folder = `tita_${code.replace(/-/g, '_')}`;
      await toReview(page, `Tita ${code}`);
      await page.getByTestId('import-lang').fill(code);
      await until('Import is enabled', () => page.getByTestId('import-run').isEnabled());
      await page.getByTestId('import-run').click();
      const made = path.join(env.projectsDir, folder, 'metadata.json');
      const ok = await until(`${code} imported`, () => fs.existsSync(made), 60_000).catch(() => false);
      const shown = (await page.locator('body').innerText()).replace(/\s+/g, ' ');
      const tag = ok ? JSON.parse(fs.readFileSync(made, 'utf8')).languages[0].tag : null;
      results[code] = { made: Boolean(ok), tag, rawError: /write-failed|PankosmiaError|import\.[a-z-]+\./.test(shown) };
      console.log(`IMPORT ${code}: ${JSON.stringify(results[code])}${ok ? '' : `; page: ${JSON.stringify(shown.slice(0, 500))}; folders: ${fs.readdirSync(env.projectsDir).join(',')}`}`);
    }
    for (const code of ['x-scrqa', 'x-scratchqa', 'x-qiu', 'es-419']) assert.ok(seen[code].importEnabled, `${code} accepted on the page`);
    for (const code of ['x-scrqa', 'qaa', 'kau']) {
      assert.equal(results[code].rawError, false, `${code}: no raw error`);
      assert.equal(results[code].tag, code, `${code} imported and stored`);
    }
  });
});
