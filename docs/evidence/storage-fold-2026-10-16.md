# The fold at Bible scale

Budgets (docs/PERFORMANCE.md) that `tc-journal` owns or feeds:

| Budget | Value |
|---|---|
| Cold open, whole Bible (the fold is part of it; `tc-store` owns the whole) | 3 s |
| Update project-wide counts and the conflict list after a save, whole Bible | 1 ms incremental; 100 ms from scratch |
| Save one verse: fold update and the chapter's projection | 2 ms |
| Open a book (Psalms): the first chapter's verses | 50 ms; the book 150 ms |
| Materialize one book (Psalms; `tc-store`, the projection is the fold's part) | 300 ms |
| Memory, whole Bible open (`tc-store`; the folded state is the fold's part) | 350 MiB peak; 150 MiB steady |
| Corpus generator, whole Bible | well under a minute |

## After the fold cache, hash-once, `ring` and the projection fix (evening, 2026-10-16)

### Machine and method

Intel Xeon @ 2.80 GHz (Cascade Lake, family 6 model 85: AVX2 and AVX-512, **no SHA
instructions**), 4 vCPUs under KVM, 15 GiB RAM, a virtio disk; Linux 6.18; Rust 1.97.0,
release profile, glibc malloc unless a row says jemalloc 5.3 (`LD_PRELOAD`). The VM was
restarted at 20:49 UTC. From 21:10 to 21:18 UTC no other job ran on it (`ps`, and the start
times of the jobs that came after):
the 1-minute load average of 2.4 to 4.5 in those rows is the benchmark's own four reading
threads. From 21:18 other agents' builds and tests shared the VM (load 3 to 9); rows measured
then say so.

Commits: the opens, the open's parts and the incremental steps were measured at `8ebccfa`,
the Psalms projection and the memory with Psalms in use at `1665eba`, which changed only
`render_book` (no open and no incremental step calls it). `foldbench` as committed with this
section only adds the Psalms-only mode and the RSS after the projections. The other build of
the same cache version is `8ac67d9`: `INDEX_VERSION` 3 as at HEAD, hashing with `sha2`
instead of `ring`.

The corpus is the one of the sections below (seed 1, 617,143 changes, 131 packs and 172
pending files, 705,135,600 bytes), generated again with `foldbench save bible` at `8ebccfa`:
byte for byte the corpus saved at 19:55 with the hash-once names. Its fold cache is
64,521,542 bytes.

Each open in the first table is its own process, so no open inherits another's heap
(`docs/evidence/storage-fold-2026-10-16/opens.py`, raw rows in `opens-8ebccfa.tsv` beside
it). Each of 9 rounds runs, in this order: (c) delete `index.bin`, read every file once,
open without a cache (it writes the cache); (a) read every file, open with the cache; (b)
evict, open with the cache; (c, cold) delete `index.bin`, evict, open without a cache (the
cache it writes must equal the first byte for byte); then the other build opens without a
cache and writes its own cache, which must equal this build's byte for byte (it did in every
round), and (d) this build opens with that cache.

**Cold page cache.** `/proc/sys/vm/drop_caches` cannot be written in this VM. Eviction is
`fsync` then `posix_fadvise(POSIX_FADV_DONTNEED)` on every pack, pending file and `index.bin`
(`cachectl.py evict`). Before every cold open the driver asserts with `mincore` that none of
their bytes (769,657,142 with the cache, 705,135,600 without) is resident, and a plain read of
the 705 MB took 0.27 s with the files in the page cache and 0.68 s after eviction. The guest's page cache is cold; the host's
may not be (a virtio disk), and 1 GB/s is faster than a SATA SSD. On the reference laptop's
SSD at about 500 MB/s, reading the files takes about 1.4 s; how much of it the open's
fingerprinting and fold hide is not measured.

### Results

Load and fold of the whole Bible (§6.1 O8 only), 4 reading threads, one open per process,
21:10 to 21:16 UTC:

| Action | Page cache | Runs | Median | p95 | CPU, all threads (median) | The fold after the last file (median, p95) | Peak RSS | Load |
|---|---|---:|---:|---:|---:|---:|---:|---|
| (a) With the fold cache | warm | 9 | 1,802 ms | 1,890 ms | 2,480 ms | 518, 696 ms | 329–334 MiB | 2.7–4.3 |
| (b) With the fold cache | cold | 9 | 1,875 ms | 1,980 ms | 2,700 ms | 574, 601 ms | 326–331 MiB | 2.7–4.3 |
| (c) Without a cache: the first open | warm | 9 | 6,290 ms | 8,483 ms | 19,880 ms | 603, 651 ms | 433–441 MiB | 2.4–4.3 |
| (c) Without a cache | cold | 9 | 5,899 ms | 6,556 ms | 20,420 ms | 583, 736 ms | 431–435 MiB | 2.6–4.2 |
| (d) With a cache another build wrote (`8ac67d9`) | warm | 9 | 1,724 ms | 1,945 ms | 2,460 ms | 540, 556 ms | 330–334 MiB | 2.7–4.5 |
| The other build's first open (`8ac67d9`, `sha2`) | warm | 9 | 6,291 ms | 8,578 ms | 21,350 ms | 580, 658 ms | 428–433 MiB | 2.8–4.5 |

The budget of this branch's PERFORMANCE.md is 3 s for the whole cold open. The owner ruled on
2026-10-16 that an open without a usable cache, or the first after a restart with a cold disk
cache, may take 10 s (`docs/JOURNAL-DECISION-2026-10-16.md`, Decisions); that row is not in
this branch's PERFORMANCE.md yet. Both must show progress within 300 ms (rule 4).

The same opens with jemalloc, interleaved with glibc, 22:24 to 22:27 UTC while other jobs kept
the load at 4.9 to 5.7 (3 runs each; CPU is the firmer figure under load): without a cache,
glibc 7,177 to 11,959 ms wall and 19,640 to 20,960 ms of CPU, jemalloc 5,048 to 7,154 ms wall
and 13,490 to 13,850 ms of CPU, a third less CPU, with the same peak (427 to 442 MiB); with the
cache, 1.8 to 2.0 s for both.

Every part, one process (`foldbench run DIR 4`, `8ebccfa`, 21:16 to 21:18 UTC, load 3.0 to
4.4):

| Action | Runs | Median | p95 | Budget |
|---|---:|---:|---:|---|
| Read, verify and prepare (4 threads; 17.4 to 18.8 s of CPU) | 5 | 7,069 ms | 8,418 ms | |
| SHA-256 (`ring`) of every file once (4 threads) | 3 | 987 ms | 1,098 ms | |
| Full fold of the prepared changes (one thread) | 5 | 2,223 ms | 2,400 ms | |
| Open with the fold cache, five times in one process | 5 | 1,964 ms | 2,015 ms | 3 s |
| Open without a cache, five times in one process (19.0 to 19.6 s of CPU) | 5 | 8,629 ms | 9,000 ms | 3 s |
| Incremental step: save one Psalms verse | 200 | 0.047 ms | 0.238 ms | 2 ms |
| Incremental step: record one Psalms decision | 200 | 0.011 ms | 0.018 ms | 1 ms |
| Incremental step: save one Psalms alignment | 200 | 0.009 ms | 0.014 ms | 2 ms |
| Conflict counts after a save | 200 | 0.000 ms | 0.000 ms | 1 ms |
| Incremental step with a live snapshot: save one Psalms verse | 200 | 0.200 ms | 0.289 ms | 2 ms |
| Incremental step with a live snapshot: record one Psalms decision | 200 | 0.165 ms | 0.304 ms | 1 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.051 ms | 0.171 ms | 1 ms |
| Conflict list and counts from scratch | 20 | 17.8 ms | 29.2 ms | 100 ms |
| Psalms USFM alone | 20 | 15.6 ms | 23.6 ms | 150 ms |
| One Psalms chapter's USFM | 20 | 0.133 ms | 0.700 ms | 50 ms |
| Snapshot (clone of the fold) | 20 | 0.194 ms | 0.794 ms | |

An open without a cache in a process that has already opened the project several times takes
longer on the same CPU time (8.6 s against 6.3 s in a fresh process); the first table, one
open per process, is the app's case.

The Psalms projection (every file of the book, values read from the files; what materializing
it renders) and memory with Psalms in use, `FOLDBENCH_RUNS=1 FOLDBENCH_PSALMS_ONLY=1
foldbench run DIR 4`: one open with the cache, then 20 projections, in one process. `head` is
the build of `8ebccfa`, before the projection fix, interleaved with `1665eba`:

| Build | Allocator | Load before..after | Runs | Median | p95 | CPU, all threads (median) | Budget |
|---|---|---|---:|---:|---:|---:|---|
| `8ebccfa`, before the fix | glibc | 2.2..2.5 | 20 | 362 ms | 442 ms | 730 ms | 300 ms |
| `1665eba` | glibc | 2.5..3.2 | 20 | **205 ms** | **247 ms** | 520 ms | 300 ms |
| `1665eba` | jemalloc | 3.2..4.0 | 20 | 243 ms | 385 ms | 460 ms | 300 ms |
| `8ebccfa`, before the fix | glibc | 4.0..4.6 | 20 | 521 ms | 642 ms | 630 ms | 300 ms |
| `1665eba` | glibc | 4.6..6.2 | 20 | 376 ms | 420 ms | 520 ms | 300 ms |
| `1665eba` | jemalloc | 6.2..6.8 | 20 | 317 ms | 405 ms | 460 ms | 300 ms |
| `8ebccfa`, before the fix | glibc | 6.8..7.6 | 20 | 511 ms | 672 ms | 650 ms | 300 ms |
| `1665eba` | glibc | 7.6..8.6 | 20 | 348 ms | 428 ms | 520 ms | 300 ms |
| `1665eba` | jemalloc | 8.6..8.9 | 20 | 331 ms | 409 ms | 480 ms | 300 ms |
| `1665eba` | glibc | 1.9..1.9 | 20 | **206 ms** | **262 ms** | 530 ms | 300 ms |
| `8ebccfa`, before the fix | glibc | 1.9..1.9 | 20 | 341 ms | 367 ms | 700 ms | 300 ms |
| `1665eba` | glibc | 1.9..2.9 | 20 | **217 ms** | **278 ms** | 560 ms | 300 ms |
| `1665eba` | jemalloc | 2.9..3.9 | 20 | **185 ms** | **211 ms** | 490 ms | 300 ms |

Rows are in the order they ran: the first nine from 22:10 to 22:13 UTC, when from the second
row on other agents' builds and tests shared the VM; the last four from 22:48 to 22:49 UTC,
with one other job running. With at most one other job running, the projection meets its
budget (205 to 217 ms median, 247 to 278 ms p95 with glibc; 185 ms and 211 ms with jemalloc);
as the load grows it misses it. In every pair the build before the fix was 1.4 to 1.8 times
slower. Whether the budget holds on the reference laptop is not measured.

Each projection writes 5 files, 18,747,613 bytes, byte for byte the same with both builds.

### Memory

| State | glibc malloc | jemalloc | Budget |
|---|---:|---:|---|
| Peak of an open with the cache | 326–336 MiB | 335–340 MiB | 350 MiB: met |
| Peak of the first open without a cache | 427–441 MiB | 424–442 MiB | 350 MiB: **missed** |
| RSS after an open with the cache, the fold open | 264–277 MiB | 250–252 MiB | 150 MiB steady: **missed** |
| Peak while Psalms is projected, after an open with the cache (`1665eba`) | 409–429 MiB (450–502 at `8ebccfa`) | 317–323 MiB (354–355 at `8ebccfa`) | 350 MiB: **missed** with glibc, met with jemalloc |
| RSS with the fold open after Psalms was projected (`1665eba`) | 409–429 MiB | 295–298 MiB | 150 MiB steady: **missed** |

The folded state itself is 205 to 214 MB of heap (massif, sections below): the steady budget
is missed by the fold's tables alone. glibc keeps what the reading and projecting threads
free in their arenas, so its RSS after a projection stays at its peak; jemalloc returns it.
The first open's peak also holds the sections of the new cache (64.5 MB) until the cache is
written.

### What is not measured

Only O8 of §6.1 is measured here. O1 to O7 (lock, temporary files, git, format and filesystem
gates, identity, finishing interrupted work) and O9 to O13 (classifying the derived and
carried paths, materializing, backfill, drafts, the report) need `tc-store`, which is not
built. **The 3 s cold open is therefore not shown end to end.** Nor is anything here measured
on the reference laptop, on Windows with Defender, on a CPU with SHA instructions, or with the
host's disk cache cold.

### What changed since the numbers below (`6e8adac`, 12:25 UTC)

- `8690c52`: values and files are fingerprinted with a full-avalanche step instead of FxHash.
- `2d63657`, `c45fb07`: the fold cache `.tc/index.bin`, read by any build of its
  `INDEX_VERSION` and format minor (STORAGE.md §3.6). (d) above shows an app update that
  keeps the encoding keeps the cache: the other build's cache is byte for byte this build's,
  and opening with it takes as long as with this build's own.
- `8ac67d9`: a pack's and a pending file's names come from the header line and the line ids,
  so a read hashes each byte once. Measured then: CPU over all threads 26.5 s before, 22.6 s
  after (median of 6 interleaved runs at load 4.4 to 6.3).
- `250b2b0`: SHA-256 through `ring`, 1.7 times `sha2`'s speed on this CPU in `shabench`. In
  the opens above, `ring` saves 1.5 s of CPU on the first open (19.9 s against 21.4 s); the
  wall medians are equal, within this VM's spread.
- `1665eba`: the projection fix below.
- The VM was quiet for the first two tables; the numbers below were taken at load 6 to 10.
  CPU times compare: the first open used 25.9 to 28.4 s of CPU at `6e8adac` and 19.9 s now;
  SHA-256 of every file once took 1,022 ms at the quiet measurement of `225d19e` and 987 ms
  now.

### The Psalms projection: profile, fix and what is left

Callgrind on the whole-Bible fold opened with the cache, and the projection's phases timed on
their own (a temporary build, load about 4): listing the book's files took 43 to 65 ms on one
thread, because it listed every derived file of the project, visiting each of its 283,593
records (125 M instructions); decoding every record of the book into a shared map 93 to 121
ms on four threads (1.18 G instructions); rendering the files 113 to 141 ms, of which the
alignments sidecar, written on one thread, took 100 to 134 ms; freeing the decoded records 36
to 56 ms. `1665eba` lists the book's files from its own records, writes the alignment records
in chunks on threads, decodes decisions and alignments in their own file's chunks and frees
them as it goes, and writes each decision's context once. The files are byte for byte the
same. One projection fell from 2.59 G to 2.30 G instructions. The new operation-count test
`a_book_lists_its_files_from_its_own_records` fails on the old listing (221,293 records on the
corpus ten times larger, where Psalms holds 24,910).

What is left is CPU: decoding the book's records from JSON is about half of the instructions
(the flat parse 0.5 G, building the records 0.6 G) and the decisions about 0.7 G, a quarter of
which is the selection text that checks the selections of verses whose text changed. Keeping a decision's context
as its canonical JSON text in the record model (J1) would remove most of the decision
writing. More decisive for the person: with §6.2 as now written, an open after no change
projects no file, and a save projects only its book's files (unit S2 must show it with
`an_open_after_one_save_projects_only_that_books_files`). Materializing is background work
(PERFORMANCE.md rule 4): no screen waits for it.

### To reproduce

```sh
cargo build --release -p tc-corpus --example foldbench
target/release/examples/foldbench save bible DIR
# the first table: 9 rounds, one open per process; OTHER is a build of another commit with
# the same INDEX_VERSION (here 8ac67d9)
python3 -I docs/evidence/storage-fold-2026-10-16/opens.py DIR \
    target/release/examples/foldbench OTHER 9 opens.tsv
# every part in one process
target/release/examples/foldbench run DIR 4
# the projection and memory with Psalms in use, after one open with the cache
FOLDBENCH_RUNS=1 FOLDBENCH_PSALMS_ONLY=1 target/release/examples/foldbench run DIR 4
LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libjemalloc.so.2 FOLDBENCH_RUNS=1 \
    FOLDBENCH_PSALMS_ONLY=1 target/release/examples/foldbench run DIR 4
# instruction counts of one projection
FOLDBENCH_PROJECTIONS=1 FOLDBENCH_RUNS=1 FOLDBENCH_PSALMS_ONLY=1 \
    valgrind --tool=callgrind target/release/examples/foldbench run DIR 4
```

### What S1, S2 and P must do about these numbers

- **S1** measures the whole open (O1 to O13) on this corpus: with the cache and a warm and a
  cold page cache, without a cache, and after an update; on this VM and on the reference
  laptop (memo plan step 5). It reads with a bounded pool, shows progress within 300 ms
  (`open_shows_determinate_progress_by_300_ms`), and may write the new cache's sections as
  they are made rather than holding them (64.5 MB) to the end of the first open.
- **S2** proves that an open after no change projects nothing and reads no derived file, and
  that a save re-projects only its book's files (§16.5a).
- **P** (and `tc-app`) choose the allocator: jemalloc took a third less CPU on the first open
  and returns what threads free (steady RSS 295 MiB against 424 MiB with glibc after Psalms
  was projected). The steady budget of 150 MiB needs the fold to keep only the heads and the
  open books' histories in memory and to read an idle book's history from `.tc/index.bin`
  when a step reaches it: a change to the fold's tables and to `tc-store`, not made here.

## After the review of the fold (`6e8adac`)

Measured on the same VM and corpus as below, at the commit that adds this section, on
2026-10-16 from 12:25 to 12:29 UTC. Other jobs shared the VM's four vCPUs (load average 6 to
10): SHA-256 of every file once took 1,831 ms against 1,022 ms in the quieter measurement
below, so wall times here are about 1.8 times what an idle machine gives. CPU times are
less affected. `foldbench run DIR 4`; the cold open now hands each file's prepared changes to
the fold as soon as a reading thread has them, and the fold indexes them while the threads
read the rest.

| Action | Runs | Median | p95 | Budget |
|---|---:|---:|---:|---|
| Cold open, the fold indexing as files are read | 5 | 11,572 ms | 12,290 ms | 3 s: **missed** (the read) |
| The fold's work after the last file is read (F2 to F10) | 5 | 677 ms | 993 ms | part of 3 s |
| Read, verify and prepare alone (4 threads) | 5 | 11,083 ms | 12,265 ms | |
| Full fold of the prepared changes alone (one thread) | 5 | 2,870 ms | 3,528 ms | |
| Incremental step: save one Psalms verse | 200 | 0.053 ms | 0.297 ms | 2 ms |
| Incremental step: record one Psalms decision | 200 | 0.012 ms | 0.021 ms | 1 ms |
| Incremental step with a live snapshot: save one Psalms verse | 200 | 0.261 ms | 0.421 ms | 2 ms |
| Incremental step with a live snapshot: record one Psalms decision | 200 | 0.233 ms | 0.401 ms | 1 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.064 ms | 0.201 ms | 1 ms |
| Conflict list and counts from scratch | 20 | 22.7 ms | 29.2 ms | 100 ms |
| Psalms projection: every file of the book | 20 | 488 ms | 1,034 ms | 300 ms: **missed** |
| Psalms USFM alone | 20 | 15.8 ms | 23.0 ms | 150 ms |
| One Psalms chapter's USFM | 20 | 0.142 ms | 0.399 ms | 50 ms |
| Snapshot (clone of the fold) | 20 | 0.197 ms | 0.883 ms | |

The streamed open used 25.9 to 28.4 s of CPU over all threads, the same as reading then
folding. Its peak RSS over the five opens was 388 MiB (716 MiB before the change); massif
counts a heap peak of 233 MB (413 MB before), almost all of it the folded state itself (205
to 214 MB of useful heap at the end of the open). The Psalms projection's CPU time over all
threads has a median of 890 ms.

Resident memory after one streamed open (`FOLDBENCH_RUNS=1 FOLDBENCH_STREAM_ONLY=1`, same
build, 12:32 to 12:35 UTC), peak and then with the fold open:

| Allocator | Reading threads | Peak RSS | RSS with the fold open |
|---|---:|---:|---:|
| glibc malloc | 4 | 374 MiB | 374 MiB |
| glibc malloc | 1 | 274 MiB | 274 MiB |
| glibc malloc, `MALLOC_ARENA_MAX=1` | 4 | 323 MiB | 323 MiB |
| jemalloc 5.3 (`LD_PRELOAD`), default settings | 4 | 345 MiB | 256 MiB |
| jemalloc 5.3, `dirty_decay_ms:0,muzzy_decay_ms:0` | 4 | 285 MiB | 243 MiB |

glibc keeps most of what the reading threads freed in their arenas; jemalloc gives it back to
the system. Which allocator the app links is `tc-app`'s choice.

The parts of the folded state at massif's peak (one reading thread): each change's entry (its
id, record, batch, parents, device and sequence, time, and where its value lies) 80 MB, about
130 B a change; the record keys 44 MB; the batches 30 MB; the position indexes 29 MB; the
rest is heads, links, passages and conflict lists.

The fold of the corpus of 277,880 changes (`foldbench save bible-share 0.42`, the size of the
probe's 278,041 actions): 5 runs, median 1,047 ms, p95 1,111 ms (0.92 s in the quieter
measurement below); its streamed cold open 4.8 s, of which the fold's work after the last
file 402 ms.

Steps with a live snapshot were added by this review (finding 10). Before the change, back to
back on the same corpus: 1.51 ms (p95 2.66 ms) for the verse save and 1.43 ms (p95 2.59 ms)
for the decision; the change shares the tables in chunks of 256 instead of 4,096, the
position indexes in 256 shards instead of 64, and each book's entry on its own. A snapshot
now copies about 6,000 pointers (0.2 ms, was 0.02 ms).

### Where the misses stand

**Cold open.** The fold's own share on the critical path is now its work after the last
file, 0.68 s here (about 0.4 s on an idle VM by the SHA-256 reference). The rest is the read:
24 to 27 s of CPU for two SHA-256 passes over 664 MB (P2 and P4, about 10 s without SHA
instructions) and parsing every change. On four cores without SHA instructions that alone is
6 s or more; with SHA instructions about 13 s of CPU, still above 3 s. STORAGE.md §14 names
the remedy, `.tc/index.bin`, which `tc-store` owns: an open with an unchanged pack set reads
heads and value locations instead of verifying and parsing every pack.

**Psalms projection.** 890 ms of CPU over four threads; 358 ms on the quieter VM below.
Callgrind on the Psalms corpus: decoding the book's 25,132 projected records is about half
the instructions and already runs on four threads, in chunks; rendering the tN and TWL
decisions, also chunked on threads, is about a third. What is left is CPU: a decision's
context is decoded into a map and written out again. Keeping it as its canonical JSON text
in the record model would remove most of that work; that is a change to `tc-journal`'s
record types (J1) and every decoder, not made here. Materializing is background work
(PERFORMANCE.md rule 4): no screen waits for it.

**Memory.** The open's heap peak (233 MB) is within the 350 MiB peak, and so is its RSS
with an allocator that returns freed memory (345 MiB with jemalloc; 374 to 388 MiB with
glibc's default arenas). The steady budget of 150 MiB is missed: the folded state is 196 to
204 MiB of heap on its own (256 MiB RSS with jemalloc). Values are already read on demand
from the files; what stays resident is each change's place in the history, which every
incremental step reads without touching the disk. Meeting 150 MiB means keeping only the
heads and the open books' histories in memory and the rest in `.tc/index.bin`, read when a
step reaches an idle book. That is a change to the fold's tables and to `tc-store`, not made
here, and the budget is unchanged.

## Earlier measurement (`225d19e`, before the review)

### What is measured

`crates/tc-corpus/examples/foldbench.rs`. `foldbench save bible DIR` generates the whole-Bible
corpus of STORAGE.md §14 with `tc-corpus` (seed 1) and writes its packs and pending files
under their repository paths. `foldbench run DIR 4` then opens it the way `tc-store` will:

1. **Read, verify and prepare.** Four threads take files from a shared queue, largest
   first, read each, run `read_pack` or `read_pending` (P1 to P4: SHA-256 of the file,
   SHA-256 of every line, the per-change rules) and prepare each change with
   `FoldInput::prepare_at`, which keeps a summary, the change's span in its file and a
   64-bit fingerprint of its bytes, not the bytes.
2. **Full fold.** `FoldState::fold_from` with a `ValueSource` that reads a span of a file
   again (`pread`). Values are read on demand from here on, and checked against their
   fingerprints.
3. On the folded state: 200 incremental steps each for a verse save, a decision and an
   alignment on Psalms, 100 conflicts made and resolved on Psalms verses, the conflict list
   and counts rebuilt from every record (`conflicts_rebuilt`) 20 times, and every file of
   Psalms rendered 20 times (`render_book`: USFM, alignments, tN and TWL decisions,
   comments), with values read from the files.

A change written by a benchmark step is a new change of a benchmark device, made from the
values the fold projects, and its parents are the token rule's (`shown_heads`), as a screen
would write it. Times are wall-clock times on the main thread; the projection's CPU time is
the process's user and system time over all threads.

`crates/tc-corpus/examples/loadprof.rs` times the parts of a load on one thread (line
hashes, `parse_change` by record kind, `prepare`). Instruction counts are callgrind's, on the
Psalms corpus.

The corpus: 66 books, 31,103 verses, 215,868 checks, 3 devices over 750 working days.
617,143 changes in 553,978 batches (664,344,248 bytes of canonical JSON) in 131 packs and
172 pending files, 674 MiB on disk. The fold holds 283,593 records and lists 124 conflicts
and 65 anomalies. Every part is labelled by `Corpus::describe`: the ULT, tN, TWL, UHB and
UGNT text and rows are real for the books the releases hold, the rest sampled, and the
history generated. The corpus generated at `225d19e` is byte for byte the one generated at
`747a7f4`.

### Machine

Intel Xeon @ 2.80 GHz (Cascade Lake: AVX-512, no SHA extensions), 4 vCPU, 15 GiB RAM;
virtio disk; Linux 6.18; Rust 1.97.0, release profile. Measured at commit `225d19e`,
2026-10-16, 05:42 to 05:47 UTC.

The VM is shared and its speed varies over the day. The same code (`62cc119`; `59bc3e1`
changed only documents) folded the corpus in 1.47 s at about 02:30 and in 2.10 s at 05:43,
and generated it in 31.7 s and in 57 s. Each comparison below between builds therefore
runs both builds in the same session, one right after the other. Four loading threads did about 2.6 times the work of one (21 to 22 s of CPU
over a 5.8 s load, for about 16.8 s of single-thread work).

### Results

Whole Bible, `foldbench run DIR 4`. "Before" is the build of `59bc3e1` (the previous
evidence), run right after this one:

| Action | Runs | Median | p95 | Before: median | Budget |
|---|---:|---:|---:|---:|---|
| Read, verify and prepare (4 threads) | 5 | 5,811 ms | 5,898 ms | 7,476 ms | |
| SHA-256 of every file once (4 threads; a load hashes every byte twice) | 3 | 1,022 ms | 1,229 ms | 1,033 ms | |
| Full fold of the prepared changes (one thread) | 5 | 2,010 ms | 2,241 ms | 2,103 ms | part of 3 s |
| Cold open: read, verify, prepare, fold | 5 | 7,822 ms | 8,140 ms | 9,579 ms | 3 s: **missed** |
| Incremental step: save one Psalms verse | 200 | 0.046 ms | 0.176 ms | 0.046 ms | 2 ms |
| Incremental step: record one Psalms decision | 200 | 0.010 ms | 0.019 ms | 0.011 ms | 1 ms |
| Incremental step: save one Psalms alignment | 200 | 0.009 ms | 0.019 ms | 0.011 ms | 2 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.030 ms | 0.125 ms | 0.030 ms | 1 ms |
| Conflict counts after a save (kept by every step) | 200 | 0.000 ms | 0.000 ms | 0.000 ms | 1 ms |
| Conflict list and counts from scratch | 20 | 15.0 ms | 26.3 ms | 25.2 ms | 100 ms |
| Psalms projection: every file of the book, values read from disk | 20 | 358 ms | 399 ms | 567 ms | 300 ms (materialize): **missed** |
| Psalms USFM alone | 20 | 14.3 ms | 27.8 ms | 29.1 ms | 150 ms (open a book) |
| One Psalms chapter's USFM | 20 | 0.106 ms | 0.282 ms | 0.210 ms | 50 ms |
| Snapshot (clone of the folded state) | 20 | 0.012 ms | 0.135 ms | 0.012 ms | |

The Psalms projection writes 5 files, 18,725,857 bytes, byte for byte what `59bc3e1` wrote;
its CPU time over all threads has a median of 930 ms (1,430 ms before). A second run of this
build right after the first gave medians of 5,903 ms (load), 2,095 ms (fold), 7,861 ms (cold
open) and 372 ms (projection).

The fold of a corpus of 277,880 changes (`foldbench save bible-share 0.42`, the size of tC4's
probe corpus of 278,041 actions): 5 runs, median 921 ms, p95 994 ms; its cold open 3,474 ms.
At about 02:30, on the faster VM, the previous build folded it in 591 ms (the probe folded
its events in 1.13 s).

Corpus generation, one run each (`examples/generate`): Psalms 2.0 s (47,357 changes), Psalms
made ten times larger 31.9 s (489,713 changes), whole Bible 38.7 s (617,143 changes; 32.8 s in
an earlier run). The generator of `59bc3e1` took 57.3 s for the whole Bible in the same hour.

Instruction counts (callgrind, Psalms corpus):

| Work | `59bc3e1` | `225d19e` |
|---|---:|---:|
| `parse_change` of the 47,357 lines (48 MB) | 7.54 G | 4.15 G |
| The Psalms projection, all threads | 4.30 G | 2.13 G |

One thread, whole Bible (`loadprof bible`): SHA-256 of every line 5.0 s, `parse_change` 6.2 s
(114 MB/s), `prepare` 0.6 s; the file hashes (P2) are another pass like the line hashes.

### Memory

Massif, whole Bible, one reading thread (`FOLDBENCH_RUNS=1 FOLDBENCH_FOLD_ONLY=1`): the
folded state holds 201.9 MB of heap (218.2 MB with the allocator's overhead), about 330 bytes
per change. The change table is 83.0 MB, 136 bytes a change: 8 more than at `62cc119`, for
the fingerprint of the change's value (the state held 196.9 MB then). The key table (160
bytes a record), the batch table (56 bytes a batch) and the four position indexes did not
change. The peak during the open is 412.9 MB of heap (433.5 MB with overhead), while the
prepared inputs (233 MB) and the growing tables are both alive. The process's resident size
after five opens was 592 MiB, which includes memory the allocator keeps after frees.

### Operation counts

`crates/tc-journal/tests/operation_counts.rs` runs each step on the Psalms corpus (47,357
changes) and on the corpus made ten times larger (489,713 changes, 30 devices), on a Psalms
chapter whose records are alike in both. Every counter is equal on both corpora:

| Step | Indexed | Keys updated | Keys recomputed | Values decoded | Books checked | Chapters cohered | Verses cohered | Attached placed | Records projected |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Verse save and its chapter's USFM | 1 | 1 | 0 | 11 | 1 | 1 | 10 | 11 | 11 |
| Decision | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 |
| Alignment | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 |
| Resolution | 1 | 1 | 0 | 0 | 1 | 1 | 10 | 11 | 0 |

### What changed since the previous measurement

- **Changes are decoded from a flat parse** (`836be53`). The parser writes one list of nodes
  whose strings stay in the text, and the decoders read it directly, instead of building a
  tree of `Json` values and decoding the tree. String ends are found eight bytes at a time
  from the word mask, and an absent optional field is not looked for twice (`87cb0b5`).
- **A value read again is checked against a fingerprint** (`817d1f5`, STORAGE.md §2.6).
  SHA-256 of every value a projection read was a third of its instructions; the id was
  proved when the bytes were first read, and a 64-bit fingerprint taken then detects a file
  changed since. A book's files share each verse's derived plain text and selection text,
  decoded records are freed on four threads, decisions no longer clone their context to
  write it, and a projected record is decoded without the rest of its change.
- **The corpus generator** seals its packs on three threads and no longer scans every
  change written so far for the ones to encode (`abcaf26`).

### What misses and why

**Cold open (7.8 s against 3 s).** The fold takes 2.0 s on one thread; the rest is reading
the packs, which is CPU-bound: one thread needs about 16.8 s for the whole Bible, of which
about 10 s is SHA-256 (every byte is hashed twice, P2 and P4, at about 140 MB/s per thread
without SHA extensions) and 6.2 s is parsing and checking each change. Processors with SHA
extensions (Intel since Ice Lake and Goldmont, AMD since Zen) hash about ten times faster,
which would leave about 7.5 s of single-thread work for the load; that estimate, not a
measurement, still puts the cold open above 3 s with the fold's 2 s. STORAGE.md §14 names
the remedy: `.tc/index.bin` (heads and value locations, keyed by the hash of the pack and
pending file set), which `tc-store` owns.

**Psalms projection (358 ms against materialize's 300 ms).** The projection takes 930 ms of
CPU over four threads. Callgrind on the Psalms corpus counts 2.13 G instructions: decoding
the book's 25,132 projected records
from their JSON is 58% of it, rendering the tN and TWL decisions 34% (the selection text of
changed verses 8%), freeing the decoded records 6%. On this shared VM the four threads got
about 2.6 cores. Opening a book in the editor needs only the USFM: 14 ms, and 0.11 ms for
one chapter.

**Memory.** The folded state alone (202 MB) is above the app's steady budget of
150 MiB, and the open's peak (413 MB of heap) above its 350 MiB; both budgets are
`tc-store`'s for the whole app. Remaining levers, measured from the table sizes:
single-change batches without a batch entry (about 20 MB), check keys without heap strings
(about 15 MB), and folding the open's inputs as they are read instead of collecting them
first (most of the 233 MB the inputs hold at the peak).

### To reproduce

```sh
cargo build --release -p tc-corpus --example foldbench --example generate --example loadprof
target/release/examples/foldbench save bible /tmp/corpus-bible
target/release/examples/foldbench run /tmp/corpus-bible 4
target/release/examples/foldbench save bible-share 0.42 /tmp/corpus-278k
FOLDBENCH_FOLD_ONLY=1 target/release/examples/foldbench run /tmp/corpus-278k 4
target/release/examples/generate bible
target/release/examples/loadprof bible
cargo test -p tc-journal --test operation_counts -- --nocapture
FOLDBENCH_RUNS=1 FOLDBENCH_FOLD_ONLY=1 valgrind --tool=massif \
    target/release/examples/foldbench run /tmp/corpus-bible 1
```
