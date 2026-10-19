// The panic recovery's pure parts (`recovery.js`), in node with its built-in test runner:
//
//   node --test crates/tc-ui/app/recovery.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';

import * as R from './recovery.js';

const here = path.dirname(new URL(import.meta.url).pathname);
const golden = JSON.parse(fs.readFileSync(path.join(here, '../../tc-api/tests/golden/types/buffer.json'), 'utf8'));

/** sessionStorage or localStorage, in memory; `blocked` makes every write throw. */
function storage(entries = {}, { blocked = false } = {}) {
  const items = new Map(Object.entries(entries));
  return {
    items,
    get length() {
      return items.size;
    },
    key: (i) => [...items.keys()][i] ?? null,
    getItem: (k) => (items.has(k) ? items.get(k) : null),
    setItem: (k, v) => {
      if (blocked) throw new Error('QuotaExceededError');
      items.set(k, String(v));
    },
    removeItem: (k) => items.delete(k),
  };
}

const KEY = golden.keys[0];
const ITEM = golden.itemKeys[0];
const ENTRY = golden.entries[0];

test('three_page_loads_in_10_s_stop_the_reloads', () => {
  const session = storage();
  assert.equal(R.countPanic(session, 1000), 'reload');
  assert.equal(R.countPanic(session, 2000), 'reload');
  assert.equal(R.countPanic(session, 3000), 'stop');
  assert.deepEqual(JSON.parse(session.getItem(R.PANICS_KEY)), [1000, 2000, 3000]);
});

test('an_old_panic_does_not_count', () => {
  const session = storage({ [R.PANICS_KEY]: JSON.stringify([1000, 2000]) });
  assert.equal(R.countPanic(session, 12500), 'reload', 'both earlier loads are over 10 s old');
  assert.deepEqual(JSON.parse(session.getItem(R.PANICS_KEY)), [12500]);
  assert.equal(R.panicDecision('not json', 5).action, 'reload');
  assert.deepEqual(R.panicDecision('[1, "x", null]', 5).panics, [1, 5]);
});

test('a_blocked_store_stops_at_once', () => {
  assert.equal(R.countPanic(storage({}, { blocked: true }), 1000), 'stop');
  // A store that drops what it is given cannot count either.
  const forgetful = { ...storage(), getItem: () => null, setItem: () => {} };
  assert.equal(R.countPanic(forgetful, 1000), 'stop');
});

test('input_updates_only_text_and_at_and_keeps_unknown_keys', () => {
  const entry = { ...ENTRY, later: { kept: true } };
  const local = storage({ [ITEM]: JSON.stringify(entry) });
  const line = R.inputToEntry(local, ITEM, 'typed', 99, JSON.stringify({ ...ENTRY, text: '', sent: null }));
  const stored = JSON.parse(local.getItem(ITEM));
  assert.equal(stored.text, 'typed');
  assert.equal(stored.at, 99);
  assert.deepEqual(stored.later, { kept: true });
  assert.deepEqual(stored.sent, ENTRY.sent, 'the save in flight stays named');
  assert.deepEqual(JSON.parse(line), { type: 'changed', value: { key: KEY, entry: stored } });
});

test('input_without_an_entry_writes_the_template', () => {
  const local = storage();
  const template = { ...ENTRY, text: '', sent: null, at: 0 };
  R.inputToEntry(local, ITEM, 'first', 7, JSON.stringify(template));
  assert.deepEqual(JSON.parse(local.getItem(ITEM)), { ...template, text: 'first', at: 7 });
  assert.equal(R.inputToEntry(local, ITEM, 'x', 8, 'not json'), null, 'no template, no write');
});

test('input_leaves_another_runs_entry_and_a_damaged_one_alone', () => {
  const other = { ...ENTRY, instance: '00000000000000000000000000000000' };
  const local = storage({ [ITEM]: JSON.stringify(other) });
  assert.equal(R.inputToEntry(local, ITEM, 'x', 1, JSON.stringify(ENTRY)), null);
  assert.deepEqual(JSON.parse(local.getItem(ITEM)), other);
  const damaged = storage({ [ITEM]: '{' });
  assert.equal(R.inputToEntry(damaged, ITEM, 'x', 1, JSON.stringify(ENTRY)), null);
  assert.equal(damaged.getItem(ITEM), '{');
});

test('a_full_store_still_gives_the_mirror_its_line', () => {
  const local = storage({}, { blocked: true });
  const line = R.inputToEntry(local, ITEM, 'kept by the shell', 3, JSON.stringify(ENTRY));
  assert.equal(JSON.parse(line).value.entry.text, 'kept by the shell');
});

test('the_line_matches_tc_api_golden', () => {
  for (const line of golden.lines.filter((l) => l.type === 'changed')) {
    const keyText = JSON.stringify(line.value.key);
    const built = R.changedLine(keyText, JSON.stringify(line.value.entry));
    assert.deepEqual(JSON.parse(built), line);
  }
  assert.equal(golden.itemKeys[0], R.BUFFER_PREFIX + JSON.stringify(golden.keys[0]));
});

test('drafts_only_of_the_current_run_and_with_a_draft_call', () => {
  const otherRun = { ...ENTRY, instance: '00000000000000000000000000000000' };
  const otherFolder = { ...KEY, project: 'other' };
  const local = storage({
    [ITEM]: JSON.stringify(ENTRY),
    [golden.itemKeys[1]]: JSON.stringify(golden.entries[1]),
    [R.BUFFER_PREFIX + JSON.stringify(otherFolder)]: JSON.stringify(otherRun),
    'tc4.token': 'x',
    [`${R.BUFFER_PREFIX}garbage`]: '{}',
  });
  let n = 0;
  const requests = R.draftRequests(local, ENTRY.instance, () => `id${(n += 1)}`);
  assert.equal(requests.length, 1, 'one entry of this run has a draft call');
  assert.equal(requests[0].path, ENTRY.draft.path);
  assert.deepEqual(R.draftRequests(local, null, () => 'id'), [], 'no run known, no drafts');
});

test('a_draft_request_adds_request_and_text_and_keeps_the_rest', () => {
  const local = storage({ [ITEM]: JSON.stringify(ENTRY) });
  const [request] = R.draftRequests(local, ENTRY.instance, () => 'abc');
  assert.deepEqual(JSON.parse(request.body), { ...JSON.parse(ENTRY.draft.body), request: 'abc', text: ENTRY.text });
});

test('a_draft_call_to_another_place_is_not_posted', () => {
  for (const path of ['../evil', 'test/fieldDraft?x=1', 'http://example.com/x', '']) {
    const entry = { ...ENTRY, draft: { ...ENTRY.draft, path } };
    assert.deepEqual(R.draftRequests(storage({ [ITEM]: JSON.stringify(entry) }), ENTRY.instance, () => 'id'), [], path);
  }
});

/** A document whose elements record their attributes and children. */
function fakeDocument() {
  const make = (tag) => ({
    tag,
    attributes: [],
    children: [],
    setAttribute(name, value) {
      this.attributes.push([name, value]);
    },
    appendChild(child) {
      this.children.push(child);
      return child;
    },
  });
  return { createElement: make, createTextNode: (text) => ({ text }) };
}

function texts(node) {
  if ('text' in node) return [node.text];
  return node.children.flatMap(texts);
}

test('the_callout_sets_every_text_by_textContent', () => {
  const message = '<img src=x onerror=alert(1)>';
  const { root, button } = R.buildStaticCallout(fakeDocument(), R.ENGLISH, message);
  assert.deepEqual(texts(root), ['This screen could not be shown.', ' ', message, ' ', 'Back to Home']);
  const callout = root.children[0];
  assert.deepEqual(Object.fromEntries(callout.attributes).role, 'alert');
  assert.deepEqual(Object.fromEntries(callout.attributes)['data-testid'], 'view-error');
  assert.equal(Object.fromEntries(button.attributes)['data-testid'], 'view-error-home');
  assert.ok(!JSON.stringify(root).includes('innerHTML'));
});

test('english_when_no_text_was_kept', () => {
  assert.deepEqual(R.calloutText(storage()), R.ENGLISH);
  assert.deepEqual(R.calloutText(storage({ [R.TEXT_KEY]: '{' })), R.ENGLISH);
  const arabic = { lang: 'ar', title: 'تعذّر عرض هذه الشاشة.', home: 'العودة إلى الرئيسية' };
  assert.deepEqual(R.calloutText(storage({ [R.TEXT_KEY]: JSON.stringify(arabic) })), arabic);
  assert.equal(R.ENGLISH.title, JSON.parse(fs.readFileSync(path.join(here, '../assets/i18n/en.json'), 'utf8'))['app.viewError']);
});

test('a_trap_is_a_webassembly_runtime_error', () => {
  assert.ok(R.isTrap(new WebAssembly.RuntimeError('unreachable')));
  assert.ok(R.isTrap({ name: 'RuntimeError', message: 'unreachable' }), 'from another realm');
  assert.ok(!R.isTrap(new Error('x')));
  assert.ok(!R.isTrap(null));
});

test('a_random_id_is_32_hex_digits', () => {
  const id = R.randomId(globalThis.crypto);
  assert.match(id, /^[0-9a-f]{32}$/);
  assert.notEqual(id, R.randomId(globalThis.crypto));
});
