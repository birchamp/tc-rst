// Writes BUILD-MANIFEST.json, the one record of what a release holds (card G3): the commit and
// version, the platform, Electronite's tag and archive sha256, and the sha256 and size of every
// shipped file. `node manifest.mjs --root <staged app> [--into <dir>] --platform <p> --electronite
// <tag> --electronite-sha256 <hex>`, written into --into (default: --root); the commit is
// TC4_BUILD_COMMIT, as tc4 and the UI were built with.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const { values } = parseArgs({
  options: {
    root: { type: 'string' },
    into: { type: 'string' },
    platform: { type: 'string' },
    electronite: { type: 'string' },
    'electronite-sha256': { type: 'string' },
  },
});
for (const name of ['root', 'platform', 'electronite', 'electronite-sha256']) {
  if (!values[name]) throw new Error(`--${name} is needed`);
}
const commit = process.env.TC4_BUILD_COMMIT;
if (!/^[0-9a-f]{40}$/.test(commit ?? '')) throw new Error('TC4_BUILD_COMMIT must be the 40-hex commit the build used');

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const cargo = fs.readFileSync(path.join(repo, 'Cargo.toml'), 'utf8');
const version = cargo.match(/\[workspace\.package\][^[]*?\nversion\s*=\s*"([^"]+)"/)?.[1];
if (!version) throw new Error('no [workspace.package] version in Cargo.toml');

const files = [];
const walk = (dir) => {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.isFile() && entry.name !== 'BUILD-MANIFEST.json') {
      const bytes = fs.readFileSync(full);
      files.push({
        path: path.relative(values.root, full).split(path.sep).join('/'),
        size: bytes.length,
        sha256: crypto.createHash('sha256').update(bytes).digest('hex'),
      });
    }
  }
};
walk(values.root);

const manifest = {
  commit,
  version,
  platform: values.platform,
  electronite: { tag: values.electronite, sha256: values['electronite-sha256'] },
  files,
};
fs.writeFileSync(path.join(values.into ?? values.root, 'BUILD-MANIFEST.json'), `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`BUILD-MANIFEST.json: ${files.length} files, ${commit.slice(0, 12)}, ${version}, ${values.platform}`);
