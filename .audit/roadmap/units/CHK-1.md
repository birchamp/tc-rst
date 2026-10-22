---
id: CHK-1
title: Architecture and I/O boundary checks
track: A
lane: checks
milestone: M1
state: todo
owner: none
branch: none
worktree: none
depends: [INT-4]
needs_owner: []
proves: []
no_loss: false
size: 0.5-1 d
merged: none
updated: 2026-10-21
next: Claim this card after INT-4 merges, then do step 1.
blocked_by: none
---

# CHK-1: Architecture and I/O boundary checks

## Goal

A new test-only crate, `crates/tc-arch`, checks on every `cargo test` the dependency, I/O and
network rules of `docs/ARCHITECTURE.md` and `AGENTS.md`. Clippy refuses raw `std::fs` writes in
`tc-store` and `tc-journal` outside the atomic helpers. Each failure names its rule.
`ARCHITECTURE.md`'s crate table and network rule match the code.

## Why it matters to a translator

A translator's project must be written only through the crash-safe path, and the internet must
be reached only through the internet switch. Today these rules are prose: nothing stops new code
from writing a project file directly or calling the network around the switch.

## Before you start

- `python3 -I .audit/tools/board.py | grep '^| INT-4 '` shows `done`.
- `test -x scripts/gates.sh && echo ok` prints `ok` (INT-3 made it).
- `test -e crates/tc-arch || echo absent` prints `absent`.
- `sed -n 13p crates/tc-door43-fake/Cargo.toml` prints
  `futures-util = { version = "0.3.34", default-features = false }`: the one break of the
  shared-version rule today (the workspace has it in `[workspace.dependencies]`).
- `sed -n 86,91p docs/ARCHITECTURE.md` prints the three dependency rules. Line 89 says only
  `tc-door43` opens network connections; `AGENTS.md:16` also names `tc-resources`, which calls
  reqwest itself (`crates/tc-resources/Cargo.toml` `reqwest`; `src/aquifer.rs:281-293`).
- `python3 -I .audit/tools/board.py | grep '^| S2 '`. If S2 is `done`, its merge removed git2's
  `https` feature (`units/S2.md`, merge table row `Cargo.toml`):
  `cargo tree --locked -p tc-store -e features | grep -c 'git2 feature "https"'` prints `0`. If it
  prints more, stop (S2's card says TLS must not come back into tc-store). If S2 is not `done`:
  step 5 edits `crates/tc-store`, which S2 is changing. Do steps 1-4 and 6, then park the card
  with `next: do step 5 after S2 merges`.

## Funnel card

- **Invariants**:
  - INV-ARCHITECTURE: each crate depends only on the crates its row in the edge table allows;
    `tc-ui` only on `tc-api` and `tc-core`; reqwest, hyper and hyper-util are normal
    dependencies only of `tc-door43` and `tc-resources`; git2's `https` feature only in
    `tc-door43`; every member uses the workspace edition (2024) and `lints.workspace = true`; a
    dependency that `[workspace.dependencies]` lists is never given its own version.
  - INV-PURE-CRATES-DO-NO-IO: `tc-core`, `tc-usfm`, `tc-versification`, `tc-helps`, `tc-journal`,
    `tc-check`, `tc-align`, `tc-obs`, `tc-export` and `tc-api` have no tokio, reqwest, hyper,
    hyper-util, git2, zip, axum or notify normal dependency, and their non-test source uses no
    `std::fs`, `std::net`, `std::process`, `std::env::var`, `tokio::`, `reqwest`,
    `SystemTime::now` or `Instant::now`.
  - INV-ATOMIC-WRITES: outside the files listed with a reason in
    `crates/tc-arch/allow/fs_writes.txt`, no source file calls a filesystem mutator; inside
    `tc-store` and `tc-journal`, clippy refuses `std::fs` mutators except at sites that carry
    `#[expect(clippy::disallowed_methods, reason = "...")]`.
- **Wrong states**:
  - A new UI file reads storage directly (an edge `tc-ui -> tc-store`).
  - A pure crate opens a file or reads the clock, so its tests depend on the machine.
  - A new writer in `tc-app` or `tc-store` writes a project file with `std::fs::write`, skipping
    the temp file, fsync and rename, so a power cut can leave a half-written verse.
  - A crate reaches the network without the internet switch.
- **Code**:
  - Types: none in product code. In `crates/tc-arch/src/lib.rs` (to create), pure functions over
    text: `edges(metadata_json) -> Vec<(String, String)>`, `scan_io(path, text) -> Vec<Finding>`,
    `scan_writes(path, text) -> Vec<Finding>`, `manifest_findings(path, text, workspace_deps)`.
  - One entry point: the tests in `crates/tc-arch/tests/` are the only place these rules are
    checked; `ARCHITECTURE.md` names them.
  - Removed: the inline `futures-util` version in `crates/tc-door43-fake/Cargo.toml`.
  - Left to checks, and why a type cannot carry it: crate edges and source text are not types.
    The structural fix is typed capabilities (a `ProjectWriter` only `tc-store` can build, a `Net`
    handle only `tc-door43` can build); it is out of scope here and goes to `.audit/minors.tsv`
    as a follow-up (step 6).
- **Checks** (all to create; kind: architecture or lint; runs in: the CI job that runs
  `cargo test --workspace`, and the clippy gates):
  - `crate_edges_follow_the_architecture_table`, `network_libraries_only_in_door43_and_resources`,
    `git_https_only_in_door43`, `pure_crates_have_no_io_dependency`,
    `every_member_uses_the_workspace_edition_and_lints`,
    `shared_dependency_versions_come_from_the_workspace` in `crates/tc-arch/tests/architecture.rs`.
  - `pure_crates_do_no_io` and `only_listed_files_write_files` in
    `crates/tc-arch/tests/io_boundaries.rs`.
  - `the_io_scan_finds_a_planted_std_fs_call`, `the_io_scan_skips_a_cfg_test_module`,
    `the_write_scan_finds_a_planted_rename` in `crates/tc-arch/src/lib.rs` `#[cfg(test)]`.
  - Clippy `disallowed-methods` in `crates/tc-store/clippy.toml` and `crates/tc-journal/clippy.toml`.
  - Proof it can fail: steps 1, 3 and 5 each plant a violation and record the failing output.
- **Prose**: `docs/ARCHITECTURE.md` "Dependency rules" names `crates/tc-arch` as the check.
- **Human**: none.

## Steps

### Step 1: Create the crate with a red architecture test

- Do: create `crates/tc-arch/Cargo.toml` (`publish = false`; `version`, `edition`, `license`,
  `rust-version` all `.workspace = true`; `[lints] workspace = true`; dev-dependency
  `serde_json.workspace = true`; no other dependency) and `crates/tc-arch/src/lib.rs` with the
  pure functions of the funnel card. Write `tests/architecture.rs`. It runs
  `env!("CARGO") metadata --no-deps --format-version 1 --offline` from
  `env!("CARGO_MANIFEST_DIR")/../..`, reads each member's `Cargo.toml` as text with a small
  section-aware line reader (no new dependency), and holds the six checks of the funnel card.
  The edge table lists today's normal dependencies between members (resume-build-lane f9067bc,
  unchanged by S2): tc-core none; tc-usfm, tc-versification, tc-burrito, tc-helps, tc-api,
  tc-door43: tc-core; tc-journal: tc-core, tc-usfm, tc-burrito; tc-check: tc-core, tc-usfm,
  tc-versification, tc-helps, tc-burrito, tc-journal; tc-align: tc-core, tc-usfm,
  tc-versification, tc-journal; tc-obs: tc-helps; tc-export: tc-obs; tc-ui: tc-api, tc-core;
  tc-door43-fake none; tc-resources: tc-core, tc-door43, tc-helps, tc-journal; tc-store: tc-core,
  tc-usfm, tc-burrito, tc-journal, tc-align, tc-obs; tc-corpus: tc-core, tc-usfm, tc-burrito,
  tc-journal, tc-versification; tc-app: tc-api, tc-align, tc-check, tc-core, tc-burrito,
  tc-door43, tc-export, tc-helps, tc-journal, tc-obs, tc-resources, tc-store, tc-usfm,
  tc-versification; tc-server: tc-api; tc4: tc-api, tc-app, tc-door43, tc-server; tc-arch none.
  A member with no row fails ("add its row and its line in ARCHITECTURE.md"). Exceptions, each
  named in the message: reqwest in `tc-resources` "until DOM-1". There is no git2 `https`
  exception: no crate enables it on main or after S2's merge. Every message starts with the
  invariant, for example
  `INV-ARCHITECTURE: tc-ui may depend only on tc-api, tc-core (ARCHITECTURE.md "Dependency
  rules"); crates/tc-ui/Cargo.toml adds tc-store`. Normalise `\` to `/` in paths (the Windows
  nightly runs this too).
- Files: `crates/tc-arch/Cargo.toml`, `crates/tc-arch/src/lib.rs`,
  `crates/tc-arch/tests/architecture.rs` (to create).
- Done when: `cargo test -p tc-arch --test architecture` fails only
  `shared_dependency_versions_come_from_the_workspace`, with a message naming
  `crates/tc-door43-fake/Cargo.toml` and `futures-util`. Then add `tc-store = { path =
  "../tc-store" }` to `crates/tc-ui/Cargo.toml`: `crate_edges_follow_the_architecture_table`
  fails naming tc-ui and tc-store. Revert that line.
- Record: a log line `step 1` with both failure messages.

### Step 2: Fix the shared-version break

- Do: in `crates/tc-door43-fake/Cargo.toml` replace line 13 with `futures-util.workspace = true`
  (the workspace entry already has `default-features = false`).
- Files: `crates/tc-door43-fake/Cargo.toml`, `Cargo.lock` if cargo changes it.
- Done when: `cargo test -p tc-arch --test architecture` passes and
  `cargo test -p tc-door43-fake` passes.
- Record: a log line `step 2`.

### Step 3: Scan the source for I/O and for file writes

- Do: write `tests/io_boundaries.rs`. `pure_crates_do_no_io` scans `crates/<pure>/src/**/*.rs`
  for `std::fs`, `std::net`, `std::process`, `std::env::var`, `env::var(`, a `use std::{...}`
  group naming `fs`, `net`, `process` or `env`, `tokio::`, `reqwest`, `SystemTime::now` and
  `Instant::now`. It skips a file declared as `#[cfg(test)] mod x;` (today
  `crates/tc-usfm/src/derivations/tests.rs`, declared at `derivations/mod.rs:202-203`) and the body
  of an inline `#[cfg(test)] mod x { ... }` (today `crates/tc-journal/src/format.rs:519` holds an
  `Instant::now` at line 548). `only_listed_files_write_files` scans every other crate's `src`
  (not `tc-arch`) for `(std::|tokio::)?fs::(write|rename|remove_file|remove_dir|remove_dir_all|
  copy|create_dir|create_dir_all|hard_link|set_permissions)`, `File::create` and
  `OpenOptions::new(`, with the same `cfg(test)` skipping. Create
  `crates/tc-arch/allow/fs_writes.txt` (one line per file: path, a tab, the reason). Fill it from
  the scan's first run: about 16 files on resume-build-lane at f9067bc (for example
  `crates/tc-store/src/vfs.rs`, `crates/tc-server/src/applock.rs`,
  `crates/tc-resources/src/store.rs`), plus `history.rs`, `repack.rs`, `git.rs` and `fault.rs` of
  `crates/tc-store/src` after S2 (estimate from the S2 snapshot). Read each site before you list
  it: a site that writes inside a project folder outside `tc-store` breaks AGENTS.md; record it as
  a finding and stop (see "Stop and ask when"). A listed file with no write call left also
  fails ("stale entry"). Add the three unit tests of the funnel card to `src/lib.rs`.
- Files: `crates/tc-arch/tests/io_boundaries.rs`, `crates/tc-arch/allow/fs_writes.txt`,
  `crates/tc-arch/src/lib.rs`.
- Done when: `cargo test -p tc-arch` passes. Add `let _ = std::fs::read("x");` to a function in
  `crates/tc-check/src/lib.rs`: `pure_crates_do_no_io` fails with
  `INV-PURE-CRATES-DO-NO-IO: crates/tc-check/src/lib.rs:<line> uses std::fs`. Revert it.
- Record: a log line `step 3` with the failure message and the number of allowlisted files.

### Step 4: Run the whole gate list once

- Do: run `scripts/gates.sh` on the branch. The Windows clippy step and the tc-arch tests must
  both pass (the tests run `cargo metadata` and read files only).
- Files: none.
- Done when: `scripts/gates.sh` ends with its pass line.
- Record: a log line `step 4` with the gate log path.

### Step 5: Forbid std::fs mutators in tc-store and tc-journal

- Do: only when S2 is `done`. Create `crates/tc-store/clippy.toml` and
  `crates/tc-journal/clippy.toml`. A crate's clippy.toml replaces the root one, so first copy the
  root's four keys (`allow-unwrap-in-tests`, `allow-expect-in-tests`, `allow-panic-in-tests`,
  `allow-dbg-in-tests`, all `true`), as `crates/tc-app/clippy.toml:1-6` does. Then
  `disallowed-methods` for `std::fs::write`, `rename`, `remove_file`, `remove_dir`,
  `remove_dir_all`, `copy`, `create_dir`, `create_dir_all`, `hard_link`, `set_permissions`,
  `std::fs::File::create`, `std::fs::File::create_new` and `std::fs::OpenOptions::open`, each with
  `reason = "INV-ATOMIC-WRITES: write project files through tc_store::atomic"`. Run
  `cargo clippy -p tc-store -p tc-journal --all-targets -- -D warnings` and give each site it
  names an `#[expect(clippy::disallowed_methods, reason = "...")]`: the real filesystem
  (`src/vfs.rs`, a file-level `#![expect]`), FaultFs (`src/fault.rs`), the G4 rebuild
  (`src/history.rs:119,129` on S2), `.git` clean-ups (`src/repack.rs:597,611,612,628` and
  `src/git.rs:1079` on S2), and `#[cfg(test)]` modules, `tests/store.rs`,
  `examples/storebench.rs` and `crates/tc-journal/tests/{versioning,canonical,index}.rs` (an
  `allow` with a reason there, since a test file may stop using one).
- Files: the two `clippy.toml` (to create) and the files clippy names.
- Done when: the clippy command passes. Add `let _ = std::fs::write("x", b"x");` to a non-test
  function in `crates/tc-store/src/lib.rs`: clippy fails with `use of a disallowed method
  std::fs::write` and the INV-ATOMIC-WRITES reason. Revert it.
- Record: a log line `step 5` with the failing output and the count of `#[expect]` sites.

### Step 6: Make the documents match the code

- Do: in `docs/ARCHITECTURE.md`: add rows for `tc-corpus` ("the deterministic scale corpus of
  PERFORMANCE.md, for tests and benchmarks only") and `tc-arch` ("tests of these rules; no
  product code"); mark `tc-sync` "(unit X; not built yet)" and `tc-import` "(unit I; not built
  yet)"; replace line 89 with: only `tc-store` writes project files; only `tc-door43` and
  `tc-resources` open network connections, and only behind `tc-door43`'s internet switch
  (`tc-resources` until DOM-1 moves its calls into `tc-door43`); add "`crates/tc-arch` tests
  these rules." Append to `.audit/minors.tsv`: `CHK-1<TAB>Typed capabilities (a ProjectWriter only
  tc-store can build, a Net handle only tc-door43 can build) would make INV-ATOMIC-WRITES and the
  network rule compile-time; tc-arch's scans are the stopgap<TAB>.audit/roadmap/evidence/2026-10-21/checks-inventory.md
  (funnel note after rank 20)`.
- Files: `docs/ARCHITECTURE.md`, `.audit/minors.tsv`.
- Done when: `grep -c 'tc-corpus\|tc-arch' docs/ARCHITECTURE.md` prints at least 2 and
  `grep -n 'tc-resources' docs/ARCHITECTURE.md` shows the new network rule.
- Record: a log line `step 6`. When step 5 is done too, set `state: verifying` and `next`;
  otherwise set `state: parked` and `next: do step 5 after S2 merges`.

## Gates before merge

Run `scripts/gates.sh` (INT-3 made it; INT-4, which this card waits for, needs INT-3) on the exact tree you merge. It must print
`gates: all <n> steps passed; receipt ...`. If the script is missing, run these instead:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
3. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
   the same with `--features gallery`, `--features app` and `--features test-hooks`
4. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
6. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
   (needs `gcc-mingw-w64-x86-64` and `nasm`, as `.github/workflows/ci.yml` installs them)

Plus: `cargo test -p tc-arch` passes, and each planted violation of steps 1, 3 and 5 is in the log.

## Stop and ask when

- A write call outside `tc-store` writes inside a project folder: that breaks AGENTS.md. Record
  it in the log and in `.audit/minors.tsv`, or as a blocker card if a translator's file can be
  half-written; do not allowlist it.
- Step 5 would edit `crates/tc-store` while S2 is not `done`.
- A crate edge or dependency would have to be added to make a test pass: that changes
  `ARCHITECTURE.md`; write the reason in the log and add the row in the same commit.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/checks-inventory.md` ranks 6 and 7, finding C5, "Rules stated only in prose"
- `evidence/2026-10-21/health-domain.md` §2, §6 item 1, F1, F2
- `evidence/2026-10-21/health-storage.md` §5, F7 (S2's direct `std::fs` sites)
- `docs/ARCHITECTURE.md:59-91`; `AGENTS.md:16-17`; `crates/tc-app/clippy.toml:1-6`
- Size basis: checks-inventory ranks 6 and 7, half a day each, minus shared set-up.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
