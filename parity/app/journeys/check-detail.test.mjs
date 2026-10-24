// The Check detail column with the keyboard and the mouse (tC4 #42 `e2e/check-keyboard.spec.ts`,
// #726 `e2e/item-navigation.spec.ts`, #737 the drag, #532 `e2e/check-returns-draft.spec.ts`),
// B5b's parts: the detail's Tab stops, the Academy drawer holding the focus, the target words as
// one stop with Shift+arrow selection, the marking row in the painted order, Previous and Next
// at the ends of the list and wrapping in a narrow window, a mark saved at once, and the check
// comment saved on Done and on leaving the check.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/check-detail.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';

async function openTool(page, tool, locale = 'en') {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByRole('tab', { name: catalog(locale)['nav.check'], exact: true }).click();
  await page.getByTestId(`open-${tool}`).click();
  await page.getByTestId('check-progress').waitFor();
  await page.getByTestId('check-quote').waitFor();
}

const activeIs = (page, selector) => page.evaluate((sel) => document.activeElement?.matches(sel) ?? false, selector);
const testidOfFocus = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-testid') ?? null);

/** The check shown now: its index in the list, the rail's selected row. */
const shownIndex = (page) => page.locator('[data-testid="check-list"] [aria-current="true"]').getAttribute('data-rail-index');

/** The open's scope, for a call the page itself makes. */
async function scopeOf(env) {
  const hello = await env.call('app/hello', {});
  return hello.sessions[0].at;
}

const list = (tool) => ({ tool, unit: { type: 'book', value: 'TIT' } });
const item = async (env, tool, index) => env.call('check/item', { at: await scopeOf(env), list: list(tool), index });

/** Tab until `test` holds, at most `limit` presses; the stops passed, in order. */
async function tabTo(page, test, limit = 60, shift = false) {
  const stops = [];
  for (let n = 0; n < limit; n++) {
    await page.keyboard.press(shift ? 'Shift+Tab' : 'Tab');
    const id = await testidOfFocus(page);
    stops.push(id);
    if (test(id)) return stops;
  }
  throw new Error(`no stop matched in ${limit} Tab presses: ${stops}`);
}

async function boxesOf(row) {
  return row.locator('button').evaluateAll((buttons) => buttons.map((b) => {
    const r = b.getBoundingClientRect();
    return { x: r.x, y: r.y, right: r.right, bottom: r.bottom };
  }));
}

// #726: the marking row in the painted order, its geometry and look, and Tab following it.
for (const locale of ['en', 'ar']) {
  for (const tool of ['translationNotes', 'translationWords']) {
    browserTest(`item_navigation_${tool}_${locale}_marking_row_boundaries_tab_and_wrapping`, async () => {
      await withApp(async ({ page }) => {
        const c = catalog(locale);
        await openTool(page, tool, locale);
        const rtl = locale === 'ar';
        // The tW rail groups by word, so the DOM order is not the list's order.
        await page.locator('[data-testid="check-list"] [data-rail-index="0"]').click();
        await until('Previous is off on the first check', () => page.getByTestId('check-prev').isDisabled());
        await page.getByTestId('check-next').click();
        await until('Next moves to the second check', async () => (await shownIndex(page)) === '1');
        await until('the second check is drawn', () => page.getByTestId('check-prev').isEnabled().catch(() => false));
        const ids = ['check-prev', 'mark-valid', 'mark-invalid', 'mark-todo', 'check-next'];
        const row = page.getByTestId('check-prev').locator('..');
        assert.equal(await row.locator('button').count(), ids.length);
        assert.deepEqual(await row.locator('button').evaluateAll((b) => b.map((x) => x.getAttribute('data-testid'))), ids);
        const boxes = await boxesOf(row);
        for (let i = 1; i < boxes.length; i++) {
          assert.ok(Math.abs((boxes[i].y + boxes[i].bottom) / 2 - (boxes[0].y + boxes[0].bottom) / 2) < 1, 'one line');
          const gap = rtl ? boxes[i - 1].x - boxes[i].right : boxes[i].x - boxes[i - 1].right;
          assert.ok(gap >= 10, `a gap of ${gap}`);
        }
        const last = boxes.length - 1;
        const first = rtl ? boxes[0].x - boxes[1].right : boxes[1].x - boxes[0].right;
        const end = rtl ? boxes[last - 1].x - boxes[last].right : boxes[last].x - boxes[last - 1].right;
        assert.ok(Math.abs(first - 14) < 0.5 && Math.abs(end - 14) < 0.5, `the row's end gaps are ${first} and ${end}`);
        for (const [index, which] of [[0, 'prev'], [last, 'next']]) {
          const button = page.getByTestId(ids[index]);
          assert.equal(await button.getAttribute('title'), c[`check.${which}Item`]);
          const arrow = (which === 'prev') === rtl ? '→' : '←';
          assert.equal((await button.textContent()).trim(),
            which === 'prev' ? `${arrow} ${c['check.prevLabel']}` : `${c['check.nextLabel']} ${arrow}`);
          const look = await button.evaluate((el) => ({ height: getComputedStyle(el).height, radius: parseFloat(getComputedStyle(el).borderRadius) }));
          assert.equal(look.height, '30px');
          assert.ok(look.radius > 100, 'a pill');
        }
        // From the enabled Previous, Tab follows the painted row.
        await page.getByTestId(ids[0]).focus();
        for (const id of ids.slice(1)) {
          await page.keyboard.press('Tab');
          assert.equal(await testidOfFocus(page), id);
        }
        // The header holds no button: Previous and Next are in the marking row (#726).
        assert.equal(await page.getByTestId('check-item-counter').locator('..').locator('button').count(), 0);
        await page.getByTestId('check-prev').click();
        await until('Previous returns to the first check', async () => (await shownIndex(page)) === '0');
        await until('the first check is drawn', () => page.getByTestId('check-prev').isDisabled().catch(() => false));
        const indices = await page.locator('[data-testid="check-list"] button[data-rail-index]').evaluateAll((b) => b.map((x) => Number(x.getAttribute('data-rail-index'))));
        await page.locator(`[data-testid="check-list"] [data-rail-index="${Math.max(...indices)}"]`).click();
        await until('Next is off on the last check', () => page.getByTestId('check-next').isDisabled());
        // In a narrow window the row wraps and nothing overlaps.
        await page.setViewportSize({ width: 620, height: 1100 });
        const wrapped = await boxesOf(row);
        assert.ok(new Set(wrapped.map((b) => Math.round((b.y + b.bottom) / 2))).size > 1, 'the row wraps');
        for (let i = 0; i < wrapped.length; i++) {
          for (let j = i + 1; j < wrapped.length; j++) {
            const a = wrapped[i]; const b = wrapped[j];
            assert.ok(a.right <= b.x || b.right <= a.x || a.bottom <= b.y || b.bottom <= a.y, `buttons ${i} and ${j} overlap`);
          }
        }
      }, { locale, scenario: 'en-pins' });
    });
  }
}

// #42: the detail column's Tab stops, the drawer, the words and the marks, with the keyboard alone.
browserTest('check_keyboard_detail_drawer_words_marks_and_comment', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationWords');
    // Move on until the check's verse is drafted with three words or more.
    await page.locator('[data-testid="check-list"] [aria-current="true"]').focus();
    for (let n = 0; n < 40; n++) {
      if (await page.locator('[data-testid="check-target"][data-drafted="1"] [data-testid="tw-2"]').count()) break;
      await page.keyboard.press('ArrowDown');
    }
    await page.getByTestId('tw-2').waitFor();
    // Tab leaves the list at once, straight into the detail pane.
    await page.keyboard.press('Tab');
    assert.ok(await page.evaluate(() => !!document.activeElement?.closest('main')), 'the focus is in the detail');

    // The Academy drawer holds the focus and returns it.
    if (!(await activeIs(page, '[data-testid="open-academy"]'))) await tabTo(page, (id) => id === 'open-academy');
    await page.keyboard.press('Enter');
    const drawer = page.getByTestId('academy-drawer');
    await drawer.waitFor();
    const inside = () => page.evaluate(() => !!document.activeElement?.closest('[data-testid="academy-drawer"]'));
    await until('the drawer takes the focus', inside);
    for (let n = 0; n < 4; n++) {
      await page.keyboard.press('Tab');
      assert.ok(await inside(), `Tab stop ${n + 1} stays in the drawer`);
    }
    await page.keyboard.press('Escape');
    await drawer.waitFor({ state: 'detached' });
    await until('the focus is back on Learn more', () => activeIs(page, '[data-testid="open-academy"]'));

    // The target words are one Tab stop; Shift+arrow selects by word.
    const stops = await tabTo(page, (id) => /^tw-\d+$/.test(id ?? ''));
    assert.equal(stops.at(-1), 'tw-0');
    const pressed = (i) => page.getByTestId(`tw-${i}`).getAttribute('aria-pressed');
    const w0 = (await page.getByTestId('tw-0').textContent()).trim();
    const w1 = (await page.getByTestId('tw-1').textContent()).trim();
    await page.keyboard.press('Shift+ArrowRight');
    assert.equal(await pressed(0), 'true');
    assert.equal(await pressed(1), 'false');
    await page.keyboard.press('Shift+ArrowRight');
    assert.equal(await pressed(1), 'true');
    assert.ok(await activeIs(page, '[data-testid="tw-2"]'), 'the caret moves on past the selection');
    await page.keyboard.press('Shift+ArrowLeft');
    assert.equal(await pressed(1), 'false');
    assert.equal(await pressed(0), 'true');
    await page.keyboard.press('Shift+ArrowRight');
    assert.equal(await pressed(1), 'true');
    // Space selects the focused word alone and clears it again: gaps stay possible.
    await page.keyboard.press(' ');
    assert.equal(await pressed(2), 'true');
    await page.keyboard.press(' ');
    assert.equal(await pressed(2), 'false');
    assert.equal(await pressed(1), 'true');

    // The line is one roving stop: Tab leaves it and Shift+Tab comes back to the word the caret is on.
    await page.keyboard.press('Tab');
    assert.equal(await activeIs(page, '[data-testid^="tw-"]'), false, 'Tab leaves the words');
    await page.keyboard.press('Shift+Tab');
    assert.ok(await activeIs(page, '[data-testid="tw-2"]'), 'Shift+Tab returns to the caret word');

    // The marking row: Previous (skipped while disabled), the three marks, Next.
    if (await page.getByTestId('check-prev').isEnabled()) {
      await page.keyboard.press('Tab');
      assert.equal(await testidOfFocus(page), 'check-prev');
    }
    for (const id of ['mark-valid', 'mark-invalid', 'mark-todo', 'check-next']) {
      await page.keyboard.press('Tab');
      assert.equal(await testidOfFocus(page), id);
    }
    for (let n = 0; n < 3; n++) await page.keyboard.press('Shift+Tab');
    assert.ok(await activeIs(page, '[data-testid="mark-valid"]'));
    const index = Number(await shownIndex(page));
    await page.keyboard.press('Enter');
    // The mark is saved at once: the server shows the two words, as the checker tapped them.
    const saved = await until('the mark is stored', async () => {
      const view = await item(env, 'translationWords', index);
      return view.decision.value.status === 'valid' ? view : null;
    });
    assert.deepEqual(saved.target.selected.map((i) => saved.target.words[i]), [w0, w1]);
    // The reads the mark brought changed the detail in place: the button keeps the focus, and
    // the words keep the selection.
    await until('the reads have landed', () => page.getByTestId('mark-valid').evaluate((b) => getComputedStyle(b).backgroundColor !== 'rgb(255, 255, 255)'));
    assert.ok(await activeIs(page, '[data-testid="mark-valid"]'), 'Mark valid keeps the focus after its reads');
    assert.equal(await pressed(1), 'true');

    // A comment; Done returns the focus to the comment button.
    await tabTo(page, (id) => id === 'comment-toggle');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Tab');
    assert.equal(await testidOfFocus(page), 'comment-text');
    await page.keyboard.type('Checked with the keyboard.');
    await page.keyboard.press('Tab');
    assert.equal(await testidOfFocus(page), 'comment-done');
    await page.keyboard.press('Enter');
    await page.getByTestId('comment-editor').waitFor({ state: 'detached' });
    await until('the focus is back on the comment button', () => activeIs(page, '[data-testid="comment-toggle"]'));
    const commented = await until('the comment is stored', async () => {
      const view = await item(env, 'translationWords', index);
      return view.decision.value.comment ? view : null;
    });
    assert.equal(commented.decision.value.comment, 'Checked with the keyboard.');
    assert.equal(commented.decision.value.status, 'valid', 'the comment left the mark alone');
  }, { scenario: 'en-pins' });
});

// tC4 #737: a press on a word and a drag across others selects them whole; Mark valid saves them.
browserTest('a_drag_selects_whole_words_and_mark_valid_saves_them', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    await page.getByTestId('tw-4').waitFor();
    const box = async (i) => (await page.getByTestId(`tw-${i}`).boundingBox());
    const from = await box(1);
    const to = await box(3);
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    await page.mouse.move(to.x + to.width / 2, to.y + to.height / 2, { steps: 6 });
    await page.mouse.up();
    const pressedWords = () => page.locator('[data-testid^="tw-"][aria-pressed="true"]').evaluateAll((spans) => spans.map((s) => s.getAttribute('data-testid')));
    assert.deepEqual(await pressedWords(), ['tw-1', 'tw-2', 'tw-3']);
    // The click that ends a drag toggles nothing; the next click on a selected word clears it.
    await page.getByTestId('tw-2').click();
    assert.deepEqual(await pressedWords(), ['tw-1', 'tw-3']);
    await page.getByTestId('tw-2').click();
    const words = await Promise.all([1, 2, 3].map(async (i) => (await page.getByTestId(`tw-${i}`).textContent()).trim()));
    const index = Number(await shownIndex(page));
    await page.getByTestId('mark-valid').click();
    const saved = await until('the mark is stored', async () => {
      const view = await item(env, 'translationNotes', index);
      return view.decision.value.status === 'valid' ? view : null;
    });
    assert.deepEqual(saved.target.selected.map((i) => saved.target.words[i]), words);
    // Next before marking drops the selection (tC4's Q2).
    await page.getByTestId('check-next').click();
    await until('the next check shows', async () => Number(await shownIndex(page)) !== index);
    assert.deepEqual(await pressedWords(), []);
  }, { scenario: 'en-pins' });
});

// A-10, Q-25: a comment being typed when the checker moves on is saved with the check it was
// typed on, never dropped (tC4 drops it) and never put on the next check.
browserTest('check_comment_saved_on_leaving_the_item', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    const first = Number(await shownIndex(page));
    await page.getByTestId('comment-toggle').click();
    await page.getByTestId('comment-text').fill('Ask the team about this one.');
    // No Done: the checker goes to the next check.
    await page.getByTestId('check-next').click();
    await until('the next check shows', async () => Number(await shownIndex(page)) !== first);
    await page.getByTestId('check-quote').waitFor();
    const left = await until('the comment is stored on the check it was typed on', async () => {
      const view = await item(env, 'translationNotes', first);
      return view.decision.value.comment ? view : null;
    });
    assert.equal(left.decision.value.comment, 'Ask the team about this one.');
    assert.equal((await item(env, 'translationNotes', first + 1)).decision.value.comment, null, 'not the next check');
    // The editor is closed on the next check; back on the first, the comment is written.
    assert.equal(await page.getByTestId('comment-editor').count(), 0);
    await page.getByTestId('check-prev').click();
    await until('the first check shows again', async () => Number(await shownIndex(page)) === first);
    await until('its comment button says written', async () => (await page.getByTestId('comment-toggle').getAttribute('data-written')) === '1');
    // A comment typed and saved with Done, then deleted, leaves none.
    await page.getByTestId('comment-toggle').click();
    assert.equal(await page.getByTestId('comment-text').inputValue(), 'Ask the team about this one.');
    await page.getByTestId('comment-delete').click();
    await page.getByTestId('comment-editor').waitFor({ state: 'detached' });
    await until('the comment is removed', async () => (await item(env, 'translationNotes', first)).decision.value.comment === null);
  }, { scenario: 'en-pins' });
});

// C4, A-10: while a check comment is typed, its text is on the server too (`editor/draft` into
// the project's `.tc/drafts.json`), so a crash or a power cut before Done cannot lose it; the
// next open offers it (tc-app `a_check_comment_typed_before_the_app_ended_is_offered_and_kept_
// beside_the_mark`). The save on Done clears the draft, so a saved comment is never offered.
browserTest('a_check_comment_being_typed_is_drafted_on_the_server_until_it_is_saved', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    const index = Number(await shownIndex(page));
    await page.getByTestId('mark-invalid').click();
    await until('the mark is stored', async () => (await item(env, 'translationNotes', index)).decision.value.status === 'invalid');
    const typed = 'Ask the team about the faith of the chosen people.';
    await page.getByTestId('comment-toggle').click();
    await page.getByTestId('comment-text').fill(typed);
    const draftFile = path.join(env.projectsDir, 'sample_burrito', '.tc', 'drafts.json');
    const drafted = () => fs.existsSync(draftFile) && JSON.parse(fs.readFileSync(draftFile, 'utf8')).some((d) => d.text === typed);
    await until('the comment being typed is drafted on the server', drafted, 5_000);
    assert.equal((await item(env, 'translationNotes', index)).decision.value.comment, null, 'a draft is not a save');
    await page.getByTestId('comment-done').click();
    const saved = await until('Done saves it', async () => {
      const view = await item(env, 'translationNotes', index);
      return view.decision.value.comment ? view : null;
    });
    assert.equal(saved.decision.value.comment, typed);
    assert.equal(saved.decision.value.status, 'invalid', 'the mark stays');
    assert.equal(saved.decision.marker, null, 'no conflict with the person themselves');
    await until('the saved comment leaves no draft', async () => !drafted());
  }, { scenario: 'en-pins' });
});

// C4, A-10, the other order: a comment typed first, then a mark or a bookmark. The mark saves no
// comment, so the comment's server draft stays, and it is drafted again over the mark's version,
// so a Keep after a crash continues from the mark instead of standing beside it as a conflict.
for (const button of ['mark-invalid', 'bookmark']) {
  browserTest(`a_check_comment_typed_before_${button.replace('-', '_')}_stays_drafted_on_the_server`, async () => {
    await withApp(async ({ page, env }) => {
      await openTool(page, 'translationNotes');
      const index = Number(await shownIndex(page));
      const typed = 'Ask the team about the faith of the chosen people.';
      await page.getByTestId('comment-toggle').click();
      await page.getByTestId('comment-text').fill(typed);
      const draftFile = path.join(env.projectsDir, BIBLE, '.tc', 'drafts.json');
      const drafted = () => (fs.existsSync(draftFile) ? JSON.parse(fs.readFileSync(draftFile, 'utf8')) : [])
        .find((d) => d.text === typed);
      const before = await until('the comment being typed is drafted on the server', drafted, 5_000);
      await page.getByTestId(button).click();
      await until('the mark is stored', async () => {
        const d = (await item(env, 'translationNotes', index)).decision.value;
        return button === 'bookmark' ? d.bookmarked : d.status === 'invalid';
      });
      const after = await until('the draft names the mark\'s version', () => {
        const d = drafted();
        return d && d.shown[0] !== before.shown[0] ? d : null;
      }, 5_000);
      // Past the draft file's next write: the mark did not take the unsaved comment's draft.
      await sleep(1_500);
      assert.deepEqual(drafted()?.shown, after.shown, 'the draft stays');
      assert.equal((await item(env, 'translationNotes', index)).decision.value.comment, null, 'a draft is not a save');
      await page.getByTestId('comment-done').click();
      const saved = await until('Done saves it', async () => {
        const view = await item(env, 'translationNotes', index);
        return view.decision.value.comment ? view : null;
      });
      assert.equal(saved.decision.value.comment, typed);
      assert.equal(saved.decision.marker, null, 'no conflict with the person themselves');
      await until('the saved comment leaves no draft', async () => !drafted());
    }, { scenario: 'en-pins' });
  });
}

/** Holds every `check/decide` reply back for `ms`, as a slow fsync does: the write is stored, the page hears late. */
const slowDecides = (page, ms) => page.route('**/api/v1/check/decide', async (route) => {
  const response = await route.fetch();
  await sleep(ms);
  await route.fulfill({ response });
});

// A comment and a mark are two writes of one decision: whichever order they come in and however
// slowly the disk answers, both are on the shown version and neither is a conflict with the
// person's own earlier write.
for (const [name, delay, order, leave] of [
  ['comment_typed_then_mark_then_done_keeps_both', 0, 'mark first', 'comment-done'],
  ['comment_typed_then_mark_then_next_keeps_both', 0, 'mark first', 'check-next'],
  ['comment_typed_then_mark_while_the_disk_is_slow_then_done_keeps_both', 200, 'mark first', 'comment-done'],
  ['comment_typed_then_mark_while_the_disk_is_slow_then_next_keeps_both', 200, 'mark first', 'check-next'],
  ['comment_done_then_mark_while_the_disk_is_slow_keeps_both', 200, 'comment first', 'comment-done'],
]) {
  browserTest(name, async () => {
    await withApp(async ({ page, env }) => {
      await openTool(page, 'translationNotes');
      const index = Number(await shownIndex(page));
      if (delay) await slowDecides(page, delay);
      await page.getByTestId('comment-toggle').click();
      await page.getByTestId('comment-text').fill('Ask the team.');
      if (order === 'mark first') {
        await page.getByTestId('mark-invalid').click();
        await page.getByTestId(leave).click();
      } else {
        await page.getByTestId(leave).click();
        await page.getByTestId('mark-invalid').click();
      }
      const both = await until('the mark and the comment are stored', async () => {
        const view = await item(env, 'translationNotes', index);
        return view.decision.value.status === 'invalid' && view.decision.value.comment ? view : null;
      });
      assert.equal(both.decision.value.comment, 'Ask the team.');
      // Let a late write land, then look again: the person's own two writes are no conflict.
      await sleep(delay * 3 + 300);
      const last = await item(env, 'translationNotes', index);
      assert.equal(last.decision.value.status, 'invalid');
      assert.equal(last.decision.value.comment, 'Ask the team.');
      assert.equal(last.decision.marker, null, 'no conflict with the person themselves');
    }, { scenario: 'en-pins' });
  });
}

// A reply that comes after the checker moved on is for the check it was sent for: the next check
// shows its own decision, and a mark on it is written over its own token.
browserTest('a_slow_comment_save_does_not_touch_the_check_moved_to', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    const first = Number(await shownIndex(page));
    await slowDecides(page, 200);
    await page.getByTestId('comment-toggle').click();
    await page.getByTestId('comment-text').fill('Ask the team.');
    await page.getByTestId('check-next').click();
    await until('the next check shows', async () => Number(await shownIndex(page)) !== first);
    await page.getByTestId('check-quote').waitFor();
    await until('the comment is stored on the first check', async () => (await item(env, 'translationNotes', first)).decision.value.comment);
    // The reply reaches the page; the check shown has not taken anything from it.
    await sleep(300);
    assert.equal(await page.getByTestId('comment-toggle').getAttribute('data-written'), '0', 'the next check has no comment');
    await page.getByTestId('mark-invalid').click();
    const second = await until('the mark is stored on the second check', async () => {
      const view = await item(env, 'translationNotes', first + 1);
      return view.decision.value.status === 'invalid' ? view : null;
    });
    assert.equal(second.decision.value.comment, null);
    const left = await item(env, 'translationNotes', first);
    assert.equal(left.decision.value.status, null, 'the first check holds only its comment');
    assert.equal(left.decision.value.comment, 'Ask the team.');
  }, { scenario: 'en-pins' });
});

// tC4 #532: a verse drafted or edited while the checker was away shows when they come back.
browserTest('check_shows_new_draft_after_switching_back', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    await page.getByTestId('tw-0').waitFor();
    const index = Number(await shownIndex(page));
    const before = await item(env, 'translationNotes', index);
    assert.equal((await page.getByTestId('tw-0').textContent()).trim(), before.target.words[0]);
    // To Understand, where the draft is not shown, and the verse changes as Translate's save does.
    await page.getByRole('tab', { name: catalog('en')['nav.understand'], exact: true }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await env.call('editor/saveVerse', {
      at: await scopeOf(env),
      request: (await import('../env-rust.mjs')).newRequestId(),
      shown: before.target.verse,
      words: 'Nuevo borrador de este versículo.',
    });
    await page.getByRole('tab', { name: catalog('en')['nav.check'], exact: true }).click();
    await until('the new draft shows', async () => (await page.getByTestId('tw-0').textContent().catch(() => '')).trim() === 'Nuevo');
    assert.equal((await page.locator('[data-testid^="tw-"]').count()), 5);
  }, { scenario: 'en-pins' });
});

// tC4 #762 (`e2e/compare-card.spec.ts`, which renders 1 to 6 sources in a harness; the product
// shows two): as many source texts in a row as fit at 40ch each, all as wide as one another, a
// card alone in the last row as wide as the row, and "Your translation" a card of its own below.
// OLH-1 (owner ruling Q39, tC4 #840): the Greek row of the check card marks the check's quote,
// the words the server names, as the gateway row marks its rendering.
browserTest('the_greek_row_marks_the_quote_of_the_check', async () => {
  await withApp(async ({ page, env }) => {
    await openTool(page, 'translationNotes');
    const index = Number(await shownIndex(page));
    const view = await item(env, 'translationNotes', index);
    const verse = view.sources.value.original.value.verse;
    const named = verse.lit.map((i) => verse.tokens[i]);
    assert.ok(named.length > 0, `the first note's quote is in its verse: ${JSON.stringify(view.sources)}`);
    const marks = page.getByTestId('orig-pane').getByTestId('orig-hl');
    await until('the Greek row marks the quote', async () => (await marks.count()) > 0);
    assert.deepEqual(await marks.allTextContents(), named);
    assert.ok((await page.getByTestId('ult-pane').getByTestId('ult-hl').count()) > 0, 'the gateway row is marked too');
  });
});

browserTest('compare_card_columns_fit_the_width', async () => {
  await withApp(async ({ page }) => {
    await openTool(page, 'translationNotes');
    for (const width of [900, 1280, 1600, 1920]) {
      await page.setViewportSize({ width, height: 900 });
      await page.evaluate(() => document.fonts.ready);
      const { row, min, cards, yours } = await page.evaluate(() => {
        const box = (el) => { const r = el.getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, bottom: r.bottom }; };
        const sources = document.querySelector('[data-testid="compare-sources"]');
        // The 40ch minimum in the row's own font: a probe of that width, as tC4's harness has.
        const probe = document.createElement('div');
        probe.style.cssText = 'flex: none; width: calc(40ch + 40px); height: 1px';
        sources.appendChild(probe);
        const min = probe.getBoundingClientRect().width;
        probe.remove();
        return {
          row: box(sources),
          min,
          cards: ['orig-pane', 'ult-pane'].map((id) => box(document.querySelector(`[data-testid="${id}"]`))),
          yours: box(document.querySelector('[data-testid="compare-translation"]')),
        };
      });
      const label = `at ${width}px`;
      const GAP = 6;
      const fit = Math.max(1, Math.floor((row.width + GAP) / (min + GAP)));
      const columns = Math.min(2, fit);
      const inOneRow = Math.abs(cards[0].y - cards[1].y) < 1;
      assert.equal(inOneRow, columns === 2, `${label}: ${columns} column(s)`);
      if (inOneRow) {
        assert.ok(Math.abs(cards[0].width - cards[1].width) < 1, `${label}: equal cards`);
        for (const c of cards) assert.ok(c.width >= min - 1, `${label}: no card under 40ch`);
      } else {
        // A card alone in its row takes the full width.
        for (const c of cards) assert.ok(Math.abs(c.width - row.width) < 1, `${label}: a lone card is as wide as the row`);
      }
      // "Your translation" is a card of its own, 24 px below the sources, as wide as the row.
      assert.ok(Math.abs(yours.y - (row.bottom ?? Math.max(...cards.map((c) => c.bottom))) - 24) < 1.5
        || Math.abs(yours.y - Math.max(...cards.map((c) => c.bottom)) - 24) < 1.5, `${label}: 24 px below the sources`);
      assert.ok(Math.abs(yours.width - row.width) < 1, `${label}: as wide as the row`);
    }
  }, { scenario: 'en-pins' });
});

// tC4 #614 (`ds-labels`): the drawer's close button, eyebrow and title come from the shown
// language's catalog, not from English literals in the design system's props.
browserTest('academy_drawer_close_label_is_in_the_shown_language', async () => {
  await withApp(async ({ page }) => {
    const c = catalog('ar');
    await openTool(page, 'translationNotes', 'ar');
    await page.getByTestId('open-academy').click();
    const drawer = page.getByTestId('academy-drawer');
    await drawer.waitFor();
    const close = drawer.getByRole('button').first();
    assert.equal(await close.getAttribute('title'), c['common.close']);
    assert.equal(await close.getAttribute('aria-label'), c['common.close']);
    const eyebrow = (await drawer.locator('span').first().textContent()).trim();
    assert.ok(eyebrow.startsWith(c['check.academyEyebrow'].split('{')[0].trim()), `the eyebrow is Arabic: ${eyebrow}`);
  }, { locale: 'ar', scenario: 'en-pins' });
});
