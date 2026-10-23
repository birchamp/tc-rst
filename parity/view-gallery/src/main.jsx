// The gallery's page: tC4's own App over the stub's provider, mounted on a state the capture
// hands in. tC4's main.jsx order for the style sheets. No top-level await: the build's target
// refuses it.
//
// The capture loads the page, waits for `window.__galleryReady`, clicks the full-window
// pointer layer (so focus a view moves on mount is not drawn as keyboard focus, as in tC4
// after the click that opened the screen), then calls `window.__mount({ s, returns, locale })`
// and waits for `window.__galleryMounted`, which is set once React has committed the first
// render and run its effects (a dialog moves the focus in one). Every action call lands in
// `window.__galleryCalls` as `{ at, name, args }`, `at` being `window.__galleryPhase`
// ("mount", then "step N").
import React from 'react';
import { createRoot } from 'react-dom/client';
import '@tc4/src/ds/styles.css';
import '@tc4/src/ui.css';
import { setLocale } from '@tc4/src/i18n';
import App from '@tc4/src/App.jsx';
import { StubProvider, SELECTORS } from './state-stub.jsx';
import { makeActions, reviveValue, traceArgs } from '../lib.mjs';

/** Marks the mount done from an effect: a parent's effect runs after every child's. */
function Mounted({ children }) {
  React.useEffect(() => {
    window.__galleryMounted = true;
  }, []);
  return children;
}

window.__galleryCalls = [];
window.__galleryPhase = 'mount';

window.__mount = ({ s, returns = {}, locale = 'en' }) => {
  if (window.__galleryMounting) throw new Error('the gallery mounts one state per page');
  window.__galleryMounting = true;
  const state = reviveValue(s);
  const record = ({ name, args }) => window.__galleryCalls.push({ at: window.__galleryPhase, name, args: traceArgs(args) });
  const actions = makeActions({ s: state, returns, selectors: SELECTORS, record });
  setLocale(locale);
  document.getElementById('gallery-pointer')?.remove();
  createRoot(document.getElementById('root')).render(
    <React.StrictMode>
      <Mounted>
        <StubProvider s={state} actions={actions}>
          <App />
        </StubProvider>
      </Mounted>
    </React.StrictMode>,
  );
};

window.__galleryReady = true;
