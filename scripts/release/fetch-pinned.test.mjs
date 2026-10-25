// fetch-pinned.mjs against a local server: nothing here reaches the network.
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fetchPinned, refusal } from './fetch-pinned.mjs';

const ARCHIVE = Buffer.from('an archive of one commit');
const COMMIT = '7146d5b504f6b63b9e11f7dc0b18c594d0ae179d';
const entry = (over = {}) => ({
  repo: 'uW/obs_images_360',
  commit: COMMIT,
  file: null,
  sha256: crypto.createHash('sha256').update(ARCHIVE).digest('hex'),
  size: ARCHIVE.length,
  ...over,
});

/** A folder holding `fetched.lock` with `entries`, plus a lexicon the lock names and holds. */
function bundle(entries) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'fetch-pinned-'));
  fs.writeFileSync(path.join(dir, 'held.zip'), 'held');
  const held = { repo: 'uW/en_ugl', commit: 'd'.repeat(40), file: 'held.zip', sha256: '0'.repeat(64), size: 4 };
  fs.writeFileSync(path.join(dir, 'fetched.lock'), JSON.stringify({ entries: [held, ...entries] }));
  return dir;
}

/** A server that answers each request with the next of `answers` ([status, body]), the last one forever. */
async function serve(answers) {
  const asked = [];
  const server = http.createServer((request, response) => {
    asked.push(request.url);
    const [status, body] = answers[Math.min(asked.length, answers.length) - 1];
    response.writeHead(status).end(body);
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  return { base: `http://127.0.0.1:${server.address().port}`, asked, close: () => server.close() };
}

const quiet = { wait: 1, log: () => {} };

test('the refusal names a wrong size or a wrong sha256', () => {
  assert.equal(refusal(entry(), ARCHIVE), null);
  assert.match(refusal(entry(), Buffer.concat([ARCHIVE, Buffer.from('x')])), /bytes, fetched\.lock says/);
  const flipped = Buffer.from(ARCHIVE);
  flipped[0] ^= 1;
  assert.match(refusal(entry(), flipped), /^sha256 [0-9a-f]{64}, fetched\.lock says/);
});

test('an archive the bundle lacks is fetched by its commit, after a failed try', async () => {
  const dir = bundle([entry()]);
  const server = await serve([[503, 'busy'], [200, ARCHIVE]]);
  try {
    const fetched = await fetchPinned(dir, { base: server.base, ...quiet });
    assert.deepEqual(fetched, [`obs_images_360-${COMMIT}.zip`]);
    assert.deepEqual(server.asked, [`/uW/obs_images_360/archive/${COMMIT}.zip`, `/uW/obs_images_360/archive/${COMMIT}.zip`]);
    assert.deepEqual(fs.readFileSync(path.join(dir, fetched[0])), ARCHIVE);
    assert.deepEqual(await fetchPinned(dir, { base: server.base, ...quiet }), [], 'nothing is fetched twice');
  } finally {
    server.close();
  }
});

test('an archive that is not the locked one is refused and not kept', async () => {
  const dir = bundle([entry()]);
  const server = await serve([[200, 'something else entirely']]);
  try {
    await assert.rejects(fetchPinned(dir, { base: server.base, ...quiet }), /uW\/obs_images_360 .*fetched\.lock says/);
    assert.equal(server.asked.length, 1, 'a wrong archive is not retried');
    assert.deepEqual(fs.readdirSync(dir).sort(), ['fetched.lock', 'held.zip']);
  } finally {
    server.close();
  }
});

test('a server that never answers fails after the tries', async () => {
  const dir = bundle([entry()]);
  const server = await serve([[500, 'down']]);
  try {
    await assert.rejects(fetchPinned(dir, { base: server.base, tries: 3, ...quiet }), /HTTP 500/);
    assert.equal(server.asked.length, 3);
  } finally {
    server.close();
  }
});
