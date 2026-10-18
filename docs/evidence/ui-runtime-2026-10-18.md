# The UI runtime's boot and language switch (2026-10-18)

Unit A4a (docs/APP-DESIGN.md §6.2, §6.9, §8.3). This measures what the runtime adds before and
after the first screen: the time from the start of the page's load to the moment the root is
mounted, the size of what the page fetches, and the cost of changing the interface language.
PERFORMANCE.md has no budget for the first two; its rule 3 (nothing blocks the UI thread) and
the 16 ms per frame of its "UI" rows apply to the third. A5 is the first unit that puts a
screen on this runtime, so these numbers are the starting point it measures against.

**What this is not.** It is headless Chromium on Linux, on a loopback server, with the runtime's
stand-in root (`crates/tc-ui/src/app/web/root.rs`: a sample line, the language picker and the
internet line). It has no Electronite, no Windows, no Defender, no GPU, and no Home screen.
The language switch changes three text nodes and one attribute; a full screen has many more.

## The figures (release)

Twenty loads, each in a new browser context, for the boot; twenty switches of the language,
alternating Arabic and English, in one page. Nearest-rank percentiles.

| What | Median | **p95** | Max |
|---|---:|---:|---:|
| Boot, ms from the start of the navigation, to: | | | |
| the page's HTML, styles and `boot.js` loaded (`DOMContentLoaded`) | 91.7 | 111.2 | 116.9 |
| the wasm compiled and `run_app` started (`tc4:boot`) | 131.9 | 162.8 | 174.0 |
| `hello` answered (`tc4:hello`) | 198.7 | 243.4 | 253.5 |
| the nine catalogs fetched (`tc4:catalogs-fetched`) | 203.0 | 243.7 | 254.6 |
| the catalogs parsed and installed (`tc4:catalogs`) | 222.0 | 273.4 | 276.5 |
| **the root mounted (`tc4:mounted`)** | **229.1** | **278.1** | 285.2 |
| Of that, the parse of the nine catalogs | 19.0 | 28.4 | 32.8 |
| Language switch, ms from the `change` event to: | | | |
| the sample line's new text in the DOM | 0.3 | 0.4 | 0.5 |
| the first animation frame after that | 16.6 | 17.2 | 19.6 |

Long tasks over 50 ms (the Long Tasks API) during the twenty switches: none.

| Bytes | Release | Dev |
|---|---:|---:|
| `tc_ui_bg.wasm` | 1,166,565 | 8,117,569 |
| `tc_ui.js` (wasm-bindgen's glue) | 42,332 | 43,699 |
| the whole `ui/<build>/` folder (fonts, styles, catalogs, images) | 4,948,176 | 11,900,547 |
| fetched by the page at boot (32 requests; the 29,968 bytes of fonts it loads are left out) | 2,151,284 | 9,103,655 |

The same twenty-load, twenty-switch run on the dev build, for comparison: root mounted at a
median 329.0 ms (p95 418.3), catalog parse 58.7 ms (p95 80.6), switch to the DOM 0.8 ms (p95 1.1),
no long task. Raw samples' summaries are in `ui-runtime-2026-10-18/results-release.json` and
`results-dev.json`.

## Reading

- **The switch costs 0.3 ms.** The runtime reads the shown language in every `t()`, so a
  switch updates the text nodes that read it, in place, and nothing is built again. The
  "16.6 ms to the next frame" is the display's frame period (60 Hz), not work: the update was
  done 16 ms before the frame callback ran. The budget of 16 ms per frame is met with 15 ms to
  spare, here. That is three readers, not a screen: A5 must measure M16's preview on Home and
  on the Check screen, where thousands of `t()` readers exist, against the same 16 ms. If a
  full screen's switch is over 16 ms, the readers' number and the reactive flush are the first
  places to look, before catalog parse (which happens once, at boot, not at a switch).
- **The first Arabic.** The browser loads the Arabic face of tC4's fonts the first time Arabic
  shows (`noto-sans-arabic-arabic-wght-normal.woff2`, listed by the smoke and not counted as the
  app's request). The one sample of that switch took 1.5 ms to the DOM and 1.6 ms to the next
  frame. Earlier runs of the same measure gave 9.7 to 83.3 ms to the frame for that first
  switch, at load averages from 3 to 10. It is one sample per run and depends on the load and on
  where in the frame the dispatch falls, so it is not a figure to hold a budget to; it never
  produced a long task in a release build.
- **The boot, and the proposed 3 s repeat launch.** The root is mounted at a median 229 ms and
  p95 278 ms after the navigation starts. That is 8% and 9% of 3 s, before Electronite's
  process start, `tc4`'s start and the list of projects, which are in the 3 s and not here, and
  on a machine whose speed relative to the reference laptop (i5-8350U) is not known. The time
  goes, at the median: 92 ms to load the page and its script; 40 ms to fetch and compile
  1.17 MB of wasm; 67 ms from the wasm's start to `hello`, because the nine catalog fetches
  are issued first and share the browser's six connections with the event stream and with
  `hello` (the stream's head must come before `hello`, D4); 4 ms for the last catalog; 19 ms
  to parse the nine catalogs (870,974 bytes of JSON into hash maps); 7 ms to mount.
- **The catalog parse is the runtime's own cost at boot**, 19 ms at the median and 28 ms at p95
  in release, 59 ms and 81 ms in the dev build. tC4 parses its ten catalogs when it loads too.
  It could be cut to the shown language (the others parsed when M16 first previews them) if the
  3 s needs room; APP-DESIGN §6.9 keeps "all ten at boot" for the preview's sake (A-23), and a
  lazy parse would keep that: the text is fetched at boot, only the parse is deferred. Not done,
  because it is under 9% of the boot and no budget asks for it.
- **The wasm is 1.17 MB in release** (7.8 MiB dev) with panics set to abort and size
  optimisation, and no compression: the server sends it as it is. The fonts, the styles and the
  catalogs are most of the 4.9 MB folder. Nothing but the wasm and the catalogs is fetched at
  boot among the large files; the fonts load when a face is first used.

## The run

- Commit: the commit that adds this file. The Rust code and assets of the measured builds are
  those of that commit (after the builds, only `smoke.mjs`'s cases and the docs changed):
  `crates/tc-ui/build-app.sh --release` gave build `6e75a0757ee8e4a8`, and
  `crates/tc-ui/build-app.sh` (dev) build `309fe071cf73d0ea`. The server is `tc4`
  built with `cargo build --release -p tc4 -j 2` from the same base (phase0 `ac63e9c` and this
  branch's tc-ui); the dev results use the same release server.
- Command: `node crates/tc-ui/app/smoke.mjs --tc4 target/release/tc4 --ui target/tc-ui-app-release
  --measure 20 --out results-release.json`, and the same with `--ui target/tc-ui-app`. The smoke
  starts `tc4 serve` on a new app-data folder and an existing empty projects folder under
  `target/`, adds the token to the requests outside `/api/` as the shell does (§5.2), and
  quits it at the end.
- Machine: Intel Xeon @ 2.10 GHz, 4 vCPU container, with SHA-NI and AVX-512F, shared with other
  agents' builds. Load average (1, 5, 15 min) before and after the release run:
  2.43, 3.52, 4.68 and 2.74, 3.55, 4.68; for the dev run: 2.74, 3.55, 4.68 and 3.09, 3.60, 4.69.
  Runs under a load of 8 to 10, taken earlier the same day, were 20% to 40% slower at the
  median boot, and the dev build's switch showed an 82 ms long task in them; they are not used
  here.
- Chromium 141.0.7390.37 (build 1194), headless, Playwright 1.56.1. `performance.now()` is in
  milliseconds with 0.1 ms resolution in a page that is not cross-origin isolated.
- Definitions. A mark is `performance.mark`, in milliseconds from `performance.timeOrigin`.
  A switch is timed in the page from the dispatch of the select's `change` event to a
  MutationObserver callback on the sample line (the DOM update), and from there to the next
  `requestAnimationFrame`. Two switches (Arabic, English) that are not timed come first, and
  the first one's time is reported apart.
- A rule 5 note: this is a release build of the wasm and of `tc4`, 20 runs, median and p95, the
  machine and the commit. CI does not gate these numbers; PERFORMANCE has no budget for them.
