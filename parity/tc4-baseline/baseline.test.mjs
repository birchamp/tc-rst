// Checks that the committed baseline agrees with itself: the screenshots, the DOM snapshots,
// manifest.json, the README and the owner's rulings in docs/DIFFERENCES.md. It needs no browser, no
// tC4 checkout and no network, so CI runs it on every push:
//
//   node --test parity/tc4-baseline/baseline.test.mjs
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const manifest = JSON.parse(fs.readFileSync(path.join(HERE, 'manifest.json'), 'utf8'));
const readme = fs.readFileSync(path.join(HERE, 'README.md'), 'utf8');
const plan = fs.readFileSync(path.join(HERE, '..', '..', 'docs', 'DIFFERENCES.md'), 'utf8');
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

/** Width and height from a PNG's IHDR chunk, or null for a file that is not a PNG. */
function pngSize(bytes) {
  if (bytes.length < 24 || bytes.toString('latin1', 12, 16) !== 'IHDR') return null;
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) };
}

const sha256 = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');

/** What is wrong with a DOM snapshot (capture.mjs, parity/app/observe.mjs `domSnapshot`). */
function domProblems(name, dom) {
  const problems = [];
  if (dom.screen !== name) problems.push(`${name}: the DOM file is of screen ${dom.screen}`);
  if (!/^<html .*\blang="[^"]+"/.test(dom.html ?? '')) problems.push(`${name}: the DOM file has no <html lang>`);
  if (!Array.isArray(dom.outside) || dom.outside.length) problems.push(`${name}: the app draws outside #root: ${JSON.stringify(dom.outside)}`);
  if (!Array.isArray(dom.root) || !/^<div [^>]*id="root"/.test(dom.root[0] ?? '')) problems.push(`${name}: the DOM file does not start at #root`);
  const used = new Set((dom.root ?? []).flatMap((line) => [...line.matchAll(/ style=@([0-9a-f]{12})[ >]/g)].map((m) => m[1])));
  const keys = Object.keys(dom.styles ?? {});
  for (const key of used) if (!keys.includes(key)) problems.push(`${name}: style @${key} is not in the style table`);
  for (const key of keys) {
    if (!used.has(key)) problems.push(`${name}: style @${key} is in the table but no element has it`);
    else if (sha256(dom.styles[key]).slice(0, 12) !== key) problems.push(`${name}: style @${key} is not the sha256 of its text`);
  }
  return problems;
}

/**
 * What is wrong with the screenshots and DOM snapshots under `dir` against `manifest`: each
 * PNG must be listed, at its window size, with its byte count and its sha256, and each DOM
 * snapshot must be listed with its sha256 and be whole. Answers one line per problem.
 */
function baselineProblems(manifest, dir) {
  const problems = [];
  const listed = (ext) => manifest.screens.map((s) => `${s.name}.${ext}`).sort();
  const found = (sub, ext) => fs.readdirSync(path.join(dir, sub)).filter((f) => f.endsWith(`.${ext}`)).sort();
  for (const [sub, ext] of [['screens', 'png'], ['dom', 'json']]) {
    const want = listed(ext);
    const have = found(sub, ext);
    for (const f of want) if (!have.includes(f)) problems.push(`${sub}/${f} is missing`);
    for (const f of have) if (!want.includes(f)) problems.push(`${sub}/${f} is not in the manifest`);
  }
  for (const s of manifest.screens) {
    if (s.status === 'failed') problems.push(`${s.name} failed: ${s.error}`);
    if (s.file !== `screens/${s.name}.png`) problems.push(`${s.name}: the manifest names the screenshot ${s.file}`);
    const png = path.join(dir, `screens/${s.name}.png`);
    if (fs.existsSync(png)) {
      const bytes = fs.readFileSync(png);
      const { width, height } = s.viewport ?? manifest.viewport;
      const size = pngSize(bytes);
      if (size?.width !== width || size?.height !== height) problems.push(`${s.name}.png is ${size ? `${size.width}x${size.height}` : 'not a PNG'}, not ${width}x${height}`);
      if (bytes.length !== s.bytes) problems.push(`${s.name}.png has ${bytes.length} bytes, the manifest says ${s.bytes}`);
      if (sha256(bytes) !== s.sha256) problems.push(`${s.name}.png has sha256 ${sha256(bytes)}, the manifest says ${s.sha256}`);
    }
    if (s.dom !== `dom/${s.name}.json`) problems.push(`${s.name}: the manifest names the DOM snapshot ${s.dom}`);
    const domFile = path.join(dir, `dom/${s.name}.json`);
    if (fs.existsSync(domFile)) {
      const bytes = fs.readFileSync(domFile);
      if (sha256(bytes) !== s.domSha256) problems.push(`dom/${s.name}.json has sha256 ${sha256(bytes)}, the manifest says ${s.domSha256}`);
      let dom = null;
      try {
        dom = JSON.parse(bytes.toString('utf8'));
      } catch (e) {
        problems.push(`dom/${s.name}.json is not JSON: ${e.message}`);
      }
      if (dom) problems.push(...domProblems(s.name, dom));
    }
  }
  const t = manifest.totals;
  const count = (status) => manifest.screens.filter((s) => s.status === status).length;
  if (t.screens !== manifest.screens.length) problems.push(`totals.screens is ${t.screens}, not ${manifest.screens.length}`);
  if (t.complete !== count('complete')) problems.push(`totals.complete is ${t.complete}, not ${count('complete')}`);
  if (t.incomplete !== count('incomplete')) problems.push(`totals.incomplete is ${t.incomplete}, not ${count('incomplete')}`);
  const bytes = manifest.screens.reduce((n, s) => n + s.bytes, 0);
  if (t.bytes !== bytes) problems.push(`totals.bytes is ${t.bytes}, not ${bytes}`);
  return problems;
}

test('every PNG and DOM snapshot is in the manifest, with its window size, byte count and sha256', () => {
  assert.deepEqual(baselineProblems(manifest, HERE), []);
});

test('the check fails on a PNG or a DOM snapshot that differs from the manifest by one byte', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tc4-baseline-check-'));
  try {
    for (const sub of ['screens', 'dom']) fs.cpSync(path.join(HERE, sub), path.join(dir, sub), { recursive: true });
    assert.deepEqual(baselineProblems(manifest, dir), [], 'the copy passes before it is changed');
    // The last byte of a PNG is the CRC of its IEND chunk: the file keeps its size and its
    // IHDR, so only the hash can tell.
    const png = path.join(dir, 'screens', 'about.png');
    const bytes = fs.readFileSync(png);
    bytes[bytes.length - 1] ^= 0xff;
    fs.writeFileSync(png, bytes);
    const domFile = path.join(dir, 'dom', 'home.json');
    fs.writeFileSync(domFile, fs.readFileSync(domFile, 'utf8').replace('"screen": "home"', '"screen": "homf"'));
    const problems = baselineProblems(manifest, dir);
    const expected = [
      /^about\.png has sha256 [0-9a-f]{64}, the manifest says [0-9a-f]{64}$/,
      /^dom\/home\.json has sha256 [0-9a-f]{64}, the manifest says [0-9a-f]{64}$/,
      /^home: the DOM file is of screen homf$/,
    ];
    assert.equal(problems.length, expected.length, problems.join('\n'));
    for (const re of expected) assert.ok(problems.some((p) => re.test(p)), `no problem matches ${re}: ${problems.join(' | ')}`);
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test('the manifest records the machine of its screenshots', () => {
  // Fallback glyphs come from the system's fonts, so pixels compare only between captures of
  // one machine (APP-DESIGN 7.3 layer 1); parity/app reads this fingerprint.
  const { machine } = manifest;
  assert.ok(machine, 'manifest.json has no machine');
  assert.match(machine.platform, /^[a-z0-9]+-[a-z0-9]+$/);
  assert.equal(`Chromium ${machine.chromium} `, manifest.browser.slice(0, `Chromium ${machine.chromium} `.length), 'the machine names another Chromium than the browser');
  assert.ok(machine.fonts && Number.isInteger(machine.fonts.count) && machine.fonts.count > 0, 'the machine lists no fonts');
  assert.match(machine.fonts.sha256, /^[0-9a-f]{64}$/);
  assert.ok(readme.includes(machine.fonts.sha256.slice(0, 12)), `the README does not name the font list ${machine.fonts.sha256.slice(0, 12)}`);
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

test('the screens changed since d310bf3 are in the manifest, and each keeps its old image', () => {
  const changed = screenRows(section(readme, 'Changes since d310bf3'));
  assert.ok(changed.length > 0);
  for (const { name } of changed) {
    assert.ok(byName.has(name), `${name} is not in the manifest`);
    assert.ok(fs.existsSync(path.join(HERE, 'history', 'd310bf3', `${name}.png`)), `history/d310bf3/${name}.png is missing`);
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

test('the README lists every screen where the rebuild differs on purpose, with a ruling that DIFFERENCES records', () => {
  const rows = screenRows(section(readme, 'Where the rebuild differs on purpose'));
  const expected = manifest.screens.filter((s) => s.rebuildDiffers).map((s) => [s.name, s.rebuildDiffers.ruling]);
  assert.deepEqual(rows.map((r) => [r.name, r.cells[0]]), expected);
  const rulings = section(plan, 'Where the rebuild differs from tC4 4.0 on purpose');
  for (const [name, ruling] of expected) {
    assert.match(rulings, new RegExp(`\\b${ruling}\\b`), `DIFFERENCES does not record ${ruling}, which ${name} cites`);
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
    'the note on changes that tC4 sent again (restored)': (t) => t.restored > 0,
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

// The next tC4 sweep starts from this line (SWEEP card); without it a sweep cannot name its range.
test('tc4_issues_names_its_swept_range', () => {
  const issues = fs.readFileSync(path.join(HERE, '..', '..', 'docs', 'TC4-ISSUES.md'), 'utf8');
  assert.match(
    issues,
    /^Swept: tC4 main [0-9a-f]{40}, release\/4\.0 [0-9a-f]{40}, issues updated to \d{4}-\d{2}-\d{2}\.$/m,
    'INV-THE-SWEEP-NAMES-ITS-RANGE: docs/TC4-ISSUES.md has no line "Swept: tC4 main <40-hex>, release/4.0 <40-hex>, issues updated to <date>."',
  );
});

test("DIFFERENCES' reference commit is the baseline's", () => {
  const named = plan.match(/the reference is tC4 `[^`]+` at\s+commit\s+`([0-9a-f]{7,40})`/)?.[1];
  assert.ok(named, 'DIFFERENCES names no reference commit ("the reference is tC4 `<branch>` at commit `<sha>`")');
  const captured = manifest.tc4.commit;
  assert.ok(captured.startsWith(named), `P7/INV-CAPTURES-FRESH: DIFFERENCES names reference ${named}; the baseline was captured at ${captured}`);
});

// The screens, the vendored catalogs and CSS, and the help pages each record the tC4 commit they
// come from; a move of the reference that misses one leaves the app showing two tC4 versions.
test('the_reference_is_one_commit_everywhere', () => {
  const named = plan.match(/the reference is tC4 `[^`]+` at\s+commit\s+`([0-9a-f]{7,40})`/)?.[1];
  const ui = path.join(HERE, '..', '..', 'crates', 'tc-ui');
  const vendored = fs.readFileSync(path.join(ui, 'vendored-from-tc4.txt'), 'utf8').match(/^commit ([0-9a-f]{40})$/m)?.[1];
  const help = fs.readFileSync(path.join(ui, 'help-pages.txt'), 'utf8').match(/^commit ([0-9a-f]{40})$/m)?.[1];
  const helpInReadme = readme.match(/^- Help pages: tc-website `([0-9a-f]{40})`/m)?.[1];
  const records = { 'docs/DIFFERENCES.md': named, 'parity/tc4-baseline/manifest.json': manifest.tc4.commit, 'crates/tc-ui/vendored-from-tc4.txt': vendored };
  for (const [file, commit] of Object.entries(records)) {
    assert.ok(commit && named && (commit.startsWith(named) || named.startsWith(commit)), `INV-THE-REFERENCE-IS-ONE-COMMIT: ${file} names tC4 ${commit}; DIFFERENCES names ${named}`);
  }
  assert.equal(help, helpInReadme, `INV-THE-REFERENCE-IS-ONE-COMMIT: crates/tc-ui/help-pages.txt pins tc-website ${help}; the baseline README records ${helpInReadme}, tC4's pin at the reference`);
});
