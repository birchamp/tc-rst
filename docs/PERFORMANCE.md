# Performance at Bible scale

A team works in one project for years. Psalms is the largest book: 150 chapters, 2,461
verses, 8,212 translationNotes rows and 7,683 translationWords links (en_tn and en_twl v91),
and 21,600 alignment links in the ULT. A fully checked whole Bible holds about 620,000
changes after three years (STORAGE.md §14). tC4 needed 9.3 s to fold a checked whole Bible
and 41 s to read it on its open path (tC4 `docs/evidence/bench-fold-2026-08-25.md`). The
rebuild must stay quick at that size for the whole life of a project.

## Rules

1. **Cost follows what an action touches.** An interactive action costs in proportion to the
   verse, chapter, check or book it touches. It never costs in proportion to the size of the
   project, its history or the number of devices. Only the actions under "Background work"
   may cost more. tC4 shows why: it folds the whole history again at every save, which
   costs 23.81 µs per event of history, 6.0 s per save at whole-Bible scale, measured with
   tC4's own fold on its own corpus (`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md`, F3). Folding
   everything is a correct reading of tC4's specification, so the counters of rule 2 are
   what keep the rebuild from paying for history on an interactive action.
2. **Prove rule 1 with counts, not only with clocks.** Each crate gives its tests operation
   counters (records visited, changes parsed, values decoded, bytes read, items derived). A
   test runs an action on the Psalms corpus and again on the same corpus made ten times
   larger (more books, more history, more devices). The test fails if the count grows.
   These tests are deterministic and run in `cargo test`.
3. **The UI thread never waits** on disk, the fold, derivation, model training or the
   network. A save is acknowledged when it is durable (STORAGE.md), and the save budget
   below includes the fsync.
4. **Background work** shows determinate progress, lets saves continue, and can be cancelled
   or resumed. It shows that progress within 300 ms of the person's action, off the screen
   thread, as tC4 does (its J15, `e2e/j15-slow-open.spec.ts`, and the owner's ruling of
   2026-08-25): an action still running 300 ms after the click shows from then on its phase
   and a count or fraction that only grows within the phase; an action that ends sooner shows
   nothing, so a quick action never flashes an indicator. The indicator leaves when the action
   ends or gives way to its error; it is never left standing. The working threads count into
   shared counters and a timer off the screen thread sends the reports (rule 3), so a slow
   read cannot hold the first report back. Opening a project is such an action. `tc-store`
   proves it for the open with `open_shows_determinate_progress_by_300_ms` (unit S1,
   STORAGE.md §6.1), and each action under "Background work" gets the same test for its own
   progress.
5. **Measure release builds** on the corpus below. Record the median and p95 of at least 20
   runs (5 runs for actions over 1 s), the machine (with whether its CPU has SHA and
   AVX-512 instructions) and the commit in `docs/evidence/`. CI
   runs the same benchmarks and fails a budget only above three times its value, because CI
   machines vary.
6. **When a budget is missed,** profile, then fix the data structure or the algorithm. Add a
   cache only after that. A budget changes only with evidence, recorded here with the reason.

## Corpus

- **Psalms, real data:** `fixtures/scale/psa/` (see `fixtures/scale/README.md`). The ULT
  with its alignments is the translation; the UHB 3.0.0 Psalms is the original-language
  text; the tN v91 rows (8,212) and TWL v91 rows (7,683) are the checks; the tW v91 articles
  are the linked articles; the WEB Psalms is a second, unrelated real text for edits and
  conflicts.
- **Whole Bible, real checks and generated history:** the corpus of STORAGE.md §14. 66 books
  with verse counts from the versification files. Checks are the real tN and TWL v91 rows
  for the 59 books those releases contain (173,337 rows) and generated rows at the v89
  counts in `fixtures/tc4/conformance-fixtures/check-density.json` for the other seven. The
  Hebrew of every Old Testament book (UHB 3.0.0) and the Greek of every New Testament book
  (UGNT 0.34) are real, from `bundle/en/`.
  Translation text and alignments are the real ULT v91 for its 59 books
  (`bundle/en/en_ult_v91.sb.zip`, 369,635 alignment groups) and are sampled from
  it for the other seven. Three
  devices edit for three years at the §14 rates (about 620,000 changes), with concurrent
  edits of the same records at a realistic rate so that conflicts exist. One deterministic,
  seeded generator produces both the journal changes and the plain inputs that domain crates
  take, so every crate measures the same corpus, and its output labels every generated part.

## Budgets

p95 on the reference machine, release build. "Psalms" means Psalms inside the whole-Bible
project, so project size is always present.

The reference machine is a five-year-old Windows laptop (owner ruling, 2026-10-16): 4 cores,
8 GB, a SATA SSD, Windows 10 or 11 with Defender real-time protection on. Many laptops of that
age (Intel 10th-generation Core U-series, for example) have no SHA instructions, so every
budget must hold without them. A CPU with them hashes about 6.5 times faster
(`docs/LOCAL-BENCH-FINDINGS-2026-10-16.md`, F1), so a measurement on such a CPU alone does not
show that a budget is met.

| Area | Action | Scale | Budget |
|---|---|---|---|
| Editor | Open a book (project already open) | Psalms | 150 ms; the first chapter's verses ready in 50 ms |
| Editor | Save one verse until "Saved" | Psalms | 25 ms including fsync; fold update and the chapter's projection 2 ms |
| Editor | Move to another chapter | Psalms | 16 ms |
| Editor | Update project-wide counts and the conflict list after a save | whole Bible | 1 ms incremental; 100 ms from scratch |
| Checking | Open the Check screen: derive, map versification, place decisions, compute status | Psalms tN and tW | 250 ms the first time; 20 ms from the cache (keyed by pins and book) |
| Checking | Change filter, sort or grouping in the rail | Psalms | 16 ms |
| Checking | Record a decision until "Saved" | Psalms | 25 ms; progress counts update in 1 ms |
| Checking | Re-assess checks after a verse edit | one verse | 1 ms: only that verse's checks |
| Checking | Count what an upgrade or gateway change would invalidate | whole Bible, 215,767 checks | 2 s |
| Checking | Plan the carry-over for an upgrade or gateway change | whole Bible | 5 s in the background; 300 ms per book |
| Aligning | Open the aligner on a verse | the longest verse in Psalms | 30 ms |
| Aligning | Suggestions for one verse | model trained on the NT + GEN + PSA alignments of ULT v91 (tC4's responsiveness fixture used v89) | 50 ms; training in the background; after an alignment save the model updates in the background |
| Aligning | Any aligner edit, including the verse-order sort of the bank and of every card | the longest verse in Psalms | inside the 30 ms open budget; the sort touches only the words of that verse |
| Aligning | Reflow alignments after a verse edit | one verse | 2 ms |
| Resources | Load one book's resources from the installed store (original language, ULT, UST, tN, TWL, linked tW articles) | Psalms | 300 ms the first time, then cached; 60 MiB per open book |
| Resources | Load a lexicon (one JSON file per lexicon) and look up an entry | the Hebrew lexicon, 8,674 entries | 100 ms to load once per session; lookups in memory, under 1 ms |
| Project | Cold open with a usable fold cache | whole Bible | 3 s; determinate progress within 300 ms |
| Project | Cold open with no usable fold cache (a project received from a teammate or imported, or after the cache encoding changes), or the first open after a restart, when the disk cache is cold | whole Bible | 10 s (owner ruling, 2026-10-16); determinate progress within 300 ms |
| Project | Save a verse, record a decision or save an alignment while a materialize, checkpoint, merge commit or upgrade write runs, a whole-project materialize included (owner ruling, 2026-10-18: saving never interrupts the person) | Psalms in the whole Bible | 25 ms p95 including fsync |
| Project | Materialize one book, off the project actor, from a snapshot | Psalms | 1 s p95 on the reference laptop; it shows no indicator and nothing waits for it (an exemption from rule 4); open a book, open Check and the chapter budgets hold while it runs; a count test shows it projects and reads back only that book's files, on the corpus and on the corpus ten times larger |
| Project | Materialize many or all derived files: a book added, removed, restored or rescoped, a pins change, a resource upgrade or gateway change, a sync intake, a derivation update | whole Bible | background work under rule 4 with a progress bar (owner ruling, 2026-10-18: people expect adding a book to take time); saves continue; off the actor, one book at a time, within the project store's memory peak |
| Project | Close a project | whole Bible, Psalms changed | 3 s on the reference laptop |
| Project | Checkpoint | whole Bible | 500 ms |
| Project | Daily sync | whole Bible | 3 s plus network |
| Project | Save work for a teammate (USB export) | whole Bible | 15 s on USB 2, with progress |
| Project | Import a tC4 project | whole Bible | 2 min, with progress, cancellable |
| App | Memory: the summed private working set of `tc4 serve` and every Electronite process (Task Manager's Memory column; summed USS on Linux), read a minute after the last action | whole Bible open, Psalms in use | 1 GiB steady; 1.25 GiB peak with a side session (a sync or export from Home beside another open whole Bible) (owner ruling, 2026-10-18) |
| Project | Memory of the project store (the fold and `tc-store`, without resources): private bytes, read a minute after the last action, never just before exit | whole Bible open, Psalms in use: open, saves and decisions, leaving Psalms, close | 400 MiB steady; 450 MiB peak, the close and the first open without a cache included (owner ruling, 2026-10-18) |
| Project | Memory growth with history | whole Bible at twice today's history (1.2 million changes) | steady memory grows by at most 300 B per added change with one heap, under 350 B with the default Linux heap (owner ruling, 2026-10-18) |
| UI | Long lists (the Psalms check rail, chapter views) | Psalms | virtualized; 16 ms per frame while scrolling |
| UI | Typing in a verse, input to paint | Psalms | 16 ms |
| App | First launch: from the shortcut click to a visible, usable Home | a signed, installed Windows build with Defender on; an empty profile, and the first launch after an upgrade, measured apart | under 60 s each, with determinate progress (rule 4); see "Start-up" |
| App | Repeat launch | the installed bundle | no read and no write of a resource content file (a count test); proposed: a usable Home in 3 s |

## Rows of the project store that are open (unit S2, 2026-10-18)

Unit S2 measured the store's rows on the owner's Linux machine
(`docs/evidence/storage-materialize-2026-10-18.md`; STORAGE.md section 14 has the figures). The
reference laptop was not available, so a row is called met only when the machine's figure and the
laptop estimate both meet it. These rows are **not met or not shown**, and the owner decides what
to do with each:

| Row | State | What would change it |
|---|---|---|
| Checkpoint, whole Bible: 500 ms | **missed** by the estimate (0.8 to 1.3 s with the materialization already done, 1.3 to 2.4 s with Psalms dirty) | write one checkpoint's git objects into one pack (about 3 flushes in place of about 24) and accept libgit2's search for deltas between the new versions of the large files, or relax the row |
| Save during a checkpoint: 25 ms p95 | at the edge: 19.7 to 26.8 ms in ten rounds of thirteen, 29.8 to 44.9 ms in three; 190 to 244 ms in two rounds whose jobs the drive stalled | fewer flushes per checkpoint (the same remedy); the month-end consolidation of a month's packs is still one actor step, and a save made during it waits for that step (it runs once a month); the reference move at the end is an actor step of about 8 ms |
| Materialize one book: 1 s p95 | **not shown**: the laptop estimate is 0.5 to 1.3 s | a measurement on the laptop with Defender on |
| Close a project: 3 s | **not shown**: the laptop estimate is 2.7 to 4.2 s | a measurement on the laptop; |
| Project store memory: 400 MiB steady, 450 MiB peak | met with one heap (`MALLOC_ARENA_MAX=1`: 302 to 305 MiB, peak 321 to 326 MiB); with glibc's default heap 403 to 405 MiB steady and 457 to 461 MiB at the close, which misses by 3 to 5 and 7 to 11 MiB. One heap makes the whole-project job 3.5 times slower (7.9 to 10.7 s against 2.4 to 3.1 s) | Windows' heap is not glibc's; a measurement on the laptop decides |

The rows of saving, progress and a book's materializing touching only that book's files are met
and have count tests.

## Start-up

The first-launch budget of 60 s is a hard gate now (owner ruling Q24 in
`docs/TC4-DELTA-2026-10-16.md`). It is tC4's own release gate. The first measurement of the
rebuild on Windows sets a tighter target. That measurement must come before the target is
fixed, because `tc-resources` fsyncs every file it installs (`write_synced` in
`crates/tc-resources/src/store.rs`, lines 732-737). At a first launch, tC4 writes 3,792 files
into its store and does not fsync each one.

Why tC4 has the gate: before tC4 #740, every tC4 start opened every bundled and installed
resource file. That was 35,823 opens and 613 MB. It took about 8 minutes on a Windows laptop
with Defender real-time protection on (tC4 `docs/PACKAGING.md`, "Bundled English suite", at
`d310bf3`).

tC4's measurements are the reference (tC4 `docs/evidence/startup-740.md` at `d310bf3`). They
were taken on a Mac (arm64). tC4 has not measured the Windows gate yet either.

| tC4 launch | Usable Home |
|---|---:|
| Empty profile | 5,143 ms |
| Repeat launch | 2,186 ms |
| First launch after an upgrade | 2,518 ms |

| tC4 resource preparation only | Archive reads | Resource file reads | Resource file writes | Time |
|---|---:|---:|---:|---:|
| Empty profile | 16 | 0 | 3,739 | 2,795 ms |
| Repeat start | 0 | 0 | 0 | 41 ms |

A resource file here is a resource content file, which tC4 calls an ingredient. The 3,739
resource file writes are part of the 3,792 files that a first launch writes.

The count tests take these counts as their targets:

- A first launch reads each shipped archive once. It writes each installed file once and
  reads none of them back. tC4 ships 16 archives and writes 3,739 resource files (3,792 files
  in all).
- A repeat launch reads no archive and reads or writes no resource content file.

## Background work

Cold open, materialize, checkpoint, sync, export, import, carry-over plans, resource installs
and suggestion training. Each follows rule 4 and has its budget above. First-launch seeding
also follows rule 4: it shows "Preparing resources: n of total", and Close cancels it.

## Owners

| Crate | Budgets |
|---|---|
| `tc-journal` | fold, incremental fold, conflict list, the corpus generator |
| `tc-store` | open, save, save during background work, materialize, close, checkpoint, project store memory and its growth |
| `tc-sync` | daily sync, export, import of exchange files |
| `tc-import` | tC4 import |
| `tc-check` | Check screen, rail, decisions, re-assessment, carry-over |
| `tc-align` | aligner, suggestions, reflow |
| `tc-resources` | loading installed resources, first-launch seeding, the repeat-launch count test |
| `tc-ui`, `tc-app` | lists, typing, never blocking the UI thread |
| `tc4` (the desktop shell) | first launch and repeat launch to a usable Home; the whole app's memory, with `tc-app` |
