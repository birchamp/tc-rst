// tC4's `src/state.jsx`, as the views see it, over a recorded state: what the 32 view files
// and `App.jsx` import from it, and nothing else. The build swaps this file in for every
// import of tC4's state.jsx and fails if tC4's file is ever loaded (build.mjs), so no view
// runs tC4's provider: no boot read, no scheduler, no request. A render of a state never
// changes it.
import React, { createContext, useContext, useMemo } from 'react';
import { bookModel } from '@tc4/src/data/bookModel';
import { draftPercent } from '@tc4/src/data/usfm/indexer';
import { isSourceAbsent } from '@tc4/src/data/sourceState';
import { BOOK_NAMES } from '@tc4/src/data/bookNames';
import { GATEWAYS, gatewayKey } from '@tc4/src/data/gateways';
import { Door43Api } from '@tc4/src/data/share/door43Api';
import { useLocale } from '@tc4/src/i18n';

// state.jsx re-exports it from the same module (state.jsx:72, 77).
export { SUITE_VERSION } from '@tc4/src/data/installedSuite';
// The exact source text of state.jsx's own definitions, cut out by build.mjs.
export { SCRIPT_FONTS, isOldTestament } from 'virtual:tc4-state-exports';

/** state.jsx:132 makes it with the internet switch; here the switch is always off and its
 * fetch refuses, so no call can leave the page. The views read only `door43.server`. */
export const door43 = new Door43Api({
  allowed: () => false,
  fetchFn: () => Promise.reject(new Error('the gallery sends no request')),
});

/** The actions a view calls while it draws, answered as state.jsx:4195-4200 answers them;
 * `s` is the state the gallery mounted. Every other action is recorded and answered by the
 * case's replies, or not at all. */
export const SELECTORS = {
  sourceGateways: () => GATEWAYS.map((g) => ({ ...g, key: gatewayKey(g) })),
  installedCountFor: (s, [g]) => {
    const key = gatewayKey(g);
    return s.installedSrc.filter((x) => x.langKey === key).length;
  },
};

const AppCtx = createContext(null);

/** AppProvider's value (state.jsx:7339-7341): `{ s, ...model, sourceModel, actions,
 * BOOK_NAMES, locale }`, with `model` and `sourceModel` derived as state.jsx:3634-3645
 * derives them. */
export function StubProvider({ s, actions, children }) {
  const model = useMemo(() => {
    if (!s.project || !s.book || s.bookRaw == null) return { book: null, progress: {} };
    const { entries, byChapter, chapterNums } = bookModel(s.bookRaw);
    const draftPct = draftPercent(entries, s.bookRaw);
    return { book: { code: s.book, byChapter, chapterNums, draftPct }, progress: {} };
  }, [s.project, s.book, s.bookRaw, s.tick]);

  const sourceModel = useMemo(() => {
    const src = s.sources[s.sourceTab];
    if (!src || isSourceAbsent(src)) return src || null;
    return src.chapters;
  }, [s.sources, s.sourceTab]);

  const locale = useLocale();
  const value = { s, ...model, sourceModel, actions, BOOK_NAMES, locale };
  return <AppCtx.Provider value={value}>{children}</AppCtx.Provider>;
}

export const useApp = () => useContext(AppCtx);
