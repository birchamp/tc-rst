'use strict';

// A stand-in for Electronite and shell/main.cjs that driver.test.cjs gives run-spike.ps1, so
// the driver's own logic (the order of the passes, the spread, when the hard kill comes) is
// tested without a browser. It takes the shell's arguments and writes the files the real
// shell writes, with figures the test chooses:
//   FAKE_ORDER  a file this appends "<mode> <pass>" to for each run case
//   FAKE_P95    JSON {mode: {alone: [p95 by pass], loading: [...]}} for the painted p95
//   FAKE_COMMIT_MS  when the type case first writes "localStorage" to the profile's LevelDB
//                   folder, in ms after typing begins (Chromium does it some seconds later)

const fs = require('node:fs');
const path = require('node:path');

const args = {};
for (const arg of process.argv.slice(2)) {
  const m = /^--spike-([a-z]+)=(.*)$/.exec(arg);
  if (m) args[m[1]] = m[2];
}
const out = args.out;
const mode = args.mode;
const kase = args.case;
const tag = `${mode}-${kase}`;
const EVERY = 50;
fs.mkdirSync(out, { recursive: true });

function event(ev, data) {
  fs.appendFileSync(path.join(out, `events-${tag}.jsonl`), `${JSON.stringify({ t: Date.now(), ev, ...data })}\n`);
}
function writeJson(name, value) {
  fs.writeFileSync(path.join(out, name), JSON.stringify(value, null, 2));
}
const stats = (p95) => ({ n: 20, median: p95 / 2, p95, max: p95 + 1 });
const profile = path.join(out, `profile-${mode}`);
const mirrorFile = path.join(profile, 'fake-mirror.txt');
fs.mkdirSync(profile, { recursive: true });

if (kase === 'run') {
  const pass = Number(/-(\d+)$/.exec(path.basename(out))[1]);
  fs.appendFileSync(process.env.FAKE_ORDER, `${mode} ${pass}\n`);
  event('page-loaded', {});
  const p95 = JSON.parse(process.env.FAKE_P95)[mode];
  const roundTrip = ['alone', 'loading'].map((label) => ({
    label,
    clicks: 20,
    replyBytes: 20480,
    frameTimeouts: 0,
    reply: stats(5),
    rendered: stats(6),
    frame: stats(p95[label][pass - 1] - 2),
    painted: stats(p95[label][pass - 1]),
  }));
  writeJson(`results-${tag}.json`, {
    case: 'run',
    mode,
    probes: [{ name: 'fetch', what: 'a fetch', pass: true, detail: 'ok' }],
    roundTrip,
    shell: { versions: { electron: 'fake', chrome: 'fake', node: process.versions.node, v8: 'fake' } },
  });
} else if (kase === 'type') {
  // The shell's own pid stands for the server's, so the driver finds it gone after the kill.
  writeJson(`status-${tag}.json`, { shellPid: process.pid, serverPid: process.pid, port: 1, mode, case: kase });
  const t0 = Date.now();
  event('typing-started', { t0, every: EVERY });
  const commitMs = Number(process.env.FAKE_COMMIT_MS);
  const leveldb = path.join(profile, 'Local Storage', 'leveldb');
  const typed = path.join(out, `appdata-${tag}`, 'typed.log');
  fs.mkdirSync(path.dirname(typed), { recursive: true });
  let committed = 0;
  setInterval(() => {
    const n = Math.floor((Date.now() - t0) / EVERY);
    fs.writeFileSync(mirrorFile, String(n));
    fs.appendFileSync(typed, `${JSON.stringify({ n })}\n`);
    // Chromium appends the draft to its LevelDB log some seconds after the page sets it,
    // and then about once a second.
    if (Date.now() - t0 >= commitMs && Date.now() - committed >= 1000) {
      fs.mkdirSync(leveldb, { recursive: true });
      fs.appendFileSync(path.join(leveldb, '000003.log'), `{"n":${n}}`);
      committed = Date.now();
    }
    const control = path.join(out, `control-${tag}.txt`);
    if (fs.existsSync(control) && fs.readFileSync(control, 'utf8').trim() === 'quit') process.exit(0);
  }, 10);
} else if (kase === 'inspect') {
  let local = 0;
  const log = path.join(profile, 'Local Storage', 'leveldb', '000003.log');
  if (fs.existsSync(log)) {
    for (const m of fs.readFileSync(log, 'utf8').matchAll(/"n":(\d+)/g)) local = Math.max(local, Number(m[1]));
  }
  const mirror = fs.existsSync(mirrorFile) ? Number(fs.readFileSync(mirrorFile, 'utf8')) : 0;
  writeJson(`results-${tag}.json`, {
    case: 'inspect',
    localStorage: { n: local },
    mirror: { n: mirror },
    shell: { mirrorAtLaunch: { lines: mirror, torn: 0 } },
  });
} else {
  process.exit(4);
}
