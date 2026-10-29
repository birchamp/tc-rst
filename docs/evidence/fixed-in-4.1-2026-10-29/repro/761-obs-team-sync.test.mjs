// #761 repro on the real app with the fake Door43 for real (realSync): A's shared Open Bible
// Stories project shows "Sync with Door43" on Home and syncs from it; teammate B (a second tc4,
// driven through its API) opens it from Door43 (no sync.obs-unsupported), edits a frame and
// syncs; A presses Sync and gets B's frame; then both edit one frame and A's sync counts it.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/761-obs-team-sync.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../../../../parity/tc4-baseline/mock-server/fake-door43.mjs';
import { jobEnd, newRequestId } from '../../../../parity/app/env-rust.mjs';
import { OBS } from '../../../../parity/app/screens.mjs';
import { browserTest, catalog, withApp } from '../../../../parity/app/journeys/kit.mjs';

const en = catalog('en');
const FOLDER = 'sample_burrito_obs';
const button = (page) => page.getByTestId(`share-${FOLDER}`);
const toast = (page) => page.getByTestId('sync-result');

async function syncA(page, first = false) {
  await button(page).click();
  if (first) {
    await page.getByTestId('net-confirm').click();
    await page.getByTestId('share-signin').waitFor();
    await page.getByLabel('Username or email').fill(FAKE_USER.username);
    await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
    await page.getByTestId('signin-submit').click();
  }
  await toast(page).waitFor({ timeout: 120_000 });
  const out = {
    ok: await toast(page).getAttribute('data-ok'),
    text: (await toast(page).textContent()).replace(/\s+/g, ' ').trim(),
  };
  for (const id of ['sync-count-received', 'sync-count-conflicts']) {
    const el = page.getByTestId(id);
    out[id] = (await el.count()) ? await el.getAttribute('data-count') : null;
  }
  await toast(page).getByTestId('sync-dismiss').click();
  await toast(page).waitFor({ state: 'detached' });
  return out;
}

/** Frame `frame` (1-based) of story 1, read or saved through `call` on `project`. */
async function withStory(call, project, fn) {
  const at = (await jobEnd(call, await call('session/open', { request: newRequestId(), project, unit: null }))).value.session.at;
  try {
    return await fn(at);
  } finally {
    await call('session/close', { at, request: newRequestId() });
  }
}

browserTest('761_obs_project_syncs_with_a_teammate_from_home', async () => {
  await withApp(async ({ page, env, teammate }) => {
    await button(page).waitFor();
    const card = page.getByTestId(`share-card-${FOLDER}`);
    console.log(`#761 A's story card before connect: action=${await card.getAttribute('data-action')} button="${await button(page).textContent()}"`);
    await env.pushToDoor43(OBS);
    await page.reload();
    await button(page).waitFor();
    const action = await card.getAttribute('data-action');
    const label = await button(page).textContent();
    console.log(`#761 A's shared story card: action=${action} button="${label}" (home.sync="${en['home.sync']}")`);
    assert.equal(action, 'sync');
    const first = await syncA(page, true);
    console.log(`#761 A's first sync: ${JSON.stringify(first)}`);
    assert.equal(first.ok, '1');

    const b = await teammate('teammate-b');
    const pb = await b.join(env.door43Repo(OBS));
    console.log(`#761 B opened from Door43: ${pb}`);
    const bcall = b.env.call;
    const B_FRAME = 'Marco 2, como B lo tradujo.';
    await withStory(bcall, pb, async (at) => {
      const view = await bcall('story/open', { at, story: 1 });
      await bcall('story/saveFrame', { at, request: newRequestId(), shown: view.frames[1].token, text: B_FRAME });
    });
    console.log(`#761 B's sync: ${JSON.stringify(await b.sync(pb))}`);

    const second = await syncA(page);
    console.log(`#761 A's sync after B: ${JSON.stringify(second)}`);
    assert.equal(second.ok, '1');
    assert.ok(Number(second['sync-count-received']) >= 1, 'A received B\'s change');
    const story = fs.readFileSync(path.join(env.projectsDir, FOLDER, 'ingredients', 'content', '01.md'), 'utf8');
    assert.ok(story.includes(B_FRAME), 'A\'s story file holds B\'s frame');

    // Both edit frame 3.
    await withStory(env.call, FOLDER, async (at) => {
      const view = await env.call('story/open', { at, story: 1 });
      await env.call('story/saveFrame', { at, request: newRequestId(), shown: view.frames[2].token, text: 'Marco 3, como A lo dijo.' });
    });
    await withStory(bcall, pb, async (at) => {
      const view = await bcall('story/open', { at, story: 1 });
      await bcall('story/saveFrame', { at, request: newRequestId(), shown: view.frames[2].token, text: 'Marco 3, como B lo dijo.' });
    });
    await b.sync(pb);
    const third = await syncA(page);
    console.log(`#761 A's sync after both edited frame 3: ${JSON.stringify(third)}`);
    assert.equal(third.ok, '1');
    const conflicts = fs.existsSync(path.join(env.projectsDir, FOLDER, 'ingredients', 'tc', 'conflicts.json'))
      ? JSON.parse(fs.readFileSync(path.join(env.projectsDir, FOLDER, 'ingredients', 'tc', 'conflicts.json'), 'utf8'))
      : null;
    console.log(`#761 A's tc/conflicts.json: ${JSON.stringify(conflicts)?.slice(0, 600)}`);
    assert.ok(conflicts, 'the same-frame edit is an open conflict');
    const review = await withStory(env.call, FOLDER, (at) => env.call('conflicts/list', { at, tab: 'text' }));
    console.log(`#761 A's review, Text tab: ${JSON.stringify(review).slice(0, 900)}`);
    // What the person sees: Home's story card, then the story's Translate screen and its review entry.
    await page.screenshot({ path: path.join(import.meta.dirname, '..', 'shots', '761-home-after-frame-conflict.png') });
    const chip = page.getByTestId(`project-${FOLDER}`).getByTestId('conflicts-review');
    console.log(`#761 Home story card conflict chip: count=${await chip.count()}`);
    try {
      await page.getByTestId(`project-${FOLDER}`).getByTestId('story-tile-1').click();
      await page.getByTestId('story-understand').waitFor({ timeout: 15_000 });
      await page.getByRole('tab', { name: 'Translate', exact: true }).click();
      await page.getByTestId('story-draft').waitFor({ timeout: 15_000 });
      const entry = page.getByTestId('conflicts-review');
      console.log(`#761 story Translate review entry: count=${await entry.count()} data-count=${(await entry.count()) ? await entry.first().getAttribute('data-count') : ''}`);
      await page.screenshot({ path: path.join(import.meta.dirname, '..', 'shots', '761-story-screen-after-frame-conflict.png') });
    } catch (e) {
      console.log(`#761 could not open the story screen: ${e.message.split('\n')[0]}`);
    }
  }, { door43: true, realSync: true, scenario: 'shared' });
}, { door43: true, timeout: 170_000 });
