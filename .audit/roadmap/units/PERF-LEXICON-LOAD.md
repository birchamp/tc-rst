---
id: PERF-LEXICON-LOAD
title: The Hebrew lexicon loads in 100 ms the first time on the reference laptop
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
size: 1 d (basis: the lexicon is one 2 MB JSON file; the cost is parsing it once)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-LEXICON-LOAD: The Hebrew lexicon loads in 100 ms the first time on the reference laptop

## Goal

The row "Load a lexicon (the Hebrew lexicon, 8,674 entries): 100 ms to load once per session" of docs/PERFORMANCE.md is met on the reference laptop in a release build, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

A translator opening the first Hebrew word waits for it. Under a third of a second is a pause, but it is the first thing they do in a session.

## Before you start

- The measurement: first load 312 ms; warm median 28.6 ms, p95 36.1 ms of 21 runs (read median 0.9 ms, parse median 27.7 ms); the smaller Greek lexicon loads in 8 ms; release build, commit 003aa087, 2026-10-27. The test `loading_the_hebrew_lexicon_meets_the_budget` FAILS on this laptop (it fails above three times the budget).
- It came from PERF-LAPTOP (docs/evidence/perf-laptop-2026-10-27.md), on the Windows 10 reference
  laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on).
- Re-measure with: $(System.Collections.Hashtable.cmd). A figure from the Linux machine alone does not close this card.
- Remedy: none proposed. The first load is 11 times the warm one, so look at what the first call pays that the others do not (a cold file read under Defender, allocation, or a one-time index) before changing the parser; a lazy load behind the first lookup that needs it is the usual next step.

## Funnel card

- **Invariants**:
  - INV-LEXICON-FIRST-LOAD-IS-WITHIN-BUDGET: the first load of the largest lexicon is within 100 ms, or the lexicon loads in the background with the first lookup waiting on it behind a determinate indicator.
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: the existing `loading_the_hebrew_lexicon_meets_the_budget` passes on the laptop; no new check is proposed.
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

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Load a lexicon (the Hebrew lexicon, 8,674 entries): 100 ms to load once per session" | docs/evidence/perf-laptop-2026-10-27.md
