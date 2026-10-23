// INV-LOCAL-PASS-MEANS-CI-PASS: scripts/gates.sh is the one gate list. CI runs each step by
// name, the Windows VM's list equals it, and the workflows run the script.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = file => readFileSync(join(ROOT, file), 'utf8');

/** The `"name|command"` entries of a bash array `NAME=( ... )`. */
export function arrayEntries(script, name) {
  const body = script.split(`${name}=(`)[1]?.split('\n)')[0] ?? '';
  return [...body.matchAll(/^\s*"([^|"]+)\|([^"]*)"/gm)].map(m => ({ name: m[1], command: m[2] }));
}

const gates = read('scripts/gates.sh');
const steps = arrayEntries(gates, 'STEPS');
const nightly = arrayEntries(gates, 'NIGHTLY');
const ci = read('.github/workflows/ci.yml');
const ciLines = ci.split('\n').map(line => line.trim().replace(/^(- )?run: /, ''));
const ciSteps = ciLines.flatMap(line => (line.match(/scripts\/gates\.sh --step (\S+)/g) ?? []).map(m => m.split(' ').at(-1)));
const ciCommands = ciLines.filter(line => line.startsWith('cargo ') || line.startsWith('node --test'));

test('INV-LOCAL-PASS-MEANS-CI-PASS: every gate step runs in a CI job', () => {
  for (const step of steps) {
    assert.ok(ciSteps.includes(step.name), `gate drift: scripts/gates.sh step "${step.name}" has no "scripts/gates.sh --step ${step.name}" line in .github/workflows/ci.yml`);
  }
});

test('INV-LOCAL-PASS-MEANS-CI-PASS: CI runs no command outside the gate list', () => {
  const commands = [...steps, ...nightly].map(step => step.command);
  for (const command of ciCommands) {
    assert.ok(commands.includes(command), `gate drift: ci.yml runs "${command}" but scripts/gates.sh does not`);
  }
});

test('INV-LOCAL-PASS-MEANS-CI-PASS: the Windows VM runs the same commands', () => {
  const ps1 = read('.audit/tools/winvm/gates.ps1');
  const body = ps1.split('$gates = [ordered]@{')[1].split('\n}')[0];
  const entries = [...body.matchAll(/^\s*'?([\w-]+)'?\s*=\s*'([^']*)'/gm)];
  assert.ok(entries.length > 0, 'gate drift: gates.ps1 has no $gates entries');
  for (const [, name, command] of entries) {
    const step = steps.find(s => s.name === name);
    assert.ok(step, `gate drift: gates.ps1 step "${name}" is not a scripts/gates.sh step`);
    // These change how much a failure reports, not what passes.
    const same = command.replace(' --keep-going', '').replace(' --no-fail-fast', '');
    assert.equal(same, step.command, `gate drift: gates.ps1 step "${name}" runs "${command}" but scripts/gates.sh runs "${step.command}"`);
  }
});

test('INV-LOCAL-PASS-MEANS-CI-PASS: the workflows run the gate script', () => {
  for (const file of ['.audit/workflows/build-lane.js', '.audit/workflows/merge-main.js']) {
    const line = read(file).split('\n').find(l => l.startsWith('const GATES ='));
    assert.ok(line?.includes('scripts/gates.sh'), `gate drift: ${file} GATES does not run scripts/gates.sh`);
  }
  if (existsSync(join(ROOT, '.audit/tools/run-gates.mjs'))) {
    const body = read('.audit/tools/run-gates.mjs').split('export function defaultSteps() {')[1]?.split('\n}')[0] ?? '';
    assert.ok(body.includes('scripts/gates.sh') && !body.includes("'cargo'"), 'gate drift: .audit/tools/run-gates.mjs defaultSteps() lists its own cargo commands instead of scripts/gates.sh');
  }
});
