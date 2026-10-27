// tC4 #832 on the real app: import the tC3 project cfm_fbt_tit_book.zip, read Settings'
// Language name, choose Right to left, Save changes, reload, and read the Home card's badge.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app PLAYWRIGHT_MODULE=... \
//     node --test docs/evidence/tc4-issues-2026-10-27/repro/832-tc3-rtl-badge.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../../../../parity/app/env-rust.mjs';
import { browserTest, catalog, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const en = catalog('en');
const TIT = path.join(REPO, 'fixtures', 'tc4', 'conformance-fixtures', 'import', 'tc3', 'cfm_fbt_tit_book.zip');
const SHOTS = path.join(REPO, 'docs', 'evidence', 'tc4-issues-2026-10-27', 'shots');

browserTest('832: a tC3-imported project set to right to left shows RTL on Home after a reload, and Settings shows its language name', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'Tita tC3';
    const folder = 'tita_tc3';
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-import').click();
    await page.getByTestId('import-kind-tc3').click();
    await page.getByTestId('import-file-input').setInputFiles([TIT]);
    const toReview = page.getByTestId('import-to-review');
    await until('the file is sent', () => toReview.isEnabled());
    await toReview.click();
    await page.getByTestId('import-review').waitFor();
    await page.getByTestId('import-name').fill(name);
    await page.getByTestId('import-use-installed').click();
    await until('Import is enabled', () => page.getByTestId('import-run').isEnabled());
    await page.getByTestId('import-run').click();
    await page.getByTestId('import-toast').waitFor({ timeout: 120_000 });
    const card = () => page.getByTestId(`project-${folder}`);
    await card().waitFor();
    const before = await card().innerText();
    console.log(`CARD BEFORE: ${JSON.stringify(before)}`);

    const settings = () => card().getByRole('button', { name: en['home.settings'], exact: true });
    await settings().click();
    const save = page.getByRole('button', { name: en['settings.save'] });
    await until('Save is enabled once the values came', () => save.isEnabled());
    const langName = await page.locator('#st-lang').inputValue();
    console.log(`SETTINGS LANGUAGE NAME: ${JSON.stringify(langName)}`);
    await page.getByRole('button', { name: en['wizard.rtl'] }).click();
    await save.click();
    await save.waitFor({ state: 'detached' });
    const written = JSON.parse(fs.readFileSync(path.join(env.projectsDir, folder, 'ingredients', 'checking', 'settings.json'), 'utf8'));
    console.log(`SETTINGS.JSON textDirection: ${written.textDirection}`);
    const meta = JSON.parse(fs.readFileSync(path.join(env.projectsDir, folder, 'metadata.json'), 'utf8'));
    console.log(`METADATA languages[0]: ${JSON.stringify(meta.languages[0])}`);

    // A reload as a person makes it: env.load would reset the scenario and drop the import.
    await page.goto('about:blank');
    await page.goto(`${env.origin}/#t=${env.token}`);
    await page.getByTestId('account-menu').waitFor();
    await card().waitFor();
    const after = await card().innerText();
    console.log(`CARD AFTER RELOAD: ${JSON.stringify(after)}`);
    await card().screenshot({ path: path.join(SHOTS, '832-tc3-rtl-badge.png') });
    assert.equal(written.textDirection, 'rtl');
    assert.match(after, /\bRTL\b/, 'the Home badge shows the direction the project opens with');
    assert.doesNotMatch(after, /\bLTR\b/);
    assert.equal(langName, 'Chin, Falam', 'Settings shows the metadata language name');
  });
});
