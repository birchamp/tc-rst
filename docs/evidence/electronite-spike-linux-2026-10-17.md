# Electronite spike G0 on Linux (2026-10-17)

This is the run in the build container, not on the reference laptop: Electronite's Linux build
under Xvfb as root with `--no-sandbox`, software rendering, on a shared 4-vCPU VM with a load
average of 3 to 4 from other agents' work. It tests the driver end to end and gives a Linux
baseline; the Windows numbers come from `.audit/tasks/g0-windows.md`.

This file was written by `spikes/electronite/run-spike.ps1`. The spike is throwaway and never
shipped (docs/APP-DESIGN.md section 10, unit G0). **The server is a stand-in**: `tc4-standin`
(`spikes/electronite/src`), a small axum server on 127.0.0.1 with the per-launch token, the
Host, Origin and X-Tc-Api checks of section 5.2, an address line on stdout and a quit on a closed stdin,
because the real `tc4 serve` (unit A2) is being built at the same time. Its loopback cost is not
tc4 serve's: unit A2 measures that. What this file measures is the shell, the browser engine and
the operating system around a server of the same kind.

- Commit: d3ad187c6f9f44250ac47948ce92d77d3d6506b1 on p0-g0
- Run: 2026-10-17 12:27 to 12:29 UTC; `-Clicks 200 -Warmup 20`
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
is a filter over every path.

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
| eventsource (EventSource (informational: AD10 does not use it)) | **fails** | **fails** | **fails** |
| download (an <a download> link) | passes | passes | passes |
| print (the shell's hidden print window and printToPDF) | passes | passes | passes |
| child-window (a same-origin window.open, kept in the app (§5.7, A-33)) | passes | passes | passes |
| api-hop (no /api/ request passes through the main process) | passes | skipped | skipped |

What each probe saw:

- filter, document: this page is running
- filter, script: this script is running; server: 1× 200 token ok
- filter, stylesheet: its rules apply; server: 1× 200 token ok
- filter, fetch: hello from instance 5ac452c18173; server: 1× 200 token ok
- filter, image: loaded, 640×360; server: 1× 200 token ok
- filter, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter, font: 1 face loaded; server: 1× 200 token ok
- filter, css-url: the element is styled; server: 1× 200 token ok
- filter, event-stream: HTTP 200, 5 frames, the first at 54 ms, the fifth at 412 ms
- filter, eventsource: error event; server: 1× 401 token missing. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter, api-hop: the filter (patterns with the port) saw 0 /api/ request(s) and 18 in all: {"root":1,"ui":6,"media":2,"help":7,"download":1,"print":1}
- protocol, document: this page is running
- protocol, script: this script is running; server: 1× 200 token ok
- protocol, stylesheet: its rules apply; server: 1× 200 token ok
- protocol, fetch: hello from instance bd33edba8383; server: 1× 200 token ok
- protocol, image: loaded, 640×360; server: 1× 200 token ok
- protocol, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- protocol, font: 1 face loaded; server: 1× 200 token ok
- protocol, css-url: the element is styled; server: 1× 200 token ok
- protocol, event-stream: HTTP 200, 5 frames, the first at 46 ms, the fifth at 406 ms
- protocol, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- protocol, download: completed, 1910 bytes saved; server: 1× 200 token ok
- protocol, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- protocol, child-window: the child window got HTTP 200; server: 1× 200 token ok
- protocol, api-hop: every request crosses the main process under protocol.handle (33 so far)
- filter-all, document: this page is running
- filter-all, script: this script is running; server: 1× 200 token ok
- filter-all, stylesheet: its rules apply; server: 1× 200 token ok
- filter-all, fetch: hello from instance 298074a86066; server: 1× 200 token ok
- filter-all, image: loaded, 640×360; server: 1× 200 token ok
- filter-all, iframe: the help page shows, 4 of 4 pictures; server: page 1× 200 token ok; style sheet 1× 200 token ok; pictures 4× 200 token ok
- filter-all, font: 1 face loaded; server: 1× 200 token ok
- filter-all, css-url: the element is styled; server: 1× 200 token ok
- filter-all, event-stream: HTTP 200, 5 frames, the first at 51 ms, the fifth at 406 ms
- filter-all, eventsource: error event; server: 1× 409 token ok. EventSource sends no header of its own: under AD11 /api/ gets no token (401), and it can never send X-Tc-Api (409)
- filter-all, download: completed, 1910 bytes saved; server: 1× 200 token ok
- filter-all, print: HTTP 200, a PDF of 15193 bytes; server: 1× 200 token ok
- filter-all, child-window: the child window got HTTP 200; server: 1× 200 token ok
- filter-all, api-hop: the filter (patterns with the port) saw 17 /api/ request(s) and 35 in all: {"root":1,"ui":6,"api":17,"media":2,"help":7,"download":1,"print":1}

## 2. The aligner round trip (AD18, Q-13)

One click: the page posts to `/api/v1/align/click` with its own fetch, gets about 20 KB of
JSON, parses it and rebuilds the word bank and the cards. "Reply" ends when the JSON is
parsed, "rendered" when the DOM is rebuilt, "frame" at the start of the next animation frame,
which paints it. Times in ms, nearest-rank, after the warm-up clicks. While loading: a
story's 15 OBS-sized pictures (640x360, about 70 KB) every 500 ms, the help page every second,
and the event stream open. The budget is 30 ms (PERFORMANCE.md, Aligning).

| Mode | While | Clicks | Reply median | Reply p95 | Rendered p95 | Frame median | Frame p95 | Frames missed | Loaded meanwhile |
|---|---|---:|---:|---:|---:|---:|---:|---:|---|
| filter | alone | 200 | 6.7 | 14.2 | 16.3 | 14.3 | 28.7 | 0 | - |
| filter | loading | 200 | 6.7 | 44.5 | 45.9 | 14.1 | 53.7 | 0 | 300 pictures, 9 help pages, 39 events |
| protocol | alone | 200 | 10.0 | 19.2 | 21.1 | 17.5 | 30.6 | 0 | - |
| protocol | loading | 200 | 10.0 | 71.6 | 72.4 | 16.6 | 79.3 | 0 | 345 pictures, 11 help pages, 45 events |
| filter-all | alone | 200 | 6.1 | 9.9 | 10.9 | 12.7 | 21.5 | 0 | - |
| filter-all | loading | 200 | 6.8 | 35.0 | 36.9 | 13.6 | 48.1 | 0 | 285 pictures, 9 help pages, 38 events |

## 3. Supervision (AD39)

Times in ms from the moment the script acted.

| Case | Result |
|---|---|
| End the server process, filter mode | the shell saw the exit after 10.0, had a new server address after 25.0, and Home was loaded again after 79.0; new server: True, new port: True |
| End the server process, protocol mode | the shell saw the exit after 6.0, had a new server address after 47.0, and Home was loaded again after 124.0; new server: True, new port: True |
| Close the window (the shell was asked to close its window) | the server exited after 27.0 (its quit: quit-message); the shell exited |
| End the shell's process (server started plainly (Node's default spawn)) | the server exited after 1412.0; its quit: stdin-closed. Child processes before: electron, electron, tc4-standin, electron. Left after 5 s: none |
| End the shell's process (server started with detached: true) | the server exited after 1316.0; its quit: stdin-closed. Child processes before: electron, electron, tc4-standin, electron. Left after 5 s: none |

## 4. A hard kill while typing (AD21)

The page types one letter every 50 ms. Each keystroke is written to `localStorage` at once,
sent to the shell, which appends it to its mirror file (synced at most once a second), and
reported to the server. The script ends the shell's process (or, as the control, asks the
shell to quit normally), then opens the same profile again (same origin) and reads what was
kept. Counts are keystrokes; "made" counts them from the clock up to the moment the script
acted, so a normal quit can keep a few more.

| Mode | How the shell ended | Made (from the clock) | The server saw | localStorage kept | The mirror kept | Mirror lines (torn) | The server |
|---|---|---:|---:|---:|---:|---|---|
| filter | hard kill | 57 | 57 | 0 | 57 | 57 (0) | exited after 1575.0 (quit: stdin-closed) |
| filter | normal quit (control) | 56 | 58 | 58 | 58 | 58 (0) | exited after 173.0 (quit: quit-message) |
| protocol | hard kill | 54 | 54 | 0 | 54 | 54 (0) | exited after 1261.0 (quit: stdin-closed) |
| protocol | normal quit (control) | 55 | 56 | 56 | 56 | 56 (0) | exited after 133.0 (quit: quit-message) |

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
- **Q-1, the custom scheme.** The design's rule: take `protocol.handle` if it adds less to
  the aligner's round trip than the filter, otherwise the filter. Compare the frame p95 of
  `protocol` and `filter`, alone and while loading, and the probe rows.
- **Q-13, the aligner fallback.** If the frame p95 of `filter` misses 30 ms on the reference
  laptop, Q-13 goes to the owner with these numbers.
- **AD39 and G1.** The restart rows show whether the shell restarts the server and reloads
  Home. The "end the shell" rows show whether the server quits by itself when the shell dies,
  or is ended with it, which decides how G1 starts `tc4`.
- **AD21.** The hard-kill rows show how many keystrokes `localStorage` and the shell's mirror
  keep when the whole app dies.

