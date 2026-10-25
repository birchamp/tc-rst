// A read-only open (APP-DESIGN 6.4, unit UI-3b): a project another computer holds open opens
// read-only, and the align and check controls are off before any click instead of failing
// after it. No `align/edit` or `check/decide` request leaves the page.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/read-only.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, catalog, sleep, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const en = catalog('en');

/** `.tc/lock` as another computer leaves it (STORAGE §3.6): the open is read-only. */
function lockedElsewhere(env) {
  const tc = path.join(env.projectsDir, BIBLE, '.tc');
  fs.mkdirSync(tc, { recursive: true });
  fs.writeFileSync(path.join(tc, 'lock'), JSON.stringify({ host: 'other-computer', pid: 4242, device: 'Other', at: '2026-10-25T09:00:00Z' }));
}

/** Requests that would write the project. */
function writes(page) {
  const sent = [];
  page.on('request', (request) => {
    if (/\/api\/v1\/(align\/edit|check\/decide)$/.test(new URL(request.url()).pathname)) sent.push(request.url());
  });
  return sent;
}

async function openTitus(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByRole('tab', { name: en['nav.check'], exact: true }).waitFor();
}

browserTest('read_only_align_takes_no_click_or_drag_and_sends_no_edit', async () => {
  await withApp(async ({ page, env }) => {
    lockedElsewhere(env);
    const sent = writes(page);
    await openTitus(page);
    await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    const chips = page.locator('[data-testid="align-bank"] > button');
    assert.ok(await chips.count() > 0, 'the bank shows its words');
    for (const chip of await chips.all()) {
      assert.equal(await chip.isDisabled(), true, 'a bank word takes no click');
      assert.equal(await chip.getAttribute('draggable'), 'false', 'a bank word takes no drag');
    }
    const cards = page.locator('[data-testid^="align-card-"]');
    assert.ok(await cards.count() > 0, 'the cards show');
    for (const header of await cards.locator('> div[draggable]').all()) {
      assert.equal(await header.getAttribute('draggable'), 'false', 'a card takes no drag');
    }
    assert.equal(await page.getByTestId('align-mark-valid').isDisabled(), true, 'Mark valid is off');
    // A click on a disabled control and a drop of a word on a card both send nothing.
    await chips.first().click({ force: true });
    await cards.first().dispatchEvent('drop');
    await page.getByTestId('align-mark-valid').click({ force: true });
    await sleep(500);
    assert.deepEqual(sent, [], 'no align edit leaves the page');
  });
});

browserTest('read_only_check_marks_are_disabled_and_send_no_decision', async () => {
  await withApp(async ({ page, env }) => {
    lockedElsewhere(env);
    const sent = writes(page);
    await openTitus(page);
    await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-quote').waitFor();
    for (const id of ['mark-valid', 'mark-invalid', 'mark-todo', 'bookmark']) {
      assert.equal(await page.getByTestId(id).isDisabled(), true, `${id} is off`);
      await page.getByTestId(id).click({ force: true });
    }
    await sleep(500);
    assert.deepEqual(sent, [], 'no check decision leaves the page');
  });
});
