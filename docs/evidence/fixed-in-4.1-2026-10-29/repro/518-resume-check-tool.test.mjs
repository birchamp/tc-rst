// tC4 #518 repro on the real app, its three parts:
//  1. A project reopened at a remembered Check tool shows no clickable tool list before the tool
//     opens (here across a reload of the page, as the window opening again loads it afresh).
//  2. A different choice made while the resume opens wins: the person clicks Translate at once.
//  3. A place recorded just before the app quits is on disk: open Translation Words, quit
//     within ~100 ms, read the project's prefs file.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/518-resume-check-tool.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { newRequestId } from '../../../../parity/app/env-rust.mjs';
import { browserTest, sleep, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const BIBLE = 'sample_burrito';
const tile = (page) => page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ });
const tab = (page, name) => page.getByTestId('mode-switch').getByRole('tab', { name, exact: true });
const home = async (page) => {
  await page.getByTestId('project-switch').click();
  await page.getByTestId(`project-${BIBLE}`).waitFor();
};
const watchPicker = (page) => page.evaluate(() => {
  window.__picker = 0;
  new MutationObserver(() => {
    if (document.querySelector('[data-testid^="preflight-"], [data-testid^="open-translation"]')) window.__picker += 1;
  }).observe(document.body, { childList: true, subtree: true });
});

async function openTool(page, tool) {
  await tab(page, 'Check').click();
  const card = page.getByTestId(`open-${tool}`);
  const all = page.getByRole('button', { name: /All checking tools/ });
  await card.or(all).first().waitFor();
  if (!(await card.count())) await all.click();
  await card.click();
  await page.getByTestId('check-progress').waitFor();
}

browserTest('518_resume_check_tool_three_parts', async () => {
  // The kit validates the projects through the server after the body; part 3 quit it on purpose.
  await withApp(async ({ page, env, allow }) => {
    // Part 1: Check › Translation Notes, Home, the page loaded afresh, the tile.
    await tile(page).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await openTool(page, 'translationNotes');
    await home(page);
    await page.reload();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    await watchPicker(page);
    await tile(page).click();
    await page.getByTestId('check-progress').waitFor();
    const resource = await page.getByTestId('check-rail').getAttribute('data-resource');
    const pickerDraws = await page.evaluate(() => window.__picker);
    console.log('part 1: resumed rail resource', resource, '| picker drawn while opening:', pickerDraws);
    assert.match(resource, /_tn$/);
    assert.equal(pickerDraws, 0, 'no tool list is drawn while the tool opens');

    // Part 2: the tile, then Translate at once; the resume must not pull the person back.
    await home(page);
    await tile(page).click();
    await tab(page, 'Translate').click();
    await sleep(3000);
    const selected = await tab(page, 'Translate').getAttribute('aria-selected');
    const railShown = await page.getByTestId('check-rail').count();
    console.log('part 2: Translate selected 3 s after clicking it during the resume:', selected, '| check rail shown:', railShown);
    assert.equal(selected, 'true');
    assert.equal(railShown, 0);

    // Part 3: open Translation Words, quit within ~100 ms, read the place on disk.
    await openTool(page, 'translationWords');
    await sleep(100);
    allow(/./); // the page loses its server after the quit
    const t0 = Date.now();
    await env.call('app/quit', { request: newRequestId() });
    const quitMs = Date.now() - t0;
    const dir = path.join(env.appData, 'prefs');
    const files = fs.readdirSync(dir).filter((f) => f.endsWith('.json') && f !== 'app.json');
    const places = files.map((f) => JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8')));
    const text = JSON.stringify(places);
    console.log(`part 3: quit answered in ${quitMs} ms; project prefs on disk:`, text.slice(0, 600));
    assert.match(text, /translationWords/, 'the place recorded ~100 ms before the quit is on disk');
  }, { scenario: 'en-pins' }).catch((e) => {
    if (!/test\/validate: status 503/.test(String(e?.message ?? e))) throw e;
    console.log('after the body: the kit\'s project validation found the server quit (expected)');
  });
});
