// Open a project from Door43 end to end (E1b; tC4 e2e/j17-open-from-door43.spec.ts at b86270f,
// :95, and e2e/j14-join-isolation.spec.ts): a project the team shares on the fake Door43 is opened
// on this computer under a new name, shows on Home, and syncs; a similar project of this
// computer stays apart from it. The private repository's sign-in and retry is
// crates/tc-app/tests/app/port_contract.rs `open_private_repo_signs_in_and_retries` (the fake's
// control port makes public repositories only).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/open-door43.test.mjs
import assert from 'node:assert/strict';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { DOOR43_TEAM, saveTitus, titusWords } from '../env-rust.mjs';
import { BIBLE as BIBLE_REPO } from '../screens.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const OPENED = 'titus_aqui';
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

/** Open a project from Door43: the team's copy, as `name`. */
async function openFromDoor43(page, env, name) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-door43').click();
  await page.getByTestId('open-door43-modal').waitFor();
  await page.getByTestId('open-door43-repo').fill(`${DOOR43_TEAM}/${env.door43Repo(BIBLE_REPO)}`);
  await page.getByTestId('open-door43-name').fill(name);
  await page.getByTestId('open-door43-submit').click();
  await page.getByTestId('open-door43-modal').waitFor({ state: 'detached', timeout: 120_000 });
  await page.getByTestId(`project-${name}`).waitFor();
}

browserTest('J17. Another translator opens the project from Door43: it shows on Home under its new name with the text of the team, and it syncs', async () => {
  await withApp(async ({ page, env }) => {
    await teamCopy(page, env);
    await openFromDoor43(page, env, OPENED);
    assert.equal(await titusWords(env.call, OPENED, 1, 1), WORDS, 'the opened project holds the team\'s text');
    // It is a shared project: its card syncs.
    await page.getByTestId(`share-${OPENED}`).click();
    await until('the sync ends', async () => (await page.getByTestId('sync-result').count()) > 0, 120_000);
    assert.equal(await page.getByTestId('sync-result').getAttribute('data-ok'), '1', 'the opened project syncs');
  }, opts);
});

const MINE = 'para la fe, solo en esta computadora';

browserTest('J14. Opening the team copy beside a similar project of this computer mixes nothing: the opened project lacks the unsynced verse of this computer, and that project keeps it', async () => {
  await withApp(async ({ page, env }) => {
    await teamCopy(page, env);
    await saveTitus(env.call, BIBLE, 1, 2, MINE);
    await openFromDoor43(page, env, OPENED);
    assert.equal(await titusWords(env.call, OPENED, 1, 1), WORDS, 'the opened project is the team\'s copy');
    const theirs = await titusWords(env.call, OPENED, 1, 2);
    assert.ok(theirs && theirs !== MINE, `it has the team's verse, none of this computer's unsynced work: ${theirs}`);
    assert.equal(await titusWords(env.call, BIBLE, 1, 2), MINE, 'the project of this computer keeps its work');
    assert.equal(await page.getByTestId(`project-${BIBLE}`).count(), 1, 'both projects are on Home, apart');
  }, opts);
});
