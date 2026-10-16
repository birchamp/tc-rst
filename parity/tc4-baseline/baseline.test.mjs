// Checks that the committed baseline agrees with itself: the screenshots, manifest.json, the
// README and the owner's rulings in docs/PLAN.md. It needs no browser, no tC4 checkout and no
// network, so CI runs it on every push:
//
//   node --test parity/tc4-baseline/baseline.test.mjs
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(fs.readFileSync(path.join(HERE, 'manifest.json'), 'utf8'));
const readme = fs.readFileSync(path.join(HERE, 'README.md'), 'utf8');
const plan = fs.readFileSync(path.join(HERE, '..', '..', 'docs', 'PLAN.md'), 'utf8');
const byName = new Map(manifest.screens.map((s) => [s.name, s]));

/** The text of one `## ` or `### ` section of a Markdown file, without its heading. */
function section(markdown, heading) {
  const lines = markdown.split('\n');
  const start = lines.findIndex((line) => /^#{2,3} /.test(line) && line.replace(/^#+ /, '') === heading);
  assert.notEqual(start, -1, `section "${heading}" is missing`);
  const level = lines[start].match(/^#+/)[0].length;
  const end = lines.findIndex((line, i) => i > start && new RegExp(`^#{1,${level}} `).test(line));
  return lines.slice(start + 1, end === -1 ? undefined : end).join('\n');
}

/** The rows of the first table in a section whose first cell is a backticked screen name. */
function screenRows(text) {
  return text.split('\n')
    .filter((line) => /^\| `[^`]+` \|/.test(line))
    .map((line) => line.slice(1, -1).split('|').map((cell) => cell.trim()))
    .map(([first, ...rest]) => ({ name: first.slice(1, -1), cells: rest }));
}

const flat = (text) => text.replace(/\s+/g, ' ');

/** Width and height from a PNG's IHDR chunk. */
function pngSize(file) {
  const bytes = fs.readFileSync(file);
  assert.equal(bytes.toString('latin1', 12, 16), 'IHDR', `${file} is not a PNG`);
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

test('every PNG is in the manifest, at its window size and with its byte count', () => {
  const pngs = fs.readdirSync(path.join(HERE, 'screens')).filter((f) => f.endsWith('.png')).sort();
  assert.deepEqual(pngs, manifest.screens.map((s) => `${s.name}.png`).sort());
  for (const s of manifest.screens) {
    const file = path.join(HERE, s.file);
    const { width, height } = s.viewport ?? manifest.viewport;
    assert.deepEqual(pngSize(file), { width, height }, s.name);
    assert.equal(fs.statSync(file).size, s.bytes, s.name);
    assert.notEqual(s.status, 'failed', s.name);
  }
  const t = manifest.totals;
  assert.equal(t.screens, manifest.screens.length);
  assert.equal(t.complete, manifest.screens.filter((s) => s.status === 'complete').length);
  assert.equal(t.incomplete, manifest.screens.filter((s) => s.status === 'incomplete').length);
  assert.equal(t.bytes, manifest.screens.reduce((n, s) => n + s.bytes, 0));
});

test('the README lists every screen of the manifest in order, with its scenario, view, steps, what it shows and status', () => {
  const rows = screenRows(section(readme, 'The screenshots'));
  assert.deepEqual(rows.map((r) => r.name), manifest.screens.map((s) => s.name));
  for (const { name, cells } of rows) {
    const s = byName.get(name);
    const [scenario, view, steps, shows, status] = cells;
    assert.equal(scenario, s.scenario, `scenario of ${name}`);
    assert.equal(view, s.route.modal ? `${s.route.view} + ${s.route.modal}` : s.route.view, `view of ${name}`);
    // The column gives the steps after the launch on Home: the manifest's first "Home" is left out.
    const home = s.steps.indexOf('Home');
    assert.equal(steps, s.steps.filter((_, i) => i !== home).join(' → '), `steps of ${name}`);
    assert.equal(shows, s.shows, `what ${name} shows`);
    assert.equal(status, s.status, `status of ${name}`);
  }
});

test('the README states the tC4 commit and the totals of the manifest', () => {
  const text = flat(readme);
  const t = manifest.totals;
  assert.ok(text.includes(manifest.tc4.commit), `the README does not record tC4 ${manifest.tc4.commit}`);
  assert.ok(text.includes(`${t.screens} screenshots`), `the README does not say "${t.screens} screenshots"`);
  assert.ok(text.includes(`${t.bytes.toLocaleString('en-US')} bytes`), `the README does not give ${t.bytes} bytes`);
  assert.ok(text.includes(`${t.complete} are complete and ${t.incomplete} are incomplete`), 'the README does not give the complete and incomplete counts');
});

test('the marking row of each tool is captured in English and Arabic, at 1280 px and at 620 px', () => {
  for (const tool of ['translationWords', 'translationNotes', 'align']) {
    for (const locale of ['en', 'ar']) {
      for (const width of [manifest.viewport.width, 620]) {
        const found = manifest.screens.some((s) => s.markingRow === tool
          && (s.locale ?? 'en') === locale && (s.viewport ?? manifest.viewport).width === width);
        assert.ok(found, `no screen shows the ${tool} marking row in ${locale} at ${width} px`);
      }
    }
  }
});

test('the README lists every screen where the rebuild differs on purpose, with a ruling that PLAN records', () => {
  const rows = screenRows(section(readme, 'Where the rebuild differs on purpose'));
  const expected = manifest.screens.filter((s) => s.rebuildDiffers).map((s) => [s.name, s.rebuildDiffers.ruling]);
  assert.deepEqual(rows.map((r) => [r.name, r.cells[0]]), expected);
  const rulings = section(plan, 'Where the rebuild differs from tC4 4.0 on purpose');
  for (const [name, ruling] of expected) {
    assert.match(rulings, new RegExp(`\\b${ruling}\\b`), `PLAN does not record ${ruling}, which ${name} cites`);
  }
});

test('the screens named as changed or added are in the manifest, and each changed one keeps its old image', () => {
  const changed = screenRows(section(readme, 'Changes since d494a9d'));
  const added = screenRows(section(readme, 'Screens added for d310bf3'));
  assert.ok(changed.length > 0 && added.length > 0);
  for (const { name } of [...changed, ...added]) assert.ok(byName.has(name), `${name} is not in the manifest`);
  for (const { name } of changed) {
    assert.ok(fs.existsSync(path.join(HERE, 'history', 'd494a9d', `${name}.png`)), `history/d494a9d/${name}.png is missing`);
  }
  const old = JSON.parse(fs.readFileSync(path.join(HERE, 'history', 'd494a9d', 'manifest.json'), 'utf8'));
  for (const { name } of added) assert.ok(!old.screens.some((s) => s.name === name), `${name} was already in the d494a9d baseline`);
});

test('every part of the sync result toast is captured (tC4 SyncResult.jsx)', () => {
  const toasts = manifest.screens.filter((s) => s.syncToast).map((s) => ({ name: s.name, status: s.status, ...s.syncToast }));
  const failed = toasts.find((t) => !t.ok && t.code);
  assert.ok(failed, 'no screen shows the toast of a sync that did not complete, with its code');
  assert.equal(failed.status, 'complete', `${failed.name} is not complete`);
  const ok = toasts.filter((t) => t.ok);
  const parts = {
    'the count line with no change from teammates': (t) => t.received === 0 && t.verseConflicts === 0,
    'teammates\' changes with "Changes from"': (t) => t.received > 0 && t.from,
    'a verse conflict with Review': (t) => t.verseConflicts > 0 && t.links.includes('review'),
    'the note on missing resource versions with Download': (t) => t.missingPins > 0 && t.links.includes('download'),
  };
  for (const [part, shows] of Object.entries(parts)) assert.ok(ok.some(shows), `no screen shows ${part}`);
});

test('a toast that counts missing resource versions is on an incomplete screen that names them', () => {
  // Complete means what a full install shows, and a full install has every pinned version,
  // so it counts none. The capture lists the versions by tC4's own rule (missingPinCount).
  const counted = manifest.screens.filter((s) => s.syncToast?.missingPins > 0);
  assert.ok(counted.length > 0, 'no screen shows the note on missing resource versions');
  const absent = flat(manifest.absentResources.join(' '));
  const incompleteSection = section(readme, 'Incomplete screens and why');
  for (const s of counted) {
    const { missingPins, missingVersions } = s.syncToast;
    assert.equal(missingVersions?.length, missingPins, `${s.name} counts ${missingPins} versions, but the manifest lists ${JSON.stringify(missingVersions)}`);
    assert.equal(s.status, 'incomplete', `${s.name} counts versions that the mock lacks, so it is not what a full install shows`);
    for (const version of missingVersions) {
      const named = version.split('/').at(-1); // "es-419_twl v18"
      assert.ok(absent.includes(named), `${named}, which ${s.name} counts, is not listed as absent`);
      assert.ok(s.incomplete.includes(named), `the reason ${s.name} is incomplete does not name ${named}`);
    }
    assert.ok(incompleteSection.includes(`\`${s.name}\``), `"Incomplete screens and why" does not name ${s.name}`);
  }
});

test('every screen where a shared OBS card reads "Upload changes" cites Q2', () => {
  // Q2: the rebuild syncs OBS projects, so no shared card of the rebuild offers Upload changes.
  const uploads = manifest.screens.filter((s) => Object.values(s.homeCards ?? {}).includes('upload'));
  assert.ok(uploads.length > 0, 'no screen records a card with Upload changes (homeCards)');
  for (const s of uploads) assert.equal(s.rebuildDiffers?.ruling, 'Q2', `${s.name} shows Upload changes but does not cite Q2`);
});
