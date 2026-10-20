// The cases of the gallery (APP-DESIGN 7.4), in report order. A case is a recorded state
// (states.mjs), an optional patch, optional steps, and, for a DOM-first group, the scope its
// DOM is compared in until its pixels unit captures it whole.
//
// A case: name (kebab-case, never a baseline screen's name); group (a group of §7.4's table,
// whose units capture the Rust side: never typed here); state; patch (s, { state, kit }) =>
// the `set` patch tC4's reducer would apply, written only where the mock cannot go, from the
// recorded values and tC4's own code (`kit`, patch-kit.mjs) wherever it can be; returns
// (replies added over the state's recorded ones); steps (steps.mjs); scope; masks (masks of
// parity/app/masks.json whose element the case shows); rust (how the Rust app reaches the
// case, when the default does not apply: parity/app/capture-rust.mjs --case); why.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { applyPatch } from './lib.mjs';
import { REPO } from './node-lib.mjs';
import { stateNamed } from './states.mjs';

/** tC4's repo path of the Bible sample. */
const BIBLE = '_local_/_local_/sample_burrito';
/** The last dialog on the page: a DOM-first case compares it alone. */
const LAST_DIALOG = { attr: 'role', value: 'dialog', which: 'last' };
/** The times of a Report, which no view shows; tC4 refuses a Report without them. */
const AT = '2026-10-18T12:00:00.000Z';
/** What a request answers when the local server does not: Chromium's message for a fetch that
 * could not connect, which tC4 does not wrap (ServerApi awaits fetch). Used where a case shows
 * a failed read, so the failure is one tC4 really meets and no text is made up. */
const FETCH_FAILED = 'Failed to fetch';

/** A case's hand replies (`replies/<case>.json`), which its Rust route gives the page or the
 * server (APP-DESIGN §7.4). */
const readReplies = (name) => JSON.parse(fs.readFileSync(path.join(REPO, 'parity', 'view-gallery', 'replies', `${name}.json`), 'utf8'));
/** The `home/list` reply a case's Rust route answers once. */
const homeReply = (name) => readReplies(name).replies.find((r) => r.endpoint === 'home/list').reply;
const newRequestId = () => crypto.randomBytes(16).toString('hex');

/** B3's route to the open overlay: Home, the open held before the store opens the project
 * (`test/hold` at `open`), `jobs/get` answering the open job with `progress` (the stage the case
 * shows; the overlay reads it at its next look at the job), and the Titus tile clicked. The
 * open stays in flight for the shot (`held`). */
const heldOpen = (progress) => async (app, env) => {
  // A route of an earlier case's shot (a read held unanswered) goes first.
  await app.page.unrouteAll({ behavior: 'ignoreErrors' });
  await app.home();
  await env.call('test/hold', { request: newRequestId(), at: 'open' });
  if (progress) {
    await app.page.route('**/api/v1/jobs/get', async (route) => {
      const { job } = JSON.parse(route.request().postData());
      const value = { id: job, kind: 'open', project: env.projectId(BIBLE), state: { type: 'running', value: progress } };
      await route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ type: 'ok', value }) });
    });
  }
  await app.card(BIBLE).getByRole('button', { name: /Titus/ }).click();
  const stage = progress ? { readingChanges: 'journal', finishingWork: 'state', writingFiles: 'prepare' }[progress.phase.value] : 'journal';
  await app.page.locator(`[data-testid="open-progress"][data-stage="${stage}"]`).waitFor();
  if (progress?.total) await app.page.getByText(`${progress.done} of ${progress.total}`, { exact: false }).waitFor();
};
/** C8's route to the change of checking language: Home, the Bible sample's Settings, English
 * chosen on the Checking language card, which opens the project and asks the change. */
const gatewayChange = async (app) => {
  await app.page.unrouteAll({ behavior: 'ignoreErrors' });
  await app.home();
  await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
  await app.page.getByTestId('settings-gateway-en::unfoldingWord').click();
  await app.page.getByTestId('gateway-change').waitFor();
};
/** C8's data for the confirmations the mock draws with hand-made carry-over counts (the cases'
 * `why`): the offers the server answers and the plan it makes, scripted through `test/script`
 * (a test hook, never in a release build), and the route to the confirmation from Home. The
 * rows are the recorded gateway change's, as tC4's own cases borrow them. */
const unit = { type: 'book', value: 'TIT' };
const planRow = (tool, decisions, against, carry) => ({ tool, unit, decisions, against, carry });
const wordsRow = (carry) => planRow('translationWords', 2, 'es-419_twl', carry);
const notesRow = (carry) => planRow('translationNotes', 2, 'es-419_tn', carry);
const carried = (carriedN, checkAgain) => ({ carried: carriedN, checkAgain });
const move = (repo, from, to) => ({ repo, from, to, published: null });
const tnMove = move('es-419_tn', 'v66', 'v67');
const twlMove = move('es-419_twl', 'v18', 'v19');
const taMove = move('es-419_ta', 'v4', 'v5');
const setPlan = (moves, rows, blocked = []) => ({
  plan: 90, what: { type: 'set', value: { rung: 'primary', language: 'es-419', moves } },
  decisionsAtRisk: rows.reduce((n, r) => n + r.decisions, 0), rows, blocked, alignments: null,
});
const offers = (primaryMoves, texts = [], sets = true) => ({
  sets: sets ? [{ rung: 'primary', language: 'es-419', moves: primaryMoves }, { rung: 'fallback', language: 'en', moves: [] }] : [],
  texts, textsError: null,
});
const script = (env, next) => env.call('test/script', { request: newRequestId(), next });

/** Home, the internet on, the offers and the plan scripted, the Bible sample's Settings, Manage
 * source texts, Check for updates, and the offer's Upgrade: the confirmation over Get source
 * texts. `apply` presses Upgrade too, which the server refuses (a scripted plan is kept under no
 * id: PlanStale, tC4's "the plan no longer matches the project"). */
const upgradeConfirmation = ({ updates, plan, offer = 'upgrade-set-primary', apply = false }) => async (app, env) => {
  await app.page.unrouteAll({ behavior: 'ignoreErrors' });
  await app.home();
  await env.call('net/set', { request: newRequestId(), on: true });
  await script(env, { type: 'updates', value: updates });
  await script(env, { type: 'plan', value: plan });
  await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
  await app.page.getByTestId('settings-manage-sources').click();
  await app.page.getByTestId('sources-modal').waitFor();
  await app.page.getByTestId('check-updates').click();
  await app.page.getByTestId(offer).waitFor();
  await app.page.getByTestId(offer).click();
  await app.page.getByTestId('upgrade-confirm').waitFor();
  // Pressing the offer scrolled Get source texts to it; tC4's shot is the dialog at its top.
  await app.page.evaluate(() => {
    for (const el of document.querySelectorAll('[data-testid="sources-modal"] *')) if (el.scrollTop > 0) el.scrollTop = 0;
  });
  // The question was answered and the release is in: the switch goes back off, as tC4's shot has it.
  await env.call('net/set', { request: newRequestId(), on: false });
  await app.page.locator('[data-testid="account-menu"][data-internet="off"]').waitFor();
  if (apply) {
    await app.page.getByTestId('upgrade-apply').click();
    await app.page.getByTestId('upgrade-confirm-error').waitFor();
  }
};

/** The change of checking language over Understand, from Project settings, the plan scripted. */
const gatewayScripted = (plan, after = async () => {}) => async (app, env) => {
  await app.page.unrouteAll({ behavior: 'ignoreErrors' });
  await app.home();
  await script(env, { type: 'plan', value: plan });
  await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
  await app.page.getByTestId('settings-gateway-en::unfoldingWord').click();
  await app.page.getByTestId('gateway-change').waitFor();
  await after(app, env);
};
/** The confirm held on the server: the dialog while the change is written (tC4 #580). */
const holdApply = async (app) => {
  await app.page.route('**/api/v1/resources/applyPlan', () => {});
  await app.page.getByTestId('gateway-confirm').click();
  await app.page.locator('[data-testid="gateway-confirm"][disabled]').waitFor();
};
/** The save refused: a disk with no room, the reason the dialog states under "The change was not applied". */
const failApply = async (app) => {
  await app.page.route('**/api/v1/resources/applyPlan', (route) => route.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({ type: 'err', value: { type: 'notSaved', value: { type: 'disk', value: 'There is no room left on the disk.' } } }),
  }));
  await app.page.getByTestId('gateway-confirm').click();
  await app.page.getByTestId('gateway-error').waitFor();
};
/** A plan that could not be made: the server's refusal, as tC4's preview fails with the browser's. */
const gatewayFails = async (app) => {
  await app.page.unrouteAll({ behavior: 'ignoreErrors' });
  await app.page.route('**/api/v1/resources/planGateway', (route) => route.fulfill({
    status: 200, contentType: 'application/json',
    body: JSON.stringify({ type: 'err', value: { type: 'refused', value: { type: 'resources', value: { type: 'gatewayFailed', value: { reason: FETCH_FAILED } } } } }),
  }));
  await app.home();
  await app.card(BIBLE).getByRole('button', { name: 'Settings', exact: true }).click();
  await app.page.getByTestId('settings-gateway-en::unfoldingWord').click();
  await app.page.getByTestId('gateway-error').waitFor();
};
const gatewayPlan = (rows, blocked = []) => ({
  plan: 91,
  what: {
    type: 'gateway',
    value: {
      from: { gateway: { language: 'es-419', owner: 'es-419_gl' }, name: 'Spanish (Latin American)', autonym: 'Español (Latinoamérica)', direction: 'ltr', seen: { tn: 'v66', tw: 'v37', ta: 'v4' } },
      to: { gateway: { language: 'en', owner: 'unfoldingWord' }, name: 'English', autonym: 'English', direction: 'ltr', seen: { tn: 'v89', tw: 'v89', ta: 'v89' } },
      images: null,
    },
  },
  decisionsAtRisk: rows.reduce((n, r) => n + r.decisions, 0), rows, blocked, alignments: null,
});
const openProgress = (phase, done = 0, total = null) => ({ phase: { type: 'open', value: phase }, done, total, item: null });

/** The next release name after `version` (v66 -> v67, v0.34 -> v0.35). */
const nextVersion = (version) => version.replace(/(\d+)$/, (n) => String(Number(n) + 1));

/** The slice of tC4's upgrade state at rest (state.jsx UPGRADE_IDLE), with `patch` over it. */
const upgradeSlice = (s, patch) => ({ ...s.upgrade, ...patch });

/** A release upgrade of the primary set as tC4's Source texts offers it (data/upgrade.ts
 * offerForSet over reposOfSet): each repo pinned in a slot of `moved` to its next release name,
 * every other repo current. The new commits are left out: no view shows them, and a made-up
 * commit would look real. */
function primaryOffer(s, kit, moved) {
  const upgrades = [];
  const current = [];
  for (const { repoPath, pin, slots } of kit.reposOfSet(s.projectPins.languageSets.primary)) {
    if (!slots.some((slot) => moved.includes(slot))) {
      current.push(repoPath);
      continue;
    }
    upgrades.push({ repoPath, slots, from: pin, to: { repoPath: pin.repoPath, version: nextVersion(pin.version), flavor: pin.flavor }, publishedAt: null });
  }
  return { rung: 'primary', upgrades, current };
}

/** Every repo the fallback set pins, once (data/upgrade.ts reposOfSet), all current. */
function fallbackCurrent(s, kit) {
  return { rung: 'fallback', upgrades: [], current: kit.reposOfSet(s.projectPins.languageSets.fallback).map((r) => r.repoPath) };
}

/** The Bible sample's decision files as tC4 reads them for a change of the pin file (state.jsx
 * storedGatewayDecisions): each book of the project, each tool of TOOL_SLOT, in that order, a
 * missing file left out. The baseline's mock serves the sample from these files. */
export function storedDecisions(s, kit) {
  if (s.project?.repoPath !== BIBLE) throw new Error(`the stored decision files are the Bible sample's, not ${s.project?.repoPath}'s`);
  const dir = path.join(REPO, 'fixtures', 'tc4', 'sample-burrito', 'ingredients', 'checking');
  const stored = [];
  for (const book of s.project.bookCodes) {
    for (const tool of Object.keys(kit.TOOL_SLOT)) {
      const file = path.join(dir, tool, `${book}.json`);
      if (fs.existsSync(file)) stored.push({ tool, book, file: JSON.parse(fs.readFileSync(file, 'utf8')) });
    }
  }
  return stored;
}

/** The confirmation of the set offer `offer` as tC4's applyOffer opens it (state.jsx:4548-4583,
 * planUpgradeAfterInstall and planResourcesChange, state.jsx:4425-4482). tC4's own rules give the
 * pins after the offer (applyUpgrade) and the consequences (consequencesOfGatewayChange over the
 * sample's stored decision files), so a row appears exactly for a check whose resource the offer
 * moves; the plan holds a row for each affected (tool, book) not in `blocked`, as
 * gatewayChangePlan skips a blocked one. Three things are taken, not drawn: each new release
 * covers the books of the one it replaces (the coverage backfillCoverage would read from the
 * installed copy); the machine holds every release a check resolves through after the offer,
 * except where `blocked` says otherwise; and a row's carry-over counts, which need the new
 * release's checks, are the recorded gateway change's for the same (tool, book). */
function setPreview(s, offer, { state, kit }, blocked = []) {
  const gateway = state('gateway').gatewayPreview;
  const covered = { ...offer, upgrades: offer.upgrades.map((u) => ({ ...u, to: { ...u.to, ...(u.from.books ? { books: u.from.books } : {}) } })) };
  const next = kit.applyUpgrade(s.projectPins, covered);
  const { primary } = next.languageSets;
  const consequences = kit.consequencesOfGatewayChange(storedDecisions(s, kit), { primary, fallback: next.languageSets.fallback ?? primary }, {}, 'bible');
  const plan = consequences.affected
    .filter((a) => !blocked.some((b) => b.tool === a.tool && b.book === a.book))
    .map((a) => {
      const row = gateway.plan.find((p) => p.tool === a.tool && p.book === a.book);
      if (!row) throw new Error(`the recorded gateway change has no carry-over row for ${a.tool} ${a.book}`);
      return row;
    });
  return {
    rung: offer.rung, offer, next, resourcesMd5: gateway.resourcesMd5, repoPath: s.project.repoPath,
    consequences, plan, blocked,
    carried: plan.reduce((n, p) => n + p.carried, 0),
    invalidated: plan.reduce((n, p) => n + p.invalidated, 0),
  };
}

/** The confirmation of an upgrade of the primary set's `moved` slots over Source texts, with
 * the offers behind it. */
function upgradeSetPatch(s, ctx, { moved = ['translationNotes', 'translationWordsLinks'], blocked = [] } = {}) {
  const offer = primaryOffer(s, ctx.kit, moved);
  return {
    upgrade: upgradeSlice(s, { offers: { primary: offer, fallback: fallbackCurrent(s, ctx.kit) }, textOffers: [], offersFor: s.project.repoPath, preview: setPreview(s, offer, ctx, blocked) }),
    installEpoch: s.installEpoch + 1,
  };
}

/** The verses of Titus an upgrade of the Greek text marks invalid: tC4's rule
 * (data/upgrade.ts invalidateAlignments) over the sample's alignment file. */
function titusAlignedVerses() {
  const file = JSON.parse(fs.readFileSync(path.join(REPO, 'fixtures', 'tc4', 'sample-burrito', 'ingredients', 'checking', 'alignments', 'TIT.json'), 'utf8'));
  let verses = 0;
  for (const records of Object.values(file.chapters)) {
    for (const record of Object.values(records)) {
      const empty = Array.isArray(record.alignments) && record.alignments.length === 0 && Array.isArray(record.wordBank) && record.wordBank.length === 0;
      if (record.invalid !== true && !empty) verses += 1;
    }
  }
  return verses;
}

/** A check session tC4 settles with no items (state.jsx emptyCheckSession, then
 * assembleCheckSession adds the session's seq and whether its frame is not eng). */
function emptySession(s, empty, { crossFrame = false, dropped = null } = {}) {
  const cs = s.checkSession;
  return { checkSession: { loading: false, tool: cs.tool, book: cs.book, items: [], empty, resource: cs.resource, ...(dropped ? { dropped } : {}), seq: cs.seq, crossFrame } };
}

/** The open overlay as performProjectOpen reports it (state.jsx:2156): `startedAt` 0 makes
 * the view show it at once, past its 300 ms threshold, whatever the clock says. */
const opening = (stage, done = 0, total = 0) => ({ opening: { repoPath: BIBLE, stage, done, total, startedAt: 0 } });

/** The share dialog's run (state.jsx shareRun): progress with the steps reported so far. */
const shareProgress = (steps) => (s) => ({ sh: { ...s.sh, step: 'progress', busy: true, steps, error: null } });

/** The feedback dialog after a send that ended with `result` (state.jsx sendFeedback). */
const feedbackResult = (result) => (s) => ({ fb: { ...s.fb, sending: false, result } });

const ALIGN_UNAVAILABLE = ['undrafted', 'unpinned', 'missing', 'versification-unavailable', 'no-counterpart', 'versification-unknown', 'unreadable'];

export const CASES = [
  // ---- S4: the verse conflict review --------------------------------------------------------
  { name: 'conflict-review', group: 'S4', state: 'review',
    why: 'The review of Titus 1:1 after two teammates changed it from one version on Door43, no version selected.' },
  { name: 'conflict-review-selected', group: 'S4', state: 'review',
    steps: [{ do: 'click', target: '[data-testid="conflict-version"]' }],
    why: 'The same review with the first version selected (view-local in ConflictReview.jsx).' },
  { name: 'conflict-review-mine', group: 'S4', state: 'review-mine',
    why: 'The review with three versions: two teammates\' and one I saved before the sync.' },

  // ---- S12: OBS Community Checking --------------------------------------------------------
  { name: 'cc-obs', group: 'S12', state: 'cc-obs',
    why: 'OBS Community Checking of the sample story project, drawn from the recorded reply of readPrintStories.' },
  { name: 'cc-obs-wrapped', group: 'S12', state: 'cc-obs',
    steps: [{ do: 'click', target: '[aria-labelledby="cc-layout-label"] button >> nth=1' }],
    why: 'The same with "Pictures wrapped" chosen (the page setup is view-local, CommunityChecking.jsx).' },

  // ---- S14: the open overlay ---------------------------------------------------------------
  { name: 'open-progress-journal', group: 'S14', state: 'home', patch: () => opening('journal', 120, 480),
    // The Rust app: the open held, the job reading its changes, 120 of 480 files (B3).
    rust: heldOpen(openProgress('readingChanges', 120, 480)), held: true,
    why: 'A slow open of the Bible sample reading its journal, 120 of 480 saved actions (a determinate bar). The mock opens the sample faster than the 300 ms threshold, so it never shows.' },
  { name: 'open-progress-state', group: 'S14', state: 'home', patch: () => opening('state'),
    // The Rust app: the open held, the job finishing earlier work, tC4's state stage (Q-7).
    rust: heldOpen(openProgress('finishingWork')), held: true,
    why: 'The open checking the project state (indeterminate), as the store reports it after the journal (journalingStore.ts:1364).' },
  { name: 'open-progress-prepare', group: 'S14', state: 'home', patch: () => opening('prepare'),
    // The Rust app: the open held, the job writing the project's files, tC4's preparing (Q-7).
    rust: heldOpen(openProgress('writingFiles')), held: true,
    why: 'The open preparing the project (indeterminate), its last stage (state.jsx:2185).' },

  // ---- M9: the release upgrade confirmation -------------------------------------------------
  { name: 'upgrade-set', group: 'M9', state: 'sources', patch: (s, ctx) => upgradeSetPatch(s, ctx),
    rust: upgradeConfirmation({ updates: offers([tnMove, twlMove]), plan: setPlan([tnMove, twlMove], [wordsRow(carried(2, 0)), notesRow(carried(0, 1))]) }),
    why: 'The confirmation of an upgrade of the primary set (es-419 tN and TWL to their next release) over Source texts, with the offers behind it: a row for each Titus check whose resource moves, by tC4\'s own rule over the sample\'s decision files. The mock has no newer release, so the carry-over counts of the rows are borrowed from the recorded gateway change.' },
  { name: 'upgrade-set-harmless', group: 'M9', state: 'sources', patch: (s, ctx) => upgradeSetPatch(s, ctx, { moved: ['translationAcademy'] }),
    rust: upgradeConfirmation({ updates: offers([taMove]), plan: setPlan([taMove], []) }),
    why: 'An upgrade of the primary set that moves only es-419 tA, which no check resolves through, so tC4\'s rule finds no decision made against the release being left: the green headline, no rows.' },
  { name: 'upgrade-set-blocked', group: 'M9', state: 'sources',
    rust: upgradeConfirmation({ updates: offers([tnMove, twlMove]), plan: setPlan([tnMove, twlMove], [wordsRow(null), notesRow(carried(0, 1))], [{ tool: 'translationWords', unit, reason: 'noResource' }]) }),
    patch: (s, ctx) => upgradeSetPatch(s, ctx, { blocked: [{ tool: 'translationWords', book: 'TIT' }] }),
    why: 'The upgrade of es-419 tN and TWL when the es-419 tW articles that Titus\'s new word links derive against are not on this machine, so tC4 blocks Titus for Translation Words (gatewayChangePlan, state.jsx:1007-1016, #643): no carry-over row for it, the callout, Upgrade disabled (UpgradeSet.jsx BlockedCallout). Which releases are installed is not drawn, so the blocked entry is written here.' },
  { name: 'upgrade-set-error', group: 'M9', state: 'sources',
    rust: upgradeConfirmation({ updates: offers([tnMove, twlMove]), plan: setPlan([tnMove, twlMove], [wordsRow(carried(2, 0)), notesRow(carried(0, 1))]), apply: true }),
    patch: (s, ctx) => {
      const patch = upgradeSetPatch(s, ctx);
      return { ...patch, upgrade: { ...patch.upgrade, error: ctx.kit.t('upgrade.failed', { reason: ctx.kit.t('upgrade.stale') }) } };
    },
    why: 'The same upgrade after Upgrade failed because the plan no longer matches the project (upgrade.stale, as state.jsx:1480 and 4806 write it).' },
  { name: 'upgrade-text', group: 'M9', state: 'sources',
    rust: upgradeConfirmation({ updates: offers([], [{ kind: 'original', change: move('el-x-koine_ugnt', 'v0.34', 'v0.35') }], false), plan: { plan: 92, what: { type: 'text', value: { kind: 'original', change: move('el-x-koine_ugnt', 'v0.34', 'v0.35') } }, decisionsAtRisk: 0, rows: [], blocked: [], alignments: titusAlignedVerses() }, offer: 'upgrade-text-apply-el-x-koine_ugnt' }),
    patch: (s) => {
      const pin = s.projectPins.resources.originalLanguage.nt;
      const offer = { repoPath: pin.repoPath, kind: 'original', slots: [{ group: 'originalLanguage', testament: 'nt' }], fromPins: [pin], from: pin, to: { repoPath: pin.repoPath, version: nextVersion(pin.version), flavor: pin.flavor }, publishedAt: null };
      const invalidatedVerses = titusAlignedVerses();
      return {
        upgrade: upgradeSlice(s, { offers: null, textOffers: [offer], offersFor: s.project.repoPath, preview: { kind: 'text', offer, next: s.projectPins, repoPath: s.project.repoPath, alignments: [], invalidatedVerses } }),
        installEpoch: s.installEpoch + 1,
      };
    },
    why: 'The confirmation of an upgrade of the Greek New Testament text (UpgradeSet.jsx TextUpgrade), with the number of Titus verses whose alignments it marks invalid, counted by tC4\'s rule over the sample\'s alignment file.' },
  { name: 'upgrade-repin', group: 'M9b', state: 'guided-fix',
    patch: (s, ctx) => {
      const from = s.fix.pin;
      const offer = { rung: s.fix.rung, upgrades: [{ repoPath: from.repoPath, slots: ['translationWordsLinks'], from, to: { repoPath: from.repoPath, version: nextVersion(from.version), flavor: from.flavor }, publishedAt: null }], current: [], kind: 'repin' };
      // The confirmation replaces the fix screen (state.jsx fixRepin, closeModal).
      return { modal: null, np: null, ab: null, st: null, fix: null, im: null, sh: null, od: null, upgrade: upgradeSlice(s, { preview: setPreview(s, offer, ctx) }), installEpoch: s.installEpoch + 1 };
    },
    why: 'The guided fix\'s re-pin of the missing es-419 TWL to an installed copy of the same repository: the same confirmation, titled "Re-pin" (UpgradeSet.jsx, offer.kind repin), with the one row tC4\'s rule gives for a move of the TWL pin alone, Titus\'s Translation Words. The mock holds no other copy, so the installed release name is written here and the row\'s carry-over counts are borrowed from the recorded gateway change.' },

  // ---- M10: the gateway change -------------------------------------------------------------
  { name: 'gateway-change', group: 'M10', state: 'gateway',
    // The Rust app: Project settings of the Bible sample, the English package chosen, which opens the project and asks the change.
    rust: gatewayChange,
    why: 'The change of the checking language to English from Project settings, with tC4\'s carry-over counts for Titus.' },
  { name: 'gateway-change-busy', group: 'M10', state: 'gateway', patch: () => ({ gatewayBusy: true }),
    rust: gatewayScripted(gatewayPlan([wordsRow(carried(2, 0)), notesRow(carried(0, 1))]), holdApply),
    held: true, heldRequests: ['/api/v1/resources/applyPlan'],
    why: 'The same after Change: committed, both buttons disabled and no close (GatewayChange.jsx, #580). The mock commits too fast to photograph it.' },
  { name: 'gateway-change-blocked', group: 'M10', state: 'gateway',
    rust: gatewayScripted(gatewayPlan([wordsRow(carried(2, 0)), notesRow(carried(0, 1))], [{ tool: 'translationNotes', unit, reason: 'noResource' }])),
    patch: (s) => ({ gatewayPreview: { ...s.gatewayPreview, blocked: [{ tool: 'translationNotes', book: 'TIT' }] } }),
    why: 'The same when English would not cover Titus for Translation Notes: blocked, Change disabled.' },
  { name: 'gateway-change-error', group: 'M10', state: 'gateway',
    rust: gatewayScripted(gatewayPlan([wordsRow(carried(2, 0)), notesRow(carried(0, 1))]), failApply),
    patch: (s, { kit }) => ({ gatewayError: kit.t('gateway.otherProject') }),
    why: 'The same after the commit failed: "The change was not applied" with the refusal commitGatewayChange throws when another project is open now (state.jsx confirmGatewayChange).' },
  { name: 'gateway-change-failed', group: 'M10', state: 'gateway',
    rust: gatewayFails,
    patch: (s) => ({ gatewayBusy: false, gatewayPreview: { failed: true, gateway: s.gatewayPreview.gateway, projectId: s.gatewayPreview.projectId }, gatewayError: FETCH_FAILED }),
    why: 'The dialog of a preview that failed to read (askGatewayChange\'s catch): the error and Close, no counts and no Change.' },

  // ---- M18: the empty section question -----------------------------------------------------
  { name: 'section-empty-confirm', group: 'M18', state: 'section-editor',
    steps: [
      { do: 'click', target: '[data-testid="section-editor"] textarea' },
      { do: 'press', key: 'ControlOrMeta+A' },
      { do: 'press', key: 'Backspace' },
      { do: 'click', target: '[data-testid="section-editor"] button:has-text("Save section")' },
    ],
    why: '"Remove all the text of section 1–3?" after every word of the section was deleted and Save section pressed (SectionEditor.jsx #600).' },

  // ---- license, net-failed: dialogs over Home ----------------------------------------------
  { name: 'license', group: 'license', state: 'home', scope: LAST_DIALOG,
    steps: [
      { do: 'click', target: '[data-testid="account-menu"]' },
      { do: 'click', target: '[data-testid="account-about"]' },
      { do: 'click', target: '[data-testid="about-read-license"]' },
    ],
    why: 'The License dialog from About (AccountMenu.jsx LicenseDialog).' },
  { name: 'net-failed', group: 'net-failed', state: 'home', scope: LAST_DIALOG, patch: () => ({ netFailed: true }),
    // The Rust app: the Share question over Home, with `net/set` answering once that the
    // switch stayed off (B2 D6, D10): the step cannot run, as when turning it on fails.
    rust: async (app, env) => {
      await app.home();
      await app.page.route('**/api/v1/net/set', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify({ type: 'ok', value: { on: false } }) }), { times: 1 });
      await app.shareButton(BIBLE).click();
      await app.page.getByTestId('net-confirm').click();
      await app.page.getByTestId('net-failed').waitFor();
    },
    why: '"tC4 could not use the internet": turning the internet on failed (AccountMenu.jsx InternetDialog). The mock\'s gate never fails.' },

  // ---- share ---------------------------------------------------------------------------------
  { name: 'share-progress-preparing', group: 'share', state: 'share-check', patch: shareProgress([]), steps: [{ do: 'blur' }],
    why: 'Share on Door43 running, before its first step (state.jsx shareRun). The mock cannot create a repository or take a push.' },
  { name: 'share-progress-create', group: 'share', state: 'share-check', patch: shareProgress(['create']), steps: [{ do: 'blur' }],
    why: 'The share creating the repository on Door43.' },
  { name: 'share-progress-push', group: 'share', state: 'share-check', patch: shareProgress(['create', 'push']), steps: [{ do: 'blur' }],
    why: 'The share pushing the project.' },
  { name: 'share-done', group: 'share', state: 'share-check',
    patch: (s, { kit }) => {
      const repository = `${s.door43User}/${s.sh.name}`;
      const report = kit.okReport('share', AT, AT, { repository, url: `https://git.door43.org/${repository}`, created: true });
      return { sh: { ...s.sh, step: 'done', busy: false, steps: ['create', 'push'], error: null, report } };
    },
    steps: [{ do: 'blur' }],
    why: 'The share done: the repository\'s address on Door43, from tC4\'s ok Report of a first share to the signed-in account (shareOperation.ts).' },

  // ---- import --------------------------------------------------------------------------------
  { name: 'import-files', group: 'import', state: 'import-files',
    why: 'Import a project, USFM files, with 57-TIT.usfm chosen.' },
  { name: 'import-review-usfm', group: 'import', state: 'import-review-usfm',
    why: 'The review of the USFM import: Titus, the name from the file, no language yet.' },
  { name: 'import-review-tc3', group: 'import', state: 'import-review-tc3',
    why: 'The review of the tC3 project import, with its resource versions not looked up.' },
  { name: 'import-working', group: 'import', state: 'import-review-usfm',
    patch: (s) => ({ im: { ...s.im, busy: true, step: 'working', error: null, report: null } }), steps: [{ do: 'blur' }],
    why: 'The import creating the Bible (state.jsx importRun). The mock cannot create a project.' },
  { name: 'import-failed', group: 'import', state: 'import-review-usfm',
    patch: (s, { kit }) => {
      const language = s.projects.find((p) => p.id === BIBLE).languageTag;
      const repoPath = kit.importRepoPath(s.im.name, language);
      const refusal = new kit.Refusal('import.write-failed', 'the ops record of the import was not saved; the imported project was removed', { repoPath });
      const report = kit.failedReport('import', AT, AT, refusal, { parser: s.im.kind, repoPath, books: ['TIT'], rolledBack: true });
      return { im: { ...s.im, lang: language, busy: false, step: 'failed', report } };
    },
    steps: [{ do: 'blur' }],
    why: 'The import stopped while writing: tC4\'s Report when the ops record of a finished import was not saved and the project was rolled back (import/shell.ts runImport), for the file\'s Titus named in the sample\'s language.' },

  // ---- add-book ------------------------------------------------------------------------------
  { name: 'add-book-pick', group: 'add-book', state: 'add-book-pick',
    why: 'Add a book, "Start a blank book": the book picker.' },
  { name: 'add-book-pick-multi', group: 'add-book', state: 'add-book-pick-multi',
    why: 'Add a book, several at once, Philemon and Hebrews chosen.' },
  { name: 'add-book-usfm', group: 'add-book', state: 'add-book-usfm',
    why: 'Add books from USFM files, before a file is chosen.' },
  { name: 'add-book-usfm-files', group: 'add-book', state: 'add-book-usfm-files',
    why: 'Add books from USFM files: 57-TIT.usfm refused (Titus is in the project), 58-PHM.txt valid.' },

  // ---- feedback ------------------------------------------------------------------------------
  { name: 'feedback-sending', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'],
    patch: (s) => ({ fb: { ...s.fb, sending: true, result: null } }),
    why: 'The bug report "Ask for help" made, while it is sent (state.jsx sendFeedback). The browser build has no sending bridge.' },
  { name: 'feedback-sent', group: 'feedback', state: 'feedback-ask',
    patch: (s) => ({ fb: { ...s.fb, sending: false, message: '', name: '', email: '', result: 'sent' } }),
    why: 'The report sent: tC4 clears what was typed and shows the thanks.' },
  { name: 'feedback-not-sent-offline', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'], patch: feedbackResult('offline'),
    why: 'Not sent: no internet.' },
  { name: 'feedback-not-sent-timeout', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'], patch: feedbackResult('timeout'),
    why: 'Not sent: the help desk did not answer in 30 seconds.' },
  { name: 'feedback-not-sent-refused', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'], patch: feedbackResult('refused'),
    why: 'Not sent: the help desk refused the report.' },
  { name: 'feedback-not-sent-not-configured', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'], patch: feedbackResult('not-configured'),
    why: 'Not sent: sending is not available in this build.' },
  { name: 'feedback-not-sent-no-desktop', group: 'feedback', state: 'feedback-ask', masks: ['feedback-attachment'], patch: feedbackResult('no-desktop'),
    why: 'Not sent: the browser build, which has no desktop bridge (what tC4 itself answers here).' },

  // ---- sign-in -------------------------------------------------------------------------------
  // B9b: the Rust side takes the state's own route, Share and then the sign-in step, against
  // the fake Door43.
  { name: 'signin-error', group: 'sign-in', state: 'signin-error', scope: LAST_DIALOG, masks: ['signin-server'],
    why: 'The sign-in step after Door43 refused a wrong password.' },

  // ---- check-empty ---------------------------------------------------------------------------
  { name: 'check-empty-none', group: 'check-empty', state: 'check-tn', patch: (s) => emptySession(s, 'none'),
    why: 'The Translation Notes session of a book its pinned resource lists no checks for (state.jsx emptyCheckSession).' },
  { name: 'check-empty-all-dropped', group: 'check-empty', state: 'check-tn',
    patch: (s) => emptySession(s, 'all-dropped', { crossFrame: true, dropped: { count: s.checkSession.items.length, scheme: 'vul', reasons: ['no-chapter'] } }),
    why: 'The session when none of the 112 checks has a verse in the project\'s numbering: a project in another versification (vul here) than the resource\'s (deriveCheckItems). The sample is eng, so the scheme is written here.' },
  { name: 'check-empty-missing', group: 'check-empty', state: 'check-tn', patch: (s) => emptySession(s, 'missing'),
    why: 'The session when the pinned resource has no file for the book.' },
  { name: 'check-empty-versification-unavailable', group: 'check-empty', state: 'check-tn', patch: (s) => emptySession(s, 'versification-unavailable', { crossFrame: true }),
    why: 'The session when the project\'s numbering data could not be loaded.' },
  { name: 'check-empty-versification-unknown', group: 'check-empty', state: 'check-tn', patch: (s) => emptySession(s, 'versification-unknown', { crossFrame: true }),
    why: 'The session when the project\'s versification is not known.' },
  { name: 'check-deriving', group: 'check-empty', state: 'check-tn', patch: (s) => ({ checkSession: { loading: true, seq: s.checkSession.seq } }),
    why: '"Deriving the check list…" while the session opens (state.jsx openCheckTool).' },
  { name: 'check-error', group: 'check-empty', state: 'check-tn', patch: (s) => ({ checkSession: { loading: false, error: FETCH_FAILED, seq: s.checkSession.seq } }),
    why: 'The session after its derivation failed to read (openCheckTool\'s catch).' },
  { name: 'check-rail-empty', group: 'check-empty', state: 'check-tn',
    steps: [{ do: 'click', target: '[data-testid^="filter-"][data-count="0"]' }],
    why: 'A rail filter that matches nothing: "Nothing matches this filter." (Check.jsx).' },
  { name: 'check-picker-no-items', group: 'check-empty', state: 'check-picker',
    patch: (s) => ({ pickerProgress: { ...s.pickerProgress, translationNotes: { done: 0, total: 0, dropped: null, nextItem: null, empty: 'none' } } }),
    steps: [{ do: 'refocus', target: '[role="tab"][aria-selected="true"]' }],
    why: 'The tool picker with a Translation Notes card for a book with no checks: "No items in this book yet" (state.jsx loadPickerProgress).' },

  // ---- align-unavailable ---------------------------------------------------------------------
  ...ALIGN_UNAVAILABLE.map((kind) => ({
    name: `align-unavailable-${kind}`, group: 'align-unavailable', state: 'align',
    patch: (s) => ({ alignSession: { unavailable: kind, seq: s.alignSession.seq } }),
    why: `Align when the verse cannot be aligned: ${kind}, exactly what openAlign settles (state.jsx:5096-5141).`,
  })),
  { name: 'align-loading', group: 'align-unavailable', state: 'align', patch: (s) => ({ alignSession: { loading: true, seq: s.alignSession.seq } }),
    why: 'Align while the verse loads.' },
  { name: 'align-error', group: 'align-unavailable', state: 'align', patch: (s) => ({ alignSession: { error: FETCH_FAILED, seq: s.alignSession.seq } }),
    why: 'Align after the verse failed to read: the error and Retry (openAlign\'s catch).' },

  // ---- home ----------------------------------------------------------------------------------
  // home-loading photographs a read in flight, which B3 can capture (S14 needs it too); the
  // open's and the checkpoint's errors need the open (B3) and the leave checkpoint (D1) (B2 D10).
  { name: 'home-loading', group: 'home-held', state: 'home', patch: () => ({ projects: null }),
    // The Rust app: Home with its home/list held unanswered for the shot (B3's capture of work
    // in flight, `held`).
    rust: async (app, env) => {
      await app.page.unrouteAll({ behavior: 'ignoreErrors' });
      await app.page.route('**/api/v1/home/list', () => {});
      await env.load(app.page, { locale: 'en' });
      await app.page.getByTestId('account-menu').waitFor();
    },
    held: true, heldRequests: ['/api/v1/home/list'],
    why: 'Home before the project list is read.' },
  { name: 'home-empty', group: 'home', state: 'home', patch: () => ({ projects: [] }),
    // The Rust app: home/list answered once with no project, a hand reply (replies/home-empty.json).
    rust: async (app, env) => {
      await app.page.route('**/api/v1/home/list', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(homeReply('home-empty')) }), { times: 1 });
      await env.load(app.page, { locale: 'en' });
      await app.page.getByText(env.catalog('en')['home.noProjects']).waitFor();
    },
    why: 'Home with no project.' },
  { name: 'home-list-error', group: 'home', state: 'home', patch: () => ({ projects: null, bookError: `TypeError: ${FETCH_FAILED}` }),
    // The Rust app: home/list answered once with an error, a hand reply (replies/home-list-error.json):
    // a 200 with an error, since a failed load would log a console error, which fails a capture.
    rust: async (app, env) => {
      await app.page.route('**/api/v1/home/list', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(homeReply('home-list-error')) }), { times: 1 });
      await env.load(app.page, { locale: 'en' });
      await app.page.getByTestId('projects-retry').waitFor();
    },
    why: 'Home when the project list failed to read: the banner with Retry (state.jsx refreshProjects\' catch writes String(e)).' },
  { name: 'home-open-error', group: 'home-open', state: 'home-open-error',
    // The Rust app: session/open refused once, a hand reply (replies/home-open-error.json): this
    // app reads a file written outside it as an outside edit and opens (STORAGE §11.1), so the
    // refusal is one it does give, a project file it cannot read.
    rust: async (app, env) => {
      await app.page.unrouteAll({ behavior: 'ignoreErrors' });
      await app.home();
      const { reply } = readReplies('home-open-error').replies[0];
      await app.page.route('**/api/v1/session/open', (r) => r.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(reply) }), { times: 1 });
      await app.card(BIBLE).getByRole('button', { name: /Titus/ }).click();
      await app.page.getByTestId('home-open-error').waitFor();
      await app.page.getByTestId('book-tile-TIT').focus();
    },
    why: 'Home after an open refused a file written outside the app.' },
  { name: 'home-checkpoint-error', group: 'home-checkpoint', state: 'home',
    patch: (s, { kit }) => ({ commitError: `${kit.t('app.commitError')}: ${FETCH_FAILED}`, commitErrorHelp: null, commitErrorRepo: BIBLE }),
    why: 'Home after the checkpoint of the project just left failed (state.jsx startLeaveCheckpoint).' },

  // ---- resume, import-toast ------------------------------------------------------------------
  { name: 'home-resume', group: 'resume', state: 'resume',
    // The Rust app: the Resume record tC4 recorded, through test/lastEdit (replies/home-resume.json),
    // before any save exists (B2 D12).
    rust: async (app, env) => {
      await app.home();
      const { request } = readReplies('home-resume').replies[0];
      await env.call('test/lastEdit', { ...request, request: newRequestId() });
      await app.page.getByTestId('resume-card').waitFor();
    },
    why: 'Home with the Resume card after a section of Titus 1 was saved.' },
  { name: 'import-toast', group: 'import-toast', state: 'home',
    patch: (s, { kit }) => {
      const project = s.projects.find((p) => p.id === BIBLE);
      return { importedRepo: BIBLE, importToast: { name: project.name, books: project.bookCodes.map((code) => kit.bookName(code)).join(' · ') } };
    },
    why: 'Home after an import: the toast and the Imported badge (state.jsx importRun\'s last patch), for the Bible sample as if it had just been imported. The mock cannot create a project.' },
];

/** The steps the gallery takes for case `c`: a case with no patch is its state as the route
 * left it, so the state's focus steps come first. */
export const caseSteps = (c) => [...(c.patch ? [] : stateNamed(c.state).focus ?? []), ...(c.steps ?? [])];

/** What the gallery mounts for case `c` over its recorded state `state` ({ s, returns }):
 * the patched state, the replies and the locale; `patchKeys` lists what the patch sets.
 * `readS(name)` reads another recorded state's `s`; `kit` is patch-kit.mjs's. */
export function caseData(c, state, readS, kit) {
  const patch = c.patch ? c.patch(state.s, { state: readS, kit }) : {};
  return {
    s: applyPatch(state.s, patch),
    returns: { ...state.returns, ...(c.returns ?? {}) },
    locale: stateNamed(c.state).locale,
    patchKeys: Object.keys(patch).sort(),
  };
}

