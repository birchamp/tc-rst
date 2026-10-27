---
id: PERF-MATERIALIZE-BOOK
title: Materializing one changed book takes 1 s p95 on the reference laptop
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
no_loss: false
size: 2-3 d (basis: S2's count test already proves only that book's files are touched; the cost is the bytes)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-MATERIALIZE-BOOK: Materializing one changed book takes 1 s p95 on the reference laptop

## Goal

The row "Materialize one book, off the project actor: 1 s p95" of docs/PERFORMANCE.md is met on the reference laptop in a release build, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

A translator saves a verse in Psalms; the book's files are rebuilt behind them. Nothing waits for it, but the Check screen and the chapter views read those files, so a slow rebuild is a stale view.

## Before you start

- The measurement: Psalms after a verse save: median 2.22 s, p95 3.05 s; after a decision: median 0.99 s, p95 1.25 s; Jude after a verse save: median 90 ms, p95 126 ms; 20 runs each, release build, commit 003aa087, 2026-10-27. Per run: 152 MB read, 67 MB hashed, 17 MB written, 30 fsyncs (verse save).
- It came from PERF-LAPTOP (docs/evidence/perf-laptop-2026-10-27.md), on the Windows 10 reference
  laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on).
- Re-measure with: $(System.Collections.Hashtable.cmd). A figure from the Linux machine alone does not close this card.
- Remedy: none proposed yet. S2 estimated 0.5 to 1.3 s from the Linux machine. Start from the bytes read and hashed (a verse save re-reads and re-hashes 152 MB and 67 MB for one Psalms verse), and from the 30 fsyncs; then either cut the work or relax the row with the owner.

## Funnel card

- **Invariants**:
  - INV-MATERIALIZE-COST-IS-THE-BOOK: materializing one book reads and hashes at most the bytes of that book's own files and ingredients (a count of bytes, not only of files).
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: a bytes-read and bytes-hashed count test on the Psalms corpus and on the corpus ten times larger; the existing count test of files stays.
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

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Materialize one book, off the project actor: 1 s p95" | docs/evidence/perf-laptop-2026-10-27.md
