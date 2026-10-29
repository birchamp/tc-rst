// tC4 #741 on the real app: open Help and guides from the account menu and read how the top
// bar's "translationCore" wordmark is drawn (weight, size, spacing, smoothing, the face that
// loaded), then save a picture of the banner.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/741-help-wordmark.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, openMenu, recordRequests, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');

browserTest('741_help_wordmark_is_drawn_light_and_smooth', async () => {
  await withApp(async ({ page }) => {
    const requests = recordRequests(page);
    await openMenu(page);
    await page.getByTestId('account-help').click();
    await page.getByTestId('help-panel').waitFor();
    await until('the panel stops', async () => (await page.getByTestId('help-panel').evaluate((el) => getComputedStyle(el).transform)) === 'none');
    await page.mouse.move(900, 600);
    const frame = page.frameLocator('[data-testid="help-frame"]');
    await until('the help home', async () => (await frame.getByRole('heading', { level: 1 }).textContent().catch(() => null)) === 'translationCore 4 Help');
    const mark = frame.locator('.wordmark').first();
    const got = await mark.evaluate(async (el) => {
      await document.fonts.ready;
      const s = getComputedStyle(el);
      const faces = [...document.fonts].map((f) => `${f.family} ${f.weight} ${f.status}`);
      return {
        fontFamily: s.fontFamily, fontWeight: s.fontWeight, fontSize: s.fontSize, letterSpacing: s.letterSpacing,
        smoothing: s.webkitFontSmoothing ?? s.getPropertyValue('-webkit-font-smoothing'),
        bundledFaces: faces,
        mulishAvailable: document.fonts.check('600 16px Mulish'),
      };
    });
    console.log('wordmark:', JSON.stringify(got, null, 1));
    const remote = requests.all.map((r) => r.url).filter((u) => !/^https?:\/\/(127\.0\.0\.1|localhost)/.test(u) && !u.startsWith('data:') && !u.startsWith('blob:'));
    console.log('requests to another host:', JSON.stringify(remote));
    await page.waitForTimeout(500);
    await frame.locator('.brand').screenshot({ path: path.join(SHOTS, '741-help-banner.png') });
    assert.equal(got.fontWeight, '600', 'weight 600, not the 800 the tester saw');
    assert.deepEqual(remote, []);
  });
});
