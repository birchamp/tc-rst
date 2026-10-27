// Sync with Door43 from a Home card (B10): the halves of tC4's e2e/sync-button.spec.ts that the
// scripted sync reaches, and the closed set of the refusals a sync can end with, each drawn by
// the toast through the app. The sync's own steps, and this spec's ground truth on Door43 main,
// are E1a's. tc4 signs in to the fake Door43 on loopback, whose control port tells what Door43
// received; the sync's outcome is what `test/script` set (the environment's Door43 steps).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/sync-button.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { DOOR43_OWNER, REPO, newRequestId } from '../env-rust.mjs';
import { BIBLE } from '../screens.mjs';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };
const opts = { door43: true, scenario: 'shared' };
const FOLDER = 'sample_burrito';
const button = (page) => page.getByTestId(`share-${FOLDER}`);
const toast = (page) => page.getByTestId('sync-result');

async function syncSignedIn(page) {
  await button(page).click();
  await page.getByTestId('net-confirm').click();
  await page.getByTestId('share-signin').waitFor();
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
}

async function dismiss(page) {
  await toast(page).getByTestId('sync-dismiss').click();
  await toast(page).waitFor({ state: 'detached' });
}

browserTest('#678. Nothing goes to Door43 at launch or after 5 s idle; one press asks, signs in and syncs once, and the result shows until it is dismissed', async () => {
  await withApp(async ({ page, env }) => {
    await button(page).waitFor();
    await sleep(5_000);
    assert.deepEqual(await env.door43.requests(), [], 'no Door43 request at launch or while idle');
    assert.equal(await page.getByTestId(`share-card-${FOLDER}`).getAttribute('data-action'), 'sync');
    await env.pushToDoor43(BIBLE);
    await syncSignedIn(page);
    await toast(page).waitFor();
    assert.equal(await page.getByTestId('share-signin').count(), 0, 'the sign-in step closed for the result');
    assert.equal(await toast(page).getAttribute('data-ok'), '1');
    for (const id of ['sync-count-received', 'sync-count-newest', 'sync-count-conflicts']) {
      assert.equal(await page.getByTestId(id).getAttribute('data-count'), '0', id);
    }
    assert.equal(await button(page).textContent(), en['home.sync']);
    assert.equal(await button(page).isDisabled(), false);
    await dismiss(page);
    // Another press with nothing new on Door43 asks nothing more: the internet is on and the
    // sign-in held. Until unit X, a sync nobody scripted is not available, and says so plainly.
    await button(page).click();
    await toast(page).waitFor();
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    assert.equal(await toast(page).getAttribute('data-ok'), '0');
    assert.equal(await page.getByTestId('sync-message').textContent(), en['syncResult.error.unexpected']);
  }, opts);
});

/** Every refusal a sync can end with, as tc-api writes it: each block, and each step's refusal
 * before and after the teammates' work was received (tc-api's `SyncFailure`, whose variants the
 * host test `every_sync_refusal_shows_its_own_sentence_never_door43s_words_or_a_path` matches
 * with no catch-all). */
const DOOR43 = 'protected branch hook declined';
const PATH = '/home/ana/translationCore4/projects/sample_burrito/.git/index.lock';
const STEPS = [
  { type: 'offline' }, { type: 'notSignedIn' }, { type: 'authFailed' },
  { type: 'noWriteAccess', value: { serverMessage: DOOR43 } },
  { type: 'refRejected', value: { serverMessage: DOOR43 } },
  { type: 'busy', value: { attempts: 6 } }, { type: 'remoteMissing' }, { type: 'unreachable' },
  { type: 'readOnly' }, { type: 'io', value: { reason: `${PATH}: No space left on device (os error 28)` } },
];
const BLOCKS = [
  { type: 'notShared' }, { type: 'projectMismatch' },
  { type: 'needsNewerApp', value: { major: 3, minReaderMinor: 0 } }, { type: 'tc4TeamRepository' },
];
const FAILURES = [
  ...BLOCKS.map((value) => ({ type: 'blocked', value })),
  ...STEPS.map((value) => ({ type: 'beforeIntake', value })),
  ...STEPS.map((value) => ({ type: 'afterIntake', value })),
];

browserTest('#678, #724 item 6. Every refusal a sync can end with shows its own sentence and code on the toast, never Door43\'s words or a path', async () => {
  await withApp(async ({ page, env }) => {
    const script = (outcome) => env.call('test/script', { request: newRequestId(), next: { type: 'sync', value: { project: FOLDER, outcome } } });
    await script({ type: 'synced', value: { teammates: null, restored: 0 } });
    await syncSignedIn(page);
    await toast(page).waitFor();
    await dismiss(page);
    const seen = new Map();
    for (const failure of FAILURES) {
      await script({ type: 'failed', value: failure });
      await button(page).click();
      await toast(page).waitFor();
      const label = JSON.stringify(failure);
      assert.equal(await toast(page).getAttribute('data-ok'), '0', label);
      assert.equal(await toast(page).getAttribute('role'), 'alert', label);
      assert.equal(await page.getByTestId('sync-count-received').count(), 0, label);
      const text = await page.getByTestId('sync-message').textContent();
      assert.ok(text && !text.includes('{') && !Object.keys(en).includes(text), `${label}: ${text}`);
      for (const words of [DOOR43, '/home/', 'os error']) assert.ok(!text.includes(words), `${label}: ${text}`);
      const code = await toast(page).getAttribute('data-code');
      if (code && seen.has(text)) assert.equal(seen.get(text), code, `two codes read ${text}`);
      if (code) seen.set(text, code);
      await dismiss(page);
    }
    assert.ok(seen.size >= 6, `${seen.size} coded sentences`);
    await until('no sync runs', async () => !(await env.busy(page)));
    assert.equal(await button(page).isDisabled(), false);
  }, opts);
});

// E1a: the sync for real, on the fake Door43. The shared project is connected to an empty
// repository of the fake's user; a press pushes it and the toast says synced; a second press
// syncs again with nothing new; a connection cut during the fetch fails before anything came in.
browserTest('E1a. A real sync pushes the project to Door43; a cut connection fails and changes nothing', async () => {
  await withApp(async ({ page, env }) => {
    await button(page).waitFor();
    await env.pushToDoor43(BIBLE);
    await env.door43.clear();
    await syncSignedIn(page);
    await toast(page).waitFor();
    assert.equal(await toast(page).getAttribute('data-ok'), '1', await toast(page).textContent());
    const pushed = (await env.door43.requests()).filter((r) => r.path.endsWith('/git-receive-pack'));
    assert.ok(pushed.length > 0, 'the project went to Door43');
    await dismiss(page);
    await button(page).click();
    await toast(page).waitFor();
    assert.equal(await toast(page).getAttribute('data-ok'), '1', 'a second sync with nothing new');
    await dismiss(page);
    await env.door43.fault({ method: 'GET', path: `/${DOOR43_OWNER}/${env.door43Repo(BIBLE)}.git/info/refs`, times: 1, fault: { type: 'breakOff', after: 0 } });
    await button(page).click();
    await toast(page).waitFor();
    assert.equal(await toast(page).getAttribute('data-ok'), '0', 'the cut sync failed');
    assert.notEqual(await page.getByTestId('sync-message').textContent(), en['syncResult.error.keptStopped'], 'nothing came in, so nothing is said to be kept');
  }, { ...opts, realSync: true });
});

// D88 point 6, #724: a sync whose project now pins versions this computer lacks offers Download;
// it opens the project and its Source texts, and the toast closes once they are open.
browserTest('E1a. Download on the sync toast opens the project\'s Source texts and then closes the toast', async () => {
  await withApp(async ({ page, env }) => {
    await button(page).waitFor();
    await env.pushToDoor43(BIBLE);
    await syncSignedIn(page);
    await toast(page).waitFor();
    assert.equal(await page.getByTestId('sync-download').count(), 1, 'Download is offered');
    await page.getByTestId('sync-download').click();
    await page.getByTestId('sources-modal').waitFor();
    // The toast is drawn only on Home: back there, it is gone.
    await page.keyboard.press('Escape');
    await page.getByTestId('sources-modal').waitFor({ state: 'detached' });
    await page.getByTestId('project-switch').click();
    await button(page).waitFor();
    assert.equal(await toast(page).count(), 0, 'the toast closed once Source texts opened');
  }, { ...opts, realSync: true });
});
