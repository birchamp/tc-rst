// The Check picker's way out of a card that cannot open its tool (B5a): the "Get source texts"
// and "Download" buttons open the source texts dialog (tC4 `Check.jsx` calls `openSources` on
// both, and the Download one stops the click reaching the card), and a tool that is no longer
// ready when the screen opens it leaves the picker, whose Fix card says why (tC4's
// `resumeCheckTool`), instead of a dead-end error.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/check-picker.test.mjs
import assert from 'node:assert/strict';
import { browserTest, catalog, withApp } from './kit.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';

const openProject = async (page) => {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
};

browserTest('picker_get_source_texts_and_download_open_the_source_texts_dialog', async () => {
  await withApp(async ({ page }) => {
    // The sample's Translation Words is pinned to a resource this computer lacks, and its
    // Translation Notes is ready: the answer is rewritten into a tool with no pins and a tool
    // that opens on the English fallback.
    await page.route('**/api/v1/check/picker', async (route) => {
      const response = await route.fetch();
      const body = await response.json();
      const [words, notes] = body.value.tools;
      Object.assign(words, { state: 'unpinned', pin: null, rung: null });
      notes.fallbackFor = { repoPath: 'git.door43.org/es-419_gl/es-419_tn', version: 'v18', sha: '0'.repeat(40) };
      await route.fulfill({ response, json: body });
    });
    await openProject(page);
    await page.getByRole('tab', { name: 'Check' }).click();
    const modal = page.getByTestId('sources-modal');

    await page.getByTestId('preflight-translationWords').getByRole('button', { name: en['check.fix.getResources'] }).click();
    await modal.waitFor();
    await page.keyboard.press('Escape');
    await modal.waitFor({ state: 'detached' });

    // The Download button sits inside a card that opens its tool when clicked: only the dialog opens.
    await page.getByTestId('fetch-primary-translationNotes').click();
    await modal.waitFor();
    assert.equal(await page.getByTestId('check-progress').count(), 0, 'the click did not open the tool');
    await page.keyboard.press('Escape');
    await modal.waitFor({ state: 'detached' });
    await page.getByTestId('preflight-translationNotes').waitFor();
  });
});

browserTest('resume_into_a_tool_that_is_not_ready_shows_the_picker_and_its_fix', async () => {
  await withApp(async ({ page }) => {
    await openProject(page);
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-translationNotes').click();
    await page.getByTestId('check-progress').waitFor();
    await page.getByTestId('project-switch').click();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    // The place kept says Translation Notes; its pins went missing meanwhile (the sample
    // lacks Translation Words' es-419 resource, so the open asks for that tool).
    await page.route('**/api/v1/check/open', (route) => {
      const body = route.request().postDataJSON();
      body.list.tool = 'translationWords';
      return route.continue({ postData: JSON.stringify(body) });
    });
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('fix-translationWords').waitFor();
    assert.equal(await page.getByTestId('check-error').count(), 0, 'no dead-end error');
    await page.getByTestId('preflight-translationNotes').waitFor();
  });
});
