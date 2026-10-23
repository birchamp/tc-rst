import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { countSkips } from './no-skips.mjs';

const SCRIPT = fileURLToPath(new URL('./no-skips.mjs', import.meta.url));

const CLEAN = `TAP version 13
# Subtest: home_lists_projects
ok 1 - home_lists_projects
  ---
  duration_ms: 812.4
  ...
# tests 1
# suites 0
# pass 1
# fail 0
# cancelled 0
# skipped 0
# todo 0
`;

const SKIPS = `TAP version 13
# Subtest: home_lists_projects
ok 1 - home_lists_projects
  ---
  duration_ms: 812.4
  ...
# Subtest: sign_in_with_door43
ok 2 - sign_in_with_door43 # SKIP set TC4_DOOR43_FAKE (cargo build -p tc-door43-fake)
  ---
  duration_ms: 0.2
  ...
    ok 1 - nested_probe # SKIP set TC4_UI_HOOKS (build-app.sh --test-hooks)
# tests 3
# pass 1
# fail 0
# skipped 2
`;

/** Runs the script on `tap` as CI does; returns its exit code and output. */
function run(tap) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'no-skips-'));
  const file = path.join(dir, 'run.tap');
  fs.writeFileSync(file, tap);
  try {
    return { code: 0, out: execFileSync(process.execPath, [SCRIPT, file], { encoding: 'utf8' }) };
  } catch (error) {
    return { code: error.status, out: error.stdout };
  } finally {
    fs.rmSync(dir, { recursive: true });
  }
}

test('no_skips_passes_a_run_with_no_skip', () => {
  assert.deepEqual(countSkips(CLEAN), { tests: 1, skipped: [] });
  assert.deepEqual(run(CLEAN), { code: 0, out: 'app-e2e: 1 tests, 0 skipped\n' });
});

test('no_skips_names_each_skipped_test_and_its_reason', () => {
  assert.deepEqual(countSkips(SKIPS).skipped, [
    { name: 'sign_in_with_door43', reason: 'set TC4_DOOR43_FAKE (cargo build -p tc-door43-fake)' },
    { name: 'nested_probe', reason: 'set TC4_UI_HOOKS (build-app.sh --test-hooks)' },
  ]);
  const { code, out } = run(SKIPS);
  assert.equal(code, 1);
  assert.match(out, /^INV-A-SKIP-IS-A-FAILURE: 2 tests skipped in app-e2e/);
  assert.match(out, /\nsign_in_with_door43: set TC4_DOOR43_FAKE/);
  assert.match(out, /\nnested_probe: set TC4_UI_HOOKS/);
});

test('no_skips_fails_a_run_with_no_summary', () => {
  const cut = CLEAN.slice(0, CLEAN.indexOf('# tests'));
  assert.equal(countSkips(cut).tests, null);
  assert.equal(run(cut).code, 1);
  assert.equal(run('').code, 1);
  assert.equal(run(CLEAN.replace('# tests 1', '# tests 0')).code, 1);
});
