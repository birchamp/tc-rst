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
/** The golden entry with no save in flight: one the static callout posts a draft for. */
const UNSENT = { ...ENTRY, sent: null };

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

test('input_writes_the_template_with_the_text_and_keeps_unknown_keys', () => {
  const entry = { ...ENTRY, later: { kept: true } };
  const local = storage({ [ITEM]: JSON.stringify(entry) });
  const { line, stored } = R.inputToEntry(local, ITEM, 'typed', 99, JSON.stringify({ ...ENTRY, text: '', at: 0 }));
  const kept = JSON.parse(local.getItem(ITEM));
  assert.equal(stored, true);
  assert.equal(kept.text, 'typed');
  assert.equal(kept.at, 99);
  assert.deepEqual(kept.later, { kept: true });
  assert.deepEqual(kept.sent, ENTRY.sent, 'the save in flight the template names stays named');
  assert.deepEqual(JSON.parse(line), { type: 'changed', value: { key: KEY, entry: kept } });
});

test('input_never_keeps_another_pages_save_or_tokens', () => {
  // Two tabs share the item: tab A's save is in flight under its tokens. Tab B's keystroke writes
  // B's own state, so a reload of B never sends A's request or saves over A's answer.
  const local = storage({ [ITEM]: JSON.stringify(ENTRY) });
  const other = 'st1.eyJoZWFkcyI6W10sImtleSI6eyJ4IjoxfX0AAAAAAAAAAAAAAAAAAAAA';
  const template = { ...ENTRY, base: [other], sent: null, text: '', at: 0 };
  const { stored } = R.inputToEntry(local, ITEM, 'B1', 5, JSON.stringify(template));
  const kept = JSON.parse(local.getItem(ITEM));
  assert.equal(stored, true);
  assert.equal(kept.sent, null);
  assert.deepEqual(kept.base, [other]);
  assert.equal(kept.text, 'B1');
});

test('input_without_an_entry_writes_the_template', () => {
  const local = storage();
  const template = { ...ENTRY, text: '', sent: null, at: 0 };
  assert.equal(R.inputToEntry(local, ITEM, 'first', 7, JSON.stringify(template)).stored, true);
  assert.deepEqual(JSON.parse(local.getItem(ITEM)), { ...template, text: 'first', at: 7 });
  assert.deepEqual(R.inputToEntry(local, ITEM, 'x', 8, 'not json'), { line: null, stored: false }, 'no template, no write');
});

test('input_leaves_another_runs_entry_and_a_damaged_one_alone_and_still_mirrors', () => {
  const other = { ...ENTRY, instance: '00000000000000000000000000000000' };
  const local = storage({ [ITEM]: JSON.stringify(other) });
  const template = { ...ENTRY, text: '', at: 0 };
  const result = R.inputToEntry(local, ITEM, 'x', 1, JSON.stringify(template));
  assert.equal(result.stored, false);
  assert.deepEqual(JSON.parse(local.getItem(ITEM)), other);
  assert.deepEqual(JSON.parse(result.line), { type: 'changed', value: { key: KEY, entry: { ...template, text: 'x', at: 1 } } });
  const damaged = storage({ [ITEM]: '{' });
  const again = R.inputToEntry(damaged, ITEM, 'y', 2, JSON.stringify(template));
  assert.equal(again.stored, false);
  assert.equal(damaged.getItem(ITEM), '{');
  assert.equal(JSON.parse(again.line).value.entry.text, 'y', 'the shell still gets the keystroke');
});

test('a_full_store_still_gives_the_mirror_its_line', () => {
  const local = storage({}, { blocked: true });
  const { line, stored } = R.inputToEntry(local, ITEM, 'kept by the shell', 3, JSON.stringify(ENTRY));
  assert.equal(stored, false, 'the WebAssembly then writes the entry itself');
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
  const otherRun = { ...UNSENT, instance: '00000000000000000000000000000000' };
  const otherFolder = { ...KEY, project: 'other' };
  const local = storage({
    [ITEM]: JSON.stringify(UNSENT),
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
  const local = storage({ [ITEM]: JSON.stringify(UNSENT) });
  const [request] = R.draftRequests(local, ENTRY.instance, () => 'abc');
  assert.deepEqual(JSON.parse(request.body), { ...JSON.parse(ENTRY.draft.body), request: 'abc', text: ENTRY.text });
});

test('an_entry_with_a_save_in_flight_gets_no_draft', () => {
  // Its tokens are those from before the save: a draft over them would stand beside the
  // person's own save as a conflict. The entry stays, with its save, for the offer (6.6).
  const local = storage({ [ITEM]: JSON.stringify(ENTRY) });
  assert.deepEqual(R.draftRequests(local, ENTRY.instance, () => 'id'), []);
  assert.deepEqual(JSON.parse(local.getItem(ITEM)), ENTRY, 'and it stays in the buffer');
});

test('a_store_the_browser_refuses_to_hand_out_is_handled_as_none', () => {
  // boot.js reaches localStorage and sessionStorage through a getter that gives null when the
  // browser throws from the getter itself (site data blocked).
  assert.equal(R.countPanic(null, 1000), 'stop', 'a page that cannot count stops at once');
  assert.deepEqual(R.calloutText(null), R.ENGLISH);
  assert.deepEqual(R.draftRequests(null, ENTRY.instance, () => 'id'), []);
  const { line, stored } = R.inputToEntry(null, ITEM, 'kept by the shell', 4, JSON.stringify(UNSENT));
  assert.equal(stored, false);
  assert.equal(JSON.parse(line).value.entry.text, 'kept by the shell', 'the mirror still gets it');
});

test('a_draft_call_to_another_place_is_not_posted', () => {
  for (const path of ['../evil', 'test/fieldDraft?x=1', 'http://example.com/x', '']) {
    const entry = { ...UNSENT, draft: { ...ENTRY.draft, path } };
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
  const arabic = { lang: 'ar', dir: 'rtl', title: 'تعذّر عرض هذه الشاشة.', home: 'العودة إلى الرئيسية' };
  assert.deepEqual(R.calloutText(storage({ [R.TEXT_KEY]: JSON.stringify(arabic) })), arabic);
  const odd = { ...arabic, dir: 'sideways' };
  assert.equal(R.calloutText(storage({ [R.TEXT_KEY]: JSON.stringify(odd) })).dir, 'ltr');
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
