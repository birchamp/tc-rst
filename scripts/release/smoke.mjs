#!/usr/bin/env node
// INV-EVERY-INSTALLER-PASSES-THE-SMOKE-TEST: post-install smoke test of a release (card G3, step 4): run on a fresh runner of each
// platform against the installed `tc4`, with no network and no npm dependencies.
//
//   node scripts/release/smoke.mjs --tc4 <exe> --ui <dir> --bundle <dir> --manifest <BUILD-MANIFEST.json>
//
// Prints `ok <step>` or `FAIL <step>: <why>` and exits non-zero at the first failure. It prints
// the projects folder last, so the CI job can check the project survives an uninstall.
import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import readline from 'node:readline';
import { parseArgs } from 'node:util';

const { values: opt } = parseArgs({
  options: { tc4: { type: 'string' }, ui: { type: 'string' }, bundle: { type: 'string' }, manifest: { type: 'string' } },
});
for (const name of ['tc4', 'ui', 'bundle', 'manifest']) {
  if (!opt[name]) {
    console.error('usage: node scripts/release/smoke.mjs --tc4 <exe> --ui <dir> --bundle <dir> --manifest <BUILD-MANIFEST.json>');
    process.exit(2);
  }
}

const HANG_MS = 60_000;
// A port nothing listens on: the bundled resources must install with no network.
const DEAD_DOOR43 = 'http://127.0.0.1:9/';
const BOOK = 'TIT';
const WORDS = 'Pablo, siervo de Dios — smoke test ñ 1:1';

const root = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-smoke-'));
const appData = path.join(root, 'appdata');
const projects = path.join(root, 'projects');
fs.mkdirSync(projects);

class Hung extends Error {}
const requestId = () => crypto.randomBytes(16).toString('hex');
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function bounded(promise, what) {
  let timer;
  const hang = new Promise((_, reject) => { timer = setTimeout(() => reject(new Hung(what)), HANG_MS); });
  return Promise.race([promise, hang]).finally(() => clearTimeout(timer));
}

async function until(what, poll, ms = HANG_MS) {
  const deadline = Date.now() + ms;
  let last;
  for (;;) {
    const value = await poll((seen) => { last = seen; });
    if (value) return value;
    // The last state seen tells a slow step from a stuck one.
    if (Date.now() > deadline) throw new Hung(last === undefined ? what : `${what}; last seen ${JSON.stringify(last)}`);
    await pause(200);
  }
}

/** Starts `tc4 serve` on the run's folders; answers its first stdout line, parsed, and the child. */
async function start(what) {
  const child = spawn(path.resolve(opt.tc4), [
    'serve', '--app-data', appData, '--projects', projects, '--ui', path.resolve(opt.ui),
    '--bundle', path.resolve(opt.bundle), '--door43', DEAD_DOOR43,
  ], { stdio: ['pipe', 'pipe', 'inherit'] });
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve(code ?? signal)));
  child.on('error', () => {});
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const first = await bounded(Promise.race([lines.next(), exited.then(() => ({ done: true }))]), what);
  const line = first.done ? null : JSON.parse(first.value);
  return { child, exited, line };
}

/** A running tc4: its address line checked, and `call` for its API as the page makes it. */
async function serve(what) {
  const { child, exited, line } = await start(what);
  if (!line?.url || !line?.token) {
    child.kill();
    throw new Error(`tc4 printed no address: ${JSON.stringify(line)} (exit ${await exited})`);
  }
  const base = new URL(line.url);
  const call = (endpoint, body = {}) => bounded(new Promise((resolve, reject) => {
    const text = JSON.stringify(body);
    const request = http.request({
      host: base.hostname, port: base.port, method: 'POST', path: `/api/v1/${endpoint}`,
      headers: {
        Host: base.host, Origin: base.origin, 'X-Tc-Token': line.token, 'X-Tc-Api': '1',
        'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(text),
      },
    }, (response) => {
      const parts = [];
      response.on('data', (part) => parts.push(part));
      response.on('end', () => {
        const reply = Buffer.concat(parts).toString('utf8');
        if (response.statusCode !== 200) return reject(new Error(`${endpoint}: status ${response.statusCode} ${reply.slice(0, 300)}`));
        const parsed = JSON.parse(reply);
        if (parsed.type !== 'ok') return reject(new Error(`${endpoint}: ${reply.slice(0, 300)}`));
        resolve(parsed.value);
      });
    });
    request.on('error', reject);
    request.end(text);
  }), what);
  const quit = async () => {
    if (child.exitCode !== null || child.signalCode !== null) return;
    child.stdin.end('{"quit":true}\n');
    const code = await bounded(exited, `${what} (quit)`);
    if (code !== 0) throw new Error(`tc4 quit with exit ${code}`);
  };
  return { call, quit, kill: () => child.kill('SIGKILL') };
}

/** Opens the project through the `session/open` job; answers the session's scope. */
async function open(app, project, what) {
  const job = await app.call('session/open', { request: requestId(), project, unit: null });
  const state = await until(what, async () => {
    const view = await app.call('jobs/get', { job });
    return view && !['queued', 'running', 'cancelling'].includes(view.state.type) && view.state;
  });
  if (state.type !== 'done') throw new Error(`session/open ended ${JSON.stringify(state)}`);
  const session = (await app.call('app/hello')).sessions.find((s) => s.at.project === project);
  if (!session) throw new Error('app/hello names no open session for the project');
  return session.at;
}

const verseOne = async (app, at) => {
  const view = await app.call('editor/chapters', { at, book: BOOK });
  const row = view.chapters.find((c) => c.chapter === 1)?.verses.find((v) => v.key === '1');
  if (!row) throw new Error(`editor/chapters has no ${BOOK} 1:1`);
  return row;
};

let running = [];
let state = {};
async function step(name, run) {
  try {
    await run();
    console.log(`ok ${name}`);
  } catch (error) {
    console.log(`FAIL ${name}: ${error instanceof Hung ? `hung: ${error.message}` : error.message}`);
    for (const app of running) app.kill();
    process.exit(1);
  }
}

await step('address', async () => {
  state.app = await serve('address');
  running = [state.app];
});

await step('about_commit_matches_the_release_record', async () => {
  const manifest = JSON.parse(fs.readFileSync(opt.manifest, 'utf8'));
  const { build } = await state.app.call('app/hello');
  if (!manifest.commit || build.commit !== manifest.commit) {
    throw new Error(`app/hello says commit ${build.commit}, the manifest says ${manifest.commit}`);
  }
});

await step('bundled_resources_without_network', async () => {
  // `hello.startup` is exact from the first request: a first start with a bundle is preparing.
  const first = (await state.app.call('app/hello')).startup;
  if (first.type !== 'preparing' || !(first.value.total > 0)) {
    throw new Error(`a first start has nothing to install from the bundle: ${JSON.stringify(first)}`);
  }
  // A fresh Windows runner scans each new file, so the first install once took over 60 s.
  const end = await until('bundled_resources_without_network', async (seen) => {
    const now = (await state.app.call('app/hello')).startup;
    seen(now);
    return now.type !== 'preparing' && now;
  }, 180_000);
  if (end.type !== 'ready') throw new Error(`installing the bundle ended ${JSON.stringify(end)}`);
});

await step('create_a_project_with_one_book', async () => {
  const details = {
    name: 'Smoke Test', languageName: 'Spanish', code: 'es-419', door43Code: null,
    direction: 'ltr', font: 'Noto Sans (default)',
  };
  const { project } = await state.app.call('create/bible', { request: requestId(), details, versification: 'eng' });
  const { added } = await state.app.call('create/addBlankBooks', { request: requestId(), project, books: [BOOK] });
  if (added.join() !== BOOK) throw new Error(`create/addBlankBooks added ${JSON.stringify(added)}`);
  state.project = project;
});

await step('save_a_verse', async () => {
  const at = await open(state.app, state.project, 'save_a_verse');
  const row = await verseOne(state.app, at);
  const ack = await state.app.call('editor/saveVerse', { at, request: requestId(), shown: row.words.token, words: WORDS });
  if (ack.newConflicts.length) throw new Error(`the save made conflicts: ${JSON.stringify(ack.newConflicts)}`);
});

await step('restart_reads_the_verse_back', async () => {
  await state.app.quit();
  state.app = await serve('restart_reads_the_verse_back');
  running = [state.app];
  const at = await open(state.app, state.project, 'restart_reads_the_verse_back');
  const row = await verseOne(state.app, at);
  if (row.words.value !== WORDS) throw new Error(`read back ${JSON.stringify(row.words.value)}, saved ${JSON.stringify(WORDS)}`);
});

await step('a_second_server_is_refused', async () => {
  // tc4 waits up to 10 s for a holder that may be closing before it refuses.
  const second = await start('a_second_server_is_refused');
  running.push({ kill: () => second.child.kill('SIGKILL') });
  const code = await bounded(second.exited, 'a_second_server_is_refused');
  if (second.line?.url) throw new Error(`a second tc4 served at ${second.line.url}`);
  if (code === 0) throw new Error('a second tc4 exited 0');
  await state.app.call('app/hello');
});

await step('projects_survive', async () => {
  await state.app.quit();
  const folder = path.join(projects, state.project);
  const files = fs.existsSync(folder) ? fs.readdirSync(folder, { recursive: true }) : [];
  if (!files.length) throw new Error(`${folder} is missing or empty after quit`);
  console.log(`projects folder: ${projects}`);
});
