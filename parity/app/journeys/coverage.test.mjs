// INV-P1-COVERED: every live tC4 journey and every tC4 e2e spec maps to journey tests that exist,
// or to a card that is not done. The map is coverage.json beside this file.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { states, titlesOf } from '../../../.audit/tools/claims.mjs';

const REPO = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..', '..');
const coverage = JSON.parse(fs.readFileSync(path.join(REPO, 'parity/app/journeys/coverage.json'), 'utf8'));
const known = states(REPO);

/** The problems of one mapping: a missing file or test, and a pending card that is done. */
function problems(id, mapping) {
  const out = [];
  for (const part of Array.isArray(mapping) ? mapping : [mapping]) {
    for (const card of [part.pending ?? []].flat()) {
      if (!known.has(card)) out.push(`INV-P1-COVERED: ${id} is pending on ${card}, but no card ${card} exists`);
      else if (known.get(card) === 'done') out.push(`INV-P1-COVERED: ${id} is pending on ${card}, but ${card} is done; add its journey`);
    }
    const file = part.file ?? part.rust;
    if (!file) continue;
    const full = path.join(REPO, file);
    if (!fs.existsSync(full)) {
      out.push(`INV-P1-COVERED: ${id} maps to ${file}, which does not exist`);
      continue;
    }
    const text = fs.readFileSync(full, 'utf8');
    for (const title of part.tests ?? []) {
      const found = part.rust
        ? new RegExp(`\\bfn ${title}\\b`).test(text)
        : (({ exact, patterns }) => exact.has(title) || patterns.some((re) => re.test(title)))(titlesOf(text));
      if (!found) out.push(`INV-P1-COVERED: ${id} maps to ${file}: "${title}", but that file has no such test`);
    }
    if ((part.tests ?? []).length === 0) out.push(`INV-P1-COVERED: ${id} maps to ${file} with no test named`);
  }
  return out;
}

/** The ids tC4 has that the map lacks, and the ones it maps that tC4 does not have. */
function keyProblems(kind, mapped, wanted) {
  return [
    ...wanted.filter((id) => !mapped.includes(id)).map((id) => `INV-P1-COVERED: tC4's ${kind} ${id} is not mapped`),
    ...mapped.filter((id) => !wanted.includes(id)).map((id) => `INV-P1-COVERED: ${id} is mapped but is not a live tC4 ${kind}`),
  ];
}

test('INV-P1-COVERED: every live journey is mapped', () => {
  assert.deepEqual(keyProblems('journey', Object.keys(coverage.journeys), coverage.tc4.liveJourneys), []);
  assert.deepEqual(Object.entries(coverage.journeys).flatMap(([id, m]) => problems(id, m)), []);
});

test('INV-P1-COVERED: every tC4 spec is mapped', () => {
  assert.equal(coverage.tc4.specs.length, 37, 'tC4 has 37 e2e specs at d310bf3');
  assert.deepEqual(keyProblems('spec', Object.keys(coverage.specs), coverage.tc4.specs), []);
  assert.deepEqual(Object.entries(coverage.specs).flatMap(([id, m]) => problems(id, m)), []);
});

test('INV-P1-COVERED: the reference is the baseline\'s', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(REPO, 'parity/tc4-baseline/manifest.json'), 'utf8'));
  assert.equal(coverage.tc4.commit, manifest.tc4.commit);
});
