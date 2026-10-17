// The state each parity scenario starts from is the same whether build-state.mjs reads tC4's
// fixtures from a tC4 checkout or this repository's copies of them (state.mjs), so the tC4
// capture and the Rust app start from the same bytes. Without a tC4 checkout (CI), a build
// from this repository is checked against state.sha256.json, which `state.mjs --record`
// writes from a tC4 checkout.
//
//   node --test parity/app/state.test.mjs        (TC4_ROOT names another tC4 checkout)
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { buildState, SCENARIOS } from '../tc4-baseline/fixtures/build-state.mjs';
import { buildScenarioState, DIGEST_FILE, filesUnder, repoCopyOf, TC4, treeDigest, withTempDir } from './state.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const recorded = JSON.parse(fs.readFileSync(DIGEST_FILE, 'utf8'));
const baseline = JSON.parse(fs.readFileSync(path.join(HERE, '..', 'tc4-baseline', 'manifest.json'), 'utf8'));
const haveTc4 = fs.existsSync(path.join(TC4, 'conformance', 'sample-burrito'));

/** The paths that differ between two digests: missing, extra or changed. */
function differences(want, got) {
  const out = [];
  for (const rel of Object.keys(want)) if (!(rel in got)) out.push(`missing ${rel}`);
  for (const rel of Object.keys(got)) {
    if (!(rel in want)) out.push(`extra ${rel}`);
    else if (got[rel] !== want[rel]) out.push(`changed ${rel}`);
  }
  return out;
}

test('state.sha256.json was recorded from the tC4 commit of the baseline, for every scenario', () => {
  assert.equal(recorded.tc4, baseline.tc4.commit);
  assert.deepEqual(Object.keys(recorded.scenarios), Object.keys(SCENARIOS));
});

test('the state built from this repository equals the state recorded from tC4', () => {
  for (const scenario of Object.keys(SCENARIOS)) {
    const got = withTempDir((dir) => {
      buildScenarioState({ scenario, out: dir });
      return treeDigest(dir);
    });
    assert.deepEqual(differences(recorded.scenarios[scenario], got), [], `scenario ${scenario}`);
  }
});

test('state_from_repo_equals_state_from_tc4', { skip: !haveTc4 && `no tC4 checkout at ${TC4}` }, () => {
  for (const scenario of Object.keys(SCENARIOS)) {
    withTempDir((repo) => withTempDir((tc4) => {
      buildScenarioState({ scenario, out: repo });
      buildScenarioState({ scenario, out: tc4, from: TC4 });
      const files = filesUnder(tc4);
      assert.deepEqual(filesUnder(repo), files, `scenario ${scenario}: the same files`);
      for (const rel of files) {
        assert.ok(fs.readFileSync(path.join(repo, rel)).equals(fs.readFileSync(path.join(tc4, rel))), `scenario ${scenario}: ${rel}`);
      }
    }));
  }
});

test('a build from this repository reads only the copied tC4 folders', () => {
  assert.equal(repoCopyOf('conformance/sample-burrito-obs/metadata.json'), path.resolve(HERE, '..', '..', 'fixtures/tc4/sample-burrito-obs/metadata.json'));
  assert.equal(repoCopyOf('test/fixtures/vrs'), path.resolve(HERE, '..', '..', 'fixtures/tc4/test-fixtures/vrs'));
  assert.throws(() => repoCopyOf('src/i18n/en.json'), /keeps no copy/);
  assert.throws(() => repoCopyOf('conformance/sample-burrito-other/metadata.json'), /keeps no copy/);
});

test('the state check fails when a fixture copy changes', () => {
  // This repository's copy of one fixture, with one byte changed, read in place of the copy.
  const rel = 'test/fixtures/en_ust/TIT.usfm';
  withTempDir((dir) => {
    const changed = path.join(dir, 'TIT.usfm');
    const bytes = fs.readFileSync(repoCopyOf(rel));
    bytes[bytes.length - 2] ^= 1;
    fs.writeFileSync(changed, bytes);
    const out = path.join(dir, 'state');
    buildState({ tc4: (wanted) => (wanted === rel ? changed : repoCopyOf(wanted)), scenario: 'sample', out });
    assert.deepEqual(differences(recorded.scenarios.sample, treeDigest(out)), [
      'changed provenance.json',
      'changed repos/_local_/_sideloaded_/en_ust/ingredients/TIT.usfm',
      'changed repos/_local_/_sideloaded_/en_ust/metadata.json',
    ]);
  });
});
