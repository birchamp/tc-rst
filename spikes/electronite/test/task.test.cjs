'use strict';

// Checks the task the owner's Windows session follows (.audit/tasks/g0-windows.md) against the
// scripts it runs and against the Windows track it is part of.
//
//   node --test spikes/electronite/test/task.test.cjs
const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const REPO = path.join(__dirname, '..', '..', '..');
const TASK = fs.readFileSync(path.join(REPO, '.audit', 'tasks', 'g0-windows.md'), 'utf8');
// What task 3 of .audit/tasks/windows-track.md writes on its branch windows-bench.
const TRACK_STORAGE_REPORT = 'docs/evidence/storage-fold-windows-2026-10-17.md';
const SPIKE_REPORT = 'docs/evidence/electronite-spike-2026-10-17';

// The lines of every fenced code block, which the session pastes as they are.
function codeLines(text) {
  const lines = [];
  let inside = false;
  for (const line of text.split('\n')) {
    if (/^\s*```/.test(line)) {
      inside = !inside;
      continue;
    }
    if (inside) lines.push(line.trim());
  }
  assert.equal(inside, false, 'a code block is not closed');
  return lines.filter((line) => line);
}

// Every command line, in code blocks or inline, that calls one of the scripts.
function scriptCalls(script) {
  const inline = [...TASK.matchAll(/`([^`\n]+)`/g)].map((m) => m[1]);
  return [...codeLines(TASK), ...inline].filter((line) => line.includes(`${script} `));
}

function declaredParameters(file) {
  const text = fs.readFileSync(path.join(REPO, file), 'utf8');
  const block = /\nparam\(([\s\S]*?)\n\)/.exec(text);
  assert.ok(block, `no param block in ${file}`);
  const names = [...block[1].matchAll(/\$([A-Za-z]+)/g)].map((m) => m[1].toLowerCase());
  const mandatory = [...block[1].matchAll(/Mandatory = \$true\)\]\[[a-z]+\]\$([A-Za-z]+)/g)].map((m) => m[1].toLowerCase());
  return { names, mandatory };
}

test('every command is one line, with no PowerShell line continuation to break in another shell', () => {
  const lines = codeLines(TASK);
  assert.ok(lines.length > 10);
  for (const line of lines) {
    assert.ok(!line.endsWith('`'), `a PowerShell line continuation: ${line}`);
    assert.ok(!/^-[A-Z]/.test(line), `a parameter on a line of its own: ${line}`);
  }
});

test('the task says to run its commands in PowerShell, not Git Bash', () => {
  assert.match(TASK, /^## Run every command in PowerShell, not Git Bash$/m);
  assert.match(TASK, /`-Electron: command not found`, or "missing mandatory parameters"/);
});

for (const [script, file] of [
  ['run-spike.ps1', 'spikes/electronite/run-spike.ps1'],
  ['run-foldbench.ps1', 'spikes/foldbench-windows/run-foldbench.ps1'],
]) {
  test(`every call of ${script} passes only parameters it declares, and the mandatory ones`, () => {
    const { names, mandatory } = declaredParameters(file);
    const calls = scriptCalls(script);
    assert.ok(calls.length > 0, `the task never calls ${script}`);
    for (const call of calls) {
      const after = call.slice(call.indexOf(script) + script.length);
      const given = [...after.matchAll(/(?:^|\s)-([A-Za-z]+)/g)].map((m) => m[1].toLowerCase());
      for (const name of given) assert.ok(names.includes(name), `${script} has no -${name}: ${call}`);
      // A rerun with -Only names the rest of the line in the text; a full call names them all.
      if (!given.includes('only')) {
        for (const name of mandatory) assert.ok(given.includes(name), `-${name} is missing: ${call}`);
      }
    }
  });
}

test("the task reuses the Windows track's clone and never writes task 3's storage report", () => {
  assert.match(TASK, /Reuse the clone `windows-track\.md` made/);
  assert.ok(!codeLines(TASK).some((line) => line.startsWith('git clone')), 'a second clone in a code block');
  // Part 2 runs only when windows-bench lacks task 3's report, and then writes its own file.
  assert.ok(codeLines(TASK).includes(`git ls-tree --name-only origin/windows-bench ${TRACK_STORAGE_REPORT}`));
  for (const call of scriptCalls('run-foldbench.ps1')) {
    const report = /-Report (\S+)/.exec(call);
    assert.ok(report, `run-foldbench.ps1 without -Report writes the default name: ${call}`);
    assert.notEqual(report[1], TRACK_STORAGE_REPORT, call);
  }
});

test('the commit step adds only what part 1 always writes; the rest is added only if it ran', () => {
  const adds = codeLines(TASK).filter((line) => line.startsWith('git add'));
  assert.equal(adds.length, 1);
  for (const file of adds[0].split(/\s+/).slice(2)) {
    assert.ok(file.startsWith(SPIKE_REPORT), `added whether or not it was written: ${file}`);
  }
  assert.match(TASK, /If part\s+2 ran here, also add its files/);
});
