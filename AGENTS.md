# Working in this repository

This repository is a Rust rebuild of translationCore 4 (tC4), a desktop app for Bible
translation. Read `docs/PLAN.md` first. Plan and live state: `.audit/roadmap/README.md`; record
your work as `.audit/roadmap/TRACKING.md` says. Read `docs/STORAGE.md` before you touch
anything that writes project data.

## The rules that protect translators' work

- Never lose, hide or silently rewrite a translator's text, alignment, check decision or
  comment. If two versions of verse text disagree, keep both and show the conflict. For
  everything else (checks, alignments, comments, settings, books), the newest version wins, the
  older one stays in History where the translator can restore it, and the Received sheet says
  what was replaced (owner ruling H5, `.audit/roadmap/rulings/2026-10-24-H5.md`).
- Every project on disk and every project that leaves the app is a valid Scripture
  Burrito. Code that writes a project validates it in tests.
- Write files atomically: write a temporary file in the same directory, fsync it, rename
  it over the target, then fsync the directory. Use the helper in `tc-store`. Never write
  a project file in place.
- No network access outside the clients in `tc-door43` and `tc-resources`, and only
  behind the app's internet switch. Tests never touch the network. They use fixtures and
  local fake servers.

## Matching tC4

- Match tC4 4.0's look and behavior: its screens, words, keys and flows.
- Where tC4's behavior works against what a feature is for, follow what the translator needs
  instead of copying tC4 (owner ruling, 2026-10-17). Record each such difference with its
  reason where the design lists declared differences, and tell the owner.

## Code

- Rust 2024 edition, stable toolchain. `unsafe` is forbidden.
- Parse external data (files, HTTP, JSON, USFM, TSV, user input) into typed values at the
  boundary. Inside the core, trust the types. Use newtypes for identifiers such as
  `BookId`, `ActorId` and `OpId`, and enums for states, never bags of booleans.
- Library code returns `Result` with a `thiserror` error type per crate. `unwrap`,
  `expect` and `panic!` are allowed only in tests, or with a comment that proves the
  invariant.
- Business logic lives in pure functions. File and network I/O stays at the edges.
- A comment explains a non-obvious reason. It never narrates what the code does.
- Shared dependency versions live in `[workspace.dependencies]`.

## Tests

- Test behavior through the public API with real fixtures from `fixtures/`. A test must
  fail if the behavior it names breaks.
- Parsers get round-trip property tests (`proptest`). Merge and storage code gets
  property tests for convergence and no-loss, plus crash-injection tests.
- While you build, run the tests of the crates and journeys you touched. `scripts/gates.sh`
  (`--list` prints its steps) is the list CI runs on every batch; every step must pass, and main
  moves only to a commit `scripts/gates.sh --check-receipt` accepts. How a card gets ready:
  `.audit/roadmap/TRACKING.md` "Checklist: merge an item".

## Performance

- A project holds a whole Bible for years: about 620,000 changes and 215,767 checks. Read
  `docs/PERFORMANCE.md` before you write code that runs when a person acts.
- An interactive action costs what it touches (a verse, a chapter, a check, a book), never
  the size of the project or its history. Prove it with operation-count tests on the Psalms
  corpus and on a larger corpus: about twice the size in the gate, ten times larger in the
  nightly run (owner ruling, `.audit/roadmap/rulings/2026-10-26-test-scale.md`). Meet the
  budgets in release builds.

## Commits

- Each commit is one coherent change that builds and passes its tests.
- Commit messages say what changed and why, in plain words.
