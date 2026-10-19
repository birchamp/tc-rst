#!/usr/bin/env node
// Compare the Leptos port's screens with tC4's: screens-rust/<C>/<s>.png against
// screens/<C>/<s>.png, pixel by pixel.
//
//   node diff.mjs [--only <prefix>[,<prefix>...]]
//
// Writes diff-report.json: per case, the number of pixels that differ at all (pixelmatch with
// threshold 0 and anti-aliased pixels counted), and the explanation from residuals.json for a
// case that does not reach 0 (an entry there is a string, or an object whose `why` it is: a
// declared difference also names its ruling, its layers and its DOM lines, which
// crates/tc-ui/tests/gallery_record.rs checks). For each case that differs, diffs/<C>/<s>.png
// shows where. Exit code 1 when a case differs and residuals.json does not explain it.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';
import { argValue } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** Compares every case (or those whose id starts with one of `only`) and writes the report. */
export function diff(only) {
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const residuals = fs.existsSync(path.join(HERE, 'residuals.json'))
    ? JSON.parse(fs.readFileSync(path.join(HERE, 'residuals.json'), 'utf8')) : {};
  const reportFile = path.join(HERE, 'diff-report.json');
  const previous = fs.existsSync(reportFile) ? JSON.parse(fs.readFileSync(reportFile, 'utf8')) : null;
  const kept = new Map((previous?.cases ?? []).map((r) => [r.id, r]));
  for (const c of spec.cases) {
    if (only && !only.some((p) => c.id.startsWith(p))) continue;
    const rel = path.join(c.component, `${c.state}.png`);
    const out = path.join(HERE, 'diffs', rel);
    fs.rmSync(out, { force: true });
    const row = { id: c.id, frame: `${c.frame.width}x${c.frame.height}` };
    const want = path.join(HERE, 'screens', rel);
    const got = path.join(HERE, 'screens-rust', rel);
    if (!fs.existsSync(got)) {
      kept.set(c.id, { ...row, mismatched: null, error: 'no screen from the port' });
      continue;
    }
    const a = PNG.sync.read(fs.readFileSync(want));
    const b = PNG.sync.read(fs.readFileSync(got));
    if (a.width !== b.width || a.height !== b.height) {
      kept.set(c.id, { ...row, mismatched: null, error: `size ${b.width}x${b.height}, want ${a.width}x${a.height}` });
      continue;
    }
    const d = new PNG({ width: a.width, height: a.height });
    const mismatched = pixelmatch(a.data, b.data, d.data, a.width, a.height, { threshold: 0, includeAA: true });
    if (mismatched) {
      fs.mkdirSync(path.dirname(out), { recursive: true });
      fs.writeFileSync(out, PNG.sync.write(d));
    }
    kept.set(c.id, { ...row, mismatched, ...(mismatched && residuals[c.id] ? { why: typeof residuals[c.id] === 'string' ? residuals[c.id] : residuals[c.id].why } : null) });
  }
  const cases = spec.cases.map((c) => kept.get(c.id) ?? { id: c.id, mismatched: null, error: 'not compared' });
  const zero = cases.filter((r) => r.mismatched === 0).length;
  const unexplained = cases.filter((r) => r.mismatched !== 0 && !r.why);
  const report = {
    about: 'Pixels that differ between tC4 (screens/) and the Leptos port (screens-rust/), per case. pixelmatch threshold 0 with anti-aliased pixels counted: any difference in any channel counts.',
    total: cases.length,
    zero,
    nonzero: cases.length - zero,
    unexplained: unexplained.map((r) => r.id),
    worst: cases.filter((r) => r.mismatched).sort((x, y) => y.mismatched - x.mismatched).slice(0, 20).map((r) => `${r.id}: ${r.mismatched}`),
    cases,
  };
  fs.writeFileSync(reportFile, `${JSON.stringify(report, null, 2)}\n`);
  const mine = (r) => !only || only.some((p) => r.id.startsWith(p));
  return {
    zero: cases.filter((r) => mine(r) && r.mismatched === 0).length,
    total: cases.filter(mine).length,
    unexplained: unexplained.filter(mine),
  };
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const only = argValue('--only')?.split(',').filter(Boolean);
  const { zero, total, unexplained } = diff(only);
  console.log(`${zero}/${total} cases at 0 mismatched pixels; ${unexplained.length} differ without an explanation`);
  for (const r of unexplained.slice(0, 40)) console.log(`  ${r.id}: ${r.mismatched ?? r.error}`);
  process.exitCode = unexplained.length ? 1 : 0;
}
