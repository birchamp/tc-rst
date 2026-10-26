// The shell (tC4's App.jsx, unit A5): the frame and the top bar on Home, the save indicator
// (X2) over the no-loss probe's saves, the view-error callout after a panic with the top bar
// kept, a language switch that keeps typed text, and a boot that adds no request.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     node --test parity/app/journeys/shell.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../env-rust.mjs';
import {
  browserTest, catalog, choose, recordRequests, sleep, snapshot, subtreeOf, tc4Dom, until, withApp,
} from './kit.mjs';

/** The frame's line (the first element under #root) with its style spelled out. */
const frameLine = (dom) => subtreeOf({ ...dom, root: dom.root.slice(1, 2) }, '<div')[0];

browserTest('top_bar_on_home_is_tc4s: the frame and the top bar are tC4\'s DOM', async () => {
  await withApp(async ({ page }) => {
    const rust = await snapshot(page);
    const tc4 = tc4Dom('home');
    assert.equal(frameLine(rust), frameLine(tc4));
    assert.deepEqual(subtreeOf(rust, '<header'), subtreeOf(tc4, '<header'));
  });
});

const indicator = (page) => page.locator('[data-testid="probe-x2"] [data-testid="save-indicator"]');
const state = (page) => indicator(page).getAttribute('data-state');

async function typeInto(page, text) {
  const field = page.getByTestId('probe-field-a');
  await field.click();
  await page.keyboard.type(text);
}

browserTest('save_indicator_saved_is_tc4s: X2 on a stored text is the DOM tC4 draws', async () => {
  await withApp(async ({ page }) => {
    await typeInto(page, 'Saved text');
    await page.locator('body').click({ position: { x: 600, y: 700 } });
    await until('saved', async () => (await state(page)) === 'saved');
    await page.waitForFunction(() => window.__tcIdle === true);
    const rust = subtreeOf(await snapshot(page), 'data-testid="save-indicator"');
    const tc4 = subtreeOf(tc4Dom('understand-tit1'), 'data-testid="save-indicator"');
    assert.deepEqual(rust, tc4);
  }, { probe: true });
});

browserTest('save_indicator_states: unsaved, saving, a failed save with its Retry, and Retry', async () => {
  await withApp(async ({ page, env, allow }) => {
    const en = catalog('en');
    await typeInto(page, 'T1');
    assert.equal(await state(page), 'dirty');
    assert.match(await indicator(page).textContent(), new RegExp(en['app.unsaved']));
    // A held save: Saving.
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await page.locator('body').click({ position: { x: 600, y: 700 } });
    await until('saving', async () => (await state(page)) === 'saving');
    assert.match(await indicator(page).textContent(), new RegExp(en['app.saving']));
    await env.call('test/release', { request: newRequestId(), at: 'save' });
    await until('saved', async () => (await state(page)) === 'saved');
    // A failed save: a non-empty folder in place of the test fields' file (A4b's P7).
    const file = path.join(env.appData, 'test-hooks', 'fields.json');
    const original = fs.readFileSync(file);
    fs.rmSync(file);
    fs.mkdirSync(file);
    fs.writeFileSync(path.join(file, 'keep'), 'x');
    allow(/Failed to load resource|tc4:/);
    await typeInto(page, ' T2');
    await page.locator('body').click({ position: { x: 600, y: 700 } });
    await until('error', async () => (await state(page)) === 'error');
    assert.match(await indicator(page).textContent(), new RegExp(en['app.saveError']));
    const retry = indicator(page).locator('button');
    assert.equal(await retry.count(), 1, 'a Retry');
    assert.equal(await retry.getAttribute('data-testid'), null, 'a test field is neither a story nor a note');
    assert.equal(await retry.getAttribute('data-i'), 'quiet');
    const colour = await indicator(page).evaluate((el) => el.style.color);
    assert.equal(colour, 'var(--tc-invalid-on-dark)');
    fs.rmSync(file, { recursive: true });
    fs.writeFileSync(file, original);
    await retry.click();
    await until('saved after Retry', async () => (await state(page)) === 'saved');
    assert.equal(await indicator(page).locator('button').count(), 0);
  }, { probe: true });
});

browserTest('a_panic_reload_keeps_the_top_bar_and_back_to_home_gates', async () => {
  await withApp(async ({ page, env, allow }) => {
    allow(/panic|RuntimeError|unreachable|tc4 test/);
    await page.evaluate(() => sessionStorage.setItem('tc4.test.panicAtBoot', '1'));
    await page.goto('about:blank');
    await page.goto(`${env.origin}/#t=${env.token}`);
    // boot.js reloads once; the App shows the callout in place of the screen, with the top bar.
    await page.getByTestId('view-error').waitFor({ timeout: 20_000 });
    assert.equal(await page.getByTestId('account-menu').count(), 1, 'the top bar stays');
    assert.equal(await page.getByTestId('add-project').count(), 0, 'the screen is not drawn');
    await page.getByTestId('view-error-home').click();
    await page.getByTestId('add-project').waitFor();
    assert.equal(await page.getByTestId('view-error').count(), 0);
    assert.equal(await page.evaluate(() => sessionStorage.getItem('tc4.crash.v1')), null, 'the crash marker is gone');
  }, { probe: true });
});

browserTest('the_language_switch_keeps_typed_text: a preview, Cancel and Apply leave a held field as it is', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await typeInto(page, 'Texto escrito');
    await page.locator('body').click({ position: { x: 600, y: 700 } });
    await until('saving', async () => (await state(page)) === 'saving');
    const item = () => page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith('tc4.drafts.v2:')).map((k) => localStorage.getItem(k)));
    const before = await item();
    assert.equal(before.length, 1);
    const mark = requests.mark();
    await page.getByTestId('probe-language').click();
    await page.getByTestId('language-dialog').waitFor();
    await choose(page, 'ar');
    await page.getByTestId('language-cancel').click();
    await page.getByTestId('probe-language').click();
    await choose(page, 'fr');
    await page.getByTestId('language-apply').click();
    await until('the dialog closes', async () => (await page.getByTestId('language-dialog').count()) === 0);
    assert.equal(await page.evaluate(() => document.documentElement.lang), 'fr');
    assert.equal(await page.getByTestId('probe-field-a').inputValue(), 'Texto escrito');
    assert.deepEqual(await item(), before, 'the buffer item is unchanged');
    assert.deepEqual(requests.api(mark).filter((p) => p !== '/api/v1/prefs/setLocale'), [], 'the switch sends no save');
    await env.call('test/release', { request: newRequestId(), at: 'save' });
    await until('saved', async () => (await state(page)) === 'saved');
    const log = await env.call('test/fieldLog', { field: 'probe-a' });
    assert.deepEqual(log.stores.map((s) => s.text), ['Texto escrito'], 'one store, with the typed text');
  }, { probe: true });
});

browserTest('boot_adds_no_request: one hello, the nine catalogs, the stream and Home\'s reads', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await page.goto('about:blank');
    await page.goto(`${env.origin}/#t=${env.token}`);
    await page.getByTestId('account-menu').waitFor();
    await page.waitForFunction(() => window.__tcIdle === true);
    await sleep(500);
    const paths = requests.all.map((r) => new URL(r.url).pathname);
    // Besides hello and the stream, only Home's reads: the list, and each card's own (once at
    // its mount, once more when its summary is ready; B2 D1).
    const api = paths.filter((p) => p.startsWith('/api/'));
    const count = (p) => api.filter((a) => a === p).length;
    assert.deepEqual([count('/api/v1/app/hello'), count('/api/v1/events'), count('/api/v1/home/list')], [1, 1, 1], api.join(' '));
    assert.ok(count('/api/v1/home/card') <= 2, api.join(' '));
    // Each card's last sync, for its "replaced" link (HIST-1, ruling H5): one read per card.
    assert.ok(count('/api/v1/door43/lastSync') <= count('/api/v1/home/card'), api.join(' '));
    assert.deepEqual(api.filter((p) => !['/api/v1/app/hello', '/api/v1/events', '/api/v1/home/list', '/api/v1/home/card', '/api/v1/door43/lastSync'].includes(p)), []);
    assert.equal(paths.filter((p) => /\/i18n\/[^/]+\.json$/.test(p)).length, 9);
    assert.equal(paths.filter((p) => p.startsWith('/help/')).length, 0, 'help loads only when it opens');
  });
});

// tC4 #599: the mode switch stays centred in the top bar while the save indicator beside it
// changes its words (unsaved, saving, saved), within 1 px, in English and in Arabic.
for (const id of ['en', 'ar']) {
  browserTest(`switcher_stays_centred_while_the_save_indicator_changes (${id})`, async () => {
    await withApp(async ({ page }) => {
      const words = catalog(id);
      const centre = () => page.getByTestId('mode-switch').evaluate((el) => {
        const r = el.getBoundingClientRect();
        return r.left + r.width / 2 - document.documentElement.clientWidth / 2;
      });
      await page.getByTestId('project-sample_burrito').getByRole('button', { name: /Titus|تيطس/ }).first().click();
      await page.getByTestId('mode-switch').getByRole('tab', { name: words['nav.draft'] }).click();
      await page.getByTestId('draft-target-text').first().waitFor();
      const at = [await centre()];
      await page.getByRole('tab', { name: words['understand.byVerse'], exact: true }).click();
      await page.getByTitle(words['draft.editVerse']).first().click();
      const box = page.getByLabel(words['draft.verseLabel'].replace('{n}', '1'), { exact: true });
      await box.waitFor();
      await box.fill(`${await box.inputValue()} más`);
      at.push(await centre());
      await page.getByRole('button', { name: words['draft.saveVerse'] }).click();
      await page.locator('[data-testid="save-indicator"][data-state="saved"]').waitFor({ timeout: 60_000 });
      at.push(await centre());
      const spread = Math.max(...at) - Math.min(...at);
      assert.ok(spread <= 1, `the switch moved ${spread}px as the indicator changed: ${at.join(', ')}`);
    }, { locale: id });
  });
}
