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
const projectFile = (env, name, rel) => JSON.parse(fs.readFileSync(path.join(env.projectsDir, abbrOf(name), rel), 'utf8'));
const pinsOf = (env, name) => projectFile(env, name, path.join('ingredients', 'checking', 'resources.json'));
const decisionsOf = (env, name, tool, book) => projectFile(env, name, path.join('ingredients', 'checking', tool, `${book}.json`));
const statement = (env, name) => projectFile(env, name, 'metadata.json').copyright;
/** The commit the fake Door43 gives `repo`'s `tag`. */
async function tagSha(env, repo, tag) {
  const response = await fetch(`${env.door43.url}/api/v1/repos/${repo}/tags/${tag}`);
  assert.equal(response.status, 200, `${repo} ${tag}`);
  return (await response.json()).commit.sha;
}
/** tC4's `pickOption`: the combobox, then the option by its name. */
async function pickOption(page, combobox, option) {
  await combobox.click();
  await page.getByRole('option', { name: option, exact: true }).click();
}

/** From Home to the review step of `files` (paths or payloads), with `name` typed. */
async function review(page, files, { name } = {}) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-import').click();
  // tC4's KindStep: a kind with no importer is drawn disabled, at half opacity.
  for (const kind of ['usfm', 'burrito']) {
    const card = page.getByTestId(`import-kind-${kind}`);
    assert.ok(await card.isDisabled(), `${kind} waits for its importer`);
    assert.match(await card.innerText(), /Arrives with its importer\./);
    assert.equal(await card.evaluate((e) => getComputedStyle(e).opacity), '0.5');
  }
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
    for (const part of ['Verses46', 'Alignments0 verses', 'Checking decisions5', 'Contributors0']) assert.ok(carried.includes(part), `${part} in ${carried}`);
    assert.match(await page.getByTestId('import-book-TIT').textContent(), /3 chapters · 46 verses/);
    assert.ok(await page.getByText('CC BY-SA 4.0', { exact: true }).isVisible());
    // Offline: the versions are not looked up, so Import waits for a choice.
    assert.equal(await state(page), 'offline');
    assert.ok(await page.getByTestId('import-allow-internet').isVisible());
    assert.ok(await page.getByTestId('import-run').isDisabled());
    await page.getByTestId('import-use-installed').click();
    assert.equal(await state(page), 'installed');
    await until('the carry-over is counted', async () => /\d+ decisions carry over\. \d+ decisions must be checked again\./.test(await resources(page).textContent()));
    await imported(page, name);
    const folder = path.join(env.projectsDir, abbrOf(name));
    const meta = JSON.parse(fs.readFileSync(path.join(folder, 'metadata.json'), 'utf8'));
    assert.equal(meta.languages[0].tag, 'cfm');
    assert.deepEqual(meta.copyright, { shortStatements: [{ statement: 'CC BY-SA 4.0' }] });
    assert.equal(fs.readFileSync(path.join(folder, 'ingredients', 'TIT.usfm'), 'utf8').match(/^\\c \d+/gm).length, 3);
    // D95: nothing asked the internet.
    assert.equal(await page.getByTestId('net-ask').count(), 0);
  });
});

browserTest('tC3 multi-zip: three books of one language are one project; the license is chosen on the review page', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'KJV tC3';
    await review(page, MULTI, { name });
    for (const code of ['JHN', 'JOB', 'LUK']) await page.getByTestId(`import-book-${code}`).waitFor();
    assert.equal(await page.getByTestId('import-damaged').count(), 0);
    // The files disagree on the license: the check says so.
    assert.equal(await page.getByTestId('import-check-license').getAttribute('data-status'), 'warn');
    // tC4 tc3.ts: the licenses are joined with " · ".
    assert.match(await page.getByTestId('import-check-license').textContent(), /\S · \S/);
    await page.getByTestId('import-use-installed').click();
    assert.equal(await state(page), 'installed');
    // the files disagree (CC BY-SA 4.0, CC0 1.0): Import waits for the choice
    await until('the carry-over is counted', async () => /decisions? must be checked again/.test(await resources(page).textContent()));
    assert.ok(await page.getByTestId('import-run').isDisabled());
    await pickOption(page, page.getByTestId('import-license'), 'CC0 1.0 Public Domain');
    await imported(page, name);
    assert.deepEqual(statement(env, name), { shortStatements: [{ statement: 'CC0 1.0 Public Domain' }] });
    for (const code of ['JHN', 'JOB', 'LUK']) assert.ok(fs.existsSync(path.join(env.projectsDir, abbrOf(name), 'ingredients', `${code}.usfm`)), code);
  });
});

browserTest('tC3 online: each version Door43 has is a full pin; the project opens and the guided fix lists the en_tn v87 this computer lacks', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'Tita tC3 online';
    const v87 = await tagSha(env, 'unfoldingWord/en_tn', 'v87');
    await env.door43.clear();
    await review(page, [TIT], { name });
    assert.equal(await state(page), 'offline');
    assert.deepEqual((await env.door43.requests()).filter((r) => r.path.includes('/tags')), [], 'nothing looked up before the click');
    // the review page finds the versions; Import needs no choice
    await lookUp(page);
    assert.ok((await env.door43.requests()).some((r) => r.path.includes('/tags')), 'the lookup asked Door43 for the tags');
    assert.equal(await state(page), 'found');
    assert.ok((await resources(page).textContent()).includes('unfoldingWord/en_tn v87'));
    await imported(page, name);
    // on disk: translationNotes is pinned to en_tn v87 by its sha, and the decisions record it
    const notes = pinsOf(env, name).languageSets.primary.translationNotes;
    assert.deepEqual([notes.version, notes.sha, notes.books], ['v87', v87, ['TIT']]);
    assert.equal(decisionsOf(env, name, 'translationNotes', 'TIT').resource.sha, v87);
    // Check offers the guided fix for the pinned en_tn v87 (D88: a missing pin is always a fetch)
    await page.getByTestId(`project-${abbrOf(name)}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
    const card = page.getByTestId('preflight-translationNotes');
    await until('the notes card offers a fetch', async () => (await card.getAttribute('data-state')) === 'fetch', 60_000);
    await card.getByTestId('fix-translationNotes').click();
    await page.getByTestId('guided-fix').waitFor();
    assert.ok((await page.getByTestId('fix-pin').textContent()).includes(v87));
  }, { door43: true });
});

browserTest('tC3 online, mixed: the found pins stay named when translationWords moves to the installed versions, and they are stored', async () => {
  await withApp(async ({ page, env }) => {
    const name = 'KJV tC3 mixed';
    const v87 = await tagSha(env, 'unfoldingWord/en_tn', 'v87');
    await review(page, MULTI, { name });
    await lookUp(page);
    assert.equal(await state(page), 'missing');
    // LUK names en_tn v87 and ugnt v0.34 (found); its translationWords decisions name no version
    const before = await resources(page).textContent();
    for (const found of ['unfoldingWord/en_tn v87', 'unfoldingWord/el-x-koine_ugnt v0.34']) assert.ok(before.includes(found), `${found} in ${before}`);
    await page.getByTestId('import-use-installed').click();
    assert.equal(await state(page), 'installed');
    const after = await resources(page).textContent();
    assert.ok(after.includes('unfoldingWord/en_tn v87'), after);
    assert.ok(after.includes('The versions on this computer will be used for translationWords.'), after);
    await until('the carry-over is counted', async () => /decisions? (?:must be checked again|carr(?:y|ies) over)/.test(await resources(page).textContent()));
    await pickOption(page, page.getByTestId('import-license'), 'CC BY-SA 4.0');
    await imported(page, name);
    const notes = pinsOf(env, name).languageSets.primary.translationNotes;
    assert.deepEqual([notes.version, notes.sha, notes.books], ['v87', v87, ['LUK']]);
    assert.equal(decisionsOf(env, name, 'translationNotes', 'LUK').resource.sha, v87);
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
    // tC4's toBeEnabled waits; here Import also waits for the carry-over count.
    await until('Import is on', () => run.isEnabled());
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
