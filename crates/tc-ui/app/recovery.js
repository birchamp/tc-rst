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
export const TOKEN_KEY = 'tc4.token';
export const BUFFER_PREFIX = 'tc4.drafts.v2:';
export const PANIC_WINDOW_MS = 10000;
export const PANICS_TO_STOP = 3;

/** The English words of the callout, for a page that never kept its language's. */
export const ENGLISH = { lang: 'en', title: 'This screen could not be shown.', home: 'Back to Home' };

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

/** The mirror's line for a field's entry, byte for byte tc-ui's `buffer::changed_line`. */
export function changedLine(keyText, entryText) {
  return `{"type":"changed","value":{"key":${keyText},"entry":${entryText}}}`;
}

/**
 * A keystroke into a tracked editor, before the WebAssembly sees it: the field's entry gets the
 * new text and time, or, when it has none, the editor's template becomes its entry. Every other
 * key of the entry is kept. An entry of another server run, or one that does not parse, is left
 * alone. Returns the mirror's line, which goes to the shell even when the storage refused the
 * write, or null.
 */
export function inputToEntry(store, itemKey, text, now, templateText) {
  let entry;
  let template;
  try {
    template = JSON.parse(templateText);
  } catch {
    return null;
  }
  let stored = null;
  try {
    stored = store.getItem(itemKey);
  } catch {
    stored = null;
  }
  if (stored != null) {
    try {
      entry = JSON.parse(stored);
    } catch {
      return null;
    }
    if (!entry || typeof entry !== 'object' || entry.instance !== template.instance) return null;
  } else {
    entry = template;
  }
  entry.text = text;
  entry.at = now;
  const entryText = JSON.stringify(entry);
  try {
    store.setItem(itemKey, entryText);
  } catch {
    // The mirror's line still goes: the shell then holds the keystroke.
  }
  return changedLine(itemKey.slice(BUFFER_PREFIX.length), entryText);
}

/**
 * The drafts to post after three panics (6.8): one per buffer entry of `project` (the folder
 * the page was in) that has a draft call, its body with a fresh `request` and the entry's
 * `text` added.
 */
export function draftRequests(store, project, freshId) {
  if (typeof project !== 'string' || project === '') return [];
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
    if (!key || key.project !== project || !entry || typeof entry.text !== 'string') continue;
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
      return { lang: kept.lang, title: kept.title, home: kept.home };
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

/**
 * tC4's view-error callout (`tC4:src/App.jsx:177-190`) as static DOM, the same elements and
 * attributes as tc-ui's `ViewError` renders (the no-loss smoke compares the two), with every text
 * set as a text node: a panic message can hold a project name or typed text.
 */
export function buildStaticCallout(doc, text, message) {
  // VIEW_ERROR_DOM: generated from tc-ui's ViewError; see crates/tc-ui/app/no-loss.mjs, case P3.
  const button = element(
    doc,
    'button',
    [
      ['type', 'button'],
      ['data-testid', 'view-error-home'],
    ],
    [text.home],
  );
  const strong = element(doc, 'strong', [], [text.title]);
  const callout = element(
    doc,
    'div',
    [
      ['role', 'alert'],
      ['data-testid', 'view-error'],
    ],
    [strong, ' ', String(message ?? ''), ' ', button],
  );
  return { root: element(doc, 'div', [['style', 'flex:1;padding:24px']], [callout]), button };
}
