// Open and Understand (B3): J15's overlay on a held open, the open from a tile and its refusal,
// the close at Home, the book rail's tooltip, a unit's focus and its lit words, the helps
// panel's divider and the article pane.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/understand.test.mjs
import assert from 'node:assert/strict';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, recordRequests, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const overlay = (page) => page.getByTestId('open-progress');

/** Opens Titus from its tile and waits for Understand with its passage. */
async function openTitus(page) {
  await titus(page).click();
  await page.getByTestId('understand').waitFor();
  await page.getByTestId('understand-unit-s1').waitFor();
}

// J15 (UI half): the overlay shows 300 ms after the click by its own timer. The open is held
// before the server has it (its request waits in the page), so no job exists and no event can
// be what shows it; released, the open goes on and the overlay goes once Titus shows.
browserTest('j15_the_overlay_shows_at_300_ms_of_a_held_open_with_no_event', async () => {
  await withApp(async ({ page }) => {
    let release;
    const held = new Promise((resolve) => { release = resolve; });
    await page.route('**/api/v1/session/open', async (route) => { await held; await route.continue(); });
    await page.evaluate(() => {
      window.__overlayAt = null;
      new MutationObserver(() => {
        const shown = document.querySelector('[data-testid="open-progress"]');
        if (shown && !window.__overlayAt) window.__overlayAt = { at: performance.now(), stage: shown.dataset.stage };
      }).observe(document.body, { childList: true, subtree: true, attributes: true });
    });
    const clicked = await page.evaluate(() => performance.now());
    await titus(page).click();
    await sleep(150);
    assert.equal(await overlay(page).count(), 0, 'nothing for the first 300 ms');
    await overlay(page).waitFor({ timeout: 2_000 });
    const first = await page.evaluate(() => window.__overlayAt);
    const after = first.at - clicked;
    assert.ok(after >= 300 && after < 600, `shown ${Math.round(after)} ms after the click`);
    assert.equal(first.stage, 'journal', 'tC4\'s first stage, with no report');
    assert.equal(await page.getByTestId('open-progress-stage').textContent(), 'Reading the journal');
    assert.equal(await overlay(page).getByRole('progressbar').getAttribute('aria-busy'), 'true', 'indeterminate until a count');
    assert.equal(await page.getByTestId('understand').count(), 0, 'nothing is open while the open is held');
    release();
    await page.getByTestId('understand-unit-s1').waitFor();
    await overlay(page).waitFor({ state: 'detached' });
  });
});

// The server half: an open held on the server (`test/hold` at `open`) keeps its overlay up and
// its project unopened until it is released.
browserTest('an_open_held_on_the_server_keeps_the_overlay_until_released', async () => {
  await withApp(async ({ page, env }) => {
    await env.call('test/hold', { request: newRequestId(), at: 'open' });
    await titus(page).click();
    await overlay(page).waitFor({ timeout: 2_000 });
    await sleep(1_200);
    assert.equal(await overlay(page).count(), 1, 'still up while held');
    assert.equal(await page.getByTestId('understand').count(), 0);
    await env.call('test/release', { request: newRequestId(), at: 'open' });
    await page.getByTestId('understand-unit-s1').waitFor();
    await overlay(page).waitFor({ state: 'detached' });
  });
});

browserTest('a_quick_open_never_shows_the_overlay', async () => {
  await withApp(async ({ page }) => {
    let seen = false;
    await page.exposeFunction('__overlaySeen', () => { seen = true; });
    await page.evaluate(() => new MutationObserver(() => {
      if (document.querySelector('[data-testid="open-progress"]')) window.__overlaySeen();
    }).observe(document.body, { childList: true, subtree: true }));
    await openTitus(page);
    await sleep(400);
    assert.equal(seen, false, 'an open under 300 ms shows no bar');
  });
});

// tC4 #507 and #603: a tile opens Understand at chapter 1; each rail row's share has its tooltip.
browserTest('a_tile_opens_understand_at_chapter_1_with_the_rails_tooltips', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    assert.equal(await page.locator('main h2').textContent(), 'Titus 1');
    assert.equal(await page.getByTestId('mode-switch').getByRole('tab', { selected: true }).textContent(), 'Understand');
    assert.equal(await page.getByTitle('11% of this book\'s verses are drafted').count(), 1);
    const units = await page.locator('[data-testid^="understand-unit-"]').evaluateAll((els) => els.map((e) => e.dataset.testid));
    assert.deepEqual(units, ['s1', 's4', 's6', 's8', 's10', 's12', 's15'].map((k) => `understand-unit-${k}`));
    // Verse view: one unit per verse, and the choice is this computer's for the project.
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByTestId('understand-unit-v16').waitFor();
    // Chapter 2 from the rail.
    await page.getByRole('button', { name: '2', exact: true }).click();
    await until('Titus 2', async () => (await page.locator('main h2').textContent()) === 'Titus 2');
  });
});

browserTest('a_refused_open_says_why_on_home_and_opens_nothing', async () => {
  await withApp(async ({ page, env }) => {
    const refused = { type: 'err', value: { type: 'refused', value: { type: 'open', value: { type: 'unreadable', value: { reason: 'ingredients/tc/project.json is not valid JSON' } } } } };
    await page.route('**/api/v1/session/open', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(refused) }), { times: 1 });
    await titus(page).click();
    const banner = page.getByTestId('home-open-error');
    await banner.waitFor();
    assert.match(await banner.textContent(), /The project could not be opened\. ingredients\/tc\/project\.json is not valid JSON/);
    assert.equal(await banner.getByTestId('ask-for-help').count(), 1, 'Ask for help (tC4 #378)');
    assert.equal(await overlay(page).count(), 0, 'the overlay never stays up');
    assert.deepEqual((await env.call('app/hello', {})).sessions, []);
  });
});

browserTest('going_home_closes_the_screen_session', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus(page);
    assert.equal((await env.call('app/hello', {})).sessions.length, 1);
    await page.getByTitle(/Switch project/).click();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    await until('the session closed', async () => (await env.call('app/hello', {})).sessions.length === 0);
  });
});

// F3: a unit's focus selects its first help, whose quote lights the source words; hovering a
// card lights its own.
browserTest('a_focused_help_lights_its_words_in_the_passage', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId('understand-unit-s4').click();
    await until('words lit in Titus 1:4-5', async () => (await page.getByTestId('understand-unit-s4').getByTestId('source-hl').count()) > 0);
    assert.equal(await page.getByTestId('understand-unit-s4').getAttribute('data-focused'), 'true');
    assert.deepEqual(requests.api(mark), [], 'focus and highlight read nothing');
    await page.getByTestId('helps-note').first().hover();
    await sleep(100);
    assert.ok(await page.getByTestId('source-hl').count() > 0);
  }, { scenario: 'en-pins' });
});

// tC4 #602: the divider resizes the panel within 280 to 720 px, toward the passage.
browserTest('helps_panel_width_follows_the_divider_drag', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    const panel = page.getByTestId('helps-panel');
    const before = (await panel.boundingBox()).width;
    const divider = (await page.getByTestId('helps-divider').boundingBox());
    const x = divider.x + divider.width / 2;
    const y = divider.y + 200;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x - 60, y, { steps: 4 });
    await page.mouse.up();
    await until('wider by 60 px', async () => Math.round((await panel.boundingBox()).width) === Math.round(before + 60));
    await page.mouse.move(x - 60, y);
    await page.mouse.down();
    await page.mouse.move(x - 2000, y, { steps: 4 });
    await page.mouse.up();
    await until('at most 720 px', async () => Math.round((await panel.boundingBox()).width) === 720);
    await page.getByTestId('toggle-helps').click();
    await panel.waitFor({ state: 'detached' });
  });
});

// tC4 #601, #619: the article's header stays in view while it scrolls; its marks are drawn.
browserTest('article_close_button_visible_after_scroll', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    await page.getByRole('tab', { name: 'Words', exact: true }).click();
    await page.getByTestId('helps-word').filter({ hasText: 'of God' }).first().getByText('Read the article').click();
    const article = page.getByTestId('understand-article');
    await article.waitFor();
    assert.equal(await page.getByTestId('helps-article-title').textContent(), 'God');
    const text = await article.textContent();
    assert.ok(!text.includes('__') && !text.includes('**'), 'no raw bold marks');
    await article.evaluate((el) => { el.parentElement.scrollTop = el.parentElement.scrollHeight; });
    await sleep(100);
    assert.ok(await page.getByTestId('helps-article-close').isVisible());
    await page.getByTestId('helps-article-close').click();
    await page.getByTestId('helps-article-pane').waitFor({ state: 'detached' });
  }, { scenario: 'en-pins' });
});
