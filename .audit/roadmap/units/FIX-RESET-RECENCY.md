---
id: FIX-RESET-RECENCY
title: Each captured screen starts with Home's cards in the seed's order
track: A
lane: app
milestone: M1
state: done
owner: reset-recency session (Claude Code d4604680)
branch: fix-reset-recency
worktree: .worktrees/fix-reset-recency
depends: []
needs_owner: []
proves: [P8]
no_loss: false
size: 0.25 d (basis: a test hook and one test)
merged: 9be14d41
updated: 2026-10-26
next: none
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
  - INV-RESET-STOPS-OLD-WORK: after `test/reset` returns, no work begun before it writes anything.
- **Wrong states**:
  - An open held at `test/hold open` is released by the reset, opens the project and records its
    last use after the preferences were reset; Home then lists that project first.
- **Code**:
  - `Handle<Reset>` cancels every live job after releasing the holds and waits for each to end
    (`JobRegistry::cancel_all`), before it closes the sessions and resets the preferences.
- **Checks**:
  - `a_reset_during_a_held_open_leaves_home_in_the_seed_order` (crates/tc-app/tests/app/store_scenario.rs),
    on the scenario store with slow reads, as the gallery runs it.
  - `opening_a_project_moves_its_card_to_the_top` (crates/tc-app/tests/app/understand.rs): Home
    stays most-recently-used first (owner, 2026-10-26: "project order should be MRU").
- **Human**: none.

## Steps

### Step 1: The reset waits for every job; the test

## Log

- 2026-10-26 | reset-recency session (Claude Code d4604680) | created | none | from the orchestrator's message: Home's card order leaks between gallery cases (checks lane, CHK-6 step 5) | /home/bot/.config/Claude/side-session-notes/local_d49edca7-414b-4925-aa79-3e6882b3638b/CHK-6-step5-recapture-findings.md
- 2026-10-26 | reset-recency session (Claude Code d4604680) | claim | none | the open job records the project's last use after its hold; the reset releases the hold and resets the preferences without waiting for the job | crates/tc-app/src/understand.rs open_job
- 2026-10-26 | reset-recency session (Claude Code d4604680) | step 1 | none | `test/reset` cancels every live job after releasing the holds and waits for each to end. The new store-scenario test failed without it (`left: [sample_burrito, sample_burrito_obs]`, `right: [sample_burrito_obs, sample_burrito]`) and passes with it 3 runs of 3; the 17 reset tests pass. On the memory backend the open finishes before the reset reaches the prefs, so the test runs on the scenario store. On main open-progress-prepare's residual is pixels only, for glyph anti-aliasing, not for this order: nothing to drop | cargo nextest run -p tc-app --all-features --test app reset
- 2026-10-26 | reviewer (Claude Code subagent of d4604680) | verify | f42d712b | no blocker, no major. No job the harness reaches ignores cancel while the reset waits (open's hold is released first; train, download and the Home summary stop on cancel); the registry lock is dropped before any await; the test is red without the fix and cannot fail spuriously, because the recorded use and the prefs reset go through one FIFO channel. Two minors to minors.tsv | code reading
- 2026-10-26 | reset-recency session (Claude Code d4604680) | gate | f42d712b | `scripts/gates.sh --touched` exits 0 with main merged: tc-app 1039 tests pass (6 slow, none new), app-e2e 8 of 8 | scripts/gates.sh --touched
- 2026-10-26 | reset-recency session (Claude Code d4604680) | note | none | step 2 (recapture) dropped on the orchestrator's word: CHK-6 step 5 lands last with one full recapture | none
- 2026-10-26 21:30 EDT | integration lane (Claude Code 3d07d5f8) | merge | 9be14d41 | merged into main in batch 24 (a test file reformatted by cargo fmt), gated by scripts/gates.sh --wait-ci batch-24 | scripts/gates.sh --wait-ci batch-24
