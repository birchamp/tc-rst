// scripts/ci-changes.sh lets CI skip the Rust tests, the journeys and the shell only for a change
// wholly in prose; anything a test reads, or anything unknown, runs them.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = join(dirname(fileURLToPath(import.meta.url)), '..', 'ci-changes.sh');
const classify = (...paths) =>
  execFileSync('bash', [SCRIPT, '--classify'], { input: paths.join('\n') }).toString().trim();

test('a change wholly in docs/ or top-level markdown skips the code checks', () => {
  assert.equal(classify('docs/evidence/fixed-in-4.1-2026-10-29/README.md', 'docs/MAP.md'), 'false');
  assert.equal(classify('AGENTS.md', 'docs/rulings/2026-10-29-x.md'), 'false');
});

test('docs/contracts/, which tests read, is code', () => {
  assert.equal(classify('docs/contracts/SHELL.md'), 'true');
  assert.equal(classify('docs/MAP.md', 'docs/contracts/shell-bridge.json'), 'true');
});

test('any file outside the prose is code', () => {
  assert.equal(classify('docs/MAP.md', 'crates/tc-ui/src/ds/select.rs'), 'true');
  assert.equal(classify('crates/tc-ui/README.md'), 'true');
  assert.equal(classify('Cargo.lock'), 'true');
  assert.equal(classify('.github/workflows/ci.yml'), 'true');
});

test('an empty list, which a failed compare gives, is code', () => {
  assert.equal(classify(), 'true');
});
