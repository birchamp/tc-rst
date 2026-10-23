# First-launch seeding (unit A9), measured

What a first launch of `tc4 serve --bundle` costs here when it copies the whole bundled English
suite into the machine's resource store, what a repeat launch costs, and how fast Close stops a
seed. Unit A9 of `docs/APP-DESIGN.md` §10; the budgets are `docs/PERFORMANCE.md` rule 5 and
"Start-up".

- **Commit:** the program measured is `crates/tc-app/examples/seedbench.rs` at `cc9781a` (branch
  `p0-a9`), over the seeding of the commits before it. The numbers are in the commit that adds
  this file.
- **Machine:** the owner's Linux PC: Intel Core i7-12700 (12 cores, 20 threads), 61 GB, Ubuntu
  26.04, kernel 7.0.0-38. It **has** SHA instructions (`sha_ni`) and no AVX-512. `/home` is a
  Samsung 870 EVO SATA SSD, ext4. Other agents were building and testing on it during the runs
  (load average 8.7 to 11.3). It is not the reference laptop of `docs/PERFORMANCE.md` (4 cores,
  8 GB, SATA SSD, Windows with Defender), which has neither SHA instructions nor this CPU.
- **Build:** release (`cargo build --release -p tc-app --example seedbench --example
  bundle_store`).
- **Bundle:** 15 of the 16 installs, the OBS pictures absent (`fetched.lock` names no file for
  them: the packaging step downloads them), 3,153 content files, 268,660,351 bytes (256.2 MiB).
  Built from `bundle/en` by the example `bundle_store`, which installs with tc-resources' own
  installers (about 16 s).

## Method

`seedbench` runs the real `App` over the real disk, under `target/a9/bench/`, with
`AppDeps::production()` and no pacing. The bundled store is read only.

- **First launch** (5 runs): a fresh app data, one process per run so that the peak resident
  memory (`VmHWM`) is that of one run. Before each run `.audit/tools/evict-page-cache.py` drops
  the bundled files from the page cache (`posix_fadvise(DONTNEED)`), so they are read from the
  disk as at a person's first launch and not from the memory the build of the bundle left them
  in. Timed: `App::start`, which makes the plan, and the time from before `App::start` to the
  `Startup(Ready)` frame on the event stream. The `seed` report gives the counts.
- **Repeat launch** (20 runs): `App::start` on the seeded app data, until it returns. The state
  is `Ready` at once, so this is the cost the person waits for.
- **Close** (5 runs): a fresh app data, `App::quit` 500 ms into a seed, until it returns; then
  the store is opened read only and every install in it is verified. A run whose seed had ended
  before the close is an error.
- **Reads:** reading and checking each bundled install (`Installed::verify`), the first time
  from the disk and 4 more times from the cache. A seed reads and checks a whole install before
  it writes the first file of its copy, and asks to stop only between files, so this is the
  longest stretch a close waits for.

## Results

| What | Median | p95 (5 runs: the slowest) | Worst |
|---|---:|---:|---:|
| Plan, `App::start` of a first launch | 7.3 ms | 9.6 ms | 9.6 ms |
| First launch, `App::start` to `Ready` (5 runs: 14.1, 14.5, 14.8, 15.6, 16.2 s) | 14,806 ms | 16,220 ms | 16,220 ms |
| Repeat launch, `App::start` (20 runs) | 4.1 ms | 4.3 ms | 4.9 ms |
| `App::quit` 500 ms into a seed (5 runs: 0.6, 2.0, 4.6, 4.6, 39.1 ms) | 4.6 ms | 39.1 ms | 39.1 ms |
| Peak resident memory of a first launch (5 runs) | 97.3 MiB | 97.7 MiB | 97.7 MiB |

| Counts | First launch (all 5 runs) | Repeat launch (20 runs) |
|---|---:|---:|
| Content files opened by the plan | 0 | 0 |
| Content files written by the plan | 0 | 0 |
| Content files opened by the seed (read from the bundle) | 3,153 = the plan's files | 0 |
| Content files written by the seed | 3,153 = the plan's files | 0 |
| `Startup(Preparing)` frames on the stream | 133 to 148 | 0 |
| Seed job, report | one, `ok`, 15 added | none |

At the stop of the five closes, 4, 4, 4, 1 and 1 installs were whole, and each verified; the
copy being made was in `resources/.partial/`.

Reading and checking one install, cold from the disk and warm from the cache (each bundled
install; the lexicons are one file each):

| Install | Files | Bytes | Cold | Warm, median |
|---|---:|---:|---:|---:|
| `en_ust` v91 | 61 | 93,729,991 | 277 ms | 79 ms |
| `en_ult` v91 | 62 | 80,601,531 | 235 ms | 68 ms |
| `en_twl` v91 | 1,015 | 8,049,222 | 122 ms | 10 ms |
| `en_tn` v91 | 61 | 34,464,630 | 110 ms | 28 ms |
| `en_tw` v91 | 1,022 | 9,618,870 | 96 ms | 12 ms |
| `hbo_uhb` v3.0.0 | 41 | 24,944,917 | 85 ms | 20 ms |
| `en_ta` v91 | 736 | 1,235,837 | 67 ms | 5 ms |
| the other eight | 1 to 61 | 0.09 to 10.6 MB | 0.6 to 36 ms | 0.1 to 8.8 ms |
| all fifteen | 3,153 | 268,660,351 | 1,059 ms | 236 ms |

The window and Close, in Chromium against the real `tc4` with the bundle
(`crates/tc-ui/app/prepare-smoke.mjs`, debug `tc4`, Playwright 1.56.1, Chromium 141 build 1194),
on this machine:

- the five-install bundle (1,085 files, seed paced 4 ms a file): the window counts
  "Preparing resources: 39 of 1085" then "67 of 1085", keeps counting after a reload in Arabic
  (`lang="ar" dir="rtl"`), and Close (`{"quit":true}`) is answered `{"closed":true}` 3 ms later
  with 4 whole installs of 5; the relaunch plans the 1,022 files left, reaches Home, and every
  earlier copy is byte- and time-identical. `prepare-smoke.json` and three screenshots are in
  `docs/evidence/first-launch-seeding-2026-10-18/`.
- the whole suite (15 installs, 3,153 files, paced 2 ms): the same steps pass; Close answered in
  101 ms, with the stop inside the first copy; the relaunch plans 3,153 files, finishes and
  hands off to Home with 15 verified installs. This run is not committed.

## What the numbers say

- **A first launch is limited by the fsync of each file.** 3,153 files in 14.8 s is 4.7 ms a
  file. A probe on this disk (a Python loop, not committed) wrote 3,153 files of 2 KB: 11,746 ms
  with an fsync after each (3.73 ms a file) and 37 ms without. So about 11.7 s of the 14.8 s is
  `write_synced`'s `sync_all` (`crates/tc-resources/src/store.rs`). Reading and checking all the
  files from the disk is about 1.1 s (the table above). The other 2 s or so (hashing each file a
  second time for the receipt, the receipts, the directory syncs and the renames) were not timed
  apart. tC4 does not fsync each file (`docs/PERFORMANCE.md` "Start-up"), and its preparation on a
  Mac took 2,795 ms for 3,739 file writes; the rebuild takes about five times that here, on
  another machine, because it makes each file durable. The lever, if the Windows measurement shows it matters, is to make each install's
  files durable once before the rename instead of one by one; nothing is changed here, as
  PERFORMANCE.md asks for the measurement first.
- **The 60 s gate holds here by 3.7 times at the slowest run, and it is not the gate.** The gate is a signed
  installed Windows build with Defender on, from the shortcut click to a usable Home
  (`docs/PERFORMANCE.md`, Q24). Defender scans each file the app opens, and tC4's own record
  was 35,823 opens in about eight minutes on such a laptop. Nothing here shows what a
  3,153-file seed costs there: that is DELTA §3.6 item 6, to be measured by G1 and G2 on the
  Windows runner. The count tests, which are exact, do not depend on the machine.
- **The counts are the targets of PERFORMANCE.md.** A first launch opens each bundled file once
  and writes each once (3,153 and 3,153), and reads none of what it wrote. A repeat launch
  opens and writes no content file, plans in 4.1 ms (the receipts and check records of 15
  installs), starts no job and sends no frame. The expanded bundle here stands in for the
  archives; "each shipped archive once" waits for the packaging step (DELTA §3.6 item 3).
  After the plan began to compare the bundle's manifest with its folders (the bundle that lost
  an install, D12 in the A9 row) the same bench, run again with 3 first launches, 20 repeat
  launches and 3 closes, gave a plan of 9.2 ms, a repeat launch of 4.6 ms, a first launch of
  15.4 s (3 runs) and the same counts (0 and 0 on a repeat launch). The check reads the
  manifest and one receipt for each install the manifest lists, which are not content files;
  the machine was loaded (load average 4.6 to 7.6), so the 0.5 to 2 ms more is within the noise
  of the table above.
- **Close is quick because a seed asks to stop between files.** `App::quit` returned in 0.6 to
  39 ms, and the slowest is a stop that fell on a larger file. The longest a close can wait is
  the install being read and checked: 277 ms for `en_ust` from the disk here. On a disk or a
  scanner that makes each open slower this stretch grows with the install's file count (1,022
  for `en_tw`); the server's 5 s close cuts it, and that is safe because an install is staged
  and renamed whole (`docs/APP-DESIGN.md` §4.6). A poll between the reads would end it at the
  next file; it is left until a slow reader shows it matters.
- **Memory.** The peak, 97 MiB, is one whole install in memory (`en_ust`, 89.4 MiB unpacked) and
  the process around it. It is the same memory the download of that install needs
  (`docs/evidence/streaming-downloads-2026-10-17.md`), and a seed runs before any project is
  open.

## Reproduce

```sh
cargo build --release -p tc-app --example bundle_store --example seedbench -j 6
target/release/examples/bundle_store --from bundle/en --out target/a9/bundle-full
target/release/examples/seedbench --bundle target/a9/bundle-full --first 5 --repeat 20 \
  --close 5 --evict .audit/tools/evict-page-cache.py --out target/a9/seedbench.json
```

The window: `cargo build -p tc4 --features test-hooks`, build a bundle with `--only
en_tw,en_obs,en_obs-tn,en_obs-tq,en_obs-twl`, and run `node crates/tc-ui/app/prepare-smoke.mjs`
(header of the file). Delete `target/a9/` afterwards: the bundle is 268 MB and each run writes
as much again.
