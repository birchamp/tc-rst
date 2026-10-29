// tC4 #548 and #552 on the real app: create a Bible (as J1 does), open "Add a blank book" and
// drive its searchable Book dropdown the way a translator does: type in the search, Home and End
// in the search, the page's ids while the list is open, the search field's name, a code-only
// filter, a press on the label of the open list, Escape inside the dialog, a press outside, and
// the arrow keys and Enter.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/548-552-book-dropdown.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, catalog, focused, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const en = catalog('en');

async function openAddBook(page) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-bible').click();
  await page.getByLabel('Language name').waitFor();
  await page.getByLabel('Bible name').fill('Equipo 548');
  await page.getByLabel('Language name').fill('Español');
  await page.getByLabel('Code', { exact: true }).click();
  await page.getByLabel('Code', { exact: true }).fill('es-419');
  await page.getByRole('button', { name: 'Left to right' }).click();
  await page.getByRole('button', { name: en['newBible.create'] }).click();
  await page.getByRole('button', { name: en['addBook.blankTitle'] }).click();
}

const book = (page) => page.getByRole('combobox', { name: en['addBook.book'], exact: true });
const listbox = (page) => page.getByRole('listbox');
const options = (page) => listbox(page).getByRole('option').allTextContents();
const search = (page) => page.getByPlaceholder(en['addBook.findBook']);
const isOpen = async (page) => (await book(page).getAttribute('aria-expanded')) === 'true';
const active = (page) => search(page).getAttribute('aria-activedescendant');

browserTest('548_552_book_dropdown_as_a_translator_uses_it', async () => {
  await withApp(async ({ page }) => {
    const out = {};
    try {
    await openAddBook(page);
    await book(page).waitFor();
    out.comboboxFoundByName = await book(page).count();

    // #548 item 2: ids while the searchable list is open, and the search field's own name.
    await book(page).click();
    await search(page).waitFor();
    out.focusAfterOpen = await page.evaluate(() => `${document.activeElement.tagName}#${document.activeElement.id}`);
    out.duplicateIds = await page.evaluate(() => {
      const seen = {};
      for (const el of document.querySelectorAll('[id]')) (seen[el.id] ??= []).push(el.tagName);
      return Object.entries(seen).filter(([, v]) => v.length > 1);
    });
    out.search = await search(page).evaluate((el) => ({ id: el.id, ariaLabel: el.getAttribute('aria-label'), labelledBy: el.getAttribute('aria-labelledby') }));
    out.searchByRoleAndName = await page.getByRole('searchbox', { name: en['addBook.findBook'] }).or(page.getByRole('textbox', { name: en['addBook.findBook'] })).count();

    // #548 item 1: Home and End in the search field move the text cursor, not the highlight.
    await search(page).fill('jo');
    const before = await active(page);
    await search(page).press('Home');
    const atHome = await search(page).evaluate((el) => el.selectionStart);
    const afterHome = await active(page);
    await search(page).press('End');
    const atEnd = await search(page).evaluate((el) => el.selectionStart);
    out.homeEnd = { highlightBefore: before, highlightAfterHome: afterHome, caretAfterHome: atHome, caretAfterEnd: atEnd, highlightAfterEnd: await active(page) };
    out.filterJo = await options(page);

    // #548 item 3: a text that matches a code and no name.
    await search(page).fill('jhn');
    out.filterJhn = await options(page);
    await search(page).fill('1jn');
    out.filter1jn = await options(page);
    await search(page).fill('jn');
    out.filterJn = await options(page);
    await page.screenshot({ path: path.join(SHOTS, '548-book-list-search.png') });

    // #552 edge 1: a press on the label of the open list.
    await search(page).fill('jo');
    out.labelElement = await page.getByText(en['addBook.book'], { exact: true }).first().evaluate((el) => `${el.tagName} for=${el.htmlFor ?? el.getAttribute('for')} -> ${document.getElementById(el.htmlFor)?.tagName}`);
    await page.getByText(en['addBook.book'], { exact: true }).first().click();
    await page.waitForTimeout(300);
    out.labelPressOnOpenList = { open: await isOpen(page), searchText: (await search(page).count()) ? await search(page).inputValue() : null, focus: await focused(page) };
    if (await isOpen(page)) await page.keyboard.press('Escape');

    // A click on the label of the closed list focuses the button (#548 criterion 3).
    await page.getByText(en['addBook.book'], { exact: true }).first().click();
    await page.waitForTimeout(200);
    out.labelClickClosed = { focusIsButton: await book(page).evaluate((el) => el === document.activeElement), open: await isOpen(page) };
    if (await isOpen(page)) await page.keyboard.press('Escape');

    // Escape in the list closes the list, not the dialog, and the focus is on the field.
    await book(page).click();
    await search(page).waitFor();
    await page.keyboard.press('Escape');
    await until('closed', async () => !(await isOpen(page)));
    out.escape = { dialogs: await page.getByRole('dialog').count(), focusIsButton: await book(page).evaluate((el) => el === document.activeElement) };

    // The keyboard alone: ArrowDown opens, typing filters, ArrowDown and Enter choose.
    await book(page).focus();
    await page.keyboard.press('ArrowDown');
    await search(page).waitFor();
    await page.keyboard.type('john');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await until('closed', async () => !(await isOpen(page)));
    out.keyboardChoice = (await book(page).textContent()).trim();

    // A press outside the open list closes it and changes nothing.
    await book(page).click();
    await search(page).waitFor();
    const box = await page.getByRole('dialog').boundingBox();
    await page.mouse.click(box.x + 12, box.y + 12);
    await page.waitForTimeout(200);
    out.outsidePress = { open: await isOpen(page), value: (await book(page).textContent()).trim(), dialogs: await page.getByRole('dialog').count() };

    // Add the chosen book, as J1 does, so the new project is a whole Burrito.
    await page.getByRole('button', { name: en['addBook.create'] }).click();
    await page.getByTestId('understand').waitFor();
    out.bookAdded = true;
    } finally { console.log(JSON.stringify(out, null, 1)); }
    assert.deepEqual(out.duplicateIds, [], 'no two elements share an id');
    assert.equal(out.homeEnd.caretAfterHome, 0);
    assert.equal(out.homeEnd.caretAfterEnd, 2);
    assert.equal(out.homeEnd.highlightAfterHome, out.homeEnd.highlightBefore);
  });
});
