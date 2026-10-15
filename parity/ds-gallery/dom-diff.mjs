#!/usr/bin/env node
// Compare the DOM the Leptos port draws with the DOM tC4's React gallery draws, case by case:
// the frame's elements, their attributes, their inline style as the browser parsed it (every
// longhand and its value), their text, input values and the focused element, after the case's
// steps.
//
//   node dom-diff.mjs [--only <prefix>[,<prefix>...]] [--jobs 6] [--show 3]
//
// Needs both builds: dist/ (`npx vite build`, the React gallery) and target/tc-ui-gallery/
// (crates/tc-ui/build-gallery.sh). Writes dom-report.json and prints the first differences of
// each case that differs. Generated ids (tc-f1, tc-dlg2, …) are compared as they are: both
// sides number them in render order.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { argValue, captureCase, openBrowser, pool, serve } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');

/** Runs in the page: the frame's subtree as lines, one per node. */
function snapshot() {
  const out = [];
  const walk = (n, depth) => {
    const pad = '  '.repeat(depth);
    if (n.nodeType === Node.TEXT_NODE) {
      if (n.data.length) out.push(`${pad}"${n.data}"`);
      return;
    }
    if (n.nodeType !== Node.ELEMENT_NODE) return;
    // An image's URL is the gallery's: Vite serves tC4's assets under hashed names.
    const value = (a) => (a.name === 'src' ? a.value.replace(/^.*\//, '').replace(/-[\w-]{8}(\.\w+)$/, '$1') : a.value);
    const attrs = Array.from(n.attributes)
      .filter((a) => a.name !== 'style')
      .map((a) => `${a.name}=${JSON.stringify(value(a))}`)
      .sort();
    // Longhands sorted by name. React writes a shorthand's longhand in place where a parsed
    // style attribute appends it, which reorders cssText without changing any value.
    const decls = n.style ? Array.from(n.style, (p) => `${p}: ${n.style.getPropertyValue(p)}`).sort() : [];
    const style = decls.length ? ` style=${JSON.stringify(decls.join('; '))}` : '';
    const props = [];
    if (n instanceof HTMLInputElement || n instanceof HTMLTextAreaElement) {
      props.push(`.value=${JSON.stringify(n.value)}`);
      if (n.type === 'checkbox' || n.type === 'radio') props.push(`.checked=${n.checked}`, `.indeterminate=${n.indeterminate}`);
    }
    if (document.activeElement === n) props.push('.focused');
    out.push(`${pad}<${n.tagName.toLowerCase()} ${attrs.join(' ')}${style}${props.length ? ' ' + props.join(' ') : ''}>`);
    // Adjacent text nodes render as one run; compare their joined text. Comments (Leptos's
    // markers for dynamic children) take no part in layout.
    let text = '';
    const flush = () => {
      if (text) out.push(`${pad}  "${text}"`);
      text = '';
    };
    for (const c of n.childNodes) {
      if (c.nodeType === Node.TEXT_NODE) text += c.data;
      else if (c.nodeType === Node.COMMENT_NODE) continue;
      else {
        flush();
        walk(c, depth + 1);
      }
    }
    flush();
  };
  walk(document.querySelector('[data-case]'), 0);
  return out;
}

function firstDiffs(a, b, show) {
  const out = [];
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n && out.length < show; i++) {
    if (a[i] !== b[i]) out.push(`line ${i + 1}\n    tC4:  ${a[i] ?? '(none)'}\n    port: ${b[i] ?? '(none)'}`);
  }
  return out;
}

async function main() {
  const only = argValue('--only')?.split(',').filter(Boolean);
  const jobs = Number(argValue('--jobs') ?? 6);
  const show = Number(argValue('--show') ?? 3);
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const cases = spec.cases.filter((c) => !only || only.some((p) => c.id.startsWith(p)));
  const react = await serve(path.join(HERE, 'dist'));
  const port = await serve(path.join(ROOT, 'target/tc-ui-gallery'));
  const origin = (s) => `http://127.0.0.1:${s.address().port}`;
  const { browser, context } = await openBrowser();
  const rows = [];
  await pool(cases, jobs, async (c) => {
    const a = await captureCase(context, origin(react), c, null, snapshot);
    const b = await captureCase(context, origin(port), c, null, snapshot);
    const same = JSON.stringify(a.info) === JSON.stringify(b.info);
    rows.push({ id: c.id, same, errors: [...a.errors.map((e) => `tC4: ${e}`), ...b.errors.map((e) => `port: ${e}`)], diffs: same ? [] : firstDiffs(a.info ?? [], b.info ?? [], show) });
  });
  await browser.close();
  react.close();
  port.close();
  rows.sort((x, y) => spec.cases.findIndex((c) => c.id === x.id) - spec.cases.findIndex((c) => c.id === y.id));
  fs.writeFileSync(path.join(HERE, 'dom-report.json'), `${JSON.stringify({ same: rows.filter((r) => r.same).length, total: rows.length, cases: rows }, null, 2)}\n`);
  for (const r of rows.filter((x) => !x.same || x.errors.length)) {
    console.log(`${r.id}${r.errors.length ? '  [' + r.errors.join(' | ') + ']' : ''}`);
    for (const d of r.diffs) console.log(`  ${d}`);
  }
  console.log(`${rows.filter((r) => r.same).length}/${rows.length} cases with the same DOM`);
}

main();
