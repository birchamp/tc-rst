// Update a book from USFM (F1 step 8; STORAGE §13.6, owner ruling OE 5): a book file edited in
// another program comes into the open project. A verse the file changed, with no known starting
// point, waits in the conflict review (H20); the report counts it as "to review", never as
// imported, and links to the review. tC4 4.0 has no such item: a declared difference.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/update-book.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../env-rust.mjs';
import { browserTest, until, withApp } from './kit.mjs';

const rebuilt = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8'));

browserTest('update Titus from an edited USFM file: the counts, then the link to the review', async () => {
  await withApp(async ({ page, env }) => {
    const project = 'sample_burrito';
    const book = path.join(env.projectsDir, project, 'ingredients', 'TIT.usfm');
    const usfm = fs.readFileSync(book, 'utf8');
    // One verse edited elsewhere: its first word of 1:1 changes.
    const edited = usfm.replace(/(\\v 1 )(\S+)/, (_, v, word) => `${v}${word}X`);
    assert.notEqual(edited, usfm, 'the fixture has a 1:1 to edit');
    const file = path.join(env.appData, 'TIT-edited.usfm');
    fs.writeFileSync(file, edited);

    await page.getByTestId(`project-${project}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByRole('heading', { name: /^Titus \d+$/ }).waitFor({ timeout: 120_000 });
    await page.getByTestId('account-menu').click();
    await page.getByTestId('account-update-book').click();
    const modal = page.getByTestId('update-book-modal');
    await modal.waitFor();
    await page.getByTestId('update-book-input').setInputFiles(file);
    const next = page.getByTestId('update-book-next');
    await until('the file is sent', () => next.isEnabled());
    await next.click();
    await page.getByTestId('update-book-book').waitFor();
    await page.getByTestId('update-book-run').click();
    const report = page.getByTestId('update-book-report');
    await report.waitFor({ timeout: 120_000 });
    const review = Number(await report.getAttribute('data-review'));
    const imported = Number(await report.getAttribute('data-imported'));
    assert.ok(review >= 1, 'the edited verse waits in the review');
    const counts = (imported === 1 ? rebuilt['updateBook.reportOne'] : rebuilt['updateBook.report'])
      .replace('{book}', 'Titus').replace('{n}', String(imported)).replace('{review}', String(review));
    assert.equal(await page.getByTestId('update-book-counts').textContent(), counts);
    assert.equal(await modal.getByRole('button', { name: /undo/i }).count(), 0, 'no Undo (delta Q7)');
    // Until the person chooses, the project keeps its text (U3: never provisional).
    assert.equal(fs.readFileSync(book, 'utf8'), usfm);

    await page.getByTestId('update-book-review').click();
    await page.getByTestId('conflict-review').waitFor({ timeout: 120_000 });
    await modal.waitFor({ state: 'detached' });
  });
});
