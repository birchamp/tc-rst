# Unit P plan: Bible-scale performance of the storage core and the domain crates

Planner's plan for the builder. Worktree `/home/bot/tc-rst/.claude/worktrees/storage`, branch
`storage`. Commit this file with your first commit. Every shell command that runs cargo, rustc,
node or npx starts with `. ~/.tc-env && `; cargo always with `-j 6`. Never push.

The unit's promise, in one line: every budget in `docs/PERFORMANCE.md` owned by `tc-journal`,
`tc-store`, `tc-check`, `tc-align` and `tc-resources` is measured end to end on the whole-Bible
corpus, judged against the reference laptop, proved by operation counts, guarded by a CI job at
three times the budget, and shown to stay flat over three simulated years. A miss is profiled
and fixed at its cause, never hidden by a loosened budget. Nothing a translator saved is lost,
hidden or silently rewritten by any fix.

---

## 0. Facts gathered while planning (read this before anything else)

### 0.1 State of the branches

- `storage` is at `19e7fd2` (S2's last review fixes), 32 commits ahead of `origin/storage`
  (`0d7d4f7`, the WIP the cloud session pushed). The worktree was clean at planning time.
  Main's log (`.audit/rebuild.tsv` on `claude/focused-ride-5uzj7p`, 2026-10-19T03:09Z) says S2's
  third fix round had no check after it and that a verify loop (`verify-loop.js`) is to run on
  `storage` before S2 merges. Before you start, run `git log --oneline -5` and `git status`: if
  commits newer than `19e7fd2` appeared (a verify loop's fixes), build on them; if the tree is
  dirty, a stopped builder left work (refs under `refs/wip/*`, never delete them): read the diff,
  keep what is sound.
- Main (`origin/claude/focused-ride-5uzj7p`, `c1bfa78`) is 190 commits ahead of `storage`. It
  holds what P needs: A9's first-launch seeding in `tc-resources` (`seed_plan`, `seed`,
  `SeedWatch`, `content_files_opened`/`content_files_written`), `bundle/en/README.md` with the
  tW/TWL ruling text, `.audit/tools/evict-page-cache.py`, `docs/TC4-DELTA-2026-10-19.md`,
  `docs/TC4-ISSUES.md`, the newest `APP-DESIGN.md` and the 1 s window row in `PERFORMANCE.md`.
  The lexicon v1 (`tc-lexicon` JSON, `open_lexicon`, `Lexicons`) and the Windows G0 evidence
  (`docs/evidence/storage-fold-windows-g0-2026-10-17.md`) are already on `storage`.
- A dry run (`git merge-tree --write-tree HEAD origin/claude/focused-ride-5uzj7p`) gives one
  conflict, in `Cargo.toml` (two profile blocks; keep both, see step 0). `PERFORMANCE.md`,
  `STORAGE.md`, `Cargo.lock` and `crates/tc-journal/tests/properties.rs` merge by themselves.

### 0.2 This machine is not the task text's "VM", and it has SHA instructions

- Intel Core i7-12700, 20 logical CPUs: 0-15 are the 8 performance cores (two threads each),
  16-19 the 4 efficient cores (`lscpu --extended`). `sha_ni` **yes**, `avx2` yes, `avx512f`
  **no**. 61 GiB. The checkout is on a Samsung 870 EVO SATA SSD, ext4. Ubuntu 26.04, Linux 7.0.
  `docs/evidence/linux-machine-2026-10-18.md` (on main) records it.
- The task text says "a CPU without SHA instructions (this VM)". That is false here: say so in
  the evidence. A verdict for a CPU without SHA instructions comes from the laptop ratio of
  step 2 (the laptop has no SHA and no AVX-512), and for hashing-heavy rows from the foldbench
  SHA row, never from this machine's raw figure.
- Up to four agents build and test at once. Load average was 1.8 to 8.4 at planning time. Record
  `uptime` with every round; rerun a miss once before calling it a miss.
- `perf` is installed but `kernel.perf_event_paranoid` is 4: unprivileged sampling is refused,
  and changing it is a system setting you must not touch. Profile with temporary timers
  (`Instant` around phases, removed before commit) and the operation counters, as S2 did.
- `/proc/sys/vm/drop_caches` needs root. For a cold disk cache use
  `posix_fadvise(DONTNEED)` on the files (`rustix::fs::fadvise`, or
  `python3 -I .audit/tools/evict-page-cache.py <dir>` after the merge), after `sync`.

### 0.3 The reference laptop and its measured rows

`docs/evidence/storage-fold-windows-g0-2026-10-17.md` (+ `rows-warm.tsv`, `everything.txt`, the
`app-N.txt` files): Dell Latitude 7490, i5-8350U, 4 cores / 8 threads, 7.9 GiB, WD Blue SATA SSD,
Windows 10 19045, Defender on, no SHA, no AVX-512. foldbench of `storage` at **`58b6874`**, 4
reading threads, rayon's default pool (8 threads), 5 rounds, one open per process. Medians
(p95):

| foldbench row (laptop) | median | p95 |
|---|---:|---:|
| (a) open with the fold cache, warm, one per process | 1,855 ms | 1,987 |
| (c) first open without a cache, warm, one per process | 6,288 ms | 6,543 |
| Read, verify and prepare (`run`) | 6,172 ms | 6,488 |
| SHA-256 of every file once (`run`) | 813 ms | 892 |
| Full fold of the prepared changes (`run`) | 1,336 ms | 1,357 |
| Cold open with the fold cache (`run`) | 1,902 ms | 2,020 |
| Cold open without a cache (`run`) | 6,820 ms | 7,231 |
| Psalms projection (`app` / psalms-only) | 342-401 ms | 384-490 |
| Psalms USFM alone (one thread) | 35.9-47.0 ms | 47-51 |
| One Psalms chapter's USFM | 0.25-0.34 ms | 0.76-0.90 |
| Conflict list and counts from scratch | 19.7-20.5 ms | 22.5-24.7 |
| Snapshot (clone of the fold) | 0.10 ms | 0.24-0.34 |
| Working set with the fold open / peak / peak private | 192-210 / 269-273 / 282-283 MiB | |
| First open without a cache, peak private | 368-369 MiB | |

No disk-flush figure exists for the laptop. A save is two flushes; no foldbench row scales it.
The owner's quiet Ice Lake run with SHA (`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md`,
`docs/evidence/journal-head-to-head-2026-10-16.md`): first open without a cache 3,783 ms, Psalms
projection 269 ms, SHA-256 of every file 158 ms, full fold 1,126 ms, conflict list 16.7 ms, Psalms
USFM 15.8 ms. Quote these beside each row that has one.

### 0.4 The budgets P owns (PERFORMANCE.md after the owner's rulings of 2026-10-16 and 2026-10-18)

- tc-journal: open a book 150 ms (first chapter 50 ms); fold update and the chapter's projection
  after a save 2 ms; move to another chapter 16 ms; project-wide counts and conflict list after a
  save 1 ms incremental, 100 ms from scratch.
- tc-store: save a verse / record a decision / save an alignment until durable 25 ms incl. fsync;
  the same 25 ms p95 **while** a materialize, a checkpoint, a merge commit or an upgrade write
  runs, a whole-project materialize included; cold open with a usable fold cache 3 s; cold open
  without one, or the first open after a restart with a cold disk cache, 10 s; progress within
  300 ms; materialize one book off the actor 1 s p95 on the laptop, no indicator, with open a
  book / open Check / chapter budgets holding meanwhile, and a count test; whole-project
  materialize as background work with progress, saves continuing, one book at a time; close 3 s on
  the laptop; checkpoint 500 ms (review it, never change it: the owner decides); project store
  memory 400 MiB steady, 450 MiB peak, private bytes read a minute after the last action, never at
  exit; growth at most 300 B per added change with one heap, under 350 B with the default Linux
  heap, at twice today's history (1.2 million changes).
- tc-check: Check screen open (derive, map versification, place decisions, status) for Psalms tN
  and tW 250 ms first, 20 ms from the cache (the cache is tc-app's `CheckListCache`, B5a: measure
  the work left after a hit); rail filter/sort/grouping 16 ms; record a decision 25 ms with
  progress counts in 1 ms; re-assess after a verse edit 1 ms (that verse's checks only); count
  what an upgrade or gateway change invalidates, whole Bible, 215,767 checks, 2 s; plan the
  carry-over 5 s whole Bible in the background, 300 ms per book.
- tc-align: open the aligner on the longest Psalms verse 30 ms; every edit inside that 30 ms;
  suggestions for one verse 50 ms with a model trained on the NT + GEN + PSA alignments of ULT v91
  (training and the update after a save in the background); reflow 2 ms.
- tc-resources: load one book's resources (original language, ULT, UST, tN, TWL, linked tW
  articles) 300 ms first time, 60 MiB per open book; load a lexicon (en_uhl, 8,674 entries)
  100 ms, lookups under 1 ms; first launch under 60 s (hard gate, Q24; the gate itself is the
  signed Windows build, `tc4`'s; P measures the seeding part and asserts the counts); repeat
  launch reads and writes no resource content file.
- Rule 5: at least 20 runs (5 for actions over 1 s), median and p95, machine with its SHA and
  AVX-512 bits, the commit. CI fails a budget only above three times its value.
- Not P's: daily sync, USB export, tC4 import (units X, I), the whole app's 1 GiB (tc-app and the
  shell), lists and typing (tc-ui). The report prints them as "not built here (unit X/I/app)".

### 0.5 What exists to build on

- `crates/tc-store/examples/storebench.rs` (S2): `setup` writes the corpus through the store
  (open, checkpoint); cases `open warm|nocache|cold`, `run`, `saves`, `materialize`, `whole`,
  `checkpoint`, `close`, `outside`, `memory`, `repack`; helpers `save_one`, `decide_one`,
  `verses_of`, `decisions_of`, `decision_target`, `pins_value`, `ProgressLog`, `cost`,
  `private_mib`, `status_mib`, `machine_line`. Its `cold` case writes `drop_caches` and gives up
  without root. **It is built with tc-store's `test-hooks`** (tc-store dev-depends on itself with
  that feature, and examples link dev-dependencies), so S2's numbers include the hold points'
  mutex checks.
- `crates/tc-corpus/examples/foldbench.rs`: `save psalms|scaled N|bible|bible-share S DIR`,
  `run DIR [threads]`, `app DIR [threads]`; env `FOLDBENCH_RUNS`, `FOLDBENCH_FIRST_ONLY`,
  `FOLDBENCH_CACHED_ONLY`, `FOLDBENCH_PSALMS_ONLY`. The Windows runner
  `spikes/foldbench-windows/run-foldbench.ps1` shows which invocation made each laptop row
  (lines 235, 253, 274, 304, 476).
- Corpus: `tc_corpus::Spec::{psalms, scaled(seed, 10), whole_bible}`, `.with_history(days,
  share)`; `Corpus::generate`. `Spec::whole_bible(1)` gives 617,086 changes at both 58b6874 and
  now (S2 evidence). `history::Sim::simulate` (history.rs 436-520) works day by day, seals in
  background threads, and on disk leaves one pack per device per finished month (as if
  consolidated) and a pack per working day in the last month.
- Store API (`tc_store`): `Store::start(StoreConfig { app_data, projects, vfs, platform, clock,
  threads, watch_debounce })`, `store.open(folder, &OpenOptions{progress,..})`,
  `ProjectHandle::{snapshot, mint, submit, leave(Leave::Book|Mode), checkpoint(reason),
  read_book(book) -> Pending<BookRead>, counters, subscribe, focus, wait_idle, close}`,
  `StoreCounters` (files_opened, files_created, bytes_read/written, fsyncs, projected,
  derived_read/written, bytes_hashed, writer_yield_ns, queue_wait_ns/_max_ns, git_*...),
  `ManualClock::at(ms)` / `.advance(ms)`, `Snapshot::training_input(book, None)`.
- Fold API (`tc_journal::fold::FoldState`): `clone` (O(1)), `own_counters`, `counters()` /
  `Counters::since` (changes_indexed, keys_updated, keys_recomputed, changes_visited,
  values_decoded, chapters_cohered, verses_cohered, attached_placed, conflicts_built,
  records_projected, values_read, records_listed...), `add`, `book_usfm`, `chapter_usfm`,
  `chapter_keys`, `chapters_of`, `projected_books`, `conflict_counts`, `conflicts`,
  `decisions(tool, Collection::Book(b)) -> Vec<ProjectedDecision>` (per-book key index),
  `check_texts`, `heads`, `projected`. `tc_journal::{Change, ChangeBody, DeviceId::local,
  BatchId::from_bytes, BatchRef::new}` build a synthetic change.
- Count tests that exist: `crates/tc-journal/tests/operation_counts.rs` (verse save + its
  chapter, verse save per anomaly, decision, alignment, resolution, book render, book file
  listing); `crates/tc-store/tests/store/opcount.rs` (open after no change, each pack once, a
  save reads no pack and writes one file with two flushes, open after one save, scope entry,
  racy files, fold cache kept); S2's `groups.rs` (`a_books_materialize_touches_only_that_books_files`,
  `a_book_change_marks_only_that_books_files`), `checkpoint.rs`
  (`a_checkpoint_after_one_save_writes_only_changed_trees`), the outside-edit and focus counts,
  `writer.rs` (`a_save_is_served_between_the_steps_of_a_seal`), `progress.rs`
  (`open_shows_determinate_progress_by_300_ms`). Store corpora on `FaultFs`:
  `tests/store/corpus.rs` `psalms()` / `psalms_x10()`.
- Domain tests that time things: `tc-check/tests/performance.rs` (derive Psalms lists, 3x
  budget in release), `tc-align/tests/psalms.rs` (open/edits/reflow of PSA 42:4, the verse with
  the most words in `fixtures/scale/psa/en_ult/19-PSA.usfm`), `tc-align/tests/bench.rs`
  (suggestion ramp-up, ignored), `tc-resources/tests/lexicon_performance.rs` (load and lookup,
  with count tests), main's `tc-resources/tests/seed.rs` (count tests on three fixture exports).

---

## Step 0. Baseline, merge main, gates

1. `git status`, `git log --oneline -8`. Run the three gates once on the branch as it is and keep
   the test count (`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -j 6 --
   -D warnings`; `cargo test --workspace -j 6`). If a test fails here, it is S2's: fix it at its
   root in its own commit before anything else (STORAGE.md first if the spec is what is wrong).
2. `git fetch origin`, then `git merge origin/claude/focused-ride-5uzj7p`. Resolve `Cargo.toml`
   by keeping both blocks: storage's `[profile.dev.package.libgit2-sys]` and
   `[profile.dev.package.libz-sys]` and main's `png`, `fdeflate`, `flate2`, `miniz_oxide`,
   `crc32fast`, `simd-adler32` blocks, each header followed by `opt-level = 3`.
3. Re-read the auto-merged texts where both sides wrote: `docs/STORAGE.md` §5.0 step 3 (held-file
   retry; main's `732755d`, `a33aef4` and S2's text) and §16.7; `docs/PERFORMANCE.md` must hold
   main's "Window on screen" row **and** storage's "Rows of the project store that are open"
   section. Remove any duplicated or contradicting sentence (one statement per rule).
4. Gates after the merge, with main's extra ones: the three above, plus
   `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings` (also
   `--features gallery`, `--features app`), `cargo clippy -p tc-api --target
   wasm32-unknown-unknown --all-targets -- -D warnings`, `cargo clippy -p tc-server -p tc4 --lib
   --bins -- -D warnings`, `cargo clippy -p tc-app --lib -- -D warnings`. Main's parity tests
   run in `cargo test`; Playwright 1.56.1 and its Chromium are installed here and `~/.tc-env`
   sets `PLAYWRIGHT_HOST_PLATFORM_OVERRIDE`.
5. Commit the merge ("Merge main into storage for unit P: ..."). Then commit this plan file (or
   commit it with the merge).

---

## Step 1. The report binary's frame, and the CI job

### 1.1 Where it lives, and why there

`crates/tc-corpus/examples/perfreport/` (a directory example: `main.rs` plus modules). Reason: an
example of `tc-store` links tc-store's `test-hooks` (the self dev-dependency), so storebench's
numbers include hold-point checks; an example of `tc-corpus` that dev-depends on `tc-store`
without features builds the release configuration when run as
`cargo run --release -p tc-corpus --example perfreport`. The task allows tc-corpus or tc-store.
Keep `storebench` as it is (S2's evidence names its commands).

- `crates/tc-corpus/Cargo.toml` `[dev-dependencies]`: add `tc-store`, `tc-check`, `tc-align`,
  `tc-resources`, `tc-helps`, `tc-usfm` (already a dep), `rayon` (there), `tempfile`, `zip`
  (workspace), `sysinfo` (workspace), and on unix `rustix` (workspace, `fs`). Dev-dependency
  cycles (tc-store dev-depends on tc-corpus) are allowed; tc-journal already has them.
- `crates/tc-store/src/lib.rs`: add
  `pub const TEST_HOOKS: bool = cfg!(feature = "test-hooks");` and print it in the machine line.
  A report run with `TEST_HOOKS == true` (a `cargo build --workspace` that unified the feature)
  says so in every row and refuses `--strict`.
- Shared helpers: move storebench's case helpers that perfreport also needs into
  `crates/tc-store/examples/common/mod.rs`? No: perfreport is in another crate. Copy what you
  need into `perfreport/store_rows.rs`, and delete nothing from storebench.

### 1.2 Modules and types

```rust
// perfreport/main.rs
//   perfreport setup DIR                 corpus project + bundled resource store under DIR
//   perfreport run DIR [--ratios FILE] [--only a,b] [--rounds N] [--strict]
//   perfreport ci DIR [--tolerance 3]    setup if missing, run every CI row, exit 1 on a fail
//   perfreport r9 DIR                    saves and reads during background work (step 5)
//   perfreport memory DIR                M2 (step 7); perfreport growth DIR: M3 (step 7)
//   perfreport years DIR [--days 750] [--open-every 1] [--seals-per-day 4]   (step 8)
//   perfreport child <case> DIR ...      one measurement in a fresh process; prints ROW lines
//   perfreport flush DIR                 1 KiB atomic_write + SyncDir::Now, 200 runs (disk ratio)

// perfreport/table.rs
pub enum Class { Sha, OpenCache, OpenNoCache, Multi, Single, Disk, Memory, Timer }
pub enum Limit { Ms(f64), MiB(f64), BytesPerChange(f64), Count(u64), Info }
pub struct Budget { pub limit: Limit, pub on: Stat }        // Stat: P95 | Median | Max | Value
pub struct Row {
    pub id: &'static str, pub area: &'static str, pub action: &'static str,
    pub scale: &'static str, pub owner: &'static str, pub budget: Budget,
    pub class: Class, pub samples: Vec<f64>, pub note: String,
}
pub enum Verdict { Pass, Fail, NotShown(String), Info }
pub fn verdict_ci(row: &Row, tolerance: f64) -> Verdict;     // p95 (or the budget's stat) <= tolerance x limit
pub fn laptop_estimate(row: &Row, ratios: &Ratios) -> Option<f64>; // None for Disk until a laptop flush ratio exists
pub fn verdict_laptop(row: &Row, ratios: &Ratios) -> Verdict;
pub fn print_table(rows: &[Row], mode: Mode);               // stdout, one table
pub fn write_tsv(path: &Path, rows: &[Row]); pub fn write_json(path: &Path, rows: &[Row]);

// perfreport/machine.rs
pub fn machine_line() -> String
// CPU brand and logical CPUs (sysinfo), the affinity mask (Linux: Cpus_allowed_list in
// /proc/self/status), is_x86_feature_detected! for sha, avx2, avx512f, avx512bw, avx512vl (under
// #[cfg(target_arch = "x86_64")]), RAM, OS, load (Linux /proc/loadavg, else "unavailable"),
// RAYON_NUM_THREADS, MALLOC_ARENA_MAX, tc_store::TEST_HOOKS, the commit (`git rev-parse HEAD`).

// perfreport/ratios.rs
pub struct Ratios { pub sha: f64, pub open_cache: f64, pub open_nocache: f64, pub multi: f64,
                    pub single: f64, pub memory: f64, pub disk: Option<f64>, pub source: String }
```

- Table columns: id | area | action | scale | budget | runs | median | p95 | class | laptop p95
  (est.) | verdict here | verdict laptop | Ice Lake (LOCAL-BENCH) | note. Times in ms with three
  significant figures; memory in MiB.
- `ci` mode judges raw p95 against `tolerance x budget` (no ratios: CI machines vary) and exits
  1 when any row fails. `run` judges against the laptop estimate and exits 0 unless `--strict`.
- Child protocol: the parent runs `current_exe() child <case> ...` with its environment, reads
  lines `ROW\t<json>` (serde_json::Value), and passes everything else through to its log.
- Memory readings: Linux `Private_Clean + Private_Dirty` of `/proc/self/smaps_rollup`, `RssAnon`,
  `VmHWM`; elsewhere print `MARK <name>` and sleep 3 s so the Windows runner (step 12) samples
  the private working set from outside; the row then says "measured from outside".
- Cold disk cache (Linux): `rustix::fs::sync()`, then `fadvise(fd, 0, None, Advice::DontNeed)` on
  every file under the project folder (packs, pending, `.tc/index.bin`, derived files, `.git`)
  and on the bundled store for the seeding rows. Windows: "cold" needs a restart; print
  "not measured (needs a restart)".
- No network anywhere. Everything comes from `bundle/en`, `fixtures/` and the generated corpus.

### 1.3 The CI job

`.github/workflows/ci.yml`, new job:

```yaml
  # docs/PERFORMANCE.md rule 5: the release benchmarks of every budget the storage core and the
  # domain crates own, on the whole-Bible corpus; a row fails only above three times its budget.
  perf:
    runs-on: ubuntu-latest
    needs: rust
    timeout-minutes: 60
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo build --release -p tc-corpus --example perfreport
      - run: target/release/examples/perfreport ci "$RUNNER_TEMP/perf" --tolerance 3
```

`ci` runs every row except `years` (hours) and except the cold-cache open where eviction fails;
it runs `growth` (adds about 5 minutes and 2 GB) unless `PERF_SKIP=growth`. Print the machine
line first. Name the job in PERFORMANCE.md rule 5 ("the `perf` job of `.github/workflows/ci.yml`
runs `perfreport ci`").

### 1.4 Tests and commit

- Unit tests inside the example are not run by `cargo test`; put the pure parts (percentile,
  verdicts, ratio lookup) in `perfreport/table.rs` with `#[cfg(test)]` and run them with
  `cargo test -p tc-corpus --example perfreport`. Names: `p95_of_twenty_is_the_nineteenth`,
  `a_row_fails_ci_only_above_three_times_its_budget`, `a_disk_row_is_not_shown_without_a_laptop_flush_ratio`.
- Commit: "perfreport: the report frame, the machine line and the CI job".

---

## Step 2. This machine's speed ratio to the laptop

Run the laptop's exact foldbench, at `58b6874`, here, and divide.

1. Build 58b6874's foldbench without a second target directory:
   ```sh
   cd /home/bot/tc-rst/.claude/worktrees/storage
   mkdir -p target/ref-58b6874 target/perf/bin
   git archive 58b6874 | tar -x -C target/ref-58b6874
   . ~/.tc-env && cargo build --release -j 6 --manifest-path target/ref-58b6874/Cargo.toml \
       -p tc-corpus --example foldbench --target-dir target
   cp target/release/examples/foldbench target/perf/bin/foldbench-58b6874
   ```
   (The build writes into the worktree's own `target/`; copy the binary at once, because a
   later build of HEAD's foldbench overwrites `target/release/examples/foldbench`.)
2. Emulate the laptop's shape: `taskset -c 0-7` (4 performance cores, 8 threads) and
   `RAYON_NUM_THREADS=8`, 4 reading threads. Use the same for every perfreport run (step 3 on),
   so the ratio maps like to like.
3. `foldbench-58b6874 save bible target/perf/fold-58b6874` (617,086 changes expected; say if
   not). Then 5 rounds, each: `run DIR 4` with `FOLDBENCH_FIRST_ONLY=1 FOLDBENCH_RUNS=1` after
   removing `DIR/index.bin` (row c), `run DIR 4` with `FOLDBENCH_CACHED_ONLY=1
   FOLDBENCH_RUNS=1` (row a), `run DIR 4` with `FOLDBENCH_PSALMS_ONLY=1 FOLDBENCH_RUNS=1`, and
   `app DIR 4`; plus one `run DIR 4` with defaults ("every part"). Interleave the rounds; keep
   every log under `target/perf/ratio/`.
4. Ratios (laptop median / here median, per row; then per class, the largest of its rows):
   `sha` = SHA-256 of every file once; `open_cache` = (a) and "Cold open with the fold cache";
   `open_nocache` = (c), "Cold open without a cache", "Read, verify and prepare"; `multi` =
   Psalms projection; `single` = Psalms USFM alone and the conflict list from scratch (and one
   chapter's USFM as a check); `memory` = laptop working set / here RSS of the app's case with
   `MALLOC_ARENA_MAX=1` (expect 0.97-1.07, the budget review's figure); `disk` = none.
5. Write `docs/evidence/storage-scale-<date>/laptop-ratios.json` (the rows, both figures, the
   ratios, both machines, the commit, the load) and keep the raw logs out of git except a short
   `ratio-rows.tsv`. A small parser is fine as `.audit/tools/laptop-ratios.py` (stdlib only, run
   with `python3 -I`); perfreport reads the JSON with `--ratios`.
6. Delete `target/ref-58b6874` and `target/perf/fold-58b6874` when the evidence is written.
7. Commit: "Measure this machine's speed against the reference laptop with the laptop's own
   foldbench".

---

## Step 3. The store's rows end to end, first full round

`perfreport/store_rows.rs`. `setup` = storebench's `setup` (corpus `Spec::whole_bible(1)` into
`DIR/projects/bible`, device 0's `.tc/device.json`, first open, `checkpoint(AfterImport)`,
close, `tc_burrito::validate_dir`), plus the bundled resource store (step 11.1) under
`DIR/bundle`. Every timed row below runs with `taskset -c 0-7`, `RAYON_NUM_THREADS=8`,
`StoreConfig.threads = 4`, the file watcher on (1 s debounce), and repack on its real thresholds
(this build has no `repack_after`).

| id | What it times | Runs | Class |
|---|---|---:|---|
| `open.cache` | child: open with `.tc/index.bin`, until usable; also first progress report time and the background-done time | 20 | OpenCache |
| `open.nocache` | child: index.bin removed | 5 if over 1 s, else 20 | OpenNoCache |
| `open.cold` | child: index.bin kept, page cache evicted (step 1.2) | 5 | OpenNoCache, and disk |
| `open.progress` | first progress report after the call (budget 300 ms; Timer class, no ratio; pass if under 330 ms) | from the opens | Timer |
| `book.open` | `handle.read_book(BookId::Psalms).wait()` wall time | 20 | Single |
| `book.first_chapter` | `snapshot.fold().chapter_usfm(Psalms, 1)` | 20 | Single |
| `chapter.move` | `chapter_keys` + `chapter_usfm` of a random Psalms chapter | 200 | Single |
| `save.verse` | `SaveVerse` until the durable reply (Psalms, words never saved before) | 200 | Disk |
| `save.fold_step` | `snapshot.fold().clone()`, `own_counters`, `add` of one synthetic verse change (parents = `shown_heads`), then that chapter's `chapter_usfm`; budget 2 ms | 200 | Single |
| `save.decision` | `SaveDecision` (bookmark toggle, as `decide_one`) | 200 | Disk |
| `save.alignment` | `SaveAlignment` of a Psalms alignment with `done` toggled (shown token of the alignment and the verse) | 200 | Disk |
| `counts.incremental` | `conflict_counts()` read after a save's fold step | 200 | Single |
| `conflicts.scratch` | walk `conflicts()` and build each row's shown data (as foldbench's row) | 20 | Single |
| `materialize.book` | storebench `materialize`: Psalms after a verse save, after a decision; Jude | 20 each | Multi + Disk (report both parts, step 3.2) |
| `materialize.whole` | storebench `whole`: pins change, `leave(Mode)`; the time, the first progress report, events | 5 | Info (no budget) + Timer |
| `checkpoint.dirty` / `.settled` | storebench `checkpoint` | 20 each | Multi + Disk |
| `close` | storebench `close` | 5 | Multi + Disk |
| `flush` | 1 KiB `atomic_write` + `SyncDir::Now` in `DIR/flush/` | 200 | Disk (the ratio input when the laptop runs it) |

### 3.1 Rules for every row

- Print each case's `StoreCounters` delta (storebench's `cost` line) and, for fold rows, the
  `tc_journal::fold::Counters` delta: the evidence's "What each action did" table comes from
  them.
- Verse and decision targets must change a value every time (storebench appends the run id).
- `save.fold_step`'s synthetic change: `Change::new(ChangeBody { v: tc_journal::FORMAT_MINOR (or
  the constant tc-store uses), record: <verse with new words>, parents: fold.shown_heads(&key,
  None), device: DeviceId::local(random), seq: DeviceSeq(1), at: WallTime(now), batch:
  BatchRef::new(BatchId::from_bytes(random), 1), cause: Cause::Edit })`. The fork is dropped.
- The snapshot taken for reads is the published one (`handle.snapshot()`); never time against a
  fold the actor is still writing.

### 3.2 Splitting mixed rows for the laptop

A materialize, checkpoint or close is CPU on several threads plus flushes. Report: wall time;
`fsyncs` from the counters; this machine's median flush from `flush`; the CPU part = wall minus
fsyncs x flush median; laptop estimate = CPU part x `multi` + fsyncs x laptop flush. Until the
laptop's `flush` row exists (step 12), the verdict is "not shown on the laptop" with the CPU part's
estimate printed; never call such a row met.

### 3.3 First round

Run `perfreport run DIR --ratios <file>` once at a quiet moment (`uptime` under 4 if possible).
Keep the table in `target/perf/round-1.txt`. Every row whose laptop estimate is over the budget
or over half of it goes to step 6's list (rule: profile everything within half of its budget).

Commit: "perfreport: the store's rows end to end on the whole Bible".

---

## Step 4. Operation counts: audit and add what is missing

Each new test runs the action on the Psalms corpus and on `Spec::scaled(1, 10)` and fails if a
count grows (PERFORMANCE rule 2). Add a row to STORAGE.md §16.5a for each, and a §16.7 row naming
the mutation each catches.

### 4.1 tc-journal (`crates/tc-journal/tests/operation_counts.rs`)

- `opening_a_book_costs_that_book`: `fork(state)`, `book_usfm(Psalms)`; `records_projected`,
  `values_decoded`, `values_read`, `records_listed` no larger on the larger corpus
  (`assert_no_growth`), and `records_projected` at most Psalms' verse and front records.
- `moving_to_a_chapter_costs_that_chapter`: `chapter_keys` + `chapter_usfm` of
  `common_chapter()`; counts no larger; `records_projected` equals the chapter's verse records.
- `the_conflict_counts_after_a_save_cost_nothing_per_conflict`: a verse save in
  `common_chapter()` on Psalms, on the larger corpus, and on a Psalms fold with 2,000 extra
  conflicts on Genesis (built like `a_verse_save_costs_nothing_per_anomaly`, two devices editing
  the same 2,000 verses); `conflicts_built` is 0 on all three and every count of the step equal;
  `conflict_counts()` before and after differ only by what the save changed.
- `a_verse_save_costs_nothing_per_year_of_history`: the same verse save on
  `Spec::psalms(1).with_history(250, 1.0 / 3.0)` and on `Spec::psalms(1)` (three times the
  history, same books and devices): no count grows. This is the history axis alone, which the
  ten-times corpus mixes with books and devices.

### 4.2 tc-store (`crates/tc-store/tests/store/opcount.rs`)

Model each on `a_save_reads_no_pack_and_writes_a_bounded_number_of_files` (open on a copy of
`psalms()` / `psalms_x10()`, `fs.clear_log()`, act, read `fs.log()`, `fs.opened()`,
`fs.bytes_read()`, the counters):

- `a_decision_save_reads_no_pack_and_writes_one_pending_file`: one `SaveDecision`; no file
  opened, under 64 KiB read, one file created, 2 fsyncs, same write points on both.
- `an_alignment_save_reads_no_pack_and_writes_one_pending_file`: one `SaveAlignment`; same
  assertions.
- `a_resolution_reads_no_pack_and_writes_one_pending_file`: make a conflict on a Psalms verse
  (two teammate packs put into the folder between opens, as §16.5a allows), `Resolve` with a
  candidate; same assertions.
- `reading_a_book_reads_only_its_values`: `read_book(Psalms)` after the open's background work
  (`wait_idle`); no file opened that the open had not; bytes read at most the Psalms values'
  bytes (the sum of the value lengths of Psalms' projected changes), and equal on both corpora.
- Leave the draft writer alone unless `drafts.rs` lacks a test that a draft write creates one
  file and flushes it without the actor; if it lacks one, add `a_draft_write_touches_one_file`.

### 4.3 Coverage table for the evidence

List every interactive action of PERFORMANCE.md's table owned by tc-journal and tc-store with the
test that counts it (old or new). Snapshot and token minting are an `Arc` clone and an HMAC of a
few hundred bytes: say so, with the timings, instead of a count test.

Commit: "Operation counts for opening a book, moving chapter, conflict counts, history alone, and
the store's decision, alignment, resolution and book reads".

---

## Step 5. Saves and reads while background work runs (R9), and tC4's J8

`perfreport/r9.rs`. Each case runs a job and, on a second thread, a save every 20 ms, alternating
verse save, decision, alignment save on Psalms records the job does not touch (the job's own
records for the upgrade case are excluded by construction), until the job ends. Report per
case: job time; saves n, p50, p95, max; `queue_wait_max_ns`; `writer_yield_ns`; and a verdict
against 25 ms. Disk class: a laptop verdict waits for step 12; here a miss is a miss.

| Case | Job |
|---|---|
| `r9.materialize` | a Psalms save, then `leave(Book(Psalms))`, 20 times |
| `r9.whole` | `SetPins` + `leave(Mode)` (storebench `whole`) |
| `r9.checkpoint` | Psalms save + `checkpoint(LeavingTranslate)`, 20 times |
| `r9.repack` | storebench `repack` cannot force a repack in this build: drive loose objects past §9.8's threshold with checkpoints after Psalms saves until `git_repacks` moves, then measure saves while it runs |
| `r9.consolidation` | the month-end consolidation, below |
| `r9.upgrade` | a 150 MB-class `ApplyUpgrade`, below |
| `r9.merge_proxy` | no merge commit exists before unit X. Proxy: the first checkpoint after a whole-project materialize (334 changed files: the biggest tree a commit writes). Label it a proxy; write in the evidence and in STORAGE §18 that unit X must add the real case to perfreport |
| `r9.j8` | tC4's J8 (AD34, Q-5): 400 decisions at one per 300 ms with `leave(Mode)` + `checkpoint(LeavingTranslate)` every 10 decisions, against the same session with no checkpoints. Report save p95 overall and for saves that overlapped a checkpoint, the commits made, the loose objects added |

Reads during background work (the materialize row's condition): during `r9.materialize` and
`r9.whole`, a third thread times `read_book(Psalms)`, `chapter_usfm`, and the tc-check session
open of step 9 every 100 ms. Rows `read.book_during_materialize` etc., budgets 150 / 16 / 250 ms,
classes as in steps 3 and 9.

### 5.1 The consolidation case

Store started with `clock: Arc<ManualClock>` set to 2026-11-02T08:00Z (after the corpus's last
month and after today). For 21 working days: advance the clock a day, make 275 verse saves
(about a device's daily share: 620,000 / 3 / 750), `checkpoint(LeavingTranslate)` four times a day
(each seals its pending files into a pack in 2026-11). Then set the clock to 2026-12-01T08:00Z and
run a checkpoint while the save thread runs: its seal consolidates November's own packs (§4.7),
about 6 MB. Report the consolidation step's time (`SealReport.consolidated`) and the saves during
it.

### 5.2 The upgrade case

Build the plan from the open project's snapshot: for every book and both tools,
`fold.decisions(tool, Collection::Book(b))`; for each decision, `PlannedChange { record:
Record::Check { key, value: CheckValue::Decision(d with resource = the same resource with version
"v92" and a new sha) }, parents: fold.heads(&RecordKey::Check(key)), cause: Cause::CarryOver }`;
plus the `Pins` change of the primary group (tN and TWL slots at v92, parents = heads). Report
the plan's change count and encoded size (expect 150 to 260 MB; R9 names 150 MB). Submit
`Command::ApplyUpgrade { plan }` while the save thread runs; then `leave(Mode)` (the whole-project
materialize it causes) with saves going on. Also report private memory before, at the reply and a
minute later (the batch must not stay in memory: step 6, F1).

Expected today: every save waits behind the actor's `build` + `write_pending` (150+ MB written
and flushed) + `fold.add` of about 200,000 changes, seconds on the laptop. That is F1.

Commit: "perfreport: saves and reads while a materialize, a checkpoint, a repack, a consolidation
and an upgrade run, and the mode-switch checkpoint session".

---

## Step 6. Fixes, by measurement

Order: F1 and F2 are expected misses (saves wait behind one long actor step); do them first. F3
to F6 only when the step 3 or step 5 numbers say so. For each fix: measure before (keep the
binary as `target/perf/bin/perfreport-<commit>`), change, measure after with the same rounds,
and put both in the evidence. Data structures and algorithms before caches (rule 6). Each fix is
its own commit with its tests and its spec text.

### F1. A large batch is written and folded off the actor (the upgrade write)

Problem: `Actor::handle(Msg::Command)` runs `commands::plan` and `Project::commit` for
`ApplyUpgrade` on the actor (`commands.rs` 1151-1163 checks every key's heads and builds a
`BTreeSet` of 200,000 keys; `project.rs` 343-482 builds every `Change` (canonical JSON, SHA-256),
encodes the pending file (a line hash each), writes 150+ MB and flushes it, then
`self.fold.add` of every change with its value inline). A save queued behind waits for all of it,
and the fold keeps 150+ MB of values in memory.

Design (spec text to write in STORAGE §5.0 "B-steps for a large batch", §5.8 step 3, §6.3, §6.4,
§14, §16.7):

1. Threshold: a planned batch of more than 2,048 changes or 1 MiB of records is *staged*
   (`const STAGE_CHANGES: usize = 2048`). Today only `ApplyUpgrade` reaches it in practice; apply
   the rule to `ApplyUpgrade` only, and report `AddBook`/`RestoreBook` of Psalms (about 2,500
   changes) in R9 as an extra row to decide whether they need it.
2. On the actor, at receipt (O(1) work): reserve the seqs `[next_seq, next_seq + n)` and advance
   `next_seq`; take `at = clock.now()`, the batch id, `F0 = self.fold.clone()` and
   `seq0 = shared.seq()`; register the final pending path in the `FileSource` and keep its
   number; start recording the record keys of every batch the actor commits from now on
   (`touched: HashSet<RecordKey>`); count the job (`shared.job_started()`, so `wait_idle` and
   `close` wait for it); store the reply in `Actor.staged: Option<Staged>`; hand the plan to a
   worker thread ("tc-store batch"). One staged batch at a time: a second large command waits in
   `deferred` until the first ends.
3. Worker: check the plan against `F0` (two changes of one record → `Refusal::TwoChangesOfOneRecord`;
   parents ≠ `F0.heads(key)` → `PlanStale`) and answer the actor at once if it fails; build each
   `Change` with its reserved seq; stream the pending file into `.tc/pending/<temp_name(name)>`
   through a new `atomic::StagedFile` (below) in 4 MiB chunks, calling
   `gate.yield_to_saves(MAX_YIELD)` before each chunk's `sync_all`, so no single flush carries
   more than 4 MiB while a save waits; keep, per change, its id and the span of its JSON in the
   file; finish with a last `sync_all`. Then send `Msg::Staged { temp, ids, keys, inputs_spans }`.
4. Actor, on `Msg::Staged`: if `touched ∩ plan keys ≠ ∅`, abandon the temp (remove it, held
   retry), reply `Err(CommandError::PlanStale)` ("A newer change just arrived. Look again.",
   J12's look-again), end the job. Otherwise rename the temp to its final name and flush the
   folder (`StagedFile::commit(SyncDir::Now)`, the durability point, a few ms); insert the
   `journal.pending` entry; send `Msg::FoldStaged` to the worker.
5. Worker: `F1 = F0` + the batch's `FoldInput`s made with `FoldInput::new(Verified::Change(c),
   None).prepare_at(ValueAt::new(file_number, span))`, so values stay in the file, not in memory;
   send `Msg::Folded { fold: F1, delta }`.
6. Actor: add to `F1`, in order, the batches it committed since `seq0` (kept as their
   `FoldInput`s while the batch is staged; a few small batches); `self.fold = F1`; mark dirty
   from the batch's delta and the conflicts touched; publish; emit `Committed` for the batch;
   reply `Ok(StoreReply)`; stop recording `touched`; end the job. The fold is a pure function of
   its change set (`fold_converges_under_any_union` in `tests/properties.rs` proves order and
   grouping do not matter), so `F0 + batch + served` equals the sequential result.
7. Checkpoints: a `Msg::Checkpoint` that arrives between the rename (4) and the swap (6) waits in
   `deferred` until the swap: the seal must never pack a pending file whose changes the snapshot
   it commits lacks. Before the rename the temp is not a pending file: `load::list` skips names
   `atomic::temp_target` accepts (`load.rs` about line 82), so `Project::unknown_files` and K1
   never see it; keep it so, and assert it in `a_checkpoint_waits_for_a_renamed_batch` (a
   checkpoint while the worker writes reports no second writer).
8. The ledger (K5, `seal.rs` 410-420) writes `next_seq - 1` today. With reserved seqs that can
   name seqs no file holds (a refused or unfinished staged batch), and the next open would rotate
   the device as "backup restored" (`open.rs` 1036-1048). Write the largest seq this device has in
   a durable file instead (track `durable_max_seq` in `Project`: set by `write_pending` and by the
   staged commit).
9. Crash semantics: a temp that is incomplete is removed at the next open (O2, as today). A
   complete temp whose rename never happened is rolled forward by O2 as any complete pending temp
   is (its seqs are its own, so `(device, seq)` keeps it): the upgrade the person confirmed lands,
   and a record a save changed meanwhile shows a **conflict** between the save and the carried
   decision, never a replacement. Write exactly this in §5.8 step 3 and as a §6.4 row.
10. Close while staged: `close()` already waits for jobs before it sends `Msg::Close`, and the
    actor keeps serving `Msg::Staged`/`Msg::Folded` meanwhile, so the batch ends first.
11. Progress (PERFORMANCE rule 4: every background action shows determinate progress from
    300 ms on): add `Work::Batch` to `handle::Work` and have the worker count changes encoded and
    bytes written into shared counters that the existing 300 ms progress timer reports, as the
    whole-project materialize does; no report when the batch ends sooner.

New code: `crates/tc-store/src/stage.rs` (the worker), `atomic::StagedFile`
(`pub(crate) struct StagedFile; fn create(vfs: &dyn Vfs, dir: &Path, name: &str) -> Result<Self,
WriteError>; fn write(&mut self, bytes: &[u8]) -> io::Result<()>; fn sync(&mut self) ->
io::Result<()>; fn commit(self, sync: SyncDir) -> Result<(), WriteError>; fn abandon(self)`,
removing the temp on drop when neither `commit` nor `abandon` ran, with `atomic_write`'s held-file
retries), `Msg::{Staged, FoldStaged, Folded}`, `hooks::Point::{BeforeStagedRename,
BeforeStagedSwap}` (test holds), `Counters::staged_batches`.

Tests (`crates/tc-store/tests/store/stage.rs`, on `psalms()`; its decisions exceed 2,048):
- `a_save_is_durable_while_a_large_batch_is_written`: hold `BeforeStagedRename`; a verse save's
  reply is `Durable` while the upgrade's reply is pending; release; both in the fold; reopen: both.
- `a_large_batch_whose_records_moved_is_refused_and_leaves_no_file`: hold; `SaveDecision` on a
  plan key; release; `Err(PlanStale)`; no pending file and no temp of the batch; the save kept.
- `a_checkpoint_waits_for_a_renamed_batch`: hold `BeforeStagedSwap`; request a checkpoint; it
  resolves only after the swap, and its commit's tree holds the batch's pack and the files showing
  it.
- `a_refused_large_batch_never_rotates_the_device`: refused batch, a checkpoint (K5), close,
  reopen: no `DeviceRotated` notice, no `DuplicateDevice` anomaly.
- `crash_during_a_large_batch` (FaultFs kill at every write point of the staged path): after each
  reopen the batch is all there or not at all, every save the person was told was saved is there,
  any plan key a save touched shows a conflict, the folder is a valid burrito.
- `a_large_batch_shows_determinate_progress_by_300_ms`: with the worker slowed by a `FaultFs`
  delay, the first `Progress { work: Work::Batch, .. }` comes at 300 ms or a little after, its
  `done` only grows, and none comes for a batch that ends in under 300 ms.
- `a_large_batch_keeps_its_values_in_the_file`: after the swap, the fold's
  `counters().values_read` rises when a carried decision is projected (its value is read from the
  file, not held).
- Mutation rows (§16.7): "fold a large batch on the actor" → the first test; "skip the touched
  check" → the second; "ledger from next_seq" → the fourth.

Measure again: `r9.upgrade` saves p95 must be at or under 25 ms here and its job time reported;
memory a minute after the upgrade back near the steady figure.

### F2. The month's consolidation runs in steps

Problem: S2 split the seal into steps but K6 (`seal.rs` `consolidate`, Q1-Q4) is one step: read
and verify every old pack, build the new pack, write and flush it, re-read and verify it, remove
the old ones. Once a month a save waits for all of it (S2's evidence names it).

Change: make Q1-Q4 seal phases the actor serves commands between: one step per pack read and
verified (Q1); one step builds the new pack(s) (`tc_journal::consolidate::consolidate`); the write
goes through `StagedFile` in 4 MiB chunks, one chunk per step, yielding to saves before each
flush; one step renames and flushes the folder; Q3 re-reads in 4 MiB steps (verify at the end);
Q4 removes one pack per step, then flushes the folder. A crash at any step leaves the old packs
(Q4 never ran) or the new pack verified plus some old ones (redundant, removed at the next seal).

Tests: `a_save_is_served_between_the_steps_of_a_consolidation` (like `writer.rs`'s seal test, with
`hooks.at_seal_step`, a month of own packs made with a `ManualClock`); extend the consolidation
crash test in `tests/store/crash.rs` to kill at every new step boundary (no change lost, no
change duplicated as two different ids, redundant packs end removed). STORAGE §5.11 K6 and §5.12
say "in steps"; §16.7 row.

### F3. Interactive reads beside the writer's projection (only if `read.*_during_*` misses)

`handle.read_book` runs on rayon's global pool (`handle.rs` 922-940), the same pool the writer's
render uses (the budget review saw a Genesis read go from 10 ms to a p95 of 186-296 ms during a
materialize). Fix: give the writer its own `rayon::ThreadPool` (2 threads on a 4-core machine,
`threads.saturating_sub(2).max(1)` in general) and run its projections inside
`pool.install(..)`; reads keep the global pool. Test:
`a_book_read_does_not_wait_for_a_whole_project_materialize` (hold the writer mid-job with a hold
point inside its render loop; a `read_book` resolves). Measure the reads again.

### F4. The checkpoint (500 ms): measure, try one pack per checkpoint, give the owner the choice

S2's laptop estimate is 0.8 to 1.3 s (already materialized) and 1.3 to 2.4 s (Psalms dirty): about
twelve loose objects with two flushes each, plus the tree. Candidate: write a checkpoint's new
objects as one pack. Do not rely on libgit2's packbuilder to skip deltas: libgit2 1.9.7 reads its
big-file threshold from the key `pack.deltaCacheSize` (`pack-objects.c:118`, a libgit2 bug), and a
delta search over the 5-8 MB Psalms files is what S2 measured as too slow. Write the pack stream
yourself (header, each object `type+size` varint + zlib, SHA-1 trailer: no deltas) into
`odb.packwriter()`, which indexes and flushes it (about 3 flushes in all). Then the repack (§9.8)
must also count packs and merge small ones. Keep it only if the checkpoint's laptop estimate
drops and every git test (`tests/store/git.rs`, `checkpoint.rs`, `repack.rs`, `crash.rs`)
passes; otherwise revert and report. Either way, **do not edit the 500 ms row**: write the review
of step 13.4 for the owner.

### F5. Close (only if its laptop estimate is over 3 s)

The close writes the whole 65 MB fold cache again (`actor.rs` close path, `open::write_index_file`)
though the open's background job wrote it and a session adds a few files. Algorithmic fix: write
only the sections of files added since the open, by keeping one cache file per month folder or an
append-only cache whose directory is rewritten atomically; any format change bumps
`INDEX_VERSION` (the first open after it is a 10 s open, which the budget allows). Measure the
close before and after; keep `the_fold_cache_is_kept_by_every_open_of_the_same_encoding` green.

### F6. Materialize one book (only if its laptop estimate is over 1 s)

11 flushes per Psalms materialize (S2 counters). Look for flushes `DirSet`/`SyncDir::Deferred`
can share (one folder flush after the book's renames, not one per file) without breaking I7/I8
and the crash tests of `groups.rs` and `crash.rs`. Defender's scan of each created file is only
measurable on the laptop (step 12).

### Domain fixes

If a domain row misses (step 9-11), the same rule: count first, then the data structure. Likely
suspects, to check with counts: tc-check carry-over's quote matching (must be by index, not
O(n²)); tc-resources `load_book` opening hundreds of small tW article files (count them: on
Windows each open is a Defender scan; if the count is in the hundreds, load articles on demand
and report it).

---

## Step 7. Memory of the project store (M2) and its growth (M3)

- `perfreport memory DIR` (child process per variant, both `MALLOC_ARENA_MAX=1` and the default
  heap): storebench's `memory` case (open, 200 saves and 200 decisions in Psalms, `leave`, wait
  60 s, read private bytes), then a whole-project materialize and the close, reading `VmHWM` after
  each; plus a child doing only `open.nocache` for that open's peak. Budgets 400 MiB steady, 450 MiB
  peak (the close and the first open without a cache included). Class Memory (laptop estimate =
  the one-heap figure x `memory` ratio; the default-heap figure is reported for Linux users).
- `perfreport growth DIR`: a second project `DIR/projects/bible-2x` from
  `Spec::whole_bible(1).with_history(750, 2.0)` (about 1.2 million changes; what `foldbench save
  bible-share 2.0` makes), set up like `setup`. Steady private bytes (the memory case's reading) of
  both, each heap: growth = (steady_2x - steady_1x) / (changes_2x - changes_1x). Budgets 300 B one
  heap, 350 B default heap. Delete `bible-2x` after.
- If M2's one-heap figure misses, profile with the fold's table sizes (`size_of` assertions in
  `fold/state.rs` 2299-2312) and the store's caches before touching allocators; `unsafe` is
  forbidden, so `mallopt`/`malloc_trim` are not available.
- Commit: "perfreport: the project store's memory and its growth at twice today's history".

---

## Step 8. Three simulated years

### 8.1 A day-by-day timeline in tc-corpus

`crates/tc-corpus/src/timeline.rs` (pub), driven by `history::Sim`:

```rust
/// A team's work day by day, as a fourth computer that syncs every working day receives it:
/// the teammates' packs on the real seal schedule (a checkpoint before each sync and at the
/// end of the day, `seals_per_day` in all), their consolidations (§4.7, by
/// tc_journal::consolidate), and the packs those made redundant.
pub struct Schedule { pub seals_per_day: u32 }            // 4
pub struct Day {
    pub day: u32, pub evening: WallTime, pub month: YearMonth, pub month_ends: bool,
    pub add: Vec<PackFile>,        // packs that reached the hub today
    pub remove: Vec<String>,       // pack paths a pack added today lists in `replaces`
    pub changes: usize,            // changes in every pack the fourth computer holds after today
}
impl Corpus {
    pub fn timeline(spec: &Spec, schedule: &Schedule,
                    on_day: &mut dyn FnMut(&Day) -> Result<(), String>) -> Result<Stats, CorpusError>;
}
// Spec: pub reserved: Vec<(BookId, u16)>  (empty by default; `Corpus::generate` unchanged)
impl Spec { pub fn with_reserved(self, chapters: Vec<(BookId, u16)>) -> Self }
```

- Reserved chapters: their verses are seeded by `add_book` and never edited again by the team; no
  alignment, decision on a check of theirs or comment targets them (filter them out of each
  `Part`'s `texts`, `alignments` and `checks` in `assign_books` and out of `comment` targets).
  The measuring computer edits them, so its edits never conflict with the team's. `Spec::rates`
  sets `reserved: Vec::new()`; every existing constructor goes through it.
- The years driver needs the check keys of Genesis 50's rows: make the generator's private
  `Sim::check_key(row: &CheckRow)` (history.rs line 1405) a public free function
  `tc_corpus::history::check_key(row: &CheckRow) -> Result<CheckKey, CorpusError>` and call it
  from both places. Keep it out of `domain.rs`, whose types carry no tc-journal type by design.
- Seals: split each device's `work_day` actions into `seals_per_day` runs and seal after each
  (`encode()` then `tc_journal::pack::seal` synchronously, not the sealer threads); after each seal,
  `tc_journal::consolidate::consolidation_groups(&own, month)` and `consolidate(spec, &reads)` on
  that device's own packs, keeping its current and previous month's packs in memory for it.
- Exchange: a device's sealed packs wait in `unpushed_packs` until its next sync (`sync_rate`);
  at a sync the pending changes are sealed first (the checkpoint before sync). The fourth computer
  pulls every day. A pack whose `replaces` names packs it already holds removes them; packs a
  consolidated pack replaces that never reached it are never added.
- The change-level push/pull the teammates use among themselves is unchanged, and so is
  `generate()`'s output for every existing spec (617,086 changes, same packs): pin it with a test.
- Memory: emit each day's packs to the callback and drop them; keep only what consolidation needs.

Tests (`crates/tc-corpus/tests/corpus.rs`, on `Spec::psalms(1).with_history(60, 0.2)`):
- `the_timeline_ends_with_the_changes_of_the_corpus`: the change ids of every pack held after the
  last day equal those of `generate` for the same spec (its packs plus its pending files).
- `reserved_chapters_hold_only_their_seed`: every change of a reserved key has `Cause::Seed`.
- `the_folder_never_holds_a_redundant_pack`: after every day, no held pack is named in another
  held pack's `replaces`, and every consolidated pack's `replaces` names only packs of its sealer.
- `generate_is_unchanged`: `Spec::psalms(1)` change count and the first pack's id equal the values
  recorded before this change (record them first).

### 8.2 The years driver

`perfreport years DIR [--days 750] [--open-every 1] [--seals-per-day 4]`:

- Spec: `Spec::whole_bible(1).with_reserved(vec![(BookId::Genesis, 50), (BookId::Psalms, 119)])`.
  Genesis is the team's first book (present from day 0); Psalms is added when the team reaches it
  (books are worked in order, `current_book`), about a third of the way in.
- One `Store` for the whole run, `clock: Arc<ManualClock>`, folder `DIR/years/project` with
  `project.json`, `format.json`, `.gitignore`, `.gitattributes` as `setup` writes them and **no
  device file** (the store mints the fourth computer's device at the first open).
- Each simulated day: write `add`, remove `remove` (the project is closed: a pack put into the
  folder between two opens is how §16.5a stands in for unit X's intake); set the clock to the
  day's evening; every `--open-every` days and on every month's last day: open, `wait_idle`, (on
  month ends: measure), close (the close checkpoints: seal, consolidation on the store's own
  clock, materialize, commit, repack on its thresholds).
- Month-end measurement (one row per simulated month): changes held, packs, pending files, `.git`
  MiB, working tree MiB; open usable ms and first progress; 50 verse saves on Genesis 50's verses
  (p50, p95; files opened, created, fsyncs and bytes written per save); 50 decisions on Genesis
  50's checks (the first month creates them with a status; later months toggle the bookmark;
  `resource` copied from a decided check of the same tool in the project); `read_book(Genesis)`
  and, once present, `read_book(Psalms)` 20 times (p50, p95, bytes read); `chapter_usfm` p95; the
  fold step of step 3 on a Genesis 50 verse 200 times (p50, p95 and its `Counters`);
  `conflict_counts` after a save; the close's checkpoint ms; private MiB a few seconds after the
  last action.
- Assertions (the run exits 1 when one fails): per-save store counts identical every month; the
  fold step's counts identical every month for the same verse (its own history grows by the
  measured saves: that is the point); and for save p95, book open p95 (Genesis) and fold step
  p95, the median of months 31-36 at most 1.25 x the median of months 4-9 (after Genesis is
  drafted). Print the least-squares slope per month of each of those, and of the open time (which
  may grow: it is background work with its own budget).
- Output: `DIR/years/years.tsv` and a table on stdout; the evidence gets the whole table.
- Expect 1 to 2 hours with `--open-every 1` (750 opens and closes, 36 measurements). Run it once
  for the evidence; use `--open-every 10` while developing.

Commits: "tc-corpus: a day-by-day timeline with the real seal and consolidation schedule" and
"perfreport: three simulated years, measured every month".

---

## Step 9. tc-check

### 9.1 New API (pure, no I/O)

- `crates/tc-check/src/session.rs`: an index built in `open`, `by_verse: BTreeMap<(u16, u16),
  Vec<usize>>`, every verse a check's target covers (parse `RefPart` spans like "9-10"; a front
  target as verse 0). `pub fn checks_at(&self, chapter: u16, verse: u16) -> &[usize]` and
  `pub fn reassess<'t>(&mut self, chapter: u16, verse: u16, decisions: &[ProjectedDecision],
  resolution: Option<&ResourceRef>, text: impl Fn(&CheckTarget) -> Option<&'t TargetText>) ->
  Vec<usize>`: recomputes `states[i]` with `CheckState::of` for those checks only, returns their
  indices (the rows a screen redraws). If `Session` is built anywhere but `open`, adapt.
- `crates/tc-check/src/rail.rs`: `impl Progress { pub fn update(&mut self, before: &CheckState,
  after: &CheckState) }`, O(1).

### 9.2 Count tests (`crates/tc-check/tests/operation_counts.rs`; dev-dependency `tc-corpus`)

Folds of `Spec::psalms(1)` and `Spec::scaled(1, 10)` (as tc-journal's test builds them), Psalms tN
and TWL from `fixtures/scale/psa/`, the en_tw v91 articles (as `performance.rs` reads them):

- `opening_a_psalms_session_assesses_each_check_once`: `session::open` with
  `fold.decisions(tool, Book(Psalms))` and `fold.check_texts(..)`; the `text` closure is called at
  most once per check (count with a `Cell`), the same on both corpora, and the fold's
  `records_listed`/`values_read` for `decisions(..)` no larger on the larger corpus.
- `reassessing_a_verse_touches_only_its_checks`: `reassess(23, 1, ..)` returns exactly
  `checks_at(23, 1)` and calls `text` that many times, on both corpora; no other state changes.
- `progress_after_a_decision_costs_one_row`: proptest, any sequence of state changes:
  `Progress::update` equals `progress(states)` recomputed.
- `rail_groups_visit_each_row_once`: `rail::groups` on the Psalms session visits each row once
  per pass (count through the predicate or a comparator `Cell`); APP-DESIGN B5a names this test.
- `counting_consequences_visits_each_decision_once`: `change::consequences` over the whole-corpus
  units visits each stored decision once (count through the `local` closure and the units).
- `a_books_carry_over_costs_that_book`: `carry::plan` for Psalms compares quotes at most
  `c x n log n` times (count with a `Cell` around `same_quote`, or add a counter); if it is
  quadratic, index by (verse, normalized quote) and say so.

### 9.3 Rows in perfreport (`perfreport/check_rows.rs`)

On the open whole-Bible project's snapshot (the corpus's decisions are on the real v91 tN and TWL
rows of 59 books; the other seven are generated):
- `check.open_first` (250 ms): parse Psalms tN and TWL TSVs (from `bundle/en` exports, as the app
  reads installed files), derive both lists for an `eng` project, `fold.decisions(..)` for both
  tools, `session::open`, progress. First of a process (a child) and 21 later opens.
- `check.open_cached` (20 ms): the lists already derived: `fold.decisions` + `session::open`
  (what a `CheckListCache` hit leaves, B5a).
- `check.rail` (16 ms): `rail::groups` with each filter, sort and grouping on the Psalms tW and tN
  sessions.
- `check.decision_counts` (1 ms): `Progress::update` (and the full `progress` for reference).
- `check.reassess` (1 ms): `reassess` of a Psalms verse with checks on it.
- `check.consequences` (2 s): `change::consequences(current, next, units, local)` over all 132
  (tool, book) units, `next` = the pins with tN and TWL at a new version; units from
  `fold.decisions` of every book (215,767 checks; report the decision count actually found).
- `check.carry_book` (300 ms) and `check.carry_bible` (5 s, background): `carry::plan(items,
  resource, stored)` per book, where `items` are the v91 lists with 2% of rows dropped and 2% of
  quotes changed, deterministically (seeded), and `resource` a new ResourceRef.
All Single class.

Commit: "tc-check: re-assess one verse through a per-verse index, O(1) progress, count tests; perfreport rows".

---

## Step 10. tc-align

### 10.1 Count tests (`crates/tc-align/tests/operation_counts.rs`)

- `opening_the_aligner_sorts_only_the_verse`: `Aligner::open` on PSA 42:4 (the stored value with
  reversed bank and cards, as `psalms.rs` builds it) compares at most `n log2 n x 4` word pairs,
  n = the verse's words (add a thread-local comparison counter in the verse-order sort, read by
  `tc_align::edit::sort_comparisons()`, or count through the word-position lookup).
- `a_suggestion_costs_the_verse_not_the_corpus`: a thread-local counter of memory lookups in
  `suggest::wordmap` (`Memory::read`), exposed as `tc_align::suggest::lookups()`; predicting PSA
  42:4 with a model trained on GEN + PSA and with one trained on GEN + PSA + ten more OT books of
  the ULT v91 export: equal lookups and equal prediction counts (the n-grams of one verse). If the
  count grows with the corpus, that is a finding: index it.
- `a_reflow_costs_the_verse`: `reflow` examines each link of the verse once (count through the
  value's groups).

### 10.2 Rows (`perfreport/align_rows.rs`)

- `align.open` (30 ms), `align.edits` (44 edits inside 30 ms), `align.reflow` (2 ms): exactly
  `tc-align/tests/psalms.rs`'s procedure on PSA 42:4 of `fixtures/scale/psa/en_ult/19-PSA.usfm`
  with the UHB 3.0.0.
- `align.suggest` (50 ms): training verses from `bundle/en/en_ult_v91.sb.zip`
  (`ingredients/<BOOK>.usfm`, `TrainingVerse::from_aligned_usfm(reference, unit.body())` as
  `tests/bench.rs` reads them): the NT model trained on the 27 NT books, the OT model on GEN + PSA
  with PSA 42:4 held out (the verse being aligned has no alignment yet); predict PSA 42:4 with the
  OT model, 21 runs. Report both trainings' times (background, `Info`) and
  `Model::append_verse` (the update after a save, background, `Info`).
All Single class.

Commit: "tc-align: count tests for the aligner, suggestions and reflow; perfreport rows".

---

## Step 11. tc-resources

### 11.1 The bundled store from `bundle/en`

Add `crates/tc-resources/src/bundle.rs`, `pub fn install_bundle(from: &Path, store:
&ResourceStore, wanted: &dyn Fn(&ResourcePin) -> bool) -> Result<BundleInstalled, BundleError>`:
the core of main's `crates/tc-app/examples/bundle_store/build.rs` (`build_bundle`): each pinned
export checked against `SHA256SUMS` and installed with `install_dcs_zip`, each lexicon against
`fetched.lock` with `install_lexicon_zip`, a pin whose file is absent (the OBS pictures) skipped
and named. Make tc-app's `build_bundle` call it and keep writing its `BUNDLE-MANIFEST.json`
(migrate the caller in the same commit; run `cargo test -p tc-app -p tc4`). perfreport `setup` and
the tests below use it.

### 11.2 The owner's tW/TWL ruling on the real exports

`crates/tc-resources/tests/tw_twl_ruling.rs`, installing `bundle/en/en_tw_v91.sb.zip` and
`en_twl_v91.sb.zip` with `suite`'s pins (`suite::english_helps()` for tW, `suite::english_twl()`;
`suite::project_pins()` for the ladder). Add a thread-local recorder of opened content paths to
`store.rs` next to `content_files_opened`: `pub fn with_opened_paths<T>(f: impl FnOnce() -> T) ->
(T, Vec<PathBuf>)` (records only while armed; `read_content` pushes the path).

- `num_reports_word_links_missing_and_reads_none_of_the_tw_exports_tsvs`: `load_book(store,
  &pins, &HelpsChoice::Dcs, BookId::Numbers)`: the word-links part is `PartStatus::Missing(..)`
  (expect `NotCovered` naming the TWL pin), no word link is loaded, and no opened path is a `.tsv`
  under the en_tw install (its 66 master-built TWL files).
- `psalms_loads_exactly_7683_rows_from_en_twl`: `load_book(.., Psalms)`: 7,683 word links, the
  part's source is the en_twl pin, and every opened `.tsv` is under the en_twl install.
- `articles_come_from_en_tw_only`: build a copy of the en_twl export with
  `ingredients/payload/kt/god.md` replaced by different bytes and its `metadata.json`
  ingredient md5 and size rewritten so the copy installs; install it as the en_twl pin; load
  Psalms; the "god" article's text equals en_tw's bytes, and no opened path is under
  `ingredients/payload/` of the en_twl install.
- `the_tw_pin_covers_every_book_and_the_twl_pin_its_59`: `resolve_slot(&pins,
  SetSlot::TranslationWords, Collection::Book(b), &store)` is usable for all 66 books (NUM
  included); the en_twl install's `books()` is exactly the 59 books of its export's metadata, and
  `resolve_tool(.., ToolId::TranslationWords, Collection::Book(b), ..)` is `None` for NUM, 1CH,
  2CH, ECC, ISA, JER and EZK.

`crates/tc-check/tests/twl_payload.rs`:
- `payload_links_derive_the_same_checks_as_rc_links_in_all_59_books`: for each of the 59 book
  TSVs of the en_twl export (`zip` crate), derive `word_links_for_book` from the export text
  (`./payload/<category>/<slug>.md`) and from the same text with each link written back as
  `rc://*/tw/dict/bible/<category>/<slug>`: equal `CheckList`s (items, skipped, dropped). For
  Psalms, also equal to the list from the repository's own file
  `fixtures/scale/psa/en_twl/twl_PSA.tsv` (rc:// form, the v91 commit). The articles gate is the
  en_tw v91 article set, as `performance.rs` reads it.

### 11.3 Seeding counts on the real bundle, with tC4's start counts

`crates/tc-resources/tests/bundle_seed.rs`:
- `the_bundled_english_suite_seeds_as_tc4_counts`: `install_bundle(bundle/en)` into store A; fresh
  store B; `seed_plan(&A)` opens and writes no content file; `seed(plan, ..)` opens each of A's
  content files exactly once (`with_opened_paths`: no duplicate, every path under A), writes each
  planned file once (`content_files_written() == plan.files()`), reads none back (no opened path
  under B); a second `seed_plan` is empty and seeding it opens and writes nothing. tC4's targets
  (PERFORMANCE "Start-up"): each shipped archive read once, each installed file written once and
  never read back, a repeat start 0 and 0. Print the rebuild's numbers beside tC4's (16 archives,
  3,739 resource files, 3,792 files; the rebuild today: 15 installs, about 3,153 files, the OBS
  pictures absent) and assert the per-file rules, not tC4's totals (the pictures will change
  them).
- Debug-build cost: installing the 15 exports verifies 268 MB of md5 and inflates as much. Add
  `[profile.dev.package.md-5]`, `zip` and `zlib-rs` (check the lock for the inflate crate's name)
  at `opt-level = 3` with a comment, like the other overrides. If the test still takes over
  60 s in debug, seed only en_tw, en_twl, en_tn and both lexicons in `cargo test` and keep the full
  suite in perfreport's `seed.counts` row (which fails the report on a count mismatch).

### 11.4 Loading Psalms resources

`crates/tc-resources/tests/operation_counts.rs`:
- `loading_a_book_opens_only_its_files`: store with the bundle; `with_opened_paths(||
  load_book(.., Psalms))`: every opened file is Psalms' own (`PSA.tsv`, `PSA.usfm`, `19-PSA.usfm`
  names as each export names them) or a tW article Psalms' links name; the count equals the
  distinct linked articles plus one file per part. Then install ten more copies of en_tn under
  other owners (rewritten pins) and load again: the same paths.

Rows (`perfreport/resource_rows.rs`):
- `res.load_psalms` (300 ms; first of a child process, page cache evicted and warm both; report
  the content files opened, which on Windows are Defender scans each) and `res.memory_psalms`
  (60 MiB: private bytes after `load_book(Psalms)` kept minus before).
- `lex.load` (100 ms) and `lex.lookup` (1 ms): `lexicon_performance.rs`'s procedure (en_uhl, every
  `strong` of the UHB Psalms).
- `seed.first` (60 s, the seeding part of the gate, labeled so): plan + seed of the whole bundle
  into a fresh app data folder, bundled store evicted from the page cache, 5 runs; `seed.repeat`:
  plan + seed again, with the counts (0 content files read or written) as a hard check;
  `seed.counts`: the counts of 11.3 on the full suite.
Classes: Single for loads and lookups; Disk for seeding (a file flush each).

Commit(s): "tc-resources: install_bundle from bundle/en; the tW/TWL ruling on the real exports;
seeding counts on the real bundle; load counts; perfreport rows".

---

## Step 12. The laptop run, prepared for the owner's Windows session

P cannot run on the Dell. Make it one command there:
- `spikes/perfreport-windows/run-perfreport.ps1`, modeled on `spikes/foldbench-windows/
  run-foldbench.ps1` (its `Invoke-Foldbench` samples the process from outside at watched stdout
  lines): build `cargo build --release -p tc-corpus --example perfreport` (or `-SkipBuild`), run
  `setup`, then `run` (no ratios: it is the laptop), `r9`, `memory` with readings of the private
  working set (`\Process(perfreport*)\Working Set - Private`) at each `MARK` line; write
  `docs/evidence/storage-scale-windows-<date>.md` with the table, the machine
  (`spikes/machine.ps1`), Defender's state; 3 rounds.
- `.audit/tasks/windows-track.md`: add "Task 7: the storage core's budgets on the laptop (unit
  P)": branch `windows-perf` from `origin/storage`, Defender on, no exclusion for the project or
  app data folders, run the script, push. Say which rows only it can settle: every Disk row
  (saves, R9, flush), materialize one book, checkpoint, close, memory (Windows heap), seeding.
- perfreport must build and run on Windows: every `/proc` read behind `cfg(target_os = "linux")`
  with "unavailable" elsewhere, no `taskset`, no `rustix` outside `cfg(unix)`.

Commit: "A one-command run of perfreport on the reference laptop, for the Windows track".

---

## Step 13. Final rounds, evidence, spec, cleanup

### 13.1 Final measurements

At least three full rounds of `perfreport run`, `r9` and `memory` at different times of day on
the final commit, with `taskset -c 0-7`, `RAYON_NUM_THREADS=8`; `growth` once; `years` once with
`--open-every 1`. Record `uptime` before and after each. Ranges over rounds, as S2's evidence
does; a round the drive stalled in is named, not dropped.

### 13.2 `docs/evidence/storage-scale-<date>.md`

Sections: machine and method (this PC with its SHA/AVX bits and why its raw figures do not judge
the laptop; taskset, threads, heaps, `TEST_HOOKS = false`; commits; commands); the laptop ratio
(step 2 table); the verdict table (every owned row: here median/p95, class, laptop estimate,
Ice Lake figure where one exists, budget, verdict on the laptop, "holds without SHA
instructions?" yes/no/not shown); R9 and J8 (with the answer for AD34/Q-5: adopt a checkpoint at
every mode switch only if saves during a checkpoint stay within 25 ms here **and** on the laptop's
run; until that run, Q-5's limit of one per two minutes is the safe default); reads during
background work; what each action did (counters); the three years (per-month table, slopes,
verdicts, `.git` and pack counts at year 3); memory and growth; what was optimized (before/after,
commits); the checkpoint review (13.4); domain crates (rows, the ruling's results, seeding counts
next to tC4's); what is not shown (Disk rows on the laptop, Defender, the merge commit (proxy),
M1, sync/export/import, the laptop's cold restart); reproduce.

### 13.3 STORAGE.md

- §14: replace the measured rows (cold opens, save, materialize, checkpoint, whole-project, close,
  saves during background work, memory, growth) with P's figures and laptop estimates; the `.git`
  after three years and the pack count from the years run; the "VM rows below are the CPU class
  without SHA instructions" sentence corrected (this PC has SHA; the laptop ratio stands in).
  Update the "Rows of the project store that are not met or not shown" paragraph.
- §5.0, §5.8 step 3, §6.3, §6.4 (F1); §5.11 K6 and §5.12 (F2); §9.4 or §9.8 (F4, if kept); §3.6
  and §18 item 15 (F5, if made); §16.5a (every new count test); §16.7 (a mutation row per new
  test); §18: the merge commit's R9 case is unit X's.
- Any sentence P finds wrong: fix it in the same commit and say why in the message.

### 13.4 PERFORMANCE.md and APP-DESIGN.md

- Do not change a budget. Rewrite "Rows of the project store that are open" with P's verdicts.
- Add "Checkpoint: review for the owner" (the same shape as BUDGET-REVIEW §1): what a person feels
  of a checkpoint now that it runs off the actor (saves during it: the 25 ms row; the first phase
  of a sync or export; the close; reopening after Home), the measured and estimated figures before
  and after F4, the options (keep 500 ms and build F4; reframe as "off the actor, saves continue,
  N s on the laptop"; relax), and a recommendation. The owner decides; the row stays 500 ms.
- Rule 5: name the `perf` CI job.
- APP-DESIGN §9.2 R9 row and Q-5: one line each pointing to the evidence ("measured by unit P:
  ..."); change no decision.

### 13.5 Cleanup and gates

Delete `target/perf/` corpora and projects, `target/ref-58b6874`, bench binaries; check `df -h
/home`. Run `cargo fmt --all`, `cargo clippy --workspace --all-targets -j 6 -- -D warnings`,
`cargo test --workspace -j 6`, and the wasm32 clippy runs of step 0 (main's crates are in the
workspace now). Then `cargo build --release -p tc-corpus --example perfreport` and one
`perfreport ci target/perf-ci --tolerance 3` as the CI job will run it; delete `target/perf-ci`.

---

## Invariants every change must keep

- A save is acknowledged only when durable (I-rules, §5.0 B3); a staged batch's reply comes only
  after its rename is flushed and its fold swapped in.
- No silent replacement: a record touched by a save during a staged batch either refuses the batch
  (PlanStale) or, after a crash, shows a conflict. Never "newest wins".
- A pending file's changes are packed only by a seal whose snapshot holds them (F1 point 7).
- No seq is used twice by one device; the ledger never names a seq no durable file holds (F1
  point 8).
- The folder is a valid Scripture Burrito after every write group (`assert_valid_burrito` in every
  new store test that writes).
- `Corpus::generate` gives the same corpus as before for every existing spec (other units'
  evidence compares against 617,086 changes).
- No network in any test or benchmark; no `unsafe`; library code returns `Result`.

## Traps found while planning

1. This PC has SHA instructions; the task text's "this VM" does not exist any more. Judge every
   budget through the laptop ratio.
2. storebench links tc-store's `test-hooks`; perfreport in tc-corpus does not, as long as you run
   it with `-p tc-corpus` alone. A workspace build can unify the feature: perfreport prints
   `TEST_HOOKS`.
3. Without `test-hooks` there is no `repack_after`: repacks run at §9.8's thresholds during the
   cases. Report `git_repacks` beside every checkpoint row.
4. `drop_caches` needs root: evict with `fadvise` after `sync`. Directory entries and inodes stay
   cached, so "cold" is approximate; say so.
5. `perf` sampling is refused (`perf_event_paranoid` 4). Use timers and counters.
6. Never rebuild perfreport while a benchmark runs from `target/release/examples/`: copy each
   measured build to `target/perf/bin/perfreport-<short commit>` first.
7. 58b6874's tree builds against its own `Cargo.lock`; build it with `--manifest-path` into the
   worktree's own `target/` and copy the binary out; never a second target folder.
8. The laptop rows are working set (Windows) while here private bytes and RSS are read; the
   budget review measured the one-heap ratio at 0.97-1.07. Compare like with like.
9. A save's cost is two flushes; on a shared SATA drive another agent's build can stall a flush
   for hundreds of ms (S2 saw 190-244 ms rounds). Rerun such a round, name it.
10. ApplyUpgrade's plan alone is 150-260 MB of records in memory before the store sees it, and the
    bench holds it too: watch the bench's own peak, and drop the plan as soon as it is submitted.
11. `load::list` skips `.~*.tmp` names today (`temp_target`); if a staged temp ever got another
    name pattern, the next checkpoint's listing would call it a second writer and turn the
    project read-only.
12. Reserved chapters change the corpus: only the years run uses them; `generate()` must stay
    byte-for-byte the same for existing specs.
13. A pack put into the folder while the store is open is a second writer (read-only): the years
    run writes teammates' packs only while the project is closed.
14. The years run's measuring computer must not edit records the team edits later, or its edits
    turn into thousands of conflicts that distort the counts: that is what the reserved chapters
    are for.
15. `content_files_opened` is thread-local: count on the thread that loads or seeds.
16. On Windows, every resource file opened is a Defender scan; a count of hundreds of tW article
    files per book is a laptop risk even when the Linux time is small. Report the count.
17. libgit2 1.9.7 reads its pack big-file threshold from `pack.deltaCacheSize`, not
    `core.bigFileThreshold` (`pack-objects.c:118`): config cannot turn off the delta search for
    one packbuilder without affecting the repack.
18. Main's parity tests run in `cargo test --workspace` after the merge and need `~/.tc-env`
    (node, Playwright override).
19. `STORAGE.md` is about 5,250 lines now, not 3,500; §14 is at about line 4412.
20. The 300 ms Psalms projection row and the 350/150 MiB memory row no longer exist; print the
    projection only as a foldbench ratio input, never as a budget.

## What the final answer must list

Commits; the verdict table's summary (rows met, missed, not shown on the laptop); the R9 numbers
and the J8 answer; the fixes with before/after; the years run's verdict; memory and growth; the
domain rows and the ruling's test results; spec changes (STORAGE.md sections and why); the
checkpoint question for the owner; what only the laptop run can settle; open issues.
