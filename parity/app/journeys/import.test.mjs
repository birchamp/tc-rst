// Import a translationCore 3 project (F1): the tC3 cases of tC4's e2e/j09-import.spec.ts
// (b86270f): a damaged file (:168), all or nothing (:176), offline and several zips (:225,
// :345), online on the fake Door43 (:280, :321) and the name clash (:367). The USFM and Scripture
// Burrito kinds wait for their importers in this release (rulings/2026-10-24-H3-H4-H7).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/import.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../env-rust.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const TC3 = path.join(REPO, 'fixtures', 'tc4', 'conformance-fixtures', 'import', 'tc3');
const TIT = path.join(TC3, 'cfm_fbt_tit_book.zip');
const MULTI = ['jhn', 'job', 'luk'].map((b) => path.join(TC3, 'multi', `en_kjv_${b}_book.zip`));
const abbrOf = (name) => name.toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
const listed = (env) => fs.readdirSync(env.projectsDir).filter((n) => !n.startsWith('.')).sort();

/** From Home to the review step of `files` (paths or payloads), with `name` typed. */
async function review(page, files, { name } = {}) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-import').click();
  await page.getByTestId('import-kind-tc3').click();
  await page.getByTestId('import-file-input').setInputFiles(files);
  const run = page.getByTestId('import-to-review');
  await until('every file is sent', () => run.isEnabled());
  await run.click();
  await page.getByTestId('import-review').waitFor();
  if (name !== undefined) await page.getByTestId('import-name').fill(name);
}

const resources = (page) => page.getByTestId('import-resources');
const state = (page) => resources(page).getAttribute('data-state');

/** "Look up on Door43", the internet question, and its yes (D95). */
async function lookUp(page) {
  await page.getByTestId('import-allow-internet').click();
  const ask = page.getByTestId('net-ask');
  await ask.waitFor();
  assert.equal(await ask.getAttribute('data-kind'), 'importVersions');
  await page.getByTestId('net-confirm').click();
  await until('the lookup ends', async () => !['offline', 'looking'].includes(await state(page)));
}

async function imported(page, name) {
  await page.getByTestId('import-run').click();
  const toast = page.getByTestId('import-toast');
  await toast.waitFor({ timeout: 120_000 });
  assert.match(await toast.textContent(), new RegExp(`${name} was imported`));
  const card = page.getByTestId(`project-${abbrOf(name)}`);
  await card.waitFor();
  await card.getByTestId('imported-badge').waitFor();
}

browserTest('refuse: a damaged file is refused on the review page and nothing is written', async () => {
  await withApp(async ({ page, env }) => {
    const before = listed(env);
    const bytes = fs.readFileSync(TIT);
    await review(page, [{ name: 'cfm-damaged.zip', mimeType: 'application/zip', buffer: bytes.subarray(0, bytes.length >> 1) }]);
    const damaged = page.getByTestId('import-damaged');
    assert.equal(await damaged.getAttribute('data-code'), 'import.damaged.truncated');
    assert.match(await damaged.textContent(), /cfm-damaged\.zip is not a complete zip file\./);
    assert.ok(await page.getByTestId('import-run').isDisabled());
    assert.deepEqual(listed(env), before);
  });
});

browserTest('all or nothing: a write failure leaves no project and says import.write-failed', async () => {
  await withApp(async ({ page, env }) => {
    const before = listed(env);
    await review(page, [TIT], { name: 'Falla tC3' });
    await page.getByTestId('import-use-installed').click();
    // The projects folder refuses every new entry, so the create fails while it writes.
    fs.chmodSync(env.projectsDir, 0o555);
    try {
      await page.getByTestId('import-run').click();
      const failed = page.getByTestId('import-failed');
      await failed.waitFor({ timeout: 120_000 });
      assert.equal(await failed.getAttribute('data-code'), 'import.write-failed');
      assert.match(await failed.textContent(), new RegExp(en['refusal.import.write-failed'].replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    } finally {
      fs.chmodSync(env.projectsDir, 0o755);
    }
    assert.deepEqual(listed(env), before, 'no project and no folder at its name');
  });
});

browserTest('tC3 offline: the review page shows what carries over; Use installed versions; one new project', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'Tita tC3';
    await review(page, [TIT], { name });
    await page.getByTestId('import-book-TIT').waitFor();
    assert.equal(await page.getByTestId('import-damaged').count(), 0);
    const carried = await page.getByTestId('import-carried').textContent();
    for (const part of ['Verses46', 'Checking decisions5', 'Contributors0']) assert.ok(carried.includes(part), `${part} in ${carried}`);
    assert.match(await page.getByTestId('import-book-TIT').textContent(), /3 chapters · 46 verses/);
    assert.ok(await page.getByText('CC BY-SA 4.0', { exact: true }).isVisible());
    // Offline: the versions are not looked up, so Import waits for a choice.
    assert.equal(await state(page), 'offline');
    assert.ok(await page.getByTestId('import-allow-internet').isVisible());
    assert.ok(await page.getByTestId('import-run').isDisabled());
    await page.getByTestId('import-use-installed').click();
    assert.equal(await state(page), 'installed');
    await imported(page, name);
    const folder = path.join(env.projectsDir, abbrOf(name));
    const meta = JSON.parse(fs.readFileSync(path.join(folder, 'metadata.json'), 'utf8'));
    assert.equal(meta.languages[0].tag, 'cfm');
    assert.equal(fs.readFileSync(path.join(folder, 'ingredients', 'TIT.usfm'), 'utf8').match(/^\\c \d+/gm).length, 3);
    // D95: nothing asked the internet.
    assert.equal(await page.getByTestId('net-ask').count(), 0);
  });
});

browserTest('tC3 multi-zip: three books of one language are one project', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'KJV tC3';
    await review(page, MULTI, { name });
    for (const code of ['JHN', 'JOB', 'LUK']) await page.getByTestId(`import-book-${code}`).waitFor();
    assert.equal(await page.getByTestId('import-damaged').count(), 0);
    // The files disagree on the license: the check says so.
    assert.equal(await page.getByTestId('import-check-license').getAttribute('data-status'), 'warn');
    await page.getByTestId('import-use-installed').click();
    await imported(page, name);
    for (const code of ['JHN', 'JOB', 'LUK']) assert.ok(fs.existsSync(path.join(env.projectsDir, abbrOf(name), 'ingredients', `${code}.usfm`)), code);
  });
});

browserTest('tC3 online: the lookup waits for its click and the question, then names what Door43 has', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'Tita tC3 online';
    await env.door43.clear();
    await review(page, [TIT], { name });
    assert.equal(await state(page), 'offline');
    assert.deepEqual((await env.door43.requests()).filter((r) => r.path.includes('/tags')), [], 'nothing looked up before the click');
    await lookUp(page);
    const asked = (await env.door43.requests()).filter((r) => r.path.includes('/tags'));
    assert.ok(asked.length > 0, 'the lookup asked Door43 for the tags');
    const shown = await state(page);
    assert.ok(['found', 'missing'].includes(shown), shown);
    if (shown === 'missing') await page.getByTestId('import-use-installed').click();
    await imported(page, name);
  }, { door43: true });
});

browserTest('tC3 online, mixed: Use installed versions keeps the found pins named', async () => {
  await withApp(async ({ page }) => {
    await review(page, MULTI, { name: 'KJV tC3 mixed' });
    await lookUp(page);
    const before = await resources(page).textContent();
    if ((await state(page)) === 'missing') {
      await page.getByTestId('import-use-installed').click();
      assert.equal(await state(page), 'installed');
      const after = await resources(page).textContent();
      const found = before.match(/were found: (.*?)\./)?.[1];
      if (found) assert.ok(after.includes(found), 'the found versions stay named');
    }
    await imported(page, 'KJV tC3 mixed');
  }, { door43: true });
});

browserTest('tC3 name clash: a Bible name of a project on this computer is flagged and Import stays off; a stale review still writes nothing (#436)', async () => {
  await withApp(async ({ page, env }) => {
    const message = en['refusal.import.name-exists'];
    const field = page.getByTestId('import-name-field');
    const run = page.getByTestId('import-run');
    // The sample's own project, by the name its folder makes.
    await review(page, [TIT], { name: 'Sample Burrito' });
    await page.getByTestId('import-use-installed').click();
    await until('the name-exists message', async () => (await field.getByRole('alert').count()) > 0);
    assert.equal(await field.getByRole('alert').textContent(), message);
    assert.ok(await run.isDisabled());
    await page.getByTestId('import-name').fill('Tita tC3 clash');
    await until('the message clears', async () => (await field.getByRole('alert').count()) === 0);
    assert.ok(await run.isEnabled());
    // Stale: a folder of the name appears after the review read the listing.
    const before = listed(env);
    fs.cpSync(path.join(env.projectsDir, 'sample_burrito'), path.join(env.projectsDir, 'tita_tc3_clash'), { recursive: true });
    await run.click();
    const failed = page.getByTestId('import-failed');
    await failed.waitFor({ timeout: 60_000 });
    assert.equal(await failed.getAttribute('data-code'), 'import.name-exists');
    assert.ok((await failed.textContent()).includes(message));
    assert.deepEqual(listed(env), [...before, 'tita_tc3_clash'].sort(), 'nothing else written');
    fs.rmSync(path.join(env.projectsDir, 'tita_tc3_clash'), { recursive: true, force: true });
  });
});
