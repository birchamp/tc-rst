// tC4 #836 repro (report only, outside the gate's glob). Fixtures hold no Arabic helps (no BSOJ
// package, and en-pins installs no questions), so the server's real Titus reply is kept and only
// its set direction is turned to rtl, as an Arabic set's reply carries it; the questions slot gets
// one English and one Arabic question. Records the computed direction of every text part.
import assert from 'node:assert/strict';
import path from 'node:path';
import { browserTest, withApp } from '../../../../parity/app/journeys/kit.mjs';

const SHOTS = path.resolve(import.meta.dirname, '..', 'shots');
const dirOf = (loc) => loc.evaluate((el) => getComputedStyle(el).direction);

browserTest('836_english_cards_ltr_arabic_card_rtl_under_an_rtl_set', async () => {
  await withApp(async ({ page }) => {
    await page.route('**/api/v1/helps/passage', async (route) => {
      const reply = await (await route.fetch()).json();
      for (const slot of ['notes', 'words']) if (reply.value[slot].type === 'ready') reply.value[slot].value.dir = 'rtl';
      reply.value.questions = {
        type: 'ready',
        value: {
          items: [
            { id: 'q1', chapter: '1', verse: '1', question: 'Who wrote this letter?', response: 'Paul.' },
            { id: 'q2', chapter: '1', verse: '1', question: 'من كتب هذه الرسالة؟', response: 'بولس.' },
          ],
          dir: 'rtl',
          fallback: false,
        },
      };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) });
    });
    await page.getByTestId('project-sample_burrito').getByRole('button', { name: /Titus/ }).click();
    await page.getByTestId('understand-unit-s1').waitFor();
    const out = {};
    const note = page.getByTestId('helps-note').first();
    await note.waitFor();
    out.noteCard = await dirOf(note);
    out.noteTexts = await note.evaluate((el) => [...el.querySelectorAll('h1,h2,h3,h4,p,span,div')]
      .filter((e) => e.childElementCount === 0 && e.textContent.trim().length > 3)
      .slice(0, 4).map((e) => [e.textContent.trim().slice(0, 30), getComputedStyle(e).direction]));
    await page.getByRole('tab', { name: 'Words', exact: true }).click();
    const word = page.getByTestId('helps-word').first();
    await word.waitFor();
    out.wordCard = await dirOf(word);
    out.wordTexts = await word.evaluate((el) => [...el.querySelectorAll('*')]
      .filter((e) => e.childElementCount === 0 && e.textContent.trim().length > 1)
      .slice(0, 3).map((e) => [e.textContent.trim().slice(0, 30), getComputedStyle(e).direction]));
    await page.getByRole('tab', { name: 'Questions', exact: true }).click();
    const qs = page.getByTestId('understand-question');
    await qs.nth(1).waitFor();
    out.questions = [];
    for (const i of [0, 1]) {
      const q = qs.nth(i);
      out.questions.push({ card: await dirOf(q), question: await dirOf(q.locator('p').first()), answer: await dirOf(q.locator('p').nth(1).locator('span[dir="auto"]')) });
    }
    console.log(JSON.stringify(out, null, 1));
    await page.screenshot({ path: path.join(SHOTS, '836-mixed-direction.png') });
    assert.equal(out.questions[0].question, 'ltr', 'the English question reads left to right');
    assert.equal(out.questions[1].question, 'rtl', 'the Arabic question stays right to left');
    assert.equal(out.questions[1].answer, 'rtl');
  }, { scenario: 'en-pins' });
});
