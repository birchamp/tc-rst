# Judge report: the Leptos port of tC4's design system

Branch `worktree-wf_ed7beb1a-ead-2` at `2b76db2`, judged on 2026-10-15 against tC4 at
`d494a9d6ba9f4cdcc09faff9038c93159106045b`.

## Verdict: fix first

The port's pixel and DOM claims are true. I rebuilt everything from clean and measured them
myself. Every case I drove with the keyboard and mouse behaves like tC4, except one: a
controlled RadioGroup. The code is careful and well commented.

Three things should be fixed before merge:

1. CI never compiles the 6,510 lines of components.
2. Several props take only a string, where tC4's own screens pass elements.
3. Some state changes rebuild DOM nodes that React keeps. Because of this, transitions do not
   run, and `Table` can drop the keyboard focus.

The remaining findings are minor.

## What I ran

| Check | Result |
|---|---|
| Dev build from clean (`target/wasm32-unknown-unknown` and `target/tc-ui-gallery` deleted) | 1 min 43 s |
| Release build from clean (`wasm-release`) | 2 min 12 s. The wasm is 1,017,408 bytes, or 327,127 bytes with gzip -9. Both numbers match the report exactly. |
| Baseline recapture (`npm run capture`) | 702 cases, 0 failed, 203 s. `git status` stays clean, so `screens/` and `gallery-spec.json` are byte-identical to the committed files. |
| Rust capture, dev build (`screens-rust/` deleted first) | 702 cases, 0 failed. **702 of 702 at 0 mismatched pixels.** |
| Rust capture, release build (`screens-rust/` deleted first) | 702 cases, 0 failed. **702 of 702 at 0 mismatched pixels.** |
| `cmp` of every `screens-rust/` PNG with its `screens/` PNG | 702 of 702 byte-identical, for both builds |
| `dom-diff.mjs` | **702 of 702 cases have the same DOM.** `dom-report.json` comes out byte-identical to the committed one. |
| Sensitivity: Badge md padding changed from `2px 7px` to `2px 8px`, then reverted | 9 of 16 Badge cases differ, by 279 to 1,127 pixels. The DOM diff reports the same 9. The harness catches a 1 px change. |
| Console warnings and errors while drawing all 702 cases | 0 on tC4's side, 0 on the port's side |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` (host) | pass |
| `cargo test --workspace` | pass: 21 `tc-ui` tests and 4 `tc-core` tests |
| `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets [--features gallery] -- -D warnings` | pass, with and without `gallery` |
| Line counts | 9,936 lines of Rust: 6,510 in `ds`, 1,678 in the gallery, 1,748 host-testable. The scripts are 444 lines. All match the report. |
| Copied assets | All 69 files under `assets/ds` are byte-identical to tC4's `src/ds`, and so are both logos |

**Worst 10 cases.** No case differs, so no case ranks worst. Instead I put ten of the hardest
cases side by side and looked at each pair: `Select/open-keyboard`, `Select/open-groups`,
`Modal/rtl`, `Drawer/rtl`, `Menu/open-rtl`, `Tooltip/rtl-end`, `SuggestField/rtl-open`,
`ScripturePassage/nastaliq`, `AppHeader/project-name-rtl` and `Tokens/type-roles`. In every pair
the two images are the same, and `ImageChops.difference(...).getbbox()` is `None`. The renders
are real content, not blank frames. Nastaliq is drawn unjoined on both sides, as the README says.

## Components and props

All 52 component files have a port, which covers all 53 exports, `FieldContext`/`useField` and
`FieldGroupContext`/`useFieldGroup`. tC4's `Row` is an internal export of `Select.jsx`. It is
ported as `select::row`.

A script compared the props each React component destructures with the props of the matching
Rust `#[component]`. No React prop is missing, apart from two:

- `Layer.container`. tC4 never sets `window.ReactDOM`, so its Layer never portals, and the port
  renders in place too.
- `Modal.zIndex`. tC4 accepts it and ignores it.

The props found only in Rust are React's `...rest` written out (`value`, `on_change`,
`placeholder`, …).

The prop *types* are narrower in places. See finding 2.

## Interactive behaviour

I wrote a scenario harness. It mounts a case on both galleries by the capture protocol, runs a
key and mouse sequence, and after every step takes the frame's DOM snapshot (as `dom-diff.mjs`
does), `document.activeElement` and `body.style.overflow`. It ran 22 scenarios, 133 steps in all.

| Scenario | Steps | Result |
|---|---|---|
| `Menu/closed`: click to open, ArrowDown/Up, End, Home, typeahead, Escape, outside click, Tab | 13 | same |
| `Menu/open`: arrows, End, typeahead, Escape | 5 | same |
| `Select/value`: Tab, ArrowDown to open, arrows, End/Home, typeahead, Enter, Space, Escape, Tab out, clicks, row hover, row click | 18 | same |
| `Select/open-search`: click, type "jo", arrows, Home in the search field, Backspace, Enter, Escape, outside click | 11 | same |
| `Select/open-groups`: type, arrows, Tab out | 5 | same |
| `SuggestField/value`: arrows, Enter, Escape, typing into a controlled value, Tab | 11 | same |
| `Switcher/underline` and `Switcher/disabled-option`: roving focus with arrows, Home, End | 10 | same |
| `Modal/default`, `Drawer/default`, `ConfirmDialog/default`: Tab and Shift+Tab focus trap, Escape, scrim click, scroll lock | 22 | same |
| `Tooltip/resting`: hover, leave, focus, blur, with timed snapshots | 8 | 8 of 9 snapshots the same. The odd one was taken 0 ms after the hover, and the port was one frame ahead. Sampling every frame shows the same enter and exit curves (opacity and transform) on both sides. |
| `Accordion/closed`: click, Enter, Space | 7 | same |
| `Table/row-hover`: Tab, Enter, Space | 5 | same |
| `Checkbox`, `Toggle`, `TextField`, `SearchField`, `Card`: clicks and typing on controlled and uncontrolled values | 15 | same. A rejected change is undone as React does it. |
| `RadioGroup/legend`: click the other radio while `onChange` ignores it | 2 | **differs** (finding 4) |

Separate probes, all the same on both sides:

- Clicking the Select and Menu triggers four times opens and closes the list in turn.
- The enter transitions of Tooltip, Modal and Drawer, sampled every frame.

Toast timing does not apply here. tC4's Toast has no timer: "the queue — adding, timing out,
capping the stack — is application policy and is not here".

I also read `layer.rs` against `Layer.jsx` line by line. The escape stack ordered by depth, the
outside-press stack with the scrim's mousedown rule, the focus trap, focus restoration, the
scroll lock, roving navigation with typeahead, and the measure-then-flip placement all match.
Leptos is built without the `delegation` feature, so `on:` handlers attach to the elements
themselves. Because of that, the field's `stopPropagation` on mousedown still runs before the
document listener, as it does in React. The toggle probe confirms it.

Neither harness covers nested layers: Escape or an outside press with a menu open inside a
dialog.

## Code quality

Strengths:

- Typed keyword enums with `parse` at the gallery boundary.
- The `Css` builder copies React's spread and unit semantics and has tests.
- Placement and concentric-corner arithmetic are pure functions with host tests.
- No `unwrap` or `expect` in library code.
- Comments explain reasons.
- The CSS builder is used consistently: 170 `css!` blocks. The 17 static `style="…"` literals are
  constant styles, and the DOM diff checks them.

Weaknesses: findings 5 to 7.

## Required fixes

### 1. Major: CI never compiles the components

`crates/tc-ui/src/lib.rs:20-21` compiles `ds` (and `gallery`) only for `wasm32`. CI
(`.github/workflows/ci.yml:20-22`) runs only fmt, host clippy and host tests.

To prove it, I appended `pub fn broken() -> u32 { "not a number" }` to `ds/core/mod.rs`. Host
clippy and `cargo test -p tc-ui` still passed. `cargo check --target wasm32-unknown-unknown`
failed with E0308. Then I reverted the change.

**Fix:** add these steps to `ci.yml`:

- `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`
- the same command with `--features gallery`

The target is already installed there. Say in AGENTS.md or the README that this check is part
of the pre-commit gate.

### 2. Major: slot props are narrowed to strings that tC4's screens fill with elements

These props are `String` or `Option<String>` in the port:

- `OptionCard` `title`, `description` and `trailing` (`forms/mod.rs:233-238`)
- `Modal` `title` and `subtitle` (`surfaces/mod.rs:398-399`)
- `Drawer` `title` (`surfaces/mod.rs:247`)

In tC4 all of them are rendered as nodes, and tC4's own screens pass elements:

- `ProjectSettings.jsx:46-47`: `title={<span dir={g.dir}>…</span>}` and
  `trailing={current ? <Badge …/> : '→'}`
- `GuidedFix.jsx:46`: `description={<span style={mono}>…</span>}`
- `SourceTexts.jsx:28-31` and `:46-48`: element `title`, `description` and `trailing`
- `AddBook.jsx:95`: `subtitle={<>… <strong …>…</strong></>}`

`Drawer.jsx` even handles a title that is not a string (`typeof title === 'string' ? title :
'Panel'`). The port cannot express these screens, so its API will have to change when they are
ported.

**Fix:** take content for these slots, for example a `Content` enum of text or a `ViewFn`, with
`From<&str>` and `From<String>` so string call sites stay the same. Add gallery cases that pass
an element.

### 3. Major: state changes rebuild DOM nodes that React keeps

Several components wrap a subtree in `move || view! {…}`. When the state changes, they draw a
new element where React keeps the old one and changes its style. The pixel and DOM harness
cannot see this. The user can.

- **Accordion chevron** (`data/mod.rs:77-83`). Measured frame by frame: tC4 rotates it from 0°
  to 90° over about 140 ms. The port shows a new node already at 90° in the first frame.
- **Toggle knob slide and Radio dot** (`primitives/choice.rs:130-165`, the `inner` closure).
  `transition: inset-inline-start …` never runs. tC4's CommunityChecking uses `Toggle`.
- **Progress ring arc** (`primitives/progress.rs:47`, the `mark` closure). The
  `stroke-dashoffset` transition never runs. The bar is fine, because its `fill` style is
  reactive on a node that stays.
- **Table** (`primitives/table.rs:150-163`). The `body` closure rebuilds every `<tr>` whenever
  `rows` or `selected_key` changes. When a keyboard user presses Enter on a focused row and the
  owner selects it, the focused `<tr>` is replaced and the focus falls to `<body>`. React's keyed
  rows keep it. tC4's screens do not use `Table` yet.

**Fix:** keep these nodes and make their `style` and attributes reactive. For Table, use a keyed
`<For>` with per-row reactive `data-selected`, `aria-selected` and `style`. Add a test that
toggles a state and checks that the node and the focus stay.

### 4. Minor: a rejected RadioGroup change leaves no radio checked

In `RadioGroup/legend`, click "rtl" while `onChange` ignores it. The native inputs then read:

- tC4: `ltr=true, rtl=false`
- port: `ltr=false, rtl=false`

`choice.rs:193` restores only the input that fired `change`. The browser unchecked the other
input of the group, and nothing restores it. The drawn marks still show "ltr", because they
follow the prop, so the accessibility tree and arrow-key navigation disagree with what is drawn.

**Fix:** restore every input of the group, as React's `restoreControlledState` does for named
radios. For example, RadioGroup can re-apply `checked` to each of its Choices on any change.

### 5. Minor: props-struct literals repeat every default

40 call sites build a full `XProps { … }` literal. Among them are 6 `LayerProps` of about 25
fields each, 9 `SurfaceProps` and 7 `ActionProps`. Each literal repeats the component's
defaults, such as Layer's `offset: 6.0`, `animate: true`, `restore_focus: true` and
`scrim_attrs: Attrs::new()`. Four of the six `#[allow(clippy::too_many_arguments)]` come from
the same pattern. A new prop or a changed default means editing every site, and a copied default can drift from the component's
`#[prop(default)]`.

**Fix:** use `#[prop(optional_no_strip)]` or `into` where a caller must forward an `Option`, so
`view!` can be used. Otherwise add small constructors that keep the defaults in one place, such
as `LayerProps::anchored(anchor)`.

### 6. Minor: `build-gallery.sh` ignores `CARGO_TARGET_DIR`

Line 42 binds `$ROOT/target/wasm32-unknown-unknown/$DIR/tc_ui.wasm`, but cargo writes to
`$CARGO_TARGET_DIR` when that variable is set. The script then binds a missing or stale wasm,
and the parity run passes or fails on old code.

**Fix:** use `${CARGO_TARGET_DIR:-$ROOT/target}`, or read `target_directory` from
`cargo metadata`.

### 7. Minor: the harness has gaps

- **No interaction parity.** The 702 cases are a still image and a DOM after at most three
  steps. The 22-scenario run above found finding 4, which they missed.

  **Fix:** add a scenario harness to `parity/ds-gallery`: multi-step sequences, with a DOM,
  focus and scroll-lock snapshot after each step on both galleries. Include nested layers
  (Escape and outside press with a Menu or Select inside a Modal).
- **`dom-diff.mjs --only` overwrites the full report.** It writes `dom-report.json` with only
  the subset (line 99), where `diff.mjs` merges. A partial run overwrites the committed full
  report.

  **Fix:** merge as `diff.mjs` does.
- **`crates/tc-ui/assets/ui.css` is unused.** It is copied, but nothing loads it.

  **Fix:** drop it until the app uses it.

## Not required

- Hindi and Devanagari, motion beyond the enter and exit curves, and a machine with other
  fonts. The README already lists these.
- The 29 MB dev wasm and the missing `wasm-opt` pass. Both are fine for a gallery.
