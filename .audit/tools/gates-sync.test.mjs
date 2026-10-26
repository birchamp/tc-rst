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
    const report = (c) => c.replace(' --keep-going', '').replace(' --no-fail-fast', '');
    assert.equal(report(command), report(step.command), `gate drift: gates.ps1 step "${name}" runs "${command}" but scripts/gates.sh runs "${step.command}"`);
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

/** The jobs of a workflow file: each job id with the text of its block. */
function jobsOf(text) {
  const body = text.split(/^jobs:\s*$/m)[1] ?? '';
  return [...body.matchAll(/^ {2}([a-z0-9-]+):\s*$([\s\S]*?)(?=^ {2}[a-z0-9-]+:\s*$|(?![\s\S]))/gm)].map((m) => ({ id: m[1], text: m[2] }));
}

test('INV-CI-IS-THE-GATE: every cargo step builds --locked with the pinned toolchain', () => {
  const toolchain = read('rust-toolchain.toml');
  assert.match(toolchain, /^channel = "\d+\.\d+\.\d+"$/m, 'INV-CI-IS-THE-GATE: rust-toolchain.toml pins an exact version');
  const unlocked = [...steps, ...nightly].filter((s) => s.command.startsWith('cargo ') && !s.command.startsWith('cargo fmt') && !s.command.includes(' --locked'));
  assert.deepEqual(unlocked.map((s) => `INV-CI-IS-THE-GATE: step ${s.name} builds without --locked`), []);
  const cargoSteps = new Set([...steps, ...nightly].filter((s) => s.command.startsWith('cargo ')).map((s) => s.name).concat('journeys'));
  const problems = [];
  for (const file of ['.github/workflows/ci.yml', '.github/workflows/app-e2e.yml']) {
    const text = read(file);
    if (text.includes('dtolnay/rust-toolchain')) problems.push(`INV-CI-IS-THE-GATE: ${file} installs a floating toolchain`);
    for (const job of jobsOf(text)) {
      const builds = [...job.text.matchAll(/scripts\/gates\.sh --step (\S+)/g)].some((m) => cargoSteps.has(m[1]));
      if (builds && !job.text.includes('rustup toolchain install')) problems.push(`INV-CI-IS-THE-GATE: ${file} job ${job.id} builds without installing rust-toolchain.toml's toolchain`);
    }
  }
  assert.deepEqual(problems, []);
});

test('INV-BUILDS-ON-WINDOWS: the gate list builds for Windows, and CI runs it', () => {
  const windows = steps.find((s) => s.name === 'clippy-windows');
  assert.ok(windows?.command.includes('--target x86_64-pc-windows-gnu'), 'INV-BUILDS-ON-WINDOWS: scripts/gates.sh has no clippy-windows step for x86_64-pc-windows-gnu');
  assert.ok(ciSteps.includes('clippy-windows'), 'INV-BUILDS-ON-WINDOWS: ci.yml does not run scripts/gates.sh --step clippy-windows');
});

test('INV-JOURNEYS-RUN: every browser journey runs in CI on every main and gate push', () => {
  const e2e = read('.github/workflows/app-e2e.yml');
  assert.match(e2e.split(/^jobs:/m)[0], /^\s+push:\n\s+branches: \[claude\/focused-ride-5uzj7p, 'gate\/\*\*'\]/m, 'INV-JOURNEYS-RUN: app-e2e does not run on pushes to main and gate/**');
  assert.match(e2e, /scripts\/gates\.sh --step journeys/, 'INV-JOURNEYS-RUN: app-e2e does not run scripts/gates.sh --step journeys');
  assert.match(gates, /node --test[^\n]*parity\/app\/journeys\/\*\.test\.mjs/, 'INV-JOURNEYS-RUN: the journeys step does not run every parity/app/journeys/*.test.mjs');
});

test('INV-FULL-TESTS-GATE-MERGES: the full tests run on main, gate/* and on request, not on other pushes', () => {
  const ci = read('.github/workflows/ci.yml');
  for (const job of ['test', 'test-app', 'alignment-budget', 'parity-baseline']) {
    const body = ci.split(new RegExp(`^  ${job}:$`, 'm'))[1]?.split(/^  [a-z-]+:$/m)[0] ?? '';
    assert.match(body, /if: .*refs\/heads\/claude\/focused-ride-5uzj7p.*startsWith\(github\.ref, 'refs\/heads\/gate\/'\).*workflow_dispatch/, `INV-FULL-TESTS-GATE-MERGES: ci.yml job ${job} runs on every push`);
  }
});

test('INV-A-HUNG-TEST-IS-NAMED-AND-STOPPED: every gate test has a time limit (CHK-8)', () => {
  const problems = [];
  for (const step of steps) {
    const libtest = step.command.split('&&').filter((c) => /cargo test /.test(c) && !/ --doc\b| --list\b/.test(c));
    if (libtest.length) problems.push(`step ${step.name} runs Rust tests through cargo test, which stops no hung test; use cargo nextest run`);
  }
  const nextest = read('.config/nextest.toml');
  const config = nextest.split(/^\[profile\.default\]$/m)[1]?.split(/^\[/m)[0] ?? '';
  if (!/^slow-timeout = \{ period = "60s", terminate-after = 5 \}$/m.test(config)) problems.push('.config/nextest.toml [profile.default] does not stop a test after 5 periods of 60 s');
  // An override may lengthen a measured test's limit, never remove it or stretch it past 20 minutes.
  for (const line of nextest.split('\n').filter((l) => /^slow-timeout\b/.test(l))) {
    const m = line.match(/^slow-timeout = \{ period = "60s", terminate-after = (\d+) \}$/);
    if (!m || Number(m[1]) > 20) problems.push(`.config/nextest.toml: ${line} sets no limit of 20 minutes or less`);
  }
  const nextestSteps = new Set(steps.filter((s) => s.command.includes('cargo nextest')).map((s) => s.name));
  for (const job of jobsOf(ci)) {
    const runs = [...job.text.matchAll(/scripts\/gates\.sh --step (\S+)/g)].some((m) => nextestSteps.has(m[1]));
    if (runs && !job.text.includes('tool: cargo-nextest@')) problems.push(`ci.yml job ${job.id} runs a nextest step without installing a pinned cargo-nextest`);
  }
  if (!/^\s*"node\|node --test --test-timeout=\d+ /m.test(gates)) problems.push('the node step sets no --test-timeout');
  const kit = read('parity/app/journeys/kit.mjs');
  const limit = Number(kit.match(/export function browserTest\([^)]*timeout = ([\d_]+)/)?.[1].replaceAll('_', ''));
  if (!(limit > 0 && limit <= 120_000)) problems.push('browserTest in parity/app/journeys/kit.mjs has no default timeout of 2 minutes or less');
  assert.deepEqual(problems, []);
});
