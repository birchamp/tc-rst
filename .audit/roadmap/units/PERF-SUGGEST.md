---
id: PERF-SUGGEST
title: A suggestion for one verse takes 50 ms on the reference laptop
track: A
lane: domain
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: 2-4 d (basis: the figure is a model-size effect; two models need two answers)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-SUGGEST: A suggestion for one verse takes 50 ms on the reference laptop

## Goal

The row "Aligning, suggestions for one verse: 50 ms" of docs/PERFORMANCE.md is met on the reference laptop in a release build, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

A translator aligning a verse sees suggestions as they click. Over 200 ms a click feels delayed, and the aligner is where translators spend most of their checking time.

## Before you start

- The measurement: old project (3,994 aligned verses): median 52.3 ms, p95 56.1 ms; new project (7,958 aligned verses): median 231.6 ms, p95 235.5 ms, 4.7 times the budget; training 11 s and 26 s in the background; test `suggestion_budget`, release build, commit 003aa087, 2026-10-27. The small Titus bench gives 6 ms plain and 18 to 19 ms boosted.
- It came from PERF-LAPTOP (docs/evidence/perf-laptop-2026-10-27.md), on the Windows 10 reference
  laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on).
- Re-measure with: $(System.Collections.Hashtable.cmd). A figure from the Linux machine alone does not close this card.
- Remedy: none proposed. The time grows with the number of aligned verses, so look at what a suggestion scans per question (the whole model, or only the verse's words) and cut it to the verse; or relax the row for a model of this size with the owner.

## Funnel card

- **Invariants**:
  - INV-SUGGESTION-COST-IS-THE-VERSE: the time of a suggestion grows with the words of the verse asked about, not with the number of aligned verses the model was trained on.
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: an operation-count test of the model's entries visited per suggestion at 3,994 and 7,958 aligned verses (equal within a small factor); the laptop timing is the measurement.
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

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Aligning, suggestions for one verse: 50 ms" | docs/evidence/perf-laptop-2026-10-27.md
