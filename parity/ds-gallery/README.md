# tC4 design-system baseline gallery

Every component of translationCore 4's design system (`src/ds`), in its meaningful states, each
drawn alone in a fixed-size frame and captured as a PNG. The Rust/Leptos port renders the same
cases from `gallery-spec.json` and compares its images with `screens/`.

- tC4 source: `/home/user/unfoldingword/translationcore4`, commit
  **`d494a9d6ba9f4cdcc09faff9038c93159106045b`** (version `4.0.0-rc.1`, clean tree). The gallery
  imports `src/ds/index.js` and `src/ds/styles.css` from that checkout through a Vite alias. It
  copies nothing and changes nothing there.
- Output: **702 cases**, 702 PNGs in `screens/<Component>/<state>.png`: 666 component cases for
  the 53 components that the 52 files in `src/ds/components` export (`Radio.jsx` exports `Radio`
  and `RadioGroup`), plus 36 token cases. 5.8 MB in total.
- Contract: `gallery-spec.json` lists every case with its component, state, source file, frame,
  document language and direction, exact props as JSON, interaction steps and what it needs.
- Versions: React 18.3.1, react-dom 18.3.1, Vite 6.4.3, @vitejs/plugin-react 4.7.0 and esbuild
  0.25.12, the same as tC4's `package.json`. Playwright 1.56.1 with Chromium 141.0.7390.37,
  headless. tC4 pins @playwright/test 1.61.1, but the only Chromium in `/opt/pw-browsers` is
  build 1194, which is Playwright 1.56's (as in `parity/tc4-baseline`).
- Two consecutive full runs, each with a fresh spec and build, gave byte-identical PNGs.

## Rerun

```sh
cd /home/user/tc-rst/parity/ds-gallery
npm install          # the versions above, into ./node_modules (git-ignored)
npm run capture      # spec, vite build, then one screenshot per case (about 2.5 minutes)
```

`npm run capture` runs `capture.mjs`, which:

1. Runs `build-spec.mjs`. It writes `gallery-spec.json` from `cases/*.mjs`. It stops if a
   component file has no case, a case names a component that `index.js` does not export, or two
   cases share an id.
2. Builds the gallery with Vite into `dist/` (git-ignored).
3. Serves `dist/` on a free local port and captures each case by the spec's `protocol` (below),
   six pages at a time.
4. Fails when a case throws, logs an error, or overflows its frame. A full run also deletes
   screens that no longer have a case.

| Option | Effect |
|---|---|
| `--only Button/,Select/open` | Capture only the cases whose id starts with one of these prefixes. |
| `--jobs 6` | Pages captured at once. |
| `--no-build` | Use the existing `gallery-spec.json` and `dist/`. |
| `TC4_ROOT=/path/to/translationcore4` | Use another tC4 checkout (spec, build and token pages). |

To browse: `npm run dev` and open the address it prints. The index shows every case. Cases that
open a layer, need an interaction or set the document language load in an iframe, without their
steps. `?case=Button/primary-md&auto=1` shows one case alone.

## Layout

| Path | What it is |
|---|---|
| `cases/*.mjs` | The cases, by DS folder. `cases/lib.mjs` has the helpers and the shared copy. `cases/tokens.mjs` reads tC4's token files, so the token pages follow them. |
| `build-spec.mjs` | Writes and checks `gallery-spec.json`. |
| `gallery-spec.json` | The contract: every case, the capture protocol and the node format. The app renders from it, so the spec and the screens cannot disagree. |
| `src/main.jsx`, `src/render.jsx` | The gallery app: one case alone (`?case=`) or the browsing index. `render.jsx` turns a spec tree into React elements. |
| `protocol.mjs` | The capture protocol, shared by both captures: the static server, Chromium's flags, the steps, the settle, the screenshot. |
| `capture.mjs` | Build, serve, screenshot, check. |
| `screens/` | The baseline. |
| `capture-rust.mjs` | The same capture of the Leptos port (`crates/tc-ui`), into `screens-rust/`, then `diff.mjs`. |
| `diff.mjs`, `diff-report.json` | The pixel comparison of `screens-rust/` with `screens/`, per case. |
| `dom-diff.mjs`, `dom-report.json` | The DOM comparison of the two galleries, per case. |
| `screens-rust/` | The port's screens. Git-ignored: `capture-rust.mjs` writes them again. |
| `residuals.json` | Optional: an explanation per case id for a case that does not reach 0. Absent while every case is at 0. |

## The Leptos port

`crates/tc-ui` ports every component to Leptos 0.8.22 (client-side rendering, compiled to
`wasm32-unknown-unknown`) with the same DOM. Its `gallery` feature draws every case of
`gallery-spec.json` by the same protocol: the same page globals (`__galleryBooted`, `__mount`,
`__galleryMounted`), the same frame and tC4's stylesheet, fonts and images, copied verbatim into
`crates/tc-ui/assets/`.

### Build

Needs the `wasm32-unknown-unknown` target and the wasm-bindgen CLI at the version the crate pins.
No trunk and no cargo-leptos.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked   # once; the crate pins =0.2.129

crates/tc-ui/build-gallery.sh             # dev profile, about 1.5 minutes from clean
crates/tc-ui/build-gallery.sh --release   # wasm-release profile: opt-level z, LTO, 1 unit
```

Run them from the repository root. The script builds `tc-ui` with `--features gallery` (cargo
`-j 2`; set `CARGO_JOBS` to change it), runs `wasm-bindgen --target web`, and assembles a static
site in `target/tc-ui-gallery/` (`--out DIR` to change it): `index.html`, `pkg/`, `assets/` and a
copy of `gallery-spec.json`. Both profiles draw the same pixels. The release wasm is 1,017,408
bytes (327 KB with gzip -9), the gallery's spec renderer and `serde_json` included.

### Capture and compare

```sh
cd parity/ds-gallery
npm install                       # once; adds pixelmatch and pngjs to the baseline's packages
node capture-rust.mjs             # build (dev), capture screens-rust/, compare (about 7 minutes)
node capture-rust.mjs --release   # the same with the wasm-release build
node diff.mjs                     # compare again without capturing
npm run build && node dom-diff.mjs   # the DOM of both galleries; needs the React build in dist/
```

`capture-rust.mjs` takes `--only`, `--jobs` and `--no-build` as `capture.mjs` does. It serves
`target/tc-ui-gallery/` with the same static server and captures each case with the same code
(`protocol.mjs`) into `screens-rust/`. `diff.mjs` counts, per case, the pixels that differ in any
channel (pixelmatch with threshold 0 and anti-aliased pixels counted) and writes
`diff-report.json`. For a case that differs it writes `diffs/<C>/<s>.png` (git-ignored) and takes
the explanation from `residuals.json`. It fails when a case differs without one. `dom-diff.mjs`
compares, per case and after the steps, the frame's elements, their attributes, every longhand of
their inline style, their text, input values and checked states, and the focused element, and
writes `dom-report.json`.

Result at this commit: 702 of 702 cases at 0 mismatched pixels, with both the dev and the
wasm-release build. Every `screens-rust/` file is byte-identical to its `screens/` file. 702 of
702 cases have the same DOM. There is no `residuals.json` because nothing needs explaining. Both galleries are captured on the same
machine, so the machine-dependent rendering under "Skipped, and what to know" (fallback glyphs,
monospace, Nastaliq) affects both sides alike.

## The contract

Each case in `gallery-spec.json`:

| Field | Meaning |
|---|---|
| `id` | `<component>/<state>`, also the screen's path. |
| `component`, `file` | The DS export and its file in `src/ds` (`Tokens` for the token pages). |
| `frame` | `width`, `height`, `padding` (16 unless stated) and `background` (`#F7FAFC`, which is `--surface-app`). |
| `document` | `lang` and `dir` of `<html>`. 51 cases use the Arabic interface (`ar`, `rtl`), as tC4 sets it. |
| `props` | The exact props of the component under test, as JSON. |
| `render` | Only on 63 cases that place the component in something: a row, an Ocean ground, a centring box. It is the whole tree. Without it the tree is the component with `props`. |
| `steps` | Interactions before the screenshot (89 cases): `focus` (Tab until the target has keyboard focus), `hover`, `press` (button held), `click`, `key`, `type`. A target is a CSS selector inside the frame. |
| `needs` | `callbacks`: props that hold a callback (214 cases). `elements`: props that hold elements, such as `children`, `footer`, `trigger` or `center` (180 cases). |
| `isolate` | The case opens a layer, has steps or sets the document language. The browsing index loads it in an iframe. |
| `note` | Present where a case needs explaining. |

Props are plain JSON wherever the component allows it. A tree uses three markers:
`{ "$el": "Name", "props": {...} }` is an element (a DS export when the name is capitalised, an
HTML or SVG tag when it is lower case), `{ "$fn": "noop" }` is a callback that does nothing (some
components draw a control only when they get one, such as Modal's ✕ or Toast's action), and
`{ "$asset": "public/assets/…" }` is a file of the tC4 checkout. Styles are React inline styles.
`defaultValue` is a field's initial value.

The `protocol` object in the spec states how each case is drawn. In short: load
`src/ds/styles.css`. Set `<html lang dir>`. Size the viewport to the frame at device scale 1. The
frame is a `<div data-case tabindex="-1">` at the top-left corner, so a Layer's fixed scrim covers
exactly the frame and an anchored popover measures the frame as its window. Load every font.
Click the pointer at (1, 1) before mounting: the frame takes the focus without a ring, so a
dialog that focuses its first control on mount shows no ring, as in the app after a click opened
it. Mount, run the steps, turn every transition and animation off, wait for fonts, images and two
frames, and screenshot the frame.

## What the cases cover

Each component gets its variants, tones and sizes, disabled and selected states, hover and
keyboard focus where it has them, long text, and right-to-left where it handles direction. Overlay
states are real: Modal, Drawer, ConfirmDialog and Layer open in their frame, Menu opens with
`open`, Tooltip opens on hover and focus, and Select and SuggestField open by click or arrow key
(with the search field, groups, badges, meta, disabled rows and no matches).

| Component | File | Cases |
|---|---|---|
| Badge | core/Badge.jsx | 16 |
| Button | core/Button.jsx | 43 |
| Divider | core/Divider.jsx | 5 |
| IconButton | core/IconButton.jsx | 13 |
| Overline | core/Overline.jsx | 6 |
| ProgressBar | core/ProgressBar.jsx | 13 |
| StatusDot | core/StatusDot.jsx | 12 |
| Accordion | data/Accordion.jsx | 7 |
| Avatar | data/Avatar.jsx | 12 |
| KeyValueGrid | data/KeyValueGrid.jsx | 5 |
| ConfirmDialog | feedback/ConfirmDialog.jsx | 6 |
| EmptyState | feedback/EmptyState.jsx | 7 |
| InlineError | feedback/InlineError.jsx | 3 |
| Spinner | feedback/Spinner.jsx | 9 |
| Toast | feedback/Toast.jsx | 9 |
| Tooltip | feedback/Tooltip.jsx | 8 |
| Checkbox | forms/Checkbox.jsx | 12 |
| DropZone | forms/DropZone.jsx | 6 |
| FilterChip | forms/FilterChip.jsx | 11 |
| OptionCard | forms/OptionCard.jsx | 12 |
| Radio, RadioGroup | forms/Radio.jsx | 8, 6 |
| SearchField | forms/SearchField.jsx | 6 |
| Select | forms/Select.jsx | 17 |
| SuggestField | forms/SuggestField.jsx | 6 |
| TextArea | forms/TextArea.jsx | 10 |
| TextField | forms/TextField.jsx | 12 |
| Toggle | forms/Toggle.jsx | 9 |
| AppHeader | navigation/AppHeader.jsx | 10 |
| Breadcrumb | navigation/Breadcrumb.jsx | 7 |
| Menu | navigation/Menu.jsx | 7 |
| Action | primitives/Action.jsx | 52 |
| Choice | primitives/Choice.jsx | 29 |
| Field | primitives/Field.jsx | 13 |
| FieldGroup | primitives/FieldGroup.jsx | 7 |
| Input | primitives/Input.jsx | 13 |
| Layer | primitives/Layer.jsx | 7 |
| Progress | primitives/Progress.jsx | 18 |
| Rule | primitives/Rule.jsx | 5 |
| Stack | primitives/Stack.jsx | 8 |
| Surface | primitives/Surface.jsx | 55 |
| Switcher | primitives/Switcher.jsx | 17 |
| Table | primitives/Table.jsx | 8 |
| Text | primitives/Text.jsx | 40 |
| BookTile | surfaces/BookTile.jsx | 13 |
| Callout | surfaces/Callout.jsx | 7 |
| Card | surfaces/Card.jsx | 9 |
| Drawer | surfaces/Drawer.jsx | 7 |
| HelpCard | surfaces/HelpCard.jsx | 10 |
| Modal | surfaces/Modal.jsx | 7 |
| ScripturePassage | translation/ScripturePassage.jsx | 14 |
| VerseMarker | translation/VerseMarker.jsx | 8 |
| WordChip | translation/WordChip.jsx | 6 |
| Tokens | tokens/*.css | 36 |

The 36 token cases: one swatch page per section of `colors.css` (10), the nine tone slots of
each of the seven tones on a light ground and on the Ocean ground (14), the context slots light
and dark (2), the UI and scripture type scales, the weights, every Text role, the font families,
spacing, radius, stroke, elevation and control heights (10). `motion.css` and `layer.css` hold
durations and z-indices, which a still image cannot show, and `print.css` styles only the print
document.

## Skipped, and what to know

No component was skipped: every file in `src/ds/components` has cases. These parts are not
cases of their own:

- `Layer` with `placement="anchor"`: it needs a ref to an anchor element, which JSON cannot
  carry. Menu, Tooltip, Select and SuggestField anchor their own Layers and cover it, in both
  directions.
- `Table`'s `cell` and `rowKey` are render functions. The table cases use the default cell,
  which is a `Text` of the row's value.
- `FieldContext`, `useField`, `FieldGroupContext` and `useFieldGroup` are a context and a hook,
  not components. The Field and FieldGroup cases exercise them.
- Behaviour that a still image cannot show: Escape and outside-press dismissal, focus trapping,
  roving keyboard focus past one step, dragging a VerseMarker or a WordChip, and the Layer enter
  and exit motion. The spinning ring and the sliding bar are captured still, at their start.
- The Hindi interface: Devanagari has no bundled face, so it would draw in whatever the machine
  has.

Rendering that depends on the machine, not on tC4:

- Mulish and Charis SIL have no glyph for ✕ ✓ ▾ ▴ ★ ☰ → ← ≠. Close buttons, check marks, the
  Select chevron and the arrows in labels such as "Create Bible →" draw in a fallback font that
  Chromium picks from the machine's fonts (this machine has DejaVu, Free, Inter, Liberation and
  IPA faces). tC4 has the same dependency.
- `--font-mono` (the `meta` Text role) names no bundled face. It draws in the machine's
  monospace font.
- This machine has Awami Nastaliq Regular installed (`/root/.fonts`), so `local("Awami
  Nastaliq")` in `fonts.css` picks it before the bundled `.ttf`. The two files are identical
  (same MD5), so the images do not change. Chromium has no Graphite, so Nastaliq draws unjoined
  in every Nastaliq case. tC4 ships in Electronite, which joins it.
- The DS relies on `text-box: trim-both cap alphabetic` (headings and controls) and
  `font-size-adjust` (Greek, Hebrew and Arabic scripture). Chromium 141 has both. A webview
  without them draws different line boxes.

Sample text: interface strings come from tC4's English and Arabic catalogs where they exist there.
Scripture lines are illustrative samples in Latin, Greek, Hebrew, Arabic and Urdu script, not
quotations of a pinned resource.
