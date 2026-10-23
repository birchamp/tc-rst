// The Check pages with the keyboard (tC4 #42, `e2e/check-keyboard.spec.ts`), B5a's parts: the
// picker's Tab stops with the design system's ring, the guided fix holding the focus and giving
// it back to its button, a session's rail taking the focus once and being one Tab stop whose
// arrow keys move the selection, Tab leaving the list for the detail column, and Back giving the
// focus to the card that opened the session. The detail column's stops are B5b's. Also tC4
// #518 and #513: a resume into a tool opens it with no picker to click meanwhile, and once.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/check-keyboard.test.mjs
import assert from 'node:assert/strict';
import { browserTest, recordRequests, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';

async function openCheck(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('preflight-translationNotes').waitFor();
}

/** The focused element's test id, and whether it shows tC4's ring: an outline 2px or wider in
 * the colour of `--accent`. */
const focusedWithRing = (page) => page.evaluate(() => {
  const el = document.activeElement;
  const probe = document.createElement('span');
  probe.style.color = 'var(--accent)';
  document.body.appendChild(probe);
  const accent = getComputedStyle(probe).color;
  probe.remove();
  const cs = getComputedStyle(el);
  return {
    testid: el?.dataset?.testid ?? null,
    ring: cs.outlineStyle !== 'none' && parseFloat(cs.outlineWidth) >= 2 && cs.outlineColor === accent,
  };
});

const activeIs = (page, selector) => page.evaluate((sel) => document.activeElement?.matches(sel) ?? false, selector);

browserTest('check_keyboard_picker_guided_fix_and_rail', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    // 1 · The picker: the not-ready card's fix button, then the ready card's open button, each
    // with the ring.
    await page.getByRole('tab', { name: 'Check' }).focus();
    const stops = [];
    for (let n = 0; n < 6; n++) {
      await page.keyboard.press('Tab');
      const stop = await focusedWithRing(page);
      stops.push(stop.testid);
      assert.ok(stop.ring, `Tab stop ${stop.testid} shows the ring`);
      if (stop.testid === 'open-translationNotes') break;
    }
    assert.ok(stops.indexOf('fix-translationWords') >= 0, `${stops}`);
    assert.ok(stops.indexOf('fix-translationWords') < stops.indexOf('open-translationNotes'), `${stops}`);

    // 6 · The guided fix holds the focus, and Escape gives it back to its button.
    await page.getByTestId('fix-translationWords').focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByTestId('guided-fix');
    await dialog.waitFor();
    const inside = () => page.evaluate(() => !!document.activeElement?.closest('[data-testid="guided-fix"]'));
    await until('the dialog takes the focus', inside);
    for (let n = 0; n < 8; n++) {
      await page.keyboard.press('Tab');
      assert.ok(await inside(), `Tab stop ${n + 1} stays in the dialog`);
    }
    await page.keyboard.press('Shift+Tab');
    assert.ok(await inside(), 'Shift+Tab stays in too');
    await page.keyboard.press('Escape');
    await dialog.waitFor({ state: 'detached' });
    await until('the focus is back on Fix this…', () => activeIs(page, '[data-testid="fix-translationWords"]'));

    // 2 · A session: its current row takes the focus once.
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    await until('the current row has the focus', () => activeIs(page, '[data-testid="check-list"] [aria-current="true"]'));
    const current = await page.locator('[data-testid="check-list"] [aria-current="true"]').getAttribute('data-rail-index');
    assert.equal(current, '1', 'item 2, the first not decided');

    // 3 · One Tab stop: the arrows move the selection, and the focus with it.
    const tabbable = await page.locator('[data-testid="check-list"] [tabindex="0"]').count();
    assert.equal(tabbable, 1);
    await page.keyboard.press('ArrowDown');
    await until('Down selects the next row', () => activeIs(page, '[data-testid="check-list"] [aria-current="true"][data-rail-index]:not([data-rail-index="1"])'));
    await page.keyboard.press('ArrowUp');
    await until('Up returns', () => activeIs(page, '[data-testid="check-list"] [aria-current="true"][data-rail-index="1"]'));
    await page.keyboard.press('End');
    const last = await page.locator('[data-testid="check-list"] button[data-rail-index]').last().getAttribute('data-rail-index');
    await until('End selects the last row', () => activeIs(page, `[data-rail-index="${last}"][aria-current="true"]`));
    await page.keyboard.press('Home');
    await until('Home selects the first row', () => activeIs(page, '[data-testid="check-list"] button[data-rail-index]:first-of-type[aria-current="true"]'));
    // Tab leaves the list at once: "Back to translating" is out of the Tab order.
    await page.keyboard.press('Tab');
    assert.equal(await activeIs(page, '[data-testid="check-list"] *'), false, 'Tab leaves the list');
    assert.equal(await activeIs(page, '[data-testid="check-to-draft"]'), false);

    // 7 · Back gives the focus to the card that opened the session.
    await page.getByTestId('check-back').focus();
    await page.keyboard.press('Enter');
    await page.getByTestId('preflight-translationNotes').waitFor();
    await until('the focus is back on the card', () => activeIs(page, '[data-testid="open-translationNotes"]'));
  });
});

// tC4 #518 and #513: a tile that returns to a Check tool opens it at once, with no picker drawn
// to click while it opens, and asks for its rail once.
browserTest('resume_and_tab_click_open_the_tool_once', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    // Home, then the tile: the place kept says Check and Translation Notes.
    await page.getByTestId('project-switch').click();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    let pickerSeen = false;
    await page.exposeFunction('__pickerSeen', () => { pickerSeen = true; });
    await page.evaluate(() => new MutationObserver(() => {
      if (document.querySelector('[data-testid="preflight-translationNotes"]')) window.__pickerSeen();
    }).observe(document.body, { childList: true, subtree: true }));
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('check-progress').waitFor();
    // A tab click meanwhile changes nothing: the tool stays open.
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('check-progress').waitFor();
    assert.equal(pickerSeen, false, 'no picker shows while the tool opens');
    const opens = requests.api(mark).filter((p) => p === '/api/v1/check/open');
    assert.equal(opens.length, 1, `${opens}`);
  });
});

/** A rail of Psalms tN's size (8,212 rows, PERFORMANCE.md) in place of the sample's 112: the
 * server's own session reply with its rows replaced, so the screen draws a virtual list. */
async function bigRail(page) {
  await page.route('**/api/v1/check/open', async (route) => {
    const response = await route.fetch();
    const body = await response.json();
    const rows = Array.from({ length: 8212 }, (_, i) => ({
      index: i,
      checkId: `p${i}`,
      place: `PSA ${1 + Math.floor(i / 55)}:${1 + (i % 55)}`,
      title: `check ${i}`,
      flags: { dot: 'todo', decided: false, invalidated: false, invalid: false, bookmarked: i % 97 === 0, commented: false },
      term: i % 485,
      category: { type: 'note', value: ['figures', 'grammar', 'culture'][i % 3] },
      marker: null,
    }));
    const terms = Array.from({ length: 485 }, (_, i) => `term-${String(i).padStart(3, '0')}`);
    const value = { ...body.value, rows: { type: 'full', value: rows }, terms, progress: { decided: 0, total: 8212 }, start: 0 };
    await route.fulfill({ response, json: { ...body, value } });
  });
}

// §6.11 (A-24): the check-keyboard steps on a rail of Psalms' size. Only the rows in view are
// drawn; the tab-stop row stays mounted wherever it is; the keys move through all 8,212 rows.
browserTest('check_keyboard_on_a_psalms_sized_rail', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    await bigRail(page);
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    const drawn = () => page.locator('[data-testid="check-list"] button[data-rail-index]').count();
    await until('the first row has the focus', () => activeIs(page, '[data-rail-index="0"][aria-current="true"]'));
    assert.ok(await drawn() < 80, `only the rows in view are drawn (${await drawn()})`);
    await page.keyboard.press('End');
    await until('End reaches the last row and focuses it', () => activeIs(page, '[data-rail-index="8211"][aria-current="true"]'));
    const scrolled = await page.getByTestId('check-list').evaluate((el) => el.scrollTop);
    assert.ok(scrolled > 300_000, `the list scrolled to the end (${scrolled})`);
    await page.keyboard.press('ArrowDown');
    await until('Down wraps to the first row', () => activeIs(page, '[data-rail-index="0"][aria-current="true"]'));
    await page.keyboard.press('ArrowUp');
    await until('Up wraps to the last row', () => activeIs(page, '[data-rail-index="8211"][aria-current="true"]'));
    // Scrolled back to the top, the selected row is still mounted: Tab and Shift+Tab return to it.
    await page.getByTestId('check-list').evaluate((el) => { el.scrollTop = 0; });
    await until('the top rows are drawn', () => page.locator('[data-rail-index="0"]').count().then((n) => n === 1));
    assert.equal(await page.locator('[data-rail-index="8211"][tabindex="0"]').count(), 1, 'the tab stop stays mounted');
    await page.keyboard.press('Tab');
    await page.keyboard.press('Shift+Tab');
    await until('Shift+Tab comes back to the tab stop', () => activeIs(page, '[data-rail-index="8211"]'));
    // A filter and a sort change the list in place.
    await page.getByTestId('filter-bookmarked').click();
    assert.equal(await page.getByTestId('filter-bookmarked').getAttribute('data-count'), '85');
    await page.getByTestId('sort-byCategory').click();
    await page.getByTestId('check-list').evaluate((el) => { el.scrollTop = 0; });
    await until('By category heads the groups', () => page.locator('[data-testid="check-list"] p').first().textContent().then((t) => t === 'figures'));
  });
});

// PERF:78 (APP-DESIGN §8.2, "Rail filter, sort, group"): a filter or a sort on a rail of Psalms'
// size fits in a frame (16 ms) and sends no request. What is timed is the update's work, from the
// click to the list drawn and laid out (its effects run, then a forced layout); the frame after it
// is reported beside it, since it also waits for the screen's next refresh. The budget holds in a
// release build (`build-app.sh --release`): run with TC4_PERF=1 and that build in TC4_UI to hold
// the times to it, else they are printed.
browserTest('rail_filter_and_sort_paint_within_a_frame_on_a_psalms_sized_rail', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    await bigRail(page);
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    const requests = recordRequests(page);
    const mark = requests.mark();
    const { work, frames } = await page.evaluate(async () => {
      const frame = () => new Promise((resolve) => requestAnimationFrame(() => resolve()));
      const task = () => new Promise((resolve) => setTimeout(resolve, 0));
      const work = [];
      const frames = [];
      const ids = ['filter-bookmarked', 'filter-all', 'sort-byCategory', 'sort-byVerse', 'filter-todo', 'filter-all'];
      for (let round = 0; round < 5; round += 1) {
        for (const id of ids) {
          await frame();
          const start = performance.now();
          document.querySelector(`[data-testid="${id}"]`).click();
          await task();
          void document.querySelector('[data-testid="check-list"]').getBoundingClientRect().height;
          work.push(performance.now() - start);
          await frame();
          frames.push(performance.now() - start);
        }
      }
      return { work, frames };
    });
    assert.deepEqual(requests.api(mark), [], 'a filter or a sort sends no request');
    const p = (xs, q) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * q)];
    console.log(`rail filter/sort on 8,212 rows: work p50 ${p(work, 0.5).toFixed(1)} ms, p95 ${p(work, 0.95).toFixed(1)} ms; to the next frame p50 ${p(frames, 0.5).toFixed(1)} ms, p95 ${p(frames, 0.95).toFixed(1)} ms`);
    if (process.env.TC4_PERF === '1') assert.ok(p(work, 0.95) <= 16, `work p95 ${p(work, 0.95).toFixed(1)} ms over the 16 ms budget`);
  });
});
