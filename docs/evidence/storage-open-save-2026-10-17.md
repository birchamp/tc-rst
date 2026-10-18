# The store at Bible scale: open, save, memory (unit S1)

First recorded 2026-10-17 and measured again on 2026-10-18, after the independent review's
fixes (held files, carried files, the best-effort open, the close order, the storage check's
counters) and the single-instance ruling. This page carries the second measurement; the first
(commit `9ef5f92`, two runs) gave the same verdicts within noise: with the cache 1.8 to 1.9 s
median, without 3.0 to 3.5 s, saves 2.7 to 6.3 ms median.

Budgets (`docs/PERFORMANCE.md`) that `tc-store` owns, measured on the whole-Bible corpus written
to disk through the store's own open, seal, consolidation and materialization:

| Budget | Value |
|---|---|
| Cold open with a usable fold cache | 3 s; determinate progress within 300 ms |
| Cold open with no usable fold cache, or the first open after a restart with a cold disk cache | 10 s; determinate progress within 300 ms |
| Save one verse until "Saved" | 25 ms including fsync |
| Record a decision until "Saved" | 25 ms |
| Memory, whole Bible open | 350 MiB peak; 150 MiB steady |

## Verdicts

| Case (O1 to O13, until the project is usable) | Median | p95 | Budget | Verdict on this machine |
|---|---:|---:|---:|---|
| With the cache, warm page cache | 1.63 s | 2.06 s | 3 s | met |
| With the cache, page cache dropped | 1.69 s | 1.79 s | 10 s | met |
| Without a cache | 3.37 s | 3.49 s | 10 s | met |
| With a cache another build of the same encoding wrote | 1.57 s | 1.73 s | 3 s | met |
| First progress report | 300.1 to 307.8 ms after the command, every open | | 300 ms | met |
| Verse save until the durable reply (200 saves) | 2.95 ms | 4.98 ms | 25 ms | met |
| Decision until the durable reply (200 decisions) | 2.18 ms | 3.68 ms | 25 ms | met |
| Memory peak (an open; an open and 400 saves) | 272 to 316 MiB; 329 MiB | | 350 MiB | met |
| Memory steady (the project open, after the open's background work) | 271 to 316 MiB | | 150 MiB | **missed** |

Seven opens per case, each in a child process of its own, in one run (`run-3`) at a load average
of 3.7 to 5.7 (other agents' builds on the same four cores; printed before and after every
open). The p95 of the first row is the first open of the series, which finds the entries the
setup's last materialization wrote still unsettled (§6.2): it read 187 derived files and opened
320, where every later open of the series read none and opened 133. The save rows are 200 saves
and 200 decisions, the project open.

**This machine has SHA instructions.** The VM this task was written for had none (Cascade
Lake, `docs/evidence/storage-fold-2026-10-16.md`); after a restart on 2026-10-17 the VM runs
on an Intel Xeon of family 6, model 207 (Emerald Rapids), which has `sha_ni` and AVX-512
(`run-3` printed `sha_ni true, avx512f true` for its 4 cores). Only the open without a cache
hashes in bulk (every change line once, 666 MB of JSON); the other cases fingerprint files and
hash nothing but new lines. So the cases with the cache carry over to a CPU without SHA
instructions, and the case without a cache does not. Here `ring` hashes the corpus's lines at
1,138 MB/s on one thread (`shabench`, median of 5, load 4.9 to 5.2); on the VM without SHA
instructions it did 292 MB/s. The difference is 1.7 s of one thread's time over the open, 0.4 s
if the four reading threads share it evenly and 0.85 s if two do: an open without a cache on a
CPU of this speed without SHA instructions would take about 3.8 to 4.2 s (median) and 4.3 s
(p95), against its 10 s budget. The J2 record measured the load and fold alone without SHA
instructions at 5.4 to 5.5 s on the slower Cascade Lake VM, also within 10 s. Neither is a
measurement on the reference laptop.

## What each open did

From the counters of every open (`StoreCounters`, APP-DESIGN R14), taken twice: when the
project is usable, and when the open's background work (the fold cache's write and the storage
check O14) has ended.

| An open after no change, whole Bible | When usable | After the background work |
|---|---:|---:|
| Files opened (with the cache) | 132 to 133 | 466 |
| Bytes read (with the cache) | 771 MB | 1,062 MB |
| Bytes read (without a cache) | 706 MB | 997 MB |
| Derived files read for the open (`derived_read`) | 0 | 0 |
| Derived files read by the storage check (`check_read`) | | 334 |
| Files projected | 0 | 0 |
| Time from the command (warm; page cache dropped; no cache) | 1.5 to 1.7 s; 1.6 to 1.8 s; 3.1 to 3.5 s | 2.0 to 2.4 s; 2.4 to 2.7 s; 3.8 to 4.5 s |

- **Files opened until the project is usable: 132 to 133**, of which 124 are journal files, each
  opened once (`an_open_reads_each_pack_once`); the rest are `project.json`, `format.json`, the
  lock, `device.json`, the manifest, the fold cache and the app data. The open itself opens none
  of the 334 derived files and projects none (`an_open_after_no_change_projects_no_file_and_reads_no_derived_file`).
- **The storage check then opens all 334 derived files once**, in the background: 290 MB (277
  MiB), finished 0.5 to 0.6 s after the project was usable with the page cache warm and 0.8 to
  1.0 s after with the page cache dropped. The first record of this page counted only the
  open's own reads and said that an open after no change opens none of the derived files;
  that holds until the project is usable, not after. (On the Psalms corpus the same check
  took an open from 133 files and 54 MB to 143 files and 72 MB.) The project answers
  commands meanwhile; the check competes with them for the disk, which §6.1 O14 names as the
  price of the one place a stat that lies is found. On Windows with Defender each of the 334
  opens is a scan, after the project is usable: that cost is not measured here (this VM has no
  Defender).
- **Derived files read: 0, projected: 0** in every open after the first of a series (the first
  read the files its predecessor's session wrote, whose entries are not settled yet, projected
  none of them, and settled them); from then on O9 takes every path as converged by its size,
  mtime and scope digest without reading it.
- **Journal files from the cache:** all of them in the warm, cold and update cases (124 of
  124); none without a cache, which then writes the cache again in the background. In the
  update case the cache was written, before each measured open, by `storebench-other`, a
  release build with overflow checks on (other code, same `INDEX_VERSION`, format minor and
  probe stamp): every file came from it.
- **Progress:** the first report came 300.1 to 307.8 ms after the open began, from the reporter
  thread, with a phase and a total ("Reading 31/124" with the cache, "Reading 8/124" without);
  14 to 18 reports per open with the cache, 29 to 34 without; the last one before the project
  was usable.

## What a save did

Per verse save and per decision, with the whole Bible open: **0 files opened, 2 fsyncs** (the
pending file and its folder), one file created, no pack read
(`a_save_reads_no_pack_and_writes_a_bounded_number_of_files` asserts the same write points on
Psalms and on the corpus ten times larger). Verse saves: median 2.95 ms, p95 4.98 ms, worst
8.63 ms; decisions: median 2.18 ms, p95 3.68 ms, worst 9.85 ms. A save's time is mostly its
two fsyncs. They ran after the opens, with no storage check left to run.

## Memory

`VmHWM` and `VmRSS` of the benchmark's process, glibc malloc:

| Process | Peak | Resident after |
|---|---:|---:|
| Open with the cache | 272 to 284 MiB | 271 to 284 MiB |
| Open without a cache | 308 to 316 MiB | 308 to 316 MiB |
| Open, then 200 saves and 200 decisions | 329 MiB | 329 MiB |

The peak is met. The steady 150 MiB is missed, as `docs/STORAGE.md` §14 and the J2 record
already say: the fold's tables for 617,086 changes and 283,606 records are about 170 MB of
heap on their own, and glibc keeps each worker thread's heap at its high-water mark (fewer
arenas give back up to 30 MiB; the first record measured `MALLOC_ARENA_MAX` 4, 2 and 1 at
281, 267 and 254 MiB). Meeting 150 MiB needs the fold to keep only the heads and the open
books' histories in memory, a change to `tc-journal`'s tables that unit S1 does not make.

## The first open of a project that has no derived files

Writing the corpus's packs into an empty folder and opening it took 13.7 s: the open projected
and wrote every derived file (about 210 MiB) because none existed. That is not one of the open
cases above: a project made by create, import or join is materialized in its staging folder
before it is renamed into place (§6.4 N1, N2; the create path does it through the same
checkpoint), so its first open finds the files and a manifest. The import budget (2 min, with
progress) covers that work.

## Machine and method

- VM: 4 vCPUs of an Intel Xeon @ 2.10 GHz, family 6 model 207, `sha_ni` and `avx512f` set;
  16 GB of RAM; ext4 on a virtio disk. Shared with other agents: the load average was 3.7 to
  5.7 during `run-3`, printed before and after every open in the log.
- Commit `cc57f88` (`storebench` as committed, the open code as of the review fixes and the
  single-instance ruling).
- Corpus: `tc-corpus` `Spec::whole_bible(1)`, 617,086 changes in 124 pack and pending files as
  generated; written with `storebench setup` (generation 27.0 s, the first open 13.7 s, the
  checkpoint's seal, consolidation and materialization 0.81 s); `tc_burrito` found 0 problems
  with the folder. The save runs added packs.
- `cargo build --release -p tc-store --example storebench`, then
  `STOREBENCH_OTHER=target/bench-s1/storebench-other storebench run target/bench-s1 7` (2 min
  3 s): each open in a child process of its own, 7 per case; then 200 verse saves spread over
  every book and 200 bookmark toggles of decided translationWords checks, each timed from
  `submit` to the durable reply. "Page cache dropped" writes `3` to `/proc/sys/vm/drop_caches`
  before the open; the host's own cache of the virtual disk is not dropped, so this case is a
  cold guest, not a cold disk. `storebench-other` is the same example built with
  `CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true`.
- The log of the run, as printed, was `target/bench-s1/run-3.log` and is summarized above; the
  corpus was deleted after the run.

## What is not shown

- The reference laptop: Windows 10 or 11, Defender on, a SATA SSD, a CPU without SHA
  instructions. The cases with the cache hash nothing in bulk, but their file reads (771 MB at
  usable, 1,062 MB with the storage check) and the per-file scans of Defender (133 until
  usable, 466 with the check) are not measured here.
- A cold disk under the guest: the host may have served the reads from its own cache.
