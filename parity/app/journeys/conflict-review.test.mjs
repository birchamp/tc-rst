// J18 keep one version of a verse in conflict (B11, tC4 #679, #680, #724): the UI steps of tC4's
// JOURNEYS.md J18 over a scripted sync in which two teammates changed Titus 1:1 from one
// version (the environment's `teammatesChangeOneVerse`, as B10's toast screens use it). The
// review opens from the sync toast, from Home's chip, from Translate's button and from a
// verse's marker; it shows each version with its owner and date, newest first; a click selects
// one; Keep this version writes one resolution, which a second click never repeats (A-27); and
// leaving writes nothing.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/conflict-review.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO } from '../env-rust.mjs';
import { BIBLE } from '../screens.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };
const opts = { door43: true, scenario: 'shared' };
const FOLDER = 'sample_burrito';
const ONE = 'Pablo, versión del compañero 1.';
const TWO = 'Pablo, versión del compañero 2.';
const toast = (page) => page.getByTestId('sync-result');
const review = (page) => page.getByTestId('conflict-review');
const versions = (page) => page.getByTestId('conflict-version');
const scope = async (env) => (await env.call('app/hello', {})).sessions[0].at;

/** The teammates' versions arrive by a sync from Home, through the question and the sign-in. */
async function teammatesSynced(page, env) {
  await env.teammatesChangeOneVerse(BIBLE);
  await page.getByTestId(`share-${FOLDER}`).click();
  await page.getByTestId('net-confirm').click();
  await page.getByTestId('share-signin').waitFor();
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
  await toast(page).locator('[data-testid="sync-count-conflicts"][data-count="1"]').waitFor({ timeout: 60_000 });
}

/** What the review shows of each version: its owner's label, its owner kind and its text. */
const shown = (page) => versions(page).evaluateAll((els) => els.map((e) => ({
  owner: e.querySelector('[data-testid="conflict-owner"]').textContent,
  kind: e.dataset.owner,
  date: e.querySelector('[data-testid="conflict-date"]').textContent,
  text: e.querySelector('[data-testid="conflict-text"]').textContent,
  pressed: e.getAttribute('aria-pressed'),
})));

/** The bodies of the resolutions the page sends. */
function resolutions(page) {
  const sent = [];
  page.on('request', (r) => {
    if (r.url().endsWith('/api/v1/conflicts/resolve')) sent.push(JSON.parse(r.postData()));
  });
  return sent;
}

browserTest('J18 (#679, #724). The toast\'s Review opens the review and closes the toast; each version shows its teammate and date, newest first; Keep this version writes one resolution however often it is pressed, and the verse, Translate and Home show it', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    await toast(page).getByTestId('sync-review').click();
    await review(page).waitFor();
    await toast(page).waitFor({ state: 'detached' });
    assert.equal(await page.getByTestId('conflict-ref').textContent(), 'Titus 1:1');
    // Translate goes to the book of the first conflict, though the project's first book is Jonah.
    await until('Back names Titus 1', async () => (await page.getByTestId('conflict-back').textContent()) === 'Back to Titus 1');
    const position = page.getByTestId('conflict-position');
    assert.deepEqual([await position.getAttribute('data-n'), await position.getAttribute('data-m')], ['1', '1']);
    assert.equal(await page.getByTestId('conflict-previous').isDisabled(), true);
    assert.equal(await page.getByTestId('conflict-next').isDisabled(), true);
    assert.equal(await review(page).locator('[role="tablist"]').count(), 0, 'tC4\'s one tab: no tab strip with only verse conflicts');
    // Newest first, each labelled as the toast labelled its teammate (#680), coloured as a
    // teammate's, with its date.
    await versions(page).nth(1).waitFor();
    const before = await shown(page);
    assert.deepEqual(before.map((v) => [v.owner, v.kind, v.text]), [
      ['Teammate 2', 'teammate', TWO],
      ['Teammate 1', 'teammate', ONE],
    ]);
    assert.ok(before.every((v) => /\d{4}/.test(v.date) && v.date.includes(' · ')), JSON.stringify(before));
    assert.equal(await page.getByTestId('conflict-keep').isDisabled(), true, 'Keep waits for a choice');
    // History lists every version of the verse, the one Translate shows marked (#78).
    await page.getByTestId('conflict-history').click();
    const history = page.getByTestId('history-version');
    await history.nth(2).waitFor();
    assert.equal(await page.locator('[data-testid="history-version"][data-shown="true"]').count(), 1);
    // A click selects one version; the other fades, and the footer says what Keep does.
    await versions(page).nth(1).click();
    const picked = await shown(page);
    assert.deepEqual(picked.map((v) => v.pressed), ['false', 'true']);
    assert.match(await page.getByTestId('conflict-consequence').textContent(), /Teammate 1’s version replaces the other version/);
    // Two presses: the button is off from the first, and every send is one request (A-27).
    const kept = await versions(page).nth(1).getAttribute('data-ts');
    const { record } = (await env.call('conflicts/list', { at: await scope(env), tab: 'text' })).entries.value[0];
    const sent = resolutions(page);
    await page.getByTestId('conflict-keep').dblclick();
    await page.getByTestId('conflict-none').waitFor({ timeout: 30_000 });
    assert.ok(sent.length >= 1);
    assert.equal(new Set(sent.map((r) => r.request)).size, 1, 'one request however often Keep was pressed');
    assert.equal(sent[0].keep, kept, 'the version clicked is the one kept');
    assert.equal(await page.getByTestId('conflict-error').count(), 0);
    const after = await env.call('conflicts/history', { at: await scope(env), record });
    assert.equal(after.versions.filter((v) => v.cause === 'kept').length, 1, 'one resolution');
    assert.deepEqual(after.versions[0].version.value, { type: 'text', value: ONE });
    // Back: Translate shows the kept text, with no marker and no button.
    await page.getByTestId('conflict-back').click();
    await review(page).waitFor({ state: 'detached' });
    await page.getByText(ONE).first().waitFor();
    assert.equal(await page.locator('[data-testid^="conflict-marker-"]').count(), 0);
    assert.equal(await page.getByTestId('conflicts-review').count(), 0);
    // Home counts no conflict.
    await page.getByTitle(/Switch project/).click();
    await page.getByTestId(`project-${FOLDER}`).waitFor();
    // The card is read again once the leave's checkpoint has written the conflicts file.
    await until('Home counts no conflict', async () => (await page.getByTestId(`conflicts-${FOLDER}`).count()) === 0, 15_000);
  }, opts);
});

browserTest('J18 (#679). Home\'s chip opens the review in Translate; Back leaves the conflict open and writes nothing; Translate\'s button and the verse\'s marker open it again at that verse', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    await toast(page).getByTestId('sync-dismiss').click();
    const chip = page.getByTestId(`conflicts-${FOLDER}`);
    await chip.waitFor();
    assert.equal(await chip.getAttribute('data-count'), '1');
    const sent = resolutions(page);
    await chip.click();
    await review(page).waitFor();
    assert.equal(await page.getByTestId('conflict-back').textContent(), 'Back to Titus 1');
    await versions(page).first().click();
    await page.getByTestId('conflict-back').click();
    await review(page).waitFor({ state: 'detached' });
    assert.deepEqual(sent, [], 'leaving writes nothing');
    const button = page.getByTestId('conflicts-review');
    await button.waitFor();
    assert.equal(await button.getAttribute('data-count'), '1');
    await button.click();
    await versions(page).nth(1).waitFor();
    assert.deepEqual((await shown(page)).map((v) => v.pressed), ['false', 'false'], 'the review opens with nothing selected');
    await page.getByTestId('conflict-back').click();
    const marker = page.getByTestId('conflict-marker-1:1');
    await marker.waitFor();
    assert.match(await marker.textContent(), /2 versions · Review/);
    await marker.click();
    await review(page).waitFor();
    assert.equal(await page.getByTestId('conflict-ref').textContent(), 'Titus 1:1');
    // A mode tab leaves the review; it never shows over another mode.
    await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
    await review(page).waitFor({ state: 'detached' });
    assert.deepEqual(sent, []);
    const at = await scope(env);
    const list = await env.call('conflicts/list', { at, tab: 'text' });
    assert.equal(list.entries.value.length, 1, 'the conflict is still open');
  }, opts);
});
