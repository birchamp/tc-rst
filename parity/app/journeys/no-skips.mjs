// app-e2e's skip rule (INV-A-SKIP-IS-A-FAILURE): kit.mjs skips a browser test when a build or a
// variable is missing, and node counts a skip as a pass, so the job reads the TAP output instead.
//
//   node parity/app/journeys/no-skips.mjs app-e2e.tap
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

/** The run's test count (null without a summary) and each skipped test with its reason. */
export function countSkips(text) {
  const skipped = [];
  let tests = null;
  for (const line of text.split('\n')) {
    const skip = line.match(/^\s*(not )?ok \d+ - (.*) # SKIP(.*)$/i);
    if (skip) skipped.push({ name: skip[2].trim(), reason: skip[3].trim() });
    const count = line.match(/^# tests (\d+)$/);
    if (count) tests = Number(count[1]);
  }
  return { tests, skipped };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { tests, skipped } = countSkips(fs.readFileSync(process.argv[2], 'utf8'));
  if (!tests) {
    // A cut-off or empty run must not read as "0 skipped".
    console.log(`INV-A-SKIP-IS-A-FAILURE: no "# tests N" summary with N > 0 in ${process.argv[2]}; the run did not finish`);
    process.exit(1);
  }
  if (skipped.length > 0) {
    console.log(`INV-A-SKIP-IS-A-FAILURE: ${skipped.length} tests skipped in app-e2e; every journey must run here`);
    for (const { name, reason } of skipped) console.log(`${name}: ${reason}`);
    process.exit(1);
  }
  console.log(`app-e2e: ${tests} tests, 0 skipped`);
}
