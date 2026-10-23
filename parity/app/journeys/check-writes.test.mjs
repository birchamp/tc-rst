// Check writes (C4) on the real store: J4 (tC4 `e2e/j04-check-book.spec.ts`: a mark on dragged
// words, a comment and a bookmark are one decision record on disk, and the book's text is the
// same bytes), J6 (`e2e/j06-edit-invalidation.spec.ts`: a verse edited in Translate flags its
// checks and keeps their decisions, and the save writes nothing on a decision), J22
// (`e2e/j22-obs-check.spec.ts`: a story's check marked in the browser is stored under its story
// and frame, and a frame edit flags it and keeps it), check-project-switch (#434) and
// check-returns-draft (#532, the story half; check-detail has the Bible half), and the late
// draft (DELTA-10-16 §3.15.6): a check shown before its verse has text shows the text when it
// arrives, with no navigation.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/check-writes.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const OBS = 'sample_burrito_obs';
const SCENARIO = { scenario: 'en-pins' };
const en = catalog('en');

const scopeOf = async (env) => (await env.call('app/hello', {})).sessions[0].at;
const bookList = (tool) => ({ tool, unit: { type: 'book', value: 'TIT' } });
const storyList = (tool) => ({ tool, unit: { type: 'story', value: 1 } });
const item = async (env, list, index) => env.call('check/item', { at: await scopeOf(env), list, index });

const checkTab = (page) => page.getByRole('tab', { name: en['nav.check'], exact: true });
const translateTab = (page) => page.getByRole('tab', { name: en['nav.draft'], exact: true });
const allTools = (page) => page.getByRole('button', { name: /All checking tools/ });
const shownIndex = async (page) => Number(await page.locator('[data-testid="check-list"] [aria-current="true"]').getAttribute('data-rail-index'));
const row = (page, index) => page.locator(`[data-testid="check-list"] [data-rail-index="${index}"]`);
const pressedWords = (page) => page.locator('[data-testid^="tw-"][aria-pressed="true"]').evaluateAll((spans) => spans.map((s) => s.textContent.trim()));

async function openTitus(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await checkTab(page).waitFor();
}

/** Story 1 from its tile; a story opens in Understand the first time (tC4 D87). */
async function openStory(page) {
  await page.getByTestId(`project-${OBS}`).getByTestId('story-tile-1').click();
  await checkTab(page).waitFor();
}

/** From any mode: the Check tab, then `tool` from the picker; a tool still open (the switch to
 * Translate leaves it open, #532) goes back to the picker first. */
async function openTool(page, tool) {
  await checkTab(page).click();
  const picker = page.getByTestId(`open-${tool}`);
  await picker.or(allTools(page)).first().waitFor();
  if (!(await picker.count())) await allTools(page).click();
  await picker.click();
  await page.getByTestId('check-progress').waitFor();
  await page.getByTestId('check-quote').waitFor();
}

/** Check `index` of the rail shown. */
async function select(page, index) {
  await row(page, index).click();
  await until(`check ${index} shows`, async () => (await shownIndex(page)) === index);
  await page.getByTestId('check-target').waitFor();
}

/** Home, and the leave's checkpoint written: the project's files are what it holds. */
async function leave(page, env, project) {
  await page.getByTitle(/Switch project/).click();
  await page.getByTestId(`project-${project}`).waitFor();
  await env.call('settings/viewFromHome', { project });
}

const decisionFile = (env, project, tool, book) =>
  JSON.parse(fs.readFileSync(path.join(env.projectsDir, project, 'ingredients', 'checking', tool, `${book}.json`), 'utf8'));

browserTest('j4_a_mark_on_dragged_words_a_comment_and_a_bookmark_are_one_decision_record_on_disk', async () => {
  await withApp(async ({ page, env }) => {
    const usfm = path.join(env.projectsDir, BIBLE, 'ingredients', 'TIT.usfm');
    const text = fs.readFileSync(usfm);
    await openTitus(page);
    await openTool(page, 'translationNotes');
    await select(page, 0);
    await page.getByTestId('tw-4').waitFor();
    const box = async (i) => page.getByTestId(`tw-${i}`).boundingBox();
    const from = await box(1);
    const to = await box(3);
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 6 });
    await page.mouse.up();
    const words = await pressedWords(page);
    assert.equal(words.length, 3, 'the drag selects three whole words');
    await page.getByTestId('mark-valid').click();
    await page.getByTestId('comment-toggle').click();
    await page.getByTestId('comment-text').fill('Revisar con el equipo.');
    await page.getByTestId('comment-done').click();
    await page.getByTestId('bookmark').click();
    const list = bookList('translationNotes');
    const stored = await until('the mark, the comment and the bookmark are stored', async () => {
      const view = await item(env, list, 0);
      const d = view.decision.value;
      return d.status === 'valid' && d.comment && d.bookmarked ? view : null;
    });
    assert.deepEqual(stored.target.selected.map((i) => stored.target.words[i]), words);
    assert.equal(stored.decision.marker, null, 'the person\'s own three writes are no conflict');
    await until('the rail row says so', async () => {
      const r = row(page, 0);
      return (await r.getAttribute('data-decided')) === '1' && (await r.getAttribute('data-bookmarked')) === '1' && (await r.getAttribute('data-commented')) === '1';
    });

    await leave(page, env, BIBLE);
    const file = decisionFile(env, BIBLE, 'translationNotes', 'TIT');
    const mine = file.decisions.filter((d) => d.comments === 'Revisar con el equipo.');
    assert.equal(mine.length, 1, 'one record holds the mark, the comment and the bookmark');
    const [record] = mine;
    assert.equal(record.status, 'valid');
    assert.equal(record.reminders, true);
    assert.equal(record.invalidated, false);
    assert.deepEqual(record.selections.map((s) => s.text), words);
    assert.deepEqual(fs.readFileSync(usfm), text, 'checking wrote nothing into the book');

    // They survive reopening; clearing the comment stores `false` in the same record.
    await openTitus(page);
    await page.getByTestId('check-quote').waitFor();
    await select(page, 0);
    assert.equal(await page.getByTestId('comment-toggle').getAttribute('data-written'), '1');
    assert.equal(await page.getByTestId('bookmark').getAttribute('data-active'), '1');
    assert.deepEqual(await pressedWords(page), words);
    await page.getByTestId('comment-toggle').click();
    await page.getByTestId('comment-delete').click();
    await page.getByTestId('comment-editor').waitFor({ state: 'detached' });
    await until('the comment is cleared', async () => (await item(env, list, 0)).decision.value.comment === null);
    await leave(page, env, BIBLE);
    const cleared = decisionFile(env, BIBLE, 'translationNotes', 'TIT').decisions
      .filter((d) => d.reminders === true && d.status === 'valid' && JSON.stringify(d.selections.map((s) => s.text)) === JSON.stringify(words));
    assert.equal(cleared.length, 1);
    assert.equal(cleared[0].comments, false);
  }, SCENARIO);
});

// J6: the seeded tW decision on Titus 1:1 selects "Dios". Editing the verse in Translate so that
// it no longer holds the word flags the check and keeps the decision; the verse's save writes
// nothing on the decision (STORAGE §0 item 4), so its version is the one it had.
browserTest('j6_a_verse_edited_in_translate_flags_its_checks_and_keeps_their_decisions', async () => {
  await withApp(async ({ page, env }) => {
    const list = bookList('translationWords');
    await openTitus(page);
    await openTool(page, 'translationWords');
    const index = 2;
    const before = await item(env, list, index);
    assert.equal(before.decision.value.status, 'valid');
    assert.deepEqual(before.target.selected.map((i) => before.target.words[i]), ['Dios']);
    assert.equal(before.decision.value.invalidated, false);

    await translateTab(page).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByTitle('Edit this verse').first().click();
    const verse = page.getByLabel('Verse 1', { exact: true });
    await verse.waitFor();
    const edited = (await verse.inputValue()).replaceAll('Dios', 'Señor');
    await verse.fill(edited);
    await page.getByRole('button', { name: 'Save verse' }).click();
    await verse.waitFor({ state: 'detached' });
    await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved');

    await checkTab(page).click();
    await page.getByTestId('check-quote').waitFor();
    await select(page, index);
    await page.getByTestId('invalidated-notice').waitFor();
    assert.equal(await row(page, index).getAttribute('data-invalid'), '1', 'the rail flags the check');
    const after = await item(env, list, index);
    assert.equal(after.decision.value.invalidated, true);
    assert.equal(after.decision.token, before.decision.token, 'the verse save wrote nothing on the decision');

    await leave(page, env, BIBLE);
    const kept = decisionFile(env, BIBLE, 'translationWords', 'TIT').decisions
      .filter((d) => d.contextId.reference.chapter === 1 && d.contextId.reference.verse === 1 && (d.selections || []).some((s) => s.text === 'Dios'));
    assert.equal(kept.length, 1, 'the decision is kept with its words');
    assert.equal(kept[0].invalidated, true);
    assert.equal(kept[0].userStatus, 'valid', 'the person\'s own mark is kept');
  }, SCENARIO);
});

// J22: a note of frame 1, one tapped word, Mark valid: the record is under {story: 1, frame: 1}
// with that word, the story file is the same bytes; an edit of frame 1 in Translate that drops
// the word flags the check and keeps the record.
browserTest('j22_a_story_check_marked_in_the_browser_is_stored_under_its_frame_and_a_frame_edit_flags_it', async () => {
  await withApp(async ({ page, env }) => {
    const storyFile = path.join(env.projectsDir, OBS, 'ingredients', 'content', '01.md');
    const storyBytes = fs.readFileSync(storyFile);
    const list = storyList('translationNotes');
    await openStory(page);
    await openTool(page, 'translationNotes');
    const index = 1;
    await select(page, index);
    assert.equal(await row(page, index).getAttribute('data-ref'), '1:1');
    assert.equal(await page.getByTestId('check-target').getAttribute('data-drafted'), '1');
    const word = page.getByTestId('check-target').locator('[data-testid^="tw-"]', { hasText: 'principio' }).first();
    await word.click();
    assert.equal(await word.getAttribute('aria-pressed'), 'true');
    await page.getByTestId('mark-valid').click();
    const marked = await until('the mark is stored', async () => {
      const view = await item(env, list, index);
      return view.decision.value.status === 'valid' ? view : null;
    });
    await until('the rail row is decided', async () => (await row(page, index).getAttribute('data-decided')) === '1');

    await leave(page, env, OBS);
    const [record, ...more] = decisionFile(env, OBS, 'translationNotes', 'OBS').decisions;
    assert.equal(more.length, 0);
    assert.deepEqual(record.contextId.reference, { story: 1, frame: 1 });
    assert.equal(record.status, 'valid');
    assert.deepEqual(record.selections, [{ text: 'principio', occurrence: 1, occurrences: 1 }]);
    assert.deepEqual(fs.readFileSync(storyFile), storyBytes, 'checking wrote nothing into the story');

    await openStory(page);
    await translateTab(page).click();
    await page.getByTestId('story-draft').waitFor();
    await page.getByTestId('story-frame-1').getByTestId('story-unit-text').click();
    const editor = page.getByTestId('story-unit-editor');
    await editor.waitFor();
    await page.getByTestId('story-frame-1').getByRole('textbox').fill('Así fue como Dios hizo todo al comienzo.');
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'detached' });
    await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved');

    await openTool(page, 'translationNotes');
    await select(page, index);
    await page.getByTestId('invalidated-notice').waitFor();
    assert.equal(await row(page, index).getAttribute('data-invalid'), '1');
    const kept = await item(env, list, index);
    assert.equal(kept.decision.value.invalidated, true);
    assert.equal(kept.decision.token, marked.decision.token, 'the frame\'s save wrote nothing on the decision');
    await leave(page, env, OBS);
    const [flagged] = decisionFile(env, OBS, 'translationNotes', 'OBS').decisions;
    assert.equal(flagged.invalidated, true);
    assert.equal(flagged.userStatus, 'valid', 'the person\'s own mark is kept');
    assert.deepEqual(flagged.selections, record.selections);
  }, SCENARIO);
});

// #434: Check uses the open project's own pins after a switch between a story project and a
// Bible project in one session: the rail's resource is the open project's.
browserTest('check_project_switch_resolves_the_open_projects_own_pins', async () => {
  await withApp(async ({ page, env }) => {
    const seen = [];
    const both = async (project, pins) => {
      for (const [tool, repo] of Object.entries(pins)) {
        await openTool(page, tool);
        seen.push([project, tool, await page.getByTestId('check-rail').getAttribute('data-resource')]);
        assert.equal(seen.at(-1)[2], `git.door43.org/unfoldingWord/${repo}`, `${project} ${tool}`);
        assert.equal(await page.getByTestId('check-empty').count(), 0);
        await allTools(page).click();
      }
      // Left through Translate, so the tile opens in Translate, not in a tool (tC4's spec).
      await translateTab(page).click();
      await page.getByTitle(/Switch project/).click();
      await page.getByTestId(`project-${project}`).waitFor();
    };
    const STORIES = { translationNotes: 'en_obs-tn', translationWords: 'en_obs-twl' };
    const TITUS = { translationNotes: 'en_tn', translationWords: 'en_twl' };
    await openStory(page);
    await both(OBS, STORIES);
    await openTitus(page);
    await both(BIBLE, TITUS);
    await openStory(page);
    await both(OBS, STORIES);
    assert.equal(seen.length, 6);

    // A tile whose last place is a check tool resumes that tool with its own project's pin.
    for (const [open, project, repo] of [[openTitus, BIBLE, 'en_tn'], [openStory, OBS, 'en_obs-tn']]) {
      await open(page);
      await openTool(page, 'translationNotes');
      await page.getByTitle(/Switch project/).click();
      await page.getByTestId(`project-${project}`).waitFor();
      await open(page);
      await page.getByTestId('check-quote').waitFor();
      assert.equal(await page.getByTestId('check-rail').getAttribute('data-resource'), `git.door43.org/unfoldingWord/${repo}`);
      await page.getByTitle(/Switch project/).click();
      await page.getByTestId(`project-${project}`).waitFor();
    }
  }, SCENARIO);
});

// #532, the story half: a frame drafted in Translate while the tool stayed open shows under
// "Your translation" on the return to Check; a frame still empty still says so.
browserTest('check_returns_draft_a_frame_drafted_in_translate_shows_on_the_return', async () => {
  await withApp(async ({ page }) => {
    await openStory(page);
    await openTool(page, 'translationNotes');
    const [drafting, empty] = [15, 18];
    assert.equal(await row(page, drafting).getAttribute('data-ref'), '1:3');
    assert.equal(await row(page, empty).getAttribute('data-ref'), '1:4');
    await select(page, drafting);
    assert.equal(await page.getByTestId('check-target').getAttribute('data-drafted'), '0');

    await translateTab(page).click();
    await page.getByTestId('story-draft').waitFor();
    await page.getByTestId('story-frame-3').getByRole('button').click();
    const editor = page.getByTestId('story-unit-editor');
    await editor.waitFor();
    await page.getByTestId('story-frame-3').getByRole('textbox').fill('La tierra estaba oscura y vacía.');
    await editor.getByRole('button', { name: 'Save', exact: true }).click();
    await editor.waitFor({ state: 'detached' });
    await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved');

    await checkTab(page).click();
    await page.getByTestId('check-quote').waitFor();
    await select(page, drafting);
    await until('the new frame shows', async () => (await page.getByTestId('check-target').getAttribute('data-drafted')) === '1');
    assert.ok((await page.getByTestId('check-target').locator('[data-testid^="tw-"]').allTextContents()).includes('oscura'));
    await select(page, empty);
    assert.equal(await page.getByTestId('check-target').getAttribute('data-drafted'), '0');
    assert.equal((await page.getByTestId('check-target').textContent()).trim(), en['check.notDraftedFrame']);
  }, SCENARIO);
});

// The late draft (DELTA-10-16 §3.15.6): the check shown has no text yet; the verse's text
// arrives (here written through the server as another request does, as a teammate's verse
// does when it lands) and the check shows it with no click, its words ready to tap.
browserTest('a_check_shown_before_its_verse_has_text_shows_the_text_when_it_arrives', async () => {
  await withApp(async ({ page, env }) => {
    const list = bookList('translationNotes');
    await openTitus(page);
    await openTool(page, 'translationNotes');
    const index = 17;
    await select(page, index);
    const target = page.getByTestId('check-target');
    assert.equal(await target.getAttribute('data-drafted'), '0');
    assert.equal((await target.textContent()).trim(), en['check.notDrafted']);
    const shown = await item(env, list, index);
    assert.equal(shown.place, 'TIT 1:6');
    assert.deepEqual(shown.target.words, []);
    const at = await scopeOf(env);
    const chapter = (await env.call('editor/chapters', { at, book: 'TIT' })).chapters[0];
    const empty = chapter.verses.find((v) => v.key === '6');
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: empty.words.token, words: 'si alguno es irreprensible, marido de una sola mujer' });
    await until('the text shows where the check is', async () => (await target.getAttribute('data-drafted')) === '1');
    assert.equal(await shownIndex(page), index, 'the same check, with no navigation');
    const words = await target.locator('[data-testid^="tw-"]').allTextContents();
    assert.deepEqual(words, ['si', 'alguno', 'es', 'irreprensible', 'marido', 'de', 'una', 'sola', 'mujer']);
    assert.deepEqual((await item(env, list, index)).target.words, words);
  }, SCENARIO);
});

// Found by these journeys: a story's tabs show before its story is read, and the open's overlay
// waits for that read (it shows 300 ms after the tile's press). The Check tab pressed before then
// left the overlay up for good, its scrim over the whole window and Tab held in it.
browserTest('a_mode_tab_pressed_before_the_story_is_read_leaves_no_open_overlay', async () => {
  await withApp(async ({ page }) => {
    // The story's read is slow, as on a busy computer: the tabs are there before it lands.
    await page.route('**/api/v1/story/open', async (route) => {
      const response = await route.fetch();
      await sleep(1_000);
      await route.fulfill({ response });
    });
    await page.getByTestId(`project-${OBS}`).getByTestId('story-tile-1').click();
    await checkTab(page).click();
    await page.getByTestId('open-translationNotes').waitFor();
    await sleep(700);
    assert.equal(await page.getByTestId('open-progress').count(), 0, 'no open overlay is left over the picker');
    await page.getByTestId('open-translationNotes').click({ timeout: 5_000 });
    await page.getByTestId('check-quote').waitFor();
  }, SCENARIO);
});
