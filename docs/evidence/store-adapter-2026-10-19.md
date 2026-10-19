# The store adapter: what it adds to tc-store (unit C0, 2026-10-19)

Unit C0 put tc-store behind tc-app's storage seam (`crates/tc-app/src/backend/store.rs`). No
PERFORMANCE row belongs to the adapter alone: it sits inside "Save one verse until 'Saved'"
(25 ms p95 with the fsync, Psalms in the whole Bible) and the open and close rows, which tc-store
owns. C0's own target, stated here and not a PERFORMANCE budget: **the adapter adds at most
0.5 ms p95 to a save, and nothing measurable to an open or a snapshot.** It is met: with no disk
in the way the seam adds 0.006 to 0.012 ms to a save's median and at most 0.022 ms to its p95
(M7); on the disk the two paths differ by less than the disk's own noise, in both directions.

## Machine and commit

- The owner's Linux PC: 12th Gen Intel Core i7-12700, 20 logical CPUs, 61 GB, a local SSD. The
  CPU has SHA instructions (`sha_ni`) and no AVX-512 (`avx512f` absent). Shared with up to three
  other agents building: the load average was 3.8 to 5.6 during the two runs below (12 during an
  earlier run, whose saves showed the same picture with larger p95s on both paths).
- Code at `b18a961` (branch `app-c0`) plus the bench of the commit that adds this file; release
  build.
- The project is tc-store's own whole-Bible bench project (617,086 changes, three devices for
  three years, real tN and TWL checks; `generated 617086 changes`, first open 13.4 s, checkpoint
  0.89 s, no Burrito problem):

  ```text
  cargo run --release -p tc-store --example storebench -j 6 -- setup target/store-adapter-bench
  cargo run --release -p tc-app --features test-hooks --example store_adapter_bench -j 6 -- target/store-adapter-bench
  STORE_ADAPTER_BENCH_SWAP=1 target/release/examples/store_adapter_bench target/store-adapter-bench
  ```

- Each row times the same work through tc-store directly and through the seam, alternating the
  two (A, B, A, B) so disk noise falls on both. The direct path uses `projects/bible`, the seam
  a copy of it, `projects/bible-seam`, so both can be open at once; the second run swaps the two
  folders (`STORE_ADAPTER_BENCH_SWAP=1`), so a difference that follows the folder is the disk's.
- The reference laptop's numbers are G2's: this machine has SHA instructions and a fast SSD.

## Numbers

Median and p95. Run 1: direct on `bible`, seam on `bible-seam`; run 2: swapped.

| Row | What | Run 1 tc-store | Run 1 seam | Run 2 tc-store | Run 2 seam |
|---|---|---:|---:|---:|---:|
| M1 | `snapshot()` per call, 100,000 calls a run, 20 runs | 17.3 / 17.5 ns | 43.6 / 44.1 ns | 17.0 / 17.2 ns | 43.1 / 43.4 ns |
| M2 | Save a Psalms verse until its reply, 200 saves | 14.06 / 23.71 ms | 14.24 / 22.54 ms | 9.42 / 14.97 ms | 9.41 / 15.70 ms |
| M3 | From the submit to the subscriber's `Committed`, 200 saves | 14.07 / 23.72 ms | 14.25 / 22.54 ms | 9.42 / 14.98 ms | 9.41 / 15.71 ms |
| M4 | `test/saveVerse` through `App::call` until its `Ack` (dispatch, reply cache, session, `wire::write::ack`, store), beside a direct save, 200 each | 17.53 / 28.71 ms | 17.74 / 29.67 ms | 9.08 / 15.33 ms | 9.35 / 16.46 ms |
| M5 | Open of the whole Bible, warm cache, 5 runs | 601.9 / 632.0 ms | 588.6 / 640.8 ms | 600.7 / 609.3 ms | 604.8 / 651.9 ms |
| M5 | When the seam's first forwarded progress report reaches the open's sink | — | 301.1 / 301.2 ms | — | 300.9 / 301.1 ms |
| M6 | Close after 10 saves, 5 runs | 576.2 / 694.6 ms | 563.7 / 657.0 ms | 456.7 / 536.5 ms | 451.2 / 546.9 ms |
| M7 | A save on tc-store's in-memory file system, 2,000 each: blocking on tc-store's reply / awaiting it on the runtime / through the seam | 0.116 / 0.223, 0.117 / 0.230 ms | 0.128 / 0.246 ms | 0.120 / 0.201, 0.120 / 0.200 ms | 0.126 / 0.199 ms |

One save through the seam costs, by tc-store's counters (R14): 0 files opened, 1 created, 2
flushes, 1 rename, the same as tc-store's own save (`the_adapter_adds_no_store_work` asserts the
equality on every counter that counts files).

## What the numbers say

- **A save.** M7 is the adapter's own cost, with no disk: 0.006 to 0.012 ms on the median and
  -0.001 to 0.022 ms on the p95, across the two runs (three earlier runs on this machine, which
  gave each path its own project as these do, showed 0.004 to 0.011 ms and 0.001 to 0.011 ms).
  That is the relay thread's hand-off: tc-store resolves the reply on its actor, the relay
  receives it, records the batch's origin and wakes the caller, one more thread wake-up than a
  caller blocked on tc-store's reply. On the disk (M2) the medians differ by -0.01 to +0.18 ms
  and the p95s by -1.2 to +0.7 ms, and the sign follows neither the path nor the folder: the
  fsync's own variation (p95 from 15 to 24 ms between two runs of the same code) is two orders of
  magnitude larger than what the adapter adds. Earlier runs at a load average of 12 showed p95
  differences of up to 2.6 ms in both directions for the same reason.
- **The event comes with the reply.** M3 is M2 plus 0.001 to 0.010 ms: the events thread passes
  a command's `Committed` on as soon as the relay has recorded its origin, which happens as the
  reply arrives.
- **The app's save path.** M4 adds dispatch, the session's reply cache, the scope check, the
  token parse and `wire::write::ack` to the seam: 0.21 to 0.27 ms on the median over a direct
  save in the same loop, under the 25 ms budget with the fsync by a wide margin on this machine.
  HTTP adds A2's loopback (5.2 ms p95 on the build container; `docs/evidence/loopback-2026-10-18.md`).
- **A snapshot.** 26 ns more than tc-store's (43 against 17 ns): one small allocation to put
  tc-store's snapshot behind the seam's `Snapshot`. A screen takes one snapshot per request, so
  this is nothing a person can see; the contract case `snapshot_clone_costs_nothing` shows it does
  no fold work at all.
- **Open and close.** Within noise of each other (M5: -13 to +4 ms on the median of a 600 ms
  open; M6: -12 to -6 ms on a 450 to 580 ms close). The seam's first progress report reaches the
  open's sink 300.9 to 301.1 ms after the open began: tc-store's reporter keeps the clock and
  tc-app adds no timer (AD8; `app_forwards_open_reports_unchanged` asserts it).

## Storage findings for the storage team

- `cargo clippy -p tc-app -- -D warnings` (tc-app without features, so tc-store without
  `test-hooks`) fails in tc-store: `clippy::default_constructed_unit_structs` at
  `crates/tc-store/src/store.rs:175` (`Arc::new(Hooks::default())`, where `Hooks` is a unit
  struct without the feature). The workspace's own clippy run builds tc-store with `test-hooks`
  (its tests turn it on), so the workspace step does not see it; CI's two steps without test
  hooks (`cargo clippy -p tc-app --lib`, `cargo clippy -p tc-server -p tc4 --lib --bins`) do,
  since C0 makes tc-app depend on tc-store. *(After the first verification: C0 changed that one
  line to `Arc::default()`, the same value with and without the feature, and the same in
  tc-app's `StoreBackend::on_disk`, which the tc-store error had hidden; the storage branch
  should take the same line.)*
- No other finding: the projection of records written through `ApplyUpgrade` gives the sample's
  Titus and Jonah back byte for byte (`chapter_view_equals_seed_usfm` on the store), and every
  contract case passes.

## After the first verification

The events thread now counts the submits in flight and the highest seq the relay settled
(`Origins`), one more lock of a mutex per submit. Two runs of the same bench on the fixed code
(load average 3.0 to 3.6; the second with the folders swapped): M7 through the seam 0.118 /
0.163 ms and 0.137 / 0.243 ms, against tc-store awaited on the runtime 0.111 / 0.156 ms and
0.128 / 0.291 ms, so the adapter adds 0.007 to 0.009 ms to the median, as before. M6's p95 on
the seam was 1.8 s and 1.5 s in these runs. Three more runs with each close printed (15
closes a path) showed one slow close on each path, 1.35 s on the seam and 1.59 s on tc-store's
own, and 450 to 790 ms for every other close on both; the code before the fix gave 590 and
717 ms in two runs. So the slow close is the disk's now and then, not the adapter's.

## After the second verification

Nothing on a save's path changed (`submit_journal`, the relay, the events thread and `Origins`
are as measured above), so M6 and M7 stand. Three things changed elsewhere:

- `StoreBackend::list` reads the projects folder itself. A project folder costs one read of
  `project.json`, where `Store::list` and the old entry check cost a stat and a read; a folder
  without a readable `project.json` costs up to three more stats (`metadata.json`,
  `ingredients/`, `.tc/`). The listing still costs the folder's entries, never a project's
  history.
- `App::start` starts one short-lived thread that waits for tc-store's start cleanup and
  records what it removed; the start does not wait for it.
- Two tests run submits on a four-thread runtime (15 rounds of 8 concurrent saves, and 20
  rounds of 8 saves racing a close); together they take under a second.
