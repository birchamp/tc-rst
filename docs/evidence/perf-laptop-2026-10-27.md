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