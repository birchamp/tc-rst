// The request tracker and `settle` of observe.mjs, against a stand-in for the page and its
// DevTools session, so they run without a browser:
//
//   node --test parity/app/observe.test.mjs
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { settle, trackRequests } from './observe.mjs';

/** A page whose DevTools session the test drives: `cdp.emit(method, params)`. */
function fakePage() {
  const cdp = new EventEmitter();
  cdp.sent = [];
  cdp.send = async (method) => { cdp.sent.push(method); };
  const page = {
    context: () => ({ newCDPSession: async (p) => (p === page ? cdp : null) }),
    waitForTimeout: (ms) => new Promise((r) => setTimeout(r, Math.min(ms, 5))),
    evaluate: async () => {},
  };
  return { page, cdp };
}

const api = (url) => new URL(url).pathname.startsWith('/api/');
const commit = (cdp, frameId, loaderId) => cdp.emit('Page.frameNavigated', { frame: { id: frameId, loaderId } });
const send = (cdp, requestId, loaderId, path, { frameId = 'main', type = 'Fetch' } = {}) => cdp.emit('Network.requestWillBeSent', {
  requestId, loaderId, frameId, type, request: { method: 'GET', url: `http://127.0.0.1:1${path}` },
});

test('the tracker counts tracked requests until they finish or fail', async () => {
  const { page, cdp } = fakePage();
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

test('a frame that commits a new document forgets its old document\'s requests, even ones reported late', async () => {
  const { page, cdp } = fakePage();
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

test('settle waits for the app to go quiet', async () => {
  const { page, cdp } = fakePage();
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/a');
  let ended = null;
  setTimeout(() => {
    ended = Date.now();
    cdp.emit('Network.loadingFinished', { requestId: 'r1' });
  }, 60);
  await settle(page, inFlight, { quietMs: 40, deadlineMs: 2_000 });
  assert.ok(ended !== null, 'settled while the request was in flight');
  // The loop looks every 5 ms here, so quiet is measured from the last look that saw the request.
  assert.ok(Date.now() - ended >= 40 - 5, `settled ${Date.now() - ended} ms after the request ended, not 40`);
});

test('a screen whose app never goes quiet fails at the deadline and names what is in flight', async () => {
  const { page, cdp } = fakePage();
  const inFlight = await trackRequests(page, api);
  commit(cdp, 'main', 'doc1');
  send(cdp, 'r1', 'doc1', '/api/never-answered');
  await assert.rejects(settle(page, inFlight, { quietMs: 40, deadlineMs: 150 }),
    /did not go quiet for 40 ms within 0\.15 s; in flight: GET http:\/\/127\.0\.0\.1:1\/api\/never-answered \(\d+ ms\)/);
});
