#!/usr/bin/env node
// Stage reports are restart hints; executable gate receipts remain the test evidence.
import { createHash, randomUUID } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { closeSync, fsyncSync, mkdirSync, openSync, readFileSync, realpathSync, renameSync, unlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { arch, homedir, platform, release } from 'node:os';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(import.meta.url);
const version = 1;
const usage = 'Usage: lane-state.mjs read|begin <worktree> <unit> <stage> | save <worktree> <unit> <stage> <report.json>';
const digest = value => createHash('sha256').update(value).digest('hex');
const git = (cwd, ...args) => execFileSync('git', ['-C', cwd, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();

function identifier(value) {
  if (typeof value !== 'string' || !/^[A-Za-z0-9][A-Za-z0-9._-]{0,79}$/.test(value)) {
    throw new Error('Unit and stage must be short identifiers without slashes.');
  }
  return value;
}

function toolVersion(tool) {
  try { return execFileSync(tool, ['--version'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim(); }
  catch { return 'unavailable'; }
}

function policyDigest() {
  const base = resolve(dirname(source), '..');
  return digest([source, join(base, 'workflows/build-lane.js'), join(base, 'PROCESS.md')].map(path => {
    try { return readFileSync(path); }
    catch (error) { if (error.code === 'ENOENT') return Buffer.from('missing'); throw error; }
  }).map(bytes => digest(bytes)).join('\n'));
}

function cargoConfiguration(cwd, env) {
  const paths = new Set();
  for (let current = cwd; ; current = dirname(current)) {
    for (const name of ['config', 'config.toml']) paths.add(join(current, '.cargo', name));
    if (dirname(current) === current) break;
  }
  for (const name of ['config', 'config.toml']) paths.add(join(env.CARGO_HOME || join(env.HOME || homedir(), '.cargo'), name));
  return [...paths].sort().flatMap(path => {
    try { return [[path, digest(readFileSync(path))]]; }
    catch (error) { if (error.code === 'ENOENT') return []; throw error; }
  });
}

export function snapshot(worktree, env = process.env) {
  const cwd = realpathSync(worktree);
  const root = realpathSync(git(cwd, 'rev-parse', '--show-toplevel'));
  if (cwd !== root) throw new Error('Use the worktree root for a stage report.');
  const before = git(cwd, 'status', '--porcelain=v1', '--untracked-files=all');
  const sha = git(cwd, 'rev-parse', 'HEAD');
  const tree = git(cwd, 'rev-parse', 'HEAD^{tree}');
  const environment = Object.entries(env).filter(([key]) => !['PWD', 'OLDPWD', 'SHLVL', '_'].includes(key)).sort(([a], [b]) => a.localeCompare(b));
  const context = {
    version, cwd, sha, tree, policy: policyDigest(),
    runtime: { node: process.version, cargo: toolVersion('cargo'), rustc: toolVersion('rustc'), os: platform(), release: release(), arch: arch() },
    environment: digest(JSON.stringify(environment)),
    configuration: cargoConfiguration(cwd, env),
  };
  const after = git(cwd, 'status', '--porcelain=v1', '--untracked-files=all');
  return { ...context, clean: !before && !after && git(cwd, 'rev-parse', 'HEAD') === sha };
}

function reportPath(worktree, unit, stage) {
  return join(worktree, '.enjoy-lane/stages', identifier(unit), `${identifier(stage)}.json`);
}

function identity(context) {
  return digest(JSON.stringify(context));
}

function durableJson(path, value) {
  mkdirSync(dirname(path), { recursive: true });
  const temporary = `${path}.${randomUUID()}.tmp`;
  let fd;
  try {
    fd = openSync(temporary, 'wx', 0o600);
    writeFileSync(fd, `${JSON.stringify(value, null, 2)}\n`);
    fsyncSync(fd);
    closeSync(fd);
    fd = undefined;
    renameSync(temporary, path);
    const directory = openSync(dirname(path), 'r');
    try { fsyncSync(directory); } finally { closeSync(directory); }
  } finally {
    if (fd !== undefined) closeSync(fd);
    try { unlinkSync(temporary); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
}

function validReport(report, sha) {
  return report !== null && typeof report === 'object' && !Array.isArray(report)
    && report.sha === sha && typeof report.summary === 'string' && report.summary.trim().length > 0;
}

export function readStage(worktree, unit, stage, env = process.env) {
  identifier(unit);
  identifier(stage);
  let context;
  try { context = snapshot(worktree, env); }
  catch (error) {
    if (error.code === 'ENOENT') return { reusable: false, reason: 'Worktree has not been created.' };
    throw error;
  }
  const path = reportPath(context.cwd, unit, stage);
  if (!context.clean) return { reusable: false, reason: 'Worktree has uncommitted or untracked changes.', sha: context.sha };
  let saved;
  try { saved = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    if (error.code === 'ENOENT') return { reusable: false, reason: 'Stage has not completed.', sha: context.sha };
    if (error instanceof SyntaxError) return { reusable: false, reason: 'Stage report is incomplete or damaged.', sha: context.sha };
    throw error;
  }
  if (saved?.version !== version || saved.unit !== unit || saved.stage !== stage || saved.identity !== identity(context) || !validReport(saved.report, context.sha)) {
    return { reusable: false, reason: 'Commit, policy, tools, environment or report changed.', sha: context.sha };
  }
  return { reusable: true, sha: context.sha, completedAt: saved.completedAt, report: saved.report };
}

export function saveStage(worktree, unit, stage, report, env = process.env) {
  const context = snapshot(worktree, env);
  const path = reportPath(context.cwd, unit, stage);
  if (!context.clean) throw new Error('Commit the finished stage before saving its report; the worktree must be clean.');
  if (!validReport(report, context.sha)) throw new Error('A stage report needs the current full sha and a nonempty summary.');
  const final = snapshot(worktree, env);
  if (identity(context) !== identity(final)) throw new Error('Worktree changed while preparing the stage report.');
  let started;
  try { started = JSON.parse(readFileSync(`${path}.started`, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error; }
  const completedAt = new Date().toISOString();
  const timing = started?.unit === unit && started?.stage === stage && Number.isFinite(Date.parse(started.startedAt))
    ? { startedAt: started.startedAt, inputSha: started.inputSha, durationMs: Math.max(0, Date.parse(completedAt) - Date.parse(started.startedAt)) } : {};
  const saved = { version, unit, stage, identity: identity(context), sha: context.sha, ...timing, completedAt, report };
  durableJson(path, saved);
  return { saved: true, sha: context.sha, path };
}

export function beginStage(worktree, unit, stage, env = process.env) {
  const context = snapshot(worktree, env);
  const started = { unit: identifier(unit), stage: identifier(stage), startedAt: new Date().toISOString(), inputSha: context.sha };
  durableJson(`${reportPath(context.cwd, unit, stage)}.started`, started);
  return started;
}

function main(args) {
  const [action, worktree, unit, stage, reportFile] = args;
  if (args.length === 1 && action === '--help') return { usage };
  if (!worktree || !unit || !stage || !['read', 'begin', 'save'].includes(action) || args.length !== (action === 'save' ? 5 : 4)) {
    throw new Error(usage);
  }
  if (action === 'read') return readStage(worktree, unit, stage);
  if (action === 'begin') return beginStage(worktree, unit, stage);
  return saveStage(worktree, unit, stage, JSON.parse(readFileSync(reportFile, 'utf8')));
}

if (process.argv[1] && resolve(process.argv[1]) === source) {
  try { process.stdout.write(`${JSON.stringify(main(process.argv.slice(2)))}\n`); }
  catch (error) { process.stderr.write(`${error.message}\n`); process.exitCode = 1; }
}
