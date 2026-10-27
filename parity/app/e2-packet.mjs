// The USB exchange screens (unit E2 step 7) for the owner to sign off: a picture of each state
// of "Save work for a teammate" and "Receive work from a file", taken from the real app as the
// journey parity/app/journeys/usb-exchange.test.mjs drives it (two computers, A with the sample
// and B with nothing; a shell stub answers the folder and file pickers).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node parity/app/e2-packet.mjs
//
// Each screen is saved as `<name>.png` in docs/evidence/e2-usb-2026-10-26/.
import fs from 'node:fs';
import path from 'node:path';
import { REPO, newRequestId } from './env-rust.mjs';
import { catalog, setup, withApps } from './journeys/kit.mjs';

const en = { ...catalog('en'), ...JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8')) };

const OUT = path.join(REPO, 'docs', 'evidence', 'e2-usb-2026-10-26');
const BIBLE = 'sample_burrito';
if (setup.skip) throw new Error(setup.skip);
fs.mkdirSync(OUT, { recursive: true });

async function shot(page, name) {
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(OUT, `${name}.png`) });
  console.log(name);
}

await withApps(['sample', 'store'], async ([a, b]) => {
  await a.page.setViewportSize({ width: 1024, height: 768 });
  await shot(a.page, '01-home-card-1024');
  await a.page.setViewportSize({ width: 1280, height: 800 });
  await shot(a.page, '02-home-card-1280');

  const folder = fs.mkdtempSync(path.join(a.env.projectsDir, '..', 'stick-'));
  await a.page.getByTestId(`usb-menu-${BIBLE}`).click();
  await a.page.getByRole('menuitem', { name: en['home.saveForTeammate'], exact: true }).click();
  await shot(a.page, '03-save-choose-folder');
  await a.pickFolder(folder);
  await a.page.getByTestId('save-for-teammate-choose').click();
  await a.page.getByTestId('save-for-teammate-save').waitFor();
  await shot(a.page, '04-save-folder-chosen');
  await a.page.getByTestId('save-for-teammate-save').click();
  await a.page.getByTestId('save-for-teammate-done').waitFor();
  await shot(a.page, '05-save-done');
  await a.page.getByTestId('save-for-teammate-leave').click();
  const file = path.join(folder, fs.readdirSync(folder).find((f) => f.endsWith('.zip')));

  await b.page.getByTestId('add-project').click();
  await shot(b.page, '06-add-project-cards');
  await b.page.getByTestId('add-project-receive').click();
  await shot(b.page, '07-receive-new-choose-file');
  await b.pickFile(file);
  await b.page.getByTestId('receive-work-choose').click();
  await b.page.getByTestId('receive-work-name').waitFor();
  await shot(b.page, '08-receive-new-confirm');
  await b.page.getByTestId('receive-work-receive').click();
  await b.page.getByTestId('receive-work-added').waitFor();
  await shot(b.page, '09-receive-new-added');
  await b.page.getByTestId('receive-work-leave').click();
  await shot(b.page, '10-home-after-receive-new');

  // B writes Titus 1:1 and saves a file back, so A's receive has something to take in.
  const onB = await b.page.getByTestId('receive-work-name').inputValue().catch(() => null)
    ?? fs.readdirSync(b.env.projectsDir).find((f) => !f.startsWith('.'));
  await b.page.getByTestId(`project-${onB}`).getByRole('button', { name: /Titus/ }).click();
  await b.page.getByTestId('mode-switch').waitFor();
  const atB = (await b.env.call('app/hello', {})).sessions[0].at;
  const chapter = await b.env.call('editor/chapters', { at: atB, book: 'TIT' });
  const row = chapter.chapters[0].verses.find((v) => v.key === '1');
  await b.env.call('editor/saveVerse', { at: atB, request: newRequestId(), shown: row.words.token, words: 'Pablo, siervo de Dios, escrito en el otro computador' });
  await b.page.getByTestId('project-switch').click();
  await b.page.getByTestId('account-menu').waitFor();
  const folderB = fs.mkdtempSync(path.join(b.env.projectsDir, '..', 'stick-'));
  await b.page.getByTestId(`usb-menu-${onB}`).click();
  await b.page.getByRole('menuitem', { name: en['home.saveForTeammate'], exact: true }).click();
  await b.pickFolder(folderB);
  await b.page.getByTestId('save-for-teammate-choose').click();
  await b.page.getByTestId('save-for-teammate-save').click();
  await b.page.getByTestId('save-for-teammate-done').waitFor();
  await b.page.getByTestId('save-for-teammate-leave').click();
  const fromB = path.join(folderB, fs.readdirSync(folderB).find((f) => f.endsWith('.zip')));

  await a.page.getByTestId(`usb-menu-${BIBLE}`).click();
  await a.page.getByRole('menuitem', { name: en['home.receiveWork'], exact: true }).click();
  await shot(a.page, '11-receive-choose-file');
  await a.pickFile(fromB);
  await a.page.getByTestId('receive-work-choose').click();
  await a.page.getByTestId('receive-work-confirm').waitFor();
  await shot(a.page, '12-receive-confirm');
  await a.page.getByTestId('receive-work-receive').click();
  await a.page.getByTestId('receive-work-received').waitFor();
  await shot(a.page, '13-receive-received');
  await a.page.getByTestId('receive-work-leave').click();
});
