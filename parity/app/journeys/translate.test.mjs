// Translate (B4a): the grid by section and by verse, a chapter switch, the verse card on real
// saves through the production `editor/saveVerse`, its
// save states, Cancel, the editing lock, and the refresh rule of AD37 when a teammate's change
// arrives under an open card (a Door43 sync or a USB file brings one; here the test writes it
// through the store as another request would), with an Arabic IME composition.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     node --test parity/app/journeys/translate.test.mjs
import assert from 'node:assert/strict';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, choose, openLanguage, recordRequests, sleep, until, withApp } from './kit.mjs';

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
  await env.call('editor/saveVerse', { at: await scope(env), request: newRequestId(), shown: row.words.token, words });
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

// OLH-1 (owner ruling Q39, tC4 #840): with the Greek tab chosen, a note's quote is marked in
// the Greek words, at the note's occurrence.
browserTest('a_note_marks_its_quote_in_the_greek_tab', async () => {
  await withApp(async ({ page }) => {
    await openTranslate(page);
    await page.getByTestId('source-tab-orig').click();
    await until('the Greek text', async () => (await page.getByTestId('draft-source-text').first().getAttribute('lang')) === 'el');
    const note = page.getByTestId('helps-note').filter({ hasText: 'abstract nouns' }).first();
    await note.hover();
    const marks = page.getByTestId('draft-source-text').first().getByTestId('source-hl');
    await until('the quote is marked in Titus 1:1', async () => (await marks.count()) > 0);
    assert.deepEqual(await marks.allTextContents(), ['κατὰ', 'πίστιν', 'ἐκλεκτῶν', 'Θεοῦ', 'καὶ', 'ἐπίγνωσιν', 'ἀληθείας']);
  }, { scenario: 'en-pins' });
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
  });
});

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
  });
});

// A failed production save keeps the exact text and offers Retry on the normal UI build.
browserTest('a_failed_verse_save_keeps_the_text_and_retry_stores_it', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await openVerse(page, 3);
    const original = await box(page, 3).inputValue();
    const wanted = `${original} x`;
    let refuse = true;
    await page.route('**/api/v1/editor/saveVerse', async (route) => {
      if (refuse) {
        refuse = false;
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ type: 'err', value: { type: 'notSaved', value: { type: 'disk', value: 'fixture write refused' } } }) });
      }
      return route.continue();
    });
    await box(page, 3).fill(wanted);
    await until('error', async () => (await indicator(page).getAttribute('data-state')) === 'error', 5_000);
    assert.equal(await box(page, 3).inputValue(), wanted);
    assert.equal((await verse(env, 3)).words.value, original);
    await indicator(page).getByRole('button', { name: 'Retry' }).click();
    await until('retry saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 3)).words.value, wanted);
  });
});

// FIX-HIDE (never hide a translator's text): a verse whose save keeps failing, then "Switch
// project". The leave is held, and the typed text stays on the page, with the failure and
// Retry, until Retry stores it.
browserTest('a_held_leave_keeps_a_failed_verse_text_on_the_page', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await openVerse(page, 3);
    const original = await box(page, 3).inputValue();
    const wanted = `${original} escrito y no guardado`;
    let refuse = true;
    await page.route('**/api/v1/editor/saveVerse', async (route) => {
      if (refuse) {
        return route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ type: 'err', value: { type: 'notSaved', value: { type: 'disk', value: 'fixture write refused' } } }) });
      }
      return route.continue();
    });
    await box(page, 3).fill(wanted);
    await until('error', async () => (await indicator(page).getAttribute('data-state')) === 'error', 5_000);
    await page.getByTestId('project-switch').click();
    await sleep(1000);
    assert.equal(await page.getByTestId(`project-${BIBLE}`).count(), 0, 'the leave is held');
    const shownInBox = (await box(page, 3).count()) > 0 && (await box(page, 3).inputValue()) === wanted;
    const shownInGrid = (await page.locator('main').innerText()).includes(wanted);
    assert.ok(shownInBox || shownInGrid, 'the typed text is on the page');
    assert.equal(await indicator(page).getAttribute('data-state'), 'error', 'the failure is shown');
    refuse = false;
    await indicator(page).getByRole('button', { name: 'Retry' }).click();
    await until('retry saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 3)).words.value, wanted);
  });
});

// REF1b, tC4 #810 (j02's poetry cases): the verse card shows a poem verse's inner lines with one
// tab per level, and what is typed at a level is saved as that level's marker.
browserTest('poem_lines_show_as_tabs_and_save_as_their_markers', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await teammateWrites(env, 4, 'a Tito,\n\\q2 mi verdadero hijo\n\\ts\\*\n\\q1 en la fe común');
    await openVerse(page, 4);
    assert.equal(await box(page, 4).inputValue(), 'a Tito,\n\t\tmi verdadero hijo\n\n\ten la fe común');
    const typed = 'a Tito,\n\t\tmi verdadero hijo\n\n\ten la fe común:\n\t\t\tgracia y paz';
    await box(page, 4).fill(typed);
    await page.getByRole('button', { name: 'Save verse' }).click();
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    assert.equal((await verse(env, 4)).words.value, typed);
    // The grid shows the words alone, every line kept.
    assert.equal((await verse(env, 4)).text, 'a Tito, mi verdadero hijo en la fe común: gracia y paz');
    // Home and back: the card reads the verse from the server again.
    await page.getByTestId('project-switch').click();
    await titus(page).click();
    await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
    await openVerse(page, 4);
    assert.equal(await box(page, 4).inputValue(), typed, 'the card opens again as it was typed');
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
  });
});

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
  });
});

// The toolbar's count of conflicts is the project's: a conflict a sync or a USB file brings in
// another book shows on the screen the person is on, with no need to leave it.
browserTest('a_conflict_in_another_book_shows_in_the_open_books_toolbar', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    assert.equal(await page.getByTestId('conflicts-review').count(), 0);
    const at = await scope(env);
    const jonah = await env.call('editor/chapters', { at, book: 'JON' });
    const { token } = jonah.chapters[0].verses.find((v) => v.key === '1').words;
    for (const words of ['Jonás, uno', 'Jonás, dos']) {
      await env.call('editor/saveVerse', { at, request: newRequestId(), shown: token, words });
    }
    await page.getByTestId('conflicts-review').waitFor();
    assert.equal(await page.getByTestId('conflicts-review').getAttribute('data-count'), '1');
  });
});

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

// tC4 app-language e: a comprehension note and a verse typed before a language switch are
// saved by their blurs and survive the switch; the book and chapter stay, the card opens again
// in the new language on the same text, and the note shows in Understand (C3 adds the note).
browserTest('app_language_e_a_verse_typed_before_the_switch_survives_it', async () => {
  await withApp(async ({ page, env }) => {
    const en = catalog('en');
    const es = catalog('es-419');
    const TEXT = 'Pablo, siervo de Dios y apóstol de Jesucristo.';
    const NOTE = 'Pablo se presenta como siervo y apóstol.';
    await titus(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByRole('button', { name: '2', exact: true }).click();
    await page.getByRole('heading', { name: 'Titus 2' }).waitFor();
    await page.getByRole('tab', { name: en['understand.byVerse'], exact: true }).click();
    const note = page.getByPlaceholder(en['understand.commentsPlaceholderVerse']).first();
    await note.fill(NOTE);
    await note.blur();
    await until('the note is saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await page.getByTestId('mode-switch').getByRole('tab', { name: en['nav.draft'] }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByRole('button', { name: 'start this verse' }).first().click();
    await box(page, 1).fill(TEXT);
    // Opening the menu leaves the card: its blur saves the text.
    await openLanguage(page);
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await choose(page, 'es-419');
    await page.getByTestId('language-apply').click();
    await until('the dialog closes', async () => (await page.getByTestId('language-dialog').count()) === 0);
    await page.getByRole('heading', { name: 'Titus 2' }).waitFor();
    assert.ok((await page.getByTestId('draft-target-text').first().textContent()).includes(TEXT));
    const view = await env.call('editor/chapters', { at: (await env.call('app/hello', {})).sessions[0].at, book: 'TIT' });
    assert.equal(view.chapters[1].verses[0].words.value, TEXT);
    await page.getByTitle(es['draft.editVerse']).first().click();
    const label = es['draft.verseLabel'].replace('{n}', '1');
    assert.equal(await page.getByLabel(label, { exact: true }).inputValue(), TEXT);
    await page.getByRole('button', { name: es['draft.cancelVerse'], exact: true }).click();
    await page.getByLabel(label, { exact: true }).waitFor({ state: 'detached' });
    await page.getByTestId('mode-switch').getByRole('tab', { name: es['nav.understand'] }).click();
    const shown = page.getByPlaceholder(es['understand.commentsPlaceholderVerse']).first();
    await until('the note shows in Spanish', async () => (await shown.count()) > 0 && (await shown.inputValue()) === NOTE);
    assert.ok((await env.call('editor/book', { at: await scope(env), book: 'TIT' })).comments.some((c) => c.text === NOTE));
  });
});

// tC4 app-language i (#657, the left-to-right project's half; a right-to-left project is made by
// C6's create): scripture follows the project, not the interface. Under Arabic the page reads
// right to left, and the source, the translation and the verse card read left to right.
browserTest('app_language_i_scripture_follows_the_project_under_an_arabic_interface', async () => {
  await withApp(async ({ page }) => {
    const ar = catalog('ar');
    await titus(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: ar['nav.draft'] }).click();
    await page.getByTestId('draft-target-text').first().waitFor();
    assert.equal(await page.evaluate(() => document.documentElement.dir), 'rtl');
    const direction = (locator) => locator.evaluate((el) => getComputedStyle(el).direction);
    assert.equal(await direction(page.getByTestId('draft-source-text').first()), 'ltr');
    assert.equal(await direction(page.getByTestId('draft-target-text').first()), 'ltr');
    await page.getByRole('tab', { name: ar['understand.byVerse'], exact: true }).click();
    await page.getByTitle(ar['draft.editVerse']).first().click();
    const card = page.getByLabel(ar['draft.verseLabel'].replace('{n}', '1'), { exact: true });
    assert.equal(await direction(card), 'ltr');
    assert.equal(await card.getAttribute('dir'), 'ltr');
  }, { locale: 'ar' });
});
