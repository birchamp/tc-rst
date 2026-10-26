---
id: CHK-8
title: A hung test is named and stopped within minutes, in CI and in scripts/gates.sh
track: A
lane: checks
milestone: M1
state: gating
owner: checks lane (Claude Code 70ea6b18)
branch: track-CHK-8
worktree: .worktrees/track-CHK-8
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: one runner swap, one config file, a timeout per journey file)
merged: none
updated: 2026-10-26
next: Ready for a batch: the integration lane merges track-CHK-8 and gates it with scripts/gates.sh --wait-ci.
blocked_by: none
---

# CHK-8: A hung test is named and stopped within minutes, in CI and in scripts/gates.sh

## Goal

The owner (2026-10-26): "We need to know deterministically when something is unnecessarily
hung." On 2026-10-26 a D1 tc-sync test sat idle for 61 minutes, and a CI test job stops only at
its 150-minute job limit, with no name for the test that hung. The gate's Rust tests run through
cargo-nextest, which names a test that runs past 60 s and kills it after 5 minutes; a longer
limit goes only to tests the timing data proves need one. Each journey file gets a per-test
timeout.

## Why it matters to a translator

A hung gate holds a batch for two and a half hours and says nothing about why, so fixes reach
translators a day late. A named, stopped test costs minutes and points at the bug.

## Funnel card

- **Invariants**:
  - INV-A-HUNG-TEST-IS-NAMED-AND-STOPPED: every Rust test the gate runs, and every journey test,
    has a time limit; past it the run names the test and stops it.
- **Checks**:
  - `.audit/tools/gates-sync.test.mjs` "INV-A-HUNG-TEST-IS-NAMED-AND-STOPPED"; kind: unit; runs
    in CI's node step. Every gate step runs Rust tests through nextest (doctests and `--list`
    aside), `[profile.default]` stops a test after 5 periods of 60 s, every override keeps a
    limit of 20 minutes or less, every CI job that runs a nextest step installs a pinned
    cargo-nextest, the node step sets `--test-timeout`, and `browserTest`'s default timeout is 2
    minutes or less.
  - Proof it can fail: a step back on `cargo test`, or kit's old 600 s default, fails it (two
    problems named); an override of 16 minutes fails it.
  - The hang proof, `evidence/2026-10-26-CHK-8/hang-proof/`: a test that waits forever, run
    with the repository's config, is named SLOW each minute and TIMEOUT at 300 s; exit 100.
- **Prose**: TRACKING "How a card gets ready" item 3 names the limits and the install line;
  `.config/nextest.toml` says why each override exists.

## Steps

### Step 1: Measure

- Do: run each suite through nextest and through cargo test; compare time, memory and failures
  (shared-state tests). Take per-test times from CI logs.

### Step 2: The runner, the limits and the journeys

- Do: `.config/nextest.toml`, gates.sh steps, ci.yml install, the Windows VM list, journey
  timeouts; a hung test that proves the limit.

## Gates before merge

`scripts/gates.sh --wait-ci <batch>`.

- The review gate of `FUNNEL.md` §8: a `verify` log line before `done`.

## Log

- 2026-10-26 14:03 EDT | checks lane (Claude Code 70ea6b18) | created | none | the orchestrator's brief after a D1 tc-sync test hung 61 minutes | message from the orchestrator
- 2026-10-26 14:03 EDT | checks lane (Claude Code 70ea6b18) | claim | none | claimed on track-CHK-8 | python3 -I .audit/tools/board.py
- 2026-10-26 17:20 EDT | checks lane (Claude Code 70ea6b18) | step 1 | 4e2c3f92 | measured nextest against libtest on every suite at 4 threads, before and after CHK-7c: at 10x tc-app's op_counts could not move (7.1 GB a test process, OOM at 22 GB); at CHK-7c's 2 every suite runs on nextest, 4.4 and 6.3 GB summed peaks, 978 s and 588 s. Found on main: tc-sync's race test hung 2 of 5 runs (fixed in D1); fixed folders under CARGO_TARGET_TMPDIR raced across processes | .audit/roadmap/evidence/2026-10-26-CHK-8/measurements.md
- 2026-10-26 17:20 EDT | checks lane (Claude Code 70ea6b18) | step 2 | 4e2c3f92 | .config/nextest.toml (60 s SLOW, stop at 5 min; longer only for tc-api's round trips and tc-app's scenario crash test, with their times; never_blocked alone); gates.sh steps test, test-doc, test-app and alignment-budget on nextest, --touched too; CI installs cargo-nextest 0.9.149 in every job that runs them, test-doc runs even after a failed test step; the VM's list matches; node --test-timeout=120000; browserTest's default 600 s to 120 s; build_once_per_run for the four bundle folders; the gates-sync check. The hang proof: TIMEOUT at 300.007 s, exit 100 | .audit/roadmap/evidence/2026-10-26-CHK-8/hang-proof.log
- 2026-10-26 17:20 EDT | checks lane (Claude Code 70ea6b18) | verify | 6bfe2f04 | one pass by a fresh read-only reviewer agent: override filters match their binaries and tests; default-filter, --run-ignored only and --package lists behave; cmd /c quoting right; every nextest job installs it; build_once_per_run right within a run (no prefix collision, the rename loser reuses the winner). §8: code tooling; checks exist and run in CI's node step; reach N/A (tooling); siblings: the four fixed bundle folders; no-loss N/A; prose TRACKING item 3. One major: tc-server's never_blocked relied on a static lock to run alone. Minors: `a && b` steps skipped later parts after a failure; ps1 counted no timed out; overrides unbounded in the check; Windows FaultFs mirrors, pid reuse, other wall-clock asserts, NIGHTLY on libtest | reviewer report in this session
- 2026-10-26 17:20 EDT | checks lane (Claude Code 70ea6b18) | fix | 6bfe2f04 | never_blocked runs alone (threads-required = num-test-threads); each part is its own step (test-doc) and CI runs it even after a failure; ps1 counts timed out; the check bounds every override; the other minors are in .audit/minors.tsv | node --test .audit/tools/gates-sync.test.mjs
- 2026-10-26 17:40 EDT | checks lane (Claude Code 70ea6b18) | gate | f0d236eb | hand-off checks with main (ad4b67bc) merged: scripts/gates.sh --touched (2 crates: nextest 1,001 tests pass in 508 s, doctests pass), clippy -D warnings on tc-app and tc4 all targets, scripts/gates.sh --step node (# pass 128, # fail 0; journeys skip without builds), gates-sync # pass 9. Every journey file depends on kit.mjs's new 120 s default, which no --touched selection reaches: the batch gate's app-e2e runs them all (CI's slowest journey test took 14.8 s) | scripts/gates.sh --touched
