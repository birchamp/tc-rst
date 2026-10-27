---
id: PERF-ALIGNER-OPEN
title: The aligner opens on a verse in 30 ms
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
size: 1 d (basis: a fixed cost of about 220 ms, the same for every verse, points to one wait)
merged: none
updated: 2026-10-27
next: Claim this card, then do step 1.
blocked_by: none
---

# PERF-ALIGNER-OPEN: The aligner opens on a verse in 30 ms

## Goal

The row "Open the aligner on a verse, the longest verse in Psalms: 30 ms" of `docs/PERFORMANCE.md` is met in the installed app on the reference laptop, or
the owner has chosen to relax it (owner ruling H17: measure, don't block).

## Why it matters to a translator

Aligning moves verse by verse; a quarter-second pause on each verse adds up over a book.

## Before you start

- The measurement: 17:14 (the longest): 220 to 245 ms to the word bank, 236 to 260 ms painted; 10-word verses the same; the time does not grow with the verse. Installed unsigned 4.1.0-preview (05d7a661), the owner's `ar_avd`
  Psalms project imported from translationCore 3, 2026-10-27.
- It came from `PERF-LAPTOP` (`docs/evidence/perf-laptop-2026-10-27.md`, Part 4), on the Windows 10
  reference laptop (Intel Core i5-8350U, 8 GiB, SSD, Defender on, 60 Hz display).
- Re-measure with the scripts in Part 4 (the app started with `--remote-debugging-port`).
- Remedy: find the fixed 220 ms (a timer, a round trip, or a fixed wait); the word work itself looks small.

## Funnel card

- **Invariants**: to be named in step 1.
- **Wrong states**:
  - The row reported as met from the build machine's figure only.
- **Code**: to be chosen in step 1.
- **Checks**: an operation-count test for the action on Psalms and on ten times Psalms, where the
  cost is the code's; the laptop timing is the measurement.
- **Prose**: the row of `docs/PERFORMANCE.md`, updated with the new figure.
- **Human**: the owner chooses between a fix and relaxing the row if step 1 finds no cheap fix.

## Steps

### Step 1: Find where the time goes

- Do: profile the action (Chromium's performance panel on the installed app, and the server's own
  timings) and write what limits it, with its figure, in the evidence file.
- Files: `docs/evidence/perf-laptop-2026-10-27.md` (or a new evidence file).
- Done when: the evidence names the limit and says what a fix would cost.
- Record: a log line `step 1`.

### Step 2: Fix it or ask

- Do: make the fix step 1 found, with its check, or put the owner's decision to relax the row in
  the ruling and the `PERFORMANCE.md` row.
- Files: the code step 1 names; `docs/PERFORMANCE.md`.
- Done when: the laptop re-measurement meets the budget.
- Record: a log line `step 2`.

## Gates before merge

The full gate list (`scripts/gates.sh`, or every command in `AGENTS.md` "Tests" plus the Windows
clippy step from `.github/workflows/ci.yml`), plus:

- A laptop re-measurement of the row, recorded in `docs/evidence/`.
- The review gate of `FUNNEL.md` section 8: a `verify` log line before `done`.

## Stop and ask when

- The fix would show a translator a state before it is saved, and a failed save could hide or lose it.

## References

- `docs/PERFORMANCE.md`; `docs/evidence/perf-laptop-2026-10-27.md`; `rulings/2026-10-27-H17.md`; `PERF-LAPTOP`

## Log

- 2026-10-27 | reference-laptop session (Claude Code) | create | none | Card created by PERF-LAPTOP step 2 for the missed row "Open the aligner on a verse, the longest verse in Psalms: 30 ms" | docs/evidence/perf-laptop-2026-10-27.md
