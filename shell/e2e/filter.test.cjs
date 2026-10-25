'use strict';

// The token on every kind of request the browser makes by itself (AD11, G0's request table in
// docs/evidence/electronite-spike-linux-2026-10-17.md), and an /api/ that the mirror's writes do
// not slow (card G1 step 5).

const assert = require('node:assert/strict');
const h = require('./harness.cjs');

// Every response of the window's session, recorded in the main process.
async function record(app) {
  await app.evaluate(({ session }) => {
    globalThis.__seen = [];
    session.defaultSession.webRequest.onResponseStarted((d) => globalThis.__seen.push({ url: d.url, status: d.statusCode, type: d.resourceType }));
  });
  return async () => app.evaluate(() => globalThis.__seen);
}

h.shellTest('header_filter_reaches_every_request_type', async () => {
  const dir = h.appDataDir('filter');
  const app = await h.launch(dir);
  try {
    let page = await h.home(app);
    const seen = await record(app);
    // A save dialog would wait for a person: the print probe cancels it.
    await app.evaluate(({ dialog }) => {
      dialog.showSaveDialog = async () => ({ canceled: true });
    });
    // The document and what it loads, again with the recorder on.
    await page.reload();
    page = await h.home(app);
    const build = await page.evaluate(() => new URL(document.querySelector('script[src*="/ui/"]').src).pathname.split('/')[2]);
    const answers = await page.evaluate(async (ui) => {
      // The handlers come before the address, so a fast load is not missed.
      const added = (tag, attrs) => new Promise((resolve) => {
        const el = document.createElement(tag);
        el.onload = () => resolve('load');
        el.onerror = () => resolve('error');
        Object.assign(el, attrs);
        document.body.append(el);
      });
      const styled = document.createElement('div');
      styled.style.cssText = 'width:4px;height:4px;background-image:url(/help/img/bp-export-usfm-1.png?probe=css-url)';
      document.body.append(styled);
      const face = new FontFace('probe', `url(/ui/${ui}/ds/assets/fonts/charis-sil-cyrillic-700-italic.woff2?probe=font)`);
      const out = {
        script: await added('script', { src: `/ui/${ui}/probe-missing.js?probe=script` }),
        stylesheet: await added('link', { rel: 'stylesheet', href: '/help/assets/style.css?probe=stylesheet' }),
        image: await added('img', { src: '/help/img/obs-story-tiles-2.png?probe=image' }),
        iframe: await added('iframe', { src: '/help/al-open.html?probe=iframe' }),
        font: await face.load().then(() => 'load', () => 'error'),
        fetch: (await fetch('/ui/' + ui + '/ui.css?probe=fetch')).status,
        media: (await fetch('/media/obs/probe/missing.jpg?probe=media')).status,
        prepare: (await fetch('/prepare.html?probe=prepare')).status,
      };
      const link = document.createElement('a');
      link.href = '/download/probe/missing.zip?probe=download';
      link.download = 'missing.zip';
      document.body.append(link);
      link.click();
      out.print = await window.tc4Shell.printPdf('/print/probe?probe=print');
      window.open('/help/al-open.html?probe=child-window', '_blank');
      return out;
    }, build);
    await h.until('the same-origin child window', () =>
      app.evaluate(({ BrowserWindow }) => BrowserWindow.getAllWindows().some((w) => w.webContents.getURL().includes('probe=child-window'))),
    );
    assert.equal(answers.image, 'load');
    assert.equal(answers.iframe, 'load');
    assert.equal(answers.font, 'load');
    assert.equal(answers.fetch, 200);
    assert.equal(answers.prepare, 200);
    const probes = ['script', 'stylesheet', 'image', 'iframe', 'css-url', 'font', 'fetch', 'media', 'prepare', 'download', 'print', 'child-window'];
    let all;
    await h.until('every probe answered', async () => {
      all = await seen();
      return probes.every((p) => all.some((r) => r.url.includes(`probe=${p}`)));
    }, 30000);
    const origin = new URL(page.url()).origin;
    const mine = all.filter((r) => r.url.startsWith(origin));
    const document = mine.find((r) => r.type === 'mainFrame');
    assert.ok(document && document.status === 200, 'the document');
    for (const probe of probes) {
      const got = mine.filter((r) => r.url.includes(`probe=${probe}`));
      assert.ok(got.length > 0, probe);
      // tc4 answers 401 to a request without the token on every path outside /api/.
      for (const r of got) assert.notEqual(r.status, 401, `${probe}: ${r.status}`);
    }
    // The page's own /api/ calls, the event stream among them, carry the token it sets.
    const api = mine.filter((r) => new URL(r.url).pathname.startsWith('/api/'));
    assert.ok(api.some((r) => r.url.includes('/api/v1/events')), 'the event stream was read');
    for (const r of api) assert.notEqual(r.status, 401, r.url);
    for (const r of mine) assert.notEqual(r.status, 401, r.url);
  } finally {
    await h.finish(app);
  }
});

h.shellTest('api_p95_is_not_raised_by_mirror_writes', async () => {
  const dir = h.appDataDir('p95');
  const app = await h.launch(dir);
  try {
    const page = await h.home(app);
    const line = require('../../fixtures/shell/buffer-vectors.json').vectors[0].lines[0];
    const p95 = (busy) => page.evaluate(async ({ busy, line }) => {
      const token = sessionStorage.getItem('tc4.token');
      const headers = { 'X-Tc-Token': token, 'X-Tc-Api': '1', 'Content-Type': 'application/json' };
      const timer = busy ? setInterval(() => window.tc4Shell.bufferChanged(line), 50) : null;
      const times = [];
      for (let i = 0; i < 200; i++) {
        const start = performance.now();
        await fetch('/api/v1/net/state', { method: 'POST', headers, body: '{}' });
        times.push(performance.now() - start);
      }
      clearInterval(timer);
      times.sort((a, b) => a - b);
      return times[Math.floor(times.length * 0.95)];
    }, { busy, line });
    await p95(false);
    const idle = await p95(false);
    const busy = await p95(true);
    console.log(`# /api/ p95 idle ${idle.toFixed(1)} ms, with 20 bufferChanged a second ${busy.toFixed(1)} ms`);
    // docs/PERFORMANCE.md:43's CI tolerance, over a 5 ms floor so that timer noise on a
    // sub-millisecond idle p95 does not count.
    assert.ok(busy <= 3 * Math.max(idle, 5), `busy p95 ${busy} ms is over three times the idle ${idle} ms`);
    const kept = require('node:fs').readFileSync(require('node:path').join(dir, 'appdata', 'drafts-buffer.log'), 'utf8');
    assert.ok(kept.includes(line), 'the lines reached the mirror');
  } finally {
    await h.finish(app);
  }
});
