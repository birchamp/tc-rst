#!/usr/bin/env node
// Writes gallery-spec.json from cases/*.mjs: every case the gallery renders, with its exact
// props, frame and steps. The gallery app renders from this file, so the spec and the screens
// cannot drift apart. Refuses (exit 1) when a component file has no case, a case names an
// unknown component, or two cases share an id.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import core from './cases/core.mjs';
import dataFeedback from './cases/data-feedback.mjs';
import forms from './cases/forms.mjs';
import navigation from './cases/navigation.mjs';
import primitives from './cases/primitives.mjs';
import surfacesTranslation from './cases/surfaces-translation.mjs';
import { tokenCases } from './cases/tokens.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TC4 = path.resolve(process.env.TC4_ROOT ?? '/home/user/unfoldingword/translationcore4');
const DS = path.join(TC4, 'src/ds');
const OUT = path.join(HERE, 'gallery-spec.json');

const FRAME = { padding: 16, background: '#F7FAFC' };
const DOCUMENT = { lang: 'en', dir: 'ltr' };
/* Components that draw a fixed layer over the page while they are open. */
const LAYERED = new Set(['Modal', 'Drawer', 'ConfirmDialog', 'Layer', 'Tooltip']);
const STATE = /^[a-z0-9]+(-[a-z0-9]+)*$/;

const PROTOCOL = {
  page: 'Load src/ds/styles.css (every token file and @font-face). Set <html lang> and <html dir> from the case\'s `document`. Size the viewport to the frame (device scale factor 1). The body has margin 0 and the frame\'s background.',
  frame: 'A <div data-case="<id>" tabindex="-1"> at the top-left corner of the page: box-sizing border-box, the frame\'s width, height, padding and background, overflow hidden. The case renders inside it. Because the viewport is the frame, a Layer\'s fixed scrim covers exactly the frame and an anchored popover measures the frame as its window.',
  fonts: 'Load every @font-face of the stylesheet before mounting, then wait for document.fonts.ready.',
  pointerFirst: 'Before mounting, click the pointer at (1, 1), inside the frame\'s padding. The click gives the empty frame the focus without a focus ring, as a click on the button that opens a dialog does in the app, so a component that moves the focus by script on mount (Modal, Drawer and ConfirmDialog focus their first control) shows no ring either: Chromium draws a ring on script focus only when the element that had the focus showed one, or when nothing had the focus. The pointer stays at (1, 1) unless a step moves it.',
  steps: {
    focus: 'Press Tab until the first element inside the frame that matches `target` has the focus. Keyboard focus is what makes :focus-visible match.',
    hover: 'Move the pointer to the centre of the first element inside the frame that matches `target`.',
    press: 'Move the pointer to the centre of the target and press the primary button without releasing it (:active).',
    click: 'Click the centre of the target.',
    key: 'Press `key` on the focused element.',
    type: 'Type `text` into the focused element, one key at a time.',
  },
  motion: 'Every CSS transition and animation is off (as if finished; an infinite animation shows its element without the animation). The text caret is hidden.',
  settle: 'After mounting wait 150 ms, and after each step 300 ms, then for document.fonts.ready, for every image to decode, and for two animation frames. Then screenshot the frame element.',
  output: 'screens/<component>/<state>.png, the frame element only, 1 CSS px = 1 image px.',
};

const NODES = {
  element: '{ "$el": "<Name>", "props": { ... } } — a DS export (index.js) when the name is capitalised, an HTML or SVG tag when it is lower case. `children` is a prop.',
  callback: '{ "$fn": "noop" } — a callback that does nothing. Several components draw a control only when they are given a callback (Modal\'s ✕, Toast\'s action, Breadcrumb\'s links, a clickable Surface).',
  asset: '{ "$asset": "<path>" } — a file of the tC4 checkout, by its path relative to the checkout, served as a URL.',
  json: 'Everything else is a plain JSON value. Style objects are React inline styles: camelCase properties, a bare number means px except on unitless properties (lineHeight, flex, opacity, fontWeight, zIndex). defaultValue is the initial value of an uncontrolled field.',
  cell: '{ "$cell": "<name>" } — the current value of a cell of the case\'s `cells`. A write draws the case again with the new value, as a parent\'s state would.',
  set: '{ "$fn": "set", "cell": "<name>", "value": <json> } or { "$fn": "set", "cell": "<name>", "from": "<path>" } — a callback that writes the cell: `value`, or the value at `from` in the callback\'s arguments ("0" is the first argument, "0.target.checked" the checked state of the first argument\'s target, "0.id" the id field of a table row).',
  if: '{ "$if": "<name>", "then": <node>, "else": <node> } — `then` while the cell is truthy, else `else`; an absent branch is undefined. A screen\'s `onClose={busy ? undefined : close}` or `title={busy ? a : b}`: the component on screen gets the new value, as from a parent\'s state.',
  preventDefault: '{ "$fn": "preventDefault" } — a callback that calls preventDefault on its event (tC4\'s `onPointerDownCapture={(e) => e.preventDefault()}`, which keeps the focus where it is).',
  render: 'When `render` is absent the case renders { "$el": component, "props": props }. When it is present it is the whole tree, and the component under test sits inside it with exactly `props`.',
};

const git = (...args) => execFileSync('git', ['-C', TC4, ...args], { encoding: 'utf8' }).trim();

/** Exported name -> file, from src/ds/index.js. */
function dsExports() {
  const index = fs.readFileSync(path.join(DS, 'index.js'), 'utf8');
  const out = new Map();
  for (const m of index.matchAll(/export\s*\{([^}]+)\}\s*from\s*'\.\/(components\/[^']+)'/g)) {
    for (const name of m[1].split(',').map((s) => s.trim()).filter(Boolean)) out.set(name, m[2]);
  }
  return out;
}

function componentFiles() {
  const root = path.join(DS, 'components');
  return fs.readdirSync(root).flatMap((dir) => fs.readdirSync(path.join(root, dir))
    .filter((f) => f.endsWith('.jsx')).map((f) => `components/${dir}/${f}`)).sort();
}

/** Every `$fn` and `$el` under a props object, by path. */
function markers(props) {
  const callbacks = [];
  const elements = new Set();
  const walk = (v, at, top) => {
    if (Array.isArray(v)) v.forEach((x, i) => walk(x, `${at}[${i}]`, top));
    else if (v && typeof v === 'object') {
      if ('$fn' in v) callbacks.push(at);
      else if ('$el' in v) elements.add(top);
      else for (const [k, x] of Object.entries(v)) walk(x, at ? `${at}.${k}` : k, top ?? k);
    }
  };
  for (const [k, v] of Object.entries(props)) walk(v, k, k);
  return { callbacks, elements: [...elements] };
}

function nodes(tree, visit) {
  if (Array.isArray(tree)) tree.forEach((t) => nodes(t, visit));
  else if (tree && typeof tree === 'object') {
    if ('$el' in tree) visit(tree);
    for (const v of Object.values('$el' in tree ? tree.props ?? {} : tree)) nodes(v, visit);
  }
}

/** JSON.stringify drops undefined; dropping it here too keeps `props` equal to what renders. */
const plain = (v) => JSON.parse(JSON.stringify(v));

function main() {
  const exportsMap = dsExports();
  const files = componentFiles();
  const raw = [...core, ...dataFeedback, ...forms, ...navigation, ...primitives, ...surfacesTranslation, ...tokenCases(TC4)];
  const problems = [];
  const seen = new Set();
  const cases = raw.map((r) => {
    const id = `${r.component}/${r.state}`;
    if (seen.has(id)) problems.push(`duplicate case ${id}`);
    seen.add(id);
    if (!STATE.test(r.state)) problems.push(`${id}: state must be kebab-case`);
    const tokens = r.component === 'Tokens';
    if (!tokens && !exportsMap.has(r.component)) problems.push(`${id}: ${r.component} is not exported by src/ds/index.js`);
    const props = plain(r.props);
    const node = { $el: tokens ? 'div' : r.component, props };
    const render = tokens || r.wrap ? plain(r.wrap ? r.wrap(node) : node) : undefined;
    let layered = false;
    nodes(render ?? node, (n) => {
      if (/^[A-Z]/.test(n.$el) && !exportsMap.has(n.$el)) problems.push(`${id}: unknown component ${n.$el}`);
      if (LAYERED.has(n.$el) || (n.$el === 'Menu' && n.props?.open)) layered = true;
    });
    const document = { ...DOCUMENT, ...r.document };
    const steps = r.steps ?? [];
    const { callbacks, elements } = markers(props);
    const needs = {};
    if (callbacks.length) needs.callbacks = callbacks;
    if (elements.length) needs.elements = elements;
    return {
      id,
      component: r.component,
      state: r.state,
      file: tokens ? 'tokens/*.css' : exportsMap.get(r.component),
      frame: { width: r.frame.width, height: r.frame.height, padding: r.frame.padding ?? FRAME.padding, background: r.frame.background ?? FRAME.background },
      document,
      props,
      ...(render ? { render } : null),
      ...(r.cells ? { cells: plain(r.cells) } : null),
      ...(steps.length ? { steps } : null),
      ...(Object.keys(needs).length ? { needs } : null),
      isolate: layered || steps.length > 0 || document.lang !== DOCUMENT.lang || document.dir !== DOCUMENT.dir,
      ...(r.note ? { note: r.note } : null),
    };
  });
  for (const c of cases) {
    if (!(c.frame.width > 0 && c.frame.height > 0)) problems.push(`${c.id}: frame has no size`);
  }

  const covered = new Set(cases.map((c) => c.file));
  for (const f of files) if (!covered.has(f)) problems.push(`no case for ${f}`);
  if (problems.length) {
    console.error(problems.join('\n'));
    process.exit(1);
  }

  const components = [...new Set(cases.map((c) => c.component))].map((name) => ({
    name,
    file: name === 'Tokens' ? 'tokens/*.css' : exportsMap.get(name),
    cases: cases.filter((c) => c.component === name).length,
  }));
  const pkg = JSON.parse(fs.readFileSync(path.join(TC4, 'package.json'), 'utf8'));
  const spec = {
    about: 'translationCore 4 design-system baseline. Each case is one component in one state inside a fixed-size frame; screens/<component>/<state>.png is how tC4 draws it. A port renders the same case by the same protocol and compares images.',
    source: {
      repo: TC4,
      commit: git('rev-parse', 'HEAD'),
      version: pkg.version,
      ds: 'src/ds',
      index: 'src/ds/index.js',
      stylesheet: 'src/ds/styles.css',
      componentFiles: files.length,
      react: pkg.dependencies.react,
    },
    protocol: PROTOCOL,
    nodes: NODES,
    components,
    cases,
  };
  fs.writeFileSync(OUT, `${JSON.stringify(spec, null, 2)}\n`);
  console.log(`gallery-spec.json: ${cases.length} cases, ${components.length - 1} components from ${files.length} files, plus tokens`);
}

main();
