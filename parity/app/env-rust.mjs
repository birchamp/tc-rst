// The Rust environment of the driver: `tc4 serve` on an empty app-data folder and projects
// folder under target/, the page loaded with its token, and everything the shared capture needs
// to wait for a quiet app there (APP-DESIGN 7.1). The tC4 environment is env-tc4.mjs.
//
// Built-ins only: CI imports this file for its pure tests, with no Playwright installed.
import { spawn } from 'node:child_process';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import readline from 'node:readline';
import { fileURLToPath } from 'node:url';
import { CONTEXT_OPTIONS } from './observe.mjs';
import { BIBLE, OBS } from './screens.mjs';
import { buildScenarioState } from './state.mjs';

export const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');

/** The folders the Rust app gives the two projects of each scenario, by tC4 repo path: the
 * names B1b's scenario loader gives them (`tc_app::backend::scenario`). */
export const PROJECTS = {
  sample: { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
  'en-pins': { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
  shared: { [BIBLE]: 'sample_burrito', [OBS]: 'sample_burrito_obs' },
  // No scenario: tc-store over a projects folder the caller fills.
  store: {},
};

/** The requests a shot waits for: the API and pictures, never the event stream, which stays
 * open for the life of the page; counted, it would make every shot wait out the deadline
 * (APP-DESIGN 7.1). The catalogs under /ui/ are covered by the page's `__tcIdle`. */
export function rustTracks(url) {
  const p = new URL(url).pathname;
  return (p.startsWith('/api/v1/') && p !== '/api/v1/events') || p.startsWith('/media/');
}

/** A request id as tc-api spells it: 16 random bytes in lowercase hex. */
export const newRequestId = () => crypto.randomBytes(16).toString('hex');

/** What Home looks like when the Rust app has drawn it (B2 D13): every card `home/list` names,
 * or `+ Add a project` when it names none. The cards are the server's, so a scenario whose
 * project the seed check refuses (the OBS sample, B1a §4.1-4.3) is not waited for. */
export async function rustHomeReady(page, call) {
  await page.getByTestId('add-project').waitFor();
  const home = await call('home/list', {});
  for (const card of home.cards) await page.getByTestId(`project-${card.project}`).waitFor();
}

/** A Door43 step the Rust environment cannot take yet. */
export function notYet(step, unit) {
  return async () => {
    throw new Error(`${step} needs the fake Door43 in the Rust environment, which unit ${unit} adds (APP-DESIGN §7.1)`);
  };
}

/** Builds `scenario`'s state into `<dir>/state` from this repository's copies of tC4's fixtures,
 * the same bytes the tC4 environment starts from (APP-DESIGN 7.2), and answers the flags that
 * start `tc4` on it: its projects are written into the run's empty projects folder and opened
 * through the store (unit C1a), or held by the memory backend with `backend` 'memory' (unit
 * B1b); its sideloaded resources go into the app's store. */
export function scenarioArgs(scenario, dir, backend = 'store') {
  if (!['store', 'memory'].includes(backend)) throw new Error(`the backend is store or memory, not ${backend}`);
  const state = path.join(dir, 'state');
  buildScenarioState({ scenario, out: state });
  return ['--scenario', state, ...(backend === 'memory' ? ['--backend', 'memory'] : [])];
}

/** A port nothing listens on: Door43 stays on this machine even if the internet switch were
 * turned on. A run that signs in passes a started tc-door43-fake instead (A8). */
const DEAD_DOOR43 = 'http://127.0.0.1:9/';

/** A plain HTTP request to the fake Door43's control port; answers the parsed JSON body. */
function controlCall(base, method, route, body) {
  const text = body === undefined ? '' : JSON.stringify(body);
  return new Promise((resolve, reject) => {
    const request = http.request(new URL(route, base), {
      method,
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(text) },
    }, (response) => {
      const parts = [];
      response.on('data', (part) => parts.push(part));
      response.on('end', () => {
        const reply = Buffer.concat(parts).toString('utf8');
        if (response.statusCode !== 200) return reject(new Error(`fake Door43 ${method} ${route}: status ${response.statusCode} ${reply.slice(0, 200)}`));
        resolve(reply ? JSON.parse(reply) : null);
      });
    });
    request.on('error', reject);
    request.end(text);
  });
}

/**
 * Starts the fake Door43 (`cargo build -p tc-door43-fake`; APP-DESIGN 7.1, R13) on loopback,
 * seeded with tC4's parity account (`facilitator-zq`), and answers its address and its control
 * port: the requests Door43 received (never a secret), faults, held requests, tokens.
 */
export async function startDoor43Fake(bin) {
  // stdin stays open: a closed stdin makes the fake quit.
  const child = spawn(path.resolve(bin), [], { stdio: ['pipe', 'pipe', 'inherit'] });
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  const first = await Promise.race([lines.next(), exited.then((end) => ({ done: true, end }))]);
  if (first.done) throw new Error(`the fake Door43 ended before printing its address (exit ${first.end?.code ?? first.end?.signal})`);
  const address = JSON.parse(first.value);
  if (address.error) {
    child.kill('SIGKILL');
    throw new Error(`the fake Door43 did not start: ${address.error}`);
  }
  const control = (method, route, body) => controlCall(address.control, method, route, body);
  return {
    url: address.url,
    host: new URL(address.url).host,
    /** Every request Door43 received: `{ method, path, query, auth }`, auth `none`,
     * `basic:<login>`, `token` or `other`. */
    requests: () => control('GET', '/requests'),
    clear: () => control('POST', '/requests/clear'),
    /** `{ method?, path, times?, fault: { type: 'status', status, message } | { type: 'delay', ms } | { type: 'hold' } }` */
    fault: (rule) => control('POST', '/faults', { method: null, times: null, ...rule }),
    /** The oldest held request goes on. */
    release: () => control('POST', '/release'),
    issueToken: async (login, name, scopes) => (await control('POST', '/tokens', { login, name, scopes })).secret,
    tokens: (login) => control('GET', `/tokens/${encodeURIComponent(login)}`),
    secret: async (login, name) => (await control('GET', `/secret/${encodeURIComponent(login)}/${encodeURIComponent(name)}`)).secret,
    async close() {
      if (child.exitCode !== null || child.signalCode !== null) return;
      child.stdin.write('{"quit":true}\n');
      const timeout = new Promise((resolve) => setTimeout(() => resolve('timeout'), 5_000));
      if (await Promise.race([exited, timeout]) === 'timeout') {
        child.kill('SIGKILL');
        await exited;
      }
    },
  };
}

/** A POST to the API as the page makes it: Host, Origin, the token and the API header. Answers
 * the `ok` value; any other status, or an `err`, rejects with the endpoint and the reply. */
function apiCall(base, token, endpoint, body) {
  const text = JSON.stringify(body);
  return new Promise((resolve, reject) => {
    const request = http.request({
      host: base.hostname,
      port: base.port,
      method: 'POST',
      path: `/api/v1/${endpoint}`,
      headers: {
        Host: base.host,
        Origin: base.origin,
        'X-Tc-Token': token,
        'X-Tc-Api': '1',
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(text),
      },
    }, (response) => {
      const parts = [];
      response.on('data', (part) => parts.push(part));
      response.on('end', () => {
        const reply = Buffer.concat(parts).toString('utf8');
        if (response.statusCode !== 200) {
          const error = new Error(`${endpoint}: status ${response.statusCode} ${reply.slice(0, 200)}`);
          error.status = response.statusCode;
          return reject(error);
        }
        const parsed = JSON.parse(reply);
        if (parsed.type !== 'ok') return reject(new Error(`${endpoint}: ${reply.slice(0, 300)}`));
        resolve(parsed.value);
      });
    });
    request.on('error', reject);
    request.end(text);
  });
}

/**
 * Starts `tc4 serve` for `scenario` and answers the AppEnv of driver.mjs, plus what the capture
 * needs: the context options with the token, the route that keeps every request on this
 * machine, the requests a shot waits for, the failure callouts, and `busy` for `settle`.
 *
 * `tc4` is a binary built with `--features test-hooks,fixture-backend`; `ui` is the folder
 * build-app.sh made.
 * Every run gets its own app-data and projects folders under `runDir` (in target/, never the
 * person's own folders: without `--projects`, tc4 uses `~/translationCore4/projects`).
 * `door43` is a started fake Door43 (`startDoor43Fake`), which `tc4` then signs in to;
 * without it Door43 is a port nothing listens on.
 */
export async function startRustEnv({ scenario, tc4, ui, seed, door43, backend = 'store', runDir = path.join(REPO, 'target', 'parity-rust', `${process.pid}-${Date.now()}`) }) {
  if (!PROJECTS[scenario]) throw new Error(`no scenario ${scenario}; the scenarios are ${Object.keys(PROJECTS).join(', ')}`);
  const dir = path.join(runDir, scenario);
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(path.join(dir, 'appdata'), { recursive: true });
  fs.mkdirSync(path.join(dir, 'projects'), { recursive: true });
  // `store`: tc-store over a projects folder `seed(folder)` fills, with no scenario (B2's
  // journey of a folder that cannot be opened).
  if (scenario === 'store') seed?.(path.join(dir, 'projects'));
  // stdin stays open: a closed stdin makes tc4 quit.
  const child = spawn(path.resolve(tc4), [
    'serve', '--app-data', path.join(dir, 'appdata'), '--projects', path.join(dir, 'projects'), '--ui', path.resolve(ui),
    '--door43', door43?.url ?? DEAD_DOOR43, '--test-hooks', ...(scenario === 'store' ? [] : scenarioArgs(scenario, dir, backend)),
  ], { stdio: ['pipe', 'pipe', 'inherit'] });
  const lines = readline.createInterface({ input: child.stdout })[Symbol.asyncIterator]();
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  const first = await Promise.race([lines.next(), exited.then((end) => ({ done: true, end }))]);
  if (first.done) {
    fs.rmSync(dir, { recursive: true, force: true });
    const end = first.end ?? await exited;
    throw new Error(`tc4 ended before printing its address (exit ${end.code ?? end.signal}); `
      + 'an exit 2 is the usage error of a tc4 built without `--features test-hooks,fixture-backend`, '
      + 'which does not know --test-hooks or --scenario');
  }
  const address = JSON.parse(first.value);
  if (address.error) {
    child.kill('SIGKILL');
    fs.rmSync(dir, { recursive: true, force: true });
    throw new Error(`tc4 did not start: ${JSON.stringify(address.error)}`);
  }
  const { url, token } = address;
  const base = new URL(url);
  const origin = base.origin;
  const call = (endpoint, body = {}) => apiCall(base, token, endpoint, body);
  try {
    await call('test/quiet', {});
  } catch (error) {
    child.kill('SIGKILL');
    fs.rmSync(dir, { recursive: true, force: true });
    if (error.status === 404) throw new Error('tc4 answers no test hooks: start it with --test-hooks from a build with `--features test-hooks,fixture-backend`');
    throw error;
  }
  const refused = new Set();
  const projects = PROJECTS[scenario];

  return {
    scenario,
    origin,
    token,
    projects,
    call,
    /** The fake Door43 tc4 signs in to, or null. */
    door43: door43 ?? null,
    /** Nothing leaves the machine: every request to another origin is aborted before it is
     * sent, and listed (`refused`). `extraHTTPHeaders` would give the token to any origin. */
    async prepare(context) {
      await context.route((u) => u.origin !== origin, (route) => {
        refused.add(route.request().url());
        return route.abort('blockedbyclient');
      });
    },
    /** The token on every request the browser makes by itself, as the shell adds it
     * (APP-DESIGN AD11, 5.2); the page's own calls set it too, and the server takes a repeated
     * header whose values are all the token. */
    contextOptions: { ...CONTEXT_OPTIONS, extraHTTPHeaders: { 'X-Tc-Token': token } },
    tracks: rustTracks,
    failureTestIds: ['view-error', 'boot-error'],
    catalog: (locale) => JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n', `${locale}.json`), 'utf8')),
    /**
     * Home afresh: preferences back to the seed, holds released, the internet off
     * (`test/reset`); a non-English `locale` saved as the app language; the page's storage
     * cleared; and a new load of the page. A `goto` to the address the page already shows only
     * moves to its fragment, so the load goes through about:blank.
     */
    async load(page, { locale }) {
      await call('test/reset', { request: newRequestId() });
      if (locale !== 'en') await call('prefs/setLocale', { request: newRequestId(), locale });
      if (page.url().startsWith(origin)) await page.evaluate(() => { localStorage.clear(); sessionStorage.clear(); });
      await page.goto('about:blank');
      await page.goto(`${origin}/#t=${token}`);
    },
    homeReady: (page) => rustHomeReady(page, call),
    /** The run's app-data folder, where a test can make a save fail (A4b's P7). */
    appData: path.join(dir, 'appdata'),
    /** Work the request tracker cannot see: the server's (`test/quiet`) and the page's own
     * (`window.__tcIdle`, which also covers the catalogs it fetches). */
    async busy(page) {
      if ((await call('test/quiet', {})).busy) return true;
      return page.evaluate(() => window.__tcIdle === false).catch(() => false);
    },
    projectId: (repo) => {
      const folder = projects[repo];
      if (!folder) throw new Error(`the ${scenario} scenario has no project for ${repo}`);
      return folder;
    },
    pushToDoor43: notYet('pushToDoor43', 'B9b'),
    dropNextClone: notYet('dropNextClone', 'E1a'),
    teammatesChangeOneVerse: notYet('teammatesChangeOneVerse', 'B10'),
    loseFromDoor43: notYet('loseFromDoor43', 'B10'),
    refused: () => [...refused],
    /** What `hello` says of the build, and the folder of the UI build the page loads. */
    async build() {
      const hello = await call('app/hello', {});
      const folders = fs.readdirSync(path.join(path.resolve(ui), 'ui'));
      return { ui: folders.length === 1 ? folders[0] : null, commit: hello.build?.commit ?? null, api: hello.api ?? null };
    },
    /** Quits as the shell does; ends tc4 if it has not gone 10 s later; removes the run's
     * folders. */
    async close() {
      if (child.exitCode === null && child.signalCode === null) {
        child.stdin.write('{"quit":true}\n');
        const timeout = new Promise((resolve) => setTimeout(() => resolve('timeout'), 10_000));
        if (await Promise.race([exited, timeout]) === 'timeout') {
          child.kill('SIGKILL');
          await exited;
        }
      }
      fs.rmSync(dir, { recursive: true, force: true });
      try {
        fs.rmdirSync(runDir);
      } catch {
        // another scenario of the run still has its folder there
      }
    },
  };
}
