# Electronite spike G0 on Windows (2026-10-17)

The reference machine: a Dell Latitude 7490 (a 2018 model) with 4 cores, 8 GB and a SATA SSD,
older than the five years the reference names, so at least as slow
(`docs/evidence/windows-machine-2026-10-17.md` on `windows-fixes`). Defender excludes no
folder: the owner confirmed it on 2026-10-18 (the machine table below could not read the list,
which only an administrator can).

This file was written by `spikes/electronite/run-spike.ps1`. The spike is throwaway and never
shipped (docs/APP-DESIGN.md section 10, unit G0). **The server is a stand-in**: `tc4-standin`
(`spikes/electronite/src`), a small axum server on 127.0.0.1 with the per-launch token, the
Host, Origin and X-Tc-Api checks of section 5.2, an address line on stdout and a quit on a closed stdin,
because the real `tc4 serve` (unit A2) is being built at the same time. Its loopback cost is not
tc4 serve's: unit A2 measures that. What this file measures is the shell, the browser engine and
the operating system around a server of the same kind.

- Commit: 939596af99c7231768b85d0d5663151b244bfe4a on g0-windows
- Run: 2026-10-17 19:41 to 19:46 UTC; `-Clicks 200 -Warmup 20`
- Electronite 37.1.0, Chromium 138.0.7204.35, Node 22.16.0, V8 13.8.258.18-electron.0
- Raw results: `./docs/evidence/electronite-spike-2026-10-17` (results, events and screenshots; the full run is in `./spikes/electronite/out/20261017-194147`)

## Machine

| | |
|---|---|
| OS | Microsoft Windows 10 Pro 10.0.19045 (build 19045), 64-bit |
| Computer | Dell Inc. Latitude 7490 |
| RAM | 7.9 GiB |
| CPU | Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz |
| Cores | 4 cores, 8 threads |
| Disks | WDC WDS100T2B0B-00YS70 (SSD, RAID, 932 GB) |
| Power | on mains power |
| Power plan | Balanced |
| Defender | real-time protection on; antivirus enabled True; mode Normal; the excluded folders can be read only by an administrator |
| SHA instructions | False |
| AVX-512 | F False, BW False, VL False |
| AVX2 | True |

## First attempts that failed

- hardkill, filter mode, kill: the first attempt failed (Invalid object passed in, ':' or '}' expected. (73): {"ev":"quit","instance":"72d49e04fbb7","pid":1216,"t":1792266313959,"why"); a second attempt ran

## 1. Request types (AD11, Q-1)

A probe passes when the page saw the load succeed and the server saw the request with the
token. `filter` is AD11 (onBeforeSendHeaders on the paths outside `/api/`), `protocol` is
the Q-1 prototype (a privileged `tc4://` scheme proxied by `protocol.handle`), `filter-all`
is a filter over every path. A cell that differs between passes shows each pass in turn.
Rows marked informational are not request types the app relies on, and never count as a
gap in AD11.

`api-main-thread` times the page's own `/api/` fetch with the shell's main thread idle, then
while that thread is blocked 200 ms of every 250 ms. Electron passes every request of the
window through a proxy on that thread, whatever the filter's URL list says; the list only
spares the filter's JS listener (`api-listener`). "Waits for the main process" means the
busy p95 rose by more than 50 ms.

| Request | filter | protocol | filter-all |
|---|---|---|---|
| document (index.html, loaded by the window itself) | passes | passes | passes |
| script (a <script src>) | passes | passes | passes |
| stylesheet (a <link rel=stylesheet>) | passes | passes | passes |
| fetch (the page's own fetch, with the header it sets) | passes | passes | passes |
| image (an <img>) | passes | passes | passes |
| iframe (a help page in an <iframe>, with its own style sheet and pictures) | passes | passes | passes |
| font (a web font from @font-face) | passes | passes | passes |
| css-url (a picture named by url() in a style sheet) | passes | passes | passes |
| event-stream (the event stream, read with fetch and the header (AD10)) | passes | passes | passes |
| eventsource (EventSource (informational: AD10 does not use it)) | fails (informational, not used: AD10) | fails (informational, not used: AD10) | fails (informational, not used: AD10) |
| download (an <a download> link) | passes | passes | passes |
| print (the shell's hidden print window and printToPDF) | passes | passes | passes |
| child-window (a same-origin window.open, kept in the app (§5.7, A-33)) | passes | passes | passes |
| api-listener (the filter's JS listener saw no /api/ request (AD11's URL list)) | passes | no filter: protocol.handle sees every request | saw 17 /api/ request(s): this filter lists every path / saw 18 /api/ request(s): this filter lists every path |
| api-main-thread (the page's /api/ fetch while the shell's main thread is busy (informational)) | waits for the main process | waits for the main process | waits for the main process |

What each probe saw:

- filter, pass 1, document: this page is running
- filter, pass 1, script: this script is running; server: 1× 200 token ok
- filter, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- filter, pass 1, fetch: hello from instance 03d8ffe9cd15; server: 1× 200 token ok
- filter, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- filter, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter, pass 1, font: 1 face loaded; server: 1× 200 token ok
- filter, pass 1, css-url: the element is styled; server: 1× 200 token ok
- filter, pass 1, event-stream: HTTP 200, 5 frames, the first at 13 ms, the fifth at 408 ms
- filter, pass 1, eventsource: error event; server: 1× 401 token missing. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter, pass 1, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- filter, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter, pass 1, api-listener: the filter (patterns with the port) saw 0 /api/ request(s) and 18 in all: {"root":1,"ui":6,"media":2,"help":7,"download":1,"print":1}
- filter, pass 1, api-main-thread: idle: median 2.9 ms, p95 7.1 ms; main thread blocked 200 ms of every 250 ms: median 154.4 ms, p95 202.4 ms (40 requests each)
- protocol, pass 1, document: this page is running
- protocol, pass 1, script: this script is running; server: 1× 200 token ok
- protocol, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- protocol, pass 1, fetch: hello from instance 09a80c2e580d; server: 1× 200 token ok
- protocol, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- protocol, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- protocol, pass 1, font: 1 face loaded; server: 1× 200 token ok
- protocol, pass 1, css-url: the element is styled; server: 1× 200 token ok
- protocol, pass 1, event-stream: HTTP 200, 5 frames, the first at 14 ms, the fifth at 410 ms
- protocol, pass 1, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- protocol, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- protocol, pass 1, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- protocol, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- protocol, pass 1, api-listener: no filter under protocol.handle: its handler saw 33 request(s) so far
- protocol, pass 1, api-main-thread: idle: median 5.6 ms, p95 10.8 ms; main thread blocked 200 ms of every 250 ms: median 170.4 ms, p95 203.3 ms (40 requests each)
- filter-all, pass 1, document: this page is running
- filter-all, pass 1, script: this script is running; server: 1× 200 token ok
- filter-all, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- filter-all, pass 1, fetch: hello from instance d16a874534c1; server: 1× 200 token ok
- filter-all, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- filter-all, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter-all, pass 1, font: 1 face loaded; server: 1× 200 token ok
- filter-all, pass 1, css-url: the element is styled; server: 1× 200 token ok
- filter-all, pass 1, event-stream: HTTP 200, 5 frames, the first at 14 ms, the fifth at 406 ms
- filter-all, pass 1, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter-all, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter-all, pass 1, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- filter-all, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter-all, pass 1, api-listener: the filter (patterns with the port) saw 17 /api/ request(s) and 35 in all: {"root":1,"ui":6,"api":17,"media":2,"help":7,"download":1,"print":1}
- filter-all, pass 1, api-main-thread: idle: median 3.5 ms, p95 8 ms; main thread blocked 200 ms of every 250 ms: median 155.3 ms, p95 202.3 ms (40 requests each)
- filter-all, pass 2, document: this page is running
- filter-all, pass 2, script: this script is running; server: 1× 200 token ok
- filter-all, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- filter-all, pass 2, fetch: hello from instance 9b831e538951; server: 1× 200 token ok
- filter-all, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- filter-all, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter-all, pass 2, font: 1 face loaded; server: 1× 200 token ok
- filter-all, pass 2, css-url: the element is styled; server: 1× 200 token ok
- filter-all, pass 2, event-stream: HTTP 200, 5 frames, the first at 9 ms, the fifth at 414 ms
- filter-all, pass 2, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter-all, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter-all, pass 2, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- filter-all, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter-all, pass 2, api-listener: the filter (patterns with the port) saw 18 /api/ request(s) and 36 in all: {"root":1,"ui":6,"api":18,"media":2,"help":7,"download":1,"print":1}
- filter-all, pass 2, api-main-thread: idle: median 6.1 ms, p95 7.9 ms; main thread blocked 200 ms of every 250 ms: median 154.5 ms, p95 202.3 ms (40 requests each)
- protocol, pass 2, document: this page is running
- protocol, pass 2, script: this script is running; server: 1× 200 token ok
- protocol, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- protocol, pass 2, fetch: hello from instance bdc5ea5f7108; server: 1× 200 token ok
- protocol, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- protocol, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- protocol, pass 2, font: 1 face loaded; server: 1× 200 token ok
- protocol, pass 2, css-url: the element is styled; server: 1× 200 token ok
- protocol, pass 2, event-stream: HTTP 200, 5 frames, the first at 9 ms, the fifth at 416 ms
- protocol, pass 2, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- protocol, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- protocol, pass 2, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- protocol, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- protocol, pass 2, api-listener: no filter under protocol.handle: its handler saw 34 request(s) so far
- protocol, pass 2, api-main-thread: idle: median 4.4 ms, p95 10.5 ms; main thread blocked 200 ms of every 250 ms: median 156.7 ms, p95 202.8 ms (40 requests each)
- filter, pass 2, document: this page is running
- filter, pass 2, script: this script is running; server: 1× 200 token ok
- filter, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- filter, pass 2, fetch: hello from instance a32272e87656; server: 1× 200 token ok
- filter, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- filter, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter, pass 2, font: 1 face loaded; server: 1× 200 token ok
- filter, pass 2, css-url: the element is styled; server: 1× 200 token ok
- filter, pass 2, event-stream: HTTP 200, 5 frames, the first at 10 ms, the fifth at 416 ms
- filter, pass 2, eventsource: error event; server: 1× 401 token missing. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter, pass 2, print: HTTP 200, a PDF of 37284 bytes; server: 1× 200 token ok
- filter, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter, pass 2, api-listener: the filter (patterns with the port) saw 0 /api/ request(s) and 18 in all: {"root":1,"ui":6,"media":2,"help":7,"download":1,"print":1}
- filter, pass 2, api-main-thread: idle: median 5.6 ms, p95 8.5 ms; main thread blocked 200 ms of every 250 ms: median 169.5 ms, p95 202 ms (40 requests each)

## 2. The aligner round trip (AD18, Q-13)

One click: the page posts to `/api/v1/align/click` with its own fetch, gets about 20 KB of
JSON, parses it and rebuilds the word bank and the cards. "Reply" ends when the JSON is
parsed, "rendered" when the DOM is rebuilt, "frame" at the start of the next animation frame,
and "painted" when the main thread has finished that frame's style, layout and paint (a
message posted from the frame's requestAnimationFrame callback, which Chromium handles only
after them; the compositor shows the frame at the next display refresh). Painted is the
figure for Q-13 and Q-1. Times in ms, nearest-rank, after the warm-up clicks. While loading:
a story's 15 OBS-sized pictures (640x360, about 70 KB) every 500 ms, the help page every
second, and the event stream open. The budget is 30 ms (PERFORMANCE.md, Aligning).

| Mode | Pass | While | Clicks | Reply median | Reply p95 | Rendered p95 | Frame p95 | Painted median | Painted p95 | Frames missed | Loaded meanwhile |
|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| filter | 1 | alone | 200 | 2.6 | 3.2 | 4.4 | 20.6 | 9.5 | 22.4 | 0 | - |
| filter | 1 | loading | 200 | 2.6 | 6.4 | 7.9 | 21.5 | 9.5 | 24.5 | 0 | 240 pictures, 7 help pages, 32 events |
| filter | 2 | alone | 200 | 2.6 | 3.5 | 4.8 | 20.8 | 9.7 | 22.9 | 0 | - |
| filter | 2 | loading | 200 | 2.5 | 4.4 | 5.7 | 21.3 | 9.2 | 23.5 | 0 | 240 pictures, 7 help pages, 32 events |
| protocol | 1 | alone | 200 | 3.3 | 4.4 | 5.8 | 20.7 | 10.4 | 22.7 | 0 | - |
| protocol | 1 | loading | 200 | 3.2 | 7.9 | 8.8 | 20.5 | 10.4 | 22.7 | 0 | 240 pictures, 7 help pages, 32 events |
| protocol | 2 | alone | 200 | 3.4 | 4.5 | 6.1 | 20.5 | 10.7 | 22.8 | 0 | - |
| protocol | 2 | loading | 200 | 3.2 | 9.4 | 10.4 | 21.5 | 9.8 | 25.5 | 0 | 255 pictures, 8 help pages, 33 events |
| filter-all | 1 | alone | 200 | 2.8 | 3.8 | 5.0 | 20.5 | 10.2 | 22.3 | 0 | - |
| filter-all | 1 | loading | 200 | 2.7 | 5.1 | 7.1 | 20.7 | 9.9 | 22.6 | 0 | 240 pictures, 7 help pages, 32 events |
| filter-all | 2 | alone | 200 | 2.7 | 3.3 | 4.6 | 20.1 | 9.3 | 22.0 | 0 | - |
| filter-all | 2 | loading | 200 | 2.7 | 4.7 | 5.8 | 21.3 | 9.1 | 23.4 | 0 | 240 pictures, 7 help pages, 32 events |

The painted p95 of each pass, and its spread (the highest minus the lowest):

| Mode | While | Painted p95 by pass | Lowest | Highest | Spread |
|---|---|---|---:|---:|---:|
| filter | alone | 22.4, 22.9 | 22.4 | 22.9 | 0.5 |
| filter | loading | 24.5, 23.5 | 23.5 | 24.5 | 1.0 |
| protocol | alone | 22.7, 22.8 | 22.7 | 22.8 | 0.1 |
| protocol | loading | 22.7, 25.5 | 22.7 | 25.5 | 2.8 |
| filter-all | alone | 22.3, 22.0 | 22.0 | 22.3 | 0.3 |
| filter-all | loading | 22.6, 23.4 | 22.6 | 23.4 | 0.8 |

- Q-1, alone (painted p95, protocol against filter): the passes overlap: the difference is inside the spread and does not decide Q-1.
- Q-1, loading (painted p95, protocol against filter): the passes overlap: the difference is inside the spread and does not decide Q-1.

## 3. Supervision (AD39)

Times in ms from the moment the script acted.

| Case | Result |
|---|---|
| End the server process, filter mode | the shell saw the exit after 38.0, had a new server address after 78.0, and Home was loaded again after 109.0; new server: True, new port: True |
| End the server process, protocol mode | the shell saw the exit after 11.0, had a new server address after 52.0, and Home was loaded again after 109.0; new server: True, new port: True |
| Close the window (WM_CLOSE to the window, as its close button sends) | the server exited after 115.0 (its quit: quit-message); the shell exited |
| End the shell's process (server started plainly (Node's default spawn)) | the server exited after 74.0; its quit: stdin-closed. Child processes before: electron.exe, tc4-standin.exe, electron.exe, electron.exe. Left after 5 s: none |
| End the shell's process (server started with detached: true) | the server exited after 43.0; its quit: stdin-closed. Child processes before: electron.exe, tc4-standin.exe, electron.exe, electron.exe. Left after 5 s: none |

## 4. A hard kill while typing (AD21)

The page types one letter every 50 ms. Each keystroke is written to `localStorage` at once,
sent to the shell, which appends it to its mirror file (synced at most once a second), and
reported to the server. Chromium writes `localStorage` to the profile some seconds after the
page sets it. The script waits until the profile's files on disk hold the draft (the "first
write to disk" column), lets the page type 10 s more, then ends the shell's process (or, as the
control, asks the shell to quit normally), opens the same profile again (same origin) and
reads what was kept. Counts are keystrokes; "made" counts them from the clock up to the moment
the script acted, so a normal quit can keep a few more. "Lost" is the typing time missing at
the end: (made - kept) x 50 ms.

| Mode | How the shell ended | Made (from the clock) | First write to disk | On disk just before | The server saw | localStorage kept | localStorage lost | The mirror kept | The mirror lost | Mirror lines (torn) | The server |
|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|---|
| filter | hard kill | 301 | 5.1 s after typing began | 100 | 301 | 100 | 10.1 s | 301 | 0.0 s | 301 (0) | exited after 121.0 (quit: none: the process was ended from outside before it could quit) |
| filter | normal quit (control) | 302 | 5.1 s after typing began | 101 | 305 | 306 | 0.0 s | 306 | 0.0 s | 306 (0) | exited after 427.0 (quit: quit-message) |
| protocol | hard kill | 302 | 5.1 s after typing began | 101 | 301 | 101 | 10.1 s | 302 | 0.0 s | 302 (0) | exited after 105.0 (quit: none: the process was ended from outside before it could quit) |
| protocol | normal quit (control) | 306 | 5.3 s after typing began | 100 | 309 | 311 | 0.0 s | 311 | 0.0 s | 311 (0) | exited after 444.0 (quit: quit-message) |

## 5. Graphite (informational)

Awami Nastaliq has Graphite tables and no OpenType substitutions: with Graphite its letters
join and cascade; without it they stand alone and the line is about twice as wide. In
Chromium 141 without Graphite (Playwright, `spikes/electronite/test`) the test text measured
639.8 px wide with an ascent of 46 px; Electronite 37.1.0 on Linux measured 302.9 px and
61 px. "Joined" below means narrower than 480 px. This is a smoke test, not the P8 probe.

| Mode | Font loaded | Width | Height | Canvas width | Ascent | Descent | Joined | Screenshot |
|---|---|---:|---:|---:|---:|---:|---|---|
| filter | True | 302.9 | 76.7 | 322.9 | 61.0 | 13.2 | yes | `graphite-filter-run.png` |
| protocol | True | 302.9 | 76.7 | 322.9 | 61.0 | 13.2 | yes | `graphite-protocol-run.png` |
| filter-all | True | 302.9 | 76.7 | 322.9 | 61.0 | 13.2 | yes | `graphite-filter-all-run.png` |

## What these results decide

- **AD11 and capability URLs.** A request type that fails under `filter` above is one that
  the header filter misses; section 5.2 then moves `/media`, `/download` and `/print` to capability
  URLs. A failure under `filter` that also fails under `filter-all` is not the filter's list.
  Informational rows (EventSource, `api-main-thread`) are never such a failure.
- **The main process.** `api-main-thread` shows whether the page's `/api/` requests wait for
  the shell's main thread. If they do, the shell must keep synchronous work off that thread
  (the mirror's appends and fsync in G1), or every aligner click waits for it.
- **Q-1, the custom scheme.** The design's rule: take `protocol.handle` if it adds less to
  the aligner's round trip than the filter, otherwise the filter. Compare the painted p95 of
  `protocol` and `filter`, alone and while loading, across the passes, and the probe rows. A
  difference inside the spread between passes does not decide Q-1.
- **Q-13, the aligner fallback.** If the painted p95 of `filter` misses 30 ms in any pass on
  the reference laptop, Q-13 goes to the owner with these numbers.
- **AD39 and G1.** The restart rows show whether the shell restarts the server and reloads
  Home. The "end the shell" rows show whether the server quits by itself when the shell dies,
  or is ended with it, which decides how G1 starts `tc4`.
- **AD21.** The hard-kill rows show how many keystrokes, and how many seconds of typing,
  `localStorage` and the shell's mirror keep when the whole app dies after Chromium has
  written `localStorage` once.

