// The recorded states of the gallery: where real tC4, driven against the baseline's mock
// (parity/app/env-tc4.mjs), is taken before its whole state is read from its React tree
// (record-states.mjs). A gallery case starts from one of them (cases.mjs).
//
// Each state: name; scenario (parity/tc4-baseline/fixtures/build-state.mjs SCENARIOS); locale;
// baseline (the baseline screen the state is, when the route ends exactly there: its real
// capture must then equal the committed screen); route (the steps from a fresh environment,
// through the driver of parity/app/driver.mjs and, where `portable` is false, the tC4
// environment's own handles); portable (the route uses only the driver, so the Rust app can
// take it too); calls (read-only actions whose replies the view draws, called once the state
// is read, their replies recorded); focus (steps that put the focus back where the route left
// it, which a state drawn afresh does not have: calibration replays them, and so does every
// case on the state that has no patch); notes.
import fs from 'node:fs';
import path from 'node:path';
import { FAKE_USER } from '../tc4-baseline/mock-server/fake-door43.mjs';
import { BIBLE, SCREEN_LIST } from '../app/screens.mjs';
import { REPO } from './node-lib.mjs';

const IMPORT_FIXTURES = path.join(REPO, 'fixtures', 'tc4', 'conformance-fixtures', 'import');

/** The focus a route leaves when its last click took away the control it had focused (a step
 * of a dialog replaced by the next): the page has it, not a control. */
const BLUR = [{ do: 'blur' }];

/** The steps of a baseline screen. */
const screen = (name) => {
  const found = SCREEN_LIST.find((s) => s.name === name);
  if (!found) throw new Error(`no baseline screen ${name}`);
  return (app) => found.run(app);
};

/** Home, then Titus opened once and left: tC4 seeds a conformance sample's journal at its
 * first open, so a change made outside the app must come after it. */
async function homeAfterTitus(app) {
  await app.home();
  await app.openTitus();
  await app.page.getByTitle(/Switch project/).click();
  await app.card(BIBLE).waitFor();
}

/** Home's open error: a file tC4 never wrote appears in the project, and the next open
 * refuses it (state.jsx performProjectOpen). */
async function homeOpenError(app, env) {
  await homeAfterTitus(app);
  const file = path.join(env.state, 'repos', ...BIBLE.split('/'), 'ingredients', 'checking', 'custom', 'notes.json');
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, '{"note":"written outside the app"}\n');
  await app.card(BIBLE).getByRole('button', { name: /Titus/ }).click();
  await app.page.getByTestId('home-open-error').waitFor({ timeout: 60_000 });
}

/** Import a project, of `kind`, with `file` chosen. */
async function importFiles(app, kind, file) {
  const { page } = app;
  await app.home();
  await page.getByTestId('add-project').click();
  await page.getByTestId('add-project-import').click();
  await page.getByTestId(`import-kind-${kind}`).click();
  await page.getByTestId('import-file-input').setInputFiles(path.join(IMPORT_FIXTURES, file));
  await page.getByTestId('import-file').waitFor();
}

async function importReview(app, kind, file) {
  await importFiles(app, kind, file);
  await app.page.getByTestId('import-to-review').click();
  await app.page.getByTestId('import-run').waitFor();
  await app.page.getByTestId('import-review').waitFor();
}

async function addBookPick(app) {
  await screen('add-book')(app);
  await app.page.getByRole('button', { name: /Start a blank book/ }).click();
  await app.page.locator('#ab-book').waitFor();
}

async function addBookUsfm(app) {
  await screen('add-book')(app);
  await app.page.getByTestId('ab-usfm-option').click();
  await app.page.getByTestId('ab-usfm').waitFor();
}

export const STATES = [
  {
    name: 'home', scenario: 'sample', locale: 'en', baseline: 'home', portable: true, calls: [],
    route: screen('home'),
    notes: 'Home of the sample scenario: the baseline screen home.',
  },
  {
    name: 'check-picker', scenario: 'sample', locale: 'en', baseline: 'check-picker', portable: true, calls: [], focus: [{ do: 'refocus', target: '[role="tab"][aria-selected="true"]' }],
    route: screen('check-picker'),
    notes: 'The Check tool picker for Titus: the baseline screen check-picker.',
  },
  {
    name: 'check-tn', scenario: 'sample', locale: 'en', baseline: 'check-tn-session', portable: true, calls: [],
    route: screen('check-tn-session'),
    notes: 'The Translation Notes session at item 2: the baseline screen check-tn-session.',
  },
  {
    name: 'align', scenario: 'sample', locale: 'en', baseline: 'align-workspace', portable: true, calls: [],
    route: screen('align-workspace'),
    notes: 'Align at Titus 1:1: the baseline screen align-workspace.',
  },
  {
    name: 'cc-obs', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: ['readPrintStories'],
    route: async (app) => {
      await screen('obs-check-picker')(app);
      await app.page.getByTestId('open-community-checking').click();
      await app.page.getByTestId('community-checking').waitFor();
      await app.page.getByTestId('cc-pictures').waitFor();
    },
    notes: 'OBS Community Checking of the sample story project. The view draws the reply of readPrintStories, which is recorded.',
  },
  {
    name: 'review', scenario: 'shared', locale: 'en', baseline: null, portable: false, calls: [],
    route: async (app) => {
      await screen('sync-ok-teammates')(app);
      await app.page.getByTestId('sync-review').click();
      await app.page.getByTestId('conflict-version').first().waitFor();
    },
    notes: 'The verse conflict review after the sync of sync-ok-teammates: two teammates changed Titus 1:1 from one version. Not portable: the teammates write through the tC4 environment.',
  },
  {
    name: 'review-mine', scenario: 'shared', locale: 'en', baseline: null, portable: false, calls: [],
    route: async (app) => {
      const { page } = app;
      await app.home();
      await app.openTitus();
      await app.home();
      await app.teammatesChangeOneVerse(BIBLE);
      await app.openTitus();
      await app.tab('Translate', page.getByRole('button', { name: /Draft section 1–3/ }));
      await page.getByRole('tab', { name: 'Verse', exact: true }).click();
      await page.getByTitle('Edit this verse').nth(0).click();
      const box = page.getByLabel('Verse 1', { exact: true });
      await box.waitFor();
      await box.fill('Pablo, mi versión del versículo.');
      await page.getByRole('button', { name: 'Save verse' }).click();
      await page.locator('[data-testid="save-indicator"][data-state="saved"]').waitFor({ timeout: 30_000 });
      await page.getByTitle(/Switch project/).click();
      await app.card(BIBLE).waitFor();
      await app.syncFromHome(BIBLE);
      await page.locator('[data-testid="sync-result"][data-ok="1"]').waitFor({ timeout: 120_000 });
      await page.getByTestId('sync-review').click();
      await page.getByTestId('conflict-version').first().waitFor();
      const owners = await page.getByTestId('conflict-version').evaluateAll((els) => els.map((e) => e.dataset.owner));
      if (owners.length !== 3 || owners.filter((o) => o === 'mine').length !== 1) throw new Error(`the review shows the versions of ${owners.join(', ')}, not two teammates' and mine`);
    },
    notes: 'The conflict review with a version of mine: I saved Titus 1:1 while two teammates changed it on Door43, then synced.',
  },
  {
    name: 'gateway', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [],
    route: async (app) => {
      await screen('project-settings')(app);
      await app.page.locator('[data-testid^="settings-gateway-"][data-current="0"]').first().click();
      await app.page.getByTestId('gateway-change').waitFor({ timeout: 60_000 });
    },
    notes: 'The gateway change dialog from Project settings: the English row of the checking language (it opens the project to compute what changes).',
  },
  {
    name: 'sources', scenario: 'sample', locale: 'en', baseline: 'source-texts', portable: true, calls: [],
    route: screen('source-texts'),
    notes: 'Get source texts over Project settings: the baseline screen source-texts.',
  },
  {
    name: 'guided-fix', scenario: 'sample', locale: 'en', baseline: 'guided-fix', portable: true, calls: [],
    route: screen('guided-fix'),
    notes: 'The guided fix of Translation Words: the baseline screen guided-fix.',
  },
  {
    name: 'section-editor', scenario: 'en-pins', locale: 'en', baseline: 'en-translate-section-editor', portable: true, calls: [],
    route: screen('en-translate-section-editor'),
    notes: 'The section editor of Titus 1:1-3: the baseline screen en-translate-section-editor.',
  },
  {
    name: 'feedback-ask', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [],
    route: async (app, env) => {
      await homeOpenError(app, env);
      await app.page.getByTestId('ask-for-help').click();
      await app.page.getByTestId('feedback').waitFor();
    },
    notes: 'Feedback opened by "Ask for help" on Home\'s open error: tC4 fills a bug report with the refusal (state.jsx askForHelp). Not portable: the route writes a file into the project.',
  },
  {
    name: 'signin-error', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [], focus: BLUR,
    route: async (app) => {
      const { page } = app;
      await app.home();
      await app.shareButton(BIBLE).click();
      await page.getByTestId('net-confirm').click();
      await page.getByLabel('Door43 username or email').fill(FAKE_USER.username);
      await page.getByLabel('Password', { exact: true }).fill('wrong');
      await page.getByTestId('signin-submit').click();
      await page.getByTestId('signin-error').waitFor();
    },
    notes: 'The sign-in step after Door43 refused a wrong password.',
  },
  {
    name: 'share-check', scenario: 'sample', locale: 'en', baseline: 'share-dialog-check', portable: true, calls: [], focus: [{ do: 'refocus', target: '[data-testid="share-submit"]' }],
    route: screen('share-dialog-check'),
    notes: 'Share on Door43 at "Check what will be shared": the baseline screen share-dialog-check.',
  },
  {
    name: 'import-files', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [], focus: BLUR,
    route: (app) => importFiles(app, 'usfm', 'usfm/57-TIT.usfm'),
    notes: 'Import a project, USFM files, with 57-TIT.usfm chosen. Not portable: a file upload.',
  },
  {
    name: 'import-review-usfm', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [], focus: BLUR,
    route: (app) => importReview(app, 'usfm', 'usfm/57-TIT.usfm'),
    notes: 'The import review of 57-TIT.usfm.',
  },
  {
    name: 'import-review-tc3', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [], focus: BLUR,
    route: (app) => importReview(app, 'tc3', 'tc3/cfm_fbt_tit_book.zip'),
    notes: 'The import review of the tC3 project cfm_fbt_tit_book.zip.',
  },
  {
    name: 'add-book-pick', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [], focus: BLUR,
    route: addBookPick,
    notes: 'Add a book, "Start a blank book": the book picker.',
  },
  {
    name: 'add-book-pick-multi', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [], focus: [{ do: 'refocus', target: '[role="dialog"] button[data-selected="true"] >> nth=1' }],
    route: async (app) => {
      const { page } = app;
      await addBookPick(app);
      await page.getByRole('button', { name: /Choose several at once/ }).click();
      for (const book of ['Philemon', 'Hebrews']) await page.getByRole('button', { name: book, exact: true }).click();
      await page.getByRole('button', { name: 'Create 2 books' }).waitFor();
    },
    notes: 'Add a book, several at once, with Philemon and Hebrews chosen.',
  },
  {
    name: 'add-book-usfm', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [], focus: BLUR,
    route: addBookUsfm,
    notes: 'Add a book from USFM files, no file chosen yet.',
  },
  {
    name: 'add-book-usfm-files', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [], focus: BLUR,
    route: async (app) => {
      const { page } = app;
      await addBookUsfm(app);
      await page.getByTestId('ab-usfm-input').setInputFiles(['usfm/57-TIT.usfm', 'usfm/58-PHM.txt'].map((f) => path.join(IMPORT_FIXTURES, f)));
      await page.locator('[data-testid="ab-usfm-file"][data-status="refused"]').waitFor();
      await page.locator('[data-testid="ab-usfm-file"][data-status="valid"]').waitFor();
    },
    notes: 'Add a book from USFM files with 57-TIT.usfm (refused: Titus is in the project) and 58-PHM.txt (valid) chosen. Not portable: a file upload.',
  },
  {
    name: 'resume', scenario: 'sample', locale: 'en', baseline: null, portable: true, calls: [],
    route: async (app) => {
      const { page } = app;
      await screen('translate-tit1')(app);
      await page.getByRole('button', { name: /Draft section 1–3/ }).click();
      await page.getByTestId('section-editor').waitFor();
      await page.locator('[data-testid="section-editor"] textarea').press('End');
      await page.keyboard.type(' x');
      await page.getByRole('button', { name: 'Save section' }).click();
      await page.locator('[data-testid="save-indicator"][data-state="saved"]').waitFor({ timeout: 30_000 });
      await page.getByTitle(/Switch project/).click();
      await page.getByTestId('resume-card').waitFor();
    },
    notes: 'Home with the Resume card, after a section of Titus 1 was saved and the project left.',
  },
  {
    name: 'home-open-error', scenario: 'sample', locale: 'en', baseline: null, portable: false, calls: [], focus: [{ do: 'refocus', target: '[data-testid="book-tile-TIT"]' }],
    route: homeOpenError,
    notes: 'Home after an open refused a file written outside the app into the Bible sample. Not portable: the route writes into the project.',
  },
];

/** The state named `name`. */
export function stateNamed(name) {
  const found = STATES.find((s) => s.name === name);
  if (!found) throw new Error(`no recorded state ${name}`);
  return found;
}
