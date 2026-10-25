// Team sync for real (E1a step 6, tC4 e2e/j26-team-sync.spec.ts): A shares the sample from the
// browser; teammates B and C, each on another computer (`teammate`, a second tc4 driven
// through its API), open it from the fake Door43, change verses and sync; A's next sync takes
// their work in and names them. Every computer ends with the same text.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/team-sync.test.mjs
import assert from 'node:assert/strict';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { saveTitus, titusWords } from '../env-rust.mjs';
import { BIBLE } from '../screens.mjs';
import { browserTest, withApp } from './kit.mjs';

const FOLDER = 'sample_burrito';
const opts = { door43: true, realSync: true, scenario: 'shared' };
const button = (page) => page.getByTestId(`share-${FOLDER}`);
const toast = (page) => page.getByTestId('sync-result');

/** One press of A's Sync on Home, signing in the first time; the toast's ok flag. */
async function syncA(page, first = false) {
  await button(page).click();
  if (first) {
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    await page.getByLabel('Username or email').fill(FAKE_USER.username);
    await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
  }
  await toast(page).waitFor({ timeout: 120_000 });
  return toast(page).getAttribute('data-ok');
}

async function dismiss(page) {
  await toast(page).getByTestId('sync-dismiss').click();
  await toast(page).waitFor({ state: 'detached' });
}

const B_WORDS = 'Pablo, siervo de Dios, como B lo tradujo';
const C_WORDS = 'para la fe de los escogidos, como C lo tradujo';

browserTest('J26 [G1]. Three computers: B and C change verses, A takes their work in by name, and every computer ends with the same text', async () => {
  await withApp(async ({ page, env, teammate }) => {
    await button(page).waitFor();
    await env.pushToDoor43(BIBLE);
    assert.equal(await syncA(page, true), '1', 'A shares the project');
    await dismiss(page);
    const b = await teammate('teammate-b');
    const c = await teammate('teammate-c');
    const pb = await b.join(env.door43Repo(BIBLE), 'titus-b');
    const pc = await c.join(env.door43Repo(BIBLE), 'titus-c');
    await b.saveVerse(pb, 1, 1, B_WORDS);
    await b.sync(pb);
    await c.saveVerse(pc, 1, 2, C_WORDS);
    await c.sync(pc);
    assert.equal(await syncA(page), '1', 'A syncs');
    const received = Number(await page.getByTestId('sync-count-received').getAttribute('data-count'));
    assert.ok(received >= 2, `A received both teammates' changes (${received})`);
    const names = await toast(page).textContent();
    for (const name of ['teammate-b', 'teammate-c']) assert.ok(names.includes(name), `the toast names ${name}: ${names}`);
    await dismiss(page);
    // B and C sync once more; then all three hold the same two verses.
    await b.sync(pb);
    await c.sync(pc);
    for (const [verse, wanted] of [[1, B_WORDS], [2, C_WORDS]]) {
      assert.equal(await titusWords(env.call, FOLDER, 1, verse), wanted, `A has 1:${verse}`);
      assert.equal(await b.words(pb, 1, verse), wanted, `B has 1:${verse}`);
      assert.equal(await c.words(pc, 1, verse), wanted, `C has 1:${verse}`);
    }
  }, opts);
});

browserTest('J26 [G6]. A and B change the same verse: both versions are kept, and A\'s sync counts one verse conflict to review', async () => {
  await withApp(async ({ page, env, teammate }) => {
    await button(page).waitFor();
    await env.pushToDoor43(BIBLE);
    assert.equal(await syncA(page, true), '1');
    await dismiss(page);
    const b = await teammate('teammate-b');
    const pb = await b.join(env.door43Repo(BIBLE), 'titus-b');
    await b.saveVerse(pb, 1, 3, 'y a su debido tiempo, como B lo dijo');
    await b.sync(pb);
    await saveTitus(env.call, FOLDER, 1, 3, 'y a su debido tiempo, como A lo dijo');
    assert.equal(await syncA(page), '1', 'A syncs');
    assert.equal(await page.getByTestId('sync-count-conflicts').getAttribute('data-count'), '1', 'one verse conflict to review');
    assert.equal(await page.getByTestId('sync-review').count(), 1, 'Review is offered');
  }, opts);
});
