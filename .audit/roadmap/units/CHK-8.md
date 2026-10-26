---
id: CHK-8
title: A hung test is named and stopped within minutes, in CI and in scripts/gates.sh
track: A
lane: checks
milestone: M1
state: claimed
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
next: Step 1, measure nextest against cargo test on each suite.
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
  - A deliberately hung test, run through the gate's runner: nextest names it as SLOW at 60 s
    and TIMEOUT at 5 min, and the run fails.
- **Prose**: TRACKING "How a card gets ready" names the time limits; `.config/nextest.toml`
  says why each override exists.

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
