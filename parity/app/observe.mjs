// How a screen is photographed and read, the same way for every app the driver runs: the
// browser and its context, the wait for a quiet app, the screenshot, what the DOM shows at the
// shot (`observe`, for the manifest) and the DOM itself (`domSnapshot`, rooted at #root), the
// machine that took it (`machineFingerprint`) and where an element lies (`measureTargets`).
// No npm import: the entry that runs these brings Playwright.
import { execFileSync } from 'node:child_process';
import crypto from 'node:crypto';

/** Rendering flags that keep repeated runs pixel-identical: without them a few rounded
 * edges differ by up to 8/255 between runs (raster tiling). */
export const CHROMIUM_ARGS = ['--force-color-profile=srgb', '--disable-partial-raster', '--disable-gpu-rasterization', '--disable-lcd-text', '--font-render-hinting=none'];
export const VIEWPORT = { width: 1280, height: 800 };
/** The browser context of every capture, before the environment adds its routes. */
export const CONTEXT_OPTIONS = { viewport: VIEWPORT, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'UTC', colorScheme: 'light' };
/** An empty stretch of the title bar (and the scrim under a dialog): the pointer rests here
 * for every screenshot, so no hover state leaks into an image. */
export const POINTER_REST = { x: 1000, y: 28 };

/** The tracked requests in flight, by request id, and `lastEnded`: the time the last of them
 * finished, failed or was forgotten. */
class InFlight extends Map {
  lastEnded = -Infinity;

  delete(requestId) {
    const ended = super.delete(requestId);
    if (ended) this.lastEnded = Date.now();
    return ended;
  }
}

/**
 * Every request in flight from the page's documents that `tracks(url)` counts, by request id,
 * with its method, URL and the time it started, so a screenshot waits until the app has
 * stopped reading and writing.
 *
 * A document's requests are cut off when its frame commits another document, and Chromium
 * does not always report their end: a story-title read on Home, cut off by the next screen's
 * reload, stayed "in flight" for good in Playwright's request events, and every later screen
 * of the scenario waited out the settle deadline. Its request event can even arrive after
 * the commit. So the tracker reads the DevTools protocol, where each request names the
 * document that sent it (`loaderId`): a frame's commit forgets the requests of its earlier
 * documents, a request of a document that is no longer its frame's is never counted, and a
 * frame that is removed takes its requests with it.
 */
export async function trackRequests(page, tracks) {
  const cdp = await page.context().newCDPSession(page);
  const documentOf = new Map();
  const inFlight = new InFlight();
  cdp.on('Page.frameNavigated', ({ frame }) => {
    documentOf.set(frame.id, frame.loaderId);
    for (const [id, r] of inFlight) if (r.frameId === frame.id && r.loaderId !== frame.loaderId) inFlight.delete(id);
  });
  cdp.on('Page.frameDetached', ({ frameId }) => {
    documentOf.delete(frameId);
    for (const [id, r] of inFlight) if (r.frameId === frameId) inFlight.delete(id);
  });
  cdp.on('Network.requestWillBeSent', (e) => {
    if (e.type === 'Document' || !tracks(e.request.url) || inFlight.has(e.requestId)) return;
    if (documentOf.has(e.frameId) && documentOf.get(e.frameId) !== e.loaderId) return;
    inFlight.set(e.requestId, { method: e.request.method, url: e.request.url, frameId: e.frameId, loaderId: e.loaderId, at: Date.now() });
  });
  const done = ({ requestId }) => inFlight.delete(requestId);
  cdp.on('Network.loadingFinished', done);
  cdp.on('Network.loadingFailed', done);
  await cdp.send('Page.enable');
  await cdp.send('Network.enable');
  return inFlight;
}

/** Waits until no tracked request has been in flight for `quietMs`, then for the fonts and
 * two frames. A screen whose app is not quiet by `deadlineMs` fails: a shot taken then would
 * show a moment, not a state. `inFlight` is what `trackRequests` returns.
 *
 * The quiet counts from when the last request ended, as the tracker heard it, not from the
 * loop's last look that saw the request: that look can be a whole wait earlier, and more when
 * the event loop is busy.
 *
 * `busy`, when given, is asked once the requests are quiet: work the tracker cannot see (a job
 * on the server, a call the page has not sent yet). Once it answers true it is asked at every
 * look until it answers false, and the quiet counts again from that answer, so the shot comes
 * `quietMs` after the work stopped, and only when `busy` still answers false then. */
export async function settle(page, inFlight, { quietMs = 800, deadlineMs = 30_000, busy = null } = {}) {
  const deadline = Date.now() + deadlineMs;
  let quietSince = Date.now();
  let serverBusy = false;
  for (;;) {
    if (inFlight.size > 0) quietSince = Date.now();
    else if (serverBusy) {
      serverBusy = await busy();
      quietSince = Date.now();
    } else if (Date.now() - Math.max(quietSince, inFlight.lastEnded) >= quietMs) {
      if (!busy) break;
      serverBusy = await busy();
      if (!serverBusy) break;
      quietSince = Date.now();
    }
    if (Date.now() >= deadline) {
      const open = [...inFlight.values()].map((r) => `${r.method} ${r.url} (${Date.now() - r.at} ms)`);
      throw new Error(`the app did not go quiet for ${quietMs} ms within ${deadlineMs / 1000} s; in flight: ${open.join(', ') || 'nothing now'}${serverBusy ? '; the server answered busy (test/quiet)' : ''}`);
    }
    await page.waitForTimeout(50);
  }
  await page.evaluate(() => document.fonts.ready);
  await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

/** The words a failure callout's test id is reported with. */
const FAILURE_WORDS = { 'view-error': 'view error', 'boot-error': 'boot error' };

/**
 * Drives one screen and photographs it into `file`: the window size, the screen's steps, a
 * quiet app, the pointer at rest, a viewport screenshot with animations disabled. Then reads
 * what the DOM shows (`observe`) and the DOM itself (`domSnapshot`). A step that fails, or a
 * failure callout on screen (the first of `failureTestIds` that shows), is the screen's
 * `error`; the shot is still taken, so the failure can be seen. `busy` and `settleOptions` go
 * to `settle`.
 */
export async function captureScreen({ page, app, screen, inFlight, file, busy = null, failureTestIds = ['view-error'], settleOptions = {} }) {
  const size = screen.viewport ?? VIEWPORT;
  const now = page.viewportSize();
  if (now.width !== size.width || now.height !== size.height) await page.setViewportSize(size);
  let error = null;
  try {
    await screen.run(app);
    await settle(page, inFlight, { ...settleOptions, busy });
    for (const id of failureTestIds) {
      const callout = page.getByTestId(id);
      if (await callout.count()) {
        error = `${FAILURE_WORDS[id] ?? id}: ${await callout.innerText()}`;
        break;
      }
    }
  } catch (e) {
    error = e.message.split('\n')[0];
  }
  await page.mouse.move(POINTER_REST.x, POINTER_REST.y);
  await page.waitForTimeout(150);
  await page.screenshot({ path: file, animations: 'disabled' });
  const { homeCards, syncToast } = await observe(page);
  const dom = await page.evaluate(domSnapshot);
  return { error, homeCards, syncToast, dom };
}

/**
 * The machine a capture ran on, as far as it decides the pixels: the platform, the Chromium
 * version, and the fonts the system offers for fallback glyphs, as the count and sha256 of
 * `fc-list`'s lines (file, face index, font version), sorted bytewise, each ending in a
 * newline. Two captures compare by pixels only when their fingerprints are equal
 * (APP-DESIGN 7.3 layer 1). `fonts` is null where `fc-list` cannot run (Windows, macOS): such
 * a capture is never pixel-comparable. `run` is for the unit test.
 */
export function machineFingerprint(chromium, { run = defaultFontList } = {}) {
  let fonts = null;
  try {
    const lines = run().split('\n').filter((line) => line.length > 0).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
    fonts = { count: lines.length, sha256: crypto.createHash('sha256').update(lines.map((line) => `${line}\n`).join('')).digest('hex') };
  } catch {
    fonts = null;
  }
  return { platform: `${process.platform}-${process.arch}`, chromium, fonts };
}

function defaultFontList() {
  return execFileSync('fc-list', ['--format', '%{file}\t%{index}\t%{fontversion}\n'], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
}

/**
 * Where each target lies in the viewport: `targets` is `[{ id, target }]`, a target is
 * `{ testid }` (the first element with that `data-testid`) or `{ text }` (the first element,
 * in document order, whose first text run starts with the text). Answers
 * `{ [id]: { x, y, width, height } | null }` in whole pixels that cover the element, clipped to
 * the viewport; null when nothing matches or nothing of it is in the viewport. An element with
 * no box of its own (`display: contents`) is measured by what it holds.
 */
export function measureTargets(page, targets) {
  return page.evaluate((list) => {
    const firstRun = (el) => {
      let text = '';
      for (const c of el.childNodes) {
        if (c.nodeType === Node.TEXT_NODE) text += c.data;
        else if (c.nodeType === Node.ELEMENT_NODE) {
          if (text) return text;
          text = '';
        }
      }
      return text || null;
    };
    const find = (target) => {
      if (target.testid !== undefined) return document.querySelector(`[data-testid="${CSS.escape(target.testid)}"]`);
      const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
      for (let el = walker.nextNode(); el; el = walker.nextNode()) {
        if (firstRun(el)?.startsWith(target.text)) return el;
      }
      return null;
    };
    const out = {};
    for (const { id, target } of list) {
      const el = find(target);
      if (!el) {
        out[id] = null;
        continue;
      }
      let r = el.getBoundingClientRect();
      // An element with `display: contents` has no box; its children are laid out in its
      // parent's (tC4's Open from Door43 note is one).
      if (r.width === 0 || r.height === 0) {
        const range = document.createRange();
        range.selectNodeContents(el);
        r = range.getBoundingClientRect();
      }
      const left = Math.max(0, Math.floor(r.left));
      const top = Math.max(0, Math.floor(r.top));
      const right = Math.min(innerWidth, Math.ceil(r.right));
      const bottom = Math.min(innerHeight, Math.ceil(r.bottom));
      out[id] = right > left && bottom > top ? { x: left, y: top, width: right - left, height: bottom - top } : null;
    }
    return out;
  }, targets);
}

/**
 * What the DOM shows at the shot, for the checks in baseline.test.mjs:
 * - `homeCards`: for each project card whose share button the image shows, the button's
 *   action (tC4 Home.jsx `data-action`: share, sync or upload). A button under a dialog's
 *   translucent scrim shows; one under an opaque panel does not.
 * - `syncToast`: the parts of the sync result toast (tC4 SyncResult.jsx), when it shows.
 */
export function observe(page) {
  return page.evaluate(() => {
    const inWindow = (el) => {
      const r = el.getBoundingClientRect();
      return r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < innerHeight && r.left < innerWidth
        && getComputedStyle(el).visibility !== 'hidden';
    };
    const alpha = (color) => {
      const m = color.match(/rgba?\(([^)]*)\)/);
      if (!m) return color === 'transparent' ? 0 : 1;
      const parts = m[1].split(/[\s,/]+/).filter(Boolean);
      return parts.length > 3 ? Number(parts[3]) : 1;
    };
    /** Whether nothing opaque lies over the centre of `el`. */
    const shown = (el) => {
      if (!inWindow(el)) return false;
      const r = el.getBoundingClientRect();
      for (const over of document.elementsFromPoint(r.left + r.width / 2, r.top + r.height / 2)) {
        if (over === el || el.contains(over)) return true;
        if (over.contains(el)) continue;
        const style = getComputedStyle(over);
        if (Number(style.opacity) === 1 && (alpha(style.backgroundColor) === 1 || style.backgroundImage !== 'none')) return false;
      }
      return false;
    };
    const homeCards = {};
    for (const card of document.querySelectorAll('[data-testid^="share-card-"]')) {
      const repo = card.dataset.testid.slice('share-card-'.length);
      const button = card.querySelector(`[data-testid="share-${repo}"]`);
      if (button && shown(button)) homeCards[repo] = card.dataset.action;
    }
    const toast = document.querySelector('[data-testid="sync-result"]');
    if (!toast || !inWindow(toast)) return { homeCards, syncToast: null };
    const part = (id) => toast.querySelector(`[data-testid="${id}"]`);
    const count = (id) => Number(part(id)?.dataset.count ?? 0);
    return {
      homeCards,
      syncToast: {
        ok: toast.dataset.ok === '1',
        code: toast.dataset.code || null,
        text: toast.innerText.replace(/\s+/g, ' ').trim(),
        received: count('sync-count-received'),
        newestWins: count('sync-count-newest'),
        verseConflicts: count('sync-count-conflicts'),
        from: part('sync-received-from')?.innerText ?? null,
        restored: count('sync-restored'),
        missingPins: count('sync-missing-pins'),
        links: ['review', 'download'].filter((link) => part(`sync-${link}`)),
      },
    };
  });
}

/**
 * Runs in the page: the app's DOM at the shot, in the line format of
 * parity/ds-gallery/protocol.mjs `domSnapshot`, rooted at #root. `html` is the document
 * element with its attributes (`lang` and `dir` live there); `root` is #root's subtree, one
 * line per element or text run, with the sorted attributes, the inline style, input values and
 * checked states, and the focused element; `outside` lists the elements of <body> other than
 * #root and scripts, which an app that keeps its layers inside #root never has.
 *
 * An inline style is every longhand as the browser parsed it, sorted by name. Most of a
 * screen's bytes are styles that many elements share, so a line names its style by the first
 * 12 hex digits of the style's sha256 (`style=@…`) and `styles` holds each style once. Two
 * lines are equal exactly when their elements are, styles included.
 *
 * Values are recorded as they are: generated ids, hashed asset names and project test ids are
 * normalized by the comparison, not here.
 *
 * With `ids`, each element line ends with the number `tagNodes` gave the element (` #<n>`), or
 * ` #new` for an element drawn since, so a behaviour step shows a node drawn again even when it
 * looks the same. Without it the lines are exactly the baseline's.
 */
export async function domSnapshot({ ids = false } = {}) {
  const attrsOf = (n) => Array.from(n.attributes)
    .filter((a) => a.name !== 'style')
    .map((a) => `${a.name}=${JSON.stringify(a.value)}`)
    .sort();
  // The walk is synchronous, so the app cannot change the DOM halfway through it; each line
  // holds its style as text until the walk is done and the styles are keyed.
  const lines = [];
  const walk = (n, depth) => {
    const pad = '  '.repeat(depth);
    // Longhands sorted by name. React writes a shorthand's longhand in place where a parsed
    // style attribute appends it, which reorders cssText without changing any value.
    const decls = n.style ? Array.from(n.style, (p) => `${p}: ${n.style.getPropertyValue(p)}`).sort() : [];
    const props = [];
    if (n instanceof HTMLInputElement || n instanceof HTMLTextAreaElement || n instanceof HTMLSelectElement) {
      props.push(`.value=${JSON.stringify(n.value)}`);
      if (n.type === 'checkbox' || n.type === 'radio') props.push(`.checked=${n.checked}`, `.indeterminate=${n.indeterminate}`);
    }
    if (document.activeElement === n) props.push('.focused');
    if (ids) props.push(`#${n.__sid ?? 'new'}`);
    lines.push([`${pad}<${n.tagName.toLowerCase()} ${attrsOf(n).join(' ')}`, decls.join('; '), `${props.length ? ' ' + props.join(' ') : ''}>`]);
    // Adjacent text nodes render as one run; compare their joined text. Comments take no
    // part in layout.
    let text = '';
    const flush = () => {
      if (text) lines.push([`${pad}  "${text}"`]);
      text = '';
    };
    for (const c of n.childNodes) {
      if (c.nodeType === Node.TEXT_NODE) text += c.data;
      else if (c.nodeType === Node.ELEMENT_NODE) {
        flush();
        walk(c, depth + 1);
      }
    }
    flush();
  };
  const root = document.getElementById('root');
  walk(root, 0);
  const html = `<html ${attrsOf(document.documentElement).join(' ')}>`;
  const outside = Array.from(document.body.children)
    .filter((el) => el !== root && el.tagName !== 'SCRIPT')
    .map((el) => `<${el.tagName.toLowerCase()} ${attrsOf(el).join(' ')}>`);
  const keys = new Map();
  for (const [, style] of lines) {
    if (!style || keys.has(style)) continue;
    const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(style)));
    keys.set(style, Array.from(digest.slice(0, 6), (b) => b.toString(16).padStart(2, '0')).join(''));
  }
  const styles = [...keys].map(([style, key]) => [key, style]).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  if (new Set(styles.map(([key]) => key)).size !== styles.length) throw new Error('two styles share a key');
  return {
    html,
    outside,
    root: lines.map(([head, style, tail]) => (tail === undefined ? head : `${head}${style ? ` style=@${keys.get(style)}` : ''}${tail}`)),
    styles: Object.fromEntries(styles),
  };
}

/** Runs in the page: numbers, in document order, #root and the elements under it that have no
 * number yet (parity/ds-gallery/protocol.mjs `tagNodes`, rooted at #root). */
export function tagNodes() {
  let next = window.__nextSid ?? 0;
  const root = document.getElementById('root');
  for (const el of [root, ...root.querySelectorAll('*')]) {
    if (el.__sid === undefined) el.__sid = next++;
  }
  window.__nextSid = next;
}
