// tC4 #568 repro on the real app, keyboard only: the issue's three steps (place a word, merge
// two cards, confirm one suggestion). Each step is tried with Tab, Enter and Space alone.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/568-keyboard-align.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();
const placed = (page) => page.getByTestId('align-progress').textContent();
const cardCount = (page) => page.locator('[data-testid^="align-card-"]').count();
const active = (page) => page.evaluate(() => {
  const a = document.activeElement;
  return a ? `${a.tagName.toLowerCase()} ${a.getAttribute('data-testid') ?? ''} ${(a.textContent || '').trim().slice(0, 30)}` : null;
});
/** Where the focus goes on each of `n` Tab presses from the focused element (Shift+Tab with
 * `back`), with no stop on a repeat. */
async function tabStops(page, n = 80, back = false) {
  const seen = [];
  for (let i = 0; i < n; i += 1) {
    await page.keyboard.press(back ? 'Shift+Tab' : 'Tab');
    seen.push(await active(page));
  }
  return seen;
}

browserTest('568_keyboard_only_align', async () => {
  await withApp(async ({ page }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();

    // 1. Place a word: Enter on a bank word, Tab to a card, Enter.
    const start = await placed(page);
    await chip(page, 'fe').focus();
    await page.keyboard.press('Enter');
    const stops = [...await tabStops(page), ...(await chip(page, 'fe').focus(), await tabStops(page, 80, true))];
    const cardStops = stops.filter((s) => / align-card-\d+ /.test(s));
    console.log('step 1: card Tab stops while a word is chosen:', cardStops.length, '| tabindex of the cards:', JSON.stringify(await page.locator('[data-testid^="align-card-"]').evaluateAll((cs) => [...new Set(cs.map((c) => c.getAttribute('tabindex')))])), '| Tab walk:', JSON.stringify([...new Set(stops)]));
    await card(page, 'πίστιν').focus();
    await page.keyboard.press('Enter');
    await until('placed', async () => (await placed(page)) !== start);
    console.log('step 1: placed', start, '->', await placed(page));

    // 2. Merge two cards: is any merge control reachable by Tab, and does a key on a card header merge?
    const cards0 = await cardCount(page);
    await chip(page, 'piedad').focus();
    const idle = [...new Set([...await tabStops(page, 120), ...await tabStops(page, 120, true)])];
    const merge = idle.filter((s) => /merge|join/i.test(s));
    const headers = await page.locator('[data-testid^="align-card-"] [draggable="true"]').evaluateAll((hs) => hs.map((h) => h.tabIndex));
    console.log('step 2: Tab stops with nothing chosen:', idle.length, '| merge controls:', JSON.stringify(merge),
      '| card headers (draggable) tabIndex:', JSON.stringify([...new Set(headers)]));
    for (const key of ['Enter', ' ', 'Control+ArrowRight', 'Alt+ArrowRight']) {
      await card(page, 'πίστιν').focus();
      await page.keyboard.press(key);
    }
    await page.waitForTimeout(500);
    console.log('step 2: cards before', cards0, '| after keys on a card:', await cardCount(page));

    // 3. Confirm one suggestion from the keyboard.
    await page.locator('[data-testid="align-verse-list"] [data-ref="1:2"]').click();
    await until('verse 1:2', async () => (await page.getByTestId('align-item-counter').textContent()) === 'Item 2 of 5');
    const sw = page.getByTestId('align-suggest-switch');
    await sw.focus();
    await page.keyboard.press('Space');
    await until('ready', async () => (await page.getByTestId('align-suggestions').getAttribute('data-status')) === 'ready');
    await page.getByTestId('align-suggest').focus();
    await page.keyboard.press('Enter');
    await until('proposals stand', async () => (await page.locator('[data-testid^="align-suggested-"]').count()) > 0);
    const n = await page.locator('[data-testid^="align-suggested-"]').count();
    await page.getByTestId('align-suggest').focus();
    const withSuggestions = [...await tabStops(page, 120), ...await tabStops(page, 120, true)];
    const chipStops = withSuggestions.filter((s) => / align-suggested-\d+ /.test(s));
    const focusable = await page.locator('[data-testid^="align-suggested-"]').evaluateAll(
      (cs) => cs.map((c) => [c.tabIndex, ...[...c.querySelectorAll('*')].map((e) => e.tabIndex)].some((t) => t >= 0)));
    console.log('step 3: suggestion chips', n, '| Tab stops on a chip:', chipStops.length, '| any focusable part:', JSON.stringify(focusable));
    await page.screenshot({ path: path.join(SHOTS, '568-suggestions-standing.png'), fullPage: true });

    assert.ok(cardStops.length > 0, 'cards are Tab stops while a word is chosen');
    assert.equal(merge.length, 0, 'no merge control from the keyboard');
    assert.equal(chipStops.length, 0, 'a single suggestion cannot be reached by Tab');
  });
});
