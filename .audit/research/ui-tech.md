# UI technology for the Rust rebuild of tC4 — a Graphite-first evaluation

Date: 2026-10-14. Machine for the probes: Ubuntu 24.04.5 x86_64 (container), Rust 1.97.0.
Scope: research only. I changed nothing in any repository. The probes are throwaway code under
`/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/ui-probe/`.

Tags: **[MEASURED]** = run this session; **[SOURCE-READ]** = read in source code this session;
**[CITED]** = from a web source listed at the end; **[tC4]** = from tC4's own evidence records;
**[UNVERIFIED]** = from memory or inference, not checked this session.

---

## Recommendation

**Choose option (a): a Rust core, plus a Rust-to-WASM web UI (Leptos in client-side mode, or
Dioxus web), served on localhost and shown in Electronite.**

- This option is the only one that has proven, correct Graphite rendering today, and it has the
  path to all three OSes (D20, `graphite-shaping-packaged-2026-08-14.md`).
- Editing quality comes from Chromium no matter which UI framework you use. With a Rust UI, you
  give up some development speed but none of the text correctness.
- Testing stays on Playwright and CDP, which tC4 already uses.
- Packaging stays on the existing Electronite pipeline.
- "Done in Rust" covers the core and the UI logic. The only non-Rust code is the Electronite
  shell (Chromium C++) and a small Electron main script.

Keep the shell boundary thin: the UI talks to the core over HTTP, SSE or WebSocket only. A
small preload exposes printToPDF and the file dialogs. With that boundary, you can later swap
Electronite for **Tauri plus a Graphite-patched CEF** (an all-Rust host) without rewriting the
UI. Electronite's Graphite change is a Chromium-level patch, so it ports to CEF.

**Reject Tauri with the system webviews.** It fails Graphite on Windows (WebView2 is Chromium)
and on macOS (WKWebView shapes with CoreText). On Linux it is subtly wrong, which I measured
this session: WebKitGTK engages Graphite but **discards the vertical glyph offsets**. Nastaliq
renders on a flat baseline, and even ordinary OpenType Arabic diacritics are misplaced, so
بِ looks like پ.

**Do not build a fully native Rust UI for the first release.** Graphite shaping in a Rust binary
is easy; I proved it this session with a static HarfBuzz + graphite2 build through the `cc`
crate. But you would have to fork parley or cosmic-text, build a bidi/IME-quality editor, a PDF
typesetter that also shapes Graphite, spell check and accessibility. Every one of those already
works in Chromium.

---

## What was measured this session

| # | Probe | Result |
|---|---|---|
| P1 | System HarfBuzz (Ubuntu 24.04, 8.3.0) through ctypes, Awami Nastaliq, the Urdu sample from tC4's evidence | Compiled shapers `['graphite2','ot','fallback']`. Graphite: 31 glyphs, total advance 15149, y offsets −477…3500. Forced `ot`: 23 glyphs, advance 32366, y = 0. The ratio is about 0.47, the same discriminator as tC4's 390 px against 770 px. [MEASURED] |
| P2 | `hbgr`: a pure Cargo crate that compiles graphite2 (master `ca8d821`, 2026-06-01, the same 30 `.cpp` files Electronite uses) and HarfBuzz 12.1.0 (`harfbuzz.cc`, `HAVE_GRAPHITE2`, `GRAPHITE2_STATIC`) with `cc` | Builds in 68 s. The binary is 1.67 MB, statically linked: `ldd` shows no harfbuzz or graphite2. The default `hb_shape_full` picks graphite2, and the output is identical to P1. [MEASURED] |
| P3 | WebKitGTK 2.52.6 (`libwebkit2gtk-4.1-0`, the library Tauri and wry use on Linux) under Xvfb. Awami as a web font, with a control copy whose `Silf` table tag is renamed | Span width 378 px against 758 px in the control: Graphite engages. **But the cascade is flat**: canvas ascent is 58 px (Electronite: 86 px), and the Awami ink height is 67 px against Pango's 100 px. [MEASURED] Screenshot: `webkitprobe/webkitgtk-awami.png` |
| P4 | The same text in WebKitGTK and in Pango 1.52.1 (GTK's own HarfBuzz text stack), compared pixel by pixel. DejaVu Sans `سِ بِ` (OpenType marks with y offsets) and Awami | DejaVu ink height: WebKit 61 px, Pango 80 px (the widths match at 234 px). The kasra rides up into the base: **بِ renders like پ**. Awami ink height: WebKit 67 px, Pango 100 px. Pango renders the correct Nastaliq cascade. [MEASURED] Screenshot: `webkitprobe/side-by-side.png` (left: Pango, right: WebKitGTK) |

The root cause of P3 and P4 [SOURCE-READ]: WebKit's Skia port builds horizontal glyph runs with
`SkTextBlobBuilder::allocRunPosH`, which stores x positions only. Each glyph's y component is
computed (in `ComplexTextControllerSkia.cpp` and the shared `ComplexTextController.cpp`) and
then dropped when the run is built. The code was in `FontCascadeSkia.cpp` from the Skia switch
in **webkitgtk-2.46.0** through 2.50. In 2.52 it moved to `FontSkia.cpp::buildTextBlob`, and it
is still there on `main`. An older bug of the same kind in the Cairo/FreeType era was WebKit
bug 161493 ("Y offsets returned from HarfBuzz are discarded"). [CITED]

**Method lesson for tC4's own probe.** In P3, WebKitGTK passes a width-only Graphite test. tC4's
`docs/evidence/tools/graphite-probe.js` record also measured ascent (86 against 58), and that
measurement would catch this failure. Make the vertical-extent assertion mandatory, and add an
OpenType mark case such as `بِ` in any font. Then run the probe on all three OS artifacts. Today
only macOS arm64 is measured [tC4].

---

## 1. Which web engines shape Graphite today

| Engine (where it ships) | Shaping path | Graphite? | Evidence |
|---|---|---|---|
| **Gecko** (Firefox) | Its own `gfxGraphiteShaper` on graphite2, sandboxed with RLBox (wasm2c) since Firefox 74/75 and widened in Firefox 95 | **Yes**, on all desktop OSes | [CITED] Mozilla Hacks, RLBox. A WebKit bug says Awami "only works currently on Gecko browsers". Gecko cannot be embedded through wry or Tauri: wry supports only WebView2, WKWebView, WebKitGTK and Android [SOURCE-READ wry README] |
| **Chromium / Blink** (Chrome, Edge, **WebView2**, stock Electron, stock CEF) | Bundled HarfBuzz built without `HAVE_GRAPHITE2`. Blink's OTS web-font sanitizer also drops the `Feat/Glat/Gloc/Silf/Sill` tables | **No** | [tC4] The Playwright Chromium 151 negative control rendered unjoined Awami (770 px). [SOURCE-READ] Electronite's `patches/chromium/add_graphite.patch` exists to add exactly those three things |
| **Electronite** (unfoldingWord's Electron fork) | Chromium with a 210-line patch: a graphite2 GN target pinned in `DEPS` (`silnrsi/graphite@92f59dc`), `hb-graphite2.cc` plus `HAVE_GRAPHITE2` in `third_party/harfbuzz-ng/BUILD.gn`, and the Graphite tables allowed in `web_font_decoder.cc` | **Yes** | [tC4] Measured in the packaged macOS arm64 app, v37.1.0-graphite. [SOURCE-READ] patch at tag `v39.2.6-graphite` |
| **WebKitGTK** (Linux; Tauri, wry, Dioxus desktop) | System HarfBuzz: Ubuntu's `libwebkit2gtk-4.1` links `libharfbuzz` and `libgraphite2`. A plain `hb_shape` puts graphite2 first | **Engaged but rendered wrongly**: per-glyph y offsets are dropped (2.46 and later) | [MEASURED] P3 and P4. [SOURCE-READ] `SkiaHarfBuzzFont.cpp` builds the face from the whole font stream, so `Silf` is reachable |
| **WKWebView** (macOS; Tauri, wry) | Cocoa WebKit shapes complex text with CoreText (`ComplexTextControllerCoreText.mm`). CoreText has no Graphite engine | **No** | [SOURCE-READ] WebKit source. [CITED] WebKit bug 241717 (Gecko-only; Safari also has Nastaliq positioning problems). Not measured: no Mac here |
| **Servo** | `harfbuzz-sys 0.8` with `features = ["bundled"]` (`components/fonts/Cargo.toml`). The bundled build never defines `HAVE_GRAPHITE2` (P2 background) | **No** | [SOURCE-READ] |
| **Blitz** (Dioxus native) | parley, then harfrust | **No** | [SOURCE-READ] crate dependencies |
| *Reference: Pango/GTK* (not a web engine) | System HarfBuzz | **Yes** on Linux, with correct rendering | [MEASURED] P4 |

---

## 2. Rust GUI options and their text stacks

Versions are the stable releases on crates.io, queried 2026-10-14 (latest prerelease in brackets).

| Option | Text stack | Bidi | Editable text | IME | Accessibility | Graphite | Maturity |
|---|---|---|---|---|---|---|---|
| **Tauri 2 / wry** (tauri 2.12.4 [3.0.0-alpha.5], wry 0.57.0) | The system webview: WebView2, WKWebView or WebKitGTK | Engine-grade | Engine-grade (contenteditable, textarea) | Yes | Yes (the platform webview) | **No** on Windows and macOS. Wrong vertical offsets on Linux (§1) | Mature |
| **Tauri + CEF runtime** (tauri-runtime-cef 3.0.0-alpha.6, **no stable release**; cef 154.5.0+154.0.34) | Chromium | Chromium | Chromium | Yes | Yes | **Only with a self-built CEF** carrying Electronite's patch. `cef-dll-sys` honours `CEF_PATH` [SOURCE-READ build.rs] | Alpha, tied to Tauri 3 alpha. Gaps such as navigation APIs were still open in March 2026 [CITED] |
| **Dioxus desktop** (0.7.10 [0.8.0-alpha.2]) | wry, so the same as Tauri | as Tauri | as Tauri | as Tauri | as Tauri | as Tauri | Good |
| **Dioxus native / Blitz** (blitz-dom 0.2.4 [0.3.0-beta.3]) | parley (harfrust), Vello | Yes | Basic inputs | via winit | AccessKit (optional) | No | Beta |
| **Leptos / Yew** (leptos 0.8.22 [0.9.0-beta2], yew 0.23.0) | Whatever webview hosts them | engine | engine | engine | engine | **= the host engine** (Electronite: yes) | Leptos is active and mature on the web. Yew is slower-moving |
| **egui / eframe** (0.36.2) | epaint: harfrust and skrifa (harfrust since 0.35, "better kerning and ligatures") | **No bidi crate in its dependencies**, so I infer no bidi reordering [SOURCE-READ deps]. PR #5547 fixed TextEdit in RTL *layouts* only | Plain `TextEdit` | Improved in 0.35 | AccessKit always on | No; needs a fork of epaint shaping | Mature for tools. An immediate-mode GUI is a poor fit for a translator's document editor |
| **iced** (0.14.0, 2025-12-07) | cosmic-text 0.15: harfrust, unicode-bidi, swash | Yes (cosmic-text) | `text_editor` widget | **New in 0.14** (#2777), several preedit fixes | **None**: no AccessKit dependency, nothing in the changelog | No; needs a cosmic-text fork | Pre-1.0. Fixes ship about once a year |
| **Slint** (1.18.1, 2026-09-21) | parley (`shared-parley`, on by default) with femtovg, Skia or software renderers | Yes (parley) | `TextInput`, `TextEdit` | Yes (`input-method-hints`) | AccessKit; 1.18 exposes input text and selection | No; needs a parley fork | Most production-ready of the native options. Licence: GPLv3, royalty-free with attribution, or commercial |
| **Xilem / Masonry** (0.4.0, 2025-10-29) | parley (harfrust, ICU4X), Vello | Yes | `PlainEditor`-based text area | via winit | AccessKit | No; needs a parley fork | Experimental. One release a year |
| **gpui** (0.2.2) | CoreText (macOS), DirectWrite (Windows), cosmic-text (Linux) | Platform-dependent | Zed's editor model; RTL editing not supported [UNVERIFIED] | Yes | **None**: no AccessKit dependency | No (platform shapers) | Built for Zed. Little general-purpose documentation |
| **Makepad** (makepad-widgets 1.0.0, 2025-05-13) | makepad-rustybuzz 0.8, unicode-bidi, SDF glyphs | Partial | Own widgets | Partial [UNVERIFIED] | **None**: no AccessKit dependency | No | Niche |
| **Freya** (0.4.3 [0.5.0-rc.10]) | Skia `SkParagraph` (freya-skia-safe) with Skia's bundled HarfBuzz | Yes (SkParagraph/ICU) | freya-edit, a rope-based editor | via winit | AccessKit | Only on Linux, through `SKIA_USE_SYSTEM_LIBRARIES` (skia-bindings links the system harfbuzz on Linux only) | Small project, single maintainer |
| *Not on the list: gtk4-rs / Relm4* | Pango with HarfBuzz | Yes | `GtkTextView`, mature | Yes | AT-SPI on Linux. Windows and macOS only through the **opt-in** AccessKit backend from GTK 4.18 [CITED] | **Yes on Linux** [MEASURED P4]. Probably yes on macOS and Windows if you ship Homebrew or MSYS2 HarfBuzz, which both enable graphite2 [SOURCE-READ formula and PKGBUILD]. gvsbuild's MSVC HarfBuzz does not | Mature on Linux; second-class on Windows and macOS. The toolkit is in C |

---

## 3. Getting Graphite into a Rust-native stack

| Route | Finding |
|---|---|
| `graphite2` / `graphite2-sys` crates | **Do not exist** on crates.io (`cargo info`: not found). The `graphite` crate is an unrelated metrics library. **There are no Rust Graphite bindings and no pure-Rust Graphite engine.** [MEASURED] |
| `harfbuzz-sys` 0.8.0 (servo/rust-harfbuzz, 2026-08-04) | Features: `coretext`, `directwrite`, `freetype`, `bundled`. **`bundled`** compiles vendored HarfBuzz **8.4.0** without `HAVE_GRAPHITE2`, so graphite2 is compiled out. Without `bundled`, it uses `pkg-config harfbuzz`, the system library: Linux distros have graphite2 (P1), Windows has no system HarfBuzz, and macOS has none outside Homebrew. [SOURCE-READ build.rs] |
| `harfbuzz_rs` 2.0.1 (2021) and `harfbuzz_rs_now` 2.3.2 | Only `build-native-harfbuzz` options; no Graphite switch. [MEASURED cargo info] |
| `harfrust` 0.14.0 and `rustybuzz` 0.20.1 (pure Rust) | OpenType only. rustybuzz's `wasm-shaper` runs a **font-embedded `Wasm` table** (HarfBuzz's wasm-shaper). Awami and Padauk have no such table: Awami's tables are `Feat GPOS Glat Gloc OS/2 Silf Sill Silt …`, so this is not a route for stock SIL fonts. [SOURCE-READ, MEASURED] |
| `skia-safe` 0.153.3 | **Naming trap:** its `graphite` feature is Google's *Skia Graphite GPU backend*, not SIL Graphite. Skia's text layout uses Skia's own HarfBuzz, and only on Linux can `SKIA_USE_SYSTEM_LIBRARIES` swap in the system one. [SOURCE-READ] |
| cosmic-text 0.19.0 (iced, gpui on Linux) | Hard-wired to harfrust, with no shaper trait. The harfrust calls sit in one shaping function in `src/shape.rs` (about lines 130–270), and `y_offset` is carried into layout. **A contained fork:** route faces that have a `Silf` table through HarfBuzz C. [SOURCE-READ] |
| parley 0.12.0 / parley_engine (Slint, Xilem, Blitz) | Hard-wired to harfrust: `ShaperData` and `ShaperInstance` caches in `parley_engine/src/shape/{shaper,cache,shaped_text}.rs`. `y_offset` is carried. A fork is feasible but touches the caches. The 0.12 notes say "reshaping of lines … in a cursive script" is still in progress. [SOURCE-READ] |
| **Static HarfBuzz + graphite2 through `cc`** | **Works** (P2): 30 graphite2 `.cpp` files and one `harfbuzz.cc`, with no CMake or meson needed. Compiled here on Linux x86_64 only. Chromium and Electronite compile the same sources on Windows and macOS, but I did not test that here. Upstream Linebender and pop-os would probably not accept a C++ dependency [UNVERIFIED], so plan for a **permanent fork**. graphite2 has a history of memory-safety bugs, which is why Firefox sandboxes it; in a native app it would run in-process. |

---

## 4. Architecture options

| Criterion | **(a) Rust core + Rust-WASM UI in Electronite** | (b) Tauri + system webviews | (b′) Tauri + Graphite-patched CEF | (c) Fully native Rust (fork of parley or cosmic-text + HarfBuzz C/graphite2) |
|---|---|---|---|---|
| Graphite on all 3 OSes | **Yes.** Proven on macOS arm64 [tC4]. Windows and Linux use the same patched binary but are unmeasured. | **No**: Windows no, macOS no, Linux wrong (P3, P4) | Yes in principle: the same Chromium patch. Not built or measured. | Feasible: shaping proven (P2); layout, rendering and editor integration not built |
| Editing quality (IME, bidi caret, selection) | **Chromium-grade**, plus Hunspell spell check, find, undo and screen readers | Three different engines; Linux breaks Arabic diacritics | Chromium-grade | The weakest. Toolkit editors are young (iced IME arrived in 0.14; Slint and parley are the best). No spell check |
| PDF with Graphite | **The existing `printToPDF` bridge** [tC4 pdf-bridge]. Awami in PDF not yet measured. | Engine-dependent | CEF `PrintToPDF` | Must build a typesetter that also shapes Graphite (Typst and krilla use rustybuzz, so no Graphite) |
| Development speed | Medium. Rust-WASM UI ecosystem is smaller than React's, and a rich verse editor must be written or bridged to a JS editor. No engine work. | High, but you cannot ship it | Medium, plus heavy Chromium/CEF build infrastructure | **Low**: a text-stack fork, an editor, PDF output, accessibility |
| Testability in CI | **Best.** Playwright on headless Chromium for most UI tests; CDP or `_electron` against packaged Electronite on 3 runners for the Graphite golden test; `cargo test` and `wasm-bindgen-test` | `tauri-driver` WebDriver on Windows and Linux only; macOS needs the WebdriverIO service [CITED] | Playwright `connectOverCDP` | Toolkit harnesses (egui_kittest through AccessKit, Slint's testing backend, Masonry's TestHarness) plus pixel goldens. No Playwright |
| Package size | Large: current artifacts are 178.6 MB (macOS), 185.6 MB (Linux) and 193.7 MB (Windows), including about 36 MB of resources [tC4 PACKAGING] | Small: a few MB plus resources; Linux needs WebKitGTK [UNVERIFIED size] | Chromium-class, about the same as (a) [UNVERIFIED] | Small: tens of MB [UNVERIFIED]. HarfBuzz + graphite2 adds about 1.5 MB (P2) |
| "Done in Rust" | Core and UI in Rust. Shell: Chromium C++ plus a small JS main script | Core and shell in Rust | **Everything authored is Rust** (CEF is C++) | Most Rust (HarfBuzz and graphite2 are C/C++) |
| Verdict | **Recommended now** | Reject | **Target for later**, once tauri-runtime-cef is stable | Reject for v1. Revisit only if Chromium becomes untenable |

### Why (a) and not (c), in one line

What the requirement needs (bidi caret movement, IME on 3 OSes, selection inside Graphite
clusters, accessibility, spell check, PDF output with the same shaping) already works in
Chromium, and Electronite adds Graphite with a 210-line patch. A native stack must rebuild all
of it on a forked text engine.

### How to do (a)

1. **Core:** a Rust crate workspace as the authority for storage, Burrito, git and sync. This
   follows the current `pankosmia-web` pattern (Rocket on localhost) or a lean axum
   replacement. Keep the core free of any UI framework.
2. **UI:** Leptos 0.8 in client-side mode (or Dioxus 0.7 web) compiled to WASM and served by the
   core. Put no Electron API in the UI. A preload exposes only `printPdf`, the file dialogs and
   single-instance handling, as tC4 does today.
3. **Shell:** Electronite (an unchanged packaging pipeline) with a minimal `main.js` that starts
   the core binary and opens the window.
4. **Graphite test in CI:** extend `graphite-probe.js` to assert width **and** vertical ink
   extent, add an OpenType mark case and Padauk, and run it on the packaged artifacts on all
   three runners. Add an Awami page to the PDF probe.
5. **Shell seam:** keep it small enough that Tauri + CEF (b′) is a shell swap later. Build a CEF
   with `add_graphite.patch` and link it through `CEF_PATH` once `tauri-runtime-cef` is stable.

---

## Risks

1. **Electronite lags upstream.** Upstream Electron is at 44.7.2 [MEASURED `git ls-remote`]. The
   newest Electronite is `v39.2.6-graphite` (2026-01-05), and tC4 ships `v37.1.0-graphite`
   (Chrome 138). Both are outside Electron's three-major support window [UNVERIFIED policy text,
   not re-fetched]. Chromium security fixes therefore lag. Mitigation: a budget for regular
   rebases (the patch has been stable since 2019; the build notes describe re-applying it with
   `git am`), or move to (b′).
2. **One fork, one maintainer organisation.** Electronite builds are heavy Chromium builds for
   each OS. Option (b′) has the same burden, built through CEF's tooling instead.
3. **Windows and Linux Graphite are unmeasured** [tC4 scope note]. Treat them as open until the
   CI probe runs on those artifacts.
4. **Width-only probes give false positives**, as P3 shows. Any future engine or shell change
   must pass the vertical-extent check and the OpenType mark check.
5. **Rust-WASM UI ecosystem.** Leptos 0.9 and Dioxus 0.8 are in prerelease, so expect API churn.
   A rich USFM-aware editor needs either your own contenteditable layer or interop with a JS
   editor (ProseMirror or CodeMirror) through `wasm-bindgen`. Interop weakens "pure Rust" but
   not text correctness.
6. **Package size stays near 180–195 MB.** That is acceptable for an offline desktop app, and it
   is the price of correct rendering.
7. **Licence**, if (c) is ever revisited: Slint is GPLv3, royalty-free with attribution, or
   commercial.

---

## Probe artifacts (reproducible)

- `ui-probe/hbprobe/hb_shapers.py`: P1 (`python3 -I hb_shapers.py <font>`).
- `ui-probe/hbgr/`: P2. `HBGR_VENDOR=<dir with graphite/ and harfbuzz/> cargo build --release`,
  then `target/release/hbgr <font>`. The sources are in `ui-probe/vendor/`.
- `ui-probe/webkitprobe/`: P3 and P4. `run_webkit.py` (GTK OffscreenWindow + WebKit2 4.1,
  `xvfb-run`), `site/index.html`, `site/one.html`, `pango_one.py`, `inkbox.py`, and the PNGs
  `webkitgtk-awami.png` and `side-by-side.png`.
- The Awami font came from `translationcore4/src/ds/assets/fonts/AwamiNastaliq-Regular.ttf`.

All paths are relative to
`/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/`.

## Sources

- tC4 evidence: `docs/evidence/graphite-shaping-packaged-2026-08-14.md`,
  `desktop-packaging-spike-2026-08-14.md`, `pdf-bridge-2026-09-24.md`; `docs/PACKAGING.md`
  (sizes, Electronite pins); `docs/DECISIONS.md` D20; `docs/VISION.md` (Initial public release).
- Electronite: github.com/unfoldingWord/electronite, tag `v39.2.6-graphite`,
  `patches/chromium/add_graphite.patch`, `DEPS`, `docs/development/Electronite/ElectroniteBuildNotes.md`.
- WebKit (raw.githubusercontent.com/WebKit/WebKit, `main` and the `webkitgtk-2.46.0` through
  `2.52.6` tags): `platform/graphics/skia/{FontSkia,FontCascadeSkia,ComplexTextControllerSkia,SkiaHarfBuzzFont}.cpp`,
  `platform/graphics/ComplexTextController.cpp`, `platform/graphics/FontCascade.cpp`,
  `platform/graphics/coretext/ComplexTextControllerCoreText.mm`.
- [WebKit bug 161493: FreeType, incorrect Y glyph positioning](https://bugs.webkit.org/show_bug.cgi?id=161493);
  [WebKit bug 241717: Nastaliq; Awami Gecko-only](https://bugs.webkit.org/show_bug.cgi?id=241717);
  [WebKit bug 255972: Nastaliq marks in Epiphany](https://bugs.webkit.org/show_bug.cgi?id=255972).
- [Mozilla Hacks: RLBox sandboxing of Graphite](https://hacks.mozilla.org/?p=34163);
  [SecurityWeek: Firefox RLBox](https://www.securityweek.com/framework-isolates-libraries-firefox-improve-security/).
- Servo `components/fonts/Cargo.toml` (harfbuzz-sys `bundled`); servo/rust-harfbuzz
  `harfbuzz-sys` 0.8.0 `build.rs` and `hb-shaper-list.hh`.
- wry README (platform engines); tauri-docs `develop/Tests/WebDriver/index.mdx` (macOS has no
  WKWebView driver); cef-rs README and `cef-dll-sys` `build.rs` (`CEF_PATH`);
  [Tauri release listings](https://releasealert.dev/github/tauri-apps/tauri);
  [Tauri CEF navigation-API gap task](https://hub.harborframework.com/tasks/abundant/tauri-apps__tauri-14661).
- Changelogs: emilk/egui, iced-rs/iced, slint-ui/slint, linebender/parley, pop-os/cosmic-text
  (raw GitHub, read 2026-10-14). crates.io API for versions and dependencies.
- [GTK blog: an accessibility update (AccessKit backend in 4.18)](https://blog.gtk.org/2025/05/12/an-accessibility-update/).
- Homebrew `Formula/h/harfbuzz.rb` (depends on graphite2); MSYS2 `mingw-w64-harfbuzz/PKGBUILD`
  (graphite2 enabled); gvsbuild `projects/harfbuzz.py` (no graphite2).
