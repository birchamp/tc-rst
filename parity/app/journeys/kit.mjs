// What the journeys of APP-DESIGN §7.7 share: a `tc4 serve` and a Chromium page made as the
// parity capture makes them (env-rust.mjs: the token header, every other origin aborted), and
// small helpers over tC4's test ids.
//
// A browser test runs when `TC4_BIN` (tc4 built with `--features test-hooks`), `TC4_UI` (the
// folder of crates/tc-ui/build-app.sh) and Playwright (PLAYWRIGHT_MODULE, or
// parity/tc4-baseline/node_modules) are there; a test of the no-loss probe also needs
// `TC4_UI_HOOKS` (`build-app.sh --test-hooks`). Otherwise it is skipped and says why, so CI's
// node job runs these files with no browser. Each test starts its own `tc4` and ends it, and
// fails on a page error, a console error or a request to another origin it did not expect.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import test from 'node:test';
import { REPO, newRequestId, startRustEnv } from '../env-rust.mjs';
import { CHROMIUM_ARGS, domSnapshot } from '../observe.mjs';

function browserSetup() {
  const { TC4_BIN, TC4_UI, TC4_UI_HOOKS } = process.env;
  if (!TC4_BIN || !TC4_UI) return { skip: 'set TC4_BIN (tc4 built with --features test-hooks) and TC4_UI (target/tc-ui-app)' };
  try {
    const dir = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
    return { tc4: TC4_BIN, ui: TC4_UI, hooks: TC4_UI_HOOKS, chromium: createRequire(import.meta.url)(dir).chromium };
  } catch (error) {
    return { skip: `Playwright does not resolve (${error.code ?? error.message}): npm ci in parity/tc4-baseline` };
  }
}
export const setup = browserSetup();

/** A test that needs `tc4` and a browser; `hooks` also needs the probe build. */
export function browserTest(name, body, { hooks = false, timeout = 120_000 } = {}) {
  let skip = setup.skip ?? false;
  if (!skip && hooks && !setup.hooks) skip = 'set TC4_UI_HOOKS (build-app.sh --test-hooks --out target/tc-ui-app-hooks)';
  return test(name, { skip, timeout }, body);
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Polls `read` until it gives something truthy; fails after `ms` with `what`. */
export async function until(what, read, ms = 10_000) {
  const stop = Date.now() + ms;
  let last;
  for (;;) {
    last = await read();
    if (last) return last;
    if (Date.now() > stop) throw new Error(`timed out after ${ms} ms waiting for ${what} (last: ${JSON.stringify(last)})`);
    await sleep(25);
  }
}

/** A message catalog of the app, as it ships (tC4's, vendored). */
export const catalog = (id) => JSON.parse(fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'assets', 'i18n', `${id}.json`), 'utf8'));

/** The installed languages, ids and native names in picker order, from the registry the app uses. */
export const LOCALES = [...fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'src', 'i18n.rs'), 'utf8')
  .matchAll(/id: "([^"]+)",\s*label: "([^"]+)"/g)].map(([, id, label]) => ({ id, label }));
export const LABEL = Object.fromEntries(LOCALES.map((l) => [l.id, l.label]));

/**
 * A browser, a `tc4` and a page, Home loaded in `locale` (the probe page with `probe`), then
 * `body({ env, page, context, errors, allow })`. `init(context, env)` runs before the page
 * exists (routes, init scripts). `allow(re)` lets a console error matching `re` pass.
 */
export async function withApp(body, { locale = 'en', probe = false, init } = {}) {
  const browser = await setup.chromium.launch({ args: CHROMIUM_ARGS });
  const env = await startRustEnv({ scenario: 'sample', tc4: setup.tc4, ui: probe ? setup.hooks : setup.ui });
  try {
    const context = await browser.newContext(env.contextOptions);
    await env.prepare(context);
    if (init) await init(context, env);
    const page = await context.newPage();
    const errors = [];
    const allowed = [];
    page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
    page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text()}`); });
    if (probe) await loadProbe(page, env);
    else await loadHome(page, env, locale);
    const result = await body({ env, page, context, errors, allow: (re) => allowed.push(re) });
    const unexpected = errors.filter((e) => !allowed.some((re) => re.test(e)));
    assert.deepEqual(unexpected, [], 'no page error and no console error');
    const failures = await page.locator('[data-testid="view-error"], [data-testid="boot-error"]').count();
    assert.equal(failures, 0, 'no view error or boot error left on the page');
    return result;
  } finally {
    await env.close();
    await browser.close();
  }
}

/** Home afresh in `locale`, as the capture's `home()` loads it. */
export async function loadHome(page, env, locale = 'en') {
  await env.load(page, { locale });
  await page.getByTestId('account-menu').waitFor();
  await page.waitForFunction((lang) => document.documentElement.lang === lang, locale);
}

/** The no-loss probe page of the test build, with its two fields ready. */
export async function loadProbe(page, env) {
  await env.call('test/reset', { request: newRequestId() });
  await page.goto('about:blank');
  await page.goto(`${env.origin}/?probe=fields#t=${env.token}`);
  await page.locator('[data-testid="probe-field-a"][data-ready="true"]').waitFor();
}

export const trigger = (page) => page.getByTestId('account-menu');
export const panel = (page) => page.getByTestId('account-menu-panel');

export async function openMenu(page) {
  await trigger(page).click();
  await panel(page).waitFor();
}

/** The App language dialog from the menu. */
export async function openLanguage(page) {
  await openMenu(page);
  await page.getByTestId('account-language').click();
  await page.getByTestId('language-dialog').waitFor();
}

/** A language chosen in the App language dialog's list, by mouse. */
export async function choose(page, id) {
  await page.getByTestId('language-select').click();
  // The list's options: the stand-in Home's own <select> has options of the same names.
  await page.getByRole('listbox').getByRole('option', { name: LABEL[id], exact: true }).click();
  await until(`the select shows ${LABEL[id]}`, async () => (await page.getByTestId('language-select').textContent()).includes(LABEL[id]));
}

/** The test id of the focused element, or its role. */
export const focused = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-testid') ?? document.activeElement?.getAttribute('role') ?? document.activeElement?.tagName ?? null);

/** The app shows `id`: the account button's name, the stand-in Home's sample line, `<html
 * lang>`, and, while the App language dialog is open, its title, hint, Apply and Cancel. */
export async function expectLanguage(page, id) {
  const c = catalog(id);
  const en = catalog('en');
  const want = (key) => c[key] ?? en[key];
  await until(`the page in ${id}`, async () => {
    const lang = await page.evaluate(() => document.documentElement.lang);
    const label = await trigger(page).getAttribute('aria-label');
    const sample = await page.getByTestId('runtime-sample').textContent().catch(() => null);
    return lang === id && label === want('account.label.out') && (sample === null || sample === want('account.language'));
  });
  if (await page.getByTestId('language-dialog').count()) {
    const dialog = page.locator('[data-testid="language-dialog"] [role="dialog"]');
    const text = await dialog.textContent();
    for (const key of ['language.title', 'language.hint']) assert.ok(text.includes(want(key)), `${id}: ${key} in the dialog`);
    assert.equal(await page.getByTestId('language-apply').textContent(), want('language.apply'));
    assert.equal(await page.getByTestId('language-cancel').textContent(), want('language.cancel'));
  }
}

/** Every request the page makes from now on: { url, method, type }. */
export function recordRequests(page) {
  const all = [];
  page.on('request', (r) => all.push({ url: r.url(), method: r.method(), type: r.resourceType() }));
  return {
    all,
    mark: () => all.length,
    since: (mark) => all.slice(mark),
    /** The app's own API calls since `mark`, by path, the event stream left out. */
    api: (mark = 0) => all.slice(mark).map((r) => new URL(r.url).pathname).filter((p) => p.startsWith('/api/v1/') && p !== '/api/v1/events'),
  };
}

/** The subtree of the element with `data-testid=testid` in `domSnapshot`'s lines, with styles
 * spelled out and same-origin asset paths reduced to their file name, re-indented to start at 0. */
export function subtreeOf(snapshot, attr) {
  const lines = snapshot.root;
  const at = lines.findIndex((l) => l.includes(attr));
  assert.ok(at >= 0, `${attr} is in the DOM`);
  const depth = lines[at].length - lines[at].trimStart().length;
  const out = [lines[at]];
  for (const line of lines.slice(at + 1)) {
    if (line.length - line.trimStart().length <= depth) break;
    out.push(line);
  }
  const spell = (line) => line.replace(/style=@([0-9a-f]{12})/g, (_, k) => `style="${snapshot.styles[k] ?? k}"`)
    .replace(/(src|href)="\/[^"]*\/([^"/]+)"/g, '$1="$2"');
  return out.map((line) => spell(line.slice(depth)));
}

/** The DOM snapshot of the page, as the capture takes it. */
export const snapshot = (page) => page.evaluate(domSnapshot, {});

/** tC4's committed DOM file of baseline screen `name`. */
export const tc4Dom = (name) => JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'tc4-baseline', 'dom', `${name}.json`), 'utf8'));
