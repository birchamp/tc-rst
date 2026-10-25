// C7b step 8, owner ruling H5 (.audit/roadmap/rulings/2026-10-24-H5.md): outside verse text, the
// newest version wins with no question and the older stays in History. A scripted sync brings
// Teammates 1 and 2's changes, from one version, of the first Translation Notes decision of Titus
// and of the first alignment (tc-app backend/memory.rs `Teammates::change_kind`), Teammate 2's
// newer: the toast says tC4's "2 conflicts kept the newest version", the review asks nothing,
// and the Check screen holds Teammate 2's comment.
//
// Then the Check screen's History (HIST-1) lists both versions, and *Restore this version* on
// Teammate 1's brings it back: the Check screen and the decision file hold Teammate 1's comment.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/conflict-kinds.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO, newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };
const opts = { door43: true, scenario: 'shared' };
const FOLDER = 'sample_burrito';
const ONE = 'Comentario del compañero 1.';
const TWO = 'Comentario del compañero 2.';
const toast = (page) => page.getByTestId('sync-result');
const scopeOf = async (env) => (await env.call('app/hello', {})).sessions[0].at;
const list = { tool: 'translationNotes', unit: { type: 'book', value: 'TIT' } };
const checkTab = (page) => page.getByRole('tab', { name: en['nav.check'], exact: true });
const allTools = (page) => page.getByRole('button', { name: /All checking tools/ });
const row = (page, index) => page.locator(`[data-testid="check-list"] [data-rail-index="${index}"]`);

/** The index and comment of every Translation Notes check of Titus that has a comment. */
async function comments(env) {
  const at = await scopeOf(env);
  const found = [];
  for (let index = 0; ; index += 1) {
    const view = await env.call('check/item', { at, list, index }).catch(() => null);
    if (!view) return found;
    if (view.decision.value?.comment) found.push({ index, comment: view.decision.value.comment });
  }
}

browserTest('newest_version_wins_and_history_restores_the_older: a teammate\'s newer check and alignment arrive with no question; the Check screen\'s History restores the older decision, and the decision file holds it', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${FOLDER}`).waitFor();
    await env.call('test/script', {
      request: newRequestId(),
      next: { type: 'sync', value: { project: FOLDER, outcome: { type: 'synced', value: { teammates: null, kinds: ['check', 'alignment'], restored: 0 } } } },
    });
    const jobs = [];
    page.on('response', async (r) => {
      if (!r.url().endsWith('/api/v1/door43/sync')) return;
      const reply = await r.json();
      if (reply.type === 'ok') jobs.push(reply.value);
    });
    await page.getByTestId(`share-${FOLDER}`).click();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
    await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    await toast(page).waitFor({ timeout: 60_000 });
    assert.equal(await toast(page).getAttribute('data-ok'), '1');
    const newest = page.getByTestId('sync-count-newest');
    assert.equal(await newest.getAttribute('data-count'), '2');
    assert.equal(await newest.textContent(), en['syncResult.newestMany'].replace('{n}', '2'));
    assert.equal(await page.getByTestId('sync-count-conflicts').getAttribute('data-count'), '0');
    // No question: no Review link, no conflict chip, and the review's tabs list nothing.
    assert.equal(await toast(page).getByTestId('sync-review').count(), 0);
    assert.equal(await page.getByTestId(`conflicts-${FOLDER}`).count(), 0);
    // What a newer version replaced, the Received sheet's rows (E2 draws that sheet).
    const job = await until('the sync job started', async () => jobs[0]);
    const done = await until('the sync job is done', async () => {
      const view = await env.call('jobs/get', { job });
      return view?.state?.type === 'done' ? view.state.value : null;
    });
    const teammate = (n) => ({ who: { type: 'teammate', value: n }, computer: null });
    assert.deepEqual(done.value.replaced, [
      { by: teammate(2), whose: teammate(1), kind: 'check', place: { type: 'check', value: { tool: 'translationNotes', place: 'TIT 1:4' } }, count: 1 },
      { by: teammate(2), whose: teammate(1), kind: 'alignment', place: { type: 'verse', value: { book: 'TIT', chapter: 1, verse: 1 } }, count: 1 },
    ], 'Teammate 2\'s newer check and alignment replaced Teammate 1\'s');
    await toast(page).getByTestId('sync-dismiss').click();

    await page.getByTestId(`project-${FOLDER}`).getByRole('button', { name: /Titus/ }).click();
    await checkTab(page).waitFor();
    const at = await scopeOf(env);
    for (const tab of ['alignments', 'checks']) {
      const listed = await env.call('conflicts/list', { at, tab });
      assert.equal(listed.entries.value.length, 0, `nothing on the ${tab} tab`);
    }
    assert.equal(await page.locator('[data-testid^="conflict-marker-"]').count(), 0);

    await checkTab(page).click();
    const picker = page.getByTestId('open-translationNotes');
    await picker.or(allTools(page)).first().waitFor();
    if (!(await picker.count())) await allTools(page).click();
    await picker.click();
    await page.getByTestId('check-quote').waitFor();
    const teammates = (await comments(env)).filter((c) => c.comment === ONE || c.comment === TWO);
    assert.deepEqual(teammates.map((c) => c.comment), [TWO], 'one check shows the newest comment');
    const [{ index }] = teammates;
    await row(page, index).click();
    await until(`check ${index} shows`, async () => (await page.locator('[data-testid="check-list"] [aria-current="true"]').getAttribute('data-rail-index')) === String(index));
    assert.equal(await page.getByTestId('comment-toggle').getAttribute('data-written'), '1');
    await page.getByTestId('comment-toggle').click();
    await until('the comment shows', async () => (await page.getByTestId('comment-text').inputValue()) === TWO);
    await page.keyboard.press('Escape');

    // History keeps Teammate 1's older decision, and *Restore this version* brings it back.
    await page.getByTestId('check-history-toggle').click();
    const versions = page.getByTestId('check-history').getByTestId('history-version');
    await until('the check\'s History lists both versions', async () => (await versions.count()) >= 2);
    assert.equal(await versions.first().getAttribute('data-shown'), 'true', 'the newest is shown, first');
    assert.match(await versions.first().textContent(), new RegExp(TWO));
    const older = versions.filter({ hasText: ONE });
    assert.equal(await older.getAttribute('data-shown'), 'false');
    await older.getByTestId('history-restore').click();
    await until('the restored comment shows', async () => (await comments(env)).some((c) => c.comment === ONE));
    await until('History shows the restored version', async () => (await versions.first().textContent()).includes(ONE));
    if (!(await page.getByTestId('comment-text').isVisible())) await page.getByTestId('comment-toggle').click();
    await until('the comment shows the restored text', async () => (await page.getByTestId('comment-text').inputValue()) === ONE);
    await page.keyboard.press('Escape');

    await page.getByTitle(/Switch project/).click();
    await page.getByTestId(`project-${FOLDER}`).waitFor();
    await env.call('settings/viewFromHome', { project: FOLDER });
    const file = JSON.parse(fs.readFileSync(path.join(env.projectsDir, FOLDER, 'ingredients', 'checking', 'translationNotes', 'TIT.json'), 'utf8'));
    const held = file.decisions.map((d) => d.comments).filter((c) => c === ONE || c === TWO);
    assert.deepEqual(held, [ONE], 'the decision file holds the restored comment');
  }, opts);
});
