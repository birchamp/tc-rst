# The tC4 view-state gallery (unit A7, 2026-10-18)

Measured on the owner's Linux PC (docs/evidence/linux-machine-2026-10-18.md): 20 threads,
shared with up to three other agents, so each timing gives the load (`uptime`) beside it. Plan:
`.audit/plans/A7.md`.

tC4 `d310bf3` in a fresh clone (`git clone --no-checkout`, `git checkout d310bf3`), its
`node_modules` copied from another `d310bf3` clone with the same `package-lock.json` and its help
pages copied from it, so nothing was fetched. Playwright 1.56.1 from `parity/tc4-baseline`
(`npm ci` there), Chromium 141.0.7390.37 (build 1194). Machine: `linux-x64`, 535 font lines,
sha256 `a15fd0433b66112396faed20c7b88e11229781916faeaad035b9c8961a72eb33`, the fingerprint of
`parity/tc4-baseline/manifest.json`, of `parity/view-gallery/states.json` and of
`parity/app/cases.json` alike.

## M1: this PC reproduces the baseline

`TC4_ROOT=<clone> node parity/tc4-baseline/capture.mjs --only home,feedback,check-tn-session` at
22:39 (load 25.1) took 27 s and left `git status` clean: the three PNGs, DOM files and
`manifest.json` byte-identical to the committed ones. Recording then checked the eight states
that are baseline screens (`home`, `check-picker`, `check-tn-session`, `align-workspace`,
`source-texts`, `guided-fix`, `en-translate-section-editor`, `share-dialog-check`): each real
capture equalled the committed screen's PNG sha256 and DOM digest.

## M2: calibration

Each of the 23 recorded states drawn by the gallery with no patch, against real tC4's capture of
it when it was recorded: the sha256 of the PNG and of `lib.mjs` `domDigestInput` (the DOM with
generated ids renumbered as the gate renumbers them).

| Run | Equal pixels | Equal DOM |
|---|---|---|
| first, no focus steps | 23 of 23 | 12 of 23 |
| with the states' focus steps | 23 of 23 | 23 of 23 (`calibration.json`) |

The 11 DOM differences were all `.focused`, on one line each: a route leaves the focus where its
last click put it, and a state drawn afresh does not have that. The focus steps put it back:

| State | Real tC4's focus | Step |
|---|---|---|
| `check-picker` | the Check tab, clicked by the route | `refocus` the selected tab |
| `share-check` | Share, the button Next became | `refocus` `share-submit` |
| `add-book-pick-multi` | the Hebrews chip, clicked last | `refocus` the second selected chip |
| `home-open-error` | the Titus tile, clicked to open | `refocus` `book-tile-TIT` |
| `signin-error`, `import-files`, `import-review-usfm`, `import-review-tc3`, `add-book-pick`, `add-book-usfm`, `add-book-usfm-files` | the page: the clicked control of a dialog step went with the step | `blur` (the gallery's dialog focuses its Close button on mount) |

One later run found `add-book-usfm-files` unequal again: the page had flagged the mount right
after `render()`, before React's commit, so the `blur` ran before the dialog's own mount focus.
The flag now comes from an effect of a wrapper, which runs after every effect below it; every
run since calibrated all 23.

## M3: two captures are byte-identical

The digest of a set is the sha256 of the lines `"<case> <sha256>\n"` in `cases.json` order.

| Run | Started (load) | PNG digest | DOM digest | Trace digest |
|---|---|---|---|---|
| 1 (`--out`) | 23:19:00 (9.07) | `f74bf9c54df8b583de86fdae261256dc6c2fde829955b1fa5c28b7cc3abbd11f` | `174b73907d50c57186aa24aa32e726fc12a4f25056cbe5f931011af1db1b4521` | `c975d0c9538ea45df0445c867f93462cdd693c585d89b02257ab02343d37d91a` |
| 2 (`--no-build --out`) | 23:23:23 (1.51) | the same | the same | the same |

`diff -r` of the two output folders (screens, DOM files, traces, `calibration.json`,
`cases.json`, `masks.json`) was empty. `node --test parity/view-gallery/view-gallery.test.mjs`
with `TC4_ROOT` ran 16 tests, 16 passed in 5 min 56 s (two more captures, byte-identical to each
other, and the first equal to the committed record file for file).

## M4: times

| What | Took | Load |
|---|---|---|
| gallery build (`build.mjs`, tC4's Vite) | 4.3 s | 25 |
| `record-states.mjs`, 23 states | 2 min 24 s; slowest `review-mine` 20.5 s, `review` 11.7 s, `cc-obs` 11.2 s | 28.6 |
| `capture.mjs`, 23 calibrations and 68 cases | 3 min 11 s; median 1.9 s per state, 1.9 s per case | 9.1 |
| `cargo test -p tc-ui --test screen_record` before A7 (`4b88ff2`, 33 tests) | 0.79 to 0.97 s | 3 to 5 |
| the same after A7 (42 tests, 130 rows) | 0.89 to 1.69 s | 3 to 9 |

## M5: bytes added

| Files | Bytes |
|---|---|
| 68 PNGs (`screens/`) | 6,331,034 |
| 68 DOM files (`dom/`) | 3,091,524 |
| 68 action traces (`actions/`) | 119,610 |
| 23 state files | 540,473 |
| 8 state parts | 5,556,393 |
| total | 15,639,034 (budget 25 MB) |

Parts are content-addressed and shared: a Titus `sources` part (1,083,171 bytes) by
`check-tn`, `review`, `review-mine` and `section-editor`; another by `check-picker` and
`guided-fix`; a third by `align` (the three differ only in the order tC4's loads inserted `ult`
and `ust`); the Titus `understand` part (484,288 bytes) by `check-picker`, `check-tn`, `align`
and `guided-fix`; the shared scenario's by `review` and `review-mine`.

## M6, M7: the record

- `cases.json` `machine` equals `manifest.json`'s.
- `report.json` after `UPDATE_REPORT=1`: 130 rows (62 screens, 68 cases), 130 `unported`.
- 19 cases are a recorded state as tC4 drew it, with at most view-local steps; 49 carry a patch.

## M8: what tC4 asks for on mount, per group (for the porting units)

The distinct actions of each group's traces at `mount` (no case's steps call an action):

| Group | Actions on mount |
|---|---|
| `S4`, `M10`, `M18` | `loadUnderstand` |
| `S12` | `readPrintStories` |
| `S14`, `license`, `net-failed`, `share`, `import`, `add-book`, `feedback`, `sign-in`, `home`, `resume`, `import-toast` | `loadShared`, `loadProgress`, `loadConflicts` (for the project cards; none on `home-loading` and `home-empty`) |
| `M9` | over Source texts: `sourceGateways`, `installedCountFor` (18 times), `loadUnderstand`; over the tool picker: `runPreflight`, `loadPickerProgress` |
| `check-empty` | `runPreflight`, with `refreshCheckDraft` in a session or `loadPickerProgress` at the picker |
| `align-unavailable` | `openAlign`, `loadAlignIndex`, `runPreflight` |

## Found while building

- tC4's state holds one value JSON cannot carry: the sign-in step's identity Symbol
  (`state.jsx` `si.opened: Symbol('sign-in step')`, compared only by identity). It is carried as
  `{ "$symbol": "sign-in step" }` and revived as a new Symbol; nothing is dropped.
- The order of the versions in the review with a version of mine follows the recording's clocks:
  this recording shows mine first, the planner's showed it last. The route checks three versions
  with exactly one of mine, in any order.
- `capture-rust.mjs` dropped case entries when it rewrote `captures.json`, and
  `measure-masks.mjs` crashed on a rectangle named after a case; both are fixed.
