#!/usr/bin/env node
// Linux: flock owns the open file description, so a crash releases the lock without stale-PID deletion.
import { createHash, randomUUID } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { constants } from 'node:fs';
import * as fs from 'node:fs/promises';
import net from 'node:net';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const RECEIPT_VERSION = 1;
const digest = value => createHash('sha256').update(value).digest('hex');
const executableDigests = new Map();

export function defaultSteps() {
  return [['scripts/gates.sh']];
}

function capture(command, args, cwd, env) {
  const result = spawnSync(command, args, { cwd, env, encoding: 'utf8', maxBuffer: 16 << 20 });
  if (result.error || result.status !== 0) {
    throw new Error(`${command} ${args.join(' ')} failed: ${result.error?.message ?? result.stderr.trim()}`);
  }
  return result.stdout.trim();
}

async function repository(cwd, env) {
  const root = await fs.realpath(capture('git', ['rev-parse', '--show-toplevel'], cwd, env));
  const common = await fs.realpath(capture('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'], root, env));
  return { root, common };
}

async function executable(command, cwd, env) {
  const candidates = command.includes('/')
    ? [path.resolve(cwd, command)]
    : (env.PATH ?? '').split(path.delimiter).map(dir => path.resolve(cwd, dir, command));
  for (const candidate of candidates) {
    try {
      await fs.access(candidate, constants.X_OK);
      const real = await fs.realpath(candidate);
      const stat = await fs.stat(real, { bigint: true });
      if (!stat.isFile()) continue;
      const stamp = `${real}:${stat.size}:${stat.mtimeNs}:${stat.ctimeNs}`;
      if (!executableDigests.has(stamp)) executableDigests.set(stamp, digest(await fs.readFile(real)));
      return { path: real, sha256: executableDigests.get(stamp) };
    } catch (error) {
      if (!['ENOENT', 'ENOTDIR', 'EACCES'].includes(error.code)) throw error;
    }
  }
  throw new Error(`Executable not found: ${command}`);
}

async function cargoConfiguration(cwd, env) {
  const files = new Set();
  let dir = cwd;
  for (;;) {
    for (const name of ['config', 'config.toml']) files.add(path.join(dir, '.cargo', name));
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  const cargoHome = env.CARGO_HOME ?? path.join(env.HOME ?? os.homedir(), '.cargo');
  for (const name of ['config', 'config.toml']) files.add(path.join(cargoHome, name));
  const values = [];
  for (const file of [...files].sort()) {
    try { values.push([file, digest(await fs.readFile(file))]); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  return values;
}

async function snapshot(repo, cwd, command, env) {
  const git = args => capture('git', args, repo.root, env);
  // Ignored build products are outputs. Nonignored untracked files disable reuse, including new tests.
  const status = git(['status', '--porcelain=v1', '-z', '--untracked-files=all', '--ignore-submodules=none']);
  const tools = { node: process.version, executable: await executable(command[0], cwd, env) };
  if (path.basename(command[0]) === 'cargo') {
    tools.cargo = capture(command[0], ['--version', '--verbose'], cwd, env);
    tools.rustc = capture('rustc', ['--version', '--verbose'], cwd, env);
    if (command[1] === 'clippy') tools.clippy = capture(command[0], ['clippy', '--version'], cwd, env);
    if (command[1] === 'fmt') tools.rustfmt = capture(command[0], ['fmt', '--version'], cwd, env);
    tools.configuration = await cargoConfiguration(cwd, env);
  }
  const inputs = {
    version: RECEIPT_VERSION,
    root: repo.root,
    cwd,
    head: git(['rev-parse', 'HEAD']),
    tree: git(['rev-parse', 'HEAD^{tree}']),
    gitConfiguration: digest(git(['config', '--null', '--show-origin', '--list'])),
    status,
    command,
    tools,
    // Hash rather than log environment values: they may contain tokens. All inherited variables
    // participate, avoiding an incomplete allowlist of compiler/test switches.
    environment: digest(JSON.stringify(Object.entries(env).sort(([a], [b]) => a.localeCompare(b)))),
    runner: digest(await fs.readFile(fileURLToPath(import.meta.url))),
  };
  return { key: digest(JSON.stringify(inputs)), clean: status.length === 0, inputs };
}

async function lockRepository(common) {
  // Fixture TMPDIR differs between jobs; the repository's shared directory does not.
  const lockPath = path.join(common, 'enjoy-gates.lock');
  const handle = await fs.open(lockPath, constants.O_CREAT | constants.O_RDWR | constants.O_NOFOLLOW, 0o600);
  const result = spawnSync('flock', ['--nonblock', '--conflict-exit-code', '75', '3'], {
    encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe', handle.fd],
  });
  if (result.error || result.status !== 0) {
    await handle.close();
    if (result.status === 75) throw new Error(`Gate runner busy: another worktree holds ${lockPath}. Retry when it finishes.`);
    throw new Error(`Cannot acquire gate lock: ${result.error?.message ?? result.stderr.trim()}`);
  }
  return handle;
}

export async function loopbackPreflight() {
  const server = net.createServer(socket => socket.end('ready'));
  let client;
  try {
    await new Promise((resolve, reject) => {
      server.once('error', reject);
      server.listen(0, '127.0.0.1', resolve);
    });
    await new Promise((resolve, reject) => {
      client = net.createConnection({ host: '127.0.0.1', port: server.address().port });
      let reply = '';
      client.setTimeout(3000, () => client.destroy(new Error('Loopback connection timed out')));
      client.on('data', bytes => { reply += bytes; });
      client.once('error', reject);
      client.once('end', () => reply === 'ready' ? resolve() : reject(new Error('Loopback reply was incomplete')));
    });
  } finally {
    client?.destroy();
    if (server.listening) await new Promise(resolve => server.close(resolve));
  }
}

export async function preflight(root, loopback = loopbackPreflight) {
  // Fail on sandbox bind restrictions or an unusable output filesystem before compiling/tests.
  const base = path.join(root, '.enjoy-lane', 'gates');
  await fs.mkdir(base, { recursive: true });
  const temp = await fs.mkdtemp(path.join(base, 'preflight-'));
  try {
    const file = await fs.open(path.join(temp, 'before'), 'wx');
    try { await file.writeFile('gate preflight'); await file.sync(); }
    finally { await file.close(); }
    await fs.rename(path.join(temp, 'before'), path.join(temp, 'after'));
    const directory = await fs.open(temp, 'r');
    try { await directory.sync(); } finally { await directory.close(); }
    if (await fs.readFile(path.join(temp, 'after'), 'utf8') !== 'gate preflight') throw new Error('Filesystem preflight readback differs');
    await loopback();
  } finally { await fs.rm(temp, { recursive: true, force: true }); }
}

async function readReceipt(file, key) {
  try {
    const value = JSON.parse(await fs.readFile(file, 'utf8'));
    if (!(value.version === RECEIPT_VERSION && value.key === key && value.exitCode === 0 && value.finishedAt && value.log && value.logSha256)) return false;
    return digest(await fs.readFile(value.log)) === value.logSha256;
  } catch (error) {
    if (['ENOENT', 'ENOTDIR'].includes(error.code) || error instanceof SyntaxError) return false;
    throw error;
  }
}

async function saveReceipt(file, receipt) {
  const temp = `${file}.${randomUUID()}.tmp`;
  try {
    const handle = await fs.open(temp, 'wx', 0o600);
    try { await handle.writeFile(`${JSON.stringify(receipt, null, 2)}\n`); await handle.sync(); }
    finally { await handle.close(); }
    await fs.rename(temp, file);
    const directory = await fs.open(path.dirname(file), 'r');
    try { await directory.sync(); } finally { await directory.close(); }
  } finally { await fs.rm(temp, { force: true }); }
}

async function execute(command, { cwd, env, log, signal, onOutput, lockFd }) {
  if (signal?.aborted) return { code: null, signal: 'SIGTERM' };
  return new Promise((resolve, reject) => {
    // The child retains the same lock if the runner is killed; an orphaned build must still
    // block another gate run. Never unlink this lock file or use PID-based stale cleanup.
    const child = spawn(command[0], command.slice(1), { cwd, env, detached: true, stdio: ['ignore', 'pipe', 'pipe', lockFd] });
    const abort = () => {
      if (!child.pid) return;
      try { process.kill(-child.pid, signal.reason === 'SIGINT' ? 'SIGINT' : 'SIGTERM'); }
      catch (error) { if (error.code !== 'ESRCH') reject(error); }
    };
    signal?.addEventListener('abort', abort, { once: true });
    if (signal?.aborted) abort();
    for (const [stream, destination] of [[child.stdout, 'stdout'], [child.stderr, 'stderr']]) {
      stream.on('data', bytes => { fsSyncWrite(log, bytes); onOutput?.(destination, bytes); });
    }
    child.once('error', reject);
    child.once('close', (code, childSignal) => {
      signal?.removeEventListener('abort', abort);
      resolve({ code, signal: childSignal });
    });
  });
}

// Synchronous writes preserve output order and finish before the success receipt is published.
import { writeSync as fsSyncWrite, closeSync, openSync, fsyncSync } from 'node:fs';

export async function runGates({ cwd = process.cwd(), command, fresh = false, jobs = 6,
  env = process.env, signal, onEvent = () => {}, onOutput,
  preflightCheck = preflight } = {}) {
  if (!Number.isSafeInteger(jobs) || jobs < 1) throw new Error('--jobs must be a positive integer');
  cwd = await fs.realpath(cwd);
  const runEnv = { ...env, CARGO_NET_OFFLINE: 'true', CARGO_BUILD_JOBS: String(jobs) };
  const repo = await repository(cwd, runEnv);
  const lock = await lockRepository(repo.common);
  try {
    await preflightCheck(repo.root);
    if (signal?.aborted) throw new Error('Gate run interrupted during preflight');
    const receipts = path.join(repo.root, '.enjoy-lane', 'gates', 'receipts');
    const logs = path.join(repo.root, '.enjoy-logs', 'gates');
    await fs.mkdir(receipts, { recursive: true });
    await fs.mkdir(logs, { recursive: true });
    // These output paths must be ignored; otherwise status correctly prevents caching them.
    const steps = command ? [command] : defaultSteps();
    const results = [];
    for (const args of steps) {
      if (!args.length || args.some(arg => typeof arg !== 'string')) throw new Error('A command needs executable and string arguments');
      if (signal?.aborted) throw new Error('Gate run interrupted');
      const before = await snapshot(repo, cwd, args, runEnv);
      const receiptFile = path.join(receipts, `${before.key}.json`);
      if (!fresh && before.clean && await readReceipt(receiptFile, before.key)) {
        const result = { command: args, cached: true, key: before.key };
        results.push(result); onEvent({ type: 'cached', ...result }); continue;
      }
      // A failed/interrupted forced retry supersedes an earlier pass on the same inputs.
      await fs.rm(receiptFile, { force: true });
      const startedAt = new Date().toISOString();
      const clock = performance.now();
      const log = path.join(logs, `${startedAt.replaceAll(':', '-')}-${randomUUID()}.log`);
      const logFd = openSync(log, 'wx', 0o600);
      const start = { type: 'start', command: args, startedAt, head: before.inputs.head, clean: before.clean, log };
      let outcome;
      try {
        fsSyncWrite(logFd, `${JSON.stringify(start)}\n`); onEvent(start);
        outcome = await execute(args, { cwd, env: runEnv, log: logFd, signal, onOutput, lockFd: lock.fd });
        const after = await snapshot(repo, cwd, args, runEnv);
        const unchanged = before.key === after.key;
        const result = { type: 'finish', command: args, cached: false, key: before.key,
          startedAt, finishedAt: new Date().toISOString(), durationMs: Math.round(performance.now() - clock),
          exitCode: outcome.code, signal: outcome.signal, unchanged, clean: before.clean && after.clean,
          interrupted: signal?.aborted ?? false, log };
        fsSyncWrite(logFd, `\n${JSON.stringify(result)}\n`); fsyncSync(logFd);
        results.push(result); onEvent(result);
        if (outcome.code !== 0 || outcome.signal || signal?.aborted) {
          throw new Error(`Gate failed${outcome.signal ? ` (${outcome.signal})` : ` (exit ${outcome.code})`}: ${JSON.stringify(args)}. Log: ${log}`);
        }
        if (!unchanged) throw new Error(`Gate inputs changed while it ran; success was not recorded. Rerun on stable inputs. Log: ${log}`);
        if (before.clean && after.clean) await saveReceipt(receiptFile, {
          version: RECEIPT_VERSION, ...result, logSha256: digest(await fs.readFile(log)),
        });
      } finally { closeSync(logFd); }
    }
    return results;
  } finally { await lock.close(); }
}

export function parseArguments(args) {
  const options = {};
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--') {
      options.command = args.slice(i + 1);
      if (!options.command.length) throw new Error('Expected a command after --');
      break;
    }
    if (arg === '--fresh') options.fresh = true;
    else if (arg === '--cwd' || arg === '--jobs') {
      const value = args[++i];
      if (value === undefined) throw new Error(`Expected a value after ${arg}`);
      options[arg.slice(2)] = arg === '--jobs' ? Number(value) : value;
    } else if (arg === '--help' || arg === '-h') options.help = true;
    else throw new Error(`Unknown option: ${arg}`);
  }
  return options;
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  if (options.help) {
    process.stdout.write('Usage: node .audit/tools/run-gates.mjs [--cwd DIR] [--jobs N] [--fresh] [-- executable args...]\nLinux flock; one runner per repository, offline, default jobs=6.\nSuccess receipts require a clean unchanged Git tree. Ignored build outputs are not inputs; use --fresh when changing external/ignored fixtures or dependencies.\n');
    return;
  }
  const invocation = await repository(process.cwd(), process.env);
  const cwd = await fs.realpath(options.cwd ?? process.cwd());
  const requested = await repository(cwd, process.env);
  if (requested.root !== invocation.root) throw new Error('--cwd must stay in the current worktree; launch with cwd set to the job worktree');
  const controller = new AbortController();
  const interrupt = name => controller.abort(name);
  const sigint = () => interrupt('SIGINT');
  const sigterm = () => interrupt('SIGTERM');
  process.on('SIGINT', sigint); process.on('SIGTERM', sigterm);
  try {
    await runGates({ ...options, cwd, signal: controller.signal,
      onEvent: event => process.stdout.write(`${JSON.stringify(event)}\n`),
      onOutput: (destination, bytes) => process[destination].write(bytes) });
  } finally {
    process.off('SIGINT', sigint); process.off('SIGTERM', sigterm);
    if (controller.signal.aborted) process.exitCode = controller.signal.reason === 'SIGINT' ? 130 : 143;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { process.stderr.write(`${error.message}\n`); if (!process.exitCode) process.exitCode = 1; });
}
