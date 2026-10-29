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

// INV-TOUCHED-LINTS-WHAT-A-CHANGE-REACHES: each PR lint step runs over the picked crates and is
// skipped when none of its crates is picked, so a narrow change lints in minutes.
test('INV-TOUCHED-LINTS-WHAT-A-CHANGE-REACHES: lint steps are narrowed to the picked crates', async () => {
  const { readFile } = await import('node:fs/promises');
  const { spawnSync } = await import('node:child_process');
  const gates = await readFile(new URL('../gates.sh', import.meta.url), 'utf8');
  const list = gates.match(/^PR_LINTS=\([^)]*\)/m)[0];
  const lint = gates.match(/^touched_lint\(\) \{[\s\S]*?^\}/m)[0];
  const run = (crates) => {
    const script = [
      'set -euo pipefail',
      'run_step(){ echo "step $1"; }',
      'cargo(){ echo "cargo $*"; }',
      'run_step_windows_ready(){ return 0; }',
      list,
      lint,
      `for s in "\${PR_LINTS[@]}"; do touched_lint "$s" ${crates.join(' ')}; done`,
    ].join('\n');
    const out = spawnSync('bash', ['-c', script], { encoding: 'utf8' });
    assert.equal(out.status, 0, out.stderr);
    return out.stdout.trim().split('\n');
  };
  assert.deepEqual(run([]), ['step fmt', 'step arch'], 'a change to no crate runs fmt and the architecture rules alone');
  const store = run(['tc-store']);
  assert.ok(store.includes('cargo clippy --locked --package=tc-store --all-targets -- -D warnings'));
  assert.ok(store.some((l) => l.includes('--package=tc-store') && l.includes('x86_64-pc-windows-gnu')));
  assert.ok(!store.some((l) => /wasm|release/.test(l)), store.join('\n'));
  const ui = run(['tc-ui']);
  for (const step of ['wasm-ui', 'wasm-ui-gallery', 'wasm-ui-app', 'wasm-ui-hooks']) assert.ok(ui.includes(`step ${step}`), step);
  assert.ok(!ui.includes('step wasm-api'));
  const app = run(['tc-api', 'tc-app', 'tc4']);
  for (const step of ['clippy-release-bins', 'clippy-release-app', 'wasm-api']) assert.ok(app.includes(`step ${step}`), step);
});
