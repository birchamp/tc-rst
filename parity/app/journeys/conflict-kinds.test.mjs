// C7b: one resolution per conflict kind through the UI, each group of kinds as the owner
// signed it off in the H5 packet (docs/evidence/q9-2026-10-24.md). Teammates' changes of each
// kind arrive by a scripted sync from a fake Door43 (`teammatesChangeKinds`); the review opens
// from Home's chip, the tab that holds them is chosen, and each choice, however often it is
// pressed, sends one resolution and settles the conflict.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/conflict-kinds.test.mjs
import assert from 'node:assert/strict';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { BIBLE } from '../screens.mjs';
import { browserTest, catalog, withApp } from './kit.mjs';

const en = catalog('en');
const opts = { door43: true, scenario: 'shared' };
const FOLDER = 'sample_burrito';
/** The Translation Notes check the scripted teammates change: Titus 1:4, “Grace and peace”. */
const CHECK = 'gr8c';

/** Teammates' changes of `kinds` arrive by a sync from Home, through the internet question
 * and the sign-in; the toast's Review opens the review. */
async function reviewKinds(page, env, kinds) {
  await page.getByTestId(`share-${FOLDER}`).waitFor();
  await env.teammatesChangeKinds(BIBLE, kinds);
  await page.getByTestId(`share-${FOLDER}`).click();
  await page.getByTestId('net-confirm').click();
  await page.getByTestId('share-signin').waitFor();
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
  // A check's conflict shows its newest version until someone reviews it, so neither the
  // toast nor Home offers Review (they count verses, as tC4's do): the decision's "2 versions"
  // marker on the Check screen opens it.
  await page.getByTestId('sync-result').getByTestId('sync-dismiss').click({ timeout: 60_000 });
  await page.getByTestId(`project-${FOLDER}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
  await page.getByTestId('open-translationNotes').click();
  await page.getByTestId('check-progress').waitFor();
  await page.locator(`[data-testid="check-list"] [data-check-id="${CHECK}"]`).first().click();
  await page.getByTestId('check-conflict-marker').click();
  await page.getByTestId('conflict-review').waitFor();
  await page.locator('[data-testid="review-tab-checks"][aria-selected="true"]').waitFor();
}

/** The bodies of the resolutions the page sends. */
function resolutions(page) {
  const sent = [];
  page.on('request', (r) => {
    if (r.url().endsWith('/api/v1/conflicts/resolve')) sent.push(JSON.parse(r.postData()));
  });
  return sent;
}

/** `button` pressed twice: one resolution with `type`, and the tab has nothing left. */
async function resolvesOnce(page, button, type) {
  const sent = resolutions(page);
  await button.dblclick();
  await page.getByTestId('conflict-none').waitFor({ timeout: 30_000 });
  assert.ok(sent.length >= 1);
  assert.equal(new Set(sent.map((r) => r.request)).size, 1, 'one request however often it was pressed');
  assert.equal(sent[0].choice.type, type);
  assert.equal(await page.getByTestId('conflict-error').count(), 0);
}

browserTest('C7b group (a), one_resolution_per_kind_through_the_ui. A check: Keep Teammate 2’s, Choose per line and Keep both are offered, and Choose per line keeps the comment picked; a moved check: Apply Teammate 2’s change to the moved check', async () => {
  await withApp(async ({ page, env }) => {
    await reviewKinds(page, env, ['check']);
    const keeps = page.getByTestId('conflict-keep-check');
    assert.deepEqual(await keeps.allTextContents(), ['Keep Teammate 2’s', 'Keep Teammate 1’s']);
    await page.getByTestId('conflict-keep-both').waitFor();
    await page.getByTestId('conflict-per-line').click();
    const keep = page.getByTestId('conflict-per-line-keep');
    assert.ok(await keep.isDisabled(), 'off until every part has a version');
    await page.getByTestId('conflict-part').getByText('Use Teammate 1’s').click();
    await page.locator('[data-testid="conflict-part-pick"][aria-pressed="true"]').waitFor();
    await resolvesOnce(page, keep, 'perUnit');
  }, opts);
  await withApp(async ({ page, env }) => {
    await reviewKinds(page, env, ['checkMoved']);
    await page.getByTestId('conflict-keep-where-it-was').waitFor();
    const apply = page.getByTestId('conflict-apply-to-moved');
    assert.equal(await apply.textContent(), 'Apply Teammate 2’s change to the moved check');
    await resolvesOnce(page, apply, 'applyToMovedCheck');
  }, opts);
});
