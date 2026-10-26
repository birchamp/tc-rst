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

/** Drops `payload` on `target` as a real drag would carry it. */
async function drop(page, target, payload) {
  const transfer = await page.evaluateHandle((text) => {
    const t = new DataTransfer();
    t.setData('text/plain', text);
    return t;
  }, JSON.stringify(payload));
  await target.dispatchEvent('drop', { dataTransfer: transfer });
}

async function openAlign(page) {
  await openTitus(page);
  await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
  await page.getByTestId('open-align').click();
  await page.getByTestId('align-bank').waitFor();
}

const bankChips = (page) => page.locator('[data-testid="align-bank"] > button');
const cardHeaders = (page) => page.locator('[data-testid^="align-card-"] > div[draggable]');
const placedWords = (page) => page.locator('[data-testid^="align-card-"] button[data-word]');

browserTest('a_writable_open_has_its_align_and_check_controls_on', async () => {
  await withApp(async ({ page }) => {
    await openAlign(page);
    assert.equal(await bankChips(page).first().isDisabled(), false, 'a bank word takes a click');
    assert.equal(await bankChips(page).first().getAttribute('draggable'), 'true');
    assert.equal(await cardHeaders(page).first().getAttribute('draggable'), 'true');
    assert.equal(await page.getByTestId('align-mark-valid').isDisabled(), false);
    assert.equal(await page.getByTestId('align-read-only').count(), 0, 'no read-only sentence');
  });
});

browserTest('read_only_align_takes_no_click_or_drag_and_sends_no_edit', async () => {
  await withApp(async ({ page, env }) => {
    lockedElsewhere(env);
    const sent = writes(page);
    await openAlign(page);
    assert.match(await page.getByTestId('align-read-only').textContent(), /another computer/i, 'the aligner says why');
    const groups = [['bank word', bankChips(page)], ['card', cardHeaders(page)], ['placed word', placedWords(page)]];
    for (const [what, all] of groups) {
      assert.ok(await all.count() > 0, `a ${what} shows`);
      for (const one of await all.all()) {
        assert.equal(await one.getAttribute('draggable'), 'false', `a ${what} takes no drag`);
        assert.equal(await one.getAttribute('title'), null, `a ${what} offers no action`);
      }
    }
    for (const chip of await bankChips(page).all()) assert.equal(await chip.isDisabled(), true, 'a bank word takes no click');
    for (const word of await placedWords(page).all()) assert.equal(await word.isDisabled(), true, 'a placed word takes no click');
    for (const id of ['align-mark-valid', 'align-suggest-accept-all', 'align-suggest-reject-all', 'align-suggest-reset']) {
      const control = page.getByTestId(id);
      if (await control.count()) assert.equal(await control.isDisabled(), true, `${id} is off`);
    }
    const cursor = await page.getByTestId('align-mark-valid').evaluate((b) => getComputedStyle(b).cursor);
    assert.equal(cursor, 'default', 'Mark valid no longer reads as clickable');
    // A drop with a real payload (a card merged into the first): the card's own handler refuses it.
    await drop(page, page.locator('[data-testid^="align-card-"]').first(), { kind: 'card', from: 1 });
    await sleep(500);
    assert.deepEqual(sent, [], 'INV-READ-ONLY-HAS-NO-WRITE-CONTROLS: no align edit leaves the page');
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
    assert.match(await page.getByTestId('check-read-only').textContent(), /another computer/i, 'the check says why');
    for (const id of ['mark-valid', 'mark-invalid', 'mark-todo', 'bookmark']) {
      assert.equal(await page.getByTestId(id).isDisabled(), true, `${id} is off`);
      assert.equal(await page.getByTestId(id).evaluate((b) => getComputedStyle(b).cursor), 'default', `${id} no longer reads as clickable`);
    }
    await page.getByTestId('comment-toggle').click();
    assert.equal(await page.getByTestId('comment-text').getAttribute('readonly'), '', 'the comment takes no typing');
    await sleep(500);
    assert.deepEqual(sent, [], 'INV-READ-ONLY-HAS-NO-WRITE-CONTROLS: no check decision leaves the page');
  });
});
