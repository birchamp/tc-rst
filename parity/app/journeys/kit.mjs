// What the journeys of APP-DESIGN §7.7 share: a `tc4 serve` and a Chromium page made as the
// parity capture makes them (env-rust.mjs: the token header, every other origin aborted), and
// small helpers over tC4's test ids.
//
// A browser test runs when `TC4_BIN` (tc4 built with `--features test-hooks,fixture-backend`,
// which the environment's `--scenario` needs since B1b), `TC4_UI` (the folder of
// crates/tc-ui/build-app.sh) and Playwright (PLAYWRIGHT_MODULE, or
// parity/tc4-baseline/node_modules) are there; a test of the no-loss probe also needs
// `TC4_UI_HOOKS` (`build-app.sh --test-hooks`), and a test that signs in to Door43 needs
// `TC4_DOOR43_FAKE` (`cargo build -p tc-door43-fake`). Otherwise it is skipped and says why, so
// CI's node job runs these files with no browser. Each test starts its own `tc4` and ends it, and
// fails on a page error, a console error or a request to another origin it did not expect.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import test from 'node:test';
import { REPO, newRequestId, startDoor43Fake, startRustEnv, startTeammate } from '../env-rust.mjs';
import { CHROMIUM_ARGS, domSnapshot } from '../observe.mjs';

function browserSetup() {
  const { TC4_BIN, TC4_UI, TC4_UI_HOOKS, TC4_DOOR43_FAKE } = process.env;
  if (!TC4_BIN || !TC4_UI) return { skip: 'set TC4_BIN (tc4 built with --features test-hooks,fixture-backend) and TC4_UI (target/tc-ui-app)' };
  try {
    const dir = process.env.PLAYWRIGHT_MODULE ?? path.join(REPO, 'parity', 'tc4-baseline', 'node_modules', 'playwright');
    return { tc4: TC4_BIN, ui: TC4_UI, hooks: TC4_UI_HOOKS, door43: TC4_DOOR43_FAKE, chromium: createRequire(import.meta.url)(dir).chromium };
  } catch (error) {
    return { skip: `Playwright does not resolve (${error.code ?? error.message}): npm ci in parity/tc4-baseline` };
  }
}
export const setup = browserSetup();

/** A test that needs `tc4` and a browser; `hooks` also needs the probe build, `door43` the fake
 * Door43. */
export function browserTest(name, body, { hooks = false, door43 = false, timeout = 120_000 } = {}) {
  let skip = setup.skip ?? false;
  if (!skip && hooks && !setup.hooks) skip = 'set TC4_UI_HOOKS (build-app.sh --test-hooks --out target/tc-ui-app-hooks)';
  if (!skip && door43 && !setup.door43) skip = 'set TC4_DOOR43_FAKE (cargo build -p tc-door43-fake)';
  return test(name, { skip, timeout }, (t) => {
    running = name;
    return body(t);
  });
}

/** The browser test that is running, for the P5 message (a file's tests run one at a time). */
let running = 'this test';

/**
 * P5/INV-VALID-BURRITO after a journey: every project folder it left is a valid Scripture
 * Burrito (`test/validate`). A named scenario always has projects, so checking none means the
 * check did not happen; a seeded folder may hold none.
 */
async function assertValidProjects(env, { seeded, excused }) {
  if (!env.alive()) {
    console.log(`P5: ${running} ended tc4, so test/validate could not run`);
    return;
  }
  const view = await env.call('test/validate', {});
  if (excused) console.log(`P5: ${running} leaves ${excused.folder} not a valid Burrito on purpose: ${excused.why}`);
  const invalid = view.invalid.filter((p) => p.folder !== excused?.folder).map((p) => `P5/INV-VALID-BURRITO: after ${running}, ${p.folder} is not a valid Scripture Burrito: ${p.problems.join('; ')}`);
  if (invalid.length > 0) assert.fail(invalid.join('\n'));
  if (!seeded) assert.ok(view.checked > 0, `P5/INV-VALID-BURRITO: after ${running}, test/validate checked no project folder`);
}

export const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Polls `read` until it gives something truthy; fails after `ms` with `what`. The bound only
 * catches a hang, never slowness: a loaded runner took over 10 s for waits that pass in 1 s
 * (ruling 2026-10-23, tests prove behavior, not speed). */
export async function until(what, read, ms = 60_000) {
  const stop = Date.now() + ms;
  let last;
  for (;;) {
    last = await read();
    if (last) return last;
    if (Date.now() > stop) throw new Error(`hung: no ${what} after ${ms} ms (last: ${JSON.stringify(last)})`);
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
 * A browser, a `tc4` and a page, Home loaded in `locale` (the probe page with `probe`; with
 * `seed(folder)`, tc-store over a projects folder it fills instead of the sample scenario), then
 * `body({ env, page, context, errors, allow })`; `scenario` names another parity scenario
 * (`en-pins` for B3, `shared` for B10). `init(context, env)` runs before the page
 * exists (routes, init scripts). `allow(re)` lets a console error matching `re` pass. With
 * `door43`, tc4 signs in to a fake Door43 of its own (`env.door43`); with `realSync` too, sync
 * and open from Door43 go to it for real (E1a). With `hooks`, the page is the
 * test build's (`TC4_UI_HOOKS`), which also includes the no-loss probe. Translate saves use production endpoints in either build.
 * After `body`, every project folder must be a valid Scripture Burrito (P5);
 * `invalidOnPurpose: { folder, why }` excuses the one folder a journey leaves invalid on purpose.
 */
export async function withApp(body, { locale = 'en', probe = false, hooks = false, init, seed, door43: fake = false, realSync = false, scenario: named = 'sample', invalidOnPurpose } = {}) {
  const browser = await setup.chromium.launch({ args: CHROMIUM_ARGS });
  const door43 = fake ? await startDoor43Fake(setup.door43) : undefined;
  const scenario = seed ? 'store' : named;
  const teammates = [];
  let env;
  try {
    env = await startRustEnv({ scenario, seed, tc4: setup.tc4, ui: probe || hooks ? setup.hooks : setup.ui, door43, realSync });
  } catch (error) {
    await door43?.close();
    await browser.close();
    throw error;
  }
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
    // E1a: another computer on the same fake Door43, driven through its API (`startTeammate`).
    const teammate = async (name) => {
      if (!door43) throw new Error('a teammate needs the fake Door43 (door43: true)');
      const t = await startTeammate({ tc4: setup.tc4, ui: setup.ui, door43, name });
      teammates.push(t);
      return t;
    };
    const result = await body({ env, page, context, errors, allow: (re) => allowed.push(re), teammate });
    const unexpected = errors.filter((e) => !allowed.some((re) => re.test(e)));
    assert.deepEqual(unexpected, [], 'no page error and no console error');
    const failures = await page.locator('[data-testid="view-error"], [data-testid="boot-error"]').count();
    assert.equal(failures, 0, 'no view error or boot error left on the page');
    await assertValidProjects(env, { seeded: Boolean(seed), excused: invalidOnPurpose });
    return result;
  } finally {
    for (const t of teammates) await t.close();
    await env.close();
    await door43?.close();
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
  await page.getByRole('listbox').getByRole('option', { name: LABEL[id], exact: true }).click();
  await until(`the select shows ${LABEL[id]}`, async () => (await page.getByTestId('language-select').textContent()).includes(LABEL[id]));
}

/** The test id of the focused element, or its role. */
export const focused = (page) => page.evaluate(() => document.activeElement?.getAttribute('data-testid') ?? document.activeElement?.getAttribute('role') ?? document.activeElement?.tagName ?? null);

/** The app shows `id`: the account button's name, Home's title when Home shows, `<html lang>`,
 * and, while the App language dialog is open, its title, hint, Apply and Cancel. */
export async function expectLanguage(page, id) {
  const c = catalog(id);
  const en = catalog('en');
  const want = (key) => c[key] ?? en[key];
  await until(`the page in ${id}`, async () => {
    const lang = await page.evaluate(() => document.documentElement.lang);
    const label = await trigger(page).getAttribute('aria-label');
    const title = await page.locator('main h1').textContent({ timeout: 500 }).catch(() => null);
    return lang === id && label === want('account.label.out') && (title === null || title === want('home.yourProjects'));
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

/** `test/keychain`: the test keychain's mode and what it holds, then, with `relaunch`, the
 * account as a launch finds it. Answers `{ mode, holds, keeps, reads, forgets }`. */
export const keychain = (env, { mode, token, nothing = false, relaunch = false } = {}) => env.call('test/keychain', {
  request: newRequestId(),
  mode: mode ?? null,
  holds: token !== undefined ? { type: 'token', value: token } : nothing ? { type: 'nothing' } : null,
  relaunch,
});

/** Counts the internet questions the page opens, as tC4's journeys count them: an init script
 * that watches for `net-ask` to appear. Read it with `asks(page)`. */
export async function countAsks(context) {
  await context.addInitScript(() => {
    window.__tcAsks = 0;
    const seen = new WeakSet();
    new MutationObserver(() => {
      for (const node of document.querySelectorAll('[data-testid="net-ask"]')) {
        if (!seen.has(node)) { seen.add(node); window.__tcAsks += 1; }
      }
    }).observe(document, { childList: true, subtree: true });
  });
}
export const asks = (page) => page.evaluate(() => window.__tcAsks);

/** Every key and value of the page's localStorage and sessionStorage, as one text. */
export const storageDump = (page) => page.evaluate(() => JSON.stringify([
  Object.entries(localStorage), Object.entries(sessionStorage),
]));

/** The files under `dir` whose bytes hold `text`. */
export function filesHolding(dir, text) {
  const found = [];
  const needle = Buffer.from(text);
  const walk = (d) => {
    for (const entry of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, entry.name);
      if (entry.isDirectory()) walk(p);
      else if (entry.isFile() && fs.readFileSync(p).includes(needle)) found.push(p);
    }
  };
  walk(dir);
  return found;
}
