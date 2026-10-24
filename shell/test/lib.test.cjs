'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const lib = require('../lib.cjs');

const TOKEN = 'A'.repeat(21) + '_-' + 'z'.repeat(20);
const ORIGIN = 'http://127.0.0.1:4100';

test('the_shell_arguments_name_tc4_its_ui_and_app_data', () => {
  const options = lib.parseArgs([
    '--no-sandbox', '--tc4', '/x/tc4', '--ui=/x/ui', '--app-data', '/x/data', '--', '--scenario', '/s',
  ]);
  assert.deepEqual(options, { tc4: '/x/tc4', ui: '/x/ui', appData: '/x/data', bundle: null, extra: ['--scenario', '/s'] });
  assert.deepEqual(lib.serveArgs(options), ['serve', '--app-data', '/x/data', '--ui', '/x/ui', '--scenario', '/s']);
  assert.deepEqual(
    lib.serveArgs(lib.parseArgs(['--tc4', 't', '--ui', 'u', '--app-data', 'd', '--bundle', 'b'])),
    ['serve', '--app-data', 'd', '--ui', 'u', '--bundle', 'b'],
  );
  assert.throws(() => lib.parseArgs(['--tc4', 't', '--ui', 'u']), /--app-data is needed/);
  assert.throws(() => lib.parseArgs(['--tc4']), /--tc4 needs a value/);
});

test('the_address_line_is_parsed_per_the_contract', () => {
  assert.deepEqual(lib.parseAddress(JSON.stringify({ url: 'http://127.0.0.1:4100/', token: TOKEN })), {
    address: { origin: ORIGIN, port: 4100, token: TOKEN },
  });
  assert.deepEqual(lib.parseAddress('{"error":{"type":"appLockHeld","message":"held by 12"}}'), {
    error: { type: 'appLockHeld', message: 'held by 12' },
  });
  assert.equal(lib.parseAddress('{"error":{"type":"start"}}').error.type, 'start');
  assert.deepEqual(lib.parseAddress('{"error":{"type":"start","value":{"reason":"the projects folder cannot be used"}}}'), {
    error: { type: 'start', message: 'the projects folder cannot be used' },
  });
  assert.equal(lib.parseAddress('not json').error.type, 'garbled');
  assert.equal(lib.parseAddress(JSON.stringify({ url: 'http://0.0.0.0:1/', token: TOKEN })).error.type, 'garbled');
  assert.equal(lib.parseAddress(JSON.stringify({ url: 'http://127.0.0.1:1/', token: 'short' })).error.type, 'garbled');
  assert.match(lib.exitReason(3, null), /holds the app data folder/);
  assert.match(lib.exitReason(4, null), /could not start/);
  assert.match(lib.exitReason(2, null), /wrong arguments/);
  assert.match(lib.exitReason(1, null), /server error/);
  assert.match(lib.exitReason(null, 'SIGKILL'), /SIGKILL/);
});

test('filter_adds_the_token_only_outside_api_on_the_tc4_origin', () => {
  const yes = ['/', '/prepare.html', '/ui/abc/app.js', '/help/x.html', '/media/a.png', '/download/f', '/print/p'];
  const no = ['/api/v1/app/hello', '/api/v1/events', '/apix', '/other', '/ui', '/prepare.htmlx'];
  for (const path of yes) assert.ok(lib.wantsToken(`${ORIGIN}${path}`, ORIGIN), path);
  for (const path of no) assert.ok(!lib.wantsToken(`${ORIGIN}${path}`, ORIGIN), path);
  for (const url of ['http://127.0.0.1:4101/', 'https://127.0.0.1:4100/', 'http://localhost:4100/', 'https://door43.org/ui/']) {
    assert.ok(!lib.wantsToken(url, ORIGIN), url);
  }
  // Electron's patterns decide first: none of them may reach /api/.
  // Electron's patterns decide first: each one Electron matches against the path and the query.
  const patterns = lib.filterPatterns(ORIGIN).map((pattern) => {
    assert.ok(pattern.startsWith(`${ORIGIN}/`), pattern);
    return new RegExp(`^${pattern.replace(/[.?+^$()|[\]\\{}]/g, '\\$&').replace(/\*/g, '.*')}$`);
  });
  const matched = (url) => patterns.some((re) => re.test(url));
  for (const path of yes) {
    assert.ok(matched(`${ORIGIN}${path}`), path);
    assert.ok(matched(`${ORIGIN}${path}?probe=fields`), `${path} with a query`);
  }
  assert.ok(!matched(`${ORIGIN}/api/v1/app/hello`) && !matched(`${ORIGIN}/api/v1/events?after=3`));
});

test('token_is_redacted_from_every_log_line', () => {
  const line = `load failed: ${ORIGIN}/#t=${TOKEN} and ${TOKEN} again, and key K3Y`;
  const out = lib.redact(line, [TOKEN, 'K3Y']);
  assert.ok(!out.includes(TOKEN), out);
  assert.ok(!out.includes('K3Y'), out);
  assert.match(out, /#t=<token>/);
  assert.equal(lib.redact('nothing here', [null, '']), 'nothing here');
});

test('links_on_the_app_origin_stay_in_the_app', () => {
  assert.equal(lib.linkTarget(`${ORIGIN}/help/x.html`, ORIGIN), 'app');
  assert.equal(lib.linkTarget('https://door43.org/u/x', ORIGIN), 'browser');
  assert.equal(lib.linkTarget('http://127.0.0.1:4101/', ORIGIN), 'browser');
  for (const url of ['file:///etc/passwd', 'javascript:alert(1)', 'tc4://app/', 'not a url']) {
    assert.equal(lib.linkTarget(url, ORIGIN), 'refuse', url);
  }
});
