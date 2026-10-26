// notices.mjs on the real Cargo.lock and a small bundled store; cargo runs offline.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { bundledInstalls, lockedCrates, missing, notices } from './notices.mjs';

process.env.CARGO_NET_OFFLINE = 'true';
const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const TN = { repoPath: 'git.door43.org/unfoldingWord/en_tn', version: 'v91', sha: 'e'.repeat(40) };
const UGL = { repoPath: 'git.door43.org/uW/en_ugl', sha: 'd'.repeat(40) };

/** A bundled store of one burrito export and one lexicon, laid out as bundle_store does. */
function bundle() {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'notices-'));
  const metadata = (title) => ({
    identification: { name: { en: title } },
    copyright: { shortStatements: [{ statement: '© unfoldingWord 2026, CC BY-SA 4.0' }] },
  });
  const install = (folder, pin, packaging, file, body) => {
    const at = path.join(dir, 'dcs', ...folder, pin.sha);
    fs.mkdirSync(path.join(at, 'content'), { recursive: true });
    fs.writeFileSync(path.join(at, 'receipt.json'), JSON.stringify({ pin, packaging }));
    fs.writeFileSync(path.join(at, 'content', file), JSON.stringify(body));
  };
  install(['unfoldingword', 'en_tn'], TN, 'burritoExport', 'metadata.json', metadata('Translation Notes'));
  install(['uw', 'en_ugl'], UGL, 'lexicon', 'lexicon.json', { metadata: metadata('Greek Lexicon') });
  fs.writeFileSync(path.join(dir, 'BUNDLE-MANIFEST.json'), JSON.stringify({ manifestVersion: 1, installs: [TN, UGL] }));
  const electronite = path.join(dir, 'ELECTRONITE-LICENSE');
  fs.writeFileSync(electronite, 'Copyright (c) Electron contributors');
  return { bundle: dir, electroniteLicense: electronite };
}

const given = bundle();
const text = notices(given);
const crates = lockedCrates(fs.readFileSync(path.join(repo, 'Cargo.lock'), 'utf8'));
const installs = bundledInstalls(given.bundle);

/** `text` without the block whose heading line is `heading`. */
function without(heading) {
  const at = text.indexOf(`\n${heading}\n`);
  assert.notEqual(at, -1, heading);
  const end = text.indexOf('\n=====', at + 1);
  return text.slice(0, at) + text.slice(end);
}

test('every_crate_in_cargo_lock_has_a_notice', () => {
  assert.ok(crates.length > 100, `${crates.length} crates`);
  assert.deepEqual(missing(text, crates, []), []);
  const [name, version] = crates[crates.length >> 1];
  assert.deepEqual(missing(without(`Crate ${name} ${version}`), crates, []), [`crate ${name} ${version}`]);
});

test('every_bundled_resource_has_an_attribution', () => {
  assert.deepEqual(missing(text, [], installs), []);
  assert.match(text, /Title: Translation Notes\nCreator: unfoldingWord .*\nRights: © unfoldingWord 2026, CC BY-SA 4\.0\nSource: https:\/\/git\.door43\.org\/unfoldingWord\/en_tn release v91, commit e{40}\n/);
  assert.match(text, /Title: Greek Lexicon\n[^]*?Changes: repackaged/);
  assert.deepEqual(missing(without(`Resource ${UGL.repoPath} @ ${UGL.sha}`), [], installs), [`resource ${UGL.repoPath} @ ${UGL.sha}`]);
});

test('the notices hold the licences of the app, its notices, Electronite and the fonts', () => {
  for (const heading of ['translationCore 4 (tc-rst)', 'tc-door43 NOTICE', 'tc-align NOTICE', 'Font licence OFL-CharisSIL.txt', 'Font licence OFL-AwamiNastaliq.txt']) {
    assert.ok(text.includes(`\n${heading}\n`), heading);
  }
  assert.ok(text.includes('Copyright (c) Electron contributors'));
});
