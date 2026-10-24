// INV-CLAIMED-TESTS-EXIST: every test a document names exists, or the row says which card that
// is not done will write it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { claimsOf, claimsWithPending, idsIn, pendingOf, resolve, states, tables, testNames } from './claims.mjs';

const REPO = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const names = testNames(REPO);
const known = states(REPO);

/** The problems of one test cell: unresolved claims, and pending markers on done cards. */
function cellProblems(where, cell, what) {
  const problems = [];
  const pending = pendingOf(cell);
  for (const id of pending) {
    if (!known.has(id)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is pending on ${id}, but no card ${id} exists`);
    else if (known.get(id) === 'done') problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is pending on ${id}, but ${id} is done: write the test or fix the row`);
  }
  // A name followed by a pending marker on a card that is not done may be the test that card
  // will write; every other name must exist.
  for (const { claim, pending: after } of claimsWithPending(cell)) {
    if (after.some((id) => known.has(id) && known.get(id) !== 'done')) continue;
    if (!resolve(claim, names)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is ${what} by ${claim}, but no such test exists`);
  }
  return problems;
}

test('the resolver finds fns, journey titles, template titles and parity ids', () => {
  assert.ok(resolve('fold_converges_under_any_union', names), 'a Rust fn');
  assert.ok(resolve('properties::fold_converges_under_any_union', names), 'a mod::fn path');
  assert.ok(resolve('j25_a_story_opens_with_its_rail_its_units_and_writable_comment_boxes', names), 'a journey title');
  assert.ok(resolve('item_navigation_translationWords_en_marking_row_boundaries_tab_and_wrapping', names), 'a template title');
  assert.ok(resolve('crates/tc-journal/tests/properties.rs', names), 'a test file path');
  assert.ok(!resolve('no_test_has_this_name_xyz', names));
  assert.ok(!resolve('retry_delay', names), 'a product fn is no test');
  assert.ok(resolve('create', names), 'a journey file by its stem');
  assert.deepEqual(claimsOf('`a_{b,c}_d` and `e`'), ['a_b_d', 'a_c_d', 'e']);
  assert.deepEqual(pendingOf('`x` (pending: I, F1)'), ['I', 'F1']);
});

test('INV-CLAIMED-TESTS-EXIST: TC4-ISSUES covered rows', () => {
  const file = 'docs/TC4-ISSUES.md';
  const problems = [];
  for (const row of tables(fs.readFileSync(path.join(REPO, file), 'utf8'), ['Status', 'Regression test'])) {
    const status = row.cells.Status;
    const issue = row.cells.Issue ?? row.cells['#'];
    const where = `${file}:${row.line} (${issue})`;
    const cell = row.cells['Regression test'];
    if (status === 'covered' || status === 'done') {
      if (/to add/i.test(cell) && pendingOf(cell).length === 0) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is covered, but its test cell says "to add": a covered row cannot wait for its test`);
      problems.push(...cellProblems(where, cell, 'covered'));
    } else if (status === 'unit todo') {
      const ids = idsIn(row.cells['Unit or crate'] ?? '', known);
      if (ids.length === 0) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} waits on no card: name the card that will write its test`);
      else if (ids.every((id) => known.get(id) === 'done')) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} waits on ${ids.join(', ')}, but they are done: write the test or mark the row`);
    }
  }
  assert.deepEqual(problems, []);
});

/** The lines of a card's `- **<label>**` bullet in its `## Funnel card`, up to the next one. */
function bullet(card, label) {
  const funnel = card.split('## Funnel card')[1]?.split('\n## ')[0] ?? '';
  const start = funnel.indexOf(`- **${label}**`);
  if (start < 0) return '';
  const rest = funnel.slice(start + 1);
  const next = rest.search(/\n- \*\*/);
  return next < 0 ? rest : rest.slice(0, next);
}

/** A bullet's items, without those marked "(not in code: <why>)" (a one-time step, such as a
 * backup or a merge, that a command checked) or "(retired <by what>)"; each mark names its reason. */
function claimed(text) {
  // "Proof it can fail" names the product code a mutation breaks, not a test.
  return text
    .split(/\n(?= {2}- )/)
    .filter((item) => !/\((?:not in code|retired)\b[^)]*\S[^)]*\)/.test(item))
    .filter((item) => !/^\s*- Proof it can fail:/.test(item));
}

/** Every file of the trees an invariant's name may be printed or tested in, read once. */
const sources = (() => {
  const out = [];
  const walk = (dir) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      if (e.name === 'node_modules' || e.name === 'target') continue;
      const p = path.join(dir, e.name);
      if (e.isDirectory()) walk(p);
      else if (/\.(rs|mjs|js|sh|py)$/.test(e.name)) out.push(fs.readFileSync(p, 'utf8'));
    }
  };
  for (const d of ['crates', 'parity', '.audit/tools', 'scripts']) if (fs.existsSync(path.join(REPO, d))) walk(path.join(REPO, d));
  return out.join('\n');
})();

test('INV-CLAIMED-TESTS-EXIST: units', () => {
  const dir = path.join(REPO, '.audit/roadmap/units');
  const problems = [];
  for (const file of fs.readdirSync(dir).filter((f) => f.endsWith('.md') && !f.startsWith('_'))) {
    const card = fs.readFileSync(path.join(dir, file), 'utf8');
    if (!/^state: done$/m.test(card)) continue;
    for (const m of claimed(bullet(card, 'Checks')).join('\n').matchAll(/`([a-z][a-z0-9]*(?:_[a-z0-9]+)+)`/g)) {
      if (!resolve(m[1], names)) problems.push(`INV-CLAIMED-TESTS-EXIST: units/${file} names ${m[1]} but no test has that name`);
    }
    for (const m of new Set(claimed(bullet(card, 'Invariants')).join('\n').match(/INV-[A-Z0-9-]*[A-Z0-9]/g) ?? [])) {
      if (!sources.includes(m)) problems.push(`INV-INVARIANT-NAMES-ARE-USED: units/${file} names ${m} but nothing prints or tests it`);
    }
  }
  assert.deepEqual(problems, []);
});

test('INV-CLAIMED-TESTS-EXIST: P-LEDGER', () => {
  const file = 'docs/P-LEDGER.md';
  const rows = tables(fs.readFileSync(path.join(REPO, file), 'utf8'), ['P', 'CI job', 'Tests']);
  const jobs = new Set();
  for (const wf of fs.readdirSync(path.join(REPO, '.github/workflows'))) {
    const body = fs.readFileSync(path.join(REPO, '.github/workflows', wf), 'utf8').split(/^jobs:\s*$/m)[1] ?? '';
    for (const m of body.matchAll(/^ {2}([a-z0-9-]+):\s*$/gm)) jobs.add(m[1]);
  }
  const problems = [];
  for (let p = 1; p <= 10; p++) if (!rows.some((r) => r.cells.P === `P${p}`)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${file} has no row for P${p}`);
  for (const row of rows) {
    const where = `${file}:${row.line} (${row.cells.P})`;
    for (const job of claimsOf(row.cells['CI job'])) if (!jobs.has(job)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} names CI job ${job}, but no workflow has that job`);
    for (const id of pendingOf(row.cells.Tests)) {
      if (!known.has(id)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is pending on ${id}, but no card ${id} exists`);
      else if (known.get(id) === 'done') problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is pending on ${id}, but ${id} is done: name its tests`);
    }
    // A ledger row names only tests that exist; a pending marker says what is still owed.
    for (const claim of claimsOf(row.cells.Tests.replace(/\(pending:[^)]*\)/g, ''))) {
      if (!resolve(claim, names)) problems.push(`INV-CLAIMED-TESTS-EXIST: ${where} is shown by ${claim}, but no such test exists`);
    }
  }
  assert.deepEqual(problems, []);
});
