#!/usr/bin/env node
// The state each parity scenario starts from, built by parity/tc4-baseline/fixtures/build-state.mjs
// from this repository's own copies of tC4's fixtures, so the Rust app's capture needs no tC4
// checkout. build-state.mjs reads only tC4's `test/fixtures` and the two conformance samples
// (beside fixtures/vendor/, which lives here); this repository keeps byte-identical copies of
// them under fixtures/tc4 (fixtures/README.md).
//
//   node parity/app/state.mjs [--scenario sample] [--out <dir>]   build from this repository
//   node parity/app/state.mjs --record                            record state.sha256.json
//
// state.sha256.json is the sha256 of every file of every scenario as built from a tC4
// checkout (TC4_ROOT, or the default of build-state.mjs). state.test.mjs checks that a build
// from this repository gives the same files, with a tC4 checkout and without one.
import crypto from 'node:crypto';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildState, DEFAULT_TC4, SCENARIOS } from '../tc4-baseline/fixtures/build-state.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..', '..');
export const DIGEST_FILE = path.join(HERE, 'state.sha256.json');
/** The tC4 checkout to compare with: TC4_ROOT, or the one build-state.mjs defaults to. */
export const TC4 = path.resolve(process.env.TC4_ROOT ?? DEFAULT_TC4);

/** Each tC4 folder that build-state.mjs reads, and this repository's copy of it. */
export const REPO_COPIES = {
  'test/fixtures': 'fixtures/tc4/test-fixtures',
  'conformance/sample-burrito': 'fixtures/tc4/sample-burrito',
  'conformance/sample-burrito-obs': 'fixtures/tc4/sample-burrito-obs',
};

/** This repository's copy of `rel`, a path in tC4. A path outside the copied folders is an
 * error, so a build from this repository can never read anything else of tC4. */
export function repoCopyOf(rel) {
  for (const [folder, copy] of Object.entries(REPO_COPIES)) {
    if (rel === folder || rel.startsWith(`${folder}/`)) return path.join(ROOT, copy, rel.slice(folder.length));
  }
  throw new Error(`${rel}: this repository keeps no copy of it (REPO_COPIES in parity/app/state.mjs)`);
}

/** Builds `scenario` into `out`, from this repository's copies (`from: 'repo'`) or from the
 * tC4 checkout at `from`. */
export function buildScenarioState({ scenario, out, from = 'repo' }) {
  return buildState({ tc4: from === 'repo' ? repoCopyOf : from, scenario, out });
}

/** Every file under `dir`, as sorted `/`-joined paths relative to it. */
export function filesUnder(dir, rel = '') {
  return fs.readdirSync(path.join(dir, rel), { withFileTypes: true }).flatMap((entry) => {
    const sub = rel ? `${rel}/${entry.name}` : entry.name;
    return entry.isDirectory() ? filesUnder(dir, sub) : [sub];
  }).sort();
}

/** The sha256 of every file under `dir`, by path. */
export function treeDigest(dir) {
  return Object.fromEntries(filesUnder(dir).map((rel) => [rel, crypto.createHash('sha256').update(fs.readFileSync(path.join(dir, rel))).digest('hex')]));
}

/** Runs `use(dir)` with a new empty directory that is removed afterwards. */
export function withTempDir(use) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tc-parity-state-'));
  try {
    return use(dir);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

function record() {
  const commit = execFileSync('git', ['-C', TC4, 'rev-parse', 'HEAD'], { encoding: 'utf8', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } }).trim();
  const scenarios = {};
  for (const scenario of Object.keys(SCENARIOS)) {
    scenarios[scenario] = withTempDir((dir) => {
      buildScenarioState({ scenario, out: dir, from: TC4 });
      return treeDigest(dir);
    });
  }
  fs.writeFileSync(DIGEST_FILE, `${JSON.stringify({ tc4: commit, scenarios }, null, 2)}\n`);
  console.log(`${path.relative(ROOT, DIGEST_FILE)}: ${Object.keys(scenarios).length} scenarios from tC4 ${commit.slice(0, 7)} at ${TC4}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const arg = (name) => {
    const at = process.argv.indexOf(name);
    return at > 0 ? process.argv[at + 1] : undefined;
  };
  if (process.argv.includes('--record')) {
    record();
  } else {
    const scenario = arg('--scenario') ?? 'sample';
    const out = path.resolve(arg('--out') ?? path.join(HERE, '.state', scenario));
    buildScenarioState({ scenario, out });
    console.log(`state ${scenario} from this repository: ${out}`);
  }
}
