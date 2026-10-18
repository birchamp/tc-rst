# Electronite spike G0 on Linux (2026-10-17)

The hard-kill case only, run again after the shell's mirror moved its appends and fsync off the
main process's UI thread (AD21, AD11). The other cases, and the Electronite 37.1.0 and Chromium
versions, are in `electronite-spike-linux-2026-10-17.md`, whose hard-kill rows were taken with the
mirror writing synchronously on that thread. Same container, same Electronite build.

This file was written by `spikes/electronite/run-spike.ps1`. The spike is throwaway and never
shipped (docs/APP-DESIGN.md section 10, unit G0). **The server is a stand-in**: `tc4-standin`
(`spikes/electronite/src`), a small axum server on 127.0.0.1 with the per-launch token, the
Host, Origin and X-Tc-Api checks of section 5.2, an address line on stdout and a quit on a closed stdin,
because the real `tc4 serve` (unit A2) is being built at the same time. Its loopback cost is not
tc4 serve's: unit A2 measures that. What this file measures is the shell, the browser engine and
the operating system around a server of the same kind.

- Commit: ccd4f9f7af6000aa0456fb15c0491b7ab0edcef5 on p0-g0 (with local changes)
- Run: 2026-10-17 19:04 to 19:05 UTC; `-Clicks 200 -Warmup 20`
- Electronite and Chromium versions: unavailable (no run case finished)
- Raw results: `./docs/evidence/electronite-spike-linux-2026-10-17-hardkill` (results, events and screenshots; the full run is in `/tmp/claude-0/-home-user-tc-rst/4a142024-63e1-59b8-bcc4-3b15bbfdf386/scratchpad/g0fix/hk-out`)

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

## 4. A hard kill while typing (AD21)

The page types one letter every 50 ms. Each keystroke is written to `localStorage` at once,
sent to the shell, which appends it to its mirror file off its main thread (synced at most once a second), and
reported to the server. Chromium writes `localStorage` to the profile some seconds after the
page sets it. The script waits until the profile's files on disk hold the draft (the "first
write to disk" column), lets the page type 10 s more, then ends the shell's process (or, as the
control, asks the shell to quit normally), opens the same profile again (same origin) and
reads what was kept. Counts are keystrokes; "made" counts them from the clock up to the moment
the script acted, so a normal quit can keep a few more. "Lost" is the typing time missing at
the end: (made - kept) x 50 ms.

| Mode | How the shell ended | Made (from the clock) | First write to disk | On disk just before | The server saw | localStorage kept | localStorage lost | The mirror kept | The mirror lost | Mirror lines (torn) | The server |
|---|---|---:|---|---:|---:|---:|---:|---:|---:|---|---|
| filter | hard kill | 303 | 5.2 s after typing began | 101 | 303 | 101 | 10.1 s | 303 | 0.0 s | 303 (0) | exited after 2017.0 (quit: stdin-closed) |
| filter | normal quit (control) | 305 | 5.3 s after typing began | 101 | 309 | 309 | 0.0 s | 309 | 0.0 s | 309 (0) | exited after 284.0 (quit: quit-message) |
| protocol | hard kill | 303 | 5.2 s after typing began | 101 | 303 | 101 | 10.1 s | 304 | 0.0 s | 304 (0) | exited after 1136.0 (quit: stdin-closed) |
| protocol | normal quit (control) | 302 | 5.1 s after typing began | 101 | 304 | 305 | 0.0 s | 305 | 0.0 s | 305 (0) | exited after 223.0 (quit: quit-message) |

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

