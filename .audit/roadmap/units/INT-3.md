---
id: INT-3
title: One gate list for CI and every local runner; merge only with a passing gate
track: A
lane: checks
milestone: M0
state: todo
owner: none
branch: none
worktree: none
depends: [INT-2]
needs_owner: []
proves: []
no_loss: false
size: 0.5-1 d
merged: none
updated: 2026-10-22
next: Claim this card after INT-2 merges, then do step 1.
blocked_by: none
---

# INT-3: One gate list for CI and every local runner; merge only with a passing gate

## Goal

One script, `scripts/gates.sh`, holds the gate list. CI runs each step by name; the lane and
merge workflows run the script; a test checks the Windows VM's list against it. A gate run
never rewrites or blesses a file. A clean full run writes a receipt for the exact git tree,
and main moves only to a tree that has one.

## Why it matters to a translator

A translator never sees it. It protects their work: B11 and C4 were merged before their gates
passed, and no local list ran the Windows build, so changes CI rejects passed on this PC.

## Before you start

- INT-2 is done (`python3 -I .audit/tools/board.py`), `rust-toolchain.toml` is on main, and
  `gh run list --branch claude/focused-ride-5uzj7p --limit 1` shows `success`.
- No `scripts/` folder on main: `git -C /home/bot/tc-rst ls-tree claude/focused-ride-5uzj7p scripts`
  prints nothing.
- `git -C /home/bot/tc-rst grep -n "^const GATES" claude/focused-ride-5uzj7p -- .audit/workflows`
  lists five files. Live: `build-lane.js:21`, `merge-main.js:21`. History, left alone:
  `app-layer.js`, `app-phase0.js`, `fix-text-align.js`.
- The VM's list: `.audit/tools/winvm/gates.ps1` lines 8-13 (`fmt`, `clippy`, `test`, `wasm`).
- Variables that bless or rewrite instead of checking (each read by `env::var_os` on main):
  `UPDATE_REPORT` (`crates/tc-ui/tests/screen_record.rs`), `UPDATE_GOLDEN`
  (`crates/tc-api/tests/golden.rs`), `UPDATE_PREPARE` (`crates/tc-ui/tests/prepare_page.rs`),
  `TC_JOURNAL_BLESS` (`crates/tc-journal/tests/canonical.rs`, `versioning.rs`), `TC_WRITE_PROBE`
  (`crates/tc-journal/tests/index.rs:701`: writes `src/index_probe.batch`, skips the assertion).
- Step 5 needs `which x86_64-w64-mingw32-gcc nasm` to print two paths (INT-2 step 7).

## Funnel card

- **Invariants**:
  - INV-LOCAL-PASS-MEANS-CI-PASS: `scripts/gates.sh` runs exactly the commands CI runs; CI runs
    each by name; every other runner calls the script or is checked against it.
  - INV-NO-UNGATED-MERGE: main moves only to a commit whose tree has a receipt
    `target/gates/<tree sha>.ok` from a full run of `scripts/gates.sh` on a clean checkout.
  - INV-GATES-NEVER-BLESS: a gate run never rewrites a file or blesses a snapshot.
- **Wrong states**:
  - Local gates pass, CI fails: no local list had the Windows clippy (checks-inventory C2).
  - A merge lands before its gates pass: f07fd9e (B11) at 11:56:05, its gate log started at
    11:56:07 with no pass line; 1a5d5d1 (C4) the same (C3).
  - A gate blesses what it then checks: the lane's untracked `.enjoy-logs/resume-full-gates.sh`
    runs `cargo fmt --all`, and for c5 `UPDATE_REPORT=1` on `screen_record` before that gate.
- **Code**:
  - Types: bash arrays `STEPS` and `NIGHTLY` of `"name|command"` lines in `scripts/gates.sh`
    (to create); `NIGHTLY` is empty until S2 adds the long model.
  - One entry point: `scripts/gates.sh`. CI jobs call `scripts/gates.sh --step <name>`, so CI
    cannot drift by construction.
  - Removed: the gate strings of `build-lane.js` and `merge-main.js`; the command list in
    `AGENTS.md` "Tests".
  - Left to checks, and why a type cannot carry it: `gates.ps1` is PowerShell on Windows and
    cannot call bash, so a test compares it. Git cannot refuse a local ref move without a
    hook every clone installs by hand, so the merge procedure and `merge-main.js` check the
    receipt, and CI on main is the backstop.
- **Checks** (all `node:test`, run in CI's `parity-baseline` job through `--step node`):
  - `.audit/tools/gates-sync.test.mjs` (to create); kind: contract. Tests:
    (a) `INV-LOCAL-PASS-MEANS-CI-PASS: every gate step runs in a CI job` (each `STEPS` name
    appears in a `ci.yml` line `scripts/gates.sh --step <name>`);
    (b) `INV-LOCAL-PASS-MEANS-CI-PASS: CI runs no command outside the gate list` (each `ci.yml`
    line that starts with `cargo ` or `node --test` equals a `STEPS` command);
    (c) `INV-LOCAL-PASS-MEANS-CI-PASS: the Windows VM runs the same commands` (each `gates.ps1`
    entry equals the `STEPS` command of the same name; the VM runs a subset);
    (d) `INV-LOCAL-PASS-MEANS-CI-PASS: the workflows run the gate script`.
  - `.audit/tools/gates-script.test.mjs` (to create); kind: unit. Tests:
    `INV-GATES-NEVER-BLESS: the gate script refuses a blessing environment`;
    `INV-NO-UNGATED-MERGE: --check-receipt refuses a tree without a receipt`;
    `--list prints every step`.
  - Proof they can fail (step 2): add `      - run: cargo test --workspace --release` to
    `ci.yml`; (b) prints `gate drift: ci.yml runs "cargo test --workspace --release" but
    scripts/gates.sh does not`. Delete the refusal loop from `gates.sh`; the blessing test
    fails naming `UPDATE_REPORT`. Restore both.
- **Prose**: one bullet in `AGENTS.md` "Tests"; TRACKING.md "merge an item" step 1.
- **Human**: none.

## Steps

### Step 1: Write the gate script

- Do: claim the card;
  `git -C /home/bot/tc-rst worktree add -b track-INT-3 /home/bot/tc-rst/.worktrees/int-3 claude/focused-ride-5uzj7p`.
  Create `scripts/gates.sh` (bash, `set -euo pipefail`, mode 755) with, one per line:

  ```
  STEPS=(
    "fmt|cargo fmt --all --check"
    "clippy|cargo clippy --locked --workspace --all-targets -- -D warnings"
    "clippy-release-bins|cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings"
    "clippy-release-app|cargo clippy --locked -p tc-app --lib -- -D warnings"
    "clippy-windows|cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings"
    "test|cargo test --locked --workspace"
    "wasm-ui|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings"
    "wasm-ui-gallery|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features gallery -- -D warnings"
    "wasm-ui-app|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features app -- -D warnings"
    "wasm-ui-hooks|cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets --features test-hooks -- -D warnings"
    "wasm-api|cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings"
    "node|<ci.yml's parity-baseline node --test line, copied exactly> .audit/tools/*.test.mjs"
  )
  NIGHTLY=()
  ```

  The cargo strings equal INT-2's `ci.yml` lines character for character. On af4eef3 the
  `node` entry reads, on one line: `node|node --test parity/tc4-baseline/baseline.test.mjs
  parity/tc4-baseline/mock-server/server.test.mjs parity/app/state.test.mjs
  parity/app/observe.test.mjs parity/app/env-rust.test.mjs parity/view-gallery/view-gallery.test.mjs
  crates/tc-ui/app/recovery.test.mjs parity/app/journeys/*.test.mjs .audit/tools/*.test.mjs`.
  If main's `ci.yml` line differs when you start, copy main's. Behavior:
  - First of all: if `UPDATE_REPORT`, `UPDATE_GOLDEN`, `UPDATE_PREPARE`, `TC_JOURNAL_BLESS` or
    `TC_WRITE_PROBE` is set (`[[ -v NAME ]]`, even empty), print to stderr
    `gates: refusing to run: NAME is set; a gate never blesses or rewrites files (INV-GATES-NEVER-BLESS)`
    and exit 2.
  - `--list`: print `<n><TAB><name><TAB><command>` per `STEPS` entry, then each `NIGHTLY` entry
    with `nightly` as `<n>`.
  - `--step <name or number>`: run that one step (`STEPS` or `NIGHTLY`) with `bash -c` from
    `git rev-parse --show-toplevel`; exit with its code; no receipt.
  - `clippy-windows` first checks `command -v x86_64-w64-mingw32-gcc nasm`; if one is missing
    it prints `gates: clippy-windows needs x86_64-w64-mingw32-gcc and nasm: sudo apt-get install -y --no-install-recommends gcc-mingw-w64-x86-64 nasm`
    and fails.
  - No arguments: note `git status --porcelain` and `git rev-parse HEAD^{tree}`; run `STEPS`
    in order, printing `gates: <n>/<total> <name>`; on a failure print
    `gates: step <name> failed (exit <code>)` and exit 1. When all pass and the tree was clean
    before and is clean and the same after, write `${CARGO_TARGET_DIR:-target}/gates/<tree>.ok`
    (lines `tree`, `head`, `date` in UTC, `rustc --version`, `steps`, `journeys yes|no`) and
    print `gates: all <total> steps passed; receipt <path>`. On a dirty tree print
    `gates: all <total> steps passed; no receipt: the tree has changes`.
  - `--journeys`: after `STEPS`, run `cargo build --locked -p tc4 --features test-hooks,fixture-backend`,
    `cargo build --locked -p tc-door43-fake`, `crates/tc-ui/build-app.sh --out <target>/tc-ui-app`,
    `crates/tc-ui/build-app.sh --test-hooks --out <target>/tc-ui-app-hooks`, then
    `node --test --test-reporter=tap parity/app/journeys/*.test.mjs` with
    `TC4_BIN=<target>/debug/tc4`, `TC4_UI=<target>/tc-ui-app`,
    `TC4_UI_HOOKS=<target>/tc-ui-app-hooks`, `TC4_DOOR43_FAKE=<target>/debug/tc-door43-fake`
    (`<target>` is `${CARGO_TARGET_DIR:-target}` as an absolute path). Leave
    `PLAYWRIGHT_MODULE` unset unless the caller set it; `parity/app/journeys/kit.mjs:25` then
    uses `parity/tc4-baseline/node_modules/playwright`. Fail when the TAP summary's
    `# fail` or `# skipped` is above 0 (everything is built, so a skip is a missing build).
  - `--check-receipt`: on a clean tree whose receipt exists, print `gates: receipt <path>` and
    exit 0; else print `INV-NO-UNGATED-MERGE: no passing gate receipt for tree <tree> (HEAD <short sha>); run scripts/gates.sh on this exact tree`
    and exit 1.
  - A comment at the top carries the reason of `AGENTS.md` lines 52-56: the workspace build has
    the test hooks, so the release-like clippy steps check what a release compiles.
- Files: `scripts/gates.sh` (to create).
- Done when: after `git add`, `git ls-files -s scripts/gates.sh` starts `100755`;
  `scripts/gates.sh --list | wc -l` prints 12; `UPDATE_REPORT= scripts/gates.sh --list; echo $?`
  prints the refusal and `2`; `scripts/gates.sh --step fmt` exits 0.
- Record: a log line `step 1`; `state: building`; `next: step 2`.

### Step 2: Write the two tests, red first

- Do: both files use `node:test`, `node:assert/strict`, `node:fs`, `node:child_process` only.
  - `gates-sync.test.mjs` reads: `scripts/gates.sh` (each `"name|command"` between `STEPS=(`
    or `NIGHTLY=(` and `)`); `.github/workflows/ci.yml` (each `scripts/gates.sh --step <name>`,
    and each line whose text after an optional `- run: ` or `run: ` starts with `cargo ` or
    `node --test`); `.audit/tools/winvm/gates.ps1` (each `name = 'command'` in
    `$gates = [ordered]@{ ... }`, name quoted or not); the `const GATES =` line of
    `.audit/workflows/build-lane.js` and `merge-main.js`. Messages start `gate drift:` and name
    the file, step and command. (c) drops ` --keep-going` and ` --no-fail-fast` before
    comparing: they change how much a failure reports, not what passes. When
    `.audit/tools/run-gates.mjs` exists (S2 brings it), (d) also reads the body of its
    `export function defaultSteps() {` up to the next line that is only `}`, and fails unless
    it contains `scripts/gates.sh` and no `'cargo'`.
  - `gates-script.test.mjs` spawns `bash scripts/gates.sh`. Its receipt test builds a scratch
    repository under `target/gates-test-<random>/` (never `/tmp`), copies `scripts/gates.sh`
    in, commits with `git -c user.name=t -c user.email=t@example.invalid commit`, sets
    `CARGO_TARGET_DIR` to the scratch `target`, and expects: exit 1 and `INV-NO-UNGATED-MERGE`
    with no receipt; exit 0 once it writes `target/gates/<tree>.ok`; exit 1 after it adds an
    untracked file. The test writes the receipt file itself (it never runs cargo). It removes
    the scratch folder.
  - Run the two mutations of "Proof they can fail", then restore.
- Files: `.audit/tools/gates-sync.test.mjs`, `.audit/tools/gates-script.test.mjs` (to create).
- Done when: `node --test .audit/tools/gates-sync.test.mjs .audit/tools/gates-script.test.mjs`
  fails in exactly these places, which steps 3-4 turn green: (a); (b) for the `node --test`
  line only (the script's `node` step adds `.audit/tools/*.test.mjs`); (c); (d). With the
  mutations, (b) also names `cargo test --workspace --release` and the blessing test names
  `UPDATE_REPORT`.
- Record: a log line `step 2` quoting both mutation outputs; `next: step 3`.

### Step 3: Make CI run the script

- Do: in `.github/workflows/ci.yml` replace each cargo or node `run:` with
  `- run: scripts/gates.sh --step <name>`: `lint-linux`: `fmt`, `clippy`,
  `clippy-release-bins`, `clippy-release-app`; `lint-windows`: keep the apt-get and rustup
  lines, end with `scripts/gates.sh --step clippy-windows`; `lint-wasm`: `wasm-ui`,
  `wasm-ui-gallery`, `wasm-ui-app`, `wasm-ui-hooks`, `wasm-api`; `test`: `test`;
  `parity-baseline`: `node`. Keep every comment and setup step.
- Files: `.github/workflows/ci.yml`.
- Done when: `grep -c 'scripts/gates.sh --step' .github/workflows/ci.yml` prints 12;
  `grep -cE '^\s*(- run: |run: )?(cargo|node --test) ' .github/workflows/ci.yml` prints 0;
  tests (a) and (b) pass.
- Record: a log line `step 3`; `next: step 4`.

### Step 4: Point the VM and the workflows at the list

- Do:
  - `gates.ps1` entries: `fmt = 'cargo fmt --all --check'`,
    `clippy = 'cargo clippy --locked --workspace --all-targets --keep-going -- -D warnings'`,
    `test = 'cargo test --locked --workspace --no-fail-fast'` (the script reads
    `$logs\test.log`; keep the name), and
    `'wasm-ui' = 'cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings'`.
  - `build-lane.js` and `merge-main.js`:
    `const GATES = 'scripts/gates.sh, from the worktree root with CARGO_BUILD_JOBS=6 exported; it prints "gates: all N steps passed" only when every step passed'`.
    In `merge-main.js`'s gate prompt, `green=true` also requires
    `scripts/gates.sh --check-receipt` to exit 0.
- Files: `.audit/tools/winvm/gates.ps1`, `.audit/workflows/build-lane.js`,
  `.audit/workflows/merge-main.js`.
- Done when: `node --test .audit/tools/*.test.mjs` passes every test.
- Record: a log line `step 4`; `next: step 5`.

### Step 5: Prove the receipt; make the merge procedure require it

- Do: commit steps 1-4 (`git status --porcelain` prints nothing), then
  `. ~/.tc-env && scripts/gates.sh` (about 40 min, estimate; one of the PC's two heavy-run
  slots). Then change `.audit/roadmap/TRACKING.md` "Checklist: merge an item" step 1 to: "Make
  the merge commit, with the card's `done` edit, in a separate worktree; run `scripts/gates.sh`
  there; main moves only when `scripts/gates.sh --check-receipt` exits 0 on that commit. A fix
  after a failed gate is a new commit, gated again."
- Files: `.audit/roadmap/TRACKING.md`.
- Done when: the run ends `gates: all 12 steps passed; receipt target/gates/<tree>.ok`;
  `scripts/gates.sh --check-receipt` exits 0; `touch probe && scripts/gates.sh --check-receipt; echo $?`
  prints the INV-NO-UNGATED-MERGE line and `1`; then `rm probe`.
- Record: a log line `gate` with the receipt path and test totals; `next: step 6`.

### Step 6: Point AGENTS.md at the script

- Do: in `AGENTS.md` "Tests", replace the bullets "Before you commit, run ..." and "The
  workspace build always has the test hooks ..." (lines 46-56 on af4eef3) with: "Before you
  commit, run `scripts/gates.sh` (`--list` prints its steps). It is the list CI runs; every
  step must pass, and main moves only to a commit `scripts/gates.sh --check-receipt` accepts."
- Files: `AGENTS.md`.
- Done when: `grep -c 'scripts/gates.sh' AGENTS.md` prints at least 1;
  `grep -c 'cargo clippy' AGENTS.md` prints 0.
- Record: a log line `step 6`; `state: gating`; `next: step 7`.

### Step 7: Merge with a receipt, push, and hand notes on

- Do: merge by the new TRACKING.md step 1 in `/home/bot/tc-rst/.worktrees/int-3-merge`
  (detached at main): title `Merge INT-3: one gate list, and no merge without a passing gate`,
  trailer `Unit: INT-3`, the card's `done` edit and the INT-4 note below in that commit. Run
  `scripts/gates.sh` there. The receipt lives in that worktree's `target/gates/`, so
  `--check-receipt` runs there:
  `cd /home/bot/tc-rst/.worktrees/int-3-merge && scripts/gates.sh --check-receipt`. Only when it
  exits 0 does the integration lane run `git -C /home/bot/tc-rst merge --ff-only <sha>`, push main
  (push rule: main after its full gates), and watches `gh run watch <id> --exit-status`. If you are not the
  integration lane, put those commands in `next` and stop. Remove both worktrees after.
- Files: `.audit/roadmap/units/INT-3.md`.
- Done when: all five CI jobs are green on the pushed merge commit.
- Record: a log line `merge`, and a log line `note`: "INT-4: keep INT-3's GATES lines in
  build-lane.js and merge-main.js. Add the line
  `"alignment-budget|cargo test --locked -p tc-align --release --test suggestion_budget -- --ignored --nocapture"`
  to STEPS and make that job run `scripts/gates.sh --step alignment-budget`; test (b) fails
  until you do."

## Gates before merge

Until this card lands, every command in `AGENTS.md` "Tests" plus CI's steps, from the
worktree root after `. ~/.tc-env`: `cargo fmt --all --check`;
`cargo clippy --locked --workspace --all-targets -- -D warnings`; `cargo test --locked --workspace`;
`cargo clippy --locked -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
the same with `--features gallery`, `--features app` and `--features test-hooks`;
`cargo clippy --locked -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`;
`cargo clippy --locked -p tc-app --lib -- -D warnings`;
`cargo clippy --locked -p tc-server -p tc4 --lib --bins -- -D warnings`;
`cargo clippy --locked --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`;
the `node --test` line of CI's `parity-baseline` job. `scripts/gates.sh` with no arguments runs
exactly these, so on the merge commit it is this card's gate. Plus:

- `scripts/gates.sh --check-receipt` exits 0 in the merge worktree.
- `node --test .audit/tools/*.test.mjs` passes, and both step-2 mutations were seen failing.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- MinGW or NASM is still missing at step 5: no receipt can be written here. Set
  `state: blocked`, `blocked_by: the owner has not installed MinGW and NASM (INT-2 step 7)`,
  and remind the owner once.
- A CI step has no `STEPS` entry and you cannot tell whether it is a gate: ask the
  orchestrator; never drop it.
- The full run fails twice in a row on a step that passes in CI on the same commit.

## References

- `.audit/roadmap/evidence/2026-10-21/checks-inventory.md` C2, C3; ranks 2, 3 and 18.
- `.audit/roadmap/evidence/2026-10-21/process-integration.md` F14 (S2's receipts digest every
  environment variable and live in git-ignored `.enjoy-lane/`).
- S2 branch: `.audit/tools/run-gates.mjs` (`defaultSteps()` at line 16, a lock, receipts) and
  `lane-state.mjs`; S2's merge brings them, and test (d) then checks `defaultSteps()`. Today
  `defaultSteps()` lists ten `'cargo'` commands of its own, so S2's merge must make it run
  `scripts/gates.sh` (the S2 card says so). Its test `run-gates.test.mjs:39` asserts the old
  list and changes with it.
- `TRACKING.md` "Checklist: merge an item"; `FUNNEL.md` §5.

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
