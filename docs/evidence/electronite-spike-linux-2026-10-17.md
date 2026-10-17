# Electronite spike G0 on Linux (2026-10-17)

This is the run in the build container, not on the reference laptop: Electronite's Linux build
under Xvfb as root with `--no-sandbox`, software rendering, on a shared 4-vCPU VM with a load
average of 2 to 3 from other agents' work. It tests the driver end to end and gives a Linux
baseline; the Windows numbers come from `.audit/tasks/g0-windows.md`.

This file was written by `spikes/electronite/run-spike.ps1`. The spike is throwaway and never
shipped (docs/APP-DESIGN.md section 10, unit G0). **The server is a stand-in**: `tc4-standin`
(`spikes/electronite/src`), a small axum server on 127.0.0.1 with the per-launch token, the
Host, Origin and X-Tc-Api checks of section 5.2, an address line on stdout and a quit on a closed stdin,
because the real `tc4 serve` (unit A2) is being built at the same time. Its loopback cost is not
tc4 serve's: unit A2 measures that. What this file measures is the shell, the browser engine and
the operating system around a server of the same kind.

- Commit: d9239c2ca27788ea7f3c7b65405f869096dd65ac on p0-g0 (with local changes: the earlier run's files, removed before this run)
- Run: 2026-10-17 17:54 to 17:58 UTC; `-Clicks 200 -Warmup 20`
- Electronite 37.1.0, Chromium 138.0.7204.35, Node 22.16.0, V8 13.8.258.18-electron.0
- Raw results: `./docs/evidence/electronite-spike-linux-2026-10-17` (results, events and screenshots; the profiles and logs stayed in the container's scratch folder)

## Machine

| | |
|---|---|
| OS | Linux 6.18.44-fc-v139 |
| CPU | Intel(R) Xeon(R) Processor @ 2.10GHz |
| Cores | 4 logical CPUs |
| RAM | 15.7 GiB |
| Disks | not read on Linux |
| Power | not read on Linux |
| Defender | not Windows |
| SHA instructions | True |
| AVX-512 | F True, BW True, VL True |
| AVX2 | True |

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
| api-listener (the filter's JS listener saw no /api/ request (AD11's URL list)) | passes | no filter: protocol.handle sees every request | saw 17 /api/ request(s): this filter lists every path |
| api-main-thread (the page's /api/ fetch while the shell's main thread is busy (informational)) | waits for the main process | waits for the main process | waits for the main process |

What each probe saw:

- filter, pass 1, document: this page is running
- filter, pass 1, script: this script is running; server: 1× 200 token ok
- filter, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- filter, pass 1, fetch: hello from instance 4889f411d3ef; server: 1× 200 token ok
- filter, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- filter, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter, pass 1, font: 1 face loaded; server: 1× 200 token ok
- filter, pass 1, css-url: the element is styled; server: 1× 200 token ok
- filter, pass 1, event-stream: HTTP 200, 5 frames, the first at 87 ms, the fifth at 486 ms
- filter, pass 1, eventsource: error event; server: 1× 401 token missing. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter, pass 1, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter, pass 1, api-listener: the filter (patterns with the port) saw 0 /api/ request(s) and 18 in all: {"root":1,"ui":6,"media":2,"help":7,"download":1,"print":1}
- filter, pass 1, api-main-thread: idle: median 6.1 ms, p95 10.7 ms; main thread blocked 200 ms of every 250 ms: median 154.2 ms, p95 207 ms (40 requests each)
- protocol, pass 1, document: this page is running
- protocol, pass 1, script: this script is running; server: 1× 200 token ok
- protocol, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- protocol, pass 1, fetch: hello from instance bde95eac2870; server: 1× 200 token ok
- protocol, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- protocol, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- protocol, pass 1, font: 1 face loaded; server: 1× 200 token ok
- protocol, pass 1, css-url: the element is styled; server: 1× 200 token ok
- protocol, pass 1, event-stream: HTTP 200, 5 frames, the first at 52 ms, the fifth at 404 ms
- protocol, pass 1, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- protocol, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- protocol, pass 1, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- protocol, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- protocol, pass 1, api-listener: no filter under protocol.handle: its handler saw 34 request(s) so far
- protocol, pass 1, api-main-thread: idle: median 8.6 ms, p95 16.9 ms; main thread blocked 200 ms of every 250 ms: median 151.5 ms, p95 208.9 ms (40 requests each)
- filter-all, pass 1, document: this page is running
- filter-all, pass 1, script: this script is running; server: 1× 200 token ok
- filter-all, pass 1, stylesheet: its rules apply; server: 1× 200 token ok
- filter-all, pass 1, fetch: hello from instance 62588ea88ad9; server: 1× 200 token ok
- filter-all, pass 1, image: loaded, 640×360; server: 1× 200 token ok
- filter-all, pass 1, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter-all, pass 1, font: 1 face loaded; server: 1× 200 token ok
- filter-all, pass 1, css-url: the element is styled; server: 1× 200 token ok
- filter-all, pass 1, event-stream: HTTP 200, 5 frames, the first at 48 ms, the fifth at 406 ms
- filter-all, pass 1, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter-all, pass 1, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter-all, pass 1, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter-all, pass 1, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter-all, pass 1, api-listener: the filter (patterns with the port) saw 17 /api/ request(s) and 35 in all: {"root":1,"ui":6,"api":17,"media":2,"help":7,"download":1,"print":1}
- filter-all, pass 1, api-main-thread: idle: median 6.9 ms, p95 10.8 ms; main thread blocked 200 ms of every 250 ms: median 135.9 ms, p95 206.4 ms (40 requests each)
- filter-all, pass 2, document: this page is running
- filter-all, pass 2, script: this script is running; server: 1× 200 token ok
- filter-all, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- filter-all, pass 2, fetch: hello from instance 5941845afe9b; server: 1× 200 token ok
- filter-all, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- filter-all, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter-all, pass 2, font: 1 face loaded; server: 1× 200 token ok
- filter-all, pass 2, css-url: the element is styled; server: 1× 200 token ok
- filter-all, pass 2, event-stream: HTTP 200, 5 frames, the first at 48 ms, the fifth at 408 ms
- filter-all, pass 2, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter-all, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter-all, pass 2, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter-all, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter-all, pass 2, api-listener: the filter (patterns with the port) saw 17 /api/ request(s) and 35 in all: {"root":1,"ui":6,"api":17,"media":2,"help":7,"download":1,"print":1}
- filter-all, pass 2, api-main-thread: idle: median 7.5 ms, p95 16.9 ms; main thread blocked 200 ms of every 250 ms: median 157.1 ms, p95 208.3 ms (40 requests each)
- protocol, pass 2, document: this page is running
- protocol, pass 2, script: this script is running; server: 1× 200 token ok
- protocol, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- protocol, pass 2, fetch: hello from instance d9797266fb73; server: 1× 200 token ok
- protocol, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- protocol, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- protocol, pass 2, font: 1 face loaded; server: 1× 200 token ok
- protocol, pass 2, css-url: the element is styled; server: 1× 200 token ok
- protocol, pass 2, event-stream: HTTP 200, 5 frames, the first at 48 ms, the fifth at 409 ms
- protocol, pass 2, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- protocol, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- protocol, pass 2, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- protocol, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- protocol, pass 2, api-listener: no filter under protocol.handle: its handler saw 33 request(s) so far
- protocol, pass 2, api-main-thread: idle: median 11.4 ms, p95 19.8 ms; main thread blocked 200 ms of every 250 ms: median 170.8 ms, p95 210.8 ms (40 requests each)
- filter, pass 2, document: this page is running
- filter, pass 2, script: this script is running; server: 1× 200 token ok
- filter, pass 2, stylesheet: its rules apply; server: 1× 200 token ok
- filter, pass 2, fetch: hello from instance beb7e1e069e0; server: 1× 200 token ok
- filter, pass 2, image: loaded, 640×360; server: 1× 200 token ok
- filter, pass 2, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter, pass 2, font: 1 face loaded; server: 1× 200 token ok
- filter, pass 2, css-url: the element is styled; server: 1× 200 token ok
- filter, pass 2, event-stream: HTTP 200, 5 frames, the first at 46 ms, the fifth at 406 ms
- filter, pass 2, eventsource: error event; server: 1× 401 token missing. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter, pass 2, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter, pass 2, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter, pass 2, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter, pass 2, api-listener: the filter (patterns with the port) saw 0 /api/ request(s) and 18 in all: {"root":1,"ui":6,"media":2,"help":7,"download":1,"print":1}
- filter, pass 2, api-main-thread: idle: median 5.8 ms, p95 10.6 ms; main thread blocked 200 ms of every 250 ms: median 155 ms, p95 207.9 ms (40 requests each)

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
| filter | 1 | alone | 200 | 5.8 | 9.9 | 11.3 | 22.9 | 15.8 | 25.9 | 0 | - |
| filter | 1 | loading | 200 | 6.1 | 34.7 | 36.3 | 43.5 | 15.7 | 48.8 | 0 | 315 pictures, 10 help pages, 41 events |
| filter | 2 | alone | 200 | 6.0 | 9.6 | 10.8 | 22.0 | 16.0 | 24.5 | 0 | - |
| filter | 2 | loading | 200 | 6.4 | 27.3 | 27.9 | 42.5 | 16.8 | 45.0 | 0 | 285 pictures, 9 help pages, 38 events |
| protocol | 1 | alone | 200 | 8.3 | 12.4 | 14.1 | 25.3 | 19.0 | 28.6 | 0 | - |
| protocol | 1 | loading | 200 | 9.1 | 48.4 | 49.1 | 56.1 | 20.9 | 64.5 | 0 | 330 pictures, 10 help pages, 43 events |
| protocol | 2 | alone | 200 | 10.1 | 15.8 | 17.4 | 27.4 | 21.2 | 30.0 | 0 | - |
| protocol | 2 | loading | 200 | 9.8 | 49.8 | 50.6 | 61.9 | 21.2 | 67.5 | 0 | 345 pictures, 11 help pages, 45 events |
| filter-all | 1 | alone | 200 | 6.5 | 8.5 | 9.9 | 20.7 | 16.7 | 24.0 | 0 | - |
| filter-all | 1 | loading | 200 | 7.1 | 25.7 | 27.0 | 43.1 | 17.7 | 47.6 | 0 | 300 pictures, 9 help pages, 40 events |
| filter-all | 2 | alone | 200 | 7.5 | 11.9 | 13.6 | 24.3 | 19.6 | 27.5 | 0 | - |
| filter-all | 2 | loading | 200 | 7.9 | 23.0 | 24.7 | 41.9 | 18.7 | 46.8 | 0 | 300 pictures, 9 help pages, 40 events |

The painted p95 of each pass, and its spread (the highest minus the lowest):

| Mode | While | Painted p95 by pass | Lowest | Highest | Spread |
|---|---|---|---:|---:|---:|
| filter | alone | 25.9, 24.5 | 24.5 | 25.9 | 1.4 |
| filter | loading | 48.8, 45.0 | 45.0 | 48.8 | 3.8 |
| protocol | alone | 28.6, 30.0 | 28.6 | 30.0 | 1.4 |
| protocol | loading | 64.5, 67.5 | 64.5 | 67.5 | 3.0 |
| filter-all | alone | 24.0, 27.5 | 24.0 | 27.5 | 3.5 |
| filter-all | loading | 47.6, 46.8 | 46.8 | 47.6 | 0.8 |

- Q-1, alone (painted p95, protocol against filter): filter lower in every pass, by 2.7 ms or more.
- Q-1, loading (painted p95, protocol against filter): filter lower in every pass, by 15.7 ms or more.

## 3. Supervision (AD39)

Times in ms from the moment the script acted.

| Case | Result |
|---|---|
| End the server process, filter mode | the shell saw the exit after 11.0, had a new server address after 23.0, and Home was loaded again after 75.0; new server: True, new port: True |
| End the server process, protocol mode | the shell saw the exit after 4.0, had a new server address after 19.0, and Home was loaded again after 95.0; new server: True, new port: True |
| Close the window (the shell was asked to close its window) | the server exited after 102.0 (its quit: quit-message); the shell exited |
| End the shell's process (server started plainly (Node's default spawn)) | the server exited after 1399.0; its quit: stdin-closed. Child processes before: electron, electron, tc4-standin, electron. Left after 5 s: none |
| End the shell's process (server started with detached: true) | the server exited after 1449.0; its quit: stdin-closed. Child processes before: electron, electron, tc4-standin, electron. Left after 5 s: none |

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
| filter | hard kill | 305 | 5.3 s after typing began | 101 | 305 | 101 | 10.2 s | 305 | 0.0 s | 305 (0) | exited after 1299.0 (quit: stdin-closed) |
| filter | normal quit (control) | 302 | 5.1 s after typing began | 101 | 305 | 306 | 0.0 s | 306 | 0.0 s | 306 (0) | exited after 285.0 (quit: quit-message) |
| protocol | hard kill | 303 | 5.1 s after typing began | 101 | 303 | 101 | 10.1 s | 303 | 0.0 s | 303 (0) | exited after 1272.0 (quit: stdin-closed) |
| protocol | normal quit (control) | 302 | 5.1 s after typing began | 101 | 302 | 303 | 0.0 s | 303 | 0.0 s | 303 (0) | exited after 124.0 (quit: quit-message) |

## 5. Graphite (informational)

Awami Nastaliq has Graphite tables and no OpenType substitutions: with Graphite its letters
join and cascade; without it they stand alone and the line is about twice as wide. In
Chromium 141 without Graphite (Playwright, `spikes/electronite/test`) the test text measured
639.8 px wide with an ascent of 46 px; Electronite 37.1.0 on Linux measured 302.9 px and
61 px. "Joined" below means narrower than 480 px. This is a smoke test, not the P8 probe.

| Mode | Font loaded | Width | Height | Canvas width | Ascent | Descent | Joined | Screenshot |
|---|---|---:|---:|---:|---:|---:|---|---|
| filter | True | 302.9 | 76.0 | 322.9 | 61.0 | 13.2 | yes | `graphite-filter-run.png` |
| protocol | True | 302.9 | 76.0 | 322.9 | 61.0 | 13.2 | yes | `graphite-protocol-run.png` |
| filter-all | True | 302.9 | 76.0 | 322.9 | 61.0 | 13.2 | yes | `graphite-filter-all-run.png` |

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

