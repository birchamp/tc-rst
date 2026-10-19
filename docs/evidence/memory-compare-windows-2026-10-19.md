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
| tC4 4.0 | Not installed. Not installed for this measurement, as the task requires | — |
| Paratext 9 | Not installed. Not installed for this measurement, as the task requires | — |

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
| *Discarded:* Edge, no window, before any page was opened | 45.9, 45.9, 30.6 | 3,027, 2,360, 1,048 |

The discarded row ran on 2026-10-18, before the name filter was exact, so its sum includes
3.6 MiB of `msedgewebview2`. Builds were running during it: 2, then 3, then 18 processes.
Its third reading shows the trimming that a build causes. Its first reading, counting only
`msedge`, was 42.3 MiB. That is only a hint of what Edge holds before any page is opened.

## What the numbers say about the limits

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

- **tC4 4.0 and Paratext 9.** Neither is installed, and the task says not to install either
  without the owner's yes.
- **e-Sword 12.01.** It is installed but is not in the task.
- **Chrome 155.0.8059.80.** It is installed, but the task asks for one browser, and Edge is the
  one Windows sets up by default.
- **The rebuild's own page.** The Electronite figure is for the G0 spike's page, not the
  rebuild's UI.
