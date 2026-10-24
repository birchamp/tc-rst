// Home (B2): when a card reads its progress again, a failed listing and its Retry, the Share
// button's question, the dialogs Home opens, a folder that cannot be opened, and a language
// preview that redraws no node of Home.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/home.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, choose, focused, openLanguage, openMenu, recordRequests, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const card = (page) => page.getByTestId(`project-${BIBLE}`);
const cardReads = (requests, mark) => requests.api(mark).filter((p) => p === '/api/v1/home/card').length;
const quiet = (page) => page.waitForFunction(() => window.__tcIdle === true);

// tC4's homeProjectCard case 4: a re-render does not read again; a new mount does.
browserTest('a_card_reads_again_on_a_new_home_visit_not_on_a_redraw', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await card(page).waitFor();
    await until('the tiles are known', async () => (await page.getByTestId('book-tile-TIT').textContent()).includes('11%'));
    // The Home summary job may still be running; its card event makes the page read the card,
    // which belongs before the count, not in it. Wait for the server, then the page.
    await until('the server is quiet', async () => !(await env.call('test/quiet', {})).busy);
    await quiet(page);
    // A language preview redraws Home's texts: no read.
    let mark = requests.mark();
    await openLanguage(page);
    await choose(page, 'ar');
    await page.getByTestId('language-cancel').click();
    await quiet(page);
    await sleep(300);
    assert.equal(cardReads(requests, mark), 0, 'a preview reads no card');
    // Help covers Home and keeps it mounted: no read.
    mark = requests.mark();
    await openMenu(page);
    await page.getByTestId('account-help').click();
    await page.getByTestId('help-panel').waitFor();
    await page.getByTestId('help-close').click();
    await quiet(page);
    await sleep(300);
    assert.equal(cardReads(requests, mark), 0, 'Help and back reads no card');
    // A new Home visit (a reload) reads the card again.
    mark = requests.mark();
    await env.load(page, { locale: 'en' });
    await card(page).waitFor();
    await quiet(page);
    assert.ok(cardReads(requests, mark) >= 1, 'a new visit reads the card');
  });
});

browserTest('retry_after_a_failed_list_reads_it_again', async () => {
  await withApp(async ({ page, env }) => {
    const failing = (route) => route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ type: 'err', value: { type: 'internal', value: { report: 0 } } }) });
    await page.route('**/api/v1/home/list', failing, { times: 1 });
    await env.load(page, { locale: 'en' });
    await page.getByTestId('projects-retry').waitFor();
    assert.equal(await page.getByText('No projects yet', { exact: false }).count(), 0, 'a failed listing never invites a new project');
    await page.getByTestId('projects-retry').click();
    await card(page).waitFor();
    assert.equal(await page.getByTestId('home-open-error').count(), 0);
  });
});

browserTest('the_share_button_asks_first_and_not_now_sends_nothing', async () => {
  await withApp(async ({ page }) => {
    const requests = recordRequests(page);
    await card(page).waitFor();
    await quiet(page);
    const before = await card(page).innerHTML();
    const mark = requests.mark();
    await page.getByTestId(`share-${BIBLE}`).click();
    await page.locator('[data-testid="net-ask"][data-kind="share"]').waitFor();
    await page.getByTestId('net-cancel').click();
    await until('the question closes', async () => (await page.getByTestId('net-ask').count()) === 0);
    await sleep(300);
    assert.deepEqual(requests.api(mark).filter((p) => !p.startsWith('/api/v1/home/')), [], 'nothing is sent');
    assert.equal(await card(page).innerHTML(), before, 'the card is unchanged');
  });
});

browserTest('add_a_book_opens_the_method_step_for_that_project', async () => {
  await withApp(async ({ page }) => {
    await card(page).waitFor();
    const tile = card(page).getByRole('button', { name: /Add a book/ });
    await tile.click();
    const dialog = page.getByRole('dialog', { name: 'Add a book' });
    await dialog.waitFor();
    assert.match(await dialog.textContent(), /Equipo Ejemplo — Tito y Jonás/);
    assert.equal(await page.getByTestId('ab-usfm-option').count(), 1);
    await page.keyboard.press('Escape');
    await until('the dialog closes', async () => (await dialog.count()) === 0);
  });
});

browserTest('add_project_dialogs_swap_in_place_and_close', async () => {
  await withApp(async ({ page }) => {
    const chooser = page.getByTestId('add-project-modal');
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-bible').click();
    await page.locator('#nb-name').waitFor();
    assert.equal(await chooser.count(), 0, 'New Bible takes the chooser\'s place');
    await page.getByRole('button', { name: 'Close' }).click();
    await until('New Bible closes', async () => (await page.locator('#nb-name').count()) === 0);
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-obs').click();
    await page.locator('#no-name').waitFor();
    await page.keyboard.press('Escape');
    await until('New Open Bible Stories closes', async () => (await page.locator('#no-name').count()) === 0);
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-import').click();
    await page.getByTestId('import-kind').waitFor();
    await page.getByTestId('import-kind-back').click();
    await chooser.waitFor();
    assert.equal(await page.getByTestId('import-modal').count(), 0, 'Back returns to the chooser');
  });
});

browserTest('an_unusable_folder_shows_its_name_and_reason_and_opens_nothing', async () => {
  const seed = (projects) => {
    fs.mkdirSync(path.join(projects, 'broken', 'ingredients'), { recursive: true });
    fs.writeFileSync(path.join(projects, 'broken', 'metadata.json'), '{}');
  };
  await withApp(async ({ page }) => {
    const unusable = page.getByTestId('unusable-broken');
    await unusable.waitFor();
    const text = await unusable.textContent();
    assert.match(text, /broken/);
    assert.match(text, /Cannot be opened/);
    assert.match(text, /no ingredients\/tc\/project\.json/);
    assert.equal(await unusable.getByRole('button').count(), 0, 'it offers nothing to press');
    assert.equal(await page.getByText('No projects yet', { exact: false }).count(), 0, 'no invitation to make a project in its place');
  }, { seed, invalidOnPurpose: "the journey seeds a folder whose metadata.json is not a Burrito's, to show it cannot be opened" });
});

browserTest('a_language_preview_redraws_no_home_node', async () => {
  await withApp(async ({ page }) => {
    await card(page).waitFor();
    await quiet(page);
    const nodes = [
      await page.$('main h1'),
      await page.$('[data-testid="book-tile-TIT"]'),
      await page.$('[data-testid="project-summary"]'),
      await page.$(`[data-testid="share-${BIBLE}"]`),
    ];
    const title = await page.textContent('main h1');
    await openLanguage(page);
    await choose(page, 'ar');
    await until('Home is in Arabic', async () => (await page.textContent('main h1')) !== title);
    for (const node of nodes) assert.ok(await node.evaluate((n) => n.isConnected), 'the node stays');
    await choose(page, 'en');
    await until('Home is in English', async () => (await page.textContent('main h1')) === title);
    for (const node of nodes) assert.ok(await node.evaluate((n) => n.isConnected), 'the node stays');
    await page.getByTestId('language-cancel').click();
    assert.ok(typeof (await focused(page)) === 'string');
  });
});
