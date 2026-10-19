// The panic recovery's pure parts (APP-DESIGN 6.6, 6.8, AD30): what boot.js decides and builds,
// over objects it is handed, so `recovery.test.mjs` runs them in node. Nothing here touches a
// global when it is imported.
//
// The keys are tc-ui's (`crates/tc-ui/src/app/recovery.rs`, `boot.rs`, tc-api's `buffer.rs`);
// `crates/tc-ui/tests/app/recovery.rs` checks each `export const` line below against them.

export const CRASH_KEY = 'tc4.crash.v1';
export const PANICS_KEY = 'tc4.panics.v1';
export const TEXT_KEY = 'tc4.viewErrorText.v1';
export const NAV_KEY = 'tc4.nav.v1';
export const INSTANCE_KEY = 'tc4.instance.v1';
export const TOKEN_KEY = 'tc4.token';
export const BUFFER_PREFIX = 'tc4.drafts.v2:';
export const PANIC_WINDOW_MS = 10000;
export const PANICS_TO_STOP = 3;
export const API_VERSION = 1;

/** The English words of the callout, for a page that never kept its language's. */
export const ENGLISH = { lang: 'en', dir: 'ltr', title: 'This screen could not be shown.', home: 'Back to Home' };

/** A draft endpoint: `service/name`, so a damaged entry cannot post anywhere else. */
const DRAFT_PATH = /^[a-z][a-z0-9]*\/[A-Za-z][A-Za-z0-9]*$/;

/** Whether `value` is a WebAssembly trap: what every call into an aborted instance throws. */
export function isTrap(value) {
  if (typeof WebAssembly !== 'undefined' && WebAssembly.RuntimeError && value instanceof WebAssembly.RuntimeError) {
    return true;
  }
  // Another realm's RuntimeError is not an instance of this one's.
  return value != null && typeof value === 'object' && value.name === 'RuntimeError' && typeof value.message === 'string';
}

/**
 * What to do about a page load that ended in a panic. `keptText` is what PANICS_KEY holds: the
 * times of the earlier loads that ended in one. Those older than 10 s are dropped; three within
 * 10 s, this one included, stop the reloads.
 */
export function panicDecision(keptText, now) {
  let kept = [];
  try {
    const parsed = JSON.parse(keptText ?? '[]');
    if (Array.isArray(parsed)) kept = parsed.filter((t) => typeof t === 'number' && t > now - PANIC_WINDOW_MS && t <= now);
  } catch {
    kept = [];
  }
  const panics = [...kept, now];
  return { panics, action: panics.length >= PANICS_TO_STOP ? 'stop' : 'reload' };
}

/**
 * Counts a page load that ended in a panic in `sessionStore` and says what to do: 'reload', or
 * 'stop' after three within 10 s, and also when the store cannot keep the count, since a page
 * that cannot count would reload for ever.
 */
export function countPanic(sessionStore, now) {
  let kept = null;
  try {
    kept = sessionStore.getItem(PANICS_KEY);
  } catch {
    kept = null;
  }
  const decision = panicDecision(kept, now);
  const text = JSON.stringify(decision.panics);
  try {
    sessionStore.setItem(PANICS_KEY, text);
    if (sessionStore.getItem(PANICS_KEY) !== text) return 'stop';
  } catch {
    return 'stop';
  }
  return decision.action;
}

/** The mirror's line for a field's entry, byte for byte tc-ui's `buffer::changed_line`. */
export function changedLine(keyText, entryText) {
  return `{"type":"changed","value":{"key":${keyText},"entry":${entryText}}}`;
}

/**
 * A keystroke into a tracked editor, before the WebAssembly sees it. The template is the field's
 * entry as this page's save machine holds it (its run, its tokens and its save in flight): the item
 * becomes the template with the new text and time, keeping every key of the stored entry this
 * build does not know, so a keystroke in one tab never leaves another tab's save or tokens under
 * its own text. An entry of another server run, or one that does not parse, is never written over;
 * the keystroke still gets its mirror's line, as this page's entry. Returns the mirror's line,
 * which goes to the shell even when the storage refused the write, and whether the item now holds
 * the text: only then may the WebAssembly leave the keystroke to this listener.
 */
export function inputToEntry(store, itemKey, text, now, templateText) {
  const none = { line: null, stored: false };
  let template;
  try {
    template = JSON.parse(templateText);
  } catch {
    return none;
  }
  if (!template || typeof template !== 'object' || Array.isArray(template)) return none;
  const keyText = itemKey.slice(BUFFER_PREFIX.length);
  let stored = null;
  try {
    stored = store.getItem(itemKey);
  } catch {
    stored = null;
  }
  let entry = {};
  if (stored != null) {
    try {
      entry = JSON.parse(stored);
    } catch {
      entry = null;
    }
    if (!entry || typeof entry !== 'object' || Array.isArray(entry) || entry.instance !== template.instance) {
      return { line: changedLine(keyText, JSON.stringify({ ...template, text, at: now })), stored: false };
    }
  }
  const entryText = JSON.stringify(Object.assign(entry, template, { text, at: now }));
  let wrote = false;
  try {
    store.setItem(itemKey, entryText);
    wrote = true;
  } catch {
    // The mirror's line still goes: the shell then holds the keystroke.
  }
  return { line: changedLine(keyText, entryText), stored: wrote };
}

/**
 * The drafts to post after three panics (6.8): one per buffer entry of the server run
 * `instance` (the run the page last met, which the page keeps in INSTANCE_KEY) that has a draft
 * call, its body with a fresh `request` and the entry's `text` added. Each draft names its own
 * open, so one of a project that is not open is refused by the server and changes nothing;
 * entries of another run wait for the offer at their project's next open.
 */
export function draftRequests(store, instance, freshId) {
  if (typeof instance !== 'string' || instance === '') return [];
  const requests = [];
  let length = 0;
  try {
    length = store.length;
  } catch {
    return [];
  }
  for (let i = 0; i < length; i += 1) {
    let item;
    let entry;
    let key;
    try {
      item = store.key(i);
      if (typeof item !== 'string' || !item.startsWith(BUFFER_PREFIX)) continue;
      key = JSON.parse(item.slice(BUFFER_PREFIX.length));
      entry = JSON.parse(store.getItem(item));
    } catch {
      continue;
    }
    if (!key || !entry || entry.instance !== instance || typeof entry.text !== 'string') continue;
    const draft = entry.draft;
    if (!draft || typeof draft.path !== 'string' || !DRAFT_PATH.test(draft.path) || typeof draft.body !== 'string') continue;
    let body;
    try {
      body = JSON.parse(draft.body);
    } catch {
      continue;
    }
    if (!body || typeof body !== 'object' || Array.isArray(body)) continue;
    body.request = freshId();
    body.text = entry.text;
    requests.push({ path: draft.path, body: JSON.stringify(body) });
  }
  return requests;
}

/** The callout's words the page kept in the shown language, or English. */
export function calloutText(sessionStore) {
  try {
    const kept = JSON.parse(sessionStore.getItem(TEXT_KEY));
    if (kept && typeof kept.title === 'string' && typeof kept.home === 'string' && typeof kept.lang === 'string') {
      return { lang: kept.lang, dir: kept.dir === 'rtl' ? 'rtl' : 'ltr', title: kept.title, home: kept.home };
    }
  } catch {
    // English below.
  }
  return { ...ENGLISH };
}

/** 32 hex digits from the platform's random source: a request id or a page id. */
export function randomId(crypto) {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
}

function element(doc, tag, attributes, children) {
  const node = doc.createElement(tag);
  for (const [name, value] of attributes) node.setAttribute(name, value);
  for (const child of children) node.appendChild(typeof child === 'string' ? doc.createTextNode(child) : child);
  return node;
}

// The elements and attributes, in order, that tc-ui's `ViewError` renders (`web/view_error.rs`:
// the design system's Callout with a warn tone and its outline Button). `app/no-loss.mjs`, case P3,
// compares the two callouts' outerHTML, so a design-system change that moves one fails there.
const CALLOUT_STYLE =
  'box-sizing: border-box; background: var(--tone-soft); color: var(--tone-on-soft); border: var(--stroke) solid var(--tone-border); border-radius: var(--radius-lg); box-shadow: none; padding: 12px; overflow-wrap: anywhere;';
const TEXT_STYLE =
  'font-family: var(--font-ui); margin: 0; font-size: var(--fs-caption-lg); font-weight: var(--fw-regular); line-height: var(--lh-ui); letter-spacing: var(--track-12-5); color: var(--tone-text); display: contents;';
const BUTTON_STYLE =
  'box-sizing: border-box; font-family: var(--font-ui); font-weight: var(--fw-heavy); letter-spacing: var(--track-12-5); display: inline-flex; align-items: center; gap: var(--control-gap); justify-content: center; text-align: center; text-decoration: none; cursor: pointer; flex: none; line-height: 1.2; white-space: nowrap; font-size: var(--control-fs-sm); background: var(--fill-plain); color: var(--tone-text); border: var(--stroke-selected) solid var(--tone-border); border-radius: var(--radius-pill); height: var(--control-h-sm); padding-inline: var(--action-px-sm); padding-block: 0;';

/**
 * tC4's view-error callout (`tC4:src/App.jsx:177-190`) as static DOM, the same elements and
 * attributes as tc-ui's `ViewError`, with every text set as a text node: a panic message can hold
 * a project name or typed text. Returns the callout's root and its "Back to Home" button.
 */
export function buildStaticCallout(doc, text, message) {
  const button = element(
    doc,
    'button',
    [
      ['type', 'button'],
      ['data-tone', 'ocean'],
      ['data-i', 'outline'],
      ['data-trim', 'cap'],
      ['style', BUTTON_STYLE],
      ['data-testid', 'view-error-home'],
    ],
    [text.home],
  );
  const strong = element(doc, 'strong', [], [text.title]);
  const inner = element(doc, 'div', [['style', TEXT_STYLE]], [strong, ' ', String(message ?? ''), ' ', button]);
  const callout = element(
    doc,
    'div',
    [
      ['data-tone', 'warn'],
      ['style', CALLOUT_STYLE],
      ['role', 'alert'],
      ['data-testid', 'view-error'],
    ],
    [inner],
  );
  return { root: element(doc, 'div', [['style', 'flex:1;padding:24px']], [callout]), button };
}
