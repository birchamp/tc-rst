// The account menu (X6, tC4 D88, D95): its rows in tC4's order, the keyboard, the internet
// switch that stays open, and a menu that sends nothing when it opens.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/account-menu.test.mjs
import assert from 'node:assert/strict';
import {
  browserTest, catalog, focused, openMenu, panel, recordRequests, sleep, trigger, until, withApp,
} from './kit.mjs';

const rowIds = (page) => panel(page).locator('[role^="menuitem"]').evaluateAll((rows) => rows.map((r) => r.getAttribute('data-testid')));

browserTest('the_menu_has_tc4s_rows_in_order_for_signed_out', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    assert.deepEqual(await rowIds(page), [
      'account-sign-in', 'account-internet', 'account-help', 'account-language', 'account-report', 'account-about',
    ]);
    assert.equal(await page.getByTestId('account-sign-out').count(), 0, 'no sign-out with nobody signed in');
    assert.equal(await trigger(page).getAttribute('aria-expanded'), 'true');
    assert.equal(await focused(page), 'account-sign-in', 'the first row takes the focus');
    const en = catalog('en');
    assert.match(await page.getByTestId('account-language').textContent(), /English/);
    assert.match(await page.getByTestId('account-sign-in').textContent(), new RegExp(en['account.signInHint'].replace('.', '\\.')));
  });
});

browserTest('the_keys_move_wrap_and_tab_closes_the_menu', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    for (const [key, want] of [
      ['ArrowDown', 'account-internet'], ['End', 'account-about'], ['ArrowDown', 'account-sign-in'],
      ['ArrowUp', 'account-about'], ['Home', 'account-sign-in'],
    ]) {
      await page.keyboard.press(key);
      await until(`${key} focuses ${want}`, async () => (await focused(page)) === want);
    }
    await page.keyboard.press('Escape');
    await until('the menu closes', async () => (await panel(page).count()) === 0);
    await until('the focus returns to the button', async () => (await focused(page)) === 'account-menu');
    await page.keyboard.press('Enter');
    await panel(page).waitFor();
    await until('the first row again', async () => (await focused(page)) === 'account-sign-in');
    // tC4's Tab does not stop the browser's move; the menu closes.
    await page.keyboard.press('Tab');
    await until('Tab closes the menu', async () => (await panel(page).count()) === 0);
    assert.equal(await trigger(page).getAttribute('aria-expanded'), 'false');
  });
});

browserTest('the_internet_row_stays_open_and_switches', async () => {
  await withApp(async ({ page, env }) => {
    const en = catalog('en');
    await openMenu(page);
    const row = page.getByTestId('account-internet');
    assert.equal(await row.getAttribute('aria-checked'), 'false');
    await row.click();
    await until('on', async () => (await trigger(page).getAttribute('data-internet')) === 'on');
    assert.equal(await panel(page).count(), 1, 'the menu stays open');
    assert.equal(await row.getAttribute('aria-checked'), 'true');
    assert.equal(await page.getByTestId('internet-indicator').count(), 1);
    assert.equal((await env.call('net/state', {})).on, true);
    assert.match(await row.textContent(), new RegExp(en['account.internetOn']));
    assert.equal(await trigger(page).getAttribute('title'), en['net.on.tooltip']);
    assert.equal(await trigger(page).getAttribute('aria-label'), `${en['account.label.out']}. ${en['net.on.tooltip']}`);
    assert.equal(await focused(page), 'account-internet', 'the row keeps the focus');
    await row.click();
    await until('off', async () => (await trigger(page).getAttribute('data-internet')) === 'off');
    assert.equal(await page.getByTestId('internet-indicator').count(), 0);
    assert.equal((await env.call('net/state', {})).on, false);
    assert.equal(await trigger(page).getAttribute('title'), en['account.label.out']);
  });
});

browserTest('opening_the_menu_sends_nothing', async () => {
  await withApp(async ({ page }) => {
    const requests = recordRequests(page);
    await page.waitForFunction(() => window.__tcIdle === true);
    const mark = requests.mark();
    await openMenu(page);
    await sleep(500);
    await page.keyboard.press('Escape');
    await sleep(300);
    assert.deepEqual(requests.since(mark), [], 'no request at all');
  });
});
