// The Rust environment of the parity driver (env-rust.mjs).
//
//   node --test parity/app/env-rust.test.mjs
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/env-rust.test.mjs
//
// The first group needs nothing and runs in CI. The browser group needs `tc4` built with
// `--features test-hooks,fixture-backend` (TC4_BIN), the folder of crates/tc-ui/build-app.sh (TC4_UI) and
// Playwright (parity/tc4-baseline/node_modules, or PLAYWRIGHT_MODULE); without them each of its
// tests is skipped and says why. Each test starts its own `tc4` and ends it.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { recordSequence } from './behaviour.mjs';
import { caseRoute, mergeCaptures } from './capture-rust.mjs';
import { REPO, newRequestId, notYet, rustTracks, scenarioArgs, startRustEnv } from './env-rust.mjs';
import { CHROMIUM_ARGS, captureScreen, settle, trackRequests } from './observe.mjs';
import { designUnits } from './units.mjs';

const at = (p) => `http://127.0.0.1:4321${p}`;

test('rustTracks counts the API and pictures and never the event stream', () => {
  assert.equal(rustTracks(at('/api/v1/app/hello')), true);
  assert.equal(rustTracks(at('/api/v1/events?after=0')), false);
  assert.equal(rustTracks(at('/api/v1/events')), false);
  assert.equal(rustTracks(at('/media/obs/x/01-01.jpg')), true);
  assert.equal(rustTracks(at('/ui/abcd/i18n/ar.json')), false);
  assert.equal(rustTracks(at('/')), false);
});

test('every Door43 step names the unit that brings it', async () => {
  for (const [step, unit] of [['pushToDoor43', 'B9b'], ['dropNextClone', 'E1a'], ['teammatesChangeOneVerse', 'B10'], ['loseFromDoor43', 'B10']]) {
    await assert.rejects(notYet(step, unit)('_local_/_local_/sample_burrito'), new RegExp(`^Error: ${step} needs the fake Door43 in the Rust environment, which unit ${unit} adds`));
  }
});

test('a scenario reaches tc4 as the state folder it was built into', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tc-env-rust-'));
  try {
    const args = scenarioArgs('shared', dir);
    assert.deepEqual(args, ['--scenario', path.join(dir, 'state')]);
    for (const file of ['client_settings/uw-tc4.json', 'repos/_local_/_local_/sample_burrito/metadata.json',
      'repos/_local_/_local_/sample_burrito/.remotes.json', 'repos/_local_/_sideloaded_/en_tn/metadata.json']) {
      assert.ok(fs.existsSync(path.join(args[1], file)), file);
    }
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('a request id is 16 random bytes in lowercase hex', () => {
  const a = newRequestId();
  assert.match(a, /^[0-9a-f]{32}$/);
  assert.notEqual(a, newRequestId());
});

test('the units of the design are read from its section 10', () => {
  const doc = ['## 9. Before', '| A99 not here |', '## 10. Work breakdown', '| Unit | Scope |', '|---|---|',
    '| A6b Parity harness | x |', '| G0 Electronite spike | y |', '| B1a seed | z |', 'text', '## 11. Risks', '| C9 after |'].join('\n');
  assert.deepEqual([...designUnits(doc)], ['A6b', 'G0', 'B1a']);
});

test('a screens-only rerun keeps the case entries, and a case rerun keeps the screens', () => {
  const screens = [{ name: 'home' }, { name: 'about' }];
  const cases = [{ name: 'home-empty' }, { name: 'license' }];
  const previous = [
    { name: 'about', kind: 'screen', by: 'A5' },
    { name: 'license', kind: 'case', by: 'A5' },
  ];
  assert.deepEqual(
    mergeCaptures(previous, [{ name: 'home', kind: 'screen', by: 'B2' }], screens, cases).map((e) => `${e.kind}:${e.name}:${e.by}`),
    ['screen:home:B2', 'screen:about:A5', 'case:license:A5'],
  );
  assert.deepEqual(
    mergeCaptures(previous, [{ name: 'license', kind: 'case', by: 'B2' }, { name: 'home-empty', kind: 'case', by: 'B2' }], screens, cases).map((e) => `${e.kind}:${e.name}:${e.by}`),
    ['screen:about:A5', 'case:home-empty:B2', 'case:license:B2'],
  );
  // A case and a screen of one name are two entries (the gate refuses such a case anyway).
  assert.equal(mergeCaptures([{ name: 'home', kind: 'screen' }], [{ name: 'home', kind: 'case' }], screens, [{ name: 'home' }]).length, 2);
});

test('a case takes its rust route, or its state\'s route when it is that state as tC4 drew it', async () => {
  const route = async () => {};
  const rust = async () => {};
  const portable = { portable: true, route };
  assert.equal(caseRoute({ rust }, { portable: false, route }), rust);
  assert.equal(caseRoute({}, { portable: false, route }), null, 'a route that writes into the project is not the Rust app\'s');
  assert.equal(caseRoute({ patch: () => ({}) }, portable), null, 'a patched case is a state the route does not reach');
  assert.equal(caseRoute({ returns: { x: { value: 1 } } }, portable), null);
  const steps = [];
  const run = caseRoute({ steps: [] }, { portable: true, route: async (app) => steps.push(['route', app.page]) });
  await run({ page: 'the page' }, null);
  assert.deepEqual(steps, [['route', 'the page']]);
});

// ---- with tc4 and a browser ----------------------------------------------------------------

function browserSetup() {
  const { TC4_BIN, TC4_UI } = process.env;
  if (!TC4_BIN || !TC4_UI) return { skip: 'set TC4_BIN (tc4 built with --features test-hooks,fixture-backend) and TC4_UI (target/tc-ui-app)' };
  try {
    const dir = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
    return { tc4: TC4_BIN, ui: TC4_UI, chromium: createRequire(import.meta.url)(dir).chromium };
  } catch (error) {
    return { skip: `Playwright does not resolve (${error.code ?? error.message}): npm ci in parity/tc4-baseline` };
  }
}
const setup = browserSetup();
const browserTest = (name, body) => test(name, { skip: setup.skip ?? false, timeout: 120_000 }, body);

/** A browser, a `tc4` and a page in a context made as the capture makes it; ended after `body`. */
async function withRustPage(body) {
  const browser = await setup.chromium.launch({ args: CHROMIUM_ARGS });
  const env = await startRustEnv({ scenario: 'sample', tc4: setup.tc4, ui: setup.ui });
  try {
    const context = await browser.newContext(env.contextOptions);
    await env.prepare(context);
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await body({ env, page, busy: () => env.busy(page), errors });
  } finally {
    await env.close();
    await browser.close();
  }
}

const htmlLangDir = (page) => page.evaluate(() => [document.documentElement.lang, document.documentElement.dir]);

browserTest('E1 home loads the page with the token, and the shot settles while the event stream stays open', async () => {
  await withRustPage(async ({ env, page, busy, errors }) => {
    const inFlight = await trackRequests(page, env.tracks);
    const events = await trackRequests(page, (url) => new URL(url).pathname === '/api/v1/events');
    await env.load(page, { locale: 'en' });
    const loaded = Date.now();
    await page.getByTestId('runtime-ready').waitFor({ timeout: 10_000 });
    assert.deepEqual(await htmlLangDir(page), ['en', 'ltr']);
    await settle(page, inFlight, { busy });
    const settledIn = Date.now() - loaded;
    console.log(`# E1: settle returned ${settledIn} ms after the load`);
    assert.ok(settledIn < 5_000, `settle took ${settledIn} ms`);
    assert.equal(inFlight.size, 0);
    assert.deepEqual([...events.values()].map((r) => new URL(r.url).pathname), ['/api/v1/events'], 'the event stream is open when the shot is taken');
    assert.deepEqual(errors, []);
  });
});

browserTest('E2 counting the event stream would make every shot wait out the deadline', async () => {
  await withRustPage(async ({ env, page }) => {
    const all = await trackRequests(page, (url) => new URL(url).pathname.startsWith('/api/'));
    await env.load(page, { locale: 'en' });
    await page.getByTestId('runtime-ready').waitFor({ timeout: 10_000 });
    await assert.rejects(settle(page, all, { deadlineMs: 2_000 }), /did not go quiet for 800 ms within 2 s; in flight: GET http:\/\/127\.0\.0\.1:\d+\/api\/v1\/events/);
  });
});

browserTest('E3 home saves the screen\'s language and forgets everything else', async () => {
  await withRustPage(async ({ env, page, busy }) => {
    const inFlight = await trackRequests(page, env.tracks);
    await env.load(page, { locale: 'ar' });
    await page.waitForFunction(() => document.documentElement.lang === 'ar', null, { timeout: 10_000 });
    await settle(page, inFlight, { busy });
    assert.deepEqual(await htmlLangDir(page), ['ar', 'rtl']);
    await page.evaluate(() => localStorage.setItem('x', '1'));
    await env.call('net/set', { request: newRequestId(), on: true });
    await env.load(page, { locale: 'en' });
    await page.waitForFunction(() => document.documentElement.lang === 'en', null, { timeout: 10_000 });
    await settle(page, inFlight, { busy });
    assert.deepEqual(await htmlLangDir(page), ['en', 'ltr']);
    assert.equal(await page.evaluate(() => localStorage.getItem('x')), null);
    assert.deepEqual(await env.call('net/state', {}), { on: false });
  });
});

browserTest('E4 a screen whose server stays busy reaches the settle deadline and fails', async () => {
  await withRustPage(async ({ env, page, busy }) => {
    const inFlight = await trackRequests(page, env.tracks);
    const { current } = await env.call('test/fieldLog', { field: 'busy' });
    await env.call('test/hold', { request: newRequestId(), at: 'save' });
    // An armed hold alone is not work: the store must be waiting at it.
    const store = env.call('test/fieldStore', { request: newRequestId(), field: 'busy', shown: current.token, text: 'x' });
    for (let tries = 0; !(await env.call('test/quiet', {})).busy; tries += 1) {
      assert.ok(tries < 100, 'the held store never made the server busy');
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    const plainLoad = { name: 'held', run: async () => { await page.goto('about:blank'); await page.goto(`${env.origin}/#t=${env.token}`); } };
    const file = path.join(REPO, 'target', `parity-e4-${process.pid}.png`);
    const shot = await captureScreen({ page, app: null, screen: plainLoad, inFlight, file, busy, failureTestIds: env.failureTestIds, settleOptions: { quietMs: 200, deadlineMs: 3_000 } });
    assert.match(shot.error ?? '', /did not go quiet for 200 ms within 3 s; in flight: nothing now; the server answered busy \(test\/quiet\)/);
    assert.ok(fs.statSync(file).size > 0, 'the shot of a failed screen is still taken');
    fs.rmSync(file);
    // home() resets: the hold is released, the store answers, and the next shot settles.
    const again = await captureScreen({ page, app: null, screen: { name: 'home', run: () => env.load(page, { locale: 'en' }) }, inFlight, file, busy, failureTestIds: env.failureTestIds });
    fs.rmSync(file);
    assert.equal(again.error, null);
    const ack = await store;
    assert.equal(ack.stored, 'written');
  });
});

browserTest('E5 nothing leaves the machine', async () => {
  await withRustPage(async ({ env, page, busy }) => {
    const inFlight = await trackRequests(page, env.tracks);
    await env.load(page, { locale: 'en' });
    await page.getByTestId('runtime-ready').waitFor({ timeout: 10_000 });
    await settle(page, inFlight, { busy });
    assert.deepEqual(env.refused(), [], 'the app asks for nothing outside tc4');
    // The page's own CSP (connect-src 'self') stops a fetch to another origin first.
    const outcome = await page.evaluate(() => fetch('https://example.org/').then(() => 'sent', (e) => `failed: ${e.message}`));
    assert.match(outcome, /^failed/);
    // A load the CSP does not cover still meets the context's route, which aborts it unsent:
    // the token header that the context adds to every request never leaves the machine.
    await assert.rejects(page.goto('https://example.org/'), /ERR_BLOCKED_BY_CLIENT/);
    assert.deepEqual(env.refused(), ['https://example.org/']);
  });
});

browserTest('E6 the recorder keeps node identity across a language switch', async () => {
  await withRustPage(async ({ env, page, busy }) => {
    const inFlight = await trackRequests(page, env.tracks);
    const screen = { name: 'stand-in', run: async () => { await env.load(page, { locale: 'en' }); await page.getByTestId('runtime-ready').waitFor({ timeout: 10_000 }); } };
    const recording = await recordSequence({
      page, app: null, sequence: 'locale-switch', screen, inFlight, busy,
      steps: [{ click: '[data-testid="runtime-locale"]' }, { key: 'ArrowDown' }, { key: 'Enter' }],
    });
    assert.deepEqual(recording.steps.map((s) => s.label), ['start', '{"click":"[data-testid=\\"runtime-locale\\"]"}', '{"key":"ArrowDown"}', '{"key":"Enter"}']);
    const line = (step, testid) => step.dom.root.find((l) => l.includes(`data-testid="${testid}"`));
    const id = (l) => l.match(/ #(\d+|new)>$/)?.[1];
    const [first, last] = [recording.steps[0], recording.steps.at(-1)];
    for (const testid of ['runtime-locale', 'runtime-sample']) {
      assert.match(id(line(first, testid)) ?? '', /^\d+$/, `${testid} has a number at the start`);
      assert.equal(id(line(last, testid)), id(line(first, testid)), `${testid} is the same node after the switch`);
    }
    assert.match(last.focus, /^select /, `the focus is on the picker, not ${last.focus}`);
    const text = (step) => step.dom.root[step.dom.root.indexOf(line(step, 'runtime-sample')) + 1];
    assert.notEqual(text(last), text(first), 'the sample changed language');
    assert.equal(last.overflow, '(none)');
  });
});
