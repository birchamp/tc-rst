// The section editor (B4b), J2's section steps on real saves through the store (the test
// build's `test/saveSection`, until C2's `editor/saveSection`): the card saves only on Save,
// Cancel leaves no buffer entry, a Save that leaves no words asks first (#600), a span is made
// and broken with two clicks (#598), the Place hints read by state (#604), a joined span has no
// blank line (#575), the exits stay off under an open card and keep its draft, and a reload
// saves nothing.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     node --test parity/app/journeys/section.test.mjs
import assert from 'node:assert/strict';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, recordRequests, sleep, storageDump, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const NINE = 'Exhorta a los siervos a que se sujeten a sus amos y a que agraden en todo';
const TEN = 'no defraudando sino mostrando toda buena fe';

const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const indicator = (page) => page.getByTestId('save-indicator');
const pill = (page, span) => page.getByRole('button', { name: `Draft section ${span}` });
const card = (page) => page.getByTestId('section-editor');
const typing = (page, span) => page.getByRole('textbox', { name: `Section ${span}` });
const saveButton = (page) => page.getByRole('button', { name: 'Save section' });
const bank = (page) => page.getByTestId('pin-bank');
const words = (page) => page.getByTestId('place-words');

/** Titus 2 in Translate, by section. */
async function openTitus2(page) {
  await titus(page).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
  await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
  await page.getByRole('button', { name: '2', exact: true }).click();
  await page.getByRole('heading', { name: 'Titus 2' }).waitFor();
  await pill(page, '9–10').waitFor();
}

/** The open session's scope, as `app/hello` names it. */
async function scope(env) {
  const hello = await env.call('app/hello', {});
  return hello.sessions[0].at;
}

/** Titus 2 as `editor/chapters` reads it now. */
async function chapter2(env) {
  const view = await env.call('editor/chapters', { at: await scope(env), book: 'TIT' });
  return view.chapters.find((c) => c.chapter === 2);
}
const verseOf = async (env, key) => (await chapter2(env)).verses.find((v) => v.key === key);

/** Draft 9 and 10 through the card, as the J2 steps do, and wait until the card is gone. */
async function draftTwoVerses(page, { gap = false } = {}) {
  await pill(page, '9–10').click();
  await typing(page, '9–10').fill(`9 ${NINE}\n${gap ? '\n' : ''}10 ${TEN}`);
  await saveButton(page).click();
  await card(page).waitFor({ state: 'detached' });
  await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
}

const savesSent = (requests, mark) => requests.api(mark).filter((p) => p.includes('saveSection')).length;

// A-1: typing, placing pins and leaving the field send no save; only Save does, and one.
browserTest('section_editor_saves_only_on_save', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`${NINE} ${TEN}`);
    // Leave the field, and let more than the 2 s of a pause pass: tC4's verse card would save.
    await card(page).getByText('Drafting 9–10').click();
    await sleep(2600);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    await bank(page).getByRole('button', { name: 'Move where verse 10 begins' }).click();
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    await sleep(2600);
    assert.equal(savesSent(requests, mark), 0, requests.api(mark).join(', '));
    assert.equal((await verseOf(env, '9')).words.value, '___', 'nothing is written');
    assert.equal(await indicator(page).getAttribute('data-state'), 'dirty');
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal(savesSent(requests, mark), 1, 'Save sends one save');
    const nine = (await verseOf(env, '9')).words.value;
    const ten = (await verseOf(env, '10')).words.value;
    assert.equal(`${nine} ${ten}`, `${NINE} ${TEN}`, 'the words are all there, split where 10 was placed');
    assert.ok(ten.startsWith('no '), ten);
  }, { hooks: true });
}, { hooks: true });

// Cancel leaves no buffer entry and no draft, and writes nothing.
browserTest('cancel_leaves_no_buffer_entry_and_no_draft', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n10 ${TEN}`);
    await card(page).getByText('Drafting 9–10').click();
    assert.ok((await storageDump(page)).includes(NINE), 'the card is kept in the browser buffer while it is open');
    await card(page).getByRole('button', { name: 'Cancel' }).click();
    await card(page).waitFor({ state: 'detached' });
    await sleep(300);
    const dump = await storageDump(page);
    assert.ok(!dump.includes(NINE) && !dump.includes('tc4.drafts.v2'), dump);
    assert.equal(savesSent(requests, mark), 0);
    assert.equal((await verseOf(env, '9')).words.value, '___');
    assert.equal(await indicator(page).getAttribute('data-state'), 'saved');
    // The lock ends with the card.
    assert.equal(await page.getByTestId('mode-switch').getByRole('tab', { name: 'Check' }).isDisabled(), false);
    // And the section opens again from what is stored.
    await pill(page, '9–10').click();
    assert.equal(await typing(page, '9–10').inputValue(), '');
  }, { hooks: true });
}, { hooks: true });

// #600: a Save that leaves the section with no words asks first; Escape and Keep editing write
// nothing; the answer writes the stub for each verse, a paragraph between them stays; a section
// with no draft has nothing to save.
browserTest('emptied_section_save_asks_then_writes_stubs', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await draftTwoVerses(page, { gap: true });
    assert.equal((await verseOf(env, '10')).format, 'p');
    const ask = page.getByRole('dialog', { name: 'Remove all the text of section 9–10?' });
    const requests = recordRequests(page);
    for (const typed of ['', '9\n10']) {
      await pill(page, '9–10').click();
      assert.equal(await typing(page, '9–10').inputValue(), `9 ${NINE}\n\n10 ${TEN}`);
      await typing(page, '9–10').fill(typed);
      assert.equal(await saveButton(page).isEnabled(), true);
      const mark = requests.mark();
      await saveButton(page).click();
      await ask.waitFor();
      assert.equal(await ask.getByText('The section has no text now.', { exact: false }).count(), 1);
      await page.keyboard.press('Escape');
      await ask.waitFor({ state: 'detached' });
      assert.equal(await typing(page, '9–10').inputValue(), typed, 'Escape keeps the card');
      await saveButton(page).click();
      await ask.getByRole('button', { name: 'Keep editing' }).click();
      await ask.waitFor({ state: 'detached' });
      assert.equal(savesSent(requests, mark), 0);
      assert.equal((await verseOf(env, '9')).words.value, NINE, 'Keep editing writes nothing');
      await saveButton(page).click();
      await ask.getByRole('button', { name: 'Remove the text' }).click();
      await card(page).waitFor({ state: 'detached' });
      await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
      assert.equal((await verseOf(env, '9')).words.value, '___');
      assert.equal((await verseOf(env, '10')).words.value, '___');
      assert.equal((await verseOf(env, '10')).format, 'p', 'the paragraph break between them stays');
      assert.equal(savesSent(requests, mark), 1);
      await draftTwoVerses(page, { gap: true });
    }
    // Nothing drafted, nothing typed: nothing to save.
    await env.call('test/reset', { request: newRequestId() });
  }, { hooks: true });
}, { hooks: true });

browserTest('an_undrafted_section_has_nothing_to_save', async () => {
  await withApp(async ({ page }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    assert.equal(await typing(page, '9–10').inputValue(), '');
    assert.equal(await saveButton(page).isDisabled(), true);
    await typing(page, '9–10').fill('9 x');
    assert.equal(await saveButton(page).isEnabled(), true);
    await typing(page, '9–10').fill('');
    assert.equal(await saveButton(page).isDisabled(), true);
  }, { hooks: true });
}, { hooks: true });

// #604: the bank's instruction reads by state.
browserTest('section_card_reads_the_idle_place_hint', async () => {
  await withApp(async ({ page }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`${NINE} ${TEN}`);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    // Verse 9 begins the section and is fixed: no pin for it in the bank.
    assert.equal(await bank(page).getByRole('button', { name: 'Move where verse 9 begins' }).count(), 0);
    const idle = bank(page).getByText('Click a verse number, then click the word that begins the verse.');
    const drop = bank(page).getByText(/^Click the word where verse \d+ begins$/);
    const empty = bank(page).getByText('Every verse is placed. Click a verse number to move where its verse begins.');
    assert.equal(await idle.isVisible(), true);
    assert.equal(await drop.isVisible(), false);
    assert.equal(await empty.count(), 0);
    const before = await words(page).boundingBox();
    await bank(page).getByRole('button', { name: 'Move where verse 10 begins' }).click();
    assert.equal(await idle.isVisible(), false);
    assert.equal(await drop.isVisible(), true);
    assert.equal(await drop.textContent(), 'Click the word where verse 10 begins');
    assert.deepEqual(await words(page).boundingBox(), before, 'the words do not move when a number is picked up');
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    assert.equal(await empty.isVisible(), true);
    assert.equal(await idle.isVisible(), false);
    assert.equal(await drop.isVisible(), false);
    assert.equal(await bank(page).getByRole('button', { name: /Move where verse/ }).count(), 0);
    assert.equal(await words(page).getByRole('button', { name: 'Move where verse 10 begins' }).isVisible(), true);
  }, { hooks: true });
}, { hooks: true });

// #598, #63: a placed number moves with two clicks, and a span is broken where it is clicked.
browserTest('break_a_span_with_two_clicks', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await draftTwoVerses(page);
    // Stack 10 on 9: one span.
    await pill(page, '9–10').click();
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    await words(page).getByRole('button', { name: 'Move where verse 10 begins' }).click();
    await page.getByRole('button', { name: 'Join verse 10 to verse 9 at Exhorta' }).click();
    assert.equal(await bank(page).getByRole('button', { name: /Move where verse/ }).count(), 0);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    const span = await verseOf(env, '9-10');
    assert.equal(span.words.value, `${NINE}\n${TEN}`, 'a single line break stays as typed');
    assert.equal(span.covered.length, 1);
    // The span opens as one line, its numbers stacked; two clicks move 10 to "no".
    await pill(page, '9-10').click();
    assert.equal(await typing(page, '9-10').inputValue(), `9-10 ${NINE}\n${TEN}`);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    const pin = words(page).getByRole('button', { name: 'Move where verse 10 begins' });
    await pin.click();
    assert.equal(await bank(page).getByText('Click the word where verse 10 begins').isVisible(), true);
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    // A double click on the number picks it up and puts it back.
    await pin.dblclick();
    assert.equal(await bank(page).getByText('Click the word where verse 10 begins').isVisible(), false);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '9')).words.value, NINE);
    assert.equal((await verseOf(env, '10')).words.value, TEN);
    assert.equal(await verseOf(env, '9-10'), undefined, 'the span is two verses again');
    await pill(page, '9–10').waitFor();
  }, { hooks: true });
}, { hooks: true });

// #575: the blank line a paragraph left between two verses is one space once they are a span.
browserTest('joined_span_collapses_blank_lines_to_one_space', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n\n10 ${TEN}`);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    await words(page).getByRole('button', { name: 'Move where verse 10 begins' }).click();
    await page.getByRole('button', { name: 'Join verse 10 to verse 9 at Exhorta' }).click();
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '9-10')).words.value, `${NINE} ${TEN}`);
  }, { hooks: true });
}, { hooks: true });

// #54: Tab and Shift+Tab indent the line for poetry, a blank line opens a paragraph, and the
// formats land in the layout of the verse before; typing them again takes them away.
browserTest('paragraph_and_poetry_lines_from_the_card_land_in_the_verse_before', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    const field = typing(page, '9–10');
    await field.fill(`9 ${NINE}\n10 ${TEN}`);
    // Tab at the start of verse 10's line.
    await field.evaluate((el) => el.setSelectionRange(el.value.indexOf('\n10') + 1, el.value.indexOf('\n10') + 1));
    assert.equal(await page.evaluate(() => document.activeElement?.tagName), 'TEXTAREA');
    await field.press('Tab');
    assert.equal(await field.inputValue(), `9 ${NINE}\n\t10 ${TEN}`);
    assert.equal(await page.evaluate(() => document.activeElement?.tagName), 'TEXTAREA', 'Tab keeps the focus');
    await field.press('Tab');
    assert.equal(await field.inputValue(), `9 ${NINE}\n\t\t10 ${TEN}`);
    await field.press('Tab');
    assert.equal(await field.inputValue(), `9 ${NINE}\n\t\t10 ${TEN}`, 'at most two tabs');
    await field.press('Shift+Tab');
    assert.equal(await field.inputValue(), `9 ${NINE}\n\t10 ${TEN}`);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '10')).format, 'q1');
    // It opens again with the tab, and a blank line makes it a paragraph.
    await pill(page, '9–10').click();
    assert.equal(await typing(page, '9–10').inputValue(), `9 ${NINE}\n\t10 ${TEN}`);
    await typing(page, '9–10').fill(`9 ${NINE}\n\n10 ${TEN}`);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '10')).format, 'p');
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n10 ${TEN}`);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '10')).format, null);
  }, { hooks: true });
}, { hooks: true });

// #598 in one Place session: a number moved twice is a move of the number only. The card read its
// own text again after each drop, so the second move kept a line break inside verse 10 and the
// save wrote a paragraph before it.
browserTest('a_number_moved_twice_in_place_keeps_the_text_and_adds_no_paragraph', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE} ${TEN}`);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    await bank(page).getByRole('button', { name: 'Move where verse 10 begins' }).click();
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    const pin = words(page).getByRole('button', { name: 'Move where verse 10 begins' });
    await pin.click();
    await page.getByRole('button', { name: 'Begin verse 10 at sino' }).click();
    await pin.click();
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '9')).words.value, NINE);
    assert.equal((await verseOf(env, '10')).words.value, TEN);
    assert.equal((await verseOf(env, '10')).format, null, 'no paragraph was typed');
  }, { hooks: true });
}, { hooks: true });

// D7: the card is read-only while its save runs. Tab on a line and a drop of a number picked up
// before Save change nothing, so the save that comes back closes the card; a change there left
// the card dirty, and its next save wrote verse 10 into verse 9.
browserTest('a_held_section_save_takes_no_tab_and_no_drop', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    const saving = () => until('saving', async () => (await indicator(page).getAttribute('data-state')) === 'saving', 5_000);
    const release = async () => {
      await env.call('test/release', { request: newRequestId(), at: 'save' });
      await card(page).waitFor({ state: 'detached' });
      await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    };
    await pill(page, '9–10').click();
    const field = typing(page, '9–10');
    await field.fill(`9 ${NINE}\n10 ${TEN}`);
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await saveButton(page).click();
    await saving();
    await field.evaluate((el) => el.setSelectionRange(el.value.indexOf('\n10') + 1, el.value.indexOf('\n10') + 1));
    await field.press('Tab');
    assert.equal(await field.inputValue(), `9 ${NINE}\n10 ${TEN}`, 'Tab changes nothing while the save runs');
    await release();
    assert.equal((await verseOf(env, '9')).words.value, NINE);
    assert.equal((await verseOf(env, '10')).words.value, TEN);
    assert.equal((await verseOf(env, '10')).format, null);
    // Place: 10 moves to "sino", is picked up again, and a word is clicked while the save runs.
    await pill(page, '9–10').click();
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    const pin = words(page).getByRole('button', { name: 'Move where verse 10 begins' });
    await pin.click();
    await page.getByRole('button', { name: 'Begin verse 10 at sino' }).click();
    await pin.click();
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await saveButton(page).click();
    await saving();
    await page.getByRole('button', { name: 'Begin verse 10 at no' }).click();
    await release();
    assert.equal((await verseOf(env, '9')).words.value, `${NINE}\nno defraudando`, 'the line break typed after verse 9 stays');
    assert.equal((await verseOf(env, '10')).words.value, 'sino mostrando toda buena fe');
  }, { hooks: true });
}, { hooks: true });

// A page that reloads while a section save runs sends that save again with no card. The card
// opened afterwards is based on every verse it shows: a base cut to the first verse made its
// next save write the whole text into verse 9.
browserTest('a_section_card_opened_after_a_reload_during_its_save_saves_each_verse', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n10 ${TEN}`);
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await saveButton(page).click();
    await until('saving', async () => (await indicator(page).getAttribute('data-state')) === 'saving', 5_000);
    await page.reload();
    await page.getByTestId('account-menu').waitFor();
    await env.call('test/release', { request: newRequestId(), at: 'save' });
    await until('stored', async () => (await verseOf(env, '10')).words.value === TEN);
    if (!(await pill(page, '9–10').isVisible())) await openTitus2(page);
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await pill(page, '9–10').click();
    assert.equal(await typing(page, '9–10').inputValue(), `9 ${NINE}\n10 ${TEN}`);
    await typing(page, '9–10').fill(`9 ${NINE} amén\n10 ${TEN}`);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '9')).words.value, `${NINE} amén`);
    assert.equal((await verseOf(env, '10')).words.value, TEN);
  }, { hooks: true });
}, { hooks: true });

// #241: a held number takes the press and selects no text; Escape on a word puts it back.
browserTest('a_held_number_selects_no_text_and_escape_puts_it_back', async () => {
  await withApp(async ({ page }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 a\n10 b`);
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    const prevented = await page.evaluate(() => {
      const pin = document.querySelector('[data-testid="pin-10"]');
      return !pin.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true }));
    });
    assert.equal(prevented, true, 'the press on a number is default-prevented');
    assert.equal(await words(page).evaluate((el) => el.style.userSelect), 'none');
    await words(page).getByText('a', { exact: true }).press('Escape');
    assert.equal(await words(page).evaluate((el) => el.style.userSelect), '');
    assert.equal(await words(page).getByRole('button', { name: 'Move where verse 10 begins' }).count(), 1, 'still where it was');
  }, { hooks: true });
}, { hooks: true });

// tC4's sectionDraftExits: while the card is open the chapter, the book, the mode tabs, the
// project button and the other sections' pills are off, a press on one keeps the draft in the
// card, and the card still holds it after the card's own Cancel and re-open.
browserTest('section_card_keeps_its_draft_on_every_exit', async () => {
  await withApp(async ({ page }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    const typed = '9 texto que no se pierde';
    await typing(page, '9–10').fill(typed);
    const tab = (name) => page.getByTestId('mode-switch').getByRole('tab', { name });
    const off = [
      page.getByRole('button', { name: '3', exact: true }),
      page.getByRole('button', { name: /Jonah/ }),
      tab('Understand'),
      tab('Check'),
      pill(page, '11–13'),
      page.getByRole('tab', { name: 'Verse', exact: true }),
    ];
    for (const control of off) {
      assert.equal(await control.isDisabled(), true, String(control));
      await control.click({ force: true });
      assert.equal(await typing(page, '9–10').inputValue(), typed, `a press on ${control} keeps the draft`);
    }
    // The project button is not drawn off, but it goes nowhere while the card is open.
    await page.getByTestId('project-switch').click();
    assert.equal(await typing(page, '9–10').inputValue(), typed, 'a press on the project button keeps the draft');
    assert.equal(await page.getByRole('heading', { name: 'Titus 2' }).count(), 1, 'still on Titus 2');
    assert.equal(await page.getByTestId('save-indicator').getAttribute('data-state'), 'dirty');
    // The Place mode keeps it too, and back.
    await page.getByRole('tab', { name: 'Place verse numbers' }).click();
    await page.getByRole('tab', { name: 'Type', exact: true }).click();
    assert.equal(await typing(page, '9–10').inputValue(), typed.replace(/ +$/, ''));
  }, { hooks: true });
}, { hooks: true });

// A reload saves nothing: the card's buffer comes back as a draft and only the card's Save
// writes it (APP-DESIGN 6.5).
browserTest('a_reload_never_saves_a_section_buffer', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n10 ${TEN}`);
    await card(page).getByText('Drafting 9–10').click();
    assert.ok((await storageDump(page)).includes(NINE));
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.reload();
    await page.getByTestId('account-menu').waitFor();
    await sleep(3000);
    assert.equal(savesSent(requests, mark), 0, requests.api(mark).join(', '));
    assert.equal((await verseOf(env, '9')).words.value, '___');
    assert.ok((await storageDump(page)).includes(NINE), 'the buffer is still there for the offer');
  }, { hooks: true });
}, { hooks: true });

// STORAGE 5.2: a change that arrived under the open card becomes a second version beside the
// card's save, never a revert and never a refusal.
browserTest('a_teammates_change_under_a_section_card_becomes_a_conflict', async () => {
  await withApp(async ({ page, env }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}\n10 ${TEN}`);
    const row = await verseOf(env, '9');
    await env.call('test/saveVerse', { at: await scope(env), request: newRequestId(), shown: row.words.token, words: 'Los siervos, sujetos' });
    await sleep(300);
    await saveButton(page).click();
    await card(page).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verseOf(env, '9')).words.marker?.versions, 2, 'both versions are kept');
    assert.equal((await verseOf(env, '10')).words.value, TEN);
    await page.getByTestId('conflict-marker-2:9').waitFor();
  }, { hooks: true });
}, { hooks: true });

// Without a section save (C2 brings editor/saveSection) a Save is refused and says so: the card
// and its text stay and the indicator offers Retry.
browserTest('a_build_without_a_section_save_says_not_saved_and_keeps_the_card', async () => {
  await withApp(async ({ page }) => {
    await openTitus2(page);
    await pill(page, '9–10').click();
    await typing(page, '9–10').fill(`9 ${NINE}`);
    await saveButton(page).click();
    await until('error', async () => (await indicator(page).getAttribute('data-state')) === 'error', 5_000);
    assert.equal(await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).count(), 1);
    assert.equal(await typing(page, '9–10').inputValue(), `9 ${NINE}`);
    assert.equal(await card(page).count(), 1, 'the card stays open');
  });
});

// #614, tC4 `ds-labels` d: a verse number's name in Place comes from the catalog, so it follows
// the app language.
browserTest('ds_labels_d_a_verse_numbers_name_follows_the_app_language', async () => {
  const [en, es] = [catalog('en'), catalog('es-419')];
  const spanish = (key, vars = {}) => {
    assert.ok(es[key] && es[key] !== en[key], `${key}: translated`);
    return Object.entries(vars).reduce((s, [k, v]) => s.replace(`{${k}}`, String(v)), es[key]);
  };
  await withApp(async ({ page }) => {
    await titus(page).click();
    await page.getByTestId('mode-switch').getByRole('tab', { name: spanish('nav.draft'), exact: true }).click();
    await page.getByRole('button', { name: '2', exact: true }).click();
    await page.getByRole('button', { name: spanish('draft.draftSection', { span: '9–10' }) }).click();
    await page.getByRole('textbox', { name: spanish('draft.sectionLabel', { span: '9–10' }) }).fill(`${NINE} ${TEN}`);
    await page.getByRole('tab', { name: spanish('draft.modePlace') }).click();
    const pin = bank(page).getByTestId('pin-10');
    assert.equal(await pin.getAttribute('aria-label'), spanish('draft.moveVerse', { n: 10 }));
    await card(page).getByRole('button', { name: spanish('draft.cancelVerse'), exact: true }).click();
    await card(page).waitFor({ state: 'detached' });
  }, { locale: 'es-419' });
});
