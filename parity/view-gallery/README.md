# The tC4 view-state gallery

> **Retired 2026-10-29.** The screenshot comparison with tC4 was done for 4.1.0 and no longer
> gates a change (`docs/rulings/2026-10-29-tc4-is-history.md`). This folder is kept for what
> other checks still use from it; the rest is a record.

Many states of tC4's screens have no baseline screen: the open overlay, a release upgrade, the
check list's empty states, a failed import, Home with no project. The gallery draws them with
tC4's own views, so the Rust port has a reference for each (docs/APP-DESIGN.md §7.4, unit A7).
Each case is a row of `parity/app/report.json`, and `crates/tc-ui/tests/screen_record.rs` gates
it like a baseline screen.

## How it works

- `build.mjs` builds a Vite app that mounts tC4's real `src/App.jsx` from a tC4 clone. It uses
  the clone's own Vite, React plugin, node polyfills and React, through a `node_modules` link to
  the clone's, in release mode with tC4's base (`/clients/uw-tc4/`) and defines, as
  `npm run build` builds tC4; the CSS comes out byte-identical to tC4's release build. A
  resolver plugin replaces every import of tC4's `src/state.jsx` with `src/state-stub.jsx` and
  fails the build if tC4's file is ever loaded.
- `src/state-stub.jsx` exports what the 32 view files and `App.jsx` import: `useApp`,
  `SCRIPT_FONTS`, `SUITE_VERSION`, `door43` and `isOldTestament` (the second and the last cut
  from `state.jsx`'s own source). `useApp` answers tC4's value, `book` and `sourceModel` derived
  as tC4's provider derives them. `actions` records every call with its arguments, answers the
  two selectors Get source texts calls while drawing (`sourceGateways`, `installedCountFor`)
  with tC4's own code, and otherwise returns nothing unless the state or the case gives a reply.
  No provider effect of tC4 runs: no boot read, no scheduler, no request.
- `record-states.mjs` drives real tC4 against the baseline's mock (`parity/app/env-tc4.mjs`)
  along each route of `states.mjs`, photographs the screen with the baseline's capture rules,
  and reads tC4's whole state `s` from its React tree, with the replies of the read-only actions
  the view draws. That is a recorded state (`states/`, `states.json`).
- `cases.mjs` lists the cases. A case is a recorded state, an optional patch (the `set` patch
  tC4's reducer would apply, written only where the mock cannot go, from recorded values and
  tC4's own functions, `patch-kit.mjs`), optional steps (`steps.mjs`), and, for a DOM-first
  group, a `scope`. Its group is a row of §7.4's table, which names the units that capture its
  Rust side; `cases.mjs` never types a unit.
- `capture.mjs` builds the gallery, calibrates every recorded state, draws every case and
  photographs it as the baseline does (`parity/app/observe.mjs`): Playwright 1.56.1, Chromium
  build 1194, the five flags, 1280x800, the pointer at rest, animations disabled.

## Setup

The shared tC4 checkout has no `node_modules`, so make a clone of the reference commit with its
dependencies (never run `npm ci` in a shared or read-only checkout):

```sh
. ~/.tc-env
git clone --no-checkout /path/to/translationcore4 "$SCRATCH/tc4"
git -C "$SCRATCH/tc4" checkout d310bf3
(cd "$SCRATCH/tc4" && npm ci --no-audit --no-fund)
(cd parity/tc4-baseline && npm ci)          # Playwright 1.56.1 for the capture
export TC4_ROOT="$SCRATCH/tc4"
```

The clone's own Playwright (1.61.1 at `d310bf3`) is never used: `playwright.mjs` loads
`parity/tc4-baseline`'s and refuses any other version.

## Running it

```sh
node parity/view-gallery/record-states.mjs [--only <state>,...] [--port 19997]
node parity/view-gallery/capture.mjs [--only <case>,...] [--no-build] [--out <dir>] [--check]
node --test parity/view-gallery/view-gallery.test.mjs
```

- `record-states.mjs` records into `states/` and `states.json`. A state that is a baseline screen
  must photograph exactly as the committed screen, or the machine does not reproduce the
  baseline and nothing of it is written. Recording is not reproducible byte for byte (tC4 writes
  wall-clock times into journals, saves and the conflict review's versions), so the committed
  states are the reference and are reviewed like a baseline capture. It uses the mock's
  `parity/tc4-baseline/.state` folders: never run it beside `parity/tc4-baseline/capture.mjs` or
  `parity/app/measure-masks.mjs`.
- `capture.mjs` writes `screens/<case>.png`, `dom/<case>.json`, `actions/<case>.json` and
  `calibration.json` here, `parity/app/cases.json`, and the rectangles of the cases' masks in
  `parity/app/masks.json`. A case fails, and nothing is written, on a page error, a console
  error, the view-error callout, a request (every request outside the page's own files is
  aborted), a step that fails, or an empty `#root`. `--out <dir>` writes everything into `<dir>`
  instead; `--check` captures into a temporary folder and compares every file with the committed
  one byte for byte. Two captures from the same states are byte-identical.
- Then `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`, and review the report.

## Calibration

Every recorded state is drawn with no patch and compared with real tC4's own capture of it at
the moment it was recorded (`real` in `states.json`): the PNG byte for byte, and the DOM after
the gate's id renumbering (`lib.mjs` `domDigestInput`). All 23 are equal (`calibration.json`).
So the stub adds nothing, and a case differs from what tC4 shows only by its patch and steps.

Focus is the one thing a state drawn afresh does not have: a route leaves it where its last click
put it. A state's `focus` steps put it back (`refocus`, by script and with no keyboard ring, or
`blur`, when the focused control went away); calibration replays them, and so does every case on
the state that has no patch. Before the state is mounted, the capture clicks a full-window layer
at the pointer's resting point, so focus a view moves on mount is not drawn as keyboard focus,
as in tC4 after the click that opened the screen.

## The case format

```js
{ name: 'gateway-change-busy',        // kebab-case, never a baseline screen's name
  group: 'M10',                       // a group of APP-DESIGN §7.4
  state: 'gateway',                   // a state of states.mjs
  patch: (s, { state, kit }) => ({ gatewayBusy: true }),  // optional: tC4's `set` patch
  returns: { name: { resolves: {} } },// optional: replies over the state's recorded ones
  steps: [{ do: 'click', target: '[data-testid="x"]' }],   // optional (steps.mjs)
  scope: { attr: 'role', value: 'dialog', which: 'last' }, // exactly for a DOM-first group
  masks: ['feedback-attachment'],     // masks whose element the case shows
  rust: async (app, env) => {},       // optional: how the Rust app reaches the case
  why: 'what it shows, and for a patch why the mock cannot reach it and which tC4 line it follows' }
```

`cases.json` records, for each case, the top-level keys of tC4's state its patch sets (none when
tC4 drew the recorded state itself): 49 of the 68 cases carry a patch.

## The replies (filled by the porting units)

A case's Rust side needs tc-api replies that match its tC4 state. None exists when the gallery is
built: `MemoryBackend` is unit B1b's and a screen's view types are built by its unit (APP-DESIGN
§10). So the unit that ports a case's group writes `replies/<case>.json`:

```json
{ "case": "<name>",
  "replies": [ { "endpoint": "<service>/<name>", "request": {}, "reply": {}, "source": "memory" } ] }
```

`request` and `reply` are tc-api's JSON; `source` is `memory` (recorded from `MemoryBackend`, where
the state exists there) or `hand` (written by hand, only where it cannot exist yet).
`view-gallery.test.mjs` checks the shape and that the case exists.

## What a porting unit does

1. Port the screen; add `rust` to the case in `cases.mjs` when the default route does not apply
   (a case with no patch and no replies on a state whose route uses only the driver takes that
   route and the case's steps).
2. Write `replies/<case>.json`.
3. `node parity/app/capture-rust.mjs --unit <unit> --case <names> [--layers dom]`, then
   `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record`, and review the rows.
4. Compare the action trace (`actions/<case>.json`: what tC4 asked for on mount and at each
   step) with the Rust screen's requests and navigation. No gate rule checks it yet.

A patched case's focus is where the view puts it when the state is drawn; a Rust route to the
same state may leave it elsewhere, which the porting unit matches with the case's steps or
explains as a residual.

## The files, and who edits them

| File | Edited by |
|---|---|
| `states/`, `states.json` | `record-states.mjs` only |
| `screens/`, `dom/`, `actions/`, `calibration.json`, `parity/app/cases.json`, the case rectangles of `parity/app/masks.json` | `capture.mjs` only |
| `states.mjs`, `cases.mjs` | by hand; record or capture again after a change |
| `replies/` | the unit that ports the case's group |
