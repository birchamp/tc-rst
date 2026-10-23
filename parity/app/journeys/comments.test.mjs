// C3: the real comment fields, their blur-only writes, revisions and lost/held replies.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, recordRequests, sleep, storageDump, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const OBS = 'sample_burrito_obs';
const scenario = { scenario: 'en-pins' };
const scope = async (env) => (await env.call('app/hello', {})).sessions[0].at;
const book = async (env) => env.call('editor/book', { at: await scope(env), book: 'TIT' });
const story = async (env) => env.call('story/open', { at: await scope(env), story: 1 });
const sent = (requests, mark) => requests.api(mark).filter((p) => p === '/api/v1/editor/addComment');

async function openTitus(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
}

async function emptyBox(page) {
  const boxes = page.locator('[data-testid^="understand-unit-"] textarea');
  for (let i = 0; i < await boxes.count(); i += 1) {
    if ((await boxes.nth(i).inputValue()) === '') return boxes.nth(i);
  }
  assert.fail('the fixture must offer a new-comment field');
}

browserTest('j16_three_help_steps_then_three_pauses_and_a_blur_add_one_comment', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus(page);
    const file = path.join(env.projectsDir, BIBLE, 'ingredients', 'TIT.usfm');
    const bytes = fs.readFileSync(file);
    const before = (await book(env)).comments.length;
    await page.getByTestId('source-tab-ust').click();
    await page.getByTestId('helps-note').first().click();
    await page.getByRole('button', { name: 'Translation Academy →', exact: true }).first().click();
    await page.getByTestId('understand-article').waitFor();
    await page.getByTestId('helps-article-close').click();
    const box = await emptyBox(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    for (const value of ['A comment', 'A comment with more words', 'A comment with all its words']) {
      await box.fill(value);
      await sleep(2100);
      assert.equal(sent(requests, mark).length, 0, 'pauses draft without adding comments');
    }
    const wanted = 'A comment with all its words';
    await box.blur();
    await until('one new comment', async () => {
      const comments = (await book(env)).comments;
      return comments.length === before + 1 && comments.some((c) => c.text === wanted);
    });
    assert.equal(sent(requests, mark).length, 1);
    await box.fill(`${wanted}, revised`);
    await box.blur();
    await until('revision of that same comment', async () => {
      const comments = (await book(env)).comments;
      return comments.length === before + 1 && comments.some((c) => c.text === `${wanted}, revised`);
    });
    await box.fill('');
    await box.blur();
    await until('clearing restores the saved comment', async () => (await box.inputValue()) === `${wanted}, revised`);
    assert.equal(sent(requests, mark).length, 2, 'clearing never silently retracts a comment');
    assert.deepEqual(fs.readFileSync(file), bytes, 'comment writes leave scripture bytes unchanged');
  }, scenario);
});

browserTest('an_ack_for_a_new_comment_preserves_newer_typing_and_establishes_one_identity', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus(page);
    const before = (await book(env)).comments.length;
    const box = await emptyBox(page);
    let release;
    let reached;
    const held = new Promise((resolve) => { release = resolve; });
    const entered = new Promise((resolve) => { reached = resolve; });
    const bodies = [];
    await page.route('**/api/v1/editor/addComment', async (route) => {
      bodies.push(route.request().postDataJSON());
      if (bodies.length !== 1) return route.continue();
      const response = await route.fetch();
      reached();
      await held;
      return route.fulfill({ response });
    });
    try {
      await box.fill('the first comment');
      await box.blur();
      await entered;
      await box.fill('the first comment with newer typing');
      assert.equal(await box.evaluate((el) => el === document.activeElement), true);
      release();
      await until('the Ack leaves newer typing dirty', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'dirty');
      assert.equal(await box.inputValue(), 'the first comment with newer typing');
      assert.equal(await box.evaluate((el) => el === document.activeElement), true, 'cache refresh keeps the editor mounted');
      await box.blur();
      await until('one revised comment', async () => {
        const comments = (await book(env)).comments;
        return comments.length === before + 1 && comments.some((c) => c.text === 'the first comment with newer typing');
      });
      assert.equal(bodies[0].shown, null);
      assert.equal(typeof bodies[1].shown, 'string', 'the later save revises the identity the Ack established');
    } finally {
      release();
    }
  }, scenario);
});

browserTest('a_refused_comment_save_retains_text_and_retry_adds_it_once', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus(page);
    const before = (await book(env)).comments.length;
    const box = await emptyBox(page);
    let refused = false;
    await page.route('**/api/v1/editor/addComment', async (route) => {
      if (refused) return route.continue();
      refused = true;
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({
        type: 'err', value: { type: 'notSaved', value: { type: 'disk', value: 'fixture disk refused comment' } },
      }) });
    });
    const wanted = 'these words survive a refused save';
    await box.fill(wanted);
    await box.blur();
    await until('save failure', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'error');
    assert.equal(await box.inputValue(), wanted);
    assert.ok((await storageDump(page)).includes(wanted));
    assert.equal((await book(env)).comments.length, before);
    await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).click();
    await until('one retried comment', async () => {
      const comments = (await book(env)).comments;
      return comments.length === before + 1 && comments.some((c) => c.text === wanted);
    });
  }, scenario);
});

browserTest('j25_a_frame_comment_uses_its_help_tabs_and_leaves_the_story_unchanged', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`project-${OBS}`).getByTestId('story-tile-1').click();
    const unit = page.getByTestId('story-understand-unit-1');
    await unit.waitFor();
    const bytes = fs.readFileSync(path.join(env.projectsDir, OBS, 'ingredients', 'content', '01.md'));
    await unit.click();
    for (const name of ['Notes', 'Words', 'Questions']) {
      await page.getByTestId('helps-panel').getByRole('tab', { name, exact: true }).click();
    }
    const before = (await story(env)).comments;
    const wanted = 'a user comment on this story frame';
    await unit.getByRole('textbox').fill(wanted);
    await unit.getByRole('textbox').blur();
    await until('the frame comment is saved', async () => (await story(env)).comments.some((c) => c.frame === 1 && c.text === wanted));
    const after = (await story(env)).comments;
    for (const c of before.filter((c) => c.frame !== 1)) assert.ok(after.some((v) => v.frame === c.frame && v.text === c.text));
    assert.deepEqual(fs.readFileSync(path.join(env.projectsDir, OBS, 'ingredients', 'content', '01.md')), bytes);
    await page.getByRole('tab', { name: 'Translate', exact: true }).click();
    await page.getByTestId('story-draft').waitFor();
    await page.getByTestId('helps-panel').getByRole('tab', { name: 'Comments', exact: true }).click();
    await until('Translate lists the frame comment', async () => (await page.getByTestId('helps-comment').allTextContents()).some((text) => text.includes(wanted)));
  }, scenario);
});
