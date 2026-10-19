# The app parity harness

P7 (docs/PLAN.md) asks that the Rust app's screens be compared with tC4's and that every
difference be listed. This folder holds the harness that captures both apps the same way and
the record the gate in `cargo test` checks (docs/APP-DESIGN.md §7). The tC4 side's record is
`parity/tc4-baseline`; the Rust side's is here.

## One harness, two environments

| File | What it is |
|---|---|
| `screens.mjs` | the 62 baseline screens and the steps that reach each |
| `driver.mjs` | the steps a person takes, over an environment (`AppEnv`) |
| `observe.mjs` | the capture rules: the browser, request tracking, `settle`, the screenshot, `observe` (what the DOM shows), `domSnapshot`, `machineFingerprint`, `measureTargets` |
| `env-tc4.mjs` | tC4 against the Node mock of the Pankosmia API |
| `env-rust.mjs` | `tc4 serve --test-hooks --scenario <state>` on empty folders under `target/`: the scenario's state built by `state.mjs`, its projects written into the run's projects folder and opened through the store (C1a; `backend: 'memory'` passes `--backend memory`, B1b's memory backend), the token header, `test/reset`, `test/quiet`, the event stream left out of the wait, every other origin aborted |
| `capture-rust.mjs` | captures Rust screens into `screens/`, `dom/` and `captures.json` |
| `behaviour.mjs` | records a sequence of steps from a baseline screen on either side (layer 4) |
| `measure-masks.mjs` | measures each mask's rectangle on tC4's baseline screens |
| `state.mjs` | builds the scenarios' state from this repository's copies of tC4's fixtures |
| `units.mjs` | the units of §10 and the gallery groups of §7.4, read from docs/APP-DESIGN.md |

## Capturing

tC4's side: `parity/tc4-baseline/capture.mjs` (its README). The Rust side, after a unit ports
its screens:

```sh
. ~/.tc-env
cargo build -p tc4 --features test-hooks,fixture-backend   # target/debug/tc4
crates/tc-ui/build-app.sh                        # target/tc-ui-app
node parity/app/capture-rust.mjs --unit B2 --only home,add-project
UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record
git diff parity/app/report.json                  # review every row that changed
```

`--unit` is the unit of §10 that captures. `--layers dom` is a DOM-first capture: a screen whose
DOM unit comes before its pixels unit (§7.6), compared within its `scopes.json` scope until the
pixels unit captures it whole. Screens are captured on the store, the default since C1a: the
gate fails a capture on `--backend memory` once C1a has landed (APP-DESIGN §7.5).
`--backend memory+scripted` marks a screen captured on scripted job outcomes (on the memory
backend, until its recapture unit lands). `--case <names>` captures gallery cases instead
(below). Playwright comes from
`parity/tc4-baseline/node_modules` (`npm ci` there).

Door43 stays on this machine. Without a fake, `tc4 serve` is pointed at a port nothing listens
on. A screen or case that signs in needs the fake Door43 (A8): `cargo build -p tc-door43-fake`,
and `capture-rust.mjs` starts `target/debug/tc-door43-fake` for each scenario when it is there
(`--door43-fake <bin>` names another). `startDoor43Fake` in `env-rust.mjs` starts it and answers
its control port (the requests Door43 received, faults, held requests, tokens); the journeys get
it with `withApp(body, { door43: true })` and `TC4_DOOR43_FAKE`.

A screen whose dialog the Rust app reaches another way until its unit builds the trigger has a
`rust` route in `screens.mjs` (A8: `share-signin` opens from the account menu until B9b's
Share card opens it). `capture-rust.mjs` takes it only for a capture that claims no pixels
(`screenRoute`): the pixels come the tC4 way, and the unit that captures them deletes the route.

## Gallery cases

Many states have no baseline screen. `parity/view-gallery` draws them with tC4's own views over
states recorded from real tC4 (APP-DESIGN §7.4, its README), and `cases.json` lists each case:
its group of §7.4, its scenario, the units that capture its DOM and its pixels, the scope a
DOM-first case compares, and the sha256 of its tC4 screenshot, DOM and action trace. Each case is
a row of `report.json` (`kind: case`), `unported` until its units capture it; a case has no
meaning layer, and its pixels compare only with a Rust capture of the machine in `cases.json`.
A unit captures a case with `node parity/app/capture-rust.mjs --unit <unit> --case <names>
[--layers dom]`, through the case's `rust` route in `parity/view-gallery/cases.mjs`, or, for a
case that is its state as tC4 drew it on a route of the driver alone, that route and the case's
steps.

## The gate

`crates/tc-ui/tests/screen_record.rs` recomputes all four layers from the committed files on
every `cargo test` and fails when `report.json` is not exactly what they give, so the report is
never stale and never edited by hand. `UPDATE_REPORT=1` rewrites it and draws `diffs/<name>.png`
for each row whose pixels differ (red outside the masks, yellow inside).

| Layer | Compared |
|---|---|
| pixels | every pixel whose channel bytes differ (pixelmatch at threshold 0), outside the masks' rectangles; only between captures of one machine |
| DOM | the snapshot lines after the listed normalizations: the Rust side's project folders as repo paths, same-origin asset paths as file names, the masks' elements taken out, generated ids renumbered by first appearance |
| meaning | `homeCards` and `syncToast` field by field |
| behaviour | each recorded step's DOM with node numbers, focus and scroll lock |

A row is `same`, `declared` (every difference inside a mask), `differs` (explained in
`residuals.json`), `unported` (no layer captured yet; `missing` names the unit that captures
each layer) or `failed` (a capture error, a page error, a view or boot error, a refused request,
the settle deadline, a screenshot of another size than tC4's, pixels from another machine, a
mask or scope not found). Nothing excuses a failed row.

## The files, and who edits them

| File | Edited by |
|---|---|
| `captures.json`, `screens/`, `dom/` | `capture-rust.mjs` only |
| `behaviour/` | `behaviour.mjs --env rust` only (tC4's: `parity/tc4-baseline/behaviour/`, `--env tc4`) |
| `report.json`, `diffs/` | `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record` only |
| `units.json` | each unit adds its own id in the branch that builds it |
| `masks.json` | by hand, each mask with the ruling it cites (`build`, or a `Q<n>` that docs/PLAN.md lists as a difference on purpose) and its reason; rectangles by `measure-masks.mjs` on baseline screens and by `parity/view-gallery/capture.mjs` on gallery cases. A unit whose rebuilt element differs from tC4's sets the mask's `rust` target |
| `residuals.json` | by hand: a reason for every `differs` row, with exactly the layers it differs in (`pixels`, `dom`, `meaning`, `behaviour`), and the differences no row shows |
| `scopes.json` | by hand, only for DOM-first screens |
| `cases.json` | `parity/view-gallery/capture.mjs` only |

## What a unit that captures must do

1. Add its id to `units.json`. From then on the gate fails while a layer that §7.6 gives it is
   not captured, and a scripted screen whose recapture unit it is still runs on scripted
   outcomes.
2. Capture its screens with `--unit <its id>`, and its sequences with `behaviour.mjs`.
3. Rewrite the report and review it: fix the port where it differs, or explain each remaining
   `differs` row in `residuals.json`.

## The machine

Fallback glyphs come from the system's fonts, so pixels are compared only between captures
whose `machine` fingerprints are equal: the platform, the Chromium version and the sha256 of
`fc-list`'s sorted font lines. tC4's baseline was recaptured on the owner's Linux PC for this
(`parity/tc4-baseline/README.md`, "Recaptured on the Linux PC"). A pixels row from another
machine fails; capture both sides again on one machine (`capture.mjs`, then
`capture-rust.mjs --all`), or, on a machine without `fc-list`, capture `--layers dom,meaning`.
The DOM and meaning layers compare on any machine.

## Tests

`node --test parity/app/observe.test.mjs parity/app/env-rust.test.mjs` (CI runs both; the
browser tests of `env-rust.test.mjs` run with `TC4_BIN=target/debug/tc4
TC4_UI=target/tc-ui-app`, and the fake Door43's with `TC4_DOOR43_FAKE`), `node --test parity/view-gallery/view-gallery.test.mjs` (CI runs its
record checks; its browser tests run with `TC4_ROOT`), and
`cargo test -p tc-ui --test screen_record`.
