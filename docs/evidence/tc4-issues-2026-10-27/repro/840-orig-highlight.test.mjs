// tC4 #840 repro (report only, outside the gate's glob): the Greek tab of Translate marks a
// current card's quote at its occurrence, and nothing with no current card; and whether a Hebrew
// verse (Jonah, en-pins serves hbo_uhb) can get a current note or word card at all.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app ... tc-heavy node --test docs/evidence/tc4-issues-2026-10-27/repro/840-orig-highlight.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';

browserTest('840_greek_tab_plain_without_a_card_and_second_occurrence_marks_the_second', async () => {
  await withApp(async ({ page }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
    await page.getByTestId('source-tab-orig').click();
    const v1 = page.getByTestId('draft-source-text').first();
    await until('the Greek text', async () => (await v1.getAttribute('lang')) === 'el');
    // Move the mouse off every card: no current card.
    await page.mouse.move(2, 2);
    await page.waitForTimeout(500);
    const plain = await page.getByTestId('source-hl').count();
    console.log('marks with no current card:', plain);
    assert.equal(plain, 0, 'with no current note the Greek pane is plain');

    // The Words tab: TIT 1:1 has two links to kt/god, Θεοῦ occurrence 1 and occurrence 2.
    await page.getByRole('tab', { name: 'Words', exact: true }).click();
    const gods = page.locator('[data-testid="helps-word"][data-frame="1"]').filter({ hasText: 'of God' });
    await page.getByTestId('helps-word').first().waitFor();
    console.log('word cards:', JSON.stringify(await page.getByTestId('helps-word').evaluateAll((els) => els.slice(0, 14).map((e) => [e.dataset.frame, e.innerText.replace(/\n/g, ' / ')]))));
    await until('two god cards in 1:1', async () => (await gods.count()) >= 2, 5_000);
    const at = async () => v1.evaluate((el) => {
      const words = (el.textContent.match(/\p{L}+/gu) ?? []);
      const marks = [...el.querySelectorAll('[data-testid="source-hl"]')].map((m) => m.textContent);
      // Word index of each mark among the verse's words.
      const idx = [];
      let n = 0;
      const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
      for (let t = walker.nextNode(); t; t = walker.nextNode()) {
        const ws = t.textContent.match(/\p{L}+/gu) ?? [];
        const inMark = !!t.parentElement.closest('[data-testid="source-hl"]');
        for (const w of ws) { if (inMark) idx.push([n, w]); n++; }
      }
      return { words: words.length, marks, idx };
    });
    await gods.nth(0).hover();
    await until('first Θεοῦ marked', async () => (await page.getByTestId('source-hl').count()) > 0);
    const first = await at();
    await gods.nth(1).hover();
    await until('the mark moves', async () => JSON.stringify((await at()).idx) !== JSON.stringify(first.idx));
    const second = await at();
    console.log('occurrence 1 marks:', JSON.stringify(first.idx), 'occurrence 2 marks:', JSON.stringify(second.idx));
    await page.screenshot({ path: path.join(SHOTS, '840-orig-highlight.png') });
    assert.deepEqual(first.idx.map(([, w]) => w), ['Θεοῦ']);
    assert.deepEqual(second.idx.map(([, w]) => w), ['Θεοῦ']);
    assert.ok(second.idx[0][0] > first.idx[0][0], 'the second occurrence marks the later Θεοῦ, not the first');
  }, { scenario: 'en-pins' });
});

browserTest('840_hebrew_jonah_has_a_card_to_mark', async () => {
  await withApp(async ({ page }) => {
    await page.getByTestId(`project-${BIBLE}`).getByTestId('book-tile-JON').click();
    await page.getByTestId('mode-switch').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByTestId('source-tab-orig').click();
    const v1 = page.getByTestId('draft-source-text').first();
    await until('the Hebrew text', async () => (await v1.getAttribute('lang')) === 'hbo');
    await page.waitForTimeout(1500);
    const tabs = await page.getByTestId('helps-panel').getByRole('tab').allTextContents();
    const counts = {};
    for (const name of tabs) {
      await page.getByTestId('helps-panel').getByRole('tab', { name, exact: true }).click();
      await page.waitForTimeout(500);
      counts[name] = await page.locator('[data-testid="helps-note"], [data-testid="helps-word"]').count();
    }
    console.log('Jonah helps tabs and card counts:', JSON.stringify(counts));
    console.log('Jonah helps panel text:', (await page.getByTestId('helps-panel').innerText()).slice(0, 300).replace(/\n/g, ' | '));
    const card = page.locator('[data-testid="helps-note"], [data-testid="helps-word"]').first();
    if (await card.count()) {
      await card.hover();
      await page.waitForTimeout(800);
      console.log('Hebrew marks:', JSON.stringify(await page.getByTestId('source-hl').allTextContents()));
    }
    await page.screenshot({ path: path.join(SHOTS, '840-hebrew-jonah.png') });
  }, { scenario: 'en-pins' });
});
