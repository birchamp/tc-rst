# Architecture

This document describes how the Rust rebuild of tC4 is put together and why. The storage,
journal, merge and exchange design is in `docs/STORAGE.md`. The goal and the definition
of done are in `docs/PLAN.md`. The reasons and evidence for each decision are rows in
`.audit/rebuild.tsv`.

## Process model

The app is one Rust program, `tc4`, plus a window.

1. `tc4` starts an HTTP server on `127.0.0.1` with a port the operating system picks.
2. The server owns every project file, every resource and every network request. No other
   process writes project data.
3. The window loads the UI from that server. The UI is written in Rust (Leptos), compiled
   to WebAssembly, and talks to the server through a typed JSON API.
4. In a release build the window is Electronite. Electronite is an Electron fork whose
   Chromium shapes SIL Graphite fonts. A small Electronite main script starts `tc4`,
   waits for its address, and opens the window. It also gives the UI print-to-PDF and
   native file dialogs. In development and tests, any Chromium-based browser opens the
   same address.

Why a server and a web UI:

- tC4's look comes from its CSS, design tokens and fonts. A web UI reuses them as they
  are, so screens match tC4 closely. The parity harness checks this against
  `parity/tc4-baseline`.
- Correct right-to-left and complex-script editing (bidi caret, IME, selection, spell
  check) comes from the browser engine. The egui probe showed that a native Rust text
  stack lays Hebrew and Arabic out backwards today.
- Only Electronite has shown correct Graphite shaping. Tauri's system webviews fail it on
  all three operating systems. The shell is thin, so it can be replaced later without
  touching the app.
- Playwright drives the real UI in CI, so every journey has an end-to-end test.

## Security of the local server

A local web server is reachable by other programs on the same computer and by web pages
open in the user's browser. The server therefore:

- binds to `127.0.0.1` only;
- creates a random token at each launch. The shell passes the token to the UI in the URL
  fragment, and the UI sends it in a header with every request. A request without the
  token is refused;
- refuses any request whose `Host` header is not its own address, which blocks DNS
  rebinding;
- refuses cross-origin requests and sends no CORS headers.

## The internet switch

Network access goes through one switch in `tc-door43`. The switch is off at every
launch. An action that needs the internet asks the user first, and "Not now" sends
nothing. This matches tC4 (D95). Tests never use the network. They use fixtures and the
fake Door43 server in `tc-door43-fake`.

Door43 tokens live only in memory, or in the operating-system keychain when the user
chooses "Stay signed in". No password is stored.

## Crates

Every crate lives in `crates/`. Arrows read "is used by".

| Crate | Owns |
|---|---|
| `tc-core` | Book ids, verse spans, references. |
| `tc-usfm` | Lossless USFM parsing and writing, verse units, plain text, alignment markup. |
| `tc-versification` | Versification schemes and mapping, ported from Proskomma. |
| `tc-burrito` | Scripture Burrito metadata, ingredient checksums, validation. |
| `tc-helps` | Formats of translation helps from Door43 and Aquifer, one model for both. |
| `tc-door43`, `tc-door43-fake` | The Door43 HTTP client, the internet switch, and a fake server for tests. |
| `tc-journal` | Operations, journal segments, the fold, conflicts. See `STORAGE.md`. |
| `tc-store` | The project folder: atomic writes, rebuilding derived files, recovery, local git history. |
| `tc-resources` | Installing, pinning, verifying and upgrading resources from Door43 or Aquifer. Seeds the bundled English suite from `bundle/en/`. Repackages each lexicon into one JSON file that the app reads into memory. |
| `tc-check` | Building tN and tW checks, selections, decisions, invalidation. |
| `tc-align` | Alignment editing, keeping alignments across edits, suggestions. |
| `tc-obs` | Open Bible Stories: stories and frames. |
| `tc-sync` | Door43 sync, share, join and connect, and USB exchange (unit X): one state machine over `tc-store`, the network through `tc_door43::git`. It writes no project file itself: project files go through `tc-store`'s atomic writes; it writes only the exchange file it exports. |
| `tc-import` | Import into a new project's records: tC3 projects (unit I). tC4 4.0, raw USFM and Scripture Burrito imports come after the first release. Pure: the store writes the records. |
| `tc-export` | The print model of a book or an OBS project, with its page setup; aligned and plain USFM, Burrito zip, OBS Markdown, print HTML for PDF. |
| `tc-api` | Request and response types shared by the server and the UI. Builds for wasm32. |
| `tc-app` | Application services: the operations behind every screen. |
| `tc-server` | The axum HTTP server: routes, token check, events to the UI. |
| `tc-ui` | The Leptos UI: tC4's screens and design system, with tC4's CSS, fonts and catalogs. |
| `tc4` | The program: starts the server and the shell. |
| `tc-corpus` | The deterministic scale corpus of `PERFORMANCE.md`, for tests and benchmarks only. |
| `tc-arch` | Tests of the rules below; no product code. |

Dependency rules:

- `tc-ui` depends only on `tc-api` and `tc-core`, so the UI cannot touch storage directly.
- Only `tc-store` writes project files. Only `tc-door43` opens network connections, HTTP
  through `tc_door43::Http` and sync's git through `tc_door43::git`, and only behind its
  internet switch; `tc-resources`' Aquifer client sends through `Http`.
- Pure logic (`tc-usfm`, `tc-versification`, `tc-helps`, `tc-journal`'s fold, `tc-check`,
  `tc-align`, `tc-obs`, `tc-export`) does no I/O and is tested with plain values.

`crates/tc-arch` tests these rules on every `cargo test`: the crate edges, the network and I/O
libraries, the workspace edition, lints and shared versions, I/O in pure crates, the files
allowed to write (`crates/tc-arch/allow/fs_writes.txt`), the internals that stay in their owning
crate (`crates/tc-arch/allow/imports.txt`), and the local server's guard. Each failure names its
rule. Typed capabilities (a `ProjectWriter` only `tc-store` can build, a `Net` only `tc-door43`
can build) make the write and network rules compile-time: see card DOM-3; `tc-arch`'s scans are
the stopgap and the catch-all.

## UI

- Leptos 0.8.22 in client-side mode, pinned to that exact version. The bake-off against
  Dioxus is recorded in the decision log.
- The DOM follows tC4's React output: the same elements, `data-*` hooks and inline
  styles, so tC4's CSS applies unchanged. tC4's design-system components become Leptos
  components with the same names and props.
- One `css!`-style builder joins inline styles. Later properties win, and a shorthand
  resets its longhands, as in React.
- Text the user types is written to a browser-side draft buffer on every input and
  restored after a reload. A WebAssembly panic stops the whole UI, so this buffer is
  what saves the last sentence. The server-side save follows on blur and after two
  seconds idle, as in tC4.
- The message catalogs are tC4's JSON catalogs with the same keys.

## PDF

The export kernel in `tc-export` produces print HTML and CSS. The window prints it to PDF:
Electronite through `printToPDF`, Playwright through `page.pdf()` in tests. The PDF uses
the same fonts and shaping as the screen.

## Order of work

1. Foundations with no storage or UI decision: `tc-core`, `tc-usfm`, `tc-versification`,
   `tc-burrito`, `tc-helps`, `tc-door43`.
2. Storage: `tc-journal` and `tc-store`, with the property and crash tests from
   `STORAGE.md`, before any feature records data.
3. Domain: `tc-resources`, `tc-check`, `tc-align`, `tc-obs`.
4. Exchange: `tc-sync`, `tc-import`, `tc-export`.
5. App: `tc-api`, `tc-app`, `tc-server`, then `tc-ui` screen by screen against the
   parity baseline, then `tc4` and the Electronite shell.
6. Journeys: a Playwright test per journey, then the team-sync and USB journeys.
