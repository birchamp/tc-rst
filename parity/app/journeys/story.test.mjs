// OBS stories (B7): J25's open of a story from its tile (the story rail, Understand with a
// read-only comment box per unit, the helps panel and the frame the rail moves) and J22's reads
// (a Translation Notes session of a story: its quote, the gateway frame with the quote's words
// lit, Previous and Next). Reading a story writes nothing.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/story.test.mjs
import assert from 'node:assert/strict';
import { browserTest, recordRequests, until, withApp } from './kit.mjs';

const OBS = 'sample_burrito_obs';
const SCENARIO = { scenario: 'en-pins' };

const card = (page, env) => page.getByTestId(`project-${env.projectId(`_local_/_local_/${OBS}`)}`);

/** Story 1 from its tile on Home, in Understand. */
async function openStory1(page, env) {
  await card(page, env).getByTestId('story-tile-1').click();
  await page.getByTestId('story-understand').waitFor();
  await page.getByTestId('story-understand-unit-1').waitFor();
  await page.getByTestId('helps-panel').waitFor();
}

/** The API paths of the requests since `mark`, a path once however often it was asked. */
const paths = (requests, mark) => [...new Set(requests.api(mark))].sort();

browserTest('j25_a_story_opens_with_its_rail_its_units_and_a_comment_box_that_takes_no_typing', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    assert.equal(await page.getByTestId(/^story-\d+$/).count(), 50, 'the rail lists the fifty stories');
    assert.equal(await page.getByTestId('story-1').getAttribute('aria-current'), 'page', 'the open story is the current row');
    assert.equal(await page.locator('[data-testid^="story-understand-unit-"]').count(), 17, 'the title and sixteen frames');
    const boxes = page.locator('[data-testid="story-understand-comment"] textarea');
    assert.equal(await boxes.count(), 17);
    assert.equal(await page.locator('[data-testid="story-understand-comment"] textarea[readonly]').count(), 17, 'no box takes typing until C3 saves it');
    const before = await boxes.nth(1).inputValue();
    await boxes.nth(1).click();
    await page.keyboard.type('typed words');
    assert.equal(await boxes.nth(1).inputValue(), before, 'what is typed into a read-only box is not kept');
  }, SCENARIO);
});

browserTest('j25_the_rail_moves_the_frame_and_the_helps_panel_follows_it', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    await page.getByTestId('frame-marker-2').click();
    await until('frame 2 is in focus', async () => (await page.getByTestId('story-understand-unit-2').getAttribute('data-focused')) === 'true');
    assert.equal(await page.getByTestId('story-understand-unit-1').getAttribute('data-focused'), null, 'one frame at a time');
    for (const tab of ['Notes', 'Words', 'Questions']) {
      assert.equal(await page.getByRole('tab', { name: tab, exact: true }).count(), 1, `the ${tab} tab`);
    }
    assert.equal(await page.getByRole('tab', { name: 'Comments', exact: true }).count(), 0, 'Understand has no Comments tab; Translate does');
    assert.ok(await page.getByTestId('helps-note').count() > 0, 'the notes of the story are listed');
    await page.getByTestId('story-2').click();
    await until('story 2 is current', async () => (await page.getByTestId('story-2').getAttribute('aria-current')) === 'page');
    assert.equal(await page.getByTestId('story-1').getAttribute('aria-current'), null);
    await page.getByRole('tab', { name: 'Translate', exact: true }).click();
    await page.getByTestId('story-draft').waitFor();
    await page.getByRole('tab', { name: 'Comments', exact: true }).waitFor();
  }, SCENARIO);
});

// A frame move or a click on a card lights cards and leaves the list as it was: a note the
// person expanded stays expanded, as tC4 keeps each keyed card.
browserTest('an_expanded_note_stays_expanded_when_the_frame_moves_and_a_card_is_clicked', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    // Story 17's real note at frame 7 exceeds the preview; story 1's notes are all short.
    await page.getByTestId('story-17').click();
    await until('story 17 is current', async () => (await page.getByTestId('story-17').getAttribute('aria-current')) === 'page');
    const notes = page.getByTestId('helps-note');
    await notes.first().waitFor();
    const expand = notes.filter({ has: page.getByTestId('note-expand') }).first().getByTestId('note-expand');
    await expand.click();
    assert.equal(await expand.getAttribute('aria-expanded'), 'true');
    await page.getByTestId('frame-marker-2').click();
    await until('frame 2\'s notes are lit', async () => (await notes.and(page.locator('[data-frame="2"]')).first().getAttribute('data-focused')) === 'true');
    assert.equal(await notes.and(page.locator('[data-frame="1"]')).first().getAttribute('data-focused'), null, 'frame 1\'s are not');
    assert.equal(await expand.getAttribute('aria-expanded'), 'true', 'the frame move keeps the note expanded');
    const card = notes.and(page.locator('[data-frame="3"]')).first();
    await card.click({ position: { x: 8, y: 8 } });
    await until('the clicked card is lit', async () => (await card.getAttribute('data-focused')) === 'true');
    assert.equal(await expand.getAttribute('aria-expanded'), 'true', 'the click keeps the note expanded');
  }, SCENARIO);
});

browserTest('story_open_reads_one_story_and_writes_nothing', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId('story-2').click();
    await until('story 2 is current', async () => (await page.getByTestId('story-2').getAttribute('aria-current')) === 'page');
    await page.getByTestId('helps-panel').waitFor();
    const asked = paths(requests, mark);
    assert.ok(asked.includes('/api/v1/story/open'), `the story is read (${asked})`);
    assert.deepEqual(asked.filter((p) => !/\/(story|helps)\//.test(p) && p !== '/api/v1/prefs/recordPlace'), [], 'only reads of the story and its helps, and the place the person is remembered at');
  }, SCENARIO);
});

browserTest('j22_a_story_check_session_reads_its_quote_and_moves_with_previous_and_next', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-session').waitFor();
    await page.getByTestId('check-quote').waitFor();
    assert.equal(await page.getByTestId('check-quote').textContent(), '“The Creation”', 'the title is the first check');
    assert.match(await page.getByTestId('check-item-counter').textContent(), /^Item 1 of \d+$/);
    assert.equal(await page.getByTestId('story-hl').count(), 2, 'the quote lights its two words in the gateway title');
    const words = await page.getByTestId('check-target').locator('[data-testid^="tw-"]').allTextContents();
    assert.deepEqual(words, ['La', 'Creación'], 'the translator\'s own title, as words to tap');
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId('check-next').click();
    await until('the second check', async () => (await page.getByTestId('check-quote').textContent()) === '“the beginning”');
    assert.match(await page.getByTestId('check-item-counter').textContent(), /^Item 2 of \d+$/);
    await page.getByTestId('check-prev').click();
    await until('the first check again', async () => (await page.getByTestId('check-quote').textContent()) === '“The Creation”');
    const asked = paths(requests, mark);
    assert.deepEqual(asked, ['/api/v1/check/item'], 'moving through a story\'s checks only reads');
  }, SCENARIO);
});

// tC4's Previous/Next and item counter follow the resource's item order, even when the
// rail groups the word checks alphabetically. Story 1 starts God, Spirit, then God again.
browserTest('story_word_checks_move_through_each_item_in_resource_order', async () => {
  await withApp(async ({ page, env }) => {
    await openStory1(page, env);
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-translationWords').click();
    await page.getByTestId('check-session').waitFor();
    await page.getByTestId('check-quote').waitFor();
    const quote = page.getByTestId('check-quote');
    const counter = page.getByTestId('check-item-counter');
    assert.equal(await quote.textContent(), '“God”');
    assert.equal(await counter.textContent(), 'Item 1 of 35');
    assert.equal(await page.getByTestId('check-prev').isDisabled(), true);
    await page.getByTestId('check-next').click();
    await until('the Spirit check follows the first God check', async () => (await quote.textContent()) === '“Spirit”');
    assert.equal(await counter.textContent(), 'Item 2 of 35');
    await page.getByTestId('check-next').click();
    await until('the next God check follows Spirit', async () => (await quote.textContent()) === '“God”');
    assert.equal(await counter.textContent(), 'Item 3 of 35');
    await page.getByTestId('check-prev').click();
    await until('Previous returns to Spirit', async () => (await quote.textContent()) === '“Spirit”');
    assert.equal(await counter.textContent(), 'Item 2 of 35');
  }, SCENARIO);
});
