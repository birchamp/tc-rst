// Share on Door43 end to end (E1b; tC4 e2e/j11-share.spec.ts at b86270f, tests 1, 6 and 11):
// the run from the check step, its progress lines and its end, against the fake Door43 for
// real. A teammate on another computer opens what was shared, so "Door43's main is what the
// translator saw" is checked by the words it holds.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/share.test.mjs
import assert from 'node:assert/strict';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { DOOR43_TEAM, saveTitus } from '../env-rust.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const STORIES = 'sample_burrito_obs';
const opts = {
  door43: true,
  realSync: true,
  init: (context) => context.grantPermissions(['clipboard-read', 'clipboard-write']),
};

const pushes = async (door43) => (await door43.requests()).filter((r) => r.path.includes('git-receive-pack'));

/** A card's Share, through the internet question and the sign-in the first time, to the
 * where-it-goes step with the organizations read. */
async function openShare(page, folder) {
  await page.getByTestId(`share-${folder}`).click();
  const ask = page.getByTestId('net-confirm');
  const signIn = page.getByTestId('share-signin');
  const dialog = page.getByTestId('share-dialog');
  await until('the question, the sign-in or the dialog', async () => (await ask.count()) + (await signIn.count()) + (await dialog.count()) > 0);
  if (await ask.count()) await ask.click();
  await until('the sign-in or the dialog', async () => (await signIn.count()) + (await dialog.count()) > 0);
  if (await signIn.count()) {
    await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
    await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
  }
  await dialog.waitFor();
  await until('the organizations are read', async () => (await page.getByTestId('share-orgs-loading').count()) === 0);
}

const WORDS = 'Pablo, siervo de Dios, como se compartió';

browserTest('J11 1. Share to the team organization: one progress line, the end with its address, Copy link and the team-access line; the main on Door43 holds the verse saved just before', async () => {
  await withApp(async ({ page, env, context, teammate }) => {
    await page.getByTestId(`share-${BIBLE}`).waitFor();
    await saveTitus(env.call, BIBLE, 1, 1, WORDS);
    await openShare(page, BIBLE);
    await page.getByTestId(`share-org-${DOOR43_TEAM}`).click();
    await page.getByTestId('share-next').click();
    assert.equal(await page.getByLabel('Repository name').inputValue(), BIBLE, 'the folder name by default');
    assert.equal(await page.getByTestId('share-items').textContent(), 'Jonah, Titus');
    await page.getByTestId('share-submit').click();
    const done = page.getByTestId('share-done');
    await done.waitFor({ timeout: 120_000 });
    assert.equal(await done.getAttribute('data-steps'), 'create,push', 'the two steps, in order');
    const url = `${env.door43.url}/${DOOR43_TEAM}/${BIBLE}`;
    assert.equal(await page.getByTestId('share-url').textContent(), url);
    assert.equal(await page.getByTestId('share-done-text').textContent(), en['shareDialog.doneText']);
    assert.equal(await page.getByTestId('share-team-access').textContent(), en['shareDialog.teamAccess']);
    await page.getByTestId('share-copy').click();
    await until('Copied', async () => (await page.getByTestId('share-copy').textContent()) === en['shareDialog.copied']);
    assert.equal(await page.evaluate(() => navigator.clipboard.readText()), url);
    // Open on Door43 opens the repository's page at once: the internet is on (internet-consent d).
    // The fake Door43 serves no web pages: the page answers here, so the popup's address stays.
    await context.route(url, (route) => route.fulfill({ status: 200, contentType: 'text/html', body: '<p>Door43</p>' }));
    const opened = page.waitForEvent('popup');
    await page.getByTestId('share-open').click();
    const popup = await opened;
    assert.equal(popup.url(), url);
    await popup.close();
    await page.getByTestId('share-close').click();
    await page.getByTestId('share-dialog').waitFor({ state: 'detached' });
    // INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW: a teammate who opens it gets the verse.
    const b = await teammate('teammate-b');
    const project = await b.join(BIBLE, 'titus-b');
    assert.equal(await b.words(project, 1, 1), WORDS, 'INV-SHARE-PUSHES-WHAT-THE-TRANSLATOR-SAW');
  }, opts);
});

browserTest('J11 6. A name that exists on Door43 is refused on the check step with its sentence, and nothing is pushed; another name shares', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${BIBLE}`).waitFor();
    await env.door43.addRepo(DOOR43_TEAM, BIBLE);
    await openShare(page, BIBLE);
    await page.getByTestId(`share-org-${DOOR43_TEAM}`).click();
    await page.getByTestId('share-next').click();
    await env.door43.clear();
    await page.getByTestId('share-submit').click();
    const error = page.getByTestId('share-error');
    await error.waitFor({ timeout: 60_000 });
    assert.equal(await error.getAttribute('data-code'), 'share.name-exists');
    assert.ok((await error.textContent()).includes(en['shareDialog.error.name-exists']), await error.textContent());
    assert.equal(await page.getByTestId('share-check').count(), 1, 'back on the check step');
    assert.deepEqual(await pushes(env.door43), [], 'nothing was pushed');
    await page.getByLabel('Repository name').fill(`${BIBLE}_2`);
    assert.equal(await error.count(), 0, 'typing takes the refusal away');
    await page.getByTestId('share-submit').click();
    await page.getByTestId('share-done').waitFor({ timeout: 120_000 });
    assert.equal(await page.getByTestId('share-url').textContent(), `${env.door43.url}/${DOOR43_TEAM}/${BIBLE}_2`);
  }, opts);
});

browserTest('J11 11, J24. An Open Bible Stories project shares the same way, and its end names the team access too (#724)', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`share-${STORIES}`).waitFor();
    await openShare(page, STORIES);
    await page.getByTestId('share-next').click();
    assert.equal(await page.getByLabel('Repository name').inputValue(), STORIES);
    assert.equal(await page.getByTestId('share-items').textContent(), en['shareDialog.stories']);
    await page.getByTestId('share-submit').click();
    await page.getByTestId('share-done').waitFor({ timeout: 120_000 });
    assert.equal(await page.getByTestId('share-url').textContent(), `${env.door43.url}/${FAKE_USER.username}/${STORIES}`);
    // Declared difference: tC4 leaves this line off a stories share, whose round 1 did not sync.
    assert.equal(await page.getByTestId('share-team-access').textContent(), en['shareDialog.teamAccess']);
  }, opts);
});
