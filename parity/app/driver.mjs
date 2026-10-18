// The steps a person takes in the app, as each screen's `run` (screens.mjs) calls them. The
// driver clicks and waits in the page; everything that depends on which app runs behind the
// page comes from the environment: env-tc4.mjs for tC4 against its mock, and the Rust
// environment for `tc4 serve`.
import { FAKE_USER } from '../tc4-baseline/mock-server/fake-door43.mjs';
import { BIBLE, OBS } from './screens.mjs';

/**
 * What the driver needs from the app behind the page.
 *
 * @typedef {object} AppEnv
 * @property {(page: import('playwright').Page, options: { locale: string }) => Promise<void>} load
 *   Load Home afresh with `locale` saved as the app language and nothing else remembered: no
 *   place, no Resume card, no last-used order. The driver then waits for both project cards
 *   and for `<html lang>`.
 * @property {(locale: string) => Record<string, string>} catalog The app's message catalog of
 *   `locale`, for steps that wait on a text.
 * @property {(repo: string) => string} projectId The id that the app's test ids use for the
 *   project at tC4 repo path `repo` (`project-<id>`, `share-<id>`, `share-card-<id>`).
 * @property {(repo: string) => unknown} pushToDoor43 Door43 `main` of the shared project
 *   becomes the project as it is now: the push of its first share.
 * @property {(repo: string) => unknown} dropNextClone The next clone of the project's
 *   repository fails to connect, once.
 * @property {(repo: string) => Promise<string[]>} teammatesChangeOneVerse Two teammates change
 *   Titus 1:1 on Door43 `main` from the same version; answers each teammate's segment.
 * @property {(repo: string, segment: string) => unknown} loseFromDoor43 Door43 `main` loses
 *   one of those segments, as when someone replaces main with an older copy.
 */

/** Helpers each screen's `run` receives, over the environment `env` (see AppEnv). */
export function appDriver({ page, env }) {
  /** The catalog of the app language that home() set: the steps wait on its texts. */
  let c = env.catalog('en');
  const card = (repo) => page.getByTestId(`project-${env.projectId(repo)}`);
  const tab = async (name, ready) => {
    await page.getByRole('tab', { name, exact: true }).click();
    if (ready) await ready.waitFor();
  };
  return {
    page,
    /** Reload Home with nothing remembered, in `locale` (English by default). */
    async home({ locale = 'en' } = {}) {
      c = env.catalog(locale);
      await env.load(page, { locale });
      await card(BIBLE).waitFor();
      await card(OBS).waitFor();
      await page.waitForFunction((lang) => document.documentElement.lang === lang, locale);
    },
    card,
    /** The project card's Share, Sync or Upload button. */
    shareButton: (repo) => page.getByTestId(`share-${env.projectId(repo)}`),
    /** The part of the project card that carries the share button's action (`data-action`). */
    shareCard: (repo) => page.getByTestId(`share-card-${env.projectId(repo)}`),
    tab,
    async openTitus() {
      await card(BIBLE).getByRole('button', { name: /Titus/ }).click();
      await page.getByTestId('understand').waitFor();
      await page.getByText(c['understand.loading']).first().waitFor({ state: 'hidden' });
    },
    async openStory1() {
      await card(OBS).getByTestId('story-tile-1').click();
      await page.getByTestId('helps-panel').waitFor();
    },
    async openCheck() {
      await tab(c['nav.check']);
      await page.getByRole('heading', { name: c['check.title'] }).waitFor();
      await page.getByText(c['check.checking']).waitFor({ state: 'hidden' });
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
    /** Sign in as the fake Door43 user in the sign-in step that is open. */
    async signIn() {
      await page.getByTestId('share-signin').waitFor();
      await page.getByLabel('Door43 username or email').fill(FAKE_USER.username);
      await page.getByLabel('Password', { exact: true }).fill(FAKE_USER.password);
      await page.getByTestId('signin-submit').click();
    },
    pushToDoor43: (repo) => env.pushToDoor43(repo),
    dropNextClone: (repo) => env.dropNextClone(repo),
    teammatesChangeOneVerse: (repo) => env.teammatesChangeOneVerse(repo),
    loseFromDoor43: (repo, segment) => env.loseFromDoor43(repo, segment),
    /** Sync with Door43 from the project card: the internet question, then the sign-in. */
    async syncFromHome(repo) {
      await this.shareButton(repo).click();
      await page.getByTestId('net-confirm').click();
      await this.signIn();
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
