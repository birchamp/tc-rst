# Plan for design unit C1a: moving the app onto the store

Planner's revision 1, 2026-10-19. Commit this file with your first commit; it is the only file the
planner wrote.

Worktree `/home/bot/tc-rst/.claude/worktrees/app-c1a`, branch `app-c1a`. A previous planner had
created the worktree at `15db5ce` and stopped with nothing written; this planner fast-forwarded it
to the integration branch `app` at `4f20f52` (the merge of A5, A4b, A7 and the tC4 sweep). Nothing
else was changed. Before you start: `git -C /home/bot/tc-rst log --oneline -3 app`; if `app` moved,
`git merge app` first and re-read anything below that the merge touched. **Look in particular for
B2 and B3**: `grep -E '"B2"|"B3"' parity/app/units.json`. They are being built at the same time as
C1a (worktree `app-b2` exists), and step 11 depends on them (§1 D7, §2).

The unit's row (docs/APP-DESIGN.md §10, phase 2):

> C1a Move the app onto the store *(A-17)* | app data on `atomic_write`; persisted summaries;
> parity projects on disk through S1's create and sealing API; the parity environment switched to
> the store | C0 | — | `home_costs_projects_not_history` on the store *(A-25)*; J15 on real
> `FaultFs` delays | every phase-1 screen captured so far is recaptured on `store` with the same
> report; `MemoryBackend` is deleted when the last B unit has moved

**No owner question gates this row.** Its "Waits for" is empty. The tC4 items that name C1a
(§7) need storage requests (R27 to R30, new, §1 D11 and §6); each has a fallback, so nothing
waits. The final answer has no OWNER QUESTION section.

**`MemoryBackend` is not deleted in C1a.** The row deletes it "when the last B unit has moved";
B2 to B11 have not landed, and every one of them, and most of tc-app's tests (`Parts::new`), still
build on it. C1a keeps it and its feature and moves the parity environment's default to the store.

**Never edit** `crates/tc-store`, `crates/tc-journal`, `docs/STORAGE.md`, branch `storage` or
`.claude/worktrees/storage`, or any other worktree. A storage defect or a missing storage API is a
request to the storage team (§6, the evidence file and your final answer), never a fix here. The
one exception C0 made (a one-line clippy fix CI forced) does not apply to anything in this plan.

What the planner read (re-read the parts a step cites): AGENTS.md; docs/PLAN.md;
docs/ARCHITECTURE.md; docs/PERFORMANCE.md (rules 1 to 4, the budgets, "Start-up");
docs/APP-DESIGN.md "Owner's answers (2026-10-17)", §0, §1, §2.1 (AD6, AD7, AD12, AD13, AD35,
AD36, AD38), §2.2 row 30, §4.1, §4.2 (all, with the B1b and C0 notes and the contract table),
§4.3, §4.4, §4.5, §4.6, §4.9, §5.1, §5.8, §7.1, §7.2, §7.3, §7.5, §7.6, §7.7, §8.1, §8.2, §9.1 to
§9.3, §10 (introduction, the A5, A6b, A9, B1a, B1b, B2, B3, C0, C1a, C1b, C2, C6 rows and the
order paragraph), §11 risks 2, 11, 13, 22, §12, RT-5, RT-17, RT-25 and "How each point was
answered"; docs/TC4-ISSUES.md (the C1a items and §1); docs/TC4-DELTA-2026-10-19.md §1, §2 C36,
§3.1, §3.3, §3.8, §3.10, §3.12.4, §4, §5; `.audit/plans/C0.md` (§2, the handoff list at its end,
traps 2 to 4), `.audit/plans/A3.md` (the folders.rs and atomic-helper notes),
`.audit/plans/A9.md` T20, `.audit/workflows/app-layer.js` (C1a runs after C0 only, beside B2).
Code: tc-app `app.rs` (all), `config.rs`, `prefs.rs` (all), `reports.rs` (all), `folders.rs`,
`feedback.rs`, `hooks.rs` (holds, the atomic writer, `Quiet`, `Reset`), `handlers.rs`
(`SetLocale`), `session.rs` (open, `close_all`, `busy`), `jobs.rs` (`ProgressSink`),
`backend/mod.rs` (all), `backend/store.rs` (all), `backend/memory.rs` (all), `backend/scenario.rs`
(all), `backend/seed.rs` (types, `tc4_state_seed`, `build`'s tail, `seal_and_fold`),
`tests/app/common.rs`, `tests/app/store.rs` (head), `tests/app/scenario.rs` (test list),
`tests/support/store_world.rs`, `tests/store_adapter.rs` (`app_forwards_open_reports_unchanged`),
`tests/op_counts/{harness,corpus,session}.rs`, `Cargo.toml`, `clippy.toml`; tc-store `lib.rs`,
`store.rs` (all), `open.rs` (`ProjectJson`, `open_project`), `appdata.rs`, `atomic.rs`, `vfs.rs`
(the trait), `platform.rs` (all), `hooks.rs`, `counters.rs`, `handle.rs` (`wait_idle`, `close`),
`manifest.rs` (head), `fault.rs` (public API), `tests/store/corpus.rs`,
`tests/store/opcount.rs` (`teammate_comment_pack`), `tests/store/progress.rs`; tc-journal
`change.rs` (`Cause`, `SystemReason`), `command.rs` (`Command`), `pack.rs` (`PACKS_DIR`,
`PackPath`, `PackSpec`, `encode_pack`, `seal`); tc4 `src/lib.rs`, `src/args.rs`,
`tests/scenario.rs`, `tests/join.rs`, `tests/seed.rs` (what it compares), `tests/release.rs`;
tc-server `src/applock.rs` (`write_record`); tc-ui `tests/screen_record/{gate,design}.rs` (the
backend rules), `tests/screen_record/planted.rs` (the scripted-row case); parity `app/README.md`,
`env-rust.mjs`, `env-rust.test.mjs`, `capture-rust.mjs`, `behaviour.mjs` (main),
`journeys/kit.mjs`, `journeys/app-language.test.mjs`, `captures.json`, `report.json`,
`units.json`; tC4 `e2e/j15-slow-open.spec.ts`, `e2e/j08-resume.spec.ts` (head),
`src/data/journal/runtime.ts` (reports).

---

## 0. Ground rules for the builder

- Every shell command that runs cargo, rustc, node, npx or Playwright starts with `. ~/.tc-env && `.
  Use `-j 6` on every cargo command (four agents share 20 CPUs). Use only this worktree's
  `target/`. If `df -h /home` shows under 30 GB free and no rustc runs in this worktree, run
  `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py /home/bot/tc-rst/.claude/worktrees/app-c1a/target`.
  Never delete anything outside this worktree.
- Tests never touch the network. Store tests run on tc-store's `FaultFs` (in memory, with fault
  injection) unless a test says it needs a real folder (then `tempfile`, never a person's folder).
- Commit each step below as soon as it builds and its tests pass. Each commit message says what
  changed and why; a commit that changes the design text says why in the message too. End every
  commit message with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`. Do not push.
- C1a is not a ◆ unit, but it writes files in a person's app-data folder and, in fixture builds,
  project folders, so every write path below gets a crash or fault test as AGENTS.md asks.
- `tc-journal`'s `mixed_minor_versions_converge` fails about one run in 35 (§11 risk 22, the
  storage team's): rerun the workspace tests once before you read such a failure as yours.

---

## 1. Decisions the planner made, with the reason for each

Each becomes a D-n in the *(C1a: built …)* note of the row (step 13). Where a decision changes what
the design text says, step 13 changes the text and the commit says why.

- **D1. Scenario projects reach the disk through tc-store's staging and placing, with the seed's
  own packs, not through `Store::create`.** `Store::create` (`crates/tc-store/src/store.rs`,
  `build`) gives the project a random id and writes `NewProject.records` as one batch of this
  computer's device with `cause: Seed` and the clock's time. A decision's projected
  `modifiedTimestamp` is the time of its newest change (`crates/tc-journal/src/fold/project.rs`,
  `m.str("modifiedTimestamp", &rfc3339(newest))`), so `create` would rewrite every decision's
  time to the moment of the run, the screens and §13.2's field-for-field check would no longer be
  tC4's, and `prefs/<projectId>.json` would change name every run. So the writer does what an
  import does and what tc-store's own corpus tests do (`crates/tc-store/tests/store/corpus.rs`,
  `put_corpus`): `Store::staging(StagingKind::Import)` (which waits for the start cleanup, #746),
  the fixed files, the packs `tc4_state_seed` sealed (B1a's `seal_and_fold`, the same changes,
  the same `Importer("tc4-seed")` device, the same project id `fixture_project_id`), the carried
  files, `Store::place` (no replace), then one open at the final path, which materializes the
  derived files and mints this computer's device, and a close. Every file is written with
  `tc_store::atomic_write` and `atomic_create_dirs` through the store's own `Vfs`. This is what
  the row's "S1's create and sealing API" can mean on main: R2's staging and placing, and
  tc-journal's sealing. `project.json` is the one file whose writer tc-store keeps private
  (`ProjectJson` is `pub(crate)` in `open.rs`), so the writer serializes its four fields itself,
  the same four C0's listing reads (`backend/store.rs`, `project_json`); R29 asks tc-store for a
  public writer (§6). The writer is fixture code (`fixture-backend`), never in a release build.
- **D2. `tc4 serve --scenario <dir>` runs on the store; `--backend memory` keeps the memory
  backend.** §5.1's usage line already reads `[--backend memory --scenario <dir>]`. On the store
  the scenario writes its projects into the `--projects` folder, so `--scenario` needs
  `--projects`, and that folder must exist and be empty: a scenario never writes into, or resets,
  a folder that holds anyone's work. The check runs before the store is built, because
  `tc_store::Store::start` creates a missing projects folder (`atomic_create_dirs`), which would
  defeat `App::start`'s rule that a named folder that does not exist stops the start.
- **D3. `test/reset` on the store writes the scenario's projects again.** C0 left
  `StoreBackend::reset` as `Ok(())` with "C1a's scenario store writes its projects again here".
  The scenario store re-writes each scenario project that was opened, or given teammates, since it
  was last written, and removes every other folder in the projects folder (a project a journey
  created). It runs on the blocking pool (it fsyncs), after `close_all` has closed every session.
- **D4. App data is files, written by one writer through `tc_store::atomic_write` on the blocking
  pool.** `DiskFiles` implements `AppDataFiles` over tc-store's `Vfs` (`StdFs` in production,
  `FaultFs` in tests); `AppDeps.files` becomes `FilesChoice { OnDisk, Given(..) }`, as C0 made
  `AppDeps.store` a `StoreChoice`, because `App::start` is the first place that knows the
  app-data folder. The writer task hands a disk write to `spawn_blocking` (the comment at
  `prefs.rs` `write_now` asks for it), and keeps `MemoryFiles`' writes inline so the paused-clock
  timing tests stay exact. `test-hooks/fields.json` moves to `tc_store::atomic_write` too (the
  `hooks.rs` module doc asks for it). `app-lock.json` stays tc-server's own writer: tc-server
  does not depend on tc-store, it is written before the app exists, and A2's plan ruled it so;
  §4.9's sentence "All files are written by one PrefsWriter task" gets that exception.
- **D5. `reports.json` keeps the last 50 reports across launches.** §4.9 lists it and
  `reports.rs` says "until unit C1a writes reports.json". tC4 keeps its reports in memory only
  (`store.lastReport`), so after a crash and a relaunch tC4's "Ask for help" attaches nothing of
  what went wrong; the rebuild's attaches the newest report, dated by its `endedAt`. Declared
  difference, recorded in §4.9, told to the owner (no answer needed).
- **D6. `folders.rs` is kept.** §4.1 and §4.9 say C1a deletes it for
  `tc_store::platform::StdPlatform::folder`. That would regress: tc-store's detection still
  calls any folder under a home that has `~/.dropbox` synced, although that folder is Dropbox's
  settings (it holds `info.json`) and marks no sync root (`crates/tc-store/src/platform.rs`,
  `cloud_client`, the ancestors loop; the same on branch `storage`); and it reads `std::env`, so
  the first-launch rule could no longer be tested for a Windows or OneDrive computer on Linux
  (`HostEnv`). Worse, tc-store's own open uses that detection for every project
  (`open.rs:611`), so on a Linux or macOS computer with Dropbox installed every project already
  opens read-only "copied by Dropbox" and no save is possible. R28 asks the storage team to fix
  the marker and take the environment as a value (§6); `folders.rs` goes when R28 lands. Add the
  read-only-with-Dropbox case as risk 23 in §11.
- **D7. Persisted summaries: C1a builds the store's half; Home's half joins whichever of B2 and
  C1a lands second.** `SummaryCache`, the cards and the `home/*` endpoints are B2's, and B2 is being
  built beside C1a (`app-layer.js`: C1a depends on C0 only, B2 on B1b, A5 and A6b). C1a builds what
  needs no Home: the summary stamp, the summary files in app data and their reset. The `HomeSummary`
  job over derived files, writing a summary at close, the cards reading the files, and
  `home_costs_projects_not_history` need B2's `Summary` and endpoints: step 11 builds them if B2
  has landed when you reach it; otherwise the design text says B2 builds them on the store (the
  A2/A3 "whichever merges second joins them" precedent of the order paragraph). The same holds
  for recapturing B2's and B3's screens (step 10).
- **D8. The summary stamp lists one flat folder, not the packs.** §4.5 says "SHA-256 over the sorted
  names of the pack and pending files plus `materialized.json`'s size and mtime" and "listing two
  directories costs what one card touches". Packs are not one directory: they live at
  `ingredients/tc/changes/<sealer>/<YYYY-MM>/<name>.ndjson` (`crates/tc-journal/src/pack.rs`,
  `PACKS_DIR`, `PackPath`), so listing them walks one folder per device and month, a cost that
  grows with the project's history, against PERFORMANCE rule 1. It is also not needed: every pack
  is an ingredient listed with its md5 in `metadata.json`, and `materialized.json` records the md5
  of every pack it lists (`crates/tc-store/src/manifest.rs`, module doc), so a seal, a
  consolidation or an intake rewrites `materialized.json`. The stamp is SHA-256 over the sorted
  names in `.tc/pending/` (one flat folder; the saves not sealed yet) and the size and mtime of
  `.tc/materialized.json`: one listing and one stat per card, at any history. §4.5 and §4.2's
  `peek` doc change accordingly.
- **D9. The gate fails a capture on the memory backend once C1a has landed.** That is the test of
  "the parity environment switched to the store": a new rule `MemoryAfterStore` in
  `crates/tc-ui/tests/screen_record/gate.rs`. Scripted captures (`memory+scripted`) keep their
  own rule (`ScriptedOverdue`) and are not failed by it. Every capture made so far (A5's five
  dialogs and the `license` case; B2's and B3's if they landed) is recaptured on the store, and the
  report must be the same except each row's `by` and `backend`.
- **D10. The operation-count tests run on the store too** (§8.1: "from C1a on, on
  `StoreBackend` too"), over tc-corpus Psalms and ten times Psalms written to a `FaultFs` as
  tc-store's `put_corpus` writes them. On the store an open folds the project, which is the
  store's budget (tc-store owns the open's counts, `tests/store/opcount.rs`), so a row whose action
  is the open checks the app's counts only; every other row also checks the fold's and the
  store's counters, after `wait_idle` (C0's trap 3: fold counters are shared with the actor's
  background work).
- **D11. #766 (creation pins at the first open) is a storage request with an ignored test.**
  Writing pins needs a new `SystemReason` (a format addition before format 1.0,
  TC4-DELTA-2026-10-19 §3.3 item 4) and a write inside tc-store's open (§3.8 item 3, which gives the
  write to tc-store). tc-app could only send `SetPins`, whose cause is `Edit` (`commands.rs`), so
  History would say the translator changed the resources: a silent misattribution. R27 asks for
  it; `open_writes_creation_pins_only_when_none_exist` is written and `#[ignore]`d until R27, as C0
  did for R26. Fallback: tC4 4.0 at `d310bf3`: nothing is written at open, and Check shows "No
  resources pinned" with its guided fix (B5a).
- **D12. Teammates in a scenario on the store are packs sealed by each teammate's device** (AD36,
  "the store's sealing API, from C1a"), written into a project while no session holds it. S1's
  only intake while a project is open is a second writer, which turns it read-only, so a teammate
  pack is never dropped into an open project. B10 and `test/writeTeammate` use it later.
- **D13. `test/hold` and `test/quiet` see the store** (C0's handoff list): `test/hold`'s five
  points arm tc-store's `Hooks` too (`Save` = `BeforeDurablePending`, `BookRead` =
  `BeforeBookRead`, `Checkpoint` = `BeforeCheckpoint`, `Materialize` = `BeforeMaterialize`,
  `DraftWrite` = `BeforeDraftWrite`), and `test/quiet` is busy while a store command is in the
  adapter's flight count. tc-store has no non-blocking "idle" (only the blocking `wait_idle`, which
  also flushes drafts), so its background work after an open stays unseen: R30 asks for
  `ProjectHandle::is_idle`. It changes nothing a C1a capture shows: the writer has already
  materialized and checked each scenario project, so a later open finds every file clean.

---

## 2. What C1a depends on, and what to check before each step

| Needs | State at `4f20f52` | What it means for you |
|---|---|---|
| C0 (`StoreBackend`, contract suite, `test/saveVerse`) | merged | everything below builds on it |
| A5 (the app's frame; 5 DOM captures and the `license` case on `memory`) | merged | step 10 recaptures them on the store |
| B2 (Home, `SummaryCache`, `home/*`, `peek`) | **not merged** (worktree `app-b2` at the same base) | step 11 only if `units.json` lists B2; otherwise D7's text |
| B3 (open job, overlay, Understand) | not merged | J15's UI journey and the `understand-*` recaptures are B3's on the store (step 10 note) |
| R27, R28, R29, R30 | not on main | fallbacks in D11, D6, D1, D13 |

`git -C /home/bot/tc-rst log --oneline -5 app` at the start, before step 10 and before your last
commit. If `app` moved, merge it (`git merge app`), resolve, rebuild, rerun the tests, and redo
step 10's recapture if the merge brought captures (B2 or B3 landed): the recapture and the
`units.json` line must be in the same commit as the captures they cover.

---

## 3. Steps

### Step 1. Commit the plan

`git add .audit/plans/C1a.md && git commit` ("C1a: the plan").

### Step 2. App data on disk through `tc_store::atomic_write` (D4, #525)

Files: `crates/tc-app/src/prefs.rs`, `crates/tc-app/src/app.rs`, `crates/tc-app/src/lib.rs`
(re-export `FilesChoice`), `crates/tc-app/src/hooks.rs`, `crates/tc-app/tests/app/common.rs`,
`crates/tc-app/examples/memory_bench.rs`, `crates/tc-app/examples/store_adapter_bench.rs`,
new `crates/tc-app/tests/app/app_data.rs` (add `mod app_data;` to `tests/app.rs`),
`crates/tc4/tests/join.rs`.

1. In `prefs.rs`, change the seam:

   ```rust
   /// How far a write got (tc-store's `WriteError`): in place and flushed, or in place with the
   /// disk's error after the rename, which a power cut could still undo.
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub enum Written { Durable, NotDurable(String) }

   pub trait AppDataFiles: Send + Sync + 'static {
       fn read(&self, path: &AppDataPath) -> Result<Option<Vec<u8>>, AppDataError>;
       fn write(&self, path: &AppDataPath, bytes: &[u8]) -> Result<Written, AppDataError>;
       fn remove(&self, path: &AppDataPath) -> Result<(), AppDataError>;
       /// Whether a write waits on a disk, so the writer runs it on the blocking pool.
       fn blocks(&self) -> bool;
   }
   ```

   `MemoryFiles`: `write` returns `Written::Durable`, `remove` drops the entry (and fails while
   `fail_writes` is on), `blocks` is `false`. Keep every existing `MemoryFiles` method.
2. Add `DiskFiles` beside `MemoryFiles`:

   ```rust
   /// The app-data folder on a disk: each file replaced atomically and durably by
   /// `tc_store::atomic_write` (AGENTS.md), through tc-store's `Vfs`.
   pub struct DiskFiles { vfs: Arc<dyn tc_store::Vfs>, root: PathBuf }
   impl DiskFiles {
       /// Removes what a crash left of this writer's temporary files (`.~<name>.<hex>.tmp`
       /// beside `prefs/*`, `summaries/*` and `reports.json`), and nothing else.
       pub fn open(vfs: Arc<dyn tc_store::Vfs>, root: &Path) -> Self;
   }
   ```

   - The path of an `AppDataPath` is built by components (`root.join(part)` for each `/`-separated
     part), never by joining the text, so Windows gets its own separators.
   - `read`: `vfs.read(path)`; not found is `Ok(None)` (`tc_store::vfs::is_not_found` is
     crate-private in tc-store: match `std::io::ErrorKind::NotFound`).
   - `write`: `atomic_create_dirs(vfs, parent)` then `atomic_write(vfs, parent, name, bytes,
     SyncDir::Now)`; `WriteError::NotWritten` is `Err`, `WriteError::WrittenNotDurable` is
     `Ok(Written::NotDurable(reason))`.
   - `remove`: `tc_store::atomic_remove(vfs, path, SyncDir::Now)`; not found is `Ok(())`.
   - `open`'s cleanup: list `prefs/`, `summaries/` and the root; remove an entry only when
     `tc_store::atomic::temp_target(name)` names `reports.json` (root) or any name (in `prefs/` and
     `summaries/`, which only this writer writes). The root also holds tc-store's
     `installation.json` and `ledger.json`, written by the same helper: never touch their
     temporary files.
   - `blocks` is `true`.
3. `AppDataPath` gains `reports()` = `reports.json` (step 3) and `summary(folder: &ProjectRef)` =
   `summaries/<32 hex>.json`, the first 16 bytes of SHA-256 of the folder's name (step 8).
4. The writer task (`Task`, `prefs.rs:694` onward) becomes asynchronous where it writes:
   `async fn handle`, `async fn write_now`, `async fn write_pending`, `async fn save_locale`,
   `async fn reset`. One helper does every write:

   ```rust
   /// Runs the write where it may wait: the blocking pool for a disk, inline for memory. The
   /// task awaits it, so writes still never overlap and each takes the document as it is now.
   async fn write_file(shared: &Arc<Shared>, path: AppDataPath, bytes: Vec<u8>) -> Result<Written, AppDataError>
   ```

   Snapshot the bytes under the `docs` lock, release the lock, then await the write. A
   `Written::NotDurable` keeps the value as written and adds a `PrefsWrite` report ("written, but
   the disk reported an error after it"); for `set_locale` it is a success. `Shared::load` (the
   reads at start and a project document's first read) stays inline: the files are a few hundred
   bytes; step 12 measures it.
5. `app.rs`: `pub enum FilesChoice { OnDisk, Given(Arc<dyn AppDataFiles>) }`; `AppDeps.files:
   FilesChoice`; `AppDeps::production()` gives `FilesChoice::OnDisk` (fix its doc comment: no more
   "in-memory app data (§4.9: until unit C1a)"); `App::start` builds
   `DiskFiles::open(Arc::new(tc_store::StdFs), &config.app_data)` right after it creates the
   app-data folder, before anything adds a report (step 3 reads `reports.json` there). Update
   `tests/app/common.rs` (`try_start`: `files: FilesChoice::Given(parts.files.clone())`), the two
   examples and `tc4/src/lib.rs` (it uses `..AppDeps::production()`, so only the scenario path
   changes, in step 6).
6. `hooks.rs`: replace `write_file_atomically`, `TEMP_COUNTER` and `sync_folder` with
   `tc_store::atomic_create_dirs` + `tc_store::atomic_write(&tc_store::StdFs, &dir, FIELDS_FILE,
   &bytes, SyncDir::Now)` (a `WrittenNotDurable` counts as written, as the test fields need the
   bytes in place), still on `spawn_blocking` where it is today. Drop the module doc's "Unit C1a
   replaces the atomic write here".

Invariants: a prefs file holds the old bytes or the new, never a mixture; a file that is not a JSON
object is copied to `<file>.unreadable` before anything replaces it (unchanged rule, now on disk);
unknown members survive a rewrite; no write runs on a runtime worker; nothing in app data is
removed but this writer's own temporary files and, on `test/reset`, its summaries.

Tests (`tests/app/app_data.rs` unless named otherwise):

- `disk_files_write_atomically_and_read_back`: on `FaultFs`, write, read, overwrite, remove;
  missing reads as `None`; no `.~*.tmp` left.
- `a_crash_at_any_write_point_of_a_prefs_file_leaves_the_old_or_the_new_bytes`: for every write
  point `n` of one `set_locale` on `FaultFs` (count them with `ops()` on a dry run, then
  `kill_before(n, torn)` for `torn` in both values, then `crash(power_cut)` for both values), a
  new `PrefsWriter` over a new `DiskFiles` reads `en` or `ar`, adds no `PrefsWrite` report, and the
  next `open` removes any leftover temporary file.
- `a_held_disk_write_never_stalls_the_runtime`: a `Vfs` wrapper over `FaultFs` whose
  `create_new` waits on a `std::sync::Barrier`/`Condvar` until the test releases it; on a
  `current_thread` runtime, while `set_locale` is held in its write, a `tokio::time::sleep(10ms)`
  task still completes (a write on the runtime's only thread would freeze it; guard the test with
  a 5 s `std::thread` watchdog that fails instead of hanging).
- `leftover_temporary_files_are_removed_and_others_are_not`: plant `prefs/.~app.json.<16 hex>.tmp`,
  `.~reports.json.<16 hex>.tmp`, `.~installation.json.<16 hex>.tmp` and `prefs/notes.txt`; after
  `DiskFiles::open` only the first two are gone.
- `the_saved_app_language_survives_a_relaunch` (#525): `TestApp` with `FilesChoice::OnDisk`
  (add a `Parts`-free helper or set `deps.files = FilesChoice::OnDisk` in the edit closure),
  `prefs/setLocale ar`, `App::quit`, a second app over the same folder: `app/hello`'s
  `prefs.appLocale` is `ar`, and `prefs/app.json` on disk is a JSON object with `"appLocale":"ar"`.
- `place_and_the_resume_record_survive_a_relaunch` (J8's resume half, §7.7): record a place and a
  `LastEdit`, `quit` (which flushes), restart: `prefs.place(..)` and `prefs.last_edit()` give
  them back.
- `unknown_members_survive_a_rewrite_on_disk`: seed `prefs/app.json` with an unknown member and a
  bad `appLocale`, set the language: the unknown member is still there, in place.
- `crates/tc4/tests/join.rs` `tc4_serve_answers_hello_with_real_state`: the second launch also
  asserts `again["prefs"]["appLocale"] == "pt-BR"` (#525 on the real binary). Rename nothing.
- Every existing `tests/app/prefs.rs` test passes unchanged on `MemoryFiles`.

Commit: "tc-app: app data on disk, written by tc_store::atomic_write off the runtime (C1a)".

### Step 3. `reports.json` (D5)

Files: `crates/tc-app/src/reports.rs`, `crates/tc-app/src/prefs.rs`, `crates/tc-app/src/app.rs`,
`crates/tc-app/src/hooks.rs` (reset), `crates/tc-app/tests/app/reports.rs`.

1. `OperationReport` and `ReportOutcome` derive `Serialize, Deserialize`: camelCase fields;
   `ReportOutcome` as `#[serde(tag = "type", content = "value", rename_all = "camelCase",
   rename_all_fields = "camelCase")]` (`{"type":"ok"}`, `{"type":"refused","value":{"code":…,"key":…}}`,
   `{"type":"failed","value":{"reason":…}}`); `ReportOp` gets `Deserialize` with its kebab-case
   names. The file is `{"schemaVersion":1,"reports":[…]}`, oldest first, at most 50.
2. `ReportLog::with_kept(kept: Vec<OperationReport>) -> Self`: keeps the newest 50; the next id is
   one more than the highest kept id, so an id is never given twice, across launches either.
   `ReportLog::on_change(&self, notify: impl Fn() + Send + Sync + 'static)` (once; store it in an
   `Arc<OnceLock<Box<dyn Fn() + Send + Sync>>>`): `add` and `clear` call it after they release
   their lock.
3. `prefs.rs`: `pub fn read_reports(files: &dyn AppDataFiles) -> (Vec<OperationReport>,
   Option<String>)`: no file is an empty list; a file that is not `{"reports":[…]}` is copied to
   `reports.json.unreadable` (unless one is there already) and gives the problem; an entry this
   build cannot read (an `op` from a newer build) is left out. `App::start` calls it first and
   builds `ReportLog::with_kept(..)`, then adds the problem as a `PrefsWrite` report. Reading after
   the start's first reports were added would number them twice.
4. `PrefsWriter::start` registers `reports.on_change` to send `Msg::ReportsChanged`; the task keeps
   `Target::Reports` pending, due `REPORTS_WRITE_AFTER = 1 s` after the first change it has not
   written (keep the earlier due time, so a stream of reports still writes once a second), and
   `flush` (quit, leave) writes it at once. The bytes are `reports.all()` taken when the write runs.
5. `test/reset` already calls `reports.clear()`; the change hook writes the empty list.

Tests (`tests/app/reports.rs`):

- `reports_survive_a_relaunch_and_ids_continue`: on disk, an `app/clientError` adds a report, quit,
  restart: `feedback/compose` attaches that report (its `endedAt` is the first run's), and a new
  report's id is higher.
- `reports_are_written_a_second_after_the_first_change_and_at_quit` (paused clock, `MemoryFiles`):
  nothing at 999 ms, one write at 1 s, a second report written at quit.
- `an_unreadable_reports_file_is_kept_aside_and_reported`.
- `a_reset_empties_the_reports_file`.

Commit: "tc-app: the last 50 reports survive a relaunch in reports.json (C1a)". The message says this
is a declared difference from tC4 (D5).

### Step 4. The seed hands out its packs (D1)

File: `crates/tc-app/src/backend/seed.rs` (fixture-backend).

- `seal_and_fold` returns `(FoldState, Vec<tc_journal::pack::EncodedPack>)`; `Built` and
  `StateSeed` gain `pub packs: Vec<EncodedPack>` (the packs as sealed and read back) and
  `pub created: WallTime` (the `project_at` the packs were sealed with: `meta.dateCreated`, else
  tC4's floor). No other change: the seed's changes, ids, fold and check stay byte for byte (B1a's
  tests and `parity/app/state.sha256.json` prove it).
- Test (`backend/seed/tests.rs`): `the_seed_packs_hold_exactly_its_changes`: reading `packs` back
  with `read_pack` gives every change of `batches` once, and nothing else.

Commit with step 5.

### Step 5. The scenario store: projects on disk, reset and teammates (D1, D3, D12, #746)

New file `crates/tc-app/src/backend/scenario_store.rs` (`#[cfg(feature = "fixture-backend")] pub
mod scenario_store;` in `backend/mod.rs`); changes in `backend/store.rs` (two accessors) and
`backend/scenario.rs` (new `ScenarioError` variants).

```rust
/// A parity scenario's projects on disk, through tc-store (APP-DESIGN §4.2, C1a): what
/// `tc4 serve --scenario` runs on, and `test/reset` puts back.
pub struct ScenarioStore {
    backend: StoreBackend,
    projects: Vec<WrittenProject>,                 // by folder name
    remotes: BTreeMap<ProjectRef, Door43Remote>,   // the shared scenario's, for B10
    refused: Vec<(ProjectRef, SeedRefusal)>,
    touched: Mutex<BTreeSet<ProjectRef>>,          // opened or given teammates since written
    teammates: Mutex<BTreeMap<ProjectRef, Teammates>>,
}
struct WrittenProject {
    folder: ProjectRef,
    id: tc_journal::ProjectId,
    kind: ProjectKind,
    created: WallTime,
    packs: Vec<EncodedPack>,
    carried: Vec<(String, Vec<u8>)>,               // repository path and bytes
}
impl ScenarioStore {
    /// Seeds every project of `state` (B1a's check), writes those that pass into the empty
    /// projects folder of `backend`, and reports the others, which are not written.
    pub fn write(backend: StoreBackend, state: &ScenarioState) -> Result<(Self, ScenarioReport), ScenarioError>;
    pub fn backend(&self) -> &StoreBackend;
    pub fn remote(&self, folder: &ProjectRef) -> Option<&Door43Remote>;
    /// Teammates' edits of `key` (`memory::teammates::Teammates::edit_verse`), one pack per
    /// teammate sealed by the teammate's own device, written while no session holds the project.
    pub fn teammates_edit_verse(&self, folder: &ProjectRef, key: &RecordKey, texts: &[&str]) -> Result<Vec<ChangeId>, ScenarioError>;
}
impl ProjectStore for ScenarioStore { /* list, open: the backend's (open marks `touched`); reset (test-hooks) */ }
```

`StoreBackend` gains, under `#[cfg(any(feature = "test-hooks", feature = "fixture-backend"))]`,
`pub fn vfs(&self) -> &Arc<dyn tc_store::Vfs>` (writes go through the store's own file system, so
the tests run on `FaultFs`) and `pub fn project_path(&self, folder: &ProjectRef) -> PathBuf`.

**Writing one project** (`fn write_project(backend: &StoreBackend, p: &WrittenProject) ->
Result<PathBuf, ScenarioError>`):

1. `let staging = backend.store().staging(StagingKind::Import);` then `vfs.create_dir(&staging)`.
2. Through `atomic_create_dirs` + `atomic_write(.., SyncDir::Now)`, each relative to the staging
   folder: `.gitignore` (`tc_store::layout::GITIGNORE_BYTES`), `.gitattributes`
   (`GITATTRIBUTES_BYTES`), `tc_store::layout::FORMAT_JSON` (`FormatDoc::current().to_bytes()`),
   `tc_store::layout::PROJECT_JSON` (below), each pack at `pack.path.to_string()`, each carried
   file at its repository path. `project.json` is `serde_json::to_vec` of
   `{"schemaVersion":1,"projectId":"<id>","kind":"bible"|"obs","created":"<rfc3339>"}` in that key
   order plus `\n`, `created` from `tc_store::appdata::rfc3339(p.created)` (the bytes
   `Store::create` would write for the same values).
3. `backend.store().place(&staging, p.folder.as_str())` (it removes the staging folder itself on a
   failure). A failure before `place` removes the staging folder.
4. `backend.store().open(&final, &OpenOptions::default())`; a read-only mode is
   `ScenarioError::ReadOnly { folder, reason }` (a projects folder on FAT, in a synced folder, or
   R28's Dropbox case: say which); then, as tc-store's own `put_corpus` callers do after an
   import's first open, `checkpoint(tc_store::CheckpointReason::AfterImport).wait()`,
   `wait_idle()`, `close()`.
5. Validate the folder as a Scripture Burrito (P5): read every file outside `.tc/` through the
   `Vfs`, `tc_burrito::validate_project(metadata, &MemProject)`; problems are
   `ScenarioError::NotABurrito { folder, problems }` and the start fails. (B1a's check does not
   validate `metadata.json`; this does.)

**`write`**: the projects folder (`backend.projects()`) must hold no entry
(`ScenarioError::ProjectsFolderNotEmpty { path }`); then for each `state.projects()`: the seed as
`MemoryBackend::from_scenario` does (`tc4_state_seed(&project.tc4()?, fixture_project_id(name))`);
a refusal is listed and nothing is written; a seed writes. `carried`: for each
`seed.fold.carried_files()` placement, the bytes `seed.carried[oid]`. The report is the same
`ScenarioReport` the memory backend gives.

**`reset`** (`#[cfg(feature = "test-hooks")]`): every session is closed (the registry's
`close_all` ran first, and its future resolves only after each `close().await`, `session.rs`
`close_all`). Then: every entry of the projects folder that is no written project's folder and
does not start with `.staging-` is removed (`vfs.remove_dir_all`; on a link it removes the link,
never what it points to); every written project in `touched`, or whose folder is gone, is removed
and written again; `touched` and `teammates` are cleared. A project not touched is not rewritten
(its device, fold cache and files stay). Errors are `StoreError::Other` with the folder named.

**`teammates_edit_verse`**: `backend.store().open(path, ..)` (a session holding the project makes
it `LockHeld`: return `ScenarioError::Open { folder }`, nothing written); take the record's heads
from that handle's snapshot (`Teammates::edit_verse(handle.snapshot().fold(), key, texts)`, the
memory backend's generator, `backend/memory.rs` `pub mod teammates`; keep it there and call it
from here, both are `fixture-backend`); `wait_idle`, `close`; then for each change
`encode_pack(&PackSpec { project, sealer: change.body().device, sealed: change.body().at },
&[Verified::from(change)], &[])` written at `<folder>/<pack.path>`; mark `touched`.

`ScenarioError` (in `scenario.rs`) gains `ProjectsFolderNotEmpty { path: String }`,
`Write { folder: String, problem: String }`, `ReadOnly { folder: String, reason: String }`,
`NotABurrito { folder: String, problems: Vec<String> }`, `Open { folder: String }`,
`Teammates(String)`.

Invariants: the writer writes only inside a `Store::staging` folder or, for teammates, into a
closed project of the scenario; it never writes a project into a non-empty projects folder; reset
removes only inside that folder; every file goes through `tc_store::atomic_write`; the packs are
the seed's, never hand-made (AD36); a refused project shows nowhere (AD7, as on memory).

Tests (new `crates/tc-app/tests/app/store_scenario.rs`, `mod store_scenario;` in `tests/app.rs`;
a helper `scenario_on(fs: &FaultFs, name: &str) -> ScenarioStore` builds a `StoreBackend` with
`store_world::backend_over` and calls `write`):

- `every_scenario_writes_its_bible_project_and_refuses_its_obs_project`: for `sample`, `en-pins`,
  `shared`: the listing holds `sample_burrito` only; `sample_burrito_obs` is refused with the 53
  mismatches (same as `every_scenario_loads_its_bible_project_and_refuses_its_obs_project`).
- `the_store_scenario_holds_exactly_the_seed` (the "same seed"): open through the seam; for every
  key of the memory backend's fold of the same scenario (tests may call `record_keys`), the store
  fold's heads are the same change ids and `projected` is equal; the store fold has no key the
  memory fold lacks; `project.json`'s id is `fixture_project_id("sample_burrito")`.
- `memory_and_store_scenarios_read_alike`: for every book and chapter, `chapter_usfm` is equal on
  both backends.
- `the_scenario_project_on_disk_holds_the_fixture_bytes`: every `ingredients/*.usfm` and
  `ingredients/vrs.json` in the folder equals the tC4 fixture's bytes; every other derived file
  equals `seed.fold.render(..)`; the folder validates as a Burrito.
- `a_scenario_needs_an_empty_projects_folder`: a folder holding a project refuses with its path,
  and every byte under it is unchanged afterwards.
- `a_scenario_written_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` (#746):
  hold `Point::BeforeStagingRemoval`, plant a `.staging-import-<16 hex>` folder before the store
  starts, start `ScenarioStore::write` on a thread, wait until the cleanup reached its hold,
  release: the old staging folder is gone (and reported by `removed_at_start` with kind `Import`),
  the scenario's project is in place and valid.
- `a_reset_puts_back_a_changed_project_and_removes_others` (through `App::call`): save Titus 1:1
  through `test/saveVerse`, make another folder in the projects folder, `test/reset`: Titus 1:1
  reads the seed's text, the extra folder is gone, the listing is the scenario's.
- `a_reset_rewrites_no_project_nobody_opened`: `.tc/device.json` of an untouched project keeps its
  bytes across a reset.
- `teammates_written_into_a_closed_project_stand_as_a_conflict`: two teammates on Titus 1:1; the
  next open shows the record in conflict with two heads, by `teammates::device(1)` and `(2)`, each
  in a pack sealed by its own device; the project opens writable; the folder validates.
- `teammates_are_refused_while_a_session_holds_the_project`: nothing is written.
- `a_crash_while_a_scenario_project_is_written_leaves_no_half_project`: `FaultFs::kill_before(n)`
  for each write point of one `write_project` (dry run for the count): after `crash(false)` and a
  new store's start cleanup, the projects folder holds the whole project or nothing (a staging
  folder the cleanup removes is "nothing").

Commit: "tc-app: a parity scenario's projects on disk through tc-store's staging and the seed's packs,
put back by test/reset (C1a)".

### Step 6. `tc4 serve --scenario` on the store (D2)

Files: `crates/tc4/src/args.rs`, `crates/tc4/src/lib.rs`, `crates/tc4/tests/scenario.rs`,
`crates/tc4/tests/release.rs`, `crates/tc-app/src/backend/mod.rs` if needed.

- `Args` gains `#[cfg(feature = "fixture-backend")] pub backend: ScenarioBackend` (`enum
  ScenarioBackend { Store, Memory }`, default `Store`). `--backend memory` sets `Memory`; any other
  value is `ArgsError::BadValue { flag: "--backend", reason }`; `--backend` without `--scenario` is
  `BadValue { flag: "--backend", reason: "needs --scenario" }`; `--scenario` on the store without
  `--projects` is `BadValue { flag: "--scenario", reason: "needs --projects: the scenario writes its
  projects there" }`. `fixture_flags!()` becomes `" [--scenario <state dir> [--backend memory]]"`.
- `scenario_deps(args: &Args) -> Result<AppDeps, String>`: read the scenario first (so a missing
  scenario folder is still the error that names it); for `Memory` as today; for `Store`: the
  `--projects` folder must be a folder and empty (else the start fails with its path), then
  `StoreBackend::on_disk(&args.app_data, projects)`, `ScenarioStore::write`, the same stderr lines
  as today (`loaded {folder} ({n} declared differences)`, `refused …`, `{n} resources
  installed`) plus `tc4: scenario {at}: {n} projects written to {projects}`, and
  `store: StoreChoice::Given(Arc::new(scenario_store))`. Install the resources as today.
- `tests/scenario.rs`: the existing test becomes
  `tc4_serve_with_a_scenario_writes_its_projects_to_the_store`: the projects folder holds exactly
  `sample_burrito`, which validates as a Burrito and whose `ingredients/TIT.usfm` equals the
  fixture's bytes; the stderr lines as today. New
  `tc4_serve_with_a_scenario_on_the_memory_backend_writes_no_project` (`--backend memory`) keeps
  today's "0 entries" assertion. `a_missing_scenario_folder_stops_the_start_with_its_path` passes
  `--projects <empty dir>` and keeps its assertions. New
  `a_scenario_refuses_a_projects_folder_that_holds_anything`: exit code 4, one error line naming
  the folder, the folder's file unchanged.
- `args.rs` unit tests: the three refusals above, `--backend memory` parses, and a build without
  the feature knows no `--backend` (extend `a_build_without_the_fixture_backend_knows_no_scenario`).
- `tests/release.rs`: `HOOK_TEXT` gains `"--backend"` (the array length becomes 14); run the
  ignored release test once (§9).

Commit: "tc4: --scenario writes its projects to the store; --backend memory keeps the memory backend
(C1a)".

### Step 7. `test/hold` and `test/quiet` see the store (D13)

Files: `crates/tc-app/src/backend/mod.rs`, `backend/store.rs`, `backend/scenario_store.rs`,
`backend/memory.rs`, `crates/tc-app/src/session.rs`, `crates/tc-app/src/hooks.rs`.

- `ProjectStore` gains `#[cfg(feature = "test-hooks")] fn hooks(&self) -> Option<Arc<tc_store::Hooks>>
  { None }`; `StoreBackend` and `ScenarioStore` return their hooks.
- `ProjectHandle` gains `#[cfg(feature = "test-hooks")] fn busy(&self) -> bool { false }`;
  `StoreHandle` answers `in_flight > 0` from its `Origins` (`backend/store.rs`, `OriginState`).
- `SessionRegistry::busy()` also asks each open session's handle (test-hooks only).
- `test/hold { at }` arms the app's hold as today and, when the store has hooks, `hooks.hold(point)`
  for the mapped point (D13); `test/release` releases both; `test/reset` releases every store
  point too (`Holds::release_all` plus the five points). `holding()` adds the store points held
  with `reached(point) > 0`.
- Tests (`tests/app/store_scenario.rs`): `a_held_store_save_holds_the_reply_and_quiet_says_busy`
  (hold `save`, send `test/saveVerse`, `test/quiet` is busy and the write is not durable yet,
  release: the reply comes and quiet is idle); `a_reset_releases_the_store_holds`.

Commit: "tc-app: test/hold arms tc-store's hold points and test/quiet counts store commands in flight
(C1a)".

### Step 8. The summary stamp and summary files (D7, D8)

Files: `crates/tc-app/src/backend/mod.rs` (`SummaryStamp`), `backend/store.rs`,
`backend/scenario_store.rs` (delegates), `crates/tc-app/src/prefs.rs`.

```rust
/// What a Home card's summary was computed from (§4.5, C1a D8): the names of the saves not sealed
/// yet (`.tc/pending/`) and the size and mtime of `.tc/materialized.json`. One listing and one stat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SummaryStamp([u8; 32]);
impl SummaryStamp { pub fn to_hex(&self) -> String; pub fn parse_hex(text: &str) -> Option<Self>; }
impl StoreBackend { pub fn summary_stamp(&self, folder: &ProjectRef) -> Result<SummaryStamp, StoreError>; }
```

SHA-256 (ring) over `b"tc4-summary-stamp\0"`, then each name of `.tc/pending/` in byte order that
is not a temporary file (`temp_target`), each followed by `\0`, then `\x01` + size (u64 LE) + mtime
(i64 LE) of the manifest, or `\x00` when it is missing. Inherent on `StoreBackend` (not a trait
method: the memory backend writes no files; B2's `peek` takes it in).

`PrefsWriter` keeps summary files:

```rust
pub struct StoredSummary { pub project: ProjectId, pub stamp: SummaryStamp, pub summary: serde_json::Value }
impl PrefsWriter {
    /// The summary kept for `folder`, read on the first ask; `None` when there is none, it cannot be
    /// read, or it names another folder.
    pub fn summary(&self, folder: &ProjectRef) -> Option<StoredSummary>;
    /// Written at once.
    pub fn record_summary(&self, folder: ProjectRef, summary: StoredSummary);
}
```

File: `{"schemaVersion":1,"folder":"<name>","projectId":"<id>","stamp":"<64 hex>","summary":{…}}`.
The summary's own shape is B2's (`Summary`, §4.5); this layer stores it as JSON and B2 parses it at
its boundary. `reset` removes every summary file the writer knows (`AppDataFiles::remove`).

Tests:

- `a_summary_stamp_follows_saves_and_seals` (`tests/app/store_scenario.rs`, FaultFs): the stamp is
  the same twice; a save (a pending file) changes it; a checkpoint changes it again.
- `a_summary_stamp_reads_no_file_and_costs_the_same_at_any_history` (`tests/op_counts/`, step 9's
  store corpora): computing it opens no file (`fs.opened()` unchanged) and makes one listing and one
  stat, on Psalms and on ten times Psalms alike. `FaultFs::log()` records writes only (`OpKind`),
  so count the reads with a small test `Vfs` that wraps the `FaultFs`, counts `read_dir`,
  `metadata`, `read` and `open` calls, and delegates everything else; start the `StoreBackend`
  over it.
- `a_summary_is_kept_across_a_relaunch_and_never_used_for_another_folder` (`tests/app/app_data.rs`).
- `a_reset_removes_every_summary`.

Commit: "tc-app: the summary stamp lists one folder, and summaries are kept in app data (C1a)". The
message says why the stamp no longer lists packs (D8).

### Step 9. Operation counts on the store (D10)

Files: `crates/tc-app/tests/op_counts/{corpus,harness,session}.rs`, new
`crates/tc-app/tests/support/store_corpus.rs` (`#[path]` it into `tests/op_counts.rs`, with
`support/store_world.rs` for `backend_over` and `support/counting_store.rs` for step 11, as
`tests/store_adapter.rs` includes them).

- `corpus.rs`: keep the generated `tc_corpus::Corpus` of each spec in the static (the memory
  variant folds its lines as today; the store variant writes its files).
- `store_corpus.rs`: a port of tc-store's `put_corpus` (`crates/tc-store/tests/store/corpus.rs`):
  `project.json`, `format.json`, `.gitignore`, `.gitattributes`, the corpus's packs and pending
  files, and `.tc/device.json` so the corpus's device 0 is the folder's device
  (`tc_store::device::fingerprint(store.app_data().secret(), <the FakePlatform's machine>,
  <the folder path as the store sees it>)`; use the same machine string as the `FakePlatform`
  `backend_over` starts the store with). Then open, `checkpoint`, `wait_idle`, close, assert the
  folder validates, and keep the `FaultFs` in a `OnceLock`; each test takes `fs.copy()` (cheap: the
  file bytes are `Arc`s).
- `harness.rs`: `enum Backing { Memory, Store }`; `Scale` holds either; `costs_what_it_touches`
  runs both and names the backend in its failure. On the store: after `before`, call
  `ProjectHandle::wait_idle` on the open session before the baseline; the fold's counters come from
  the session's snapshot (`handle().snapshot().fold().counters()`); `Cost` gains
  `store: StoreCounters` (from `handle().counters()`, zero on memory) and `growth` names any store
  field that grew. A row whose action opens the project (`open_lists_no_key`) takes `OpensProject`
  and on the store checks `AppCounts` only (D10).
- Every existing row passes on both, including the planted
  `the_harness_fails_an_action_that_costs_the_project` (it must fail on the store too). If
  `bytes_read` differs between the corpora on a chapter read because the chapter's values differ
  in length (the corpora match the chapter's keys and states, not its bytes), compare
  `files_opened` and `values_read` instead for that row and say so in a comment.

Commit: "tc-app: the operation-count rows run on the store too (C1a)".

### Step 10. The parity environment on the store, and the recapture (D9)

Files: `parity/app/env-rust.mjs`, `parity/app/env-rust.test.mjs`, `parity/app/capture-rust.mjs`,
`parity/app/behaviour.mjs` (nothing, if it calls `startRustEnv` with defaults),
`parity/app/journeys/kit.mjs` (nothing, same), `parity/app/README.md`, `parity/app/units.json`,
`parity/app/captures.json`, `parity/app/screens/*`, `parity/app/dom/*`,
`parity/app/behaviour/*`, `parity/app/report.json`, `crates/tc-ui/tests/screen_record/gate.rs`,
`crates/tc-ui/tests/screen_record/planted.rs`.

1. `env-rust.mjs`: `scenarioArgs(scenario, dir, backend = 'store')` answers `['--scenario',
   state]`, plus `['--backend', 'memory']` for `memory`; `startRustEnv({ scenario, tc4, ui, backend
   = 'store', runDir })` passes it on. Update the doc comments ("held by the memory backend").
2. `capture-rust.mjs`: `--backend` defaults to `store`; the environment runs on the memory backend
   for `memory` and `memory+scripted` and on the store for `store`.
3. `env-rust.test.mjs`: the `scenarioArgs` case checks both forms.
4. Gate: `Rule::MemoryAfterStore` (doc: "A capture on the memory backend after C1a moved the parity
   environment to the store (§7.5)"); in the capture loop of `gate.rs`, when `landed` holds `C1a`
   and `capture.backend == Backend::Memory`, add it with "captured on the memory backend after C1a
   landed: capture it again on the store (`--backend store`, the default)". Planted test
   `a_memory_capture_fails_once_c1a_has_landed`: `about` on `memory` is clean, landing C1a fails it
   with exactly `[Rule::MemoryAfterStore]`, `store` is clean again, and `sync-ok` on
   `memory+scripted` with C1a landed and E1a not landed is not failed by it.
5. Build: `cargo build -p tc4 --features test-hooks,fixture-backend -j 6` and
   `crates/tc-ui/build-app.sh`. Check the machine: `captures.json`'s `machine` must equal this
   PC's fingerprint (`capture-rust.mjs` refuses to mix machines; A6b recaptured the baseline here).
6. Save the report before: `git show HEAD:parity/app/report.json > <scratchpad>/report-before.json`.
7. Recapture every capture of `captures.json` on the store, as the unit that does it:
   `node parity/app/capture-rust.mjs --unit C1a --only account-menu,app-language,help,about,feedback
   --layers dom` and `node parity/app/capture-rust.mjs --unit C1a --case license --layers dom`
   (plus every screen B2 or B3 captured, if they landed: their own layers, `--unit C1a`). Re-record
   the Rust behaviour sequences: `node parity/app/behaviour.mjs --env rust --only
   account-menu-keyboard,app-language-preview,feedback-typing`; `git diff parity/app/behaviour`
   must be empty (a recording names no backend; a difference is a real change, find it).
8. Add `"C1a"` to `parity/app/units.json` `landed`, then
   `UPDATE_REPORT=1 cargo test -p tc-ui --test screen_record -j 6`.
9. "The same report": compare `<scratchpad>/report-before.json` with the new report row by row,
   ignoring only `by` and `backend`; every other field (status, missing, every layer's result, the
   Rust DOM and PNG hashes) must be equal. Write the check as a short `python3 -I` script in the
   scratchpad and quote its output in the evidence file. A DOM hash that changed means the store
   changed what a dialog shows: find out why before you go on.
10. Run the journeys and the environment's browser tests on the store:
    `TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/env-rust.test.mjs
    parity/app/journeys/*.test.mjs` (§7.7: "memory, then store").
11. README: the env-rust row and "Capturing" say the store is the default, `--backend memory` for a
    unit that still needs the memory backend, and that the gate fails a memory capture after C1a.

Note for B3: J15's UI journey (the overlay at 300 ms) is B3's; on the store it needs slow reads of
a real disk, which `FaultFs` cannot give `tc4 serve`. C1a's J15 is the app-level test of step 12.

Commit: "parity: the Rust environment runs on the store; every capture so far taken again there with
the same report (C1a)".

### Step 11. Home on the store (only if B2 has landed; D7)

Skip it if `units.json` does not list B2 when you reach it; step 13 then writes D7 into the design
and your final answer says that B2 does this on the store. If B2 has landed:

- `peek` on the store returns the `SummaryStamp` (step 8) with the derived-file facts; a card whose
  stored summary has the current stamp uses it; otherwise the card shows "—" (B2's rule) and one
  `HomeSummary` job (one at a time, cancelled when any project opens, progress at most once a
  second, §4.6) reads the derived USFM files and `ingredients/tc/conflicts.json` with no lock and no
  fold, computes B2's `Summary` and records it (`record_summary`).
- While a project is open its summary follows each delta (B2's `SummaryCache`); it is recorded at
  the session's close.
- Tests: `home_costs_projects_not_history` (§8.2, store only, `tests/op_counts/` with the store
  corpora and nine small projects): with fresh summaries, ten cards make no `ProjectStore::open`
  (count with `tests/support/counting_store.rs`), read no pack or pending file (`fs.opened()` holds
  no path under `ingredients/tc/changes/` or `.tc/pending/`), open at most five files per card, and
  cost the same with the Psalms card and with the ten-times-Psalms card; a card whose summary file
  is removed starts exactly one `HomeSummary` job, and its next listing uses the new file.
  `summary_from_files_equals_summary_from_the_fold` on the sample and on Psalms (the job's numbers
  equal B2's numbers from a fold).
- Recapture B2's screens on the store in step 10 (`home`'s Recapture is C1a in §7.6).

### Step 12. J15 on real `FaultFs` delays, and the #766 test (D10 note, D11)

File: `crates/tc-app/tests/app/store_scenario.rs`.

- `j15_a_slow_open_reports_from_300_ms_and_counts_up`: the `sample` scenario written by
  `ScenarioStore` on a `FaultFs`; `set_read_delay(Some(60 ms))`; `open_screen` with a sink sampled
  every 2 ms on a `std::thread` (as C0's `app_forwards_open_reports_unchanged`). Assert: the open
  took over 600 ms; the first change of the sink came at 300 ms or later and by 420 ms (tc-store's
  own test allows 300-400 ms, `tests/store/progress.rs`; 20 ms more for the sampler and the
  blocking-pool hop, say so in a comment); every report has a total and `done <= total`
  (determinate); phases only move forward; counts never fall within a phase; the last report is
  at its total.
- `j15_a_quick_open_reports_nothing_before_300_ms`: no delay; if the sink changed at all, it changed
  at 300 ms or later (tC4's rule in `e2e/j15-slow-open.spec.ts`: the ruling, not the machine's
  speed, is the criterion), so a loaded machine cannot make it flaky.
- `open_writes_creation_pins_only_when_none_exist` (`#[ignore = "R27: tc-store writes the creation
  pins at the first open (TC4-DELTA-2026-10-19 §3.8 item 3)"]`): a project made with
  `Store::create` and no pin records, opened through `App`: every pin group equals
  `tc_resources::suite::project_pins()`, written in one batch whose cause is a `System` reason (not
  `Edit`), and `ingredients/checking/resources.json` lists them; a second open writes nothing; the
  sample scenario's project, which has pins, keeps `resources.json` byte for byte across an open;
  adding a book leaves the pins equal (§3.12.4 item 2).

Measurements (write `docs/evidence/app-on-store-<date>.md`, PERFORMANCE rule 5: machine, CPU flags
`grep -o -w 'sha_ni\|avx512f' /proc/cpuinfo | sort -u`, commit, median and p95 of 20 runs):

| What | How | Why |
|---|---|---|
| `prefs/setLocale` round trip on disk | `App::call` 20 times, release build (`cargo run --release --example`) | the dialog waits for it (fsync) |
| A project prefs file's first read inline | time `PrefsWriter::place` on a cold document | D4's residual |
| `ScenarioStore::write` of `sample` | release, 20 runs, split seed / write / place / first open / validate | the harness's start time |
| `test/reset` with 0 and 1 touched project | through `App::call`, 20 runs | each screen's `home()` pays it |
| `tc4 serve --scenario` to the address line, store and memory | 5 runs each | the capture run's cost |
| The six recaptures, store against memory | wall time of `capture-rust.mjs` | the same |
| `summary_stamp` on Psalms and ten times Psalms | opens, listings, stats, time | D8 |
| The count rows on the store | the `Cost` of each row on both corpora | §8.1 |
| J15 on FaultFs | first report time, report count | J15 |

Commit: "tc-app: J15 on real FaultFs delays; the creation-pins test waits for R27 (C1a)".

### Step 13. The design text, the evidence and the logs

`docs/APP-DESIGN.md` (keep every table row on one line; a `|` inside a cell is written `\|`, or
`design.rs` and `units.mjs` misread the row):

- §10 C1a row: append a *(C1a: built …)* note with D1 to D13 in plain words, the tests, the tC4
  items (#525, #746, #766) and the evidence file. In "Depends on" leave `C0`; in the note write
  D7's rule: the persisted summaries' Home half, `home_costs_projects_not_history` and the
  recapture of B2's and B3's screens are done by whichever of B2/B3 and C1a lands second.
- §4.1 module map: `prefs.rs` (`DiskFiles`, `FilesChoice`), `backend/{…,scenario_store}.rs`,
  `folders.rs` "kept until R28 (C1a D6)".
- §4.2: replace "C1a then moves the app onto the store: the parity scenario projects are written to
  disk through S1's create and sealing API with the same seed" with D1's method and reason; the
  `peek` doc comment's stamp inputs (D8); `ProjectStore::hooks` and `ProjectHandle::busy` (D13).
- §4.5: the stamp (D8) and "one listing and one stat per card".
- §4.9: "Before S1 these files live in memory" becomes what step 2 built; `reports.json` with D5's
  declared difference; the `app-lock.json` exception (D4); the `folders.rs` paragraph with D6.
- §5.1: the usage line already has `[--backend memory --scenario <dir>]`; add that `--scenario`
  needs an empty `--projects` folder (D2).
- §5.8: `test/reset` on the store (D3), `test/hold` and `test/quiet` (D13).
- §7.1 `env-rust.mjs` row and §7.2's second bullet: the store, `--backend memory`.
- §7.5: a bullet "a capture is on the memory backend after C1a has landed" (D9).
- §7.6, prose before the table: from C1a on, an "M" screen is captured on the store.
- §8.1: the count rows run on both backends (D10).
- §9.2: rows R27 to R30 (§6 of this plan), each with owner, "In a task text: no" and its fallback.
- §11: risk 23 (D6's Dropbox case).

`docs/evidence/app-on-store-<date>.md`: the measurements, the "same report" check's output, the
storage requests with their evidence.

`.audit/rebuild.tsv` and `.audit/PROGRESS.md`: one line each if earlier units added theirs (look at
how C0 did it: `git log -p -1 --format= 772fa42 -- .audit/rebuild.tsv .audit/PROGRESS.md`).

Commit: "docs: C1a's decisions in the design, its storage requests and its evidence". The message
lists the design changes with their reasons (D1, D6, D7, D8 change what the text said).

---

## 4. Tests, by what they prove

| Row or item | Test | File |
|---|---|---|
| app data on `atomic_write` | `disk_files_write_atomically_and_read_back`, `a_crash_at_any_write_point_of_a_prefs_file_leaves_the_old_or_the_new_bytes`, `a_held_disk_write_never_stalls_the_runtime`, `leftover_temporary_files_are_removed_and_others_are_not`, `unknown_members_survive_a_rewrite_on_disk` | `tests/app/app_data.rs` |
| #525 | `the_saved_app_language_survives_a_relaunch`; `tc4_serve_answers_hello_with_real_state` (second launch) | `tests/app/app_data.rs`; `crates/tc4/tests/join.rs` |
| J8's resume half | `place_and_the_resume_record_survive_a_relaunch` | `tests/app/app_data.rs` |
| `reports.json` | `reports_survive_a_relaunch_and_ids_continue`, `reports_are_written_a_second_after_the_first_change_and_at_quit`, `an_unreadable_reports_file_is_kept_aside_and_reported`, `a_reset_empties_the_reports_file` | `tests/app/reports.rs` |
| parity projects on disk | `every_scenario_writes_its_bible_project_and_refuses_its_obs_project`, `the_store_scenario_holds_exactly_the_seed`, `memory_and_store_scenarios_read_alike`, `the_scenario_project_on_disk_holds_the_fixture_bytes`, `a_scenario_needs_an_empty_projects_folder`, `a_crash_while_a_scenario_project_is_written_leaves_no_half_project`, `the_seed_packs_hold_exactly_its_changes` | `tests/app/store_scenario.rs`, `backend/seed/tests.rs` |
| #746 | `a_scenario_written_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` | `tests/app/store_scenario.rs` |
| `test/reset` on the store | `a_reset_puts_back_a_changed_project_and_removes_others`, `a_reset_rewrites_no_project_nobody_opened`, `a_reset_releases_the_store_holds` | same |
| AD36 on the store | `teammates_written_into_a_closed_project_stand_as_a_conflict`, `teammates_are_refused_while_a_session_holds_the_project` | same |
| `test/hold`, `test/quiet` | `a_held_store_save_holds_the_reply_and_quiet_says_busy` | same |
| persisted summaries (store half) | `a_summary_stamp_follows_saves_and_seals`, `a_summary_stamp_reads_no_file_and_costs_the_same_at_any_history`, `a_summary_is_kept_across_a_relaunch_and_never_used_for_another_folder`, `a_reset_removes_every_summary` | `tests/app/store_scenario.rs`, `tests/op_counts/`, `tests/app/app_data.rs` |
| `home_costs_projects_not_history` (with B2) | as step 11 | `tests/op_counts/` |
| counts on the store | every row of `tests/op_counts/session.rs` on both backends | `tests/op_counts/` |
| J15 on real `FaultFs` delays | `j15_a_slow_open_reports_from_300_ms_and_counts_up`, `j15_a_quick_open_reports_nothing_before_300_ms` | `tests/app/store_scenario.rs` |
| #766 | `open_writes_creation_pins_only_when_none_exist` (ignored until R27) | same |
| `tc4 serve --scenario` | `tc4_serve_with_a_scenario_writes_its_projects_to_the_store`, `tc4_serve_with_a_scenario_on_the_memory_backend_writes_no_project`, `a_scenario_refuses_a_projects_folder_that_holds_anything`, `a_missing_scenario_folder_stops_the_start_with_its_path`, the args cases, `HOOK_TEXT` | `crates/tc4/tests/`, `src/args.rs` |
| done when: recaptured with the same report | the gate (`MemoryAfterStore`, `a_memory_capture_fails_once_c1a_has_landed`), the committed captures, the report check of step 10.9, the behaviour recordings unchanged | `crates/tc-ui/tests/screen_record*`, `parity/app/` |

---

## 5. What does not change

- The memory backend, its feature, `test/script` and every test that uses `Parts::new()`.
- The screens: C1a changes no tc-ui source; the recaptured DOM must be identical.
- tc-api: no new endpoint or type (the `test/hold` points and `test/reset` keep their wire forms).
- `app-lock.json` (tc-server), `drafts-buffer.log` (the shell), `resources/` (tc-resources).
- `crates/tc-app/src/bundle.rs`'s manifest writer (a build-time tool, not app data).

---

## 6. Storage requests (new rows of §9.2)

| R | Need | Owner | Fallback |
|---|---|---|---|
| R27 | tC4 #766: at the first open of a project whose pin groups are all empty, write the creation pins (one `Pins` change per group from `tc_resources::suite::project_pins()`, one batch) with a new `System` reason beside `PinCoverageBackfill`, before format 1.0; a project with any pin keeps its pins byte for byte (TC4-DELTA-2026-10-19 §3.3 item 4, §3.8 item 3) | S2 or V | as tC4 at `d310bf3`: nothing written at open; Check shows "No resources pinned" and its guided fix; `open_writes_creation_pins_only_when_none_exist` is ignored until it lands |
| R28 | tc-store's cloud detection (`platform.rs`, `cloud_client`): `~/.dropbox` that holds `info.json` is Dropbox's settings folder, not a sync marker (today every project under such a home opens read-only, "copied by Dropbox"); and a way to give it the environment as a value, so the app's first-launch check can call it | S1 owner | `crates/tc-app/src/folders.rs` stays (it has the right marker rule); projects on a computer with Dropbox installed open read-only until R28 (risk 23) |
| R29 | a public writer of `ingredients/tc/project.json` (or an import entry over `Store::staging` and `Store::place`) for unit I and for C1a's scenario writer | I | the scenario writer serializes the four fields C0's listing reads; `the_store_scenario_holds_exactly_the_seed` opens what it wrote |
| R30 | `ProjectHandle::is_idle()`: no queued command, no background job, no draft write pending, without blocking and without flushing drafts (`wait_idle` does both) | S1 owner | `test/quiet` counts the adapter's commands in flight only; the open's storage check and a materialize stay unseen (nothing a capture shows depends on them today) |

---

## 7. tC4 issues and changes that name C1a

| Item | Handled how | Test |
|---|---|---|
| #525 (TC4-ISSUES §6): "C1a adds a relaunch test that the saved language survives" | app data on disk (step 2) | `the_saved_app_language_survives_a_relaunch`; the second launch of `tc4_serve_answers_hello_with_real_state` |
| #746 (§5): "C1a moves tc-app onto the store and creates, imports and joins only through `Store`" | the scenario writer goes through `Store::staging` and `Store::place` only; nothing else in tc-app writes a project folder | `a_scenario_written_while_the_start_cleanup_runs_keeps_its_staging_folder_and_finishes` |
| #766 / C36 (§4, DELTA §3.12.4): "C1a, for an existing project: when every pin group is empty at the first open, write the creation pins" | R27 (D11): needs a format addition and a write inside tc-store's open; fallback is tC4 4.0's behavior | `open_writes_creation_pins_only_when_none_exist`, ignored until R27 |

Say each of these in your final answer, with the test names and, for #766, why it waits.

---

## 8. Traps

1. **The worktree was behind `app`.** The planner fast-forwarded it; check again before each
   commit that matters (§2).
2. **B2 and B3 run beside you.** Step 11 and part of step 10 depend on them; never build a second
   `Summary` or a second Home: B2 owns them (D7).
3. **`Store::create` is the wrong tool for the scenarios** (D1): random id, `Seed` cause at the
   run's time, decisions' `modifiedTimestamp` rewritten.
4. **`ProjectJson` is private** in tc-store: write the four fields in tc-store's order; never
   change tc-store (R29).
5. **`StateSeed` had no packs**: `seal` is deterministic, but expose the packs B1a already read back
   rather than sealing twice (step 4).
6. **`Store::start` creates a missing projects folder**: the scenario path checks `--projects`
   (exists, empty) before it builds the store (D2).
7. **`ProjectStore::reset` is synchronous and called inside an async handler** (`hooks.rs`,
   `Handle<Reset>`): wrap the call in `tokio::task::spawn_blocking` (the store's reset fsyncs).
8. **The prefs writer is a tokio task**: a disk write must go to the blocking pool; `MemoryFiles`
   stays inline, or the paused-clock tests in `tests/app/prefs.rs` lose their exact times
   (tokio 1.53 stops auto-advance while a blocking task runs, but `settle()` does not wait for one).
9. **Reports added before the prefs writer starts** (`App::start` adds the projects-folder and
   keychain reports early): read `reports.json` before the first `add`, or ids repeat.
10. **`folders.rs` must not be deleted** (D6). The design says so today; step 13 changes the text.
11. **Fold counters are shared with the actor** (C0 trap 3): `wait_idle` before every counted
    section on the store, or the open's storage check counts into the action.
12. **The store's open folds the project**: `open_lists_no_key` on the store checks the app's counts
    only (D10).
13. **Packs are nested by sealer and month**: never list them for a Home card (D8).
14. **`test/quiet` does not see tc-store's background work** (R30, D13).
15. **The tc4 scenario test asserts an empty projects folder** (memory): split it (step 6).
16. **`a_missing_scenario_folder_stops_the_start_with_its_path` passes no `--projects`**: read the
    scenario before checking `--projects`, and give the test an empty `--projects`.
17. **`release.rs`'s `HOOK_TEXT`** gets `--backend`; the release test is `#[ignore]` and needs
    `TC4_RELEASE_BIN` (§9).
18. **The gate must not fail scripted rows** with `MemoryAfterStore` (D9).
19. **Captures from another machine are refused**: recapture on this PC, whose fingerprint the
    committed captures carry.
20. **`units.json` must list C1a in the same commit as the recaptures** (the gate's
    `CapturedByUnlandedUnit`), and that commit must hold no memory capture (`MemoryAfterStore`).
21. **A `|` inside a §10 cell breaks the table** for `design.rs` and `units.mjs`: write `\|`.
22. **CI's two clippy steps without test hooks** (`cargo clippy -p tc-app --lib -- -D warnings`,
    `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`) build `DiskFiles` without
    features: no `#[cfg(feature = "test-hooks")]` item may be named from it. Also build tc4 with
    `fixture-backend` alone and with `test-hooks` alone (§9): `ScenarioStore::reset` is
    test-hooks only, `--backend` is fixture-backend only.
23. **The disallowed-methods lint** (`crates/tc-app/clippy.toml`): product code in
    `scenario_store.rs` may not call `record_keys`, `render` and the other walks; tests may.
    `carried_files` is allowed (`tests/op_counts/lint.rs` lists its cost). Do not add an
    `#[expect(clippy::disallowed_methods)]` in product code: `tests/op_counts/lint.rs` fails on it.
24. **Each rewrite mints a new device** at its first open and adds a line to tc-store's
    `ledger.json` in app data: harmless in a run folder; say so in the evidence.
25. **Reset removes folders**: only inside the scenario's projects folder, which was empty at the
    start; `remove_dir_all` on a link removes the link. Never call it on a path built from anything
    but the projects folder and a listed entry name.
26. **tc-store's close waits for its background work**; `close_all` resolves after every close, so
    reset never races an actor still writing into a folder it removes. Keep it that way: do not
    drop a handle instead of closing it.
27. **Fixture-only code is still product code** in the `fixture-backend` build: every error is a
    typed `ScenarioError`, no `unwrap` outside tests.
28. **The flaky storage property** (`mixed_minor_versions_converge`, risk 22): rerun once.

---

## 9. Gates before your last commit

```sh
. ~/.tc-env && cargo fmt --all -- --check
. ~/.tc-env && cargo clippy --workspace --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo test --workspace -j 6
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-app --lib -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-server -p tc4 --lib --bins -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc4 --features fixture-backend --lib --bins -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc4 --features test-hooks --lib --bins -j 6 -- -D warnings
. ~/.tc-env && cargo build --release -p tc4 -j 6 && TC4_RELEASE_BIN=target/release/tc4 cargo test -p tc4 --test release -j 6 -- --ignored
. ~/.tc-env && cargo build -p tc4 --features test-hooks,fixture-backend -j 6 && crates/tc-ui/build-app.sh
. ~/.tc-env && node --test parity/tc4-baseline/baseline.test.mjs parity/app/state.test.mjs parity/app/observe.test.mjs parity/app/env-rust.test.mjs parity/view-gallery/view-gallery.test.mjs crates/tc-ui/app/recovery.test.mjs parity/app/journeys/*.test.mjs
. ~/.tc-env && TC4_BIN=target/debug/tc4 TC4_UI=target/tc-ui-app node --test parity/app/env-rust.test.mjs parity/app/journeys/*.test.mjs
```

(At `4f20f52` tc-ui's features are `gallery`, `app` and `test-hooks`, as CI's wasm steps list
them; there is no `harness` feature yet. If a merge adds one, add its line.)

---

## 10. Your final answer

- The commits, one line each.
- D1 to D13, each in a sentence, marking the four that change what the design said (D1, D6, D7,
  D8) and the declared difference (D5).
- The tC4 items of §7 with their tests; #766 waits for R27.
- The storage requests R27 to R30, with R28 first: on a Linux or macOS computer with Dropbox
  installed, every project opens read-only today.
- Whether step 11 ran (B2 landed) or was left to B2 by D7's rule, and which captures were taken
  again, with the "same report" check's result.
- The measurements' headline numbers and the evidence file's path.
- No OWNER QUESTION section: the row waits for no owner answer.
