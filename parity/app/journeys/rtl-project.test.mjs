// A right-to-left project through the page (G4 steps 5 and 6, INV-RTL-PROJECTS-EDIT-CORRECTLY;
// tC4 `e2e/j10-rtl.spec.ts`, J10 as an axis of J2, J4 and J5): an Arabic Bible created, Titus
// added from a USFM file, a verse drafted, a check decided and a word aligned, each read right to
// left and saved as typed and chosen. The Hebrew original in Align on Jonah waits for a fixture
// with hbo_uhb and for the cards' lang to follow the original (G4's log, REQUEST).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/rtl-project.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const listed = (env) => fs.readdirSync(env.projectsDir).filter((name) => !name.startsWith('.')).sort();
const json = (env, ...parts) => JSON.parse(fs.readFileSync(path.join(env.projectsDir, ...parts), 'utf8'));

// Titus 1 in Arabic (Van Dyck), three verses: enough for a book the app takes whole.
const TITUS = Buffer.from([
  '\\id TIT',
  '\\usfm 3.0',
  '\\h تيطس',
  '\\mt تيطس',
  '\\c 1',
  '\\p',
  '\\v 1 بولس عبد الله',
  '\\v 2 على رجاء الحياة الأبدية',
  '\\v 3 وإنما أظهر كلمته في أوقاتها الخاصة',
  '',
].join('\n'));
const TYPED = 'بولس، عبد الله ورسول يسوع المسيح';

const indicator = (page) => page.getByTestId('save-indicator');
const checkTab = (page) => page.getByRole('tab', { name: en['nav.check'], exact: true });
const allTools = (page) => page.getByRole('button', { name: /All checking tools/ });
const bank = (page) => page.locator('[data-testid="align-bank"] > button');
const placed = (page) => page.getByTestId('align-progress').textContent();
const direction = (locator) => locator.evaluate((el) => getComputedStyle(el).direction);

/** Home, and the leave's checkpoint written: the project's files are what it holds. */
async function leave(page, env, project) {
  await page.getByTitle(/Switch project/).click();
  await page.getByTestId(`project-${project}`).waitFor();
  await env.call('settings/viewFromHome', { project });
}

/** A new Bible in Arabic, right to left, as J1 fills it, with Titus from `TITUS`. */
async function createArabicTitus(page, env) {
  const before = listed(env);
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-bible').click();
  await page.getByLabel(en['newBible.langName']).waitFor();
  await page.getByLabel(en['newBible.name']).fill('Arabic Titus');
  await page.getByLabel(en['newBible.langName']).fill('العربية');
  await page.getByLabel(en['newBible.code'], { exact: true }).click(); // leaving the field closes its suggestions
  await page.getByLabel(en['newBible.code'], { exact: true }).fill('ar');
  await page.getByRole('button', { name: en['wizard.rtl'] }).click();
  await page.getByRole('button', { name: en['newBible.create'] }).click();
  await page.getByTestId('ab-usfm-option').click();
  await page.getByTestId('ab-usfm-input').setInputFiles([{ name: 'TIT.usfm', mimeType: 'text/plain', buffer: TITUS }]);
  const row = page.getByTestId('ab-usfm-file');
  await until('the file is read as valid', async () => (await row.count()) === 1 && (await row.getAttribute('data-status')) === 'valid');
  await page.getByTestId('ab-usfm-add').click();
  await page.getByTestId('ab-usfm').waitFor({ state: 'detached' });
  const [folder, ...more] = listed(env).filter((name) => !before.includes(name));
  assert.deepEqual(more, [], 'exactly one new project folder');
  await page.getByTestId(`project-${folder}`).getByTestId('book-tile-TIT').waitFor();
  return folder;
}

browserTest('rtl_project_drafts_checks_and_aligns', async () => {
  await withApp(async ({ page, env }) => {
    const project = await createArabicTitus(page, env);
    assert.equal(json(env, project, 'ingredients', 'checking', 'settings.json').textDirection, 'rtl');
    const usfm = path.join(env.projectsDir, project, 'ingredients', 'TIT.usfm');

    // J2: verse 1 drafted in Arabic, in a card that reads right to left.
    await page.getByTestId(`project-${project}`).getByTestId('book-tile-TIT').click();
    await page.getByRole('heading', { name: 'Titus 1', exact: true }).waitFor();
    await page.getByTestId('mode-switch').getByRole('tab', { name: en['nav.draft'] }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByTitle(en['draft.editVerse']).first().click();
    const verse = page.getByLabel(en['draft.verseLabel'].replace('{n}', '1'), { exact: true });
    await verse.waitFor();
    assert.equal(await verse.getAttribute('dir'), 'rtl');
    assert.equal(await direction(verse), 'rtl');
    await verse.fill(TYPED);
    await page.getByRole('button', { name: 'Save verse' }).click();
    await verse.waitFor({ state: 'detached' });
    await until('saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await leave(page, env, project);
    const saved = fs.readFileSync(usfm).toString('utf8').split('\n').find((line) => line.startsWith('\\v 1 '));
    assert.ok(saved, 'Titus 1:1 is in the file');
    assert.deepEqual(Buffer.from(saved.slice('\\v 1 '.length)), Buffer.from(TYPED), 'the verse on disk is the typed bytes');

    // J4: a tapped word marked valid is one decision record on disk.
    await page.getByTestId(`project-${project}`).getByTestId('book-tile-TIT').click();
    await checkTab(page).click();
    const picker = page.getByTestId('open-translationNotes');
    await picker.or(allTools(page)).first().waitFor();
    if (!(await picker.count())) await allTools(page).click();
    await picker.click();
    await page.getByTestId('check-quote').waitFor();
    await page.locator('[data-testid="check-list"] [data-rail-index="0"]').click();
    const target = page.getByTestId('check-target');
    await until('the drafted verse shows', async () => (await target.getAttribute('data-drafted')) === '1');
    assert.equal(await direction(target), 'rtl', 'the checked verse reads right to left');
    const word = target.locator('[data-testid^="tw-"]', { hasText: 'يسوع' }).first();
    await word.click();
    assert.equal(await word.getAttribute('aria-pressed'), 'true');
    await page.getByTestId('mark-valid').click();
    await until('the rail row is decided', async () => (await page.locator('[data-testid="check-list"] [data-rail-index="0"]').getAttribute('data-decided')) === '1');
    await leave(page, env, project);
    const decisions = json(env, project, 'ingredients', 'checking', 'translationNotes', 'TIT.json').decisions;
    assert.equal(decisions.length, 1, 'one decision record');
    assert.equal(decisions[0].status, 'valid');
    assert.deepEqual(decisions[0].selections.map((s) => s.text), ['يسوع']);

    // J5: the bank reads right to left, and one placed word is a link on disk.
    await page.getByTestId(`project-${project}`).getByTestId('book-tile-TIT').click();
    await checkTab(page).click();
    await page.getByTestId('open-align').or(allTools(page)).first().waitFor();
    if (!(await page.getByTestId('open-align').count())) await allTools(page).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    assert.equal(await page.getByTestId('align-bank').getAttribute('dir'), 'rtl');
    assert.equal(await direction(page.getByTestId('align-bank')), 'rtl');
    const words = await bank(page).evaluateAll((chips) => chips.map((c) => c.getAttribute('data-word')));
    assert.deepEqual(words.slice(0, 2), ['بولس', 'عبد'], 'the bank keeps the typed order');
    const first = bank(page).first();
    const [right, left] = [await first.boundingBox(), await bank(page).nth(1).boundingBox()];
    assert.ok(left.x < right.x, 'the second word sits left of the first');
    const before = await placed(page);
    await first.click();
    await until('the word is chosen', async () => (await first.getAttribute('data-armed')) === '1');
    const greek = await page.locator('[data-testid^="align-card-"] span[lang]').first().textContent();
    await page.locator('[data-testid^="align-card-"]').first().click();
    await until('one more word placed', async () => (await placed(page)) !== before);
    await until('the link saved', async () => (await indicator(page).getAttribute('data-state')) === 'saved');
    await leave(page, env, project);
    const alignment = json(env, project, 'ingredients', 'checking', 'alignments', 'TIT.json').chapters['1']['1'];
    assert.ok(alignment.alignments.some((group) => group.topWords.some((w) => w.word === greek)
      && group.bottomWords.some((w) => w.word === 'بولس')), `${greek} keeps بولس on disk`);
  });
});
