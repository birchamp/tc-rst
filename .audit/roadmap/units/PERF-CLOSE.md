---
id: PERF-CLOSE
title: Closing a project takes 3 s on the reference laptop
track: A
lane: storage
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: true
size: 1-2 d (basis: the close is a checkpoint plus the fold cache and drafts; PERF-CHECKPOINT shares its cost)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-CLOSE: Closing a project takes 3 s on the reference laptop

## Goal

The row "Close a project, whole Bible, Psalms changed: 3 s" of docs/PERFORMANCE.md is met on the reference laptop in a release build, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

A translator closing the laptop lid or the app waits for the close. A 4 s wait is a reason to pull the plug, which is the one thing the store must never need.

## Before you start

- The measurement: median 3.13 s, p95 3.88 s (5 runs); usable-after-open 2.3 to 4.0 s; release build, commit 003aa087, 2026-10-27. The store adapter bench (10 saves then close) gave median 1.15 s, p95 1.86 s, so the cost depends on what changed.
- It came from PERF-LAPTOP (docs/evidence/perf-laptop-2026-10-27.md), on the Windows 10 reference
  laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on).
- Re-measure with: $(System.Collections.Hashtable.cmd). A figure from the Linux machine alone does not close this card.
- Remedy: none proposed beyond the checkpoint remedy: a close is seal, files, tree, commit, fold cache and drafts, so PERF-CHECKPOINT lowers it first; measure again after that card lands, then decide whether the fold-cache write needs its own change or the row is relaxed.

## Funnel card

- **Invariants**:
  - INV-CLOSE-IS-SAFE-TO-SKIP: nothing a close writes is needed to keep saved work (the cache and checkpoint are rebuilt on the next open); the close is only faster, never required.
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: a crash-injection test that kills the process at each close step and opens again with nothing lost (it may exist already in tc-store; name it here); the timing is the laptop measurement.
- **Prose**: the row of docs/PERFORMANCE.md and its open-rows table, updated with the new figure.
- **Human**: the owner chooses between a fix and relaxing the row if step 1 finds no cheap fix.

## Steps

### Step 1: Find where the time goes

- Do: profile the row on the laptop (or on the Windows VM, then confirm on the laptop) and write what
  limits it, with its figure, in the evidence file.
- Files: docs/evidence/perf-laptop-2026-10-27.md (or a new evidence file).
- Done when: the evidence names the limit and says what a fix would cost.
- Record: a log line step 1.

### Step 2: Fix it or ask

- Do: make the fix step 1 found, with the check named above, or put the owner's decision to relax
  the row in the ruling and the PERFORMANCE.md row.
- Files: the code step 1 names; docs/PERFORMANCE.md.
- Done when: the laptop re-measurement in the "Before you start" command meets the budget.
- Record: a log line step 2.

## Gates before merge

The full gate list (scripts/gates.sh, or every command in AGENTS.md "Tests" plus the Windows
clippy step from .github/workflows/ci.yml), plus:

- A laptop re-measurement of the row, recorded in docs/evidence/.
- The review gate of FUNNEL.md section 8: a erify log line before done.

## Stop and ask when

- Step 1 shows the budget cannot be met without a change to how work is written to disk that could
  lose or hide a translator's work: stop and ask the owner.

## References

- docs/PERFORMANCE.md; docs/evidence/perf-laptop-2026-10-27.md; ulings/2026-10-27-H17.md; PERF-LAPTOP

## Log

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Close a project, whole Bible, Psalms changed: 3 s" | docs/evidence/perf-laptop-2026-10-27.md
- 2026-10-27 | reference-laptop session (Claude Code) | step 1 (none on the laptop) | none | Not profiled; a close carries the checkpoint's and the Psalms materialize's costs. Measure again after PERF-CHECKPOINT and PERF-MATERIALIZE-BOOK | docs/evidence/perf-laptop-2026-10-27.md
