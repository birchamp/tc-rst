# How much memory other programs use with Psalms open (Windows, 2026-10-19)

Task 5 of `.audit/tasks/windows-track.md`. The owner accepted new memory limits in
`docs/PERFORMANCE.md`: the whole app at 1 GiB steady (1.25 GiB peak with a side session), and
the project store at 400 MiB steady. This file sets those limits beside programs a translator
already runs, on the reference laptop.

## The machine

The owner's Dell Latitude 7490, recorded in `docs/evidence/windows-machine-2026-10-17.md`:
- Windows 10 Pro 22H2 (build 19045), Intel Core i5-8350U with 4 cores and 8 threads.
- 8 GB of memory: Windows sees 8,050 MB (7.86 GiB). A SATA SSD.
- Defender real-time protection on, with no exclusions.

It matches the reference machine of `docs/PERFORMANCE.md`.

Programs that ran in the background, as they do every day on this laptop:
- Defender
- Dell SupportAssist and TechHub
- OneDrive
- Snagit
- Edge's startup boost

This session's own tool, the Claude desktop app, was also running. A translator would not have
it, so the together reading below gives its share separately.

## How each program was measured

The same way the limit is defined: the **private working set** (Task Manager's Memory column).
It is summed over all of a program's processes, using the counter
`\Process(<name>*)\Working Set - Private`. Instances are matched by exact name, so `msedge`
does not pick up `msedgewebview2`, the WebView2 runtime other programs use.

- Each reading starts a minute after the last action.
- There are three readings a minute apart, and the table gives the median.
- Every reading also records how many `cargo`, `rustc` and `link` processes were running.
  - A build running at the same time takes memory from everything else, and Windows then trims
    the other programs' working sets.
  - So only readings taken with no build running are used.

Every reading is in `memory-compare-windows-2026-10-19/readings.jsonl`. The scripts that took
them are in `spikes/memory-compare-windows/`, in the order the steps below give.

## The Psalms text

- **Source.** `bundle/en/en_ult_v91.sb.zip` was expanded outside the repository, and its
  `ingredients/PSA.usfm` (5,146,082 bytes, ULT v91) was converted by `usfm-to-psalms.mjs`.
- **What was dropped.** All alignment data (`\zaln-s` and `\zaln-e`), the attributes of
  `\w word|...\w*` (the words are kept), and the one footnote (18:13).
- **Text.** One paragraph per verse, starting with the verse number. Poetry markers become
  spaces. "Selah" and the ULT's own `{braces}` are kept.
- **Headings, each in its own paragraph where the USFM puts it:**
  - "Psalm N" for each chapter
  - the 5 book divisions
  - the 114 superscriptions
  - the 22 acrostic headings of Psalm 119
- **Result.** 2,752 paragraphs: 2,461 verses and 291 headings.

**How it was checked.**
- I compared Psalms 1-3 against the USFM by eye.
- Five independent checkers then compared the text with the USFM from five angles: a 40-verse
  sample, structure, leftover markup, spacing, and the HTML.
  - They found two layout bugs in the first version: the Psalm 119 acrostic headings were
    stacked at the top, and 50 superscriptions ran into verse 1.
  - Both were fixed.
- After the fix, a separate checker re-derived all 2,752 lines from the USFM with its own
  extractor. Every line of `psalms.txt` and `psalms.html` matches, and all 42,292 `\w` words
  appear in order.

**The two files the programs opened.**
- **The Word file.** Word 2016 opened the HTML and saved it as `.docx` in Print Layout: 115,372
  bytes, 137 pages, 45,052 words by Word's count, 2,752 paragraphs.
- **The page for Edge.** The same text as a local HTML file of 252,931 bytes.

## The table

| Program | What was open | Private working set (median of 3) |
|---|---|---:|
| Microsoft Word 2016 (16.0.4266.1001) | The Psalms `.docx` (137 pages). Closed, opened again by double-click, scrolled to the end and back. 1 process | **52.6 MiB** |
| Microsoft Edge 155.0.4283.62 | The Psalms page in one window, one tab, in the owner's normal profile, scrolled to the end and back. 9 processes | **129.2 MiB** |
| Microsoft Edge, no window | Right after that window was closed. Startup boost keeps Edge running. 7 processes | 151.7 MiB |
| Electronite 37.1.0 (the rebuild's window) | The G0 spike's shell with its Home page, idle (filter mode, as in part 1 of `.audit/tasks/g0-windows.md`). 4 processes | **83.3 MiB** |
| The G0 stand-in server (`tc4-standin`) | Beside that window. It is a stub, not `tc4 serve` | 0.8 MiB |
| tC4 4.0.0-rc.2 (signed release/4.0 build of 2026-10-17) | A throwaway project with the whole of Psalms, aligned (below), open in the Translation Words check on item 1 of 7,683 (Psalms 1:1). 4 `electron` processes and `server.exe` | **709.3 MiB** |
| Paratext 9.5.110.1 | Not measured: on first start it asks for a registration (below), which the owner does not have. Installed, then uninstalled | — |

**Everything together.** Word with Psalms and Edge with Psalms were open at the same time,
with the background programs above. No build was running.

| | Median of 3 |
|---|---:|
| Total memory Windows sees | 8,050 MB |
| Available (`\Memory\Available MBytes`) | **2,037 MB** |
| Committed (`\Memory\Committed Bytes`) | 8,317 MB, of a commit limit of 16,534 MB (memory plus page file) |
| Word's share at that moment | 32.4 MiB |
| Edge's share at that moment | 210.3 MiB |
| The Claude desktop app (this session's tool, not a translator's) | 917.9 MiB |

**Everything together, with tC4 4.0.** Word with Psalms, Edge with Psalms and tC4 4.0 with its
Psalms project in the Translation Words check were open at the same time. No build was
running.

| | Median of 3 |
|---|---:|
| Available | **1,970 MB** |
| Committed | 9,847 MB, of 16,534 MB |
| Word's share at that moment | 33.0 MiB |
| Edge's share at that moment | 133.1 MiB |
| tC4's share at that moment | 525.2 MiB in `electron`, and 4.1 MiB in `server.exe` |
| The Claude desktop app | 764.8 MiB |

## tC4 4.0: what was measured

- **The build.** `tC4-4.0.0-rc.2-windows-x64.exe` from the release/4.0 package build of
  2026-10-17: GitHub Actions run 39026068549, commit `90b8d6e1`.
  - Authenticode: signed by unfoldingWord (Microsoft ID Verified CS AOC CA 04), with a valid
    timestamp. The artifact's sha256 is
    `639bec3dbc2aaa7a12d612cbfa302179db564f2be6638349135a6e9578897db9`.
  - v4.0.0-rc.1, the only tag, has unsigned Windows builds only.
  - It installed for this user (Inno Setup, silent) in 52 s, with the owner's yes.
- **The project.** A throwaway project, made through the running tC4's own local API by
  `spikes/memory-compare-windows/tc4-psalms-project.ps1`. Its Psalms is the bundled ULT v91
  Psalms with its 21,600 alignments.
  - **NFC.** The file had to be put in Unicode NFC. Otherwise tC4 refuses to open the project:
    "fold-of-seed differs from disk bytes (is the file NFC?)".
  - **One `\v` per line.** Each `\v` had to start its own line, as tC4 writes USFM. With the ULT's
    `\q1 \v 1` lines, tC4 read only 22 of the 2,461 verses: 21 chapters in its rail, "0 of 22
    verses aligned", and Psalm 1 empty in Translate. After the change it reads 2,411 verses and
    lists all 150 chapters. The Check page then shows Translation Words "0 of 7683", Translation
    Notes "0 of 7969" and Align "0 of 2411 verses aligned".
  - The owner's own projects were not touched: tC4's project folder,
    `%USERPROFILE%\pankosmia\tc4-projects`, did not exist before this install.
- **Where the memory is.** It is almost all in one Electron renderer: 596.2 of the 709.3 MiB.
  The other Electron processes hold 62.8, 39.0 and 6.9 MiB, and `server.exe` 4.5 MiB. tC4
  keeps its data in the page, not the server.
- **In the background.** tC4 held 180 MiB less once Word and Edge were in front of it: 525.2 MiB
  in the together reading, against 709.3 MiB alone. Chromium trims a window that is not in
  front. The alone figure is the one in the table.

## Paratext 9

- Paratext 9.5.110.1 came from paratext.org: the online installer, 231,861,704 bytes, signed by
  United Bible Societies Association. It needs an administrator install, and the owner
  approved its Windows prompt.
- On first start it asks for a language, then shows **Registration Information**: Registration
  Name, Registration Code (both from an email from paratext.org), an email address and a
  supporter name. OK stays disabled until they are filled in. Cancel closes Paratext.
- The owner has no registration, so Paratext was uninstalled. Its memory with Psalms open is not
  measured.

## Startup times

The orchestrator asked for these so the rebuild's own launch can be measured the same way later,
against its budgets: the window within 1 s, a first launch under 60 s, and a usable Home in about
3 s on repeat launches.

### Method

`spikes/memory-compare-windows/startup-time.ps1` (Word, Edge) and `tc4-time.ps1` (tC4) share
`startup-lib.ps1`. They poll every 15 ms on one thread, so each moment is late by at most about
15 ms plus one screen capture.

**The moments:**
- **t0** is just before the launch, which goes through ShellExecute as a double-click does. That
  means the document for Word, the page for Edge (through `msedge.exe`, the default browser
  here), and the Start-menu shortcut for tC4.
- **Window** is the first visible top-level window of the program's processes that is at least
  200 × 150.
- **Painted** is the moment that window shows a picture rather than one flat colour: a 32 × 32
  grid of its pixels holds at least 12 colours. A blank white or grey first frame fails this.
- **Usable** is tested for each program:
  - **Word:** its document window (`_WwG`) has the keyboard focus, so the cursor is in the text.
  - **Edge:** its window carries the page's title, "Psalms (ULT v91)", and is painted.
  - **tC4:** four steps in a row.
    - Home: the blue "+ Add a project" button and the project list are drawn.
    - The script then clicks the Psalms tile, and waits for the project to open: one of its top
      tabs drawn active.
    - It clicks Translate, and waits for that tab to be drawn active.
    - Translate usable: Psalm 1's verse text drawn in the source column.

**Conditions.** Every program was closed between runs, Defender stayed on, and no build ran.
Windows were captured from the screen, so none was covered. tC4's positions are fractions of its
maximized window on this 1920 × 1080 screen at 150 % scale.

**Warm and cold.** A start is warm when the program ran since boot, so its files are in Windows'
cache. A cold start needs a reboot first; see below.

### Warm starts (5 runs each; median, then the fastest and slowest)

| Program, with Psalms | Window | Painted | Usable |
|---|---:|---:|---:|
| Word 2016 (the `.docx`) | 179 ms (155–254) | 483 ms (419–643) | **2.82 s** (2.45–3.23): the cursor in the text |
| Edge 155 (the page) | 242 ms (200–491) | 339 ms (284–648) | **0.58 s** (0.48–1.09): the page drawn |
| tC4 4.0.0-rc.2 | 338 ms (303–368) | 364 ms (339–392) | Home **2.73 s** (2.63–2.86) |
| tC4, then Psalms open | | | **4.90 s** (4.62–5.18) from the launch. The click to the open project took 2.17 s (1.88–2.40) |
| tC4, then Translate with Psalm 1 drawn | | | **6.34 s** (6.22–6.59) from the launch. The click to the drawn verses took 1.41 s (1.36–1.65) |

- **Edge was never cold in the usual sense.** It starts at login with startup boost, so its
  window comes from a browser process that is already running.
- **Not every tC4 run was used.** Five of the ten tC4 runs were left out, all through the
  timing script's own mistakes. Each one is still in `startup.jsonl`:
  - Runs 1-3 tested a Home layout without the "Resume" banner that tC4 shows once a project has
    been opened, so they never saw Home.
  - The original set's run 5 found the button before the project list, and clicked an empty
    place.
  - The next run started from where that click left tC4.

### First launch and cold starts

- **tC4's first launch** used an empty profile, so tC4 copied its bundled resources. The
  window appeared at 902 ms, was painted at 1,170 ms, and Home was usable at **15.95 s**. It ran
  once, on 2026-10-19, 1,117 minutes after the last boot.
- **The first open of the Psalms project** (tC4 seeds its journal then) was not timed. The run
  that was timed opened the first version of the project, the one where tC4 read only 22
  verses: from the click, it took 2.59 s.
- **Cold starts** are one start each of tC4, Word and Edge, in that order, right after a reboot
  with nothing else opened. Run them with
  `warm-starts.ps1 -Runs 1 -Programs tc4,word,edge -Tag 'cold (first start after a reboot)'`.
  They wait for the owner's reboot, and their rows will be added here.

### What the startup times say

- **Word and Edge.** Each shows its window within 0.25 s and is usable within 3 s.
- **tC4 4.0.**
  - It shows its window within 0.4 s, and Home in 2.7 s.
  - Psalms in Translate takes 6.3 s from a double-click, including the two clicks.
  - Its first launch takes 16 s to reach Home.
- **The rebuild's budgets.**
  - The window within 1 s: tC4 already does this on a warm start.
  - A usable Home in about 3 s on repeat launches: tC4's 2.7 s is just inside it.
  - A first launch under 60 s: tC4 takes 16 s on this laptop.

## Every reading

All readings were taken on 2026-10-19 with no build running, except the first row.

| Program | Readings (MiB) | Available memory during them (MB) |
|---|---|---|
| Word, alone | 52.8, 52.6, 52.6 | 1,995-2,136 |
| Edge with the Psalms page, alone | 129.3, 129.2, 128.9 | 2,165-2,221 |
| Edge, no window, after the page was closed | 151.9, 151.7, 151.4 | 2,075-2,082 |
| Electronite window, idle | 83.3, 83.3, 83.7 | 2,067-2,200 |
| Stand-in server | 0.8, 0.8, 0.8 | 2,189-2,195 |
| Together, Word / Edge / Claude app | 32.5, 32.4, 32.4 / 210.7, 210.3, 210.0 / 917.3, 917.9, 919.5 | 2,014, 2,037, 2,098 |
| tC4 4.0.0-rc.2 with Psalms in the Translation Words check, alone | 709.3, 709.3, 709.2 | 2,208-2,236 |
| Together with tC4, Word / Edge / tC4 (`electron` + `server`) / Claude app | 33.1, 33.0, 33.0 / 133.1, 133.1, 134.4 / 525.8 + 4.1, 525.2 + 4.1, 525.2 + 4.1 / 764.8, 764.8, 751.8 | 1,970, 1,970, 2,074 |
| *Discarded:* Edge, no window, before any page was opened | 45.9, 45.9, 30.6 | 3,027, 2,360, 1,048 |

The discarded row ran on 2026-10-18, before the name filter was exact, so its sum includes
3.6 MiB of `msedgewebview2`. Builds were running during it: 2, then 3, then 18 processes.
Its third reading shows the trimming that a build causes. Its first reading, counting only
`msedge`, was 42.3 MiB. That is only a hint of what Edge holds before any page is opened.

## What the numbers say about the limits

- **Next to tC4 4.0, the limit is about 1.4 times what it takes.** tC4 4.0 took 709.3 MiB with
  Psalms open in its Translation Words check, which is 13 times Word and 5.5 times Edge.
  - That was a project of one book. The rebuild's limits are for a whole Bible: 1 GiB steady
    for the whole app, and 400 MiB for the project store.
  - So these figures say the limits are not loose next to the program translators use now. They
    do not say how tC4 does with a whole Bible.
- **Next to Word and Edge, 1 GiB is large.** 1 GiB steady is about 19 times Word with all of
  Psalms open, and about 8 times Edge with the same text. tC4 4.0's data layer alone peaked at
  838-1,033 MiB (`docs/BUDGET-REVIEW-2026-10-17.md`, section 2), so the limit still sits below
  the program the rebuild replaces. The rebuild is also a different kind of program: it holds
  the whole Bible's history, the resources and the aligner's model, and Word and Edge hold
  neither.
- **The room left on this laptop.** With Word and Edge on Psalms, 2,037 MB was available.
  - **Without this session's Claude app,** about 2.9-3.0 GB would be available. That estimate
    assumes closing the app frees roughly its 918 MiB (963 MB) of private working set. The
    budget review's estimate from forum reports was 2.3-2.8 GB, so this laptop has slightly
    more room.
  - **An app at its 1 GiB steady limit** (1,074 MB) would leave about 1.9 GB. With the Claude
    app still open, it would leave about 1.0 GB.
  - **At the 1.25 GiB peak with a side session** (1,342 MB), it would leave about 1.6 GB. With
    the Claude app open, about 0.7 GB.
  - Either way, the laptop does not have to move memory to disk.
- **The window measured on Windows.** The budget review estimated the window at 190-213 MiB, from
  Electronite with tC4's own page on a Linux VM, counted as private memory. On this laptop, the
  Electronite shell with the spike's small Home page takes 83.3 MiB.
  - That is the window's floor: the rebuild's real page (its WebAssembly UI and a whole Bible's
    screens) will add to it.
  - The whole-app estimate in `docs/BUDGET-REVIEW-2026-10-17.md` (0.6-0.7 GiB for one session)
    used the higher window figure, so it is not too low on this count.

## Odd results, recorded as they were found

- **Word measured less with Edge open (32.4 MiB) than alone (52.6 MiB).** Both readings were
  taken a minute after the same scroll, and neither run had a build or memory pressure: 2.0-2.2
  GB was available in both. One difference is that the alone run was Word's first open after
  its first-run dialog. Nothing here explains the gap further. The alone figure is the one in
  the table, because the task measures each program alone.
- **Edge held more after its window closed (151.7 MiB) than with the page open (129.2 MiB).**
  With startup boost on, Edge keeps running after its last window closes, and it kept memory
  from the closed page. The together reading (210.3 MiB) came after that, so it includes what
  Edge had kept. Edge's first window after a restart would be nearer the 129.2 MiB alone figure.
- **Committed memory (8,317 MB) is above the memory Windows sees (8,050 MB).** That is normal:
  committed memory counts what programs have reserved, and the page file backs it. The
  available figure is the one that says how close the laptop is to moving memory to disk.

## Not measured

- **Paratext 9.** It needs a registration the owner does not have; see "Paratext 9" above.
- **tC4 4.0 with a whole Bible.** Only the Psalms project was measured.
- **e-Sword 12.01.** It is installed but is not in the task.
- **Chrome 155.0.8059.80.** It is installed, but the task asks for one browser, and Edge is the
  one Windows sets up by default.
- **The rebuild's own page.** The Electronite figure is for the G0 spike's page, not the
  rebuild's UI.
