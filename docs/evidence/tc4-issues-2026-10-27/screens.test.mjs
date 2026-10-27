// Screenshots for docs/evidence/tc4-issues-2026-10-27/fixed.md, from the real app on the sample
// project (Titus). Not in the gate: the node step globs parity/app/journeys only.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test docs/evidence/tc4-issues-2026-10-27/screens.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { browserTest, until, withApp } from '../../../parity/app/journeys/kit.mjs';

const OUT = path.join(path.dirname(fileURLToPath(import.meta.url)), 'screens');

async function openCheck(page) {
  await page.getByTestId('project-sample_burrito').getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('preflight-translationNotes').waitFor();
}

// tC4 #769: tN sorted By figure, then Fold all keeps only the active check's group open.
browserTest('tc4_769_fold_all_screens', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    await page.getByTestId('sort-byFigure').click();
    const headings = page.getByTestId('rail-group');
    await headings.first().waitFor();
    await page.screenshot({ path: path.join(OUT, '769-by-figure-open.png') });
    const foldAll = page.getByTestId('rail-fold-all');
    await foldAll.click();
    await until('Unfold all shows', async () => (await foldAll.textContent()) === 'Unfold all');
    const open = await headings.evaluateAll((els) => els.filter((el) => el.getAttribute('aria-expanded') === 'true').length);
    assert.equal(open, 1, 'only the active group stays open');
    await page.screenshot({ path: path.join(OUT, '769-fold-all.png') });
  });
});

// tC4 #790: Titus 1:1 opens in Align with one card for each of the 17 UGNT words.
browserTest('tc4_790_titus_1_1_cards_screen', async () => {
  await withApp(async ({ page }) => {
    await openCheck(page);
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    await page.screenshot({ path: path.join(OUT, '790-titus-1-1-align.png'), fullPage: true });
    // A merged card draws its words in one span, so count words, not spans.
    const spans = await page.locator('[data-testid^="align-card-"] span[lang="el"]').allTextContents();
    console.log(`# cards: ${JSON.stringify(spans)}`);
    const words = spans.flatMap((t) => t.trim().split(/\s+/)).filter(Boolean);
    assert.equal(words.length, 17, 'Titus 1:1 shows all 17 UGNT words');
  });
});
