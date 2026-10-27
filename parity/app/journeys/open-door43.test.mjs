// Open a project from Door43 end to end (E1b; tC4 e2e/j17-open-from-door43.spec.ts at b86270f,
// :95, and e2e/j14-join-isolation.spec.ts): a project the team shares on the fake Door43 is opened
// on another computer without a name asked (ruling H14), under the team's name, and syncs; the
// computer that has it gets no second copy; a teammate's copy holds none of this computer's
// unsynced work. The private repository's sign-in and retry is
// crates/tc-app/tests/app/port_contract.rs `open_private_repo_signs_in_and_retries` (the fake's
// control port makes public repositories only).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/open-door43.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { DOOR43_OWNER, REPO, saveTitus, titusWords } from '../env-rust.mjs';
import { BIBLE as BIBLE_REPO } from '../screens.mjs';
import { browserTest, catalog, withApp } from './kit.mjs';

const en = catalog('en');
const rebuild = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8'));
const BIBLE = 'sample_burrito';
const WORDS = 'Pablo, siervo de Dios, como el equipo lo tiene';
// The `shared` scenario: the sample's card syncs, as a team project's does.
const opts = { door43: true, realSync: true, scenario: 'shared' };

/** The sample on the fake Door43 as the team's copy, pushed by a first sync from Home. */
async function teamCopy(page, env) {
  await page.getByTestId(`share-${BIBLE}`).waitFor();
  await env.pushToDoor43(BIBLE_REPO);
  await saveTitus(env.call, BIBLE, 1, 1, WORDS);
  await page.getByTestId(`share-${BIBLE}`).click();
  await page.getByTestId('net-confirm').click();
  await page.getByTestId('share-signin').waitFor();
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
  await page.getByTestId('sync-result').waitFor({ timeout: 120_000 });
  assert.equal(await page.getByTestId('sync-result').getAttribute('data-ok'), '1', 'the team\'s copy is on Door43');
  await page.getByTestId('sync-dismiss').click();
}

/** Open from Door43 the repository of the sample, which this computer shared (ruling H14):
 *  the dialog asks no name, makes no second copy, and offers to open the one here. */
async function openOwnFromDoor43(page, env) {
  const before = (await env.call('home/list', {})).cards.map((c) => c.project);
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-door43').click();
  await page.getByTestId('open-door43-modal').waitFor();
  assert.equal(await page.getByTestId('open-door43-name').count(), 0, 'the dialog asks no name');
  await page.getByTestId('open-door43-repo').fill(`${DOOR43_OWNER}/${env.door43Repo(BIBLE_REPO)}`);
  await page.getByTestId('open-door43-submit').click();
  const yours = page.getByTestId('open-door43-yours');
  await yours.waitFor({ timeout: 120_000 });
  assert.ok((await yours.textContent()).includes(rebuild['openDoor43.alreadyHere']));
  assert.deepEqual((await env.call('home/list', {})).cards.map((c) => c.project), before, 'no second copy');
  await page.getByTestId('open-door43-open-yours').click();
  await page.getByTestId('open-door43-modal').waitFor({ state: 'detached' });
}

browserTest('J17. The project opens from Door43 on another computer with the text and name of the team, and syncs; on the computer that has it, Open from Door43 makes no second copy and opens the one there', async () => {
  await withApp(async ({ page, env, teammate }) => {
    await teamCopy(page, env);
    const b = await teammate('teammate-b');
    const opened = await b.join(env.door43Repo(BIBLE_REPO));
    assert.equal(await b.words(opened, 1, 1), WORDS, 'the opened project holds the team\'s text');
    const name = async (call, project) => (await call('home/list', {})).cards.find((c) => c.project === project).name;
    assert.equal(await name(b.env.call, opened), await name(env.call, BIBLE), 'one name for the whole team');
    await b.sync(opened);
    await openOwnFromDoor43(page, env);
    await page.getByTestId('mode-switch').waitFor();
  }, opts);
});

const MINE = 'para la fe, solo en esta computadora';

browserTest('J14. A teammate who opens the team copy gets none of the unsynced work of this computer, and this computer keeps it', async () => {
  await withApp(async ({ env, page, teammate }) => {
    await teamCopy(page, env);
    await saveTitus(env.call, BIBLE, 1, 2, MINE);
    const b = await teammate('teammate-b');
    const opened = await b.join(env.door43Repo(BIBLE_REPO));
    assert.equal(await b.words(opened, 1, 1), WORDS, 'the opened project is the team\'s copy');
    const theirs = await b.words(opened, 1, 2);
    assert.ok(theirs && theirs !== MINE, `it has the team's verse, none of this computer's unsynced work: ${theirs}`);
    assert.equal(await titusWords(env.call, BIBLE, 1, 2), MINE, 'the project of this computer keeps its work');
  }, opts);
});
