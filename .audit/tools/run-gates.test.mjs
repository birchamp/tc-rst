import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import * as fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defaultSteps, loopbackPreflight, parseArguments, preflight, runGates } from './run-gates.mjs';

async function fixture(t) {
  const cwd = await fs.mkdtemp(path.join(os.tmpdir(), 'tc-gate-test-'));
  t.after(() => fs.rm(cwd, { recursive: true, force: true }));
  const git = args => {
    const result = spawnSync('git', args, { cwd, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  git(['init', '-q']);
  git(['config', 'user.email', 'fixture@example.invalid']);
  git(['config', 'user.name', 'Gate fixture']);
  await fs.writeFile(path.join(cwd, '.gitignore'), '/.enjoy-lane/\n/.enjoy-logs/\n');
  await fs.writeFile(path.join(cwd, 'source.txt'), 'first\n');
  git(['add', '.']); git(['commit', '-qm', 'Fixture']);
  const counter = path.join(cwd, '.enjoy-logs', 'counter');
  const command = [process.execPath, '-e', `
    const fs = require('node:fs');
    const p = process.argv[1];
    const n = fs.existsSync(p) ? Number(fs.readFileSync(p, 'utf8')) : 0;
    fs.writeFileSync(p, String(n + 1));
    console.log('ran');
  `, counter];
  const options = { cwd, command, env: { ...process.env },
    preflightCheck: root => preflight(root, async () => {}) };
  const count = async () => Number(await fs.readFile(counter, 'utf8'));
  const receipts = async () => (await fs.readdir(path.join(cwd, '.enjoy-lane/gates/receipts'))).filter(name => name.endsWith('.json'));
  return { cwd, git, command, options, counter, count, receipts };
}

test('default gate suite retains host, release and every wasm feature check in order', () => {
  const steps = defaultSteps();
  assert.deepEqual(steps.slice(0, 3), [
    ['cargo', 'fmt', '--all', '--check'],
    ['cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings'],
    ['cargo', 'test', '--workspace'],
  ]);
  assert.deepEqual(steps[3], ['cargo', 'clippy', '-p', 'tc-app', '--lib', '--', '-D', 'warnings']);
  assert.deepEqual(steps[4], ['cargo', 'clippy', '-p', 'tc-server', '-p', 'tc4', '--lib', '--bins', '--', '-D', 'warnings']);
  assert.equal(steps.length, 10);
  assert.deepEqual(steps.slice(5, 9).map(args => args.includes('--features') ? args[args.indexOf('--features') + 1] : 'none'),
    ['none', 'gallery', 'app', 'test-hooks']);
  for (const args of steps.slice(5)) {
    assert.ok(args.includes('wasm32-unknown-unknown'));
    assert.ok(args.includes('--all-targets'));
    assert.deepEqual(args.slice(-3), ['--', '-D', 'warnings']);
  }
  assert.equal(steps.at(-1)[3], 'tc-api');
});

test('clean successes cache and fresh runs preserve separate complete logs', async t => {
  const f = await fixture(t);
  const events = [];
  const first = await runGates({ ...f.options, onEvent: event => events.push(event) });
  assert.equal(first[0].cached, false);
  assert.equal(first[0].exitCode, 0);
  assert.equal(first[0].clean, true);
  assert.equal(first[0].unchanged, true);
  assert.ok(first[0].durationMs >= 0);
  assert.deepEqual(events.map(event => event.type), ['start', 'finish']);
  assert.match(await fs.readFile(first[0].log, 'utf8'), /ran\n/);
  assert.match(await fs.readFile(first[0].log, 'utf8'), /"finishedAt"/);
  assert.equal((await runGates(f.options))[0].cached, true);
  assert.equal(await f.count(), 1);
  const fresh = await runGates({ ...f.options, fresh: true });
  assert.equal(fresh[0].cached, false);
  assert.equal(await f.count(), 2);
  assert.notEqual(first[0].log, fresh[0].log);
  assert.equal((await fs.readdir(path.dirname(first[0].log))).length, 2);
  assert.equal((await f.receipts()).length, 1);
});

test('changed SHA, environment, command and job count invalidate previous success', async t => {
  const f = await fixture(t);
  await runGates(f.options);
  await fs.writeFile(path.join(f.cwd, 'source.txt'), 'second\n');
  f.git(['add', 'source.txt']); f.git(['commit', '-qm', 'Second input']);
  assert.equal((await runGates(f.options))[0].cached, false);
  assert.equal((await runGates({ ...f.options, env: { ...f.options.env, PROPTEST_CASES: '33' } }))[0].cached, false);
  assert.equal((await runGates({ ...f.options, command: [...f.command, 'literal;$(not-a-shell)'] }))[0].cached, false);
  assert.equal((await runGates({ ...f.options, jobs: 2 }))[0].cached, false);
  assert.equal(await f.count(), 5);
});

test('dirty tracked, staged and untracked inputs always run and never create receipts', async t => {
  const f = await fixture(t);
  await runGates(f.options);
  await fs.writeFile(path.join(f.cwd, 'source.txt'), 'dirty\n');
  assert.equal((await runGates(f.options))[0].cached, false);
  f.git(['add', 'source.txt']);
  assert.equal((await runGates(f.options))[0].cached, false);
  f.git(['reset', '--hard', '-q', 'HEAD']);
  const untracked = path.join(f.cwd, 'new-fixture.txt');
  await fs.writeFile(untracked, 'untracked\n');
  assert.equal((await runGates(f.options))[0].cached, false);
  await fs.writeFile(untracked, 'different untracked contents\n');
  assert.equal((await runGates(f.options))[0].cached, false);
  assert.equal((await f.receipts()).length, 1);
  assert.equal(await f.count(), 5);
  await fs.rm(untracked);
  assert.equal((await runGates(f.options))[0].cached, true);
});

test('changing the command executable at the same path invalidates its receipt', async t => {
  const f = await fixture(t);
  const executable = path.join(f.cwd, '..', `gate-executable-${path.basename(f.cwd)}`);
  t.after(() => fs.rm(executable, { force: true }));
  await fs.writeFile(executable, '#!/bin/sh\nprintf first\n', { mode: 0o700 });
  const options = { ...f.options, command: [executable] };
  assert.equal((await runGates(options))[0].cached, false);
  assert.equal((await runGates(options))[0].cached, true);
  await fs.writeFile(executable, '#!/bin/sh\nprintf second\n');
  assert.equal((await runGates(options))[0].cached, false);
});

test('failure and source mutation never leave reusable success receipts', async t => {
  const f = await fixture(t);
  const command = [...f.command]; command[2] += '\nprocess.exitCode = 9;';
  await assert.rejects(runGates({ ...f.options, command }), /exit 9/);
  await assert.rejects(runGates({ ...f.options, command }), /exit 9/);
  assert.equal(await f.count(), 2);
  assert.deepEqual(await f.receipts(), []);
  await assert.rejects(runGates({ ...f.options, command: [process.execPath, '-e',
    "require('node:fs').writeFileSync('source.txt', 'changed during test')"] }), /inputs changed/);
  assert.deepEqual(await f.receipts(), []);
});

test('missing or tampered archived logs invalidate receipts', async t => {
  const f = await fixture(t);
  const first = await runGates(f.options);
  await fs.appendFile(first[0].log, 'altered output\n');
  const second = await runGates(f.options);
  assert.equal(second[0].cached, false);
  await fs.rm(second[0].log);
  assert.equal((await runGates(f.options))[0].cached, false);
  assert.equal(await f.count(), 3);
});

test('a failed fresh retry invalidates its previous pass', async t => {
  const f = await fixture(t);
  const flag = path.join(f.cwd, '.enjoy-logs', 'fail');
  const command = [...f.command];
  command[2] += `\nif (require('node:fs').existsSync(${JSON.stringify(flag)})) process.exitCode = 7;`;
  const options = { ...f.options, command };
  await runGates(options);
  await fs.writeFile(flag, 'fail');
  await assert.rejects(runGates({ ...options, fresh: true }), /exit 7/);
  assert.deepEqual(await f.receipts(), []);
  await fs.rm(flag);
  assert.equal((await runGates(options))[0].cached, false);
});

test('preflight failure prevents child launch and releases the repository lock', async t => {
  const f = await fixture(t);
  await assert.rejects(runGates({ ...f.options, preflightCheck: async () => { throw new Error('loopback denied'); } }), /loopback denied/);
  await assert.rejects(fs.stat(f.counter), { code: 'ENOENT' });
  assert.equal((await runGates(f.options))[0].cached, false);
});

test('repository lock refuses overlap; interrupted child exits and releases lock without a receipt', async t => {
  const f = await fixture(t);
  const controller = new AbortController();
  let ready;
  const started = new Promise(resolve => { ready = resolve; });
  let childPid;
  const linked = `${f.cwd}-linked`;
  t.after(() => fs.rm(linked, { recursive: true, force: true }));
  f.git(['worktree', 'add', '--detach', '-q', linked, 'HEAD']);
  const pending = runGates({ ...f.options, signal: controller.signal,
    command: [process.execPath, '-e', "console.log(process.pid); setInterval(() => {}, 1000)"],
    onOutput: (_destination, bytes) => { childPid = Number(bytes.toString().trim()); ready(); } });
  // Attach the rejection handler before interrupting, keeping Node's unhandled-rejection audit meaningful.
  const rejected = assert.rejects(pending, /Gate failed.*SIGTERM/);
  try {
    await Promise.race([started, new Promise((_, reject) => setTimeout(() => reject(new Error('child did not start')), 5000).unref())]);
    await assert.rejects(runGates({ ...f.options, cwd: linked }), /Gate runner busy/);
  } finally { controller.abort('SIGTERM'); }
  await rejected;
  assert.throws(() => process.kill(childPid, 0), { code: 'ESRCH' });
  assert.deepEqual(await f.receipts(), []);
  assert.equal((await runGates(f.options))[0].cached, false);
});

test('a child retains the shared lock across temporary directories after its runner is killed', async t => {
  const f = await fixture(t);
  const childTmp = path.join(f.cwd, '.enjoy-lane', 'child-tmp');
  await fs.mkdir(childTmp, { recursive: true });
  const runnerPath = fileURLToPath(new URL('./run-gates.mjs', import.meta.url));
  const childCode = "console.log('CHILD:' + process.pid); setInterval(() => {}, 1000)";
  const runner = spawn(process.execPath, [runnerPath, '--', process.execPath, '-e', childCode], {
    cwd: f.cwd, env: { ...f.options.env, TMPDIR: childTmp }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '';
  let errors = '';
  runner.stderr.on('data', bytes => { errors += bytes; });
  const exit = new Promise(resolve => runner.once('close', resolve));
  let childPid;
  try {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(new Error(`runner child did not start: ${errors}`)), 5000);
      runner.stdout.on('data', bytes => {
        output += bytes;
        const match = output.match(/\nCHILD:(\d+)\n/);
        if (match) { childPid = Number(match[1]); clearTimeout(timeout); resolve(); }
      });
      runner.once('close', code => { clearTimeout(timeout); if (!childPid) reject(new Error(`runner exited ${code}: ${errors}`)); });
    });
    runner.kill('SIGKILL');
    await exit;
    await assert.rejects(runGates(f.options), /Gate runner busy/);
    assert.deepEqual(await f.receipts(), []);
  } finally {
    runner.kill('SIGKILL');
    if (childPid) {
      try { process.kill(-childPid, 'SIGTERM'); } catch (error) { if (error.code !== 'ESRCH') throw error; }
    }
  }
  let result;
  for (let attempt = 0; attempt < 50; attempt++) {
    try { result = await runGates(f.options); break; }
    catch (error) { if (!/Gate runner busy/.test(error.message)) throw error; }
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.equal(result?.[0].cached, false, 'the child exit releases the inherited lock');
});

test('loopback preflight either proves a local connection or reports its sandbox refusal', async () => {
  try { await loopbackPreflight(); }
  catch (error) { assert.ok(['EPERM', 'EACCES'].includes(error.code), error.message); }
});

test('CLI parsing preserves command arguments without shell interpretation', () => {
  assert.deepEqual(parseArguments(['--cwd', 'subdir', '--jobs', '3', '--fresh', '--', 'node', '-e', 'console.log("a;$(b)")']), {
    cwd: 'subdir', jobs: 3, fresh: true, command: ['node', '-e', 'console.log("a;$(b)")'],
  });
  assert.throws(() => parseArguments(['--jobs']), /Expected a value/);
  assert.throws(() => parseArguments(['--']), /Expected a command/);
});
