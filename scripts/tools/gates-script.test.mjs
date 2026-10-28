// The gate script's own behavior: it never blesses, it lists every step, and a receipt exists
// only for the exact clean tree a full run passed on.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const SCRIPT = join(ROOT, 'scripts', 'gates.sh');
const clean = Object.fromEntries(Object.entries(process.env).filter(([k]) => !/^(UPDATE_|TC_JOURNAL_BLESS|TC_WRITE_PROBE)/.test(k)));
/** The `"name|command"` entries of a bash array `NAME=( ... )`. */
const arrayEntries = (script, name) => [...(script.split(`${name}=(`)[1]?.split('\n)')[0] ?? '').matchAll(/^\s*"([^|"]+)\|([^"]*)"/gm)].map(m => ({ name: m[1], command: m[2] }));
const run = (args, cwd = ROOT, env = {}) => spawnSync('bash', [SCRIPT, ...args], { cwd, env: { ...clean, ...env }, encoding: 'utf8' });

test('INV-GATES-NEVER-BLESS: the gate script refuses a blessing environment', () => {
  for (const name of ['UPDATE_REPORT', 'UPDATE_GOLDEN', 'UPDATE_PREPARE', 'TC_JOURNAL_BLESS', 'TC_WRITE_PROBE']) {
    const r = run(['--list'], ROOT, { [name]: '' });
    assert.equal(r.status, 2, `${name} set: exit ${r.status}, stderr: ${r.stderr}`);
    assert.match(r.stderr, new RegExp(`${name} is set.*INV-GATES-NEVER-BLESS`));
  }
});

test('--list prints every step', () => {
  const script = readFileSync(SCRIPT, 'utf8');
  const expected = [...arrayEntries(script, 'STEPS').map((s, i) => `${i + 1}\t${s.name}\t${s.command}`), ...arrayEntries(script, 'NIGHTLY').map(s => `nightly\t${s.name}\t${s.command}`)];
  const r = run(['--list']);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.stdout.trimEnd().split('\n'), expected);
});
