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

## Machine

Intel Xeon @ 2.80 GHz (Cascade Lake: AVX-512, no SHA extensions), 4 vCPU, 15 GiB RAM;
virtio disk; Linux 6.18; Rust 1.97.0, release profile. Measured at commit `225d19e`,
2026-10-16, 05:42 to 05:47 UTC.

The VM is shared and its speed varies over the day. The same code (`62cc119`; `59bc3e1`
changed only documents) folded the corpus in 1.47 s at about 02:30 and in 2.10 s at 05:43,
and generated it in 31.7 s and in 57 s. Each comparison below between builds therefore
runs both builds in the same session, one right after the other. Four loading threads did about 2.6 times the work of one (21 to 22 s of CPU
over a 5.8 s load, for about 16.8 s of single-thread work).

## Results

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

## Memory

Massif, whole Bible, one reading thread (`FOLDBENCH_RUNS=1 FOLDBENCH_FOLD_ONLY=1`): the
folded state holds 201.9 MB of heap (218.2 MB with the allocator's overhead), about 330 bytes
per change. The change table is 83.0 MB, 136 bytes a change: 8 more than at `62cc119`, for
the fingerprint of the change's value (the state held 196.9 MB then). The key table (160
bytes a record), the batch table (56 bytes a batch) and the four position indexes did not
change. The peak during the open is 412.9 MB of heap (433.5 MB with overhead), while the
prepared inputs (233 MB) and the growing tables are both alive. The process's resident size
after five opens was 592 MiB, which includes memory the allocator keeps after frees.

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

## What changed since the previous measurement

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

## What misses and why

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

## To reproduce

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
