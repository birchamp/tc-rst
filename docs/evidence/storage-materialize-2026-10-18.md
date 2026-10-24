# The store's materialize, checkpoint, close and memory at Bible scale (unit S2)

Measured on 2026-10-18 on the owner's Linux machine, with the whole-Bible corpus written to a
real folder through the store's own open, seal, consolidation, materialization and first
checkpoint (`storebench setup`). The budgets are the owner's rulings of 2026-10-18
(`docs/BUDGET-REVIEW-2026-10-17.md`, "Owner's ruling"; `docs/PERFORMANCE.md`). The reference
machine is the five-year-old Windows laptop (4 cores, 8 GB, SATA SSD, Defender on, no SHA
instructions). **This is not a measurement on it**: every laptop figure below is this machine's
figure times the ratios the budget review measured (2.1 to 2.6 for work on several threads, 2.9
to 4.0 for work on one), and leaves Defender's scan of new files out.

## Verdicts

Nine rounds of each case ran over the day on three builds of this branch, with other jobs on the
machine and its drive (load average 3 to 14); the ranges below are over the rounds. A round that
the drive stalled in (a flush that waited for another process's journal commit) is named, not
dropped.

| Budget (PERFORMANCE.md) | This machine | Laptop, estimated | Verdict |
|---|---|---|---|
| Save a verse, record a decision or save an alignment while a materialize, checkpoint or whole-project materialize runs: 25 ms p95 | during a whole-project materialize: verse 14 to 24 ms p95 and decision 14 to 22 ms in eight of ten rounds (238 to 331 saves each), **190 to 244 ms in the two rounds whose jobs the drive stalled** (two of five jobs took 10 s, not 3); during a checkpoint (413 to 531 saves per round over ten checkpoints, one about every 15 ms): 19.7 to 26.8 ms p95 in ten of thirteen rounds and 29.8 to 44.9 ms in three, **23.9 to 26.8 ms in the last five**, after the seal was split into steps; beside a repack 14.9 to 16.2 ms; with nothing running 13.3 to 17.6 ms (median 7.4 to 8.7 ms: two flushes) | not estimated: a save is two flushes, which no ratio scales | **met in a quiet round, at the edge during a checkpoint, missed in a stalled one**; not shown on the laptop |
| Materialize one book: 1 s p95 on the reference laptop, no indicator | Psalms after one verse save 185 to 257 ms median, p95 231 to 333 ms in eight rounds of nine (1.28 s in one stalled round); after one decision 184 to 259 ms, p95 215 to 305 ms (1.19 s in one); Jude after one verse save 57 to 81 ms, p95 63 to 84 ms (0.98 s in one) | p95 0.5 to 1.3 s by the two bounds, about 0.6 to 1.0 s with the mix of a materialize (a third of it on four threads) | **not shown**: the estimate straddles 1 s |
| Materialize many or all files (a book added, pins changed, an upgrade): background, progress bar, saves continue | pins change over the whole Bible: 2.8 to 4.7 s median (five runs in each of eight rounds; 10 to 11 s for the stalled runs); first progress report 315 to 325 ms after the job began, 366 to 407 ms after the command; 23 to 43 progress events | 6 to 19 s (the budget review estimated 10 to 25 s for the same work on the actor) | **met** (no number is budgeted; progress and saves are) |
| Close a project: 3 s on the laptop (whole Bible, Psalms changed) | 0.92 to 1.05 s median, p95 1.0 to 1.4 s (five runs in each of seven rounds); 26 s once, before the repack ran on its own thread | 2.7 to 4.2 s median | **not shown**: the estimate straddles 3 s |
| Checkpoint: 500 ms (whole Bible) | after one Psalms save 460 to 591 ms median, p95 586 to 761 ms in nine rounds of thirteen and 1.1 to 3.4 s in four (a stalled drive, or the first checkpoint after a whole-project job); after the materialization already ran 266 to 319 ms median, p95 316 to 525 ms; the first one after a whole-project job 0.46 to 6.7 s | 0.8 to 1.3 s (already materialized), 1.3 to 2.4 s (Psalms dirty) | **missed by the estimate**; reported to the orchestrator for the owner |
| Memory of the project store: 400 MiB steady, 450 MiB peak, private bytes a minute after the last action; the close and the whole-project materialize included | one heap (`MALLOC_ARENA_MAX=1`, as the review's ratios assume): steady 302 to 305 MiB, peak 321 to 326 MiB. glibc's default heap: steady 403 to 405 MiB, peak 437 to 445 MiB at the materialize and 457 to 461 MiB at the close | the review measured the laptop at 0.97 to 1.07 of the one-heap figure | **met with one heap**; the default glibc heap misses by 3 to 5 MiB steady and 7 to 11 MiB at the close |
| Open with a usable fold cache: 3 s; without: 10 s; progress within 300 ms | 0.72 to 0.86 s median (p95 0.80 to 1.05 s); without a cache 0.90 to 1.41 s | not re-estimated (unit S1: 1.6 s and 3.4 s on a slower VM) | **no regression** |
| Outside edit of three verses, no budget | file written to the notice 30 to 49 ms median, p95 46 to 78 ms | | |

## What each action did

From the counters of the last run of each case (`StoreCounters`, APP-DESIGN R14).

| Action | Files opened / created | Derived read / projected / written | Bytes read | Bytes hashed (md5) | fsyncs |
|---|---:|---:|---:|---:|---:|
| Materialize Psalms after one verse save | 3 / 5 | 2 / 5 / 1 | 34 MB | 24.8 MB | 11 |
| Materialize Psalms after one decision | 2 / 5 | 1 / 5 / 1 | 39.7 MB | 23.0 MB | 11 |
| Materialize Jude after one verse save | 3 / 5 | 2 / 5 / 1 | 0.6 MB | 0.5 MB | 11 |
| Whole-project job (a pins change; 334 files) | 2 to 73 / 157 to 184 | 1 to 71 / 334 / 1 to 6 | 400 to 456 MB | 300 to 318 MB | 318 to 370 |
| Checkpoint with Psalms dirty | 12 / 9 | 3 / 5 / 2 | 57 MB | 29 MB | 19 |
| Examination of an outside edit of three Psalms verses | 8 / 6 | 1 / 5 / 0 | 28.6 MB | 19.0 MB | 13 |

- **A book's materialize touches only that book's files**: five files projected (the Psalms USFM,
  alignments, translationNotes, translationWords and comments), the others the book's heads files,
  `metadata.json`, the manifest and the intent. The count tests
  `a_books_materialize_touches_only_that_books_files` and `a_book_change_marks_only_that_books_files`
  assert it on the Psalms corpus and on the one ten times larger.
- **Each buffer is hashed once**: 24.8 MB hashed for 18.7 MB rendered and 6 MB read for the
  compare-and-swap of the one file that changed. The per-record digests of the heads files
  (D10) add 260 KB, 1% (`digest_bytes`).
- **An unchanged file is not read back**: 5 files rendered, 2 read (the one written and its
  predecessor in the group whose entry was still unsettled), 1 written.
- **The pacing**: a whole-project job waited 116 to 242 ms in all behind saves
  (`writer_yield_ns`), 0 when nothing was saved.

## Where the time goes

Temporary timers in the writer, taken on the build before the repack changes at load about 10:

| Psalms after one verse save (190 ms) | ms |
|---|---:|
| Render the book's five files (four threads; the largest file takes 60 to 95 ms) | 85 to 100 |
| M1: read and hash what is about to be replaced | 12 to 18 |
| M2: `metadata.json` for the planned result | 6 to 16 |
| M3 to M5: the intent, the file and its heads file, the folders | 30 to 90 |
| M6 to M8: `metadata.json`, its heads file, the manifest, the intent | 15 to 60 |

| Checkpoint after one save (about 300 ms) | ms |
|---|---:|
| Seal on the actor (the pending file into a pack, K1 to K4) | 22 to 35 |
| The writer's materialization (the new pack is listed in `metadata.json`) | 30 to 60 |
| Blobs of the changed derived files (read, md5, SHA-1, zlib, two fsyncs each) | 80 to 130 |
| `metadata.json` of the tree and `tc_burrito::validate_project` | 25 |
| `TreeUpdateBuilder` (five trees, each a loose object with its fsyncs) | 45 to 85 |
| The commit object, the branch (reference lock, write, flush, rename) and the index | 14 |

A checkpoint writes about twelve loose objects with two flushes each, and compresses 14 MB of
changed files. That, not the seal or the projection, is why the 500 ms row is missed; the
remedy is to write the objects of one checkpoint into one pack (libgit2's memory-backed object
database and `packwriter`: three flushes instead of about twenty-four), and it has not been
built (see the open issues in the unit's report).

## What the benchmark changed

- **The first run of `close` took 26 s once.** The close's own checkpoint made a repack due
  (the loose objects of 250 checkpoints), the writer ran it before it stopped, and libgit2's
  search for deltas between the versions of the Psalms files took that long. The repack now
  runs on a thread of its own in runs of at most 96 MiB of loose objects, a closing project
  starts none and stops the one that runs, and the close after a checkpoint with a repack due
  took 715 to 887 ms (`storebench repack`; its first run packed the backlog of 1,574 objects in
  9.3 s beside a project that opened in 0.94 s and saved at p95 14.9 ms).
- **The fold cache is written piece by piece.** Encoding its 65 MB into one `Vec` held up to
  twice that beside the fold at the close; the peak at close fell from 519 to 461 MiB with the
  default heap and from 375 to 321 MiB with one.

## Saves during a checkpoint

A checkpoint is about 40 flushes in half a second (the seal's, the materialization's, the two
flushes of each of the twelve loose git objects, the reference's), and a save needs two on the
same drive: the drive's flush queue, not the actor, is what a save waits for once the seal is in
steps. The seal itself was 22 to 35 ms on the actor and made the p95 of a save made during it
44.9 ms in one round; it now runs in steps (two pending files read, the packs built, each pack
written and read back, four files removed, the ledger, the consolidation) and the actor serves
the commands that wait between two steps (`a_save_is_served_between_the_steps_of_a_seal`), which
gave 23.9 to 26.3 ms in the three rounds that followed, at a load of 6 to 10. Three things are
left: the consolidation of a month's packs is one step (it runs once a month, and a save made
during it waits for the step), the reference move at the end of a checkpoint is an actor step of
about 8 ms (R21 allows it), and a checkpoint writes its git objects one at a time. Writing them
as one pack would take about 3 flushes instead of about 24, and was not built: libgit2 looks
for deltas between the objects of any pack it writes, and the new versions of three files of
5 to 8 MB would take that search longer than the flushes it saves.

## Memory in detail

| Moment | One heap | glibc default |
|---|---:|---:|
| After the open (private / peak) | 258 to 261 / 276 to 278 MiB | 340 to 347 / 342 to 357 MiB |
| A minute after 200 saves and 200 decisions in Psalms | 300 to 302 / 317 to 319 MiB | 403 to 404 / 402 to 406 MiB |
| After a whole-project materialize | 308 to 309 / 321 to 323 MiB | 437 to 440 / 439 to 443 MiB |
| After the close | 249 to 250 / 321 to 323 MiB | 384 to 393 / 457 to 461 MiB |

With one heap the whole-project job took 7.9 to 10.7 s, against 2.4 to 3.1 s with the default
heap: the projection allocates from several threads, and one arena makes them wait for each
other. Windows' heap does not do that, so the one-heap row is the memory estimate and the
default-heap row is the time estimate for this machine only.

## Machine and method

- Intel Core i7-12700 (12th generation), 20 threads, **SHA instructions yes, AVX-512 no**, 61 GB,
  ext4 on a SATA drive (`/dev/sdb2`), Linux 7.0. Other agents built and tested on the same
  machine and disk throughout: load average 5 to 14, printed in every log. The spikes of the
  noisy rounds (a save of 794 ms in one round of the checkpoint case, a checkpoint of 3.7 s) are
  fsyncs that waited for another process's journal commit; the quiet rounds do not show them.
- Corpus: `Corpus::generate(Spec::whole_bible(1))`, 617,086 changes, 334 derived files of
  277 MiB, 147 journal files of 674 MB, written by `storebench setup target/bench-s2`; the
  `.git` after the setup's checkpoint 167 MiB. Each case then wrote to the same project, so the
  later rounds ran on a project with a few hundred extra saves.
- Commits: three passes, on `d4649c7`, on `e0ed96d` and on `7f58505` (release,
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, which changes no code). Only the last had the
  file watcher running: the first two opened a relative folder name, which the watcher then did not
  match (fixed in `7f58505`), so the earlier rounds ran without it; a comparison of the whole-project
  case at a load of 3, once with the watcher telling and once with its word held back for an hour
  (`STOREBENCH_NO_WATCH=1`), gave 3.1 to 3.2 s with it and 3.4 to 3.7 s without, saves at p95 20 ms
  and 21 to 24 ms: the watcher costs nothing there. Its echo of the app's own writes is one read of
  each file the job wrote (6.3 MB after a Psalms materialize).
- Commands: `storebench materialize|whole|checkpoint|close|outside|memory|repack DIR` and
  `storebench run DIR 7` for the opens and the idle saves; `memory` once with
  `MALLOC_ARENA_MAX=1`. Rounds: materialize 3, whole 2, checkpoint 3, close 2 (5 runs each),
  outside 1 (20), memory 2, run 1 (7 opens per case, 200 saves and 200 decisions), repack 1.
  Every case prints the CPU's SHA and AVX-512 flags and the load average.

## What is not shown

- Windows with Defender: every file the writer creates is a scan there (a Psalms materialize
  creates 5 files and renames 3, a whole-project job about 170), and NTFS's flush costs differ.
- The laptop itself. Its checkpoint and close figures are the least certain: they are one-thread
  work and flushes, where the ratio is widest.
- A save during the writer's flushes on the laptop; the pacing (50 ms at most) was tested with
  `FaultFs` delays and measured here only.
- The SHA-256 of the seal is 2 KB per checkpoint here (44 KB for 20), so the CPU's SHA
  instructions do not matter to these cases; md5 (25 MB per Psalms materialize), libgit2's SHA-1
  and zlib have no hardware help on either machine.
