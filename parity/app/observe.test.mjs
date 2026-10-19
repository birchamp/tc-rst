// The request tracker and `settle` of observe.mjs, against a stand-in for the page, its
// DevTools session and its clock, so they run without a browser and the same on a busy machine:
//
//   node --test parity/app/observe.test.mjs
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { machineFingerprint, settle, trackRequests } from './observe.mjs';

/** A page whose DevTools session the test drives (`cdp.emit(method, params)`), and its clock:
 * `Date.now()` reads `clock.now`, which only the page's waits move, and `clock.at(ms, event)`
 * runs `event` at `ms`, inside the wait that passes it. */
function fakePage(t) {
  const clock = { now: 0, due: [] };
  t.mock.method(Date, 'now', () => clock.now);
  clock.at = (ms, event) => clock.due.push({ ms, event });
  const cdp = new EventEmitter();
  cdp.sent = [];
  cdp.send = async (method) => { cdp.sent.push(method); };
  const page = {
    context: () => ({ newCDPSession: async (p) => (p === page ? cdp : null) }),
    waitForTimeout: async (ms) => {
      const until = clock.now + ms;
      for (const { ms: at, event } of clock.due.filter((e) => e.ms <= until).sort((a, b) => a.ms - b.ms)) {
        clock.now = at;
        event();
      }
      clock.due = clock.due.filter((e) => e.ms > until);
      clock.now = until;
    },
    evaluate: async () => {},
  };
  return { page, cdp, clock };
}

/** Fails unless settle returned after `quietMs` of quiet since `since`, and not long after. */
function assertQuietFor(quietMs, since) {
  const quiet = Date.now() - since;
  assert.ok(quiet >= quietMs && quiet < quietMs + 200, `settled ${quiet} ms after the app went quiet, not ${quietMs}`);
}

const api = (url) => new URL(url).pathname.startsWith('/api/');
const commit = (cdp, frameId, loaderId) => cdp.emit('Page.frameNavigated', { frame: { id: frameId, loaderId } });
const send = (cdp, requestId, loaderId, path, { frameId = 'main', type = 'Fetch' } = {}) => cdp.emit('Network.requestWillBeSent', {
  requestId, loaderId, frameId, type, request: { method: 'GET', url: `http://127.0.0.1:1${path}` },
});

test('the tracker counts tracked requests until they finish or fail', async (t) => {
  const { page, cdp } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  assert.deepEqual(cdp.sent, ['Page.enable', 'Network.enable']);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/a');
  send(cdp, 'r2', 'doc1', '/api/b');
  send(cdp, 'r3', 'doc1', '/clients/uw-tc4/assets/app.js');
  send(cdp, 'r4', 'doc1', '/api/page', { type: 'Document' });
  assert.deepEqual([...inFlight.keys()], ['r1', 'r2']);
  send(cdp, 'r1', 'doc1', '/api/a-redirected');
  cdp.emit('Network.loadingFinished', { requestId: 'r1' });
  cdp.emit('Network.loadingFailed', { requestId: 'r2' });
  assert.equal(inFlight.size, 0);
});

test('a frame that commits a new document forgets its old document\'s requests, even ones reported late', async (t) => {
  const { page, cdp } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  commit(cdp, 'help', 'helpdoc1');
  send(cdp, 'r1', 'doc1', '/api/content/46.md');
  send(cdp, 'h1', 'helpdoc1', '/api/in-the-help-frame', { frameId: 'help' });
  commit(cdp, 'main', 'doc2');
  assert.deepEqual([...inFlight.keys()], ['h1'], 'the other frame keeps its request');
  // Chromium can report a request of the replaced document after the commit.
  send(cdp, 'r2', 'doc1', '/api/content/47.md');
  assert.deepEqual([...inFlight.keys()], ['h1']);
  send(cdp, 'r3', 'doc2', '/api/content/01.md');
  assert.deepEqual([...inFlight.keys()], ['h1', 'r3']);
  cdp.emit('Page.frameDetached', { frameId: 'help' });
  assert.deepEqual([...inFlight.keys()], ['r3'], 'a removed frame takes its requests with it');
});

test('settle waits until no request has been in flight for quietMs', async (t) => {
  const { page, cdp, clock } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/a');
  // settle looks every 50 ms. The request ends 5 ms before its look at 100, so the look at 50
  // is the last that sees it: counted from there, the app would be "quiet" at 100.
  clock.at(95, () => cdp.emit('Network.loadingFinished', { requestId: 'r1' }));
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000 });
  assertQuietFor(40, 95);
});

test('settle waits quietMs when nothing is in flight, and counts from when a reload cut a request off', async (t) => {
  const { page, cdp, clock } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000 });
  assertQuietFor(40, 0);
  const start = Date.now();
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/content/46.md');
  clock.at(start + 90, () => commit(cdp, 'main', 'doc2'));
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000 });
  assertQuietFor(40, start + 90);
});

test('a screen whose app never goes quiet fails at the deadline and names what is in flight', async (t) => {
  const { page, cdp } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/never-answered');
  await assert.rejects(settle(page, inFlight, { quietMs: 40, deadlineMs: 150 }),
    /did not go quiet for 40 ms within 0\.15 s; in flight: GET http:\/\/127\.0\.0\.1:1\/api\/never-answered \(\d+ ms\)/);
});

test('settle waits while the server answers busy, and counts the quiet from when it stops', async (t) => {
  const { page } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000, busy: async () => Date.now() < 300 });
  assertQuietFor(40, 300);
});

test('a screen whose server stays busy fails at the deadline and says so', async (t) => {
  const { page } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  await assert.rejects(settle(page, inFlight, { quietMs: 40, deadlineMs: 150, busy: async () => true }),
    /did not go quiet for 40 ms within 0\.15 s; in flight: nothing now; the server answered busy \(test\/quiet\)/);
});

test('busy is asked only once the requests are quiet', async (t) => {
  const { page, cdp, clock } = fakePage(t);
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/a');
  clock.at(100, () => cdp.emit('Network.loadingFinished', { requestId: 'r1' }));
  const asked = [];
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000, busy: async () => { asked.push(Date.now()); return false; } });
  assert.equal(asked.length, 1, `busy was asked ${asked.length} times`);
  assert.ok(asked[0] >= 100 + 40, `busy was asked at ${asked[0]} ms, before the requests had been quiet for 40 ms`);
  assertQuietFor(40, 100);
});

test('the machine fingerprint hashes the sorted font list', () => {
  const lines = ['/usr/share/fonts/b.ttf\t0\t65536', '/usr/share/fonts/a.ttf\t1\t131072'];
  const print = machineFingerprint('141.0.7390.37', { run: () => `${lines.join('\n')}\n` });
  const sorted = `${[...lines].sort().join('\n')}\n`;
  assert.deepEqual(print, {
    platform: `${process.platform}-${process.arch}`,
    chromium: '141.0.7390.37',
    fonts: { count: 2, sha256: crypto.createHash('sha256').update(sorted).digest('hex') },
  });
  const blind = machineFingerprint('141.0.7390.37', { run: () => { throw new Error('fc-list: not found'); } });
  assert.equal(blind.fonts, null);
});
