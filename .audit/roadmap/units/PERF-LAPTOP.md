---
id: PERF-LAPTOP
title: Measure every speed and memory budget on the reference laptop
track: A
lane: checks
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: 1 d (basis: S2's store measurements on the owner's Linux machine, 2026-10-18)
merged: none
updated: 2026-10-27
next: Claim this card on the reference laptop ("Part 3 complete" session) when it is back online, then do step 1.
blocked_by: none
---

# PERF-LAPTOP: Measure every speed and memory budget on the reference laptop

## Goal

Every budget row of `docs/PERFORMANCE.md` has a number measured on the reference laptop, in a
release build of 4.1.0-preview, recorded in `docs/evidence/`. Each row that misses its budget has
a card of its own. Owner ruling H17 (`rulings/2026-10-27-H17.md`): measure, don't block.

## Why it matters to a translator

Translators work on laptops like the reference one, not on the build machine. A slow spot found
here is named and has a card, before it reaches more translators.

## Before you start

- `grep -c '^| \(Project\|UI\|App\)' docs/PERFORMANCE.md` counts the budget rows (30 on
  2026-10-27).
- `docs/PERFORMANCE.md` "Rows of the project store that are open" names the five rows S2 could
  only estimate (checkpoint, save during a checkpoint, materialize one book, close a project, store
  memory). They need this laptop.
- The laptop session is "Part 3 complete" (offline on 2026-10-27). It measures only; the Windows
  VM on the Linux PC builds and tests. Its procedure is `.audit/tasks/windows-track.md`.
- The installer is 4.1.0-preview from release.yml on main (REL-1 step 3). The start-up rows need
  the signed installer (H4), so they wait for signing.

## Funnel card

- **Invariants**:
  - INV-EVERY-BUDGET-IS-MEASURED (not in code: a measurement on a person's laptop): each budget row
    has a laptop number, the build it came from and the date.
- **Wrong states**:
  - A row reported as met from the build machine's figure only.
  - A number from a debug build, or from a build other than the one named.
  - A missed row with no card.
- **Code**: none. The benches and examples that exist are the tools (`crates/tc-app/examples/`).
- **Checks**: none in CI. The evidence file is the record.
- **Prose**: `docs/evidence/perf-laptop-<date>.md`; `docs/PERFORMANCE.md`'s open-rows table
  updated.
- **Human**: the owner runs the laptop or lets its session run.

## Steps

### Step 1: Measure the rows

- Do: on the reference laptop, with Defender on, run each row's measurement in a release build of
  4.1.0-preview: the store rows with the store benches, the UI rows in the installed app, the
  start-up rows on the signed installer once it exists.
- Files: `docs/evidence/perf-laptop-<date>.md` (to create): row, budget, figure, met or missed,
  build and commit.
- Done when: every row has a figure, or a line saying what it waits for (signing for the start-up
  rows).
- Record: a log line `step 1`.

### Step 2: A card for each missed row

- Do: for each missed row, add a card that names the row, the figure and the remedy
  `PERFORMANCE.md` proposes. Update `PERFORMANCE.md`'s open-rows table.
- Files: `.audit/roadmap/units/`, `docs/PERFORMANCE.md`.
- Done when: `python3 -I .audit/tools/board.py --check` shows 0 errors, and each missed row names
  its card.
- Record: a log line `step 2`.

## Gates before merge

- `python3 -I -B .audit/tools/board.py --check` prints `0 errors`.
- The review gate of `FUNNEL.md` §8: a `verify` log line before `done` (one lens: someone other
  than the measurer checks that each figure came from the named build).

## Stop and ask when

- The laptop is not available for more than a week: ask the owner whether another Windows 10
  laptop of the same class may stand in.

## References

- `rulings/2026-10-27-H17.md`; `docs/PERFORMANCE.md`; `docs/evidence/storage-materialize-2026-10-18.md`

## Log

- 2026-10-27 08:11 EDT | integration lane (Claude Code 3d07d5f8) | create | none | Card created by owner ruling H17 (measure, don't block), as the orchestrator asked. REL-1 does not depend on it | .audit/roadmap/rulings/2026-10-27-H17.md
