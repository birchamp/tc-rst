// Builds the gallery with the tC4 clone's own toolchain: its Vite, React plugin, node polyfills
// and React, through a `node_modules` link to the clone's, in release mode with tC4's base
// and defines, as tC4's `npm run build` builds tC4 (vite.config.js). Two plugins swap the
// state module: `stateStub` resolves every import of tC4's `src/state.jsx` to
// `src/state-stub.jsx` and fails the build if tC4's file is ever loaded, and `stateExports`
// serves the exact source of state.jsx's `SCRIPT_FONTS` and `isOldTestament`.
//
//   TC4_ROOT=<tC4 clone with node_modules> node parity/view-gallery/build.mjs
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const HERE = path.dirname(fileURLToPath(import.meta.url));
export const DIST = path.join(HERE, 'dist');
const STUB = path.join(HERE, 'src', 'state-stub.jsx');
const VIRTUAL = 'virtual:tc4-state-exports';

/** TC4_ROOT, checked: a git checkout of tC4 with its dependencies installed. */
export function requireTc4(root = process.env.TC4_ROOT) {
  const help = 'see "Setup" in parity/view-gallery/README.md';
  if (!root) throw new Error(`set TC4_ROOT to a tC4 clone with node_modules (${help})`);
  const tc4 = path.resolve(root);
  if (!fs.existsSync(path.join(tc4, 'src', 'state.jsx'))) throw new Error(`TC4_ROOT=${tc4} is not a tC4 checkout: it has no src/state.jsx (${help})`);
  if (!fs.existsSync(path.join(tc4, 'node_modules', 'vite', 'package.json'))) throw new Error(`TC4_ROOT=${tc4} has no node_modules/vite: run npm ci in it (${help})`);
  return tc4;
}

/** The commit and version a build of `tc4` names (tC4's vite.config.js reads them the same way). */
export function tc4Build(tc4) {
  const git = (...args) => execFileSync('git', ['-C', tc4, ...args], { encoding: 'utf8', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } }).trim();
  const { version } = JSON.parse(fs.readFileSync(path.join(tc4, 'package.json'), 'utf8'));
  return { version, commit: git('rev-parse', 'HEAD'), short: git('rev-parse', '--short=7', 'HEAD') };
}

/** The source text of the virtual module: state.jsx's `SCRIPT_FONTS` and `isOldTestament`,
 * each found exactly once, after the import `isOldTestament` needs. */
export function stateExportsSource(stateSource) {
  const take = (what, pattern) => {
    const found = [...stateSource.matchAll(pattern)];
    if (found.length !== 1) throw new Error(`tC4's state.jsx has ${found.length} definitions of ${what}, not one: the stub cannot take it`);
    return found[0][0];
  };
  const fonts = take('SCRIPT_FONTS', /^export const SCRIPT_FONTS = \[[\s\S]*?^\];$/gm);
  const oldTestament = take('isOldTestament', /^export function isOldTestament\([\s\S]*?^\}$/gm);
  return `import { BOOK_NAMES } from '@tc4/src/data/bookNames';\n${fonts}\n${oldTestament}\n`;
}

/** Resolves every import of `<tc4>/src/state.jsx` to the stub; loading tC4's file fails. */
export function stateStub(tc4, stub = STUB) {
  const state = path.join(tc4, 'src', 'state.jsx');
  const isState = (id) => path.resolve(id.split('?')[0]) === state;
  return {
    name: 'tc4-state-stub',
    enforce: 'pre',
    async resolveId(source, importer, options) {
      if (!importer || !/(^|\/)state(\.jsx)?$/.test(source)) return null;
      const resolved = await this.resolve(source, importer, { ...options, skipSelf: true });
      return resolved && isState(resolved.id) ? stub : null;
    },
    load(id) {
      if (isState(id)) throw new Error(`tC4's ${state} was loaded: an import reached it past the stub`);
      return null;
    },
  };
}

/** Serves `virtual:tc4-state-exports` from `<tc4>/src/state.jsx`. */
export function stateExports(tc4) {
  const source = stateExportsSource(fs.readFileSync(path.join(tc4, 'src', 'state.jsx'), 'utf8'));
  return {
    name: 'tc4-state-exports',
    resolveId: (id) => (id === VIRTUAL ? `\0${VIRTUAL}` : null),
    load: (id) => (id === `\0${VIRTUAL}` ? source : null),
  };
}

/** Points `parity/view-gallery/node_modules` at the clone's: replaces a link that points
 * elsewhere and refuses a real folder there. */
function linkModules(tc4) {
  const link = path.join(HERE, 'node_modules');
  const target = path.join(tc4, 'node_modules');
  let stat = null;
  try {
    stat = fs.lstatSync(link);
  } catch {
    stat = null;
  }
  if (stat && !stat.isSymbolicLink()) throw new Error(`${link} is a real folder; the gallery uses the tC4 clone's node_modules through a link there: remove it`);
  if (stat && fs.readlinkSync(link) === target) return;
  if (stat) fs.rmSync(link);
  fs.symlinkSync(target, link, 'dir');
}

/** Builds the gallery into parity/view-gallery/dist. Answers tC4's build identity. */
export async function buildGallery({ tc4 = requireTc4() } = {}) {
  linkModules(tc4);
  // Bare imports from this folder resolve through the link, so these are the clone's packages.
  const vite = await import('vite');
  const { default: react } = await import('@vitejs/plugin-react');
  const { nodePolyfills } = await import('vite-plugin-node-polyfills');
  const build = tc4Build(tc4);
  await vite.build({
    configFile: false,
    root: HERE,
    base: '/clients/uw-tc4/',
    // tC4's public folder: assets/translationcore-logo.png (App.jsx) and the icon.
    publicDir: path.join(tc4, 'public'),
    cacheDir: path.join(HERE, '.cache', 'vite'),
    plugins: [stateStub(tc4), stateExports(tc4), react(), nodePolyfills()],
    define: { __APP_VERSION__: JSON.stringify(build.version), __APP_COMMIT__: JSON.stringify(build.short) },
    resolve: { alias: { '@tc4': tc4 }, dedupe: ['react', 'react-dom'] },
    build: { outDir: DIST, emptyOutDir: true, chunkSizeWarningLimit: 8000 },
    logLevel: 'warn',
  });
  return build;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const build = await buildGallery();
  console.log(`gallery built from tC4 ${build.version} at ${build.short} into ${path.relative(process.cwd(), DIST)}`);
}
