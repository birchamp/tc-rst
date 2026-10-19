// Translate (B4a): the grid by section and by verse, a chapter switch, the verse card on real
// saves through the store (the test build's `test/saveVerse`, until C2's `editor/saveVerse`), its
// save states, Cancel, the editing lock, and the refresh rule of AD37 when a teammate's change
// arrives under an open card (a Door43 sync or a USB file brings one; here the test writes it
// through the store as another request would), with an Arabic IME composition.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     node --test parity/app/journeys/translate.test.mjs
import assert from 'node:assert/strict';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, recordRequests, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const titus = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const indicator = (page) => page.getByTestId('save-indicator');
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });

/** Titus in Translate, by section. */
async function openTranslate(page) {
  await titus(page).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
  await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
}

/** By verse, with the card open on verse `n` (a drafted verse). */
async function openVerse(page, n) {
  await page.getByRole('tab', { name: 'Verse', exact: true }).click();
  await page.getByTitle('Edit this verse').nth(n - 1).click();
  await box(page, n).waitFor();
}

/** The open session's scope, as `app/hello` names it. */
async function scope(env) {
  const hello = await env.call('app/hello', {});
  return hello.sessions[0].at;
}

/** Titus 1:`n` as `editor/chapters` reads it now. */
async function verse(env, n) {
  const view = await env.call('editor/chapters', { at: await scope(env), book: 'TIT' });
  return view.chapters[0].verses.find((v) => v.key === String(n));
}

/** A teammate's change to Titus 1:`n`, written through the store over the version it shows. */
async function teammateWrites(env, n, words) {
  const row = await verse(env, n);
  await env.call('test/saveVerse', { at: await scope(env), request: newRequestId(), shown: row.words.token, words });
}

browserTest('translate_shows_the_source_beside_the_translation_by_section_and_by_verse', async () => {
  await withApp(async ({ page }) => {
    await openTranslate(page);
    const chips = await page.locator('[data-testid^="source-tab-"]').allTextContents();
    assert.deepEqual(chips, ['ULT', 'UST', 'Greek']);
    assert.equal(await page.getByTestId('source-name').textContent(), 'unfoldingWord Literal Text · v91 · pinned');
    const sections = await page.getByRole('button', { name: /Draft section/ }).allTextContents();
    assert.deepEqual(sections, ['Draft section 1–3', 'Draft section 4–5', 'Draft section 6–7', 'Draft section 8–9', 'Draft section 10–11', 'Draft section 12–14', 'Draft section 15–16']);
    const first = await page.getByTestId('draft-target-text').first().textContent();
    assert.ok(first.startsWith('1Pablo, siervo de Dios'), first);
    // By verse: no section pill, a drafted verse revises alone, an undrafted one has its pill.
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByRole('button', { name: 'start this verse' }).first().waitFor();
    assert.equal(await page.getByRole('button', { name: /Draft section/ }).count(), 0);
    assert.equal(await page.getByTitle('Edit this verse').count(), 5);
    assert.equal(await page.getByRole('button', { name: 'start this verse' }).first().textContent(), 'Draft verse 6');
    // The original: Greek, read left to right.
    await page.getByTestId('source-tab-orig').click();
    await until('the Greek text', async () => (await page.getByTestId('draft-source-text').first().getAttribute('lang')) === 'el');
    assert.equal(await page.getByTestId('source-name').textContent(), 'unfoldingWord Greek New Testament · v0.34 · pinned');
  });
});

// AD16: the translation of every chapter came with the book; a chapter switch reads the new
// chapter's source panes and helps (per chapter, as Understand reads them) and records the
// place, and reads nothing of the translation or the book again.
browserTest('chapter_switch_sends_no_request', async () => {
  await withApp(async ({ page }) => {
    await openTranslate(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByRole('button', { name: '2', exact: true }).click();
    await page.getByRole('heading', { name: 'Titus 2' }).waitFor();
    await page.getByRole('button', { name: /Draft section 1–/ }).waitFor();
    await sleep(300);
    const sent = requests.api(mark);
    assert.ok(!sent.some((p) => p.includes('editor/chapters') || p.includes('editor/book')), sent.join(', '));
    assert.deepEqual([...new Set(sent)].sort(), ['/api/v1/editor/sources', '/api/v1/helps/passage', '/api/v1/prefs/recordPlace']);
  });
});

browserTest('a_typed_verse_saves_on_blur_and_the_card_locks_the_exits_meanwhile', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await openVerse(page, 2);
    assert.equal(await box(page, 2).evaluate((el) => el === document.activeElement), true, 'the card takes the focus');
    // The lock: Understand and Check, the book rail's numbers and the Section/Verse switch are off.
    const tab = (name) => page.getByTestId('mode-switch').getByRole('tab', { name });
    assert.equal(await tab('Understand').isDisabled(), true);
    assert.equal(await tab('Check').isDisabled(), true);
    assert.equal(await page.getByRole('button', { name: '2', exact: true }).isDisabled(), true);
    assert.equal(await page.getByRole('tab', { name: 'Section', exact: true }).isDisabled(), true);
    await page.getByRole('button', { name: '2', exact: true }).click({ force: true });
    assert.equal(await box(page, 2).evaluate((el) => el === document.activeElement), true, 'a press on an exit keeps the focus');
    await box(page, 2).fill('con la esperanza de la vida eterna, que Dios prometió');
    await until('dirty', async () => (await indicator(page).getAttribute('data-state')) === 'dirty');
    await page.getByRole('button', { name: 'Save verse' }).click();
    await box(page, 2).waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 2)).words.value, 'con la esperanza de la vida eterna, que Dios prometió');
    await until('the grid shows it', async () => (await page.getByTestId('draft-target-text').first().textContent()).includes('que Dios prometió'));
    assert.equal(await tab('Understand').isDisabled(), false, 'the lock ends with the card');
  }, { hooks: true });
}, { hooks: true });

browserTest('a_held_save_shows_saving_and_cancel_puts_the_verse_back', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await openVerse(page, 1);
    const before = await box(page, 1).inputValue();
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    await box(page, 1).press('Control+End');
    await box(page, 1).pressSequentially(' amén');
    // The pause saves after 2 s; the store holds it at its durability point.
    await until('saving', async () => (await indicator(page).getAttribute('data-state')) === 'saving', 5_000);
    await env.call('test/release', { request: newRequestId(), at: 'save' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 1)).words.value, `${before} amén`);
    // Cancel puts back the text the card opened with, and saves it as the card closes.
    await page.getByRole('button', { name: 'Cancel' }).click();
    await box(page, 1).waitFor({ state: 'detached' });
    await until('saved again', async () => (await verse(env, 1)).words.value === before);
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
  }, { hooks: true });
}, { hooks: true });

// Without a verse writer (C2 brings `editor/saveVerse`), a save is refused and says so: the
// text stays in the card and in the buffer, the indicator offers Retry.
browserTest('a_build_without_a_verse_save_says_not_saved_and_keeps_the_text', async () => {
  await withApp(async ({ page }) => {
    await openTranslate(page);
    await openVerse(page, 3);
    await box(page, 3).press('Control+End');
    await box(page, 3).pressSequentially(' x');
    await until('error', async () => (await indicator(page).getAttribute('data-state')) === 'error', 5_000);
    assert.equal(await page.getByTestId('save-indicator').getByRole('button', { name: 'Retry' }).count(), 1);
    assert.ok((await box(page, 3).inputValue()).endsWith(' x'));
  });
});

// AD37: a card with no input takes a teammate's change whole, value and token together.
browserTest('refresh_under_open_clean_editor_swaps_value_and_token_together', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await openVerse(page, 4);
    await box(page, 4).evaluate((el) => el.blur());
    await page.getByTitle('Edit this verse').nth(3).click();
    await box(page, 4).waitFor();
    await teammateWrites(env, 4, 'a Tito, mi verdadero hijo');
    // Focused: the new pair waits for the blur, so nothing is written into the field.
    await sleep(500);
    assert.notEqual(await box(page, 4).inputValue(), 'a Tito, mi verdadero hijo');
    await page.getByRole('button', { name: 'Save verse' }).click();
    await page.getByTitle('Edit this verse').nth(3).click();
    await box(page, 4).waitFor();
    assert.equal(await box(page, 4).inputValue(), 'a Tito, mi verdadero hijo');
    assert.equal(await page.getByTestId('verse-changed-notice').count(), 0);
    await page.getByRole('button', { name: 'Save verse' }).click();
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 4)).words.marker, null, 'no conflict: the card saved nothing over the change');
  }, { hooks: true });
}, { hooks: true });

// AD37: a card the person typed in keeps its text and its base; the change is named, and the
// save becomes a second version, never a revert. The typing is an Arabic IME composition.
browserTest('refresh_under_typed_editor_keeps_text_and_token_through_an_arabic_ime', async () => {
  await withApp(async ({ page, env, context }) => {
    await openTranslate(page);
    await openVerse(page, 5);
    const cdp = await context.newCDPSession(page);
    await box(page, 5).press('Control+End');
    await cdp.send('Input.imeSetComposition', { text: ' مر', selectionStart: 3, selectionEnd: 3 });
    // The teammate's change and its frame arrive mid-composition.
    await teammateWrites(env, 5, 'Por esto te dejé en Creta');
    await page.getByTestId('verse-changed-notice').waitFor();
    assert.equal(
      await page.getByTestId('verse-changed-notice').textContent(),
      'This verse was changed while you were typing. Your text will be kept as a second version.',
    );
    await cdp.send('Input.insertText', { text: ' مرحبا' });
    const typed = await box(page, 5).inputValue();
    assert.ok(typed.endsWith(' مرحبا'), typed);
    assert.ok(!typed.startsWith('Por esto te dejé'), 'the change is not written into the field');
    await page.getByRole('button', { name: 'Save verse' }).click();
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    const row = await verse(env, 5);
    assert.equal(row.words.marker?.versions, 2, 'the save is a second version beside the change');
    await page.getByTestId('conflict-marker-1:5').waitFor();
    assert.equal(await page.getByTestId('conflicts-review').getAttribute('data-count'), '1');
  }, { hooks: true });
}, { hooks: true });

// tC4 #131, #118: in a project of another numbering, Translate's source column shows for each
// project verse the source verse its numbering names, under that verse's place, one verse per
// row with the verse-only statement in place of the switch; a verse in another book or in no
// place is stated. The sample is in `eng`, so the reply is the server's for a project of this
// shape: Titus 1 whose verse 1 is the source's 1:2, verse 2 in another book, verse 3 nowhere.
browserTest('cross_frame_translate_shows_the_mapped_source_verse', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/editor/sources', async (route) => {
      const reply = await (await route.fetch()).json();
      const source = (key) => ({ '1': { type: 'verse', value: { chapter: 1, verse: '2' } }, '2': { type: 'otherBook', value: { to: 'NEH 1:1' } }, '3': { type: 'unmapped' } })[key]
        ?? { type: 'verse', value: { chapter: 1, verse: key } };
      const rows = reply.value.panes[0].text.value.verses.map((v) => ({ verse: v.key, source: source(v.key) }));
      for (const pane of [...reply.value.panes, reply.value.original].filter(Boolean)) {
        if (pane.text.type !== 'ready') continue;
        const byKey = Object.fromEntries(pane.text.value.verses.map((v) => [v.key, v]));
        pane.text.value.sections = [];
        pane.text.value.verses = rows.filter((r) => r.source.type === 'verse')
          .map((r) => ({ ...byKey[r.source.value.verse], key: r.verse }));
      }
      reply.value.frame = { type: 'mapped', value: { rows } };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await titus(page).click();
    await page.getByTestId('understand').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await page.getByTestId('draft-verse-only').waitFor();
    assert.equal(await page.getByRole('tab', { name: 'Verse', exact: true }).count(), 0, 'no Section or Verse switch');
    const first = page.getByTestId('draft-source-text').first();
    assert.ok((await first.textContent()).startsWith('2with the certain hope'), await first.textContent());
    const stated = await page.getByTestId('draft-source-stated').allTextContents();
    assert.deepEqual(stated, [
      'Project verse 1:2 corresponds to NEH 1:1 in the source\'s numbering — a different book, so it cannot be shown here yet.',
      'Project verse 1:3 has no place in the source\'s verse numbering — nothing to show for it here.',
    ]);
    const labels = await page.locator('main span').filter({ hasText: /^Titus 1:/ }).allTextContents();
    assert.equal(labels[0], 'Titus 1:2', 'the first row is labelled with the source verse it shows');
    // One row per project verse, each with its own verse of the translation.
    assert.equal(await page.getByTestId('draft-target-text').count(), 16);
  }, { scenario: 'en-pins' });
});
