---
id: FIX-RESET-RECENCY
title: Each captured screen starts with Home's cards in the seed's order
track: A
lane: app
milestone: M1
state: claimed
owner: reset-recency session (Claude Code d4604680)
branch: fix-reset-recency
worktree: .worktrees/fix-reset-recency
depends: []
needs_owner: []
proves: [P8]
no_loss: false
size: 0.25 d (basis: a test hook and one test)
merged: none
updated: 2026-10-26
next: step 1, the reset waits for every job before it resets
blocked_by: none
---

# FIX-RESET-RECENCY: Each captured screen starts with Home's cards in the seed's order

## Goal

`test/reset` leaves no trace of an earlier case: Home lists the projects in the seed's order.

## Why it matters to a translator

Nothing a translator sees changes. The gallery compares our screens with tC4's; a case that
inherits an earlier case's last-opened project shows the wrong card first, and a residual can hide
it (open-progress-prepare's did).

## Before you start

- The checks lane found it (CHK-6 step 5 recapture): after the open-progress cases open "Tito y
  Jonás", license, open-progress-state and once open-progress-prepare were captured with that
  card first. Each case alone has tC4's order.

## Funnel card

- **Invariants**:
  - After `test/reset` returns, no work begun before it writes anything.
- **Wrong states**:
  - An open held at `test/hold open` is released by the reset, opens the project and records its
    last use after the preferences were reset; Home then lists that project first.
- **Code**:
  - `Handle<Reset>` cancels every live job after releasing the holds and waits for each to end
    (`JobRegistry::cancel_all`), before it closes the sessions and resets the preferences.
- **Checks**:
  - `a_reset_during_a_held_open_leaves_home_in_the_seed_order` (crates/tc-app/tests/app/hooks.rs).
- **Human**: none.

## Steps

### Step 1: The reset waits for every job; the test

### Step 2: Recapture the affected screens (after CHK-6 step 5 lands)

## Log

- 2026-10-26 | reset-recency session (Claude Code d4604680) | created | none | from the orchestrator's message: Home's card order leaks between gallery cases (checks lane, CHK-6 step 5) | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/CHK-6-step5-recapture-findings.md
- 2026-10-26 | reset-recency session (Claude Code d4604680) | claim | none | the open job records the project's last use after its hold; the reset releases the hold and resets the preferences without waiting for the job | crates/tc-app/src/understand.rs open_job
