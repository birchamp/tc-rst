// tC4's own code for the cases' patches (cases.mjs): a value a patch writes into tC4's state
// is made by the function tC4 makes it with, not typed by hand. Bundled from the tC4 clone
// with its own esbuild, as parity/app/env-tc4.mjs bundles `missingPinCount`.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { HERE } from './node-lib.mjs';

/** The kit, from the tC4 clone `tc4`: `t` (tC4's own, refusing a key its English catalog
 * lacks, so a patch never shows a raw key), `bookName`, `importRepoPath`, and the Report
 * makers `okReport`, `failedReport` and `Refusal` of tC4's journal. */
export async function loadPatchKit(tc4) {
  const dir = path.join(HERE, '.cache', 'patch-kit');
  fs.mkdirSync(dir, { recursive: true });
  const entry = path.join(dir, 'entry.mjs');
  const from = (rel) => JSON.stringify(path.join(tc4, rel));
  fs.writeFileSync(entry, [
    `export { t, _catalogs } from ${from('src/i18n/index.js')};`,
    `export { bookName } from ${from('src/data/bookNames.ts')};`,
    `export { importRepoPath } from ${from('src/data/import/shell.ts')};`,
    `export { okReport, failedReport, Refusal } from ${from('journal/report.mjs')};`,
    '',
  ].join('\n'));
  const out = path.join(dir, 'kit.mjs');
  execFileSync(path.join(tc4, 'node_modules', '.bin', 'esbuild'),
    [entry, '--bundle', '--format=esm', '--platform=node', `--outfile=${out}`, '--log-level=error', '--loader:.json=json',
      `--define:import.meta.env.DEV=false`, `--define:__APP_VERSION__="0"`, `--define:__APP_COMMIT__="0"`]);
  const kit = await import(`${pathToFileURL(out).href}?${Date.now()}`);
  const t = (key, vars) => {
    if (typeof kit._catalogs.en[key] !== 'string') throw new Error(`tC4's English catalog has no key ${key}`);
    return kit.t(key, vars);
  };
  return { ...kit, t };
}
