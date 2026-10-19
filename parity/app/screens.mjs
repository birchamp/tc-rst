// The 62 baseline screens: what each shows and the steps that reach it, driven through
// `appDriver` (driver.mjs), so the same list runs on tC4 (env-tc4.mjs) and on the Rust app.
// Moved out of parity/tc4-baseline/capture.mjs unchanged, except that the project's share
// button and card are reached through the driver, whose environment names their test ids,
// and that every Door43 step is awaited, because another environment may answer it later.
import { FAKE_ORGANIZATIONS, FAKE_USER } from '../tc4-baseline/mock-server/fake-door43.mjs';

/** The two local projects, as tC4 names them: their repo paths in the Pankosmia layout. */
export const BIBLE = '_local_/_local_/sample_burrito';
export const OBS = '_local_/_local_/sample_burrito_obs';
/** A Door43 repository that no sample uses, so its default project name is free. */
const OPEN_REPO = `${FAKE_USER.username}/es-419_tit_equipo`;
/** The owner ruling under which every shared OBS card differs: tC4 reads "Upload changes" there. */
const OBS_SYNCS = { ruling: 'Q2', rebuild: 'The OBS card reads "Sync with Door43", like the Bible card: the rebuild syncs OBS projects too.' };
/** Why an ok sync toast is incomplete: its note counts pinned versions that the mock lacks.
 * The manifest lists them (`syncToast.missingVersions`), by tC4's own rule. */
const MISSING_VERSIONS = 'The note on missing resource versions counts the 5 versions that the Bible sample pins and the mock does not have: es-419_twl v18, es-419_tw v37, es-419_ta v4, en_tq v91 and hbo_uhb v3.0.0. With every pinned version on the computer, the toast shows no such note and no Download.';

// ---- the screens -------------------------------------------------------------
//
// Each screen: name (the PNG name), scenario (SCENARIOS of
// parity/tc4-baseline/fixtures/build-state.mjs), view (tC4's s.view: home | read | draft |
// check | publish; read = Understand, draft = Translate), modal (what is layered over the
// view), steps (what a person does, in tC4's own words), shows, incomplete (why the screen
// cannot show what a full install shows, when that is so), and run (the same steps, driven).
// Optional: locale (the app language, which run passes to home(); English by default),
// viewport (another window size than 1280x800), markingRow (the tool whose whole marking row
// the image shows) and rebuildDiffers (an owner ruling under which the rebuild must not copy
// this screen exactly: the ruling, as numbered in docs/TC4-DELTA-2026-10-16.md §5, and what
// the rebuild shows). The manifest also records what the DOM shows at the shot: `homeCards`
// and `syncToast` (see `observe` in observe.mjs).

export const SCREEN_LIST = [
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
    name: 'align-workspace-row', scenario: 'sample', view: 'check', modal: null, markingRow: 'align',
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Align card', 'Scroll the marking row into view'],
    shows: 'align-workspace scrolled to the marking row: the word cards, then "← Previous", Mark valid and "Next →" under the card',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openAlign();
      await app.page.getByTestId('align-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'ar-align-workspace-row', scenario: 'sample', view: 'check', modal: null, markingRow: 'align', locale: 'ar',
    steps: ['App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Align card', 'Scroll the marking row into view'],
    shows: 'align-workspace with the interface in Arabic, scrolled to the marking row: the chrome and the marking row run right to left ("→ السابق", Mark valid, "التالي ←", with the arrows turned); the ULT, the Greek and the draft stay left to right',
    run: async (app) => {
      await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openAlign();
      await app.page.getByTestId('align-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'align-workspace-620', scenario: 'sample', view: 'check', modal: null, markingRow: 'align', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Align card', 'Scroll the marking row into view'],
    shows: 'align-workspace in a window 620 px wide, scrolled to the marking row, which wraps onto two lines ("Next →" alone on the second); at this width the project button in the header lies over the mode switch',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openAlign();
      await app.page.getByTestId('align-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'ar-align-workspace-620', scenario: 'sample', view: 'check', modal: null, markingRow: 'align', locale: 'ar', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Align card', 'Scroll the marking row into view'],
    shows: 'ar-align-workspace-row in a window 620 px wide: the marking row wraps onto two lines, right to left ("التالي ←" alone on the second)',
    run: async (app) => {
      await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openAlign();
      await app.page.getByTestId('align-next').scrollIntoViewIfNeeded();
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
      await app.shareButton(BIBLE).click();
      await app.page.getByTestId('net-ask').waitFor();
    },
  },
  {
    name: 'share-signin', scenario: 'sample', view: 'home', modal: 'signIn',
    steps: ['Home', 'Bible sample card: Share on Door43', 'Turn on internet'],
    shows: 'Sign in to Door43 dialog (signs in to git.door43.org), empty fields, and the notice that the Door43 username goes into the project, stays in it, and goes into each Scripture Burrito export',
    run: async (app) => {
      await app.home();
      await app.shareButton(BIBLE).click();
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
      await app.shareButton(BIBLE).click();
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
      await app.shareButton(BIBLE).click();
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
  {
    name: 'open-door43', scenario: 'sample', view: 'home', modal: 'openDoor43',
    steps: ['Home', '+ Add a project', 'Open a project from Door43'],
    shows: 'Open a project from Door43 dialog, empty: the note "The project opens from git.door43.org. Only public repositories can be opened.", Door43 repository, Project name, the note that tC4 checks every file before it adds the project, Cancel / Open project (disabled)',
    rebuildDiffers: { ruling: 'Q3', rebuild: 'The note has no sentence that only public repositories can be opened: a new key replaces openDoor43.where in all 10 languages. A private repository opens after sign-in.' },
    run: async (app) => {
      await app.home(); await app.page.getByTestId('add-project').click();
      await app.page.getByText('Open a project from Door43', { exact: true }).click();
      await app.page.getByTestId('open-door43-modal').waitFor();
    },
  },
  {
    name: 'open-door43-internet-consent', scenario: 'sample', view: 'home', modal: 'netAsk',
    steps: ['Home', '+ Add a project', 'Open a project from Door43', `Repository: ${OPEN_REPO}`, 'Open project'],
    shows: '"Turn on the internet?" asked for an open from Door43 (kind `open`): get a full copy of the project and check its files; nothing from your projects is sent',
    rebuildDiffers: { ruling: 'Q3', rebuild: 'Behind the scrim, the note of the Open a project from Door43 dialog has no sentence that only public repositories can be opened (see open-door43).' },
    run: async (app) => {
      const { page } = app;
      await app.home(); await page.getByTestId('add-project').click();
      await page.getByText('Open a project from Door43', { exact: true }).click();
      await page.getByTestId('open-door43-repo').fill(OPEN_REPO);
      await page.getByTestId('open-door43-submit').click();
      await page.locator('[data-testid="net-ask"][data-kind="open"]').waitFor();
    },
  },
  {
    name: 'open-door43-refused', scenario: 'sample', view: 'home', modal: 'openDoor43',
    steps: ['Home', '+ Add a project', 'Open a project from Door43', `Repository: ${OPEN_REPO}`, 'Open project', 'Turn on internet'],
    shows: 'Open a project from Door43 dialog with the refusal callout "This project cannot be opened." for a clone that failed (code sync.clone-failed), "No project was added." and Ask for help. The clone fails because Door43 has no repository of that name',
    rebuildDiffers: { ruling: 'Q3', rebuild: 'The refusal sentence (openDoor43.refused.other) says "Make sure that the repository is public". The rebuild opens private repositories, so its sentence must not say this. The delta document replaces only openDoor43.where, so the new text of this sentence is not decided yet. When Door43 answers 401 or 404 and nobody is signed in, the rebuild shows the sign-in step instead of this refusal.' },
    run: async (app) => {
      const { page } = app;
      await app.home(); await app.door43Lacks(OPEN_REPO);
      await page.getByTestId('add-project').click();
      await page.getByText('Open a project from Door43', { exact: true }).click();
      await page.getByTestId('open-door43-repo').fill(OPEN_REPO);
      await page.getByTestId('open-door43-submit').click();
      await page.getByTestId('net-confirm').click();
      await page.getByTestId('open-door43-refused').waitFor();
    },
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
    name: 'en-check-tw-session', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationWords',
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
    name: 'en-obs-check-tn-session', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationNotes',
    steps: ['Home', 'OBS sample card: story tile "1 · La Creación"', 'Mode switch: Check', 'Translation Notes card: Start checking'],
    shows: 'OBS Translation Notes session, story 1 (sorted By frame), item "The Creation" 1:0: the title note, the English gateway title highlighted, the Spanish title, the marking row with "← Previous" and "Next →"',
    run: async (app) => { await app.home(); await app.openStory1(); await app.openCheck(); await app.openTool('translationNotes'); },
  },
  {
    name: 'ar-check-tw-session', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationWords', locale: 'ar',
    steps: ['App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Translation Words card: Start checking'],
    shows: 'en-check-tw-session with the interface in Arabic: the whole chrome right to left, and the marking row "→ السابق", the triage buttons, "التالي ←", with the arrows turned for right to left',
    run: async (app) => { await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openTool('translationWords'); },
  },
  {
    name: 'en-check-tw-session-620', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationWords', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Words card: Start checking', 'Scroll the marking row into view'],
    shows: 'en-check-tw-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines; at this width the project button in the header lies over the mode switch',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationWords');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'ar-check-tw-session-620', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationWords', locale: 'ar', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Translation Words card: Start checking', 'Scroll the marking row into view'],
    shows: 'ar-check-tw-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines, right to left',
    run: async (app) => {
      await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openTool('translationWords');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'en-check-tn-session-row', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationNotes',
    steps: ['Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Notes card: Start checking', 'Scroll the marking row into view'],
    shows: 'en-check-tn-session scrolled to the marking row: the end of WHAT TO CHECK, "Learn more: Abstract Nouns", the compare card, then "← Previous", Mark valid, Invalid, To do, "Next →", and Bookmark / Add a comment',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'ar-check-tn-session-row', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationNotes', locale: 'ar',
    steps: ['App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Translation Notes card: Start checking', 'Scroll the marking row into view'],
    shows: 'en-check-tn-session-row with the interface in Arabic: the chrome and the marking row run right to left ("→ السابق", the triage buttons, "التالي ←", with the arrows turned)',
    run: async (app) => {
      await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'en-check-tn-session-620', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationNotes', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'Home', 'Bible sample card: Titus tile', 'Mode switch: Check', 'Translation Notes card: Start checking', 'Scroll the marking row into view'],
    shows: 'en-check-tn-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines; at this width the project button in the header lies over the mode switch',
    run: async (app) => {
      await app.home(); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  {
    name: 'ar-check-tn-session-620', scenario: 'en-pins', view: 'check', modal: null, markingRow: 'translationNotes', locale: 'ar', viewport: { width: 620, height: 800 },
    steps: ['Window 620 px wide', 'App language: العربية (saved before the launch)', 'Home', 'Bible sample card: Titus tile', 'Mode switch: مراجعة (Check)', 'Translation Notes card: Start checking', 'Scroll the marking row into view'],
    shows: 'ar-check-tn-session-row in a window 620 px wide: the marking row wraps onto three lines, right to left',
    run: async (app) => {
      await app.home({ locale: 'ar' }); await app.openTitus(); await app.openCheck(); await app.openTool('translationNotes');
      await app.page.getByTestId('check-next').scrollIntoViewIfNeeded();
    },
  },
  // -------- scenario: shared (both samples shared on Door43) --------
  {
    name: 'shared-home', scenario: 'shared', view: 'home', modal: null,
    steps: ['Launch tC4'],
    shows: 'Home with both samples shared on Door43: "On Door43" beside each name; the Bible card reads "Sync with Door43", the OBS card "Upload changes" (tC4 4.0 syncs Bible projects only)',
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => { await app.home(); await app.shareCard(BIBLE).and(app.page.locator('[data-action="sync"]')).waitFor(); },
  },
  {
    name: 'sync-internet-consent', scenario: 'shared', view: 'home', modal: 'netAsk',
    steps: ['Home', 'Bible sample card: Sync with Door43'],
    shows: '"Turn on the internet?" asked for a sync (kind `sync`): get the changes of your teammates and send the changes of this project. Behind the scrim, the OBS card reads "Upload changes"',
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => {
      await app.home();
      await app.shareButton(BIBLE).click();
      await app.page.locator('[data-testid="net-ask"][data-kind="sync"]').waitFor();
    },
  },
  {
    name: 'sync-failed', scenario: 'shared', view: 'home', modal: null,
    steps: ['Home', 'Door43 main: the Bible sample as it is now (the push of its first share)', 'Door43: the connection drops at the next clone of the Bible sample\'s repository', 'Bible sample card: Sync with Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`],
    shows: 'Home with the sync result toast of a sync that did not complete: red dot, "The sync did not complete." and the sentence of code sync.clone-failed, the dismiss button. Door43 has main, so the sync starts, and then the clone of main fails because the connection to git.door43.org drops',
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => {
      await app.home();
      await app.pushToDoor43(BIBLE);
      await app.dropNextClone(BIBLE);
      await app.syncFromHome(BIBLE);
      await app.page.locator('[data-testid="sync-result"][data-ok="0"]').waitFor();
    },
  },
  {
    name: 'sync-ok', scenario: 'shared', view: 'home', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Home', 'Door43 main: the Bible sample as it is now (the push of its first share)', 'Bible sample card: Sync with Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`],
    shows: 'Home with the toast of a sync that completed with no change from teammates: kindle dot, "Synced with Door43 · 0 changes from teammates · 0 conflicts kept the newest version · 0 verse conflicts to review", the note "This project now uses 5 resource versions that are not on this computer.", Download and the dismiss button',
    incomplete: `${MISSING_VERSIONS} Its dot is then green.`,
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => {
      await app.home(); await app.openOnceBeforePush();
      await app.pushToDoor43(BIBLE);
      await app.syncFromHome(BIBLE);
      await app.page.locator('[data-testid="sync-result"][data-ok="1"]').waitFor({ timeout: 120_000 });
    },
  },
  {
    name: 'sync-ok-teammates', scenario: 'shared', view: 'home', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Home', 'Door43 main: the Bible sample as it is now (the push of its first share)', 'Door43 main: two teammates change Titus 1:1 from the same version', 'Bible sample card: Sync with Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`],
    shows: 'Home with the toast of a sync that brought two teammates\' changes to Titus 1:1: kindle dot, "Synced with Door43 · 2 changes from teammates · 0 conflicts kept the newest version · 1 verse conflict to review", "Changes from: Teammate 1 · Teammate 2", the note on the 5 missing resource versions, Review, Download and the dismiss button; the Bible card shows the chip "1 conflict · Review" and the Titus tile "1 conflict"',
    incomplete: MISSING_VERSIONS,
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => {
      await app.home(); await app.openOnceBeforePush();
      await app.teammatesChangeOneVerse(BIBLE);
      await app.syncFromHome(BIBLE);
      await app.page.locator('[data-testid="sync-result"][data-ok="1"]').waitFor({ timeout: 120_000 });
    },
  },
  {
    name: 'sync-ok-restored', scenario: 'shared', view: 'home', modal: null,
    steps: ['Home', 'Bible sample card: Titus tile', 'Home', 'Door43 main: the Bible sample as it is now (the push of its first share)', 'Door43 main: two teammates change Titus 1:1 from the same version', 'Bible sample card: Sync with Door43', 'Turn on internet', `Sign in as ${FAKE_USER.username}`, 'Sync result: dismiss', 'Door43 main: the second teammate\'s change is gone, as when someone replaces main with an older copy', 'Bible sample card: Sync with Door43'],
    shows: 'Home with the toast of a second sync, after Door43 main lost a teammate\'s change that the first sync brought to this computer: kindle dot, "Synced with Door43 · 0 changes from teammates · 0 conflicts kept the newest version · 1 verse conflict to review", the note "Door43 did not have 1 change from a teammate. tC4 sent it again.", the note on the 5 missing resource versions, Review, Download and the dismiss button',
    incomplete: MISSING_VERSIONS,
    rebuildDiffers: OBS_SYNCS,
    run: async (app) => {
      await app.home(); await app.openOnceBeforePush();
      const [, second] = await app.teammatesChangeOneVerse(BIBLE);
      await app.syncFromHome(BIBLE);
      const toast = app.page.getByTestId('sync-result');
      await app.page.locator('[data-testid="sync-result"][data-ok="1"]').waitFor({ timeout: 120_000 });
      await toast.getByTestId('sync-dismiss').click();
      await toast.waitFor({ state: 'detached' });
      await app.loseFromDoor43(BIBLE, second);
      // The internet is on and the sign-in is kept, so the second sync asks nothing.
      await app.shareButton(BIBLE).click();
      await app.page.locator('[data-testid="sync-result"][data-ok="1"]').waitFor({ timeout: 120_000 });
    },
  },
];
