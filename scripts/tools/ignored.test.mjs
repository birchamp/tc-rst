// INV-NO-BLESS-IN-CI: a gate run never rewrites what it checks, and no ignored test is silently
// left out of CI.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (file) => fs.readFileSync(path.join(ROOT, file), 'utf8');

function rustFiles(dir, out = []) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    if (e.name === 'target' || e.name === 'node_modules') continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) rustFiles(p, out);
    else if (e.name.endsWith('.rs')) out.push(p);
  }
  return out;
}

test('INV-NO-BLESS-IN-CI: every blessing variable is refused by the gate script', () => {
  const refused = read('scripts/gates.sh').match(/^for name in ([A-Z_ ]+); do$/m)?.[1].split(' ') ?? [];
  assert.ok(refused.length > 0, 'scripts/gates.sh has no refusal list');
  const problems = [];
  for (const file of rustFiles(path.join(ROOT, 'crates'))) {
    for (const m of fs.readFileSync(file, 'utf8').matchAll(/env::var(?:_os)?\("([A-Z_]+)"\)/g)) {
      const name = m[1];
      if ((name.startsWith('UPDATE_') || name.endsWith('_BLESS') || name === 'TC_WRITE_PROBE') && !refused.includes(name)) {
        problems.push(`INV-NO-BLESS-IN-CI: ${path.relative(ROOT, file)} reads ${name}, which scripts/gates.sh does not refuse; add it to the refusal list`);
      }
    }
  }
  assert.deepEqual([...new Set(problems)], []);
});

test('INV-NO-BLESS-IN-CI: every ignored test names its job', () => {
  const jobs = new Set();
  for (const wf of fs.readdirSync(path.join(ROOT, '.github/workflows'))) {
    const body = read(`.github/workflows/${wf}`).split(/^jobs:\s*$/m)[1] ?? '';
    for (const m of body.matchAll(/^ {2}([a-z0-9-]+):\s*$/gm)) jobs.add(m[1]);
  }
  const problems = [];
  for (const [i, line] of read('scripts/ignored-tests.txt').split('\n').entries()) {
    if (!line.trim() || line.startsWith('#')) continue;
    const [name, where] = line.split('\t');
    if (!where?.trim()) problems.push(`INV-NO-BLESS-IN-CI: scripts/ignored-tests.txt:${i + 1} ${name} names no job`);
    else if (!where.startsWith('local: ') && !jobs.has(where.trim())) problems.push(`INV-NO-BLESS-IN-CI: scripts/ignored-tests.txt:${i + 1} ${name} names job ${where}, which no workflow has`);
  }
  assert.deepEqual(problems, []);
});
