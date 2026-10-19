// The internet question around the Door43 account (A8; tC4's e2e/internet-consent.spec.ts, the
// halves that need no project): the internet starts off, a step asks before it sends, the
// account menu's three states, the sign-in step (M12) and sign-out. tc4 signs in to the fake
// Door43 on loopback, whose control port tells what Door43 received.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/internet-consent.test.mjs
//
// Not here, with the unit that takes them: the Share halves of a, b and d (B9b, E1b), the "in a
// project" halves of c and e (B3, B5a); i and j ran in A5. tC4's f2 (a gate that cannot be read)
// has no counterpart: the switch is the server's, with no separate gate (APP-DESIGN 5.6 item 2).
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO } from '../env-rust.mjs';
import {
  asks, browserTest, catalog, countAsks, filesHolding, focused, keychain, loadHome, openMenu, panel,
  recordRequests, sleep, snapshot, storageDump, subtreeOf, trigger, until, withApp,
} from './kit.mjs';

const en = catalog('en');
const SCOPES = ['write:repository', 'write:organization', 'write:user'];
const opts = { door43: true };

const state = (page) => trigger(page).getAttribute('data-state');
const internet = (page) => trigger(page).getAttribute('data-internet');
const userRequests = async (door43) => (await door43.requests()).filter((r) => r.path.startsWith('/api/'));

async function openSignIn(page) {
  await openMenu(page);
  await page.getByTestId('account-sign-in').click();
  await page.getByTestId('share-signin').waitFor();
}

async function fill(page, login, password) {
  await page.getByLabel(en['signIn.login']).fill(login);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(password);
}

/** The page loaded again without `test/reset`: what a launch shows after `test/keychain`'s
 * relaunch. */
async function relaunch(page, env) {
  await page.goto('about:blank');
  await page.goto(`${env.origin}/#t=${env.token}`);
  await page.getByTestId('account-menu').waitFor();
}

const quiet = (env) => until('the server is quiet', async () => !(await env.call('test/quiet', {})).busy);

/** A token as if an earlier session had kept it, in the test keychain; the page relaunched. */
async function keptToken(page, env, extra = {}) {
  const token = await env.door43.issueToken(FAKE_USER.username, 'translationCore', SCOPES);
  await keychain(env, { token, relaunch: true, ...extra });
  await relaunch(page, env);
  await until('the sign-in shows as saved', async () => (await state(page)) === 'saved');
  await env.door43.clear();
  return token;
}

async function signIn(page, { stay = false } = {}) {
  await openSignIn(page);
  await fill(page, FAKE_USER.username, FAKE_USER.password);
  if (stay) await page.getByLabel(en['signIn.stay']).check();
  await page.getByTestId('signin-submit').click();
  if (await internet(page) === 'off') await page.getByTestId('net-confirm').click();
  await until('signed in', async () => (await state(page)) === 'in');
}

async function turnOn(page) {
  await openMenu(page);
  await page.getByTestId('account-internet').click();
  await until('on', async () => (await internet(page)) === 'on');
  await page.keyboard.press('Escape');
}

const noInternetStored = async (page, env) => {
  const storage = JSON.parse(await storageDump(page));
  for (const [key] of [...storage[0], ...storage[1]]) assert.doesNotMatch(key, /internet/i, `page storage key ${key}`);
  assert.deepEqual(filesHolding(env.appData, '"internet"'), [], 'no file under app data holds the internet choice');
};

browserTest('a. a launch starts with the internet off: no indicator, the switch off, no question, nothing stored', async () => {
  await withApp(async ({ page, env }) => {
    assert.equal(await internet(page), 'off');
    assert.equal(await page.getByTestId('internet-indicator').count(), 0);
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    await openMenu(page);
    const row = page.getByTestId('account-internet');
    assert.equal(await row.getAttribute('role'), 'menuitemcheckbox');
    assert.equal(await row.getAttribute('aria-checked'), 'false');
    assert.match(await row.textContent(), new RegExp(en['account.internet']));
    assert.match(await row.textContent(), new RegExp(en['account.internetOff'].replace('.', '\\.')));
    await page.keyboard.press('Escape');
    await noInternetStored(page, env);
    await turnOn(page);
    await noInternetStored(page, env);
    await loadHome(page, env);
    assert.equal(await internet(page), 'off', 'a launch starts off again');
    assert.equal((await env.call('net/state', {})).on, false);
  }, opts);
});

browserTest('b. one question after launch, none after Turn on internet; off in the menu asks again; a launch starts off and asks again; nothing is stored', async () => {
  let counted = 0;
  await withApp(async ({ page, env }) => {
    await loadHome(page, env); // the init script counts from this load on
    await openSignIn(page);
    await fill(page, FAKE_USER.username, 'wrong');
    await page.getByTestId('signin-submit').click();
    await page.locator('[data-testid="net-ask"][data-kind="signIn"]').waitFor();
    assert.deepEqual(await userRequests(env.door43), [], 'nothing before the answer');
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('signin-error').waitFor();
    const first = await userRequests(env.door43);
    assert.deepEqual(first.map((r) => [r.method, r.path, r.auth]), [['GET', '/api/v1/user', `basic:${FAKE_USER.username}`]]);
    // On: the next press asks nothing and sends again.
    await page.getByLabel(en['signIn.password'], { exact: true }).fill('wrong');
    await page.getByTestId('signin-submit').click();
    await until('sent again', async () => (await userRequests(env.door43)).length === 2);
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    // Off in the menu: the next press asks; Not now sends nothing and keeps the step.
    await page.keyboard.press('Escape');
    await until('the step closes', async () => (await page.getByTestId('share-signin').count()) === 0);
    await openMenu(page);
    await page.getByTestId('account-internet').click();
    await until('off', async () => (await internet(page)) === 'off');
    await page.keyboard.press('Escape');
    await openSignIn(page);
    await fill(page, FAKE_USER.username, 'wrong');
    await page.getByTestId('signin-submit').click();
    await page.locator('[data-testid="net-ask"][data-kind="signIn"]').waitFor();
    await page.getByTestId('net-cancel').click();
    await sleep(300);
    assert.equal((await userRequests(env.door43)).length, 2, 'Not now sends nothing');
    assert.equal(await page.getByTestId('share-signin').count(), 1, 'the step stays open');
    counted += await asks(page);
    // A launch starts off and asks again.
    await loadHome(page, env);
    assert.equal(await internet(page), 'off');
    await openSignIn(page);
    await fill(page, FAKE_USER.username, 'wrong');
    await page.getByTestId('signin-submit').click();
    await page.locator('[data-testid="net-ask"][data-kind="signIn"]').waitFor();
    counted += await asks(page);
    assert.equal(counted, 3, 'three questions in all');
    await noInternetStored(page, env);
  }, { ...opts, init: (context) => countAsks(context) });
});

browserTest('b. signing in and signing out leave the internet state alone', async () => {
  await withApp(async ({ page }) => {
    await turnOn(page);
    await signIn(page);
    assert.equal(await internet(page), 'on');
    await openMenu(page);
    await page.getByTestId('account-sign-out').click();
    await until('out', async () => (await state(page)) === 'out');
    assert.equal(await internet(page), 'on');
  }, opts);
});

browserTest('c. signed out: Sign in to Door43; no Sign out; nothing is sent', async () => {
  await withApp(async ({ page, env }) => {
    assert.equal(await state(page), 'out');
    await openMenu(page);
    assert.match(await page.getByTestId('account-sign-in').textContent(), new RegExp(en['account.signIn']));
    for (const id of ['account-page', 'account-check', 'account-sign-out']) assert.equal(await page.getByTestId(id).count(), 0, id);
    await sleep(300);
    assert.deepEqual(await env.door43.requests(), []);
  }, opts);
});

browserTest('c. sign-in saved but not checked: no identity, nothing sent; Check asks; Not now sends nothing; Turn on internet checks with one request', async () => {
  await withApp(async ({ page, env }) => {
    await keptToken(page, env);
    await openMenu(page);
    const check = page.getByTestId('account-check');
    assert.match(await check.textContent(), new RegExp(en['account.saved']));
    assert.match(await check.textContent(), new RegExp(en['account.savedCheck']));
    assert.equal(await page.getByTestId('account-page').count(), 0);
    assert.equal(await page.getByTestId('account-sign-out').count(), 1);
    assert.doesNotMatch(await panel(page).textContent(), new RegExp(FAKE_USER.username));
    await sleep(500);
    assert.deepEqual(await env.door43.requests(), [], 'a saved sign-in sends nothing by itself');
    await check.click();
    const ask = page.locator('[data-testid="net-ask"][data-kind="checkSignIn"]');
    await ask.waitFor();
    assert.match(await ask.textContent(), new RegExp(en['net.ask.checkSignIn'].replace('.', '\\.')));
    await page.getByTestId('net-cancel').click();
    await sleep(300);
    assert.deepEqual(await env.door43.requests(), []);
    assert.equal(await state(page), 'saved');
    assert.equal(await internet(page), 'off');
    assert.equal((await keychain(env)).holds, true);
    await openMenu(page);
    await page.getByTestId('account-check').click();
    await page.getByTestId('net-confirm').click();
    await until('signed in', async () => (await state(page)) === 'in');
    assert.equal(await internet(page), 'on');
    assert.deepEqual((await env.door43.requests()).map((r) => [r.method, r.path, r.auth]), [['GET', '/api/v1/user', 'token']]);
  }, opts);
});

browserTest('c. signed in: the login and Open my page on Door43; the page opens at once while the internet is on and asks after it is turned off', async () => {
  await withApp(async ({ page, context, env }) => {
    const profile = `${env.door43.url}/${FAKE_USER.username}`;
    await turnOn(page);
    await signIn(page);
    await openMenu(page);
    assert.match(await panel(page).textContent(), new RegExp(`@${FAKE_USER.username}`));
    assert.match(await page.getByTestId('account-page').textContent(), new RegExp(en['account.openPage']));
    assert.equal(await page.getByTestId('account-sign-out').count(), 1);
    assert.equal(await page.getByTestId('account-sign-in').count(), 0);
    let opened = context.waitForEvent('page');
    await page.getByTestId('account-page').click();
    let popup = await opened;
    await until('the profile opens', async () => popup.url() === profile);
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    await popup.close();
    // Off: it asks; Not now opens nothing; Turn on opens it.
    await openMenu(page);
    await page.getByTestId('account-internet').click();
    await until('off', async () => (await internet(page)) === 'off');
    await page.getByTestId('account-page').click();
    await page.locator('[data-testid="net-ask"][data-kind="profile"]').waitFor();
    await page.getByTestId('net-cancel').click();
    await sleep(400);
    assert.equal(context.pages().length, 1, 'Not now opens nothing');
    await openMenu(page);
    opened = context.waitForEvent('page');
    await page.getByTestId('account-page').click();
    await page.getByTestId('net-confirm').click();
    popup = await opened;
    await until('the profile opens', async () => popup.url() === profile);
    await popup.close();
  }, {
    ...opts,
    // The window would load the fake Door43's page: a stub answers it in the browser.
    init: (context, env) => context.route(`${env.door43.url}/${FAKE_USER.username}`, (route) => route.fulfill({ contentType: 'text/html', body: '<title>stub</title>' })),
  });
});

browserTest('c. the signed-in account button is tC4\'s', async () => {
  await withApp(async ({ page }) => {
    await turnOn(page);
    await signIn(page);
    const want = JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'view-gallery', 'dom', 'share-done.json'), 'utf8'));
    // The span around the button and the dot, with its subtree.
    const wrapper = (snap) => {
      const lines = snap.root;
      const depth = (l) => l.length - l.trimStart().length;
      const at = lines.findIndex((l) => l.includes('data-testid="account-menu"'));
      let parent = at - 1;
      while (depth(lines[parent]) >= depth(lines[at])) parent -= 1;
      // Where the focus is says how the page got here, not how the button looks.
      return subtreeOf({ ...snap, root: lines.slice(parent) }, lines[parent].trim()).map((l) => l.replace(/ \.focused>$/, '>'));
    };
    assert.deepEqual(wrapper(await snapshot(page)), wrapper(want));
  }, opts);
});

browserTest('d. sign-in from the menu asks when the password is sent; Not now keeps the dialog and sends nothing; Turn on internet signs in', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await page.waitForFunction(() => window.__tcIdle === true);
    const mark = requests.mark();
    await openSignIn(page);
    await sleep(300);
    assert.deepEqual(requests.api(mark), [], 'opening the step makes no API request');
    assert.equal(await page.getByTestId('net-ask').count(), 0, 'opening the step asks nothing');
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    const ask = page.locator('[data-testid="net-ask"][data-kind="signIn"]');
    await ask.waitFor();
    assert.match(await ask.textContent(), new RegExp(en['net.ask.signIn'].replace('.', '\\.')));
    await page.getByTestId('net-cancel').click();
    await sleep(300);
    assert.deepEqual(await env.door43.requests(), []);
    assert.equal(await state(page), 'out');
    assert.equal(await page.getByTestId('share-signin').count(), 1);
    await page.getByTestId('signin-submit').click();
    await page.getByTestId('net-confirm').click();
    await until('signed in', async () => (await state(page)) === 'in');
    assert.equal(await internet(page), 'on');
    assert.equal(await page.getByTestId('share-signin').count(), 0, 'the step closes');
  }, opts);
});

browserTest('d. Cancel while the sign-in is sent signs nothing in; the step opened after it signs in with no second question', async () => {
  let counted = 0;
  await withApp(async ({ page, env }) => {
    await loadHome(page, env);
    await env.door43.fault({ method: 'GET', path: '/api/v1/user', times: 1, fault: { type: 'hold' } });
    await openSignIn(page);
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByLabel(en['signIn.stay']).check();
    await page.getByTestId('signin-submit').click();
    await page.getByTestId('net-confirm').click();
    await until('Door43 holds the sign-in', async () => (await env.door43.requests()).length === 1);
    await page.getByTestId('signin-cancel').click();
    await openSignIn(page);
    await env.door43.release();
    await quiet(env);
    await sleep(300);
    assert.equal(await state(page), 'out');
    const kc = await keychain(env);
    assert.equal(kc.keeps, 0);
    assert.equal(kc.holds, false);
    assert.deepEqual(await env.door43.tokens(FAKE_USER.username), []);
    assert.equal(await page.getByLabel(en['signIn.password'], { exact: true }).inputValue(), '');
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    await until('signed in', async () => (await state(page)) === 'in');
    counted = await asks(page);
    assert.equal(counted, 1, 'one question in all');
  }, { ...opts, init: (context) => countAsks(context) });
});

browserTest('d. a cancelled sign-in that Door43 answers late changes nothing; the sign-in after it keeps its session', async () => {
  await withApp(async ({ page, env }) => {
    await turnOn(page);
    await env.door43.fault({ method: 'GET', path: '/api/v1/user', times: 1, fault: { type: 'hold' } });
    await openSignIn(page);
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    await until('Door43 holds the sign-in', async () => (await env.door43.requests()).length === 1);
    await page.getByTestId('signin-cancel').click();
    await signIn(page);
    await env.door43.release();
    await quiet(env);
    await sleep(300);
    assert.equal(await state(page), 'in');
    assert.deepEqual((await env.door43.tokens(FAKE_USER.username)).map((t) => t.name), ['translationCore']);
    assert.equal((await env.call('account/state', {})).user, FAKE_USER.username);
  }, opts);
});

browserTest('d. two quick Sign in presses send one sign-in; Cancel then signs nothing in', async () => {
  await withApp(async ({ page, env }) => {
    await turnOn(page);
    await env.door43.fault({ method: 'GET', path: '/api/v1/user', times: 1, fault: { type: 'hold' } });
    await openSignIn(page);
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByTestId('signin-submit').dblclick();
    await until('Door43 holds the sign-in', async () => (await env.door43.requests()).length === 1);
    await sleep(300);
    assert.equal((await env.door43.requests()).filter((r) => r.auth.startsWith('basic:')).length, 1, 'one sign-in');
    await page.getByTestId('signin-cancel').click();
    await env.door43.release();
    await quiet(env);
    await sleep(300);
    assert.equal(await state(page), 'out');
    assert.deepEqual(await env.door43.tokens(FAKE_USER.username), []);
  }, opts);
});

browserTest('e. (Home half) startup, idle and the menu send nothing, before and after Turn on internet; a kept sign-in does not change it', async () => {
  await withApp(async ({ page, env }) => {
    await keptToken(page, env);
    await sleep(5_000);
    await openMenu(page);
    await page.keyboard.press('Escape');
    assert.deepEqual(await env.door43.requests(), []);
    assert.deepEqual(env.refused(), []);
    await turnOn(page);
    await sleep(2_000);
    assert.deepEqual(await env.door43.requests(), [], 'turning the internet on sends nothing by itself');
  }, { ...opts, timeout: 60_000 });
});

browserTest('f. a switch that does not turn on stops the step: net-failed, nothing runs, nothing is sent; the next step asks again', async () => {
  await withApp(async ({ page, env }) => {
    const failing = (route) => route.fulfill({
      status: 200, contentType: 'application/json',
      body: JSON.stringify({ type: 'err', value: { type: 'internal', value: { report: 0 } } }),
    });
    await page.route('**/api/v1/net/set', failing);
    await openSignIn(page);
    await fill(page, FAKE_USER.username, FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    await page.getByTestId('net-confirm').click();
    const failed = page.getByTestId('net-failed');
    await failed.waitFor();
    assert.match(await failed.textContent(), new RegExp(en['net.failed.body'].replace(/[.,]/g, '.')));
    assert.equal(await page.getByTestId('share-signin').count(), 1, 'the step stays open');
    assert.deepEqual(await env.door43.requests(), []);
    assert.equal(await internet(page), 'off');
    await page.getByTestId('net-failed-close').click();
    await page.getByTestId('signin-submit').click();
    await page.locator('[data-testid="net-ask"][data-kind="signIn"]').waitFor();
    await page.unroute('**/api/v1/net/set');
    await page.getByTestId('net-confirm').click();
    await until('signed in', async () => (await state(page)) === 'in');
  }, opts);
});

browserTest('g. sign out sends no request, forgets the saved token, and stays out at the next launch', async () => {
  await withApp(async ({ page, env }) => {
    await keptToken(page, env);
    await openMenu(page);
    await page.getByTestId('account-sign-out').click();
    await until('out', async () => (await state(page)) === 'out');
    const kc = await keychain(env);
    assert.equal(kc.holds, false);
    assert.ok(kc.forgets >= 1);
    await openMenu(page);
    assert.equal(await page.getByTestId('account-error').count(), 0);
    await page.keyboard.press('Escape');
    assert.deepEqual(await env.door43.requests(), []);
    await keychain(env, { relaunch: true });
    await relaunch(page, env);
    assert.equal(await state(page), 'out');
    await openMenu(page);
    assert.equal(await page.getByTestId('account-sign-out').count(), 0);
  }, opts);
});

browserTest('g. a keychain that cannot forget: the failure shows and the sign-in stays saved', async () => {
  await withApp(async ({ page, env }) => {
    await keptToken(page, env, { mode: 'failingForget' });
    await openMenu(page);
    await page.getByTestId('account-sign-out').click();
    await openMenu(page);
    const error = page.getByTestId('account-error');
    await error.waitFor();
    assert.match(await error.textContent(), new RegExp(en['account.signOutFailed'].replace('.', '\\.')));
    assert.equal(await state(page), 'saved');
    assert.equal((await keychain(env)).holds, true);
  }, opts);
});

browserTest('h. keyboard with a saved sign-in: Enter opens on the first row, the arrows wrap, Escape returns the focus, an outside click closes', async () => {
  await withApp(async ({ page, env }) => {
    await keptToken(page, env);
    await trigger(page).focus();
    await page.keyboard.press('Enter');
    await panel(page).waitFor();
    const rows = await panel(page).locator('[role^="menuitem"]').evaluateAll((all) => all.map((r) => r.getAttribute('data-testid')));
    assert.deepEqual(rows, ['account-check', 'account-internet', 'account-help', 'account-language', 'account-report', 'account-about', 'account-sign-out']);
    await until('the first row has the focus', async () => (await focused(page)) === 'account-check');
    await page.keyboard.press('ArrowUp');
    await until('wraps to the last', async () => (await focused(page)) === 'account-sign-out');
    await page.keyboard.press('ArrowDown');
    await until('wraps to the first', async () => (await focused(page)) === 'account-check');
    await page.keyboard.press('Escape');
    await until('closed', async () => (await panel(page).count()) === 0);
    await until('the focus returns', async () => (await focused(page)) === 'account-menu');
    await openMenu(page);
    await page.mouse.click(10, 700);
    await until('an outside click closes', async () => (await panel(page).count()) === 0);
    assert.deepEqual(await env.door43.requests(), []);
  }, opts);
});
