// The measurements of unit A8 (docs/evidence/door43-account-*.md): the Door43 requests each
// account action makes, read from the fake Door43's own log; a sign-in against the fake, from
// the click on Sign in to the account button's `data-state="in"`; and the account menu's "Sign
// in to Door43", from the click to the sign-in step's title and to the next frame. PERFORMANCE
// rule 5: at least 20 runs, the median and the p95, the machine and the commit.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app-release TC4_DOOR43_FAKE=target/debug/tc-door43-fake \
//     node crates/tc-ui/app/measure-account.mjs --runs 20 --out docs/evidence/door43-account-<date>/release.json
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { FAKE_USER } from '../../../parity/tc4-baseline/mock-server/fake-door43.mjs';
import { keychain, openMenu, sleep, until, withApp } from '../../../parity/app/journeys/kit.mjs';

const args = Object.fromEntries(process.argv.slice(2).reduce((pairs, arg, i, all) => {
  if (arg.startsWith('--')) pairs.push([arg.slice(2), all[i + 1]]);
  return pairs;
}, []));
const runs = Number(args.runs ?? 20);

const summary = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (q) => sorted[Math.min(sorted.length - 1, Math.ceil(q * sorted.length) - 1)];
  return { runs: sorted.length, median: Number(at(0.5).toFixed(1)), p95: Number(at(0.95).toFixed(1)), max: Number(sorted[sorted.length - 1].toFixed(1)) };
};

const machine = () => {
  const cpu = fs.readFileSync('/proc/cpuinfo', 'utf8');
  const flags = (/^flags\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? '').split(' ');
  return {
    cpu: /^model name\s*:\s*(.*)$/m.exec(cpu)?.[1] ?? 'unknown',
    vcpus: os.cpus().length,
    sha_ni: flags.includes('sha_ni'),
    avx512f: flags.includes('avx512f'),
    load: os.loadavg(),
  };
};

const state = (page) => page.getByTestId('account-menu').getAttribute('data-state');
const api = (requests) => requests.map((r) => `${r.method} ${r.path.replace(FAKE_USER.username, '{user}').replace(/\/tokens\/\d+$/, '/tokens/{id}')}`);

async function openSignIn(page) {
  await openMenu(page);
  await page.getByTestId('account-sign-in').click();
  await page.getByTestId('share-signin').waitFor();
}

async function turnOn(page) {
  await openMenu(page);
  await page.getByTestId('account-internet').click();
  await until('on', async () => (await page.getByTestId('account-menu').getAttribute('data-internet')) === 'on');
  await page.keyboard.press('Escape');
}

const result = { ui: path.resolve(process.env.TC4_UI), runs, machine: machine() };

// 1. Door43's requests per action, and the sign-in time.
Object.assign(result, await withApp(async ({ page, env }) => {
  const door43 = env.door43;
  const requests = {};
  await openMenu(page);
  await page.keyboard.press('Escape');
  await openSignIn(page);
  await sleep(300);
  requests.openMenuAndStep = api(await door43.requests());
  await page.getByTestId('signin-cancel').click();
  await turnOn(page);
  const times = [];
  for (let i = 0; i < runs; i++) {
    await openSignIn(page);
    await page.getByLabel('Door43 username or email').fill(FAKE_USER.username);
    await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
    await door43.clear();
    // Measured in the page: the click's own time stamp to the button's data-state="in".
    const timing = page.evaluate(() => new Promise((resolve) => {
      let clicked = null;
      const button = document.querySelector('[data-testid="account-menu"]');
      document.addEventListener('click', (e) => { clicked = e.timeStamp; }, { capture: true, once: true });
      const observer = new MutationObserver(() => {
        if (clicked === null || button.getAttribute('data-state') !== 'in') return;
        observer.disconnect();
        resolve(performance.now() - clicked);
      });
      observer.observe(button, { attributes: true, attributeFilter: ['data-state'] });
    }));
    await page.getByTestId('signin-submit').click();
    times.push(await timing);
    await until('signed in', async () => (await state(page)) === 'in', 10_000);
    // The first sign-in finds no token; every later one replaces the one before.
    if (i === 0) requests.signIn = api(await door43.requests());
    if (i === 1) requests.signInReplacing = api(await door43.requests());
    await door43.clear();
    await openMenu(page);
    await page.getByTestId('account-sign-out').click();
    await until('out', async () => (await state(page)) === 'out');
    if (i === 0) requests.signOut = api(await door43.requests());
  }
  // A resume of a kept token.
  const token = await door43.issueToken(FAKE_USER.username, 'translationCore', ['write:repository', 'write:organization', 'write:user']);
  await keychain(env, { token, relaunch: true });
  await page.goto('about:blank');
  await page.goto(`${env.origin}/#t=${env.token}`);
  await until('saved', async () => (await state(page)) === 'saved');
  await door43.clear();
  await openMenu(page);
  // A launch starts with the internet off: the check asks first.
  await page.getByTestId('account-check').click();
  await page.getByTestId('net-confirm').click();
  await until('resumed', async () => (await state(page)) === 'in');
  requests.resume = api(await door43.requests());
  return { requests, signIn: summary(times) };
}, { door43: true }));

// 2. The account menu's "Sign in to Door43": the click to the step's title, and to the next frame.
result.openStep = await withApp(async ({ page }) => {
  await page.waitForFunction(() => window.__tcIdle === true);
  const times = [];
  for (let i = 0; i < runs; i++) {
    await openMenu(page);
    const row = page.getByTestId('account-sign-in');
    const timing = page.evaluate(() => new Promise((resolve) => {
      let clicked = null;
      document.addEventListener('click', (e) => { clicked = e.timeStamp; }, { capture: true, once: true });
      const observer = new MutationObserver(() => {
        if (clicked === null || !document.querySelector('[data-testid="share-signin"]')) return;
        observer.disconnect();
        const dom = performance.now() - clicked;
        requestAnimationFrame(() => resolve({ dom, frame: performance.now() - clicked }));
      });
      observer.observe(document.body, { childList: true, subtree: true });
    }));
    await row.click();
    times.push(await timing);
    await page.getByTestId('signin-cancel').click();
    await until('closed', async () => (await page.getByTestId('share-signin').count()) === 0);
    await sleep(50);
  }
  return { dom: summary(times.map((t) => t.dom)), frame: summary(times.map((t) => t.frame)) };
});

const wasm = fs.readdirSync(path.join(path.resolve(process.env.TC4_UI), 'ui')).map((d) => path.join(path.resolve(process.env.TC4_UI), 'ui', d, 'tc_ui_bg.wasm')).find((f) => fs.existsSync(f));
result.wasmBytes = wasm ? fs.statSync(wasm).size : null;

const text = `${JSON.stringify(result, null, 2)}\n`;
if (args.out) {
  fs.mkdirSync(path.dirname(args.out), { recursive: true });
  fs.writeFileSync(args.out, text);
}
process.stdout.write(text);
