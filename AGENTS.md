# Working in this repository

This repository is translationCore 4 (tC4), a desktop app for Bible translation, rebuilt in Rust.
It is `main` of `unfoldingWord/translationCore4`. The JavaScript tC4 it replaces is branch
`legacy-4.0`; the reference version the rebuild matches is tC4 commit `d25b7871` (4.0.0-rc.2,
ruling H7).

## Start here

1. Read the issue you are working on (`gh issue view <n>`), and the issues and pull requests it
   links. `gh issue list` and `gh pr list` show what is open and who has it.
2. Read `CONTRIBUTING.md` before your first change. It is the loop every change follows.
3. Then read only what your task needs ("Read what your task needs" below). The big design
   documents are numbered by section; read the sections, not the whole file.

Live state (what is open, what passes) comes from GitHub and from running the checks. Never type
it into a document.

## The tower

The documents form one chain from why to proof. Each layer cites the one above by id, so any test
traces up to the commitment it protects, and any commitment traces down to its proof.

| Layer | Answers | Where | Ids |
|---|---|---|---|
| Vision | Why, for whom, how to choose | `docs/VISION.md` | V1–V8 |
| Journeys | What a person does; the end state | `docs/JOURNEYS.md` | J1–J30 |
| Promises | What is always true | `docs/PLAN.md` | P1–P10 |
| Design | How it works | `docs/STORAGE.md`, `docs/ARCHITECTURE.md`, `docs/APP-DESIGN.md`, `docs/PERFORMANCE.md` | `INV-…`, sections |
| Checks | The proof | tests named after invariants; `parity/app/journeys/coverage.json` (journeys); `docs/P-LEDGER.md` (promises); `scripts/gates.sh` | `INV-…` |
| Code | The app | `crates/` (map: `docs/ARCHITECTURE.md` "Crates") | — |

Across every layer: **rulings** (`docs/rulings/`, H-ids) change any layer; **work** moves things
down the tower (GitHub issues and pull requests, built by the method in `docs/FUNNEL.md`);
**evidence** (`docs/evidence/`, dated documents) records what was measured or decided at a moment.
`docs/MAP.md` lists every document, its layer and whether it is a rule or a record.

## Read what your task needs

| Your task | Read |
|---|---|
| Any change | `CONTRIBUTING.md`; `docs/FUNNEL.md` §1–3; the issue |
| Anything that writes project data | `docs/STORAGE.md` §0 and §1, then the sections the issue names |
| A screen, an endpoint, a flow | the journey in `docs/JOURNEYS.md`; `docs/APP-DESIGN.md` §0 and the sections the issue names |
| Code that runs when a person acts | `docs/PERFORMANCE.md` |
| A new feature | `docs/PLAN.md` "How a new feature enters" |
| A screen | "tC4 and its design system" below |
| A word you do not know | `docs/GLOSSARY.md` |
| Why something is the way it is | `docs/rulings/`, then `docs/rebuild.tsv`; a `tracker:<path>` link is in git at 9249a60d (`docs/MAP.md`) |
| Where code lives | `graft ask "<question>" --source` when graft is installed; else `docs/ARCHITECTURE.md` "Crates" and `grep` |

## How work lands

- Work is tracked in GitHub issues on unfoldingWord/translationCore4. Name the issue a change
  closes in its pull request.
- Every change lands on `main` by pull request through the merge queue. Never push to `main`.
  A pull request needs one approving review and the required checks: `lint-linux`,
  `lint-windows`, `lint-wasm`, `test`, `test-app`, `alignment-budget`, `parity-baseline`,
  `unit`, `builds`, `e2e-linux` and `journeys`. A pull request runs only the lints; the merge
  queue runs the full list.

## The rules that protect translators' work

- Never lose, hide or silently rewrite a translator's text, alignment, check decision or
  comment. If two versions of verse text disagree, keep both and show the conflict. For
  everything else (checks, alignments, comments, settings, books), the newest version wins, the
  older one stays in History where the translator can restore it, and the Received sheet says
  what was replaced (owner ruling H5, `docs/rulings/2026-10-24-H5.md`).
- Every project on disk and every project that leaves the app is a valid Scripture
  Burrito. Code that writes a project validates it in tests.
- Write project files only through the atomic helpers in `crates/tc-store/src/atomic.rs`
  (`atomic_write` and its siblings). They write a temporary file in the same directory, fsync it,
  rename it over the target and fsync the directory. Never write a project file in place.
- No network access outside the clients in `tc-door43` and `tc-resources`, and only
  behind the app's internet switch. Tests never touch the network. They use fixtures and
  local fake servers.
- Count only scenarios a translator can reach: the app's own screens and API, the app or computer
  stopping at any moment, another program editing or holding a project file the app wrote, a
  teammate's work arriving by sync or file, a full disk, the network dropping. Do not test, fix or
  file anything else.

## tC4 and its design system

- tC4 4.0 is history (`docs/rulings/2026-10-29-tc4-is-history.md`). `docs/VISION.md` decides what
  the app does.
- Every screen, new or old, uses tC4's design system: its tokens, CSS, fonts and components
  (`crates/tc-ui/src/ds/`), and the style of its words. Never replace or bypass it; extend it.
- A screen or behavior that came from tC4 may change when the vision calls for it. Add the change,
  with its issue or ruling, to `docs/DIFFERENCES.md`, note it in the journey's entry, and say so
  in the pull request.
- tC4's source at the reference version is `git show d25b7871:<path>`. Read it to learn how a
  screen came to be, not to decide what it must do.

## Code

- Build every change down the enforcement funnel (`docs/FUNNEL.md`): make the wrong state
  impossible in code first, then catch the rest with checks, then explain only what is left in
  prose, and ask the owner only what a translator would notice.
- Rust 2024 edition, stable toolchain. `unsafe` is forbidden.
- Parse external data (files, HTTP, JSON, USFM, TSV, user input) into typed values at the
  boundary. Inside the core, trust the types. Use newtypes for identifiers such as
  `BookId`, `ActorId` and `OpId`, and enums for states, never bags of booleans.
- Logic that decides about a translator's work, or any lifecycle (a load, an upload, a save), is
  a typed state machine: a state enum, an event enum, one pure transition function, and an
  executor for the I/O.
- One job, one function. Before you write code, look for code that already does the job, and
  reuse it (`CONTRIBUTING.md` "Fix a bug"). Merge copies you find.
- Library code returns `Result` with a `thiserror` error type per crate. `unwrap`,
  `expect` and `panic!` are allowed only in tests, or with a comment that proves the
  invariant.
- Business logic lives in pure functions. File and network I/O stays at the edges.
- A comment explains a non-obvious reason. It never narrates what the code does.
- Shared dependency versions live in `[workspace.dependencies]`.

## Tests

- Test behavior through the public API with real fixtures from `fixtures/`. A test must
  fail if the behavior it names breaks. Name it after the invariant or journey it proves.
- Parsers get round-trip property tests (`proptest`). Merge and storage code gets
  property tests for convergence and no-loss, plus crash-injection tests.
- A journey is proven by an end-to-end test in `parity/app/journeys/`, mapped under its id in
  `coverage.json`.
- While you build, run the tests of the crates and journeys you touched. Before you open a
  pull request, run `scripts/gates.sh --touched`. `scripts/gates.sh` (`--list` prints its
  steps) is the list CI runs; `--step <name>` runs one step.

## Performance

- A project holds a whole Bible for years: about 620,000 changes and 215,767 checks. Read
  `docs/PERFORMANCE.md` before you write code that runs when a person acts.
- An interactive action costs what it touches (a verse, a chapter, a check, a book), never
  the size of the project or its history. Prove it with operation-count tests on the Psalms
  corpus and on a corpus ten times larger. Measure the budgets in release builds on the reference
  laptop and record them in `docs/evidence/`; a row that misses becomes an issue (ruling H17).

## Commits

- Each commit is one coherent change that builds and passes its tests.
- Commit messages say what changed and why, in plain words.
