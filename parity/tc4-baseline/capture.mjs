#!/usr/bin/env node
// Capture the translationCore 4 visual-parity baseline.
//
//   node capture.mjs [--only <name>[,<name>...]] [--port 19998] [--rebuild] [--build-only]
//
// 1. Builds the tC4 client into .cache/tc4-dist once per tC4 commit (--rebuild forces it), as
//    `npm run build` does: tC4's prebuild step fetches the pinned help pages from GitHub, then
//    vite builds the release client (base /clients/uw-tc4/). Without GitHub the help pages are
//    left out and the manifest says so.
// 2. For each scenario, rebuilds the mock server's state from tC4's fixtures
//    (fixtures/build-state.mjs) and starts the mock server (mock-server/server.mjs), which
//    serves both the Pankosmia API and the built client.
// 3. Drives Chromium at 1280x800 through each screen below and writes screens/<name>.png.
// 4. Writes manifest.json: every screenshot with its scenario, the app view it shows, the
//    steps that reach it, whether it is complete, and the server reads that came back absent.
//
// Every screen starts from a reload of Home with the per-client place records cleared, so a
// screen never depends on where an earlier screen left the app.
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';
import { buildState, DEFAULT_TC4, SCENARIOS, ABSENT } from './fixtures/build-state.mjs';
import { startMockServer } from './mock-server/server.mjs';
import { routeFakeDoor43, FAKE_USER, FAKE_ORGANIZATIONS } from './mock-server/fake-door43.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TC4 = path.resolve(process.env.TC4_ROOT ?? DEFAULT_TC4);
const DIST = path.join(HERE, '.cache', 'tc4-dist');
const SCREENS = path.join(HERE, 'screens');
const VIEWPORT = { width: 1280, height: 800 };
const BIBLE = '_local_/_local_/sample_burrito';
const OBS = '_local_/_local_/sample_burrito_obs';
/** An empty stretch of the title bar (and the scrim under a dialog): the pointer rests here
 * for every screenshot, so no hover state leaks into an image. */
const POINTER_REST = { x: 1000, y: 28 };

/** Rendering flags that keep repeated runs pixel-identical: without them a few rounded
 * edges differ by up to 8/255 between runs (raster tiling). */
const CHROMIUM_ARGS = ['--force-color-profile=srgb', '--disable-partial-raster', '--disable-gpu-rasterization', '--disable-lcd-text', '--font-render-hinting=none'];

const argValue = (name) => {
  const at = process.argv.indexOf(name);
  return at > 0 ? process.argv[at + 1] : undefined;
};
const only = argValue('--only')?.split(',');
const port = Number(argValue('--port') ?? process.env.TC4_BASELINE_PORT ?? 19998);

// ---- the tC4 client build ---------------------------------------------------

const git = (...args) => execFileSync('git', ['-C', TC4, ...args], { encoding: 'utf8' }).trim();

function tc4Identity() {
  const pkg = JSON.parse(fs.readFileSync(path.join(TC4, 'package.json'), 'utf8'));
  return {
    root: TC4,
    commit: git('rev-parse', 'HEAD'),
    commitDate: git('show', '-s', '--format=%cI', 'HEAD'),
    version: pkg.version,
    dirty: git('status', '--porcelain', '--untracked-files=no') !== '',
  };
}

/** Build the client unless this commit's build (with its help pages) is already there.
 * Returns whether the build carries the help pages. */
function buildClient(commit) {
  const stampFile = path.join(DIST, '.tc4-build.json');
  const stamp = fs.existsSync(stampFile) ? JSON.parse(fs.readFileSync(stampFile, 'utf8')) : null;
  if (!process.argv.includes('--rebuild') && stamp?.commit === commit && stamp.helpPages) return true;
  if (!fs.existsSync(path.join(TC4, 'node_modules', 'vite'))) {
    console.log(`tC4 dependencies: npm ci in ${TC4}`);
    execFileSync('npm', ['ci', '--no-audit', '--no-fund'], { cwd: TC4, stdio: 'inherit' });
  }
  fs.mkdirSync(path.dirname(DIST), { recursive: true });
  const log = path.join(HERE, '.cache', 'vite-build.log');
  const out = () => ['ignore', fs.openSync(log, 'a'), fs.openSync(log, 'a')];
  fs.writeFileSync(log, '');
  // tC4's own prebuild step (scripts/fetch-help.mjs): the help pages of its pinned tc-website
  // commit into public/help/, which tC4 git-ignores. It needs GitHub; without it the build
  // goes on without the pages, as a build with no network would.
  let helpPages = true;
  try {
    execFileSync('node', ['scripts/fetch-help.mjs'], { cwd: TC4, stdio: out() });
  } catch {
    helpPages = false;
    console.log('tC4 help pages: not fetched (see .cache/vite-build.log); the help screen will show their absence');
  }
  console.log(`tC4 client: vite build at ${commit.slice(0, 7)} -> ${path.relative(HERE, DIST)}`);
  execFileSync('npx', ['vite', 'build', '--outDir', DIST, '--emptyOutDir'], { cwd: TC4, stdio: out() });
  fs.writeFileSync(stampFile, `${JSON.stringify({ commit, helpPages })}\n`);
  return helpPages;
}

// ---- driving the app ---------------------------------------------------------

/** Every /api request in flight from the browser, so a screenshot waits until the app has
 * stopped reading and writing. */
function trackRequests(context) {
  const inFlight = new Set();
  context.on('request', (r) => { if (new URL(r.url()).pathname.startsWith('/api/')) inFlight.add(r); });
  const done = (r) => inFlight.delete(r);
  context.on('requestfinished', done);
  context.on('requestfailed', done);
  return inFlight;
}

async function settle(page, inFlight, quietMs = 800) {
  const deadline = Date.now() + 30_000;
  let quietSince = Date.now();
  while (Date.now() < deadline) {
    if (inFlight.size > 0) quietSince = Date.now();
    else if (Date.now() - quietSince >= quietMs) break;
    await page.waitForTimeout(50);
  }
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

/** Helpers each screen's `run` receives. */
function appDriver({ page, origin, state }) {
  const settingsFile = path.join(state, 'client_settings', 'uw-tc4.json');
  const pristineSettings = fs.readFileSync(settingsFile);
  const card = (repo) => page.getByTestId(`project-${repo}`);
  const tab = async (name, ready) => {
    await page.getByRole('tab', { name, exact: true }).click();
    if (ready) await ready.waitFor();
  };
  return {
    page,
    /** Reload Home with the per-client document back to its seeded form (install records
     * only): no remembered place, no Resume card, no last-used order. */
    async home() {
      fs.writeFileSync(settingsFile, pristineSettings);
      await page.goto(`${origin}/clients/uw-tc4/`);
      await card(BIBLE).waitFor();
      await card(OBS).waitFor();
    },
    card,
    tab,
    async openTitus() {
      await card(BIBLE).getByRole('button', { name: /Titus/ }).click();
      await page.getByTestId('understand').waitFor();
      await page.getByText('Loading the passage and its helps…').first().waitFor({ state: 'hidden' });
    },
    async openStory1() {
      await card(OBS).getByTestId('story-tile-1').click();
      await page.getByTestId('helps-panel').waitFor();
    },
    async openCheck() {
      await tab('Check');
      await page.getByRole('heading', { name: 'Choose a checking tool' }).waitFor();
      await page.getByText('Checking what is available…').waitFor({ state: 'hidden' });
    },
    async openAlign() {
      await page.getByTestId('open-align').click();
      await page.getByTestId('align-rail').waitFor();
      await page.getByTestId('align-bank').waitFor();
    },
    async openTool(id) {
      await page.getByTestId(`open-${id}`).click();
      await page.getByTestId('check-session').waitFor();
      await page.getByTestId('check-quote').waitFor();
    },
    /** Turn the internet on from the account menu (D95: every launch starts off). */
    async internetOn() {
      await page.getByTestId('account-menu').click();
      await page.getByTestId('account-internet').click();
      await page.keyboard.press('Escape');
      await page.getByTestId('account-menu-panel').waitFor({ state: 'detached' });
    },
  };
}

// ---- the screens -------------------------------------------------------------
//
// Each screen: name (the PNG name), scenario (fixtures/build-state.mjs SCENARIOS), view
// (tC4's s.view: home | read | draft | check | publish; read = Understand, draft =
// Translate), modal (what is layered over the view), steps (what a person does, in tC4's
// own words), shows, incomplete (why the screen cannot show what a full install shows,
// when that is so), and run (the same steps, driven).

const SCREEN_LIST = [
  // -------- scenario: sample (the conformance samples as tC4 ships them) --------
  {
    name: 'home', scenario: 'sample', view: 'home', modal: null,
    steps: ['Launch tC4'],
    shows: 'Home: "Your projects" with the OBS sample (story tiles, 1% drafted) and the Bible sample (Jonah and Titus tiles, "+ Add a book")',
    run: async (app) => { await app.home(); },
  },
  {
    name: 'understand-tit1', scenario: 'sample', view: 'read', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile'],
    shows: 'Understand, Titus 1, Section view: book rail, ULT source units with the comment box, helps panel on Notes (es-419 Translation Notes of the primary set)',
    run: async (app) => { await app.home(); await app.openTitus(); },
  },
  {
    name: 'translate-tit1', scenario: 'sample', view: 'draft', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Translate'],
    shows: 'Translate, Titus 1, Section mode: ULT beside the es-419 draft, each "Draft section" a pill in the accent colour at the end of its row, helps panel with the Comments tab',
    run: async (app) => {
      await app.home(); await app.openTitus();
      await app.tab('Translate', app.page.getByRole('button', { name: /Draft section 1–3/ }));
    },
  },
  {
    name: 'check-picker', scenario: 'sample', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check'],
    shows: 'Check tool picker for Titus: Translation Words "Needs downloading", Translation Notes ready (1 of 112), Align, Community Checking',
    incomplete: 'Translation Words reads "Needs downloading": the sample pins es-419_twl v18 / es-419_tw v37 for its primary set and tC4 vendors neither (see en-check-picker for the ready card).',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); },
  },
  {
    name: 'check-tn-session', scenario: 'sample', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Notes card: Continue'],
    shows: 'Translation Notes session on the es-419 notes, resumed at item 2 ("knowledge", 1:1) after the decided item 1: check rail (filters, sort, list), the quote, WHAT TO CHECK with Show more, the compare card (Greek, ULT highlight, the es-419 draft), the marking row ("← Previous", the triage buttons, "Next →") below the fold; the header shows only "Item 2 of 112"',
    incomplete: 'The link under WHAT TO CHECK reads "Learn more: Translation Academy" instead of the article title: the sample pins es-419_ta v4 for its primary set and tC4 vendors no copy (en-check-tn-session shows the titled link).',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes'); },
  },
  {
    name: 'align-workspace', scenario: 'sample', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Align card'],
    shows: 'Align, Titus 1:1, Gateway · ULT mode: the align rail (46 verses, 5 drafted), the ULT verse, Suggest links (off), the word bank of the es-419 draft in verse order, with an occurrence number on each repeated word (6 of 27 placed), and the ULT word cards with their Greek and their target words in verse order, the sample\'s six links placed',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); await app.openAlign(); },
  },
  {
    name: 'align-original', scenario: 'sample', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Align card', 'Align mode: Original · Greek'],
    shows: 'Align, Titus 1:1, Original · Greek mode: the Greek verse and one word card per Greek word, the six placed links, an occurrence number on each repeated word (Θεοῦ¹, Dios¹), the word bank in verse order, and the top of the marking row ("← Previous", Mark valid, "Next →") at the bottom edge',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openAlign();
      await app.page.getByTestId('align-mode-orig').click();
      await app.page.getByTestId('align-bank').waitFor();
    },
  },
  {
    name: 'project-settings', scenario: 'sample', view: 'home', modal: 'settings',
    steps: ['Home', 'Bible sample card: Settings'],
    shows: 'Project settings dialog: read-only name/language/code, text direction chips, checking language card (English package offered), script font',
    run: async (app) => {
      await app.home();
      await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
      await app.page.getByTestId('settings-gateway').waitFor();
      await app.page.getByText('Looking for the packages on this computer…').waitFor({ state: 'hidden' });
    },
  },
  {
    name: 'add-project', scenario: 'sample', view: 'home', modal: 'addProject',
    steps: ['Home', '+ Add a project'],
    shows: 'Add a project chooser with four cards: New Bible, New Open Bible Stories, Import a project, Open a project from Door43',
    run: async (app) => { await app.home(); await app.page.getByTestId('add-project').click(); await app.page.getByTestId('add-project-modal').waitFor(); },
  },
  {
    name: 'new-bible', scenario: 'sample', view: 'home', modal: 'newProject',
    steps: ['Home', '+ Add a project', 'New Bible'],
    shows: 'New Bible dialog, empty: name, language name, code, text direction, checking language (English, Recommended), script font, Versification disclosure',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('add-project').click();
      await app.page.getByText('New Bible', { exact: true }).click();
      await app.page.getByPlaceholder('e.g. Kanuri Bible').waitFor();
    },
  },
  {
    name: 'import', scenario: 'sample', view: 'home', modal: 'import',
    steps: ['Home', '+ Add a project', 'Import a project'],
    shows: 'Import a project, step 1 of 3: tC3 project, USFM files, Scripture Burrito',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('add-project').click();
      await app.page.getByText('Import a project', { exact: true }).click();
      await app.page.getByTestId('import-modal').waitFor();
    },
  },
  {
    name: 'share-internet-consent', scenario: 'sample', view: 'home', modal: 'netAsk',
    steps: ['Home', 'Bible sample card: Share on Door43'],
    shows: '"Turn on the internet?" consent dialog that every internet step asks first while the internet is off',
    run: async (app) => {
      await app.home();
      await app.page.getByTestId(`share-${BIBLE}`).click();
      await app.page.getByTestId('net-ask').waitFor();
    },
  },
  {
    name: 'share-signin', scenario: 'sample', view: 'home', modal: 'signIn',
    steps: ['Home', 'Bible sample card: Share on Door43', 'Turn on internet'],
    shows: 'Sign in to Door43 dialog (signs in to git.door43.org), empty fields, and the notice that the Door43 username goes into the project, stays in it, and goes into each Scripture Burrito export',
    run: async (app) => {
      await app.home();
      await app.page.getByTestId(`share-${BIBLE}`).click();
      await app.page.getByTestId('net-confirm').click();
      await app.page.getByTestId('share-signin').waitFor();
    },
  },
  {
    name: 'share-dialog', scenario: 'sample', view: 'home', modal: 'share',
    steps: ['Home', 'Bible sample card: Share on Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`],
    shows: 'Share on Door43, step "where it goes": your account (selected), an organization marked Recommended, one you cannot create in',
    incomplete: `Door43 is faked in the browser (mock-server/fake-door43.mjs): the account ${FAKE_USER.username} and the organizations ${FAKE_ORGANIZATIONS.map((o) => o.username).join(', ')} are invented.`,
    run: async (app) => {
      const { page } = app;
      await app.home();
      await page.getByTestId(`share-${BIBLE}`).click();
      await page.getByTestId('net-confirm').click();
      await page.getByLabel('Door43 username or email').fill(FAKE_USER.username);
      await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
      await page.getByTestId('signin-submit').click();
      await page.getByTestId('share-next').waitFor();
      await page.getByText('Reading your organizations…').waitFor({ state: 'hidden' });
    },
  },
  {
    name: 'share-dialog-check', scenario: 'sample', view: 'home', modal: 'share',
    steps: ['Home', 'Bible sample card: Share on Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`, 'Next'],
    shows: 'Share on Door43, step "Check what will be shared": target, repository name, books shared',
    incomplete: 'Door43 is faked in the browser (see share-dialog). Nothing is created or pushed.',
    run: async (app) => {
      const { page } = app;
      await app.home();
      await page.getByTestId(`share-${BIBLE}`).click();
      await page.getByTestId('net-confirm').click();
      await page.getByLabel('Door43 username or email').fill(FAKE_USER.username);
      await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
      await page.getByTestId('signin-submit').click();
      await page.getByText('Reading your organizations…').waitFor({ state: 'hidden' });
      await page.getByTestId('share-next').click();
      await page.getByTestId('share-submit').waitFor();
    },
  },
  {
    name: 'account-menu', scenario: 'sample', view: 'home', modal: 'account menu (popover)',
    steps: ['Home', 'Account button (top right)'],
    shows: 'Account menu: Sign in to Door43, Use the internet (off), Help and guides, App language, Report a problem, About translationCore',
    run: async (app) => { await app.home(); await app.page.getByTestId('account-menu').click(); await app.page.getByTestId('account-menu-panel').waitFor(); },
  },
  {
    name: 'app-language', scenario: 'sample', view: 'home', modal: 'app language',
    steps: ['Home', 'Account button', 'App language'],
    shows: 'App language dialog with English selected',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('account-menu').click();
      await app.page.getByRole('menuitem', { name: /App language/ }).click();
      await app.page.getByTestId('language-dialog').waitFor();
    },
  },
  {
    name: 'help', scenario: 'sample', view: 'home', modal: 'help and guides (full-window panel)',
    steps: ['Home', 'Account button', 'Help and guides'],
    shows: 'Help and guides: the full-window panel with the help pages of the build (tc-website tc-help/ at the commit tC4 pins) open on their index',
    run: async (app) => {
      const { page } = app;
      await app.home(); await page.getByTestId('account-menu').click();
      await page.getByTestId('account-help').click();
      await page.getByTestId('help-panel').waitFor();
      const frame = page.frameLocator('[data-testid="help-frame"]');
      await frame.locator('body').waitFor();
      await frame.locator('img').evaluateAll((imgs) => Promise.all(imgs.map((img) => (img.complete ? null : new Promise((r) => { img.onload = r; img.onerror = r; })))));
    },
  },
  {
    name: 'about', scenario: 'sample', view: 'home', modal: 'about',
    steps: ['Home', 'Account button', 'About translationCore'],
    shows: 'About translationCore: version and commit of the build, copyright line, license name and Read the license',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('account-menu').click();
      await app.page.getByTestId('account-about').click();
      await app.page.getByTestId('about-dialog').waitFor();
    },
  },
  {
    name: 'feedback', scenario: 'sample', view: 'home', modal: 'feedback',
    steps: ['Home', 'Account button', 'Report a problem'],
    shows: 'Report a problem dialog, empty message',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('account-menu').click();
      await app.page.getByTestId('account-report').click();
      await app.page.getByTestId('feedback').waitFor();
    },
  },
  {
    name: 'new-obs', scenario: 'sample', view: 'home', modal: 'newObs',
    steps: ['Home', '+ Add a project', 'New Open Bible Stories'],
    shows: 'New Open Bible Stories dialog, empty',
    run: async (app) => {
      await app.home(); await app.page.getByTestId('add-project').click();
      await app.page.getByTestId('add-project-obs').click();
      await app.page.getByRole('dialog', { name: 'New Open Bible Stories' }).waitFor();
    },
  },
  {
    name: 'add-book', scenario: 'sample', view: 'home', modal: 'addBook',
    steps: ['Home', 'Bible sample card: + Add a book'],
    shows: 'Add a book to the Bible sample: Start a blank book, or Add books from USFM files',
    run: async (app) => {
      await app.home();
      await app.card(BIBLE).getByRole('button', { name: /Add a book/ }).click();
      await app.page.getByRole('dialog', { name: 'Add a book' }).waitFor();
    },
  },
  {
    name: 'source-texts', scenario: 'sample', view: 'home', modal: 'sources',
    steps: ['Home', 'Bible sample card: Settings', 'Manage source texts'],
    shows: 'Get source texts: Pinned resources (Check for updates) and the gateway-language packages to choose from. Opening it opens the project, so Understand (Jonah) is behind it',
    run: async (app) => {
      await app.home();
      await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
      await app.page.getByTestId('settings-manage-sources').click();
      await app.page.getByTestId('sources-modal').waitFor();
    },
  },
  {
    name: 'guided-fix', scenario: 'sample', view: 'check', modal: 'fix',
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Words card: Fix this'],
    shows: 'Guided fix for the Translation Words tool: what is missing (the es-419 TWL/tW pins) and the ways to get it',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck();
      await app.page.getByTestId('fix-translationWords').click();
      await app.page.getByTestId('guided-fix').waitFor();
    },
  },
  {
    name: 'obs-understand', scenario: 'sample', view: 'read', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"'],
    shows: 'OBS Understand, story 1: story rail with the frame grid, one unit per frame with the comment box, helps panel',
    incomplete: 'Every gateway frame reads "Gateway text is unavailable." and the helps say "The pinned resource is not on this computer yet": the sample pins the es-419 OBS set (es-419_obs, es-419_obs-tn, es-419_obs-twl) for its primary set, and no copy is available. No picture pack (uW/obs_images_360) either. en-obs-understand shows the English set.',
    run: async (app) => { await app.home(); await app.openStory1(); },
  },
  {
    name: 'obs-translate', scenario: 'sample', view: 'draft', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"', 'Mode switch: Translate'],
    shows: 'OBS Translate, story 1: the gateway-story and picture notices, the title and frames 1-2 drafted in Spanish, "Draft frame N" pills',
    incomplete: 'The source column reads "Gateway text is unavailable." under the "pinned gateway story … is not on this machine / Get source" notice, and there are no pictures: no copy of the pinned es-419_obs v2 or of the picture pack uW/obs_images_360 is available. en-obs-translate shows the English gateway story.',
    run: async (app) => {
      await app.home(); await app.openStory1();
      await app.tab('Translate', app.page.getByTestId('story-draft'));
    },
  },
  {
    name: 'obs-check-picker', scenario: 'sample', view: 'check', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"', 'Mode switch: Check'],
    shows: 'OBS Check tool picker: Translation Words and Translation Notes "Needs downloading", Community Checking (no Align for OBS)',
    incomplete: 'Both helps tools read "Needs downloading": es-419_obs-twl / es-419_obs-tn are not vendored (see en-obs-check-tn-session for a ready session).',
    run: async (app) => { await app.home(); await app.openStory1(); await app.openCheck(); },
  },

  // -------- scenario: en-pins (the installed English suite pinned, as tC4 pins a new project) --------
  {
    name: 'en-understand-tit1', scenario: 'en-pins', view: 'read', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile'],
    shows: 'Understand, Titus 1, Section view, helps panel on Notes with the English Translation Notes v91 (intro note, abstract-nouns note)',
    run: async (app) => { await app.home(); await app.openTitus(); },
  },
  {
    name: 'en-understand-tw-article', scenario: 'en-pins', view: 'read', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Helps tab: Words', 'Card "of God": Read the article'],
    shows: 'Understand with the Words tab (Key word cards) and the Translation Words article pane open on "God" (kt/god.md)',
    run: async (app) => {
      const { page } = app;
      await app.home(); await app.openTitus();
      await page.getByRole('tab', { name: 'Words', exact: true }).click();
      await page.getByTestId('helps-word').filter({ hasText: 'of God' }).first().getByText('Read the article').click();
      await page.getByTestId('understand-article').waitFor();
      await page.getByText('Loading the article…').waitFor({ state: 'hidden' });
    },
  },
  {
    name: 'en-understand-questions', scenario: 'en-pins', view: 'read', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Helps tab: Questions'],
    shows: 'Understand with the Questions tab selected',
    incomplete: 'The tab shows its absent state "The pinned resource is not on this computer yet": tC4 does not vendor en_tq v91.',
    run: async (app) => {
      await app.home(); await app.openTitus();
      await app.page.getByRole('tab', { name: 'Questions', exact: true }).click();
    },
  },
  {
    name: 'en-translate-tit1', scenario: 'en-pins', view: 'draft', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Translate'],
    shows: 'Translate, Titus 1, Section mode, English helps, each "Draft section" a pill in the accent colour at the end of its row',
    run: async (app) => {
      await app.home(); await app.openTitus();
      await app.tab('Translate', app.page.getByRole('button', { name: /Draft section 1–3/ }));
    },
  },
  {
    name: 'en-translate-section-editor', scenario: 'en-pins', view: 'draft', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Translate', 'Draft section 1–3'],
    shows: 'Section editor open on Titus 1:1-3 (Type mode): one "N body" line per verse, Save section / Cancel; while it is open, the Understand and Check tabs, the book rail and the other "Draft section" pills are disabled',
    run: async (app) => {
      await app.home(); await app.openTitus();
      await app.tab('Translate', app.page.getByRole('button', { name: /Draft section 1–3/ }));
      await app.page.getByRole('button', { name: /Draft section 1–3/ }).click();
      await app.page.getByTestId('section-editor').waitFor();
    },
  },
  {
    name: 'en-translate-verse-editor', scenario: 'en-pins', view: 'draft', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Translate', 'Unit switch: Verse', 'Verse 2 (drafted text)', 'Scroll the editor to the middle of the pane'],
    shows: 'Verse mode with the verse editor open on Titus 1:2 ("Editing below" marker, "DRAFTING 2" card with Save verse / Cancel); while it is open, the Understand and Check tabs and the book rail are disabled',
    run: async (app) => {
      const { page } = app;
      await app.home(); await app.openTitus();
      await app.tab('Translate', page.getByRole('button', { name: /Draft section 1–3/ }));
      await page.getByRole('tab', { name: 'Verse', exact: true }).click();
      await page.getByTitle('Edit this verse').nth(1).click();
      await page.getByLabel('Verse 2', { exact: true }).waitFor();
      // The editor opens below the fold of the first section; centre it in its pane.
      await page.getByLabel('Verse 2', { exact: true }).evaluate((el) => el.scrollIntoView({ block: 'center' }));
    },
  },
  {
    name: 'en-check-picker', scenario: 'en-pins', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check'],
    shows: 'Check tool picker with every tool ready: Translation Words (2 of 182), Translation Notes (0 of 157), Align (0 of 5 verses), Community Checking',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); },
  },
  {
    name: 'en-check-tw-session', scenario: 'en-pins', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Words card: Start checking'],
    shows: 'Translation Words session (sorted By word), item "Paul" 1:1: rail, "Learn more: Paul, Saul", compare card (Greek, ULT highlight, the es-419 draft), the marking row ("← Previous", Mark valid, Invalid, To do, "Next →"), Bookmark / Add a comment; the header shows only "Item 1 of 182"',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationWords'); },
  },
  {
    name: 'en-check-tw-article', scenario: 'en-pins', view: 'check', modal: 'academy drawer',
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Words card: Start checking', 'Rail item "of God" (1:1)', 'Learn more: God'],
    shows: 'Translation Words session on the decided item "of God" with the article drawer open on "God" (kt/god.md)',
    run: async (app) => {
      const { page } = app;
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationWords');
      await page.getByTestId('check-list').getByRole('button').filter({ hasText: '“of God”' }).first().click();
      await page.getByTestId('open-academy').click();
      await page.getByTestId('academy-drawer').waitFor();
      await page.getByText('Loading the article…').waitFor({ state: 'hidden' });
    },
  },
  {
    name: 'en-check-tn-session', scenario: 'en-pins', view: 'check', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Notes card: Start checking'],
    shows: 'Translation Notes session on the English notes v91, item 1 (figs-abstractnouns, 1:1): the Greek verse, the quote highlighted in the ULT, the es-419 draft',
    run: async (app) => { await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes'); },
  },
  {
    name: 'en-check-ta-drawer', scenario: 'en-pins', view: 'check', modal: 'academy drawer',
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Notes card: Start checking', 'Learn more: Abstract Nouns'],
    shows: 'Translation Academy drawer on "Abstract Nouns" (translate/figs-abstractnouns) over the Translation Notes session',
    run: async (app) => {
      const { page } = app;
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes');
      await page.getByTestId('open-academy').click();
      await page.getByTestId('academy-drawer').waitFor();
      await page.getByText('Loading the article…').waitFor({ state: 'hidden' });
    },
  },
  {
    name: 'en-community-checking', scenario: 'en-pins', view: 'publish', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Community Checking card: Open'],
    shows: 'Community Checking: the typeset A4 preview of Titus (drop-cap chapter, "[ verse not yet drafted ]" runs) and the side panel with Export and Page setup',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck();
      await app.page.getByTestId('open-community-checking').click();
      await app.page.getByTestId('community-checking').waitFor();
    },
  },
  {
    name: 'en-obs-understand', scenario: 'en-pins', view: 'read', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"'],
    shows: 'OBS Understand, story 1: the English gateway frames with a comment box each, helps panel with the English OBS notes (en_obs-tn v13)',
    incomplete: 'No frame shows its picture: no copy of the picture pack uW/obs_images_360 is available.',
    run: async (app) => { await app.home(); await app.openStory1(); },
  },
  {
    name: 'en-obs-translate', scenario: 'en-pins', view: 'draft', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"', 'Mode switch: Translate'],
    shows: 'OBS Translate, story 1: the English gateway story beside the Spanish title and frames, English OBS helps',
    incomplete: 'No picture shows, and a notice lists why: no copy of the picture pack uW/obs_images_360 is available.',
    run: async (app) => {
      await app.home(); await app.openStory1();
      await app.tab('Translate', app.page.getByTestId('story-draft'));
    },
  },
  {
    name: 'en-obs-check-tn-session', scenario: 'en-pins', view: 'check', modal: null,
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"', 'Mode switch: Check', 'Translation Notes card: Start checking'],
    shows: 'OBS Translation Notes session, story 1 (sorted By frame), item "The Creation" 1:0: the title note, the English gateway title highlighted, the Spanish title, the marking row with "← Previous" and "Next →"',
    run: async (app) => { await app.home(); await app.openStory1(); await app.openCheck(); await app.openTool('translationNotes'); },
  },
];

// ---- the run ---------------------------------------------------------------

/** Fold reads that differ only in a story number (Home reads every gateway story title) into
 * one line, so the manifest stays readable. */
function compactReads(reads) {
  const groups = new Map();
  for (const read of reads) {
    const key = read.replace(/content\/\d\d\.md/, 'content/NN.md');
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(read);
  }
  return [...groups].map(([key, list]) => {
    if (list.length < 3) return list;
    const numbers = list.map((read) => read.match(/content\/(\d\d)\.md/)[1]);
    return [`${key.replace('NN', `{${numbers[0]}..${numbers.at(-1)}}`)} (${list.length} reads)`];
  }).flat();
}

async function captureScenario(browser, scenario, screens, tc4) {
  const state = path.join(HERE, '.state', scenario);
  buildState({ tc4: TC4, out: state, scenario });
  const mock = await startMockServer({ state, client: DIST, port });
  const context = await browser.newContext({ viewport: VIEWPORT, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'light' });
  await routeFakeDoor43(context);
  // Nothing else may leave the browser: any other origin is refused and reported.
  const external = new Set();
  await context.route((url) => url.origin !== mock.origin && url.origin !== 'https://git.door43.org', (route) => {
    external.add(route.request().url());
    return route.abort('blockedbyclient');
  });
  const inFlight = trackRequests(context);
  const page = await context.newPage();
  const pageErrors = [];
  page.on('pageerror', (error) => pageErrors.push(error.message));
  const app = appDriver({ page, origin: mock.origin, state });
  const results = [];
  for (const screen of screens) {
    const firstRequest = mock.requests.length;
    pageErrors.length = 0;
    process.stdout.write(`  ${screen.name} ... `);
    let error = null;
    try {
      await screen.run(app);
      await settle(page, inFlight);
      if (await page.getByTestId('view-error').count()) error = `tC4 view error: ${await page.getByTestId('view-error').innerText()}`;
    } catch (e) {
      error = e.message.split('\n')[0];
    }
    await page.mouse.move(POINTER_REST.x, POINTER_REST.y);
    await page.waitForTimeout(150);
    const file = path.join(SCREENS, `${screen.name}.png`);
    await page.screenshot({ path: file, animations: 'disabled' });
    const absent = compactReads([...new Set(mock.requests.slice(firstRequest)
      .filter((r) => r.status >= 400 && !/checking(\/|%2F)journal/.test(`${r.path}${r.query}`))
      .map((r) => `${r.method} ${decodeURIComponent(r.path.replace(/^\/api/, ''))}${decodeURIComponent(r.query)} -> ${r.status}`))]);
    console.log(error ? `FAILED: ${error}` : 'ok');
    results.push({
      name: screen.name,
      file: `screens/${screen.name}.png`,
      scenario,
      // tC4 has no router: the URL never changes, and the screen is the app state below.
      route: { url: '/clients/uw-tc4/', view: screen.view, modal: screen.modal },
      steps: screen.steps,
      shows: screen.shows,
      status: error ? 'failed' : screen.incomplete ? 'incomplete' : 'complete',
      ...(screen.incomplete ? { incomplete: screen.incomplete } : {}),
      ...(error ? { error } : {}),
      ...(pageErrors.length ? { pageErrors: [...pageErrors] } : {}),
      absentReads: absent,
      bytes: fs.statSync(file).size,
    });
  }
  await context.close();
  await mock.close();
  if (external.size) console.log(`  refused external requests: ${[...external].join(', ')}`);
  return { results, external: [...external] };
}

async function main() {
  const tc4 = tc4Identity();
  console.log(`tC4 ${tc4.version} at ${tc4.commit}${tc4.dirty ? ' (working tree has local changes)' : ''}`);
  const helpPages = buildClient(tc4.commit);
  if (process.argv.includes('--build-only')) return;
  fs.mkdirSync(SCREENS, { recursive: true });
  const selected = SCREEN_LIST.filter((s) => !only || only.includes(s.name));
  const browser = await chromium.launch({ args: CHROMIUM_ARGS });
  const screens = [];
  const refused = [];
  for (const scenario of Object.keys(SCENARIOS)) {
    const list = selected.filter((s) => s.scenario === scenario);
    if (!list.length) continue;
    console.log(`scenario ${scenario}: ${SCENARIOS[scenario]}`);
    const { results, external } = await captureScenario(browser, scenario, list, tc4);
    screens.push(...results);
    refused.push(...external);
  }
  const browserVersion = browser.version();
  await browser.close();

  // A partial run (--only) updates its screens inside the existing manifest.
  const manifestFile = path.join(HERE, 'manifest.json');
  const previous = only && fs.existsSync(manifestFile) ? JSON.parse(fs.readFileSync(manifestFile, 'utf8')).screens : [];
  const byName = new Map(previous.map((s) => [s.name, s]));
  for (const s of screens) byName.set(s.name, s);
  const ordered = SCREEN_LIST.map((s) => byName.get(s.name)).filter(Boolean);
  const stateDir = Object.keys(SCENARIOS).map((name) => path.join(HERE, '.state', name)).find((dir) => fs.existsSync(dir));
  const provenance = JSON.parse(fs.readFileSync(path.join(stateDir, 'provenance.json'), 'utf8'));
  const manifest = {
    tc4: { commit: tc4.commit, commitDate: tc4.commitDate, version: tc4.version, dirtyWorkingTree: tc4.dirty },
    client: `the tC4 commit built as \`npm run build\` builds it (release build: base /clients/uw-tc4/, DEV false), ${helpPages ? 'with' : 'WITHOUT'} the help pages of its pinned tc-website commit, served by the mock server`,
    browser: `Chromium ${browserVersion} (Playwright ${JSON.parse(fs.readFileSync(path.join(HERE, 'node_modules', 'playwright', 'package.json'), 'utf8')).version}), headless`,
    viewport: { ...VIEWPORT, deviceScaleFactor: 1 },
    scenarios: SCENARIOS,
    resources: provenance.resources.map(({ dir, repoPath, version, sha, substitute }) => ({ dir, repoPath, version, sha, ...(substitute ? { substitute } : {}) })),
    absentResources: ABSENT,
    refusedExternalRequests: refused,
    totals: {
      screens: ordered.length,
      complete: ordered.filter((s) => s.status === 'complete').length,
      incomplete: ordered.filter((s) => s.status === 'incomplete').length,
      failed: ordered.filter((s) => s.status === 'failed').length,
      bytes: ordered.reduce((n, s) => n + s.bytes, 0),
    },
    screens: ordered,
  };
  fs.writeFileSync(manifestFile, `${JSON.stringify(manifest, null, 2)}\n`);
  const t = manifest.totals;
  console.log(`${t.screens} screens: ${t.complete} complete, ${t.incomplete} incomplete, ${t.failed} failed; ${(t.bytes / 1024 / 1024).toFixed(2)} MB`);
  if (t.failed) process.exitCode = 1;
}

await main();
