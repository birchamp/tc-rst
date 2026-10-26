// The quit (D1, INV-QUIT-SAVES-LINKS-THEN-CHECKPOINTS, RT-4): a link placed in the aligner is
// saved two seconds after the last change (AD18); a quit inside those two seconds saves it
// first, then checkpoints, so the last commit holds it.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/quit.test.mjs
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { newRequestId } from '../env-rust.mjs';
import { browserTest, until, withApp } from './kit.mjs';

const BIBLE = 'sample_burrito';
const chip = (page, word) => page.locator(`[data-testid="align-bank"] > button[data-word="${word}"]`).first();
const card = (page, greek) => page.locator('[data-testid^="align-card-"]', { has: page.locator(`span[lang="el"]:text-is("${greek}")`) }).first();

/** Whether the last commit links πίστιν to "fe" in Titus 1:1. */
function committedLink(env) {
  let text;
  try {
    text = execFileSync('git', ['-C', path.join(env.projectsDir, BIBLE), 'show', 'HEAD:ingredients/checking/alignments/TIT.json'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] });
  } catch {
    return false;
  }
  const bottom = text.split('"word":"πίστιν"}],"bottomWords":[')[1]?.split(']')[0];
  return bottom?.includes('"word":"fe"') ?? false;
}

browserTest('a quit with the aligner open saves its links, then checkpoints: the last commit holds them', async () => {
  await withApp(async ({ page, env, allow }) => {
    await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    await page.getByRole('tab', { name: 'Check' }).click();
    await page.getByTestId('open-align').click();
    await page.getByTestId('align-bank').waitFor();
    assert.equal(committedLink(env), false);
    await chip(page, 'fe').click();
    await until('fe chosen', async () => (await chip(page, 'fe').getAttribute('data-armed')) === '1');
    await card(page, 'πίστιν').click();
    await until('fe placed', async () => (await card(page, 'πίστιν').getAttribute('data-count')) === '1');
    assert.equal(await page.getByTestId('save-indicator').getAttribute('data-state'), 'dirty', 'the links are not saved yet');
    // The page loses its server with the quit.
    allow(/ERR_CONNECTION_REFUSED|Failed to load resource|Failed to fetch|NetworkError/);
    await env.call('app/quit', { request: newRequestId() }).catch(() => {});
    await until('the quit commit', async () => committedLink(env));
    // The commit lands before tc4 ends; a project check sent meanwhile meets a server that is
    // shutting down (503).
    await until('tc4 ended', async () => !env.alive());
    assert.match(execFileSync('git', ['-C', path.join(env.projectsDir, BIBLE), 'log', '-1', '--format=%s'], { encoding: 'utf8' }), /^Checkpoint, leaving the project: /);
  });
});
