// The UI's entry (APP-DESIGN 5.2, 6.6, 6.8, AD30). The page's CSP refuses inline scripts, so the
// page loads this file. Before it loads the WebAssembly, it sets up what must outlive a panic in
// it, because a panic aborts the instance: every closure the WebAssembly registered traps on its
// next call, and a reload it scheduled may never run.
//
// - A capture listener writes each keystroke of a tracked editor into its field's buffer entry
//   (and the shell's mirror) before the WebAssembly sees the event, so a panic in the input
//   handler loses nothing. The WebAssembly tracks the focused editor with `tc4Boot.track(el,
//   itemKey, template)`; the template is the entry to write when the field has none.
// - The first trap of the page (a window `error` or an `unhandledrejection` whose error is a
//   WebAssembly.RuntimeError, or one `run_app` throws) is handled once; later traps of the same
//   page are ignored. A page load that ended in a panic is counted in sessionStorage; the page
//   reloads, and after three such loads within 10 s, or when sessionStorage cannot count them, it
//   draws tC4's view-error callout as static DOM instead and posts the buffered drafts with a
//   plain `fetch`.
//
// The WebAssembly is imported only after this, with a dynamic import, so the listeners exist even
// if the glue fails to load. It is started with the build folder it was served from, where the
// catalogs are, and this page load's id.
import * as R from './recovery.js';

const page = R.randomId(crypto);
const tracked = new WeakMap();
const boot = {
  page,
  track(element, itemKey, template) {
    tracked.set(element, { itemKey, template });
  },
  untrack(element) {
    tracked.delete(element);
  },
  // An array to collect the listener's own time per keystroke, for the measurements.
  measure: null,
};
window.tc4Boot = boot;

function onInput(event) {
  const started = performance.now();
  const field = tracked.get(event.target);
  if (field) {
    const target = event.target;
    const text = typeof target.value === 'string' ? target.value : (target.textContent ?? '');
    let line = null;
    try {
      line = R.inputToEntry(localStorage, field.itemKey, text, Date.now(), field.template);
    } catch {
      line = null;
    }
    if (line) {
      try {
        window.tc4Shell?.bufferChanged?.(line);
      } catch {
        // The shell's mirror is best effort here; the WebAssembly mirrors its own writes.
      }
    }
  }
  if (Array.isArray(boot.measure)) boot.measure.push(performance.now() - started);
}
// Capture, on the window: it runs before any listener of the page, delegated ones included.
window.addEventListener('input', onInput, true);

let handled = false;

function ownMarker() {
  try {
    const marker = JSON.parse(sessionStorage.getItem(R.CRASH_KEY));
    if (marker && marker.page === page) return marker;
  } catch {
    // No marker: an abort that skipped the panic hook still counts.
  }
  return null;
}

function onTrap(error) {
  if (handled) return;
  handled = true;
  const marker = ownMarker();
  const message = marker?.message ?? String(error?.message ?? error);
  if (R.countPanic(sessionStorage, Date.now()) === 'reload') {
    location.reload();
    return;
  }
  showStatic(message, marker?.project ?? null);
}

window.addEventListener('error', (event) => {
  if (R.isTrap(event.error)) onTrap(event.error);
});
window.addEventListener('unhandledrejection', (event) => {
  if (R.isTrap(event.reason)) onTrap(event.reason);
});

function showStatic(message, project) {
  const text = R.calloutText(sessionStorage);
  const { root, button } = R.buildStaticCallout(document, text, message);
  button.addEventListener('click', () => {
    for (const key of [R.CRASH_KEY, R.PANICS_KEY, R.NAV_KEY]) {
      try {
        sessionStorage.removeItem(key);
      } catch {
        // A blocked storage keeps nothing to remove.
      }
    }
    location.reload();
  });
  document.getElementById('root')?.replaceChildren(root);
  postDrafts(project);
}

function postDrafts(project) {
  let token = null;
  try {
    token = sessionStorage.getItem(R.TOKEN_KEY);
  } catch {
    token = null;
  }
  if (!token) return;
  const requests = R.draftRequests(localStorage, project, () => R.randomId(crypto));
  for (const { path, body } of requests) {
    fetch(`/api/v1/${path}`, {
      method: 'POST',
      headers: { 'X-Tc-Token': token, 'X-Tc-Api': String(R.API_VERSION), 'Content-Type': 'application/json' },
      body,
    }).catch(() => {});
  }
}

let wasm;
try {
  wasm = await import('./tc_ui.js');
  await wasm.default();
} catch (error) {
  handled = true;
  showStatic(String(error?.message ?? error), null);
  throw error;
}
try {
  wasm.run_app(new URL('./', import.meta.url).pathname, page);
} catch (error) {
  if (R.isTrap(error)) onTrap(error);
  else throw error;
}
