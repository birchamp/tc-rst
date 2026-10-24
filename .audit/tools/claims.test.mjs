// INV-CLAIMED-TESTS-EXIST: every test a document names exists, or the row says which card that
// is not done will write it.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { claimsOf, idsIn, pendingOf, resolve, states, tables, testNames } from './claims.mjs';

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
  // A cell pending on a card that is not done may name the test that card will write.
  if (pending.some((id) => known.has(id) && known.get(id) !== 'done')) return problems;
  for (const claim of claimsOf(cell.replace(/\(pending:[^)]*\)/g, ''))) {
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
