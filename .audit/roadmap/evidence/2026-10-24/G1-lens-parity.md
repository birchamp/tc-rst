# G1 review: the browser and parity lens

Reviewer: independent verification lens (browser and parity). Read only; no file in the
repository was changed. Worktree `/home/bot/tc-rst/.worktrees/track-G1`, branch `track-G1`, head
`640bb0bc` (step 8). tC4 reference `/home/bot/unfoldingword/tc4-ref` at `d310bf3`, template
`/home/bot/Enjoy/tC4r-cdx/proof/app/electron/electronStartup.js`.

What I ran: `node --test 'shell/test/*.test.cjs'` (31 pass, 0 skipped). I did not run the e2e
suite or CI, and I did not drive Electronite. Every finding below comes from reading the code and
comparing it with tC4. The confidence of each one is given.

## Blocker

### B1. Text typed while tc4 restarts is dropped by the bridge and then lost at the reload

- Where: `shell/main.cjs:165-167` (`server = null` before `serverExit`), `shell/main.cjs:74`
  (`origin()` is null while no server runs), `shell/bridge.cjs:150-153` (`trusted` compares the
  sender's URL with `ctx.origin()`), `shell/bridge.cjs:159-163` (an untrusted call is dropped,
  `bufferChanged` included), `shell/main.cjs:277-280` (`reloadHome` loads the new origin).
- What happens: when tc4 ends, the shell sets `server = null`. Until the new tc4 prints its
  address, `origin()` is null. Every `bufferChanged` and `setCanClose` the page sends in that gap
  is refused ("bridge call refused"). Once the new tc4 is up, `origin()` is the new port, and the
  old page still runs on the old port, so its calls are still refused until `reloadHome`
  replaces it. The page sends a mirror line on every change (`crates/tc-ui/src/app/web/saves.rs:746`).
  Its other copy is `localStorage`, which belongs to the old origin (`127.0.0.1:<old port>`). The
  reloaded page, on the new port, never reads that copy. So the words the translator typed
  between the crash and the reload are in neither store the next page reads. They are gone
  without a word. The same gap is there before `Stopped` (three quick exits), where
  `showStartError` replaces the page.
- tC4: the template's server runs on a fixed port and is not supervised (`electronStartup.js:777+`),
  so this case does not exist there. It is new to the rebuild. Not declared.
- Why it is a blocker: it breaks INV-UNSAVED-TEXT-SURVIVES-A-CRASH and the no-loss rule in a case
  a translator can reach (tc4 crashes while they type). How much text is lost depends on how long
  tc4 takes to restart: up to seconds on a large project.
- Confidence: high on the mechanism (three code paths, each plain). Not reproduced: no e2e test
  types during `Restarting`.
- Fix (small): keep a set of every tc4 origin of this launch (add one at each address line,
  never remove one). In `trusted`, accept any origin in that set. A mirror line names its run,
  so lines from the old run are reduced correctly. Then add an e2e case to
  `ending_tc4_restarts_it_and_reloads_home`: hold the restart (for example a `--test-seed-pace`
  start, or a tc4 that waits before it prints its address), type in the probe field, and after
  the reload assert that `takeBuffered()` holds the text. Search for the same pattern in the
  other callers of `ctx.origin()`: `appUrl` (printPdf) and `linkTarget` in the window-open
  handler (see m9).

**Late note on B1.** While I was writing this review, an uncommitted change appeared in the
worktree (`git diff HEAD`): `shell/bridge.cjs` `trusted` now checks `ctx.served()`, a set of
every origin this launch served (`shell/main.cjs`, `served`). That is the fix proposed above for
the bridge. Still open: (a) it is not committed; (b) no e2e test types during `Restarting`;
(c) `openExternal` and `appUrl` in `bridge.cjs:37,93` and the window-open handler still use the
live `ctx.origin()` (m9). I reviewed the head commit `640bb0bc`. I did not re-review the rest of
the uncommitted diff, which also touches lib.cjs (sendQuit on a close in Starting and
Restarting), the quit timer (6 s) and mirror.cjs.

## Major

None beyond B1. Every other difference I found is either declared, not reachable today, or
too small to harm a translator's data or what they see (each is listed under Minor).

## Minor

Each line gives the rebuild's file:line, the tC4 file:line, and a fix or a declaration to add.

- **m1. The close question's default button is No; tC4 sets none.** `shell/main.cjs:238-239`
  (`defaultId: 1, cancelId: 1`) vs `electronStartup.js:805-820` (no `defaultId`, no
  `cancelId`). The words, the title and the Yes/No buttons match tC4 exactly. With tC4's options
  Enter can answer Yes on some systems; in the rebuild Enter keeps the window open. That is the
  safer choice for a translator's text. Declare it in APP-DESIGN §5.7 next to the A4b
  close-guard difference.
- **m2. Spell check is off; tC4 4.0 leaves Electron's default (on).** `shell/lib.cjs:309`
  (`spellcheck: false`) vs `electronStartup.js:777-789` (no `spellcheck`, so it is on; tC4's
  `src/` never sets `spellcheck` either). In tC4 on Windows and macOS, text fields show red
  underlines from the system's dictionary, often under every word of a minority-language
  draft. In the rebuild they never show. A translator who used tC4 would notice. Turning it off
  is defensible: Electron's Linux spell checker downloads dictionaries from Google, which the
  no-network rule forbids. But the difference must be declared, with that reason.
- **m3. The window shows at 1024x768 and then maximizes; tC4 maximizes before it shows.**
  `shell/main.cjs:183-184` (`show: true` from `lib.windowOptions`, then `win.maximize()`) vs
  `electronStartup.js:797-804` (`show: false`, then `maximize()` and `show()` at
  `ready-to-show`). The owner's ruling (show within 1 s, no splash) covers showing before
  ready-to-show, so that part is declared. The jump in size is not part of the ruling and can be
  seen on Linux and Windows. Fix: create the main window with `show: false`, then call
  `maximize()` and `show()` straight away in the same tick. That is still within 1 s.
- **m4. The first paint is #ffffff; the app background is #F7FAFC and prepare.html's is
  #f5f7fa.** `shell/lib.cjs:324` vs `crates/tc-ui/assets/ds/tokens/colors.css:22` and
  `crates/tc-ui/app/prepare.html:3`. §5.3 and PERFORMANCE.md:139 say the window is "painted in
  the app's background colour". Fix: `backgroundColor: '#F7FAFC'`.
- **m5. No window icon on Linux, and no AppUserModelId on Windows.** `shell/lib.cjs:318-328`
  has no `icon`, and `shell/main.cjs` never calls `app.setAppUserModelId`. tC4 sets
  `icon: favicon.png` (`electronStartup.js:784`) and
  `app.setAppUserModelId('org.unfoldingword.translationcore4')` on win32
  (`scripts/desktop-main.cjs:106`). On Linux the taskbar and Alt+Tab would show Electronite's
  default icon. On Windows a pinned shortcut may group apart from the running window. Fix: add
  both, or file a REQUEST for G3 if the icon file belongs to packaging.
- **m6. The prepare page and the start-error page have no drag area on Windows and macOS.**
  `shell/lib.cjs:312-317` gives the main window a hidden title bar on win32 and darwin, and
  `crates/tc-ui/app/prepare.html` and `lib.startErrorPage` (`shell/lib.cjs:381-391`) have no
  `<header>`, so `ui.css:24`'s drag rule never applies. tC4 showed preparation in its own
  520x240 window with the native frame (`scripts/desktop-prepare.cjs:29-30`), and start failures
  as a native error box (`scripts/desktop-main.cjs:175-181`). §5.3 declares that the preparation
  page loads in the main window. Not declared: a translator who un-maximizes the window during a
  first launch cannot move it. Fix: give both pages a drag strip (`-webkit-app-region: drag` on a
  top band). For prepare.html that is a REQUEST, because the file is in `crates/**`.
- **m7. Same-origin child windows get the main window's hidden frame on Windows and macOS.**
  `shell/main.cjs:312` (`openInApp`) and `shell/main.cjs:350` (window-open `allow`) both use
  `lib.windowOptions({ platform })`. That gives a help window the overlay controls on Windows and
  the traffic lights on macOS over a help page that has no header and no drag area. tC4 never
  opened app windows (`scripts/desktop-main.cjs:128-131` sends everything out, A-33). Not
  reachable today: the page sends only server-checked Door43 links to `openExternal`
  (`crates/tc-ui/src/app/web/app_ctx.rs:530,1276`), and `help-shim.js:85-99` keeps help links
  in the frame. Fix when it becomes reachable: child windows keep the native frame (a `child`
  option that skips the frame block).
- **m8. The PDF answer's shape and file name are not in the contract.** `shell/bridge.cjs:54-78`
  returns `{ok:true,file}` or `{ok:false, reason:'refused'|'failed'|'cancelled'}`, names the file
  from the print page's `<title>`, writes with a plain `writeFile` and does not use
  `onDownloadDone`. tC4 (`scripts/desktop-main.cjs:18-30`, `src/data/export/pdf.ts:120-135`)
  returns the bytes. Its page delivers them as a download named `exportFilename(book,'pdf')`
  through Electron's own save dialog, and the "Saved {file}" toast comes from `download:done`.
  A thrown error becomes `export.print-failed(-pictures)`, which tells the person to open the
  app again. `printPdf(url)` is declared (§5.5, §5.7). The answer shape, the name and the lost
  error text are not. Not reachable until D2. Fix: REQUESTs for (a) SHELL.md item 5 to name
  printPdf's answer, (b) D2's print page `<title>` to equal tC4's `exportFilename` stem, and
  (c) D2 to map `failed` to `export.print-failed(-pictures)`. Optionally return the error message.
- **m9. Other callers of `origin()` misroute while tc4 restarts.** `shell/main.cjs:347-353`: with
  `origin()` null, `linkTarget` sends a same-origin `window.open` to the system browser, which
  is exactly A-33's 401 page. That is the sibling of B1, and the same origin set fixes it.
  Rarely reachable.
- **m10. A navigation off the origin is dropped without a word; subframes are not guarded.**
  `shell/main.cjs:340-345`. `will-navigate` fires only for the main frame. tC4 has no origin
  guard at all (`electronStartup.js:825-840` asks only about unsaved changes). In the rebuild, a
  plain `<a href="https://...">` without `target` does nothing, where it could go to the system
  browser. A help iframe that navigates off the origin is stopped only by `help-shim.js`.
  Neither case is reachable today. Fix: send http(s) to `shell.openExternal` in the
  `will-navigate` guard as well, and add `will-frame-navigate` (Electron 37 has it) with the
  same rule.
- **m11. Feedback answers `offline` before `not-configured`.** `shell/bridge.cjs:102-104` vs
  `scripts/desktop-feedback.cjs:34`, where tC4 answers `not-configured` first. The net/state
  check itself is declared (Q-2, §5.7). The order matters only in a build without a key and
  with the switch off. Fine to keep; mention it in the step 7 log line.
- **m12. The right-click text menu is new in 4.0.** `shell/lib.cjs:371-378` and
  `shell/main.cjs:201-204`. tC4 4.0's template has no `context-menu` handler (grep of
  `electronStartup.js`). The card cites tC4 #833.4, a later tC4 fix. Record it as an adopted
  later fix in the declared differences, so the parity captures do not flag it.
- **m13. The test names claim more than the tests assert.**
  - `window_is_painted_within_1_s_of_the_click` (`shell/e2e/lifecycle.test.cjs:9-26`) asserts
    neither the paint nor the 1 s. It asserts that the "window shown" log line comes before
    "tc4 started". That holds by construction, because `showWindow` runs before `spawnServer`
    in one action list (`lib.cjs:61`). The `window painted` (`ready-to-show`) line is logged at
    `main.cjs:205` but never checked. The 2026-10-23 ruling says to measure speed, not assert
    it, so: assert that the `window painted` event happens, rename the test
    (`window_shows_before_tc4_answers_and_paints`), and print the time.
  - `ending_the_shell_ends_tc4_within_5s` waits up to 60 s (`:80`). Rename it, or state the
    ruling in the test.
  - `windows_window_has_no_native_bar_...` checks the overlay but not that the menu row is
    absent.
  - On every system the zoom and reload keys are proven only through the menu items' `click`,
    not through key presses. The step 7 log records this.
- **m14. Downloads and their report to the page have no end-to-end proof.** The filter probe's
  download is a 404 (`shell/e2e/filter.test.cjs:62-66`), so `will-download` / `done` /
  `shell:downloadDone` never runs end to end. The code matches tC4 line for line
  (`scripts/desktop-main.cjs:38-49` vs `shell/main.cjs:381-387`). Add a case once a real
  `/download/` exists (a REQUEST tied to an export producer).
- **m15. The Windows pid helper is more fragile than the Linux one.** `shell/e2e/harness.cjs:105-111`
  does not exclude the Electron process, as the Linux branch does at `:115`
  (`!/electron|main\.cjs/`). The Electron command line holds the marker and `tc4.exe `, so it is
  left out only because it lacks the word "serve". Today's paths do not contain that word
  (`D:\a\tc-rst\tc-rst`). Add the same exclusion. In passing, `no-skips.mjs` prints "app-e2e" in
  the shell job's output.

## Matches tC4, or declared (no action)

- One instance: the lock is taken before anything else (`main.cjs:35-39`), and the second copy
  exits 0. On `second-instance` the window is restored, shown and focused (`main.cjs:281-287`
  vs `desktop-main.cjs:134-140`). The lock is per app-data folder rather than tC4's default
  profile; with the installed launcher this is per user (INV-ONE-INSTANCE).
- Window size 1024x768, minimum 900x600, maximized: same as `electronStartup.js:777-783`.
- Frames: Linux native with the menu row (tC4's `autoHideMenuBar: false`). Windows uses
  `titleBarStyle: 'hidden'` with a 56 px #014263 overlay and white symbols. macOS is hidden with
  the traffic lights at 20,21. All three are exactly `package-desktop.zsh:383-385`. The preload's
  `data-title-bar` mark is identical to `scripts/preload.cjs:28-33`.
- Menus: Edit, View (Default Zoom Ctrl/Cmd+0, zoomIn, zoomOut), Window (Reload, Force Reload,
  Toggle Developer Tools), plus the macOS app menu. These match `InitializeMenu`
  (`electronStartup.js:560-640`) item for item.
- Close question: the title, message and buttons are tC4's own words. Asking the page first and
  then asking after 5 s is declared (§5.7, A4b).
- Outside links go to the system browser, and same-origin links stay in the app: declared (§5.7,
  A-33).
- Downloads report `{filename, state}`: same as tC4.
- Feedback: SendGrid, the same mail body, subject, reply-to, attachment, 30 s timeout and
  reasons. `help-desk.key` replaces `tc4-helpdesk.json` in the same JSON form (SHELL.md
  item 11). The internet switch check is declared (Q-2).
- `disable-dev-shm-usage` on Linux (#451), the print window with JavaScript off, and
  `preferCSSPageSize`: same as tC4.
- macOS CI job: left out, with the reason recorded (H3 open; card log, 08:30 note).

## End-to-end tests and shell.yml

- The tests really drive the shell. `harness.cjs:125-137` launches Electronite through
  Playwright's `_electron.launch` on `shell/main.cjs`, over a real tc4 and the real UI builds.
  The tests check the main process with `app.evaluate` and the page with `page.evaluate`. They
  type with `page.keyboard` and kill processes with SIGKILL or `taskkill /F`. Apart from m13,
  the assertions match the names.
- Zero skips: `shellTest` skips only when a variable or a build is missing. Both e2e jobs set
  every variable, including `TC4_BUNDLE`, and the builds artifact restores the three folders
  under `target/` (the common root of the uploaded paths is `target`). A test for one system
  exists only on that system (`window.test.cjs:56,71,87`), so it is absent elsewhere, not
  skipped. `no-skips.mjs` fails a run with no summary or with any skip.
- Linux: `chrome-sandbox` is made setuid root, as tC4's PACKAGING.md says. `--no-sandbox` is
  added only when running as root. Runs under `xvfb-run -a`. Fine.
- Windows: steps use bash, and the Electronite step uses pwsh with its own sha256 check. Paths
  go through `path.resolve`. Processes are listed with `Get-CimInstance` and killed with
  `taskkill /F`. The mirror skips the folder fsync on win32 (`mirror.cjs:106-107`).
  `detached: true` with `windowsHide` keeps tc4 out of the job object (AD39). Nothing I read
  would break on windows-2025 except the fragility in m15. Not verified: the Windows job has not
  run in this review, and no Windows result is in the card log yet.

## FUNNEL §8, for this lens

1. **Code.** B1 has a structural fix: one place that knows every origin this launch served,
   instead of the live `server` variable. Everything else is already one table (`BRIDGE`), one
   `windowOptions`, and one `dispatch`.
2. **Checks.** Each invariant has a named test, and the log shows a failing mutation for each.
   Gaps: no test types during `Restarting` (B1), the paint and 1 s are not asserted (m13), and
   there is no real download (m14). The macOS frame test never runs in CI (declared).
3. **Reach.** B1 can be reached (tc4 crashes while the person types). m7, m8, m9 and m10 cannot
   be reached today, so they stay minor.
4. **Siblings.** The other `origin()` callers were checked: `appUrl` (printPdf) and the
   window-open handler (m9).
5. **No loss.** The kill-while-typing and close-asks journeys pass according to the log. B1 is
   a loss path neither covers.
6. **Prose and Human.** The comments give reasons. For the owner (things a translator would
   notice): spell check off (m2), the close question's default button (m1), and the new text
   menu (m12). Each needs a declared-difference line rather than a question, unless the owner
   wants spell check back.
