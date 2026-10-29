// tC4 #525 on the real app: a first launch (fresh app data, no saved language) on a computer
// whose language is French (browser locale fr-FR, navigator.languages ['fr-FR','fr'], and
// LANG=fr_FR.UTF-8 for tc4). Records which interface language the app starts in.
//
//   cd <worktree> && t=$PWD/target && LANG=fr_FR.UTF-8 LANGUAGE=fr TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app TC4_UI_HOOKS=$t/tc-ui-app-hooks TC4_DOOR43_FAKE=$t/debug/tc-door43-fake tc-heavy node --test --test-timeout=180000 docs/evidence/fixed-in-4.1-2026-10-29/repro/525-first-launch-system-language.test.mjs
import assert from 'node:assert/strict';
import path from 'node:path';
import test from 'node:test';
import { startRustEnv } from '../../../../parity/app/env-rust.mjs';
import { setup } from '../../../../parity/app/journeys/kit.mjs';
import { CHROMIUM_ARGS } from '../../../../parity/app/observe.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');

test('525_first_launch_on_a_french_computer', { skip: setup.skip ?? false, timeout: 120_000 }, async () => {
  const browser = await setup.chromium.launch({ args: [...CHROMIUM_ARGS, '--lang=fr-FR'] });
  const env = await startRustEnv({ scenario: 'store', seed: () => {}, tc4: setup.tc4, ui: setup.ui });
  try {
    const context = await browser.newContext({ ...env.contextOptions, locale: 'fr-FR' });
    await env.prepare(context);
    const page = await context.newPage();
    const hello = await env.call('app/hello', {});
    console.log('server LANG:', process.env.LANG, '| app/hello prefs on first launch:', JSON.stringify(hello.prefs ?? hello.value?.prefs ?? hello));
    await page.goto(`${env.origin}/#t=${env.token}`);
    await page.getByTestId('account-menu').waitFor();
    const seen = await page.evaluate(() => ({ languages: navigator.languages, lang: document.documentElement.lang, title: document.querySelector('main h1')?.textContent ?? null }));
    console.log('page:', JSON.stringify(seen));
    await page.screenshot({ path: path.join(SHOTS, '525-first-launch-french-computer.png') });
    assert.deepEqual(seen.languages.slice(0, 1), ['fr-FR'], 'the computer says French');
    // #525 asks for fr here; record what the app does.
    console.log(seen.lang === 'fr' ? 'RESULT: French chosen automatically' : `RESULT: app started in ${seen.lang}, not French`);
  } finally {
    await env.close();
    await browser.close();
  }
});
