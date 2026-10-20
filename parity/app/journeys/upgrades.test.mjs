// The change of checking language (J13) and the release upgrade offers (J12) through the app on
// the store with the fake Door43 (unit C8): the plan is made from the project, shown, and
// confirmed by its id; the pins move only at the accept; Keep changes nothing. The upgrade of a
// set itself, with its download, is tested in tc-app against the seeded fake
// (crates/tc-app/tests/app/upgrades.rs): the parity account's fake publishes no resources.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/upgrades.test.mjs
import assert from 'node:assert/strict';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const settings = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: en['home.settings'], exact: true });
const english = (page) => page.getByTestId('settings-gateway-en::unfoldingWord');

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

browserTest('Change writes the new checking language, and Settings names it by its own name afterwards', async () => {
  await withApp(async ({ page }) => {
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
  });
});

browserTest('Check for updates asks the internet question first, and Not now sends nothing to Door43', async () => {
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
    // Asking again and turning the internet on reads what Door43 has: this account publishes
    // nothing for the sample's resources, so each set is as current as Door43 can say.
    await page.getByTestId('check-updates').click();
    await ask.waitFor();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('upgrade-offer-primary').waitFor();
    assert.equal(await page.getByTestId('upgrade-offer-primary').getAttribute('data-upgrades'), '0');
    assert.equal(await page.getByTestId('upgrade-current-primary').count(), 1);
    assert.equal(await page.getByTestId('upgrade-texts-current').count(), 1);
  }, { door43: true });
});
