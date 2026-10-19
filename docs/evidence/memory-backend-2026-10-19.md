# The memory backend and the session registry: what they cost (unit B1b, 2026-10-19)

Unit B1b built tc-app's storage seam, the memory backend, the scenario loader, the session
registry and the operation-count harness (APP-DESIGN §10, phase 1). No PERFORMANCE budget
applies to these numbers. They size the harness and check rule 1 with a clock beside the count
tests, which are the proof (`crates/tc-app/tests/op_counts.rs`).

## Machine and commit

- The owner's Linux PC: 12th Gen Intel Core i7-12700, 20 logical CPUs, 61 GB. The CPU has SHA
  instructions (`sha_ni`) and no AVX-512 (`avx512f` absent). Shared with up to three other agents
  building at the same time: the load average was 3.9 during the in-process bench and 13.8 during
  the test-suite timings, so the suite's wall times below vary by a factor of two.
- Commit `98c10ed` (branch `app-b1b`), release build unless a line says otherwise.
- In-process numbers: `cargo run --release -p tc-app --features fixture-backend,test-hooks
  --example memory_bench -- --runs 20` (20 runs; 5 for the corpora, which take over a second).
- `tc4 serve --scenario` numbers: a Python timer from spawn to the address line on stdout, 5
  runs, each with a fresh app-data folder ("cold": the twelve resources are installed) and then
  again on the same folder ("warm": they are found installed).

## Numbers

| What | Median | p95 |
|---|---:|---:|
| Generate and fold the Psalms corpus (`Spec::psalms(1)`), 5 runs | 1,106 ms | 1,161 ms |
| Generate and fold ten times Psalms (`Spec::scaled(1, 10)`), 5 runs | 12,598 ms | 14,306 ms |
| `handle.snapshot()` and a clone of it, Psalms, per call | 31.5 ns | 32.3 ns |
| The same, ten times Psalms | 31.5 ns | 33.2 ns |
| A chapter read of Psalms 13 (R1's `chapter_keys`, `key_state` and `mint` per verse, `chapter_usfm`), Psalms | 0.027 ms | 0.030 ms |
| The same, ten times Psalms | 0.029 ms | 0.032 ms |
| The first chapter read of the process, Psalms (one run) | 1.17 ms | — |
| The first chapter read on ten times Psalms, the process already warm (one run) | 0.095 ms | — |
| `open_screen`, `leave_screen` and `settled` (the session closed), Psalms | 0.031 ms | 0.048 ms |
| The same, ten times Psalms | 0.020 ms | 0.143 ms |
| `MemoryBackend::from_scenario` on `sample` (both seeds, one refused) | 24.8 ms | 28.2 ms |
| `install_resources` of `sample`'s twelve resources into a fresh store (179 files, each fsynced) | 30.7 ms | 34.9 ms |
| `tc4 serve --scenario` (release, with the fixture backend) to the address line, cold | 49.2 ms | 53.4 ms |
| The same, warm | 33.0 ms | 33.7 ms |
| `tc4 serve --scenario` (debug, what the parity harness runs) to the address line, cold | 208 ms | 219 ms |
| The same, warm | 132 ms | 138 ms |

Test-suite cost, debug, on the shared machine:

| What | Before B1b | After B1b |
|---|---:|---:|
| `cargo test -p tc-app -j 6`, built, wall | 3.2 s (271 tests) | 20.0 s, 35.3 s, 44.4 s (three runs; 349 tests) |
| `cargo test -p tc-app -j 6 --test op_counts`, built, wall | — | 25.0 s, 30.0 s, 32.6 s |

## What the numbers say

- **A snapshot is O(1).** 31.5 ns per `snapshot()` and clone on both corpora: a mutex and two
  `Arc` clones. The count test `snapshot_costs_nothing_at_any_size` agrees: no fold work, no app
  count.
- **A chapter read costs one chapter.** 27 and 29 µs on the two corpora, within noise of each
  other; the count test `a_chapter_read_costs_one_chapter` shows the same counts on both (14
  records read, 59 keys from the range read, one chapter projected, 7 values decoded, 7 records
  projected). PERFORMANCE's "first chapter's verses ready" budget is 50 ms; the read is about
  0.06 % of it. The first read of a process is slower (1.2 ms on Psalms right after the corpus was
  made; a hundred untimed reads warm the bench before its timed runs): an earlier run of the bench
  without the warm-up timed Psalms first and showed it at 1.07 ms against 0.034 ms for ten times
  Psalms, which looked like a difference between the corpora and was only the warm-up. Run in
  alternating rounds, both corpora read in 26 µs once warm.
- **An open is O(1) in the memory backend.** The first measurement found an open and close at
  0.33 ms on ten times Psalms and 0.06 ms on Psalms: the backend published a copy of its fold at
  every open (a `FoldState` clone copies its chunk pointers), which costs no fold work, so the
  count test `open_lists_no_key` passed while the clock grew. The backend now shares its fold by
  `Arc` (`98c10ed`), and the open and close take 20-31 µs on both. tc-store's open is its own
  and has its own budget (3 s / 10 s for a whole Bible).
- **A parity start is cheap.** `tc4 serve --scenario` reaches its address line in 49 ms cold
  (release) and 208 ms cold (debug, as the harness runs it); of that, the two seeds take about
  25 ms and the twelve installs about 31 ms in a release build.
- **The suite pays for the corpora.** The operation-count binary generates and folds both
  corpora once (about 14 s in a release build; tc-journal and tc-corpus build at opt-level 2 in
  the dev profile, and the debug run takes 25-33 s on the loaded machine). tc-journal's own
  `operation_counts` pays the same; AD32 puts the count rows in tc-app from B1b on.
