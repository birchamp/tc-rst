// J11's sign-in cases (A8; tC4's e2e/j11-share.spec.ts, through the account menu, which opens
// the same sign-in step, M12, that Share opens): the step's fields and notices, a refusal's
// cause, what a sign-in stores and where, Stay signed in, no keychain (#496) and sign-out. tc4
// signs in to the fake Door43 on loopback.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/j11-sign-in.test.mjs
//
// Not here: 2 (the share dialog's Change) is in share-open.test.mjs (B9b); with the unit that
// takes them, 3 (the live leg on qa.door43.org: no network in tests), 6b (tC4's login-check
// helper: 6 searches for the login itself), 7c's upload half and 7d (they start at the share's
// run: E1b; 7d's refused resume is tc-app's a_kept_token_door43_refuses_is_forgotten_with_a_notice),
// the share's run (E1b).
import assert from 'node:assert/strict';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import {
  browserTest, catalog, filesHolding, keychain, openMenu, sleep, storageDump, trigger, until, withApp,
} from './kit.mjs';

const en = catalog('en');
const opts = { door43: true };
const SCOPES = ['write:repository', 'write:organization', 'write:user'];

const state = (page) => trigger(page).getAttribute('data-state');
const internet = (page) => trigger(page).getAttribute('data-internet');

async function openSignIn(page) {
  await openMenu(page);
  await page.getByTestId('account-sign-in').click();
  await page.getByTestId('share-signin').waitFor();
}

async function signIn(page, { password = FAKE_USER.password, stay = false } = {}) {
  await openSignIn(page);
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(password);
  if (stay) await page.getByLabel(en['signIn.stay']).check();
  await page.getByTestId('signin-submit').click();
  if (await internet(page) === 'off') await page.getByTestId('net-confirm').click();
}

/** The test keychain set as `set` says, the account read as a launch finds it, the page loaded
 * again. */
async function relaunch(page, env, set = {}) {
  await keychain(env, { ...set, relaunch: true });
  await page.goto('about:blank');
  await page.goto(`${env.origin}/#t=${env.token}`);
  await page.getByTestId('account-menu').waitFor();
}

const text = (page, id) => page.getByTestId(id).textContent();

browserTest('J11 0. the account menu is the one Door43 item', async () => {
  await withApp(async ({ page }) => {
    assert.equal(await page.getByTestId('account-menu').count(), 1);
    assert.equal(await page.getByTestId('door43-bar').count(), 0);
    assert.equal(await page.getByRole('button', { name: /sign (in|out)/i }).count(), 0, 'no sign-in button outside the menu');
    assert.equal(await page.getByText(/signed in to Door43/i).count(), 0);
  }, opts);
});

browserTest('J11 1. the sign-in step shows the fields, the box, the notices and the server; Cancel creates and stores nothing', async () => {
  await withApp(async ({ page, env }) => {
    await openSignIn(page);
    const login = page.getByLabel(en['signIn.login']);
    const password = page.getByLabel(en['signIn.password'], { exact: true });
    assert.equal(await login.inputValue(), '');
    assert.equal(await password.getAttribute('type'), 'password');
    const stay = page.getByLabel(en['signIn.stay']);
    assert.equal(await stay.isChecked(), false);
    assert.equal(await page.getByLabel('Name', { exact: true }).count(), 0, 'no Name field');
    assert.equal(await page.getByLabel('Email', { exact: true }).count(), 0, 'no Email field');
    assert.equal(await text(page, 'signin-notice'), en['signIn.notice']);
    const nameNotice = await text(page, 'signin-name-notice');
    assert.equal(nameNotice, en['signIn.nameNotice']);
    for (const words of ['Door43 username', 'project', 'stays', 'Scripture Burrito export']) assert.ok(nameNotice.includes(words), words);
    assert.ok((await text(page, 'signin-server')).includes(env.door43.host), 'the server line names the Door43 tc4 signs in to');
    assert.equal(await text(page, 'signin-cancel'), en['signIn.cancel']);
    assert.equal(await text(page, 'signin-submit'), en['signIn.submit']);
    await login.fill(FAKE_USER.username);
    await page.getByTestId('signin-cancel').click();
    await until('closed', async () => (await page.getByTestId('share-signin').count()) === 0);
    await sleep(300);
    assert.deepEqual(await env.door43.requests(), []);
    assert.deepEqual(await env.door43.tokens(FAKE_USER.username), []);
    assert.ok(!(await storageDump(page)).includes(FAKE_USER.username), 'no storage holds the login');
  }, opts);
});

browserTest('J11 5. a sign-in failure names its cause with its code and stores nothing', async () => {
  await withApp(async ({ page, env }) => {
    await signIn(page, { password: 'wrong' });
    const error = page.getByTestId('signin-error');
    await error.waitFor();
    assert.equal(await error.getAttribute('data-code'), 'share.auth-failed');
    assert.match(await error.textContent(), /did not accept that username or password/);
    assert.equal(await page.getByLabel(en['signIn.password'], { exact: true }).inputValue(), '', 'the password is emptied');
    await env.door43.fault({ method: 'GET', path: '/api/v1/user', times: 1, fault: { type: 'status', status: 503, message: 'down for maintenance' } });
    await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    await until('the cause changes', async () => (await error.getAttribute('data-code')) === 'share.server-unavailable');
    assert.equal(await state(page), 'out');
    assert.deepEqual(await env.door43.tokens(FAKE_USER.username), []);
    assert.ok(!(await storageDump(page)).includes(FAKE_USER.username));
    assert.equal((await keychain(env)).keeps, 0);
  }, opts);
});

browserTest('J11 6. after a sign-in without Stay signed in, the token, the password and the login are in no file and no storage', async () => {
  await withApp(async ({ page, env }) => {
    await signIn(page);
    await until('signed in', async () => (await state(page)) === 'in');
    const token = await env.door43.secret(FAKE_USER.username, 'translationCore');
    assert.ok(token, 'Door43 minted the token');
    // Negative control: the search finds what is there.
    assert.ok(filesHolding(env.appData, 'schemaVersion').length > 0, 'the search finds files under app data');
    for (const [what, value] of [['the token', token], ['the password', FAKE_USER.password], ['the login', FAKE_USER.username]]) {
      assert.deepEqual(filesHolding(env.appData, value), [], `${what} is in no file`);
      assert.ok(!(await storageDump(page)).includes(value), `${what} is in no page storage`);
    }
    for (const endpoint of ['app/hello', 'account/state']) {
      assert.ok(!JSON.stringify(await env.call(endpoint, {})).includes(token), `${endpoint} holds no token`);
    }
  }, opts);
});

browserTest('J11 7. a new launch without a kept token asks the sign-in again; Stay off never asks the keychain to keep', async () => {
  await withApp(async ({ page, env }) => {
    await signIn(page);
    await until('signed in', async () => (await state(page)) === 'in');
    const kc = await keychain(env);
    assert.equal(kc.keeps, 0);
    assert.equal(kc.forgets, 1, 'an earlier kept token is forgotten (tC4 #366)');
    assert.equal(kc.holds, false);
    await relaunch(page, env);
    assert.equal(await state(page), 'out');
    await openSignIn(page);
    assert.equal(await page.getByLabel(en['signIn.login']).inputValue(), '');
    assert.equal(await text(page, 'signin-reason'), en['signIn.sessionNote']);
    await page.getByTestId('signin-cancel').click();
    await signIn(page);
    await until('signed in again', async () => (await state(page)) === 'in');
  }, opts);
});

browserTest('J11 7c. Stay signed in keeps the token and nothing else; the next launch finds it saved and sends nothing until a step needs it', async () => {
  await withApp(async ({ page, env }) => {
    await signIn(page, { stay: true });
    await until('signed in', async () => (await state(page)) === 'in');
    const kc = await keychain(env);
    assert.equal(kc.keeps, 1);
    assert.equal(kc.holds, true);
    assert.ok(!(await storageDump(page)).includes(FAKE_USER.username));
    await relaunch(page, env);
    await env.door43.clear();
    assert.equal(await state(page), 'saved');
    await sleep(1_000);
    assert.deepEqual(await env.door43.requests(), []);
  }, opts);
});

browserTest('no_keychain_hides_stay_signed_in (J11 7e, #496)', async () => {
  await withApp(async ({ page, env }) => {
    await relaunch(page, env, { mode: 'none' });
    await openSignIn(page);
    assert.equal(await page.getByTestId('signin-stay').count(), 0, 'no Stay signed in box');
    assert.equal(await page.getByTestId('signin-reason').count(), 0, 'no line about Stay signed in');
    assert.ok(!(await page.locator('body').innerText()).includes(en['signIn.notKept']), 'no "not available on this computer" line');
    await page.getByTestId('signin-cancel').click();
    await signIn(page);
    await until('signed in', async () => (await state(page)) === 'in');
    assert.equal((await keychain(env)).keeps, 0);
    await relaunch(page, env);
    assert.equal(await state(page), 'out');
  }, opts);
});

browserTest('J11 7f. Sign out in the account menu forgets the kept token without a request; the internet stays off', async () => {
  await withApp(async ({ page, env }) => {
    await openMenu(page);
    assert.equal(await page.getByTestId('account-sign-out').count(), 0, 'nothing kept: no sign-out row');
    await page.keyboard.press('Escape');
    const token = await env.door43.issueToken(FAKE_USER.username, 'translationCore', SCOPES);
    await relaunch(page, env, { token });
    assert.equal(await state(page), 'saved');
    await openMenu(page);
    await page.getByTestId('account-sign-out').click();
    await until('out', async () => (await state(page)) === 'out');
    assert.equal((await keychain(env)).holds, false);
    assert.deepEqual(await env.door43.requests(), []);
    assert.equal(await internet(page), 'off');
  }, opts);
});
