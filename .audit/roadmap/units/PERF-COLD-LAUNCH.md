---
id: PERF-COLD-LAUNCH
title: The installed app reaches Home in 3 s after a reboot
track: A
lane: app
milestone: M5
state: todo
owner: none
branch: none
worktree: none
depends: []
needs_owner: []
proves: []
no_loss: false
size: 1-2 d (basis: one measurement; the cause is not known)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-COLD-LAUNCH: The installed app reaches Home in 3 s after a reboot

## Goal

The row "Repeat launch: a usable Home in 3 s (proposed)" of `docs/PERFORMANCE.md` is met on the
reference laptop on the first launch after a reboot, when the files are cold, or the owner has chosen
a different budget for that case (owner ruling H17: measure, don't block).

## Why it matters to a translator

A translator opens the app once a day, after the laptop has been on for a while or just started. A
launch that takes 42 s, with a "Preparing" page for most of it, is the first thing they see each day.

## Before you start

- The measurement: spawn to Home 42.2 s, with the Preparing page from 4.2 s, one run, 170 s after a
  boot, installed unsigned 4.1.0-preview (05d7a661), Defender on. Warm, the same launch is 0.58 s
  median over 10 runs and never shows the Preparing page.
- It came from `PERF-LAPTOP` (`docs/evidence/perf-laptop-2026-10-27.md`, Part 5), on the Windows 10
  reference laptop (Intel Core i5-8350U, 8 GiB, SSD).
- Re-measure with: `C:\tc4-g0\perf-tools\cold-after-reboot.ps1` after a reboot (it needs the
  owner's reboot), or `node launch.mjs repeat 1` for a warm run.
- Lead: the seeded resources are 3,785 files and 304 MB, and the operating system's first read of a
  newly seen file costs about 70 ms per MB (PERF-LEXICON-LOAD). If the launch reads or checks every
  resource file, that explains most of the time. Not shown.

## Funnel card

- **Invariants**:
  - INV-REPEAT-LAUNCH-READS-NO-RESOURCE-CONTENT: a repeat launch reads no resource content file
    (the count test exists for a warm app; this card checks it also holds on a cold one).
- **Wrong states**:
  - The Preparing page shown on a repeat launch.
- **Code**: to be chosen in step 1.
- **Checks**: a count test of files opened by a repeat launch, if it does not cover this path.
- **Prose**: the row of `docs/PERFORMANCE.md`, updated with the new figure.
- **Human**: the owner chooses between a fix and a cold-launch budget if step 1 finds no cheap fix.

## Steps

### Step 1: Find where the 38 s goes

- Do: keep the shell's log (`shell.log`) and the server's, and record what the Preparing page
  does on a repeat launch, on a cold run (needs a reboot) and on a warm one.
- Files: `docs/evidence/perf-laptop-2026-10-27.md` (or a new evidence file).
- Done when: the evidence names what runs during the Preparing page and why it appears.
- Record: a log line `step 1`.

### Step 2: Fix it or ask

- Do: make the fix step 1 found, or put the owner's decision on a cold-launch budget in the ruling
  and the `PERFORMANCE.md` row.
- Files: the code step 1 names; `docs/PERFORMANCE.md`.
- Done when: the cold run after a reboot meets the budget.
- Record: a log line `step 2`.

## Gates before merge

The full gate list (`scripts/gates.sh`), plus:

- A cold re-measurement after a reboot, recorded in `docs/evidence/`.
- The review gate of `FUNNEL.md` section 8: a `verify` log line before `done`.

## Stop and ask when

- The fix would skip a check of the installed resources that protects a translator from a damaged one.

## References

- `docs/PERFORMANCE.md`; `docs/evidence/perf-laptop-2026-10-27.md`; `rulings/2026-10-27-H17.md`; `PERF-LAPTOP`

## Log

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the cold repeat launch (42.2 s against 3 s) | docs/evidence/perf-laptop-2026-10-27.md
