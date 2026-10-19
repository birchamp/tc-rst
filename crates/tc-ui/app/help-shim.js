// The help panel's page swap and link rules (APP-DESIGN 5.3, unit A5 D4): a JavaScript copy of
// tC4's `swapPages`, the help panel's watcher and link handler (tC4:src/views/AccountMenu.jsx:
// 182-274) and `readHelpPage` (tC4:src/data/helpPages.ts), kept line for line in tC4's own
// language. It runs in the page, on the frame's documents, as tC4's panel does; the server
// serves the vendored help pages unchanged, so help-pages.txt can check every byte.
// wasm-bindgen copies this file beside the glue (`#[wasm_bindgen(module = ...)]` in
// src/views/help_panel.rs), inside `ui/<build>/`, where the page's CSP allows scripts.
//
// The pages' own requests carry the launch's token the way the frame's do: the shell's header
// filter on /help/*, Playwright's extraHTTPHeaders, the smoke's route. Nothing here adds one.

/** The help page at `url`, parsed, or null when the server answers with an error, a redirect
 * takes the read out of the help directory, or the answer is not HTML. The panel runs the
 * page's scripts, so only a help page may come back. */
async function readHelpPage(url, signal) {
  const res = await fetch(url, { signal });
  if (!res.ok || !res.url.startsWith(new URL('.', url).href)) return null;
  if (!/^text\/html\b/i.test(res.headers.get('content-type') ?? '')) return null;
  return new DOMParser().parseFromString(await res.text(), 'text/html');
}

/** #729: a new document in the frame paints blank before its first paint, so loading each
 * topic as a page made the whole panel blink white. Every help page has the same head, so a
 * move to another help page fetches it and swaps the body in place: nothing reloads. The
 * navigation API sees every way a page moves (a link, a form, a script setting location), and
 * keeps the frame's URL and history. */
function swapPages(doc) {
  const nav = doc.defaultView?.navigation;
  // The frame's first document is about:blank, before the help home loads.
  if (!nav || !/^https?:$/.test(doc.location.protocol)) return;
  const base = new URL('.', doc.URL).href;
  let load = null; // the page that failed to swap, loaded as a page
  nav.addEventListener('navigate', (e) => {
    const to = e.destination.url;
    if (!e.canIntercept || e.hashChange || e.downloadRequest !== null || e.navigationType === 'reload' || !to.startsWith(base) || to === load) return;
    e.intercept({
      handler: async () => {
        let next = null;
        try {
          next = await readHelpPage(to, e.signal);
        } catch { /* cancelled, or no answer: both below */ }
        // A second move cancelled this one, and the second one swaps.
        if (e.signal.aborted) return;
        if (!next) {
          // No page to swap in: load it as before, so the frame shows what the server sends.
          load = to;
          doc.location.replace(to);
          return;
        }
        doc.title = next.title;
        const body = doc.adoptNode(next.body);
        // A parsed script never runs; a new one does (in order: the search reads its index).
        for (const old of body.querySelectorAll('script')) {
          const script = doc.createElement('script');
          for (const { name, value } of old.attributes) script.setAttribute(name, value);
          script.async = false;
          script.textContent = old.textContent;
          old.replaceWith(script);
        }
        // The logo in the help bar is a new image too: shown before it decodes, it blinks.
        await Promise.all([...body.querySelectorAll('img:not([loading="lazy"])')].map((img) => img.decode().catch(() => {})));
        if (e.signal.aborted) return;
        doc.body.replaceWith(body);
      },
    });
  });
}

/** #519: watches the help frame. Each page is a new document in the frame: each one is met
 * before it is first painted, so no click can reach an unwatched link (the frame's load event
 * waits for every image). A same-origin frame shares this window's event loop, so a check on
 * every animation frame runs before the new page's first paint. A link to another origin does
 * not open in the frame: `openLink(url)` asks the internet question and opens the browser.
 * Escape in the frame calls `close`. Answers the function that stops the watch. */
export function watchHelp(frame, openLink, close) {
  let seen = null;
  let tick = 0;
  const watch = () => {
    tick = requestAnimationFrame(watch);
    const doc = frame?.contentDocument;
    if (!doc || doc === seen) return;
    seen = doc;
    swapPages(doc);
    const onLink = (e) => {
      const a = e.target.closest?.('a[href]');
      if (!a) return;
      const url = new URL(a.href);
      // auxclick is the middle button here; the right button opens a context menu, not the link.
      if (e.type === 'auxclick' && e.button !== 1) return;
      if (url.origin === window.location.origin) {
        // #632: Ctrl, Shift, Command and the middle button ask for a new window, which the
        // desktop app opens in the system browser. Load the page here, as a plain click does.
        if (e.type === 'click' && !(e.ctrlKey || e.shiftKey || e.metaKey)) return;
        e.preventDefault();
        doc.location.assign(url.href);
        return;
      }
      e.preventDefault();
      if (/^https?:$/.test(url.protocol)) openLink(url.href);
    };
    doc.addEventListener('click', onLink, true);
    doc.addEventListener('auxclick', onLink, true);
    doc.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') close();
    });
  };
  watch();
  return () => cancelAnimationFrame(tick);
}
