// INV-TOUCHED-RUNS-WHAT-A-CHANGE-REACHES: `scripts/gates.sh --touched` runs every crate a branch
// changed and every crate that depends on one, and every journey that names a changed endpoint
// or test id (card CHK-7b).
import test from 'node:test';
import assert from 'node:assert/strict';
import { crateOf, withDependents, tokensOf, journeysFor } from './touched.mjs';

test('INV-TOUCHED-RUNS-WHAT-A-CHANGE-REACHES: a changed crate brings its dependents', () => {
  const deps = { 'tc-core': [], 'tc-api': ['tc-core'], 'tc-app': ['tc-api', 'tc-core'], 'tc-ui': ['tc-api'], 'tc-door43': [] };
  assert.equal(crateOf('crates/tc-api/src/app.rs'), 'tc-api');
  assert.equal(crateOf('docs/PLAN.md'), null);
  assert.deepEqual(withDependents(['tc-api'], deps), ['tc-api', 'tc-app', 'tc-ui']);
  assert.deepEqual(withDependents(['tc-door43'], deps), ['tc-door43']);
});

test('INV-TOUCHED-RUNS-WHAT-A-CHANGE-REACHES: a journey that names a changed endpoint or test id runs', () => {
  const diff = [
    '--- a/crates/tc-api/src/endpoint.rs',
    '+++ b/crates/tc-api/src/endpoint.rs',
    '+    path: "prefs/setRailFolded",',
    '--- a/crates/tc-ui/src/views/print.rs',
    '+++ b/crates/tc-ui/src/views/print.rs',
    '-        .attr("data-testid", "export-menu-trigger")',
    '--- a/crates/tc-store/src/x.rs',
    '+++ b/crates/tc-store/src/x.rs',
    '+    let s = "not-a-ui-word";',
    '--- a/crates/tc-ui/src/views/gone.rs',
    '+++ /dev/null',
    '-        .attr("data-testid", "gone-panel")',
  ].join('\n');
  const tokens = tokensOf(diff);
  assert.ok(tokens.has('prefs/setRailFolded') && tokens.has('export-menu-trigger'));
  assert.ok(!tokens.has('not-a-ui-word'), 'only the page, the API and the server count');
  assert.ok(tokens.has('gone-panel'), "a deleted file's removed lines count");
  const journeys = {
    'parity/app/journeys/cc.test.mjs': "await page.getByTestId('export-menu-trigger').click();",
    'parity/app/journeys/rail.test.mjs': "await env.call('prefs/setRailFolded', {});",
    'parity/app/journeys/other.test.mjs': "await page.getByTestId('home').click();",
    'parity/app/journeys/edited.test.mjs': '',
  };
  assert.deepEqual(journeysFor(['parity/app/journeys/edited.test.mjs'], tokens, journeys),
    ['parity/app/journeys/cc.test.mjs', 'parity/app/journeys/edited.test.mjs', 'parity/app/journeys/rail.test.mjs']);
});

// INV-TOUCHED-RUNS-THE-PR-LINTS: a pull request's CI runs only the lint-* jobs, so `--touched`
// runs each of their steps; a branch that passed it failed lint-linux's fmt (PR 857).
test('INV-TOUCHED-RUNS-THE-PR-LINTS: --touched runs every step a lint job runs', async () => {
  const { readFile } = await import('node:fs/promises');
  const ci = await readFile(new URL('../../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const gates = await readFile(new URL('../gates.sh', import.meta.url), 'utf8');
  const jobs = ci.split(/^ {2}(?=[a-z][\w-]*:\s*$)/m).filter((job) => job.startsWith('lint-'));
  assert.ok(jobs.length >= 3, 'lint-linux, lint-windows and lint-wasm are found');
  const ciSteps = new Set(jobs.flatMap((job) => [...job.matchAll(/gates\.sh --step (\S+)/g)].map((m) => m[1])));
  const list = gates.match(/^PR_LINTS=\(([^)]*)\)/m);
  assert.ok(list, 'gates.sh defines PR_LINTS');
  assert.deepEqual(new Set(list[1].split(/\s+/).filter(Boolean)), ciSteps);
});
