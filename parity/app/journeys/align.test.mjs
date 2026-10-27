// The Align tool through the page (unit B6; tC4 `e2e/j05-align-verse`, #568, #726, #735, #736):
// J5's steps up to the save (a word chosen, a card clicked, a word sent back, several words
// chosen and placed at once, a drag, a phrase merged and split, the lens, the rail, Previous and
// Next, Mark valid), the keyboard alone, and the click's round trip. The server saves the links
// itself two seconds after the last change (AD18); the save's content is C5's to prove.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/align.test.mjs
//   (TC4_PERF=1 asserts the round trip's 30 ms; build the UI with --release for it)
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../env-rust.mjs';
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

/** Hold an edit before it reaches the server, so later UI clicks certainly wait behind it. */
async function holdNextEdit(page) {
  let release;
  let reached;
  const held = new Promise((resolve) => { release = resolve; });
  const entered = new Promise((resolve) => { reached = resolve; });
  let first = true;
  await page.route('**/api/v1/align/edit', async (route) => {
    if (first) {
      first = false;
      reached();
      await held;
    }
    await route.continue();
  });
  return { entered, release };
}

/** The initial fe selection is visible; its placement, another selection and its placement
 * are then sent faster than the first reply. Both resulting links must survive. */
async function queueTwoLinks(page) {
  await chip(page, 'fe').click();
  await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
  const hold = await holdNextEdit(page);
  await card(page, 'πίστιν').click();
  await hold.entered;
  await chip(page, 'verdad').click();
  await card(page, 'ἀληθείας').click();
  return hold;
}

async function expectTwoLinks(page) {
  await until('eight words placed', async () => (await placed(page)) === '8 of 27 words placed');
  assert.equal(await card(page, 'πίστιν').locator('button[data-word="fe"]').count(), 1);
  assert.equal(await card(page, 'ἀληθείας').locator('button[data-word="verdad"]').count(), 1);
}

/** Leave through Home so the store checkpoints, then assert the projected links and manifest. */
async function expectTwoStoredLinks(page, env) {
  const closed = page.waitForResponse((response) => response.url().endsWith('/api/v1/session/close'));
  await page.getByTestId('project-switch').click();
  await page.getByTestId(`project-${BIBLE}`).waitFor();
  await closed;
  // Close schedules the checkpoint; settings waits for that folder's leave to finish.
  await env.call('settings/viewFromHome', { project: BIBLE });
  const folder = path.join(env.projectsDir, BIBLE);
  const alignment = JSON.parse(fs.readFileSync(path.join(folder, 'ingredients/checking/alignments/TIT.json'), 'utf8')).chapters['1']['1'];
  for (const [source, target] of [['πίστιν', 'fe'], ['ἀληθείας', 'verdad']]) {
    assert.ok(alignment.alignments.some((group) => group.topWords.some((word) => word.word === source)
      && group.bottomWords.some((word) => word.word === target)), `${source} keeps ${target} on disk`);
    assert.ok(!alignment.wordBank.some((word) => word.word === target), `${target} is placed`);
  }
  const metadata = JSON.parse(fs.readFileSync(path.join(folder, 'metadata.json'), 'utf8'));
  for (const [ingredient, expected] of Object.entries(metadata.ingredients)) {
    const bytes = fs.readFileSync(path.join(folder, ingredient));
    assert.equal(bytes.length, expected.size, `${ingredient}: size`);
    assert.equal(crypto.createHash('md5').update(bytes).digest('hex'), expected.checksum.md5, `${ingredient}: checksum`);
  }
}

/** Exercise the production close callback registered by Saves::guard_close. */
async function installCloseBridge(context) {
  await context.addInitScript(() => {
    window.__canClose = true;
    window.tc4Shell = {
      bufferChanged: () => {},
      takeBuffered: async () => [],
      setCanClose: (clean) => { window.__canClose = clean; },
      onCloseRequested: (answer) => { window.__requestClose = answer; },
    };
  });
}

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

/** A real suggestions-switch change emits status frames while page edits are pending. */
browserTest('suggestion_refresh_keeps_queued_links_and_the_workspace', async () => {
  await withApp(async ({ page, env }) => {
    await openAlign(page);
    const hold = await queueTwoLinks(page);
    try {
      const changed = page.waitForResponse((response) => response.url().endsWith('/api/v1/prefs/setAlignSuggestions'));
      await page.getByTestId('align-suggest-switch').check();
      await changed;
      await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      assert.equal(await page.getByTestId('align-bank').isVisible(), true, 'refresh leaves the workspace mounted');
      hold.release();
      await expectTwoLinks(page);
      await page.getByText('All checking tools').click();
      await page.getByTestId('open-align').waitFor();
      await page.getByTestId('open-align').click();
      await expectTwoLinks(page);
      await expectTwoStoredLinks(page, env);
    } finally {
      hold.release();
    }
  });
});

/** The server cannot close a verse before the queued placements even reach it. */
browserTest('closing_the_tool_drains_queued_links_before_saving', async () => {
  await withApp(async ({ page, env }) => {
    await openAlign(page);
    const requests = recordRequests(page);
    const hold = await queueTwoLinks(page);
    try {
      const mark = requests.mark();
      await page.getByText('All checking tools').click();
      await page.evaluate(() => new Promise(requestAnimationFrame));
      assert.equal(requests.api(mark).filter((url) => url.endsWith('/align/close')).length, 0, 'no close can overtake the first placement');
      hold.release();
      await page.getByTestId('open-align').waitFor();
      await page.getByTestId('open-align').click();
      await expectTwoLinks(page);
      await expectTwoStoredLinks(page, env);
    } finally {
      hold.release();
    }
  });
});

/** The production shell guard waits for queued UI edits and the real store's held save.
 * A typed flush failure is injected at the HTTP boundary, after that save, so this tests the
 * page's failure and Retry handling; store-level failure recovery remains tc-app's proof. */
browserTest('queued_links_hold_the_close_guard_through_save_failure_and_retry', async () => {
  await withApp(async ({ page, env }) => {
    await openAlign(page);
    await page.waitForFunction(() => typeof window.__requestClose === 'function');
    const requests = recordRequests(page);
    const hold = await queueTwoLinks(page);
    let first = true;
    await page.route('**/api/v1/align/flush', async (route) => {
      if (!first) return route.continue();
      first = false;
      const response = await route.fetch();
      // Observe the real saved event before replacing the flush reply, so a later Saved
      // event cannot hide the failure the page is about to receive.
      await until('real save observed', () => page.evaluate(() => window.__canClose === true));
      return route.fulfill({ response, json: { type: 'ok', value: { type: 'failed', value: { type: 'disk', value: 'The disk is full.' } } } });
    });
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    try {
      assert.equal(await page.evaluate(() => window.__canClose), false, 'pending page placements already hold the shell guard');
      const mark = requests.mark();
      await page.evaluate(() => {
        window.__closeResult = 'waiting';
        window.__requestClose().then((answer) => { window.__closeResult = answer; });
      });
      await page.evaluate(() => new Promise(requestAnimationFrame));
      assert.equal(requests.api(mark).filter((url) => url.endsWith('/align/flush')).length, 0, 'flush waits for the queued placements');
      assert.equal(await page.evaluate(() => window.__closeResult), 'waiting');
      hold.release();
      await expectTwoLinks(page);
      await until('save held before durability', async () => (await saveState(page)) === 'saving');
      assert.equal(await page.evaluate(() => window.__canClose), false);
      assert.equal(await page.evaluate(() => window.__closeResult), 'waiting', 'the held save cannot allow close');
      await env.call('test/release', { request: newRequestId(), at: 'save' });
      await until('guard refused on failure', () => page.evaluate(() => window.__closeResult === false));
      assert.equal(await saveState(page), 'error');
      assert.equal(await page.evaluate(() => window.__canClose), false);
      const beforeRetry = requests.mark();
      await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).click();
      await until('retry saved alignment', async () => (await saveState(page)) === 'saved');
      assert.equal(requests.api(beforeRetry).filter((url) => url.endsWith('/align/flush')).length, 1, 'Retry flushes the server-owned aligner');
      assert.equal(await page.evaluate(() => window.__requestClose()), true);
      await expectTwoStoredLinks(page, env);
    } finally {
      hold.release();
      await env.call('test/release', { request: newRequestId(), at: 'save' });
    }
  }, { init: installCloseBridge });
});

/** A verse is not left while its links cannot be saved (a full disk): the server refuses the
 * switch, and the page shows the verse that stays open, with its links, not an error. */
browserTest('a_refused_verse_switch_keeps_the_open_verse_shown', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    await chip(page, 'fe').click();
    await card(page, 'πίστιν').click();
    await until('seven words placed', async () => (await placed(page)) === '7 of 27 words placed');
    // The server's answer when the links of 1:1 cannot be saved (tc-app
    // `leaving_a_verse_saves_it_first_and_a_failure_keeps_the_person_there`).
    await page.route('**/api/v1/align/open', (route) => {
      const verse = route.request().postDataJSON().verse;
      if (verse?.chapter !== 1 || verse?.verse !== 2) return route.fallback();
      return route.fulfill({ status: 200, contentType: 'application/json', body: '{"type":"err","value":{"type":"notSaved","value":{"type":"disk","value":"full"}}}' });
    });
    await page.locator('[data-testid="align-verse-list"] [data-ref="1:2"]').click();
    await until('verse 1:1 is open again', async () => (await page.getByTestId('align-verse-list').locator('[data-selected="true"]').getAttribute('data-ref')) === '1:1');
    await page.getByTestId('align-bank').waitFor();
    assert.equal(await page.getByTestId('align-error').count(), 0, 'no error stands in for the open verse');
    assert.equal(await page.getByTestId('align-item-counter').textContent(), 'Item 1 of 5');
    assert.equal(await placed(page), '7 of 27 words placed', 'the links of 1:1 are shown');
  });
});

/** Home with a click not yet saved (owner ruling ALIGN-LEAVE, 2026-10-26): it waits while the
 * click is on its way and asks Stay or Leave without it only once the click is lost for sure.
 * `asked` records whether the question ever showed. */
async function watchQuestion(page) {
  await page.evaluate(() => {
    window.__asked = false;
    new MutationObserver(() => {
      if (document.querySelector('[role="alertdialog"]')) window.__asked = true;
    }).observe(document.body, { childList: true, subtree: true });
  });
  return () => page.evaluate(() => window.__asked);
}

const question = (page) => page.getByRole('alertdialog');
const home = (page) => page.getByTestId(`project-${BIBLE}`);

/** The first word is chosen, and the click that places it is held on its way. */
async function holdThePlacement(page) {
  await chip(page, 'fe').click();
  await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
  const hold = await holdNextEdit(page);
  await card(page, 'πίστιν').click();
  await hold.entered;
  return hold;
}

browserTest('a_click_that_saves_within_the_wait_leaves_with_no_question', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    const asked = await watchQuestion(page);
    const hold = await holdThePlacement(page);
    await page.getByTestId('project-switch').click();
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    assert.equal(await home(page).count(), 0, 'Home waits for the click');
    hold.release();
    await home(page).waitFor({ timeout: 10_000 }).catch(() => {});
    assert.equal(await asked(), false, 'INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: no question for a click that saved');
    await home(page).waitFor();
    await openAlign(page);
    await until('the click was saved', async () => (await placed(page)) === '7 of 27 words placed');
  });
});

browserTest('a_click_sent_again_after_a_cut_leaves_with_no_question', async () => {
  await withApp(async ({ page, allow }) => {
    allow(/Failed to load resource: net::ERR_FAILED/);
    await openAlign(page);
    const asked = await watchQuestion(page);
    await chip(page, 'fe').click();
    await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    let cut = true;
    await page.route('**/api/v1/align/edit', (route) => (cut ? route.abort('failed') : route.continue()));
    await card(page, 'πίστιν').click();
    await page.getByTestId('project-switch').click();
    await new Promise((resolve) => setTimeout(resolve, 1_000));
    cut = false;
    await home(page).waitFor({ timeout: 10_000 }).catch(() => {});
    assert.equal(await asked(), false, 'INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: Home sends the click again and asks nothing');
    await home(page).waitFor();
    await openAlign(page);
    await until('the click was saved', async () => (await placed(page)) === '7 of 27 words placed');
  });
});

browserTest('a_click_refused_for_good_asks_and_leave_without_it_goes_home', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    await chip(page, 'fe').click();
    await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    await page.route('**/api/v1/align/edit', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: '{"type":"err","value":{"type":"staleSession"}}' }));
    await card(page, 'πίστιν').click();
    await page.getByTestId('align-edit-error').waitFor();
    const started = Date.now();
    await page.getByTestId('project-switch').click();
    await question(page).waitFor({ timeout: 2_000 }).catch(() => {
      throw new Error('INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: a click refused for good asks at once');
    });
    assert.ok(Date.now() - started < 2_000, 'no wait for a click that cannot save');
    assert.equal(await home(page).count(), 0, 'Home waits for the answer');
    await question(page).getByRole('button', { name: 'Leave without it' }).click();
    await home(page).waitFor();
  });
});

browserTest('a_stuck_click_asks_after_the_limit_and_stay_keeps_it', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    const hold = await holdThePlacement(page);
    try {
      const started = Date.now();
      await page.getByTestId('project-switch').click();
      await question(page).waitFor({ timeout: 6_000 }).catch(() => {
        throw new Error('INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: a click stuck past the limit asks');
      });
      const waited = Date.now() - started;
      assert.ok(waited >= 2_900, `Home waited the limit first (${waited} ms)`);
      await question(page).getByRole('button', { name: 'Stay' }).click();
      await question(page).waitFor({ state: 'detached' });
      assert.equal(await home(page).count(), 0, 'Stay keeps the person in the aligner');
      hold.release();
      await until('the kept click saves', async () => (await placed(page)) === '7 of 27 words placed');
      await until('saved', async () => (await saveState(page)) === 'saved');
    } finally {
      hold.release();
    }
  });
});

browserTest('a_click_that_saves_while_home_asks_closes_the_question_and_goes_home', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    const hold = await holdThePlacement(page);
    try {
      await page.getByTestId('project-switch').click();
      await question(page).waitFor({ timeout: 6_000 });
      hold.release();
      await home(page).waitFor({ timeout: 10_000 }).catch(() => {
        throw new Error('INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: a click that saves while asked is not lost, so Home goes on');
      });
      assert.equal(await question(page).count(), 0);
      await openAlign(page);
      await until('the click was saved', async () => (await placed(page)) === '7 of 27 words placed');
    } finally {
      hold.release();
    }
  });
});

browserTest('stay_keeps_the_click_on_the_page', async () => {
  await withApp(async ({ page, allow }) => {
    // The cut connection: each send of the click fails, about four times a second.
    allow(/Failed to load resource: net::ERR_FAILED/);
    await openAlign(page);
    await chip(page, 'fe').click();
    await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    let cut = true;
    await page.route('**/api/v1/align/edit', (route) => (cut ? route.abort('failed') : route.continue()));
    await card(page, 'πίστιν').click();
    await page.getByTestId('project-switch').click();
    await question(page).waitFor({ timeout: 6_000 });
    await question(page).getByRole('button', { name: 'Stay' }).click();
    await question(page).waitFor({ state: 'detached' });
    cut = false;
    try {
      await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).click({ timeout: 5_000 });
      await until('the click Stay kept saves on Retry', async () => (await placed(page)) === '7 of 27 words placed');
    } catch {
      throw new Error('INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: Stay keeps the click, so Retry saves it');
    }
  });
});

/** tC4 #807: after a sync, Home waited forever on a span save queued behind a refused
 * alignment save. The rebuild has no such queue, and the aligner has no text field: leaving
 * Translate for it saved every field. The gate asks the server to save the links; a refusal no
 * retry in this open can fix (StaleScope, faked here as the server would answer it) lets the person
 * leave in one click, as a field's does (#801); the server's close keeps the links' draft (its
 * own tests). A full disk still holds the person
 * (queued_links_hold_the_close_guard_through_save_failure_and_retry). */
browserTest('a_refused_align_save_behind_a_span_save_lets_home_leave', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    await chip(page, 'fe').click();
    await card(page, 'πίστιν').click();
    await until('seven words placed', async () => (await placed(page)) === '7 of 27 words placed');
    let refused = 0;
    await page.route('**/api/v1/align/flush', (route) => {
      refused += 1;
      return route.fulfill({ status: 200, contentType: 'application/json', body: '{"type":"ok","value":{"type":"failed","value":{"type":"staleScope"}}}' });
    });
    await page.getByTestId('project-switch').click();
    await page.getByTestId(`project-${BIBLE}`).waitFor({ timeout: 10_000 }).catch(() => {
      throw new Error('INV-A-REFUSED-SAVE-NEVER-TRAPS-THE-TRANSLATOR: Home is reached in one click after a refused alignment save');
    });
    assert.ok(refused >= 1, 'the gate met the refusal');
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

/** A placement on the open verse of a book as long as the Psalms: the rail has 2,461 rows, and a
 * click must cost the verse, not the book. The rail's reply is made longer in the route. */
browserTest('align_placement_paints_within_30_ms_on_a_psalms_sized_rail', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/align/index', async (route) => {
      const response = await route.fetch();
      const body = await response.json();
      const real = body.value.items;
      const items = [...real];
      for (let chapter = 2; items.length < 2461; chapter += 1) {
        for (let verse = 1; verse <= 17 && items.length < 2461; verse += 1) {
          items.push({ place: { chapter, verse, last: verse }, text: `Verse ${chapter}:${verse} of a long book, drafted and waiting to be aligned.`, status: { state: 'todo', placed: 0, total: 20 } });
        }
      }
      await route.fulfill({ response, json: { ...body, value: { ...body.value, items } } });
    });
    await openAlign(page);
    const drawing = Date.now();
    await until('the long rail is drawn', async () => (await page.locator('[data-testid="align-verse-list"] > button').count()) >= 2461);
    console.log(`the 2,461-row rail was drawn ${Date.now() - drawing} ms after the first verse (the polling step is 25 ms)`);
    const times = await page.evaluate(async () => {
      const painted = () => new Promise((resolve) => requestAnimationFrame(() => {
        const channel = new MessageChannel();
        channel.port1.onmessage = () => resolve();
        channel.port2.postMessage(0);
      }));
      const progress = () => document.querySelector('[data-testid="align-progress"]').textContent;
      // Waits catch a hang only (ruling 2026-10-23): a minute, then the failure names the step.
      const HANG_MS = 60000;
      const hung = (what) => new Error(`align_placement_paints_within_30_ms_on_a_psalms_sized_rail: ${what}: the step hung`);
      const waitUntil = async (ok, what) => {
        const end = performance.now() + HANG_MS;
        while (!ok()) {
          if (performance.now() > end) throw hung(what);
          await new Promise((resolve) => setTimeout(resolve, 10));
        }
      };
      const step = async (act) => {
        const before = progress();
        const start = performance.now();
        act();
        await new Promise((resolve, reject) => {
          const timer = setTimeout(() => { watch.disconnect(); reject(hung('the progress line never changed after the click')); }, HANG_MS);
          const watch = new MutationObserver(() => {
            if (progress() !== before) { watch.disconnect(); clearTimeout(timer); resolve(); }
          });
          watch.observe(document.body, { subtree: true, childList: true, characterData: true, attributes: true });
        });
        await painted();
        return performance.now() - start;
      };
      const out = [];
      for (let n = 0; n < 25; n += 1) {
        const chip = () => document.querySelector('[data-testid="align-bank"] > button[data-word="fe"]');
        chip().click();
        await waitUntil(() => chip()?.getAttribute('data-armed') === '1', 'the word "fe" was never armed');
        const card = [...document.querySelectorAll('[data-testid^="align-card-"]')].find((c) => c.textContent.includes('πίστιν'));
        out.push(await step(() => card.click()));
        const placedWord = [...document.querySelectorAll('[data-testid^="align-card-"]')].find((c) => c.textContent.includes('πίστιν')).querySelector('button[data-word="fe"]');
        out.push(await step(() => placedWord.click()));
      }
      return out;
    });
    const p = (xs, q) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * q)];
    console.log(`placement on a 2,461-row rail, painted: p50 ${p(times, 0.5).toFixed(1)} ms, p95 ${p(times, 0.95).toFixed(1)} ms (${times.length} edits)`);
    if (process.env.TC4_PERF === '1') assert.ok(p(times, 0.95) <= 30, `p95 ${p(times, 0.95).toFixed(1)} ms over the 30 ms budget`);
  });
});

browserTest('a_newer_completed_edit_prevents_an_old_saved_flush_reply_from_allowing_close', async () => {
  await withApp(async ({ page, env }) => {
    await openAlign(page);
    await page.waitForFunction(() => typeof window.__requestClose === 'function');
    await chip(page, 'fe').click();
    await until('fe selected', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    await card(page, 'πίστιν').click();
    await until('first link placed', async () => (await placed(page)) === '7 of 27 words placed');
    let release;
    let reached;
    const held = new Promise((resolve) => { release = resolve; });
    const entered = new Promise((resolve) => { reached = resolve; });
    let first = true;
    await page.route('**/api/v1/align/flush', async (route) => {
      if (!first) return route.continue();
      first = false;
      const response = await route.fetch();
      reached();
      await held;
      return route.fulfill({ response });
    });
    const requests = recordRequests(page);
    const mark = requests.mark();
    try {
      await page.evaluate(() => {
        window.__closeResult = 'waiting';
        window.__requestClose().then((value) => { window.__closeResult = value; });
      });
      await entered;
      await chip(page, 'verdad').click();
      await until('verdad selected', async () => (await chip(page, 'verdad').getAttribute('data-armed')) === '1');
      await card(page, 'ἀληθείας').click();
      await expectTwoLinks(page);
      release();
      await until('close can finish after saving the newer edit', () => page.evaluate(() => window.__closeResult === true));
      assert.equal(requests.api(mark).filter((url) => url.endsWith('/align/flush')).length, 2, 'an idle queue cannot make the older flush certify the newer links');
      await expectTwoStoredLinks(page, env);
    } finally {
      release();
    }
  }, { init: installCloseBridge });
});

for (const lostReply of [false, true]) {
  browserTest(`queued_links_survive_${lostReply ? 'a_lost_reply' : 'a_request_that_never_arrived'}_and_retry_the_exact_request`, async () => {
    await withApp(async ({ page, env, errors, allow }) => {
      await openAlign(page);
      await chip(page, 'fe').click();
      await until('fe selected', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
      let release;
      let reached;
      const held = new Promise((resolve) => { release = resolve; });
      const entered = new Promise((resolve) => { reached = resolve; });
      let requests = 0;
      let original;
      await page.route('**/api/v1/align/edit', async (route) => {
        requests += 1;
        if (requests === 1) {
          original = route.request().postDataJSON();
          if (lostReply) await route.fetch();
          reached();
          await held;
          return route.abort('failed');
        }
        if (requests === 2) {
          assert.deepEqual(route.request().postDataJSON(), original, 'scope, aligner, seq, request id and edit all survive Retry');
        }
        return route.continue();
      });
      try {
        await card(page, 'πίστιν').click();
        await entered;
        await chip(page, 'verdad').click();
        await card(page, 'ἀληθείας').click();
        release();
        await until('the failed edit is visible', async () => (await saveState(page)) === 'error');
        await page.getByTestId('align-edit-error').waitFor();
        assert.equal(requests, 1, 'later clicks wait behind the unanswered request');
        assert.equal(await page.evaluate(() => window.__canClose), false, 'the failed queue still holds the shell guard');
        assert.equal(await page.evaluate(() => window.__requestClose()), false, 'a known failure refuses close instead of hanging');
        await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).click();
        await expectTwoLinks(page);
        await until('retry flushed the retained links', async () => (await saveState(page)) === 'saved');
        assert.equal(await page.getByTestId('align-edit-error').count(), 0);
        await expectTwoStoredLinks(page, env);
      } finally {
        release();
      }
      // Chromium reports the one deliberately aborted fetch; every other error still fails.
      const aborted = 'console: Failed to load resource: net::ERR_FAILED';
      assert.equal(errors.filter((error) => error === aborted).length, 1);
      allow(/^console: Failed to load resource: net::ERR_FAILED$/);
    }, { init: installCloseBridge });
  });
}

/** tC4 #830: the words placed on a card are the translation's and read in its direction, even
 * on an Old Testament verse whose original reads right to left (the server's `origDir`). */
browserTest('an_ot_card_shows_placed_words_in_the_translation_direction', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/align/open', async (route) => {
      const reply = await (await route.fetch()).json();
      // `align/open` answers an `Opened` enum: the view is inside its `ready` envelope.
      if (reply.type === 'ok' && reply.value.type === 'ready') reply.value.value.origDir = 'rtl';
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await openAlign(page);
    const placed = page.locator('[data-testid^="align-card-"] button[data-word]');
    await placed.first().waitFor();
    const directions = await placed.evaluateAll((els) => els.map((e) => getComputedStyle(e).direction));
    assert.ok(directions.length > 0 && directions.every((d) => d === 'ltr'), `placed words read in the translation's direction: ${directions.join(' ')}`);
  });
});

/** Owner answer Q30 (a), tC4 #825: a link a save or an open sent back to the word bank is listed
 * on the Align screen with its word (the server's `dropped`); nothing goes back silently. */
browserTest('a_link_sent_back_to_the_bank_is_listed', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/align/open', async (route) => {
      const reply = await (await route.fetch()).json();
      if (reply.type === 'ok' && reply.value.type === 'ready') reply.value.value.dropped = [{ card: 1, top: [{ word: 'Παῦλος', occurrence: 1, occurrences: 1 }], word: { word: 'Pablo', occurrence: 1, occurrences: 1 } }];
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await openAlign(page);
    const listed = page.getByTestId('align-dropped');
    await listed.waitFor();
    assert.match(await listed.textContent(), /word bank.*Pablo/);
  });
});
