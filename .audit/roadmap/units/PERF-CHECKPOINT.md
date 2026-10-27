---
id: PERF-CHECKPOINT
title: A checkpoint of the whole Bible takes 500 ms on the reference laptop
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
size: 2-3 d (basis: S2 found the cost in the number of flushes; the fix changes how a checkpoint writes objects)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-CHECKPOINT: A checkpoint of the whole Bible takes 500 ms on the reference laptop

## Goal

The row "Checkpoint, whole Bible: 500 ms" of docs/PERFORMANCE.md is met on the reference laptop in a release build, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

A checkpoint is what keeps a translator's work safe in history. A checkpoint that takes seconds on their laptop is one they notice, and one they may stop.

## Before you start

- The measurement: median 0.71 s (p95 1.32 s) with the materialization already done; median 1.52 s (p95 2.10 s) with Psalms dirty; 20 runs each, release build, commit 003aa087, 2026-10-27.
- It came from PERF-LAPTOP (docs/evidence/perf-laptop-2026-10-27.md), on the Windows 10 reference
  laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on).
- Re-measure with: $(System.Collections.Hashtable.cmd). A figure from the Linux machine alone does not close this card.
- Remedy: write one checkpoint's git objects into one pack (about 3 flushes in place of about 24) and accept libgit2's search for deltas between the new versions of the large files, or relax the row (docs/PERFORMANCE.md, open rows).

## Funnel card

- **Invariants**:
  - INV-CHECKPOINT-WRITES-ONE-PACK: a checkpoint writes its git objects as one pack and flushes a bounded number of times, whatever the number of changed files.
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: a count test of flushes (fsyncs) per checkpoint on the Psalms corpus and the corpus ten times larger; crash-injection through the existing checkpoint tests still passes.
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

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Checkpoint, whole Bible: 500 ms" | docs/evidence/perf-laptop-2026-10-27.md
- 2026-10-27 | reference-laptop session (Claude Code) | step 1 | none | Not flushes (fsync 1 to 13 ms here; 24 of them are 30 to 100 ms): one core of CPU. A second run met the clean case (median 428 ms, p95 494 ms; the first gave 706 ms). The Psalms-dirty case (1.04 to 1.52 s) is mostly that book's materialize, which shares PERF-MATERIALIZE-BOOK's lever. Phase timings still to add to storebench | docs/evidence/perf-laptop-2026-10-27.md
