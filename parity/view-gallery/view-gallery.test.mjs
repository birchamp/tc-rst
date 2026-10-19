// The tC4 view-state gallery (APP-DESIGN 7.4).
//
//   node --test parity/view-gallery/view-gallery.test.mjs
//   TC4_ROOT=<tC4 clone with node_modules> node --test parity/view-gallery/view-gallery.test.mjs
//
// The first groups need nothing but Node and run in CI: the pure helpers, and the committed
// record against itself (states.json, calibration.json, parity/app/cases.json and the files
// they name). The browser group needs a tC4 clone with node_modules (TC4_ROOT) and Playwright
// 1.56.1 (parity/tc4-baseline/node_modules); without them each of its tests is skipped and
// says why. Only Node built-ins are imported at the top.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { SCREEN_LIST } from '../app/screens.mjs';
import { galleryGroups } from '../app/units.mjs';
import { stateExportsSource, stateStub } from './build.mjs';
import { CASES, caseSteps } from './cases.mjs';
import { applyPatch, domDigestInput, encodeValue, makeActions, renumberIds, reviveValue, traceArgs } from './lib.mjs';
import { HERE, REPO, STATES_DIR, filesUnder, joinParts, partsOf, sha256, splitParts } from './node-lib.mjs';
import { STATES } from './states.mjs';
import { stepProblem } from './steps.mjs';

const readJson = (file) => JSON.parse(fs.readFileSync(file, 'utf8'));
const statesIndex = readJson(path.join(HERE, 'states.json'));
const casesFile = readJson(path.join(REPO, 'parity', 'app', 'cases.json'));
const calibration = readJson(path.join(HERE, 'calibration.json'));
const masks = readJson(path.join(REPO, 'parity', 'app', 'masks.json')).masks;
const groups = galleryGroups(fs.readFileSync(path.join(REPO, 'docs', 'APP-DESIGN.md'), 'utf8'));

// ---- the pure helpers ----------------------------------------------------------------------

test('encodeValue carries plain JSON, Uint8Array and symbols, and refuses everything else with its path', () => {
  const plain = { a: 1, b: 'x', c: [true, null, { d: -2.5 }], e: {}, 'f-g': [] };
  const shared = { n: 1 };
  const { value, refused } = encodeValue({ plain, twice: [shared, shared] });
  assert.deepEqual(refused, []);
  assert.deepEqual(JSON.parse(JSON.stringify(value)), { plain, twice: [{ n: 1 }, { n: 1 }] });
  assert.deepEqual(reviveValue(JSON.parse(JSON.stringify(value))), { plain, twice: [{ n: 1 }, { n: 1 }] });

  const bytes = Uint8Array.from({ length: 256 }, (_, i) => i);
  const encoded = encodeValue({ files: [{ name: 'x.usfm', bytes }] });
  assert.deepEqual(encoded.refused, []);
  assert.equal(typeof encoded.value.files[0].bytes.$u8, 'string');
  const back = reviveValue(JSON.parse(JSON.stringify(encoded.value)));
  assert.ok(back.files[0].bytes instanceof Uint8Array);
  assert.deepEqual([...back.files[0].bytes], [...bytes]);

  const marked = encodeValue({ si: { opened: Symbol('sign-in step') } });
  assert.deepEqual(marked, { value: { si: { opened: { $symbol: 'sign-in step' } } }, refused: [] });
  const opened = reviveValue(JSON.parse(JSON.stringify(marked.value))).si.opened;
  assert.equal(typeof opened, 'symbol');
  assert.equal(opened.description, 'sign-in step');

  const cycle = { a: { b: [0, 1, null] } };
  cycle.a.b[2] = cycle;
  class X {}
  for (const [bad, path, why] of [
    [{ a: { b: [0, 1, undefined] } }, 's.a.b[2]', 'undefined'],
    [{ a: { b: undefined } }, 's.a.b', 'undefined'],
    [{ a: { b: [0, 1, () => 1] } }, 's.a.b[2]', 'a function'],
    [{ a: { b: [0, 1, 10n] } }, 's.a.b[2]', 'a bigint'],
    [{ a: { b: [0, 1, Number.NaN] } }, 's.a.b[2]', 'the number NaN'],
    [{ a: { b: [0, 1, Infinity] } }, 's.a.b[2]', 'the number Infinity'],
    [{ a: { b: [0, 1, new Map()] } }, 's.a.b[2]', 'a Map instance'],
    [{ a: { b: [0, 1, new Set()] } }, 's.a.b[2]', 'a Set instance'],
    [{ a: { b: [0, 1, new Date(0)] } }, 's.a.b[2]', 'a Date instance'],
    [{ a: { b: [0, 1, new X()] } }, 's.a.b[2]', 'a X instance'],
    [cycle, 's.a.b[2]', 'a cycle'],
    [{ a: { b: [0, 1, 'blob:x'] } }, 's.a.b[2]', 'a blob: URL'],
    [{ a: { b: [0, 1, { $u8: 'x' }] } }, 's.a.b[2]', 'a plain object whose only key is the tag $u8'],
    [{ a: { b: [0, 1, { $part: 'x' }] } }, 's.a.b[2]', 'a plain object whose only key is the tag $part'],
    [{ a: { b: [0, 1, { $symbol: 'x' }] } }, 's.a.b[2]', 'a plain object whose only key is the tag $symbol'],
  ]) {
    assert.deepEqual(encodeValue(bad).refused, [`${path}: ${why}`], why);
  }
  assert.deepEqual(encodeValue({ 'a b': undefined }).refused, ['s["a b"]: undefined']);
  // Self-contained: the source alone runs, as the recorder ships it to tC4's page.
  const shipped = new Function(`return (${String(encodeValue)})`)();
  assert.deepEqual(shipped({ a: [1, new Map()] }, 'value').refused, ['value.a[1]: a Map instance']);
});

test('applyPatch replaces top-level keys as tC4\'s reducer set does', () => {
  const kept = { deep: { x: 1 } };
  const s = { sh: { step: 'check', name: 'a', busy: false }, kept };
  const next = applyPatch(s, { sh: { step: 'done' } });
  assert.deepEqual(next.sh, { step: 'done' }, 'a nested slice is replaced, not merged');
  assert.equal(next.kept, kept, 'an untouched key keeps its identity');
  assert.notEqual(next, s);
  assert.deepEqual(s.sh, { step: 'check', name: 'a', busy: false }, 'the recorded state is not changed');
});

test('the actions object keeps one function per name and answers selectors, replies and nothing', async () => {
  const calls = [];
  const s = { installedSrc: [{ langKey: 'en' }, { langKey: 'en' }, { langKey: 'fr' }] };
  const actions = makeActions({
    s,
    returns: { v: { value: 7 }, ok: { resolves: { a: 1 } }, no: { rejects: 'refused' }, wait: { pending: true } },
    selectors: { count: (state, [key]) => state.installedSrc.filter((x) => x.langKey === key).length },
    record: (call) => calls.push(call),
  });
  assert.equal(actions.x, actions.x);
  assert.equal(actions.then, undefined);
  assert.equal(actions[Symbol.iterator], undefined);
  assert.equal(actions.x(1, 'a'), undefined);
  assert.equal(actions.v(), 7);
  assert.deepEqual(await actions.ok(), { a: 1 });
  await assert.rejects(actions.no(), /^Error: refused$/);
  const settled = await Promise.race([actions.wait().then(() => 'settled'), new Promise((r) => setTimeout(() => r('pending'), 50))]);
  assert.equal(settled, 'pending');
  assert.equal(actions.count('en'), 2);
  assert.deepEqual(calls.map((c) => [c.name, c.args]), [['x', [1, 'a']], ['v', []], ['ok', []], ['no', []], ['wait', []], ['count', ['en']]]);
});

test('traceArgs stores events, nodes, functions, cycles and depth as tags', () => {
  const event = { nativeEvent: {}, type: 'click', target: null };
  event.target = event;
  const node = { nodeType: 1, nodeName: 'BUTTON', ownerDocument: {} };
  const looped = { a: 1 };
  looped.self = looped;
  const deep = { l1: { l2: { l3: { l4: { l5: { l6: { l7: 'x' } } } } } } };
  assert.deepEqual(traceArgs([event, node, () => 1, undefined, new Uint8Array(3), looped, deep, Number.NaN, new Map(), 'text', 3, null]), [
    { $event: 'click' },
    { $node: 'BUTTON' },
    { $fn: true },
    { $undefined: true },
    { $u8len: 3 },
    { a: 1, self: { $cycle: true } },
    { l1: { l2: { l3: { l4: { l5: { l6: { $deep: true } } } } } } },
    { $number: 'NaN' },
    { $instance: 'Map' },
    'text',
    3,
    null,
  ]);
});

test('renumberIds renumbers each prefix by first appearance and leaves other ids alone', () => {
  assert.deepEqual(renumberIds([
    '<div id="tc-dlg2" aria-labelledby="tc-dlg2-title">',
    '<input id="tc-f7" aria-describedby="tc-f12-list tc-f7-help">',
    '<p id="tc-dlg2">',
    '<x id="tc-x3" class="other-f1" data-a="xtc-f1">',
    '<y id="tc-cfm4 tc-rg9 tc-g3">',
  ]), [
    '<div id="tc-dlg#1" aria-labelledby="tc-dlg#1-title">',
    '<input id="tc-f#1" aria-describedby="tc-f#2-list tc-f#1-help">',
    '<p id="tc-dlg#1">',
    '<x id="tc-x3" class="other-f1" data-a="xtc-f1">',
    '<y id="tc-cfm#1 tc-rg#1 tc-g#1">',
  ]);
  const a = { html: '<html lang="en">', outside: [], root: ['<div id="tc-dlg2">'], styles: {} };
  const b = { ...a, root: ['<div id="tc-dlg1">'] };
  assert.equal(domDigestInput(a), domDigestInput(b), 'two snapshots that differ only by a counter digest alike');
  assert.notEqual(domDigestInput(a), domDigestInput({ ...a, root: ['<div id="tc-dlg1" class="x">'] }));
});

test('the stub takes tC4\'s own SCRIPT_FONTS and isOldTestament, and the build refuses tC4\'s state.jsx', async () => {
  const fonts = "export const SCRIPT_FONTS = [\n  'A',\n  'B',\n];";
  const fn = 'export function isOldTestament(bookCode) {\n  return true;\n}';
  const source = `const x = 1;\n${fonts}\n\nfunction other() {}\n${fn}\n`;
  assert.equal(stateExportsSource(source), `import { BOOK_NAMES } from '@tc4/src/data/bookNames';\n${fonts}\n${fn}\n`);
  assert.throws(() => stateExportsSource(`${source}${fonts}\n`), /2 definitions of SCRIPT_FONTS/);
  assert.throws(() => stateExportsSource(fonts), /0 definitions of isOldTestament/);

  const plugin = stateStub('/tc4', '/gallery/stub.jsx');
  assert.throws(() => plugin.load('/tc4/src/state.jsx'), /was loaded: an import reached it past the stub/);
  assert.throws(() => plugin.load('/tc4/src/state.jsx?import'), /past the stub/);
  assert.equal(plugin.load('/tc4/src/App.jsx'), null);
  const context = { resolve: async (source) => ({ id: source === '../state.jsx' ? '/tc4/src/state.jsx' : '/tc4/src/views/state.jsx' }) };
  assert.equal(await plugin.resolveId.call(context, '../state.jsx', '/tc4/src/views/Home.jsx', {}), '/gallery/stub.jsx');
  assert.equal(await plugin.resolveId.call(context, './state', '/tc4/src/views/Home.jsx', {}), null, 'another file named state stays');
  assert.equal(await plugin.resolveId.call(context, 'react', '/tc4/src/App.jsx', {}), null);
});

// ---- the recorded states ---------------------------------------------------------------------

test('parts are content-addressed and shared', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'gallery-parts-'));
  try {
    const big = { chapters: Array.from({ length: 70 }, (_, i) => 'x'.repeat(1000) + i) };
    const a = { view: 'draft', sources: big, small: { text: 'y'.repeat(10 * 1024) } };
    const b = { view: 'check', sources: structuredClone(big) };
    const sa = splitParts(a);
    const sb = splitParts(b);
    assert.deepEqual(Object.keys(sa.parts), Object.keys(sb.parts), 'one value, one part');
    assert.equal(Object.keys(sa.parts).length, 1);
    const [id] = Object.keys(sa.parts);
    assert.match(id, /^[0-9a-f]{16}$/);
    assert.ok(sha256(sa.parts[id]).startsWith(id));
    assert.deepEqual(sa.s.sources, { $part: id });
    assert.deepEqual(sa.s.small, a.small, 'a 10 KiB value stays inline');
    assert.deepEqual(partsOf(sa.s), [id]);
    fs.writeFileSync(path.join(dir, `${id}.json`), sa.parts[id]);
    assert.deepEqual(joinParts(sa.s, dir), a);
    assert.deepEqual(joinParts(sb.s, dir), b);
    fs.writeFileSync(path.join(dir, `${id}.json`), sa.parts[id].replace('x', 'z'));
    assert.throws(() => joinParts(sa.s, dir), /does not have the sha256 its name gives/);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('states.json names every state file and part, and nothing else is in states/', () => {
  const named = new Set();
  for (const entry of statesIndex.states) {
    const text = fs.readFileSync(path.join(HERE, entry.file));
    assert.equal(sha256(text), entry.sha256, `${entry.file} is not the file states.json names`);
    const file = JSON.parse(text);
    assert.equal(file.name, entry.name);
    assert.deepEqual(partsOf(file.s), entry.parts, `${entry.file} names other parts than states.json`);
    named.add(path.relative(STATES_DIR, path.join(HERE, entry.file)));
    for (const id of entry.parts) {
      const part = fs.readFileSync(path.join(STATES_DIR, 'parts', `${id}.json`));
      assert.ok(sha256(part).startsWith(id), `part ${id} does not have the sha256 its name gives`);
      named.add(`parts/${id}.json`);
    }
    assert.match(entry.real.png, /^[0-9a-f]{64}$/);
    assert.match(entry.real.dom, /^[0-9a-f]{64}$/);
  }
  assert.deepEqual(filesUnder(STATES_DIR), [...named].sort(), 'a file under states/ that no state names, or a named file missing');
  assert.match(statesIndex.tc4.commit, /^[0-9a-f]{40}$/);
});

test('every state of states.mjs is recorded, as states.mjs describes it', () => {
  assert.deepEqual(statesIndex.states.map((e) => e.name), STATES.map((s) => s.name));
  for (const state of STATES) {
    const entry = statesIndex.states.find((e) => e.name === state.name);
    const { scenario, locale, baseline, portable, calls } = state;
    assert.deepEqual(
      { scenario: entry.scenario, locale: entry.locale, baseline: entry.baseline, portable: entry.portable, calls: entry.calls },
      { scenario, locale, baseline, portable, calls },
      `states.json's entry of ${state.name} is not what states.mjs records: record it again`,
    );
  }
  assert.equal(new Set(STATES.map((s) => s.name)).size, STATES.length);
});

// ---- the cases and their record --------------------------------------------------------------

test('every case names a recorded state, a §7.4 group and a unique name', () => {
  const names = CASES.map((c) => c.name);
  assert.equal(new Set(names).size, names.length, 'two cases share a name');
  assert.equal(groups.length, 18);
  for (const c of CASES) {
    assert.match(c.name, /^[a-z0-9]+(-[a-z0-9]+)*$/, `${c.name} is not kebab-case`);
    assert.ok(!SCREEN_LIST.some((s) => s.name === c.name), `case ${c.name} is named like a baseline screen`);
    assert.ok(STATES.some((s) => s.name === c.state), `case ${c.name} starts from ${c.state}, which states.mjs does not record`);
    const group = groups.find((g) => g.group === c.group);
    assert.ok(group, `case ${c.name}: group ${c.group} is not in §7.4`);
    assert.equal(c.scope !== undefined, group.dom !== group.pixels, `case ${c.name}: a scope exactly when its group is DOM-first (${group.dom}, ${group.pixels})`);
    if (c.scope) assert.deepEqual(Object.keys(c.scope).sort(), ['attr', 'value', 'which']);
    for (const step of caseSteps(c)) assert.equal(stepProblem(step), null, `case ${c.name}`);
    for (const id of c.masks ?? []) assert.ok(masks.some((m) => m.id === id), `case ${c.name} names the mask ${id}, which masks.json does not have`);
    assert.ok(typeof c.why === 'string' && c.why.length > 20, `case ${c.name} says nothing of what it shows`);
    assert.ok(c.patch === undefined || typeof c.patch === 'function');
  }
  for (const g of groups) assert.ok(CASES.some((c) => c.group === g.group), `§7.4 group ${g.group} has no case`);
});

test('cases.json agrees with cases.mjs, §7.4 and the files', () => {
  assert.deepEqual(casesFile.cases.map((e) => e.name), CASES.map((c) => c.name), 'cases.json is not every case of cases.mjs in order: capture again');
  assert.match(casesFile.tc4.commit, /^[0-9a-f]{40}$/);
  assert.equal(casesFile.tc4.commit, statesIndex.tc4.commit, 'the cases were captured from another tC4 than the states were recorded from');
  assert.deepEqual(casesFile.machine, calibration.machine, 'the cases and the calibration come from two machines');
  const named = new Set();
  for (const c of CASES) {
    const entry = casesFile.cases.find((e) => e.name === c.name);
    const state = STATES.find((s) => s.name === c.state);
    const group = groups.find((g) => g.group === c.group);
    assert.deepEqual(
      [entry.group, entry.state, entry.scenario, entry.locale, entry.domUnit, entry.pixelsUnit, entry.scope],
      [c.group, c.state, state.scenario, state.locale, group.dom, group.pixels, c.scope ?? null],
      `cases.json's entry of ${c.name} is not what cases.mjs and §7.4 give: capture again`,
    );
    assert.equal(entry.patch.length > 0, c.patch !== undefined, `${c.name}: the keys its patch sets`);
    for (const [file, sha] of [[entry.png, entry.sha256], [entry.dom, entry.domSha256], [entry.actions, entry.actionsSha256]]) {
      assert.equal(sha256(fs.readFileSync(path.join(HERE, file))), sha, `parity/view-gallery/${file} is not the file cases.json names`);
      named.add(file);
    }
    const dom = readJson(path.join(HERE, entry.dom));
    assert.equal(dom.screen, c.name);
    assert.ok(dom.root.length > 1 && dom.root[1].trimStart().startsWith('<'), `${entry.dom}: nothing was drawn under #root`);
    const trace = readJson(path.join(HERE, entry.actions));
    assert.equal(trace.case, c.name);
    assert.ok(Array.isArray(trace.calls) && trace.calls.every((call) => typeof call.name === 'string' && typeof call.at === 'string'));
  }
  for (const dir of ['screens', 'dom', 'actions']) {
    for (const rel of filesUnder(path.join(HERE, dir))) assert.ok(named.has(`${dir}/${rel}`), `parity/view-gallery/${dir}/${rel} belongs to no case`);
  }
  // A mask's rectangle on a case: exactly the cases that show its element.
  for (const mask of masks) {
    for (const name of Object.keys(mask.rects)) {
      if (SCREEN_LIST.some((s) => s.name === name)) continue;
      const c = CASES.find((x) => x.name === name);
      assert.ok(c && (c.masks ?? []).includes(mask.id), `masks.json gives mask ${mask.id} a rectangle on ${name}, which does not show it`);
    }
  }
  for (const c of CASES) for (const id of c.masks ?? []) assert.ok(masks.find((m) => m.id === id).rects[c.name], `mask ${id} has no rectangle on ${c.name}`);
});

test('calibration.json shows every state renders as tC4 rendered it', () => {
  assert.deepEqual(calibration.states.map((e) => e.name), statesIndex.states.map((e) => e.name));
  for (const entry of calibration.states) {
    const state = statesIndex.states.find((e) => e.name === entry.name);
    assert.equal(entry.equal, true, `${entry.name} does not draw as tC4 drew it`);
    assert.deepEqual([entry.png, entry.dom], [state.real.png, state.real.dom], `${entry.name}: calibration.json disagrees with states.json`);
  }
});

test('a reply file names its case and holds tc-api replies', () => {
  const dir = path.join(HERE, 'replies');
  for (const rel of filesUnder(dir)) {
    const name = rel.replace(/\.json$/, '');
    assert.ok(rel.endsWith('.json') && CASES.some((c) => c.name === name), `replies/${rel} names no case`);
    const file = readJson(path.join(dir, rel));
    assert.equal(file.case, name);
    assert.ok(Array.isArray(file.replies));
    for (const r of file.replies) {
      assert.match(r.endpoint, /^[a-z][a-zA-Z]*\/[a-zA-Z][\w-]*$/, `replies/${rel}: an endpoint is <service>/<name>`);
      assert.ok(['memory', 'hand'].includes(r.source), `replies/${rel}: source is memory or hand`);
      assert.ok('request' in r && 'reply' in r);
    }
  }
});

// ---- with a tC4 clone and a browser -----------------------------------------------------------

function browserSetup() {
  const root = process.env.TC4_ROOT;
  if (!root || !fs.existsSync(path.join(root, 'node_modules', 'vite', 'package.json'))) return { skip: 'set TC4_ROOT to a tC4 clone with node_modules (parity/view-gallery/README.md, "Setup")' };
  const playwright = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
  if (!fs.existsSync(path.join(playwright, 'package.json'))) return { skip: 'Playwright 1.56.1 is not installed: npm ci in parity/tc4-baseline' };
  return {};
}

const runs = [];
const capture = (args) => spawnSync(process.execPath, [path.join(HERE, 'capture.mjs'), ...args], { encoding: 'utf8', env: process.env, timeout: 30 * 60_000 });

test('every case renders with no console error', { timeout: 30 * 60_000, skip: browserSetup().skip }, () => {
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'gallery-run1-'));
  const run = capture(['--out', out]);
  assert.equal(run.status, 0, `capture.mjs failed:\n${run.stdout.slice(-4000)}\n${run.stderr.slice(-2000)}`);
  assert.doesNotMatch(run.stdout, /FAILED|DIFFERS/);
  const cases = readJson(path.join(out, 'cases.json')).cases;
  assert.equal(cases.length, CASES.length);
  for (const entry of cases) {
    assert.ok(fs.statSync(path.join(out, entry.png)).size > 0);
    const dom = readJson(path.join(out, entry.dom));
    assert.ok(dom.root.length > 1, `${entry.name}: nothing under #root`);
    assert.ok(fs.existsSync(path.join(out, entry.actions)));
  }
  runs.push(out);
});

test('two captures are byte-identical', { timeout: 30 * 60_000, skip: browserSetup().skip }, () => {
  assert.equal(runs.length, 1, 'the first capture did not run');
  const out = fs.mkdtempSync(path.join(os.tmpdir(), 'gallery-run2-'));
  const run = capture(['--no-build', '--out', out]);
  assert.equal(run.status, 0, run.stdout.slice(-4000));
  const files = (dir) => filesUnder(dir).map((rel) => [rel, sha256(fs.readFileSync(path.join(dir, rel)))]);
  assert.deepEqual(files(out), files(runs[0]));
  runs.push(out);
});

test('a fresh capture equals the committed record', { timeout: 60_000, skip: browserSetup().skip }, async () => {
  assert.ok(runs.length >= 1, 'the first capture did not run');
  const { compareWithCommitted } = await import('./capture.mjs');
  const machine = readJson(path.join(runs[0], 'cases.json')).machine;
  assert.deepEqual(compareWithCommitted(runs[0], machine), [], 'files that differ from the committed record');
  for (const dir of runs) fs.rmSync(dir, { recursive: true, force: true });
});
