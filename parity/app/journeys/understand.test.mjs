// Open and Understand (B3): J15's overlay on a held open, the open from a tile and its refusal,
// the close at Home, the book rail's tooltip, a unit's focus and its lit words, the helps
// panel's divider, its tabs and the article pane, and a project in another numbering.
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

// A click on a card lights it and leaves the other cards as they were: a note the person
// expanded stays expanded, as tC4 keeps each keyed card.
browserTest('a_card_click_keeps_an_expanded_note_expanded', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    const notes = page.getByTestId('helps-note');
    await notes.first().waitFor();
    const expand = notes.filter({ has: page.getByTestId('note-expand') }).first().getByTestId('note-expand');
    await expand.click();
    assert.equal(await expand.getAttribute('aria-expanded'), 'true');
    const other = await notes.evaluateAll((els) => els.findIndex((el) => !el.querySelector('[data-testid="note-expand"]') && el.dataset.focused !== 'true'));
    assert.ok(other >= 0, 'a short note that is not lit');
    await notes.nth(other).click({ position: { x: 8, y: 8 } });
    await until('the clicked card is lit', async () => (await notes.nth(other).getAttribute('data-focused')) === 'true');
    assert.equal(await expand.getAttribute('aria-expanded'), 'true', 'the expanded note is still expanded');
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

// tC4 #118, #121: a project in another numbering (New Bible offers six) reads one unit per
// project verse, labelled with the source verse it shows, with the verse-only statement in
// place of the switch; a verse the sources have in another book is stated. The sample is in
// `eng`, so the reply is the server's for an `rsc` project of this shape: Titus 1 with its
// verse 2 in another book.
browserTest('cross_frame_project_shows_the_verse_only_statement_instead_of_the_switch', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/editor/sources', async (route) => {
      const reply = await (await route.fetch()).json();
      const rows = reply.value.panes[0].text.value.verses.map((v) => (v.key === '2'
        ? { verse: v.key, source: { type: 'otherBook', value: { to: 'NEH 1:1' } } }
        : { verse: v.key, source: { type: 'verse', value: { chapter: 1, verse: v.key } } }));
      for (const pane of reply.value.panes) {
        if (pane.text.type === 'ready') {
          pane.text.value.sections = [];
          pane.text.value.verses = pane.text.value.verses.filter((v) => v.key !== '2');
        }
      }
      reply.value.frame = { type: 'mapped', value: { rows } };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await titus(page).click();
    await page.getByTestId('understand-unit-m1:1').waitFor();
    assert.equal(await page.getByTestId('understand-verse-only').textContent(), 'Verse view. This project\'s verse numbering differs from the source, so sections are not available.');
    assert.equal(await page.getByRole('tab', { name: 'Verse', exact: true }).count(), 0, 'no Section or Verse switch');
    const units = await page.locator('[data-testid^="understand-unit-"]').count();
    assert.equal(units, 16, 'one unit per project verse');
    assert.match(await page.getByTestId('understand-unit-x1:2').textContent(), /Project verse 1:2 corresponds to NEH 1:1/);
    assert.equal(await page.getByTestId('understand-unit-m1:3').locator('sup').first().textContent(), '3');
  });
});

// tC4 #628: an article's table is a table, its numbered items keep their numbers, and a
// nested item sits further in. The sample's articles hold none, so the reply is the server's
// for an article that does (tc-helps' `ta_article_with_a_table_renders_a_table`).
browserTest('an_article_draws_its_table_numbers_and_nesting', async () => {
  await withApp(async ({ page }) => {
    const span = (text, bold = false) => ({ text, bold, italic: false });
    const block = (kind, text, more = {}) => ({ kind, quote: 0, spans: [span(text)], depth: 0, number: null, cells: [], header: false, ...more });
    const row = (cells, header) => block('tr', cells.join(' '), { cells: cells.map((c) => [span(c, header)]), header });
    const blocks = [
      block('h', 'Numbers'),
      row(['Word', 'Meaning'], true),
      row(['hesed', 'loyal love'], false),
      block('ol', 'one', { number: '1' }),
      block('li', 'top'),
      block('li', 'under', { depth: 1 }),
    ];
    await page.route('**/api/v1/helps/article', (route) => route.fulfill({ status: 200, contentType: 'application/json',
      body: JSON.stringify({ type: 'ok', value: { type: 'found', value: { title: 'God', blocks, dir: 'ltr' } } }) }));
    await openTitus(page);
    await page.getByRole('tab', { name: 'Words', exact: true }).click();
    await page.getByTestId('helps-word').first().getByText('Read the article').click();
    const article = page.getByTestId('understand-article');
    await article.waitFor();
    assert.deepEqual(await article.locator('th').allTextContents(), ['Word', 'Meaning']);
    assert.deepEqual(await article.locator('td').allTextContents(), ['hesed', 'loyal love']);
    assert.ok((await article.textContent()).includes('1. one'));
    const indent = async (text) => article.getByText(text, { exact: true }).evaluate((el) => getComputedStyle(el).paddingInlineStart);
    assert.deepEqual([await indent('top'), await indent('under')], ['14px', '28px']);
  }, { scenario: 'en-pins' });
});

// tC4 #621: the simplified text's verse numbers are superscript, as in the passage.
browserTest('simplified_tab_verse_numbers_are_superscript', async () => {
  await withApp(async ({ page }) => {
    await openTitus(page);
    await page.getByRole('tab', { name: 'UST', exact: true }).click();
    const simplified = page.getByTestId('understand-simplified');
    await simplified.waitFor();
    const numbers = await simplified.locator('sup').allTextContents();
    assert.equal(numbers[0], '1', 'the focused unit Titus 1:1-3 starts at verse 1');
    assert.equal(await simplified.locator('sup').first().evaluate((el) => getComputedStyle(el).verticalAlign), 'super');
  }, { scenario: 'en-pins' });
});

// tC4 #650: helps in a right-to-left gateway read right to left in an English interface. The
// sample's helps are English, so the reply is the server's for a project whose helps rung is
// an Arabic gateway (`rung_dir`).
browserTest('arabic_helps_panes_are_rtl_in_an_english_interface', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/helps/passage', async (route) => {
      const reply = await (await route.fetch()).json();
      for (const slot of ['notes', 'words', 'questions']) {
        if (reply.value[slot].type === 'ready') reply.value[slot].value.dir = 'rtl';
      }
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await openTitus(page);
    await page.getByTestId('helps-note').first().waitFor();
    assert.equal(await page.getByTestId('helps-note').first().getAttribute('dir'), 'rtl');
    assert.equal(await page.evaluate(() => document.documentElement.dir || 'ltr'), 'ltr', 'the interface stays English');
  }, { scenario: 'en-pins' });
});

// tC4 #836: an English question under an Arabic set keeps its own direction; the pane's
// direction lays the cards out, each text reads as it is written.
browserTest('an_english_question_under_an_arabic_set_reads_left_to_right', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/helps/passage', async (route) => {
      const reply = await (await route.fetch()).json();
      // An Arabic set's questions as the server answers them, one left in English (the
      // scenario installs no questions, so the slot is drawn as a ready one).
      reply.value.questions = {
        type: 'ready',
        value: { items: [{ id: 'q1', chapter: '1', verse: '1', question: 'Who wrote this letter?', response: 'Paul.' }], dir: 'rtl', fallback: false },
      };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await openTitus(page);
    await page.getByRole('tab', { name: 'Questions', exact: true }).click();
    const question = page.getByTestId('understand-question').first();
    await question.waitFor();
    assert.equal(await question.getAttribute('dir'), 'rtl', 'the pane is laid out in the set\'s direction');
    assert.equal(await question.locator('p').first().evaluate((el) => getComputedStyle(el).direction), 'ltr', 'the English question reads left to right');
  }, { scenario: 'en-pins' });
});
