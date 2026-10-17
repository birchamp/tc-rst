'use strict';

// Pure helpers of the spike's Electronite main script, kept apart so node can test them
// (shell/lib.test.cjs) where Electronite cannot run.

const MODES = ['filter', 'filter-all', 'protocol'];
const CASES = ['run', 'home', 'type', 'inspect'];
const SPAWNS = ['job', 'detached'];

// The shell's arguments: --spike-NAME=VALUE, after Electronite's own.
function parseArgs(argv) {
  const options = {
    mode: 'filter',
    case: 'run',
    out: null,
    server: null,
    clicks: 200,
    warmup: 20,
    port: 0,
    spawn: 'job',
  };
  for (const arg of argv) {
    const match = /^--spike-([a-z]+)=(.*)$/.exec(arg);
    if (!match) continue;
    const [, name, value] = match;
    if (name === 'clicks' || name === 'warmup' || name === 'port') {
      const number = Number(value);
      if (!Number.isInteger(number) || number < 0) throw new Error(`--spike-${name} needs a whole number`);
      options[name] = number;
    } else if (name in options) {
      options[name] = value;
    } else {
      throw new Error(`unknown argument --spike-${name}`);
    }
  }
  if (!MODES.includes(options.mode)) throw new Error(`--spike-mode must be one of ${MODES.join(', ')}`);
  if (!CASES.includes(options.case)) throw new Error(`--spike-case must be one of ${CASES.join(', ')}`);
  if (!SPAWNS.includes(options.spawn)) throw new Error(`--spike-spawn must be one of ${SPAWNS.join(', ')}`);
  if (!options.out) throw new Error('--spike-out=DIR is needed');
  if (!options.server) throw new Error('--spike-server=PATH is needed');
  return options;
}

// The address line `tc4 serve` prints first (APP-DESIGN §5.1).
function parseAddress(line) {
  let value;
  try {
    value = JSON.parse(line);
  } catch (_) {
    throw new Error('the server did not print a JSON address line');
  }
  const match = /^http:\/\/127\.0\.0\.1:(\d{1,5})\/$/.exec(value && value.url);
  if (!match) throw new Error('the address line has no loopback url');
  if (typeof value.token !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(value.token)) {
    throw new Error('the address line has no 43-character token');
  }
  const port = Number(match[1]);
  return { origin: `http://127.0.0.1:${port}`, port, token: value.token };
}

// AD11: the header filter lists only the server's paths outside /api/, so no API request and
// no event-stream read runs its JS listener. They still cross Electron's request proxy on the
// main process's UI thread, as every request of the window does (the api-main-thread probe).
// `filter-all` lists every path: what it costs if /api/ cannot be left out.
function filterPatterns(origin, mode) {
  if (mode === 'filter-all') return [`${origin}/*`];
  return [
    `${origin}/`,
    `${origin}/ui/*`,
    `${origin}/help/*`,
    `${origin}/media/*`,
    `${origin}/download/*`,
    `${origin}/print/*`,
    `${origin}/prepare.html`,
  ];
}

// A fallback filter without the port, for an Electronite whose URL patterns refuse a port;
// the listener then checks the origin and the path itself.
function portlessPatterns() {
  return ['http://127.0.0.1/*'];
}

// An URL's scheme, host and port. Node's URL gives `null` as the origin of a scheme it does not
// know (tc4:), so the origin is built by hand.
function originOf(url) {
  try {
    const parsed = new URL(url);
    return `${parsed.protocol}//${parsed.host}`;
  } catch (_) {
    return null;
  }
}

// What the AD11 filter would add the header to, decided from the URL alone.
function wantsToken(url, origin, mode) {
  if (originOf(url) !== origin) return false;
  const parsed = new URL(url);
  if (mode === 'filter-all') return true;
  const path = parsed.pathname;
  return (
    path === '/' ||
    path === '/prepare.html' ||
    ['/ui/', '/help/', '/media/', '/download/', '/print/'].some((prefix) => path.startsWith(prefix))
  );
}

// The first segment of a URL's path, for counting what the filter saw.
function pathClass(url) {
  try {
    const path = new URL(url).pathname;
    if (path === '/') return 'root';
    return path.split('/')[1] || 'root';
  } catch (_) {
    return 'unparsed';
  }
}

function sameOrigin(url, origin) {
  return originOf(url) === origin;
}

// Q-1's prototype: tc4://app/PATH goes to the server at the same path.
function proxyTarget(requestUrl, origin) {
  const url = new URL(requestUrl);
  return `${origin}${url.pathname}${url.search}`;
}

// The headers the shell sends on: the page's own, plus the token and the server's own Origin.
// Electronite hands protocol.handle a same-origin request without an Origin header, and only
// the app's own pages reach tc4://, so the shell vouches for the origin; Host is net.fetch's.
function proxyHeaders(entries, token, origin) {
  const headers = {};
  for (const [name, value] of entries) {
    const lower = name.toLowerCase();
    if (lower === 'host' || lower === 'x-tc-token' || lower === 'origin') continue;
    headers[lower] = value;
  }
  headers.origin = origin;
  headers['x-tc-token'] = token;
  return headers;
}

// The shell's mirror of typed text (AD21): one JSON line per change. The last line of each
// field wins; a line that does not parse (torn by a crash) is skipped; `removed: true` drops
// the field.
function parseMirror(text) {
  const entries = new Map();
  let lines = 0;
  let torn = 0;
  for (const line of text.split('\n')) {
    if (line === '') continue;
    lines++;
    let entry;
    try {
      entry = JSON.parse(line);
    } catch (_) {
      torn++;
      continue;
    }
    if (!entry || typeof entry.key !== 'string') {
      torn++;
      continue;
    }
    if (entry.removed) entries.delete(entry.key);
    else entries.set(entry.key, entry);
  }
  return { entries: [...entries.values()], lines, torn };
}

// The log written again with only the entries still unsaved, one line each.
function compactMirror(entries) {
  return entries.map((entry) => `${JSON.stringify(entry)}\n`).join('');
}

// Text with every launch token taken out: the `#t=` fragment of a URL, and the tokens given.
// Electron's own error messages quote the URL they failed to load, fragment and all.
function redact(text, tokens) {
  let out = String(text).replace(/#t=[A-Za-z0-9_-]+/g, '#t=<token>');
  for (const token of tokens || []) {
    if (token) out = out.split(token).join('<token>');
  }
  return out;
}

module.exports = {
  CASES,
  MODES,
  compactMirror,
  filterPatterns,
  originOf,
  parseAddress,
  parseArgs,
  parseMirror,
  pathClass,
  portlessPatterns,
  proxyHeaders,
  proxyTarget,
  redact,
  sameOrigin,
  wantsToken,
};
