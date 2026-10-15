#!/usr/bin/env node
// Capture the Leptos port's gallery by the same protocol as the baseline, then compare.
//
//   node capture-rust.mjs [--only <prefix>[,<prefix>...]] [--jobs 6] [--no-build] [--release]
//
// 1. Builds the gallery (crates/tc-ui/build-gallery.sh, with --release for the size-optimised
//    profile) into target/tc-ui-gallery/. There is no app server yet; a tiny static server
//    serves the build.
// 2. Captures each case of gallery-spec.json into screens-rust/<Component>/<state>.png with
//    protocol.mjs, exactly as capture.mjs captures tC4's React gallery into screens/.
// 3. Runs diff.mjs: diff-report.json with per-case mismatched pixel counts.
//
// Exit code 1 when a case fails to draw or differs without an explanation in residuals.json.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { diff } from './diff.mjs';
import { argValue, captureCase, openBrowser, pool, prune, serve } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const SITE = path.join(ROOT, 'target/tc-ui-gallery');
const SCREENS = path.join(HERE, 'screens-rust');

const only = argValue('--only')?.split(',').filter(Boolean);
const jobs = Number(argValue('--jobs') ?? 6);

async function main() {
  if (!process.argv.includes('--no-build')) {
    const args = process.argv.includes('--release') ? ['--release'] : [];
    execFileSync(path.join(ROOT, 'crates/tc-ui/build-gallery.sh'), args, { stdio: 'inherit' });
  }
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const cases = spec.cases.filter((c) => !only || only.some((p) => c.id.startsWith(p)));
  const server = await serve(SITE);
  const origin = `http://127.0.0.1:${server.address().port}`;
  const { browser, context } = await openBrowser();

  const failures = [];
  const started = Date.now();
  await pool(cases, jobs, async (c) => {
    const { errors } = await captureCase(context, origin, c, path.join(SCREENS, c.component, `${c.state}.png`));
    if (errors.length) failures.push({ id: c.id, errors });
  });
  await browser.close();
  server.close();

  if (!only) prune(SCREENS, spec.cases);
  console.log(`${cases.length} cases in ${((Date.now() - started) / 1000).toFixed(1)} s, ${failures.length} failed`);
  for (const f of failures) console.log(`  ${f.id}: ${f.errors.join(' | ')}`);
  const { zero, total, unexplained } = diff(only);
  console.log(`${zero}/${total} cases at 0 mismatched pixels; ${unexplained.length} differ without an explanation`);
  for (const r of unexplained.slice(0, 40)) console.log(`  ${r.id}: ${r.mismatched ?? r.error}`);
  process.exitCode = failures.length || unexplained.length ? 1 : 0;
}

main();
