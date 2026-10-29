// INV-DOCS-LINKED: the documents form one linked map. Every document has a row in docs/MAP.md,
// every path the entry documents name exists, and every vision commitment a journey cites exists.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (rel) => fs.readFileSync(path.join(REPO, rel), 'utf8');
const exists = (rel) => fs.existsSync(path.join(REPO, rel));

/** The Markdown files a reader can open as documents: the root and docs/, outside its evidence and rulings. */
function documents() {
  const md = (dir, deep) =>
    fs.readdirSync(path.join(REPO, dir), { withFileTypes: true }).flatMap((e) => {
      const rel = path.posix.join(dir, e.name);
      if (e.isDirectory()) return deep && !['docs/evidence', 'docs/rulings'].includes(rel) ? md(rel, deep) : [];
      return e.name.endsWith('.md') ? [rel] : [];
    });
  return [...md('.', false).map((p) => p.replace(/^\.\//, '')), ...md('docs', true)];
}

/** The first-column paths of docs/MAP.md's tables. */
function mapped() {
  return [...read('docs/MAP.md').matchAll(/^\| `([^`]+)` \|/gm)].map((m) => m[1]);
}

// A backticked token is a path to check when it starts at one of these roots and holds no placeholder.
const ROOTS = /^(docs|crates|parity|fixtures|scripts)\/|^(AGENTS|CONTRIBUTING|CLAUDE)\.md$/;

/** The repository paths a document names in backticks. */
function pathsIn(rel) {
  return [...read(rel).matchAll(/`([^`\s]+)`/g)]
    .map((m) => m[1])
    .filter((p) => ROOTS.test(p) && !/[<>*]/.test(p));
}

test('INV-DOCS-LINKED: every document has a row in docs/MAP.md', () => {
  const rows = new Set(mapped());
  assert.deepEqual(documents().filter((d) => !rows.has(d)), []);
});

test('INV-DOCS-LINKED: every row of docs/MAP.md names a file that exists', () => {
  assert.deepEqual(mapped().filter((p) => !exists(p)), []);
});

test('INV-DOCS-LINKED: every path the entry documents name exists', () => {
  const entry = ['AGENTS.md', 'CONTRIBUTING.md', 'docs/MAP.md', 'docs/VISION.md', 'docs/JOURNEYS.md', 'docs/FUNNEL.md', 'docs/GLOSSARY.md'];
  const missing = entry.flatMap((doc) => pathsIn(doc).filter((p) => !exists(p)).map((p) => `${doc}: ${p}`));
  assert.deepEqual(missing, []);
});

test('INV-DOCS-LINKED: every commitment a journey serves is in docs/VISION.md', () => {
  const vision = new Set([...read('docs/VISION.md').matchAll(/^\| (V\d+) \|/gm)].map((m) => m[1]));
  const cited = [...read('docs/JOURNEYS.md').matchAll(/\bV\d+\b/g)].map((m) => m[0]);
  assert.ok(vision.size > 0, 'docs/VISION.md has a commitments table');
  assert.deepEqual([...new Set(cited)].filter((v) => !vision.has(v)), []);
});
