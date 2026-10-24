---
id: CHK-5
title: Nightly, Windows and no-network jobs; the mutation executor runs nightly
track: A
lane: checks
milestone: M1
state: parked
owner: none
branch: none
worktree: none
depends: [S2, INT-3]
needs_owner: []
proves: [P3, P4]
no_loss: false
size: 1-2 d
merged: none
updated: 2026-10-24
next: Parked until after the first release (rulings/2026-10-24-H3-H4-H7-fast-release.md).
blocked_by: none
---

# CHK-5: Nightly, Windows and no-network jobs; the mutation executor runs nightly

## Goal

Every night, CI on the default branch runs what is too long for a push: the long store model,
every mutation recipe, the property tests with 4,096 cases, the whole suite with three random
seeds, the whole suite on Windows, and a dependency licence and advisory check. On every push,
the test job runs with the network blocked, and a canary test proves the block is on.

## Why it matters to a translator

The long runs are the strongest proof that no save is lost after a crash or a teammate's sync
(P3, P4), and half the translators use Windows. Today the long model never ran in CI, the
mutation proof lives in one session's ignored folder, and Windows tests run only on a VM whose
results nobody keeps.

## Before you start

- `python3 -I .audit/tools/board.py | grep -E '^\| (S2|INT-3) '` shows both `done`.
- Scheduled jobs run only from the default branch's workflow; the default branch is
  `claude/focused-ride-5uzj7p` (`git -C /home/bot/tc-rst symbolic-ref refs/remotes/origin/HEAD`).
- S2's merge left: `grep -n 'store-model-nightly\|schedule\|workflow_dispatch' .github/workflows/ci.yml`
  shows the job and both triggers; `scripts/gates.sh --list | grep nightly` shows
  `model-long` (`units/S2.md`, merge table rows `ci.yml` and `scripts/gates.sh`).
- The mutation executor is tracked: `git ls-files tools/mutants/execute.py` prints the path
  (S2 step 4). If it does not, S2 step 4 was not done: copy the files S2 step 4 lists from
  `/home/bot/tc-rst/.worktrees/s2-proof-executor/.enjoy-lane/mutation-executor/` (read only; if
  that worktree is gone, use the copy INT-6's log names), make the two path edits S2 step 4
  gives, meet its "Done when" here, and log a `note` on S2's card that CHK-5 brought the
  executor in (S2 step 4 then has nothing left to do).
- `python3 -I tools/mutants/execute.py --owner . --base "$(git rev-parse HEAD)" --cases all --list | python3 -I -c 'import json,sys; print(len(json.load(sys.stdin)))'`
  prints the recipe count (44 at S2's 3b075d7).
- `grep -n 'TC_PROPERTY_CASES' crates/tc-journal/tests/properties.rs` shows the long-sweep
  variable (line 36-38 at f9067bc). `grep -rn 'TC_API_ROUNDTRIP_CASES\|TC_CENSUS' crates` shows
  whether CHK-4 added those variables; use only the ones that exist.
- The Windows VM: `.audit/tools/winvm/run.sh` and `gates.ps1` (`.audit/HANDOFF.md`, "Windows VM").
  Its last run was 8d039a8 on 2026-10-19 and it keeps no results (checks-inventory C13).

## Funnel card

- **Invariants**:
  - INV-NO-LOSS-LONG: every night, on the default branch, the long store model
    (`model::random_histories_lose_no_work_long`), the journal properties at 4,096 cases, the
    suite at three random seeds and every mutation recipe run; a failure or a surviving mutant
    makes the run red and prints how to reproduce it.
  - INV-NO-NETWORK-IN-TESTS: the CI job that runs `cargo test` can reach nothing but loopback,
    and a canary test fails if it can.
  - INV-WINDOWS-BUILDS: every night the whole test suite passes on `windows-latest`.
- **Wrong states**:
  - The long model fails for weeks and nobody sees it (it has never run in CI).
  - A change kills the check that caught a mutant, and nothing notices.
  - A test quietly downloads from Door43; it passes on CI and fails offline or leaks traffic.
  - Windows breaks in a path or a file lock that Linux never shows.
- **Code**:
  - Types: none. Each nightly command is a `NIGHTLY` entry of `scripts/gates.sh` (INT-3), so a
    developer runs exactly what CI runs: `scripts/gates.sh --step <name>`.
  - One entry point: `scripts/gates.sh`; the scheduled jobs in `.github/workflows/ci.yml` call
    it, as S2's `store-model-nightly` does.
  - Removed: nothing.
  - Left to checks, and why a type cannot carry it: network reach and the operating system are
    properties of the machine that runs the tests.
- **Checks**:
  - `NIGHTLY` steps `mutants`, `random-seeds`, `journal-long` (and `roundtrip-long` if CHK-4
    added its variable); kind: model, property, mutation; runs in: the scheduled jobs of step 2.
  - `tests_cannot_reach_the_network_when_tc_no_net_is_set` in
    `crates/tc-door43/tests/no_network.rs` (to create); kind: contract; runs in: CI job `test`.
  - Job `nightly-windows` (to create); kind: the whole suite on Windows.
  - Job `nightly-deny` with `deny.toml` (to create); kind: lint over dependencies.
  - Proof it can fail: `TC_NO_NET=1` without the block fails the canary; a recipe that is not
    detected makes `mutants` exit non-zero; a seed is printed before each random run (steps 1-3).
- **Prose**: a comment above the scheduled jobs: why they run nightly and from the default branch.
- **Human**: a dependency licence outside the list of step 4 (see "Stop and ask when").

## Steps

### Step 1: Add the nightly steps to the gate script

- Do: in `scripts/gates.sh` `NIGHTLY`, beside `model-long`, add:
  `"mutants|python3 -I tools/mutants/execute.py --owner \"$PWD\" --base \"$(git rev-parse HEAD)\" --cases all"`;
  `"journal-long|TC_PROPERTY_CASES=4096 cargo test --locked -p tc-journal --test properties"`;
  `"random-seeds|scripts/random-seeds.sh"`; and, if `TC_API_ROUNDTRIP_CASES` exists,
  `"roundtrip-long|TC_API_ROUNDTRIP_CASES=1024 cargo test --locked -p tc-api --test roundtrip"`.
  Write `scripts/random-seeds.sh` (to create; bash, `set -euo pipefail`): three times, take a
  seed from `/dev/urandom` (`od -An -N4 -tu4 /dev/urandom | tr -d ' '`), print
  `nightly seed <n>: reproduce with PROPTEST_RNG_SEED=<n> scripts/gates.sh --step test`, run
  `PROPTEST_RNG_SEED=<n> cargo test --locked --workspace` (with `TC_CENSUS=1` if CHK-4 added it),
  and stop at the first failure. Check that `execute.py` exits non-zero when a recipe is not
  detected: run one recipe with its mutation emptied (`--cases <one>` on a copy where the recipe
  changes nothing); if it exits 0, make it exit 1 and print
  `INV-NO-LOSS-LONG: recipe <id> survived: <test> passed with the mutation`.
- Files: `scripts/gates.sh`, `scripts/random-seeds.sh` (to create), maybe `tools/mutants/execute.py`.
- Done when: `scripts/gates.sh --list` shows the new steps as `nightly`; `scripts/gates.sh --step
  journal-long` passes; the surviving-recipe run exits 1 with the message. Run `mutants` once in
  the heavy-test slot and record its time (S2's full validation took about 1 h 45 min).
- Record: a log line `step 1` with each step's time.

### Step 2: Run the nightly steps on a schedule, and on Windows

- Do: in `.github/workflows/ci.yml`, copy the shape of `store-model-nightly` (its `if:` on
  `schedule` or `workflow_dispatch`, INT-2's first steps) for: `nightly-mutants`
  (`timeout-minutes: 300`, `scripts/gates.sh --step mutants`), `nightly-seeds` (180,
  `--step random-seeds`), `nightly-properties` (120, `--step journal-long`, and `--step
  roundtrip-long` if it exists). `nightly-windows` already exists (INT-2 step 4 added it so the
  Windows VM has a fallback before INT-4); change its last step to `scripts/gates.sh --step
  test` if INT-3 has not, and keep its `workflow_dispatch` trigger. Each existing push job keeps
  `if: github.event_name != 'schedule'`.
  A GitHub runner has far fewer CPUs than this PC (2 to 4 against 20): if step 1's local
  `mutants` run took over 60 min, split the recipes into `mutants-a` and `mutants-b` (`--cases`
  with half the recipe ids each, in the form `execute.py --help` gives) and run them in two jobs.
  Before you merge, run the Windows suite once on the VM:
  `WINVM_DIR=chk5 .audit/tools/winvm/run.sh HEAD 'powershell -NoProfile -ExecutionPolicy Bypass -File .audit\tools\winvm\gates.ps1'`,
  then delete the one-off checkout `C:\src\chk5` and `C:\src\chk5.bundle` on the VM, as
  `.audit/HANDOFF.md` "The Windows VM" says (the VM's disk is 80 GB).
- Files: `.github/workflows/ci.yml`.
- Done when: `node --test .audit/tools/gates-sync.test.mjs` passes (every nightly step runs in a
  CI job); the VM run prints `exit 0` on each gate line and `tests: <n> passed, 0 failed`.
- Record: a log line `step 2` with the VM totals.

### Step 3: Block the network in the test job, with a canary

- Do: write `crates/tc-door43/tests/no_network.rs`: when `TC_NO_NET` is not set the test returns
  at once; when it is, `std::net::TcpStream::connect_timeout` to `1.1.1.1:443` with 5 s must
  fail, else it panics with `INV-NO-NETWORK-IN-TESTS: a test reached 1.1.1.1:443; the CI test job
  must block egress`. Write `scripts/block-network.sh` (to create): `cargo fetch --locked`, then
  `sudo iptables -A OUTPUT ! -o lo -j REJECT` and `sudo ip6tables -A OUTPUT ! -o lo -j REJECT`;
  with `--undo`, the same rules with `-D`. In CI's `test` job set `env: TC_NO_NET: "1"`, add
  `- run: scripts/block-network.sh` before `scripts/gates.sh --step test`, and
  `- run: scripts/block-network.sh --undo` with `if: always()` as the last step (the cache's post
  step needs the network).
- Files: `crates/tc-door43/tests/no_network.rs`, `scripts/block-network.sh` (to create),
  `.github/workflows/ci.yml`.
- Done when: `cargo test --locked -p tc-door43 --test no_network` passes; on this PC, which has
  network, `TC_NO_NET=1 cargo test --locked -p tc-door43 --test no_network` fails with the
  message (the proof); `node --test .audit/tools/gates-sync.test.mjs` passes.
- Record: a log line `step 3` with the failing output.

### Step 4: Check dependency licences, advisories and sources

- Do: list today's licences offline:
  `cargo metadata --format-version 1 --locked --offline | python3 -I -c 'import json,sys; print(sorted({p.get("license") or "NONE" for p in json.load(sys.stdin)["packages"]}))'`.
  Create `deny.toml`: `[licenses] allow` = the ids of that list that are on this list of
  licences compatible with the app's GPL-2.0-or-later: MIT, Apache-2.0, Apache-2.0 WITH
  LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, Unicode-DFS-2016, CC0-1.0,
  0BSD, BSL-1.0, MPL-2.0. Any other id is not allowed by you (see "Stop and ask when");
  `[advisories]` on; `[sources] unknown-registry = "deny"`, `unknown-git = "deny"`;
  `[bans] multiple-versions = "warn"`. Add a scheduled job `nightly-deny` that uses
  `EmbarkStudios/cargo-deny-action`, pinned to a release tag you read on its GitHub releases page
  (write the tag in the log).
- Files: `deny.toml` (to create), `.github/workflows/ci.yml`.
- Done when: every licence in the list is allowed or is in a `note` for the owner (see "Stop and
  ask when"); `deny.toml` parses (`python3 -I -c "import tomllib; tomllib.load(open('deny.toml','rb'))"`).
- Record: a log line `step 4` with the licence list.

### Step 5: Merge, then watch the first nightly run

- Do: merge per TRACKING.md. Push main only under the push rule in `ROADMAP.md`. Once main is
  pushed, start the nightly jobs by hand:
  `gh workflow run ci.yml --ref claude/focused-ride-5uzj7p`; watch with
  `gh run watch <id> --exit-status`. A failure in test code (a path, a line ending, a Unix-only
  helper) is fixed at once on a branch, gated and merged per TRACKING.md; a failure in product code is a finding (see
  "Stop and ask when"). Write a `note` for G5: the app-e2e job can block the network the same way
  after its installs.
- Files: none, or the fix.
- Done when: `gh run list --workflow ci.yml --event workflow_dispatch --limit 1` shows `success`;
  the next morning `gh run list --workflow ci.yml --event schedule --limit 1` shows `success`.
- Record: a log line `note` with both run ids.

## Gates before merge

Run `scripts/gates.sh` (INT-3 made it) on the exact tree you merge. It must print
`gates: all <n> steps passed; receipt ...`. If the script is missing, run these instead:

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`
3. `cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -- -D warnings`, then
   the same with `--features gallery`, `--features app` and `--features test-hooks`
4. `cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -- -D warnings`
5. `cargo clippy -p tc-app --lib -- -D warnings`; `cargo clippy -p tc-server -p tc4 --lib --bins -- -D warnings`
6. `cargo clippy --workspace --all-targets --target x86_64-pc-windows-gnu -- -D warnings`
   (needs `gcc-mingw-w64-x86-64` and `nasm`, as `.github/workflows/ci.yml` installs them)

Plus: `scripts/gates.sh --step journal-long` and one `--step mutants` run in the heavy-test slot;
the Windows VM run of step 2.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (three for a no-loss item, one per lens; see `GLOSSARY.md` "Verification lenses"), and a `fix` line for every blocker and every major that harms a translator's data or view.

## Stop and ask when

- A dependency's licence is not on the list of step 4: do not allow it yourself. Tell the owner in
  plain words which crate it is, what it does in the app, and the options (keep it, replace it,
  drop the feature), with your recommendation.
- A nightly or Windows failure is in product code: a blocker if a translator's data or view is
  at risk on that platform; open a card with the failing output. Never mark the job
  `continue-on-error`.
- A mutant survives: that is a gap in a no-loss check; record it and stop; the storage lane
  decides the fix.
- A gate fails twice in a row and you cannot tell why.

## References

- `evidence/2026-10-21/checks-inventory.md` ranks 8, 11, 17, 19, 20; findings C6, C13, C15
- `evidence/2026-10-21/done-definition.md` P3 and P4 rows, DOD-12
- `evidence/2026-10-21/health-storage.md` §11, F15
- `units/S2.md` step 4 and the merge table; `units/INT-3.md` (the `NIGHTLY` array)
- `crates/tc-store/tests/store/model.rs:2541-2545` (S2: `random_histories_lose_no_work_long`,
  5,000 histories)
- Size basis: checks-inventory ranks 8 (1 h), 11 (2 h), 17 (2 h), 19 (1 day), 20 (part).

## Log

- 2026-10-21 | roadmap | created | none | from the 2026-10-21 assessment | .audit/roadmap/evidence/2026-10-21/checks-inventory.md
- 2026-10-24 | roadmap | re-planned | none | owner ruling: finish in days | .audit/roadmap/rulings/2026-10-24-H3-H4-H7-fast-release.md
