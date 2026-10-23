---
id: INT-2
title: Make CI green and trustworthy
track: A
lane: checks
milestone: M0
state: building
owner: checks lane (Claude Code 2198ee19)
branch: track-INT-2
worktree: .worktrees/int-2
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: one cfg guard, a toolchain file and CI edits)
merged: none
updated: 2026-10-22
next: step 5: run the Linux gates and the VM clippy (steps 1-4 are in this commit)
blocked_by: none
---

# INT-2: Make CI green and trustworthy

## Goal

GitHub CI on main is green again and stays a gate people can trust: one pinned toolchain, a
lock file CI may not change, a runner with room on its disk, and the work split into jobs that
each have a time limit. Every step CI ran before still runs.

## Why it matters to a translator

A translator never sees CI. It protects them: since 2026-10-19 main's CI has not finished
`cargo test --workspace` once, so a change that loses a translator's work could land unseen.

## Before you start

- Main's CI is red: `gh run list --branch claude/focused-ride-5uzj7p --limit 5` shows
  `failure` for af4eef3. Run 39426828571 failed at "clippy for Windows" with
  `error[E0433]: cannot find unix in os --> crates/tc-app/tests/op_counts/harness.rs:179:14`
  and skipped `cargo test` and the wasm steps (`gh run view 39426828571 --json jobs`).
- The last green run: 39256356308 on a52bc0c, 2026-10-19 12:59Z, 25 min 39 s for the whole job
  (`gh run list --branch claude/focused-ride-5uzj7p --status success --limit 1`).
- `git -C /home/bot/tc-rst show claude/focused-ride-5uzj7p:crates/tc-app/tests/op_counts/harness.rs | sed -n 179p`
  prints `    std::os::unix::fs::symlink(`. The same line is at 179 on `resume-build-lane`.
- No `rust-toolchain.toml` on main; this PC has `rustc 1.99.1` and rustup 1.29.1.
- This PC cannot run the Windows-target clippy: `which x86_64-w64-mingw32-gcc nasm` prints
  nothing and `sudo -n true` wants a password. The Windows VM builds natively
  (`~/win11-vm.sh status`).
- The roadmap is committed on main:
  `git -C /home/bot/tc-rst cat-file -e claude/focused-ride-5uzj7p:.audit/roadmap/units/INT-2.md`
  succeeds. If not, stop and tell the orchestrator.
- `df -h /home` shows at least 80G free (estimate; basis: one merged worktree's `target`,
  app-b2, holds 61G, process-integration.md). It showed 213G free on 2026-10-21.
- `df -h /` shows at least 40G free and `grep -c TMPDIR ~/.tc-env` prints at least 1 (a
  `TMPDIR` under `$HOME`): the root drive was 97% full on 2026-10-21 and `/tmp` writes failed
  with a quota error; on 2026-10-22 `/` had 567G free, so the pressure has eased but the `/tmp`
  quota stands. If either fails, do INT-6 steps 3 and 4 (the deletion list and the owner's yes)
  first, or add the `TMPDIR` line yourself (it is a one-line change in a file outside git). INT-6
  is not split into two cards, because the disk recovered.

## Funnel card

- **Invariants**:
  - INV-CI-IS-THE-GATE: every commit pushed to main gets a CI result, built with the toolchain
    `rust-toolchain.toml` names and the dependencies `Cargo.lock` names, covering fmt, every
    clippy (Linux, release-like, Windows, wasm32) and `cargo test --workspace`.
  - INV-BUILDS-ON-WINDOWS: test code that calls a Unix-only API sits under `#[cfg(unix)]` and
    has a Windows path, unless it tests Unix-only behavior.
- **Wrong states**:
  - Main red for days: the tests never run, so a regression in saved work lands unseen.
  - The toolchain floats, so a new rustc can bring new lints with no code change. Run
    39160698131 logged "stable updated - rustc 1.99.1 (from rustc 1.99.0)". (No failure is yet
    shown to come from the float: 56 of 58 clippy failures were one lint on new code,
    `.audit/roadmap/evidence/2026-10-21/verification.md`.)
  - `cargo test` failed on named tests in at least 6 main runs (`evidence/2026-10-21/verification.md`):
    `html_conversion_never_panics_and_keeps_paragraph_text`, `a_1_gib_exchange_upload_streams_to_disk`,
    `an_import_upload_of_exactly_the_cap_is_taken_up`, `tokio_never_blocked`. Some may come
    from the full disk.
  - The runner's disk fills ("No space left on device", runs 39305042134, 39317068070).
  - CI builds other dependency versions than the developer tested.
- **Code**:
  - Types: none.
  - One entry point: `rust-toolchain.toml` alone names the toolchain; every CI job installs it.
  - Removed: `dtolnay/rust-toolchain@stable` (floating); the single `rust` job.
  - Left to checks, and why a type cannot carry it: which platform a test compiles for is not
    a type. CI's `lint-windows` job catches it. A scan that every `os::unix` use in
    `crates/*/tests` is under `cfg(unix)` belongs to CHK-1; this card does not build it.
- **Checks**:
  - Job `lint-windows` in `.github/workflows/ci.yml` (to create); kind: lint; runs in: CI.
    Proof it can fail: run 39426828571 failed with E0433 at the line step 1 guards.
  - Jobs `lint-linux`, `lint-wasm`, `test` (to create) carry every other step of the old job.
  - `--locked` on every cargo command but `cargo fmt`. Proof: remove the last `[[package]]`
    entry of `Cargo.lock`, run `cargo metadata --locked --format-version 1 > /dev/null`; it
    fails naming `--locked`; then `git checkout Cargo.lock`.
  - The pin. Proof: each rust job's log prints `rustc 1.99.1`.
- **Prose**: a comment in `ci.yml` over the disk step, and one in harness.rs on why Windows
  may copy.
- **Human**: no question. Two notices to the owner (step 7).

## Steps

### Step 1: Guard the symlink in the op-count harness

- Do: claim the card (TRACKING.md). Then
  `git -C /home/bot/tc-rst worktree add -b track-INT-2 /home/bot/tc-rst/.worktrees/int-2 claude/focused-ride-5uzj7p`.
  In `crates/tc-app/tests/op_counts/harness.rs`, replace lines 179-183
  (`std::os::unix::fs::symlink(` ... `.unwrap();`) with:
  - `let link = dir.path().join("app-data/resources");`
  - `#[cfg(unix)]` on `std::os::unix::fs::symlink(super::corpus::psalms_resources(), &link).unwrap();`
  - `#[cfg(windows)]` on a block that calls
    `std::os::windows::fs::symlink_dir(super::corpus::psalms_resources(), &link)` and, on `Err`
    (Windows allows symlinks only in Developer Mode or as administrator), calls a new
    `#[cfg(windows)] fn copy_tree(from: &Path, to: &Path)` in harness.rs, a copy of
    `copy_tree` at `crates/tc-app/tests/app/seed.rs:158` (another test binary; it cannot be
    shared). harness.rs imports neither `std::fs` nor `std::path::Path`: write the paths out in
    full (`std::fs::create_dir_all`, `std::path::Path`) inside the `cfg(windows)` items, so a
    Linux build gets no unused-import warning.
- Files: `crates/tc-app/tests/op_counts/harness.rs`.
- Done when: `. ~/.tc-env && cd /home/bot/tc-rst/.worktrees/int-2 && cargo clippy --locked -p tc-app --all-targets -- -D warnings`
  exits 0 and `cargo test --locked -p tc-app --test op_counts` prints `test result: ok`.
  Linux never compiles the `cfg(windows)` block; step 5's VM clippy checks it.
- Record: a log line `step 1`; `state: building`; `next: step 2`.

### Step 2: Check no other test code calls a Unix-only API unguarded

- Do: in the worktree, `grep -rn "os::unix" crates/*/tests crates/*/examples`. For each hit,
  read the 30 lines above it and find `#[cfg(unix)]` on its statement, function, `impl` or
  module. On af4eef3 there were 14 hits; 13 were guarded: tc-app tests `app/folders.rs:53`,
  `app/scenario.rs:77`, `app/seed.rs:558` and `:567`, `app/state_seed.rs:754`,
  `app/store.rs:179`; tc-burrito `tests/mutations.rs:1054`, `:1082`, `:1106`; tc-resources
  `tests/seed.rs:777` and `:786`; tc-store `tests/store/loss.rs:448`; tc-corpus
  `examples/foldbench.rs:253` (it has a `cfg(windows)` twin). The 14th was harness.rs:179,
  which step 1 guards. Guard any other unguarded hit as in step 1.
- Files: only files with an unguarded hit.
- Done when: `grep -rn "os::unix" crates/*/tests crates/*/examples | wc -l` prints the number
  of hits you checked, and the log names the `#[cfg(unix)]` line for each.
- Record: a log line `step 2`: "N hits, all guarded"; `next: step 3`.

### Step 3: Pin the toolchain

- Do: create `rust-toolchain.toml`:

  ```toml
  [toolchain]
  channel = "1.99.1"
  profile = "minimal"
  components = ["rustfmt", "clippy"]
  targets = ["wasm32-unknown-unknown", "x86_64-pc-windows-gnu"]
  ```

- Files: `rust-toolchain.toml` (to create).
- Done when: in the worktree, `rustup show active-toolchain` prints
  `1.99.1-x86_64-unknown-linux-gnu` and names `rust-toolchain.toml` (rustup downloads it once).
- Record: a log line `step 3`; `next: step 4`.

### Step 4: Split and harden the CI workflow

- Do: in `.github/workflows/ci.yml` keep `name`, `permissions`, `on`, `concurrency` and the
  `parity-baseline` job. Replace the `rust` job with four jobs on `ubuntu-latest`. Each starts:
  `- uses: actions/checkout@v4`; a comment (the runner image leaves too little disk for this
  workspace) and `- run: sudo rm -rf /usr/share/dotnet /usr/local/lib/android /opt/ghc`;
  `- run: rustup toolchain install && rustc --version` (with no name, rustup installs the
  toolchain the file names); `- uses: Swatinem/rust-cache@v2`. Then, with `--locked` directly
  after the cargo subcommand (INT-3 copies these strings exactly), and every old comment kept:
  - `lint-linux`, `timeout-minutes: 30`: `cargo fmt --all --check`;
    `cargo clippy --locked --workspace --all-targets -- -D warnings`;
    `cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings`;
    `cargo clippy --locked -p tc-app --lib -- -D warnings`.
  - `lint-windows`, `timeout-minutes: 30`: the step `clippy for Windows (x86_64-pc-windows-gnu)`
    with its four lines, the last now
    `cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`.
  - `lint-wasm`, `timeout-minutes: 30`: the five wasm lines (tc-ui with no features, then
    `--features gallery`, `app`, `test-hooks`; tc-api), each `cargo clippy --locked -p ...`.
  - `test`, `timeout-minutes: 75` (estimate: the old job took 26 min; the test binaries now
    sum to about 899 s locally): `cargo test --locked --workspace`.
  - `nightly-windows` (moved here from CHK-5 step 2 so a fallback for the Windows VM exists
    before INT-4 merges): `runs-on: windows-latest`, `timeout-minutes: 120`,
    `if: github.event_name == 'schedule' || github.event_name == 'workflow_dispatch'`,
    `defaults: run: shell: bash`; steps: `git config --global core.longpaths true`,
    `actions/checkout@v4`, `rustup toolchain install && rustc --version`,
    `Swatinem/rust-cache@v2`, then `cargo test --locked --workspace` (INT-3 later replaces the
    line with `scripts/gates.sh --step test`). Add `on: schedule: - cron: '0 6 * * *'` and
    `workflow_dispatch:` to the workflow's `on` block; the four push jobs keep
    `if: github.event_name != 'schedule'`.
- Files: `.github/workflows/ci.yml`.
- Done when: `grep -cE '^\s*(- run: )?cargo ' .github/workflows/ci.yml` prints 12;
  `grep -E '^\s*(- run: )?cargo ' .github/workflows/ci.yml | grep -v 'cargo fmt' | grep -vc -- '--locked'`
  prints 0; `grep -c 'timeout-minutes' .github/workflows/ci.yml` prints 5;
  `python3 -I -c "import yaml; print(sorted(yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']))"`
  prints `['lint-linux', 'lint-wasm', 'lint-windows', 'nightly-windows', 'parity-baseline', 'test']`;
  `gh workflow run ci.yml --ref <your branch>` then `gh run list --workflow ci.yml --limit 1`
  shows the `nightly-windows` job ran (green or with a Windows-only failure you log for step 5).
- Record: a log line `step 4`; `next: step 5`.

### Step 5: Run the gates here and on the Windows VM

- Do: commit steps 1-4 with the card. In the worktree, after `. ~/.tc-env`, run every command
  of "Gates before merge" except the Windows-target clippy. Then build on the VM (start it with
  `~/win11-vm.sh start` if `status` says it is off):
  `WINVM_DIR=int-2 .audit/tools/winvm/run.sh HEAD 'powershell -NoProfile -ExecutionPolicy Bypass -File .audit\tools\winvm\gates.ps1'`
  (if `mktemp` fails because `/tmp` is full, prefix `TMPDIR=$HOME/.cache`). Then delete the
  VM's copy:
  `ssh -i ~/.ssh/tc-vm -p 2222 builder@127.0.0.1 "powershell -Command Remove-Item -Recurse -Force C:\src\int-2, C:\src\int-2.bundle"`.
- Files: none.
- Done when: every Linux gate exits 0, and the VM prints `clippy  exit 0` (its clippy runs with
  `--keep-going`, so it shows every Windows error). A VM `test` line other than `exit 0` does
  not block this card (CI runs no Windows tests yet; CHK-5 adds them): write the failing test
  names in the log for the orchestrator.
- Record: a log line `gate` with the Linux test totals and the VM's summary; `state: gating`.

### Step 6: Merge into main and see CI green

- Do: the branch may be pushed under its own name (H1 answered yes, `rulings/2026-10-22-H1.md`); main moves only through INT-4.
  1. `git -C /home/bot/tc-rst worktree add --detach /home/bot/tc-rst/.worktrees/int-2-merge claude/focused-ride-5uzj7p`.
  2. There: `git merge --no-ff --no-commit track-INT-2`; in the card set `state: done`,
     `merged: <git rev-parse --short track-INT-2>`, `updated`, `next: none`, a log line
     `merge`, and the INT-4 note of "Record" below. Commit: title
     `Merge INT-2: CI on main checks every change again`, blank line, `Unit: INT-2`, then the
     Co-Authored-By line. If main moved since you branched, run the Linux gates again here.
  3. Only the integration lane (it owns the `/home/bot/tc-rst` checkout) runs
     `git -C /home/bot/tc-rst merge --ff-only <merge sha>` and
     `git -C /home/bot/tc-rst push origin claude/focused-ride-5uzj7p`. If that is not you, put
     both commands in `next` and stop.
  4. `gh run list --branch claude/focused-ride-5uzj7p --limit 1`, then
     `gh run watch <run id> --exit-status`.
  5. When green, `git -C /home/bot/tc-rst worktree remove` both worktrees (`int-2-merge`, `int-2`).
- Files: `.audit/roadmap/units/INT-2.md`.
- Done when: `gh run watch <run id> --exit-status` exits 0 and
  `gh run view <run id> --json jobs --jq '.jobs[].conclusion'` prints `success` five times. A
  job red for another reason is fixed in this card; until then every merge waits.
- Record: in the merge commit, a log line `note`: "INT-4: resume-build-lane has the same
  harness.rs:179 line and adds an `alignment-budget` job. The lane has not changed harness.rs
  since af4eef3, so the merge carries INT-2's guard; if a later lane commit changes harness.rs,
  cherry-pick INT-2's harness.rs commit onto the merge. Keep INT-2's guard, toolchain file
  and four jobs; give `alignment-budget` the same four first steps and
  `cargo test --locked -p tc-align --release --test suggestion_budget -- --ignored --nocapture`."
  After CI, a log line `note` with the run id.

### Step 7: Tell the owner the two things only the owner can do

- Do: write to the owner, in short plain sentences:
  1. "GitHub can require the 'ci' check before main changes only on a paid plan (GitHub Pro)
     or a public repository. Nothing is needed now: once INT-3 lands, our merge rule refuses an
     untested merge." (Evidence: `gh api repos/birchamp/tc-rst/branches/claude%2Ffocused-ride-5uzj7p/protection`
     answered HTTP 403 "Upgrade to GitHub Pro or make this repository public to enable this feature".)
  2. "Please run this once on the Linux PC, so our checks can build the Windows version here
     as CI does: `sudo apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 nasm`.
     It needs your password."
- Files: none.
- Done when: both notes are in your final message to the orchestrator (it relays them to the
  owner) and in the card's log.
- Record: a log line `note` that quotes them.

## Gates before merge

Every command in `AGENTS.md` "Tests" plus CI's steps, from the worktree root after
`. ~/.tc-env`: `cargo fmt --all --check`; `cargo clippy --locked --workspace --all-targets -- -D warnings`;
`cargo test --locked --workspace`; `cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`,
then with `--features gallery`, `--features app`, `--features test-hooks`;
`cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy --locked -p tc-app --lib -- -D warnings`;
`cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
(this PC lacks MinGW and NASM; until the owner installs them, the VM's `gates.ps1` clippy and
CI's `lint-windows` stand in); the `node --test` line of `parity-baseline`. Plus: the VM's
`clippy  exit 0` (step 5), and all five CI jobs green after the merge (step 6).

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- The VM or CI shows Windows errors beyond harness.rs:179: fix each the same way here. Stop
  only for one in `crates/*/src` where you cannot tell what Windows should do.
- `rustup toolchain install` fails in CI saying it has no toolchain to install: use
  `rustup show` instead (older rustup installs from the file on `show`) and log it.
- The `test` job fails on a named test: run `gh run rerun <run id> --failed` once. If the same
  test fails again, write its name and message in the log and stop for the orchestrator
  (README: a gate that fails twice). Flaky timing tests belong to CHK-4.
- A CI job fails twice in a row on the same step and you cannot tell why.
- `ci.yml` conflicts because main moved: keep these four jobs and put the other side's new
  steps in the right job; stop if the other side removed a step.

## References

- `.audit/roadmap/evidence/2026-10-21/checks-inventory.md` §0, C1, rank 1.
- `.audit/roadmap/evidence/2026-10-21/process-integration.md` F6.
- `.audit/roadmap/evidence/2026-10-21/verification.md` (corrections to checks-inventory C1).
- `.github/workflows/ci.yml` on af4eef3, lines 11-45 (the `rust` job).
- `.audit/tools/winvm/run.sh`, `.audit/tools/winvm/gates.ps1`; ROADMAP "Decisions I made".

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
- 2026-10-22 20:46 EDT | checks lane (Claude Code 2198ee19) | claim | none | Claimed on branch track-INT-2 in .worktrees/int-2; added the TMPDIR line to ~/.tc-env (it was missing) | python3 -I .audit/tools/board.py
- 2026-10-22 20:54 EDT | checks lane (Claude Code 2198ee19) | step 1 | none | harness.rs: the symlink is under cfg(unix); cfg(windows) tries symlink_dir and falls back to a copy_tree; clippy -p tc-app --all-targets passed and op_counts printed `test result: ok. 24 passed` | cargo test --locked -p tc-app --test op_counts
- 2026-10-22 20:54 EDT | checks lane (Claude Code 2198ee19) | step 2 | none | 14 hits of os::unix in crates/*/tests and crates/*/examples, all guarded: harness.rs:181 (step 1); state_seed.rs:752; scenario.rs:75; store.rs:176; seed.rs:555 (tc-app, covers 558 and 567); folders.rs:45; mutations.rs:1046 and 1071 (covers 1082 and 1106); tc-resources seed.rs:774 (covers 777 and 786); loss.rs:445; foldbench.rs:251 | grep -rn "os::unix" crates/*/tests crates/*/examples
- 2026-10-22 20:54 EDT | checks lane (Claude Code 2198ee19) | step 3 | none | rust-toolchain.toml pins 1.99.1 with rustfmt, clippy, wasm32 and x86_64-pc-windows-gnu; `rustup show active-toolchain` prints 1.99.1-x86_64-unknown-linux-gnu (overridden by rust-toolchain.toml) | rustup show active-toolchain
- 2026-10-22 20:54 EDT | checks lane (Claude Code 2198ee19) | step 4 | none | ci.yml: lint-linux, lint-windows, lint-wasm, test (push) and nightly-windows (schedule, dispatch); 12 cargo lines, 11 with --locked, 5 timeouts; --locked proof: with the last [[package]] of Cargo.lock removed, `cargo metadata --locked` exits 101 saying "--locked was passed to prevent this"; the workflow_dispatch run is logged after the push | python3 -I -c "import yaml; print(sorted(yaml.safe_load(open('.github/workflows/ci.yml'))['jobs']))"
