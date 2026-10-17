'use strict';

// node --test spikes/electronite/shell/lib.test.cjs
const test = require('node:test');
const assert = require('node:assert/strict');
const lib = require('./lib.cjs');

const ORIGIN = 'http://127.0.0.1:40123';
const TOKEN = 'a'.repeat(43);

test('arguments are parsed and checked', () => {
  const options = lib.parseArgs([
    'electron',
    'main.cjs',
    '--no-sandbox',
    '--spike-mode=protocol',
    '--spike-case=type',
    '--spike-out=/tmp/x',
    '--spike-server=/bin/s',
    '--spike-clicks=30',
    '--spike-spawn=detached',
  ]);
  assert.equal(options.mode, 'protocol');
  assert.equal(options.case, 'type');
  assert.equal(options.clicks, 30);
  assert.equal(options.spawn, 'detached');
  assert.throws(() => lib.parseArgs(['--spike-mode=cookie', '--spike-out=x', '--spike-server=s']), /spike-mode/);
  assert.throws(() => lib.parseArgs(['--spike-out=x']), /spike-server/);
  assert.throws(() => lib.parseArgs(['--spike-clicks=-1', '--spike-out=x', '--spike-server=s']), /whole number/);
});

test('the address line must be a loopback url and a 43-character token', () => {
  const parsed = lib.parseAddress(JSON.stringify({ url: `${ORIGIN}/`, token: TOKEN }));
  assert.deepEqual(parsed, { origin: ORIGIN, port: 40123, token: TOKEN });
  assert.throws(() => lib.parseAddress('listening'), /JSON/);
  assert.throws(() => lib.parseAddress(JSON.stringify({ url: 'http://0.0.0.0:1/', token: TOKEN })), /loopback/);
  assert.throws(() => lib.parseAddress(JSON.stringify({ url: `${ORIGIN}/`, token: 'short' })), /token/);
});

test("AD11's filter lists the paths outside /api/ and nothing under /api/", () => {
  const patterns = lib.filterPatterns(ORIGIN, 'filter');
  assert.deepEqual(patterns, [
    `${ORIGIN}/`,
    `${ORIGIN}/ui/*`,
    `${ORIGIN}/help/*`,
    `${ORIGIN}/media/*`,
    `${ORIGIN}/download/*`,
    `${ORIGIN}/print/*`,
    `${ORIGIN}/prepare.html`,
  ]);
  assert.ok(!patterns.some((p) => p.includes('/api')));
  assert.deepEqual(lib.filterPatterns(ORIGIN, 'filter-all'), [`${ORIGIN}/*`]);
});

test('the portless fallback adds the token only where the AD11 filter would', () => {
  assert.ok(lib.wantsToken(`${ORIGIN}/`, ORIGIN, 'filter'));
  assert.ok(lib.wantsToken(`${ORIGIN}/media/obs/01.png?r=1`, ORIGIN, 'filter'));
  assert.ok(lib.wantsToken(`${ORIGIN}/prepare.html`, ORIGIN, 'filter'));
  assert.ok(!lib.wantsToken(`${ORIGIN}/api/v1/align/click`, ORIGIN, 'filter'));
  assert.ok(!lib.wantsToken(`${ORIGIN}/?x=1`.replace('/?', '/api?'), ORIGIN, 'filter'));
  assert.ok(!lib.wantsToken('http://127.0.0.1:1/ui/probe.js', ORIGIN, 'filter'), 'another port');
  assert.ok(lib.wantsToken(`${ORIGIN}/api/v1/events`, ORIGIN, 'filter-all'));
});

test('origins of the custom scheme compare by scheme and host', () => {
  assert.equal(lib.originOf('tc4://app/help/child.html'), 'tc4://app');
  assert.ok(lib.sameOrigin('tc4://app/help/child.html', 'tc4://app'));
  assert.ok(!lib.sameOrigin('https://door43.org/', 'tc4://app'));
  assert.ok(lib.sameOrigin(`${ORIGIN}/download/1/a.txt`, ORIGIN));
  assert.ok(!lib.sameOrigin('not a url', ORIGIN));
});

test('the protocol proxy keeps the path and query and vouches for the origin', () => {
  assert.equal(lib.proxyTarget('tc4://app/media/obs/01.png?r=3', ORIGIN), `${ORIGIN}/media/obs/01.png?r=3`);
  assert.equal(lib.proxyTarget('tc4://app/', ORIGIN), `${ORIGIN}/`);
  const headers = lib.proxyHeaders(
    [
      ['Content-Type', 'application/json'],
      ['Host', 'app'],
      ['X-Tc-Token', 'forged'],
      ['Origin', 'tc4://app'],
    ],
    TOKEN,
    ORIGIN,
  );
  assert.deepEqual(headers, { 'content-type': 'application/json', origin: ORIGIN, 'x-tc-token': TOKEN });
});

test('the mirror keeps the last line per field, skips a torn line and honours removals', () => {
  const text = [
    JSON.stringify({ key: 'a', text: 'h', n: 1, at: 1 }),
    JSON.stringify({ key: 'b', text: 'x', n: 1, at: 2 }),
    JSON.stringify({ key: 'a', text: 'he', n: 2, at: 3 }),
    JSON.stringify({ key: 'b', removed: true }),
    '{"key":"a","text":"hel',
  ].join('\n');
  const parsed = lib.parseMirror(text);
  assert.equal(parsed.lines, 5);
  assert.equal(parsed.torn, 1);
  assert.deepEqual(parsed.entries, [{ key: 'a', text: 'he', n: 2, at: 3 }]);
  assert.deepEqual(lib.parseMirror(lib.compactMirror(parsed.entries)).entries, parsed.entries);
  assert.deepEqual(lib.parseMirror('').entries, []);
});
