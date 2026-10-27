# PERF-LAPTOP: budgets measured on the reference laptop, 2026-10-27 (step 1)

Part 1 is the project-store rows (`storebench`). Part 2 is the release timing tests, the app benches and the installed unsigned 4.1.0-preview app. "Not yet measured" says what is left.

## Machine and build

- Reference laptop: Windows 10 Pro, Intel Core i5-8350U (4 cores, 8 threads, no SHA, no AVX-512),
  8 GiB, WDC WDS100T2B0B SSD. Defender real-time protection on, no exclusions.
- Build: commit `003aa087` (tip of `claude/focused-ride-5uzj7p`), `cargo build --release -p tc-store
  --example storebench`, Rust 1.99.1 MSVC. This is the workspace's release profile, not the
  4.1.0-preview installer: the store rows do not need the installer.
- Corpus: `storebench setup` on the whole Bible, 617,086 changes (`Spec::whole_bible(1)`). All runs
  are warm: the files were just written, so the Windows file cache holds them. A cold run after a
  reboot is not done.
- Memory: `storebench` reads Linux's `/proc`, so it prints 0 MiB here. The memory figures below
  come from sampling the process's private bytes once a second from PowerShell.

## Rows measured

| Row | Budget | Figure | Result |
|---|---|---|---|
| Open with a usable fold cache | 3 s | median 2.71 s, p95 3.15 s (7 runs) | met at the median; p95 is 0.15 s over |
| Open with no fold cache | 10 s | median 6.60 s, p95 7.24 s (7 runs) | met |
| Materialize one book (Psalms) after a verse save | 1 s p95 | median 2.22 s, p95 3.05 s (20 runs) | **missed** |
| Materialize one book (Psalms) after a decision | 1 s p95 | median 0.99 s, p95 1.25 s (20 runs) | **missed** |
| Materialize one book (Jude) after a verse save | 1 s p95 | median 90 ms, p95 126 ms (20 runs) | met |
| Whole-project materialize, command to done | background (rule 4) | median 10.9 s, p95 12.2 s (5 runs); 51 to 55 progress events | background work; no budget figure |
| Save a verse during the whole-project job | 25 ms p95 | median 6.7 ms, p95 11.0 ms (975 saves) | met |
| Record a decision during the whole-project job | 25 ms p95 | median 5.9 ms, p95 9.0 ms (972) | met |
| Checkpoint, Psalms dirty | 500 ms | median 1.52 s, p95 2.10 s (20 runs) | **missed** |
| Checkpoint, materialization already done | 500 ms | median 0.71 s, p95 1.32 s (20 runs) | **missed** |
| Save a verse during a checkpoint | 25 ms p95 | median 6.9 ms, p95 20.0 ms (1000 saves) | met |
| Close a project (Psalms changed) | 3 s | median 3.13 s, p95 3.88 s (5 runs) | **missed** |
| Project store memory, steady | 400 MiB | 280 MiB private, a minute after 200 saves and 200 decisions | met |
| Project store memory, peak | 450 MiB | 296 MiB private, through the whole-project job and the close | met |
| Verse save, idle (until the durable reply) | not a separate row | median 8.0 ms, p95 14.2 ms (200 saves) | for the record |
| Decision save, idle | not a separate row | median 5.9 ms, p95 7.4 ms; max 323 ms once | for the record |

Raw output: `C:\tc4-g0\perf-logs\` on the laptop (`materialize`, `whole`, `checkpoint`, `close`,
`run`, `memory-win`).

## Missed rows

Each needs a card (step 2): checkpoint, materialize one book (Psalms), close a project. The remedy
`PERFORMANCE.md` proposes for the checkpoint is writing one checkpoint's objects into one pack. The
S2 estimates of 0.5 to 1.3 s for materializing one book and 2.7 to 4.2 s for the close were
optimistic for the verse-save case (2.2 s) and about right for the close.

## Part 2: release timing tests (cargo test --release, commit 003aa087, warm)

| Row | Budget | Figure | Result |
|---|---|---|---|
| Open the Check screen, Psalms tN and tW, derive | 250 ms first time | first 229 ms; median 136 ms, p95 145 ms (21 runs) | met; the 20 ms from-cache half is not printed by the test |
| Load one book's resources (ULT Psalms, 5.1 MB read apart) | 300 ms first time | best 253 ms, median 284 ms, p95 382 ms (20 runs) | met at the median; p95 over |
| Load a lexicon (en_uhl) | 100 ms once | first load 312 ms; warm median 28.6 ms, p95 36 ms | **missed** on the first load (the test fails: over 3 times the budget) |
| Lexicon lookup (19,653 Psalms words) | in memory | median 0.7 us, p95 1.6 us | met |
| Suggestions for one verse (old project, 3,994 aligned verses) | 50 ms | median 52.3 ms, p95 56.1 ms | at the edge, 6 to 12 percent over |
| Suggestions for one verse (new, 7,958 aligned verses) | 50 ms | median 231.6 ms, p95 235.5 ms | **missed**, 4.7 times |
| Aligner suggestion on Titus, plain / boosted | 50 ms | 6 / 18 to 19 ms per suggestion | met |
| USFM: parse a Psalms-sized book; replace one verse | not a row | 55 ms; 70 ms | for the record |
| OBS and export, 50 stories / Psalms | not a row | all under 22 ms p95 | for the record |
| Operation-count tests, `tc-app` (48) and `tc-journal` (8) | counts | all pass in release | met |

Home bench (`home_bench`): the list is 0.95 ms median (p95 1.5 ms); a language search 4 to 9 ms
median (p95 7 to 15 ms); first language-list read 100 ms; one summary for Psalms 15.8 ms.
Store adapter bench (whole Bible, warm): save a verse through `App::call` until its Ack 5.6 ms
median, **p95 11.0 ms** (budget 25 ms, met); open 1.75 s; close after 10 saves 1.15 s median,
p95 1.86 s.

## Part 3: the installed app (translationCore4-4.1.0-windows-x64-unsigned.exe)

- Artifact `release-windows-x64` of release run 40483548452 (branch `track-REL-1`, commit
  `05d7a661`), 175,822,526 bytes, installed per user with `/VERYSILENT /CURRENTUSER`
  (677 MB). Unsigned, so SmartScreen and signing effects are not measured.
- Driver: `C:\tc4-g0\perf-tools\launch.mjs` (starts `electron.exe` with a debugging port, polls the
  page until Home; "window shown" comes from the shell's own log).

| Row | Budget | Figure | Result |
|---|---|---|---|
| First launch, empty profile: shell start to Home, seeding 2,803 resource files | under 60 s | 15.0, 14.9, 15.1 s (3 runs); the very first run after install 32.3 s | met |
| Preparing page (determinate progress) shown | within 300 ms of the window, in effect | 0.6 to 0.74 s after the spawn | see caveat |
| Repeat launch: usable Home | proposed 3 s | median 0.58 s, range 0.56 to 0.68 s (10 runs) | met |
| Window on screen, from the spawn | 1 s | 0.28 to 0.37 s (13 runs); the shell logs "window shown" 72 to 94 ms after it starts | met, by the shell's log rather than a pixel |
| Memory: private working set of `tc4` and every Electronite process, Home, no project, a minute after launch | 1 GiB with the whole Bible open | 104 MiB, three readings 20 s apart (4 electron, 1 tc4 processes) | not comparable: no project open |

Caveats: the "from the shortcut click" part of these rows is not measured, only from the process
spawn. Defender was on and the app's files were scanned once at install. A repeat-launch read and
write count of resource files is a count test, not run here. First launch after an upgrade is not
measured (there is no earlier build installed).

## Not yet measured

- **Rows that need a Psalms project in the installed app.** Typing in a verse (16 ms), long
  lists and the check rail (16 ms a frame), open a book (150 ms), move chapter (16 ms), rail
  filters (16 ms), open the aligner (30 ms) and the Check screen from the UI, and the memory row
  with the whole Bible open. The app has no way to load one: its Scripture Burrito and USFM
  importers say "Arrives with its importer", and "New Bible" starts empty. A translationCore 3
  zip of Psalms would work through the TC3 importer.
- **Cold open after a reboot** and the first open when the disk cache is cold (needs the owner's
  reboot and nothing else run first).
- **Memory growth with history** (needs a 1.2-million-change corpus).
- **Save work for a teammate (USB export, USB 2), daily sync, import a tC4 project:** need a
  USB 2 drive, the network and a project.
- **Start-up rows on the signed installer** (H4): unsigned numbers above stand until then.
- **Resources rows not printed by tests:** the from-cache Check open (20 ms), the upgrade count
  and carry-over plan (2 s, 300 ms a book), the aligner open (30 ms) and reflow (2 ms), and the
  project-wide counts (1 ms): they have operation-count tests but no timing printed.

## Caveats

- Run counts follow the Windows track's rule (5 runs for actions over 1 s, 20 otherwise), except
  the open rows (7 runs, the bench's default) and the first launches (3).
- The measurer built and ran the benches, so the "someone other than the measurer" check of the
  review gate is still to do.

## Step 1 findings

### PERF-LEXICON-LOAD: the first read of `lexicon.json` waits about 140 ms; the code's part is 22 ms

- The test's first load is 156 to 162 ms in 8 runs (the 312 ms of the first report was taken while a
  build ran). Warm it is 21 to 24 ms: read 0.8 ms, parse 21 to 22 ms.
- Splitting the first load: the lookup of the install is 0.3 ms, **the first `std::fs::read` of the
  2,055,993-byte file is 129 to 145 ms**, the second read is 0.8 ms, and the parse is the same 21 ms
  both times. Nothing in `open_lexicon` or `Installed::read` does one-time work.
- The slow read follows the file's content, not its name, folder or how it was written. The same
  bytes named `.dat` or `.txt` read in 137 to 162 ms; the first half of the file reads in 114 to
  122 ms; a 2 MB ASCII JSON file in 7 to 17 ms; a gzip of the lexicon (455,617 bytes) in 6 to 8 ms;
  a 2 MB Door43 TSV in 8 to 10 ms. Written by the store's own staging and folder rename, by a
  one-file rename, or by Copy-Item, it costs the same. The file written and then read again after
  3 s reads in 3.7 ms. The seeded original, never read since the first launch, cost 158 ms on its
  first read.
- So the cost sits in the operating system, and it is paid once per newly seen file. It is
  consistent with Defender's real-time scan, but that is **inferred, not shown**: Defender must
  stay on for these measurements, so it was not switched off to check.
- What a fix can do, in code the card owns: (1) keep the lexicon compressed on disk. The estimate
  is 7 ms read, about 8 to 10 ms to inflate (not measured in Rust; .NET took 25 ms), and 21 ms to
  parse, so about 40 ms; (2) read both lexicons in a background task after launch, so the 140 ms
  is paid before the first lookup; (3) accept it and say so in the budget: the row is 100 ms and
  the code's own part is 22 ms. Options (1) and (2) change nothing a translator can lose.

### PERF-MATERIALIZE-BOOK: the time follows the bytes the job writes, about 0.12 s per MB

From the counters `storebench materialize` printed for the last run of each case (laptop, commit
003aa087) and the medians of its 20 runs:

| Case | Median | Bytes written | ms per MB written | Bytes read | Bytes hashed | Files opened / created | Derived read / projected / written | fsyncs |
|---|---:|---:|---:|---:|---:|---:|---|---:|
| Psalms after a verse save | 2,216 ms | 16.94 MB | 131 | 152.3 MB | 66.6 MB | 13 / 11 | 5 / 5 / 3 | 30 |
| Psalms after a decision | 993 ms | 7.79 MB | 127 | 52.7 MB | 27.7 MB | 4 / 6 | 2 / 5 / 1 | 17 |
| Jude after a verse save | 90 ms | 0.81 MB | 110 | 1.95 MB | 1.30 MB | 10 / 12 | 5 / 5 / 4 | 32 |

- Time per MB written is the same, 110 to 131 ms, across a 20-fold range of sizes. Bytes read and
  hashed do not fit as well (read: 14, 19 and 46 ms per MB). The lexicon finding above gives
  about 70 ms per MB for the operating system's first read of newly written text, so a scan of
  what the job writes is the likely limit. That is **inferred from the fit, not shown**.
- S2's Linux counters for the same case were: Psalms after a verse save 3 / 5 files, 5 projected,
  **1** derived file written, 34 MB read, 24.8 MB hashed, 11 fsyncs, 190 ms median. The laptop's
  last run wrote **3** derived files, read 4.5 times the bytes, hashed 2.7 times the bytes, and
  made 30 fsyncs. The decision case matches Linux's counts (1 derived file written, 17 fsyncs).
  Why one verse save rewrites two more derived files on the laptop is a lead for the storage
  lane: a derived file compared by something that differs on Windows (a time stamp, a line
  ending, a path case) would be written again each time. The laptop's last run may also be one
  where the watcher's echo or a periodic step ran; the counters are of one run only.
- Next for the card: print the counters of every run, not only the last, and the bytes written
  per derived file; if a second file's bytes are written each time without changing, that is
  the fix. Fewer bytes written is the lever whatever the cause.

### PERF-CHECKPOINT: one core of CPU, not flushes; the clean case is at the budget

- A flush costs little on this laptop. Create, write, fsync and rename of 25 files each:
  fsync median 1.1 ms (4 KB), 1.5 ms (64 KB), 4.9 ms (1 MB), 12.7 ms (4 MB). A checkpoint's 24
  fsyncs are 30 to 100 ms of it. Writing one pack in place of 24 flushes, the remedy
  `PERFORMANCE.md` proposes, saves at most that much here.
- A second `storebench checkpoint` run, with the process's CPU time sampled every 500 ms (67 s
  wall, 129 s CPU): during the checkpoint loops it kept one core busy (75 to 125 percent of a
  core in most samples); the samples at 300 to 400 percent are the open and fold at the start.
  So a checkpoint is single-thread CPU work on this machine: md5, zlib, libgit2's SHA-1 and its
  delta search (none has hardware help here, and the i5-8350U has no SHA instructions).
- Its figures moved between the two runs:

  | Case | First run (after the materialize bench) | Second run |
  |---|---|---|
  | Checkpoint after the materialization already ran | median 706 ms, p95 1,318 ms | median 428 ms, p95 494 ms (**met**) |
  | Checkpoint with Psalms dirty | median 1,518 ms, p95 2,098 ms | median 1,040 ms, p95 1,328 ms |
  | Save during a checkpoint | p95 20.0 ms | p95 9.8 ms |

- The Psalms-dirty case includes that book's materialize. Its last run wrote 8.7 MB in derived
  files (2 written) and read 60 MB; at the materialize finding's 0.12 s per MB written, that
  is about 1 s, which is most of the difference between the two cases. So the
  Psalms-dirty half of this row shares PERF-MATERIALIZE-BOOK's lever, and the clean half is at
  the budget, with run-to-run spread of 1.6 times.
- Next for the card: time the checkpoint's phases (seal, files, tree, commit) in `storebench`;
  the bench prints none today.

### PERF-CLOSE: not profiled

- A close is a seal, the files, tree and commit, the fold cache and the drafts, so it carries the
  checkpoint's and the materialize's costs, plus the fold-cache write. Not profiled on the
  laptop. The store-adapter bench's close after 10 saves (median 1.15 s, p95 1.86 s) against
  `storebench close` with Psalms changed (median 3.13 s) says most of a close is the Psalms
  materialize and checkpoint. Measure again after those two cards land.

### PERF-SUGGEST: not profiled here

- A suggestion's time grows with the model (52 ms at 3,994 aligned verses, 232 ms at 7,958); that
  is the code's work, not this machine's, so it can be profiled on any machine. The laptop
  figures above are its reference.

## Part 4: the UI rows in the installed app, on a Psalms project

- Projects: two translationCore 3 Psalms projects from the owner, imported through the app's TC3
  importer with "Use installed versions": `ar_avd` (2,461 verses, all aligned, 353 check
  decisions in the file) and `en_ult` (2,461 verses, 1,997 aligned). Measured on `ar_avd`.
- Same installed build as Part 3 (unsigned 4.1.0-preview, 05d7a661). The app was driven through
  the Chrome DevTools Protocol (`--remote-debugging-port`): clicks in the page, and keys and wheel
  events as trusted input (`Input.dispatchKeyEvent`, `Input.dispatchMouseEvent`). Each time is
  from the click or key to the change in the page, then the next frame. Scripts:
  `C:\tc4-g0\perf-tools\ui-*.mjs`. The display runs at 60 Hz (61.3 fps measured).

| Row | Budget | Figure | Result |
|---|---|---|---|
| Import a tC3 project (Psalms, 2,461 verses) | 2 min, with progress | 3.9 s; the progress shows at 1.03 s | met; the progress came later than rule 4's 300 ms |
| Open a book (project open): Home to the first passage | 150 ms; verses in 50 ms | median 1,266 ms, p95 1,470 ms (10 runs); it opens on Understand, which loads the passage with its helps | **missed** |
| Move to another chapter (Translate) | 16 ms | median 34 ms, range 25 to 58 ms, Psalms 2 to 21 (DOM ready 21 to 48 ms); Psalm 119 145 ms; leaving 119 124 ms | **missed** |
| Typing in a verse, input to paint | 16 ms | event timing of keydown: median 24 ms, p95 32 ms, max 40 ms, 89 of 100 over 16 ms; a bare textarea added to the same page, no app code: median 24 ms, p95 32 ms, max 48 ms, 83 of 100 over 16 ms | the app adds nothing measurable; no textarea meets 16 ms input-to-paint at 60 Hz here. The row needs a definition (owner) |
| Save a section until "Saved" | 25 ms | median 16.5 ms, p95 28.5 ms (15 saves of one keystroke); the first save after 100 keystrokes 134 ms | at the edge |
| Open the Check screen (Translation Words, 7,683 items) | 250 ms first; 20 ms from the cache | 126 ms first; then median 125 ms, p95 140 ms (7) | first **met**; from the cache **missed** (no speed-up seen) |
| Change filter or sort in the rail | 16 ms | page settled in 2 to 11 ms (To do, All, Invalid, By verse, By word; 30 changes) | met |
| Record a decision | 25 ms until "Saved" | the header never shows "Saving" for a decision; the button shows the new state in median 90.5 ms, range 87 to 107 ms (12) | **missed** on the visible response |
| Long lists: the check rail (virtualized, 69 to 101 elements for 7,683 items) | 16 ms a frame | wheel 200 px a tick: frames median 16.7 ms, p95 19.0, max 21.3; 1,000 px a tick: p95 22, p99 33, 90 of 237 frames over 20 ms | met at normal speed; frames drop on fast flings |
| Long lists: Psalm 119 in Translate (not virtualized: 2,331 elements; helps pane 4,519) | virtualized; 16 ms a frame | frames median 16.7 ms, p95 18.3, max 20.9 (centre); helps p95 18.2, max 30.3 | frame time met; "virtualized" not met |
| Open the aligner on the longest verse (17:14, 44 ULT words) | 30 ms | 220 to 245 ms to the word bank, 236 to 260 ms painted (5 runs); 10-word verses the same 221 to 238 ms | **missed**; the time does not grow with the verse, so it is a fixed cost |
| Memory: `tc4` plus every Electronite process, Psalms open, a minute after the last action | 1 GiB (whole Bible) | 473 MiB (three readings): tc4 151 MiB, Electronite 226 + 67 + 25 + 4 MiB | met for Psalms; the whole-Bible case is not measured |

### Leads that are not speed (for the owner and the lanes)

- **The resolved count lags a decision.** On one Translation Words item: Mark valid, then To do,
  left "1 of 7683 resolved" and "To do · 7682" while the item showed To do; Invalid, then To do,
  brought it to 0. The tool list then still said "1 of 7683 resolved". The counts and the item
  disagreed for at least 1.5 s after each click.
- **The import marks most alignments invalid.** After importing `ar_avd` with the installed
  original-language versions, Align shows 0 of 2,461 verses aligned: Invalid 2,192, To do 269.
  Verse 1:1, which was not edited, says "The draft changed since this alignment was made" and sent
  two words back to the word bank (16 of 18 placed). The alignments are kept, not lost, but the
  translator is told their draft changed when it did not. The import review had said all 2,461
  verses were aligned.
- **An Arabic project is shown LTR.** Home's card for `ar_avd` says "LTR".

## Part 5: the cold rows, after a reboot

- Boot 15:35:41; the script waited to 170 s after it (the `.cmd` waits until 120 s; the owner logged in
  first). Nothing else was open (Task Manager's list: the command window and Settings). Defender on.
  Release `storebench` from commit 003aa087; the installed unsigned 4.1.0-preview app.
- **One run of each.** A corpus is cold once per boot, so each figure is a single run. The earlier
  ruling that a row at its budget needs at least 5 runs cannot be met in one boot; it would take 5
  reboots with a fresh corpus copy each time.

| Row | Budget | Figure after the reboot | Warm, for comparison | Result |
|---|---|---|---|---|
| Open with a usable fold cache, first open after a restart | 10 s (3 s warm) | usable 4.92 s; background work done at 12.3 s | median 2.71 s, p95 3.15 s | met against the restart budget |
| Open with no fold cache, disk cold | 10 s | usable 9.53 s (one run); done at 15.6 s | median 6.60 s, p95 7.24 s | met, with 0.47 s to spare on one run |
| First progress report of an open | within 300 ms | 307 ms (with the cache), 303 ms (without) | about 310 to 314 ms in the store adapter bench | 3 to 7 ms over the line, every time |
| Installed app, repeat launch, cold: spawn to Home | proposed 3 s | **42.2 s**; the Preparing page shown at 4.2 s | 0.58 s median | **missed** by 14 times the proposal, and 73 times the warm figure |

- The app's cold launch is the finding. After a boot, the app sat on its Preparing page for about
  38 s before Home. Warm, a repeat launch never shows that page. The seeded resources hold 3,785
  files and 304 MB. At the 70 ms per MB that PERF-LEXICON-LOAD found for the operating system's
  first read of newly seen text, 304 MB is about 21 s, and 3,785 file opens add more. So a cold
  launch that reads or checks every resource file would explain most of it. That is **inferred, not
  shown**: the shell's own log for this run was not kept (`shell.log` holds no entry after 14:49),
  so the phases are unknown, and the launch was one run.
- The rule for a repeat launch is "no read and no write of a resource content file". The count
  test checks that on a warm app. A cold launch that shows "Preparing resources" suggests the
  check reads them, or that something else makes the page appear. It needs the card's step 1.
- Warm figures are in Parts 1 and 3. The cold-open rows of the store are met by the 10 s restart
  budget; the app's cold launch is the open item.

## Where this stopped

On 2026-10-27 the owner called these measurements good for now and will wait for user reports. Not measured, by that decision: memory growth with history, USB export, daily sync, import of a tC4 project, and the start-up rows on the signed installer (the unsigned figures in Parts 3 and 5 stand until then). Every missed row that was measured has a card.
