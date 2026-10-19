// Project settings from Home and Get source texts (B9a), through the app on the store with the
// fake Door43: the settings-contract's read part on the sample (the checking language from the
// pins, by its own name, #579), and Get source texts' second step behind the internet question
// (tC4's D95: Not now reads nothing and offers to read the list; reading it asks Door43's
// catalog for the organization). The fake's parity account publishes no resources, so the
// package is empty here; its rows and the download are tested in tc-app against the seeded fake
// (crates/tc-app/tests/app/sources.rs).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/source-texts.test.mjs
import assert from 'node:assert/strict';
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
    const none = en['sources.noneForBook'].replace('{book}', 'Titus').replace('{lang}', 'English');
    await page.getByText(none).waitFor();
    const asked = (await apiRequests(env.door43)).map((r) => `${r.path}?${r.query ?? ''}`);
    assert.deepEqual(asked, ['/api/v1/catalog/search?owner=unfoldingWord']);
    assert.equal(await page.getByTestId('sources-download').count(), 0, 'nothing to download');
  }, { door43: true });
});
