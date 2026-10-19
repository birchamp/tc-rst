// The work units of docs/APP-DESIGN.md §10, read from the document, so a capture names the unit
// that made it with an id the gate (crates/tc-ui/tests/screen_record.rs) also knows.
import fs from 'node:fs';
import path from 'node:path';

const UNIT = /^[A-G][0-9]+[ab]?$/;

/** The unit ids of the tables of §10 in `doc` (the text of docs/APP-DESIGN.md). */
export function designUnits(doc) {
  const lines = doc.split('\n');
  const start = lines.findIndex((line) => line === '## 10. Work breakdown');
  if (start === -1) throw new Error('docs/APP-DESIGN.md has no "## 10. Work breakdown"');
  const units = new Set();
  for (const line of lines.slice(start + 1)) {
    if (line.startsWith('## ')) break;
    if (!line.startsWith('| ')) continue;
    const first = line.slice(2).split(' ')[0];
    if (UNIT.test(first)) units.add(first);
  }
  return units;
}

/** The units of the repository's design. */
export function repoUnits(repo) {
  return designUnits(fs.readFileSync(path.join(repo, 'docs', 'APP-DESIGN.md'), 'utf8'));
}
