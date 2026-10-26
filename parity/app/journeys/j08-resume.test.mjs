// J8 (D1): the checkpoints, as tC4's e2e/j08-resume.spec.ts "commits happen at exactly the
// checkpoints" proves them. A save is no checkpoint; leaving Translate commits the pending work
// once; a switch with nothing pending commits nothing; leaving the project commits again.
//
// tC4's message ends "TIT text"; this app's ends "TIT (tC)" (a declared difference, D1 step 7).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/j08-resume.test.mjs
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { browserTest, sleep, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const box = (page, n) => page.getByLabel(`Verse ${n}`, { exact: true });
const git = (env, ...args) => execFileSync('git', ['-C', path.join(env.projectsDir, BIBLE), ...args], { encoding: 'utf8' }).trim();
const commits = (env) => Number(git(env, 'rev-list', '--count', 'HEAD'));
const lastMessage = (env) => git(env, 'log', '-1', '--format=%s');
const tab = (page, name) => page.getByTestId('mode-switch').getByRole('tab', { name });

/** Verse `n` of the chapter shown, typed and saved from its card. */
async function draft(page, n, words) {
  await page.getByRole('tab', { name: 'Verse', exact: true }).click();
  await page.getByTitle('Edit this verse').nth(n - 1).click();
  await box(page, n).fill(words);
  await page.getByRole('button', { name: 'Save verse' }).click();
  await box(page, n).waitFor({ state: 'detached' });
  await until('saved', async () => (await page.getByTestId('save-indicator').getAttribute('data-state')) === 'saved');
}

browserTest('commits happen at exactly the checkpoints: a mode switch and leaving the project commit pending work; a switch with nothing pending commits nothing', async () => {
  await withApp(async ({ page, env }) => {
    const before = commits(env);
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await tab(page, 'Translate').click();
    await page.getByRole('button', { name: /Draft section 1–3/ }).waitFor();
    await draft(page, 2, 'con la esperanza de la vida eterna, que Dios prometió');
    assert.equal(commits(env), before, 'a save is no checkpoint');

    // Checkpoint 1: leaving Translate.
    await tab(page, 'Check').click();
    await until('the mode switch commit', async () => commits(env) === before + 1);
    const first = lastMessage(env);
    assert.match(first, /^Checkpoint, leaving Translate: /);
    assert.match(first, /TIT/);
    assert.equal(await page.getByTestId('retry-checkpoint').count(), 0, 'no checkpoint failure shown');

    // Nothing pending: switching back commits nothing.
    await tab(page, 'Translate').click();
    await page.getByTitle('Edit this verse').first().waitFor();
    await sleep(2500);
    assert.equal(commits(env), before + 1);

    // Checkpoint 2: leaving the project, after another edit.
    await draft(page, 3, 'que a su debido tiempo manifestó su palabra');
    await page.getByTitle('Switch project').click();
    await page.getByTestId(`project-${BIBLE}`).waitFor();
    await until('the leave commit', async () => commits(env) === before + 2);
    assert.match(lastMessage(env), /^Checkpoint, leaving the project: .*TIT/);
    assert.equal(await page.getByTestId('home-checkpoint-error').count(), 0);
    assert.equal(await page.getByTestId('home-open-error').count(), 0);
  });
});
