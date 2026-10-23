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

/** The groups of §7.4's table in `doc`: `[{ group, dom, pixels }]`, in the table's order, each
 * unit one that §10 lists. */
export function galleryGroups(doc) {
  const lines = doc.split('\n');
  const start = lines.findIndex((line) => line.startsWith('### 7.4 '));
  if (start === -1) throw new Error('docs/APP-DESIGN.md has no "### 7.4"');
  const units = designUnits(doc);
  const groups = [];
  for (const line of lines.slice(start + 1)) {
    if (/^#{1,3} /.test(line)) break;
    if (!line.startsWith('| `')) continue;
    const cells = line.slice(1, -1).split('|').map((cell) => cell.trim());
    if (cells.length !== 4) throw new Error(`§7.4 line ${JSON.stringify(line)} does not have four cells`);
    const [name, , dom, pixels] = cells;
    const group = name.match(/^`([^`]+)`$/)?.[1];
    if (!group) throw new Error(`§7.4 line ${JSON.stringify(line)}: the group is not in backticks`);
    for (const unit of [dom, pixels]) {
      if (!units.has(unit)) throw new Error(`§7.4 group ${group} names ${unit}, which is no unit of §10`);
    }
    groups.push({ group, dom, pixels });
  }
  return groups;
}

/** The gallery groups of the repository's design. */
export function repoGalleryGroups(repo) {
  return galleryGroups(fs.readFileSync(path.join(repo, 'docs', 'APP-DESIGN.md'), 'utf8'));
}
