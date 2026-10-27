// E2 (USB exchange, APP-DESIGN §10): three computers, no internet. A saves a file for a
// teammate; B, who has no project, receives it as a new one, writes a verse and saves a file
// back; A receives B's file into the same project; C receives B's file as a new project. The
// exports and receives run for real (tc4 under --test-hooks scripts only Door43), so the zips
// are the files a translator would carry on a stick.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO, newRequestId } from '../env-rust.mjs';
import { browserTest, catalog, until, withApps } from './kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };

const BIBLE = 'sample_burrito';
const FROM_A = 'Pablo, siervo de Dios, escrito en A antes del viaje en memoria USB';
const FROM_B = 'Pablo, siervo de Dios, escrito en B para el viaje de vuelta';

/** Back to Home through the project switch, as tC4's Back to Home; the session closes. */
async function goHome(app) {
  await app.page.getByTestId('project-switch').click();
  await app.page.getByTestId('account-menu').waitFor();
}

/** The open session's scope after the project's Titus opened from Home (a computer with no source
 * texts installed shows the editor's chrome and no Understand units). */
async function openTitus(app, project) {
  await app.page.getByTestId(`project-${project}`).getByRole('button', { name: /Titus/ }).click();
  await app.page.getByTestId('mode-switch').waitFor();
  const hello = await app.env.call('app/hello', {});
  return hello.sessions[0].at;
}

async function titus(app, at, n) {
  const view = await app.env.call('editor/chapters', { at, book: 'TIT' });
  return view.chapters[0].verses.find((v) => v.key === String(n));
}

/** Writes Titus 1:`n` through the production save, over the version shown. */
async function writes(app, at, n, words) {
  const row = await titus(app, at, n);
  await app.env.call('editor/saveVerse', { at, request: newRequestId(), shown: row.words.token, words });
}

/** Saves a file for a teammate from Home's card: the one zip the folder then holds. */
async function saveForTeammate(app, project) {
  const folder = fs.mkdtempSync(path.join(app.env.projectsDir, '..', 'stick-'));
  await app.page.getByTestId(`usb-menu-${project}`).click();
  await app.page.getByRole('menuitem', { name: en['home.saveForTeammate'], exact: true }).click();
  await app.pickFolder(folder);
  await app.page.getByTestId('save-for-teammate-choose').click();
  await until('the folder shows', () => app.page.getByTestId('save-for-teammate-folder').textContent().then((t) => t.includes(folder)));
  await app.page.getByTestId('save-for-teammate-save').click();
  await app.page.getByTestId('save-for-teammate-done').waitFor();
  await app.page.getByTestId('save-for-teammate-leave').click();
  const zips = fs.readdirSync(folder).filter((f) => f.endsWith('.zip'));
  assert.equal(zips.length, 1, `one file in ${folder}: ${zips}`);
  return path.join(folder, zips[0]);
}

/** Receives `file` into `project` from its Home card; the Received lines. */
async function receiveInto(app, project, file) {
  await app.page.getByTestId(`usb-menu-${project}`).click();
  await app.page.getByRole('menuitem', { name: en['home.receiveWork'], exact: true }).click();
  await app.pickFile(file);
  await app.page.getByTestId('receive-work-choose').click();
  await app.page.getByTestId('receive-work-confirm').waitFor();
  await app.page.getByTestId('receive-work-receive').click();
  await app.page.getByTestId('receive-work-received').waitFor();
  const lines = await app.page.getByTestId('receive-work-received').textContent();
  await app.page.getByTestId('receive-work-leave').click();
  return lines;
}

/** Receives `file` as a new project through Add a project; the new project's folder name. */
async function receiveNew(app, file) {
  await app.page.getByTestId('add-project').click();
  await app.page.getByTestId('add-project-receive').click();
  await app.pickFile(file);
  await app.page.getByTestId('receive-work-choose').click();
  const name = app.page.getByTestId('receive-work-name');
  await name.waitFor();
  const folder = await name.inputValue();
  assert.ok(folder.length > 0, 'the new project has a default name');
  await app.page.getByTestId('receive-work-receive').click();
  await app.page.getByTestId('receive-work-added').waitFor();
  await app.page.getByTestId('receive-work-leave').click();
  await app.page.getByTestId(`project-${folder}`).waitFor();
  return folder;
}

browserTest('E2: a file saved for a teammate opens on their computer, comes back with their verse, and reaches a third computer', async () => {
  await withApps(['sample', 'store', 'store'], async ([a, b, c]) => {
    const atA = await openTitus(a, BIBLE);
    await writes(a, atA, 2, FROM_A);
    await goHome(a);
    const fromA = await saveForTeammate(a, BIBLE);

    const onB = await receiveNew(b, fromA);
    const atB = await openTitus(b, onB);
    assert.equal((await titus(b, atB, 2)).words.value, FROM_A);
    await writes(b, atB, 1, FROM_B);
    await goHome(b);
    const fromB = await saveForTeammate(b, onB);

    const lines = await receiveInto(a, BIBLE, fromB);
    assert.match(lines, /1/, `the Received sheet counts B's verse: ${lines}`);
    const atA2 = await openTitus(a, BIBLE);
    assert.equal((await titus(a, atA2, 1)).words.value, FROM_B);
    assert.equal((await titus(a, atA2, 2)).words.value, FROM_A);
    // The receive's outcome outlives the dialog: after a reload, the Received sheet's query
    // still answers it (step 4b).
    await a.page.reload();
    await a.page.getByTestId('mode-switch').waitFor();
    await goHome(a);
    const kept = await a.env.call('door43/lastSync', { project: BIBLE });
    assert.ok(kept.view, 'the receive is the last sync');
    // B's verse alone: the pins B's open wrote are bookkeeping, not counted (owner ruling
    // 2026-10-26).
    assert.equal(kept.view.received, 1, JSON.stringify(kept.view));
    assert.equal(kept.view.verseConflicts, 0, JSON.stringify(kept.view));

    const onC = await receiveNew(c, fromB);
    const atC = await openTitus(c, onC);
    assert.equal((await titus(c, atC, 1)).words.value, FROM_B);
    assert.equal((await titus(c, atC, 2)).words.value, FROM_A);
  });
});
