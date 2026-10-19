// #492: the Language name field of New Bible and New Open Bible Stories suggests languages from
// the Door43 list the app ships, ranked on the server (AD27); a choice fills the name, the Code
// box and the direction, which stay editable. The UI half of tC4's
// e2e/language-suggestions.spec.ts (the create and what it stores are C6's), and the rebuild's
// step for a Door43 code a project cannot hold (owner's answer to Q35).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/language-suggestions.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../env-rust.mjs';
import { browserTest, recordRequests, until, withApp } from './kit.mjs';

const LIST = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-app', 'assets', 'langnames.json'), 'utf8'));
const language = (code) => LIST.find((l) => l.lc === code) ?? assert.fail(`${code} is not in the shipped list`);
/** A suggestion row as the person reads it: the anglicized name, the own name when it differs, the code. */
const rowText = (code) => {
  const { ang, ln, lc } = language(code);
  const name = ang || ln;
  return `${name}${ln !== name ? ln : ''}${lc}`;
};

const nameField = (page) => page.getByLabel('Language name');
const codeField = (page) => page.getByLabel('Code', { exact: true });
const direction = async (page) => ((await page.getByRole('button', { name: 'Right to left' }).getAttribute('data-selected')) === 'true' ? 'rtl' : 'ltr');

async function suggest(page, query, expected) {
  await nameField(page).fill(query);
  const options = page.getByRole('option');
  for (const [i, code] of expected.entries()) {
    await until(`option ${i} is ${code}`, async () => (await options.nth(i).textContent().catch(() => null)) === rowText(code));
  }
}

async function openNew(page, which) {
  await page.getByTestId('add-project').click();
  await page.getByTestId(`add-project-${which}`).click();
  await nameField(page).waitFor();
}

browserTest('New Bible: suggestions from the shipped list, by keyboard and pointer, with nothing sent off this machine', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await openNew(page, 'bible');

    // Text that matches no language shows no list.
    await nameField(page).fill('zzzzqqq');
    await page.waitForTimeout(500);
    assert.equal(await page.getByRole('listbox').count(), 0);

    // "Hausa" offers Hausa (ha) first, with its own name and its code.
    await suggest(page, 'Hausa', ['ha', 'hsl']);

    // Escape closes the list and changes neither box; the dialog stays open.
    await page.keyboard.press('Escape');
    await until('the list closes', async () => (await page.getByRole('listbox').count()) === 0);
    assert.equal(await nameField(page).inputValue(), 'Hausa');
    assert.equal(await codeField(page).inputValue(), '');

    // The arrows move the highlight; Enter chooses Hausa: name, code and direction.
    await nameField(page).press('ArrowDown');
    await until('the first row is highlighted', async () => (await nameField(page).getAttribute('aria-activedescendant')) === 'nb-lang-opt-0');
    await nameField(page).press('ArrowDown');
    assert.equal(await nameField(page).getAttribute('aria-activedescendant'), 'nb-lang-opt-1');
    await nameField(page).press('ArrowUp');
    assert.equal(await nameField(page).getAttribute('aria-activedescendant'), 'nb-lang-opt-0');
    await nameField(page).press('Enter');
    await until('the list closes', async () => (await page.getByRole('listbox').count()) === 0);
    assert.equal(await nameField(page).inputValue(), 'Hausa');
    assert.equal(await codeField(page).inputValue(), 'ha');
    assert.equal(await direction(page), language('ha').ld);

    // "es-419" offers Latin American Spanish first; choosing nothing keeps the code there.
    await suggest(page, 'es-419', ['es-419']);
    await page.keyboard.press('Escape');
    assert.equal(await nameField(page).inputValue(), 'es-419');
    assert.equal(await codeField(page).inputValue(), 'ha');

    // A right-to-left language sets Right to left; a press on Left to right stays.
    await suggest(page, 'Arabic', ['ar']);
    await page.getByRole('option').first().click();
    assert.equal(await direction(page), 'rtl');
    await page.getByRole('button', { name: 'Left to right' }).click();
    assert.equal(await direction(page), 'ltr');

    // The list stays on the server and nothing leaves the machine.
    assert.deepEqual(env.refused(), []);
    assert.equal(requests.all.filter((r) => r.url.includes('langnames')).length, 0);
  });
});

browserTest('New Open Bible Stories: a press on a suggestion fills name and code; a name not in the list keeps the code', async () => {
  await withApp(async ({ page }) => {
    await openNew(page, 'obs');
    await suggest(page, 'Hausa', ['ha']);
    await page.getByRole('option').first().click();
    assert.equal(await nameField(page).inputValue(), 'Hausa');
    assert.equal(await codeField(page).inputValue(), 'ha');
    await nameField(page).fill('Hausa of my village');
    await page.keyboard.press('Escape');
    assert.equal(await codeField(page).inputValue(), 'ha');
  });
});

browserTest('a Door43 code a project cannot hold is offered in a form it can, and one sentence says so (Q35)', async () => {
  await withApp(async ({ page }) => {
    await openNew(page, 'bible');
    await suggest(page, 'Lebanese Arabic', ['acm-x-lebarabic']);
    await page.getByRole('option').first().click();
    assert.equal(await codeField(page).inputValue(), 'acm-x-lebarabi-c');
    assert.equal(await direction(page), 'rtl');
    const offer = await page.getByTestId('code-offer').textContent();
    assert.match(offer, /acm-x-lebarabic/);
    assert.match(offer, /acm-x-lebarabi-c/);
    // A code typed by the person is theirs: the sentence goes.
    await codeField(page).fill('apc');
    await until('the sentence goes', async () => (await page.getByTestId('code-offer').count()) === 0);
  });
});
