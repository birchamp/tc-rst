// The Feedback dialog (M14, tC4 #378, #521, #634): tC4's e2e/feedback.spec.ts from Report a
// problem, its cases c to i (a and b need a refused open, which B1b and B3 bring), and the
// rule that nothing of a report is stored (A5 D11). The shell's `feedbackSend` is a stub that
// answers from a list and logs every report to node (APP-DESIGN 5.7).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/feedback.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { REPO } from '../env-rust.mjs';
import { LOCALES, browserTest, catalog, openMenu, recordRequests, sleep, until, withApp } from './kit.mjs';

/** The rebuild's own English text (crates/tc-ui/assets/i18n-rebuild). */
const rebuildEn = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'en.json'), 'utf8'));

const dialog = (page) => page.getByTestId('feedback');
const message = (page) => page.getByTestId('feedback-message');
const send = (page) => page.getByTestId('feedback-send');
const result = (page) => page.getByTestId('feedback-result');
const attachment = (page) => page.getByTestId('feedback-attachment');

async function openReport(page) {
  await openMenu(page);
  await page.getByTestId('account-report').click();
  await dialog(page).waitFor();
  await until('the attachment', async () => /no Report\n?$/.test(await attachment(page).textContent()));
}

/** A shell that sends: `answers` in turn, `hold()` keeps the next answer until released. */
function bridge() {
  const calls = [];
  const answers = [];
  let gate = null;
  return {
    calls,
    answers,
    hold() {
      let release;
      gate = new Promise((resolve) => { release = resolve; });
      return () => { gate = null; release(); };
    },
    async install(context) {
      await context.exposeBinding('tc4FeedbackSent', async (_source, report) => {
        calls.push(report);
        if (gate) await gate;
        return answers.shift() ?? { ok: true };
      });
      await context.addInitScript(() => {
        window.tc4Shell = { feedbackSend: (report) => window.tc4FeedbackSent(report) };
      });
    },
  };
}

browserTest('c. opening the report and typing in it go to no other origin and ask nothing but the attachment', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await page.waitForFunction(() => window.__tcIdle === true);
    const mark = requests.mark();
    await openReport(page);
    await message(page).fill('It stopped.');
    await page.getByLabel('Name (optional)', { exact: true }).fill('Journey Tester');
    await page.getByLabel('Email (optional)', { exact: true }).fill('journey@example.invalid');
    await sleep(300);
    // Home behind the dialog reads a card again when its summary is ready, which can land at any
    // moment (the OBS project's fifty stories): that read is Home's, not the report's.
    assert.deepEqual(requests.api(mark).filter((p) => p !== '/api/v1/home/card'), ['/api/v1/feedback/compose']);
    assert.deepEqual(env.refused(), []);
    const text = await attachment(page).textContent();
    assert.match(text.split('\n')[0], /^translationCore \S+ \(\S+\) · \S+ \S+$/, 'the version line names the program and the platform');
  });
});

browserTest('g. with no desktop bridge, Send says so, asks nothing and keeps the message; h. Report a problem keeps typed text with a new attachment', async () => {
  await withApp(async ({ page }) => {
    const requests = recordRequests(page);
    await openReport(page);
    await message(page).fill('Kept text.');
    await send(page).click();
    await until('not sent', async () => (await result(page).getAttribute('data-result').catch(() => null)) === 'no-desktop');
    assert.match(await result(page).textContent(), /Sending works only in the desktop app\./);
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    assert.equal(await message(page).inputValue(), 'Kept text.');
    await page.getByTestId('feedback-cancel').click();
    await until('closed', async () => (await dialog(page).count()) === 0);
    // h: the kept report reopens with its text and the attachment of now.
    const mark = requests.mark();
    await openReport(page);
    assert.equal(await message(page).inputValue(), 'Kept text.');
    assert.deepEqual(requests.api(mark), ['/api/v1/feedback/compose'], 'a new attachment');
    await page.getByTestId('feedback-cancel').click();
  });
});

browserTest('i. (#634) in every language the Message, Name and Email boxes are named by their labels; a click on the Message label puts the cursor in it', async () => {
  await withApp(async ({ page, env }) => {
    for (const { id } of LOCALES) {
      const c = catalog(id);
      await env.load(page, { locale: id });
      await page.getByTestId('account-menu').waitFor();
      await openReport(page);
      assert.equal(await dialog(page).getByLabel('No such label (#634)', { exact: true }).count(), 0);
      for (const key of ['feedback.message', 'feedback.name', 'feedback.email']) {
        const box = dialog(page).getByLabel(c[key], { exact: true });
        assert.equal(await box.count(), 1, `${id}: ${key}`);
        assert.ok(['TEXTAREA', 'INPUT'].includes(await box.evaluate((el) => el.tagName)), `${id}: ${key} is a text box`);
      }
      const box = dialog(page).getByLabel(c['feedback.message'], { exact: true });
      assert.equal(await box.getAttribute('data-testid'), 'feedback-message');
      await dialog(page).getByLabel(c['feedback.name'], { exact: true }).focus();
      await dialog(page).locator('label').filter({ hasText: c['feedback.message'] }).click();
      await until(`${id}: the cursor in Message`, () => box.evaluate((el) => document.activeElement === el));
      await page.keyboard.type(`Typed (${id}).`);
      assert.equal(await box.inputValue(), `Typed (${id}).`);
      await page.getByTestId('feedback-cancel').click();
      await until('closed', async () => (await dialog(page).count()) === 0);
    }
  });
});

browserTest('feedback_says_in_the_app_language_that_the_attachment_is_english (#731)', async () => {
  const ar = JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n-rebuild', 'ar.json'), 'utf8'));
  await withApp(async ({ page }) => {
    await openReport(page);
    assert.match(await attachment(page).textContent(), /^translationCore .*\nno Report\n?$/, 'the attachment stays English');
    const line = dialog(page).getByTestId('feedback-attachment-english');
    assert.equal((await line.textContent()).trim(), ar['feedback.attachmentEnglish']);
    assert.notEqual(ar['feedback.attachmentEnglish'], rebuildEn['feedback.attachmentEnglish']);
  }, { locale: 'ar' });
});

browserTest('d, e, f. with the shell: Send asks first; Not now sends nothing; Turn on sends once; a held send locks the dialog; timeout keeps the text; a double click sends one; the shell gets exactly the shown attachment', async () => {
  const shell = bridge();
  shell.answers.push({ ok: false, reason: 'timeout' }, { ok: true });
  await withApp(async ({ page }) => {
    await openReport(page);
    await message(page).fill('The verse would not save.');
    await page.getByLabel('Name (optional)', { exact: true }).fill('Journey Tester');
    await page.getByLabel('Email (optional)', { exact: true }).fill('  journey@example.invalid ');
    const shown = await attachment(page).textContent();
    // d: Not now sends nothing and keeps the report.
    await send(page).click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'feedback');
    assert.match(await page.getByTestId('net-ask-reason').textContent(), /send your report to the unfoldingWord help desk/);
    await page.getByTestId('net-cancel').click();
    await until('the question closes', async () => (await ask.count()) === 0);
    assert.equal(await dialog(page).count(), 1);
    assert.equal(await message(page).inputValue(), 'The verse would not save.');
    assert.equal(shell.calls.length, 0);
    assert.equal(await send(page).isEnabled(), true, 'Send is on again');
    // d, e: Turn on sends; while the shell holds it, nothing can change; a timeout keeps the text.
    const release = shell.hold();
    await send(page).click();
    await page.getByTestId('net-confirm').click();
    await until('sent to the shell', async () => shell.calls.length === 1);
    assert.equal(await send(page).isDisabled(), true);
    assert.equal(await page.getByTestId('feedback-cancel').isDisabled(), true);
    assert.equal(await message(page).isDisabled(), true);
    assert.equal(await send(page).textContent(), catalog('en')['feedback.sending']);
    release();
    await until('timeout', async () => (await result(page).getAttribute('data-result').catch(() => null)) === 'timeout');
    assert.equal(await message(page).inputValue(), 'The verse would not save.');
    // e: a double click sends one report, with no second question (the internet is on now).
    await send(page).dblclick();
    await until('sent', async () => (await result(page).getAttribute('data-result').catch(() => null)) === 'sent');
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    assert.equal(shell.calls.length, 2);
    // f: exactly what the dialog showed.
    for (const report of shell.calls) {
      assert.deepEqual(report, {
        category: 'General Feedback',
        message: 'The verse would not save.',
        name: 'Journey Tester',
        email: 'journey@example.invalid',
        version: shown.split(' · ')[0],
        attachment: shown,
      });
    }
    await page.getByTestId('feedback-close').click();
    await until('closed', async () => (await dialog(page).count()) === 0);
    // A sent report was cleared: the next one is new.
    await openReport(page);
    assert.equal(await message(page).inputValue(), '');
    assert.equal(await result(page).count(), 0);
  }, { init: (context) => shell.install(context) });
});

// A rebuild rule (APP-DESIGN A5 D8): Send waits for the attachment the server composes, where
// tC4 composes it on the spot. A compose that fails says so in the dialog, and opening the report
// again keeps what was typed and composes again.
browserTest('a compose that fails keeps Send off and says so; the next opening keeps the text and composes again', async () => {
  await withApp(async ({ page, allow }) => {
    allow(/status of 500/);
    let failing = true;
    await page.route('**/api/v1/feedback/compose', (route) => (failing
      ? route.fulfill({ status: 500, contentType: 'text/plain', body: 'compose failed' })
      : route.fallback()));
    await openMenu(page);
    await page.getByTestId('account-report').click();
    await dialog(page).waitFor();
    const said = rebuildEn['feedback.attachmentFailed'].replace('{reason}', "the app's server answered 500");
    await until('the failure in the attachment box', async () => (await attachment(page).textContent()) === said);
    await message(page).fill('Typed before the failure was seen.');
    assert.equal(await send(page).isDisabled(), true, 'Send stays off without the attachment');
    await page.getByTestId('feedback-cancel').click();
    await until('closed', async () => (await dialog(page).count()) === 0);
    failing = false;
    await openReport(page);
    assert.equal(await message(page).inputValue(), 'Typed before the failure was seen.');
    assert.equal(await send(page).isEnabled(), true, 'Send is on with the attachment shown');
    await page.getByTestId('feedback-cancel').click();
  });
});

browserTest('feedback_report_is_never_written_to_disk: no storage item and no request but the shell carries what was typed', async () => {
  const shell = bridge();
  await withApp(async ({ page }) => {
    const requests = recordRequests(page);
    const bodies = [];
    page.on('request', (r) => { if (r.postData()) bodies.push(r.postData()); });
    await openReport(page);
    const typed = ['A private message 378', 'Private Name 378', 'private378@example.invalid'];
    await message(page).fill(typed[0]);
    await page.getByLabel('Name (optional)', { exact: true }).fill(typed[1]);
    await page.getByLabel('Email (optional)', { exact: true }).fill(typed[2]);
    await page.getByTestId('feedback-cancel').click();
    await sleep(1200);
    const stored = await page.evaluate(() => {
      const all = [];
      for (const s of [localStorage, sessionStorage]) for (let i = 0; i < s.length; i++) all.push(`${s.key(i)}=${s.getItem(s.key(i))}`);
      return all;
    });
    for (const text of typed) {
      assert.ok(!stored.some((item) => item.includes(text)), `${text} is in no storage item`);
      assert.ok(!bodies.some((body) => body.includes(text)), `${text} is in no request`);
    }
    assert.ok(requests.all.every((r) => !r.url.includes('378')));
  }, { init: (context) => shell.install(context) });
});
