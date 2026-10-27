// What a branch must test before it is handed over (card CHK-7b): the crates it changed and every
// workspace crate that depends on them, and the browser journeys that touch what it changed.
// `scripts/gates.sh --touched` runs what this prints; CI on a track branch runs only the lints.
//
//   node .audit/tools/touched.mjs [base]   prints {"crates": [...], "journeys": [...]}
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const JOURNEYS = 'parity/app/journeys';

/** The crate a changed path belongs to, or null. */
export function crateOf(file) {
  const m = /^crates\/([^/]+)\//.exec(file);
  return m ? m[1] : null;
}

/** The touched crates and every workspace crate that depends on one of them, sorted. */
export function withDependents(touched, deps) {
  const out = new Set(touched);
  for (let grew = true; grew; ) {
    grew = false;
    for (const [name, uses] of Object.entries(deps)) {
      if (!out.has(name) && uses.some((u) => out.has(u))) { out.add(name); grew = true; }
    }
  }
  return [...out].filter((name) => name in deps).sort();
}

/**
 * The words a UI or API change can be found by in a journey: endpoint paths (`service/name`) and
 * test ids (`a-b`), quoted on a changed line of the page's or the API's code.
 */
export function tokensOf(diff) {
  const tokens = new Set();
  let file = '';
  for (const line of diff.split('\n')) {
    // A deleted file's `+++` names /dev/null: keep its `---` path, so its removed lines count.
    if (line.startsWith('+++ ') || line.startsWith('--- ')) { if (!line.endsWith('/dev/null')) file = line.slice(6); continue; }
    if (!/^[+-]/.test(line) || !/^crates\/(tc-ui|tc-api|tc-app|tc-server|tc4)\/src\//.test(file)) continue;
    for (const [, word] of line.matchAll(/"([a-z][a-zA-Z0-9]*\/[a-zA-Z][a-zA-Z0-9]*|[a-z][a-z0-9]*(?:-[a-z0-9]+)+)"/g)) tokens.add(word);
  }
  return tokens;
}

/** The journey files to run: those the branch changed, and those that name a changed token. */
export function journeysFor(files, tokens, journeys) {
  const out = new Set(files.filter((f) => f.startsWith(`${JOURNEYS}/`) && f.endsWith('.test.mjs') && f in journeys));
  for (const [file, text] of Object.entries(journeys)) {
    for (const token of tokens) if (text.includes(`'${token}'`) || text.includes(`"${token}"`) || text.includes(`/${token}`)) { out.add(file); break; }
  }
  return [...out].sort();
}

function main(base) {
  const git = (...args) => execFileSync('git', args, { encoding: 'utf8', maxBuffer: 1 << 28 });
  const files = [...new Set([...git('diff', '--name-only', `${base}...HEAD`).split('\n'), ...git('diff', '--name-only', 'HEAD').split('\n')])].filter(Boolean);
  const diff = git('diff', `${base}...HEAD`) + git('diff', 'HEAD');
  const meta = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--locked'], { encoding: 'utf8', maxBuffer: 1 << 28 }));
  const deps = Object.fromEntries(meta.packages.map((p) => [p.name, p.dependencies.filter((d) => d.path).map((d) => d.name)]));
  const journeys = Object.fromEntries(fs.readdirSync(JOURNEYS).filter((f) => f.endsWith('.test.mjs')).map((f) => [`${JOURNEYS}/${f}`, fs.readFileSync(`${JOURNEYS}/${f}`, 'utf8')]));
  const crates = withDependents([...new Set(files.map(crateOf).filter(Boolean))], deps);
  console.log(JSON.stringify({ crates, journeys: journeysFor(files, tokensOf(diff), journeys) }));
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.chdir(path.join(path.dirname(fileURLToPath(import.meta.url)), '..', '..'));
  main(process.argv[2] ?? 'origin/HEAD');
}
