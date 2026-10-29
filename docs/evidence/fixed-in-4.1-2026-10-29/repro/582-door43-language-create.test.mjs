// #582 repro on the real app, the issue's own steps: Add a project › New Bible (and New Open
// Bible Stories), pick a language from the Door43 suggestions, Create. tC4 4.0 behind Pankosmia
// refused `ug` (Uyghur), `bal`, `kas`, `ajp` (not in the server's BCP47 table) and
// `acm-x-lebarabic`, `alk-alakatapue`, `bnx-tz-bangubangu` (fail the server's tag pattern).
// Each case reads metadata.json and checking/settings.json of the folder Create made.
//
//   cd <worktree> && t=$PWD/target && TC4_BIN=$t/debug/tc4 TC4_UI=$t/tc-ui-app \
//     tc-heavy node --test --test-timeout=180000 \
//     docs/evidence/fixed-in-4.1-2026-10-29/repro/582-door43-language-create.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../../../../parity/app/env-rust.mjs';
import { browserTest, catalog, until, withApp } from '../../../../parity/app/journeys/kit.mjs';

const en = catalog('en');
const LIST = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-app', 'assets', 'langnames.json'), 'utf8'));
const row = (code) => LIST.find((l) => l.lc === code) ?? assert.fail(`${code} is not in the shipped list`);
const rowText = (code) => {
  const { ang, ln, lc } = row(code);
  const name = ang || ln;
  return `${name}${ln !== name ? ln : ''}${lc}`;
};
const listed = (env) => fs.readdirSync(env.projectsDir).filter((n) => !n.startsWith('.')).sort();
const json = (env, ...parts) => JSON.parse(fs.readFileSync(path.join(env.projectsDir, ...parts), 'utf8'));

async function createWith(page, env, { obs, name, query, code }) {
  const before = listed(env);
  await page.getByTestId('add-project').click();
  await page.getByTestId(`add-project-${obs ? 'obs' : 'bible'}`).click();
  await page.getByLabel(obs ? 'Project name' : 'Bible name').fill(name);
  await page.getByLabel('Language name').fill(query);
  const options = page.getByRole('option');
  let index = -1;
  await until(`a suggestion for ${code}`, async () => {
    const texts = await options.allTextContents();
    index = texts.indexOf(rowText(code));
    return index >= 0;
  });
  await options.nth(index).click();
  const shown = await page.getByLabel('Code', { exact: true }).inputValue();
  const offer = (await page.getByTestId('code-offer').count()) ? await page.getByTestId('code-offer').textContent() : null;
  await page.getByRole('button', { name: en[obs ? 'newObs.create' : 'newBible.create'] }).click();
  // A Bible holds its metadata once it has a book: add blank Titus, as J1 does.
  if (!obs) {
    const blank = page.getByRole('button', { name: en['addBook.blankTitle'] });
    await until(`Add a book or a refusal for ${code}`, async () => (await blank.count()) > 0 || (await page.getByRole('alert').count()) > 0);
    if (await blank.count()) {
      await blank.click();
      await page.getByRole('combobox', { name: en['addBook.book'], exact: true }).click();
      await page.getByRole('option', { name: 'Titus' }).click();
      await page.getByRole('button', { name: en['addBook.create'] }).click();
    }
  }
  let created = [];
  await until(`a folder for ${code}`, async () => {
    created = listed(env).filter((n) => !before.includes(n));
    if (created.length === 1) return fs.existsSync(path.join(env.projectsDir, created[0], 'metadata.json'));
    return (await page.getByRole('alert').count()) > 0;
  }).catch(async (e) => {
    await page.screenshot({ path: path.join(import.meta.dirname, '..', 'shots', `582-hang-${code}.png`) });
    console.log(`#582 hang ${code}: created=${JSON.stringify(created)} files=${created[0] ? JSON.stringify(fs.readdirSync(path.join(env.projectsDir, created[0]), { recursive: true }).slice(0, 40)) : ''}`);
    throw e;
  });
  await until(`settings for ${code}`, async () => {
    created = listed(env).filter((n) => !before.includes(n));
    if (created.length === 1) return fs.existsSync(path.join(env.projectsDir, created[0], 'ingredients', 'checking', 'settings.json')) && fs.existsSync(path.join(env.projectsDir, created[0], 'metadata.json'));
    return (await page.getByRole('alert').count()) > 0;
  }).catch(async (e) => {
    console.log(`#582 no settings ${code}: files=${JSON.stringify(fs.readdirSync(path.join(env.projectsDir, created[0]), { recursive: true }))}`);
    throw e;
  });
  const alert = (await page.getByRole('alert').count()) ? await page.getByRole('alert').textContent() : null;
  const out = { door43: code, codeBox: shown, offer, alert, folder: created[0] ?? null };
  if (created.length === 1) {
    const meta = json(env, created[0], 'metadata.json');
    out.tag = meta.languages[0].tag;
    out.name = meta.languages[0].name;
    out.dir = meta.languages[0].scriptDirection;
    const settings = json(env, created[0], 'ingredients', 'checking', 'settings.json');
    out.door43Language = settings.door43Language ?? null;
  }
  console.log(`#582 ${JSON.stringify(out)}`);
  // Back to Home for the next case.
  await page.goto(page.url().split('#')[0]);
  await page.getByTestId('add-project').waitFor();
  return out;
}

browserTest('582_every_group_of_refused_door43_codes_creates_a_project', async () => {
  await withApp(async ({ page, env }) => {
    const cases = [
      { name: 'Uyghur Bible', query: 'Uyghur', code: 'ug', want: 'ug' },
      { name: 'Balochi Bible', query: 'Balochi', code: 'bal', want: 'bal' },
      { name: 'Kashmiri Bible', query: 'Kashmiri', code: 'kas', want: 'kas' },
      { name: 'Levantine Bible', query: 'South Levantine', code: 'ajp', want: 'ajp' },
      { name: 'Lebanese Bible', query: 'Lebanese Arabic', code: 'acm-x-lebarabic', want: 'acm-x-lebarabi-c' },
      { name: 'Alak Bible', query: 'Alak Atapue', code: 'alk-alakatapue', want: 'alk-x-alakatap-ue' },
      { name: 'Bangubangu Bible', query: 'Bangubangu', code: 'bnx-tz-bangubangu', want: null },
      { obs: true, name: 'Uyghur Stories', query: 'Uyghur', code: 'ug', want: 'ug' },
    ];
    for (const c of cases) {
      const out = await createWith(page, env, c);
      assert.equal(out.alert, null, `${c.code}: no refusal`);
      assert.ok(out.folder, `${c.code}: a project folder`);
      if (c.want) assert.equal(out.tag, c.want, `${c.code}: stored tag`);
      assert.notEqual(out.tag, 'und', `${c.code}: never Undetermined`);
      if (out.tag !== c.code) {
        assert.ok(out.offer?.includes(c.code) && out.offer.includes(out.tag), `${c.code}: one sentence names both codes`);
        assert.equal(out.door43Language?.code, c.code, `${c.code}: the Door43 code is kept`);
      }
    }
  });
}, { timeout: 170_000 });
