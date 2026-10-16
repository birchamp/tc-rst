#!/usr/bin/env node
// A mock of the pankosmia-web HTTP API that translationCore 4 calls, backed by a state
// directory on disk (fixtures/build-state.mjs builds it), plus a static server for the
// built tC4 client. It exists only so the tC4 client renders its screens with realistic
// data for the visual-parity baseline; it is not a faithful server. Routes and response
// shapes follow src/data/serverApi.ts at tC4 d310bf3 (each route cites its wrapper).
//
//   node mock-server/server.mjs [--state <dir, default .state/sample>] [--client <dist dir>] [--port 19998]
//
// Served:
//   /api/...            the Pankosmia routes (below)
//   /clients/uw-tc4/... the built client (vite build with base /clients/uw-tc4/)
//   /                   redirects to /clients/uw-tc4/
//   /__mock/requests    every /api request seen, with its status (for the capture manifest)
//
// Error texts and the absent-repo answers follow the pankosmia-web source at
// /home/user/ref/pankosmia-web (0.19.2, bf0a5d3; tC4 pins 0.18.15, whose source is not on this
// machine): a missing file reads "could not read ingredient content: <io error>", a missing
// repo lists no paths, and so on. tC4 quotes some of these texts on screen.
//
// Deliberate simplifications. None of them is visible on the captured screens, except the
// last one:
//   - no git: add-and-commit records the message in memory and clears the pending set;
//     /git/status reports the files written since the last commit;
//   - creation routes (new-text-translation, new-obs-resource, new-scripture-book),
//     push and the zip imports answer is_good:false: the capture never submits them;
//   - the gitea catalog answers "offline mode" while the net gate is off and [] when on;
//   - clone, copy and pull answer is_good:false, so a sync or an open from Door43 ends in
//     tC4's refusal for a failed clone (the screens sync-failed and open-door43-refused).
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { zipSync } from 'fflate';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BASELINE = path.resolve(HERE, '..');

const OK = { is_good: true, reason: 'ok' };
const IO_ENOENT = 'No such file or directory (os error 2)';
const ENOENT = `could not read ingredient content: ${IO_ENOENT}`;
const SAFE_SEGMENT = /^[^.\s&?:#%\\*"'<>|][^\s&?:#%\\*"'<>|]*$/;
const VERSIFICATIONS = ['eng', 'lxx', 'org', 'rsc', 'rso', 'vul'];
const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.tsv': 'text/tab-separated-values; charset=utf-8',
  '.usfm': 'text/plain; charset=utf-8',
  '.yaml': 'text/yaml; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.woff': 'font/woff',
  '.ttf': 'font/ttf',
  '.wasm': 'application/wasm',
  '.txt': 'text/plain; charset=utf-8',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
};

class HttpError extends Error {
  constructor(status, reason) {
    super(reason);
    this.status = status;
  }
}

const md5 = (bytes) => crypto.createHash('md5').update(bytes).digest('hex');

/** Every file under `dir`, as `/`-joined paths relative to it. */
function walk(dir, { skipHidden = false, skipBak = false } = {}) {
  const out = [];
  const visit = (at, rel) => {
    for (const entry of fs.readdirSync(at, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      if (skipHidden && entry.name.startsWith('.')) continue;
      const relPath = rel ? `${rel}/${entry.name}` : entry.name;
      if (entry.isDirectory()) visit(path.join(at, entry.name), relPath);
      else if (!(skipBak && entry.name.endsWith('.bak'))) out.push(relPath);
    }
  };
  if (fs.existsSync(dir)) visit(dir, '');
  return out;
}

export function createMockServer({ state, client }) {
  const reposRoot = path.join(state, 'repos');
  const settingsDir = path.join(state, 'client_settings');
  let netEnabled = false;
  /** repoPath -> Map<ipath, change_type> written since the last commit */
  const pending = new Map();
  const commits = [];
  const requests = [];

  // ---- helpers -------------------------------------------------------------

  const segmentsOk = (segments) => segments.every((s) => SAFE_SEGMENT.test(s));

  /** Take a 3-segment repo path off the front of `rest`; returns [repoPath, remaining]. */
  const takeRepo = (rest) => {
    const parts = rest.split('/').map(decodeURIComponent);
    const repo = parts.slice(0, 3);
    if (repo.length !== 3 || !segmentsOk(repo)) throw new HttpError(400, `bad repo path ${rest}`);
    return [repo.join('/'), parts.slice(3)];
  };
  const repoDir = (repoPath) => path.join(reposRoot, ...repoPath.split('/'));
  /** The repo folder, or the platform's error for a repo that is not there. */
  const mustRepo = (repoPath, reason = 'could not locate repo') => {
    const dir = repoDir(repoPath);
    if (!fs.existsSync(path.join(dir, 'metadata.json'))) throw new HttpError(400, reason);
    return dir;
  };
  const ipathOf = (url) => {
    const ipath = url.searchParams.get('ipath');
    if (!ipath || !segmentsOk(ipath.split('/'))) throw new HttpError(400, `bad ipath ${ipath}`);
    return ipath;
  };
  const readMeta = (repoPath) => JSON.parse(fs.readFileSync(path.join(mustRepo(repoPath, `could not read metadata: ${IO_ENOENT}`), 'metadata.json'), 'utf8'));
  const listRepos = () => {
    const out = [];
    for (const a of fs.existsSync(reposRoot) ? fs.readdirSync(reposRoot) : []) {
      for (const b of fs.readdirSync(path.join(reposRoot, a))) {
        for (const c of fs.readdirSync(path.join(reposRoot, a, b))) {
          if (fs.existsSync(path.join(reposRoot, a, b, c, 'metadata.json'))) out.push(`${a}/${b}/${c}`);
        }
      }
    }
    return out.sort();
  };
  const first = (record) => (record && typeof record === 'object' ? Object.values(record)[0] ?? '' : '');

  /** The platform's summary of a repo whose metadata cannot be read (summary_metadata.rs). */
  const BAD_SUMMARY = {
    name: '? Bad Metadata JSON ?', description: '?', abbreviation: '?', generated_date: '?', flavor_type: '?',
    flavor: '?', language_code: '?', language_name: '?', script_direction: '?', book_codes: [], timestamp: 0,
  };

  /** One RepoSummary (serverApi.ts:27-40), derived from metadata.json as the platform does. */
  const summaryOf = (repoPath) => {
    let meta;
    try {
      meta = readMeta(repoPath);
    } catch {
      return BAD_SUMMARY;
    }
    const id = meta.identification ?? {};
    const locale = meta.meta?.defaultLocale ?? 'en';
    const lang = (meta.languages ?? [])[0] ?? {};
    const ft = meta.type?.flavorType ?? {};
    return {
      name: id.name?.[locale] ?? first(id.name),
      description: id.description?.[locale] ?? first(id.description),
      abbreviation: id.abbreviation?.[locale] ?? first(id.abbreviation),
      generated_date: meta.meta?.dateCreated ?? '',
      flavor_type: ft.name ?? '',
      flavor: ft.flavor?.name ?? '',
      language_code: lang.tag ?? '',
      language_name: lang.name?.en ?? first(lang.name),
      script_direction: lang.scriptDirection ?? '?',
      book_codes: Object.keys(ft.currentScope ?? {}).sort(),
      timestamp: Math.floor(Date.now() / 1000),
    };
  };

  /** Re-register one ingredient (md5/size) in metadata.json; scripture rescans its scope. */
  const registerIngredients = (repoPath) => {
    const dir = mustRepo(repoPath);
    const meta = readMeta(repoPath);
    const ingredients = {};
    for (const rel of walk(path.join(dir, 'ingredients'), { skipHidden: true, skipBak: true })) {
      const key = `ingredients/${rel}`;
      const bytes = fs.readFileSync(path.join(dir, key));
      const ext = path.extname(rel);
      ingredients[key] = {
        ...(meta.ingredients?.[key] ?? {}),
        checksum: { md5: md5(bytes) },
        mimeType: meta.ingredients?.[key]?.mimeType ?? (ext === '.usfm' ? 'text/plain' : (MIME[ext] ?? 'application/octet-stream').split(';')[0]),
        size: bytes.length,
      };
    }
    meta.ingredients = ingredients;
    if (meta.type?.flavorType?.name === 'scripture') {
      const scope = {};
      for (const key of Object.keys(ingredients)) {
        const m = key.match(/^ingredients\/([A-Z0-9]{3})\.usfm$/);
        if (m) scope[m[1]] = meta.type.flavorType.currentScope?.[m[1]] ?? [];
      }
      meta.type.flavorType.currentScope = scope;
    }
    fs.writeFileSync(path.join(dir, 'metadata.json'), `${JSON.stringify(meta, null, 2)}\n`);
  };

  const markPending = (repoPath, ipath, changeType) => {
    if (!pending.has(repoPath)) pending.set(repoPath, new Map());
    pending.get(repoPath).set(`ingredients/${ipath}`, changeType);
  };

  const readJsonBody = (body) => {
    try {
      return body.length ? JSON.parse(body.toString('utf8')) : {};
    } catch {
      throw new HttpError(400, 'body is not JSON');
    }
  };

  // ---- the routes (serverApi.ts wrapper line numbers in brackets) ------------

  /** [method, route prefix, handler(rest, url, body) -> {status?, json?|text?|bytes?, type?}] */
  const routes = [
    ['GET', '/version', () => ({ json: { pkg_version: '0.18.15', mock: 'tc4-baseline' } })], // getVersion 329
    ['GET', '/git/list-local-repos', () => ({ json: listRepos() })], // listLocalRepos 334
    ['GET', '/burrito/metadata/summaries', (rest, url) => { // getSummaries 350
      const org = url.searchParams.get('org');
      const out = {};
      for (const repo of listRepos()) {
        if (org && !repo.startsWith(`${org}/`)) continue;
        out[repo] = summaryOf(repo);
      }
      return { json: out };
    }],
    ['GET', '/burrito/metadata/summary/', (rest) => ({ json: summaryOf(takeRepo(rest)[0]) })], // getSummary 361
    ['GET', '/burrito/metadata/raw/', (rest) => ({ json: readMeta(takeRepo(rest)[0]) })], // getMetadataRaw 377
    ['GET', '/burrito/ingredient/raw/', (rest, url) => { // readIngredient 384
      const [repo] = takeRepo(rest);
      // A missing repo reads like a missing file: the platform only joins paths.
      const file = path.join(repoDir(repo), 'ingredients', ipathOf(url));
      if (!fs.existsSync(file) || !fs.statSync(file).isFile()) throw new HttpError(400, ENOENT);
      return { bytes: fs.readFileSync(file), type: 'text/plain; charset=utf-8' };
    }],
    ['GET', '/burrito/ingredient/bytes/', (rest, url) => { // ingredientBytesUrl 407
      const [repo] = takeRepo(rest);
      const ipath = ipathOf(url);
      const file = path.join(repoDir(repo), 'ingredients', ipath);
      if (!fs.existsSync(file) || !fs.statSync(file).isFile()) throw new HttpError(400, ENOENT);
      return { bytes: fs.readFileSync(file), type: MIME[path.extname(ipath)] ?? 'application/octet-stream' };
    }],
    ['GET', '/burrito/ingredient/zipped/', (rest, url) => { // readIngredientZipped 397
      const [repo] = takeRepo(rest);
      const dir = path.join(mustRepo(repo), 'ingredients', ipathOf(url));
      if (!fs.existsSync(dir) || !fs.statSync(dir).isDirectory()) throw new HttpError(400, 'could not locate repo or ingredient directory');
      const files = {};
      for (const rel of walk(dir)) files[rel] = fs.readFileSync(path.join(dir, rel));
      return { bytes: Buffer.from(zipSync(files)), type: 'application/zip' };
    }],
    ['GET', '/burrito/zipped/', (rest) => { // getZippedRepo 369
      const dir = mustRepo(takeRepo(rest)[0]);
      const files = {};
      for (const rel of walk(dir)) files[rel] = fs.readFileSync(path.join(dir, rel));
      return { bytes: Buffer.from(zipSync(files)), type: 'application/zip' };
    }],
    ['GET', '/burrito/paths/', (rest) => { // listPaths 419; a missing repo lists no paths
      return { json: walk(path.join(repoDir(takeRepo(rest)[0]), 'ingredients'), { skipHidden: true, skipBak: true }) };
    }],
    ['GET', '/git/status/', (rest) => { // gitStatus 428
      const [repo] = takeRepo(rest);
      mustRepo(repo);
      return { json: [...(pending.get(repo) ?? new Map())].map(([p, change_type]) => ({ path: p, change_type })) };
    }],
    ['POST', '/burrito/ingredient/raw/', (rest, url, body) => { // writeIngredient 436
      const [repo] = takeRepo(rest);
      const ipath = ipathOf(url);
      const { payload } = readJsonBody(body);
      if (typeof payload !== 'string') throw new HttpError(400, 'payload must be a string');
      const file = path.join(mustRepo(repo), 'ingredients', ipath);
      const existed = fs.existsSync(file);
      fs.mkdirSync(path.dirname(file), { recursive: true });
      if (existed && !url.searchParams.has('no_bak')) fs.copyFileSync(file, `${file}.bak`);
      fs.writeFileSync(file, payload);
      markPending(repo, ipath, existed ? 'modified' : 'new');
      if (url.searchParams.has('update_ingredients')) registerIngredients(repo);
      return { json: OK };
    }],
    ['POST', '/burrito/ingredient/delete/', (rest, url) => { // deleteIngredient 466
      const [repo] = takeRepo(rest);
      const ipath = ipathOf(url);
      const file = path.join(mustRepo(repo), 'ingredients', ipath);
      if (!fs.existsSync(file)) throw new HttpError(400, `No such file: ${file}`);
      fs.renameSync(file, `${file}.bak`);
      markPending(repo, ipath, 'deleted');
      return { json: OK };
    }],
    ['POST', '/burrito/metadata/remake-ingredients/', (rest) => { // remakeIngredients 474
      registerIngredients(takeRepo(rest)[0]);
      return { json: OK };
    }],
    ['POST', '/git/add-and-commit/', (rest, url, body) => { // addAndCommit 484
      const [repo] = takeRepo(rest);
      mustRepo(repo);
      commits.push({ repo, message: readJsonBody(body).commit_message ?? '', at: new Date().toISOString() });
      pending.delete(repo);
      return { json: OK };
    }],
    ['GET', '/git/remotes/', (rest) => { // listRemotes 496
      const file = path.join(mustRepo(takeRepo(rest)[0]), '.remotes.json');
      return { json: { is_good: true, payload: { remotes: fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : [] } } };
    }],
    ['POST', '/git/remote/add/', (rest, url) => { // addRemote 508
      const file = path.join(mustRepo(takeRepo(rest)[0]), '.remotes.json');
      const remotes = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : [];
      const name = url.searchParams.get('remote_name');
      if (remotes.some((r) => r.name === name)) return { json: { is_good: false, reason: `remote ${name} already exists` } };
      remotes.push({ name, url: url.searchParams.get('remote_url') });
      fs.writeFileSync(file, JSON.stringify(remotes));
      return { json: OK };
    }],
    ['POST', '/git/push/', () => (netEnabled // push 527
      ? { json: { is_good: false, reason: 'push is not implemented by the tc4-baseline mock' } }
      : { status: 401, json: { is_good: false, reason: 'offline mode' } })],
    // The sync transport (#673, #676). There is no git, so a clone fails the way the
    // platform answers a failed clone, and tC4 shows its refusal for that.
    ['POST', '/git/clone-repo/', () => (netEnabled // cloneRepo 543
      ? { status: 400, json: { is_good: false, reason: 'clone is not implemented by the tc4-baseline mock' } }
      : { status: 401, json: { is_good: false, reason: 'offline mode' } })],
    ['POST', '/git/copy/', () => ({ status: 400, json: { is_good: false, reason: 'copy is not implemented by the tc4-baseline mock' } })], // copyRepo 552
    ['GET', '/git/branches/', (rest) => { // listBranches 562
      mustRepo(takeRepo(rest)[0]);
      return { json: { is_good: true, payload: { branches: [{ name: 'main', is_head: true }] } } };
    }],
    ['POST', '/git/pull-repo/', () => ({ status: 400, json: { is_good: false, reason: 'pull is not implemented by the tc4-baseline mock' } })], // pullRepo 573
    ['POST', '/git/new-text-translation', () => ({ json: { is_good: false, reason: 'not implemented by the tc4-baseline mock' } })], // 607
    ['POST', '/git/new-obs-resource', () => ({ json: { is_good: false, reason: 'not implemented by the tc4-baseline mock' } })], // 653
    ['POST', '/git/new-scripture-book/', () => ({ json: { is_good: false, reason: 'not implemented by the tc4-baseline mock' } })], // 675
    ['POST', '/git/delete/', (rest) => { // deleteRepo 343
      // pankosmia-web delete_repo.rs: remove_dir_all, and its io error as HTTP 400. tC4 reads
      // that error as "not there" (serverApi.ts isNotFound), so a sync or an open that clears
      // a scratch folder goes on. A folder that is there is never deleted here.
      if (!fs.existsSync(repoDir(takeRepo(rest)[0]))) throw new HttpError(400, `could not delete repo: ${IO_ENOENT}`);
      return { json: { is_good: false, reason: 'not implemented by the tc4-baseline mock' } };
    }],
    ['GET', '/content-utils/versifications', () => ({ json: VERSIFICATIONS })], // getVersifications 680
    ['GET', '/content-utils/versification/', (rest) => { // getVersification 685
      const name = decodeURIComponent(rest);
      const file = path.join(state, 'vrs', `${name}.json`);
      if (!VERSIFICATIONS.includes(name) || !fs.existsSync(file)) throw new HttpError(400, `unknown versification ${name}`);
      return { bytes: fs.readFileSync(file), type: 'application/json' };
    }],
    ['POST', '/app-state/current-project/', (rest) => { // setCurrentProject 692
      mustRepo(takeRepo(rest)[0]);
      return { json: OK };
    }],
    ['GET', '/client-settings/', (rest) => { // getClientSettings 701, getClientSettingsText 709
      const file = path.join(settingsDir, `${decodeURIComponent(rest)}.json`);
      return fs.existsSync(file) ? { bytes: fs.readFileSync(file), type: 'application/json' } : { json: {} };
    }],
    ['POST', '/client-settings/', (rest, url, body) => { // setClientSettings 716
      const { settings } = readJsonBody(body);
      if (!settings || typeof settings !== 'object' || Array.isArray(settings)) throw new HttpError(400, 'settings must be a JSON object');
      fs.mkdirSync(settingsDir, { recursive: true });
      fs.writeFileSync(path.join(settingsDir, `${decodeURIComponent(rest)}.json`), JSON.stringify(settings, null, 2));
      return { json: OK };
    }],
    ['GET', '/net/status', () => ({ json: { is_enabled: netEnabled } })], // getNetEnabled 728
    ['POST', '/net/enable', () => { netEnabled = true; return { json: OK }; }], // 736
    ['POST', '/net/disable', () => { netEnabled = false; return { json: OK }; }], // 741
    ['POST', '/burrito/zipped/', () => ({ json: { is_good: false, reason: 'burrito import is not implemented by the tc4-baseline mock' } })], // 753
    ['POST', '/temp/bytes', () => ({ status: 400, json: { is_good: false, reason: 'temp upload is not implemented by the tc4-baseline mock' } })], // 790
    ['POST', '/burrito/remake_burrito_from_zip/', () => ({ json: { is_good: false, reason: 'not implemented by the tc4-baseline mock' } })], // 816
    ['GET', '/gitea/remote-repos/', () => (netEnabled ? { json: [] } : { json: { is_good: false, reason: 'offline mode' } })], // remoteRepos 825
  ];

  const send = (res, status, type, bytes) => {
    res.writeHead(status, { 'Content-Type': type, 'Content-Length': bytes.length, 'Cache-Control': 'no-store' });
    res.end(bytes);
  };

  const handleApi = (req, res, url, body) => {
    const route = url.pathname.slice('/api'.length);
    const entry = routes.find(([method, prefix]) =>
      method === req.method && (prefix.endsWith('/') ? route.startsWith(prefix) : route === prefix));
    let status = 200;
    try {
      if (!entry) throw new HttpError(404, `no such route in the tc4-baseline mock: ${req.method} ${route}`);
      const out = entry[2](route.slice(entry[1].length), url, body) ?? {};
      status = out.status ?? 200;
      if (out.bytes) send(res, status, out.type ?? 'application/octet-stream', out.bytes);
      else send(res, status, 'application/json', Buffer.from(JSON.stringify(out.json ?? OK)));
    } catch (error) {
      status = error instanceof HttpError ? error.status : 500;
      if (!(error instanceof HttpError)) console.error(`[mock] ${req.method} ${url.pathname}: ${error.stack}`);
      send(res, status, 'application/json', Buffer.from(JSON.stringify({ is_good: false, reason: error.message })));
    }
    requests.push({ method: req.method, path: url.pathname, query: url.search, status, at: Date.now() });
  };

  const handleStatic = (req, res, url) => {
    if (url.pathname === '/' || url.pathname === '/clients/uw-tc4') {
      res.writeHead(302, { Location: '/clients/uw-tc4/' });
      return res.end();
    }
    if (url.pathname === '/__mock/requests') return send(res, 200, 'application/json', Buffer.from(JSON.stringify(requests)));
    if (url.pathname === '/__mock/commits') return send(res, 200, 'application/json', Buffer.from(JSON.stringify(commits)));
    const prefix = '/clients/uw-tc4/';
    if (!url.pathname.startsWith(prefix)) return send(res, 404, 'text/plain', Buffer.from('not found'));
    let rel = decodeURIComponent(url.pathname.slice(prefix.length)) || 'index.html';
    if (rel.endsWith('/')) rel += 'index.html';
    const file = path.resolve(client, rel);
    if (!file.startsWith(path.resolve(client)) || !fs.existsSync(file) || !fs.statSync(file).isFile()) {
      return send(res, 404, 'text/plain', Buffer.from(`not found: ${url.pathname}`));
    }
    return send(res, 200, MIME[path.extname(file)] ?? 'application/octet-stream', fs.readFileSync(file));
  };

  const server = http.createServer((req, res) => {
    const chunks = [];
    req.on('data', (c) => chunks.push(c));
    req.on('end', () => {
      const url = new URL(req.url, 'http://127.0.0.1');
      if (url.pathname.startsWith('/api/')) handleApi(req, res, url, Buffer.concat(chunks));
      else handleStatic(req, res, url);
    });
  });
  return { server, requests, commits };
}

export function startMockServer({ state, client, port = 19998, host = '127.0.0.1' }) {
  const mock = createMockServer({ state, client });
  return new Promise((resolve, reject) => {
    mock.server.once('error', reject);
    mock.server.listen(port, host, () => resolve({ ...mock, origin: `http://${host}:${port}`, close: () => new Promise((r) => mock.server.close(r)) }));
  });
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (name, fallback) => {
    const at = process.argv.indexOf(name);
    return at > 0 ? process.argv[at + 1] : fallback;
  };
  const state = path.resolve(arg('--state', path.join(BASELINE, '.state', 'sample')));
  const client = path.resolve(arg('--client', path.join(BASELINE, '.cache', 'tc4-dist')));
  const port = Number(arg('--port', process.env.TC4_RIG_PORT ?? 19998));
  if (!fs.existsSync(path.join(state, 'repos'))) {
    console.error(`no state at ${state}: run \`npm run state\` (or \`npm run state -- --scenario en-pins\`) first`);
    process.exit(2);
  }
  const { origin } = await startMockServer({ state, client, port });
  console.log(`tc4-baseline mock server: ${origin}/clients/uw-tc4/  (state ${state})`);
}
