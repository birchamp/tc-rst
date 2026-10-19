# The app on the store (unit C1a, 2026-10-19)

Unit C1a moved the app onto tc-store: app data on disk through `tc_store::atomic_write`, the
store's half of the persisted summaries, the parity scenarios' projects on disk, and the parity
environment switched to the store. No PERFORMANCE row belongs to C1a alone; the numbers below are
what the person waits for (the language save), what the parity harness pays, and the counts that
hold a summary stamp to PERFORMANCE rule 1.

## Machine and commit

- The owner's Linux PC: 12th Gen Intel Core i7-12700, 20 logical CPUs, `/home` on ext4 on a
  local SATA disk (`/dev/sdb2`). The CPU has SHA instructions (`sha_ni`) and no AVX-512. Shared
  with several other builds: the load average was about 20 during every run below, so the p95s
  carry that noise.
- Code at `4a90942` (branch `app-c1a`, after the merge of `claude/focused-ride-5uzj7p` at
  `a5d492a`). The scripts are in the session's scratchpad, not committed; each is a few lines over
  `tc4 serve`'s HTTP API.

## The language save on disk (release)

`prefs/setLocale` waits for its write: `prefs/app.json` replaced by `atomic_write` (temporary
file, flush, rename, folder flush) on the blocking pool. Release `tc4` without hooks, 20 round
trips after one warm-up, alternating `ar` and `en`, through HTTP:

| | median | p95 | min | max |
|---|---|---|---|---|
| `prefs/setLocale` | 14.2 ms | 19.7 ms | 6.5 ms | 21.0 ms |

The two fsyncs are most of it (the debug build gave 11.9 ms median, 18.6 ms p95, inside the same
noise). M16's Apply closes after this write, as tC4's does.

## The parity harness's costs (debug `tc4`, as the harness runs it)

| What | store | memory | runs |
|---|---|---|---|
| `tc4 serve --scenario sample` to its address line (seed, check, write, place, first open and Burrito check of `sample_burrito`; the resources install) | 1815 ms median, 2024 ms max | 1582 ms median, 2129 ms max | 5 each |
| `test/reset` with no project opened (every phase-1 screen's `home()`) | 19.3 ms median, 27.1 ms p95 | — | 20 |
| `capture-rust.mjs` of the five A5 dialogs | 32.2 s | 30.7 s | 1 each |

The store adds about a quarter of a second to a scenario's start, once per capture run. A reset
after a screen that opened the project writes that project again (one `ScenarioStore` write,
part of the 233 ms above); no phase-1 screen opens a project yet (B3 brings the open).

Each written project mints a device at its first open and adds a line to tc-store's
`ledger.json` in the run's app data: harmless in a run folder that is thrown away.

## The summary stamp (D8)

`a_summary_stamp_lists_one_folder_and_stats_one_file_at_any_history`, counted by tc-store's own
`CountingVfs`: computing the stamp of `sample_burrito` costs **1 listing, 1 stat, 0 files opened,
0 bytes read**, and exactly the same after two more teammate packs are written into the project.
The stamp never lists the packs (nested by sealer and month), so its cost cannot grow with the
history.

## The count rows on the store (D10)

`tests/op_counts/` runs every row on tc-corpus Psalms and on the corpus ten times larger, over
the memory backend and over the store (the corpora written to `FaultFs` as tc-store's own count
tests write them). Every row passes on both; a snapshot and a chapter read touch no file on the
store, and the app's counts are the same on both backends. The open row holds the app's counts
only on the store, where the fold and the journal reads are the store's budget. The planted row
that reads every chapter fails on both. The binary takes about 140 s under this machine's load
(40 s on the memory backend alone): writing and opening ten times Psalms on `FaultFs` in a debug
build dominates.

## J15 on `FaultFs` delays

`j15_a_slow_open_reports_from_300_ms_and_counts_up`: the scenario project on `FaultFs` with a
60 ms delay per read; the open takes more than 600 ms, its first report comes between 300 and
420 ms, every report has a total, phases only move forward and counts never fall within one.
`j15_a_quick_open_reports_nothing_before_300_ms`: no report before 300 ms.

## The same report (done when)

Captured again as C1a, on the store, DOM-first as A5 captured them: `account-menu`,
`app-language`, `help`, `about`, `feedback` and the gallery case `license`. The check (a short
script over `git show HEAD~:parity/app/report.json` and the new report) printed:

```text
rows: 130; rows whose by/backend/Rust DOM hash changed: 6 ['about', 'account-menu', 'app-language', 'feedback', 'help', 'license']
rows that differ in anything else: 0 []
totals equal: True
baseline equal: True
rust section equal but its build id: True (71e4a7c9e82fb4cb -> a1a0367dbfc7b70e)
```

The Rust DOM hashes moved only because the UI build id in the logo's `/ui/<build>/` link changed
(tc-ui changed after A5's captures): each DOM file equals the committed one with the build id put
back, and equals byte for byte a capture of the same build on the memory backend
(`--backend memory --out <scratch>`). The three Rust behaviour recordings, re-recorded on the
store, differ only in that build id. `env-rust.test.mjs` and every journey ran on the store with
`TC4_UI_HOOKS` set: 54 passed, none skipped.

## Crash coverage

- App data: a crash at every write point of a prefs file, with and without a power cut, leaves
  the old bytes or the new (`app_data.rs`, from the earlier C1a commit).
- The scenario writer: a crash at each of the 153 write points of writing `sample`, a stopped
  write torn at every other one, leaves the whole project or nothing in the projects folder once
  the next start's cleanup has run; a placed project opens with every record.

## Storage requests

- **R28 first.** tc-store's cloud check takes any ancestor holding `.dropbox` for a Dropbox
  folder, and Dropbox's settings folder `~/.dropbox` is an ancestor of every folder under the
  home (`crates/tc-store/src/platform.rs`, `cloud_client`, the ancestors loop), and the open uses
  that check for every project (`open.rs`, O5). On a Linux or macOS computer with Dropbox
  installed, every project opens read-only "copied by Dropbox" today (risk 23). tc-app's
  `folders.rs` keeps the right rule for the first launch.
- R27: the creation pins at the first open (tC4 #766) need a new `SystemReason` and a write in
  tc-store's open; `open_writes_creation_pins_only_when_none_exist` fails today exactly there
  ("Primary is pinned at the first open") and is ignored until R27 lands.
- R29: a public `project.json` writer (the scenario writer writes the four fields itself).
- R30: a non-blocking `ProjectHandle::is_idle()` for `test/quiet`.
