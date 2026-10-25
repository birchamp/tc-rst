'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const mirror = require('../mirror.cjs');

const vectors = require('../../fixtures/shell/buffer-vectors.json');

const sorted = (lines) => [...lines].sort();

test('mirror_reduces_like_current_lines', () => {
  assert.equal(vectors.version, 1);
  assert.ok(vectors.vectors.length >= 12);
  for (const vector of vectors.vectors) {
    const kept = vector.keep.map((i) => vector.lines[i]);
    assert.deepEqual(sorted(mirror.reduce(vector.lines)), sorted(kept), vector.name);
  }
});

function tempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-shell-mirror-'));
}

// ---- The model: random histories, a crash that cuts the file at a random byte ----

function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const RUNS = ['abababababababababababababababab', 'cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd'];
const PLACES = ['TIT 1:1', 'TIT 1:2', 'TIT 1:3'];
const TEXTS = ['Paul', 'Paul, a servant', 'é', 'é', 'κύριος', 'خادم'];

function key(place) {
  return { project: 'en_ult', id: '6f1c0e9a-2b7d-4c3e-8a5f-0b1d2c3e4f50', field: { kind: 'verse', place } };
}

function randomLine(random) {
  const pick = (list) => list[Math.floor(random() * list.length)];
  const place = pick(PLACES);
  const instance = pick(RUNS);
  const r = random();
  if (r < 0.7) {
    const entry = { api: 1, instance, base: [], text: pick(TEXTS), sent: null, at: Math.floor(random() * 1e6), draft: null };
    return JSON.stringify({ type: 'changed', value: { key: key(place), entry } });
  }
  const value = { key: key(place), instance };
  if (r < 0.85) value.text = pick(TEXTS);
  return JSON.stringify({ type: 'removed', value });
}

class Crash extends Error {}

// fs.promises, but it stops for good once `budget` bytes have been written: the write that
// crosses the budget writes only its first bytes, as a power cut leaves a file. Renames and
// fsyncs after that do not happen either.
function crashingFs(budget) {
  let left = budget;
  let crashed = false;
  const guard = () => {
    if (crashed) throw new Crash('crashed');
  };
  const spend = async (handle, text) => {
    guard();
    const bytes = Buffer.from(text, 'utf8');
    if (bytes.length <= left) {
      left -= bytes.length;
      return handle.appendFile(bytes);
    }
    await handle.appendFile(bytes.subarray(0, left));
    left = 0;
    crashed = true;
    throw new Crash('crashed');
  };
  const wrap = (handle, flags) => ({
    writeFile: async (text) => {
      guard();
      if (flags === 'w') await handle.truncate(0);
      return spend(handle, text);
    },
    appendFile: (text) => spend(handle, text),
    sync: async () => {
      guard();
      return handle.sync();
    },
    truncate: async (n) => {
      guard();
      return handle.truncate(n);
    },
    close: () => handle.close(),
  });
  return {
    crashed: () => crashed,
    mkdir: (...a) => fs.promises.mkdir(...a),
    readFile: (...a) => fs.promises.readFile(...a),
    open: async (file, flags) => {
      guard();
      const handle = await fs.promises.open(file, flags === 'w' ? 'w' : flags);
      return wrap(handle, flags);
    },
    rename: async (...a) => {
      guard();
      return fs.promises.rename(...a);
    },
  };
}

// The launch of a run: the mirror opens over the log the last run left, and the page appends
// `lines`. A crash may come at any byte, during the launch's rewrite or during the appends.
async function launch(dir, lines, budget) {
  const fsp = crashingFs(budget);
  let written = 0;
  try {
    const m = await mirror.openMirror(dir, { fsp, onError: () => {} });
    for (const line of lines) {
      m.append(line);
      await m.flush();
      if (fsp.crashed()) break;
      written++;
    }
    if (!fsp.crashed()) await m.close();
  } catch (error) {
    if (!(error instanceof Crash)) throw error;
  }
  return written;
}

test('random_mirror_histories_keep_every_unsaved_line', async () => {
  const base = Number(process.env.SHELL_MODEL_SEED || Date.now() % 1e9);
  for (let i = 0; i < 150; i++) {
    const seed = base + i;
    const random = rng(seed);
    const dir = tempDir();
    try {
      // Every line the page sent, in order; a line whose append the crash cut is `maybe`: the
      // cut may have come after its last byte but before its newline, and then it counts.
      const history = [];
      for (let run = 0; run < 4; run++) {
        const lines = Array.from({ length: 1 + Math.floor(random() * 8) }, () => randomLine(random));
        const size = Buffer.byteLength(lines.join('\n')) + 4000;
        const budget = random() < 0.6 ? Math.floor(random() * size) : Infinity;
        const written = await launch(dir, lines, budget);
        history.push(...lines.slice(0, written).map((line) => ({ line, maybe: false })));
        if (written < lines.length) history.push({ line: lines[written], maybe: true });
      }
      const m = await mirror.openMirror(dir);
      const got = sorted(m.take());
      await m.close();
      const maybes = history.filter((h) => h.maybe).length;
      const wanted = [];
      for (let mask = 0; mask < 1 << maybes; mask++) {
        let bit = 0;
        const lines = history.filter((h) => !h.maybe || (mask >> bit++) & 1).map((h) => h.line);
        wanted.push(sorted(mirror.reduce(lines)));
      }
      assert.ok(
        wanted.some((w) => JSON.stringify(w) === JSON.stringify(got)),
        `seed ${seed} (SHELL_MODEL_SEED=${seed}): the log holds ${JSON.stringify(got)}, want ${JSON.stringify(wanted[0])}`,
      );
    } finally {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  }
});

test('a_launch_rewrites_the_log_reduced_and_keeps_a_torn_tail_out', async () => {
  const dir = tempDir();
  try {
    const lines = vectors.vectors[0].lines;
    fs.writeFileSync(path.join(dir, mirror.LOG), `${lines[0]}\n${lines[1]}\n${lines[1].slice(0, 40)}`);
    const m = await mirror.openMirror(dir);
    assert.deepEqual(m.take(), [lines[1]]);
    assert.equal(fs.readFileSync(path.join(dir, mirror.LOG), 'utf8'), `${lines[1]}\n`);
    m.append(lines[0]);
    assert.deepEqual(m.take(), [lines[0]], 'takeBuffered answers with a line before it reaches the disk');
    await m.close();
    assert.equal(fs.readFileSync(path.join(dir, mirror.LOG), 'utf8'), `${lines[1]}\n${lines[0]}\n`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

// fs.promises whose appends fail `failures` times, each after writing half its bytes, as a full
// disk does; and whose rename fails when `renameFails`, as on Windows while another program holds
// the log.
function fullDiskFs({ failures = 0, renameFails = false }) {
  let left = failures;
  return {
    mkdir: (...a) => fs.promises.mkdir(...a),
    readFile: (...a) => fs.promises.readFile(...a),
    rename: async (...a) => {
      if (renameFails) throw Object.assign(new Error('EBUSY'), { code: 'EBUSY' });
      return fs.promises.rename(...a);
    },
    open: async (file, flags) => {
      const handle = await fs.promises.open(file, flags);
      return {
        writeFile: (text) => handle.writeFile(text),
        appendFile: async (text) => {
          if (left > 0) {
            left--;
            const bytes = Buffer.from(text, 'utf8');
            await handle.appendFile(bytes.subarray(0, Math.floor(bytes.length / 2)));
            throw Object.assign(new Error('ENOSPC'), { code: 'ENOSPC' });
          }
          return handle.appendFile(text);
        },
        sync: () => handle.sync(),
        close: () => handle.close(),
      };
    },
  };
}

const lineOf = (place, text, at) =>
  JSON.stringify({
    type: 'changed',
    value: { key: key(place), entry: { api: 1, instance: RUNS[0], base: [], text, sent: null, at, draft: null } },
  });

test('a_full_disk_keeps_the_lines_and_writes_them_when_space_returns', async () => {
  const dir = tempDir();
  try {
    const errors = [];
    const m = await mirror.openMirror(dir, { fsp: fullDiskFs({ failures: 2 }), onError: (e) => errors.push(e.code) });
    m.append(lineOf('TIT 1:1', 'old one', 1));
    await m.flush();
    m.append(lineOf('TIT 1:1', 'newer words', 2));
    m.append(lineOf('TIT 1:2', 'other verse', 3));
    // The retry comes a second later; flush does not wait for space that never comes.
    await mirror_settle(m);
    await m.close();
    assert.deepEqual(errors, ['ENOSPC', 'ENOSPC']);
    const again = await mirror.openMirror(dir);
    const texts = again.take().map((l) => JSON.parse(l).value.entry.text).sort();
    await again.close();
    assert.deepEqual(texts, ['newer words', 'other verse']);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

// Flushes until every queued line is written, as space comes back.
async function mirror_settle(m) {
  for (let i = 0; i < 10; i++) await m.flush();
}

test('a_launch_whose_rewrite_fails_still_offers_and_mirrors', async () => {
  const dir = tempDir();
  try {
    const file = path.join(dir, mirror.LOG);
    const kept = lineOf('TIT 1:1', 'from the last run', 1);
    fs.writeFileSync(file, `${kept}\n${kept.slice(0, 30)}`);
    const errors = [];
    const m = await mirror.openMirror(dir, { fsp: fullDiskFs({ renameFails: true }), onError: (e) => errors.push(e.code) });
    assert.deepEqual(errors, ['EBUSY']);
    assert.deepEqual(m.take(), [kept]);
    const typed = lineOf('TIT 1:2', 'typed this run', 2);
    m.append(typed);
    await m.close();
    const again = await mirror.openMirror(dir);
    assert.deepEqual(sorted(again.take()), sorted([kept, typed]));
    await again.close();
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
