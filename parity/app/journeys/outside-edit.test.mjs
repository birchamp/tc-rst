// Books another program changed (D1, owner ruling OE 3, docs/STORAGE-DELTA-2026-10-21.md
// §2.11): one question for every book found, Not now keeps the project's text and lists each
// copy in the conflict review's notices with Import, a reload lists them again, and Import takes
// the file's text in with its report. The old plan's look-again refusal of
// conflict-review.test.mjs is gone: an outside file no longer makes a version at focus.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/outside-edit.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const RUT = '\\id RUT\n\\usfm 3.0\n\\h Ruth\n\\mt Ruth\n\\c 1\n\\p\n\\v 1 RUTH OUTSIDE one.\n\\v 2 RUTH OUTSIDE two.\n';
const question = (page) => page.getByTestId('changed-books');
const kept = (page) => page.getByTestId('review-kept');
const ingredients = (env, file) => path.join(env.projectsDir, BIBLE, 'ingredients', file);

async function openTranslate(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Translate' }).click();
  await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
}

/** Another program saves Titus 1:1 and adds Ruth; then the window gets focus. */
async function editOutside(page, env) {
  const tit = ingredients(env, 'TIT.usfm');
  fs.writeFileSync(tit, fs.readFileSync(tit, 'utf8').replace('Pablo, siervo', 'PABLOX, siervo'));
  fs.writeFileSync(ingredients(env, 'RUT.usfm'), RUT);
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await question(page).waitFor();
  // The watcher may find Titus a moment before Ruth: the one question then grows to name both.
  await until('one question naming both books', async () => /These books have been changed externally: (Titus, Ruth|Ruth, Titus)\. Would you like to import them into this project\?/.test(await question(page).innerText()));
}

async function verse11(env) {
  const at = (await env.call('app/hello', {})).sessions[0].at;
  const view = await env.call('editor/chapters', { at, book: 'TIT' });
  return view.chapters[0].verses.find((v) => v.key === '1').words.value;
}

/** The notices of the conflict review, through the toolbar button a kept copy alone shows. */
async function openNotices(page) {
  const button = page.getByTestId('conflicts-review');
  await button.click();
  await page.getByTestId('conflict-review').waitFor();
  await page.getByRole('tab', { name: /Notices/ }).waitFor();
  await until('the kept copies', async () => (await kept(page).count()) > 0);
}

browserTest('two changed books are asked in one question; Not now keeps the text and lists both copies with Import, also after a reload; a refused Import says why; Import takes Titus in', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await editOutside(page, env);

    await question(page).getByRole('button', { name: 'Not now' }).click();
    await question(page).waitFor({ state: 'detached' });
    assert.match(await verse11(env), /^Pablo, siervo/, 'Not now keeps the project\'s text');
    // No verse conflict: the button counts the two copies and opens at the notices.
    assert.equal(await page.getByTestId('conflicts-review').getAttribute('data-count'), '2');
    await openNotices(page);
    assert.equal(await page.getByRole('tab', { name: /Notices/ }).getAttribute('aria-selected'), 'true');
    assert.deepEqual(await kept(page).allInnerTexts().then((rows) => rows.map((r) => r.split('\n')[0]).sort()), [
      'Ruth was changed outside translationCore. A copy is kept.',
      'Titus was changed outside translationCore. A copy is kept.',
    ].sort());
    assert.equal(await page.getByTestId('kept-import').count(), 2);

    await page.reload();
    await page.getByTestId('mode-switch').waitFor();
    await openNotices(page);
    assert.equal(await kept(page).count(), 2, 'a reload lists the copies again');

    // A copy that is gone cannot be imported: the refusal shows, and the row stays.
    const { outside } = (await env.call('app/hello', {})).sessions[0];
    const ruth = outside.kept.find((k) => k.file.endsWith('RUT.usfm'));
    fs.rmSync(path.join(env.projectsDir, BIBLE, ruth.copy));
    await kept(page).filter({ hasText: 'Ruth' }).getByTestId('kept-import').click();
    await page.getByTestId('outside-refused').waitFor();
    assert.equal(await kept(page).count(), 2);

    await kept(page).filter({ hasText: 'Titus' }).getByTestId('kept-import').click();
    await until('the report', async () => /imported/.test(await question(page).innerText().catch(() => '')));
    assert.match(await question(page).innerText(), /^Titus: 1 verse imported, 0 to review\./);
    assert.match(await verse11(env), /^PABLOX, siervo/, 'Import takes the file\'s text in');
    await question(page).getByRole('button', { name: 'Close' }).click();
    await until('the Titus copy leaves the notices', async () => (await kept(page).count()) === 1);
  });
});

browserTest('Import from the question reports each book, every line drawn alike', async () => {
  await withApp(async ({ page, env }) => {
    await openTranslate(page);
    await editOutside(page, env);
    await question(page).getByRole('button', { name: 'Import' }).click();
    await until('both reports', async () => (await question(page).innerText().catch(() => '')).match(/imported/g)?.length === 2);
    const lines = question(page).locator(':scope span, :scope p, :scope div').filter({ hasText: /imported, \d+ to review\.$/ });
    const looks = await lines.evaluateAll((els) => els
      .filter((el) => ![...el.children].some((c) => /imported/.test(c.textContent)))
      .map((el) => { const s = getComputedStyle(el); return `${el.textContent} | ${s.fontSize} ${s.color} ${s.fontWeight}`; }));
    assert.equal(looks.length, 2, looks.join('\n'));
    assert.equal(looks[0].split(' | ')[1], looks[1].split(' | ')[1], looks.join('\n'));
    assert.match(looks.join('\n'), /Ruth: 2 verses imported/);
    assert.match(looks.join('\n'), /Titus: 1 verse imported/);
  });
});

browserTest('a story file changed outside: Not now keeps its copy, and the story Translate toolbar opens the notices that list it', async () => {
  await withApp(async ({ page, env }) => {
    const OBS = 'sample_burrito_obs';
    await page.getByTestId(`project-${OBS}`).getByTestId('story-tile-1').click();
    await page.getByTestId('story-understand').waitFor();
    await page.getByRole('tab', { name: 'Translate', exact: true }).click();
    await page.getByTestId('story-draft').waitFor();
    assert.equal(await page.getByTestId('conflicts-review').count(), 0);
    const story = path.join(env.projectsDir, OBS, 'ingredients', 'content', '01.md');
    const md = fs.readFileSync(story, 'utf8');
    const edited = md.replace(/(\n\n)([^!\n#][^\n]{5})/, '$1STORY OUTSIDE $2');
    assert.notEqual(edited, md);
    fs.writeFileSync(story, edited);
    await page.evaluate(() => window.dispatchEvent(new Event('focus')));
    await question(page).waitFor();
    await question(page).getByRole('button', { name: 'Not now' }).click();
    await question(page).waitFor({ state: 'detached' });
    assert.equal(await page.getByTestId('conflicts-review').getAttribute('data-count'), '1');
    await openNotices(page);
    assert.match(await kept(page).first().innerText(), /^01\.md was changed outside translationCore\. A copy is kept\./);
    assert.equal(await page.getByTestId('kept-import').count(), 1);
  });
});
