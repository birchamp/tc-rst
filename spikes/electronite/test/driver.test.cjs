'use strict';

// Tests run-spike.ps1's own logic with fake-shell.cjs in place of Electronite: the passes and
// their order, the spread and what it lets Q-1 say, and when the hard kill comes.
//
//   TC4_PWSH=/path/to/pwsh node --test spikes/electronite/test/driver.test.cjs
//
// It needs PowerShell 7 (pwsh on the PATH, or TC4_PWSH) and is skipped without it. It runs on
// Linux (the fake is started through a small sh script).
const test = require('node:test');
const assert = require('node:assert/strict');
const { spawnSync } = require('node:child_process');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');

const SPIKE = path.join(__dirname, '..');

function findPwsh() {
  if (process.env.TC4_PWSH) return process.env.TC4_PWSH;
  const found = spawnSync('sh', ['-c', 'command -v pwsh'], { encoding: 'utf8' });
  return found.status === 0 ? found.stdout.trim() : null;
}
const PWSH = process.platform === 'win32' ? null : findPwsh();
const SKIP = PWSH ? false : 'needs pwsh on Linux (set TC4_PWSH)';

function executable(file, text) {
  fs.writeFileSync(file, text);
  fs.chmodSync(file, 0o755);
  return file;
}

// The painted p95 the fake reports, by mode and pass. Alone, the filter and protocol ranges
// overlap; while loading, the filter is lower in every pass.
const P95 = {
  filter: { alone: [20, 26], loading: [40, 42] },
  protocol: { alone: [24, 22], loading: [60, 65] },
  'filter-all': { alone: [21, 23], loading: [44, 47] },
};
const COMMIT_MS = 3000;
const AFTER_COMMIT_S = 1;

let report = '';
let order = [];
test.before(() => {
  if (SKIP) return;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-spike-driver-'));
  const node = process.execPath;
  const electron = executable(path.join(dir, 'electron'), `#!/bin/sh\nexec "${node}" "${path.join(__dirname, 'fake-shell.cjs')}" "$@"\n`);
  const server = executable(path.join(dir, 'tc4-standin'), `#!/bin/sh\necho '{"sha":false,"avx2":true,"avx512f":false}'\n`);
  const orderFile = path.join(dir, 'order.txt');
  const reportFile = path.join(dir, 'report.md');
  const run = spawnSync(
    PWSH,
    [
      '-NoProfile',
      '-File',
      path.join(SPIKE, 'run-spike.ps1'),
      '-Electron',
      electron,
      '-Server',
      server,
      '-Out',
      path.join(dir, 'out'),
      '-Report',
      reportFile,
      '-AfterCommit',
      String(AFTER_COMMIT_S),
      '-Only',
      'run,hardkill',
    ],
    {
      encoding: 'utf8',
      timeout: 300000,
      env: { ...process.env, FAKE_ORDER: orderFile, FAKE_P95: JSON.stringify(P95), FAKE_COMMIT_MS: String(COMMIT_MS) },
    },
  );
  assert.equal(run.status, 0, `run-spike.ps1 failed:\n${run.stdout}\n${run.stderr}`);
  report = fs.readFileSync(reportFile, 'utf8');
  order = fs.readFileSync(orderFile, 'utf8').trim().split('\n');
});

test('every mode runs once per pass, and the second pass runs them in the opposite order', { skip: SKIP }, () => {
  assert.deepEqual(order, ['filter 1', 'protocol 1', 'filter-all 1', 'filter-all 2', 'protocol 2', 'filter 2']);
});

test('the report shows each pass, the spread, and does not decide Q-1 inside the spread', { skip: SKIP }, () => {
  assert.match(report, /\| filter \| alone \| 20\.0, 26\.0 \| 20\.0 \| 26\.0 \| 6\.0 \|/);
  assert.match(report, /\| protocol \| alone \| 24\.0, 22\.0 \| 22\.0 \| 24\.0 \| 2\.0 \|/);
  assert.match(report, /Q-1, alone \(painted p95, protocol against filter\): the passes overlap: the difference is inside the spread and does not decide Q-1\./);
  assert.match(report, /Q-1, loading \(painted p95, protocol against filter\): filter lower in every pass, by 18\.0 ms or more\./);
  assert.match(report.replace(/\s+/g, ' '), /A difference inside the spread between passes does not decide Q-1\./);
});

test('the hard kill comes only after Chromium has written localStorage, and the loss is shown in seconds', { skip: SKIP }, () => {
  const rows = report.split('\n').filter((line) => /^\| (filter|protocol) \| (hard kill|normal quit)/.test(line));
  assert.equal(rows.length, 4, report);
  for (const row of rows) {
    const cells = row.split('|').map((c) => c.trim());
    const made = Number(cells[3]);
    const firstWrite = /^(\d+\.\d) s after typing began$/.exec(cells[4]);
    assert.ok(firstWrite, `first write: ${cells[4]}`);
    assert.ok(Number(firstWrite[1]) >= COMMIT_MS / 1000 - 0.1, row);
    // The script acted no earlier than the first write plus -AfterCommit seconds.
    assert.ok(made * 50 >= COMMIT_MS + AFTER_COMMIT_S * 1000, `made ${made} keystrokes before the script acted: ${row}`);
    assert.ok(Number(cells[5]) > 0, `on disk just before: ${row}`);
    assert.ok(Number(cells[7]) > 0, `localStorage kept: ${row}`);
    assert.match(cells[8], /^\d+\.\d s$/);
    assert.match(cells[10], /^\d+\.\d s$/);
  }
});
