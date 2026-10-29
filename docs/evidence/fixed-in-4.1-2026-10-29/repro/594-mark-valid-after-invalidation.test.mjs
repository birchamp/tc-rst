// tC4 #594 repro on the real app, the issue's own steps (J6 then a re-decision): Titus 1:1's
// seeded tW decision selects "Dios"; Translate › Verse changes "Dios" to "Señor"; Check › Translation
// Words shows the check flagged; click "Señor", Mark valid. Expected: the dot is no longer
// flagged, the check counts as decided, and the stored record is `invalidated: false,
// status: "valid"` with the new word. Then To do on the same check, to show what To do does.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/594-mark-valid-after-invalidation.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { browserTest, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const BIBLE = 'sample_burrito';
const list = { tool: 'translationWords', unit: { type: 'book', value: 'TIT' } };
const scopeOf = async (env) => (await env.call('app/hello', {})).sessions[0].at;
const item = async (env, index) => env.call('check/item', { at: await scopeOf(env), list, index });
const checkTab = (page) => page.getByRole('tab', { name: 'Check', exact: true });
const row = (page, i) => page.locator(`[data-testid="check-list"] [data-rail-index="${i}"]`);
const shownIndex = async (page) => Number(await page.locator('[data-testid="check-list"] [aria-current="true"]').getAttribute('data-rail-index'));
const progress = async (page) => (await page.getByTestId('check-progress').textContent()).trim();
const flags = async (page, i) => ({ invalid: await row(page, i).getAttribute('data-invalid'), decided: await row(page, i).getAttribute('data-decided') });

async function select(page, index) {
  await row(page, index).click();
  await until(`check ${index} shows`, async () => (await shownIndex(page)) === index);
  await page.getByTestId('check-target').waitFor();
}

browserTest('594_mark_valid_on_a_draft_invalidated_check', async () => {
  await withApp(async ({ page, env }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await checkTab(page).waitFor();
    await checkTab(page).click();
    const tw = page.getByTestId('open-translationWords');
    const all = page.getByRole('button', { name: /All checking tools/ });
    await tw.or(all).first().waitFor();
    if (!(await tw.count())) await all.click();
    await tw.click();
    await page.getByTestId('check-quote').waitFor();
    const index = 2;
    const before = await item(env, index);
    assert.deepEqual(before.target.selected.map((i) => before.target.words[i]), ['Dios']);

    // 1. Edit the verse in Translate so "Dios" becomes "Señor".
    await page.getByRole('tab', { name: 'Translate', exact: true }).click();
    await page.getByRole('tab', { name: 'Verse', exact: true }).click();
    await page.getByTitle('Edit this verse').first().click();
    const verse = page.getByLabel('Verse 1', { exact: true });
    await verse.waitFor();
    await verse.fill((await verse.inputValue()).replaceAll('Dios', 'Señor'));
    await page.getByRole('button', { name: 'Save verse' }).click();
    await verse.waitFor({ state: 'detached' });
    await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved');

    // 2. Back in Check: the check is flagged.
    await checkTab(page).click();
    await page.getByTestId('check-quote').waitFor();
    await select(page, index);
    await page.getByTestId('invalidated-notice').waitFor();
    const flagged = await flags(page, index);
    const progressFlagged = await progress(page);
    console.log('flagged:', JSON.stringify(flagged), 'progress:', JSON.stringify(progressFlagged));
    assert.equal(flagged.invalid, '1');

    // 3. Click "Señor", Mark valid.
    const target = (await item(env, index)).target;
    const senor = target.words.indexOf('Señor');
    assert.ok(senor >= 0, `Señor is in the verse: ${target.words}`);
    await page.getByTestId(`tw-${senor}`).click();
    await page.getByTestId('mark-valid').click();
    const valid = await until('the Valid is stored', async () => {
      const v = await item(env, index);
      return v.decision.value.status === 'valid' && v.decision.value.invalidated === false ? v : null;
    }, 15_000);
    await until('the rail row is no longer flagged', async () => (await flags(page, index)).invalid === '0');
    const progressValid = await progress(page);
    console.log('after Mark valid:', JSON.stringify(await flags(page, index)), 'progress:', JSON.stringify(progressValid),
      'selected:', JSON.stringify(valid.target.selected.map((i) => valid.target.words[i])),
      'notice shown:', await page.getByTestId('invalidated-notice').count());
    assert.deepEqual(valid.target.selected.map((i) => valid.target.words[i]), ['Señor']);
    assert.equal((await flags(page, index)).decided, '1');
    assert.equal(await page.getByTestId('invalidated-notice').count(), 0);
    await page.screenshot({ path: path.join(SHOTS, '594-mark-valid-after-invalidation.png') });

    // 4. On disk after leaving the project.
    await page.getByTitle(/Switch project/).click();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    await env.call('settings/viewFromHome', { project: BIBLE });
    const file = JSON.parse(fs.readFileSync(path.join(env.projectsDir, BIBLE, 'ingredients', 'checking', 'translationWords', 'TIT.json'), 'utf8'));
    const mine = file.decisions.filter((d) => d.contextId.reference.chapter === 1 && d.contextId.reference.verse === 1
      && (d.selections || []).some((s) => s.text === 'Señor'));
    console.log('on disk:', JSON.stringify(mine.map((d) => ({ status: d.status, userStatus: d.userStatus, invalidated: d.invalidated, selections: d.selections.map((s) => s.text) }))));
    assert.equal(mine.length, 1);
    assert.equal(mine[0].invalidated, false);
    assert.ok(mine[0].status === 'valid' || mine[0].userStatus === 'valid', JSON.stringify(mine[0]));
  }, { scenario: 'en-pins' });
});
