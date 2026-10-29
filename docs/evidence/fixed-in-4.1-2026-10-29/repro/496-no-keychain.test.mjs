// tC4 #496 on the real app: tc4 (test hooks) launched once with a working keychain and once with
// none; open "Sign in to Door43" from the account menu each time and read the step: is the
// "Stay signed in on this computer" box there, and what line stands under the fields. Then sign
// in without a keychain against the fake Door43 and relaunch: the sign-in lasted one session.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/496-no-keychain.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { FAKE_USER } from '../../../../parity/tc4-baseline/mock-server/fake-door43.mjs';
import { browserTest, catalog, keychain, openMenu, trigger, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const en = catalog('en');

async function relaunch(page, env, set = {}) {
  await keychain(env, { ...set, relaunch: true });
  await page.goto('about:blank');
  await page.goto(`${env.origin}/#t=${env.token}`);
  await page.getByTestId('account-menu').waitFor();
}
async function readStep(page, shot) {
  await openMenu(page);
  await page.getByTestId('account-sign-in').click();
  await page.getByTestId('share-signin').waitFor();
  const got = {
    stayBox: await page.getByTestId('signin-stay').count(),
    stayLabel: await page.getByText(en['signIn.stay']).count(),
    reason: (await page.getByTestId('signin-reason').textContent()).trim(),
    notKeptLineShown: await page.getByText(en['signIn.notKept']).count(),
  };
  await page.getByTestId('share-signin').screenshot({ path: path.join(SHOTS, shot) });
  return got;
}

browserTest('496_stay_signed_in_shows_only_where_a_sign_in_can_be_kept', async () => {
  await withApp(async ({ page, env }) => {
    await relaunch(page, env, { mode: 'working' });
    const working = await readStep(page, '496-signin-keychain.png');
    console.log('with a keychain:', JSON.stringify(working));
    await page.getByTestId('signin-cancel').click();

    await relaunch(page, env, { mode: 'none' });
    const none = await readStep(page, '496-signin-no-keychain.png');
    console.log('without a keychain:', JSON.stringify(none));
    await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
    await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
    if ((await trigger(page).getAttribute('data-internet')) === 'off') await page.getByTestId('net-confirm').click();
    await until('signed in', async () => (await trigger(page).getAttribute('data-state')) === 'in');
    console.log('signed in without a keychain; keeps:', (await keychain(env)).keeps);
    await relaunch(page, env);
    console.log('after a relaunch the account is:', await trigger(page).getAttribute('data-state'));

    assert.equal(working.stayBox, 1, 'the box shows with a keychain');
    assert.equal(none.stayBox, 0, 'no box without a keychain');
  }, { door43: true });
});
