// Share on Door43 up to its check step, and Open a project from Door43 up to its open (B9b):
// the Share halves of tC4's internet-consent a, b and d (e2e/internet-consent.spec.ts), J11 2
// (e2e/j11-share.spec.ts: who shares, and Change), the where-it-goes and check steps against the
// fake Door43's parity account (Recommended by R12's count), and the question an open asks
// (test/openFromDoor43Dialog.test.tsx's last case, through the app). tc4 signs in to the fake
// Door43 on loopback, whose control port tells what Door43 received.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/share-open.test.mjs
//
// The share's run, its progress and its end, with internet-consent d's second half (Open on
// Door43 opens at once), are share.test.mjs's J11 1 (E1b); the open of a repository
// is open-door43.test.mjs's. The last case here opens one the fake Door43 does not hold.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO } from '../env-rust.mjs';
import { asks, browserTest, catalog, countAsks, sleep, trigger, until, withApp } from './kit.mjs';

const en = catalog('en');
const rebuild = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8'));
const opts = { door43: true };
const BIBLE = 'sample_burrito';
const OPEN_REPO = `${FAKE_USER.username}/es-419_tit_equipo`;

const internet = (page) => trigger(page).getAttribute('data-internet');
const apiRequests = async (door43) => (await door43.requests()).filter((r) => r.path.startsWith('/api/'));
const share = (page) => page.getByTestId(`share-${BIBLE}`);

async function signInHere(page) {
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
}

async function orgsRead(page) {
  await page.getByTestId('share-next').waitFor();
  await until('the organizations are read', async () => (await page.getByTestId('share-orgs-loading').count()) === 0);
}

browserTest('internet-consent a, b. Share asks once per session; Not now sends nothing and keeps the person on Home', async () => {
  await withApp(async ({ page, env }) => {
    await share(page).waitFor();
    await env.door43.clear();
    assert.equal(await internet(page), 'off', 'a launch starts with the internet off');
    await share(page).click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'share');
    assert.equal(await page.getByTestId('net-ask-reason').textContent(), en['net.ask.step'].replace('{what}', en['net.ask.share']));
    await page.getByTestId('net-cancel').click();
    await until('the question closed', async () => (await ask.count()) === 0);
    await sleep(500);
    assert.deepEqual(await apiRequests(env.door43), [], 'Not now sends nothing to Door43');
    assert.equal(await page.getByTestId('share-signin').count(), 0);
    assert.equal(await page.getByTestId('share-dialog').count(), 0);
    assert.equal(await internet(page), 'off');
    // A new Share asks again; Turn on internet goes on to the sign-in step, then no question more.
    await share(page).click();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    assert.equal(await internet(page), 'on');
    await page.getByTestId('signin-cancel').click();
    await share(page).click();
    await page.getByTestId('share-signin').waitFor();
    assert.equal(await asks(page), 2, 'one question per Share while the internet was off, none after');
  }, { ...opts, init: (context) => countAsks(context) });
});

browserTest('internet-consent d, J11 2. Share: the sign-in, then where it goes with the Recommended organization, then the check step, with one question; Change signs out and asks the sign-in again', async () => {
  await withApp(async ({ page, env }) => {
    await share(page).click();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    await signInHere(page);
    await page.getByTestId('share-dialog').waitFor();
    assert.equal(await page.getByTestId('share-signin').count(), 0, 'the dialog takes the sign-in step\'s place');
    await orgsRead(page);
    assert.equal(await page.getByTestId('share-account').textContent(), `Sharing as @${FAKE_USER.username} · Change`);
    // Where it goes (D84 point 3): the person's own account first and chosen; the organization
    // with the most es-419 repositories that the person can create in is Recommended; one they
    // cannot create in is shown, says why and cannot be chosen.
    const user = page.getByTestId('share-target-user');
    assert.equal(await user.getAttribute('data-selected'), 'true');
    const team = page.getByTestId('share-org-Equipo-Ejemplo');
    const gateway = page.getByTestId('share-org-es-419_gl');
    assert.equal(await team.getAttribute('data-recommended'), '1');
    assert.equal(await gateway.getAttribute('data-recommended'), '0');
    assert.ok(await gateway.isDisabled());
    assert.ok((await gateway.textContent()).includes(en['shareDialog.orgCannot']));
    const searches = (await apiRequests(env.door43)).filter((r) => r.path === '/api/v1/repos/search');
    assert.deepEqual(searches.map((r) => r.query), [
      'owner=Equipo-Ejemplo&lang=es-419&limit=1',
      'owner=es-419_gl&lang=es-419&limit=1',
    ]);
    assert.ok(searches.every((r) => r.auth === 'token'), 'the counts are asked with the token, in a header');
    await team.click();
    assert.equal(await team.getAttribute('data-selected'), 'true');
    // Check what will be shared: the target, the folder's name, the books.
    await page.getByTestId('share-next').click();
    await page.getByTestId('share-check').waitFor();
    assert.equal(await page.getByTestId('share-where').textContent(), en['shareDialog.where'].replace('{target}', 'Equipo-Ejemplo'));
    assert.equal(await page.getByTestId('share-name').inputValue(), BIBLE);
    assert.equal(await page.getByTestId('share-items').textContent(), 'Jonah, Titus');
    // A name Door43 refuses turns Share off.
    await page.getByTestId('share-name').fill('my repo');
    assert.ok(await page.getByTestId('share-submit').isDisabled());
    await page.getByTestId('share-name').fill('titus_equipo');
    assert.ok(!(await page.getByTestId('share-submit').isDisabled()));
    await page.getByTestId('share-back').click();
    await page.getByTestId('share-target').waitFor();
    assert.equal(await asks(page), 1, 'the sign-in, the organizations and the steps needed one question');
    assert.equal((await env.door43.requests()).filter((r) => r.method === 'POST' && r.path.includes('/repos')).length, 0, 'nothing is created before Share');
    // Change: the dialog closes, the person is signed out, the sign-in step asks everything again.
    await page.getByTestId('share-account-change').click();
    await page.getByTestId('share-signin').waitFor();
    assert.equal(await page.getByTestId('share-dialog').count(), 0);
    assert.equal(await page.getByLabel(en['signIn.login']).inputValue(), '');
    await until('signed out', async () => (await trigger(page).getAttribute('data-state')) === 'out');
    // The share goes on with the new sign-in.
    await signInHere(page);
    await page.getByTestId('share-dialog').waitFor();
    await orgsRead(page);
    assert.equal(await page.getByTestId('share-account').textContent(), `Sharing as @${FAKE_USER.username} · Change`);
  }, { ...opts, init: (context) => countAsks(context) });
});

browserTest('Open a project from Door43: it asks no name (ruling H14); Open project asks the open question; Not now sends nothing; a repository Door43 does not show asks the sign-in once, then ends in its refusal, and no project was added', async () => {
  await withApp(async ({ page, env }) => {
    await env.door43.clear();
    await page.getByTestId('add-project').click();
    await page.getByTestId('add-project-door43').click();
    const dialog = page.getByTestId('open-door43-modal');
    await dialog.waitFor();
    const submit = page.getByTestId('open-door43-submit');
    assert.ok(await submit.isDisabled(), 'nothing to open yet');
    assert.equal(
      (await dialog.textContent()).includes(en['openDoor43.where'].split('{host}')[1]), false,
      'the note does not say that only public repositories open (Q3)',
    );
    assert.ok((await dialog.textContent()).includes(rebuild['openDoor43.whereAny'].replace('{host}', env.door43.host)));
    assert.equal(await page.getByTestId('open-door43-name').count(), 0, 'the dialog asks no name (ruling H14)');
    await page.getByTestId('open-door43-repo').fill('https://example.org/someone/other');
    assert.ok((await dialog.textContent()).includes(en['openDoor43.repoBad'].replace('{host}', env.door43.host)));
    assert.ok(await submit.isDisabled(), 'a repository on another server cannot be opened');
    await page.getByTestId('open-door43-repo').fill(OPEN_REPO);
    const before = (await env.call('home/list', {})).cards.length;
    await submit.click();
    const ask = page.locator('[data-testid="net-ask"][data-kind="open"]');
    await ask.waitFor();
    assert.equal(await page.getByTestId('net-ask-reason').textContent(), en['net.ask.step'].replace('{what}', en['net.ask.open']));
    await page.getByTestId('net-cancel').click();
    await until('the question closed', async () => (await ask.count()) === 0);
    await sleep(300);
    assert.deepEqual(await apiRequests(env.door43), [], 'Not now sends nothing');
    assert.equal(await submit.textContent(), en['openDoor43.open']);
    assert.ok(!(await page.getByTestId('open-door43-repo').isDisabled()), 'the form can be changed again');
    // Turn on internet: Door43 shows no such repository to a request without a sign-in, which
    // may be a private one, so the sign-in step asks once; with it, the open ends in its refusal.
    await submit.click();
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    await signInHere(page);
    const refused = page.getByTestId('open-door43-refused');
    await refused.waitFor();
    assert.equal(await page.getByTestId('share-signin').count(), 0, 'INV-PRIVATE-OPEN-ASKS-SIGN-IN-ONCE: no second ask');
    const words = await refused.textContent();
    assert.ok(words.includes(rebuild['openDoor43.refused.notOpened']), words);
    assert.ok(words.includes(en['openDoor43.nothingWritten']));
    assert.ok(!words.includes('public'), 'the sentence does not ask for a public repository (Q-21)');
    assert.equal(await refused.getAttribute('data-code'), 'sync.clone-failed', 'tC4\'s code for a clone that failed');
    assert.equal((await env.call('home/list', {})).cards.length, before, 'no project was added');
    await page.getByTestId('open-door43-cancel').click();
    await until('the dialog closed', async () => (await dialog.count()) === 0);
  }, { ...opts, realSync: true });
});
