# Working in this repository

This repository is a Rust rebuild of translationCore 4 (tC4), a desktop app for Bible
translation. Read `docs/PLAN.md` first. Read `docs/STORAGE.md` before you touch anything
that writes project data.

## The rules that protect translators' work

- Never lose, hide or silently rewrite a translator's text, alignment, check decision or
  comment. If two versions disagree, keep both and show the conflict.
- Every project on disk and every project that leaves the app is a valid Scripture
  Burrito. Code that writes a project validates it in tests.
- Write files atomically: write a temporary file in the same directory, fsync it, rename
  it over the target, then fsync the directory. Use the helper in `tc-store`. Never write
  a project file in place.
- No network access outside the clients in `tc-door43` and `tc-resources`, and only
  behind the app's internet switch. Tests never touch the network. They use fixtures and
  local fake servers.

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
- Before you commit, run `cargo fmt --all`, then
  `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace`.
  All three must pass.

## Commits

- Each commit is one coherent change that builds and passes its tests.
- Commit messages say what changed and why, in plain words.
