// Community Checking (B8): the preview of Titus on A4 sheets and of the OBS sample's drafted
// story, the page setup, the export menu (its items listed; one runs only when chosen, as j07-export shows), the way
// back to the checking tools, and the book changing while the preview is open. Reading the
// preview writes nothing.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/community-checking.test.mjs
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, recordRequests, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const OBS = 'sample_burrito_obs';

/** Titus from its tile, in Check. */
async function openTitusCheck(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('open-community-checking').waitFor();
}

/** Titus from its tile, then Check, then Community Checking. */
async function openTitusPreview(page) {
  await openTitusCheck(page);
  await page.getByTestId('open-community-checking').click();
  await page.getByTestId('community-checking').waitFor();
  await page.getByTestId('cc-page').first().waitFor();
}

/** Story 1 of the OBS sample from its tile, then Check, then Community Checking. */
async function openStoryPreview(page, env) {
  await page.getByTestId(`project-${env.projectId(`_local_/_local_/${OBS}`)}`).getByTestId('story-tile-1').click();
  await page.getByTestId('story-understand').waitFor();
  await page.getByTestId('mode-switch').getByRole('tab', { name: 'Check' }).click();
  await page.getByTestId('open-community-checking').click();
  await page.getByTestId('cc-pictures').waitFor();
}

/** What the sheets print, page after page, as one text with the numbers left out. */
const printedText = (page) => page.locator('[data-testid="cc-flow"]').evaluateAll((flows) =>
  flows.map((f) => f.textContent).join(' ').replace(/\d+/g, ' ').split(/\s+/).filter(Boolean).join(' '));

const chip = (page, group, name) => page.getByRole('group', { name: group }).getByRole('button', { name, exact: true });

browserTest('cc_titus_prints_its_drafted_chapter_and_reading_it_writes_nothing', async () => {
  await withApp(async ({ page, env }) => {
    await openTitusCheck(page);
    const commits = () => Number(execFileSync('git', ['-C', path.join(env.projectsDir, BIBLE), 'rev-list', '--count', 'HEAD'], { encoding: 'utf8' }).trim());
    const before = commits();
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId('open-community-checking').click();
    await page.getByTestId('cc-page').first().waitFor();
    assert.equal(await page.getByTestId('cc-page').count(), 1, 'one A4 sheet');
    assert.equal(await page.locator('[data-testid="cc-page"] .print-title').textContent(), 'Titus');
    // Chapter 1 has verses 1 to 5 drafted; chapters 2 and 3 have none and come after it.
    assert.equal(await page.getByTestId('cc-chapter').count(), 1);
    assert.equal(await page.getByTestId('cc-chapter-gap').count(), 0);
    assert.equal(await page.locator('.print-dropcap').textContent(), '1');
    assert.equal(await page.locator('.print-undrafted').count(), 11, 'verses 6 to 16');
    assert.equal(await page.locator('[data-testid="cc-chapter"] .print-verse-number').count(), 16);
    assert.ok((await printedText(page)).startsWith('Pablo, siervo de Dios'));
    assert.equal(await page.getByText('Incomplete draft.').count(), 1, 'the notice explains the undrafted lines');
    // Opening the preview is a mode switch, which tC4 checkpoints too (state.jsx MODE_NAME.publish,
    // D1's session/leave); with nothing pending it commits nothing.
    assert.deepEqual([...new Set(requests.api(mark))].filter((p) => p !== '/api/v1/session/leave'), ['/api/v1/export/book'], 'the preview reads its print');
    assert.equal(commits(), before, 'and writes nothing');
  });
});

/** Titus 1:`from` to 1:`to` drafted through the store, as the translator's saves write them. */
async function draftVerses(env, from, to) {
  const at = (await env.call('app/hello', {})).sessions[0].at;
  const view = await env.call('editor/chapters', { at, book: 'TIT' });
  for (let n = from; n <= to; n++) {
    const row = view.chapters[0].verses.find((v) => v.key === String(n));
    const words = `Este es el versículo ${n} del borrador, con palabras suficientes para llenar más de una línea de la página impresa.`;
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words });
  }
}

browserTest('cc_the_page_setup_resets_the_sheets_and_keeps_every_word', async () => {
  await withApp(async ({ page, env }) => {
    await openTitusCheck(page);
    await draftVerses(env, 6, 16);
    await page.getByTestId('open-community-checking').click();
    await page.getByTestId('cc-page').first().waitFor();
    assert.equal(await page.locator('.print-undrafted').count(), 0, 'every verse of chapter 1 is drafted');
    const single = await printedText(page);
    const singleSheets = await page.getByTestId('cc-page').count();
    // Drop caps off: the chapter opens with its heading.
    await page.getByLabel('Drop-cap chapters').click();
    await page.getByTestId('cc-chapter-heading').waitFor();
    assert.equal(await page.getByTestId('cc-chapter-heading').textContent(), 'Chapter 1');
    assert.equal(await page.locator('.print-dropcap').count(), 0);
    // Verse numbers off: no drafted verse prints its number.
    await page.getByLabel('Verse numbers').click();
    await until('the numbers go', async () => (await page.locator('[data-testid="cc-chapter"] .print-verse-number').count()) === 0);
    await page.getByLabel('Verse numbers').click();
    await page.getByLabel('Drop-cap chapters').click();
    await until('back to drop caps', async () => (await page.locator('.print-dropcap').count()) === 1);
    // Double spacing doubles the leading and takes more sheets; the chosen chip keeps the focus.
    await chip(page, 'Spacing', 'Double').click();
    assert.equal(await chip(page, 'Spacing', 'Double').getAttribute('aria-pressed'), 'true');
    assert.equal(await chip(page, 'Spacing', 'Single').getAttribute('aria-pressed'), 'false');
    assert.equal(await chip(page, 'Spacing', 'Double').evaluate((el) => el === document.activeElement), true);
    await until('more sheets', async () => (await page.getByTestId('cc-page').count()) > singleSheets);
    assert.equal(await page.getByTestId('cc-page').first().evaluate((el) => el.style.getPropertyValue('--print-leading')), '2.8');
    // The next sheet carries on chapter 1 from inside a verse, split between words: no title,
    // no drop cap, no number before the verse's rest; no word is lost or doubled.
    const second = page.getByTestId('cc-page').nth(1);
    assert.equal(await page.getByTestId('cc-page').first().locator('.print-chapter-continues').count(), 1);
    assert.equal(await second.locator('.print-title').count(), 0);
    assert.equal(await second.locator('.print-dropcap').count(), 0);
    assert.equal(await second.evaluate((el) => el.querySelector('section > span')?.querySelector('sup') ?? null), null);
    assert.equal(await printedText(page), single);
    // Two columns, and US Letter paper.
    await chip(page, 'Columns', '2').click();
    await until('two columns', async () => (await page.getByTestId('cc-page').first().evaluate((el) => el.style.getPropertyValue('--print-columns'))) === '2');
    await chip(page, 'Paper size', 'Letter').click();
    await until('Letter paper', async () => Math.round(await page.getByTestId('cc-page').first().evaluate((el) => el.getBoundingClientRect().width)) === 816);
    assert.equal(await printedText(page), single);
  });
});

browserTest('cc_the_export_menu_lists_the_exports_and_runs_none_until_one_is_chosen', async () => {
  await withApp(async ({ page }) => {
    await openTitusPreview(page);
    const requests = recordRequests(page);
    const mark = requests.mark();
    await page.getByTestId('export-menu-trigger').click();
    const items = page.getByRole('menuitem');
    await items.first().waitFor();
    assert.deepEqual(await items.allTextContents(), ['USFM, aligned', 'USFM, plain', 'Scripture Burrito (.zip)'], 'a browser has no PDF item');
    const off = [];
    for (const item of await items.all()) if (await item.isDisabled()) off.push(await item.textContent());
    assert.deepEqual(off, ['Scripture Burrito (.zip)'], 'D2 runs the USFM exports; the Burrito zip waits for E2');
    await page.keyboard.press('Escape');
    assert.deepEqual(requests.api(mark), [], 'no export runs');
  });
});

browserTest('cc_back_returns_to_the_checking_tools', async () => {
  await withApp(async ({ page }) => {
    await openTitusPreview(page);
    await page.getByRole('button', { name: '← All checking tools' }).click();
    await page.getByTestId('community-checking-card').waitFor();
    assert.equal(await page.getByTestId('community-checking').count(), 0);
  });
});

// The book changes while the preview is open (here a verse saved through the store, as the
// project's other writers save one): the preview follows the book, as tC4's follows its state.
browserTest('cc_the_open_preview_follows_a_changed_verse', async () => {
  await withApp(async ({ page, env }) => {
    await openTitusPreview(page);
    const hello = await env.call('app/hello', {});
    const at = hello.sessions[0].at;
    const view = await env.call('editor/chapters', { at, book: 'TIT' });
    const six = view.chapters[0].verses.find((v) => v.key === '6');
    await env.call('editor/saveVerse', { at, request: newRequestId(), shown: six.words.token, words: 'Que el anciano sea irreprensible' });
    await until('verse 6 prints', async () => (await page.locator('.print-undrafted').count()) === 10);
    assert.ok((await printedText(page)).includes('te mandé. Que el anciano sea irreprensible'));
  });
});

browserTest('cc_the_obs_sample_prints_its_drafted_story_with_its_layout', async () => {
  await withApp(async ({ page, env }) => {
    await openStoryPreview(page, env);
    const story = page.getByTestId('cc-story');
    assert.equal(await story.count(), 1, 'stories 2 to 50 come after the last drafted one');
    assert.equal(await story.getAttribute('data-story'), '1');
    assert.equal(await story.locator('h1').textContent(), 'La Creación');
    assert.equal(await page.getByTestId('cc-frame-1').count(), 1);
    assert.equal(await page.getByTestId('cc-frame-2').count(), 1);
    assert.deepEqual(await page.getByTestId('cc-frame-gap').allTextContents(), ['[ frames 3–16 not yet drafted ]']);
    assert.equal(await page.getByTestId('cc-reference').textContent(), 'Una historia bíblica de: Génesis 1-2');
    assert.equal(await page.locator('[data-testid^="cc-picture-"]').count(), 0, 'no picture pack is on this computer');
    await page.getByText('The export shows each run of undrafted frames or stories as one line', { exact: false }).waitFor();
    await chip(page, 'Layout', 'Pictures wrapped').click();
    await until('wrapped', async () => (await story.getAttribute('data-layout')) === 'wrapped');
    await page.getByTestId('cc-pictures').getByRole('switch').click();
    await until('pictures off', async () => (await story.getAttribute('data-pictures')) === '0');
    await page.getByTestId('export-menu-trigger').click();
    await page.getByRole('menuitem').first().waitFor();
    assert.deepEqual(await page.getByRole('menuitem').allTextContents(), ['Story Markdown (.zip)', 'Scripture Burrito (.zip)']);
  });
});
