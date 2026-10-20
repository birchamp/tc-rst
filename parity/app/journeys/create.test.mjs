// Create a project, add books and save settings (C6), through the app on the store: tC4's J1
// (a Bible with one book), J20 (Open Bible Stories), `language-tags` (a code a project cannot
// hold is refused with its reason and leaves nothing; regional and private-use codes are kept),
// and `settings-contract` (a new project's settings are the presentation fields only; a Save
// from a Home card writes the direction and the font). The server's side of each is tested in
// tc-app (crates/tc-app/tests/app/create.rs); this file is what a person sees and does.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/create.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../env-rust.mjs';
import { browserTest, catalog, until, withApp } from './kit.mjs';

const en = catalog('en');
const rebuilt = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8'));
const read = (env, ...parts) => fs.readFileSync(path.join(env.projectsDir, ...parts), 'utf8');
const json = (env, ...parts) => JSON.parse(read(env, ...parts));
const listed = (env) => fs.readdirSync(env.projectsDir).filter((name) => !name.startsWith('.')).sort();
const fixture = (...parts) => path.join(REPO, 'fixtures', 'tc4', 'test-fixtures', ...parts);

const nameField = (page, obs = false) => page.getByLabel(obs ? 'Project name' : 'Bible name');
const codeField = (page) => page.getByLabel('Code', { exact: true });
const languageField = (page) => page.getByLabel('Language name');

async function openNew(page, which) {
  await page.getByTestId('add-project').click();
  await page.getByTestId(`add-project-${which}`).click();
  await languageField(page).waitFor();
}

async function fill(page, { obs = false, name, language, code, dir = 'Left to right' }) {
  await nameField(page, obs).fill(name);
  await languageField(page).fill(language);
  await codeField(page).click(); // leaving the field closes its suggestions
  await codeField(page).fill(code);
  await page.getByRole('button', { name: dir }).click();
}

const createBible = (page) => page.getByRole('button', { name: en['newBible.create'] });
const createStories = (page) => page.getByRole('button', { name: en['newObs.create'] });

browserTest('J1: a Bible is created with its language and direction, then a blank book makes it a valid project', async () => {
  await withApp(async ({ page, env }) => {
    const before = listed(env);
    await openNew(page, 'bible');
    await fill(page, { name: 'Equipo Rig — Tito', language: 'Español', code: 'es_419!' });
    // A language code a project cannot hold is refused in words, and leaves nothing behind.
    await createBible(page).click();
    const alert = page.getByRole('alert');
    await alert.waitFor();
    assert.match(await alert.textContent(), /language code/i);
    assert.deepEqual(listed(env), before, 'the refused create made no folder');
    // The corrected code creates it, and Add a book follows.
    await codeField(page).fill('es-419');
    await createBible(page).click();
    await page.getByRole('button', { name: en['addBook.blankTitle'] }).click();
    await page.getByRole('combobox', { name: en['addBook.book'], exact: true }).click();
    await page.getByRole('option', { name: 'Titus' }).click();
    await page.getByRole('button', { name: en['addBook.create'] }).click();
    await page.getByTestId('understand').waitFor();
    await page.getByRole('heading', { name: 'Titus 1', exact: true }).waitFor();

    const created = listed(env).filter((name) => !before.includes(name));
    assert.deepEqual(created, ['equipo_rig_tito'], 'exactly one new project folder');
    const [folder] = created;
    assert.ok(fs.existsSync(path.join(env.projectsDir, folder, 'ingredients', 'tc', 'project.json')));
    const meta = json(env, folder, 'metadata.json');
    assert.equal(meta.languages[0].tag, 'es-419');
    assert.equal(meta.languages[0].name.en, 'Español');
    assert.equal(meta.identification.name.en, 'Equipo Rig — Tito');
    assert.ok(meta.ingredients['ingredients/TIT.usfm'], 'metadata lists the book');
    // The book is the stub book: three chapters, a stub for every verse, the paragraphs of the
    // source, no \ts and no alignment markup.
    const titus = read(env, folder, 'ingredients', 'TIT.usfm');
    assert.equal((titus.match(/^\\c /gm) ?? []).length, 3);
    assert.match(titus, /^\\v 1 ___$/m);
    assert.match(titus, /^\\p$/m);
    assert.doesNotMatch(titus, /\\ts|\\zaln/);
    assert.ok(json(env, folder, 'ingredients', 'vrs.json').maxVerses.TIT, 'the versification is written');
    const pins = json(env, folder, 'ingredients', 'checking', 'resources.json');
    assert.equal(pins.schemaVersion, 2);
    assert.deepEqual(pins.languageSets.primary, pins.languageSets.fallback, 'no gateway chosen yet');
    assert.equal(pins.extraScripture.find((e) => e.id === 'ult').version, 'v91');
    // The settings contract: the presentation fields, and no checking language.
    const settings = json(env, folder, 'ingredients', 'checking', 'settings.json');
    assert.deepEqual(Object.keys(settings).sort(), ['languageName', 'schemaVersion', 'textDirection', 'textFont']);
    assert.equal(settings.textDirection, 'ltr');
    assert.equal(settings.languageName, 'Español');
  });
});

browserTest('J20: Open Bible Stories are created with fifty stories, and Home shows the project at 0%', async () => {
  await withApp(async ({ page, env }) => {
    const before = listed(env);
    await openNew(page, 'obs');
    await fill(page, { obs: true, name: 'Equipo Rig — Historias', language: 'Español', code: 'es' });
    await createStories(page).click();
    const [folder] = await until('the project folder', () => {
      const made = listed(env).filter((name) => !before.includes(name));
      return made.length ? made : null;
    });
    assert.equal(folder, 'equipo_rig_historias');
    const card = page.getByTestId(`project-${folder}`);
    await card.waitFor();
    assert.equal(await card.getByTestId('obs-marker').textContent(), 'OBS');
    await until('0% drafted', async () => (await card.getByTestId('obs-progress').textContent()).includes('0%'));
    assert.equal(await card.getByRole('button', { name: en['home.addBookTile'] }).count(), 0, 'stories have no book to add');
    const stories = fs.readdirSync(path.join(env.projectsDir, folder, 'ingredients', 'content')).filter((n) => /^\d\d\.md$/.test(n));
    assert.equal(stories.length, 50);
    const meta = json(env, folder, 'metadata.json');
    assert.equal(meta.type.flavorType.name, 'gloss');
    assert.deepEqual(meta.type.flavorType.flavor, { name: 'textStories' });
    assert.ok(meta.localizedNames);
    assert.equal(meta.identification.name.en, 'Equipo Rig — Historias');
    const settings = json(env, folder, 'ingredients', 'checking', 'settings.json');
    assert.deepEqual(Object.keys(settings).sort(), ['languageName', 'schemaVersion', 'textDirection', 'textFont']);
    assert.equal(settings.textDirection, 'ltr');
  });
});

browserTest('language-tags: a regional or private-use code is kept whole with its name, and an invalid one is refused with nothing left', async () => {
  await withApp(async ({ page, env }) => {
    const created = async (name, language, code) => {
      const before = listed(env);
      await page.goto('about:blank');
      await env.load(page, { locale: 'en' });
      await openNew(page, 'obs');
      await fill(page, { obs: true, name, language, code });
      await createStories(page).click();
      const [folder] = await until('the project folder', () => {
        const made = listed(env).filter((n) => !before.includes(n));
        return made.length ? made : null;
      });
      await page.getByTestId(`project-${folder}`).waitFor();
      return json(env, folder, 'metadata.json').languages[0];
    };
    // An invalid code: the reason shows, and no folder or staging folder is left.
    const before = listed(env);
    await openNew(page, 'obs');
    await fill(page, { obs: true, name: 'Historias', language: 'Español', code: 'es_419!' });
    await createStories(page).click();
    await page.getByRole('alert').waitFor();
    await page.waitForTimeout(500);
    assert.deepEqual(listed(env), before);
    assert.equal(fs.readdirSync(env.projectsDir).filter((n) => n.startsWith('.staging')).length, 0);
    await page.keyboard.press('Escape');
    // A code a Burrito cannot hold is offered adjusted, with the Door43 code kept beside it (Q35).
    const regional = await created('Historias es', 'Español', 'es-419');
    assert.equal(regional.tag, 'es-419');
    assert.equal(regional.name.en, 'Español');
    const private_ = await created('Historias abc', 'Abc Language', 'x-abc');
    assert.equal(private_.tag, 'x-abc');
    assert.equal(private_.name.en, 'Abc Language');
  });
});

browserTest('Add a book from USFM files: each file shows what it is, a held book is refused in its row, and the rest are added whole', async () => {
  await withApp(async ({ page, env }) => {
    const project = 'sample_burrito';
    const card = page.getByTestId(`project-${project}`);
    await card.getByRole('button', { name: en['home.addBookTile'] }).click();
    await page.getByTestId('ab-usfm-option').click();
    // Titus is in the project; Genesis 1 is not.
    await page.getByTestId('ab-usfm-input').setInputFiles([
      fixture('en_ult', 'TIT.usfm'),
      fixture('en_ult', 'GEN-1.usfm'),
    ]);
    const rows = page.getByTestId('ab-usfm-file');
    await until('both files are read', async () => (await rows.count()) === 2 && !(await page.getByText(rebuilt['addBook.usfmReading']).count()));
    assert.equal(await rows.nth(0).getAttribute('data-status'), 'refused');
    assert.equal(await rows.nth(0).textContent(), `TIT.usfm${en['addBook.usfmExists'].replace('{name}', 'Titus')}✕`);
    assert.equal(await rows.nth(1).getAttribute('data-status'), 'valid');
    // The label stays "Add book" with one valid file; a removed file leaves the list.
    const add = page.getByTestId('ab-usfm-add');
    assert.equal(await add.textContent(), en['addBook.addOne']);
    await rows.nth(0).getByRole('button', { name: en['importer.files.remove'] }).click();
    assert.equal(await rows.count(), 1);
    await add.click();
    await page.getByTestId('ab-usfm').waitFor({ state: 'detached' });
    await card.getByTestId('book-tile-GEN').waitFor();
    // The book is in whole, with its alignments split out of the verses.
    const gen = read(env, project, 'ingredients', 'GEN.usfm');
    assert.match(gen, /^\\c 1$/m);
    assert.doesNotMatch(gen, /\\zaln/);
    assert.ok(fs.existsSync(path.join(env.projectsDir, project, 'ingredients', 'checking', 'alignments', 'GEN.json')));
  });
});

browserTest('settings-contract: Save from a Home card writes the direction and the font, and the dialog reads them back', async () => {
  await withApp(async ({ page, env }) => {
    const project = 'sample_burrito';
    const settings = () => page.getByTestId(`project-${project}`).getByRole('button', { name: en['home.settings'], exact: true });
    await settings().click();
    const save = page.getByRole('button', { name: en['settings.save'] });
    await until('Save is enabled once the values came', () => save.isEnabled());
    await page.getByRole('button', { name: en['wizard.rtl'] }).click();
    await page.getByRole('combobox', { name: en['newBible.font'] }).click();
    await page.getByRole('option', { name: 'Charis SIL' }).click();
    await save.click();
    await page.getByRole('button', { name: en['settings.save'] }).waitFor({ state: 'detached' });
    const written = json(env, project, 'ingredients', 'checking', 'settings.json');
    assert.equal(written.textDirection, 'rtl');
    assert.equal(written.textFont, 'Charis SIL');
    assert.ok(!('checkingLanguage' in written));
    await settings().click();
    await until('Save is enabled once the values came', () => save.isEnabled());
    assert.equal(await page.getByRole('button', { name: en['wizard.rtl'] }).getAttribute('data-selected'), 'true');
  });
});
