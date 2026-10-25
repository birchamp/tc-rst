// Help and guides (S13, tC4 #519, #632, #729): tC4's e2e/help.spec.ts, cases a to i, with the
// in-project half of a left to B3 (A5 D18). The pages are tc-website's, vendored (AD29), served
// by `tc4 serve` at /help/; the page swap and the link rules run in the page (app/help-shim.js).
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/journeys/help.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import http from 'node:http';
import { browserTest, focused, openMenu, panel, recordRequests, sleep, trigger, until, withApp } from './kit.mjs';

// The install page's download link, as this app serves it (H16 part 3; tc-server HELP_OVERLAY).
const DOWNLOAD = 'https://translationcore.com/tc4';
const helpPanel = (page) => page.getByTestId('help-panel');
const frame = (page) => page.frameLocator('[data-testid="help-frame"]');
const h1 = (page) => frame(page).getByRole('heading', { level: 1 });
const helpFrame = (page) => page.frames().find((f) => f.url().includes('/help/'));
const onPage = (page, file) => page.frames().some((f) => f.url().endsWith(`/help/${file}`));

async function openHelp(page) {
  await openMenu(page);
  await page.getByTestId('account-help').click();
  await helpPanel(page).waitFor();
  // The panel slides in; a click before it stops lands where the frame was.
  await until('the panel stops', async () => (await helpPanel(page).evaluate((el) => getComputedStyle(el).transform)) === 'none');
  await until('the help home', async () => (await h1(page).textContent().catch(() => null)) === 'translationCore 4 Help');
}

async function expectPage(page, file, title) {
  await until(`${file} shows`, async () => (await h1(page).textContent().catch(() => null)) === title && onPage(page, file));
}

async function openInstallPage(page) {
  await frame(page).locator('a[href="gs-install.html"]:visible').first().click();
  await expectPage(page, 'gs-install.html', 'Install and start tC4');
}

async function expectClosed(page) {
  await until('the panel closes', async () => (await helpPanel(page).count()) === 0);
  await until('the focus is on the account button', async () => (await focused(page)) === 'account-menu');
}

browserTest('a. (Home half) Help and guides sits below the internet switch; the arrows reach it and Enter opens it', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    const ids = await panel(page).locator('[role^="menuitem"]').evaluateAll((rows) => rows.map((r) => r.getAttribute('data-testid')));
    const at = ids.indexOf('account-internet');
    assert.deepEqual(ids.slice(at, at + 3), ['account-internet', 'account-help', 'account-language']);
    assert.equal((await page.getByTestId('account-help').textContent()).trim(), 'Help and guides');
    await page.getByTestId('account-internet').focus();
    await page.keyboard.press('ArrowDown');
    await until('Help is focused', async () => (await focused(page)) === 'account-help');
    await page.keyboard.press('Enter');
    await helpPanel(page).waitFor();
    await until('the help home', async () => (await h1(page).textContent().catch(() => null)) === 'translationCore 4 Help');
    await page.keyboard.press('Escape');
    await expectClosed(page);
  });
});

browserTest('b. with every other origin refused, the pages, their images and the search results show', async () => {
  await withApp(async ({ page, env }) => {
    const requests = recordRequests(page);
    await openHelp(page);
    await frame(page).locator('#q').fill('offline');
    const results = frame(page).locator('#search-results a');
    await results.first().waitFor();
    assert.ok((await results.count()) > 0);
    await frame(page).locator('a[href="section-align.html"]:visible').first().click();
    await frame(page).locator('a[href="al-link-words.html"]:visible').first().waitFor();
    await frame(page).locator('a[href="al-link-words.html"]:visible').first().click();
    await until('the URL moves', async () => onPage(page, 'al-link-words.html'));
    await until('the page is swapped in', async () => (await h1(page).textContent()) === 'Link your words to the source');
    for (const img of [frame(page).locator('img.brand-logo'), frame(page).locator('img[src^="img/"]').first()]) {
      await img.scrollIntoViewIfNeeded();
      await until('the image loads', () => img.evaluate((el) => el.complete && el.naturalWidth > 0));
    }
    assert.deepEqual(env.refused(), []);
    assert.ok(requests.all.some((r) => new URL(r.url).pathname.endsWith('/help/al-link-words.html')));
  });
});

// Q31 (b), #763: Help never names a tC4 build, so it cannot disagree with About's version.
browserTest('help_home_shows_no_rc1_build_text_offline', async () => {
  await withApp(async ({ page, env }) => {
    await openHelp(page);
    const text = await frame(page).locator('body').innerText();
    assert.doesNotMatch(text, /Pre-release software|4\.0\.0-rc\.1|\bbuild [0-9a-f]{7}\b/);
    assert.deepEqual(env.refused(), []);
  });
});

browserTest('c. with the internet off, the download link asks first; Not now opens nothing and sends nothing', async () => {
  await withApp(async ({ page, context, env }) => {
    const requests = recordRequests(page);
    let pages = 0;
    context.on('page', () => { pages += 1; });
    await openHelp(page);
    await openInstallPage(page);
    await frame(page).locator(`a[href="${DOWNLOAD}"]`).click();
    const ask = page.getByTestId('net-ask');
    await ask.waitFor();
    assert.equal(await ask.getAttribute('data-kind'), 'helpLink');
    assert.equal(await page.getByTestId('net-ask-reason').textContent(), 'This step needs the internet to open this link in your browser.');
    // Escape belongs to the question, not to the panel under it.
    await page.keyboard.press('Escape');
    await until('the question closes', async () => (await ask.count()) === 0);
    assert.equal(await helpPanel(page).count(), 1);
    await frame(page).locator(`a[href="${DOWNLOAD}"]`).click();
    await ask.waitFor();
    await page.getByTestId('net-cancel').click();
    await until('the question closes', async () => (await ask.count()) === 0);
    await sleep(500);
    assert.equal(pages, 0, 'no window opened');
    await expectPage(page, 'gs-install.html', 'Install and start tC4');
    assert.equal(await trigger(page).getAttribute('data-internet'), 'off');
    assert.equal(requests.api().filter((p) => p === '/api/v1/account/helpLink').length, 0, 'nothing asked of the server');
    assert.deepEqual(env.refused(), []);
  });
});

browserTest('d. with the internet on, the download link opens a window at its address and asks nothing; the frame stays', async () => {
  await withApp(async ({ page }) => {
    await openMenu(page);
    await page.getByTestId('account-internet').click();
    await until('on', async () => (await trigger(page).getAttribute('data-internet')) === 'on');
    await page.keyboard.press('Escape');
    await openHelp(page);
    await openInstallPage(page);
    const opened = page.waitForEvent('popup');
    await frame(page).locator(`a[href="${DOWNLOAD}"]`).click();
    const popup = await opened;
    assert.equal(popup.url(), DOWNLOAD);
    await popup.close();
    assert.equal(await page.getByTestId('net-ask').count(), 0);
    await expectPage(page, 'gs-install.html', 'Install and start tC4');
  }, {
    // The window would load translationcore.com: a stub answers it, so the run stays on this machine.
    init: (context) => context.route(DOWNLOAD, (route) => route.fulfill({ contentType: 'text/html', body: '<title>stub</title>' })),
  });
});

browserTest('e. the ✕, Escape in the panel and Escape in the frame close it; the focus returns to the account button', async () => {
  await withApp(async ({ page }) => {
    await openHelp(page);
    await page.getByTestId('help-close').click();
    await expectClosed(page);
    await openHelp(page);
    await page.getByTestId('help-close').focus();
    await page.keyboard.press('Escape');
    await expectClosed(page);
    await openHelp(page);
    await frame(page).locator('#q').click();
    await page.keyboard.press('Escape');
    await expectClosed(page);
  });
});

browserTest('f. Ctrl, Shift, Meta and middle clicks on a help link open no window; the frame shows the page', async () => {
  await withApp(async ({ page, context, env }) => {
    let pages = 0;
    context.on('page', () => { pages += 1; });
    for (const [name, options] of [['ctrl', { modifiers: ['Control'] }], ['shift', { modifiers: ['Shift'] }], ['meta', { modifiers: ['Meta'] }], ['middle', { button: 'middle' }]]) {
      const before = pages;
      await page.goto('about:blank');
      await page.goto(`${env.origin}/#t=${env.token}`);
      await trigger(page).waitFor();
      await openHelp(page);
      await frame(page).locator('summary > a[href="section-getting-started.html"]').click(options);
      await sleep(500);
      assert.equal(pages - before, 0, `${name}: no window`);
      assert.equal(await helpPanel(page).count(), 1);
      await expectPage(page, 'section-getting-started.html', 'Getting started');
    }
  });
});

browserTest('g. a topic switch reloads nothing and paints no blank frame; of two quick clicks the second wins; the search page lists its results', async () => {
  await withApp(async ({ page }) => {
    await openHelp(page);
    await helpFrame(page).evaluate(() => { window.tc4Mark = 729; });
    const sameDocument = () => helpFrame(page).evaluate(() => window.tc4Mark === 729);
    const box = await page.getByTestId('help-frame').boundingBox();
    const logo = await frame(page).locator('img.brand-logo').boundingBox();
    const segments = [
      { x0: box.x, x1: box.x + box.width, y: box.y + 4 },
      { x0: logo.x, x1: logo.x + logo.width, y: logo.y + logo.height / 2 },
    ];
    const viewport = page.viewportSize();
    const cdp = await page.context().newCDPSession(page);
    let shots = [];
    cdp.on('Page.screencastFrame', ({ data, sessionId }) => {
      shots.push(data);
      cdp.send('Page.screencastFrameAck', { sessionId }).catch(() => {});
    });
    const scorer = await page.context().newPage();
    const rowOf = (png, { x0, x1, y }) => scorer.evaluate(async ({ png, x0, x1, y, w, h }) => {
      const img = new Image();
      img.src = `data:image/png;base64,${png}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const ctx = c.getContext('2d');
      ctx.drawImage(img, 0, 0);
      const sx = img.width / w;
      return Array.from(ctx.getImageData(Math.round(x0 * sx), Math.round(y * (img.height / h)), Math.round((x1 - x0) * sx), 1).data);
    }, { png, x0, x1, y, w: viewport.width, h: viewport.height });
    const share = (row, bar) => {
      let same = 0;
      for (let i = 0; i < bar.length; i += 4) if ([0, 1, 2].every((k) => Math.abs(row[i + k] - bar[i + k]) < 24)) same += 1;
      return same / (bar.length / 4);
    };
    const rows = [];
    try {
      for (const href of ['section-bible-projects.html', 'bp-new-bible.html', 'bp-open-book.html', 'bp-import.html', 'bp-export-pdf.html']) {
        const before = await h1(page).textContent();
        shots = [];
        await cdp.send('Page.startScreencast', { format: 'png' });
        await sleep(300);
        await frame(page).locator(`nav a[href="${href}"]:visible`).click();
        await until(`${href} URL`, async () => onPage(page, href));
        await until(`${href} swapped in`, async () => (await h1(page).textContent()) !== before);
        await sleep(700);
        assert.equal(await sameDocument(), true, `${href} reloaded the frame`);
        await cdp.send('Page.stopScreencast');
        const [bar, mark] = await Promise.all(segments.map(async (segment) => {
          const pixels = await Promise.all(shots.map((png) => rowOf(png, segment)));
          return pixels.map((row) => share(row, pixels[0]));
        }));
        rows.push({ topic: href, frames: shots.length, blank: bar.filter((s, i) => s < 0.9 || mark[i] < 0.9).length });
      }
    } finally {
      await scorer.close();
      if (process.env.HELP_SWITCHES_OUT) fs.writeFileSync(process.env.HELP_SWITCHES_OUT, `${JSON.stringify(rows, null, 2)}\n`);
    }
    assert.equal(rows.length, 5);
    assert.ok(rows.every((r) => r.frames > 0), JSON.stringify(rows));
    assert.deepEqual(rows.filter((r) => r.blank > 0), [], 'no blank frame');

    // A second topic while the first still loads (held half a second): the second wins.
    await page.route('**/help/bp-add-book.html', async (route) => {
      await sleep(500);
      await route.continue().catch(() => {});
    });
    await frame(page).locator('nav a[href="bp-add-book.html"]:visible').click({ noWaitAfter: true });
    await frame(page).locator('nav a[href="bp-import.html"]:visible').click();
    await until('Import shows', async () => /^Import a project/.test(await h1(page).textContent()));
    await sleep(700);
    assert.match(helpFrame(page).url(), /\/help\/bp-import\.html$/);
    assert.match(await h1(page).textContent(), /^Import a project/);
    assert.equal(await sameDocument(), true);

    // The search page builds its list from its own address when its script runs.
    await frame(page).locator('#q').fill('offline');
    await frame(page).locator('#q').press('Enter');
    await until('the search page', async () => helpFrame(page).url().includes('/help/search.html?q=offline'));
    await frame(page).locator('#search-page-results li').first().waitFor();
    assert.equal(await sameDocument(), true);
  });
});

browserTest('h. a read that ends outside help/ or is not HTML is not swapped in and runs no script; the frame loads the page', async () => {
  const topic = 'section-bible-projects.html';
  const injected = '<!doctype html><title>Injected</title><h1>Injected</h1><script>parent.tc4Injected = 1;</script>';
  const other = http.createServer((_req, res) => {
    res.writeHead(200, { 'content-type': 'text/html', 'access-control-allow-origin': '*' });
    res.end(injected);
  });
  await new Promise((done) => other.listen(0, '127.0.0.1', done));
  const otherUrl = `http://127.0.0.1:${other.address().port}/help/${topic}`;
  try {
    await withApp(async ({ page, env, allow }) => {
      // The read a redirect sends to another origin never leaves: the page's CSP (connect-src
      // 'self', APP-DESIGN 5.2) refuses it, which tC4's page has no rule for; the shim's check
      // of where a read ended keeps out the read that stays on this origin.
      const csp = [];
      page.on('console', (m) => { if (/Refused to connect .*Content Security Policy/.test(m.text())) csp.push(m.text()); });
      allow(/Refused to connect .*Content Security Policy/);
      const cases = [
        ['redirect to another origin', (route) => route.fulfill({ status: 302, headers: { location: otherUrl } })],
        ['redirect out of help/', (route) => route.fulfill({ status: 302, headers: { location: '/' } })],
        ['not HTML', (route) => route.fulfill({ status: 200, contentType: 'application/json', body: injected })],
      ];
      for (const [name, answer] of cases) {
        await page.goto('about:blank');
        await page.goto(`${env.origin}/#t=${env.token}`);
        await trigger(page).waitFor();
        await openHelp(page);
        await page.unrouteAll({ behavior: 'ignoreErrors' });
        // Only the swap's read of the topic gets the bad answer; the frame's own load does not.
        await page.route(`${env.origin}/help/${topic}`, (route) => (route.request().resourceType() === 'fetch' ? answer(route) : route.fallback()));
        await frame(page).locator(`nav a[href="${topic}"]:visible`).click();
        await until(`${name}: the frame loads the topic`, async () => (await h1(page).textContent().catch(() => null)) === 'Bible projects' && onPage(page, topic));
        assert.equal(await page.evaluate(() => window.tc4Injected), undefined, `${name}: no script ran`);
      }
      assert.equal(csp.length, 1, 'the CSP refused the read to the other origin');
    });
  } finally {
    other.close();
  }
});

browserTest('i. Escape while a page loads closes the panel and the next open shows the home; a page the server lacks shows in the frame', async () => {
  await withApp(async ({ page, allow }) => {
    allow(/status of 404/);
    const topic = 'section-bible-projects.html';
    await openHelp(page);
    await page.route(`**/help/${topic}`, async (route) => {
      await sleep(1500);
      await route.continue().catch(() => {});
    });
    await frame(page).locator(`nav a[href="${topic}"]:visible`).click();
    await frame(page).locator('#q').press('Escape');
    await expectClosed(page);
    await sleep(2000);
    await openHelp(page);
    await page.unrouteAll({ behavior: 'ignoreErrors' });
    await page.route(`**/help/${topic}`, (route) => route.fulfill({ status: 404, contentType: 'text/plain', body: 'Not found' }));
    await frame(page).locator(`nav a[href="${topic}"]:visible`).click();
    await until('the frame is on the topic', async () => onPage(page, topic));
    await until('the server\'s answer shows', async () => (await frame(page).locator('body').textContent()) === 'Not found');
    assert.equal(await helpPanel(page).count(), 1);
    await page.getByTestId('help-close').click();
    await expectClosed(page);
  });
});
