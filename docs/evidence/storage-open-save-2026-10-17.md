# The store at Bible scale: open, save, memory (unit S1)

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
| With the cache, warm page cache | 1.77 s / 1.93 s | 2.26 s / 2.12 s | 3 s | met |
| With the cache, page cache dropped | 1.80 s / 2.07 s | 2.02 s / 2.46 s | 10 s | met |
| Without a cache | 2.97 s / 3.53 s | 3.47 s / 3.71 s | 10 s | met |
| With a cache another build of the same encoding wrote | 1.78 s / 1.70 s | 2.01 s / 1.92 s | 3 s | met |
| First progress report | 300.1 to 313.4 ms after the command, every open | | 300 ms | met |
| Verse save until the durable reply (200 saves) | 2.65 to 6.26 ms | 5.05 to 14.5 ms | 25 ms | met |
| Decision until the durable reply (200 decisions) | 1.96 to 3.55 ms | 3.13 to 13.5 ms | 25 ms | met |
| Memory peak (an open; an open and 400 saves) | 267 to 320 MiB; 316 to 332 MiB | | 350 MiB | met |
| Memory steady (the project open, after the open's background work) | 267 to 320 MiB | | 150 MiB | **missed** |

Each open row gives two runs of 7 opens each (`run-1` at load 1.5 to 2.9, `run-2` at load 4.2
to 5.1, other agents' builds on the same four cores); the save rows give the range of the
medians and p95s of five runs of 200 saves and 200 decisions.

**This machine has SHA instructions.** The VM this task was written for had none (Cascade
Lake, `docs/evidence/storage-fold-2026-10-16.md`); after a restart on 2026-10-17 the VM runs
on an Intel Xeon of family 6, model 207 (Emerald Rapids), which has `sha_ni` and AVX-512.
Only the open without a cache hashes in bulk (every change line once, 666 MB of JSON); the
other cases fingerprint files and hash nothing but new lines. So the cases with the cache
carry over to a CPU without SHA instructions, and the case without a cache does not. Here
`ring` hashes the corpus's lines at 1,138 MB/s on one thread (`shabench`, median of 5, load
4.9 to 5.2); on the VM without SHA instructions it did 292 MB/s. The difference is 1.7 s of
one thread's time over the open, 0.4 s if the four reading threads share it evenly and
0.85 s if two do: an open without a cache on a CPU of this speed without SHA instructions
would take about 3.4 to 4.4 s (median to p95), against its 10 s budget. The J2 record
measured the load and fold alone without SHA instructions at 5.4 to 5.5 s on the slower
Cascade Lake VM, also within 10 s. Neither is a measurement on the reference laptop.

## What each open did

From the counters of every open (`StoreCounters`, APP-DESIGN R14):

- **Files opened per open: 132 to 138**, of which 124 to 129 are journal files, each opened
  once (`an_open_reads_each_pack_once`); the rest are `project.json`, `format.json`, the
  lock, `device.json`, the manifest, the fold cache and the app data. On Windows with
  Defender each open file is scanned, so this count is the open's own budget; it does not
  grow with the derived files (about 400), none of which an open after no change opens.
- **Derived files read: 0, projected: 0** in every open after the first of a series. The
  first open after a session that wrote files (`run-1`'s first warm open read 151, `run-2`'s
  90) reads exactly the files that session's last materialization wrote, whose entries are
  not settled yet (§6.2), projects none of them, and settles them; from then on O9 takes
  every path as converged by its size, mtime and scope digest without reading it
  (`an_open_after_no_change_projects_no_file_and_reads_no_derived_file`).
- **Journal files from the cache:** all of them in the warm, cold and update cases (124 or
  129 of 124 or 129); none without a cache, which then writes the cache again in the
  background. In the update case the cache was written, before each measured open, by
  `storebench-other`, a release build with overflow checks on (other code, same
  `INDEX_VERSION`, format minor and probe stamp): every file came from it.
- **Bytes read:** 771 to 773 MB with the cache (every file read whole to fingerprint it);
  706 to 708 MB without (every file read and hashed line by line).
- **Progress:** the first report came 300.1 to 313.4 ms after the open began, from the
  reporter thread, with a phase and a total ("Reading 31/129" with the cache, "Reading 8/129"
  without); 15 to 36 reports per open; the last one before the project was usable.

## What a save did

Per verse save and per decision, with the whole Bible open: **0 files opened, 2 fsyncs** (the
pending file and its folder), one file created, no pack read
(`a_save_reads_no_pack_and_writes_a_bounded_number_of_files` asserts the same write points on
Psalms and on the corpus ten times larger). The p95 was 5 to 6 ms in three of the five runs and
12 to 14.5 ms in the other two, which ran while other agents' builds were writing to the same
disk: a save's time is mostly its two fsyncs.

## Memory

`VmHWM` and `VmRSS` of the benchmark's process, glibc malloc:

| Process | Peak | Resident after |
|---|---:|---:|
| Open with the cache | 267 to 288 MiB | 267 to 287 MiB |
| Open without a cache | 308 to 320 MiB | 308 to 320 MiB |
| Open, then 200 saves and 200 decisions | 316 to 332 MiB | 316 to 332 MiB |
| Open with the cache, `MALLOC_ARENA_MAX=4` / `2` / `1` | 281 / 267 / 254 MiB | 277 / 263 / 249 MiB |

The peak is met. The steady 150 MiB is missed, as `docs/STORAGE.md` §14 and the J2 record
already say: the fold's tables for 617,086 changes and 283,606 records are about 170 MB of
heap on their own, and glibc keeps each worker thread's heap at its high-water mark (fewer
arenas give back up to 30 MiB; one open each, too few to say what they cost in time).
Meeting 150 MiB needs the fold to
keep only the heads and the open books' histories in memory, a change to `tc-journal`'s
tables that unit S1 does not make.

## The first open of a project that has no derived files

Writing the corpus's packs into an empty folder and opening it took 14.4 s: the open
projected and wrote every derived file (about 210 MiB) because none existed. That is not one
of the open cases above: a project made by create, import or join is materialized in its
staging folder before it is renamed into place (§6.4 N1, N2; the create path does it through
the same checkpoint), so its first open finds the files and a manifest. The import budget
(2 min, with progress) covers that work.

## Machine and method

- VM: 4 vCPUs of an Intel Xeon @ 2.10 GHz, family 6 model 207, `sha_ni` and `avx512f` set;
  16 GB of RAM; ext4 on a virtio disk. Shared with other agents: the load average was 1.5 to
  2.9 during `run-1` and 4.2 to 5.1 during `run-2`, printed before and after every open in
  the logs below.
- Commit `5c53c96` (`storebench` as committed; the open code is that of `976ed9a`).
- Corpus: `tc-corpus` `Spec::whole_bible(1)`, 617,086 changes in 124 pack and pending files
  as generated; written with `storebench setup` (generation 24.7 s, the first open 14.4 s,
  the checkpoint's seal, consolidation and materialization 0.72 s), `tc_burrito` found 0
  problems with the folder. The save runs added packs (129 files in `run-2`).
- `cargo build --release -p tc-store --example storebench`, then
  `STOREBENCH_OTHER=target/bench-s1/storebench-other storebench run target/bench-s1 7`: each
  open in a child process of its own, 7 per case; then 200 verse saves spread over every book
  and 200 bookmark toggles of decided translationWords checks, each timed from `submit` to
  the durable reply. "Page cache dropped" writes `3` to `/proc/sys/vm/drop_caches` before
  the open; the host's own cache of the virtual disk is not dropped, so this case is a cold
  guest, not a cold disk.
- The logs of both runs, as printed, were under `target/bench-s1/` and are summarized above;
  the corpus was deleted after the runs.

## What is not shown

- The reference laptop: Windows 10 or 11, Defender on, a SATA SSD, a CPU without SHA
  instructions. The cases with the cache hash nothing in bulk, but their file reads (771 MB)
  and the per-file scans of Defender are not measured here.
- A cold disk under the guest: the host may have served the reads from its own cache.
