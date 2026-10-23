// J18 keep one version of a verse in conflict (B11, tC4 #679, #680, #724): the UI steps of tC4's
// JOURNEYS.md J18 over a scripted sync in which two teammates changed Titus 1:1 from one
// version (the environment's `teammatesChangeOneVerse`, as B10's toast screens use it). The
// review opens from the sync toast (which a refused open leaves in place), from Home's chip,
// from Translate's button and from a verse's marker; it shows each version with its owner and
// date, newest first; a click selects one; Keep this version writes one resolution, which a
// second click never repeats (A-27); Previous and Next step between two verses in conflict;
// and leaving writes nothing. Then C7a's choices: Keep Teammate 1's passage after a scripted
// join of 1:4-5 against this computer's change of 1:5, Restore this version from History, and
// Keep all of Teammate 1's over two conflicts, each writing once however often it is pressed.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/conflict-review.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO, newRequestId } from '../env-rust.mjs';
import { BIBLE } from '../screens.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };
const opts = { door43: true, scenario: 'shared' };
const FOLDER = 'sample_burrito';
const ONE = 'Pablo, versión del compañero 1.';
const TWO = 'Pablo, versión del compañero 2.';
const MINE_FIVE = 'Por esta causa te dejé en Creta: mi versión del versículo cinco.';
const toast = (page) => page.getByTestId('sync-result');
const review = (page) => page.getByTestId('conflict-review');
const versions = (page) => page.getByTestId('conflict-version');
const scope = async (env) => (await env.call('app/hello', {})).sessions[0].at;

/** The teammates' versions arrive by a sync from Home, through the question and the sign-in. */
async function teammatesSynced(page, env) {
  await env.teammatesChangeOneVerse(BIBLE);
  await syncedFromHome(page);
}

/** A sync from Home's share button, through the internet question and the sign-in, until the
 * toast counts one conflict. */
async function syncedFromHome(page) {
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

/** The bodies of the requests the page sends to `conflicts/<name>`. */
function requests(page, name) {
  const sent = [];
  page.on('request', (r) => {
    if (r.url().endsWith(`/api/v1/conflicts/${name}`)) sent.push(JSON.parse(r.postData()));
  });
  return sent;
}

/** The bodies of the resolutions the page sends. */
const resolutions = (page) => requests(page, 'resolve');

const translateTab = (page) => page.getByRole('tab', { name: en['nav.draft'], exact: true });

/** Titus from Home's card, in Translate. */
async function openTitus(page) {
  await page.getByTestId(`project-${FOLDER}`).getByRole('button', { name: /Titus/ }).click();
  await translateTab(page).waitFor();
}

/** Verse `n` of Titus 1 saved as `text` through the verse editor. */
async function saveVerse(page, n, text) {
  await translateTab(page).click();
  await page.getByRole('tab', { name: 'Verse', exact: true }).click();
  await page.getByTitle(en['draft.editVerse']).nth(n - 1).click();
  const box = page.getByLabel(`Verse ${n}`, { exact: true });
  await box.waitFor();
  await box.fill(text);
  await page.getByRole('button', { name: 'Save verse' }).click();
  await page.locator('[data-testid="save-indicator"][data-state="saved"]').waitFor({ timeout: 30_000 });
}

async function backHome(page) {
  await page.getByTitle(/Switch project/).click();
  await page.getByTestId(`project-${FOLDER}`).waitFor();
}

browserTest('J18 (#679, #724). A refused open keeps the sync toast and its Review; the next Review opens the review and closes the toast; each version shows its teammate and date, newest first; Keep this version writes one resolution however often it is pressed, and the verse, Translate and Home show it', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    // A failed open keeps the toast and its links (#724): the open refused once, as the open
    // refuses a project file another program left unreadable (the gallery's home-open-error).
    const refusal = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'view-gallery', 'replies', 'home-open-error.json'), 'utf8')).replies[0].reply;
    await page.route('**/api/v1/session/open', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(refusal) }), { times: 1 });
    await toast(page).getByTestId('sync-review').click();
    await page.getByTestId('home-open-error').waitFor();
    assert.equal(await review(page).count(), 0);
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
    assert.deepEqual(sent[0].choice, { type: 'candidate', value: { change: kept } }, 'the version clicked is the one kept');
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

browserTest('J18 step 6. With two verses in conflict, Previous and Next step between them, each shown with nothing selected; once one is kept, the other shows and the count goes down', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    await toast(page).getByTestId('sync-dismiss').click();
    // A second sync brings the teammates' two versions of Titus 1:2.
    const later = ['Con la esperanza, versión del compañero 1.', 'Con la esperanza, versión del compañero 2.'];
    await env.call('test/script', {
      request: newRequestId(),
      next: { type: 'sync', value: { project: env.projectId(BIBLE), outcome: { type: 'synced', value: { teammates: { book: 'TIT', chapter: 1, verse: 2, texts: later }, restored: 0 } } } },
    });
    await page.getByTestId(`share-${FOLDER}`).click();
    await toast(page).getByTestId('sync-review').click();
    await review(page).waitFor();
    const position = page.getByTestId('conflict-position');
    const at = async () => [await page.getByTestId('conflict-ref').textContent(), await position.getAttribute('data-n'), await position.getAttribute('data-m')];
    const pressed = async () => (await shown(page)).map((v) => v.pressed);
    await versions(page).nth(1).waitFor();
    assert.deepEqual(await at(), ['Titus 1:1', '1', '2']);
    assert.equal(await page.getByTestId('conflict-previous').isDisabled(), true);
    await versions(page).first().click();
    await page.getByTestId('conflict-next').click();
    await until('Next shows Titus 1:2', async () => (await page.getByTestId('conflict-ref').textContent()) === 'Titus 1:2');
    assert.deepEqual(await at(), ['Titus 1:2', '2', '2']);
    assert.deepEqual((await shown(page)).map((v) => v.text), [later[1], later[0]]);
    assert.deepEqual(await pressed(), ['false', 'false'], 'a step clears the choice');
    assert.equal(await page.getByTestId('conflict-next').isDisabled(), true);
    await page.getByTestId('conflict-previous').click();
    await until('Previous shows Titus 1:1', async () => (await page.getByTestId('conflict-ref').textContent()) === 'Titus 1:1');
    assert.deepEqual(await pressed(), ['false', 'false']);
    assert.deepEqual((await shown(page)).map((v) => v.text), [TWO, ONE]);
    // Keep Teammate 1's Titus 1:2: Titus 1:1 is the one left, now 1 of 1.
    await page.getByTestId('conflict-next').click();
    await until('Next shows Titus 1:2 again', async () => (await page.getByTestId('conflict-ref').textContent()) === 'Titus 1:2');
    const sent = resolutions(page);
    await versions(page).nth(1).click();
    await page.getByTestId('conflict-keep').click();
    await until('the other conflict shows', async () => (await position.getAttribute('data-m')) === '1', 30_000);
    assert.deepEqual(await at(), ['Titus 1:1', '1', '1']);
    assert.equal(sent.length, 1);
    const list = await env.call('conflicts/list', { at: await scope(env), tab: 'text' });
    assert.equal(list.entries.value.length, 1, 'Titus 1:1 is still in conflict');
  }, opts);
});

browserTest('C7a. A teammate joined Titus 1:4-5 on Door43 while this computer changed 1:5: the review lists one passage conflict with a button per side; Keep Teammate 1’s passage writes one resolution over both verses however often it is pressed, and Translate then holds 4-5 as one verse', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await openTitus(page);
    await saveVerse(page, 5, MINE_FIVE);
    await backHome(page);
    await env.teammatesJoinVerses(BIBLE);
    await syncedFromHome(page);
    await toast(page).getByTestId('sync-review').click();
    await review(page).waitFor();
    assert.equal(await page.getByTestId('conflict-ref').textContent(), 'Titus 1:4');
    await versions(page).nth(2).waitFor();
    const listed = await shown(page);
    assert.deepEqual(listed.map((v) => v.owner).sort(), ['Mine', 'Teammate 1', 'Teammate 1'], JSON.stringify(listed));
    assert.ok(listed.some((v) => v.text === MINE_FIVE), 'my verse 5 is one side');
    assert.ok(listed.some((v) => v.text.includes('a Tito') && v.text.includes('Por esta causa')), 'the joined text is the other');
    // One button per side; no version is clickable, each side is kept whole.
    const sides = page.getByTestId('conflict-keep-passage');
    assert.deepEqual((await sides.evaluateAll((els) => els.map((e) => e.textContent))).sort(), ['Keep Teammate 1’s passage', 'Keep my passage']);
    assert.equal(await page.getByTestId('conflict-keep').count(), 0);
    const at = await scope(env);
    const sent = resolutions(page);
    await page.locator('[data-testid="conflict-keep-passage"][data-who="Teammate(1)"]').dblclick();
    await page.getByTestId('conflict-none').waitFor({ timeout: 30_000 });
    assert.ok(sent.length >= 1);
    assert.equal(new Set(sent.map((r) => r.request)).size, 1, 'one request however often the button was pressed');
    assert.equal(sent[0].choice.type, 'passage');
    assert.equal(sent[0].review.length, 2, 'one token per verse of the passage');
    assert.equal(await page.getByTestId('conflict-error').count(), 0);
    const text = await env.call('editor/chapters', { at, book: 'TIT' });
    const keys = text.chapters[0].verses.map((v) => v.key);
    assert.ok(keys.includes('4-5') && !keys.includes('5'), `4 and 5 are one verse: ${keys.join(' ')}`);
    assert.equal(text.conflicts, 0);
  }, opts);
});

browserTest('C7a (#78). Every History row but the one shown has Restore this version; a double press writes one restore, the restored text is the one kept, and the replaced version stays in History', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    await toast(page).getByTestId('sync-review').click();
    await review(page).waitFor();
    await versions(page).nth(1).waitFor();
    await page.getByTestId('conflict-history').click();
    const rows = page.getByTestId('history-version');
    await rows.nth(2).waitFor();
    assert.equal(await page.getByTestId('history-restore').count(), 2, 'every row but the shown one');
    assert.equal(await page.locator('[data-testid="history-version"][data-shown="true"] [data-testid="history-restore"]').count(), 0);
    const at = await scope(env);
    const { record } = (await env.call('conflicts/list', { at, tab: 'text' })).entries.value[0];
    // Teammate 1's version, the one not shown: restored over the shown one, it is the text kept.
    const restore = rows.nth(1).getByTestId('history-restore');
    const from = await restore.getAttribute('data-ts');
    const sent = requests(page, 'restore');
    await restore.dblclick();
    await page.getByTestId('conflict-none').waitFor({ timeout: 30_000 });
    assert.ok(sent.length >= 1);
    assert.equal(new Set(sent.map((r) => r.request)).size, 1, 'one request however often Restore was pressed');
    assert.equal(sent[0].from, from);
    assert.equal(await page.getByTestId('conflict-error').count(), 0);
    const after = await env.call('conflicts/history', { at, record });
    assert.equal(after.versions[0].cause, 'restored');
    assert.deepEqual(after.versions[0].version.value, { type: 'text', value: ONE });
    assert.ok(after.versions.some((v) => v.cause === 'edited' && v.version.value.value === TWO), 'the replaced version stays in History');
    await page.getByTestId('conflict-back').click();
    await review(page).waitFor({ state: 'detached' });
    await page.getByText(ONE).first().waitFor();
  }, opts);
});

browserTest('C7a. With two verses in conflict, Keep all of Teammate 1’s keeps their version in both with one request however often it is pressed, and says how many were left', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await teammatesSynced(page, env);
    await toast(page).getByTestId('sync-dismiss').click();
    const later = ['Con la esperanza, versión del compañero 1.', 'Con la esperanza, versión del compañero 2.'];
    await env.call('test/script', {
      request: newRequestId(),
      next: { type: 'sync', value: { project: env.projectId(BIBLE), outcome: { type: 'synced', value: { teammates: { book: 'TIT', chapter: 1, verse: 2, texts: later }, restored: 0 } } } },
    });
    await page.getByTestId(`share-${FOLDER}`).click();
    await toast(page).getByTestId('sync-review').click();
    await review(page).waitFor();
    await versions(page).nth(1).waitFor();
    assert.equal(await page.getByTestId('conflict-position').getAttribute('data-m'), '2');
    const bulk = page.locator('[data-testid="conflict-keep-all"][data-who="Teammate(1)"]');
    await bulk.waitFor();
    assert.equal(await bulk.textContent(), 'Keep all of Teammate 1’s');
    const sent = requests(page, 'resolveBulk');
    await bulk.dblclick();
    const said = page.getByTestId('conflict-kept-all');
    await said.waitFor({ timeout: 30_000 });
    assert.ok(sent.length >= 1);
    assert.equal(new Set(sent.map((r) => r.request)).size, 1, 'one request however often the button was pressed');
    assert.deepEqual(sent[0].person, { type: 'teammate', value: 1 });
    assert.equal(sent[0].conflicts.length, 2);
    assert.match(await said.textContent(), /Teammate 1’s version was kept in 2 conflicts\. 0 are left/);
    await page.getByTestId('conflict-none').waitFor();
    const list = await env.call('conflicts/list', { at: await scope(env), tab: 'text' });
    assert.equal(list.entries.value.length, 0);
    const text = await env.call('editor/chapters', { at: await scope(env), book: 'TIT' });
    const texts = Object.fromEntries(text.chapters[0].verses.map((v) => [v.key, v.text]));
    assert.equal(texts['1'], ONE);
    assert.equal(texts['2'], later[0]);
  }, opts);
});
