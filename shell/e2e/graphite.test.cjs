'use strict';

// The P8 probe in the real shell, and in plain Chromium, which has no Graphite and so must be
// judged not shaped: the standing proof that the check can fail (card G4 step 3).

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const h = require('./harness.cjs');
const g = require('../graphite.cjs');

const FONT = path.join(h.REPO, 'crates', 'tc-ui', 'assets', 'ds', 'assets', 'fonts', 'AwamiNastaliq-Regular.ttf');

function report(where, measured) {
  const verdict = g.graphiteVerdict(measured);
  console.log(`# ${where} ${process.platform}-${process.arch}: ${JSON.stringify(measured)}`);
  return verdict;
}

h.shellTest('graphite_shapes_awami_nastaliq_in_the_shell', async () => {
  const dir = h.appDataDir('graphite');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    // The face the app ships, served by tc4 like every other font of the page.
    const font = await page.evaluate(() => {
      const build = new URL(document.querySelector('script[src*="/ui/"]').src).pathname.split('/')[2];
      return `/ui/${build}/ds/assets/fonts/AwamiNastaliq-Regular.ttf`;
    });
    const verdict = report('shell', await g.measureProbe(page, font));
    assert.ok(verdict.ok, `INV-GRAPHITE-SHAPES-OR-FAILS: ${process.platform}: ${verdict.broken.join('; ')}`);
  } finally {
    await h.finish(app);
  }
});

// Playwright's own Chromium is the Chromium without Graphite. Only the Linux job installs it.
if (process.platform === 'linux') {
  h.shellTest('plain_chromium_fails_the_graphite_probe', async () => {
    const { chromium } = require(process.env.PLAYWRIGHT_MODULE || path.join(h.REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright'));
    const browser = await chromium.launch();
    try {
      const page = await browser.newPage();
      await page.route('http://probe.test/**', (route) =>
        route.request().url().endsWith('.ttf')
          ? route.fulfill({ body: fs.readFileSync(FONT), contentType: 'font/ttf' })
          : route.fulfill({ body: '<!doctype html><html><body></body></html>', contentType: 'text/html' }),
      );
      await page.goto('http://probe.test/');
      const measured = await g.measureProbe(page, 'http://probe.test/AwamiNastaliq-Regular.ttf');
      assert.equal(measured.fontLoaded, true, 'the face loads, so the verdict judges the shaping');
      const verdict = report('plain chromium', measured);
      assert.equal(verdict.ok, false, 'plain Chromium does not shape Nastaliq');
      assert.ok(verdict.broken.some((b) => b.startsWith('boxWidth')), verdict.broken.join('; '));
    } finally {
      await browser.close();
    }
  });
}
