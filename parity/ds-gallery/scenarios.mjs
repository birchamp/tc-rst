#!/usr/bin/env node
// Behaviour parity: drive the same keyboard and mouse sequence on tC4's React gallery and on
// the Leptos gallery, and compare after every step: the frame's DOM (as dom-diff.mjs snapshots
// it), which element has the focus, whether the page's scroll is locked, and which elements are
// still the nodes they were. A port that draws a node again where React keeps it loses the
// node's transition and any focus on it, which no still image shows.
//
//   node scenarios.mjs [--only <name prefix>[,<prefix>...]]
//
// Needs both builds, as dom-diff.mjs does: dist/ (`npm run build`) and target/tc-ui-gallery/
// (crates/tc-ui/build-gallery.sh). Transitions run (no STILL style), so every step waits for
// them. Writes scenario-report.json (a run with --only replaces only its scenarios there).
// Exit code 1 when a scenario differs or a page reports an error.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { argValue, domSnapshot, openBrowser, serve, tagNodes } from './protocol.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');

/*
 * A step: { click: sel } clicks an element inside the frame, { hover: sel } moves the pointer
 * onto one, { mouse: [x, y] } clicks the page there (the scrim, outside a popover), { move:
 * [x, y] } moves the pointer, { key: 'Enter' } presses a key, { type: 'jo' } types. `wait` is
 * how long to let the page settle before the snapshot (350 ms unless given: the panel motion
 * takes 240).
 */
const SCENARIOS = [
  // Anchored layers: roving focus, typeahead, Escape, outside press, Tab out.
  { name: 'menu-keys', case: 'Menu/closed', steps: [
    { click: 'button' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'ArrowUp' }, { key: 'End' }, { key: 'Home' },
    { key: 'p' }, { key: 'Escape' }, { click: 'button' }, { mouse: [300, 60] }, { click: 'button' }, { key: 'Tab' }, { key: 'Escape' }] },
  { name: 'menu-open', case: 'Menu/open', steps: [{ key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'End' }, { key: 'u' }, { key: 'Escape' }] },
  { name: 'select-keys', case: 'Select/value', steps: [
    { key: 'Tab' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'End' }, { key: 'Home' },
    { key: 'c' }, { key: 'p' }, { key: 'Enter' }, { key: ' ' }, { key: 'Escape' }, { key: 'Enter' }, { key: 'Tab' }] },
  // In a frame tall enough that the open list leaves the field free to click.
  { name: 'select-toggle', case: 'Modal/nested-select', steps: [
    { click: '[role="combobox"]' }, { click: '[role="combobox"]' }, { click: '[role="combobox"]' }, { hover: '[role="option"]:nth-child(3)' },
    { click: '[role="option"]:nth-child(2)' }] },
  { name: 'select-search', case: 'Select/open-search', steps: [
    { click: '[role="combobox"]' }, { type: 'jo' }, { key: 'ArrowDown' }, { key: 'Home' }, { key: 'Backspace' }, { key: 'ArrowDown' },
    { key: 'Enter' }, { click: '[role="combobox"]' }, { key: 'Escape' }, { click: '[role="combobox"]' }, { mouse: [330, 20] }] },
  { name: 'select-groups', case: 'Select/open-groups', steps: [{ click: '[role="combobox"]' }, { type: 'e' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'Tab' }] },
  { name: 'select-choose', case: 'Modal/nested-select', steps: [
    { click: '[role="combobox"]' }, { key: 'ArrowDown' }, { key: 'Enter' }, { click: '[role="combobox"]' }, { click: '[role="option"]:nth-child(1)' }] },
  { name: 'suggest-keys', case: 'SuggestField/value', steps: [
    { key: 'Tab' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'ArrowUp' }, { key: 'Enter' },
    { key: 'ArrowDown' }, { key: 'Escape' }, { type: 'x' }, { key: 'ArrowDown' }, { key: 'Tab' }] },
  { name: 'suggest-change-keys', case: 'SuggestField/suggestions-change', steps: [
    { key: 'Tab' }, { key: 'ArrowDown' }, { type: 'u' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'Enter' }] },
  { name: 'suggest-change-click', case: 'SuggestField/suggestions-change', steps: [
    { key: 'Tab' }, { key: 'ArrowDown' }, { type: 'u' }, { click: '[role="option"]:nth-child(2)' }] },
  { name: 'select-interleaved', case: 'Select/open-interleaved-groups', steps: [
    { click: '[role="combobox"]' }, { key: 'ArrowDown' }, { key: 'ArrowUp' }, { key: 'ArrowUp' }, { key: 'End' }, { key: 'Enter' }] },
  { name: 'select-duplicate-values', case: 'Select/open-duplicate-values', steps: [
    { click: '[role="combobox"]' }, { key: 'Home' }, { key: 'ArrowDown' }, { hover: '[role="option"]:nth-child(1)' }, { key: 'Escape' }] },
  { name: 'switcher-roving', case: 'Switcher/underline', steps: [
    { key: 'Tab' }, { key: 'ArrowRight' }, { key: 'ArrowRight' }, { key: 'Home' }, { key: 'End' }, { key: 'ArrowLeft' }, { key: 'Tab' }] },
  { name: 'switcher-disabled', case: 'Switcher/disabled-option', steps: [{ key: 'Tab' }, { key: 'ArrowRight' }, { key: 'End' }] },
  // Dialogs: the focus trap both ways, Escape, the scrim, the scroll lock.
  { name: 'modal-trap', case: 'Modal/default', steps: [
    { key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Shift+Tab' }, { key: 'Shift+Tab' },
    { key: 'Shift+Tab' }, { key: 'Escape' }, { mouse: [5, 5] }] },
  { name: 'drawer-trap', case: 'Drawer/default', steps: [{ key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Shift+Tab' }, { key: 'Shift+Tab' }, { key: 'Escape' }] },
  { name: 'confirm-trap', case: 'ConfirmDialog/default', steps: [{ key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Shift+Tab' }, { key: 'Escape' }] },
  // Nested layers: Escape and an outside press close the innermost layer only.
  { name: 'nested-select-escape', case: 'Modal/nested-select', steps: [
    { click: '[role="combobox"]' }, { key: 'ArrowDown' }, { key: 'Escape' }, { key: 'Escape' }] },
  { name: 'nested-select-scrim', case: 'Modal/nested-select', steps: [
    { click: '[role="combobox"]' }, { mouse: [5, 5] }, { mouse: [5, 5] }] },
  { name: 'nested-menu-escape', case: 'Modal/nested-menu', steps: [
    { click: '[data-testid="export"]' }, { key: 'ArrowDown' }, { key: 'ArrowDown' }, { key: 'Escape' }, { key: 'Tab' }, { key: 'Escape' }] },
  { name: 'nested-menu-scrim', case: 'Modal/nested-menu', steps: [
    { click: '[data-testid="export"]' }, { mouse: [5, 5] }, { mouse: [5, 5] }] },
  { name: 'nested-menu-choose', case: 'Modal/nested-menu', steps: [
    { click: '[data-testid="export"]' }, { key: 'ArrowDown' }, { key: 'Enter' }, { key: 'Tab' }] },
  { name: 'tooltip', case: 'Tooltip/resting', steps: [{ hover: 'button' }, { move: [5, 150] }, { key: 'Tab' }, { key: 'Tab' }] },
  // State the owner keeps: the node that changes stays, and so does the focus on it.
  { name: 'accordion', case: 'Accordion/closed', steps: [
    { click: 'button' }, { click: 'button' }, { key: 'Tab' }, { key: 'Enter' }, { key: ' ' }, { key: 'Tab' }, { key: 'Enter' }] },
  { name: 'table-select', case: 'Table/stateful', steps: [
    { key: 'Tab' }, { key: 'Enter' }, { key: 'Tab' }, { key: 'Enter' }, { key: ' ' }, { key: 'Shift+Tab' }, { key: 'Enter' }, { click: 'tbody tr:nth-child(3)' }] },
  { name: 'accordion-two-open', case: 'Accordion/first-open', steps: [
    { key: 'Tab' }, { key: 'Tab' }, { key: 'Enter' }, { key: ' ' }, { key: 'Enter' }, { key: 'Shift+Tab' }, { key: 'Enter' }, { key: 'Enter' }] },
  { name: 'table-duplicate-keys', case: 'Table/duplicate-ids', steps: [
    { click: 'tbody tr:nth-child(2)' }, { key: 'Tab' }, { key: 'Enter' }, { key: 'Shift+Tab' }, { key: ' ' }, { click: 'tbody tr:nth-child(1)' }] },
  { name: 'table-rows-change', case: 'Table/rows-change', steps: [
    { key: 'Tab' }, { key: 'Tab' }, { key: 'Enter' }, { key: 'Shift+Tab' }, { key: 'Enter' }, { key: 'Tab' }, { key: 'Tab' }, { key: ' ' }] },
  // A re-sort moves the focused row: it keeps the focus, and Tab goes on from its new place.
  { name: 'table-move-up', case: 'Table/rows-move-up', steps: [
    { key: 'Tab' }, { key: 'Tab' }, { key: 'Tab' }, { key: 'Enter' }, { key: 'Tab' }, { key: 'Enter' }, { key: 'Shift+Tab' }, { key: ' ' }] },
  { name: 'table-move-down', case: 'Table/rows-move-down', steps: [
    { key: 'Tab' }, { key: 'Enter' }, { key: 'Shift+Tab' }, { key: 'Shift+Tab' }, { key: 'Enter' }] },
  { name: 'table-move-click', case: 'Table/rows-move-down', steps: [{ click: 'tbody tr:nth-child(1)' }, { key: 'Shift+Tab' }, { key: 'Enter' }] },
  { name: 'table-noop', case: 'Table/row-hover', steps: [{ key: 'Tab' }, { key: 'Enter' }, { key: ' ' }, { key: 'Tab' }, { key: 'Enter' }] },
  { name: 'toggle', case: 'Toggle/stateful', steps: [{ click: 'input' }, { click: 'input' }, { key: ' ' }, { click: 'label' }] },
  { name: 'radio-group', case: 'RadioGroup/stateful', steps: [{ click: 'input[value="rtl"]' }, { key: 'ArrowUp' }, { key: 'ArrowDown' }, { click: 'label' }] },
  { name: 'radio-group-rejected', case: 'RadioGroup/legend', steps: [{ click: 'input[value="rtl"]' }, { key: 'ArrowUp' }, { key: 'ArrowDown' }] },
  { name: 'progress-ring', case: 'Progress/ring-stateful', steps: [{ click: 'button' }] },
  // Controlled and uncontrolled values: a change the owner does not take is undone.
  { name: 'checkbox-controlled', case: 'Checkbox/label-checked', steps: [{ click: 'input' }, { click: 'label' }, { key: ' ' }] },
  { name: 'checkbox-unchecked', case: 'Checkbox/label', steps: [{ click: 'input' }, { click: 'input' }] },
  { name: 'toggle-controlled', case: 'Toggle/label-on', steps: [{ click: 'input' }] },
  { name: 'text-field', case: 'TextField/value', steps: [{ click: 'input' }, { type: 'xyz' }, { key: 'Backspace' }] },
  { name: 'search-field', case: 'SearchField/value-no-clear', steps: [{ click: 'input' }, { type: 'xyz' }, { key: 'Backspace' }] },
  { name: 'search-clear', case: 'SearchField/value-clear', steps: [{ click: 'button' }] },
  { name: 'card-keys', case: 'Card/interactive-hover', steps: [{ key: 'Tab' }, { key: 'Enter' }, { key: ' ' }] },
];

/** Runs in the page: what the DOM snapshot leaves out. */
function pageState() {
  const a = document.activeElement;
  const where = !a || a === document.body ? 'body'
    : `${a.tagName.toLowerCase()}${a.getAttribute('role') ? `[${a.getAttribute('role')}]` : ''} "${(a.textContent || a.value || '').trim().slice(0, 30)}"`;
  return [`focus: ${where}`, `body overflow: ${document.body.style.overflow || '(none)'}`];
}

async function run(context, origin, c, steps) {
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text().slice(0, 200)); });
  const snap = async (label) => {
    const lines = [...await page.evaluate(domSnapshot, true), ...await page.evaluate(pageState)];
    await page.evaluate(tagNodes);
    return { label, lines };
  };
  await page.setViewportSize({ width: c.frame.width, height: c.frame.height });
  await page.goto(`${origin}/?case=${encodeURIComponent(c.id)}`);
  await page.waitForFunction(() => window.__galleryBooted === true);
  await page.mouse.click(1, 1);
  await page.evaluate(() => window.__mount());
  await page.waitForFunction(() => window.__galleryMounted === true);
  await page.waitForTimeout(350);
  const snaps = [await snap('mounted')];
  for (const st of steps) {
    try {
      const at = (sel) => page.locator(`[data-case] ${sel}`).first();
      if (st.click) await at(st.click).click({ timeout: 2000 });
      else if (st.hover) await at(st.hover).hover({ timeout: 2000 });
      else if (st.mouse) await page.mouse.click(st.mouse[0], st.mouse[1]);
      else if (st.move) await page.mouse.move(st.move[0], st.move[1]);
      else if (st.key) await page.keyboard.press(st.key);
      else if (st.type) await page.keyboard.type(st.type);
    } catch (e) {
      errors.push(`${JSON.stringify(st)}: ${e.message.split('\n')[0]}`);
    }
    await page.waitForTimeout(st.wait ?? 350);
    snaps.push(await snap(JSON.stringify(st)));
  }
  await page.close();
  return { snaps, errors };
}

async function main() {
  const only = argValue('--only')?.split(',').filter(Boolean);
  const spec = JSON.parse(fs.readFileSync(path.join(HERE, 'gallery-spec.json'), 'utf8'));
  const scenarios = SCENARIOS.filter((s) => !only || only.some((p) => s.name.startsWith(p)));
  const react = await serve(path.join(HERE, 'dist'));
  const port = await serve(path.join(ROOT, 'target/tc-ui-gallery'));
  const origin = (s) => `http://127.0.0.1:${s.address().port}`;
  const { browser, context } = await openBrowser();
  const rows = [];
  for (const sc of scenarios) {
    const c = spec.cases.find((x) => x.id === sc.case);
    if (!c) {
      rows.push({ name: sc.name, case: sc.case, same: false, steps: sc.steps.length, diffs: [], errors: [`no case ${sc.case}`] });
      continue;
    }
    const a = await run(context, origin(react), c, sc.steps);
    const b = await run(context, origin(port), c, sc.steps);
    const diffs = [];
    a.snaps.forEach((x, i) => {
      const y = b.snaps[i]?.lines ?? [];
      const lines = [];
      for (let j = 0; j < Math.max(x.lines.length, y.length) && lines.length < 4; j++) {
        if (x.lines[j] !== y[j]) lines.push(`tC4:  ${x.lines[j] ?? '(none)'}\nport: ${y[j] ?? '(none)'}`);
      }
      if (lines.length) diffs.push({ after: x.label, lines });
    });
    const errors = [...a.errors.map((e) => `tC4: ${e}`), ...b.errors.map((e) => `port: ${e}`)];
    const row = { name: sc.name, case: sc.case, steps: sc.steps.length, same: !diffs.length, diffs, errors };
    rows.push(row);
    console.log(`${sc.name} (${sc.case}): ${row.same ? 'same' : `differs after ${diffs.length} of ${a.snaps.length} snapshots`}${errors.length ? `  [${errors.join(' | ')}]` : ''}`);
    for (const d of diffs.slice(0, 2)) console.log(`  after ${d.after}:\n    ${d.lines.join('\n    ').replace(/\n/g, '\n    ')}`);
  }
  await browser.close();
  react.close();
  port.close();

  const file = path.join(HERE, 'scenario-report.json');
  const previous = fs.existsSync(file) ? JSON.parse(fs.readFileSync(file, 'utf8')) : null;
  const kept = new Map((previous?.scenarios ?? []).map((r) => [r.name, r]));
  for (const r of rows) kept.set(r.name, r);
  const all = SCENARIOS.map((s) => kept.get(s.name) ?? { name: s.name, case: s.case, same: false, steps: s.steps.length, diffs: [], errors: ['not run'] });
  const same = all.filter((r) => r.same && !r.errors.length).length;
  fs.writeFileSync(file, `${JSON.stringify({ same, total: all.length, steps: all.reduce((n, r) => n + r.steps, 0), scenarios: all }, null, 2)}\n`);
  const bad = rows.filter((r) => !r.same || r.errors.length);
  console.log(`${rows.length - bad.length}/${rows.length} scenarios the same after every step`);
  process.exitCode = bad.length ? 1 : 0;
}

main();
