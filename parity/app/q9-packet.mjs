// The H5 packet (unit C7b step 2, Q-9): a picture and the DOM of every review screen tC4 has no
// design for, for the owner to sign off before any choice is wired. The proposed screens come
// from the test build's probe page, `?probe=q9&screen=<name>`, which draws each from the fixed
// data of crates/tc-ui/src/app/q9.rs (its SCREENS, read here so the two lists cannot drift).
// The screens the rebuild already has (History and Restore, the bulk keep, the first-launch
// failure) are taken from the real app: two teammates' versions arrive by a scripted sync from
// a fake Door43, as parity/app/journeys/conflict-review.test.mjs brings them.
//
//   TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app TC4_UI_HOOKS=target/tc-ui-app-hooks \
//     TC4_DOOR43_FAKE=target/debug/tc-door43-fake node parity/app/q9-packet.mjs
//
// Each screen is saved as `<name>.png` and `<name>.json` (its buttons' words and its DOM, as
// observe.mjs's domSnapshot writes it) in docs/evidence/q9-2026-10-24/.
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../tc4-baseline/mock-server/fake-door43.mjs';
import { REPO, newRequestId } from './env-rust.mjs';
import { catalog, setup, withApp } from './journeys/kit.mjs';
import { domSnapshot } from './observe.mjs';
import { BIBLE } from './screens.mjs';

const OUT = path.join(REPO, 'docs', 'evidence', 'q9-2026-10-24');
const FOLDER = 'sample_burrito';
const en = catalog('en');
const source = fs.readFileSync(path.join(REPO, 'crates', 'tc-ui', 'src', 'app', 'q9.rs'), 'utf8');
const PROBED = [...source.match(/pub const SCREENS: \[&str; \d+\] = \[([^\]]*)\]/)[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]);

if (setup.skip) throw new Error(setup.skip);
if (!setup.hooks) throw new Error('set TC4_UI_HOOKS (build-app.sh --test-hooks --out target/tc-ui-app-hooks)');
if (!setup.door43) throw new Error('set TC4_DOOR43_FAKE (cargo build -p tc-door43-fake)');
fs.mkdirSync(OUT, { recursive: true });

/** `<name>.png` and `<name>.json` of what the page shows now. */
async function save(page, name, { dom = true } = {}) {
  await page.evaluate(() => document.fonts.ready);
  await page.screenshot({ path: path.join(OUT, `${name}.png`), fullPage: true });
  const choices = await page.locator('[data-testid="q9-choice"], main button, section button')
    .evaluateAll((els) => els.map((e) => e.dataset.choice ?? e.textContent.trim()).filter(Boolean));
  const snapshot = dom ? await page.evaluate(domSnapshot) : { text: await page.evaluate(() => document.body.innerText) };
  fs.writeFileSync(path.join(OUT, `${name}.json`), `${JSON.stringify({ screen: name, choices, dom: snapshot }, null, 2)}\n`);
  console.log(`saved ${name}`);
}

// The proposed screens, from the probe page.
await withApp(async ({ page, env }) => {
  for (const name of PROBED) {
    await page.goto('about:blank');
    await page.goto(`${env.origin}/?probe=q9&screen=${name}#t=${env.token}`);
    await page.locator(`[data-testid="q9"][data-screen="${name}"]`).waitFor();
    await save(page, name);
  }
  // The kit checks that no boot error is left on the page.
  await page.goto('about:blank');
  await env.load(page, { locale: 'en' });
  await page.getByTestId('account-menu').waitFor();
}, { hooks: true });

// The screens the rebuild has already: the bulk keep and History over two verses in
// conflict, then the first-launch window's failure.
await withApp(async ({ page, env }) => {
  const toast = page.getByTestId('sync-result');
  await page.getByTestId(`share-${FOLDER}`).waitFor();
  await env.teammatesChangeOneVerse(BIBLE);
  await page.getByTestId(`share-${FOLDER}`).click();
  await page.getByTestId('net-confirm').click();
  await page.getByTestId('share-signin').waitFor();
  await page.getByLabel(en['signIn.login']).fill(FAKE_USER.username);
  await page.getByLabel(en['signIn.password'], { exact: true }).fill(FAKE_USER.password);
  await page.getByTestId('signin-submit').click();
  await toast.locator('[data-testid="sync-count-conflicts"][data-count="1"]').waitFor({ timeout: 60_000 });
  await toast.getByTestId('sync-dismiss').click();
  const later = ['Con la esperanza, versión del compañero 1.', 'Con la esperanza, versión del compañero 2.'];
  await env.call('test/script', {
    request: newRequestId(),
    next: { type: 'sync', value: { project: env.projectId(BIBLE), outcome: { type: 'synced', value: { teammates: { book: 'TIT', chapter: 1, verse: 2, texts: later }, restored: 0 } } } },
  });
  await page.getByTestId(`share-${FOLDER}`).click();
  await toast.getByTestId('sync-review').click();
  await page.getByTestId('conflict-review').waitFor();
  await page.getByTestId('conflict-version').nth(1).waitFor();
  await page.getByTestId('conflict-keep-all').first().waitFor();
  await save(page, 'bulk-keep');
  await page.getByTestId('conflict-history').click();
  await page.getByTestId('history-version').nth(2).waitFor();
  await page.getByTestId('conflict-history-list').scrollIntoViewIfNeeded();
  await save(page, 'history');

  const reason = 'the bundled resource en_tw could not be read: unexpected end of file';
  await page.route('**/api/v1/app/hello', (route) => route.fulfill({
    status: 200,
    contentType: 'application/json',
    body: JSON.stringify({ type: 'ok', value: { prefs: { appLocale: 'en' }, startup: { type: 'failed', value: { reason } } } }),
  }));
  await page.route('**/api/v1/events*', (route) => route.fulfill({ status: 204, body: '' }));
  await page.goto('about:blank');
  await page.goto(`${env.origin}/prepare.html#t=${env.token}`);
  await page.getByText('translationCore4 could not start').waitFor();
  await save(page, 'first-launch-failure', { dom: false });
}, { door43: true, scenario: 'shared' });

console.log(`${PROBED.length + 3} screens in ${path.relative(REPO, OUT)}`);
