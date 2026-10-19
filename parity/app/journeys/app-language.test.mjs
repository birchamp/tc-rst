// App language (M16, tC4 #522), the cases of tC4's e2e/app-language.spec.ts that need no
// project and no Home of B2: a, b, b2, c, d, f, the dialog half of g, the no-project halves of
// h and j. The Home halves (the Home heading, the cards) are B2's; e, i, k and the in-project
// halves of h and j are B3's and B4a's (A5 D18). The stand-in Home's sample line stands where
// tC4's cases read the Home heading.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/app-language.test.mjs
import assert from 'node:assert/strict';
import {
  LABEL, LOCALES, browserTest, catalog, choose, expectLanguage, focused, openLanguage, openMenu,
  recordRequests, sleep, trigger, until, withApp,
} from './kit.mjs';

const dialog = (page) => page.getByTestId('language-dialog');
const apply = (page) => page.getByTestId('language-apply');
const cancel = (page) => page.getByTestId('language-cancel');
const select = (page) => page.getByTestId('language-select');
const stored = async (env) => (await env.call('app/hello', {})).prefs.appLocale;
const closed = (page) => until('the dialog closes', async () => (await dialog(page).count()) === 0);

browserTest('a. the row shows the applied language; each selection previews before Apply; Cancel restores English and saves nothing', async () => {
  await withApp(async ({ page, env }) => {
    await expectLanguage(page, 'en');
    await openMenu(page);
    const row = await page.getByTestId('account-language').textContent();
    assert.ok(row.includes(catalog('en')['account.language']) && row.includes('English'), row);
    await page.getByTestId('account-language').click();
    await dialog(page).waitFor();
    assert.match(await select(page).textContent(), /English/);
    await select(page).click();
    const options = await page.getByRole('listbox').getByRole('option').allTextContents();
    assert.deepEqual(options.map((o) => o.replace(/^✓/, '').trim()), LOCALES.map((l) => l.label), 'the installed languages, in order');
    assert.match(await page.getByRole('listbox').getByRole('option', { selected: true }).textContent(), /English/);
    await page.keyboard.press('Escape');
    assert.equal(await dialog(page).count(), 1, 'Escape closes the list, not the dialog');
    assert.equal(await apply(page).isDisabled(), true, 'Apply is off at the applied language');
    for (const { id } of LOCALES.filter((l) => l.id !== 'en')) {
      await choose(page, id);
      await expectLanguage(page, id);
      assert.equal(await apply(page).isEnabled(), true, `${id}: Apply on`);
      assert.equal(await stored(env), 'en', `${id}: a preview saves nothing`);
    }
    await choose(page, 'en');
    assert.equal(await apply(page).isDisabled(), true);
    await choose(page, 'hi');
    await cancel(page).click();
    await closed(page);
    await expectLanguage(page, 'en');
    assert.equal(await stored(env), 'en');
    await until('the focus returns to the button', async () => (await focused(page)) === 'account-menu');
    assert.deepEqual(env.refused(), []);
  });
});

async function applyLanguage(page, id) {
  await openLanguage(page);
  await choose(page, id);
  const written = page.waitForResponse((r) => r.url().endsWith('/api/v1/prefs/setLocale'));
  await apply(page).click();
  await written;
  await closed(page);
}

browserTest('b. Apply saves the choice and closes after the write; a reload starts in it', async () => {
  await withApp(async ({ page, env }) => {
    await applyLanguage(page, 'es-419');
    await expectLanguage(page, 'es-419');
    assert.equal(await stored(env), 'es-419');
    await openMenu(page);
    const row = await page.getByTestId('account-language').textContent();
    assert.ok(row.includes(catalog('es-419')['account.language']) && row.includes(LABEL['es-419']), row);
    await page.keyboard.press('Escape');
    await page.reload();
    await expectLanguage(page, 'es-419');
    await openLanguage(page);
    assert.ok((await select(page).textContent()).includes(LABEL['es-419']));
    assert.equal(await apply(page).isDisabled(), true);
    await cancel(page).click();
    assert.deepEqual(env.refused(), []);
  });
});

browserTest('b2. each language #622 added, and Arabic, applies and survives a reload', async () => {
  await withApp(async ({ page, env }) => {
    for (const id of ['pt-BR', 'id', 'uk', 'ru', 'vi', 'ar']) {
      await applyLanguage(page, id);
      assert.equal(await stored(env), id, `${id}: saved`);
      await page.reload();
      await expectLanguage(page, id);
    }
  });
});

browserTest('c. Cancel, Escape, the ✕ and the scrim each restore the applied Spanish; a reload during a preview starts in Spanish', async () => {
  await withApp(async ({ page, env }) => {
    await expectLanguage(page, 'es-419');
    const fr = catalog('fr');
    const ways = [
      ['fr', 'cancel', () => cancel(page).click()],
      ['hi', 'escape', () => page.keyboard.press('Escape')],
      // The ✕'s name follows the preview, as every label of the dialog does.
      ['fr', 'close-button', () => page.locator('[data-testid="language-dialog"] [role="dialog"]').getByRole('button', { name: fr['common.close'], exact: true }).click()],
      ['hi', 'scrim', () => page.mouse.click(5, 400)],
    ];
    for (const [id, way, go] of ways) {
      await openLanguage(page);
      await choose(page, id);
      await expectLanguage(page, id);
      await go();
      await closed(page);
      await expectLanguage(page, 'es-419');
      assert.equal(await stored(env), 'es-419', `${way}: nothing saved`);
      await until(`${way}: the focus returns`, async () => (await focused(page)) === 'account-menu');
    }
    await openLanguage(page);
    await choose(page, 'hi');
    await page.reload();
    await expectLanguage(page, 'es-419');
    assert.equal(await stored(env), 'es-419');
  }, { locale: 'es-419' });
});

/** The server's own refusal of a language it could not save (tc-api's golden refusal). */
const REFUSED = '{"type":"err","value":{"type":"refused","value":{"type":"prefs","value":"localeNotSaved"}}}';

browserTest('d. a failed save keeps the preview and the dialog with the error; retry succeeds; a held save locks every control', async () => {
  await withApp(async ({ page, env, allow }) => {
    allow(/status of 500/);
    let answer = 'refused';
    let held = null;
    await page.route('**/api/v1/prefs/setLocale', async (route) => {
      if (held) await new Promise((resolve) => { held = resolve; });
      if (answer === 'refused') return route.fulfill({ status: 200, contentType: 'application/json', body: REFUSED });
      if (answer === '500') return route.fulfill({ status: 500, contentType: 'text/plain', body: 'disk full' });
      return route.fallback();
    });
    for (const failure of ['refused', '500']) {
      answer = failure;
      await openLanguage(page);
      await choose(page, 'fr');
      await apply(page).click();
      const error = page.getByTestId('language-error');
      await error.waitFor();
      assert.equal(await error.textContent(), catalog('fr')['language.saveFailed'], `${failure}: the error in the preview's language`);
      await expectLanguage(page, 'fr');
      assert.equal(await stored(env), 'es-419');
      assert.equal(await apply(page).isEnabled(), true);
      assert.equal(await cancel(page).isEnabled(), true);
      await cancel(page).click();
      await closed(page);
      await expectLanguage(page, 'es-419');
    }
    // A failure, then a retry that the server takes.
    answer = 'refused';
    await openLanguage(page);
    await choose(page, 'fr');
    await apply(page).click();
    await page.getByTestId('language-error').waitFor();
    answer = 'through';
    await apply(page).click();
    await closed(page);
    await expectLanguage(page, 'fr');
    assert.equal(await stored(env), 'fr');
    // A held save: data-saving, the select, Apply and Cancel off, no ✕, Escape and the scrim do nothing.
    held = () => {};
    await openLanguage(page);
    await choose(page, 'es-419');
    await apply(page).click();
    const scrim = dialog(page);
    await until('saving', async () => (await scrim.getAttribute('data-saving')) === 'true');
    assert.equal(await select(page).isDisabled(), true);
    assert.equal(await apply(page).isDisabled(), true);
    assert.equal(await cancel(page).isDisabled(), true);
    await page.keyboard.press('Escape');
    await page.mouse.click(5, 400);
    await sleep(300);
    const closeName = catalog('es-419')['common.close'];
    assert.equal(await page.locator('[data-testid="language-dialog"] [role="dialog"]').getByRole('button', { name: closeName, exact: true }).count(), 0, 'no ✕ while saving');
    assert.equal(await dialog(page).count(), 1);
    assert.equal(await scrim.getAttribute('data-saving'), 'true');
    const release = held;
    held = null;
    release();
    await closed(page);
    await expectLanguage(page, 'es-419');
    assert.equal(await stored(env), 'es-419');
  }, { locale: 'es-419' });
});

browserTest('f. the menu, the previews, Cancel and Apply go to no other origin; the keyboard alone applies a language', async () => {
  await withApp(async ({ page, env }) => {
    await trigger(page).focus();
    await page.keyboard.press('Enter');
    await until('first row', async () => (await focused(page)) === 'account-sign-in');
    for (const want of ['account-internet', 'account-help', 'account-language']) {
      await page.keyboard.press('ArrowDown');
      await until(want, async () => (await focused(page)) === want);
    }
    await page.keyboard.press('Enter');
    await dialog(page).waitFor();
    await select(page).focus();
    await page.keyboard.press('ArrowDown');
    await page.getByRole('listbox').waitFor();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await expectLanguage(page, 'es-419');
    await until('the select keeps the focus', async () => (await focused(page)) === 'language-select');
    await page.keyboard.press('Escape');
    await closed(page);
    await expectLanguage(page, 'en');
    await until('back on the button after Escape', async () => (await focused(page)) === 'account-menu');
    await page.keyboard.press('Enter');
    await until('first row', async () => (await focused(page)) === 'account-sign-in');
    await page.keyboard.press('End');
    await until('About', async () => (await focused(page)) === 'account-about');
    await page.keyboard.press('ArrowUp');
    await until('Report a problem', async () => (await focused(page)) === 'account-report');
    await page.keyboard.press('ArrowUp');
    await until('App language', async () => (await focused(page)) === 'account-language');
    await page.keyboard.press('Enter');
    await dialog(page).waitFor();
    await select(page).focus();
    await page.keyboard.press('ArrowDown');
    await page.getByRole('listbox').waitFor();
    for (let i = 1; i < LOCALES.length; i++) await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    const last = LOCALES[LOCALES.length - 1].id;
    await expectLanguage(page, last);
    await page.keyboard.press('Tab');
    await until('Cancel', async () => (await focused(page)) === 'language-cancel');
    await page.keyboard.press('Tab');
    await until('Apply', async () => (await focused(page)) === 'language-apply');
    await page.keyboard.press('Enter');
    await closed(page);
    await until('saved', async () => (await stored(env)) === last);
    await until('back on the button after Apply', async () => (await focused(page)) === 'account-menu');
    await page.reload();
    await expectLanguage(page, last);
    assert.deepEqual(env.refused(), []);
  });
});

const letterSpacing = async (locator) => {
  const v = await locator.evaluate((el) => getComputedStyle(el).letterSpacing);
  const px = v === 'normal' ? 0 : parseFloat(v);
  assert.ok(!Number.isNaN(px), `letter-spacing ${JSON.stringify(v)} of ${await locator.evaluate((el) => el.outerHTML.slice(0, 200))}`);
  return px;
};

browserTest('g. (dialog half) in Hindi and Arabic no label of Report a problem is letter-spaced; English and the others keep the tracking', async () => {
  await withApp(async ({ page, env }) => {
    const FIELDS = ['feedback.message', 'feedback.name', 'feedback.email'];
    const measure = async (id) => {
      await openMenu(page);
      await page.getByTestId('account-report').click();
      const report = page.getByTestId('feedback');
      await report.waitFor();
      const out = {};
      for (const f of FIELDS) out[f] = await letterSpacing(report.locator('label').filter({ hasText: catalog(id)[f] }));
      const scriptOwn = await page.evaluate((code) => {
        const script = code === 'hi' ? /[ऀ-ॿ]/ : code === 'ar' ? /[؀-ۿ]/ : null;
        if (!script) return [];
        const dialog = document.querySelector('[data-testid="feedback"]');
        const spaced = [];
        for (const el of dialog.querySelectorAll('*')) {
          const own = Array.from(el.childNodes).filter((n) => n.nodeType === 3).map((n) => n.textContent).join('').trim();
          if (!script.test(own) || !el.getClientRects().length) continue;
          const ls = getComputedStyle(el).letterSpacing;
          if (ls !== 'normal' && ls !== '0px') spaced.push(`${own}: ${ls}`);
        }
        return spaced;
      }, id);
      await page.getByTestId('feedback-cancel').click();
      return { labels: out, spaced: scriptOwn };
    };
    const en = await measure('en');
    for (const f of FIELDS) assert.ok(en.labels[f] > 0, `en ${f} is tracked`);
    for (const id of ['fr', 'ru', 'hi', 'ar']) {
      await env.load(page, { locale: id });
      await page.getByTestId('account-menu').waitFor();
      const seen = await measure(id);
      for (const f of FIELDS) {
        if (id === 'hi' || id === 'ar') assert.equal(seen.labels[f], 0, `${id} ${f}`);
        else assert.equal(seen.labels[f], en.labels[f], `${id} ${f} keeps English's tracking`);
      }
      assert.deepEqual(seen.spaced, [], `${id}: nothing in the script is letter-spaced`);
    }
  });
});

const directions = (page) => page.evaluate(() => ({
  html: document.documentElement.dir,
  root: getComputedStyle(document.querySelector('#root > div')).direction,
}));
const half = async (page, locator) => {
  const box = await locator.boundingBox();
  return box.x + box.width / 2 < page.viewportSize().width / 2 ? 'left' : 'right';
};

browserTest('h. (no-project half) Arabic reads right to left at once, the dialog too; Cancel and English switch back with no reload', async () => {
  await withApp(async ({ page }) => {
    assert.deepEqual(await directions(page), { html: 'ltr', root: 'ltr' });
    assert.equal(await half(page, trigger(page)), 'right');
    await openLanguage(page);
    await choose(page, 'ar');
    await until('rtl', async () => (await directions(page)).root === 'rtl');
    assert.deepEqual(await directions(page), { html: 'rtl', root: 'rtl' });
    assert.equal(await half(page, trigger(page)), 'left', 'the account button moves to the left');
    const inDialog = await page.locator('[data-testid="language-dialog"] [role="dialog"]').evaluate((el) => {
      const cs = getComputedStyle(el);
      return { direction: cs.direction, textAlign: cs.textAlign };
    });
    assert.equal(inDialog.direction, 'rtl');
    assert.ok(['start', 'right'].includes(inDialog.textAlign), inDialog.textAlign);
    await cancel(page).click();
    await closed(page);
    assert.deepEqual(await directions(page), { html: 'ltr', root: 'ltr' });
    await applyLanguage(page, 'ar');
    assert.deepEqual(await directions(page), { html: 'rtl', root: 'rtl' });
    await applyLanguage(page, 'en');
    assert.deepEqual(await directions(page), { html: 'ltr', root: 'ltr' });
    assert.equal(await half(page, trigger(page)), 'right');
  });
});

const ARABIC = 'Noto Sans Arabic';
/** The faces Chromium painted the text of `locator` with (CSS.getPlatformFontsForNode). */
async function faces(page, locator) {
  const cdp = await page.context().newCDPSession(page);
  try {
    await locator.evaluate((el) => el.setAttribute('data-face-probe', ''));
    await cdp.send('DOM.enable');
    await cdp.send('CSS.enable');
    const { root } = await cdp.send('DOM.getDocument');
    const { nodeId } = await cdp.send('DOM.querySelector', { nodeId: root.nodeId, selector: '[data-face-probe]' });
    const { fonts } = await cdp.send('CSS.getPlatformFontsForNode', { nodeId });
    const family = (name) => [ARABIC, 'Mulish'].find((known) => name.startsWith(known)) ?? name;
    return fonts.map((f) => ({ family: family(f.familyName), bundled: f.isCustomFont, glyphs: f.glyphCount }));
  } finally {
    await locator.evaluate((el) => el.removeAttribute('data-face-probe'));
    await cdp.detach();
  }
}

browserTest('j. (no-project half) Arabic interface text is painted in the bundled Noto Sans Arabic at each weight, Latin in Mulish, with no other origin', async () => {
  await withApp(async ({ page, env }) => {
    const fonts = recordRequests(page);
    const latin = await faces(page, page.getByTestId('runtime-sample'));
    assert.deepEqual(latin.map((f) => [f.family, f.bundled]), [['Mulish', true]]);
    await applyLanguage(page, 'ar');
    await page.evaluate(() => document.fonts.ready);
    const byWeight = {};
    const probe = async () => {
      const weights = await page.evaluate(() => {
        const seen = new Set();
        for (const el of document.querySelectorAll('body *')) {
          const own = Array.from(el.childNodes).filter((n) => n.nodeType === 3).map((n) => n.textContent).join('').trim();
          if (!/[؀-ۿ]/.test(own) || !el.getClientRects().length) continue;
          const weight = Number(getComputedStyle(el).fontWeight);
          if (!seen.has(weight) && !document.querySelector(`[data-face-weight="${weight}"]`)) el.setAttribute('data-face-weight', String(weight));
          seen.add(weight);
        }
        return [...document.querySelectorAll('[data-face-weight]')].map((el) => Number(el.getAttribute('data-face-weight')));
      });
      for (const weight of weights) {
        if (byWeight[weight]) continue;
        byWeight[weight] = (await faces(page, page.locator(`[data-face-weight="${weight}"]`))).filter((f) => f.family !== 'Mulish');
      }
    };
    await openMenu(page);
    await probe();
    await page.getByTestId('account-report').click();
    await page.getByTestId('feedback').waitFor();
    await page.evaluate(() => document.fonts.ready);
    await probe();
    await page.getByTestId('feedback-cancel').click();
    assert.ok(Object.keys(byWeight).length >= 2, `weights: ${Object.keys(byWeight)}`);
    for (const [weight, list] of Object.entries(byWeight)) {
      assert.deepEqual(list.map((f) => f.family), [ARABIC], `weight ${weight}`);
      assert.equal(list[0].bundled, true, `weight ${weight}`);
    }
    const fontLoads = fonts.all.filter((r) => r.type === 'font');
    assert.ok(fontLoads.length > 0 && fontLoads.every((r) => new URL(r.url).origin === env.origin), 'the faces come from the app');
    assert.deepEqual(env.refused(), []);
  });
});
