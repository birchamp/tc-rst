// The gate script's own behavior: it never blesses, it lists every step, and a receipt exists
// only for the exact clean tree a full run passed on.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, copyFileSync, rmSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomBytes } from 'node:crypto';

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

test('INV-NO-UNGATED-MERGE: --check-receipt refuses a tree without a receipt', () => {
  const scratch = join(ROOT, 'target', `gates-test-${randomBytes(4).toString('hex')}`);
  mkdirSync(join(scratch, 'scripts'), { recursive: true });
  try {
    copyFileSync(SCRIPT, join(scratch, 'scripts', 'gates.sh'));
    writeFileSync(join(scratch, '.gitignore'), '/target\n/bin\n');
    const git = (...args) => {
      const r = spawnSync('git', ['-c', 'user.name=t', '-c', 'user.email=t@example.invalid', ...args], { cwd: scratch, encoding: 'utf8' });
      assert.equal(r.status, 0, r.stderr);
      return r.stdout.trim();
    };
    git('init', '-q');
    git('add', '.');
    git('commit', '-q', '-m', 'gates');
    // A fake gh on PATH answers the runs FAKE_GH_RUNS names, so no test asks GitHub.
    mkdirSync(join(scratch, 'bin'));
    writeFileSync(join(scratch, 'bin', 'gh'), '#!/bin/bash\nprintf "%s\\n" "${FAKE_GH_RUNS:-}"\n', { mode: 0o755 });
    const env = { CARGO_TARGET_DIR: join(scratch, 'target'), PATH: `${join(scratch, 'bin')}:${process.env.PATH}` };
    const gates = () => spawnSync('bash', [join(scratch, 'scripts', 'gates.sh'), '--check-receipt'], { cwd: scratch, env: { ...clean, ...env }, encoding: 'utf8' });

    let r = gates();
    assert.equal(r.status, 1, r.stdout + r.stderr);
    assert.match(r.stdout, /^INV-NO-UNGATED-MERGE: no passing gate receipt for tree [0-9a-f]{40}/);

    const tree = git('rev-parse', 'HEAD^{tree}');
    mkdirSync(join(scratch, 'target', 'gates'), { recursive: true });
    writeFileSync(join(scratch, 'target', 'gates', `${tree}.ok`), `tree ${tree}\njourneys no\n`);
    r = gates();
    assert.equal(r.status, 1, 'a receipt without the journeys is partial: ' + r.stdout);
    assert.match(r.stdout, /^INV-NO-UNGATED-MERGE: .*\(journeys not run; drop --no-journeys\)/);

    writeFileSync(join(scratch, 'target', 'gates', `${tree}.ok`), `tree ${tree}\njourneys yes\n`);
    r = gates();
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.match(r.stdout, /^gates: receipt /);

    writeFileSync(join(scratch, 'probe'), '');
    r = gates();
    assert.equal(r.status, 1, 'an untracked file is a different tree: ' + r.stdout);
    assert.match(r.stdout, /^INV-NO-UNGATED-MERGE/);
    rmSync(join(scratch, 'probe'));
    rmSync(join(scratch, 'target', 'gates'), { recursive: true });

    // INV-NO-UNGATED-MERGE: a green ci and app-e2e run on the same tree is a receipt.
    const head = git('rev-parse', 'HEAD');
    env.FAKE_GH_RUNS = `ci 11 ${head}\napp-e2e 12 ${head}`;
    r = gates();
    assert.equal(r.status, 0, r.stdout + r.stderr);
    assert.match(r.stdout, /^gates: CI receipt: ci run 11, app-e2e run 12 on /);
    env.FAKE_GH_RUNS = `ci 11 ${head}`;
    r = gates();
    assert.equal(r.status, 1, 'ci alone is no receipt: ' + r.stdout);
    assert.match(r.stdout, /^INV-NO-UNGATED-MERGE/);
    writeFileSync(join(scratch, 'other'), '');
    git('add', '.');
    git('commit', '-q', '-m', 'other tree');
    env.FAKE_GH_RUNS = `ci 11 ${head}\napp-e2e 12 ${head}`;
    r = gates();
    assert.equal(r.status, 1, 'runs on another tree are no receipt: ' + r.stdout);
    assert.match(r.stdout, /^INV-NO-UNGATED-MERGE/);
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
});
