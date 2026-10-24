# G1 review, lens "races and protocol"

Reviewer: independent, read only. Branch track-G1 at 8743dda1. Files: shell/lib.cjs, shell/main.cjs,
shell/bridge.cjs, shell/preload.cjs, shell/mirror.cjs, shell/test/*. `node --test 'shell/test/*.test.cjs'`:
31 pass, 0 fail. I did not run the e2e tests or cargo.

Ranking: no blockers, 2 majors, 9 minors. A note about the token follows, then the six FUNNEL §8 answers.

## Majors

### M1. Text typed between a tc4 crash and the reload of Home is refused by the bridge and lost

- Where: shell/bridge.cjs:150-153 (`trusted` compares the sender with `ctx.origin()`), shell/main.cjs:74
  (`origin()` is null while `server` is null), main.cjs:165-167 (`server = null` at tc4's exit),
  main.cjs:149 (`server` gets the new origin at the new address line), main.cjs:277-280 (`reloadHome`).
- Sequence: the translator is typing in a verse. tc4 crashes. The exit handler sets `server = null` and
  dispatches `serverExit`, which moves Running to Restarting and spawns a new tc4. The page is still the
  old editor, and the translator keeps typing. Each keystroke sends `bufferChanged`. `trusted` compares
  the old origin with `origin()`, which is null, so the call is refused ("bridge call refused") and
  the line never reaches the mirror. The new tc4 prints its address. From then on `origin()` is the new
  port, which tc4 picks at random (tc-server/src/lib.rs:199), so the old page's lines are still refused.
  `reloadHome` then replaces the page. The page's `localStorage` copy is filed under the old origin, and
  the new origin's page never reads it.
- Effect: everything typed after the crash and before the reload is gone. Nothing is on disk, and
  nothing is offered at the next Home or the next launch. The same thing happens when the restart ends
  in Stopped (the start-error page replaces the editor) and when Restarting or Preparing loads the
  prepare page. The comment at lib.cjs:108, "The old page cannot reach the new port, so it holds
  nothing the mirror does not", is true only if the mirror accepts that page's lines, and it does not.
  This breaks INV-UNSAVED-TEXT-SURVIVES-A-CRASH for this window. The window is the whole Restarting
  time (spawn, store open, address, app/hello) plus the reload, normally under a second, so I rank it
  major, not blocker.
- Fix: in main.cjs, keep the set of origins this shell process has served. Give the bridge a
  `trusted` that accepts a sender when `event.sender === win.webContents` and the frame's origin is in
  that set, at least for `bufferChanged`, `setCanClose` and `closeAnswer`. Keep `openExternal`,
  `printPdf` and the file choosers on the current origin. Add a bridge unit test,
  `a_line_typed_while_tc4_restarts_reaches_the_mirror`, and an e2e case: kill tc4, type, and check that
  the reloaded Home offers the text.

### M2. A tc4 that names its address and then hangs leaves a window that cannot close

- Where: main.cjs:96-106 (`query` has no timeout), main.cjs:152-157 (`address` is dispatched only after
  `app/hello` answers), lib.cjs:68 and lib.cjs:109 (closeRequested in Starting or Restarting goes to
  Quitting with no action, so no quit timer is armed), main.cjs:291-293 (`killServer` kills only an
  addressed `server`).
- Sequence: at launch, or during a restart, tc4 prints its address line, then stalls before it answers
  `app/hello` (a slow disk, a deadlock in a large project). The window is blank white in Starting, or
  shows the dead old page in Restarting. The translator closes the window. That goes to Quitting with
  no `sendQuit` and no `quitTimer`. `address` never comes, so nothing is sent and nothing is timed.
  Every later close, Cmd+Q and second launch is answered with "stay" or "focus".
- Effect: the window does not close, and a new double-click only focuses the same stuck window. The
  translator has to end the app in the Task Manager. A slow or hung tc4 is reachable. The model test
  never offers a tc4 that neither prints nor exits, and it does not check that a close always ends.
- Fix: give `query` a timeout (`AbortSignal.timeout(5000)`; a null answer already maps to
  'preparing'). Make every entry into Quitting arm the 5 s timer, for example by having `sendQuit`
  start the timer even when no server is set. Make `killServer` also kill the child that has not yet
  printed its address (keep it in `pending`). In the model, add "a starting tc4 never answers" and a
  liveness check: after `closeRequested`, `exitApp` follows within a bounded number of timer events.

## Minors

1. Late page answer after the 5 s timeout opens a second "Unsaved changes" dialog. main.cjs:217-219
   and main.cjs:222-228: on `pageTimeout`, `closeAsk` is not raised, so the late answer still matches.
   Sequence: close; the page takes more than 5 s to answer (slow saves); dialog 1 opens; the page then
   answers `clean: false`; Closing.pageAnswered runs askUser, and dialog 2 opens. The translator answers
   No to dialog 1, which returns to Running. If they then click Yes on dialog 2, nothing happens
   (Running.userAnswered: stay). If the late answer is `clean: true` instead, the app quits while
   dialog 1 is still open; that loses nothing, because the page reported itself saved. Fix: `++closeAsk`
   when the timer fires.
2. A reload (Ctrl+R) while the page is being asked to close. main.cjs:192-197 clears `closeHandler`,
   but the timer still runs. The old page's answer never comes, and after 5 s the translator gets
   "Unsaved changes" for a page that was just reloaded. Fix: on main-frame navigation during Closing,
   answer `pageAnswered` with the reset `canClose`, or ask the page again once it registers.
3. The quit timer equals tc4's own bound. main.cjs:22 (`QUIT_MS = 5000`) against SHELL.md item 3 ("a
   close takes at most 5 s"). The timer starts at the write, so a tc4 that uses its full 5 s can be
   SIGKILLed during its last write. The atomic writes keep the files whole, but the final flush can be
   lost. Fix: a margin, for example 7 s.
4. After a long-running crash, only two quick restarts are allowed. lib.cjs:44-48: an exit that was not
   quick sets `quickExits = 1`, not 0. Sequence: tc4 runs an hour and crashes (1), then crashes within
   10 s (2), then again (3), and the shell stops. That is three exits, but only two came within 10 s
   of a restart. The card says "three times within 10 s of each restart". Fix: a non-quick exit resets
   the count to 0 and does not count, or record the rule as intended.
5. A restart that meets a held app lock loops for ever. lib.cjs:106 sends Restarting.startError through
   `afterExit`, and a lock wait lasts 10 s (tc4/src/lib.rs:27), so the exit is never "quick". If an
   orphan tc4 keeps the lock (hung after a shell crash), the shell restarts every 10 s for ever and
   shows the dead page. Starting shows the start-error page in the same case. Fix: send exit 3
   (appLockHeld) straight to Stopped.
6. Links during Restarting go to the system browser. main.cjs:347-353 and lib.cjs:292-296: while
   `origin()` is null, or is the new origin, an old-origin `target=_blank` link (help) gets
   `linkTarget` 'browser', and `shell.openExternal` opens `http://127.0.0.1:OLD/help/...`, a
   connection-refused page (an A-33 variant). No token travels, because the link has no fragment. The
   restart after Preparing does not run `reloadHome` (lib.cjs:54), so old-origin child windows also
   stay open there, and their links behave the same way. Fix: never send a 127.0.0.1 URL to the system
   browser (refuse it), and destroy other windows on every restart.
7. A second launch while the first is quitting is lost. lib.cjs:142 focuses the quitting window, and
   main.cjs:36-38 makes the second launch exit 0. Quitting can last up to 5 s for tc4 plus 5 s for the
   mirror flush. The translator closes, double-clicks again at once, sees the window vanish, and nothing
   opens. Fix: in Quitting, set a flag and call `app.relaunch()` before `app.exit`, or record it as
   accepted.
8. tc4 exiting while "Unsaved changes" is open quits without the translator's answer. lib.cjs:122. The
   typed lines are in the mirror (except for M1), so the effect is only that the window closes over an
   open question. Record it as intended, or restart instead.
9. Model test gaps. shell/test/lifecycle.test.cjs:166-169: the model's `killServer` kills every alive
   tc4, including one that is still starting, but main.cjs kills only `server`. The model has no tc4
   that never answers (M2), no late page answer after a timeout (minor 1), and no notion of which origin
   the bridge accepts (M1). It also never checks liveness (a close ends the run).

## Token (INV-THE-TOKEN-NEVER-LEAVES-THE-APP): no finding

- The filter is replaced at every address line (main.cjs:110-116). Electron keeps one
  `onBeforeSendHeaders` listener, so the old origin loses the filter once the new tc4 answers. Between
  the crash and that point, the old filter adds the dead token only to the dead port. The listener
  re-checks `wantsToken` against its own `s.origin`.
- Log lines: `log()` redacts the whole JSON line, both `#t=` and every token seen (main.cjs:51-54).
  The tc4 stderr chunks arriving before the address line are not covered by the token list, but tc4
  never logs the token (`token_never_logged`).
- System browser: `#t=` URLs are built only by `pageUrl` for `loadURL`. Links given to `openExternal`
  carry no fragment. Referer drops fragments.
- Chromium's on-disk profile (`shell-profile`): not checked. Electron keeps no history database, as far
  as I know (unverified).

## Other paths checked that hold

- At most one tc4: `spawnServer` runs only at `ready` and after the previous tc4 exited or failed.
  `failed` is guarded by `reported`. The `exit` of a non-current child is ignored.
- None left after the shell ends: every `exitApp` follows an exit or a SIGKILL. A shell crash closes
  tc4's stdin, and tc4 quits within 5 s.
- tc4 exits before its address line: in Starting, Stopped; in Restarting, a restart through
  `afterExit`; in Quitting, `exitApp`.
- tc4 exits after its address line but before `app/hello` answers: serverExit (`server` is already set),
  and the hello `.then` checks `server.child` and does nothing.
- Exits during Closing and during Quitting, two close requests, and stale page and quit timers across
  rounds: guarded (apart from minor 1).
- A second launch at every moment: the lock is taken before anything else, the second copy spawns no
  tc4, and focus works in every state (`a_second_launch_focuses_the_window_in_every_state`).
- IPC from another frame: refused unless it comes from the tc4 origin. Child windows get no preload in
  their options. Whether Electron passes the parent's preload to `window.open` children is unverified.
  It matters only if a child page calls `onCloseRequested` or `setCanClose`, and help pages do not.

## FUNNEL §8, for this lens

1. Code: the lifecycle is a table with one entry point. M1 and M2 live outside it, in `trusted`, the
   timers and the hello fetch. A structural fix exists for each and is missing. For M1, the bridge
   trusts "the shell's window and the origins this process served", one value in main.cjs. For M2,
   entering Quitting always arms the timer (an `armQuit` action on every edge into Quitting). Both are
   majors under rule 1.
2. Checks: `every_state_answers_every_event`, `random_lifecycles_end_with_no_tc4_running` and the
   restart, close and focus tests exist and pass. They run in `.github/workflows/shell.yml`. The model
   is seeded and prints the seed. There is no test for M1, M2 or minor 1, and the model has no liveness
   check.
3. Reach: every finding above is a crash or a hang of tc4, a slow page answer, Ctrl+R, a second
   double-click or a help link. I dropped a hand-edited `localhost` URL and garbled stdout from tc4.
4. Siblings: the same origin-null gap reaches `will-navigate` (refuses, which is fine),
   `setWindowOpenHandler` (minor 6) and `openExternal` (refused, which is fine). The missing timeout of
   `query` also affects `internetOn` (the feedback send waits without limit), a minor of the same root.
5. No loss: M1 is a no-loss hole that the model and the e2e `hard_kill_while_typing...` do not reach,
   because neither types after tc4 ends and before Home reloads.
6. Prose and Human: the comment at lib.cjs:108 states a reason that does not hold (M1). Fix the code,
   and the comment becomes true. Minors 4, 7 and 8 are translator-visible choices. Each needs a recorded
   decision or an owner question.
