// Project settings from Home and Get source texts (B9a), through the app on the store with the
// fake Door43: the settings-contract's read part on the sample (the checking language from the
// pins, by its own name, #579), and Get source texts' second step behind the internet question
// (tC4's D95: Not now reads nothing and offers to read the list; reading it asks Door43's
// catalog for the organization). The fake publishes unfoldingWord's resource releases (J3):
// the English package for Titus is downloaded through the screen and its source text lands on
// disk at the release Door43 publishes.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/source-texts.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const settings = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: en['home.settings'], exact: true });
const apiRequests = async (door43) => (await door43.requests()).filter((r) => r.path.startsWith('/api/'));

browserTest('Settings from Home show the project as its files hold it, and Save waits for them', async () => {
  await withApp(async ({ page }) => {
    await settings(page).click();
    const current = page.getByTestId('settings-gateway-current');
    await current.waitFor();
    assert.equal(
      await current.textContent(),
      en['settings.gatewayCurrent'].replace('{lang}', 'Español (Latinoamérica)').replace('{org}', 'es-419_gl'),
    );
    assert.equal(await page.getByTestId('settings-gateway-en::unfoldingWord').getAttribute('data-current'), '0');
    await until('Save is enabled once the values came', () => page.getByRole('button', { name: en['settings.save'] }).isEnabled());
    assert.equal(await page.getByRole('button', { name: en['wizard.ltr'] }).getAttribute('data-selected'), 'true');
  });
});

/** The installs of `owner/repo` in the run's app data: each receipt's pin. */
const installs = (env, owner, repo) => {
  const dir = path.join(env.appData, 'resources', 'dcs', owner.toLowerCase(), repo);
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir).map((sha) => JSON.parse(fs.readFileSync(path.join(dir, sha, 'receipt.json'), 'utf8')).pin);
};

/** The project's pin of unfoldingWord's `repo` among its scripture texts, on disk now. */
const textPin = (env, repo) => JSON.parse(fs.readFileSync(
  path.join(env.projectsDir, BIBLE, 'ingredients', 'checking', 'resources.json'), 'utf8'))
  .extraScripture.find((p) => p.repoPath.endsWith(`unfoldingWord/${repo}`));

// J3 (P1): Get source texts reads a package only after the internet question (D95), then
// downloads it, and the version the project pins lands on disk. The sample pins Door43's own
// commits, which the fake cannot serve (the download rightly refuses another commit under a
// pinned version); so the text's pin is first moved to the fake's release through the upgrade
// screen, as a translator's upgrade would, and its install removed: this computer lacks it.
browserTest('j3_a_download_from_the_seeded_fake_installs_the_pinned_version', async () => {
  await withApp(async ({ page, env }) => {
    await settings(page).click();
    await page.getByTestId('settings-manage-sources').click();
    await page.getByTestId('sources-modal').waitFor();
    await page.getByTestId('understand').waitFor();
    await page.getByTestId('check-updates').click();
    await page.getByTestId('net-ask').waitFor();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('upgrade-text-apply-en_ult').click();
    const confirm = page.getByTestId('upgrade-confirm');
    await confirm.waitFor();
    await confirm.getByTestId('upgrade-apply').click();
    await confirm.waitFor({ state: 'detached', timeout: 60_000 });
    const ult = (await (await fetch(`${env.door43.url}/api/v1/catalog/search?owner=unfoldingWord`)).json())
      .data.find((entry) => entry.name === 'en_ult');
    await until('the text is pinned at the fake\'s release', () => textPin(env, 'en_ult').sha === ult.commit_sha, 30_000);
    const pinned = textPin(env, 'en_ult');
    fs.rmSync(path.join(env.appData, 'resources', 'dcs', 'unfoldingword', 'en_ult'), { recursive: true, force: true });
    assert.deepEqual(installs(env, 'unfoldingWord', 'en_ult'), []);
    await env.door43.clear();
    await page.getByTestId('sources-modal').getByText('English', { exact: true }).first().click();
    const download = page.getByTestId('sources-download');
    await download.waitFor();
    await download.click();
    await until('the source text is installed', () => installs(env, 'unfoldingWord', 'en_ult').length > 0, 60_000);
    assert.equal(await page.getByTestId('sources-error').count(), 0, 'the download says no failure');
    assert.ok((await env.door43.requests()).some((r) => r.path.startsWith('/unfoldingWord/en_ult/')), 'the text came from the fake');
    const [installed] = installs(env, 'unfoldingWord', 'en_ult');
    assert.equal(installed.sha, pinned.sha, 'the install on disk is the pinned commit');
    assert.equal(installed.version, pinned.version, 'at the pinned version');
  }, { door43: true });
});

browserTest('Get source texts reads a package only after the internet question, and Not now reads nothing', async () => {
  await withApp(async ({ page, env }) => {
    await settings(page).click();
    await page.getByTestId('settings-manage-sources').click();
    await page.getByTestId('sources-modal').waitFor();
    await page.getByTestId('understand').waitFor();
    await env.door43.clear();
    await page.getByTestId('sources-modal').getByText('English', { exact: true }).first().click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'sources');
    await page.getByTestId('net-cancel').click();
    const read = page.getByTestId('sources-allow-internet');
    await read.waitFor();
    assert.equal(await read.textContent(), en['sources.goOnline']);
    await sleep(300);
    assert.deepEqual(await apiRequests(env.door43), [], 'Not now sends nothing to Door43');
    await read.click();
    await ask.waitFor();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('sources-download').waitFor();
    const asked = (await apiRequests(env.door43)).map((r) => `${r.path}?${r.query ?? ''}`);
    assert.deepEqual(asked, ['/api/v1/catalog/search?owner=unfoldingWord']);
  }, { door43: true });
});
