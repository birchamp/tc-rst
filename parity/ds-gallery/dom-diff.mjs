#!/usr/bin/env node
// Compare the DOM the Leptos port draws with the DOM tC4's React gallery draws, case by case:
// the frame's elements, their attributes, their inline style as the browser parsed it (every
// longhand and its value), their text, input values and the focused element, after the case's
// steps.
//
//   node dom-diff.mjs [--only <prefix>[,<prefix>...]] [--jobs 6] [--show 3]
//
// Needs both builds: dist/ (`npx vite build`, the React gallery) and target/tc-ui-gallery/
// (crates/tc-ui/build-gallery.sh). Writes dom-report.json (a run with --only replaces only its
// cases there) and prints the first differences of each case that differs. Generated ids (tc-f1, tc-dlg2, …) are compared as they are: both
// sides number them in render order. A case that differs and has an entry in residuals.json
// also gets its `why` and `lines`, every pair of lines that differ, which
// crates/tc-ui/tests/gallery_record.rs compares with the pairs the entry declares.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { argValue, captureCase, domSnapshot, openBrowser, pool, serve } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');

/** Every line where the two snapshots differ, as `{ line, tc4, port }` (a missing side is null). */
function allDiffs(a, b) {
  const out = [];
  const n = Math.max(a.length, b.length);
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) out.push({ line: i + 1, tc4: a[i] ?? null, port: b[i] ?? null });
  }
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
  const residuals = fs.existsSync(path.join(HERE, 'residuals.json'))
    ? JSON.parse(fs.readFileSync(path.join(HERE, 'residuals.json'), 'utf8')) : {};
  const cases = spec.cases.filter((c) => !only || only.some((p) => c.id.startsWith(p)));
  const react = await serve(path.join(HERE, 'dist'));
  const port = await serve(path.join(ROOT, 'target/tc-ui-gallery'));
  const origin = (s) => `http://127.0.0.1:${s.address().port}`;
  const { browser, context } = await openBrowser();
  const rows = [];
  await pool(cases, jobs, async (c) => {
    const a = await captureCase(context, origin(react), c, null, domSnapshot);
    const b = await captureCase(context, origin(port), c, null, domSnapshot);
    const same = JSON.stringify(a.info) === JSON.stringify(b.info);
    const residual = residuals[c.id];
    rows.push({
      id: c.id,
      same,
      errors: [...a.errors.map((e) => `tC4: ${e}`), ...b.errors.map((e) => `port: ${e}`)],
      diffs: same ? [] : firstDiffs(a.info ?? [], b.info ?? [], show),
      ...(!same && residual ? { why: residual.why ?? residual, lines: allDiffs(a.info ?? [], b.info ?? []) } : null),
    });
  });
  await browser.close();
  react.close();
  port.close();
  // A partial run (--only) updates its cases and keeps the rest of the previous report.
  const reportFile = path.join(HERE, 'dom-report.json');
  const previous = fs.existsSync(reportFile) ? JSON.parse(fs.readFileSync(reportFile, 'utf8')) : null;
  const kept = new Map((previous?.cases ?? []).map((r) => [r.id, r]));
  for (const r of rows) kept.set(r.id, r);
  const all = spec.cases.map((c) => kept.get(c.id) ?? { id: c.id, same: false, errors: ['not compared'], diffs: [] });
  fs.writeFileSync(reportFile, `${JSON.stringify({ same: all.filter((r) => r.same).length, total: all.length, cases: all }, null, 2)}\n`);
  rows.sort((x, y) => spec.cases.findIndex((c) => c.id === x.id) - spec.cases.findIndex((c) => c.id === y.id));
  for (const r of rows.filter((x) => !x.same || x.errors.length)) {
    console.log(`${r.id}${r.errors.length ? '  [' + r.errors.join(' | ') + ']' : ''}`);
    for (const d of r.diffs) console.log(`  ${d}`);
  }
  console.log(`${rows.filter((r) => r.same).length}/${rows.length} cases with the same DOM`);
}

main();
