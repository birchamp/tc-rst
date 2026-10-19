# The shell chrome and static surfaces (unit A5, 2026-10-19)

Unit A5 ports tC4's `App.jsx` and the account menu's surfaces (docs/APP-DESIGN.md §10, A5). This
file holds its measurements and the last runs of the browser suites it touched. The machine is
the owner's Linux PC (`docs/evidence/linux-machine-2026-10-18.md`): a 12th Gen Intel Core
i7-12700, 20 threads, SHA instructions yes (`sha_ni`), AVX-512 no. Other agents built on it at
the same time: the load average was 3.3 to 3.4 during the timings. The raw summaries are beside
this file, in `shell-chrome-2026-10-19/`.

The code measured is branch `p0-a5` at the commit that adds this file, whose tc-ui source is
that of `7796c76` ("Journeys for the shell…"). Builds: `crates/tc-ui/build-app.sh --release`
(the `wasm-release` profile) and the dev build, served by `tc4 serve` of `cargo build -p tc4
--features test-hooks`, in Chromium 141.0.7390.37 (Playwright 1.56.1).

Command: `TC4_BIN=target/debug/tc4 TC4_UI=<build> node crates/tc-ui/app/measure-shell.mjs --runs
20 --out docs/evidence/shell-chrome-2026-10-19/<build>.json`.

## The App language preview on Home (budget: 16 ms per frame, PERFORMANCE §8.2)

The App language dialog open over the stand-in Home, each run choosing French or English in turn
(the cold pair, whose first Arabic paint loads its faces, is left out). Two timings: from a
person's click on the language (Playwright's pointer input, the event's own time stamp) to the
dialog's title in the new language, and to the next animation frame; and the page's own work,
from a `click()` the page makes on the same row, so Chromium's routing of a pointer input to the
page is not in it.

| Release, 20 runs | median | p95 | max |
|---|---:|---:|---:|
| click to the title in the new language | 11.9 ms | 15.0 ms | 15.1 ms |
| click to the next frame | 13.9 ms | 17.1 ms | 17.2 ms |
| the page's work: `click()` to the new title | 8.1 ms | 9.4 ms | 10.0 ms |
| the page's work to the next frame | 11.1 ms | 15.3 ms | 16.4 ms |

Long tasks (over 50 ms, the Long Tasks API) during the forty previews: none.

The dev build, for comparison: click to the title 12.7 ms (p95 21.1), to the frame 14.8 ms (p95
23.2); the page's work 6.2 ms (p95 11.9), to the frame 11.0 ms (p95 15.9).

What the numbers mean:

- **The preview is drawn in the frame after the click.** "To the next frame" is quantized by the
  display's 16.6 ms period: the update is done before the frame callback runs, and the release
  p95 of the page's own work to the frame, 15.3 ms, is inside one period. The click-to-frame p95
  of 17.1 ms adds Chromium's input routing, about 3 ms here, which is not the page's work.
- **The page's work is about 8 ms, where A4a's stand-in switch was 0.3 ms**
  (`docs/evidence/ui-runtime-2026-10-18.md`). A preview here does more than change text: the
  list closes and gives the focus back to its button, which makes Chromium recalculate the styles
  of the whole page after `<html lang dir>` changed (tC4's CSS keys faces on `:lang`), and the
  dialog's select, title, hint, buttons and the top bar's account button change with it. A
  synthetic click with nothing else on the page gave 1.2 to 1.9 ms in a separate run; the
  difference is the work around the click, not the language switch.
- The figure on a full screen is B5b's to measure: the Check screen holds many more texts.

## The boot

Twenty loads of the page in one context, navigation start to the `tc4:mounted` mark: release
median 102.8 ms, p95 125.4 ms; dev 152.5 ms, p95 189.0 ms. A5 adds no request at boot (the
journey `boot_adds_no_request`: one `app/hello`, the nine catalogs, the event stream); the
mount now draws the App frame, the top bar and the account button where A4a drew the stand-in
alone.

## A help topic switch (tC4 #729)

Twenty clicks on the help page's own navigation, over five topics in turn, from the click to the
new page's heading in the frame: release median 29.6 ms, p95 42.4 ms (dev 29.1 ms, p95 42.9 ms).
The swap reads the page from `tc4 serve`, parses it and decodes its eager images before it
replaces the body. The journey's screencast of five switches on the release build
(`help-switches-release.json`, help case g) saw 18 painted frames and no blank one: each shows
the help bar and its logo, so nothing blinks white.

## Bytes

| | bytes |
|---|---:|
| `tc_ui_bg.wasm`, release, phase0 at `9086ef2` (built from `git archive`) | 1,438,872 |
| `tc_ui_bg.wasm`, release, with A5 | 2,176,892 |
| the probe build (`--test-hooks`), release | 2,398,067 |
| `help/`, 180 files, served only when Help opens | 21,880,431 |

A5 adds 738,020 bytes to the release wasm (51 %). Most of it is design-system code that no page
used before and the linker therefore left out: `AppHeader`, `Modal` and `Layer` with their focus
trap, `Select` with its list, search and keyboard, `TextArea`, `TextField`, `Switcher`,
`Callout`, `StatusDot`; then the views themselves, and tC4's `LICENSE` and `COPYING` (18,840
bytes of text, compiled in so About and License send no request, as in tC4). PERFORMANCE has no
size budget; the figure is for G2, which measures the installed app.

## Not measured

`feedback/compose`: it copies at most 50 small reports and formats one; no person waits on it
beyond the dialog's first frame, which opens before the reply.

## The last runs of the browser suites

All on the build of this commit (dev unless named):

- the journeys, `node --test parity/app/journeys/*.test.mjs`: 33 of 33 pass (shell 6, account
  menu 4, App language 9, Help 9, Feedback 5);
- A4a's smoke, `crates/tc-ui/app/smoke.mjs`: 95 checks, none failed (93 before A5, which added the
  restart's snapshot and its no-write check to case K);
- A4b's no-loss suite, `crates/tc-ui/app/no-loss.mjs`: 141 checks, none failed, on the dev and on
  the release builds;
- A9's prepare smoke with `--app-ui target/tc-ui-app`: every step, the new seventh included (the
  app's page went `/`, `/prepare.html`, `/`);
- `node --test` of the baseline, state, observe, env-rust (with its browser cases), view-gallery
  and recovery tests: 69 pass, 6 skipped (they need a tC4 checkout);
- the design-system gallery after D13: DOM 751 of 751 cases the same as tC4's React gallery,
  behaviour scenarios 53 of 53; `dom-report.json` and `scenario-report.json` unchanged.
