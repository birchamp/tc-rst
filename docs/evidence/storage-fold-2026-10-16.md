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

## What is measured

`crates/tc-corpus/examples/foldbench.rs`. `foldbench save bible DIR` generates the whole-Bible
corpus of STORAGE.md §14 with `tc-corpus` (seed 1) and writes its packs and pending files
under their repository paths. `foldbench run DIR 4` then opens it the way `tc-store` will:

1. **Read, verify and prepare.** Four threads read every file, run `read_pack` or
   `read_pending` (P1 to P4: SHA-256 of the file, SHA-256 of every line, the per-change
   rules) and prepare each change with `FoldInput::prepare_at`, which keeps a summary and
   the change's span in its file, not its bytes.
2. **Full fold.** `FoldState::fold_from` with a `ValueSource` that reads a span of a file
   again (`pread`). Values are read on demand from here on, and checked against their ids.
3. On the folded state: 200 incremental steps each for a verse save, a decision and an
   alignment on Psalms, 100 conflicts made and resolved on Psalms verses, the conflict list
   and counts rebuilt from every record (`conflicts_rebuilt`) 20 times, and every file of
   Psalms rendered 20 times (`render_book`: USFM, alignments, tN and TWL decisions,
   comments), with values read from the files.

A change written by a benchmark step is a new change of a benchmark device, made from the
values the fold projects, and its parents are the token rule's (`shown_heads`), as a screen
would write it. Times are wall-clock times on the main thread; the projection's CPU time is
the process's user and system time over all threads.

The corpus: 66 books, 31,103 verses, 215,868 checks, 3 devices over 750 working days.
617,143 changes in 553,978 batches (664,344,248 bytes of canonical JSON) in 131 packs and
172 pending files, 674 MiB on disk. The fold holds 283,593 records and lists 124 conflicts
and 65 anomalies. Every part is labelled by `Corpus::describe`: the ULT, tN, TWL, UHB and
UGNT text and rows are real for the books the releases hold, the rest sampled, and the
history generated.

## Machine

Intel Xeon @ 2.80 GHz, 4 vCPU, AVX2 and AVX-512 but no SHA extensions; 15 GiB RAM; virtio
disk; Linux 6.18; Rust 1.97.0, release profile. The machine is shared with other agents'
builds: the load average was 1.3 to 2.3 during the runs. Measured at commit `62cc119`,
2026-10-16.

## Results

Whole Bible, `foldbench run DIR 4`:

| Action | Runs | Median | p95 | Budget |
|---|---:|---:|---:|---|
| Read, verify and prepare (4 threads) | 5 | 5,958 ms | 6,080 ms | |
| SHA-256 of every file once (4 threads; a load hashes every byte twice) | 3 | 909 ms | 1,015 ms | |
| Full fold of the prepared changes (one thread) | 5 | 1,472 ms | 1,599 ms | part of 3 s |
| Cold open: read, verify, prepare, fold | 5 | 7,491 ms | 7,625 ms | 3 s: **missed** |
| Incremental step: save one Psalms verse | 200 | 0.040 ms | 0.185 ms | 2 ms |
| Incremental step: record one Psalms decision | 200 | 0.008 ms | 0.012 ms | 1 ms |
| Incremental step: save one Psalms alignment | 200 | 0.008 ms | 0.014 ms | 2 ms |
| Incremental step: resolve one Psalms conflict | 100 | 0.022 ms | 0.052 ms | 1 ms |
| Conflict counts after a save (kept by every step) | 200 | 0.000 ms | 0.000 ms | 1 ms |
| Conflict list and counts from scratch | 20 | 20.5 ms | 20.6 ms | 100 ms |
| Psalms projection: every file of the book, values read from disk | 20 | 406 ms | 435 ms | 300 ms (materialize): **missed** |
| Psalms USFM alone | 20 | 23.0 ms | 24.2 ms | 150 ms (open a book) |
| One Psalms chapter's USFM | 20 | 0.151 ms | 0.528 ms | 50 ms |
| Snapshot (clone of the folded state) | 20 | 0.011 ms | 0.163 ms | |

The Psalms projection writes 5 files, 18,725,857 bytes; its CPU time over all threads has a
median of 1,030 ms.

The fold of a corpus of 277,880 changes (`foldbench save bible-share 0.42`, the size of tC4's
probe corpus of 278,041 actions): 5 runs, median 591 ms, p95 668 ms (the probe folded its
events in 1.13 s).

Corpus generation, one run each (`examples/generate`): Psalms 2.1 s (47,357 changes), Psalms
made ten times larger 29.8 s (489,713 changes), whole Bible 31.7 s (617,143 changes).

## Memory

Massif, whole Bible, one reading thread (`FOLDBENCH_RUNS=1 FOLDBENCH_FOLD_ONLY=1`): the
folded state holds 196.9 MB of heap (213.2 MB with the allocator's overhead), about 320
bytes per change: the change table 79.2 MB (128 bytes a change), the key table 45.9 MB (160
bytes a record), the batch table 31.2 MB (56 bytes a batch), the four position indexes
29.4 MB, the rest 11 MB. The peak during the open is 430.8 MB of heap (462 MB with
overhead), while the prepared inputs (about 290 bytes a change) and the growing tables are
both alive. The process's resident size after five opens was 496 MiB, which includes memory
the allocator keeps after frees.

Before this work the folded state kept every change's bytes (675 MB) and about 550 bytes of
tables per change.

## Operation counts

`crates/tc-journal/tests/operation_counts.rs` runs each step on the Psalms corpus (47,357
changes) and on the corpus made ten times larger (489,713 changes, 30 devices), on a Psalms
chapter whose records are alike in both. Every counter is equal on both corpora:

| Step | Indexed | Keys updated | Keys recomputed | Values decoded | Books checked | Chapters cohered | Verses cohered | Attached placed | Records projected |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Verse save and its chapter's USFM | 1 | 1 | 0 | 11 | 1 | 1 | 10 | 11 | 11 |
| Decision | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 |
| Alignment | 1 | 1 | 0 | 0 | 1 | 0 | 0 | 1 | 0 |
| Resolution | 1 | 1 | 0 | 0 | 1 | 1 | 10 | 11 | 0 |

## What misses and why

**Cold open (7.5 s against 3 s).** The fold takes 1.47 s; the rest is reading the packs.
Of the 5.96 s load on four threads, about 1.8 s is SHA-256 (two passes over 674 MiB at about
740 MiB/s on four threads, without SHA extensions), and most of the rest is decoding each
change's JSON into a tree and then into a record to check the per-change rules (callgrind
on the Psalms corpus: 55% of `parse_change` is the JSON parser, a quarter decoding the
tree). STORAGE.md §14 already names the remedy for a missed cold open, `.tc/index.bin`
(heads and value locations keyed by the pack set), which `tc-store` owns; decoding without
an intermediate tree would cut the load further and is an open item of `tc-journal`.

**Psalms projection (406 ms against materialize's 300 ms).** The projection decodes every
record of the book once, on four threads, and renders the files on four threads; it costs
1.03 s of CPU. A quarter is SHA-256 checking each value read from disk against its change id
(the guard against a file changed after it was read), and most of the rest is the same JSON
decoding. On this shared machine the threads got about 2.5 cores. Opening a book in the
editor needs only the USFM: 23 ms, and 0.15 ms for one chapter.

**Memory (197 MB folded, 431 MB peak, against 150 MiB steady and 350 MiB peak for the
app).** The budget is `tc-store`'s for the whole app, and the folded state alone is above
the steady budget. Remaining levers, measured from the table sizes above: single-change
batches without a batch entry (about 20 MB), check keys without heap strings (about 15 MB),
and folding the open's inputs as they are read instead of collecting them first (about
180 MB of the peak).

## To reproduce

```sh
cargo build --release -p tc-corpus --example foldbench --example generate
target/release/examples/foldbench save bible /tmp/corpus-bible
target/release/examples/foldbench run /tmp/corpus-bible 4
target/release/examples/foldbench save bible-share 0.42 /tmp/corpus-278k
FOLDBENCH_FOLD_ONLY=1 target/release/examples/foldbench run /tmp/corpus-278k 4
target/release/examples/generate bible
cargo test -p tc-journal --test operation_counts -- --nocapture
FOLDBENCH_RUNS=1 FOLDBENCH_FOLD_ONLY=1 valgrind --tool=massif \
    target/release/examples/foldbench run /tmp/corpus-bible 1
```
