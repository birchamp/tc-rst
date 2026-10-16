# tC4's journal against the rebuild's, on one machine

**Date:** 2026-10-16. **Why:** the cloud container that builds the rebuild shares four vCPUs
at a load of 6 to 10, so its timings carry a factor the measurement cannot separate from the
code. Both sides were therefore run on one quiet local machine, interleaved, five runs each.

**tC4:** `unfoldingWord/translationCore4` at `d310bf3` (clean tree; `bench-fold.mjs` stamps the
commit and marks a dirty tree). **Rebuild:** this repository at `6e8adac` (`storage`, clean).

**Raw output:** `journal-head-to-head-2026-10-16/` — all 15 run logs with their load and
thermal-limit lines, the per-save sweep, and the two scripts, committed as they ran. See its
`README.md` for the layout.

## Machine

MacBook Pro 16,2 (13-inch, 2020). Intel Core i7-1068NG7 @ 2.30 GHz — **Ice Lake: AVX-512 AND
SHA extensions** — 4 physical cores, 8 threads. 16 GiB RAM. Internal APFS SSD, 64 GiB free.
macOS 26.6.2 (25G83). Node v22.23.2, npm 10.9.8. rustc 1.97.1, release profile.

`CPU_Speed_Limit` was 100 at the start and end of **every one of the 15 runs**: the chassis
never thermally throttled. One-minute load average at each run's start and end:

| Run | tC4 start / end | Rebuild 278k start / end | Rebuild 617k start / end |
|---|---|---|---|
| 1 | 4.68 / 1.37 | 1.37 / 1.46 | 2.78 / 4.76 |
| 2 | 1.46 / 2.96 | 2.96 / 3.43 | 4.76 / 5.02 |
| 3 | 3.43 / 3.28 | 3.28 / 3.95 | 5.02 / 4.93 |
| 4 | 3.95 / 3.24 | 3.24 / 3.43 | 4.93 / 4.80 |
| 5 | 3.43 / 2.99 | 2.99 / 3.21 | 4.80 / 4.68 |

The machine was not idle: the residue is the editor application that drove the run (its
renderer and the window server, together about one core). Spotlight was excluded from the
working trees first — while it indexed them it took 2.2 to 3.5 of the four cores, and nothing
was timed in that state.

## The corpora, and why they are comparable

tC4's whole Bible and the rebuild's whole Bible are **not the same size**: tC4's probe models
278,041 saves of a checked Bible, the rebuild's STORAGE.md §14 model 617,143 changes over
three years and three devices. `tc-corpus` already carries the corpus that closes the gap —
`foldbench save bible-share 0.42`, built to "the size of the probe's 278,041 actions". That
matched pair is the head-to-head; the 617,143-change corpus is reported separately as the
rebuild's own scale.

| Corpus | Events / changes | Bytes on disk | Files | Bytes per event |
|---|---:|---:|---:|---:|
| tC4, checked whole Bible | 278,041 | 259,580,461 | **278,041** | 934 |
| Rebuild, `bible-share 0.42` | 277,880 | 320,177,136 | **206** | 1,152 |
| Rebuild, whole Bible | 617,143 | 705,135,636 | 304 | 1,142 |

Byte counts are summed file sizes, not `du` blocks. The rebuild stores 23% **more** bytes per
event, so nothing below is explained by it writing less. The file counts differ by 1,350×:
tC4's reference reader seals one segment per event, the rebuild packs changes. The corpus
generated here holds 617,143 changes, byte-identical in count to the figure in
`storage-fold-2026-10-16.md`, so this is the same corpus that evidence measured.

## Stages, named

Each number below covers exactly these stages, and nothing else.

| Stage | tC4 | Rebuild |
|---|---|---|
| read | — | read each pack and pending file |
| verify | — | SHA-256 of the file, SHA-256 of every line, the per-change rules |
| parse | (corpus held in memory) | `parse_change`, then `FoldInput::prepare_at` |
| fold | `fold(events)`: validate, dedup, sort by ts, per-key LWW with fork detection, structural apply, **and project** | `FoldState::fold_from`: index heads and positions; values stay on disk |
| project | eager, inside the fold | deferred to `render_book` per book |

**The two folds are not the same stage.** tC4 projects while it folds; the rebuild defers
projection to the book a screen asks for. A fold-against-fold ratio therefore rewards the
rebuild for work it has not done yet. The comparable rows are the cold open and the
end-to-end row.

## Head to head at matched scale

Median of 5 runs, with the full range.

| Measurement | tC4 (278,041 events) | Rebuild (277,880 changes) |
|---|---:|---:|
| Read + verify | — | **1,331.9 ms** [1,207.4–1,415.0] |
| Fold of what is in memory | **6,024.4 ms** [5,888.8–6,699.8] | **461.3 ms** [431.5–486.6] |
| Read + verify + fold (cold open) | **63,341.5 ms** [63,061.6–65,207.6] | **1,592.6 ms** [1,515.1–1,758.3] |
| Read + verify alone | 59,200.8 ms [58,480.9–67,404.6] | 1,331.9 ms |
| The fold's work after the last file | — | 186.1 ms [181.1–196.3] |
| Project one book (Psalms, every file) | eager, in the fold | 147.6 ms [141.3–152.1] |
| Project one book's USFM alone | eager, in the fold | 14.2 ms [14.0–14.6] |
| **To the first book's text on screen** | **63,341.5 ms** | **1,606.8 ms** |

### Every run

tC4, `node conformance/bench-fold.mjs --bible` (each number is itself the median of the
harness's 3 repetitions):

| Run | fold (ms) | scan (ms) | scan+fold (ms) |
|---|---:|---:|---:|
| 1 | 6,699.8 | 59,200.8 | 63,285.0 |
| 2 | 5,942.9 | 58,480.9 | 63,061.6 |
| 3 | 5,888.8 | 58,645.9 | 65,207.6 |
| 4 | 6,379.7 | 67,404.6 | 63,341.5 |
| 5 | 6,024.4 | 59,740.1 | 64,629.5 |

Rebuild, `FOLDBENCH_RUNS=1 foldbench run corpus-278k 4`:

| Run | read+verify | fold | cold open | fold tail | Psalms proj. | Psalms USFM | SHA-256 once |
|---|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1,207.42 | 461.30 | 1,515.14 | 181.15 | 146.56 | 14.04 | 67.38 |
| 2 | 1,219.67 | 431.49 | 1,592.57 | 196.34 | 147.55 | 14.24 | 63.06 |
| 3 | 1,339.14 | 472.04 | 1,690.06 | 186.15 | 152.15 | 14.58 | 72.63 |
| 4 | 1,331.90 | 443.18 | 1,540.24 | 182.47 | 141.34 | 14.22 | 64.01 |
| 5 | 1,415.00 | 486.62 | 1,758.28 | 195.39 | 149.62 | 14.23 | 70.60 |

## What dominated each side

**tC4's open is per-file cost, not bandwidth.** 247.6 MiB in 59.2 s is 4.2 MiB/s, or **213 µs
per file** over 278,041 files. The rebuild reads 305.3 MiB from 206 files in 1,331.9 ms —
**229 MiB/s**, a 55× difference in effective rate on the same SSD. Most of the 40× open gap is
therefore the **storage layout**, not the language and not the fold algorithm. A tC4 that
packed its segments would recover a large part of it without a rewrite.

**The rebuild's open is the read, and the read is the hash.** 1,331.9 ms of its 1,592.6 ms
cold open is read-verify-prepare; the fold's own tail is 186.1 ms. Within the read, SHA-256
over every file once is 67.4 ms, and a load hashes every byte twice.

## The cost of a save — the finding that matters most

In `journalingStore.publishAndRegenerate`, every tC4 write does:

```js
this.events.push(...stamped.map(normalizeEvent));
this.foldCache = null;
await this.ensureFold();          // fold(this.events) — the WHOLE event set
```

So **a tC4 save re-folds the entire journal**, and the 6,024 ms above is that per-save cost at
whole-Bible scale. This is deliberate, not an oversight: `journal/fold.mjs` is the
BURRITO-SPEC §8.6 reference implementation and `fold(events)` is defined as *a pure function
of the event set*. Because events are deduped and **sorted by ts**, an event arriving from a
teammate can sort before events already held and retroactively change per-key LWW outcomes
and fork detection; a structural apply has branch-local effects that reach backwards. tC4 also
regenerates its derived Burrito paths from the fold and verifies them by readback. Folding the
whole set is the plainly-correct way to hold that invariant, and it is what stops a silent
loss.

The cost of holding it that way is linear in the project's whole history. Measured with tC4's
own fold module over prefixes of its own corpus at book boundaries, 3 repetitions each,
every corpus folding clean:

| Events | Fold median (ms) | µs per event |
|---:|---:|---:|
| 14,467 | 334.2 | 23.10 |
| 23,723 | 517.0 | 21.79 |
| 39,095 | 893.5 | 22.86 |
| 59,571 | 1,271.2 | 21.34 |
| 111,065 | 2,599.7 | 23.41 |
| 176,930 | 4,201.5 | 23.75 |
| 195,118 | 4,596.8 | 23.56 |
| 211,771 | 5,127.6 | 24.21 |
| 257,880 | 6,255.7 | 24.26 |
| 265,428 | 6,134.1 | 23.11 |
| 273,566 | 6,403.7 | 23.41 |
| 278,041 | 6,526.1 | 23.47 |

Least squares over these twelve points: **23.81 µs per event**, and the per-event cost drifts
2% across a 19× range of history. There is no plateau. A save costs the whole past.

The rebuild holds the same no-loss rule — AGENTS.md: *never lose, hide or silently rewrite;
if two versions disagree, keep both and show the conflict* — and meets it incrementally.
On the matched corpus, medians and p95 of the run counts shown:

| Action | Runs | Median | p95 | Budget |
|---|---:|---:|---:|---|
| Save one Psalms verse | 200 | **0.055 ms** | 0.116 ms | 2 ms |
| Record one Psalms decision | 200 | 0.044 ms | 0.066 ms | 1 ms |
| Save one Psalms alignment | 200 | 0.009 ms | 0.016 ms | 2 ms |
| Save one verse, with a live snapshot | 200 | 0.100 ms | 0.158 ms | 2 ms |
| Resolve one Psalms conflict | 100 | 0.024 ms | 0.048 ms | 1 ms |
| Conflict counts after a save | 200 | 0.000 ms | 0.000 ms | 1 ms |
| Conflict list and counts from scratch | 20 | 8.402 ms | 9.707 ms | 100 ms |

**A save at the same scale: 6,024 ms against 0.055 ms, about 110,000×.** The difference is not
speed, it is complexity class — Θ(history) against Θ(what the action touches), the latter
enforced by `tc-journal/tests/operation_counts.rs`, whose counters must be identical on Psalms
and on a corpus ten times larger.

Typing itself should stay smooth on both: tC4 runs the fold in a Web Worker (`foldWorker.ts`,
"off the main thread in the browser"). What waits on the full fold is the save's
acknowledgement — `publishAndRegenerate` awaits `ensureFold()` before it regenerates anything
— and every view that reads the folded state. `resolveVerse` is worse: it runs additional
whole-journal "dry folds" in a loop over candidate bases to find one that resolves cleanly,
"usually one", but not bounded at one.

## A CPU with SHA extensions: the estimate in `storage-fold-2026-10-16.md`, measured

That record states the remedy's premise as an estimate: *"Processors with SHA extensions
(Intel since Ice Lake and Goldmont, AMD since Zen) hash about ten times faster ... that
estimate, not a measurement"*. The VM is Cascade Lake — AVX-512, no SHA extensions. This
machine is Ice Lake: AVX-512 **and** SHA extensions. It differs from the VM in essentially
that one respect, so the comparison below is close to a controlled one.

Whole Bible, 617,143 changes, median of 5 runs (the VM column is that record's quieter
`225d19e` measurement):

| Measurement | VM, no SHA ext. | Here, SHA ext. | Ratio | Budget |
|---|---:|---:|---:|---|
| SHA-256 of every file once | 1,022 ms | **158.2 ms** [153.6–170.4] | **6.5×** | |
| Read, verify and prepare | 5,811 ms | **3,206.8 ms** [3,057.5–3,513.4] | 1.8× | |
| Full fold of the prepared changes | 2,010 ms | **1,125.8 ms** [1,111.0–1,210.2] | 1.8× | part of 3 s |
| Cold open | 7,822 ms | **3,782.8 ms** [3,536.1–3,875.8] | 2.1× | 3 s: **still missed** |
| Psalms projection | 358 ms | **268.7 ms** [264.5–286.7] | 1.3× | 300 ms: **now met** |
| Conflict list from scratch | 15.0 ms | 16.7 ms | 0.9× | 100 ms |
| Psalms USFM alone | 14.3 ms | 15.8 ms | 0.9× | 150 ms |

The last two rows are what make this readable: the stages with no hashing in them are
**slightly slower** here, so the gains above are not a faster machine. They are SHA-256.

Two verdicts in that record change on a CPU with SHA extensions:

- **Psalms projection meets its 300 ms budget** (268.7 ms), where the record has it missed.
- **The cold open still misses 3 s**, but by 26% rather than by 161%. `.tc/index.bin` is still
  needed; the gap it has to close is a quarter of what the VM shows.

Per-run figures, whole Bible:

| Run | read+verify | fold | cold open | Psalms proj. | SHA-256 once |
|---|---:|---:|---:|---:|---:|
| 1 | 3,057.54 | 1,125.75 | 3,536.13 | 264.50 | 164.24 |
| 2 | 3,255.57 | 1,110.95 | 3,650.27 | 271.18 | 156.55 |
| 3 | 3,513.42 | 1,210.24 | 3,835.94 | 286.66 | 170.43 |
| 4 | 3,206.75 | 1,140.71 | 3,875.75 | 268.72 | 158.23 |
| 5 | 3,060.93 | 1,124.12 | 3,782.81 | 265.42 | 153.64 |

## What this does not measure

- **tC4's open-time harness was not run.** `open-time-2026-09-05.md` and
  `open-time-windows-2026-09-28.md` need Playwright's Chromium, the `dev-env` pin fetches and
  the `pankosmia-web` rig — all network. The tC4 open figures here are its own reference
  reader, which that record calls "a LOWER BOUND ... with no transport": the real client adds
  one HTTP read per segment on top.
- **Save-to-"Saved" end to end** is not measured for either side. The tC4 number is the fold a
  save triggers; publish and `installAndConverge` are on top of it.
- **No CPU time and no RSS.** `foldbench` reports both as 0 on macOS — that instrumentation is
  Linux-specific — so the memory findings of `storage-fold-2026-10-16.md` are not reproduced
  here and its allocator comparison cannot be repeated.
- **The page cache was warm.** 305 MiB and 673 MiB both fit in 16 GiB and neither side purges
  between runs, so "cold open" means a cold process, not a cold cache. This matches how the
  VM evidence was taken.
- **tC4's fold is faster than its own record**: 6,024 ms here against 9,279 ms in
  `bench-fold-2026-08-25.md`. That is tC4 code between `9aa17f2` and `d310bf3`, not the
  machine — do not read it as a verdict on either processor.
- **The 13× fold ratio is not a like-for-like**, for the reason given under "Stages, named".
- The projection beyond 278,041 events (about 14.8 s a save at 620,000, by the 23.81 µs
  slope) is **arithmetic from a measured slope, not a measurement**. tC4's corpus stops at
  278,041.

## To reproduce

```sh
# tC4, at d310bf3, clean tree, after npm ci
BENCH_TMPDIR=/path/not/indexed node conformance/bench-fold.mjs --bible

# the rebuild, at 6e8adac
cargo build --release -p tc-corpus --example foldbench
target/release/examples/foldbench save bible-share 0.42 /path/corpus-278k
target/release/examples/foldbench save bible /path/corpus-bible
FOLDBENCH_RUNS=1 target/release/examples/foldbench run /path/corpus-278k 4
FOLDBENCH_RUNS=1 target/release/examples/foldbench run /path/corpus-bible 4
```

Interleave the two sides, five runs each, and read `CPU_Speed_Limit` from `pmset -g therm`
around every run: on a laptop chassis a throttled run is not comparable to an unthrottled one.
Exclude the working trees from the platform's file indexer before timing anything.
