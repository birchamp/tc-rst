// The Align tool through the page (unit B6; tC4 `e2e/j05-align-verse`, #568, #726, #735, #736):
// J5's steps up to the save (a word chosen, a card clicked, a word sent back, several words
// chosen and placed at once, a drag, a phrase merged and split, the lens, the rail, Previous and
// Next, Mark valid), the keyboard alone, and the click's round trip. The server saves the links
// itself two seconds after the last change (AD18); the save's content is C5's to prove.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/align.test.mjs
//   (TC4_PERF=1 asserts the round trip's 30 ms; build the UI with --release for it)
import assert from 'node:assert/strict';
import { browserTest, recordRequests, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';

async function openAlign(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('open-align').waitFor();
  await page.getByTestId('open-align').click();
  await page.getByTestId('align-bank').waitFor();
}

const bank = (page) => page.locator('[data-testid="align-bank"] > button');
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
/** The card whose original word is `greek`. */
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();
const placed = (page) => page.getByTestId('align-progress').textContent();
const saveState = (page) => page.getByTestId('save-indicator').getAttribute('data-state');

browserTest('j5_align_steps_that_do_not_save', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    assert.equal(await bank(page).count(), 21, 'the 21 words not placed');
    assert.equal(await placed(page), '6 of 27 words placed');
    assert.equal(await page.getByTestId('align-item-counter').textContent(), 'Item 1 of 5');
    assert.equal(await saveState(page), 'saved');

    // A click on a word arms it; a click on a card places it.
    await chip(page, 'fe').click();
    await until('the word is chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    await card(page, 'πίστιν').click();
    await until('seven words placed', async () => (await placed(page)) === '7 of 27 words placed');
    assert.equal(await bank(page).count(), 20);
    assert.equal(await card(page, 'πίστιν').getAttribute('data-count'), '1');
    assert.equal(await saveState(page), 'dirty', 'the links are not saved yet');

    // A click on a placed word sends it back to the bank.
    await card(page, 'πίστιν').locator('button[data-word="fe"]').click();
    await until('six words placed', async () => (await placed(page)) === '6 of 27 words placed');
    assert.equal(await chip(page, 'fe').count(), 1);

    // Ctrl chooses a second word, and one click places both (#736); Shift chooses a range.
    await chip(page, 'verdad').click();
    await until('one word chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 1);
    await chip(page, 'piedad').click({ modifiers: ['Control'] });
    await until('two words chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 2);
    await card(page, 'ἀληθείας').click();
    await until('eight words placed', async () => (await placed(page)) === '8 of 27 words placed');
    await chip(page, 'fe').click();
    await until('one word chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 1);
    await chip(page, 'conocimiento').click({ modifiers: ['Shift'] });
    await until('Shift chooses the words between', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) >= 2);
    // A plain click on a chosen word chooses it alone, and again clears the choice.
    await chip(page, 'fe').click();
    await until('one word chosen again', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 1);
    await chip(page, 'fe').click();
    await until('nothing chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 0);

    // A drag places a word without choosing it first (#129).
    await chip(page, 'verdad').count();
    const dragged = await bank(page).count();
    await chip(page, 'que').dragTo(card(page, 'τῆς'));
    await until('the dragged word is placed', async () => (await bank(page).count()) === dragged - 1);

    // A card dragged onto its neighbour makes a phrase; SPLIT undoes it.
    const split = page.locator('[data-testid^="align-split-"]');
    assert.equal(await split.count(), 1, 'the sample has one phrase');
    await card(page, 'κατὰ').locator('div[draggable="true"]').first().dragTo(card(page, 'πίστιν'));
    await until('the cards are one phrase', async () => (await split.count()) === 2);
    await split.last().click();
    await until('the phrase is split', async () => (await split.count()) === 1);

    // The lens: the original-language words are the headers.
    await page.getByTestId('align-mode-orig').click();
    await until('the Greek lens', async () => (await page.getByTestId('align-ref-text').textContent()).includes('Original'));
    await page.getByTestId('align-mode-gw').click();
    await until('the gateway lens', async () => (await page.getByTestId('align-ref-text').textContent()).includes('Gateway'));

    // How alignment works.
    assert.equal(await page.getByTestId('align-how').getAttribute('aria-expanded'), 'false');
    await page.getByTestId('align-how').click();
    assert.equal(await page.getByTestId('align-how').getAttribute('aria-expanded'), 'true');

    // The rail: a filter with nothing in it says so, and a verse opens on a click.
    await page.getByTestId('align-filter-invalid').click();
    await page.getByTestId('align-rail-empty').waitFor();
    await page.getByTestId('align-filter-all').click();
    await page.locator('[data-testid="align-verse-list"] [data-ref="1:2"]').click();
    await until('verse 1:2', async () => (await page.getByTestId('align-item-counter').textContent()) === 'Item 2 of 5');
    await page.getByTestId('align-next').click();
    await until('verse 1:3', async () => (await page.getByTestId('align-item-counter').textContent()) === 'Item 3 of 5');
    await page.getByTestId('align-prev').click();
    await until('back at verse 1:2', async () => (await page.getByTestId('align-item-counter').textContent()) === 'Item 2 of 5');

    // Mark valid lights the button, and a later change takes it back (D73).
    const mark = page.getByTestId('align-mark-valid');
    assert.equal(await mark.getAttribute('data-active'), '0');
    await mark.click();
    await until('marked valid', async () => (await mark.getAttribute('data-active')) === '1');
    assert.equal(await page.locator('[data-testid="align-verse-list"] [data-ref="1:2"]').getAttribute('data-status'), 'valid');
    const first = bank(page).first();
    await first.click();
    await page.locator('[data-testid^="align-card-"]').first().click();
    await until('the mark is taken back', async () => (await page.getByTestId('align-mark-valid').getAttribute('data-active')) === '0');

    // The server saves what was linked, on its own.
    await until('the links saved', async () => (await saveState(page)) === 'saved', 8000);

    // Back to the picker: nothing to save, nothing lost.
    await page.getByText('All checking tools').click();
    await page.getByTestId('open-align').waitFor();
  });
});

browserTest('align_by_keyboard_only', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    // A word is chosen with Enter and placed with Enter on a card; the focus stays where the
    // person was, though the view is drawn again (#568).
    await chip(page, 'fe').focus();
    await page.keyboard.press('Enter');
    await until('the word is chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    await until('the chip keeps the focus', () => page.evaluate(() => document.activeElement?.getAttribute('data-word') === 'fe'));
    // Ctrl with Enter chooses a second word from the keyboard (#736).
    await chip(page, 'piedad').focus();
    await page.keyboard.press('Control+Enter');
    await until('two words are chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 2);
    // The cards are a Tab stop while a word is chosen; Enter places the chosen words.
    const target = card(page, 'πίστιν');
    assert.equal(await target.getAttribute('tabindex'), '0');
    await target.focus();
    await page.keyboard.press('Enter');
    await until('both words placed', async () => (await placed(page)) === '8 of 27 words placed');
    await until('the card keeps the focus', () => page.evaluate(() => /^align-card-\d+$/.test(document.activeElement?.getAttribute('data-testid') ?? '')));
    // Nothing is chosen any more: the cards are not Tab stops, and Enter on one places nothing.
    assert.equal(await target.getAttribute('tabindex'), '-1');
    await page.keyboard.press('Enter');
    assert.equal(await placed(page), '8 of 27 words placed');
  });
});

/** The painted round trip of a click on a bank word: from the click to the paint of the frame
 * that holds the reply. */
browserTest('align_click_round_trip_p95', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    const requests = recordRequests(page);
    const times = await page.evaluate(async () => {
      const frames = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
      const first = () => document.querySelector('[data-testid="align-bank"] > button');
      const out = [];
      for (let n = 0; n < 60; n += 1) {
        await frames();
        const before = first().getAttribute('data-armed');
        const start = performance.now();
        first().click();
        await new Promise((resolve) => {
          const watch = new MutationObserver(() => {
            if (first().getAttribute('data-armed') !== before) {
              watch.disconnect();
              resolve();
            }
          });
          watch.observe(document.body, { subtree: true, childList: true, attributes: true });
        });
        // Painted, as G0 measured it: the frame that holds the reply has had its style, layout
        // and paint when a message posted from its animation frame is handled.
        await new Promise((resolve) => requestAnimationFrame(() => {
          const channel = new MessageChannel();
          channel.port1.onmessage = () => resolve();
          channel.port2.postMessage(0);
        }));
        out.push(performance.now() - start);
      }
      return out;
    });
    assert.equal(requests.api().filter((p) => p === '/api/v1/align/edit').length, 60, 'each click is one request, none refused or lost');
    const p = (xs, q) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * q)];
    console.log(`aligner click, painted: p50 ${p(times, 0.5).toFixed(1)} ms, p95 ${p(times, 0.95).toFixed(1)} ms (60 clicks)`);
    if (process.env.TC4_PERF === '1') assert.ok(p(times, 0.95) <= 30, `p95 ${p(times, 0.95).toFixed(1)} ms over the 30 ms budget`);
  });
});

/** `fast_clicks_are_queued_not_refused`, in the page: clicks made without waiting for the
 * replies all arrive, in order, and none is refused. */
browserTest('fast_clicks_in_the_page_are_queued_not_refused', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    // Five words chosen with Ctrl, one after the other with no wait between.
    await page.evaluate(() => {
      const chips = [...document.querySelectorAll('[data-testid="align-bank"] > button')].slice(0, 5);
      chips.forEach((el, i) => el.dispatchEvent(new MouseEvent('click', { bubbles: true, ctrlKey: i > 0 })));
    });
    await until('all five chosen', async () => (await page.locator('[data-testid="align-bank"] > button[data-armed="1"]').count()) === 5);
    assert.equal(requests.api(mark).filter((p) => p === '/api/v1/align/edit').length, 5, 'five clicks, five requests');
  });
});

/** Suggestions (tC4 #1, D72): the switch trains the engine in the background, Suggest proposes
 * links as dashed chips that are not links, and Accept all places them. */
browserTest('align_suggestions_through_the_page', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    await page.locator('[data-testid="align-verse-list"] [data-ref="1:2"]').click();
    await until('verse 1:2', async () => (await page.getByTestId('align-item-counter').textContent()) === 'Item 2 of 5');
    const row = page.getByTestId('align-suggestions');
    assert.equal(await row.getAttribute('data-status'), 'off');
    await page.getByTestId('align-suggest-switch').check();
    await until('the engine has learned', async () => (await row.getAttribute('data-status')) === 'ready');
    assert.match(await page.getByTestId('align-suggest-status').textContent(), /Drawn from 1 verse/);
    const before = await placed(page);
    await page.getByTestId('align-suggest').click();
    await until('proposals stand', async () => (await page.locator('[data-testid^="align-suggested-"]').count()) > 0);
    assert.equal(await placed(page), before, 'a proposal is not a link');
    assert.equal(await saveState(page), 'saved', 'and writes nothing');
    await page.getByTestId('align-mark-valid').click();
    await page.getByTestId('align-mark-valid-refused').waitFor();
    await page.getByTestId('align-suggest-accept-all').click();
    await until('the proposals are links', async () => (await page.locator('[data-testid^="align-suggested-"]').count()) === 0);
    assert.notEqual(await placed(page), before);
    assert.equal(await saveState(page), 'dirty');
    await page.getByTestId('align-suggest-switch').uncheck();
    await until('off again', async () => (await row.getAttribute('data-status')) === 'off');
  });
});
