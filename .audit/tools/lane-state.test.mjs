import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { beginStage, readStage, saveStage } from './lane-state.mjs';

const git = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
function fixture(t) {
  const cwd = mkdtempSync(join(tmpdir(), 'tc-lane-state-'));
  t.after(() => rmSync(cwd, { recursive: true, force: true }));
  git(cwd, 'init');
  git(cwd, 'config', 'user.email', 'fixture@example.invalid');
  git(cwd, 'config', 'user.name', 'Fixture');
  writeFileSync(join(cwd, '.gitignore'), '.enjoy-lane/\n');
  writeFileSync(join(cwd, 'input'), 'first\n');
  git(cwd, 'add', '.');
  git(cwd, 'commit', '-m', 'Fixture');
  return { cwd, report: { sha: git(cwd, 'rev-parse', 'HEAD'), summary: 'Three independent reviewers completed; see their findings.', problems: [] } };
}

test('a completed report survives a fresh CLI process with its full findings', t => {
  const { cwd, report } = fixture(t);
  const saved = saveStage(cwd, 'S2', 'verify-1', report);
  assert.equal(saved.saved, true);
  assert.deepEqual(readStage(cwd, 'S2', 'verify-1').report, report);
  const output = execFileSync(process.execPath, [new URL('./lane-state.mjs', import.meta.url).pathname, 'read', cwd, 'S2', 'verify-1'], { encoding: 'utf8' });
  assert.deepEqual(JSON.parse(output).report, report);
});

test('dirty tracked or new untracked inputs prevent reuse and saving', t => {
  const { cwd, report } = fixture(t);
  saveStage(cwd, 'C2', 'build', report);
  writeFileSync(join(cwd, 'input'), 'changed\n');
  assert.equal(readStage(cwd, 'C2', 'build').reusable, false);
  assert.throws(() => saveStage(cwd, 'C2', 'build', report), /must be clean/);
  git(cwd, 'restore', 'input');
  writeFileSync(join(cwd, 'new-case'), 'new input');
  assert.equal(readStage(cwd, 'C2', 'build').reusable, false);
});

test('a new commit requires new reviews even with the same tree', t => {
  const { cwd, report } = fixture(t);
  saveStage(cwd, 'C2', 'verify-1', report);
  git(cwd, 'commit', '--allow-empty', '-m', 'Different reviewed commit');
  assert.equal(readStage(cwd, 'C2', 'verify-1').reusable, false);
  assert.throws(() => saveStage(cwd, 'C2', 'verify-1', report), /current full sha/);
});

test('environment changes invalidate reports without persisting secret values', t => {
  const { cwd, report } = fixture(t);
  const env = { ...process.env, TC_TEST_SECRET: 'never-persist-this-value', RUSTFLAGS: '-C opt-level=1' };
  const saved = saveStage(cwd, 'C2', 'validate', report, env);
  assert.equal(readStage(cwd, 'C2', 'validate', env).reusable, true);
  assert.equal(readStage(cwd, 'C2', 'validate', { ...env, RUSTFLAGS: '-C opt-level=2' }).reusable, false);
  assert.equal(readFileSync(saved.path, 'utf8').includes(env.TC_TEST_SECRET), false);
});

test('missing, cut, malformed and other-stage reports fail closed', t => {
  const { cwd, report } = fixture(t);
  assert.equal(readStage(cwd, 'S2', 'prove').reusable, false);
  const saved = saveStage(cwd, 'S2', 'prove', report);
  writeFileSync(saved.path, '{');
  assert.equal(readStage(cwd, 'S2', 'prove').reusable, false);
  writeFileSync(saved.path, 'null');
  assert.equal(readStage(cwd, 'S2', 'prove').reusable, false);
  saveStage(cwd, 'S2', 'prove', report);
  const bytes = JSON.parse(readFileSync(saved.path, 'utf8'));
  bytes.stage = 'verify-1';
  writeFileSync(saved.path, JSON.stringify(bytes));
  assert.equal(readStage(cwd, 'S2', 'prove').reusable, false);
});

test('stage identifiers cannot escape the checkpoint folder', t => {
  const { cwd, report } = fixture(t);
  assert.throws(() => saveStage(cwd, '../escape', 'build', report), /identifiers/);
  assert.throws(() => readStage(cwd, 'C2', '../../escape'), /identifiers/);
});

test('an interrupted begin is never reusable and completion records timing', t => {
  const { cwd, report } = fixture(t);
  const started = beginStage(cwd, 'C2', 'build');
  assert.equal(readStage(cwd, 'C2', 'build').reusable, false);
  const saved = saveStage(cwd, 'C2', 'build', report);
  const receipt = JSON.parse(readFileSync(saved.path, 'utf8'));
  assert.equal(receipt.startedAt, started.startedAt);
  assert.equal(receipt.inputSha, report.sha);
  assert.ok(receipt.durationMs >= 0);
});

test('a new checkout can be scheduled before its directory exists', t => {
  const { cwd } = fixture(t);
  assert.equal(readStage(join(cwd, 'not-created'), 'C2', 'build').reusable, false);
});

test('changed Cargo home configuration invalidates stage validation', t => {
  const { cwd, report } = fixture(t);
  const cargoHome = join(cwd, '.enjoy-lane/cargo-home');
  mkdirSync(cargoHome, { recursive: true });
  const env = { ...process.env, CARGO_HOME: cargoHome };
  saveStage(cwd, 'C2', 'validate', report, env);
  writeFileSync(join(cargoHome, 'config.toml'), '[build]\njobs=1\n');
  assert.equal(readStage(cwd, 'C2', 'validate', env).reusable, false);
});
