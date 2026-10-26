// Writes THIRD-PARTY-NOTICES.txt (card G3 step 3; tC4 #606, #554): the licence of every crate
// Cargo.lock names, tc-rst's own NOTICE files and LICENSE, the fonts' OFL texts, Electronite's
// licence, and one attribution block per bundled Door43 resource, read from its install's burrito
// metadata (CC BY-SA asks for attribution per work). It refuses to write a file that misses one.
//
//   node notices.mjs --bundle <bundled store> --electronite-license <file> --out <file>
//
// Crate licence texts come from the registry sources `cargo metadata --locked --all-features`
// fetches (all features: Cargo.lock holds every optional crate); a crate that ships no licence
// file gets its SPDX expression.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const RULE = '='.repeat(78);
const LICENCE_FILE = /^(licen[cs]e|copying|notice)/i;

const crateHeading = (name, version) => `${RULE}\nCrate ${name} ${version}`;
const resourceHeading = (repoPath, sha) => `${RULE}\nResource ${repoPath} @ ${sha}`;

/** The registry crates of Cargo.lock's text, as `[name, version]`. */
export function lockedCrates(lockText) {
  const crates = [];
  for (const block of lockText.split('[[package]]').slice(1)) {
    if (!/^source = "registry\+/m.test(block)) continue;
    crates.push([block.match(/^name = "(.*)"/m)[1], block.match(/^version = "(.*)"/m)[1]]);
  }
  return crates;
}

/** The bundle's installs, as BUNDLE-MANIFEST.json promises them. */
export function bundledInstalls(bundle) {
  return JSON.parse(fs.readFileSync(path.join(bundle, 'BUNDLE-MANIFEST.json'), 'utf8')).installs;
}

/** The crates and installs `notices` has no block for. */
export function missing(notices, crates, installs) {
  return [
    ...crates.filter(([n, v]) => !notices.includes(`${crateHeading(n, v)}\n`)).map(([n, v]) => `crate ${n} ${v}`),
    ...installs.filter((i) => !notices.includes(`${resourceHeading(i.repoPath, i.sha)}\n`)).map((i) => `resource ${i.repoPath} @ ${i.sha}`),
  ];
}

function crateBlocks() {
  const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--locked', '--all-features'], {
    cwd: repo,
    maxBuffer: 1 << 28,
    encoding: 'utf8',
  }));
  const seen = new Map();
  const blocks = [];
  const packages = metadata.packages.filter((p) => p.source).sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version));
  for (const p of packages) {
    const dir = path.dirname(p.manifest_path);
    const files = fs.readdirSync(dir).filter((f) => LICENCE_FILE.test(f) && fs.statSync(path.join(dir, f)).isFile()).sort();
    let body = `Licence: ${p.license ?? 'see its licence file'}\nSource: ${p.repository ?? `https://crates.io/crates/${p.name}`}\n`;
    if (files.length === 0) body += '\n(The crate ships no licence file; its licence is the SPDX expression above.)\n';
    for (const f of files) {
      const text = fs.readFileSync(path.join(dir, f), 'utf8').trim();
      // The Apache text, among others, recurs word for word; it is printed once.
      const first = seen.get(text);
      if (first) body += `\n${f}: the same text as ${first} above.\n`;
      else {
        seen.set(text, `${f} of ${p.name} ${p.version}`);
        body += `\n--- ${f}\n${text}\n`;
      }
    }
    blocks.push(`${crateHeading(p.name, p.version)}\n${body}`);
  }
  return blocks;
}

function resourceBlocks(bundle) {
  const installs = path.join(bundle, 'dcs');
  const blocks = [];
  for (const owner of fs.readdirSync(installs)) {
    for (const name of fs.readdirSync(path.join(installs, owner))) {
      for (const sha of fs.readdirSync(path.join(installs, owner, name))) {
        const dir = path.join(installs, owner, name, sha);
        const receipt = JSON.parse(fs.readFileSync(path.join(dir, 'receipt.json'), 'utf8'));
        const content = path.join(dir, 'content');
        // A lexicon is repackaged into one file that keeps its burrito metadata.
        const metadata = fs.existsSync(path.join(content, 'metadata.json'))
          ? JSON.parse(fs.readFileSync(path.join(content, 'metadata.json'), 'utf8'))
          : JSON.parse(fs.readFileSync(path.join(content, 'lexicon.json'), 'utf8')).metadata;
        const id = metadata.identification ?? {};
        const rights = (metadata.copyright?.shortStatements ?? []).map((s) => s.statement);
        const { repoPath, version } = receipt.pin;
        const creator = repoPath.split('/')[1];
        blocks.push(
          `${resourceHeading(repoPath, receipt.pin.sha)}\n` +
            `Title: ${id.name?.en ?? name}${id.abbreviation?.en ? ` (${id.abbreviation.en})` : ''}\n` +
            `Creator: ${creator} (https://git.door43.org/${creator})\n` +
            `Rights: ${rights.join('; ') || 'not stated in its metadata'}\n` +
            `Source: https://${repoPath}${version ? ` release ${version},` : ''} commit ${receipt.pin.sha}\n` +
            `Licence: CC BY-SA 4.0, https://creativecommons.org/licenses/by-sa/4.0/\n` +
            (receipt.packaging === 'lexicon'
              ? 'Changes: repackaged by translationCore 4 into one file; the entries are unchanged.\n'
              : 'Changes: none; bundled as published.\n'),
        );
      }
    }
  }
  return blocks;
}

function fileBlock(title, file) {
  return `${RULE}\n${title}\n\n${fs.readFileSync(file, 'utf8').trim()}\n`;
}

export function notices({ bundle, electroniteLicense }) {
  const fonts = path.join(repo, 'crates/tc-ui/assets/ds/assets/fonts');
  return [
    'translationCore 4: third-party notices\n\nThe software and content below ship with translationCore 4, each under its own licence.\n',
    fileBlock('translationCore 4 (tc-rst)', path.join(repo, 'LICENSE')),
    fileBlock('tc-door43 NOTICE', path.join(repo, 'crates/tc-door43/NOTICE')),
    fileBlock('tc-align NOTICE', path.join(repo, 'crates/tc-align/NOTICE')),
    fileBlock('Electronite (https://github.com/unfoldingWord/electronite)', electroniteLicense),
    ...fs.readdirSync(fonts).filter((f) => /^OFL-.*\.txt$/.test(f)).sort().map((f) => fileBlock(`Font licence ${f}`, path.join(fonts, f))),
    ...resourceBlocks(bundle),
    ...crateBlocks(),
  ].join('\n');
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { values } = parseArgs({ options: { bundle: { type: 'string' }, 'electronite-license': { type: 'string' }, out: { type: 'string' } } });
  for (const name of ['bundle', 'electronite-license', 'out']) if (!values[name]) throw new Error(`--${name} is needed`);
  const text = notices({ bundle: values.bundle, electroniteLicense: values['electronite-license'] });
  const gaps = missing(text, lockedCrates(fs.readFileSync(path.join(repo, 'Cargo.lock'), 'utf8')), bundledInstalls(values.bundle));
  if (gaps.length) throw new Error(`the notices miss ${gaps.join(', ')}`);
  fs.writeFileSync(values.out, text);
  console.log(`wrote ${values.out}`);
}
