# What a quiet machine says about the journal, and what to do about it

- **Date:** 2026-10-16. **For:** whoever works on `tc-journal`, `tc-corpus` and the budgets.
- **Branch:** `bench-local` (cut from `claude/focused-ride-5uzj7p`). Nothing was pushed to
  `storage` or to `claude/focused-ride-5uzj7p`. Merge or cherry-pick from here.
- **Raw output:** `docs/evidence/journal-head-to-head-2026-10-16/` — every run log with its
  load and `CPU_Speed_Limit` lines, the per-save sweep, and both scripts, so every number
  above can be checked against what produced it.
- **Full record:** `docs/evidence/journal-head-to-head-2026-10-16.md` — machine, every run,
  medians, ranges, stage attribution, corpus byte counts.
- **Measured:** this repository at `6e8adac` (`storage`); tC4 at `d310bf3`. Both clean trees.
- **Why:** the container that builds this repository shares four vCPUs at a load of 6 to 10.
  `storage-fold-2026-10-16.md` says so itself ("wall times here are about 1.8 times what an
  idle machine gives"). Both sides were run on one quiet laptop, interleaved, five runs each,
  with the thermal speed limit read around every run. It never throttled.

## F0. Reconciliation with `docs/JOURNAL-DECISION-2026-10-16.md`

That memo landed on `claude/focused-ride-5uzj7p` (`905f82d`, `5e48078`) while this run was
going, so neither saw the other. It reaches its conclusions on the shared Xeon at load 2.6 to
17.8 **with no SHA instructions** — the conditions this run was commissioned to remove. Read
the two together as follows.

**Its question 5 to the owner is answered.** It asks: *"The reference laptop. Which machine is
it, and does it have SHA instructions? ... With them, the no-cache open is much cheaper."* The
machine used here has them (Ice Lake: SHA **and** AVX-512). F1 is the measurement, and "much
cheaper" is **6.5× on the hash, 2.1× on the cold open**.

**Its question 4 can drop an estimate.** It asks whether "about 5 to 8 s on 4 cores without
SHA instructions, estimate" is acceptable for a received project. On 4 cores **with** SHA
instructions the streamed cold open of 617,143 changes is **3,782.8 ms** measured, median of
5, p95 3,875.8 ms. The band is not needed for that CPU class.

**Where a quiet machine replaces a loaded one.** Same tC4 commit, same corpus sizes:

| Measure | Memo (loaded Xeon, no SHA ext.) | Here (quiet Ice Lake, SHA ext.) |
|---|---:|---:|
| tC4 fold only, 278,041 events | 13.9 s (median of 30) | **6.02 s** (median of 5) |
| Rebuild fold only, 617,143 changes | 2.89 s | **1.13 s** |
| Rebuild open, 4 threads, 617,143 | 12.7 s loaded; 7.8 s "quiet VM" | **3.78 s** |

**Where the two runs agree, independently.** These were reached separately and should now be
treated as settled: tC4 re-folds the whole set at every save and keeps no fold between opens
(its RT-4, F3 here); tC4's fold projects eagerly, so fold-against-fold rows compare different
stages (its RT-5, "Stages, named" here); tC4's read cost is per file (its RT-7, the 213 µs a
file here).

**One correction to this document's own headline.** The 40× cold-open ratio in F2 compares each
side **in its intended configuration** — tC4's reference reader is synchronous and
single-threaded, the rebuild's open uses 4 threads — and it ran on **APFS**, which costs more
per file than the memo's Linux virtio disk (213 µs an event here against its 135 µs). The
memo's single-thread row is the like-for-like ratio and it is **2.1×** (37.7 s against 17.6 s
at about 300k). Both numbers are honest and they answer different questions: 2.1× is "the same
work on one thread", 40× is "each design as it would actually run". **Quote 2.1× when comparing
implementations and 40× only with the thread counts and the filesystem stated.** Nothing in F2
is retracted, but the bare ratio should not travel on its own.

**One of the memo's three lessons needs narrowing.** It recommends adopting `ring` for a faster
SHA-256, measuring `sha2` 0.11 at 158 to 180 MB/s against `ring` 0.17 at 245 to 343 MB/s on a
CPU with no SHA instructions. Hashing 705,135,636 bytes once took 158.2 ms on 4 threads here —
about 4,460 MB/s aggregate, roughly **1,115 MB/s a thread, some 6.6× the memo's `sha2`
figure**. That is the size of the SHA-instruction speed-up, which means **`sha2` 0.11 is
already taking the hardware path on this CPU** and `ring` has little left to give there. Keep
the lesson for targets with no SHA instructions; do not expect it to help on recent Intel, AMD
or Apple silicon. Worth confirming with the memo's own probe run on this CPU before acting.

The memo's other two lessons are untouched by this run: hash each byte once (the open still
hashes every byte twice here, P2 and P4), and show progress after 300 ms.

**What this run does not settle for the memo.** Its central caveat stands: nobody has measured
a full open, because `tc-store` does not exist. Everything here is `foldbench`, so O9's
classification of derived paths is not in any number above, and neither is the J2 fold cache,
which was uncommitted and unreviewed and was not exercised.

## F1. The SHA-extension estimate is now a measurement, and it moves two budgets

`storage-fold-2026-10-16.md` states the premise as an estimate: *"Processors with SHA
extensions ... hash about ten times faster ... that estimate, not a measurement."* The VM is
Cascade Lake: AVX-512, no SHA extensions. The laptop is Ice Lake: AVX-512 **and** SHA
extensions, so it differs from the VM in close to that one respect.

Whole Bible, 617,143 changes, median of 5 (VM column = that record's quieter `225d19e` run):

| Measurement | VM, no SHA ext. | Ice Lake, SHA ext. | Ratio |
|---|---:|---:|---:|
| SHA-256 of every file once | 1,022 ms | **158.2 ms** | **6.5×** |
| Read, verify and prepare | 5,811 ms | 3,206.8 ms | 1.8× |
| Full fold of prepared changes | 2,010 ms | 1,125.8 ms | 1.8× |
| Cold open | 7,822 ms | 3,782.8 ms | 2.1× |
| Psalms projection | 358 ms | **268.7 ms** | 1.3× |
| Conflict list from scratch | 15.0 ms | 16.7 ms | 0.9× |
| Psalms USFM alone | 14.3 ms | 15.8 ms | 0.9× |

The last two rows carry the argument: the stages with no hashing in them are **slightly
slower** on the laptop. The gains above are SHA-256, not a faster machine.

**Two verdicts in that record change:**

- **Psalms projection meets its 300 ms budget** (268.7 ms, p95 286.7 ms). The record has it
  missed at 358 ms. The "What misses and why" section should no longer list it for a CPU with
  SHA extensions.
- **The cold open still misses 3 s** (3,782.8 ms) — but by 26%, not by 161%. `.tc/index.bin`
  is still needed. The gap it must close is a quarter of what the VM implies.

**Suggested actions.**

1. Say which CPU class a budget verdict belongs to. "Measure release builds" (PERFORMANCE.md
   rule 5) does not currently distinguish a CPU with SHA extensions from one without, and the
   difference is 6.5× on the stage that dominates the load. The reference laptop in the
   budgets table ("4 cores, SSD, 8 GB") is ambiguous on exactly the feature that matters.
2. Have `foldbench` print the SHA-extension and AVX-512 feature bits with the machine line, so
   no future record has to be re-interpreted.
3. Re-sequence `.tc/index.bin` against a 26% miss rather than a 161% one. It is still
   necessary; it may no longer be the most urgent thing.

## F2. The head-to-head, and an attribution that should temper the headline

Matched scale, median of 5. tC4's corpus is 278,041 events; `bible-share 0.42` is 277,880
changes — that corpus exists for this comparison and it did its job.

| Measurement | tC4 | This repository |
|---|---:|---:|
| Read + verify | — | 1,331.9 ms |
| Fold of what is in memory | 6,024.4 ms | 461.3 ms |
| Cold open (read + verify + fold) | **63,341.5 ms** | **1,592.6 ms** |
| Project one book (Psalms) | eager, in the fold | 147.6 ms |
| To the first book's text on screen | 63,341.5 ms | 1,606.8 ms |

**Do not report this as a language win.** tC4's read moves 247.6 MiB in 59.2 s — 4.2 MiB/s, or
**213 µs per file** across **278,041 files**. This repository reads 305.3 MiB from **206
files** at **229 MiB/s** on the same SSD. Most of the 40× is the storage layout. It also
stores 23% **more** bytes per change (1,152 B against 934 B), so none of it is explained by
writing less. A tC4 that packed its segments would recover much of that gap without a rewrite,
and anyone comparing these numbers will find that out.

Also: the 13× fold ratio is **not** like-for-like. tC4 projects eagerly inside `fold()`; this
repository defers projection to `render_book`. A fold-against-fold ratio credits this design
for work it has not done yet. Quote the cold open and the first-text row instead.

## F3. tC4's save cost is Θ(history). This is the strongest evidence for rule 1

`journalingStore.publishAndRegenerate` does, on **every** write:

```js
this.events.push(...stamped.map(normalizeEvent));
this.foldCache = null;
await this.ensureFold();          // fold(this.events) — the WHOLE event set
```

Measured with tC4's own fold over prefixes of its own corpus, 12 points from 14,467 to 278,041
events, 3 repetitions each, every corpus folding clean: **23.81 µs per event by least squares,
with 2% drift in per-event cost across a 19× range of history.** Linear, no plateau. A save at
whole-Bible scale costs 6.0 s. The same action here is 0.055 ms (p95 0.116 ms) against a 2 ms
budget — about 110,000×, and a different complexity class.

**State the reason fairly when citing this.** The full fold is not an oversight.
`journal/fold.mjs` is the BURRITO-SPEC §8.6 reference implementation and `fold(events)` is
*defined* as a pure function of the event set. Because events are deduped and sorted by ts, an
event arriving from a teammate can sort before events already held and retroactively change
per-key LWW outcomes and fork detection; a structural apply has branch-local effects that
reach backwards. tC4 also regenerates its derived Burrito paths from the fold and verifies
them by readback. Folding the whole set is the plainly correct way to hold that invariant and
is what stops a silent loss. The cost of holding it that way is the whole past, per save.

That is worth a sentence in PERFORMANCE.md beside rule 1 and in
`tests/operation_counts.rs`: the counters exist because the alternative has been measured at
23.81 µs an event, and the alternative is otherwise a reasonable reading of the spec.

## F4. `foldbench` reports no CPU time and no RSS on macOS

Every CPU and memory line came back zero: `cpu over all threads 0 ms`, `peak RSS 0 MiB`,
`RSS with the fold open 0 MiB`, and `CPU time of a projection, all threads: median 0 ms`. That
instrumentation is Linux-specific, so **none of the memory findings of
`storage-fold-2026-10-16.md` can be reproduced off Linux**, and the allocator comparison
cannot be repeated at all. Either implement the macOS path (`getrusage`/`mach_task_basic_info`
give both) or have `foldbench` say plainly that the figure is unavailable, so a zero is never
read as a measurement.

## F5. The corpus reproduces exactly

`foldbench save bible` generated **617,143 changes** — the same count as that record — in
32.0 s, 705,135,636 bytes over 304 files. `bible-share 0.42` gave 277,880 changes in 15.9 s,
320,177,136 bytes over 206 files. Byte counts are summed file sizes, not `du` blocks; tC4's
own `open-time-2026-09-05.md` carries a correction for exactly that mistake. The generator is
reproducible across OS and CPU, which is worth knowing.

## F6. Two things noticed while setting up, not measurements

- **The crate sets differ by branch.** `storage` has `tc-journal` and `tc-corpus` but not
  `tc-align`, `tc-check`, `tc-resources` or `tc-ui`; `claude/focused-ride-5uzj7p` and
  `door43-facts` have those and not the journal. Nothing holds both.
- **`tc-store`, `tc-import`, `tc-sync` and `tc-app` are on no branch**, though PERFORMANCE.md
  §Owners assigns budgets to all four. Several budgets therefore have no code to measure, and
  the cold open's named remedy (`.tc/index.bin`) belongs to a crate that does not exist.

## What this does not measure

- **tC4's open-time harness was not run.** It needs Playwright's Chromium, the `dev-env` pin
  fetches and the `pankosmia-web` rig — all network. The tC4 open figures are its own
  reference reader, which its record calls "a LOWER BOUND ... with no transport". The real
  client adds one HTTP read per segment on top, so the 40× is **conservative**.
- **Save-to-"Saved" end to end**, for either side. The tC4 figure is the fold a save triggers;
  publish and `installAndConverge` sit on top.
- **Memory**, for the reason in F4.
- **The page cache was warm.** 305 MiB and 673 MiB both fit in 16 GiB and neither side purges,
  so "cold open" means a cold process, not a cold cache — as in the VM evidence.
- tC4's fold is faster than its own August record (6,024 ms against 9,279 ms). That is tC4
  code between `9aa17f2` and `d310bf3`, not the machine.
- Anything past 278,041 events for tC4 is arithmetic on the 23.81 µs slope, not a measurement.

## Method worth copying

- Read `CPU_Speed_Limit` (`pmset -g therm`, or the platform equivalent) around every run. On a
  laptop chassis a throttled run is not comparable to an unthrottled one, and nothing in the
  output tells you it happened.
- Exclude the working trees from the platform file indexer before timing. While Spotlight
  indexed `target/` and `node_modules` it took 2.2 to 3.5 of four cores; load fell from about
  16 to 3.8 once excluded. Nothing was timed before that.
- Keep both corpora on one filesystem, outside anything indexed.
- Record the load at the start and end of each run, not once for the session.
