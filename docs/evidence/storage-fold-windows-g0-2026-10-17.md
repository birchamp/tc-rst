# The fold at Bible scale on Windows (2026-10-17)

The reference machine: a Dell Latitude 7490 (a 2018 model) with 4 cores, 8 GB and a SATA SSD,
older than the five years the reference names, so at least as slow
(`docs/evidence/windows-machine-2026-10-17.md` on `windows-fixes`). This file also stands for
task 3 of `.audit/tasks/windows-track.md`, which need not run the benchmark again. Defender
excludes no folder: the owner confirmed it on 2026-10-18 (the machine table below could not
read the list, which only an administrator can).

Written by `spikes/foldbench-windows/run-foldbench.ps1` (unit G0, docs/APP-DESIGN.md section 10). It
runs the release `foldbench` of `tc-corpus` from the storage branch on the corpus of STORAGE.md
section 14 as `docs/evidence/storage-fold-2026-10-16.md` on that branch does on Linux: one open per
process, the rounds interleaved. Only O8 of section 6.1 is measured, so the 3 s cold open is not shown
end to end (it needs `tc-store`).

foldbench reads CPU time and memory from Linux's `/proc`; on Windows it prints them as
"unavailable". This script measures them from outside: the CPU time of the whole foldbench
process, its peak working set (the Windows counterpart of peak RSS), its peak private bytes,
and its working set when foldbench reports the fold open (the steady figure). Each process
also computes the probe stamp once (about 5 ms), which its CPU time includes.

- Storage commit: 58b6874e92ed993c4a7692eea31e336fa4d29dd0 on HEAD
- Script commit: 538b9ac365fbb002e49794644293f105d63053a8 on g0-windows
- Build: rustc 1.99.1 (58cf8f9c3 2026-10-13), release profile; skipped (-SkipBuild)
- Corpus (`bible`): 131 packs and 186 pending files, 704,887,459 bytes; generated in 40.1 s; generated 617086 changes in 33.7067431s
- Run: 2026-10-17 19:48 to 19:53 UTC; 5 rounds, 4 reading threads
- foldbench's machine line: `cpu: sha false, avx2 true, avx512f false, avx512bw false, avx512vl false; 4 threads; load unavailable`
- Raw rows: `./docs/evidence/storage-fold-windows-g0-2026-10-17`

## Machine

| | |
|---|---|
| OS | Microsoft Windows 10 Pro 10.0.19045 (build 19045), 64-bit |
| Computer | Dell Inc. Latitude 7490 |
| RAM | 7.9 GiB |
| CPU | Intel(R) Core(TM) i5-8350U CPU @ 1.70GHz |
| Cores | 4 cores, 8 threads |
| Disks | WDC WDS100T2B0B-00YS70 (SSD, RAID, 932 GB) |
| Power | on mains power |
| Power plan | Balanced |
| Defender | real-time protection on; antivirus enabled True; mode Normal; the excluded folders can be read only by an administrator |
| SHA instructions | False |
| AVX-512 | F False, BW False, VL False |
| AVX2 | True |

## Load and fold of the whole Bible

Warm: every file was read once just before each open, so it was in the file cache. Times in
ms; the wall time is foldbench's own, CPU and memory are measured from outside.

| Action | Runs | Median | p95 | The fold after the last file (median) | CPU, all threads (median) | Peak working set | Peak private bytes | Working set with the fold open | Budget |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| (a) Open with the fold cache, warm | 5 | 1855 | 1987 | 423 | 3531 | 270-271 MiB | 266-267 MiB | 42-116 MiB | 3 s |
| (c) First open without a cache, warm | 5 | 6288 | 6543 | 512 | 24469 | 332-333 MiB | 368-369 MiB | - | 10 s |

## The app's case for memory

Each round runs `foldbench app` in a process of its own: one open of the whole Bible with the
fold cache, then a person working in Psalms (200 saves of a verse, decisions, alignments and
resolutions, the conflict lists, the book's projection and 20 snapshots). This is the case
the storage branch takes the memory budgets from. Working sets are read when foldbench
reports the open and when it reports the end of the steps.

| Figure | Value over the processes | Budget |
|---|---|---|
| Working set with the fold open, before the steps | 192-210 MiB | 150 MiB steady |
| Peak working set of the open | 269-271 MiB | 350 MiB peak |
| Peak working set of the process, the open and the steps | 270-273 MiB | 350 MiB peak |
| Working set after the steps | 45-114 MiB | 150 MiB steady |
| Peak private bytes | 282-283 MiB | |
| CPU time of the process | 43984-48313 ms | |

foldbench's own step times from the first round (its memory figures read "unavailable" on
Windows; the table above measures them):

| Action | Runs | Median | p95 |
|---|---:|---:|---:|
| Incremental step: save one Psalms verse | 200 | 0.066 ms | 0.252 ms |
| Incremental step: record one Psalms decision | 200 | 0.016 ms | 0.040 ms |
| Incremental step: save one Psalms alignment | 200 | 0.017 ms | 0.041 ms |
| Conflict counts after a save (incremental) | 200 | 0.000 ms | 0.000 ms |
| Incremental step with a live snapshot: save one Psalms verse | 200 | 0.143 ms | 0.355 ms |
| Incremental step with a live snapshot: record one Psalms decision | 200 | 0.060 ms | 0.151 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.044 ms | 0.088 ms |
| Conflict list and counts from scratch (every record's conflict built again) | 20 | 20.480 ms | 24.740 ms |
| Psalms projection: every file of the book (values read from disk) | 20 | 342.208 ms | 383.898 ms |
| Psalms USFM alone | 20 | 35.904 ms | 47.419 ms |
| One Psalms chapter's USFM | 20 | 0.253 ms | 0.760 ms |
| Snapshot (clone of the fold) | 20 | 0.098 ms | 0.341 ms |

## Psalms in use, without the app's steps

Each process opens the whole Bible with the fold cache, then projects every file of Psalms 20
times with the values read from disk (what materializing the book renders), and stops
(`FOLDBENCH_PSALMS_ONLY`): the projection times, and memory without the steps `foldbench app`
adds. The memory budgets apply to the app's case above.

| Figure | Value over the processes | Budget |
|---|---|---|
| Psalms projection, median of 20 | 352-401 ms | 300 ms |
| Psalms projection, p95 of 20 | 391-490 ms | 300 ms |
| Psalms USFM alone, median | 41.2-47.0 ms | 150 ms (open a book) |
| One Psalms chapter's USFM, median | 0.29-0.34 ms | 50 ms (first chapter) |
| Working set with the whole Bible open, before Psalms | 189-200 MiB | see the app's case |
| Peak working set of the process, the open and the projections | 270 MiB | see the app's case |
| Working set after Psalms was projected | 31-118 MiB | see the app's case |
| Peak private bytes | 265-266 MiB | |

Windows cannot reset a process's peak, so the peak above covers the open and the projections
together; when the projections peak lower than the open, this shows the open's peak.

## Every part, one process

`foldbench run CORPUS 4` with its defaults (5 runs of each open). Its memory and CPU columns
read "unavailable" on Windows. An open in a process that has already opened the project
several times is not the app's case; the app's case is the table above and the next part.

| Action | Runs | Median | p95 |
|---|---:|---:|---:|
| Read, verify and prepare | 5 | 6172.084 ms | 6488.279 ms |
| SHA-256 of every file once (one of the load's two passes) | 3 | 813.139 ms | 892.104 ms |
| Full fold of the prepared changes | 5 | 1335.708 ms | 1357.108 ms |
| Cold open with the fold cache: read the cache, read and fingerprint every file, take each file's changes from the cache, fold as they arrive | 5 | 1901.863 ms | 2020.331 ms |
| Cold open with the fold cache: the fold's work after the last file | 5 | 444.354 ms | 478.507 ms |
| Cold open without a cache: read, verify, prepare and fold, the fold indexing as files are read | 5 | 6820.028 ms | 7231.007 ms |
| Cold open without a cache: the fold's work after the last file is read (stages F2 to F10) | 5 | 491.527 ms | 513.215 ms |

- fold cache: 64525623 bytes for 317 files; peak RSS of each cold open with the cache ["unavailable", "unavailable", "unavailable", "unavailable", "unavailable"], RSS with the fold open unavailable
- corpus: 617086 lines, 283606 records, 119 conflicts, 65 anomalies; RSS with the fold open unavailable

## The app's case, as `foldbench run` starts it

`foldbench run` ends by starting `foldbench app` as a child process. These are that
child's rows; its memory is measured by the app's-case table above, not here.

| Action | Runs | Median | p95 |
|---|---:|---:|---:|
| Incremental step: save one Psalms verse | 200 | 0.040 ms | 0.151 ms |
| Incremental step: record one Psalms decision | 200 | 0.013 ms | 0.021 ms |
| Incremental step: save one Psalms alignment | 200 | 0.013 ms | 0.018 ms |
| Conflict counts after a save (incremental) | 200 | 0.000 ms | 0.000 ms |
| Incremental step with a live snapshot: save one Psalms verse | 200 | 0.145 ms | 0.285 ms |
| Incremental step with a live snapshot: record one Psalms decision | 200 | 0.055 ms | 0.120 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.045 ms | 0.088 ms |
| Conflict list and counts from scratch (every record's conflict built again) | 20 | 19.739 ms | 22.541 ms |
| Psalms projection: every file of the book (values read from disk) | 20 | 382.617 ms | 427.196 ms |
| Psalms USFM alone | 20 | 39.529 ms | 50.814 ms |
| One Psalms chapter's USFM | 20 | 0.290 ms | 0.896 ms |
| Snapshot (clone of the fold) | 20 | 0.106 ms | 0.240 ms |

## What these results decide

- The budgets in PERFORMANCE.md hold on the reference machine, a five-year-old Windows laptop
  with Defender on (owner ruling, 2026-10-16). These rows say, for the fold's part, whether
  they hold there: the 3 s open with the cache, the 10 s open without one and after a restart,
  the 300 ms Psalms projection, and memory (350 MiB peak, 150 MiB steady).
- A CPU without SHA instructions hashes about 6.5 times slower (LOCAL-BENCH-FINDINGS F1); the
  machine line above says whether this one has them.
- The cold rows, if a reboot was possible, are added below by `-Phase cold`.

## After a reboot (2026-10-18)

Run 2 minutes after the last boot, before anything else read the corpus: the open with the
fold cache read `tc4-g0-corpus`, the first open without a cache read its copy
`tc4-g0-corpus-nocache`. One run each, so no median.

| Action | Wall | The fold after the last file | CPU, all threads | Peak working set | Peak private bytes | Working set with the fold open | Budget |
|---|---:|---:|---:|---:|---:|---:|---|
| (b) Open with the fold cache, after a reboot | 4225 ms | 358 ms | 2406 ms | 266 MiB | 262 MiB | 95 MiB | 10 s for the first open after a restart |
| (c) First open without a cache, after a reboot | 10038 ms | 897 ms | 22719 ms | 333 MiB | 368 MiB | - | 10 s |

