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
   may cost more.
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
   runs (5 runs for actions over 1 s), the machine and the commit in `docs/evidence/`. CI
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
  (`bundle/en/en_ult-v91.zip`, 369,635 alignment groups) and are sampled from
  it for the other seven. Three
  devices edit for three years at the §14 rates (about 620,000 changes), with concurrent
  edits of the same records at a realistic rate so that conflicts exist. One deterministic,
  seeded generator produces both the journal changes and the plain inputs that domain crates
  take, so every crate measures the same corpus, and its output labels every generated part.

## Budgets

p95 on the reference laptop (4 cores, SSD, 8 GB), release build. "Psalms" means Psalms
inside the whole-Bible project, so project size is always present.

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
| Aligning | Reflow alignments after a verse edit | one verse | 2 ms |
| Resources | Load one book's resources from the installed store (original language, ULT, UST, tN, TWL, linked tW articles) | Psalms | 300 ms the first time, then cached; 60 MiB per open book |
| Project | Cold open | whole Bible | 3 s, with determinate progress |
| Project | Materialize one book | Psalms | 300 ms, in the background |
| Project | Checkpoint | whole Bible | 500 ms |
| Project | Daily sync | whole Bible | 3 s plus network |
| Project | Save work for a teammate (USB export) | whole Bible | 15 s on USB 2, with progress |
| Project | Import a tC4 project | whole Bible | 2 min, with progress, cancellable |
| Project | Memory | whole Bible open, Psalms in use | 350 MiB peak; 150 MiB steady |
| UI | Long lists (the Psalms check rail, chapter views) | Psalms | virtualized; 16 ms per frame while scrolling |
| UI | Typing in a verse, input to paint | Psalms | 16 ms |

## Background work

Cold open, materialize, checkpoint, sync, export, import, carry-over plans, resource installs
and suggestion training. Each follows rule 4 and has its budget above.

## Owners

| Crate | Budgets |
|---|---|
| `tc-journal` | fold, incremental fold, conflict list, the corpus generator |
| `tc-store` | open, save, materialize, checkpoint, memory |
| `tc-sync` | daily sync, export, import of exchange files |
| `tc-import` | tC4 import |
| `tc-check` | Check screen, rail, decisions, re-assessment, carry-over |
| `tc-align` | aligner, suggestions, reflow |
| `tc-resources` | loading installed resources |
| `tc-ui`, `tc-app` | lists, typing, never blocking the UI thread |
