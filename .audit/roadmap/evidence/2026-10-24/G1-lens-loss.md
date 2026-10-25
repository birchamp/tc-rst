# G1 review, loss lens (track-G1 at 8743dda1)

Read only. `node --test 'shell/test/*.test.cjs'`: 31 pass. Two scratch reproductions (below) in
`/tmp/claude-1000/-home-bot-tc-rst/e1bb3a0c-52ff-4492-ac09-19d371f55e29/scratchpad/`.

## Blocker

### B1. The bridge refuses every `bufferChanged` while tc4 restarts, and the reload goes to a new origin
- Where: `shell/bridge.cjs:150-153` (`trusted` compares the sender with `ctx.origin()`),
  `shell/main.cjs:74` (`origin()` is null when `server` is null) and `main.cjs:165-166`
  (`server = null` at tc4's exit). After the new tc4 names its address, `origin()` is the new
  port, and the old page is still on the old port until `reloadHome` loads.
- Scenario: a translator types a verse. tc4 crashes. The page does not know yet: the fields stay
  editable until the event stream reconnects (inference: I found no read-only mode on a lost
  server in `tc-ui`). The translator types a few more words. Each `bufferChanged` is logged
  "bridge call refused" and dropped. Then `reloadHome` loads Home at the new port.
- Why it loses text: an origin includes the port, and each tc4 launch has a new port (AD21). The
  reloaded page cannot read the old origin's `localStorage`. `lib.cjs:104-105` rests on the
  claim "the old page ... holds nothing the mirror does not". The refusal breaks that claim. The
  words typed between tc4's exit and the reload are in neither store the new page reads. The
  page's `pagehide`/`beforeunload` lines at the reload are refused too, because the origin is
  already the new port. This breaks INV-UNSAVED-TEXT-SURVIVES-A-CRASH ("after `tc4` dies") with
  one fault and no further crash. The same holds for Restarting → Stopped (`showStartError`
  replaces the page) and Restarting → close (`exitApp`).
- Reproduction: `restart.cjs` drives `bridge.register` with a fake ipcMain. The first line is
  appended. The line sent while `server` is null and the line sent from the old port after the
  new address are both "bridge call refused".
- Not tested: `ending_tc4_restarts_it_and_reloads_home` types nothing during the restart.
- Fix: trust the window's own page, not the current port. For example, check that
  `event.sender === win.webContents` and that the frame is on any origin this shell has served
  (keep a `served` set of origins and add each `server.origin`), or keep `lastOrigin` until the
  page has reloaded. Add an e2e test: hard kill tc4, type, wait for Home at the new port, then
  check that `takeBuffered` holds the words.

## Major

### M1. A full disk in the middle of a run drops the failed lines for good and spoils the next line
- Where: `shell/mirror.cjs:146-155`. `drain` takes the chunk out of `queued` before the write.
  On an error it only calls `onError`, so the chunk is never retried. `appendFile` may already
  have written part of the chunk, and nothing ends that partial line. The next append runs on
  from it, so the line formed there does not parse and is skipped at the next launch.
- Scenario: the disk fills while the translator types (in scope). Space comes back (a download
  finishes and is moved), and they type again. Then the shell or the computer stops.
- What the translator loses or sees: every line of the failed chunk is gone from disk, and so is
  the first line after the recovery. At relaunch the older line of that field is offered as the
  unsaved text, so the newest words are lost and stale text is shown. In the same run the page
  is fine, because the in-memory reduction (`append` → `reduction.add`) still answers
  `takeBuffered`. Nobody is told: `onError` only writes `shell.log`.
- Reproduction: `enospc.cjs`. After "old one" for TIT 1:1 is written, one chunk fails with
  ENOSPC after 20 bytes (TIT 1:2 "typed during full disk"), then "newest words" for TIT 1:1 is
  written. The relaunch offers only `['old one']`.
- The model test `random_mirror_histories_keep_every_unsaved_line` misses this. Its `crashingFs`
  stops for good after the cut, so it never writes again after a failed write.
- Fix: on a failed append, put the chunk back at the head of `queued` and mark the tail torn.
  Begin the next write with `\n`: an empty line or a torn line is skipped, and the lines around
  it still count (contract rule 6). Simpler still: after any failure, rewrite the whole file
  from `reduction.lines()` with `replaceAtomically` on the next try, and reopen the handle. Tell
  the page so the translator sees it, as `report_buffer_failure` does for `localStorage`. Extend
  the model with "fail, then recover, then write more".

### M2. If the launch rewrite fails, the whole run has no mirror and nobody is told
- Where: `shell/mirror.cjs:120-133` and `shell/main.cjs:388-394`. Any error in `readFile`
  (other than ENOENT), in writing the temporary file, or in `rename` rejects `openMirror`.
  `main.cjs` then sets `mirrorReady` to null. From then on `mirrorAppend` drops every line
  (`main.cjs:322`), `takeBuffered` returns `[]` (`main.cjs:326`), and `exitApp` flushes nothing.
- Scenario (reachable): on Windows, an antivirus, backup or indexing program holds
  `drafts-buffer.log` without FILE_SHARE_DELETE at launch, so `rename` fails with EPERM/EBUSY.
  Or the disk is full at launch, so the temporary file cannot be written.
- Why it loses or hides text:
  (a) This launch does not offer the last run's unsaved lines. They stay on disk, but only the
      mirror held them: a different port means a different `localStorage`.
  (b) Nothing typed in this run is mirrored. After any tc4 restart (B1's new-origin reload)
      or a shell or computer stop, the unsaved text of this run is gone.
  The only trace is the `shell.log` line "mirror could not open".
  A `.drafts-buffer.log.<pid>.tmp` is also left behind (minor).
- Fix: a failed rewrite is not fatal. Keep the reduction already read and open the existing
  file with `'a'`. Appends to an existing file still work in both cases. Retry the rewrite at
  the next launch. If even the append handle cannot open, keep the reduction in memory, retry
  the open on a timer, and tell the page so the translator sees a warning.

## Minor

- m1. `mirror.cjs:157-169, 195`: `flush()` awaits `sync()`, which hands back a sync already in
  flight if one started before the last write finished. That fsync does not cover the last
  write. At exit the last lines may then be unsynced, so a power cut within moments of quitting
  can lose them. Fix: in `flush`, wait for any running sync, then start a new one.
- m2. `mirror.cjs:125`: the whole log is read as one string. A run that lasts days (each
  keystroke writes a whole entry, which for a section is the section's text) could pass V8's
  string limit (about 512 M chars). After that, every launch fails to open the mirror, which is
  M2 for good. Reach is an inference: it depends on run length and section size. Fix: read with
  `readline` over a stream, or compact during the run once the file passes N MB.
- m3. `mirror.cjs:161-165`: a failed fsync is retried. On Linux a retried fsync after EIO can
  report success after the dirty pages were dropped. This is narrow; at least tell the page once.
- m4. `lib.cjs:112-113` (Restarting/Starting + closeRequested → Quitting with no action): if the
  new tc4 never prints its address, no quit timer runs, and the window can never close. No text
  is lost, but the app hangs until it is killed.
- m5. `lib.cjs:121`: Closing + serverExit runs `exitApp` while the "Unsaved changes" question
  may be open. The app closes although the translator may be about to answer "No". The text is
  in the mirror (once B1 is fixed), so nothing is lost, but the window vanishes without their
  answer.
- m6. The JS reduction (`parseLine`/`slotOf`) is looser than serde. It accepts any string for
  the kind, place, instance and ids, and it does not check `base` tokens, `sent` or `draft`. For
  lines the same build's page writes, the JS keeps exactly what `current_lines` keeps. I found
  no reachable line that Rust keeps and JS drops. The only way a newer JS-only-valid line could
  hide an older Rust-valid one is a version downgrade with a changed `SentSave`/`DraftCall`
  (they deny unknown fields). That is unreachable while the shell and the page ship together.
  Order differs (Map recency against BTreeMap order), and the contract allows that.
- m7. On Windows, `replaceAtomically` skips the folder fsync (`mirror.cjs:106`). A crash right
  after a launch can then bring the old log back. The old log holds a superset of the lines, so
  nothing is lost.

## Checked and fine
- The single-instance lock (`main.cjs:36`) comes before `openMirror`, so a second launch never
  rewrites the log under the first one's append handle.
- `mirrorReady` is set before `dispatch('ready')`, so no page exists before it. Appends chain on
  one promise, so their order is kept. `takeBuffered` waits for the open.
- Torn last line at launch: skipped, then removed by the atomic rewrite (temporary file, fsync,
  rename, folder fsync).
- NFC compare, removal with a text, and removal per run all match `current_lines`. All vectors
  pass.
- `exitApp` flushes and closes the mirror (with a 5 s bound) before `app.exit`.
  Quitting + quitTimeout kills tc4 only after the mirror holds the lines.

## FUNNEL §8 for this lens
1. Code: the lifecycle is one pure table and `dispatch` is the one entry point, which is good.
   The loss in B1 comes from a trust check tied to "the current server". A structure would
   carry it: a `servedOrigins` set, or trusting the shell's own `win.webContents`. M1 and M2
   need explicit error states in the mirror (Healthy / TornTail / Unavailable) instead of
   `onError` logging. Neither is carried today.
2. Checks: the invariant tests exist and run in CI (`shell.yml` job `unit`; e2e jobs). They miss
   typing during a tc4 restart (B1), a write failure followed by recovery (M1) and a failed
   launch rewrite (M2). The mutation proof covers the lifecycle and the fsync timer only.
3. Reach: B1 needs only a tc4 crash while the translator types. M1 needs a full disk that
   recovers, then a stop. M2 needs a file held by another program at launch, or a full disk.
   All are reachable. m2 is uncertain.
4. Siblings: the same `origin()`-is-null trust gap also refuses `setCanClose` and
   `takeBuffered` during Restarting. That is harmless now, but it is the same pattern.
   `shell:closeAnswer` uses the same `trusted` check.
5. No loss: this is one of the three lenses. Its result: one blocker and two majors, as above.
6. Prose/Human: the comments give reasons. The comment at `lib.cjs:104` says something the code
   does not make true (B1). One translator-visible question to raise: should the app warn when
   the typing mirror cannot write (M1, M2)? tC4 has no such message.
