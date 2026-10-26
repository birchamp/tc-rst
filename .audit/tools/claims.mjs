// INV-CLAIMED-TESTS-EXIST: the test names a document claims, resolved against the tests that
// exist. Node built-ins only.
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

function walk(dir, keep, out = []) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return out;
  }
  for (const e of entries) {
    if (e.name === 'node_modules' || e.name === 'target' || e.name.startsWith('.')) continue;
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p, keep, out);
    else if (keep(e.name)) out.push(p);
  }
  return out;
}

/** Every name a claim may resolve to: Rust fns, journey and test titles (a template title is a
 * pattern), test-binary stems, report rows and design-system scenario ids. */
export function testNames(repo) {
  const exact = new Set();
  const patterns = [];
  for (const file of walk(path.join(repo, 'crates'), (n) => n.endsWith('.rs'))) {
    const rel = path.relative(repo, file).split(path.sep);
    const testFile = rel[2] === 'tests' || path.basename(file) === 'tests.rs';
    for (const name of testFns(fs.readFileSync(file, 'utf8'), testFile)) exact.add(name);
    if (rel.length === 4 && rel[2] === 'tests') exact.add(path.basename(file, '.rs'));
  }
  // The audit tools' Python tests (`def test_...` in `.audit/tools/*_test.py`).
  for (const file of walk(path.join(repo, '.audit/tools'), (n) => n.endsWith('_test.py'))) {
    for (const m of fs.readFileSync(file, 'utf8').matchAll(/\bdef (test_\w+)/g)) exact.add(m[1]);
  }
  // A journey file is named by its stem ("journey `create`").
  for (const file of walk(path.join(repo, 'parity/app/journeys'), (n) => n.endsWith('.test.mjs'))) exact.add(path.basename(file, '.test.mjs'));
  for (const file of walk(path.join(repo, 'parity'), (n) => n.endsWith('.mjs'))) {
    const titles = titlesOf(fs.readFileSync(file, 'utf8'));
    for (const t of titles.exact) exact.add(t);
    patterns.push(...titles.patterns);
  }
  const report = path.join(repo, 'parity/app/report.json');
  if (fs.existsSync(report)) {
    for (const row of JSON.parse(fs.readFileSync(report, 'utf8')).rows ?? []) if (row.name) exact.add(row.name);
  }
  // The release pipeline's node tests and the names its scripts print as `ok <name>` (card G3).
  for (const file of walk(path.join(repo, 'scripts/release'), (n) => n.endsWith('.mjs'))) {
    const text = fs.readFileSync(file, 'utf8');
    for (const t of titlesOf(text).exact) exact.add(t);
    for (const m of text.matchAll(/\b(?:step\(|ok |FAIL )['`]?([a-z][a-z0-9]*(?:_[a-z0-9]+)+)/g)) exact.add(m[1]);
  }
  // The Electronite shell's node tests (`shell/test`, `shell/e2e`).
  for (const file of walk(path.join(repo, 'shell'), (n) => n.endsWith('.test.cjs'))) {
    const titles = titlesOf(fs.readFileSync(file, 'utf8'));
    for (const t of titles.exact) exact.add(t);
    patterns.push(...titles.patterns);
  }
  const scenarios = path.join(repo, 'parity/ds-gallery/scenarios.mjs');
  if (fs.existsSync(scenarios)) {
    for (const m of fs.readFileSync(scenarios, 'utf8').matchAll(/name:\s*'([^']+)'/g)) exact.add(m[1]);
  }
  return { exact, patterns, repo };
}

/** The fns of a Rust file that are tests or test code: every fn of a test file; in product
 * source, a fn under a test attribute, inside a `#[cfg(test)]` module or a `proptest!` block.
 * A product fn is no test, so a claim such as `new` or `open` resolves to nothing. */
export function testFns(text, testFile) {
  const fns = [...text.matchAll(/\bfn\s+([A-Za-z_][A-Za-z0-9_]*)/g)];
  if (testFile) return fns.map((m) => m[1]);
  const regions = [];
  for (const m of text.matchAll(/#\[cfg\(test\)\]\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{|proptest!\s*\{/g)) {
    let depth = 0;
    let end = text.length;
    for (let i = m.index + m[0].length - 1; i < text.length; i++) {
      if (text[i] === '{') depth++;
      else if (text[i] === '}' && --depth === 0) {
        end = i;
        break;
      }
    }
    regions.push([m.index, end]);
  }
  return fns
    .filter((m) => regions.some(([a, b]) => m.index > a && m.index < b) || /#\[(?:[\w:]+::)?test\b[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:pub\s+)?(?:async\s+)?$/.test(text.slice(Math.max(0, m.index - 300), m.index)))
    .map((m) => m[1]);
}

/** The test titles of one journey or test file: exact titles (and their leading labels), and
 * template titles as patterns. */
export function titlesOf(text) {
  const exact = new Set();
  const patterns = [];
  for (const m of text.matchAll(/\b(?:browserTest|shellTest|test|it)\(\s*(['"`])((?:\\.|(?!\1)[^\\])*)\1/g)) {
    const title = m[2];
    if (m[1] === '`' && title.includes('${')) {
      const parts = title.split(/\$\{[^}]*\}/);
      // A title that is mostly interpolation would match any claim; such a test is named by its
      // file instead.
      if (parts.join('').length < 12) continue;
      const re = parts.map((s) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('[A-Za-z0-9_-]+');
      patterns.push(new RegExp(`^${re}$`));
    } else {
      exact.add(title);
      // A title may start with its short label ("language-tags: a regional code ..."); a document
      // may name the test by that label.
      const label = title.match(/^([A-Za-z0-9_-]+):\s/);
      if (label) exact.add(label[1]);
    }
  }
  return { exact, patterns };
}

/** Whether a claimed name names a test that exists. A `mod::fn` path resolves by its last
 * segment; a path with a `/` resolves when the file exists. */
export function resolve(name, names) {
  const claim = name.replace(/\(\)$/, '');
  if (claim.includes('/')) return fs.existsSync(path.join(names.repo, claim.replace(/:\d+(-\d+)?$/, '')));
  const last = claim.split('::').pop().replace(/\.rs$/, '');
  return names.exact.has(last) || names.patterns.some((re) => re.test(last));
}

/** The claims of a cell: each backticked token, `{a,b}` expanded to one claim per choice. */
export function claimsOf(cell) {
  const expand = (token) => {
    const brace = token.match(/^([^{]*)\{([^}]*)\}(.*)$/);
    return brace ? brace[2].split(',').flatMap((choice) => expand(`${brace[1]}${choice.trim()}${brace[3]}`)) : [token];
  };
  const out = [];
  for (const m of cell.matchAll(/`([^`]+)`/g)) {
    const token = m[1].trim();
    // A tC4 e2e spec is the reference a journey ports, not a test of this repository.
    if (token.endsWith('.spec.ts')) continue;
    out.push(...expand(token));
  }
  return out;
}

/** Each claim of a cell with the cards of a `(pending: ...)` marker written right after it
 * (empty when none is): a marker excuses only the name it follows. */
export function claimsWithPending(cell) {
  const out = [];
  for (const m of cell.matchAll(/`([^`]+)`(\s*\(pending:\s*([^)]+)\))?/g)) {
    const pending = m[3] ? m[3].split(/[,\s]+/).filter(Boolean) : [];
    for (const claim of claimsOf(`\`${m[1]}\``)) out.push({ claim, pending });
  }
  return out;
}

/** The ids of `(pending: X)` and `(pending: X, Y)` markers in a cell. */
export function pendingOf(cell) {
  const out = [];
  for (const m of cell.matchAll(/\(pending:\s*([^)]+)\)/g)) out.push(...m[1].split(/[,\s]+/).filter(Boolean));
  return out;
}

/** The rows of every Markdown table in `text` whose header names all of `columns`: each row as
 * `{ line, cells: { <column>: <cell> } }`. */
export function tables(text, columns) {
  const lines = text.split('\n');
  const rows = [];
  let header = null;
  lines.forEach((raw, i) => {
    const line = raw.trim();
    if (!line.startsWith('|')) {
      header = null;
      return;
    }
    const cells = line.slice(1, line.endsWith('|') ? -1 : undefined).split(/(?<!\\)\|/).map((c) => c.trim());
    if (!header) {
      if (columns.every((c) => cells.includes(c))) header = cells;
      return;
    }
    if (cells.every((c) => /^:?-+:?$/.test(c))) return;
    rows.push({ line: i + 1, cells: Object.fromEntries(header.map((h, j) => [h, cells[j] ?? ''])) });
  });
  return rows;
}

/** Every card's state on the board, and every old unit's state in `.audit/STATUS.md`. */
export function states(repo) {
  const out = new Map();
  const status = path.join(repo, '.audit/STATUS.md');
  if (fs.existsSync(status)) {
    for (const m of fs.readFileSync(status, 'utf8').matchAll(/^\| ([A-Z][A-Za-z0-9-]*)(?: ◆)? \|[^|]*\|[^|]*\| ([a-z ]+) \|/gm)) out.set(m[1], m[2].trim());
  }
  const board = JSON.parse(execFileSync('python3', ['-I', '-B', '.audit/tools/board.py', '--json'], { cwd: repo, encoding: 'utf8' }));
  for (const item of board.items) out.set(item.id, item.state);
  return out;
}

/** The unit and card ids a cell names, among the known ones. */
export function idsIn(cell, known) {
  return [...new Set((cell.match(/[A-Z][A-Za-z0-9]*(?:-[A-Z0-9]+)?/g) ?? []).filter((t) => known.has(t)))];
}
