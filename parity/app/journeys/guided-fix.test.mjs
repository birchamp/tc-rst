// The guided fix's three ways (M11; B5b wires them to C8's endpoints; tC4 `fixFetch`,
// `fixSideload`, `fixRepin`): the sample pins es-419 Translation Words Links, which this computer
// lacks, so the Translation Words card offers "Fix this…". A download asks the internet question
// first and installs only the pinned commit; a file installs only when it declares that commit;
// an installed version opens the re-pin confirmation (M9b), which moves the pins only when the
// person confirms. A way that fails says why and leaves the screen and the other ways at hand.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node --test parity/app/journeys/guided-fix.test.mjs
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { browserTest, catalog, sleep, until, withApp } from './kit.mjs';
import { REPO } from '../../view-gallery/node-lib.mjs';

const en = catalog('en');
const BIBLE = 'sample_burrito';
const fflate = createRequire(import.meta.url)(path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'fflate'));

async function openFix(page) {
  await page.getByTestId(`project-${BIBLE}`).getByRole('button', { name: /Titus/ }).click();
  await page.getByTestId('understand-unit-s1').waitFor();
  await page.getByRole('tab', { name: en['nav.check'], exact: true }).click();
  await page.getByTestId('fix-translationWords').click();
  await page.getByTestId('guided-fix').waitFor();
  await page.getByTestId('fix-pin').waitFor();
  const [repoPath, sha] = (await page.getByTestId('fix-pin').textContent()).split(' · ');
  return { repoPath, sha };
}

/** A Door43 export of es-419 Translation Words Links (the English fixture under the Spanish
 * repository's name) whose metadata declares `commit`, as a Door43 download of it would. */
function exportZip(commit) {
  const dir = path.join(REPO, 'fixtures', 'tc4', 'test-fixtures', 'resources', 'en_twl@v86');
  const metadata = JSON.parse(fs.readFileSync(path.join(dir, 'metadata.json'), 'utf8'));
  const dcs = metadata.identification.primary.dcs;
  const declared = { ...Object.values(dcs)[0], revision: commit };
  metadata.identification.primary.dcs = { 'es-419_gl/es-419_twl': declared };
  const ingredient = Object.keys(metadata.ingredients).find((key) => key.endsWith('TIT.tsv'));
  return Buffer.from(fflate.zipSync({
    'metadata.json': fflate.strToU8(JSON.stringify(metadata, null, 2)),
    [ingredient]: fs.readFileSync(path.join(dir, 'TIT.tsv')),
  }));
}

/** The screen's sentence for a failed install, said once, with the server's reason after it. */
const installFailed = async (error) => {
  const text = await error.textContent();
  const lead = en['fix.failed'].replace('{reason}', '');
  assert.ok(text.startsWith(lead) && !text.slice(lead.length).startsWith(lead), text);
  return text.slice(lead.length);
};

const fileOf = (buffer) => ({ name: 'es-419_twl.zip', mimeType: 'application/zip', buffer });

browserTest('guided_fix_download_asks_first_and_a_download_door43_cannot_give_installs_nothing', async () => {
  await withApp(async ({ page, env }) => {
    await openFix(page);
    const go = page.getByTestId('fix-fetch-go');
    await env.door43.clear();
    await go.click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'fix');
    // Not now: nothing goes to Door43, and the screen is as it was.
    await page.getByTestId('net-cancel').click();
    await sleep(300);
    const sent = (await env.door43.requests()).filter((r) => r.path.startsWith('/api/') || r.path.includes('/archive/'));
    assert.deepEqual(sent, [], 'Not now sends nothing');
    assert.equal(await go.isEnabled(), true);
    assert.equal(await go.textContent(), en['check.fix.download']);
    assert.equal(await page.getByTestId('fix-progress').count(), 0);
    // Turned on: Door43 does not have the pinned commit, so nothing is installed, the screen
    // says why, and every way is at hand again.
    await go.click();
    await ask.waitFor();
    await page.getByTestId('net-confirm').click();
    const error = page.getByTestId('fix-error');
    await error.waitFor();
    assert.match(await installFailed(error), /not found/);
    assert.equal(await go.isEnabled(), true);
    assert.equal(await page.getByTestId('fix-progress').count(), 0);
    assert.equal(await page.getByTestId('fix-sideload-drop').isEnabled(), true);
    assert.equal(await page.getByTestId('guided-fix').count(), 1, 'the screen stays');
  }, { door43: true });
});

browserTest('guided_fix_file_of_another_commit_installs_nothing_and_the_pinned_one_is_installed', async () => {
  await withApp(async ({ page }) => {
    const { sha } = await openFix(page);
    const input = page.getByTestId('fix-sideload-file');
    // A file that declares another commit is refused before anything is written.
    const other = crypto.randomBytes(20).toString('hex');
    await input.setInputFiles(fileOf(exportZip(other)));
    const error = page.getByTestId('fix-error');
    await error.waitFor();
    assert.match(await installFailed(error), new RegExp(`declares revision "${other}"`));
    assert.equal(await page.getByTestId('fix-sideload-drop').isEnabled(), true);
    assert.equal(await page.getByTestId('fix-fetch-go').isEnabled(), true);
    // The pinned commit: installed, the screen closes, and the card reads again. The card now
    // names what the tool needs next, never the links that are here now.
    // While it is read, the screen says so and no other way can start.
    let release;
    const held = new Promise((resolve) => { release = resolve; });
    await page.route('**/api/v1/resources/fixSideload', async (route) => { await held; await route.continue(); }, { times: 1 });
    await input.setInputFiles(fileOf(exportZip(sha)));
    const progress = page.getByTestId('fix-progress');
    await progress.waitFor();
    assert.equal(await progress.textContent(), en['fix.sideload.reading'].replace('{name}', 'es-419_twl.zip'));
    assert.equal(await page.getByTestId('fix-sideload-drop').isEnabled(), false);
    assert.equal(await page.getByTestId('fix-fetch-go').isEnabled(), false);
    assert.equal(await page.getByTestId('fix-error').count(), 0, 'the failure before is gone');
    release();
    await page.getByTestId('guided-fix').waitFor({ state: 'detached' });
    const card = page.getByTestId('preflight-translationWords');
    await until('the card no longer asks for the links', async () => !(await card.textContent()).includes('es-419_twl'));
  });
});

browserTest('guided_fix_installed_version_opens_the_repin_confirmation_and_keep_moves_nothing', async () => {
  await withApp(async ({ page, env }) => {
    // The sample holds no other copy of the links: the server's answer gains one, as the
    // re-pin's parity case does.
    const candidate = { version: 'v19', sha: '19'.repeat(20) };
    await page.route('**/api/v1/resources/fixState', async (route) => {
      const response = await route.fetch();
      const body = await response.json();
      const added = { repoPath: body.value.pin.repoPath, ...candidate };
      await route.fulfill({ response, json: { ...body, value: { ...body.value, candidates: [added] } } });
    });
    await openFix(page);
    const choice = page.getByTestId(`fix-repin-${'19'.repeat(6)}`);
    // The server finds no such version installed: the plan is not made, and the screen says so
    // as tC4 says a failed upgrade, with the other ways at hand.
    await choice.click();
    const error = page.getByTestId('fix-error');
    await error.waitFor();
    assert.equal(await error.textContent(), en['upgrade.failed'].replace('{reason}', 'that version is not installed here'));
    assert.equal(await page.getByTestId('upgrade-confirm').count(), 0);
    // With a plan to read (scripted: the server keeps no plan under it), the confirmation takes
    // over from the screen, titled Re-pin.
    const move = { repo: 'es-419_twl', from: 'v18', to: 'v19', published: null };
    const plan = { plan: 93, what: { type: 'repin', value: { rung: 'primary', language: 'es-419', moves: [move] } }, decisionsAtRisk: 2, rows: [{ tool: 'translationWords', unit: { type: 'book', value: 'TIT' }, decisions: 2, against: 'es-419_twl', carry: { carried: 2, checkAgain: 0 } }], blocked: [], alignments: null };
    await env.call('test/script', { request: crypto.randomBytes(16).toString('hex'), next: { type: 'plan', value: plan } });
    await choice.click();
    const confirm = page.getByTestId('upgrade-confirm');
    await confirm.waitFor();
    assert.equal(await confirm.getAttribute('data-kind'), 'repin');
    await page.getByTestId('guided-fix').waitFor({ state: 'detached' });
    assert.equal(await page.getByTestId('upgrade-apply').textContent(), en['fix.repin.apply']);
    // Keep: nothing moved, and the card still offers the fix.
    await page.getByTestId('upgrade-cancel').click();
    await confirm.waitFor({ state: 'detached' });
    await page.getByTestId('fix-translationWords').waitFor();
  });
});
