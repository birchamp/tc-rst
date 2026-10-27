// tC4 #833.5 on the real app: Get source texts' English package card names the notes, words
// and academy releases; compare them with the releases installed on this computer and pinned by
// a project.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app PLAYWRIGHT_MODULE=... \
//     node --test docs/evidence/tc4-issues-2026-10-27/repro/833-5-source-texts-versions.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../../../../parity/app/env-rust.mjs';
import { browserTest, catalog, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const en = catalog('en');
const SHOTS = path.join(REPO, 'docs', 'evidence', 'tc4-issues-2026-10-27', 'shots');

browserTest('833.5: the English package card names the releases that are installed', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId('project-sample_burrito').getByRole('button', { name: en['home.settings'], exact: true }).click();
    await page.getByTestId('settings-manage-sources').click();
    const modal = page.getByTestId('sources-modal');
    await modal.waitFor();
    const choose = page.getByText(en['sources.chooseGateway']);
    if (!(await choose.count())) await page.getByRole('button', { name: en['sources.changeLanguage'] }).click();
    await choose.waitFor();
    const card = modal.getByText(/notes v\d+ · words v\d+/).first();
    await until('the English card', () => card.count());
    const english = await modal.locator('*', { hasText: 'English' }).filter({ hasText: /notes v\d+/ }).last().innerText();
    console.log(`ENGLISH CARD: ${JSON.stringify(english)}`);
    await modal.screenshot({ path: path.join(SHOTS, '833-5-source-texts-versions.png') });
    const installed = {};
    for (const repo of ['en_tn', 'en_tw', 'en_ta']) {
      const dir = path.join(env.appData, 'resources', 'dcs', 'unfoldingword', repo);
      installed[repo] = fs.existsSync(dir) ? fs.readdirSync(dir).map((sha) => JSON.parse(fs.readFileSync(path.join(dir, sha, 'receipt.json'), 'utf8')).pin.version ?? '?') : [];
    }
    console.log(`INSTALLED: ${JSON.stringify(installed)}`);
    const suite = fs.readFileSync(path.join(REPO, 'crates', 'tc-resources', 'src', 'suite.rs'), 'utf8').match(/"v\d+"/g);
    console.log(`NEW-PROJECT PINS (tc-resources suite.rs): ${[...new Set(suite)].join(', ')}`);
    const shown = english.match(/notes (v\d+)/)[1];
    assert.ok(Object.values(installed).flat().concat(suite.map((s) => s.slice(1, -1))).includes(shown), `the card's notes release ${shown} is one this computer has or pins`);
  });
});
