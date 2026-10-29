// The design system's dropdown (crates/tc-ui/src/ds/forms/select.rs) on real screens: the edges
// tC4 #552 left open in its hand-written behaviour. Add a blank book holds the searchable list
// (J1); Report a problem holds a list that turns off while it is open.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/dropdown.test.mjs
import assert from 'node:assert/strict';
import { browserTest, catalog, openMenu, until, withApp } from './kit.mjs';

const en = catalog('en');

async function openAddBook(page) {
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-bible').click();
  await page.getByLabel('Language name').waitFor();
  await page.getByLabel('Bible name').fill('Equipo 552');
  await page.getByLabel('Language name').fill('Español');
  await page.getByLabel('Code', { exact: true }).click();
  await page.getByLabel('Code', { exact: true }).fill('es-419');
  await page.getByRole('button', { name: 'Left to right' }).click();
  await page.getByRole('button', { name: en['newBible.create'] }).click();
  await page.getByRole('button', { name: en['addBook.blankTitle'] }).click();
}

const book = (page) => page.getByRole('combobox', { name: en['addBook.book'], exact: true });
const bookLabel = (page) => page.locator('label[for="ab-book"]');
const search = (page) => page.getByPlaceholder(en['addBook.findBook']);
const isOpen = async (combobox) => (await combobox.getAttribute('aria-expanded')) === 'true';
const searchFocused = (page) => search(page).evaluate((el) => el === document.activeElement);

browserTest('tC4 #552: a press on the label of the open book list keeps the list and its search', async () => {
  await withApp(async ({ page }) => {
    await openAddBook(page);
    await book(page).click();
    await search(page).fill('jo');
    const shown = await page.getByRole('option').allTextContents();
    await bookLabel(page).click();
    await page.waitForTimeout(300);
    assert.equal(await isOpen(book(page)), true, 'the list stays open');
    assert.equal(await search(page).inputValue(), 'jo', 'the search keeps its text');
    assert.equal(await searchFocused(page), true, 'the focus stays in the search');
    assert.deepEqual(await page.getByRole('option').allTextContents(), shown, 'the same books are listed');
    // The keys still reach the list: Enter chooses the first match.
    await page.keyboard.press('Enter');
    await until('closed', async () => !(await isOpen(book(page))));
    assert.match(await book(page).textContent(), /Joshua/);
    // On the closed list the label still does what a label does: it opens the list.
    await bookLabel(page).click();
    await search(page).waitFor();
    assert.equal(await search(page).inputValue(), '');
    await page.keyboard.press('Escape');
    await page.getByRole('button', { name: en['addBook.create'] }).click();
    await page.getByTestId('understand').waitFor();
  });
});

browserTest('tC4 #552: two quick presses on the scrim with the book list open close the list, then the dialog', async () => {
  await withApp(async ({ page }) => {
    await openAddBook(page);
    await book(page).click();
    await search(page).fill('jo');
    const dialog = page.getByRole('dialog');
    const box = await dialog.boundingBox();
    // A point on the scrim, beside the dialog.
    await page.mouse.click(Math.max(4, box.x - 20), box.y + 20, { clickCount: 1 });
    await page.mouse.click(Math.max(4, box.x - 20), box.y + 20, { clickCount: 2 });
    await until('closed', async () => (await dialog.count()) === 0);
  }, { invalidOnPurpose: { folder: 'equipo_552', why: 'Add a book closes before a book is added, so the new Bible has none' } });
});

browserTest('tC4 #552: a list that turns off while it is open closes and takes no choice', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    await page.getByTestId('account-report').click();
    await page.getByTestId('feedback').waitFor();
    await until('the attachment', async () => /no Report\n?$/.test(await page.getByTestId('feedback-attachment').textContent()));
    await page.getByTestId('feedback-message').fill('The verse would not save.');
    // A first Send turns the internet on, so the next one asks nothing and no dialog takes the
    // focus from the list.
    const send = page.getByTestId('feedback-send');
    await send.click();
    await page.getByTestId('net-confirm').click();
    await until('the first send ends', async () => (await page.getByTestId('feedback-result').getAttribute('data-result').catch(() => null)) === 'timeout');
    const category = page.getByRole('combobox', { name: en['feedback.category'], exact: true });
    const before = (await category.textContent()).trim();
    await category.click();
    await page.getByRole('option', { name: en['feedback.category.bug'] }).waitFor();
    // Send turns the form off while the list is open. A press of Send's own would close the list
    // first, so the test clicks it as a script does.
    await send.evaluate((el) => el.click());
    await until('the list closes', async () => !(await isOpen(category)), 5_000);
    assert.equal(await page.getByRole('listbox').count(), 0, 'no row is left to click');
    assert.equal(await category.isDisabled(), true, 'the list is off while sending');
    assert.equal((await category.textContent()).trim(), before, 'the click chose nothing');
  }, {
    // A desktop shell whose first send times out and whose second never answers, so the form
    // stays off.
    init: (context) => context.addInitScript(() => {
      let calls = 0;
      window.tc4Shell = { feedbackSend: () => (calls++ === 0 ? Promise.resolve({ ok: false, reason: 'timeout' }) : new Promise(() => {})) };
    }),
  });
});
