// The change of checking language (J13) and the release upgrade offers (J12) through the app on
// the store with the fake Door43 (unit C8): the plan is made from the project, shown, and
// confirmed by its id; the pins move only at the accept; Keep changes nothing. J12 upgrades a
// scripture text through the screens from the fake's unfoldingWord releases and reads the moved
// pin from the project's pin file; J13 reads the primary set from it after Change.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/upgrades.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const settings = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: en['home.settings'], exact: true });
const english = (page) => page.getByTestId('settings-gateway-en::unfoldingWord');

/** The project's pin file as it is on disk now (INV-PINS-ON-DISK). */
const pinFile = (env) => JSON.parse(fs.readFileSync(
  path.join(env.projectsDir, BIBLE, 'ingredients', 'checking', 'resources.json'), 'utf8'));

/** The release the fake's catalog publishes for unfoldingWord's `repo`. */
const published = async (env, repo) => (await (await fetch(`${env.door43.url}/api/v1/catalog/search?owner=unfoldingWord`)).json())
  .data.find((entry) => entry.name === repo);

browserTest('Keep leaves the checking language as it was; Change moves the primary set to English', async () => {
  await withApp(async ({ page }) => {
    await settings(page).click();
    await english(page).click();
    const dialog = page.getByTestId('gateway-change');
    await dialog.waitFor();
    assert.equal(await page.getByTestId('understand').count(), 1, 'the project opened behind it');
    // J13: the change is read before anything is written, by book and tool, in tC4's words.
    assert.equal(
      await page.getByTestId('gateway-headline').textContent(),
      '4 decisions in Titus were made against the notes you are leaving.',
    );
    const rows = await page.getByTestId('gateway-plan').locator('li').allTextContents();
    assert.deepEqual(rows, [
      'Titus · Translation Words: 2 carried over, 0 to check again',
      'Titus · Translation Notes: 0 carried over, 1 to check again',
    ]);
    const keep = page.getByTestId('gateway-cancel');
    assert.equal(await keep.textContent(), en['gateway.keep'].replace('{lang}', 'Español (Latinoamérica)'));
    assert.equal(
      await page.getByTestId('gateway-confirm').textContent(),
      en['gateway.change'].replace('{lang}', 'English'),
    );
    // Keep: nothing changed.
    await keep.click();
    await dialog.waitFor({ state: 'detached' });
  });
});

browserTest('j13_change_writes_the_primary_set_to_disk', async () => {
  await withApp(async ({ page, env }) => {
    assert.deepEqual(pinFile(env).languageSets.primary.gatewayLanguage, { languageId: 'es-419', owner: 'es-419_gl' });
    await settings(page).click();
    await english(page).click();
    await page.getByTestId('gateway-change').waitFor();
    await page.getByTestId('gateway-confirm').click();
    await page.getByTestId('gateway-change').waitFor({ state: 'detached' });
    // The page afresh is Home (the server's state stays): the checking language is English
    // now, read from the pins.
    await page.getByTestId('project-switch').click();
    await until('Home shows the project card', () => page.getByTestId(`project-${BIBLE}`).count());
    await settings(page).click();
    const current = page.getByTestId('settings-gateway-current');
    await current.waitFor();
    await until('the card names English', async () => (await current.textContent()).includes('English'));
    assert.equal(await english(page).getAttribute('data-current'), '1');
    // The pin file on disk says the same: the primary set is English now.
    await until('the pin file names English', () => pinFile(env).languageSets.primary.gatewayLanguage.languageId === 'en', 30_000);
    assert.deepEqual(pinFile(env).languageSets.primary.gatewayLanguage, { languageId: 'en', owner: 'unfoldingWord' });
  }, { door43: true });
});

// J12 (P1). The sample pins Door43's own commits, which the fake cannot serve; the fake's
// releases have other commits, so it offers them, as tC4 does (D58: the commit is a release's
// identity, its label only display), even where the label reads lower.
browserTest('j12_an_upgrade_through_the_screens_moves_the_pin_on_disk', async () => {
  await withApp(async ({ page, env }) => {
    await settings(page).click();
    await page.getByTestId('settings-manage-sources').click();
    await page.getByTestId('sources-modal').waitFor();
    await env.door43.clear();
    await page.getByTestId('check-updates').click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'sources');
    await page.getByTestId('net-cancel').click();
    await sleep(300);
    const sent = (await env.door43.requests()).filter((r) => r.path.startsWith('/api/'));
    assert.deepEqual(sent, [], 'Not now sends nothing');
    assert.equal(await page.getByTestId('upgrade-offer-primary').count(), 0, 'nothing is offered');
    // Asking again and turning the internet on reads what Door43 has: the primary set is
    // es-419_gl's, which the fake does not publish, so it is as current as Door43 can say; the
    // scripture text is offered at the fake's release.
    await page.getByTestId('check-updates').click();
    await ask.waitFor();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('upgrade-offer-primary').waitFor();
    assert.equal(await page.getByTestId('upgrade-offer-primary').getAttribute('data-upgrades'), '0');
    assert.equal(await page.getByTestId('upgrade-current-primary').count(), 1);
    const ult = await published(env, 'en_ult');
    const before = pinFile(env).extraScripture.find((p) => p.repoPath.endsWith('unfoldingWord/en_ult'));
    assert.notEqual(before.sha, ult.commit_sha);
    await page.getByTestId('upgrade-text-apply-en_ult').click();
    const confirm = page.getByTestId('upgrade-confirm');
    await confirm.waitFor();
    await confirm.getByTestId('upgrade-apply').click();
    await confirm.waitFor({ state: 'detached', timeout: 60_000 });
    // The pin file on disk names the release the screen offered.
    const moved = () => pinFile(env).extraScripture.find((p) => p.repoPath.endsWith('unfoldingWord/en_ult'));
    await until('the pin moved on disk', () => moved().sha === ult.commit_sha, 30_000);
    assert.equal(moved().version, ult.branch_or_tag_name);
  }, { door43: true });
});
