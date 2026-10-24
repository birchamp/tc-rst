---
id: DOM-3
title: Capability tokens for project writes and the network
track: A
lane: domain
milestone: M1
state: parked
owner: none
branch: none
worktree: none
depends: [S3, DOM-1]
needs_owner: []
proves: [P4, P5]
no_loss: true
size: 2-3 d (basis: DOM-1 moves every network call behind one crate in 1-2 d; this card threads two tokens through about a dozen call sites and deletes one write path)
merged: none
updated: 2026-10-24
next: Parked until after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md).
blocked_by: none
---

# DOM-3: Capability tokens for project writes and the network

## Goal

Two rules that `tc-arch`'s source scans hold today (CHK-1) become compile-time: a project file
can be written only by code that holds a `ProjectWriter`, which only `tc-store` builds inside
`ProjectHandle`; a network connection can be opened only by code that holds a `Net`, which only
the internet switch builds. App-data files (preferences, hooks, the install bundle's manifest)
are written through the same atomic helper with an `AppDataWriter` built once at start. CHK-1's
`only_listed_files_write_files` allow list shrinks to the files that are the helpers themselves
and the app lock, and the test fails if a new file appears.

## Why it matters to a translator

Every rule that protects a translator's files today is a scan over source text: one new file that
calls `std::fs::write` on a project folder slips past it until CI runs, and a reviewer may miss
it. With a token, such code does not compile. Nothing visible changes; what changes is how hard
it is to lose work by accident.

## Before you start

- `grep -n "core merged" .audit/roadmap/units/S3.md` prints a line: S3's `preserve::keep` and
  `atomic` reshaping is in (they are what the token threads through).
- `sed -n 's/^state: //p' .audit/roadmap/units/DOM-1.md` prints `done`: every network call is in
  `tc-door43` behind `Http`, so one token covers them.
- `grep -n "pub fn atomic_write\|pub fn atomic_remove\|pub fn atomic_create_dirs" crates/tc-store/src/atomic.rs`
  prints the three helpers this card changes.
- `grep -rln "atomic_write(" crates/tc-app/src` lists the app-data callers (`app.rs`, `hooks.rs`,
  `prefs.rs`, `backend/scenario_store.rs` on 2026-10-22).
- `sed -n 110,121p crates/tc-app/src/bundle.rs` shows the second temp-and-rename path this card
  deletes (the install bundle's manifest, written at build time).
- `cat crates/tc-arch/allow/fs_writes.txt` (CHK-1) is the list this card shrinks.

## Funnel card

- **Invariants**:
  - INV-ATOMIC-WRITES: a project file is written only through `tc_store::atomic::*`, and those
    functions take a `&ProjectWriter` that only `ProjectHandle` can construct.
  - INV-ONE-NETWORK-GATE: a connection is opened only by `tc_door43::Http`, whose constructor
    takes a `Net` that only the internet switch constructs.
  - INV-ONE-WRITE-PATH: there is one temp-and-rename implementation in the workspace
    (`tc_store::atomic`); `bundle.rs`'s copy is gone.
- **Wrong states**:
  - Product code outside `tc-store` that writes inside a project folder (compiles today, caught
    by a scan; does not compile after this card).
  - A network client built outside the internet switch (same).
  - Two atomic-write implementations that can drift (one syncs the folder, the other does not).
- **Code**:
  - Types: `ProjectWriter` (in `tc-store`, private constructor, held by `ProjectHandle`);
    `AppDataWriter` (in `tc-store`, private constructor, built once by `App::start` or the
    equivalent in `crates/tc-app/src/app.rs`, for `prefs.rs`, `hooks.rs` and the bundle
    manifest); `Net` (in `tc-door43`, private constructor, built by the internet switch;
    `Http::with_finder` and every constructor take `&Net`). The app lock
    (`crates/tc-server/src/applock.rs`) keeps its own `File::create` and `remove_file`: it is
    not a project file and must work before any store exists; it stays on the allow list with
    that reason.
  - One entry point: `tc_store::atomic::{atomic_write, atomic_remove, atomic_create_dirs}` with a
    writer token; `tc_door43::Http` with a `Net`.
  - Removed: `crates/tc-app/src/bundle.rs` lines 113-120 (its own `File::create` and `rename`);
    `reqwest::Client::new` or `builder()` anywhere but behind `Net`.
  - Left to checks, and why: that no file outside the allow list writes a file is still a scan
    (CHK-1's test), because `std::fs` cannot be made private to a crate; the allow list is now
    short enough to read in one glance, and the test fails on any new entry.
- **Checks**:
  - `only_listed_files_write_files` in `crates/tc-arch/tests/io_boundaries.rs` (CHK-1's test):
    its allow list shrinks to `crates/tc-store/src/vfs.rs`, `crates/tc-store/src/fault.rs`,
    `crates/tc-store/src/atomic.rs`, `crates/tc-server/src/applock.rs` and the S3 store files
    CHK-1 names (`history.rs`, `repack.rs`, `git.rs`, `tc-resources/src/store.rs`) with a reason
    each; kind: architecture scan; runs in: `cargo test --workspace`.
  - `a_project_writer_cannot_be_built_outside_tc_store` as a `compile_fail` doc test on
    `ProjectWriter` (and the same for `Net`); kind: compile test; runs in: `cargo test`.
  - `the_bundle_manifest_is_written_through_atomic_write` in `crates/tc-app/tests/bundle.rs`;
    kind: unit; runs in: `cargo test`.
  - Proof it can fail: add `std::fs::write` to a new file in `tc-app` and run the scan (fails
    naming the file); try `ProjectWriter::new()` in a test outside `tc-store` (does not compile,
    which the `compile_fail` test asserts).
- **Prose**: `docs/ARCHITECTURE.md` (CHK-1's file): one paragraph, "project writes and network
  connections need a token; here is who builds each"; CHK-1's "Left to checks" reason changes to
  "see DOM-3" when this merges.
- **Human**: none.

## Steps

### Step 1: `ProjectWriter` and `AppDataWriter`, red first

- Do: add the two token types to `crates/tc-store/src/atomic.rs` with private constructors;
  `ProjectHandle` holds a `ProjectWriter` and passes it to every atomic call it makes; the three
  helpers take `&dyn Writer` (a sealed trait both tokens implement) as their first argument. Add
  the `compile_fail` doc tests. Build the `AppDataWriter` once where the app starts
  (`crates/tc-app/src/app.rs`) and thread it to `prefs.rs`, `hooks.rs` and
  `backend/scenario_store.rs`.
- Files: `crates/tc-store/src/atomic.rs`, `crates/tc-store/src/lib.rs`, the `ProjectHandle`
  file, `crates/tc-app/src/app.rs`, `prefs.rs`, `hooks.rs`, `backend/scenario_store.rs`.
- Done when: `cargo build --workspace` passes; `cargo test -p tc-store --doc` runs the two
  `compile_fail` tests green; every existing store test passes unchanged.
- Record: log line `step 1`; set `state: building`; update `updated` and `next`.

### Step 2: One write path: delete `bundle.rs`'s copy

- Do: replace `crates/tc-app/src/bundle.rs` lines 113-120 with `atomic_write` under the
  `AppDataWriter`; write `the_bundle_manifest_is_written_through_atomic_write`.
- Files: `crates/tc-app/src/bundle.rs`, `crates/tc-app/tests/bundle.rs`.
- Done when: `grep -n "File::create\|fs::rename" crates/tc-app/src/bundle.rs` prints nothing;
  the test passes.
- Record: log line `step 2`.

### Step 3: `Net` for the network

- Do: add `Net` to `tc-door43` with a private constructor built by the internet switch; every
  `Http` constructor takes `&Net`; `tc-resources` (after DOM-1) receives an `Http`, never builds
  one. Add the `compile_fail` doc test.
- Files: `crates/tc-door43/src/http.rs`, the internet-switch file DOM-1 names,
  `crates/tc-resources/src/*` call sites.
- Done when: `grep -rn "reqwest::Client" crates --include=*.rs | grep -v "crates/tc-door43/src/http.rs"`
  prints nothing outside tests; `cargo test -p tc-door43 --doc` runs the `compile_fail` test.
- Record: log line `step 3`.

### Step 4: Shrink CHK-1's allow list and write it down

- Do: cut `crates/tc-arch/allow/fs_writes.txt` to the files the funnel card names, each with its
  reason; run the scan and read every file it still reports before you list it. Add the
  ARCHITECTURE.md paragraph. Change CHK-1's card "Left to checks" sentence to "see DOM-3" in a
  `note` log line on CHK-1 (if CHK-1 is done, in this card's commit; the owner rule for logs
  applies only to active cards).
- Files: `crates/tc-arch/allow/fs_writes.txt`, `docs/ARCHITECTURE.md`, `units/CHK-1.md`.
- Done when: `cargo test -p tc-arch` passes; adding `std::fs::write` to a new file fails it with
  `INV-ATOMIC-WRITES: <file> writes a file; …` (revert).
- Record: log line `step 4`; set `state: verifying`; update `next`.

## Gates before merge

The full gate list (`scripts/gates.sh` once INT-3 lands; until then every command in `AGENTS.md`
"Tests" plus the Windows clippy step from `.github/workflows/ci.yml`), plus:

- `cargo test -p tc-store --doc` and `cargo test -p tc-door43 --doc` run the `compile_fail`
  tests.
- The S2/S3 model test (`random_histories_lose_no_work`) and the crash tests pass unchanged: the
  token changes no byte on disk.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A write site outside `tc-store` turns out to write inside a project folder (the scan's list
  names one you cannot explain): that is an AGENTS.md break; record it as a finding and tell the
  storage lane before you change it.
- Threading the token needs a change to a `tc-store` public signature that X or I (in flight)
  call: write the new signature in the log and tell their owners through a `note` on their cards
  before you merge.

## References

- `units/CHK-1.md` funnel card "Left to checks" and step 6 (the minor this card promotes).
- `FUNNEL.md` §4 (typed capabilities) and §9 (DOM-3 in the Code row).
- `evidence/2026-10-21/checks-inventory.md` (the funnel note after rank 20).
- `.audit/roadmap/evidence/2026-10-21/review-2026-10-22.md` (funnel F4).

## Log

- 2026-10-22 | roadmap session (Cursor, review of 2026-10-22) | created | none | the review (funnel F4) found the Code-layer fix for INV-ATOMIC-WRITES and INV-ONE-NETWORK-GATE parked as a minor | .audit/roadmap/evidence/2026-10-21/review-2026-10-22.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
