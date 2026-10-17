// Checks that every tC4 `file:line` citation in tc-obs's sources, tests and tools points
// inside that file in the tC4 checkout given. A bare `:N` or `:N-M` continues the file cited last
// in the same source file. A file named without a directory is looked for under `src/`.
// Run from the repository root with a tC4 checkout:
//   node crates/tc-obs/tools/check-tc4-citations.mjs /path/to/translationcore4
import fs from 'node:fs';
import path from 'node:path';

const [tc4] = process.argv.slice(2);
if (!tc4) throw new Error('usage: check-tc4-citations.mjs <tc4 checkout>');
const repo = path.resolve(path.dirname(new URL(import.meta.url).pathname), '../../..');
const crate = path.join(repo, 'crates/tc-obs');

const walk = (dir) => fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
  const p = path.join(dir, e.name);
  if (e.isDirectory()) return e.name === 'template' ? [] : walk(p);
  return /\.(rs|mjs)$/.test(e.name) ? [p] : [];
});
const lineCounts = new Map();
const linesOf = (rel) => {
  if (!lineCounts.has(rel)) {
    const full = path.join(tc4, rel);
    const text = fs.existsSync(full) && fs.statSync(full).isFile() ? fs.readFileSync(full, 'utf8') : null;
    lineCounts.set(rel, text === null ? null : text.split('\n').length - (text.endsWith('\n') ? 1 : 0));
  }
  return lineCounts.get(rel);
};
const resolve = (name) => {
  if (name.includes('/') && linesOf(name) !== null) return name;
  const found = fs.readdirSync(path.join(tc4, 'src'), { recursive: true })
    .filter((p) => p === name || p.endsWith(`/${name}`));
  return found.length === 1 ? path.join('src', found[0]) : null;
};

const CITE = /([\w./-]+\.(?:mjs|ts|tsx|jsx|js)):(\d+)(?:-(\d+))?|`:(\d+)(?:-(\d+))?`|, :(\d+)(?:-(\d+))?/g;
let checked = 0;
const bad = [];
for (const file of ['src', 'tests', 'tools'].flatMap((dir) => walk(path.join(crate, dir)))) {
  const text = fs.readFileSync(file, 'utf8');
  let current = null;
  for (const m of text.matchAll(CITE)) {
    const line = text.slice(0, m.index).split('\n').length;
    const where = `${path.relative(repo, file)}:${line}`;
    let from;
    let to;
    if (m[1]) {
      current = resolve(m[1]);
      if (!current) {
        bad.push(`${where}: ${m[1]} is not a tC4 file`);
        continue;
      }
      [from, to] = [m[2], m[3]];
    } else {
      [from, to] = m[4] ? [m[4], m[5]] : [m[6], m[7]];
      if (!current) {
        bad.push(`${where}: :${from} follows no file`);
        continue;
      }
    }
    const count = linesOf(current);
    for (const n of [from, to].filter(Boolean).map(Number)) {
      checked += 1;
      if (n < 1 || n > count) bad.push(`${where}: ${current}:${n} is past its ${count} lines`);
    }
  }
}
for (const b of bad) console.log(b);
console.log(`${checked} line numbers checked, ${bad.length} wrong`);
process.exitCode = bad.length ? 1 : 0;
